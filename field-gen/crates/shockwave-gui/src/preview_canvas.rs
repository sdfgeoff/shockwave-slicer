use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Element, Fill, Point, Rectangle, Renderer, Theme};

pub fn test_triangle_view<Message: 'static>() -> Element<'static, Message> {
    canvas(TestTriangle).width(Fill).height(220).into()
}

#[derive(Debug)]
struct TestTriangle;

impl<Message> canvas::Program<Message> for TestTriangle {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb(0.06, 0.08, 0.1),
        );

        let center = frame.center();
        let size = bounds.width.min(bounds.height) * 0.34;
        let triangle = canvas::Path::new(|path| {
            path.move_to(Point::new(center.x, center.y - size));
            path.line_to(Point::new(center.x - size * 0.9, center.y + size * 0.7));
            path.line_to(Point::new(center.x + size * 0.9, center.y + size * 0.7));
            path.close();
        });

        frame.fill(&triangle, Color::from_rgb(0.15, 0.75, 0.95));
        vec![frame.into_geometry()]
    }
}
