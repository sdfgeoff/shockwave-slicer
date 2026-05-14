use shockwave_math::geometry::{Triangle, Vec3};

use crate::aabb::Aabb;
use crate::intersect::{ray_intersects_triangle, triangles_intersect};
use crate::triangle_aabb;

const EPSILON: f64 = 1.0e-9;
const LEAF_TRIANGLE_COUNT: usize = 8;
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
    nodes: Vec<AabbNode>,
    root: Option<usize>,
}

#[derive(Clone, Debug)]
struct AabbNode {
    bounds: Aabb,
    children: Option<[usize; 2]>,
    triangle_indices: Vec<usize>,
}

impl TriangleSolid {
    pub fn new(triangles: Vec<Triangle>) -> Self {
        let bounds: Vec<Aabb> = triangles.iter().map(triangle_aabb).collect();
        let mut nodes = Vec::new();
        let root = if triangles.is_empty() {
            None
        } else {
            let mut indices = (0..triangles.len()).collect::<Vec<_>>();
            Some(build_aabb_tree(&bounds, &mut indices, &mut nodes))
        };
        Self {
            triangles,
            nodes,
            root,
        }
    }

    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    pub fn classify_point(&self, point: Vec3) -> PointClassification {
        let mut hits = Vec::new();
        let mut candidates = Vec::new();
        self.collect_ray_candidates(point, RAY_DIRECTION, &mut candidates);

        for index in candidates {
            let triangle = &self.triangles[index];
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

    pub fn intersecting_triangle_indices(
        &self,
        triangle: &Triangle,
    ) -> impl Iterator<Item = usize> {
        let bounds = triangle_aabb(triangle);
        let mut candidates = Vec::new();
        self.collect_bounds_candidates(bounds, &mut candidates);
        candidates
            .into_iter()
            .filter(|index| triangles_intersect(triangle, &self.triangles[*index]))
    }

    fn collect_bounds_candidates(&self, bounds: Aabb, candidates: &mut Vec<usize>) {
        let Some(root) = self.root else {
            return;
        };
        let mut stack = vec![root];
        while let Some(node_index) = stack.pop() {
            let node = &self.nodes[node_index];
            if !node.bounds.intersects(bounds) {
                continue;
            }
            if let Some(children) = node.children {
                stack.extend(children);
            } else {
                candidates.extend_from_slice(&node.triangle_indices);
            }
        }
    }

    fn collect_ray_candidates(&self, origin: Vec3, direction: Vec3, candidates: &mut Vec<usize>) {
        let Some(root) = self.root else {
            return;
        };
        let mut stack = vec![root];
        while let Some(node_index) = stack.pop() {
            let node = &self.nodes[node_index];
            if !node.bounds.intersects_ray(origin, direction) {
                continue;
            }
            if let Some(children) = node.children {
                stack.extend(children);
            } else {
                candidates.extend_from_slice(&node.triangle_indices);
            }
        }
    }
}

fn build_aabb_tree(bounds: &[Aabb], indices: &mut [usize], nodes: &mut Vec<AabbNode>) -> usize {
    let node_bounds = bounds_for_indices(bounds, indices);
    let node_index = nodes.len();
    nodes.push(AabbNode {
        bounds: node_bounds,
        children: None,
        triangle_indices: Vec::new(),
    });

    if indices.len() <= LEAF_TRIANGLE_COUNT {
        nodes[node_index].triangle_indices = indices.to_vec();
        return node_index;
    }

    let axis = node_bounds.longest_axis();
    indices.sort_by(|a, b| {
        bounds[*a]
            .centroid_axis(axis)
            .total_cmp(&bounds[*b].centroid_axis(axis))
    });
    let mid = indices.len() / 2;
    let (left_indices, right_indices) = indices.split_at_mut(mid);
    let left = build_aabb_tree(bounds, left_indices, nodes);
    let right = build_aabb_tree(bounds, right_indices, nodes);
    nodes[node_index].children = Some([left, right]);
    node_index
}

fn bounds_for_indices(bounds: &[Aabb], indices: &[usize]) -> Aabb {
    let mut result = bounds[indices[0]];
    for index in &indices[1..] {
        result = result.union(bounds[*index]);
    }
    result
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
