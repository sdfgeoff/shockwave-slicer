use std::sync::Arc;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, mouse};
use iced::{Element, Length, Rectangle, Size, Theme};
use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;
use shockwave_config::Dimensions3;
use shockwave_math::geometry::Vec3;
use shockwave_path::{LayerToolpaths, ToolpathRole};

use crate::gpu_common::{
    Bounds3, PREVIEW_HEIGHT, TransformUniform, Vertex3D, bed_corners, data_signature,
};

pub fn scene_view<Message: 'static>(
    geometry: Arc<ToolpathPreviewGeometry>,
) -> Element<'static, Message> {
    Element::new(GpuToolpathPreview {
        geometry,
        width: Length::Fill,
        height: Length::Fixed(PREVIEW_HEIGHT),
    })
}

#[derive(Clone, Debug)]
pub struct ToolpathPreviewGeometry {
    vertices: Vec<Vertex3D>,
    transform: TransformUniform,
    signature: u64,
}

impl ToolpathPreviewGeometry {
    pub fn from_scene(layers: &[LayerToolpaths], print_volume: Dimensions3) -> Self {
        let mut bounds = Bounds3::from_print_volume(print_volume);
        for layer in layers {
            for path in &layer.paths {
                for point in &path.points {
                    bounds.include(point.position);
                }
            }
        }

        let mut vertices = Vec::new();
        push_bed_outline(&mut vertices, print_volume);
        push_toolpaths(&mut vertices, layers);

        let signature = data_signature(bytemuck::cast_slice(&vertices), vertices.len());
        Self {
            vertices,
            transform: TransformUniform::from_bounds(bounds),
            signature,
        }
    }

    fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }
}

impl Default for ToolpathPreviewGeometry {
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
struct GpuToolpathPreview {
    geometry: Arc<ToolpathPreviewGeometry>,
    width: Length,
    height: Length,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for GpuToolpathPreview
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
            ToolpathPrimitive {
                geometry: Arc::clone(&self.geometry),
            },
        );
    }
}

impl<'a, Message, Renderer> From<GpuToolpathPreview> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(value: GpuToolpathPreview) -> Self {
        Element::new(value)
    }
}

#[derive(Debug)]
struct ToolpathPrimitive {
    geometry: Arc<ToolpathPreviewGeometry>,
}

impl iced_wgpu::Primitive for ToolpathPrimitive {
    type Pipeline = ToolpathPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &iced_wgpu::graphics::Viewport,
    ) {
        pipeline.prepare(device, queue, &self.geometry);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        pipeline.draw(render_pass, self.geometry.vertex_count());
        true
    }
}

#[derive(Debug)]
struct ToolpathPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: Option<wgpu::Buffer>,
    uniform_buffer: Option<wgpu::Buffer>,
    uniform_bind_group: Option<wgpu::BindGroup>,
    signature: Option<u64>,
}

impl iced_wgpu::primitive::Pipeline for ToolpathPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/toolpath_preview.wgsl").into()),
        });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shockwave-gui.gpu-toolpath-preview.uniform-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex3D::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_bind_group_layout,
            vertex_buffer: None,
            uniform_buffer: None,
            uniform_bind_group: None,
            signature: None,
        }
    }
}

impl ToolpathPipeline {
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry: &ToolpathPreviewGeometry,
    ) {
        self.prepare_uniform(device, queue, geometry.transform);
        if self.signature == Some(geometry.signature) {
            return;
        }

        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shockwave-gui.gpu-toolpath-preview.vertices"),
                contents: bytemuck::cast_slice(&geometry.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.signature = Some(geometry.signature);
    }

    fn prepare_uniform(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        transform: TransformUniform,
    ) {
        if let Some(buffer) = &self.uniform_buffer {
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&transform));
            return;
        }

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.uniform"),
            contents: bytemuck::bytes_of(&transform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.uniform-bind-group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        self.uniform_buffer = Some(buffer);
        self.uniform_bind_group = Some(bind_group);
    }

    fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>, vertex_count: u32) {
        let (Some(vertex_buffer), Some(bind_group)) = (
            self.vertex_buffer.as_ref(),
            self.uniform_bind_group.as_ref(),
        ) else {
            return;
        };
        if vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.draw(0..vertex_count, 0..1);
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

#[cfg(test)]
mod tests {
    use super::*;
    use shockwave_path::{PathPoint, Toolpath};

    #[test]
    fn preview_scene_emits_bed_and_toolpath_segments() {
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
        let preview = ToolpathPreviewGeometry::from_scene(
            &[layer],
            Dimensions3 {
                x: 100.0,
                y: 100.0,
                z: 100.0,
            },
        );

        assert_eq!(preview.vertices.len(), 14);
        assert_ne!(preview.signature, 0);
    }

    fn point(x: f64, y: f64, z: f64) -> PathPoint {
        PathPoint {
            position: Vec3 { x, y, z },
            extrusion_width_mm: 0.4,
            layer_height_mm: 0.2,
        }
    }
}
