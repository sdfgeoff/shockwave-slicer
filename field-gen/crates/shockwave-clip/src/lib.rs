//! Triangle-mesh clipping foundations.
//!
//! This crate owns solid classification and triangle-intersection primitives.
//! Actual triangle splitting is intentionally kept separate so callers do not
//! mistake "classified" triangles for a fully clipped layer mesh.

use std::collections::{HashMap, HashSet};

mod aabb;
mod classify;
mod intersect;

use shockwave_core::geometry::{Triangle, Vec2, Vec3};
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
    let segments = solid_triangle_indices
        .iter()
        .filter_map(|index| triangle_intersection_segment(triangle, &solid.triangles()[*index]))
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return if matches!(
            solid.classify_point(triangle_centroid(triangle)),
            PointClassification::Inside | PointClassification::Boundary
        ) {
            vec![triangle.vertices]
        } else {
            Vec::new()
        };
    }

    triangulate_arrangement(triangle, &segments)
        .into_iter()
        .filter(|fragment| {
            matches!(
                solid.classify_point(triangle_vertices_centroid(fragment)),
                PointClassification::Inside | PointClassification::Boundary
            )
        })
        .collect()
}

fn triangle_intersection_segment(a: &Triangle, b: &Triangle) -> Option<[Vec3; 2]> {
    let mut points = Vec::new();
    collect_edge_plane_intersections(a, b, &mut points);
    collect_edge_plane_intersections(b, a, &mut points);
    let points = deduplicate_points(points);
    if points.len() < 2 {
        return None;
    }

    let mut best = [points[0], points[1]];
    let mut best_distance = length_squared(sub(points[1], points[0]));
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            let distance = length_squared(sub(points[j], points[i]));
            if distance > best_distance {
                best = [points[i], points[j]];
                best_distance = distance;
            }
        }
    }

    (best_distance > EPSILON * EPSILON).then_some(best)
}

fn collect_edge_plane_intersections(subject: &Triangle, plane: &Triangle, points: &mut Vec<Vec3>) {
    for [start, end] in triangle_edges(subject) {
        let Some(point) = segment_plane_intersection(start, end, plane) else {
            continue;
        };
        if point_in_triangle(point, plane) {
            points.push(point);
        }
    }
}

fn segment_plane_intersection(start: Vec3, end: Vec3, plane: &Triangle) -> Option<Vec3> {
    let normal = triangle_normal(plane);
    if length_squared(normal) <= EPSILON {
        return None;
    }

    let start_distance = dot(sub(start, plane.vertices[0]), normal);
    let end_distance = dot(sub(end, plane.vertices[0]), normal);
    let denominator = start_distance - end_distance;
    if denominator.abs() <= EPSILON {
        return None;
    }

    let t = (start_distance / denominator).clamp(0.0, 1.0);
    if !(-EPSILON..=1.0 + EPSILON).contains(&t) {
        return None;
    }

    Some(Vec3 {
        x: start.x + (end.x - start.x) * t,
        y: start.y + (end.y - start.y) * t,
        z: start.z + (end.z - start.z) * t,
    })
}

fn deduplicate_points(points: Vec<Vec3>) -> Vec<Vec3> {
    let mut deduplicated = Vec::new();
    for point in points {
        if deduplicated
            .iter()
            .any(|existing| points_close(*existing, point))
        {
            continue;
        }
        deduplicated.push(point);
    }
    deduplicated
}

#[derive(Clone, Copy, Debug)]
struct Projector {
    origin: Vec3,
    u: Vec3,
    v: Vec3,
}

impl Projector {
    fn from_triangle(triangle: &Triangle) -> Option<Self> {
        let edge = sub(triangle.vertices[1], triangle.vertices[0]);
        let edge_length = length_squared(edge).sqrt();
        if edge_length <= EPSILON {
            return None;
        }
        let normal = triangle_normal(triangle);
        let normal_length = length_squared(normal).sqrt();
        if normal_length <= EPSILON {
            return None;
        }

        let u = scale(edge, 1.0 / edge_length);
        let normal = scale(normal, 1.0 / normal_length);
        let v = cross(normal, u);
        Some(Self {
            origin: triangle.vertices[0],
            u,
            v,
        })
    }

    fn project(self, point: Vec3) -> Vec2 {
        let relative = sub(point, self.origin);
        Vec2 {
            x: dot(relative, self.u),
            y: dot(relative, self.v),
        }
    }

    fn unproject(self, point: Point2) -> Vec3 {
        add(
            self.origin,
            add(scale(self.u, point.x), scale(self.v, point.y)),
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment2d {
    a: Point2,
    b: Point2,
}

#[derive(Clone, Copy, Debug)]
struct Point2 {
    x: f64,
    y: f64,
}

fn triangulate_arrangement(triangle: &Triangle, segments: &[[Vec3; 2]]) -> Vec<[Vec3; 3]> {
    let Some(projector) = Projector::from_triangle(triangle) else {
        return Vec::new();
    };

    let corners = triangle
        .vertices
        .map(|vertex| point2_from_vec2(projector.project(vertex)));
    let mut arrangement_segments = vec![
        Segment2d {
            a: corners[0],
            b: corners[1],
        },
        Segment2d {
            a: corners[1],
            b: corners[2],
        },
        Segment2d {
            a: corners[2],
            b: corners[0],
        },
    ];

    for segment in segments {
        arrangement_segments.push(Segment2d {
            a: point2_from_vec2(projector.project(segment[0])),
            b: point2_from_vec2(projector.project(segment[1])),
        });
    }

    let graph = build_arrangement_graph(&arrangement_segments);
    extract_faces(&graph)
        .into_iter()
        .flat_map(|face| triangulate_face(&face))
        .map(|triangle| {
            [
                projector.unproject(triangle[0]),
                projector.unproject(triangle[1]),
                projector.unproject(triangle[2]),
            ]
        })
        .collect()
}

fn point2_from_vec2(point: Vec2) -> Point2 {
    Point2 {
        x: point.x,
        y: point.y,
    }
}

#[derive(Default)]
struct ArrangementGraph {
    vertices: Vec<Point2>,
    adjacency: Vec<Vec<usize>>,
}

fn build_arrangement_graph(segments: &[Segment2d]) -> ArrangementGraph {
    let mut vertex_map = Point2Map::default();
    let mut edges = HashSet::new();

    for (segment_index, segment) in segments.iter().enumerate() {
        let mut points = vec![segment.a, segment.b];
        for other in &segments[segment_index + 1..] {
            if let Some(point) = segment_intersection_2d(*segment, *other) {
                points.push(point);
            }
        }

        points.sort_by(|a, b| {
            segment_parameter(*segment, *a).total_cmp(&segment_parameter(*segment, *b))
        });
        points.dedup_by(|a, b| points2_close(*a, *b));

        for pair in points.windows(2) {
            if points2_close(pair[0], pair[1]) {
                continue;
            }
            let a = vertex_map.insert(pair[0]);
            let b = vertex_map.insert(pair[1]);
            if a != b {
                edges.insert(ordered_edge(a, b));
            }
        }
    }

    let mut adjacency = vec![Vec::new(); vertex_map.points.len()];
    for (a, b) in edges {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }

    for (index, neighbors) in adjacency.iter_mut().enumerate() {
        neighbors.sort_by(|a, b| {
            angle_between(vertex_map.points[index], vertex_map.points[*a]).total_cmp(
                &angle_between(vertex_map.points[index], vertex_map.points[*b]),
            )
        });
    }

    ArrangementGraph {
        vertices: vertex_map.points,
        adjacency,
    }
}

#[derive(Default)]
struct Point2Map {
    points: Vec<Point2>,
    indices: HashMap<Point2Key, usize>,
}

impl Point2Map {
    fn insert(&mut self, point: Point2) -> usize {
        let key = Point2Key::from_point(point);
        if let Some(index) = self.indices.get(&key) {
            return *index;
        }

        let index = self.points.len();
        self.points.push(point);
        self.indices.insert(key, index);
        index
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Point2Key {
    x: i64,
    y: i64,
}

impl Point2Key {
    fn from_point(point: Point2) -> Self {
        Self {
            x: quantize_coordinate(point.x),
            y: quantize_coordinate(point.y),
        }
    }
}

fn extract_faces(graph: &ArrangementGraph) -> Vec<Vec<Point2>> {
    let mut visited = HashSet::new();
    let mut faces = Vec::new();

    for start in 0..graph.vertices.len() {
        for &next in &graph.adjacency[start] {
            if visited.contains(&(start, next)) {
                continue;
            }

            let mut face_indices = Vec::new();
            let mut current = start;
            let mut target = next;

            while visited.insert((current, target)) {
                face_indices.push(current);
                let neighbors = &graph.adjacency[target];
                let Some(reverse_index) =
                    neighbors.iter().position(|neighbor| *neighbor == current)
                else {
                    break;
                };
                let next_index = if reverse_index == 0 {
                    neighbors.len() - 1
                } else {
                    reverse_index - 1
                };
                current = target;
                target = neighbors[next_index];

                if current == start && target == next {
                    break;
                }
                if face_indices.len() > graph.vertices.len() + graph.adjacency.len() {
                    break;
                }
            }

            if face_indices.len() >= 3 {
                let face = face_indices
                    .into_iter()
                    .map(|index| graph.vertices[index])
                    .collect::<Vec<_>>();
                if polygon_area(&face) > EPSILON {
                    faces.push(face);
                }
            }
        }
    }

    faces
}

fn triangulate_face(face: &[Point2]) -> Vec<[Point2; 3]> {
    let mut indices = (0..face.len()).collect::<Vec<_>>();
    let mut triangles = Vec::new();

    while indices.len() > 3 {
        let mut clipped = false;
        for i in 0..indices.len() {
            let previous = indices[(i + indices.len() - 1) % indices.len()];
            let current = indices[i];
            let next = indices[(i + 1) % indices.len()];
            if !is_convex(face[previous], face[current], face[next]) {
                continue;
            }
            if indices.iter().any(|index| {
                *index != previous
                    && *index != current
                    && *index != next
                    && point_in_triangle_2d(face[*index], face[previous], face[current], face[next])
            }) {
                continue;
            }

            triangles.push([face[previous], face[current], face[next]]);
            indices.remove(i);
            clipped = true;
            break;
        }

        if !clipped {
            break;
        }
    }

    if indices.len() == 3 {
        triangles.push([face[indices[0]], face[indices[1]], face[indices[2]]]);
    }

    triangles
}

fn segment_intersection_2d(a: Segment2d, b: Segment2d) -> Option<Point2> {
    let r = point2_sub(a.b, a.a);
    let s = point2_sub(b.b, b.a);
    let denominator = cross2(r, s);
    if denominator.abs() <= EPSILON {
        return None;
    }

    let qp = point2_sub(b.a, a.a);
    let t = cross2(qp, s) / denominator;
    let u = cross2(qp, r) / denominator;
    if !(-EPSILON..=1.0 + EPSILON).contains(&t) || !(-EPSILON..=1.0 + EPSILON).contains(&u) {
        return None;
    }

    Some(Point2 {
        x: a.a.x + r.x * t.clamp(0.0, 1.0),
        y: a.a.y + r.y * t.clamp(0.0, 1.0),
    })
}

fn segment_parameter(segment: Segment2d, point: Point2) -> f64 {
    let direction = point2_sub(segment.b, segment.a);
    let length = direction.x * direction.x + direction.y * direction.y;
    if length <= EPSILON {
        0.0
    } else {
        let relative = point2_sub(point, segment.a);
        (relative.x * direction.x + relative.y * direction.y) / length
    }
}

fn ordered_edge(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn angle_between(a: Point2, b: Point2) -> f64 {
    (b.y - a.y).atan2(b.x - a.x)
}

fn polygon_area(points: &[Point2]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        area += points[index].x * points[next].y - points[next].x * points[index].y;
    }
    area * 0.5
}

fn is_convex(a: Point2, b: Point2, c: Point2) -> bool {
    cross2(point2_sub(b, a), point2_sub(c, b)) > EPSILON
}

fn point_in_triangle_2d(point: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let ab = cross2(point2_sub(b, a), point2_sub(point, a));
    let bc = cross2(point2_sub(c, b), point2_sub(point, b));
    let ca = cross2(point2_sub(a, c), point2_sub(point, c));
    ab >= -EPSILON && bc >= -EPSILON && ca >= -EPSILON
}

fn point2_sub(a: Point2, b: Point2) -> Point2 {
    Point2 {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn cross2(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn points2_close(a: Point2, b: Point2) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= EPSILON * EPSILON
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

fn point_in_triangle(point: Vec3, triangle: &Triangle) -> bool {
    let a = triangle.vertices[0];
    let b = triangle.vertices[1];
    let c = triangle.vertices[2];
    let normal = cross(sub(b, a), sub(c, a));
    if dot(normal, sub(point, a)).abs() > SPLIT_DISTANCE_EPSILON * length_squared(normal).sqrt() {
        return false;
    }

    let c0 = cross(sub(b, a), sub(point, a));
    let c1 = cross(sub(c, b), sub(point, b));
    let c2 = cross(sub(a, c), sub(point, c));
    let d0 = dot(normal, c0);
    let d1 = dot(normal, c1);
    let d2 = dot(normal, c2);
    d0 >= -EPSILON && d1 >= -EPSILON && d2 >= -EPSILON
}

fn triangle_edges(triangle: &Triangle) -> [[Vec3; 2]; 3] {
    [
        [triangle.vertices[0], triangle.vertices[1]],
        [triangle.vertices[1], triangle.vertices[2]],
        [triangle.vertices[2], triangle.vertices[0]],
    ]
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

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}

fn scale(a: Vec3, scale: f64) -> Vec3 {
    Vec3 {
        x: a.x * scale,
        y: a.y * scale,
        z: a.z * scale,
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
