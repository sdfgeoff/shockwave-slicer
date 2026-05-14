use std::io::Write;

use shockwave_gcode::{MarlinConfig, write_marlin_gcode};
use shockwave_math::geometry::{Bounds, Vec3};
use shockwave_path::LayerToolpaths;

use crate::error::SliceResult;
use crate::settings::SliceSettings;

pub fn write_gcode(
    writer: &mut (impl Write + ?Sized),
    layers: &[LayerToolpaths],
    bounds: Bounds,
    settings: &SliceSettings,
) -> SliceResult<()> {
    let gcode = write_marlin_gcode(
        layers,
        MarlinConfig {
            filament_diameter_mm: settings.filament_diameter_mm,
            coordinate_offset: model_floor_coordinate_offset(bounds, settings.global_z_offset_mm),
            bed_temperature_c: settings.bed_temperature_c,
            nozzle_temperature_c: settings.nozzle_temperature_c,
            fan_speed_percent: settings.fan_speed_percent,
            ..Default::default()
        },
    )?;
    writer.write_all(gcode.as_bytes())?;
    Ok(())
}

pub fn model_floor_coordinate_offset(bounds: Bounds, global_z_offset_mm: f64) -> Vec3 {
    Vec3 {
        x: 0.0,
        y: 0.0,
        z: -bounds.min.z + global_z_offset_mm,
    }
}

#[cfg(test)]
mod tests {
    use shockwave_config::FieldMethod;
    use shockwave_math::geometry::Vec3;
    use shockwave_path::{LayerToolpaths, PathPoint, Toolpath, ToolpathRole};

    use super::*;
    use crate::field::FieldPropagation;

    #[test]
    fn global_z_offset_is_applied_only_when_writing_gcode() {
        let mut bytes = Vec::new();
        let settings = settings_with_global_z_offset(0.05);
        let layer = LayerToolpaths {
            field_value: 1.0,
            paths: vec![Toolpath {
                points: vec![PathPoint {
                    position: Vec3 {
                        x: 2.0,
                        y: 3.0,
                        z: 0.25,
                    },
                    extrusion_width_mm: 0.4,
                    layer_height_mm: 0.2,
                }],
                role: ToolpathRole::Travel,
                closed: false,
            }],
        };

        write_gcode(
            &mut bytes,
            &[layer],
            Bounds {
                min: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                max: Vec3 {
                    x: 4.0,
                    y: 4.0,
                    z: 4.0,
                },
            },
            &settings,
        )
        .unwrap();

        let gcode = String::from_utf8(bytes).unwrap();
        assert!(gcode.contains("G0 X2.00000 Y3.00000 Z0.30000"));
    }

    fn settings_with_global_z_offset(global_z_offset_mm: f64) -> SliceSettings {
        let field_rate = Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        };
        SliceSettings {
            voxel_size: field_rate,
            requested_size: None,
            padding_voxels: 3,
            origin: None,
            field_enabled: true,
            propagation: FieldPropagation::from_method(FieldMethod::Anisotropic, field_rate),
            field_rate,
            max_unreached_below_mm: 5.0,
            unreached_cone_angle_degrees: 0.0,
            iso_spacing: 1.0,
            wall_count: 2,
            extrusion_width_mm: 0.4,
            filament_diameter_mm: 1.75,
            bed_temperature_c: 60,
            nozzle_temperature_c: 215,
            fan_speed_percent: 100,
            global_z_offset_mm,
            infill_spacing_mm: Some(2.0),
        }
    }
}
