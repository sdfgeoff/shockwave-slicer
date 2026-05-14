use shockwave_config::FieldMethod;
use shockwave_math::geometry::Vec3;

use crate::field::FieldPropagation;

pub const FIELD_EXTENSION_VOXELS: usize = 2;

#[derive(Clone, Debug)]
pub struct SliceSettings {
    pub voxel_size: Vec3,
    pub requested_size: Option<Vec3>,
    pub padding_voxels: usize,
    pub origin: Option<Vec3>,
    pub field_enabled: bool,
    pub propagation: FieldPropagation,
    pub field_rate: Vec3,
    pub max_unreached_below_mm: f64,
    pub unreached_cone_angle_degrees: f64,
    pub iso_spacing: f64,
    pub wall_count: usize,
    pub extrusion_width_mm: f64,
    pub filament_diameter_mm: f64,
    pub infill_spacing_mm: Option<f64>,
}

impl SliceSettings {
    pub fn field_method_name(&self) -> &'static str {
        match &self.propagation {
            FieldPropagation::Anisotropic(_) => FieldMethod::Anisotropic.name(),
            FieldPropagation::Trapezoid => FieldMethod::Trapezoid.name(),
            FieldPropagation::ExplicitKernel(_) => "explicit-kernel",
        }
    }
}
