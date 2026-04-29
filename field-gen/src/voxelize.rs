use rayon::prelude::*;

use crate::geometry::{Segment2, Triangle, Vec2};
use crate::grid::Grid;

const EPSILON: f64 = 1.0e-9;

pub fn generate_occupancy(triangles: &[Triangle], grid: Grid) -> Vec<u8> {
    let slice_len = grid.dims[0] * grid.dims[1];
    let mut slices: Vec<(usize, Vec<u8>)> = (0..grid.dims[2])
        .into_par_iter()
        .map(|z| (z, generate_slice_occupancy(triangles, grid, z)))
        .collect();

    slices.sort_by_key(|(z, _)| *z);

    let mut occupancy = Vec::with_capacity(slice_len * grid.dims[2]);
    for (_, slice) in slices {
        occupancy.extend(slice);
    }

    occupancy
}

fn generate_slice_occupancy(triangles: &[Triangle], grid: Grid, z: usize) -> Vec<u8> {
    let mut slice = vec![0; grid.dims[0] * grid.dims[1]];
    let z_position = grid.origin.z + (z as f64 + 0.5) * grid.voxel_size.z;
    let segments = slice_segments(triangles, z_position);

    for y in 0..grid.dims[1] {
        let y_position = grid.origin.y + (y as f64 + 0.5) * grid.voxel_size.y;
        let mut crossings = row_crossings(&segments, y_position);
        if crossings.len() < 2 {
            continue;
        }

        crossings.sort_by(|a, b| a.total_cmp(b));
        dedupe_sorted_f64(&mut crossings, 1.0e-7);

        for interval in crossings.chunks_exact(2) {
            let left = interval[0].min(interval[1]);
            let right = interval[0].max(interval[1]);
            let start_x = voxel_index_at_or_after(left, grid.origin.x, grid.voxel_size.x);
            let end_x = voxel_index_before(right, grid.origin.x, grid.voxel_size.x);
            let start_x = start_x.min(grid.dims[0]);
            let end_x = end_x.min(grid.dims[0]);

            for x in start_x..end_x {
                slice[x + y * grid.dims[0]] = 255;
            }
        }
    }

    slice
}

fn slice_segments(triangles: &[Triangle], z: f64) -> Vec<Segment2> {
    triangles
        .iter()
        .filter_map(|triangle| triangle_z_intersection(triangle, z))
        .collect()
}

fn triangle_z_intersection(triangle: &Triangle, z: f64) -> Option<Segment2> {
    let vertices = triangle.vertices;
    let edges = [
        (vertices[0], vertices[1]),
        (vertices[1], vertices[2]),
        (vertices[2], vertices[0]),
    ];
    let mut points = Vec::with_capacity(2);

    for (a, b) in edges {
        let a_offset = a.z - z;
        let b_offset = b.z - z;

        if a_offset.abs() <= EPSILON && b_offset.abs() <= EPSILON {
            continue;
        }

        if (a_offset <= EPSILON && b_offset > EPSILON)
            || (b_offset <= EPSILON && a_offset > EPSILON)
        {
            let t = (z - a.z) / (b.z - a.z);
            points.push(Vec2 {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            });
        }
    }

    dedupe_vec2(&mut points, 1.0e-8);

    if points.len() == 2 && distance_squared_2d(points[0], points[1]) > 1.0e-16 {
        Some(Segment2 {
            a: points[0],
            b: points[1],
        })
    } else {
        None
    }
}

fn row_crossings(segments: &[Segment2], y: f64) -> Vec<f64> {
    let mut crossings = Vec::new();

    for segment in segments {
        let y0 = segment.a.y;
        let y1 = segment.b.y;

        if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
            let t = (y - y0) / (y1 - y0);
            crossings.push(segment.a.x + (segment.b.x - segment.a.x) * t);
        }
    }

    crossings
}

fn voxel_index_at_or_after(position: f64, origin: f64, voxel_size: f64) -> usize {
    let value = ((position - origin) / voxel_size - 0.5).ceil();
    if value <= 0.0 { 0 } else { value as usize }
}

fn voxel_index_before(position: f64, origin: f64, voxel_size: f64) -> usize {
    let value = ((position - origin) / voxel_size - 0.5).ceil();
    if value <= 0.0 { 0 } else { value as usize }
}

fn dedupe_sorted_f64(values: &mut Vec<f64>, epsilon: f64) {
    let mut write_index = 0;
    for read_index in 0..values.len() {
        if write_index == 0 || (values[read_index] - values[write_index - 1]).abs() > epsilon {
            values[write_index] = values[read_index];
            write_index += 1;
        }
    }
    values.truncate(write_index);
}

fn dedupe_vec2(points: &mut Vec<Vec2>, epsilon: f64) {
    let mut unique = Vec::with_capacity(points.len());

    for point in points.iter().copied() {
        if !unique
            .iter()
            .any(|existing| distance_squared_2d(point, *existing) <= epsilon * epsilon)
        {
            unique.push(point);
        }
    }

    *points = unique;
}

fn distance_squared_2d(a: Vec2, b: Vec2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Vec3;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    fn tri(a: Vec3, b: Vec3, c: Vec3) -> Triangle {
        Triangle {
            vertices: [a, b, c],
        }
    }

    fn cube_triangles(min: Vec3, max: Vec3) -> Vec<Triangle> {
        vec![
            tri(
                v(min.x, min.y, min.z),
                v(max.x, max.y, min.z),
                v(max.x, min.y, min.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, max.y, min.z),
                v(max.x, max.y, min.z),
            ),
            tri(
                v(min.x, min.y, max.z),
                v(max.x, min.y, max.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, max.z),
                v(max.x, max.y, max.z),
                v(min.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(max.x, min.y, min.z),
                v(max.x, min.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(max.x, min.y, max.z),
                v(min.x, min.y, max.z),
            ),
            tri(
                v(min.x, max.y, min.z),
                v(max.x, max.y, max.z),
                v(max.x, max.y, min.z),
            ),
            tri(
                v(min.x, max.y, min.z),
                v(min.x, max.y, max.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, min.y, max.z),
                v(min.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, max.y, max.z),
                v(min.x, max.y, min.z),
            ),
            tri(
                v(max.x, min.y, min.z),
                v(max.x, max.y, min.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(max.x, min.y, min.z),
                v(max.x, max.y, max.z),
                v(max.x, min.y, max.z),
            ),
        ]
    }

    fn grid(origin: Vec3, dims: [usize; 3], voxel_size: Vec3) -> Grid {
        Grid {
            origin,
            dims,
            voxel_size,
            actual_size: Vec3 {
                x: dims[0] as f64 * voxel_size.x,
                y: dims[1] as f64 * voxel_size.y,
                z: dims[2] as f64 * voxel_size.z,
            },
        }
    }

    #[test]
    fn cube_occupies_all_voxel_centers_inside_it() {
        let triangles = cube_triangles(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0));
        let occupancy = generate_occupancy(
            &triangles,
            grid(v(0.0, 0.0, 0.0), [2, 2, 2], v(5.0, 5.0, 5.0)),
        );

        assert_eq!(occupancy, vec![255; 8]);
    }

    #[test]
    fn cube_leaves_voxel_centers_outside_it_empty() {
        let triangles = cube_triangles(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0));
        let occupancy = generate_occupancy(
            &triangles,
            grid(v(-5.0, -5.0, -5.0), [4, 4, 4], v(5.0, 5.0, 5.0)),
        );
        let occupied_count = occupancy.iter().filter(|value| **value == 255).count();

        assert_eq!(occupied_count, 8);
    }
}
