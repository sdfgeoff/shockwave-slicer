use crate::common::{DEPTH_FORMAT, ScissorRect, ViewportSize};
use crate::geometry::ScenePreviewGeometry;
use crate::mesh_pipeline::MeshPipeline;
use crate::toolpath_pipeline::ToolpathPipeline;

#[derive(Debug)]
pub struct SceneRenderer {
    mesh_pipeline: MeshPipeline,
    toolpath_pipeline: ToolpathPipeline,
    depth_texture: Option<wgpu::TextureView>,
    depth_size: Option<wgpu::Extent3d>,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            mesh_pipeline: MeshPipeline::new(device, format),
            toolpath_pipeline: ToolpathPipeline::new(device, format),
            depth_texture: None,
            depth_size: None,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry: &ScenePreviewGeometry,
        viewport: ViewportSize,
    ) {
        self.prepare_depth_texture(device, viewport);
        self.mesh_pipeline.prepare(device, queue, &geometry.mesh);
        self.toolpath_pipeline
            .prepare(device, queue, &geometry.toolpath);
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        geometry: &ScenePreviewGeometry,
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
        self.mesh_pipeline
            .draw(&mut render_pass, geometry.mesh.index_count());
        self.toolpath_pipeline
            .draw(&mut render_pass, geometry.toolpath.vertex_count());
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
