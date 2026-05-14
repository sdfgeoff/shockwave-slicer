use shockwave_config::FieldMethod;
use shockwave_math::geometry::{Bounds, Vec3};
use shockwave_math::grid::Grid;
use shockwave_voxel::field::{
    AnisotropicEuclideanPropagation, ExplicitKernelPropagation, Field, KernelMove,
    PropagationConstraints, PropagationMethod, PropagationProgress, expand_field,
    propagate_field_with_progress,
};

use crate::error::SliceResult;
use crate::progress::{SlicePhase, SliceProgress};
use crate::settings::{FIELD_EXTENSION_VOXELS, SliceSettings};

#[derive(Clone, Debug)]
pub enum FieldPropagation {
    Anisotropic(Vec3),
    Trapezoid,
    ExplicitKernel(ExplicitKernelPropagation),
}

impl FieldPropagation {
    pub fn from_method(method: FieldMethod, field_rate: Vec3) -> Self {
        match method {
            FieldMethod::Anisotropic => Self::Anisotropic(field_rate),
            FieldMethod::Trapezoid => Self::Trapezoid,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrapezoidKernel {
    pub voxel_size: Vec3,
    pub r1: f64,
    pub r2: f64,
    pub half_height: f64,
    pub z_offset: f64,
    pub surface_cost: f64,
    pub max_cost: f64,
}

pub fn propagate_field(
    settings: &SliceSettings,
    occupancy: &[u8],
    grid: Grid,
    progress: &mut impl FnMut(SliceProgress),
) -> SliceResult<Field> {
    match &settings.propagation {
        FieldPropagation::Anisotropic(rate) => {
            let propagation = AnisotropicEuclideanPropagation::new(*rate);
            propagate_and_expand(settings, occupancy, grid, &propagation, progress)
        }
        FieldPropagation::Trapezoid => {
            let propagation = trapezoid_propagation(grid)?;
            propagate_and_expand(settings, occupancy, grid, &propagation, progress)
        }
        FieldPropagation::ExplicitKernel(propagation) => {
            propagate_and_expand(settings, occupancy, grid, propagation, progress)
        }
    }
}

pub fn align_field_to_model_floor(field: &mut Field, occupancy: &[u8], grid: Grid, bounds: Bounds) {
    let Some(lowest_occupied_z) = lowest_occupied_z(occupancy, grid) else {
        return;
    };
    let lowest_center_z = grid.origin.z + (lowest_occupied_z as f64 + 0.5) * grid.voxel_size.z;
    let field_offset = lowest_center_z - bounds.min.z;
    if field_offset <= 0.0 || !field_offset.is_finite() {
        return;
    }

    for distance in &mut field.distances {
        if distance.is_finite() {
            *distance += field_offset;
        }
    }
    field.max_distance += field_offset;
}

pub fn trapezoid_kernel_moves(kernel: TrapezoidKernel) -> SliceResult<Vec<KernelMove>> {
    let outer_margin = kernel.max_cost - kernel.surface_cost;
    if outer_margin < 0.0 {
        return Err(
            "trapezoid max cost must be greater than or equal to surface cost"
                .to_string()
                .into(),
        );
    }
    let vertical_cost_scale = trapezoid_vertical_cost_scale(kernel)?;

    let max_radius_mm = kernel.r1.max(kernel.r2) + outer_margin;
    let min_z_mm = -kernel.z_offset - kernel.half_height - outer_margin;
    let max_z_mm = -kernel.z_offset + kernel.half_height + outer_margin;
    let radius_x = (max_radius_mm / kernel.voxel_size.x).ceil() as isize;
    let radius_y = (max_radius_mm / kernel.voxel_size.y).ceil() as isize;
    let radius_z = (min_z_mm.abs().max(max_z_mm.abs()) / kernel.voxel_size.z).ceil() as isize;
    let mut moves = Vec::new();

    for dz in -radius_z..=radius_z {
        for dy in -radius_y..=radius_y {
            for dx in -radius_x..=radius_x {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }

                let radial_mm = ((dx as f64 * kernel.voxel_size.x).powi(2)
                    + (dy as f64 * kernel.voxel_size.y).powi(2))
                .sqrt();
                let z_mm = dz as f64 * kernel.voxel_size.z;
                let sdf = sd_trapezoid(
                    [radial_mm, z_mm + kernel.z_offset],
                    kernel.r1,
                    kernel.r2,
                    kernel.half_height,
                );
                let raw_cost = kernel.surface_cost + sdf;
                if raw_cost > 0.0 && raw_cost <= kernel.max_cost {
                    let mut cost = raw_cost / vertical_cost_scale;
                    if dz > 0 {
                        cost = cost.max(dz as f64 * kernel.voxel_size.z);
                    }
                    moves.push(KernelMove {
                        offset: [dx, dy, dz],
                        cost,
                    });
                }
            }
        }
    }

    Ok(moves)
}

pub fn sd_trapezoid(p: [f64; 2], r1: f64, r2: f64, half_height: f64) -> f64 {
    let k1 = [r2, half_height];
    let k2 = [r2 - r1, 2.0 * half_height];
    let px = p[0].abs();
    let py = p[1];

    let ca = [
        (px - if py < 0.0 { r1 } else { r2 }).max(0.0),
        py.abs() - half_height,
    ];
    let k1_minus_p = [k1[0] - px, k1[1] - py];
    let h = ((k1_minus_p[0] * k2[0] + k1_minus_p[1] * k2[1]) / dot2(k2)).clamp(0.0, 1.0);
    let cb = [px - k1[0] + k2[0] * h, py - k1[1] + k2[1] * h];
    let sign = if cb[0] < 0.0 && ca[1] < 0.0 {
        -1.0
    } else {
        1.0
    };
    sign * dot2(ca).min(dot2(cb)).sqrt()
}

fn propagate_and_expand(
    settings: &SliceSettings,
    occupancy: &[u8],
    grid: Grid,
    propagation: &impl PropagationMethod,
    progress: &mut impl FnMut(SliceProgress),
) -> SliceResult<Field> {
    let mut field = propagate_field_with_progress(
        occupancy,
        grid,
        propagation,
        PropagationConstraints {
            max_unreached_below_mm: Some(settings.max_unreached_below_mm),
            unreached_cone_angle_degrees: (settings.unreached_cone_angle_degrees > 0.0)
                .then_some(settings.unreached_cone_angle_degrees),
            unreached_cone_max_height_mm: Some(settings.max_unreached_below_mm),
        },
        &mut SlicerPropagationProgress { progress },
    )?;
    expand_field(&mut field, grid, FIELD_EXTENSION_VOXELS, propagation);
    Ok(field)
}

fn trapezoid_propagation(grid: Grid) -> SliceResult<ExplicitKernelPropagation> {
    let moves = trapezoid_kernel_moves(TrapezoidKernel {
        voxel_size: grid.voxel_size,
        r1: 2.0,
        r2: 0.2,
        half_height: 0.5,
        z_offset: 0.5,
        surface_cost: 1.0,
        max_cost: 2.0,
    })?;
    Ok(ExplicitKernelPropagation::new(
        moves,
        shockwave_voxel::field::KernelPathCheck::SweptOccupied,
    )?)
}

fn trapezoid_vertical_cost_scale(kernel: TrapezoidKernel) -> SliceResult<f64> {
    let one_voxel_up_cost = kernel.surface_cost
        + sd_trapezoid(
            [0.0, kernel.voxel_size.z + kernel.z_offset],
            kernel.r1,
            kernel.r2,
            kernel.half_height,
        );
    if one_voxel_up_cost <= 0.0 || !one_voxel_up_cost.is_finite() {
        return Err("trapezoid vertical cost scale must be finite and positive"
            .to_string()
            .into());
    }

    Ok(one_voxel_up_cost / kernel.voxel_size.z)
}

fn lowest_occupied_z(occupancy: &[u8], grid: Grid) -> Option<usize> {
    for z in 0..grid.dims[2] {
        for y in 0..grid.dims[1] {
            for x in 0..grid.dims[0] {
                if occupancy[grid.index(x, y, z)] != 0 {
                    return Some(z);
                }
            }
        }
    }
    None
}

fn dot2(v: [f64; 2]) -> f64 {
    v[0] * v[0] + v[1] * v[1]
}

struct SlicerPropagationProgress<'a, P> {
    progress: &'a mut P,
}

impl<P> PropagationProgress for SlicerPropagationProgress<'_, P>
where
    P: FnMut(SliceProgress),
{
    fn update(&mut self, reached: usize, total: usize) {
        let phase_progress = if total == 0 {
            1.0
        } else {
            reached as f32 / total as f32
        };
        (self.progress)(SliceProgress {
            phase: SlicePhase::PropagateField,
            phase_progress,
            message: format!("propagated {reached} / {total} voxels"),
        });
    }

    fn fallback_seed(&mut self, x: usize, y: usize, z: usize, distance: f64) {
        (self.progress)(SliceProgress {
            phase: SlicePhase::PropagateField,
            phase_progress: 0.0,
            message: format!("fallback seed at ({x}, {y}, {z}) distance {distance:.6}"),
        });
    }

    fn finish(&mut self, reached: usize, total: usize) {
        (self.progress)(SliceProgress {
            phase: SlicePhase::PropagateField,
            phase_progress: 1.0,
            message: format!("propagated {reached} / {total} voxels"),
        });
    }
}
