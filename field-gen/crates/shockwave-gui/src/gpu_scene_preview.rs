use std::sync::Arc;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Clipboard, Layout, Shell, Widget, mouse};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme};
use iced_wgpu::wgpu;
use shockwave_gui_render::{PREVIEW_HEIGHT, RenderScene, SceneRenderer, ScissorRect, ViewportSize};

pub use shockwave_gui_render::RenderScene as ScenePreviewGeometry;

#[derive(Clone, Copy, Debug)]
pub struct CameraDrag {
    pub delta_x: f32,
    pub delta_y: f32,
}

pub fn scene_view<Message: 'static>(
    scene: Arc<RenderScene>,
    on_camera_drag: impl Fn(CameraDrag) -> Message + 'static,
) -> Element<'static, Message> {
    Element::new(ScenePreview {
        scene,
        on_camera_drag: Box::new(on_camera_drag),
        width: Length::Fill,
        height: Length::Fixed(PREVIEW_HEIGHT),
    })
}

struct ScenePreview<Message> {
    scene: Arc<RenderScene>,
    on_camera_drag: Box<dyn Fn(CameraDrag) -> Message>,
    width: Length,
    height: Length,
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewState {
    dragging: bool,
    last_position: Option<Point>,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for ScenePreview<Message>
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

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<PreviewState>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(PreviewState::default())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<PreviewState>();
        match event {
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                state.last_position = cursor.position();
                shell.capture_event();
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    state.last_position = None;
                    shell.capture_event();
                }
            }
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) if state.dragging => {
                if let Some(previous) = state.last_position {
                    let drag = CameraDrag {
                        delta_x: position.x - previous.x,
                        delta_y: position.y - previous.y,
                    };
                    shell.publish((self.on_camera_drag)(drag));
                    shell.capture_event();
                }
                state.last_position = Some(*position);
            }
            _ => {}
        }
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
                scene: Arc::clone(&self.scene),
            },
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<PreviewState>();
        if state.dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message, Renderer> From<ScenePreview<Message>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer + iced_wgpu::primitive::Renderer + 'a,
{
    fn from(value: ScenePreview<Message>) -> Self {
        Element::new(value)
    }
}

#[derive(Debug)]
struct ScenePrimitive {
    scene: Arc<RenderScene>,
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
            &self.scene,
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
            &self.scene,
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
        let scene = RenderScene::default();
        assert!(format!("{scene:?}").contains("RenderScene"));
    }

    #[test]
    fn scene_view_takes_shared_geometry() {
        let scene = Arc::new(RenderScene::default());
        let _element = scene_view::<()>(scene, |_| ());
    }
}
