use shockwave_core::geometry::Vec3;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
}

#[derive(Clone, Debug)]
pub struct Isosurface {
    pub level: usize,
    pub value: f64,
    pub mesh: Mesh,
}

#[derive(Clone, Debug, Default)]
pub struct IsosurfaceSet {
    pub surfaces: Vec<Isosurface>,
}

impl IsosurfaceSet {
    pub fn vertex_count(&self) -> usize {
        self.surfaces
            .iter()
            .map(|surface| surface.mesh.vertices.len())
            .sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.surfaces
            .iter()
            .map(|surface| surface.mesh.triangles.len())
            .sum()
    }
}
