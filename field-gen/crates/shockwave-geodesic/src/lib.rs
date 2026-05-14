use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use shockwave_mesh::Mesh;

#[derive(Clone, Debug)]
pub struct GeodesicField {
    pub distances: Vec<f64>,
}

impl GeodesicField {
    pub fn distance(&self, vertex: usize) -> Option<f64> {
        self.distances
            .get(vertex)
            .copied()
            .filter(|distance| distance.is_finite())
    }
}

#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    vertex: usize,
    distance: f64,
}

impl Eq for QueueEntry {}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.distance == other.distance
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn distance_from_vertices(mesh: &Mesh, seeds: &[usize]) -> Result<GeodesicField, String> {
    if seeds.iter().any(|seed| *seed >= mesh.vertices.len()) {
        return Err("geodesic seed vertex is out of bounds".to_string());
    }

    let adjacency = vertex_adjacency(mesh)?;
    let mut distances = vec![f64::INFINITY; mesh.vertices.len()];
    let mut queue = BinaryHeap::new();

    for seed in seeds {
        distances[*seed] = 0.0;
        queue.push(QueueEntry {
            vertex: *seed,
            distance: 0.0,
        });
    }

    while let Some(entry) = queue.pop() {
        if entry.distance > distances[entry.vertex] {
            continue;
        }

        for neighbor in &adjacency[entry.vertex] {
            let next_distance = entry.distance + neighbor.length;
            if next_distance < distances[neighbor.vertex] {
                distances[neighbor.vertex] = next_distance;
                queue.push(QueueEntry {
                    vertex: neighbor.vertex,
                    distance: next_distance,
                });
            }
        }
    }

    Ok(GeodesicField { distances })
}

pub fn distance_from_boundary(mesh: &Mesh) -> Result<GeodesicField, String> {
    let seeds = boundary_vertices(mesh)?;
    distance_from_vertices(mesh, &seeds)
}

pub fn boundary_vertices(mesh: &Mesh) -> Result<Vec<usize>, String> {
    let mut vertices = Vec::new();
    let mut is_boundary = vec![false; mesh.vertices.len()];
    for edge in boundary_edges(mesh)? {
        is_boundary[edge[0]] = true;
        is_boundary[edge[1]] = true;
    }

    for (index, boundary) in is_boundary.into_iter().enumerate() {
        if boundary {
            vertices.push(index);
        }
    }

    Ok(vertices)
}

pub fn boundary_edges(mesh: &Mesh) -> Result<Vec<[usize; 2]>, String> {
    let mut edge_counts: HashMap<[usize; 2], usize> = HashMap::new();
    for triangle in &mesh.triangles {
        for vertex in triangle {
            if *vertex >= mesh.vertices.len() {
                return Err("mesh triangle references an out-of-bounds vertex".to_string());
            }
        }

        count_edge(&mut edge_counts, triangle[0], triangle[1]);
        count_edge(&mut edge_counts, triangle[1], triangle[2]);
        count_edge(&mut edge_counts, triangle[2], triangle[0]);
    }

    let mut edges: Vec<_> = edge_counts
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect();
    edges.sort_unstable();
    Ok(edges)
}

#[derive(Clone, Copy, Debug)]
struct Neighbor {
    vertex: usize,
    length: f64,
}

fn vertex_adjacency(mesh: &Mesh) -> Result<Vec<Vec<Neighbor>>, String> {
    let mut adjacency = vec![Vec::new(); mesh.vertices.len()];
    for triangle in &mesh.triangles {
        for vertex in triangle {
            if *vertex >= mesh.vertices.len() {
                return Err("mesh triangle references an out-of-bounds vertex".to_string());
            }
        }

        add_edge(mesh, &mut adjacency, triangle[0], triangle[1]);
        add_edge(mesh, &mut adjacency, triangle[1], triangle[2]);
        add_edge(mesh, &mut adjacency, triangle[2], triangle[0]);
    }

    Ok(adjacency)
}

fn count_edge(edge_counts: &mut HashMap<[usize; 2], usize>, a: usize, b: usize) {
    let edge = ordered_edge(a, b);
    *edge_counts.entry(edge).or_default() += 1;
}

fn ordered_edge(a: usize, b: usize) -> [usize; 2] {
    if a < b { [a, b] } else { [b, a] }
}

fn add_edge(mesh: &Mesh, adjacency: &mut [Vec<Neighbor>], a: usize, b: usize) {
    let length = edge_length(mesh, a, b);
    adjacency[a].push(Neighbor { vertex: b, length });
    adjacency[b].push(Neighbor { vertex: a, length });
}

fn edge_length(mesh: &Mesh, a: usize, b: usize) -> f64 {
    let a = mesh.vertices[a];
    let b = mesh.vertices[b];
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dz = b.z - a.z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use shockwave_math::geometry::Vec3;
    use shockwave_mesh::Mesh;

    use super::*;

    fn vertex(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn propagates_distance_across_mesh_edges() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(1.0, 0.0, 0.0),
                vertex(2.0, 0.0, 0.0),
            ],
            triangles: vec![[0, 1, 2]],
        };

        let field = distance_from_vertices(&mesh, &[0]).unwrap();

        assert_eq!(field.distance(0), Some(0.0));
        assert_eq!(field.distance(1), Some(1.0));
        assert_eq!(field.distance(2), Some(2.0));
    }

    #[test]
    fn finds_boundary_edges_on_square_mesh() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(1.0, 0.0, 0.0),
                vertex(1.0, 1.0, 0.0),
                vertex(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };

        let edges = boundary_edges(&mesh).unwrap();

        assert_eq!(edges, vec![[0, 1], [0, 3], [1, 2], [2, 3]]);
    }

    #[test]
    fn propagates_distance_from_boundary() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(2.0, 0.0, 0.0),
                vertex(2.0, 2.0, 0.0),
                vertex(0.0, 2.0, 0.0),
                vertex(1.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        };

        let field = distance_from_boundary(&mesh).unwrap();

        assert_eq!(field.distance(0), Some(0.0));
        assert_eq!(field.distance(4), Some(2.0_f64.sqrt()));
    }
}
