use std::sync::Arc;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, mouse};
use iced::{Element, Length, Rectangle, Size, Theme};
use iced_wgpu::wgpu;
use shockwave_config::Dimensions3;
use shockwave_math::geometry::{Triangle, Vec3};

use crate::gpu_common::{
    Bounds3, PREVIEW_HEIGHT, TransformUniform, Vertex3D, bed_corners, data_signature,
};
use crate::gpu_mesh_pipeline::MeshPipeline;

pub fn scene_view<Message: 'static>(
    geometry: Arc<ModelPreviewGeometry>,
) -> Element<'static, Message> {
    Element::new(GpuMeshPreview {
        geometry,
        width: Length::Fill,
        height: Length::Fixed(PREVIEW_HEIGHT),
    })
}

#[derive(Clone, Debug)]
pub struct ModelPreviewGeometry {
    pub(crate) vertices: Vec<Vertex3D>,
    pub(crate) indices: Vec<u32>,
    pub(crate) transform: TransformUniform,
    pub(crate) signature: u64,
}

impl ModelPreviewGeometry {
    pub fn from_scene(triangles: &[Triangle], print_volume: Dimensions3) -> Self {
        let mut bounds = Bounds3::from_print_volume(print_volume);
        for triangle in triangles {
            for vertex in triangle.vertices {
                bounds.include(vertex);
            }
        }

        let mut vertices = Vec::with_capacity(4 + triangles.len() * 3);
        let mut indices = Vec::with_capacity(6 + triangles.len() * 3);
        push_bed(&mut vertices, &mut indices, print_volume);
        push_model(&mut vertices, &mut indices, triangles);

        let signature = geometry_signature(&vertices, &indices);
        Self {
            vertices,
            indices,
            transform: TransformUniform::from_bounds(bounds),
            signature,
        }
    }

    pub(crate) fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}

impl Default for ModelPreviewGeometry {
    fn default() -> Self {
        Self::from_scene(
            &[],
            Dimensions3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        )
    }
}

#[derive(Debug)]
struct GpuMeshPreview {
    geometry: Arc<ModelPreviewGeometry>,
    width: Length,
    height: Length,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for GpuMeshPreview
where
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        renderer.draw_primitive(
            layout.bounds(),
            MeshPrimitive {
                geometry: Arc::clone(&self.geometry),
            },
        );
    }
}

impl<'a, Message, Renderer> From<GpuMeshPreview> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(value: GpuMeshPreview) -> Self {
        Element::new(value)
    }
}

#[derive(Debug)]
struct MeshPrimitive {
    geometry: Arc<ModelPreviewGeometry>,
}

impl iced_wgpu::Primitive for MeshPrimitive {
    type Pipeline = MeshPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        _viewport: &iced_wgpu::graphics::Viewport,
    ) {
        pipeline.prepare(device, queue, bounds, &self.geometry);
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, clip_bounds, self.geometry.index_count());
    }
}

fn push_bed(vertices: &mut Vec<Vertex3D>, indices: &mut Vec<u32>, print_volume: Dimensions3) {
    let color = [0.12, 0.16, 0.18];
    push_quad(vertices, indices, bed_corners(print_volume), color);
}

fn push_model(vertices: &mut Vec<Vertex3D>, indices: &mut Vec<u32>, triangles: &[Triangle]) {
    for triangle in triangles {
        let normal = triangle_normal(triangle);
        let shade = (0.38 + normal.z.abs() * 0.42).clamp(0.22, 0.92) as f32;
        push_triangle(vertices, indices, triangle.vertices, [0.08, shade, 0.96]);
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

fn push_triangle(
    vertices: &mut Vec<Vertex3D>,
    indices: &mut Vec<u32>,
    points: [Vec3; 3],
    color: [f32; 3],
) {
    let start = vertices.len() as u32;
    vertices.extend(points.map(|point| Vertex3D::new(point, color)));
    indices.extend([start, start + 1, start + 2]);
}

fn triangle_normal(triangle: &Triangle) -> Vec3 {
    let a = triangle.vertices[0];
    let b = triangle.vertices[1];
    let c = triangle.vertices[2];
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

fn geometry_signature(vertices: &[Vertex3D], indices: &[u32]) -> u64 {
    let mut bytes =
        Vec::with_capacity(std::mem::size_of_val(vertices) + std::mem::size_of_val(indices));
    bytes.extend_from_slice(bytemuck::cast_slice(vertices));
    bytes.extend_from_slice(bytemuck::cast_slice(indices));
    data_signature(&bytes, vertices.len() + indices.len())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn preview_scene_contains_bed_and_indexed_model_vertices() {
        let triangle = Triangle {
            vertices: [
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
        };
        let preview = ModelPreviewGeometry::from_scene(
            &[triangle],
            Dimensions3 {
                x: 100.0,
                y: 100.0,
                z: 100.0,
            },
        );

        assert_eq!(preview.vertices.len(), 7);
        assert_eq!(preview.indices.len(), 9);
        assert_ne!(preview.signature, 0);
    }

    #[test]
    fn scene_view_takes_shared_geometry() {
        let geometry = Arc::new(ModelPreviewGeometry::default());
        let _element = scene_view::<()>(geometry);
    }
}
