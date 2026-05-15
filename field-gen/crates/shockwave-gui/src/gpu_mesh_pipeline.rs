use iced::Rectangle;
use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;

use crate::gpu_common::{DEPTH_FORMAT, TransformUniform, Vertex3D};
use crate::gpu_preview::ModelPreviewGeometry;

#[derive(Debug)]
pub(crate) struct MeshPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    uniform_buffer: Option<wgpu::Buffer>,
    uniform_bind_group: Option<wgpu::BindGroup>,
    depth_texture: Option<wgpu::TextureView>,
    depth_size: Option<wgpu::Extent3d>,
    signature: Option<u64>,
}

impl iced_wgpu::primitive::Pipeline for MeshPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shockwave-gui.gpu-mesh-preview.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mesh_preview.wgsl").into()),
        });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shockwave-gui.gpu-mesh-preview.uniform-layout"),
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
            label: Some("shockwave-gui.gpu-mesh-preview.pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shockwave-gui.gpu-mesh-preview.pipeline"),
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
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
            index_buffer: None,
            uniform_buffer: None,
            uniform_bind_group: None,
            depth_texture: None,
            depth_size: None,
            signature: None,
        }
    }
}

impl MeshPipeline {
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
        geometry: &ModelPreviewGeometry,
    ) {
        self.prepare_depth_texture(device, viewport.physical_size());
        self.prepare_uniform(device, queue, geometry.transform);
        if self.signature == Some(geometry.signature) {
            return;
        }

        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shockwave-gui.gpu-mesh-preview.vertices"),
                contents: bytemuck::cast_slice(&geometry.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.index_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shockwave-gui.gpu-mesh-preview.indices"),
                contents: bytemuck::cast_slice(&geometry.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        );
        self.signature = Some(geometry.signature);
    }

    pub(crate) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
        index_count: u32,
    ) {
        let (Some(vertex_buffer), Some(index_buffer), Some(bind_group), Some(depth_view)) = (
            self.vertex_buffer.as_ref(),
            self.index_buffer.as_ref(),
            self.uniform_bind_group.as_ref(),
            self.depth_texture.as_ref(),
        ) else {
            return;
        };
        if index_count == 0 {
            return;
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shockwave-gui.gpu-mesh-preview.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        render_pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..index_count, 0, 0..1);
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
            label: Some("shockwave-gui.gpu-mesh-preview.uniform"),
            contents: bytemuck::bytes_of(&transform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shockwave-gui.gpu-mesh-preview.uniform-bind-group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        self.uniform_buffer = Some(buffer);
        self.uniform_bind_group = Some(bind_group);
    }

    fn ensure_depth_texture(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        if self.depth_size == Some(size) {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shockwave-gui.gpu-mesh-preview.depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_texture = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.depth_size = Some(size);
    }

    fn prepare_depth_texture(&mut self, device: &wgpu::Device, size: iced::Size<u32>) {
        let extent = wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        };
        self.ensure_depth_texture(device, extent);
    }
}
