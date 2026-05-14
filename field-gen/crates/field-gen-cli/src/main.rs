mod cli;

use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use cli::parse_args;
use serde_json::Value;
use shockwave_config::Dimensions3;
use shockwave_iso::{IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_math::geometry::{Triangle, mesh_bounds};
use shockwave_math::grid::Grid;
use shockwave_output::{
    Metadata, MetadataDocument, build_atlas, metadata_json, write_occupancy_bmp, write_ply_binary,
};
use shockwave_path::LayerToolpaths;
use shockwave_slicer::{
    FIELD_EXTENSION_VOXELS, FieldPropagation, SliceProgress, SliceSettings,
    clip_isosurfaces_to_solid, toolpaths_from_isosurfaces, voxelize,
};
use shockwave_slicer_io::{SliceOutputPaths, load_stl_model, write_gcode_atomically};
use shockwave_voxel::field::{ExplicitKernelPropagation, Field, KernelMove, KernelPathCheck};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let total_start = Instant::now();
    let config = parse_args(env::args().skip(1).collect())?;
    let triangles = load_mesh(&config)?;
    let settings = slicer_settings_from_config(&config)?;
    let (grid, occupancy, field) = voxelize_with_timing(&settings, &triangles)?;
    let paths = write_outputs(&config, &settings, &triangles, &occupancy, &field, grid)?;
    print_summary(&triangles, &occupancy, grid, &paths);
    log_timing("total", total_start.elapsed());
    Ok(())
}

fn load_mesh(config: &cli::Config) -> Result<Vec<Triangle>, String> {
    let start = Instant::now();
    let triangles = load_stl_model(&config.input)?;
    log_timing("load stl", start.elapsed());
    Ok(triangles)
}

fn slicer_settings_from_config(config: &cli::Config) -> Result<SliceSettings, String> {
    let settings = &config.settings;
    let field_rate = vec3_from_dimensions(settings.field.anisotropic_rate);
    let propagation = if let Some(kernel_path) = &settings.field.kernel_path {
        let load_start = Instant::now();
        let propagation = load_kernel_propagation(kernel_path)?;
        log_timing("load kernel", load_start.elapsed());
        eprintln!("Loaded kernel with {} moves", propagation.move_count());
        FieldPropagation::ExplicitKernel(propagation)
    } else {
        FieldPropagation::from_method(settings.field.method, field_rate)
    };

    Ok(SliceSettings {
        voxel_size: vec3_from_dimensions(settings.slicing.voxel_size_mm),
        requested_size: Some(vec3_from_dimensions(settings.printer.print_volume_mm)),
        padding_voxels: settings.slicing.padding_voxels,
        origin: settings.slicing.origin_mm.map(vec3_from_dimensions),
        field_enabled: settings.field.enabled,
        propagation,
        field_rate,
        max_unreached_below_mm: settings.printer.obstruction.printhead_clearance_height_mm,
        unreached_cone_angle_degrees: settings
            .printer
            .obstruction
            .printhead_clearance_angle_degrees,
        iso_spacing: settings.slicing.layer_height_mm,
        wall_count: settings.slicing.wall_count,
        extrusion_width_mm: settings.slicing.extrusion_width_mm,
        filament_diameter_mm: settings.material.filament_diameter_mm,
        bed_temperature_c: settings.material.bed_temperature_c,
        nozzle_temperature_c: settings.material.nozzle_temperature_c,
        fan_speed_percent: settings.material.fan_speed_percent,
        global_z_offset_mm: settings.slicing.global_z_offset_mm,
        infill_spacing_mm: settings.slicing.infill_line_spacing_mm(),
    })
}

fn vec3_from_dimensions(value: Dimensions3) -> shockwave_math::geometry::Vec3 {
    shockwave_math::geometry::Vec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn voxelize_with_timing(
    settings: &SliceSettings,
    triangles: &[Triangle],
) -> Result<(Grid, Vec<u8>, Option<Field>), String> {
    let start = Instant::now();
    let mut progress = stderr_progress();
    let result = voxelize(settings, triangles, &mut progress).map_err(|error| error.to_string())?;
    log_timing("voxelize", start.elapsed());
    Ok(result)
}

fn stderr_progress() -> impl FnMut(SliceProgress) {
    let mut last_percent = None;
    move |event| {
        if event.phase_progress >= 1.0 {
            eprintln!("{:?}: complete - {}", event.phase, event.message);
            last_percent = None;
            return;
        }

        let percent = (event.phase_progress * 100.0).floor() as i32;
        if last_percent != Some(percent) && percent % 5 == 0 {
            eprintln!("{:?}: {:>3}% - {}", event.phase, percent, event.message);
            last_percent = Some(percent);
        }
    }
}

fn write_outputs(
    config: &cli::Config,
    settings: &SliceSettings,
    triangles: &[Triangle],
    occupancy: &[u8],
    field: &Option<Field>,
    grid: Grid,
) -> Result<SliceOutputPaths, String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let paths = SliceOutputPaths::from_prefix(
        &config.output_prefix,
        field.is_some(),
        config.settings.output.export_ply,
        config.settings.output.gcode,
    );

    let occ_start = Instant::now();
    fs::write(&paths.volume, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", paths.volume.display()))?;
    log_timing("write occ", occ_start.elapsed());

    let bmp_start = Instant::now();
    write_occupancy_bmp(&paths.image, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", paths.image.display()))?;
    log_timing("write bmp", bmp_start.elapsed());

    if let Some(field) = field
        .as_ref()
        .filter(|_| config.settings.output.export_ply || config.settings.output.gcode)
    {
        let mesh = extract_isosurfaces_with_timing(field, grid, settings.iso_spacing)?;

        if let Some(mesh_path) = paths.mesh.as_ref() {
            let ply_start = Instant::now();
            write_ply_binary(mesh_path, &mesh)
                .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
            log_timing("write ply", ply_start.elapsed());
        }

        let clipped_mesh = clip_isosurfaces_with_timing(&mesh, triangles);

        if let Some(clipped_mesh_path) = paths.clipped_mesh.as_ref() {
            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            log_timing("write clipped ply", clipped_ply_start.elapsed());
        }

        if let Some(gcode_path) = paths.gcode.as_ref() {
            write_gcode_with_timing(gcode_path, &clipped_mesh, settings, triangles, field, grid)?;
        }
    }

    let metadata_start = Instant::now();
    fs::write(
        &paths.metadata,
        metadata_json(&MetadataDocument {
            metadata: Metadata {
                input: &config.input.display().to_string(),
                voxel_size: settings.voxel_size,
                padding_voxels: settings.padding_voxels,
                field_enabled: settings.field_enabled,
                field_method: settings.field_method_name(),
                kernel_path: config
                    .settings
                    .field
                    .kernel_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .as_deref(),
                field_rate: settings.field_rate,
                max_unreached_below_mm: settings.max_unreached_below_mm,
                unreached_cone_angle_degrees: settings.unreached_cone_angle_degrees,
                field_extension_voxels: FIELD_EXTENSION_VOXELS,
                iso_spacing: settings.iso_spacing,
            },
            bounds,
            grid,
            atlas,
            volume_path: &paths.volume,
            image_path: &paths.image,
            mesh_path: paths.mesh.as_deref(),
            clipped_mesh_path: paths.clipped_mesh.as_deref(),
            field: field.as_ref(),
            occupied_count,
            voxel_count: occupancy.len(),
        }),
    )
    .map_err(|error| format!("failed to write {}: {error}", paths.metadata.display()))?;
    log_timing("write metadata", metadata_start.elapsed());

    Ok(paths)
}

fn extract_isosurfaces_with_timing(
    field: &Field,
    grid: Grid,
    iso_spacing: f64,
) -> Result<IsosurfaceSet, String> {
    let start = Instant::now();
    let mesh = extract_regular_isosurfaces(field, grid, iso_spacing)?;
    log_timing("extract isosurfaces", start.elapsed());
    eprintln!(
        "timing: isosurface set produced {} surfaces, {} vertices and {} triangles",
        mesh.surfaces.len(),
        mesh.vertex_count(),
        mesh.triangle_count()
    );
    Ok(mesh)
}

fn clip_isosurfaces_with_timing(mesh: &IsosurfaceSet, triangles: &[Triangle]) -> IsosurfaceSet {
    let start = Instant::now();
    let clipped_mesh = clip_isosurfaces_to_solid(mesh, triangles);
    log_timing("clip isosurfaces", start.elapsed());
    eprintln!(
        "timing: clipped isosurface set produced {} surfaces, {} vertices and {} triangles",
        clipped_mesh.surfaces.len(),
        clipped_mesh.vertex_count(),
        clipped_mesh.triangle_count()
    );
    clipped_mesh
}

fn write_gcode_with_timing(
    gcode_path: &Path,
    clipped_mesh: &IsosurfaceSet,
    settings: &SliceSettings,
    triangles: &[Triangle],
    field: &Field,
    grid: Grid,
) -> Result<(), String> {
    let path_start = Instant::now();
    let layers = toolpaths_from_isosurfaces(clipped_mesh, settings, field, grid)
        .map_err(|error| error.to_string())?;
    log_timing("generate perimeter paths", path_start.elapsed());
    eprintln!(
        "timing: generated {} toolpath layers with {} paths",
        layers.len(),
        layers.iter().map(LayerToolpaths::path_count).sum::<usize>()
    );

    let gcode_start = Instant::now();
    write_gcode_atomically(gcode_path, &layers, mesh_bounds(triangles), settings)?;
    log_timing("write gcode", gcode_start.elapsed());
    Ok(())
}

fn load_kernel_propagation(path: &Path) -> Result<ExplicitKernelPropagation, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read kernel {}: {error}", path.display()))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse kernel {}: {error}", path.display()))?;

    if json.get("type").and_then(Value::as_str) != Some("explicit") {
        return Err("kernel JSON must have \"type\": \"explicit\"".to_string());
    }

    let path_check = match json
        .get("path_check")
        .and_then(Value::as_str)
        .unwrap_or("swept_occupied")
    {
        "endpoint_occupied" => KernelPathCheck::EndpointOccupied,
        "swept_occupied" => KernelPathCheck::SweptOccupied,
        value => {
            return Err(format!(
                "kernel path_check must be endpoint_occupied or swept_occupied, got {value:?}"
            ));
        }
    };

    let moves = json
        .get("moves")
        .and_then(Value::as_array)
        .ok_or_else(|| "kernel JSON must contain a moves array".to_string())?
        .iter()
        .enumerate()
        .map(parse_kernel_move)
        .collect::<Result<Vec<_>, _>>()?;

    ExplicitKernelPropagation::new(moves, path_check)
}

fn parse_kernel_move((index, value): (usize, &Value)) -> Result<KernelMove, String> {
    let offset = value
        .get("offset")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kernel move {index} must contain an offset array"))?;
    if offset.len() != 3 {
        return Err(format!("kernel move {index} offset must have three values"));
    }

    let mut parsed_offset = [0isize; 3];
    for axis in 0..3 {
        let Some(value) = offset[axis].as_i64() else {
            return Err(format!(
                "kernel move {index} offset values must be integers"
            ));
        };
        parsed_offset[axis] = value
            .try_into()
            .map_err(|_| format!("kernel move {index} offset value is out of range"))?;
    }

    let cost = value
        .get("cost")
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("kernel move {index} must contain a numeric cost"))?;

    Ok(KernelMove {
        offset: parsed_offset,
        cost,
    })
}

fn print_summary(triangles: &[Triangle], occupancy: &[u8], grid: Grid, paths: &SliceOutputPaths) {
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();
    println!("Loaded {} triangles", triangles.len());
    println!(
        "Grid: {} x {} x {} voxels",
        grid.dims[0], grid.dims[1], grid.dims[2]
    );
    println!(
        "Voxel size: {:.6} x {:.6} x {:.6} mm",
        grid.voxel_size.x, grid.voxel_size.y, grid.voxel_size.z
    );
    println!(
        "Actual size: {:.6} x {:.6} x {:.6} mm",
        grid.actual_size.x, grid.actual_size.y, grid.actual_size.z
    );
    println!("Occupied: {occupied_count} / {}", occupancy.len());
    println!("Wrote {}", paths.volume.display());
    println!("Wrote {}", paths.image.display());
    if let Some(mesh) = &paths.mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(mesh) = &paths.clipped_mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(gcode) = &paths.gcode {
        println!("Wrote {}", gcode.display());
    }
    println!("Wrote {}", paths.metadata.display());
}

fn log_timing(label: &str, duration: Duration) {
    eprintln!("timing: {label}: {:.3}s", duration.as_secs_f64());
}
