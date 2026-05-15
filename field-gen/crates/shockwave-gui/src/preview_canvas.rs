use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Element, Fill, Point, Rectangle, Renderer, Theme};
use shockwave_config::Dimensions3;
use shockwave_math::geometry::{Triangle, Vec3};

pub fn model_view<'a, Message: 'a>(
    triangles: &'a [Triangle],
    print_volume: Dimensions3,
) -> Element<'a, Message> {
    canvas(ModelPreview {
        triangles,
        print_volume,
    })
    .width(Fill)
    .height(280)
    .into()
}

#[derive(Debug)]
struct ModelPreview<'a> {
    triangles: &'a [Triangle],
    print_volume: Dimensions3,
}

impl<Message> canvas::Program<Message> for ModelPreview<'_> {
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
            Color::from_rgb(0.05, 0.06, 0.07),
        );

        let bed = bed_corners(self.print_volume);
        let mut projected = bed.iter().map(|point| project(*point)).collect::<Vec<_>>();
        for triangle in self.triangles {
            projected.extend(triangle.vertices.iter().map(|point| project(*point)));
        }
        let transform = ViewTransform::fit(&projected, bounds);

        draw_bed(&mut frame, &bed, transform);
        draw_model(&mut frame, self.triangles, transform);

        vec![frame.into_geometry()]
    }
}

fn draw_bed(frame: &mut canvas::Frame, bed: &[Vec3; 4], transform: ViewTransform) {
    let bed_path = canvas::Path::new(|path| {
        let first = transform.apply(project(bed[0]));
        path.move_to(first);
        for corner in bed.iter().skip(1) {
            path.line_to(transform.apply(project(*corner)));
        }
        path.close();
    });
    frame.fill(&bed_path, Color::from_rgba(0.14, 0.18, 0.2, 0.8));
    frame.stroke(
        &bed_path,
        canvas::Stroke::default()
            .with_color(Color::from_rgb(0.45, 0.55, 0.6))
            .with_width(1.0),
    );
}

fn draw_model(frame: &mut canvas::Frame, triangles: &[Triangle], transform: ViewTransform) {
    let mut sorted = triangles.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| {
        triangle_depth(a)
            .partial_cmp(&triangle_depth(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for triangle in sorted {
        let normal = triangle_normal(triangle);
        let shade = (0.45 + normal.z.abs() * 0.35).clamp(0.25, 0.9) as f32;
        let path = canvas::Path::new(|path| {
            let first = transform.apply(project(triangle.vertices[0]));
            path.move_to(first);
            path.line_to(transform.apply(project(triangle.vertices[1])));
            path.line_to(transform.apply(project(triangle.vertices[2])));
            path.close();
        });
        frame.fill(&path, Color::from_rgb(0.1, shade, 0.95));
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(Color::from_rgba(0.01, 0.02, 0.04, 0.35))
                .with_width(0.5),
        );
    }
}

#[derive(Clone, Copy)]
struct ViewTransform {
    min_x: f32,
    min_y: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl ViewTransform {
    fn fit(points: &[Point], bounds: Rectangle) -> Self {
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
        let scale =
            (bounds.width * 0.82 / content_width).min(bounds.height * 0.82 / content_height);
        Self {
            min_x,
            min_y,
            scale,
            offset_x: bounds.width * 0.09,
            offset_y: bounds.height * 0.09,
        }
    }

    fn apply(self, point: Point) -> Point {
        Point::new(
            self.offset_x + (point.x - self.min_x) * self.scale,
            self.offset_y + (point.y - self.min_y) * self.scale,
        )
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

fn triangle_depth(triangle: &Triangle) -> f64 {
    triangle
        .vertices
        .iter()
        .map(|vertex| vertex.x + vertex.y + vertex.z)
        .sum::<f64>()
        / 3.0
}

fn triangle_normal(triangle: &Triangle) -> Vec3 {
    let a = triangle.vertices[0];
    let b = triangle.vertices[1];
    let c = triangle.vertices[2];
    let ux = b.x - a.x;
    let uy = b.y - a.y;
    let uz = b.z - a.z;
    let vx = c.x - a.x;
    let vy = c.y - a.y;
    let vz = c.z - a.z;
    Vec3 {
        x: uy * vz - uz * vy,
        y: uz * vx - ux * vz,
        z: ux * vy - uy * vx,
    }
}
