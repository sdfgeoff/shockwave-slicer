use std::collections::HashMap;

use shockwave_core::geometry::Vec3;
use shockwave_core::grid::Grid;
use shockwave_voxel::field::Field;

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

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
}

impl Mesh {
    pub fn append(&mut self, other: Mesh) {
        let vertex_offset = self.vertices.len();
        self.vertices.extend(other.vertices);
        self.triangles
            .extend(other.triangles.into_iter().map(|triangle| {
                [
                    triangle[0] + vertex_offset,
                    triangle[1] + vertex_offset,
                    triangle[2] + vertex_offset,
                ]
            }));
    }
}

#[derive(Clone, Debug)]
pub struct Isosurface {
    pub level: usize,
    pub value: f64,
    pub mesh: Mesh,
}

#[derive(Clone, Debug, Default)]
pub struct IsosurfaceSet {
    pub surfaces: Vec<Isosurface>,
}

impl IsosurfaceSet {
    pub fn vertex_count(&self) -> usize {
        self.surfaces
            .iter()
            .map(|surface| surface.mesh.vertices.len())
            .sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.surfaces
            .iter()
            .map(|surface| surface.mesh.triangles.len())
            .sum()
    }
}

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
                mesh: Mesh::default(),
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

pub fn extract_isosurface(field: &Field, grid: Grid, iso_value: f64) -> Result<Mesh, String> {
    if field.distances.len() != grid.voxel_count() {
        return Err("field length does not match grid dimensions".to_string());
    }
    if grid.dims[0] < 2 || grid.dims[1] < 2 || grid.dims[2] < 2 {
        return Ok(Mesh::default());
    }

    let cell_dims = [grid.dims[0] - 1, grid.dims[1] - 1, grid.dims[2] - 1];
    let mut mesh = Mesh::default();
    let mut cell_vertices = vec![None; cell_dims[0] * cell_dims[1] * cell_dims[2]];

    for z in 0..cell_dims[2] {
        for y in 0..cell_dims[1] {
            for x in 0..cell_dims[0] {
                if let Some(vertex) = cell_vertex(field, grid, x, y, z, iso_value) {
                    let vertex_index = mesh.vertices.len();
                    mesh.vertices.push(vertex);
                    cell_vertices[cell_index(cell_dims, x, y, z)] = Some(vertex_index);
                }
            }
        }
    }

    emit_surface_quads(field, grid, iso_value, cell_dims, &cell_vertices, &mut mesh);
    Ok(mesh)
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

fn emit_surface_quads(
    field: &Field,
    grid: Grid,
    iso_value: f64,
    cell_dims: [usize; 3],
    cell_vertices: &[Option<usize>],
    mesh: &mut Mesh,
) {
    for z in 1..grid.dims[2] - 1 {
        for y in 1..grid.dims[1] - 1 {
            for x in 0..grid.dims[0] - 1 {
                if has_grid_edge_crossing(field, grid, [x, y, z], [x + 1, y, z], iso_value) {
                    let cells = [[x, y - 1, z - 1], [x, y, z - 1], [x, y, z], [x, y - 1, z]];
                    emit_quad(
                        cell_dims,
                        cell_vertices,
                        mesh,
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
                if has_grid_edge_crossing(field, grid, [x, y, z], [x, y + 1, z], iso_value) {
                    let cells = [[x - 1, y, z - 1], [x, y, z - 1], [x, y, z], [x - 1, y, z]];
                    emit_quad(
                        cell_dims,
                        cell_vertices,
                        mesh,
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
                if has_grid_edge_crossing(field, grid, [x, y, z], [x, y, z + 1], iso_value) {
                    let cells = [[x - 1, y - 1, z], [x, y - 1, z], [x, y, z], [x - 1, y, z]];
                    emit_quad(
                        cell_dims,
                        cell_vertices,
                        mesh,
                        cells,
                        edge_is_ascending(field, grid, [x, y, z], [x, y, z + 1]),
                    );
                }
            }
        }
    }
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
) -> std::ops::RangeInclusive<usize> {
    let value_a = field.distances[grid.index(a[0], a[1], a[2])];
    let value_b = field.distances[grid.index(b[0], b[1], b[2])];
    if !value_a.is_finite() || !value_b.is_finite() || value_a == value_b || level_count == 0 {
        return 1..=0;
    }

    let min_value = value_a.min(value_b);
    let max_value = value_a.max(value_b);
    let first = ((min_value / spacing).floor() as usize + 1).max(1);
    let last = ((max_value / spacing).floor() as usize).min(level_count);

    first..=last
}

fn emit_quad(
    cell_dims: [usize; 3],
    cell_vertices: &[Option<usize>],
    mesh: &mut Mesh,
    cells: [[usize; 3]; 4],
    reverse: bool,
) {
    let Some(a) = cell_vertices[cell_index(cell_dims, cells[0][0], cells[0][1], cells[0][2])]
    else {
        return;
    };
    let Some(b) = cell_vertices[cell_index(cell_dims, cells[1][0], cells[1][1], cells[1][2])]
    else {
        return;
    };
    let Some(c) = cell_vertices[cell_index(cell_dims, cells[2][0], cells[2][1], cells[2][2])]
    else {
        return;
    };
    let Some(d) = cell_vertices[cell_index(cell_dims, cells[3][0], cells[3][1], cells[3][2])]
    else {
        return;
    };

    if reverse {
        mesh.triangles.push([a, c, b]);
        mesh.triangles.push([a, d, c]);
    } else {
        mesh.triangles.push([a, b, c]);
        mesh.triangles.push([a, c, d]);
    }
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

fn has_grid_edge_crossing(
    field: &Field,
    grid: Grid,
    a: [usize; 3],
    b: [usize; 3],
    iso_value: f64,
) -> bool {
    has_crossing(
        field.distances[grid.index(a[0], a[1], a[2])],
        field.distances[grid.index(b[0], b[1], b[2])],
        iso_value,
    )
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

        let mesh = extract_isosurface(&field, grid, 1.0).unwrap();

        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.triangles.len(), 2);
        for vertex in mesh.vertices {
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
