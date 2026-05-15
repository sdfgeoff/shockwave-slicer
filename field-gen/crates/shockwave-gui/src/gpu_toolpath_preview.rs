use std::hash::{Hash, Hasher};

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, mouse};
use iced::{Element, Length, Point, Rectangle, Size, Theme};
use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;
use shockwave_config::Dimensions3;
use shockwave_math::geometry::Vec3;
use shockwave_path::{LayerToolpaths, ToolpathRole};

pub fn scene_view<'a, Message: 'a>(
    layers: &'a [LayerToolpaths],
    print_volume: Dimensions3,
) -> Element<'a, Message> {
    Element::new(GpuToolpathPreview {
        layers,
        print_volume,
        width: Length::Fill,
        height: Length::Fixed(280.0),
    })
}

#[derive(Debug)]
struct GpuToolpathPreview<'a> {
    layers: &'a [LayerToolpaths],
    print_volume: Dimensions3,
    width: Length,
    height: Length,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for GpuToolpathPreview<'_>
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
            ToolpathPrimitive::from_scene(self.layers, self.print_volume),
        );
    }
}

impl<'a, Message, Renderer> From<GpuToolpathPreview<'a>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(value: GpuToolpathPreview<'a>) -> Self {
        Element::new(value)
    }
}

#[derive(Debug)]
struct ToolpathPrimitive {
    vertices: Vec<Vertex>,
    signature: u64,
}

impl ToolpathPrimitive {
    fn from_scene(layers: &[LayerToolpaths], print_volume: Dimensions3) -> Self {
        let mut projected = bed_corners(print_volume)
            .iter()
            .map(|point| project(*point))
            .collect::<Vec<_>>();
        for layer in layers {
            for path in &layer.paths {
                projected.extend(path.points.iter().map(|point| project(point.position)));
            }
        }
        let transform = ViewTransform::fit(&projected);

        let mut vertices = Vec::new();
        push_bed_outline(&mut vertices, print_volume, transform);
        push_toolpaths(&mut vertices, layers, transform);

        let signature = vertex_signature(&vertices);
        Self {
            vertices,
            signature,
        }
    }
}

impl iced_wgpu::Primitive for ToolpathPrimitive {
    type Pipeline = ToolpathPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &iced_wgpu::graphics::Viewport,
    ) {
        pipeline.prepare(device, &self.vertices, self.signature);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(vertex_buffer) = pipeline.vertex_buffer.as_ref() else {
            return true;
        };
        if pipeline.vertex_count == 0 {
            return true;
        }

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.draw(0..pipeline.vertex_count, 0..1);
        true
    }
}

#[derive(Debug)]
struct ToolpathPipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    signature: Option<u64>,
}

impl iced_wgpu::primitive::Pipeline for ToolpathPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.shader"),
            source: wgpu::ShaderSource::Wgsl(TOOLPATH_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shockwave-gui.gpu-toolpath-preview.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
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
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            vertex_buffer: None,
            vertex_count: 0,
            signature: None,
        }
    }
}

impl ToolpathPipeline {
    fn prepare(&mut self, device: &wgpu::Device, vertices: &[Vertex], signature: u64) {
        self.vertex_count = vertices.len() as u32;
        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.signature = None;
            return;
        }
        if self.signature == Some(signature) {
            return;
        }

        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shockwave-gui.gpu-toolpath-preview.vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.signature = Some(signature);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x3,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy)]
struct ViewTransform {
    min_x: f32,
    min_y: f32,
    scale: f32,
}

impl ViewTransform {
    fn fit(points: &[Point]) -> Self {
        let (mut min_x, mut min_y) = (0.0, 0.0);
        let (mut max_x, mut max_y) = (1.0, 1.0);
        if let Some(first) = points.first() {
            min_x = first.x;
            max_x = first.x;
            min_y = first.y;
            max_y = first.y;
            for point in points.iter().skip(1) {
                min_x = min_x.min(point.x);
                max_x = max_x.max(point.x);
                min_y = min_y.min(point.y);
                max_y = max_y.max(point.y);
            }
        }

        let content_width = (max_x - min_x).max(1.0);
        let content_height = (max_y - min_y).max(1.0);
        Self {
            min_x,
            min_y,
            scale: 1.72 / content_width.max(content_height),
        }
    }

    fn apply(self, point: Point) -> [f32; 2] {
        [
            -0.86 + (point.x - self.min_x) * self.scale,
            0.86 - (point.y - self.min_y) * self.scale,
        ]
    }
}

fn push_bed_outline(
    vertices: &mut Vec<Vertex>,
    print_volume: Dimensions3,
    transform: ViewTransform,
) {
    let corners = bed_corners(print_volume);
    let color = [0.28, 0.36, 0.4];
    push_line(vertices, corners[0], corners[1], transform, color);
    push_line(vertices, corners[1], corners[2], transform, color);
    push_line(vertices, corners[2], corners[3], transform, color);
    push_line(vertices, corners[3], corners[0], transform, color);
}

fn push_toolpaths(vertices: &mut Vec<Vertex>, layers: &[LayerToolpaths], transform: ViewTransform) {
    for layer in layers {
        for path in &layer.paths {
            let color = role_color(path.role);
            for segment in path.points.windows(2) {
                push_line(
                    vertices,
                    segment[0].position,
                    segment[1].position,
                    transform,
                    color,
                );
            }
            if path.closed && path.points.len() > 2 {
                push_line(
                    vertices,
                    path.points.last().unwrap().position,
                    path.points[0].position,
                    transform,
                    color,
                );
            }
        }
    }
}

fn push_line(
    vertices: &mut Vec<Vertex>,
    start: Vec3,
    end: Vec3,
    transform: ViewTransform,
    color: [f32; 3],
) {
    vertices.push(Vertex {
        position: transform.apply(project(start)),
        color,
    });
    vertices.push(Vertex {
        position: transform.apply(project(end)),
        color,
    });
}

fn role_color(role: ToolpathRole) -> [f32; 3] {
    match role {
        ToolpathRole::Perimeter => [0.05, 1.0, 0.35],
        ToolpathRole::Infill => [1.0, 0.74, 0.18],
        ToolpathRole::Travel => [0.62, 0.64, 0.68],
    }
}

fn bed_corners(print_volume: Dimensions3) -> [Vec3; 4] {
    [
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        Vec3 {
            x: print_volume.x,
            y: 0.0,
            z: 0.0,
        },
        Vec3 {
            x: print_volume.x,
            y: print_volume.y,
            z: 0.0,
        },
        Vec3 {
            x: 0.0,
            y: print_volume.y,
            z: 0.0,
        },
    ]
}

fn project(point: Vec3) -> Point {
    Point::new(
        ((point.x - point.y) * 0.707) as f32,
        ((point.x + point.y) * 0.35 - point.z * 0.9) as f32,
    )
}

fn vertex_signature(vertices: &[Vertex]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    vertices.len().hash(&mut hasher);
    bytemuck::cast_slice::<Vertex, u8>(vertices).hash(&mut hasher);
    hasher.finish()
}

const TOOLPATH_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
"#;

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
        let preview = ToolpathPrimitive::from_scene(
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
