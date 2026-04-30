use std::collections::HashMap;

use shockwave_core::geometry::Vec3;
use shockwave_core::grid::Grid;
use shockwave_voxel::field::Field;

use crate::interpolate::{trilinear_gradient, trilinear_value};
use crate::mesh::{Isosurface, IsosurfaceSet};

const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

pub fn extract_regular_isosurfaces(
    field: &Field,
    grid: Grid,
    spacing: f64,
) -> Result<IsosurfaceSet, String> {
    if spacing <= 0.0 || !spacing.is_finite() {
        return Err("isosurface spacing must be greater than zero".to_string());
    }
    if field.distances.len() != grid.voxel_count() {
        return Err("field length does not match grid dimensions".to_string());
    }
    let level_count = ((field.max_distance / spacing).ceil() as usize).saturating_sub(1);
    let mut surfaces = IsosurfaceSet {
        surfaces: (1..=level_count)
            .map(|level| Isosurface {
                level,
                value: level as f64 * spacing,
                mesh: Default::default(),
            })
            .collect(),
    };

    if grid.dims[0] < 2 || grid.dims[1] < 2 || grid.dims[2] < 2 || level_count == 0 {
        return Ok(surfaces);
    }

    let cell_dims = [grid.dims[0] - 1, grid.dims[1] - 1, grid.dims[2] - 1];
    let mut cell_vertices = HashMap::new();

    emit_regular_surface_quads(
        field,
        grid,
        spacing,
        level_count,
        cell_dims,
        &mut cell_vertices,
        &mut surfaces,
    );

    Ok(surfaces)
}

fn cell_vertex(
    field: &Field,
    grid: Grid,
    x: usize,
    y: usize,
    z: usize,
    iso_value: f64,
) -> Option<Vec3> {
    let values = cell_values(field, grid, x, y, z);
    let mut crossing_sum = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let mut crossing_count = 0usize;

    for [a, b] in EDGES {
        let value_a = values[a];
        let value_b = values[b];
        if !has_crossing(value_a, value_b, iso_value) {
            continue;
        }

        let t = ((iso_value - value_a) / (value_b - value_a)).clamp(0.0, 1.0);
        let position_a = corner_position(grid, x, y, z, CORNERS[a]);
        let position_b = corner_position(grid, x, y, z, CORNERS[b]);
        crossing_sum.x += position_a.x + (position_b.x - position_a.x) * t;
        crossing_sum.y += position_a.y + (position_b.y - position_a.y) * t;
        crossing_sum.z += position_a.z + (position_b.z - position_a.z) * t;
        crossing_count += 1;
    }

    if crossing_count == 0 {
        return None;
    }

    let average = Vec3 {
        x: crossing_sum.x / crossing_count as f64,
        y: crossing_sum.y / crossing_count as f64,
        z: crossing_sum.z / crossing_count as f64,
    };

    project_to_cell_field(average, values, grid, x, y, z, iso_value).or(Some(average))
}

fn emit_regular_surface_quads(
    field: &Field,
    grid: Grid,
    spacing: f64,
    level_count: usize,
    cell_dims: [usize; 3],
    cell_vertices: &mut HashMap<(usize, usize), usize>,
    surfaces: &mut IsosurfaceSet,
) {
    for z in 1..grid.dims[2] - 1 {
        for y in 1..grid.dims[1] - 1 {
            for x in 0..grid.dims[0] - 1 {
                for level in
                    crossing_levels(field, grid, [x, y, z], [x + 1, y, z], spacing, level_count)
                {
                    let cells = [[x, y - 1, z - 1], [x, y, z - 1], [x, y, z], [x, y - 1, z]];
                    emit_regular_quad(
                        field,
                        grid,
                        spacing,
                        level,
                        cell_dims,
                        cell_vertices,
                        surfaces,
                        cells,
                        edge_is_ascending(field, grid, [x, y, z], [x + 1, y, z]),
                    );
                }
            }
        }
    }

    for z in 1..grid.dims[2] - 1 {
        for y in 0..grid.dims[1] - 1 {
            for x in 1..grid.dims[0] - 1 {
                for level in
                    crossing_levels(field, grid, [x, y, z], [x, y + 1, z], spacing, level_count)
                {
                    let cells = [[x - 1, y, z - 1], [x, y, z - 1], [x, y, z], [x - 1, y, z]];
                    emit_regular_quad(
                        field,
                        grid,
                        spacing,
                        level,
                        cell_dims,
                        cell_vertices,
                        surfaces,
                        cells,
                        !edge_is_ascending(field, grid, [x, y, z], [x, y + 1, z]),
                    );
                }
            }
        }
    }

    for z in 0..grid.dims[2] - 1 {
        for y in 1..grid.dims[1] - 1 {
            for x in 1..grid.dims[0] - 1 {
                for level in
                    crossing_levels(field, grid, [x, y, z], [x, y, z + 1], spacing, level_count)
                {
                    let cells = [[x - 1, y - 1, z], [x, y - 1, z], [x, y, z], [x - 1, y, z]];
                    emit_regular_quad(
                        field,
                        grid,
                        spacing,
                        level,
                        cell_dims,
                        cell_vertices,
                        surfaces,
                        cells,
                        edge_is_ascending(field, grid, [x, y, z], [x, y, z + 1]),
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_regular_quad(
    field: &Field,
    grid: Grid,
    spacing: f64,
    level: usize,
    cell_dims: [usize; 3],
    cell_vertices: &mut HashMap<(usize, usize), usize>,
    surfaces: &mut IsosurfaceSet,
    cells: [[usize; 3]; 4],
    reverse: bool,
) {
    let Some(a) = regular_cell_vertex(
        field,
        grid,
        spacing,
        level,
        cell_dims,
        cell_vertices,
        surfaces,
        cells[0],
    ) else {
        return;
    };
    let Some(b) = regular_cell_vertex(
        field,
        grid,
        spacing,
        level,
        cell_dims,
        cell_vertices,
        surfaces,
        cells[1],
    ) else {
        return;
    };
    let Some(c) = regular_cell_vertex(
        field,
        grid,
        spacing,
        level,
        cell_dims,
        cell_vertices,
        surfaces,
        cells[2],
    ) else {
        return;
    };
    let Some(d) = regular_cell_vertex(
        field,
        grid,
        spacing,
        level,
        cell_dims,
        cell_vertices,
        surfaces,
        cells[3],
    ) else {
        return;
    };

    let mesh = &mut surfaces.surfaces[level - 1].mesh;
    if reverse {
        mesh.triangles.push([a, c, b]);
        mesh.triangles.push([a, d, c]);
    } else {
        mesh.triangles.push([a, b, c]);
        mesh.triangles.push([a, c, d]);
    }
}

#[allow(clippy::too_many_arguments)]
fn regular_cell_vertex(
    field: &Field,
    grid: Grid,
    spacing: f64,
    level: usize,
    cell_dims: [usize; 3],
    cell_vertices: &mut HashMap<(usize, usize), usize>,
    surfaces: &mut IsosurfaceSet,
    cell: [usize; 3],
) -> Option<usize> {
    let cell_index = cell_index(cell_dims, cell[0], cell[1], cell[2]);
    let key = (level, cell_index);
    if let Some(vertex_index) = cell_vertices.get(&key) {
        return Some(*vertex_index);
    }

    let vertex = cell_vertex(
        field,
        grid,
        cell[0],
        cell[1],
        cell[2],
        level as f64 * spacing,
    )?;
    let mesh = &mut surfaces.surfaces[level - 1].mesh;
    let vertex_index = mesh.vertices.len();
    mesh.vertices.push(vertex);
    cell_vertices.insert(key, vertex_index);
    Some(vertex_index)
}

fn crossing_levels(
    field: &Field,
    grid: Grid,
    a: [usize; 3],
    b: [usize; 3],
    spacing: f64,
    level_count: usize,
) -> std::ops::Range<usize> {
    let value_a = field.distances[grid.index(a[0], a[1], a[2])];
    let value_b = field.distances[grid.index(b[0], b[1], b[2])];
    if !value_a.is_finite() || !value_b.is_finite() || value_a == value_b || level_count == 0 {
        return 0..0;
    }

    let min_value = value_a.min(value_b);
    let max_value = value_a.max(value_b);
    let first = ((min_value / spacing).floor() as usize + 1).max(1);
    let last = ((max_value / spacing).floor() as usize).min(level_count);

    first..last + 1
}

fn project_to_cell_field(
    position: Vec3,
    values: [f64; 8],
    grid: Grid,
    x: usize,
    y: usize,
    z: usize,
    iso_value: f64,
) -> Option<Vec3> {
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let base = corner_position(grid, x, y, z, [0, 0, 0]);
    let mut u = [
        ((position.x - base.x) / grid.voxel_size.x).clamp(0.0, 1.0),
        ((position.y - base.y) / grid.voxel_size.y).clamp(0.0, 1.0),
        ((position.z - base.z) / grid.voxel_size.z).clamp(0.0, 1.0),
    ];

    for _ in 0..6 {
        let value = trilinear_value(values, u) - iso_value;
        if value.abs() < 1.0e-8 {
            break;
        }

        let gradient = trilinear_gradient(values, u);
        let gradient_len_sq =
            gradient[0] * gradient[0] + gradient[1] * gradient[1] + gradient[2] * gradient[2];
        if gradient_len_sq < 1.0e-16 {
            return None;
        }

        u[0] = (u[0] - value * gradient[0] / gradient_len_sq).clamp(0.0, 1.0);
        u[1] = (u[1] - value * gradient[1] / gradient_len_sq).clamp(0.0, 1.0);
        u[2] = (u[2] - value * gradient[2] / gradient_len_sq).clamp(0.0, 1.0);
    }

    Some(Vec3 {
        x: base.x + u[0] * grid.voxel_size.x,
        y: base.y + u[1] * grid.voxel_size.y,
        z: base.z + u[2] * grid.voxel_size.z,
    })
}

fn cell_values(field: &Field, grid: Grid, x: usize, y: usize, z: usize) -> [f64; 8] {
    CORNERS.map(|corner| field.distances[grid.index(x + corner[0], y + corner[1], z + corner[2])])
}

fn corner_position(grid: Grid, x: usize, y: usize, z: usize, corner: [usize; 3]) -> Vec3 {
    Vec3 {
        x: grid.origin.x + (x + corner[0]) as f64 * grid.voxel_size.x + grid.voxel_size.x * 0.5,
        y: grid.origin.y + (y + corner[1]) as f64 * grid.voxel_size.y + grid.voxel_size.y * 0.5,
        z: grid.origin.z + (z + corner[2]) as f64 * grid.voxel_size.z + grid.voxel_size.z * 0.5,
    }
}

fn edge_is_ascending(field: &Field, grid: Grid, a: [usize; 3], b: [usize; 3]) -> bool {
    field.distances[grid.index(a[0], a[1], a[2])] < field.distances[grid.index(b[0], b[1], b[2])]
}

fn has_crossing(a: f64, b: f64, iso_value: f64) -> bool {
    a.is_finite()
        && b.is_finite()
        && ((a < iso_value && b >= iso_value) || (b < iso_value && a >= iso_value))
}

fn cell_index(cell_dims: [usize; 3], x: usize, y: usize, z: usize) -> usize {
    x + y * cell_dims[0] + z * cell_dims[0] * cell_dims[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            origin: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            dims: [3, 3, 3],
            voxel_size: Vec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            actual_size: Vec3 {
                x: 3.0,
                y: 3.0,
                z: 3.0,
            },
        }
    }

    #[test]
    fn extracts_plane_from_linear_field() {
        let grid = grid();
        let mut distances = Vec::new();
        for z in 0..grid.dims[2] {
            for y in 0..grid.dims[1] {
                for x in 0..grid.dims[0] {
                    distances.push(corner_position(grid, x, y, z, [0, 0, 0]).x);
                }
            }
        }
        let field = Field {
            distances,
            max_distance: 2.5,
        };

        let surfaces = extract_regular_isosurfaces(&field, grid, 1.0).unwrap();

        assert_eq!(surfaces.surfaces.len(), 2);
        assert_eq!(surfaces.surfaces[0].mesh.vertices.len(), 4);
        assert_eq!(surfaces.surfaces[0].mesh.triangles.len(), 2);
        for vertex in &surfaces.surfaces[0].mesh.vertices {
            assert!((vertex.x - 1.0).abs() < 1.0e-8);
        }
    }

    #[test]
    fn regular_isosurfaces_rejects_non_positive_spacing() {
        let grid = grid();
        let field = Field {
            distances: vec![0.0; grid.voxel_count()],
            max_distance: 0.0,
        };

        assert!(extract_regular_isosurfaces(&field, grid, 0.0).is_err());
    }
}
