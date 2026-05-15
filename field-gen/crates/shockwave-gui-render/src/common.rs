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
    pub fn from_bounds(bounds: Bounds3) -> Self {
        let projected = bounds.projected_corners();
        let mut min_x = projected[0].0;
        let mut max_x = projected[0].0;
        let mut min_y = projected[0].1;
        let mut max_y = projected[0].1;
        for point in projected.iter().skip(1) {
            min_x = min_x.min(point.0);
            max_x = max_x.max(point.0);
            min_y = min_y.min(point.1);
            max_y = max_y.max(point.1);
        }

        let width = (max_x - min_x).max(1.0);
        let height = (max_y - min_y).max(1.0);
        let scale = 1.72 / width.max(height);
        let offset_x = -0.86 - min_x * scale;
        let offset_y = 0.86 + min_y * scale;
        let (depth_scale, depth_offset) = depth_transform(bounds);

        Self {
            matrix: [
                [0.707 * scale, -0.35 * scale, -depth_scale, 0.0],
                [-0.707 * scale, -0.35 * scale, -depth_scale, 0.0],
                [0.0, 0.9 * scale, -depth_scale, 0.0],
                [offset_x, offset_y, depth_offset, 1.0],
            ],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Bounds3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds3 {
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

    pub fn include(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn projected_corners(self) -> [(f32, f32); 8] {
        self.corners().map(project_iso)
    }

    fn corners(self) -> [Vec3; 8] {
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

fn project_iso(point: Vec3) -> (f32, f32) {
    (
        ((point.x - point.y) * 0.707) as f32,
        ((point.x + point.y) * 0.35 - point.z * 0.9) as f32,
    )
}

fn depth_transform(bounds: Bounds3) -> (f32, f32) {
    let corners = bounds.corners();
    let mut min_depth = camera_depth(corners[0]);
    let mut max_depth = min_depth;
    for corner in corners.iter().skip(1) {
        let depth = camera_depth(*corner);
        min_depth = min_depth.min(depth);
        max_depth = max_depth.max(depth);
    }
    let range = (max_depth - min_depth).max(1.0);
    let scale = 0.82 / range;
    let offset = 0.91 - min_depth * scale;
    (scale, offset)
}

fn camera_depth(point: Vec3) -> f32 {
    (point.x + point.y + point.z) as f32
}
