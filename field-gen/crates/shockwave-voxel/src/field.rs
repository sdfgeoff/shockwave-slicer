use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

use shockwave_core::geometry::Vec3;
use shockwave_core::grid::Grid;

#[derive(Clone, Debug)]
pub struct Field {
    pub distances: Vec<f64>,
    pub max_distance: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PropagationConstraints {
    pub max_unreached_below_mm: Option<f64>,
    pub unreached_cone_angle_degrees: Option<f64>,
    pub unreached_cone_max_height_mm: Option<f64>,
}

pub trait PropagationProgress {
    fn update(&mut self, reached: usize, total: usize);
    fn finish(&mut self, reached: usize, total: usize);
}

pub trait PropagationMethod {
    fn seeds(&self, occupancy: &[u8], grid: Grid) -> Vec<usize>;

    fn for_each_neighbor(&self, index: usize, grid: Grid, visit: &mut impl FnMut(usize, f64));

    fn for_each_traversable_neighbor(
        &self,
        occupancy: &[u8],
        index: usize,
        grid: Grid,
        visit: &mut impl FnMut(usize, f64),
    ) {
        self.for_each_neighbor(index, grid, &mut |neighbor, cost| {
            if occupancy[neighbor] != 0 {
                visit(neighbor, cost);
            }
        });
    }
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

    fn for_each_neighbor(&self, index: usize, grid: Grid, visit: &mut impl FnMut(usize, f64)) {
        let slice_len = grid.slice_len();
        let z = index / slice_len;
        let remainder = index % slice_len;
        let y = remainder / grid.dims[0];
        let x = remainder % grid.dims[0];

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
                    visit(grid.index(nx as usize, ny as usize, nz as usize), cost);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelPathCheck {
    EndpointOccupied,
    SweptOccupied,
}

#[derive(Clone, Debug)]
pub struct KernelMove {
    pub offset: [isize; 3],
    pub cost: f64,
}

#[derive(Clone, Debug)]
pub struct ExplicitKernelPropagation {
    moves: Vec<PreparedKernelMove>,
    path_check: KernelPathCheck,
}

#[derive(Clone, Debug)]
struct PreparedKernelMove {
    offset: [isize; 3],
    cost: f64,
    swept_offsets: Vec<[isize; 3]>,
}

impl ExplicitKernelPropagation {
    pub fn new(moves: Vec<KernelMove>, path_check: KernelPathCheck) -> Result<Self, String> {
        if moves.is_empty() {
            return Err("kernel must contain at least one move".to_string());
        }

        for kernel_move in &moves {
            if kernel_move.offset == [0, 0, 0] {
                return Err("kernel move offset must not be [0, 0, 0]".to_string());
            }
            if kernel_move.cost <= 0.0 || !kernel_move.cost.is_finite() {
                return Err("kernel move cost must be finite and greater than zero".to_string());
            }
        }

        Ok(Self {
            moves: moves
                .into_iter()
                .map(|kernel_move| PreparedKernelMove {
                    swept_offsets: swept_offsets(kernel_move.offset),
                    offset: kernel_move.offset,
                    cost: kernel_move.cost,
                })
                .collect(),
            path_check,
        })
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }
}

impl PropagationMethod for ExplicitKernelPropagation {
    fn seeds(&self, occupancy: &[u8], grid: Grid) -> Vec<usize> {
        AnisotropicEuclideanPropagation::new(Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        })
        .seeds(occupancy, grid)
    }

    fn for_each_neighbor(&self, index: usize, grid: Grid, visit: &mut impl FnMut(usize, f64)) {
        let [x, y, z] = grid_coords(index, grid);

        for kernel_move in &self.moves {
            let nx = x as isize + kernel_move.offset[0];
            let ny = y as isize + kernel_move.offset[1];
            let nz = z as isize + kernel_move.offset[2];
            if nx < 0
                || ny < 0
                || nz < 0
                || nx >= grid.dims[0] as isize
                || ny >= grid.dims[1] as isize
                || nz >= grid.dims[2] as isize
            {
                continue;
            }

            visit(
                grid.index(nx as usize, ny as usize, nz as usize),
                kernel_move.cost,
            );
        }
    }

    fn for_each_traversable_neighbor(
        &self,
        occupancy: &[u8],
        index: usize,
        grid: Grid,
        visit: &mut impl FnMut(usize, f64),
    ) {
        let [x, y, z] = grid_coords(index, grid);

        for kernel_move in &self.moves {
            let nx = x as isize + kernel_move.offset[0];
            let ny = y as isize + kernel_move.offset[1];
            let nz = z as isize + kernel_move.offset[2];
            if nx < 0
                || ny < 0
                || nz < 0
                || nx >= grid.dims[0] as isize
                || ny >= grid.dims[1] as isize
                || nz >= grid.dims[2] as isize
            {
                continue;
            }

            let neighbor = grid.index(nx as usize, ny as usize, nz as usize);
            if occupancy[neighbor] == 0 {
                continue;
            }

            if self.path_check == KernelPathCheck::SweptOccupied
                && !swept_offsets_are_occupied(
                    occupancy,
                    [x, y, z],
                    &kernel_move.swept_offsets,
                    grid,
                )
            {
                continue;
            }

            visit(neighbor, kernel_move.cost);
        }
    }
}

pub fn propagate_field(
    occupancy: &[u8],
    grid: Grid,
    method: &impl PropagationMethod,
) -> Result<Field, String> {
    propagate_field_with_constraints(occupancy, grid, method, PropagationConstraints::default())
}

pub fn propagate_field_with_constraints(
    occupancy: &[u8],
    grid: Grid,
    method: &impl PropagationMethod,
    constraints: PropagationConstraints,
) -> Result<Field, String> {
    propagate_field_with_progress(occupancy, grid, method, constraints, &mut NoProgress)
}

pub fn propagate_field_with_progress(
    occupancy: &[u8],
    grid: Grid,
    method: &impl PropagationMethod,
    constraints: PropagationConstraints,
    progress: &mut impl PropagationProgress,
) -> Result<Field, String> {
    if occupancy.len() != grid.voxel_count() {
        return Err("occupancy length does not match grid dimensions".to_string());
    }

    let total_occupied = occupancy.iter().filter(|value| **value != 0).count();
    let mut distances = vec![f64::INFINITY; occupancy.len()];
    let mut reached = vec![false; occupancy.len()];
    let components = occupied_components(occupancy, grid);
    let mut queue = BinaryHeap::new();
    let mut deferred = Vec::new();
    let mut current_distance = 0.0;
    let mut reached_count = 0usize;
    progress.update(reached_count, total_occupied);

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
        if reached[entry.index] {
            continue;
        }
        if !constraints_allow_candidate(occupancy, grid, entry.index, &reached, constraints) {
            deferred.push(entry);
            continue;
        }

        let entry_distance = entry.distance.max(current_distance);
        distances[entry.index] = entry_distance;
        current_distance = entry_distance;
        reached[entry.index] = true;
        reached_count += 1;
        progress.update(reached_count, total_occupied);
        queue.extend(deferred.drain(..));

        method.for_each_traversable_neighbor(
            occupancy,
            entry.index,
            grid,
            &mut |neighbor, cost| {
                if components[entry.index] != components[neighbor] {
                    return;
                }

                let next_distance = entry_distance + cost;
                if next_distance < distances[neighbor] {
                    distances[neighbor] = next_distance;
                    queue.push(QueueEntry {
                        index: neighbor,
                        distance: next_distance,
                    });
                }
            },
        );
    }

    let max_distance = distances
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0, f64::max);
    progress.finish(reached_count, total_occupied);

    Ok(Field {
        distances,
        max_distance,
    })
}

pub struct StderrProgress {
    label: &'static str,
    next_percent: usize,
    last_print: Instant,
}

impl StderrProgress {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            next_percent: 0,
            last_print: Instant::now() - Duration::from_secs(1),
        }
    }
}

impl PropagationProgress for StderrProgress {
    fn update(&mut self, reached: usize, total: usize) {
        if total == 0 {
            if reached == 0 && self.next_percent == 0 {
                eprintln!("{}: 100.0% (0 / 0)", self.label);
                self.next_percent = 101;
            }
            return;
        }

        let percent = reached * 100 / total;
        let now = Instant::now();
        if percent >= self.next_percent
            || reached == total
            || now.duration_since(self.last_print) >= Duration::from_secs(2)
        {
            eprintln!(
                "{}: {:>5.1}% ({} / {})",
                self.label,
                reached as f64 * 100.0 / total as f64,
                reached,
                total
            );
            self.next_percent = percent.saturating_add(5);
            self.last_print = now;
        }
    }

    fn finish(&mut self, reached: usize, total: usize) {
        if total == 0 {
            eprintln!("{}: complete (0 / 0)", self.label);
            return;
        }

        eprintln!(
            "{}: complete {:>5.1}% ({} / {})",
            self.label,
            reached as f64 * 100.0 / total as f64,
            reached,
            total
        );
    }
}

struct NoProgress;

impl PropagationProgress for NoProgress {
    fn update(&mut self, _reached: usize, _total: usize) {}
    fn finish(&mut self, _reached: usize, _total: usize) {}
}

pub fn expand_field(field: &mut Field, grid: Grid, layers: usize, method: &impl PropagationMethod) {
    for _ in 0..layers {
        let previous_distances = field.distances.clone();
        let mut next_distances = previous_distances.clone();

        for (index, distance) in previous_distances.iter().copied().enumerate() {
            if !distance.is_finite() {
                continue;
            }

            method.for_each_neighbor(index, grid, &mut |neighbor, cost| {
                let next_distance = distance + cost;
                if next_distance < next_distances[neighbor] {
                    next_distances[neighbor] = next_distance;
                }
            });
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

fn grid_coords(index: usize, grid: Grid) -> [usize; 3] {
    let slice_len = grid.slice_len();
    let z = index / slice_len;
    let remainder = index % slice_len;
    let y = remainder / grid.dims[0];
    let x = remainder % grid.dims[0];
    [x, y, z]
}

fn swept_offsets(offset: [isize; 3]) -> Vec<[isize; 3]> {
    let [dx, dy, dz] = offset;
    let steps = dx.abs().max(dy.abs()).max(dz.abs()) as usize;
    let mut offsets = Vec::with_capacity(steps.saturating_sub(1));

    for step in 1..steps {
        let t = step as f64 / steps as f64;
        offsets.push([
            (dx as f64 * t).round() as isize,
            (dy as f64 * t).round() as isize,
            (dz as f64 * t).round() as isize,
        ]);
    }

    offsets
}

fn swept_offsets_are_occupied(
    occupancy: &[u8],
    from: [usize; 3],
    offsets: &[[isize; 3]],
    grid: Grid,
) -> bool {
    for offset in offsets {
        let x = (from[0] as isize + offset[0]) as usize;
        let y = (from[1] as isize + offset[1]) as usize;
        let z = (from[2] as isize + offset[2]) as usize;
        if occupancy[grid.index(x, y, z)] == 0 {
            return false;
        }
    }

    true
}

fn constraints_allow_candidate(
    occupancy: &[u8],
    grid: Grid,
    candidate: usize,
    reached: &[bool],
    constraints: PropagationConstraints,
) -> bool {
    if constraints.max_unreached_below_mm.is_none()
        && constraints.unreached_cone_angle_degrees.is_none()
    {
        return true;
    }

    let [candidate_x, candidate_y, candidate_z] = grid_coords(candidate, grid);
    let candidate_z_mm = candidate_z as f64 * grid.voxel_size.z;
    let cone_tan = constraints
        .unreached_cone_angle_degrees
        .map(|angle| angle.to_radians().tan());

    for (index, occupied) in occupancy.iter().copied().enumerate() {
        if occupied == 0 || reached[index] || index == candidate {
            continue;
        }

        let [x, y, z] = grid_coords(index, grid);
        let dz_mm = candidate_z_mm - z as f64 * grid.voxel_size.z;
        if dz_mm <= 0.0 {
            continue;
        }

        if let Some(max_unreached_below_mm) = constraints.max_unreached_below_mm
            && dz_mm > max_unreached_below_mm
        {
            return false;
        }

        if let Some(cone_tan) = cone_tan {
            if let Some(max_cone_height_mm) = constraints.unreached_cone_max_height_mm
                && dz_mm > max_cone_height_mm
            {
                continue;
            }

            let dx_mm = (candidate_x as isize - x as isize) as f64 * grid.voxel_size.x;
            let dy_mm = (candidate_y as isize - y as isize) as f64 * grid.voxel_size.y;
            let radial_mm = dx_mm.hypot(dy_mm);
            if radial_mm <= dz_mm * cone_tan {
                return false;
            }
        }
    }

    true
}

fn occupied_components(occupancy: &[u8], grid: Grid) -> Vec<usize> {
    let mut components = vec![usize::MAX; occupancy.len()];
    let mut next_component = 0usize;
    let mut stack = Vec::new();

    for index in 0..occupancy.len() {
        if occupancy[index] == 0 || components[index] != usize::MAX {
            continue;
        }

        components[index] = next_component;
        stack.push(index);

        while let Some(current) = stack.pop() {
            for_face_neighbor(current, grid, &mut |neighbor| {
                if occupancy[neighbor] != 0 && components[neighbor] == usize::MAX {
                    components[neighbor] = next_component;
                    stack.push(neighbor);
                }
            });
        }

        next_component += 1;
    }

    components
}

fn for_face_neighbor(index: usize, grid: Grid, visit: &mut impl FnMut(usize)) {
    let [x, y, z] = grid_coords(index, grid);
    let offsets = [
        [-1, 0, 0],
        [1, 0, 0],
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
    ];

    for [dx, dy, dz] in offsets {
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

        visit(grid.index(nx as usize, ny as usize, nz as usize));
    }
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

    #[test]
    fn explicit_kernel_can_jump_with_endpoint_path_check() {
        let grid = grid([3, 1, 2]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(1, 0, 0)] = 255;
        occupancy[grid.index(2, 0, 0)] = 255;
        occupancy[grid.index(2, 0, 1)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![KernelMove {
                offset: [2, 0, 1],
                cost: 1.0,
            }],
            KernelPathCheck::EndpointOccupied,
        )
        .unwrap();

        let field = propagate_field(&occupancy, grid, &propagation).unwrap();

        assert_eq!(field.distances[grid.index(2, 0, 1)], 1.0);
    }

    #[test]
    fn explicit_kernel_swept_path_cannot_jump_empty_voxels() {
        let grid = grid([3, 1, 2]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(2, 0, 1)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![KernelMove {
                offset: [2, 0, 1],
                cost: 1.0,
            }],
            KernelPathCheck::SweptOccupied,
        )
        .unwrap();

        let field = propagate_field(&occupancy, grid, &propagation).unwrap();

        assert!(field.distances[grid.index(2, 0, 1)].is_infinite());
    }

    #[test]
    fn explicit_kernel_cannot_cross_disconnected_occupied_gap() {
        let grid = grid([5, 1, 2]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(3, 0, 1)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![KernelMove {
                offset: [3, 0, 1],
                cost: 1.0,
            }],
            KernelPathCheck::EndpointOccupied,
        )
        .unwrap();

        let field = propagate_field(&occupancy, grid, &propagation).unwrap();

        assert!(field.distances[grid.index(3, 0, 1)].is_infinite());
    }

    #[test]
    fn explicit_kernel_can_use_large_move_within_connected_component() {
        let grid = grid([5, 1, 2]);
        let mut occupancy = vec![0; grid.voxel_count()];
        for x in 0..=3 {
            occupancy[grid.index(x, 0, 0)] = 255;
        }
        occupancy[grid.index(3, 0, 1)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![KernelMove {
                offset: [3, 0, 1],
                cost: 1.0,
            }],
            KernelPathCheck::EndpointOccupied,
        )
        .unwrap();

        let field = propagate_field(&occupancy, grid, &propagation).unwrap();

        assert_eq!(field.distances[grid.index(3, 0, 1)], 1.0);
    }

    #[test]
    fn height_constraint_delays_voxels_above_unreached_material() {
        let grid = grid([2, 1, 3]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(0, 0, 1)] = 255;
        occupancy[grid.index(1, 0, 1)] = 255;
        occupancy[grid.index(1, 0, 2)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![
                KernelMove {
                    offset: [1, 0, 2],
                    cost: 1.0,
                },
                KernelMove {
                    offset: [1, 0, 1],
                    cost: 2.0,
                },
                KernelMove {
                    offset: [0, 0, 1],
                    cost: 2.0,
                },
                KernelMove {
                    offset: [1, 0, 0],
                    cost: 2.0,
                },
            ],
            KernelPathCheck::EndpointOccupied,
        )
        .unwrap();

        let field = propagate_field_with_constraints(
            &occupancy,
            grid,
            &propagation,
            PropagationConstraints {
                max_unreached_below_mm: Some(0.5),
                unreached_cone_angle_degrees: None,
                unreached_cone_max_height_mm: None,
            },
        )
        .unwrap();

        assert_eq!(field.distances[grid.index(1, 0, 1)], 2.0);
        assert_eq!(field.distances[grid.index(1, 0, 2)], 2.0);
    }

    #[test]
    fn cone_constraint_delays_voxels_inside_unreached_access_cone() {
        let grid = grid([2, 1, 3]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(0, 0, 1)] = 255;
        occupancy[grid.index(1, 0, 1)] = 255;
        occupancy[grid.index(1, 0, 2)] = 255;
        let propagation = ExplicitKernelPropagation::new(
            vec![
                KernelMove {
                    offset: [1, 0, 2],
                    cost: 1.0,
                },
                KernelMove {
                    offset: [1, 0, 1],
                    cost: 2.0,
                },
                KernelMove {
                    offset: [0, 0, 1],
                    cost: 2.0,
                },
                KernelMove {
                    offset: [1, 0, 0],
                    cost: 2.0,
                },
            ],
            KernelPathCheck::EndpointOccupied,
        )
        .unwrap();

        let field = propagate_field_with_constraints(
            &occupancy,
            grid,
            &propagation,
            PropagationConstraints {
                max_unreached_below_mm: None,
                unreached_cone_angle_degrees: Some(45.0),
                unreached_cone_max_height_mm: None,
            },
        )
        .unwrap();

        assert_eq!(field.distances[grid.index(1, 0, 1)], 2.0);
        assert_eq!(field.distances[grid.index(1, 0, 2)], 2.0);
    }

    #[test]
    fn cone_constraint_is_capped_by_unreached_below_height() {
        let grid = grid([2, 1, 4]);
        let mut occupancy = vec![0; grid.voxel_count()];
        occupancy[grid.index(0, 0, 0)] = 255;
        occupancy[grid.index(1, 0, 3)] = 255;
        let reached = vec![false; grid.voxel_count()];

        assert!(constraints_allow_candidate(
            &occupancy,
            grid,
            grid.index(1, 0, 3),
            &reached,
            PropagationConstraints {
                max_unreached_below_mm: None,
                unreached_cone_angle_degrees: Some(80.0),
                unreached_cone_max_height_mm: Some(1.0),
            },
        ));
    }
}
