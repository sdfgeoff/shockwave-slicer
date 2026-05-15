use std::sync::Arc;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget, mouse};
use iced::{Element, Length, Rectangle, Size, Theme};
use iced_wgpu::wgpu;
use shockwave_gui_render::{PREVIEW_HEIGHT, SceneRenderer, ScissorRect, ViewportSize};

pub use shockwave_gui_render::ScenePreviewGeometry;

pub fn scene_view<Message: 'static>(
    geometry: Arc<ScenePreviewGeometry>,
) -> Element<'static, Message> {
    Element::new(ScenePreview {
        geometry,
        width: Length::Fill,
        height: Length::Fixed(PREVIEW_HEIGHT),
    })
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
        _bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        let physical_size = viewport.physical_size();
        pipeline.renderer.prepare(
            device,
            queue,
            &self.geometry,
            ViewportSize {
                width: physical_size.width,
                height: physical_size.height,
            },
        );
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.renderer.render(
            encoder,
            target,
            &self.geometry,
            ScissorRect {
                x: clip_bounds.x,
                y: clip_bounds.y,
                width: clip_bounds.width,
                height: clip_bounds.height,
            },
        );
    }
}

#[derive(Debug)]
pub(crate) struct ScenePipeline {
    renderer: SceneRenderer,
}

impl iced_wgpu::primitive::Pipeline for ScenePipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            renderer: SceneRenderer::new(device, format),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_preview_geometry_defaults() {
        let geometry = ScenePreviewGeometry::default();
        assert!(format!("{geometry:?}").contains("ScenePreviewGeometry"));
    }

    #[test]
    fn scene_view_takes_shared_geometry() {
        let geometry = Arc::new(ScenePreviewGeometry::default());
        let _element = scene_view::<()>(geometry);
    }
}
