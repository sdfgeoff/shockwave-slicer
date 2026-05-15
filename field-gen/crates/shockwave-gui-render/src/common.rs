use std::hash::{Hash, Hasher};

use shockwave_config::Dimensions3;
use shockwave_math::geometry::Vec3;

pub const PREVIEW_HEIGHT: f32 = 280.0;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
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
        let rotated = bounds
            .corners()
            .map(|point| transform_point(rotation, point));
        let (min_x, max_x) = range(rotated.map(|point| point.x));
        let (min_y, max_y) = range(rotated.map(|point| point.y));
        let (min_z, max_z) = range(rotated.map(|point| point.z));

        let width = (max_x - min_x).max(1.0);
        let height = (max_y - min_y).max(1.0);
        let scale = 1.72 * zoom / width.max(height);
        let offset_x = -0.86 - min_x * scale;
        let offset_y = 0.86 + min_y * scale;
        let depth_range = (max_z - min_z).max(1.0);
        let depth_scale = -0.82 / depth_range;
        let depth_offset = 0.91 - min_z * depth_scale;
        let fit = [
            [scale, 0.0, 0.0, 0.0],
            [0.0, -scale, 0.0, 0.0],
            [0.0, 0.0, depth_scale, 0.0],
            [offset_x, offset_y, depth_offset, 1.0],
        ];

        Self {
            matrix: mat4_mul(fit, rotation),
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
    for row in 0..4 {
        for column in 0..4 {
            out[row][column] = left[row][0] * right[0][column]
                + left[row][1] * right[1][column]
                + left[row][2] * right[2][column]
                + left[row][3] * right[3][column];
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
