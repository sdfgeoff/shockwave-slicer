use shockwave_config::Dimensions3;
use shockwave_math::geometry::Vec3;
use shockwave_mesh::Mesh;
use shockwave_path::{LayerToolpaths, ToolpathRole};

use crate::common::{
    CameraTransform, ObjectTransform, SceneBounds, Vertex3D, bed_corners, data_signature,
    geometry_signature,
};

#[derive(Clone, Debug)]
pub struct RenderScene {
    pub camera: CameraTransform,
    pub meshes: Vec<RenderMesh>,
    pub lines: Vec<RenderLines>,
    pub bounds: SceneBounds,
}

impl RenderScene {
    pub fn from_scene(mesh: &Mesh, layers: &[LayerToolpaths], print_volume: Dimensions3) -> Self {
        let mut bounds = SceneBounds::from_print_volume(print_volume);
        for vertex in &mesh.vertices {
            bounds.include(*vertex);
        }
        for layer in layers {
            for path in &layer.paths {
                for point in &path.points {
                    bounds.include(point.position);
                }
            }
        }

        Self {
            camera: CameraTransform::fit_isometric(bounds),
            meshes: vec![
                RenderMesh::print_bed(print_volume),
                RenderMesh::from_mesh(mesh, [0.08, 0.62, 0.96]),
            ],
            lines: vec![RenderLines::from_toolpaths(layers, print_volume)],
            bounds,
        }
    }

    pub fn with_orbit_camera(mut self, yaw_radians: f32, pitch_radians: f32, zoom: f32) -> Self {
        self.camera = CameraTransform::orbit(self.bounds, yaw_radians, pitch_radians, zoom);
        self
    }
}

impl Default for RenderScene {
    fn default() -> Self {
        Self::from_scene(
            &Mesh::default(),
            &[],
            Dimensions3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        )
    }
}

#[derive(Clone, Debug)]
pub struct RenderMesh {
    pub transform: ObjectTransform,
    pub(crate) vertices: Vec<Vertex3D>,
    pub(crate) indices: Vec<u32>,
    pub(crate) signature: u64,
}

impl RenderMesh {
    fn print_bed(print_volume: Dimensions3) -> Self {
        let color = [0.12, 0.16, 0.18];
        let mut vertices = Vec::with_capacity(4);
        let mut indices = Vec::with_capacity(6);
        push_quad(
            &mut vertices,
            &mut indices,
            bed_corners(print_volume),
            color,
        );
        let signature = geometry_signature(&vertices, &indices);
        Self {
            transform: ObjectTransform::identity(),
            vertices,
            indices,
            signature,
        }
    }

    fn from_mesh(mesh: &Mesh, color: [f32; 3]) -> Self {
        let mut vertices = Vec::with_capacity(mesh.vertices.len());
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        vertices.extend(
            mesh.vertices
                .iter()
                .copied()
                .map(|point| Vertex3D::new(point, color)),
        );

        for triangle in &mesh.triangles {
            let points = mesh.triangle_vertices(*triangle);
            let normal = triangle_normal(points);
            let shade = (0.38 + normal.z.abs() * 0.42).clamp(0.22, 0.92) as f32;
            for index in triangle {
                vertices[*index].color = [0.08, shade, 0.96];
            }
            indices.extend(triangle.map(|index| index as u32));
        }

        let signature = geometry_signature(&vertices, &indices);
        Self {
            transform: ObjectTransform::identity(),
            vertices,
            indices,
            signature,
        }
    }

    pub(crate) fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}

#[derive(Clone, Debug)]
pub struct RenderLines {
    pub transform: ObjectTransform,
    pub(crate) vertices: Vec<Vertex3D>,
    pub(crate) signature: u64,
}

impl RenderLines {
    fn from_toolpaths(layers: &[LayerToolpaths], print_volume: Dimensions3) -> Self {
        let mut vertices = Vec::new();
        push_bed_outline(&mut vertices, print_volume);
        push_toolpaths(&mut vertices, layers);

        let signature = data_signature(bytemuck::cast_slice(&vertices), vertices.len());
        Self {
            transform: ObjectTransform::identity(),
            vertices,
            signature,
        }
    }

    pub(crate) fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }
}

fn push_bed_outline(vertices: &mut Vec<Vertex3D>, print_volume: Dimensions3) {
    let corners = bed_corners(print_volume);
    let color = [0.28, 0.36, 0.4];
    push_line(vertices, corners[0], corners[1], color);
    push_line(vertices, corners[1], corners[2], color);
    push_line(vertices, corners[2], corners[3], color);
    push_line(vertices, corners[3], corners[0], color);
}

fn push_toolpaths(vertices: &mut Vec<Vertex3D>, layers: &[LayerToolpaths]) {
    for layer in layers {
        for path in &layer.paths {
            let color = role_color(path.role);
            for segment in path.points.windows(2) {
                push_line(vertices, segment[0].position, segment[1].position, color);
            }
            if path.closed && path.points.len() > 2 {
                push_line(
                    vertices,
                    path.points.last().unwrap().position,
                    path.points[0].position,
                    color,
                );
            }
        }
    }
}

fn push_quad(
    vertices: &mut Vec<Vertex3D>,
    indices: &mut Vec<u32>,
    points: [Vec3; 4],
    color: [f32; 3],
) {
    let start = vertices.len() as u32;
    vertices.extend(points.map(|point| Vertex3D::new(point, color)));
    indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn push_line(vertices: &mut Vec<Vertex3D>, start: Vec3, end: Vec3, color: [f32; 3]) {
    vertices.push(Vertex3D::new(start, color));
    vertices.push(Vertex3D::new(end, color));
}

fn role_color(role: ToolpathRole) -> [f32; 3] {
    match role {
        ToolpathRole::Perimeter => [0.05, 1.0, 0.35],
        ToolpathRole::Infill => [1.0, 0.74, 0.18],
        ToolpathRole::Travel => [0.62, 0.64, 0.68],
    }
}

fn triangle_normal(points: [Vec3; 3]) -> Vec3 {
    let a = points[0];
    let b = points[1];
    let c = points[2];
    let ux = b.x - a.x;
    let uy = b.y - a.y;
    let uz = b.z - a.z;
    let vx = c.x - a.x;
    let vy = c.y - a.y;
    let vz = c.z - a.z;
    let normal = Vec3 {
        x: uy * vz - uz * vy,
        y: uz * vx - ux * vz,
        z: ux * vy - uy * vx,
    };
    let length = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
    if length <= f64::EPSILON {
        return normal;
    }
    Vec3 {
        x: normal.x / length,
        y: normal.y / length,
        z: normal.z / length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shockwave_path::{PathPoint, Toolpath};

    #[test]
    fn scene_contains_bed_model_and_toolpath_objects() {
        let mesh = Mesh {
            vertices: vec![
                Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                Vec3 {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
                Vec3 {
                    x: 0.0,
                    y: 10.0,
                    z: 5.0,
                },
            ],
            triangles: vec![[0, 1, 2]],
        };
        let layer = LayerToolpaths {
            field_value: 1.0,
            paths: vec![Toolpath {
                points: vec![
                    point(0.0, 0.0, 0.2),
                    point(10.0, 0.0, 0.2),
                    point(10.0, 10.0, 0.2),
                ],
                role: ToolpathRole::Perimeter,
                closed: true,
            }],
        };
        let scene = RenderScene::from_scene(
            &mesh,
            &[layer],
            Dimensions3 {
                x: 100.0,
                y: 100.0,
                z: 100.0,
            },
        );

        assert_eq!(scene.meshes.len(), 2);
        assert_eq!(scene.meshes[0].indices.len(), 6);
        assert_eq!(scene.meshes[1].indices.len(), 3);
        assert_eq!(scene.lines.len(), 1);
        assert_eq!(scene.lines[0].vertices.len(), 14);
    }

    #[test]
    fn orbit_camera_changes_scene_camera_without_rebuilding_geometry() {
        let scene = RenderScene::default();
        let mesh_signature = scene.meshes[0].signature;
        let rotated = scene.with_orbit_camera(0.2, 0.4, 1.1);

        assert_ne!(rotated.camera, CameraTransform::default());
        assert_eq!(rotated.meshes[0].signature, mesh_signature);
    }

    fn point(x: f64, y: f64, z: f64) -> PathPoint {
        PathPoint {
            position: Vec3 { x, y, z },
            extrusion_width_mm: 0.4,
            layer_height_mm: 0.2,
        }
    }
}
