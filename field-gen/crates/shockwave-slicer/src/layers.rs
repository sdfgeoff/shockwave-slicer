use std::f64::consts::FRAC_PI_4;

use rayon::prelude::*;
use shockwave_clip::{TriangleSolid, clip_mesh_to_solid};
use shockwave_iso::{Isosurface, IsosurfaceSet};
use shockwave_math::geometry::{Triangle, Vec3};
use shockwave_math::grid::Grid;
use shockwave_path::{ContourOptions, LayerToolpaths, layer_toolpaths_from_boundary};
use shockwave_voxel::field::Field;

use crate::error::SliceResult;
use crate::settings::SliceSettings;

pub fn toolpaths_from_isosurfaces(
    surfaces: &IsosurfaceSet,
    settings: &SliceSettings,
    field: &Field,
    grid: Grid,
) -> SliceResult<Vec<LayerToolpaths>> {
    let offsets = perimeter_offsets(settings.wall_count, settings.extrusion_width_mm);
    let options = ContourOptions {
        extrusion_width_mm: settings.extrusion_width_mm,
        layer_height_mm: settings.iso_spacing,
        ..Default::default()
    };

    Ok(surfaces
        .surfaces
        .par_iter()
        .filter(|surface| !surface.mesh.is_empty())
        .map(|surface| {
            let infill_angle = if surface.level % 2 == 0 {
                -FRAC_PI_4
            } else {
                FRAC_PI_4
            };
            let mut layer = layer_toolpaths_from_boundary(
                &surface.mesh,
                surface.value,
                &offsets,
                settings.infill_spacing_mm,
                infill_angle,
                options,
            )?;
            apply_local_layer_heights(
                &mut layer,
                field,
                grid,
                settings.iso_spacing,
                surface.value,
            )?;
            Ok::<LayerToolpaths, crate::error::SliceError>(layer)
        })
        .collect::<SliceResult<Vec<_>>>()?)
}

pub fn perimeter_offsets(wall_count: usize, extrusion_width_mm: f64) -> Vec<f64> {
    (0..wall_count)
        .rev()
        .map(|index| (index as f64 + 0.5) * extrusion_width_mm)
        .collect()
}

pub fn apply_local_layer_heights(
    layer: &mut LayerToolpaths,
    field: &Field,
    grid: Grid,
    iso_spacing: f64,
    field_value: f64,
) -> SliceResult<()> {
    for path in &mut layer.paths {
        for point in &mut path.points {
            point.layer_height_mm =
                local_layer_height(field, grid, point.position, iso_spacing, field_value)?;
        }
    }
    Ok(())
}

pub fn local_layer_height(
    field: &Field,
    grid: Grid,
    position: Vec3,
    iso_spacing: f64,
    field_value: f64,
) -> SliceResult<f64> {
    let Some(gradient) = field_gradient_near_position(field, grid, position, field_value) else {
        return Err(format!(
            "field gradient is undefined at path point ({:.6}, {:.6}, {:.6})",
            position.x, position.y, position.z
        )
        .into());
    };
    let gradient_length =
        (gradient.x * gradient.x + gradient.y * gradient.y + gradient.z * gradient.z).sqrt();
    if gradient_length <= 1.0e-9 || !gradient_length.is_finite() {
        let diagnostic = field_sample_diagnostic(field, grid, position)
            .map(|text| format!("; {text}"))
            .unwrap_or_default();
        return Err(format!(
            "field gradient is invalid at path point ({:.6}, {:.6}, {:.6}): gradient=({:.6}, {:.6}, {:.6}){}",
            position.x, position.y, position.z, gradient.x, gradient.y, gradient.z, diagnostic
        )
        .into());
    }

    let height = iso_spacing / gradient_length;
    if height.is_finite() && height > 0.0 {
        Ok(height)
    } else {
        Err(format!(
            "local layer height is invalid at path point ({:.6}, {:.6}, {:.6}): iso_spacing={:.6}, gradient_length={:.6}",
            position.x, position.y, position.z, iso_spacing, gradient_length
        )
        .into())
    }
}

pub fn clip_isosurfaces_to_solid(
    surfaces: &IsosurfaceSet,
    triangles: &[Triangle],
) -> IsosurfaceSet {
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

fn field_sample_diagnostic(field: &Field, grid: Grid, position: Vec3) -> Option<String> {
    if field.distances.len() != grid.voxel_count() || grid.dims.iter().any(|dim| *dim < 2) {
        return None;
    }
    let (x, u) = cell_axis(position.x, grid.origin.x, grid.voxel_size.x, grid.dims[0]);
    let (y, v) = cell_axis(position.y, grid.origin.y, grid.voxel_size.y, grid.dims[1]);
    let (z, w) = cell_axis(position.z, grid.origin.z, grid.voxel_size.z, grid.dims[2]);
    let values = cell_values(field, grid, x, y, z);
    Some(format!(
        "cell=({x},{y},{z}) local=({u:.6},{v:.6},{w:.6}) values={values:?}"
    ))
}

fn field_gradient_near_position(
    field: &Field,
    grid: Grid,
    position: Vec3,
    target_value: f64,
) -> Option<Vec3> {
    if field.distances.len() != grid.voxel_count() || grid.dims.iter().any(|dim| *dim < 2) {
        return None;
    }
    let (x, u) = cell_axis(position.x, grid.origin.x, grid.voxel_size.x, grid.dims[0]);
    let (y, v) = cell_axis(position.y, grid.origin.y, grid.voxel_size.y, grid.dims[1]);
    let (z, w) = cell_axis(position.z, grid.origin.z, grid.voxel_size.z, grid.dims[2]);
    if let Some(gradient) = field_gradient_in_cell(field, grid, x, y, z, [u, v, w])
        .filter(|gradient| gradient_length(*gradient) > 1.0e-9)
    {
        return Some(gradient);
    }

    let mut best = None;
    for candidate_z in z.saturating_sub(1)..=(z + 1).min(grid.dims[2] - 2) {
        for candidate_y in y.saturating_sub(1)..=(y + 1).min(grid.dims[1] - 2) {
            for candidate_x in x.saturating_sub(1)..=(x + 1).min(grid.dims[0] - 2) {
                let local =
                    local_position_in_cell(position, grid, candidate_x, candidate_y, candidate_z);
                let Some((value, gradient)) = field_value_and_gradient_in_cell(
                    field,
                    grid,
                    candidate_x,
                    candidate_y,
                    candidate_z,
                    local,
                ) else {
                    continue;
                };
                let length = gradient_length(gradient);
                if length <= 1.0e-9 || !length.is_finite() {
                    continue;
                }
                let clamped_position =
                    position_from_cell_local(grid, candidate_x, candidate_y, candidate_z, local);
                let value_error = (value - target_value).abs();
                let score = value_error
                    + squared_distance(position, clamped_position).sqrt()
                        / grid
                            .voxel_size
                            .x
                            .max(grid.voxel_size.y)
                            .max(grid.voxel_size.z);
                match best {
                    Some((best_score, _)) if score >= best_score => {}
                    _ => best = Some((score, gradient)),
                }
            }
        }
    }

    best.map(|(_, gradient)| gradient)
}

fn field_gradient_in_cell(
    field: &Field,
    grid: Grid,
    x: usize,
    y: usize,
    z: usize,
    local: [f64; 3],
) -> Option<Vec3> {
    field_value_and_gradient_in_cell(field, grid, x, y, z, local).map(|(_, gradient)| gradient)
}

fn field_value_and_gradient_in_cell(
    field: &Field,
    grid: Grid,
    x: usize,
    y: usize,
    z: usize,
    local: [f64; 3],
) -> Option<(f64, Vec3)> {
    let values = cell_values(field, grid, x, y, z);
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let value = trilinear_value(values, local);
    let gradient = trilinear_gradient(values, local);
    Some((
        value,
        Vec3 {
            x: gradient[0] / grid.voxel_size.x,
            y: gradient[1] / grid.voxel_size.y,
            z: gradient[2] / grid.voxel_size.z,
        },
    ))
}

fn local_position_in_cell(position: Vec3, grid: Grid, x: usize, y: usize, z: usize) -> [f64; 3] {
    let base = position_from_cell_local(grid, x, y, z, [0.0, 0.0, 0.0]);
    [
        ((position.x - base.x) / grid.voxel_size.x).clamp(0.0, 1.0),
        ((position.y - base.y) / grid.voxel_size.y).clamp(0.0, 1.0),
        ((position.z - base.z) / grid.voxel_size.z).clamp(0.0, 1.0),
    ]
}

fn position_from_cell_local(grid: Grid, x: usize, y: usize, z: usize, local: [f64; 3]) -> Vec3 {
    Vec3 {
        x: grid.origin.x + (x as f64 + local[0] + 0.5) * grid.voxel_size.x,
        y: grid.origin.y + (y as f64 + local[1] + 0.5) * grid.voxel_size.y,
        z: grid.origin.z + (z as f64 + local[2] + 0.5) * grid.voxel_size.z,
    }
}

fn squared_distance(a: Vec3, b: Vec3) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dz = b.z - a.z;
    dx * dx + dy * dy + dz * dz
}

fn gradient_length(gradient: Vec3) -> f64 {
    (gradient.x * gradient.x + gradient.y * gradient.y + gradient.z * gradient.z).sqrt()
}

fn cell_axis(position: f64, origin: f64, voxel_size: f64, dim: usize) -> (usize, f64) {
    let coordinate = (position - origin) / voxel_size - 0.5;
    let cell = coordinate.floor().clamp(0.0, dim.saturating_sub(2) as f64) as usize;
    let local = (coordinate - cell as f64).clamp(0.0, 1.0);
    (cell, local)
}

fn cell_values(field: &Field, grid: Grid, x: usize, y: usize, z: usize) -> [f64; 8] {
    [
        field.distances[grid.index(x, y, z)],
        field.distances[grid.index(x + 1, y, z)],
        field.distances[grid.index(x, y + 1, z)],
        field.distances[grid.index(x + 1, y + 1, z)],
        field.distances[grid.index(x, y, z + 1)],
        field.distances[grid.index(x + 1, y, z + 1)],
        field.distances[grid.index(x, y + 1, z + 1)],
        field.distances[grid.index(x + 1, y + 1, z + 1)],
    ]
}

fn trilinear_value(values: [f64; 8], u: [f64; 3]) -> f64 {
    let [x, y, z] = u;
    let c00 = values[0] * (1.0 - x) + values[1] * x;
    let c10 = values[2] * (1.0 - x) + values[3] * x;
    let c01 = values[4] * (1.0 - x) + values[5] * x;
    let c11 = values[6] * (1.0 - x) + values[7] * x;
    let c0 = c00 * (1.0 - y) + c10 * y;
    let c1 = c01 * (1.0 - y) + c11 * y;
    c0 * (1.0 - z) + c1 * z
}

fn trilinear_gradient(values: [f64; 8], u: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = u;

    let dx00 = values[1] - values[0];
    let dx10 = values[3] - values[2];
    let dx01 = values[5] - values[4];
    let dx11 = values[7] - values[6];
    let dx0 = dx00 * (1.0 - y) + dx10 * y;
    let dx1 = dx01 * (1.0 - y) + dx11 * y;

    let dy00 = values[2] - values[0];
    let dy10 = values[3] - values[1];
    let dy01 = values[6] - values[4];
    let dy11 = values[7] - values[5];
    let dy0 = dy00 * (1.0 - x) + dy10 * x;
    let dy1 = dy01 * (1.0 - x) + dy11 * x;

    let dz00 = values[4] - values[0];
    let dz10 = values[5] - values[1];
    let dz01 = values[6] - values[2];
    let dz11 = values[7] - values[3];
    let dz0 = dz00 * (1.0 - x) + dz10 * x;
    let dz1 = dz01 * (1.0 - x) + dz11 * x;

    [
        dx0 * (1.0 - z) + dx1 * z,
        dy0 * (1.0 - z) + dy1 * z,
        dz0 * (1.0 - y) + dz1 * y,
    ]
}
