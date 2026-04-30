use shockwave_core::geometry::{Triangle, Vec3};

const EPSILON: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriangleIntersection {
    None,
    Intersects,
}

pub fn triangles_intersect(a: &Triangle, b: &Triangle) -> bool {
    triangle_edges(a)
        .iter()
        .any(|[start, end]| segment_intersects_triangle(*start, *end, b))
        || triangle_edges(b)
            .iter()
            .any(|[start, end]| segment_intersects_triangle(*start, *end, a))
        || point_in_triangle(a.vertices[0], b)
        || point_in_triangle(b.vertices[0], a)
}

pub(crate) fn ray_intersects_triangle(
    origin: Vec3,
    direction: Vec3,
    triangle: &Triangle,
) -> Option<f64> {
    let edge1 = sub(triangle.vertices[1], triangle.vertices[0]);
    let edge2 = sub(triangle.vertices[2], triangle.vertices[0]);
    let h = cross(direction, edge2);
    let determinant = dot(edge1, h);
    if determinant.abs() <= EPSILON {
        return None;
    }

    let inverse = 1.0 / determinant;
    let s = sub(origin, triangle.vertices[0]);
    let u = inverse * dot(s, h);
    if !(0.0 - EPSILON..=1.0 + EPSILON).contains(&u) {
        return None;
    }

    let q = cross(s, edge1);
    let v = inverse * dot(direction, q);
    if v < -EPSILON || u + v > 1.0 + EPSILON {
        return None;
    }

    let t = inverse * dot(edge2, q);
    Some(t)
}

fn segment_intersects_triangle(start: Vec3, end: Vec3, triangle: &Triangle) -> bool {
    let direction = sub(end, start);
    let Some(t) = ray_intersects_triangle(start, direction, triangle) else {
        return false;
    };
    (-EPSILON..=1.0 + EPSILON).contains(&t)
}

fn point_in_triangle(point: Vec3, triangle: &Triangle) -> bool {
    let a = triangle.vertices[0];
    let b = triangle.vertices[1];
    let c = triangle.vertices[2];
    let normal = cross(sub(b, a), sub(c, a));
    if dot(normal, sub(point, a)).abs() > EPSILON {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn detects_crossing_triangles() {
        let a = Triangle {
            vertices: [v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
        };
        let b = Triangle {
            vertices: [v(0.25, 0.25, -1.0), v(0.25, 0.25, 1.0), v(0.75, 0.25, 0.0)],
        };

        assert!(triangles_intersect(&a, &b));
    }

    #[test]
    fn rejects_separated_triangles() {
        let a = Triangle {
            vertices: [v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)],
        };
        let b = Triangle {
            vertices: [v(0.0, 0.0, 1.0), v(1.0, 0.0, 1.0), v(0.0, 1.0, 1.0)],
        };

        assert!(!triangles_intersect(&a, &b));
    }
}
