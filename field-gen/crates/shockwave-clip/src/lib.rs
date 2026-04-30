//! Triangle-mesh clipping foundations.
//!
//! This crate owns solid classification and triangle-intersection primitives.
//! Actual triangle splitting is intentionally kept separate so callers do not
//! mistake "classified" triangles for a fully clipped layer mesh.

mod aabb;
mod classify;
mod intersect;

use shockwave_core::geometry::{Triangle, Vec3};
use shockwave_mesh::Mesh;

use crate::aabb::Aabb;

pub use classify::{PointClassification, TriangleSolid};
pub use intersect::{TriangleIntersection, triangles_intersect};

const EPSILON: f64 = 1.0e-9;

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

    for indices in &mesh.triangles {
        let triangle = Triangle {
            vertices: mesh.triangle_vertices(*indices),
        };
        for fragment in clip_triangle_to_solid(&triangle, solid) {
            push_triangle(&mut clipped, fragment);
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

fn clip_triangle_to_solid(triangle: &Triangle, solid: &TriangleSolid) -> Vec<[Vec3; 3]> {
    let mut fragments = vec![triangle.vertices.to_vec()];

    for solid_triangle_index in solid.intersecting_triangle_indices(triangle) {
        let splitter = &solid.triangles()[solid_triangle_index];
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
    let distances: Vec<f64> = polygon
        .iter()
        .map(|vertex| dot(sub(*vertex, origin), normal))
        .collect();
    let has_positive = distances.iter().any(|distance| *distance > EPSILON);
    let has_negative = distances.iter().any(|distance| *distance < -EPSILON);
    if !has_positive || !has_negative {
        return vec![polygon.to_vec()];
    }

    let positive = clip_polygon_to_plane(polygon, &distances, true);
    let negative = clip_polygon_to_plane(polygon, &distances, false);
    [positive, negative]
        .into_iter()
        .filter(|fragment| fragment.len() >= 3)
        .collect()
}

fn clip_polygon_to_plane(polygon: &[Vec3], distances: &[f64], keep_positive: bool) -> Vec<Vec3> {
    let mut clipped = Vec::new();

    for index in 0..polygon.len() {
        let next = (index + 1) % polygon.len();
        let current = polygon[index];
        let next_vertex = polygon[next];
        let current_distance = distances[index];
        let next_distance = distances[next];
        let current_inside = plane_side_inside(current_distance, keep_positive);
        let next_inside = plane_side_inside(next_distance, keep_positive);

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

fn plane_side_inside(distance: f64, keep_positive: bool) -> bool {
    if keep_positive {
        distance >= -EPSILON
    } else {
        distance <= EPSILON
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

fn push_triangle(mesh: &mut Mesh, triangle: [Vec3; 3]) {
    let base = mesh.vertices.len();
    mesh.vertices.extend_from_slice(&triangle);
    mesh.triangles.push([base, base + 1, base + 2]);
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
}
