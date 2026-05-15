use crate::common::{DEPTH_FORMAT, ScissorRect, ViewportSize};
use crate::mesh_pipeline::MeshPipeline;
use crate::scene::RenderScene;
use crate::toolpath_pipeline::ToolpathPipeline;

#[derive(Debug)]
pub struct SceneRenderer {
    mesh_pipelines: Vec<MeshPipeline>,
    line_pipelines: Vec<ToolpathPipeline>,
    depth_texture: Option<wgpu::TextureView>,
    depth_size: Option<wgpu::Extent3d>,
    format: wgpu::TextureFormat,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            mesh_pipelines: vec![MeshPipeline::new(device, format)],
            line_pipelines: vec![ToolpathPipeline::new(device, format)],
            depth_texture: None,
            depth_size: None,
            format,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &RenderScene,
        viewport: ViewportSize,
    ) {
        self.prepare_depth_texture(device, viewport);
        resize_pipelines(
            &mut self.mesh_pipelines,
            scene.meshes.len(),
            device,
            self.format,
        );
        resize_pipelines(
            &mut self.line_pipelines,
            scene.lines.len(),
            device,
            self.format,
        );
        for (pipeline, mesh) in self.mesh_pipelines.iter_mut().zip(&scene.meshes) {
            pipeline.prepare(device, queue, scene.camera, mesh);
        }
        for (pipeline, lines) in self.line_pipelines.iter_mut().zip(&scene.lines) {
            pipeline.prepare(device, queue, scene.camera, lines);
        }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        scene: &RenderScene,
        scissor: ScissorRect,
    ) {
        let Some(depth_view) = self.depth_texture.as_ref() else {
            return;
        };
        if scissor.width == 0 || scissor.height == 0 {
            return;
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shockwave-gui-render.scene-preview.pass"),
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

        render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        for (pipeline, mesh) in self.mesh_pipelines.iter().zip(&scene.meshes) {
            pipeline.draw(&mut render_pass, mesh.index_count());
        }
        for (pipeline, lines) in self.line_pipelines.iter().zip(&scene.lines) {
            pipeline.draw(&mut render_pass, lines.vertex_count());
        }
    }

    fn prepare_depth_texture(&mut self, device: &wgpu::Device, size: ViewportSize) {
        let extent = wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        };
        if self.depth_size == Some(extent) {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shockwave-gui-render.scene-preview.depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_texture = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.depth_size = Some(extent);
    }
}

fn resize_pipelines<T>(
    pipelines: &mut Vec<T>,
    target_len: usize,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) where
    T: RenderPipelineFactory,
{
    while pipelines.len() < target_len {
        pipelines.push(T::new_pipeline(device, format));
    }
    pipelines.truncate(target_len);
}

trait RenderPipelineFactory {
    fn new_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self;
}

impl RenderPipelineFactory for MeshPipeline {
    fn new_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new(device, format)
    }
}

impl RenderPipelineFactory for ToolpathPipeline {
    fn new_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new(device, format)
    }
}
