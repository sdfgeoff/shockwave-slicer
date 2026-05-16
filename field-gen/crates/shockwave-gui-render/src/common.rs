use std::hash::{Hash, Hasher};

use shockwave_config::Dimensions3;
use shockwave_math::geometry::Vec3;

pub const PREVIEW_HEIGHT: f32 = 280.0;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MIN_DEPTH_SPAN_MM: f32 = 1000.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderViewport {
    pub target_size: ViewportSize,
    pub rect: ViewportRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScissorRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraTransform {
    pub matrix: [[f32; 4]; 4],
}

impl CameraTransform {
    pub fn fit_isometric(bounds: SceneBounds) -> Self {
        Self::orbit(bounds, -std::f32::consts::FRAC_PI_4, 0.55, 1.0)
    }

    pub fn orbit(bounds: SceneBounds, yaw_radians: f32, pitch_radians: f32, zoom: f32) -> Self {
        let zoom = zoom.max(0.05);
        let pitch = pitch_radians.clamp(-1.45, 1.45);
        let rotation = mat4_mul(rotation_x(pitch), rotation_z(yaw_radians));
        let center = bounds.center();
        let view = mat4_mul(
            rotation,
            translation_matrix(-center.x as f32, -center.y as f32, -center.z as f32),
        );
        let transformed = bounds.corners().map(|point| transform_point(view, point));
        let (min_z, max_z) = range(transformed.map(|point| point.z));

        let scale = 0.86 * zoom / bounds.radius().max(1.0);
        let depth_span = (max_z - min_z).max(MIN_DEPTH_SPAN_MM);
        let depth_midpoint = (min_z + max_z) * 0.5;
        let depth_min = depth_midpoint - depth_span * 0.5;
        let depth_scale = -0.9 / depth_span;
        let depth_offset = 0.95 - depth_min * depth_scale;
        let fit = [
            [scale, 0.0, 0.0, 0.0],
            [0.0, -scale, 0.0, 0.0],
            [0.0, 0.0, depth_scale, 0.0],
            [0.0, 0.0, depth_offset, 1.0],
        ];

        Self {
            matrix: mat4_mul(fit, view),
        }
    }

    pub fn mapped_to_viewport(self, viewport: RenderViewport) -> Self {
        Self {
            matrix: mat4_mul(viewport_mapping_matrix(viewport), self.matrix),
        }
    }
}

impl Default for CameraTransform {
    fn default() -> Self {
        Self {
            matrix: identity_matrix(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectTransform {
    pub matrix: [[f32; 4]; 4],
}

impl ObjectTransform {
    pub fn identity() -> Self {
        Self {
            matrix: identity_matrix(),
        }
    }
}

impl Default for ObjectTransform {
    fn default() -> Self {
        Self::identity()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex3D {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex3D {
    pub fn new(position: Vec3, color: [f32; 3]) -> Self {
        Self {
            position: [position.x as f32, position.y as f32, position.z as f32],
            color,
        }
    }

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TransformUniform {
    pub matrix: [[f32; 4]; 4],
}

impl TransformUniform {
    pub fn from_camera_object(camera: CameraTransform, object: ObjectTransform) -> Self {
        Self {
            matrix: mat4_mul(camera.matrix, object.matrix),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SceneBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl SceneBounds {
    pub fn from_print_volume(print_volume: Dimensions3) -> Self {
        Self {
            min: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max: Vec3 {
                x: print_volume.x,
                y: print_volume.y,
                z: print_volume.z,
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            min: Vec3 {
                x: f64::INFINITY,
                y: f64::INFINITY,
                z: f64::INFINITY,
            },
            max: Vec3 {
                x: f64::NEG_INFINITY,
                y: f64::NEG_INFINITY,
                z: f64::NEG_INFINITY,
            },
        }
    }

    pub fn include(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub fn include_bounds(&mut self, other: Self) {
        self.include(other.min);
        self.include(other.max);
    }

    pub fn is_empty(self) -> bool {
        !self.min.x.is_finite()
    }

    pub fn center(self) -> Vec3 {
        Vec3 {
            x: (self.min.x + self.max.x) * 0.5,
            y: (self.min.y + self.max.y) * 0.5,
            z: (self.min.z + self.max.z) * 0.5,
        }
    }

    pub fn radius(self) -> f32 {
        let center = self.center();
        self.corners()
            .iter()
            .map(|point| {
                let dx = point.x - center.x;
                let dy = point.y - center.y;
                let dz = point.z - center.z;
                (dx * dx + dy * dy + dz * dz).sqrt() as f32
            })
            .fold(0.0, f32::max)
    }

    pub(crate) fn corners(self) -> [Vec3; 8] {
        [
            Vec3 {
                x: self.min.x,
                y: self.min.y,
                z: self.min.z,
            },
            Vec3 {
                x: self.max.x,
                y: self.min.y,
                z: self.min.z,
            },
            Vec3 {
                x: self.min.x,
                y: self.max.y,
                z: self.min.z,
            },
            Vec3 {
                x: self.max.x,
                y: self.max.y,
                z: self.min.z,
            },
            Vec3 {
                x: self.min.x,
                y: self.min.y,
                z: self.max.z,
            },
            Vec3 {
                x: self.max.x,
                y: self.min.y,
                z: self.max.z,
            },
            Vec3 {
                x: self.min.x,
                y: self.max.y,
                z: self.max.z,
            },
            Vec3 {
                x: self.max.x,
                y: self.max.y,
                z: self.max.z,
            },
        ]
    }
}

pub(crate) fn bed_corners(print_volume: Dimensions3) -> [Vec3; 4] {
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

pub(crate) fn data_signature(bytes: &[u8], count: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    count.hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn geometry_signature(vertices: &[Vertex3D], indices: &[u32]) -> u64 {
    let mut bytes =
        Vec::with_capacity(std::mem::size_of_val(vertices) + std::mem::size_of_val(indices));
    bytes.extend_from_slice(bytemuck::cast_slice(vertices));
    bytes.extend_from_slice(bytemuck::cast_slice(indices));
    data_signature(&bytes, vertices.len() + indices.len())
}

fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat4_mul(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            out[column][row] = left[0][row] * right[column][0]
                + left[1][row] * right[column][1]
                + left[2][row] * right[column][2]
                + left[3][row] * right[column][3];
        }
    }
    out
}

fn rotation_x(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, cos, sin, 0.0],
        [0.0, -sin, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn rotation_z(angle: f32) -> [[f32; 4]; 4] {
    let (sin, cos) = angle.sin_cos();
    [
        [cos, sin, 0.0, 0.0],
        [-sin, cos, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn translation_matrix(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

fn viewport_mapping_matrix(viewport: RenderViewport) -> [[f32; 4]; 4] {
    let target_width = viewport.target_size.width.max(1) as f32;
    let target_height = viewport.target_size.height.max(1) as f32;
    let rect = viewport.rect;
    let x_scale = rect.width / target_width;
    let y_scale = rect.height / target_height;
    let x_offset = (2.0 * rect.x + rect.width) / target_width - 1.0;
    let y_offset = 1.0 - (2.0 * rect.y + rect.height) / target_height;

    [
        [x_scale, 0.0, 0.0, 0.0],
        [0.0, y_scale, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x_offset, y_offset, 0.0, 1.0],
    ]
}

fn transform_point(matrix: [[f32; 4]; 4], point: Vec3) -> Vec3 {
    let x = point.x as f32;
    let y = point.y as f32;
    let z = point.z as f32;
    Vec3 {
        x: (matrix[0][0] * x + matrix[1][0] * y + matrix[2][0] * z + matrix[3][0]) as f64,
        y: (matrix[0][1] * x + matrix[1][1] * y + matrix[2][1] * z + matrix[3][1]) as f64,
        z: (matrix[0][2] * x + matrix[1][2] * y + matrix[2][2] * z + matrix[3][2]) as f64,
    }
}

fn range(values: [f64; 8]) -> (f32, f32) {
    let mut min = values[0] as f32;
    let mut max = min;
    for value in values.iter().skip(1) {
        min = min.min(*value as f32);
        max = max.max(*value as f32);
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_multiplication_matches_wgsl_column_major_layout() {
        let scale = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let translate = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 20.0, 30.0, 1.0],
        ];
        let transform = mat4_mul(translate, scale);
        let point = transform_point(
            transform,
            Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
        );

        assert_close(point.x, 12.0);
        assert_close(point.y, 24.0);
        assert_close(point.z, 36.0);
    }

    #[test]
    fn orbit_camera_keeps_bed_depth_inside_clip_space_with_large_depth_span() {
        let bounds = SceneBounds::from_print_volume(Dimensions3 {
            x: 300.0,
            y: 300.0,
            z: 300.0,
        });
        let camera = CameraTransform::orbit(bounds, 0.8, 0.6, 1.0);
        let transformed = bounds
            .corners()
            .map(|point| transform_point(camera.matrix, point));

        for point in transformed {
            assert!(
                (0.0..=1.0).contains(&(point.z as f32)),
                "depth {} was outside clip range",
                point.z
            );
        }
    }

    #[test]
    fn orbit_camera_keeps_constant_screen_scale_across_rotation() {
        let bounds = SceneBounds::from_print_volume(Dimensions3 {
            x: 300.0,
            y: 300.0,
            z: 300.0,
        });
        let camera_a = CameraTransform::orbit(bounds, 0.0, 0.4, 1.0);
        let camera_b = CameraTransform::orbit(bounds, 1.2, 0.4, 1.0);
        let expected_scale = 0.86 / bounds.radius() as f64;

        assert_close(screen_x_scale(camera_a), expected_scale);
        assert_close(screen_x_scale(camera_b), expected_scale);
    }

    #[test]
    fn viewport_mapping_places_local_clip_space_inside_widget_rect() {
        let viewport = RenderViewport {
            target_size: ViewportSize {
                width: 1000,
                height: 800,
            },
            rect: ViewportRect {
                x: 100.0,
                y: 200.0,
                width: 400.0,
                height: 300.0,
            },
        };
        let mapped = CameraTransform::default().mapped_to_viewport(viewport);
        let center = transform_point(
            mapped.matrix,
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        );
        let top_right = transform_point(
            mapped.matrix,
            Vec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            },
        );

        assert_close(center.x, -0.4);
        assert_close(center.y, 0.125);
        assert_close(top_right.x, 0.0);
        assert_close(top_right.y, 0.5);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn screen_x_scale(camera: CameraTransform) -> f64 {
        let x = camera.matrix[0][0] as f64;
        let y = camera.matrix[1][0] as f64;
        let z = camera.matrix[2][0] as f64;
        (x * x + y * y + z * z).sqrt()
    }
}
