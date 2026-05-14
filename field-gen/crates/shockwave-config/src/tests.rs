use std::path::PathBuf;

use super::*;

#[test]
fn defaults_match_current_slicer_behavior() {
    let settings = SlicerSettings::default();

    assert_eq!(settings.slicing.layer_height_mm, 0.25);
    assert_eq!(settings.slicing.voxel_size_mm, Dimensions3::uniform(0.4));
    assert_eq!(settings.slicing.wall_count, 6);
    assert_eq!(settings.slicing.extrusion_width_mm, 0.45);
    assert_eq!(settings.slicing.infill_line_spacing_mm(), Some(4.0));
    assert_eq!(settings.printer.print_volume_mm.x, 256.0);
    assert_eq!(
        settings.printer.obstruction.printhead_clearance_height_mm,
        5.0
    );
    assert_eq!(
        settings
            .printer
            .obstruction
            .printhead_clearance_angle_degrees,
        55.0
    );
    assert_eq!(settings.material.nozzle_temperature_c, 215);
    assert_eq!(settings.material.bed_temperature_c, 60);
    assert!(settings.field.enabled);
    assert_eq!(settings.field.method, FieldMethod::Trapezoid);
    assert_eq!(settings.field.kernel_path, None);
    assert!(!settings.output.export_ply);
    assert!(settings.output.gcode);
}

#[test]
fn settings_roundtrip_through_json_file() {
    let path = unique_temp_path("settings-roundtrip").join("settings.json");
    let mut settings = SlicerSettings::default();
    settings.slicing.layer_height_mm = 0.3;
    settings.material.fan_speed_percent = 42;

    save_settings(&path, &settings).unwrap();
    let loaded = load_settings(&path).unwrap();

    assert_eq!(loaded, settings);
}

#[test]
fn missing_settings_loads_as_default() {
    let path = unique_temp_path("missing-settings").join("settings.json");

    let loaded = load_settings_or_default(path).unwrap();

    assert_eq!(loaded, SlicerSettings::default());
}

#[test]
fn validates_user_facing_settings() {
    let mut settings = SlicerSettings::default();
    settings.slicing.layer_height_mm = 0.0;
    settings.slicing.infill_percentage = 150.0;
    settings.printer.print_volume_mm.z = f64::NAN;
    settings.field.enabled = false;
    settings
        .printer
        .obstruction
        .printhead_clearance_angle_degrees = 90.0;

    let errors = settings.validate().unwrap_err();

    assert!(errors.iter().any(|error| error.contains("layer_height")));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("infill_percentage"))
    );
    assert!(errors.iter().any(|error| error.contains("print_volume")));
    assert!(errors.iter().any(|error| error.contains("clearance_angle")));
    assert!(errors.iter().any(|error| error.contains("output.gcode")));
}

#[test]
fn maps_infill_percentage_to_line_spacing() {
    assert_eq!(infill_line_spacing_mm(0.45, 0.0), None);
    assert_eq!(infill_line_spacing_mm(0.45, 100.0), Some(0.45));
    assert_eq!(infill_line_spacing_mm(0.45, 11.25), Some(4.0));
}

#[test]
fn settings_path_uses_app_subdirectory() {
    let path = settings_path_in_config_dir("/tmp/config-root");

    assert_eq!(
        path,
        PathBuf::from("/tmp/config-root")
            .join(APP_DIR_NAME)
            .join(SETTINGS_FILE_NAME)
    );
}

fn unique_temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
}
