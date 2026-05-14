mod cli;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cli::parse_args;
use serde_json::Value;
use shockwave_iso::{IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_math::geometry::{Triangle, mesh_bounds};
use shockwave_math::grid::Grid;
use shockwave_output::{
    Metadata, MetadataDocument, build_atlas, metadata_json, write_occupancy_bmp, write_ply_binary,
};
use shockwave_path::LayerToolpaths;
use shockwave_slicer::{
    FIELD_EXTENSION_VOXELS, FieldPropagation, SliceProgress, SliceSettings,
    clip_isosurfaces_to_solid, toolpaths_from_isosurfaces, voxelize, write_gcode,
};
use shockwave_stl::parse_stl;
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
    let read_start = Instant::now();
    let bytes = fs::read(&config.input)
        .map_err(|error| format!("failed to read {}: {error}", config.input.display()))?;
    log_timing("read stl", read_start.elapsed());

    let parse_start = Instant::now();
    let triangles = parse_stl(&bytes)?;
    log_timing("parse stl", parse_start.elapsed());
    if triangles.is_empty() {
        return Err("STL did not contain any triangles".to_string());
    }
    Ok(triangles)
}

fn slicer_settings_from_config(config: &cli::Config) -> Result<SliceSettings, String> {
    let propagation = if let Some(kernel_path) = &config.kernel_path {
        let load_start = Instant::now();
        let propagation = load_kernel_propagation(kernel_path)?;
        log_timing("load kernel", load_start.elapsed());
        eprintln!("Loaded kernel with {} moves", propagation.move_count());
        FieldPropagation::ExplicitKernel(propagation)
    } else {
        FieldPropagation::from_method(config.field_method, config.field_rate)
    };

    Ok(SliceSettings {
        voxel_size: config.voxel_size,
        requested_size: config.requested_size,
        padding_voxels: config.padding_voxels,
        origin: config.origin,
        field_enabled: config.field_enabled,
        propagation,
        field_rate: config.field_rate,
        max_unreached_below_mm: config.max_unreached_below_mm,
        unreached_cone_angle_degrees: config.unreached_cone_angle_degrees,
        iso_spacing: config.iso_spacing,
        wall_count: config.wall_count,
        extrusion_width_mm: config.extrusion_width_mm,
        filament_diameter_mm: config.filament_diameter_mm,
        infill_spacing_mm: config.infill_spacing_mm,
    })
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
) -> Result<OutputPaths, String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let volume_path = config.output_prefix.with_extension("occ");
    let image_path = config.output_prefix.with_extension("bmp");
    let mesh_path =
        (field.is_some() && config.export_ply).then(|| config.output_prefix.with_extension("ply"));
    let clipped_mesh_path = (field.is_some() && config.export_ply)
        .then(|| suffixed_output_path(config, "clipped", "ply"));
    let gcode_path = config
        .gcode_enabled
        .then(|| config.output_prefix.with_extension("gcode"));
    let metadata_path = config.output_prefix.with_extension("json");

    let occ_start = Instant::now();
    fs::write(&volume_path, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", volume_path.display()))?;
    log_timing("write occ", occ_start.elapsed());

    let bmp_start = Instant::now();
    write_occupancy_bmp(&image_path, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", image_path.display()))?;
    log_timing("write bmp", bmp_start.elapsed());

    if let Some(field) = field
        .as_ref()
        .filter(|_| config.export_ply || config.gcode_enabled)
    {
        let mesh = extract_isosurfaces_with_timing(field, grid, settings.iso_spacing)?;

        if let Some(mesh_path) = mesh_path.as_ref() {
            let ply_start = Instant::now();
            write_ply_binary(mesh_path, &mesh)
                .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
            log_timing("write ply", ply_start.elapsed());
        }

        let clipped_mesh = clip_isosurfaces_with_timing(&mesh, triangles);

        if let Some(clipped_mesh_path) = clipped_mesh_path.as_ref() {
            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            log_timing("write clipped ply", clipped_ply_start.elapsed());
        }

        if let Some(gcode_path) = gcode_path.as_ref() {
            write_gcode_with_timing(gcode_path, &clipped_mesh, settings, triangles, field, grid)?;
        }
    }

    let metadata_start = Instant::now();
    fs::write(
        &metadata_path,
        metadata_json(&MetadataDocument {
            metadata: Metadata {
                input: &config.input.display().to_string(),
                voxel_size: config.voxel_size,
                padding_voxels: config.padding_voxels,
                field_enabled: config.field_enabled,
                field_method: settings.field_method_name(),
                kernel_path: config
                    .kernel_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .as_deref(),
                field_rate: config.field_rate,
                max_unreached_below_mm: config.max_unreached_below_mm,
                unreached_cone_angle_degrees: config.unreached_cone_angle_degrees,
                field_extension_voxels: FIELD_EXTENSION_VOXELS,
                iso_spacing: config.iso_spacing,
            },
            bounds,
            grid,
            atlas,
            volume_path: &volume_path,
            image_path: &image_path,
            mesh_path: mesh_path.as_deref(),
            clipped_mesh_path: clipped_mesh_path.as_deref(),
            field: field.as_ref(),
            occupied_count,
            voxel_count: occupancy.len(),
        }),
    )
    .map_err(|error| format!("failed to write {}: {error}", metadata_path.display()))?;
    log_timing("write metadata", metadata_start.elapsed());

    Ok(OutputPaths {
        volume: volume_path,
        image: image_path,
        mesh: mesh_path,
        clipped_mesh: clipped_mesh_path,
        gcode: gcode_path,
        metadata: metadata_path,
    })
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
    let mut bytes = Vec::new();
    write_gcode(&mut bytes, &layers, mesh_bounds(triangles), settings)
        .map_err(|error| error.to_string())?;
    fs::write(gcode_path, bytes)
        .map_err(|error| format!("failed to write {}: {error}", gcode_path.display()))?;
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

struct OutputPaths {
    volume: PathBuf,
    image: PathBuf,
    mesh: Option<PathBuf>,
    clipped_mesh: Option<PathBuf>,
    gcode: Option<PathBuf>,
    metadata: PathBuf,
}

fn suffixed_output_path(config: &cli::Config, suffix: &str, extension: &str) -> PathBuf {
    let mut path = config.output_prefix.clone();
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("occupancy");
    path.set_file_name(format!("{stem}-{suffix}"));
    path.set_extension(extension);
    path
}

fn print_summary(triangles: &[Triangle], occupancy: &[u8], grid: Grid, paths: &OutputPaths) {
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
