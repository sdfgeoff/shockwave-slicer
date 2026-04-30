use shockwave_core::geometry::Vec3;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
}

impl Mesh {
    pub fn triangle_vertices(&self, triangle: [usize; 3]) -> [Vec3; 3] {
        [
            self.vertices[triangle[0]],
            self.vertices[triangle[1]],
            self.vertices[triangle[2]],
        ]
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }
}
