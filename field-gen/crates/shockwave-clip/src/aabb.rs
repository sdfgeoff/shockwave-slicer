use shockwave_math::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    pub(crate) fn from_points(points: &[Vec3]) -> Self {
        let mut min = points[0];
        let mut max = points[0];
        for point in &points[1..] {
            min = min.min(*point);
            max = max.max(*point);
        }
        Self { min, max }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub(crate) fn longest_axis(self) -> usize {
        let extent = Vec3 {
            x: self.max.x - self.min.x,
            y: self.max.y - self.min.y,
            z: self.max.z - self.min.z,
        };
        if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        }
    }

    pub(crate) fn centroid_axis(self, axis: usize) -> f64 {
        match axis {
            0 => (self.min.x + self.max.x) * 0.5,
            1 => (self.min.y + self.max.y) * 0.5,
            _ => (self.min.z + self.max.z) * 0.5,
        }
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    pub(crate) fn intersects_ray(self, origin: Vec3, direction: Vec3) -> bool {
        let (mut t_min, mut t_max) = axis_ray_range(origin.x, direction.x, self.min.x, self.max.x);
        let (y_min, y_max) = axis_ray_range(origin.y, direction.y, self.min.y, self.max.y);
        t_min = t_min.max(y_min);
        t_max = t_max.min(y_max);

        let (z_min, z_max) = axis_ray_range(origin.z, direction.z, self.min.z, self.max.z);
        t_min = t_min.max(z_min);
        t_max = t_max.min(z_max);

        t_max >= t_min.max(0.0)
    }
}

fn axis_ray_range(origin: f64, direction: f64, min: f64, max: f64) -> (f64, f64) {
    if direction.abs() <= f64::EPSILON {
        if origin < min || origin > max {
            (f64::INFINITY, f64::NEG_INFINITY)
        } else {
            (f64::NEG_INFINITY, f64::INFINITY)
        }
    } else {
        let a = (min - origin) / direction;
        let b = (max - origin) / direction;
        (a.min(b), a.max(b))
    }
}
