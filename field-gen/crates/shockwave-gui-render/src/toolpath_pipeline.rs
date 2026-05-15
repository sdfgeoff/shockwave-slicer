use wgpu::util::DeviceExt;

use crate::common::{DEPTH_FORMAT, TransformUniform, Vertex3D};
use crate::scene::RenderLines;

#[derive(Debug)]
pub(crate) struct ToolpathPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: Option<wgpu::Buffer>,
    uniform_buffer: Option<wgpu::Buffer>,
    uniform_bind_group: Option<wgpu::BindGroup>,
    signature: Option<u64>,
}

impl ToolpathPipeline {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shockwave-gui-render.toolpath-preview.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/toolpath_preview.wgsl").into()),
        });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shockwave-gui-render.toolpath-preview.uniform-layout"),
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
            label: Some("shockwave-gui-render.toolpath-preview.pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shockwave-gui-render.toolpath-preview.pipeline"),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
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

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: crate::CameraTransform,
        lines: &RenderLines,
    ) {
        self.prepare_uniform(
            device,
            queue,
            TransformUniform::from_camera_object(camera, lines.transform),
        );
        if self.signature == Some(lines.signature) {
            return;
        }

        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shockwave-gui-render.toolpath-preview.vertices"),
                contents: bytemuck::cast_slice(&lines.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.signature = Some(lines.signature);
    }

    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>, vertex_count: u32) {
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
            label: Some("shockwave-gui-render.toolpath-preview.uniform"),
            contents: bytemuck::bytes_of(&transform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shockwave-gui-render.toolpath-preview.uniform-bind-group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        self.uniform_buffer = Some(buffer);
        self.uniform_bind_group = Some(bind_group);
    }
}
