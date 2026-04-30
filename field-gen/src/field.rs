use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::geometry::Vec3;
use crate::grid::Grid;

#[derive(Clone, Debug)]
pub struct Field {
    pub distances: Vec<f64>,
    pub max_distance: f64,
}

pub trait PropagationMethod {
    fn seeds(&self, occupancy: &[u8], grid: Grid) -> Vec<usize>;
    fn neighbors(&self, index: usize, grid: Grid) -> Vec<(usize, f64)>;
}

#[derive(Clone, Copy, Debug)]
pub struct AnisotropicEuclideanPropagation {
    rate: Vec3,
}

impl AnisotropicEuclideanPropagation {
    pub fn new(rate: Vec3) -> Self {
        Self { rate }
    }
}

impl PropagationMethod for AnisotropicEuclideanPropagation {
    fn seeds(&self, occupancy: &[u8], grid: Grid) -> Vec<usize> {
        for z in 0..grid.dims[2] {
            let mut seeds = Vec::new();

            for y in 0..grid.dims[1] {
                for x in 0..grid.dims[0] {
                    let index = grid.index(x, y, z);
                    if occupancy[index] != 0 {
                        seeds.push(index);
                    }
                }
            }

            if !seeds.is_empty() {
                return seeds;
            }
        }

        Vec::new()
    }

    fn neighbors(&self, index: usize, grid: Grid) -> Vec<(usize, f64)> {
        let slice_len = grid.slice_len();
        let z = index / slice_len;
        let remainder = index % slice_len;
        let y = remainder / grid.dims[0];
        let x = remainder % grid.dims[0];
        let mut neighbors = Vec::with_capacity(26);

        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }

                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    let nz = z as isize + dz;
                    if nx < 0
                        || ny < 0
                        || nz < 0
                        || nx >= grid.dims[0] as isize
                        || ny >= grid.dims[1] as isize
                        || nz >= grid.dims[2] as isize
                    {
                        continue;
                    }

                    let cost = movement_cost(dx, dy, dz, grid, self.rate);
                    neighbors.push((grid.index(nx as usize, ny as usize, nz as usize), cost));
                }
            }
        }

        neighbors
    }
}

pub fn propagate_field(
    occupancy: &[u8],
    grid: Grid,
    method: &impl PropagationMethod,
) -> Result<Field, String> {
    if occupancy.len() != grid.voxel_count() {
        return Err("occupancy length does not match grid dimensions".to_string());
    }

    let mut distances = vec![f64::INFINITY; occupancy.len()];
    let mut queue = BinaryHeap::new();

    for seed in method.seeds(occupancy, grid) {
        distances[seed] = 0.0;
        queue.push(QueueEntry {
            index: seed,
            distance: 0.0,
        });
    }

    while let Some(entry) = queue.pop() {
        if entry.distance > distances[entry.index] {
            continue;
        }

        for (neighbor, cost) in method.neighbors(entry.index, grid) {
            if occupancy[neighbor] == 0 {
                continue;
            }

            let next_distance = entry.distance + cost;
            if next_distance < distances[neighbor] {
                distances[neighbor] = next_distance;
                queue.push(QueueEntry {
                    index: neighbor,
                    distance: next_distance,
                });
            }
        }
    }

    let max_distance = distances
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0, f64::max);

    Ok(Field {
        distances,
        max_distance,
    })
}

pub fn expand_field(field: &mut Field, grid: Grid, layers: usize, method: &impl PropagationMethod) {
    for _ in 0..layers {
        let previous_distances = field.distances.clone();
        let mut next_distances = previous_distances.clone();

        for (index, distance) in previous_distances.iter().copied().enumerate() {
            if !distance.is_finite() {
                continue;
            }

            for (neighbor, cost) in method.neighbors(index, grid) {
                let next_distance = distance + cost;
                if next_distance < next_distances[neighbor] {
                    next_distances[neighbor] = next_distance;
                }
            }
        }

        field.distances = next_distances;
    }

    field.max_distance = field
        .distances
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0, f64::max);
}

fn movement_cost(dx: isize, dy: isize, dz: isize, grid: Grid, rate: Vec3) -> f64 {
    let x = dx as f64 * grid.voxel_size.x / rate.x;
    let y = dy as f64 * grid.voxel_size.y / rate.y;
    let z = dz as f64 * grid.voxel_size.z / rate.z;
    (x * x + y * y + z * z).sqrt()
}

#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    index: usize,
    distance: f64,
}

impl Eq for QueueEntry {}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.distance == other.distance
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(dims: [usize; 3]) -> Grid {
        Grid {
            origin: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            dims,
            voxel_size: Vec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            actual_size: Vec3 {
                x: dims[0] as f64,
                y: dims[1] as f64,
                z: dims[2] as f64,
            },
        }
    }

    #[test]
    fn seeds_all_occupied_voxels_in_lowest_occupied_slice() {
        let grid = grid([2, 2, 3]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 1, 1)] = 255;
        occupancy[grid.index(1, 1, 1)] = 255;
        occupancy[grid.index(0, 0, 2)] = 255;
        let propagation = AnisotropicEuclideanPropagation::new(Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        });

        let seeds = propagation.seeds(&occupancy, grid);

        assert_eq!(seeds, vec![grid.index(0, 1, 1), grid.index(1, 1, 1)]);
    }

    #[test]
    fn propagates_only_through_occupied_voxels() {
        let grid = grid([3, 1, 2]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(2, 0, 1)] = 255;
        let propagation = AnisotropicEuclideanPropagation::new(Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        });

        let field = propagate_field(&occupancy, grid, &propagation).unwrap();

        assert_eq!(field.distances[0], 0.0);
        assert!(field.distances[1].is_infinite());
        assert!(field.distances[grid.index(2, 0, 1)].is_infinite());
    }

    #[test]
    fn higher_axis_rate_reduces_axis_cost() {
        let grid = grid([2, 2, 2]);
        let rate = Vec3 {
            x: 10.0,
            y: 5.0,
            z: 1.0,
        };

        assert!((movement_cost(1, 0, 0, grid, rate) - 0.1).abs() < 1.0e-9);
        assert!((movement_cost(0, 1, 0, grid, rate) - 0.2).abs() < 1.0e-9);
        assert!((movement_cost(1, 1, 0, grid, rate) - (0.05_f64).sqrt()).abs() < 1.0e-9);
    }

    #[test]
    fn expands_field_by_requested_layers_without_changing_occupancy_requirement() {
        let grid = grid([5, 1, 1]);
        let mut field = Field {
            distances: vec![f64::INFINITY; grid.voxel_count()],
            max_distance: 0.0,
        };
        field.distances[grid.index(2, 0, 0)] = 0.0;
        let propagation = AnisotropicEuclideanPropagation::new(Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        });

        expand_field(&mut field, grid, 2, &propagation);

        assert_eq!(field.distances[grid.index(0, 0, 0)], 2.0);
        assert_eq!(field.distances[grid.index(1, 0, 0)], 1.0);
        assert_eq!(field.distances[grid.index(2, 0, 0)], 0.0);
        assert_eq!(field.distances[grid.index(3, 0, 0)], 1.0);
        assert_eq!(field.distances[grid.index(4, 0, 0)], 2.0);
        assert_eq!(field.max_distance, 2.0);
    }
}
