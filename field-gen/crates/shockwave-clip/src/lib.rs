//! Triangle-mesh clipping foundations.
//!
//! This crate owns solid classification and triangle-intersection primitives.
//! Actual triangle splitting is intentionally kept separate so callers do not
//! mistake "classified" triangles for a fully clipped layer mesh.

use std::collections::{HashMap, HashSet};

mod aabb;
mod classify;
mod intersect;

use shockwave_core::geometry::{Triangle, Vec3};
use shockwave_mesh::Mesh;

use crate::aabb::Aabb;

pub use classify::{PointClassification, TriangleSolid};
pub use intersect::{TriangleIntersection, triangles_intersect};

const EPSILON: f64 = 1.0e-9;
const SPLIT_DISTANCE_EPSILON: f64 = 1.0e-6;
const PLANE_MERGE_SCALE: f64 = 1.0e6;
const VERTEX_MERGE_SCALE: f64 = 1.0e6;

#[derive(Clone, Debug)]
pub struct MeshTriangleClassification {
    pub triangle_index: usize,
    pub state: ClippingState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClippingState {
    Inside,
    Outside,
    Boundary,
}

pub fn clip_mesh_to_solid(mesh: &Mesh, solid: &TriangleSolid) -> Mesh {
    let mut clipped = Mesh::default();
    let mut vertices = VertexMap::default();

    for indices in &mesh.triangles {
        let triangle = Triangle {
            vertices: mesh.triangle_vertices(*indices),
        };

        let mut splitter_planes = HashSet::new();
        let candidates = solid
            .intersecting_triangle_indices(&triangle)
            .filter(|index| is_proper_splitter(&triangle, &solid.triangles()[*index]))
            .filter(|index| {
                splitter_planes.insert(PlaneKey::from_triangle(&solid.triangles()[*index]))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            if matches!(
                solid.classify_point(triangle_centroid(&triangle)),
                PointClassification::Inside | PointClassification::Boundary
            ) {
                push_triangle(&mut clipped, &mut vertices, triangle.vertices);
            }
            continue;
        }

        for fragment in clip_triangle_to_solid(&triangle, solid, &candidates) {
            push_triangle(&mut clipped, &mut vertices, fragment);
        }
    }

    clipped
}

pub fn classify_mesh_triangles(
    mesh: &Mesh,
    solid: &TriangleSolid,
) -> Vec<MeshTriangleClassification> {
    mesh.triangles
        .iter()
        .enumerate()
        .map(|(triangle_index, indices)| {
            let triangle = Triangle {
                vertices: mesh.triangle_vertices(*indices),
            };
            MeshTriangleClassification {
                triangle_index,
                state: classify_triangle(&triangle, solid),
            }
        })
        .collect()
}

fn classify_triangle(triangle: &Triangle, solid: &TriangleSolid) -> ClippingState {
    if solid
        .intersecting_triangle_indices(triangle)
        .next()
        .is_some()
    {
        return ClippingState::Boundary;
    }

    let centroid = triangle_centroid(triangle);
    match solid.classify_point(centroid) {
        PointClassification::Inside | PointClassification::Boundary => ClippingState::Inside,
        PointClassification::Outside => ClippingState::Outside,
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PlaneKey {
    nx: i64,
    ny: i64,
    nz: i64,
    d: i64,
}

impl PlaneKey {
    fn from_triangle(triangle: &Triangle) -> Self {
        let normal = triangle_normal(triangle);
        let length = length_squared(normal).sqrt();
        if length <= EPSILON {
            return Self {
                nx: 0,
                ny: 0,
                nz: 0,
                d: 0,
            };
        }

        let mut nx = normal.x / length;
        let mut ny = normal.y / length;
        let mut nz = normal.z / length;
        let mut d = -dot(
            Vec3 {
                x: nx,
                y: ny,
                z: nz,
            },
            triangle.vertices[0],
        );

        if nx < -EPSILON
            || (nx.abs() <= EPSILON && ny < -EPSILON)
            || (nx.abs() <= EPSILON && ny.abs() <= EPSILON && nz < -EPSILON)
        {
            nx = -nx;
            ny = -ny;
            nz = -nz;
            d = -d;
        }

        Self {
            nx: quantize_plane_coordinate(nx),
            ny: quantize_plane_coordinate(ny),
            nz: quantize_plane_coordinate(nz),
            d: quantize_plane_coordinate(d),
        }
    }
}

fn is_proper_splitter(layer_triangle: &Triangle, solid_triangle: &Triangle) -> bool {
    triangle_straddles_plane(layer_triangle, solid_triangle)
        && triangle_straddles_plane(solid_triangle, layer_triangle)
}

fn triangle_straddles_plane(triangle: &Triangle, plane_triangle: &Triangle) -> bool {
    let normal = triangle_normal(plane_triangle);
    let normal_length = length_squared(normal).sqrt();
    if normal_length <= EPSILON {
        return false;
    }

    let tolerance = SPLIT_DISTANCE_EPSILON * normal_length;
    let origin = plane_triangle.vertices[0];
    let mut has_positive = false;
    let mut has_negative = false;

    for vertex in triangle.vertices {
        let distance = dot(sub(vertex, origin), normal);
        has_positive |= distance > tolerance;
        has_negative |= distance < -tolerance;
    }

    has_positive && has_negative
}

fn clip_triangle_to_solid(
    triangle: &Triangle,
    solid: &TriangleSolid,
    solid_triangle_indices: &[usize],
) -> Vec<[Vec3; 3]> {
    let mut fragments = vec![triangle.vertices.to_vec()];

    for solid_triangle_index in solid_triangle_indices {
        let splitter = &solid.triangles()[*solid_triangle_index];
        let mut split_fragments = Vec::new();

        for fragment in fragments {
            split_fragments.extend(split_polygon_by_triangle_plane(&fragment, splitter));
        }

        fragments = split_fragments;
        if fragments.is_empty() {
            break;
        }
    }

    fragments
        .into_iter()
        .flat_map(|fragment| triangulate_polygon(&fragment))
        .filter(|fragment| {
            matches!(
                solid.classify_point(triangle_vertices_centroid(fragment)),
                PointClassification::Inside | PointClassification::Boundary
            )
        })
        .collect()
}

fn split_polygon_by_triangle_plane(polygon: &[Vec3], splitter: &Triangle) -> Vec<Vec<Vec3>> {
    if polygon.len() < 3 {
        return Vec::new();
    }

    let normal = triangle_normal(splitter);
    if length_squared(normal) <= EPSILON {
        return vec![polygon.to_vec()];
    }

    let origin = splitter.vertices[0];
    let tolerance = SPLIT_DISTANCE_EPSILON * length_squared(normal).sqrt();
    let distances: Vec<f64> = polygon
        .iter()
        .map(|vertex| dot(sub(*vertex, origin), normal))
        .collect();
    let has_positive = distances.iter().any(|distance| *distance > tolerance);
    let has_negative = distances.iter().any(|distance| *distance < -tolerance);
    if !has_positive || !has_negative {
        return vec![polygon.to_vec()];
    }

    let positive = clip_polygon_to_plane(polygon, &distances, tolerance, true);
    let negative = clip_polygon_to_plane(polygon, &distances, tolerance, false);
    [positive, negative]
        .into_iter()
        .filter(|fragment| fragment.len() >= 3)
        .collect()
}

fn clip_polygon_to_plane(
    polygon: &[Vec3],
    distances: &[f64],
    tolerance: f64,
    keep_positive: bool,
) -> Vec<Vec3> {
    let mut clipped = Vec::new();

    for index in 0..polygon.len() {
        let next = (index + 1) % polygon.len();
        let current = polygon[index];
        let next_vertex = polygon[next];
        let current_distance = distances[index];
        let next_distance = distances[next];
        let current_inside = plane_side_inside(current_distance, tolerance, keep_positive);
        let next_inside = plane_side_inside(next_distance, tolerance, keep_positive);

        if current_inside {
            clipped.push(current);
        }

        if current_inside != next_inside {
            clipped.push(interpolate_plane_crossing(
                current,
                next_vertex,
                current_distance,
                next_distance,
            ));
        }
    }

    deduplicate_adjacent_vertices(clipped)
}

fn plane_side_inside(distance: f64, tolerance: f64, keep_positive: bool) -> bool {
    if keep_positive {
        distance >= -tolerance
    } else {
        distance <= tolerance
    }
}

fn interpolate_plane_crossing(a: Vec3, b: Vec3, distance_a: f64, distance_b: f64) -> Vec3 {
    let t = (distance_a / (distance_a - distance_b)).clamp(0.0, 1.0);
    Vec3 {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        z: a.z + (b.z - a.z) * t,
    }
}

fn deduplicate_adjacent_vertices(vertices: Vec<Vec3>) -> Vec<Vec3> {
    let mut deduplicated = Vec::new();
    for vertex in vertices {
        if deduplicated
            .last()
            .is_some_and(|previous| points_close(*previous, vertex))
        {
            continue;
        }
        deduplicated.push(vertex);
    }

    if deduplicated.len() > 1
        && points_close(
            deduplicated[0],
            *deduplicated.last().expect("checked length"),
        )
    {
        deduplicated.pop();
    }

    deduplicated
}

fn triangulate_polygon(polygon: &[Vec3]) -> Vec<[Vec3; 3]> {
    if polygon.len() < 3 {
        return Vec::new();
    }

    (1..polygon.len() - 1)
        .filter_map(|index| {
            let triangle = [polygon[0], polygon[index], polygon[index + 1]];
            (triangle_area_squared(&triangle) > EPSILON).then_some(triangle)
        })
        .collect()
}

#[derive(Default)]
struct VertexMap {
    indices: HashMap<VertexKey, usize>,
}

impl VertexMap {
    fn insert(&mut self, mesh: &mut Mesh, vertex: Vec3) -> usize {
        let key = VertexKey::from_vec3(vertex);
        if let Some(index) = self.indices.get(&key) {
            return *index;
        }

        let index = mesh.vertices.len();
        mesh.vertices.push(vertex);
        self.indices.insert(key, index);
        index
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct VertexKey {
    x: i64,
    y: i64,
    z: i64,
}

impl VertexKey {
    fn from_vec3(vertex: Vec3) -> Self {
        Self {
            x: quantize_coordinate(vertex.x),
            y: quantize_coordinate(vertex.y),
            z: quantize_coordinate(vertex.z),
        }
    }
}

fn quantize_coordinate(value: f64) -> i64 {
    (value * VERTEX_MERGE_SCALE).round() as i64
}

fn quantize_plane_coordinate(value: f64) -> i64 {
    (value * PLANE_MERGE_SCALE).round() as i64
}

fn push_triangle(mesh: &mut Mesh, vertices: &mut VertexMap, triangle: [Vec3; 3]) {
    let indices = [
        vertices.insert(mesh, triangle[0]),
        vertices.insert(mesh, triangle[1]),
        vertices.insert(mesh, triangle[2]),
    ];
    if indices[0] != indices[1] && indices[1] != indices[2] && indices[2] != indices[0] {
        mesh.triangles.push(indices);
    }
}

fn triangle_centroid(triangle: &Triangle) -> Vec3 {
    Vec3 {
        x: (triangle.vertices[0].x + triangle.vertices[1].x + triangle.vertices[2].x) / 3.0,
        y: (triangle.vertices[0].y + triangle.vertices[1].y + triangle.vertices[2].y) / 3.0,
        z: (triangle.vertices[0].z + triangle.vertices[1].z + triangle.vertices[2].z) / 3.0,
    }
}

fn triangle_vertices_centroid(triangle: &[Vec3; 3]) -> Vec3 {
    Vec3 {
        x: (triangle[0].x + triangle[1].x + triangle[2].x) / 3.0,
        y: (triangle[0].y + triangle[1].y + triangle[2].y) / 3.0,
        z: (triangle[0].z + triangle[1].z + triangle[2].z) / 3.0,
    }
}

pub(crate) fn triangle_aabb(triangle: &Triangle) -> Aabb {
    Aabb::from_points(&triangle.vertices)
}

fn triangle_normal(triangle: &Triangle) -> Vec3 {
    cross(
        sub(triangle.vertices[1], triangle.vertices[0]),
        sub(triangle.vertices[2], triangle.vertices[0]),
    )
}

fn triangle_area_squared(triangle: &[Vec3; 3]) -> f64 {
    length_squared(cross(
        sub(triangle[1], triangle[0]),
        sub(triangle[2], triangle[0]),
    ))
}

fn points_close(a: Vec3, b: Vec3) -> bool {
    length_squared(sub(a, b)) <= EPSILON * EPSILON
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.x - b.x,
        y: a.y - b.y,
        z: a.z - b.z,
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn length_squared(vector: Vec3) -> f64 {
    dot(vector, vector)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn classifies_mesh_triangles_against_solid() {
        let solid = TriangleSolid::new(vec![
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            },
            Triangle {
                vertices: [v(1.0, 0.0, 0.0), v(1.0, 1.0, 0.0), v(0.0, 1.0, 0.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 1.0), v(0.0, 1.0, 1.0), v(1.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(1.0, 0.0, 1.0), v(0.0, 1.0, 1.0), v(1.0, 1.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)],
            },
            Triangle {
                vertices: [v(1.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 1.0, 0.0), v(1.0, 1.0, 0.0), v(0.0, 1.0, 1.0)],
            },
            Triangle {
                vertices: [v(1.0, 1.0, 0.0), v(1.0, 1.0, 1.0), v(0.0, 1.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 1.0, 0.0), v(0.0, 1.0, 1.0), v(0.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(1.0, 0.0, 0.0), v(1.0, 0.0, 1.0), v(1.0, 1.0, 0.0)],
            },
            Triangle {
                vertices: [v(1.0, 1.0, 0.0), v(1.0, 0.0, 1.0), v(1.0, 1.0, 1.0)],
            },
        ]);

        let mesh = Mesh {
            vertices: vec![
                v(0.25, 0.25, 0.5),
                v(0.75, 0.25, 0.5),
                v(0.25, 0.75, 0.5),
                v(2.0, 2.0, 2.0),
                v(3.0, 2.0, 2.0),
                v(2.0, 3.0, 2.0),
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
        };

        let classifications = classify_mesh_triangles(&mesh, &solid);
        assert_eq!(classifications[0].state, ClippingState::Inside);
        assert_eq!(classifications[1].state, ClippingState::Outside);
    }

    #[test]
    fn clips_triangle_crossing_tetrahedron_boundary() {
        let solid = TriangleSolid::new(vec![
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(1.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(0.0, 1.0, 0.0)],
            },
        ]);
        let mesh = Mesh {
            vertices: vec![v(0.1, 0.1, 0.1), v(1.2, 0.1, 0.1), v(0.1, 1.2, 0.1)],
            triangles: vec![[0, 1, 2]],
        };

        let clipped = clip_mesh_to_solid(&mesh, &solid);

        assert!(!clipped.triangles.is_empty());
        for triangle in &clipped.triangles {
            let vertices = clipped.triangle_vertices(*triangle);
            let centroid = triangle_vertices_centroid(&vertices);
            assert_ne!(solid.classify_point(centroid), PointClassification::Outside);
        }
    }

    #[test]
    fn clipped_mesh_reuses_shared_vertices() {
        let solid = TriangleSolid::new(vec![
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 1.0), v(0.0, 2.0, 1.0), v(2.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 1.0), v(0.0, 2.0, 1.0), v(2.0, 2.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(2.0, 0.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(0.0, 0.0, 1.0), v(2.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 2.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 1.0)],
            },
            Triangle {
                vertices: [v(2.0, 2.0, 0.0), v(2.0, 2.0, 1.0), v(0.0, 2.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), v(0.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(0.0, 2.0, 0.0), v(0.0, 2.0, 1.0), v(0.0, 0.0, 1.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(2.0, 0.0, 1.0), v(2.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 2.0, 0.0), v(2.0, 0.0, 1.0), v(2.0, 2.0, 1.0)],
            },
        ]);
        let mesh = Mesh {
            vertices: vec![
                v(0.25, 0.25, 0.5),
                v(1.75, 0.25, 0.5),
                v(1.75, 1.75, 0.5),
                v(0.25, 1.75, 0.5),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };

        let clipped = clip_mesh_to_solid(&mesh, &solid);

        assert_eq!(clipped.triangles.len(), 2);
        assert_eq!(clipped.vertices.len(), 4);
    }

    #[test]
    fn keeps_inside_triangle_without_boundary_splitting() {
        let solid = TriangleSolid::new(vec![
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 2.0), v(0.0, 2.0, 2.0), v(2.0, 0.0, 2.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 2.0), v(0.0, 2.0, 2.0), v(2.0, 2.0, 2.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 0.0, 2.0), v(2.0, 0.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(0.0, 0.0, 2.0), v(2.0, 0.0, 2.0)],
            },
            Triangle {
                vertices: [v(0.0, 2.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 2.0)],
            },
            Triangle {
                vertices: [v(2.0, 2.0, 0.0), v(2.0, 2.0, 2.0), v(0.0, 2.0, 2.0)],
            },
            Triangle {
                vertices: [v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), v(0.0, 0.0, 2.0)],
            },
            Triangle {
                vertices: [v(0.0, 2.0, 0.0), v(0.0, 2.0, 2.0), v(0.0, 0.0, 2.0)],
            },
            Triangle {
                vertices: [v(2.0, 0.0, 0.0), v(2.0, 0.0, 2.0), v(2.0, 2.0, 0.0)],
            },
            Triangle {
                vertices: [v(2.0, 2.0, 0.0), v(2.0, 0.0, 2.0), v(2.0, 2.0, 2.0)],
            },
        ]);
        let mesh = Mesh {
            vertices: vec![v(0.5, 0.5, 1.0), v(1.5, 0.5, 1.0), v(0.5, 1.5, 1.0)],
            triangles: vec![[0, 1, 2]],
        };

        let clipped = clip_mesh_to_solid(&mesh, &solid);

        assert_eq!(clipped.triangles, vec![[0, 1, 2]]);
        assert_eq!(clipped.vertices.len(), 3);
    }
}
