use shockwave_math::geometry::{Triangle, Vec3};

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
}

impl Mesh {
    pub fn from_triangles(triangles: &[Triangle]) -> Self {
        let mut vertices = Vec::with_capacity(triangles.len() * 3);
        let mut indices = Vec::with_capacity(triangles.len());

        for triangle in triangles {
            let start = vertices.len();
            vertices.extend(triangle.vertices);
            indices.push([start, start + 1, start + 2]);
        }

        Self {
            vertices,
            triangles: indices,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_mesh_from_triangle_soup() {
        let triangles = vec![Triangle {
            vertices: [
                Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            ],
        }];

        let mesh = Mesh::from_triangles(&triangles);

        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    }
}
