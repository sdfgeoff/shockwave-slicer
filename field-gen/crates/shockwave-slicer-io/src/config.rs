use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use shockwave_config::{Dimensions3, SlicerSettings};
use shockwave_slicer::{FieldPropagation, SliceSettings};
use shockwave_voxel::field::{ExplicitKernelPropagation, KernelMove, KernelPathCheck};

pub fn runtime_slice_settings(
    settings: &SlicerSettings,
    timing: &mut impl FnMut(&str, Duration),
) -> Result<SliceSettings, String> {
    let field_rate = vec3_from_dimensions(settings.field.anisotropic_rate);
    let propagation = if let Some(kernel_path) = &settings.field.kernel_path {
        let load_start = Instant::now();
        let propagation = load_kernel_propagation(kernel_path)?;
        timing("load kernel", load_start.elapsed());
        eprintln!("Loaded kernel with {} moves", propagation.move_count());
        FieldPropagation::ExplicitKernel(propagation)
    } else {
        FieldPropagation::from_method(settings.field.method, field_rate)
    };

    Ok(SliceSettings {
        voxel_size: vec3_from_dimensions(settings.field.voxel_size_mm),
        requested_size: Some(vec3_from_dimensions(settings.printer.print_volume_mm)),
        padding_voxels: settings.slicing.padding_voxels,
        origin: settings.slicing.origin_mm.map(vec3_from_dimensions),
        field_enabled: settings.field.enabled,
        propagation,
        field_rate,
        max_unreached_below_mm: settings.printer.obstruction.printhead_clearance_height_mm,
        unreached_cone_angle_degrees: settings
            .printer
            .obstruction
            .printhead_clearance_angle_degrees,
        iso_spacing: settings.slicing.layer_height_mm,
        wall_count: settings.slicing.wall_count,
        extrusion_width_mm: settings.slicing.extrusion_width_mm,
        filament_diameter_mm: settings.material.filament_diameter_mm,
        bed_temperature_c: settings.material.bed_temperature_c,
        nozzle_temperature_c: settings.material.nozzle_temperature_c,
        fan_speed_percent: settings.material.fan_speed_percent,
        global_z_offset_mm: settings.slicing.global_z_offset_mm,
        infill_spacing_mm: settings.slicing.infill_line_spacing_mm(),
    })
}

pub fn load_kernel_propagation(path: &Path) -> Result<ExplicitKernelPropagation, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read kernel {}: {error}", path.display()))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse kernel {}: {error}", path.display()))?;

    if json.get("type").and_then(Value::as_str) != Some("explicit") {
        return Err("kernel JSON must have \"type\": \"explicit\"".to_string());
    }

    let path_check = match json
        .get("path_check")
        .and_then(Value::as_str)
        .unwrap_or("swept_occupied")
    {
        "endpoint_occupied" => KernelPathCheck::EndpointOccupied,
        "swept_occupied" => KernelPathCheck::SweptOccupied,
        value => {
            return Err(format!(
                "kernel path_check must be endpoint_occupied or swept_occupied, got {value:?}"
            ));
        }
    };

    let moves = json
        .get("moves")
        .and_then(Value::as_array)
        .ok_or_else(|| "kernel JSON must contain a moves array".to_string())?
        .iter()
        .enumerate()
        .map(parse_kernel_move)
        .collect::<Result<Vec<_>, _>>()?;

    ExplicitKernelPropagation::new(moves, path_check)
}

fn parse_kernel_move((index, value): (usize, &Value)) -> Result<KernelMove, String> {
    let offset = value
        .get("offset")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kernel move {index} must contain an offset array"))?;
    if offset.len() != 3 {
        return Err(format!("kernel move {index} offset must have three values"));
    }

    let mut parsed_offset = [0isize; 3];
    for axis in 0..3 {
        let Some(value) = offset[axis].as_i64() else {
            return Err(format!(
                "kernel move {index} offset values must be integers"
            ));
        };
        parsed_offset[axis] = value
            .try_into()
            .map_err(|_| format!("kernel move {index} offset value is out of range"))?;
    }

    let cost = value
        .get("cost")
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("kernel move {index} must contain a numeric cost"))?;

    Ok(KernelMove {
        offset: parsed_offset,
        cost,
    })
}

fn vec3_from_dimensions(value: Dimensions3) -> shockwave_math::geometry::Vec3 {
    shockwave_math::geometry::Vec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use shockwave_config::{FieldMethod, SlicerSettings};
    use shockwave_slicer::FieldPropagation;

    use super::*;

    #[test]
    fn config_maps_to_runtime_slice_settings() {
        let mut settings = SlicerSettings::default();
        settings.field.method = FieldMethod::Anisotropic;
        settings.field.kernel_path = None;
        let mut timing = ignore_timing;

        let runtime = runtime_slice_settings(&settings, &mut timing).unwrap();

        assert!(matches!(
            runtime.propagation,
            FieldPropagation::Anisotropic(_)
        ));
        assert_eq!(runtime.voxel_size.x, settings.field.voxel_size_mm.x);
        assert_eq!(runtime.iso_spacing, settings.slicing.layer_height_mm);
    }

    #[test]
    fn loads_explicit_kernel_json() {
        let path = unique_temp_path("valid-kernel.json");
        fs::write(
            &path,
            r#"{
  "type": "explicit",
  "path_check": "endpoint_occupied",
  "moves": [
    { "offset": [1, 0, 0], "cost": 0.5 }
  ]
}"#,
        )
        .unwrap();

        let kernel = load_kernel_propagation(&path).unwrap();

        assert_eq!(kernel.move_count(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_invalid_kernel_type() {
        let path = write_kernel(
            "invalid-type.json",
            r#"{ "type": "generated", "moves": [] }"#,
        );

        let error = load_kernel_propagation(&path).unwrap_err();

        assert!(error.contains("type"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_invalid_kernel_path_check() {
        let path = write_kernel(
            "invalid-path-check.json",
            r#"{ "type": "explicit", "path_check": "bad", "moves": [] }"#,
        );

        let error = load_kernel_propagation(&path).unwrap_err();

        assert!(error.contains("path_check"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_missing_kernel_moves() {
        let path = write_kernel("missing-moves.json", r#"{ "type": "explicit" }"#);

        let error = load_kernel_propagation(&path).unwrap_err();

        assert!(error.contains("moves array"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_malformed_kernel_offset() {
        let path = write_kernel(
            "bad-offset.json",
            r#"{ "type": "explicit", "moves": [{ "offset": [1, 0], "cost": 1.0 }] }"#,
        );

        let error = load_kernel_propagation(&path).unwrap_err();

        assert!(error.contains("three values"));
        let _ = fs::remove_file(path);
    }

    fn write_kernel(name: &str, text: &str) -> PathBuf {
        let path = unique_temp_path(name);
        fs::write(&path, text).unwrap();
        path
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
    }

    fn ignore_timing(_: &str, _: Duration) {}
}
