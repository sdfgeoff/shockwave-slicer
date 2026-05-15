use std::sync::Arc;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, mouse};
use iced::{Element, Length, Rectangle, Size, Theme};
use iced_wgpu::wgpu;

use crate::gpu_common::{DEPTH_FORMAT, PREVIEW_HEIGHT};
use crate::gpu_mesh_pipeline::MeshPipeline;
use crate::gpu_preview::ModelPreviewGeometry;
use crate::gpu_toolpath_preview::{ToolpathPreviewGeometry, ToolpathPipeline};
use shockwave_config::Dimensions3;
use shockwave_mesh::Mesh;

pub fn scene_view<Message: 'static>(
    geometry: Arc<ScenePreviewGeometry>,
) -> Element<'static, Message> {
    Element::new(ScenePreview {
        geometry,
        width: Length::Fill,
        height: Length::Fixed(PREVIEW_HEIGHT),
    })
}

/// Combined geometry for both mesh and toolpath rendering.
#[derive(Clone, Debug)]
pub struct ScenePreviewGeometry {
    pub mesh: ModelPreviewGeometry,
    pub toolpath: ToolpathPreviewGeometry,
}

impl ScenePreviewGeometry {
    pub fn from_scene(
        mesh: &Mesh,
        layers: &[shockwave_path::LayerToolpaths],
        print_volume: Dimensions3,
    ) -> Self {
        Self {
            mesh: ModelPreviewGeometry::from_scene(mesh, print_volume),
            toolpath: ToolpathPreviewGeometry::from_scene(layers, print_volume),
        }
    }
}

impl Default for ScenePreviewGeometry {
    fn default() -> Self {
        Self {
            mesh: ModelPreviewGeometry::default(),
            toolpath: ToolpathPreviewGeometry::default(),
        }
    }
}

#[derive(Debug)]
struct ScenePreview {
    geometry: Arc<ScenePreviewGeometry>,
    width: Length,
    height: Length,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for ScenePreview
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
            ScenePrimitive {
                geometry: Arc::clone(&self.geometry),
            },
        );
    }
}

impl<'a, Message, Renderer> From<ScenePreview> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(value: ScenePreview) -> Self {
        Element::new(value)
    }
}

#[derive(Debug)]
struct ScenePrimitive {
    geometry: Arc<ScenePreviewGeometry>,
}

impl iced_wgpu::Primitive for ScenePrimitive {
    type Pipeline = ScenePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        pipeline.prepare(
            device,
            queue,
            bounds,
            viewport,
            &self.geometry.mesh,
            &self.geometry.toolpath,
        );
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(
            encoder,
            target,
            clip_bounds,
            self.geometry.mesh.index_count(),
            self.geometry.toolpath.vertex_count(),
        );
    }
}

#[derive(Debug)]
pub(crate) struct ScenePipeline {
    mesh_pipeline: MeshPipeline,
    toolpath_pipeline: ToolpathPipeline,
    depth_texture: Option<wgpu::TextureView>,
    depth_size: Option<wgpu::Extent3d>,
}

impl iced_wgpu::primitive::Pipeline for ScenePipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            mesh_pipeline: MeshPipeline::new(device, queue, format),
            toolpath_pipeline: ToolpathPipeline::new(device, queue, format),
            depth_texture: None,
            depth_size: None,
        }
    }
}

impl ScenePipeline {
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
        mesh_geometry: &ModelPreviewGeometry,
        toolpath_geometry: &ToolpathPreviewGeometry,
    ) {
        self.prepare_depth_texture(device, viewport.physical_size());
        self.mesh_pipeline
            .prepare(device, queue, bounds, viewport, mesh_geometry);
        // Note: we don't call mesh_pipeline.prepare_depth_texture since we manage
        // the shared depth texture ourselves. Clear the one created by mesh_pipeline.
        self.mesh_pipeline.clear_depth_texture();
        self.toolpath_pipeline.prepare(device, queue, toolpath_geometry);
    }

    pub(crate) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
        mesh_index_count: u32,
        toolpath_vertex_count: u32,
    ) {
        let Some(depth_view) = self.depth_texture.as_ref() else {
            return;
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shockwave-gui.scene-preview.pass"),
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

        // Draw mesh (triangles with depth write)
        self.draw_mesh(&mut render_pass, mesh_index_count);

        // Draw toolpaths (lines on top, with depth test but no write)
        self.draw_toolpaths(&mut render_pass, toolpath_vertex_count);
    }

    fn draw_mesh(&self, render_pass: &mut wgpu::RenderPass, index_count: u32) {
        let (Some(vertex_buffer), Some(index_buffer), Some(bind_group)) = (
            self.mesh_pipeline.vertex_buffer(),
            self.mesh_pipeline.index_buffer(),
            self.mesh_pipeline.uniform_bind_group(),
        ) else {
            return;
        };
        if index_count == 0 {
            return;
        }

        render_pass.set_pipeline(self.mesh_pipeline.pipeline());
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..index_count, 0, 0..1);
    }

    fn draw_toolpaths(&self, render_pass: &mut wgpu::RenderPass, vertex_count: u32) {
        self.toolpath_pipeline.draw(render_pass, vertex_count);
    }

    fn prepare_depth_texture(&mut self, device: &wgpu::Device, size: iced::Size<u32>) {
        let extent = wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        };
        if self.depth_size == Some(extent) {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shockwave-gui.scene-preview.depth"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_preview_geometry_defaults() {
        let geometry = ScenePreviewGeometry::default();
        assert!(geometry.mesh.index_count() > 0); // has bed
        assert!(geometry.toolpath.vertex_count() > 0); // has bed outline
    }

    #[test]
    fn scene_view_takes_shared_geometry() {
        let geometry = Arc::new(ScenePreviewGeometry::default());
        let _element = scene_view::<()>(geometry);
    }
}
