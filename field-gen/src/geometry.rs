#[derive(Clone, Copy, Debug)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
            z: self.z.max(other.z),
        }
    }

    pub fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Segment2 {
    pub a: Vec2,
    pub b: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

pub fn mesh_bounds(triangles: &[Triangle]) -> Bounds {
    let mut min = triangles[0].vertices[0];
    let mut max = triangles[0].vertices[0];

    for triangle in triangles {
        for vertex in triangle.vertices {
            min = min.min(vertex);
            max = max.max(vertex);
        }
    }

    Bounds { min, max }
}
