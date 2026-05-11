mod cli;

use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use cli::{FieldMethod, parse_args};
use rayon::prelude::*;
use serde_json::Value;
use shockwave_clip::{TriangleSolid, clip_mesh_to_solid};
use shockwave_core::geometry::{Bounds, Triangle, Vec3, mesh_bounds};
use shockwave_core::grid::{Grid, GridSpec, build_grid};
use shockwave_gcode::{MarlinConfig, write_marlin_gcode};
use shockwave_iso::{Isosurface, IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_output::{
    Metadata, MetadataDocument, build_atlas, metadata_json, write_occupancy_bmp, write_ply_binary,
};
use shockwave_path::{ContourOptions, LayerToolpaths, layer_toolpaths_from_boundary};
use shockwave_stl::parse_stl;
use shockwave_voxel::field::{
    AnisotropicEuclideanPropagation, ExplicitKernelPropagation, Field, KernelMove, KernelPathCheck,
    PropagationConstraints, PropagationMethod, StderrProgress, expand_field,
    propagate_field_with_progress,
};
use shockwave_voxel::voxelize::generate_occupancy;

const FIELD_EXTENSION_VOXELS: usize = 2;

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
    let (grid, occupancy, field) = voxelize(&config, &triangles)?;
    let paths = write_outputs(&config, &triangles, &occupancy, &field, grid)?;
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

fn voxelize(
    config: &cli::Config,
    triangles: &[Triangle],
) -> Result<(Grid, Vec<u8>, Option<Field>), String> {
    let bounds = mesh_bounds(triangles);
    let grid_start = Instant::now();
    let grid = build_grid(
        GridSpec {
            voxel_size: config.voxel_size,
            requested_size: config.requested_size,
            padding_voxels: config.padding_voxels,
            origin: config.origin,
        },
        bounds,
    )?;
    log_timing("build grid", grid_start.elapsed());

    let occupancy_start = Instant::now();
    let occupancy = generate_occupancy(triangles, grid);
    log_timing("generate occupancy", occupancy_start.elapsed());

    let field = if config.field_enabled {
        Some(propagate_configured_field(config, &occupancy, grid)?)
    } else {
        None
    };
    Ok((grid, occupancy, field))
}

fn propagate_configured_field(
    config: &cli::Config,
    occupancy: &[u8],
    grid: Grid,
) -> Result<Field, String> {
    if let Some(kernel_path) = &config.kernel_path {
        let load_start = Instant::now();
        let propagation = load_kernel_propagation(kernel_path)?;
        log_timing("load kernel", load_start.elapsed());
        eprintln!("Loaded kernel with {} moves", propagation.move_count());
        propagate_and_expand(config, occupancy, grid, &propagation)
    } else {
        match config.field_method {
            FieldMethod::Anisotropic => {
                let propagation = AnisotropicEuclideanPropagation::new(config.field_rate);
                propagate_and_expand(config, occupancy, grid, &propagation)
            }
            FieldMethod::Trapezoid => {
                let load_start = Instant::now();
                let propagation = trapezoid_propagation(grid)?;
                log_timing("generate trapezoid kernel", load_start.elapsed());
                eprintln!(
                    "Generated native trapezoid kernel with {} moves",
                    propagation.move_count()
                );
                propagate_and_expand(config, occupancy, grid, &propagation)
            }
        }
    }
}

fn trapezoid_propagation(grid: Grid) -> Result<ExplicitKernelPropagation, String> {
    let moves = trapezoid_kernel_moves(TrapezoidKernel {
        voxel_size: grid.voxel_size,
        r1: 2.0,
        r2: 0.2,
        half_height: 0.5,
        z_offset: 0.5,
        surface_cost: 1.0,
        max_cost: 2.0,
    })?;
    ExplicitKernelPropagation::new(moves, KernelPathCheck::SweptOccupied)
}

#[derive(Clone, Copy)]
struct TrapezoidKernel {
    voxel_size: Vec3,
    r1: f64,
    r2: f64,
    half_height: f64,
    z_offset: f64,
    surface_cost: f64,
    max_cost: f64,
}

fn trapezoid_kernel_moves(kernel: TrapezoidKernel) -> Result<Vec<KernelMove>, String> {
    let outer_margin = kernel.max_cost - kernel.surface_cost;
    if outer_margin < 0.0 {
        return Err("trapezoid max cost must be greater than or equal to surface cost".to_string());
    }
    let vertical_cost_scale = trapezoid_vertical_cost_scale(kernel)?;

    let max_radius_mm = kernel.r1.max(kernel.r2) + outer_margin;
    let min_z_mm = -kernel.z_offset - kernel.half_height - outer_margin;
    let max_z_mm = -kernel.z_offset + kernel.half_height + outer_margin;
    let radius_x = (max_radius_mm / kernel.voxel_size.x).ceil() as isize;
    let radius_y = (max_radius_mm / kernel.voxel_size.y).ceil() as isize;
    let radius_z = (min_z_mm.abs().max(max_z_mm.abs()) / kernel.voxel_size.z).ceil() as isize;
    let mut moves = Vec::new();

    for dz in -radius_z..=radius_z {
        for dy in -radius_y..=radius_y {
            for dx in -radius_x..=radius_x {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }

                let radial_mm = ((dx as f64 * kernel.voxel_size.x).powi(2)
                    + (dy as f64 * kernel.voxel_size.y).powi(2))
                .sqrt();
                let z_mm = dz as f64 * kernel.voxel_size.z;
                let sdf = sd_trapezoid(
                    [radial_mm, z_mm + kernel.z_offset],
                    kernel.r1,
                    kernel.r2,
                    kernel.half_height,
                );
                let raw_cost = kernel.surface_cost + sdf;
                if raw_cost > 0.0 && raw_cost <= kernel.max_cost {
                    let mut cost = raw_cost / vertical_cost_scale;
                    if dz > 0 {
                        cost = cost.max(dz as f64 * kernel.voxel_size.z);
                    }
                    moves.push(KernelMove {
                        offset: [dx, dy, dz],
                        cost,
                    });
                }
            }
        }
    }

    Ok(moves)
}

fn trapezoid_vertical_cost_scale(kernel: TrapezoidKernel) -> Result<f64, String> {
    let one_voxel_up_cost = kernel.surface_cost
        + sd_trapezoid(
            [0.0, kernel.voxel_size.z + kernel.z_offset],
            kernel.r1,
            kernel.r2,
            kernel.half_height,
        );
    if one_voxel_up_cost <= 0.0 || !one_voxel_up_cost.is_finite() {
        return Err("trapezoid vertical cost scale must be finite and positive".to_string());
    }

    Ok(one_voxel_up_cost / kernel.voxel_size.z)
}

fn sd_trapezoid(p: [f64; 2], r1: f64, r2: f64, half_height: f64) -> f64 {
    let k1 = [r2, half_height];
    let k2 = [r2 - r1, 2.0 * half_height];
    let px = p[0].abs();
    let py = p[1];

    let ca = [
        (px - if py < 0.0 { r1 } else { r2 }).max(0.0),
        py.abs() - half_height,
    ];
    let k1_minus_p = [k1[0] - px, k1[1] - py];
    let h = ((k1_minus_p[0] * k2[0] + k1_minus_p[1] * k2[1]) / dot2(k2)).clamp(0.0, 1.0);
    let cb = [px - k1[0] + k2[0] * h, py - k1[1] + k2[1] * h];
    let sign = if cb[0] < 0.0 && ca[1] < 0.0 {
        -1.0
    } else {
        1.0
    };
    sign * dot2(ca).min(dot2(cb)).sqrt()
}

fn dot2(v: [f64; 2]) -> f64 {
    v[0] * v[0] + v[1] * v[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_kernel_uses_millimeter_vertical_costs() {
        let kernel = TrapezoidKernel {
            voxel_size: Vec3 {
                x: 0.4,
                y: 0.4,
                z: 0.4,
            },
            r1: 2.0,
            r2: 0.2,
            half_height: 0.5,
            z_offset: 0.5,
            surface_cost: 1.0,
            max_cost: 2.0,
        };
        let moves = trapezoid_kernel_moves(kernel).unwrap();

        let one_up = moves
            .iter()
            .find(|kernel_move| kernel_move.offset == [0, 0, 1])
            .unwrap();
        let two_up = moves
            .iter()
            .find(|kernel_move| kernel_move.offset == [0, 0, 2])
            .unwrap();

        assert!((one_up.cost - 0.4).abs() < 1.0e-9);
        assert!((two_up.cost - 0.8).abs() < 1.0e-9);
    }

    #[test]
    fn perimeter_offsets_use_bead_centerlines() {
        let offsets = perimeter_offsets(3, 0.4);
        assert!((offsets[0] - 0.2).abs() < 1.0e-12);
        assert!((offsets[1] - 0.6).abs() < 1.0e-12);
        assert!((offsets[2] - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn model_floor_offset_uses_original_bounds_minimum() {
        let offset = model_floor_coordinate_offset(Bounds {
            min: Vec3 {
                x: -10.0,
                y: 2.0,
                z: -3.5,
            },
            max: Vec3 {
                x: 5.0,
                y: 8.0,
                z: 12.0,
            },
        });

        assert_eq!(offset.x, 10.0);
        assert_eq!(offset.y, -2.0);
        assert_eq!(offset.z, 3.5);
    }

    #[test]
    fn local_layer_height_uses_field_gradient() {
        let grid = Grid {
            origin: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            dims: [1, 1, 3],
            voxel_size: Vec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            actual_size: Vec3 {
                x: 1.0,
                y: 1.0,
                z: 3.0,
            },
        };
        let field = Field {
            distances: vec![0.0, 2.0, 4.0],
            max_distance: 4.0,
        };

        let height = local_layer_height(
            &field,
            grid,
            Vec3 {
                x: 0.5,
                y: 0.5,
                z: 1.5,
            },
            1.0,
            0.2,
        );

        assert!((height - 0.5).abs() < 1.0e-12);
    }
}

fn propagate_and_expand(
    config: &cli::Config,
    occupancy: &[u8],
    grid: Grid,
    propagation: &impl PropagationMethod,
) -> Result<Field, String> {
    let propagation_start = Instant::now();
    let mut progress = StderrProgress::new("propagate field");
    let mut field = propagate_field_with_progress(
        occupancy,
        grid,
        propagation,
        PropagationConstraints {
            max_unreached_below_mm: Some(config.max_unreached_below_mm),
            unreached_cone_angle_degrees: (config.unreached_cone_angle_degrees > 0.0)
                .then_some(config.unreached_cone_angle_degrees),
            unreached_cone_max_height_mm: Some(config.max_unreached_below_mm),
        },
        &mut progress,
    )?;
    log_timing("propagate field", propagation_start.elapsed());

    let expansion_start = Instant::now();
    expand_field(&mut field, grid, FIELD_EXTENSION_VOXELS, propagation);
    log_timing("expand field", expansion_start.elapsed());
    Ok(field)
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
    volume: std::path::PathBuf,
    image: std::path::PathBuf,
    mesh: Option<std::path::PathBuf>,
    clipped_mesh: Option<std::path::PathBuf>,
    gcode: Option<std::path::PathBuf>,
    metadata: std::path::PathBuf,
}

fn write_outputs(
    config: &cli::Config,
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
    let mesh_path = field
        .as_ref()
        .map(|_| config.output_prefix.with_extension("ply"));
    let clipped_mesh_path = field
        .as_ref()
        .map(|_| suffixed_output_path(config, "clipped", "ply"));
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

    if let (Some(field), Some(mesh_path)) = (field.as_ref(), mesh_path.as_ref()) {
        let extract_start = Instant::now();
        let mesh = extract_regular_isosurfaces(field, grid, config.iso_spacing)?;
        log_timing("extract isosurfaces", extract_start.elapsed());
        eprintln!(
            "timing: isosurface set produced {} surfaces, {} vertices and {} triangles",
            mesh.surfaces.len(),
            mesh.vertex_count(),
            mesh.triangle_count()
        );

        let ply_start = Instant::now();
        write_ply_binary(mesh_path, &mesh)
            .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
        log_timing("write ply", ply_start.elapsed());

        if let Some(clipped_mesh_path) = clipped_mesh_path.as_ref() {
            let clip_start = Instant::now();
            let clipped_mesh = clip_isosurfaces_to_solid(&mesh, triangles);
            log_timing("clip isosurfaces", clip_start.elapsed());
            eprintln!(
                "timing: clipped isosurface set produced {} surfaces, {} vertices and {} triangles",
                clipped_mesh.surfaces.len(),
                clipped_mesh.vertex_count(),
                clipped_mesh.triangle_count()
            );

            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            log_timing("write clipped ply", clipped_ply_start.elapsed());

            if let Some(gcode_path) = gcode_path.as_ref() {
                let path_start = Instant::now();
                let layers = toolpaths_from_isosurfaces(&clipped_mesh, config, field, grid)?;
                log_timing("generate perimeter paths", path_start.elapsed());
                eprintln!(
                    "timing: generated {} toolpath layers with {} paths",
                    layers.len(),
                    layers.iter().map(LayerToolpaths::path_count).sum::<usize>()
                );

                let gcode_start = Instant::now();
                let gcode = write_marlin_gcode(
                    &layers,
                    MarlinConfig {
                        filament_diameter_mm: config.filament_diameter_mm,
                        coordinate_offset: model_floor_coordinate_offset(bounds),
                        ..Default::default()
                    },
                )?;
                fs::write(gcode_path, gcode).map_err(|error| {
                    format!("failed to write {}: {error}", gcode_path.display())
                })?;
                log_timing("write gcode", gcode_start.elapsed());
            }
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
                field_method: if config.kernel_path.is_some() {
                    "explicit-kernel"
                } else {
                    config.field_method.name()
                },
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

fn toolpaths_from_isosurfaces(
    surfaces: &IsosurfaceSet,
    config: &cli::Config,
    field: &Field,
    grid: Grid,
) -> Result<Vec<LayerToolpaths>, String> {
    let offsets = perimeter_offsets(config.wall_count, config.extrusion_width_mm);
    let options = ContourOptions {
        extrusion_width_mm: config.extrusion_width_mm,
        layer_height_mm: config.nominal_layer_height_mm,
        ..Default::default()
    };

    surfaces
        .surfaces
        .par_iter()
        .filter(|surface| !surface.mesh.is_empty())
        .map(|surface| {
            let mut layer = layer_toolpaths_from_boundary(
                &surface.mesh,
                surface.value,
                &offsets,
                config.infill_spacing_mm,
                options,
            )?;
            apply_local_layer_heights(
                &mut layer,
                field,
                grid,
                config.iso_spacing,
                config.nominal_layer_height_mm,
            );
            Ok(layer)
        })
        .collect()
}

fn perimeter_offsets(wall_count: usize, extrusion_width_mm: f64) -> Vec<f64> {
    (0..wall_count)
        .map(|index| (index as f64 + 0.5) * extrusion_width_mm)
        .collect()
}

fn model_floor_coordinate_offset(bounds: Bounds) -> Vec3 {
    Vec3 {
        x: -bounds.min.x,
        y: -bounds.min.y,
        z: -bounds.min.z,
    }
}

fn apply_local_layer_heights(
    layer: &mut LayerToolpaths,
    field: &Field,
    grid: Grid,
    iso_spacing: f64,
    fallback_height: f64,
) {
    for path in &mut layer.paths {
        for point in &mut path.points {
            point.layer_height_mm =
                local_layer_height(field, grid, point.position, iso_spacing, fallback_height);
        }
    }
}

fn local_layer_height(
    field: &Field,
    grid: Grid,
    position: Vec3,
    iso_spacing: f64,
    fallback_height: f64,
) -> f64 {
    let Some(gradient) = field_gradient_at_nearest_voxel(field, grid, position) else {
        return fallback_height;
    };
    let gradient_length =
        (gradient.x * gradient.x + gradient.y * gradient.y + gradient.z * gradient.z).sqrt();
    if gradient_length <= 1.0e-9 || !gradient_length.is_finite() {
        return fallback_height;
    }

    let height = iso_spacing / gradient_length;
    if height.is_finite() && height > 0.0 {
        height
    } else {
        fallback_height
    }
}

fn field_gradient_at_nearest_voxel(field: &Field, grid: Grid, position: Vec3) -> Option<Vec3> {
    if field.distances.len() != grid.voxel_count() {
        return None;
    }
    let coords = nearest_voxel_coords(grid, position);
    Some(Vec3 {
        x: axis_gradient(field, grid, coords, 0).unwrap_or(0.0),
        y: axis_gradient(field, grid, coords, 1).unwrap_or(0.0),
        z: axis_gradient(field, grid, coords, 2).unwrap_or(0.0),
    })
}

fn nearest_voxel_coords(grid: Grid, position: Vec3) -> [usize; 3] {
    [
        nearest_voxel_axis(position.x, grid.origin.x, grid.voxel_size.x, grid.dims[0]),
        nearest_voxel_axis(position.y, grid.origin.y, grid.voxel_size.y, grid.dims[1]),
        nearest_voxel_axis(position.z, grid.origin.z, grid.voxel_size.z, grid.dims[2]),
    ]
}

fn nearest_voxel_axis(position: f64, origin: f64, voxel_size: f64, dim: usize) -> usize {
    let voxel = ((position - origin) / voxel_size - 0.5).round();
    voxel.clamp(0.0, dim.saturating_sub(1) as f64) as usize
}

fn axis_gradient(field: &Field, grid: Grid, coords: [usize; 3], axis: usize) -> Option<f64> {
    let center = field.distances[grid.index(coords[0], coords[1], coords[2])];
    if !center.is_finite() {
        return None;
    }

    let mut minus = coords;
    let mut plus = coords;
    let has_minus = coords[axis] > 0;
    let has_plus = coords[axis] + 1 < grid.dims[axis];
    if has_minus {
        minus[axis] -= 1;
    }
    if has_plus {
        plus[axis] += 1;
    }

    let minus_value = has_minus
        .then(|| field.distances[grid.index(minus[0], minus[1], minus[2])])
        .filter(|value| value.is_finite());
    let plus_value = has_plus
        .then(|| field.distances[grid.index(plus[0], plus[1], plus[2])])
        .filter(|value| value.is_finite());
    let spacing = match axis {
        0 => grid.voxel_size.x,
        1 => grid.voxel_size.y,
        2 => grid.voxel_size.z,
        _ => unreachable!(),
    };

    match (minus_value, plus_value) {
        (Some(minus), Some(plus)) => Some((plus - minus) / (2.0 * spacing)),
        (Some(minus), None) => Some((center - minus) / spacing),
        (None, Some(plus)) => Some((plus - center) / spacing),
        (None, None) => None,
    }
}

fn clip_isosurfaces_to_solid(surfaces: &IsosurfaceSet, triangles: &[Triangle]) -> IsosurfaceSet {
    let solid = TriangleSolid::new(triangles.to_vec());
    IsosurfaceSet {
        surfaces: surfaces
            .surfaces
            .par_iter()
            .map(|surface| Isosurface {
                level: surface.level,
                value: surface.value,
                mesh: clip_mesh_to_solid(&surface.mesh, &solid),
            })
            .collect(),
    }
}

fn suffixed_output_path(config: &cli::Config, suffix: &str, extension: &str) -> std::path::PathBuf {
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
