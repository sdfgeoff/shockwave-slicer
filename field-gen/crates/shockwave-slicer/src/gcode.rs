use std::io::Write;

use shockwave_gcode::{MarlinConfig, write_marlin_gcode};
use shockwave_math::geometry::{Bounds, Vec3};
use shockwave_path::LayerToolpaths;

use crate::error::SliceResult;
use crate::settings::SliceSettings;

pub fn write_gcode(
    writer: &mut impl Write,
    layers: &[LayerToolpaths],
    bounds: Bounds,
    settings: &SliceSettings,
) -> SliceResult<()> {
    let gcode = write_marlin_gcode(
        layers,
        MarlinConfig {
            filament_diameter_mm: settings.filament_diameter_mm,
            coordinate_offset: model_floor_coordinate_offset(bounds),
            ..Default::default()
        },
    )?;
    writer.write_all(gcode.as_bytes())?;
    Ok(())
}

pub fn model_floor_coordinate_offset(bounds: Bounds) -> Vec3 {
    Vec3 {
        x: 0.0,
        y: 0.0,
        z: -bounds.min.z,
    }
}
