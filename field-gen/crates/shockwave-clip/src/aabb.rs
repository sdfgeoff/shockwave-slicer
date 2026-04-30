use shockwave_core::geometry::Vec3;

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

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }
}
