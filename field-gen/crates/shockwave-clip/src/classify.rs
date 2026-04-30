use shockwave_core::geometry::{Triangle, Vec3};

use crate::aabb::Aabb;
use crate::intersect::{ray_intersects_triangle, triangles_intersect};
use crate::triangle_aabb;

const EPSILON: f64 = 1.0e-9;
const RAY_DIRECTION: Vec3 = Vec3 {
    x: 1.0,
    y: 0.3713906763541037,
    z: 0.11952286093343936,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointClassification {
    Inside,
    Outside,
    Boundary,
}

#[derive(Clone, Debug)]
pub struct TriangleSolid {
    triangles: Vec<Triangle>,
    bounds: Vec<Aabb>,
}

impl TriangleSolid {
    pub fn new(triangles: Vec<Triangle>) -> Self {
        let bounds = triangles.iter().map(triangle_aabb).collect();
        Self { triangles, bounds }
    }

    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    pub fn classify_point(&self, point: Vec3) -> PointClassification {
        let mut hits = Vec::new();
        for triangle in &self.triangles {
            let Some(distance) = ray_intersects_triangle(point, RAY_DIRECTION, triangle) else {
                continue;
            };
            if distance.abs() <= EPSILON {
                return PointClassification::Boundary;
            }
            if distance > EPSILON && !hits_with_same_distance(&hits, distance) {
                hits.push(distance);
            }
        }

        if hits.len() % 2 == 1 {
            PointClassification::Inside
        } else {
            PointClassification::Outside
        }
    }

    pub fn intersecting_triangle_indices<'a>(
        &'a self,
        triangle: &'a Triangle,
    ) -> impl Iterator<Item = usize> + 'a {
        let bounds = triangle_aabb(triangle);
        self.triangles
            .iter()
            .enumerate()
            .filter(move |(index, candidate)| {
                self.bounds[*index].intersects(bounds) && triangles_intersect(triangle, candidate)
            })
            .map(|(index, _)| index)
    }
}

fn hits_with_same_distance(hits: &[f64], distance: f64) -> bool {
    hits.iter()
        .any(|hit| (*hit - distance).abs() <= EPSILON * 64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn classifies_points_in_tetrahedron() {
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

        assert_eq!(
            solid.classify_point(v(0.1, 0.1, 0.1)),
            PointClassification::Inside
        );
        assert_eq!(
            solid.classify_point(v(2.0, 2.0, 2.0)),
            PointClassification::Outside
        );
    }
}
