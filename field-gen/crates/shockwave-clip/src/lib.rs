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

fn triangle_centroid(triangle: &Triangle) -> Vec3 {
    Vec3 {
        x: (triangle.vertices[0].x + triangle.vertices[1].x + triangle.vertices[2].x) / 3.0,
        y: (triangle.vertices[0].y + triangle.vertices[1].y + triangle.vertices[2].y) / 3.0,
        z: (triangle.vertices[0].z + triangle.vertices[1].z + triangle.vertices[2].z) / 3.0,
    }
}

pub(crate) fn triangle_aabb(triangle: &Triangle) -> Aabb {
    Aabb::from_points(&triangle.vertices)
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
}
