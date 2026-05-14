use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const APP_DIR_NAME: &str = "shockwave-slicer";
pub const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlicerSettings {
    pub slicing: SlicingSettings,
    pub printer: PrinterSettings,
    pub material: MaterialSettings,
    pub field: FieldSettings,
}

impl Default for SlicerSettings {
    fn default() -> Self {
        Self {
            slicing: SlicingSettings::default(),
            printer: PrinterSettings::default(),
            material: MaterialSettings::default(),
            field: FieldSettings::default(),
        }
    }
}

impl SlicerSettings {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        self.slicing.validate_into(&mut errors);
        self.printer.validate_into(&mut errors);
        self.material.validate_into(&mut errors);
        self.field.validate_into(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlicingSettings {
    pub layer_height_mm: f64,
    pub voxel_size_mm: Dimensions3,
    pub padding_voxels: usize,
    pub wall_count: usize,
    pub extrusion_width_mm: f64,
    pub infill_percentage: f64,
    pub global_z_offset_mm: f64,
}

impl Default for SlicingSettings {
    fn default() -> Self {
        Self {
            layer_height_mm: 0.25,
            voxel_size_mm: Dimensions3::uniform(0.4),
            padding_voxels: 3,
            wall_count: 6,
            extrusion_width_mm: 0.45,
            infill_percentage: 11.25,
            global_z_offset_mm: 0.0,
        }
    }
}

impl SlicingSettings {
    pub fn infill_line_spacing_mm(&self) -> Option<f64> {
        infill_line_spacing_mm(self.extrusion_width_mm, self.infill_percentage)
    }

    fn validate_into(&self, errors: &mut Vec<String>) {
        push_positive(errors, "slicing.layer_height_mm", self.layer_height_mm);
        self.voxel_size_mm
            .validate_positive_into(errors, "slicing.voxel_size_mm");
        if self.wall_count == 0 {
            errors.push("slicing.wall_count must be greater than zero".to_string());
        }
        push_positive(
            errors,
            "slicing.extrusion_width_mm",
            self.extrusion_width_mm,
        );
        push_finite(
            errors,
            "slicing.global_z_offset_mm",
            self.global_z_offset_mm,
        );
        if !(0.0..=100.0).contains(&self.infill_percentage) || !self.infill_percentage.is_finite() {
            errors.push("slicing.infill_percentage must be between 0 and 100".to_string());
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrinterSettings {
    pub print_volume_mm: Dimensions3,
    pub obstruction: PrinterObstructionSettings,
}

impl Default for PrinterSettings {
    fn default() -> Self {
        Self {
            print_volume_mm: Dimensions3 {
                x: 256.0,
                y: 256.0,
                z: 256.0,
            },
            obstruction: PrinterObstructionSettings::default(),
        }
    }
}

impl PrinterSettings {
    fn validate_into(&self, errors: &mut Vec<String>) {
        self.print_volume_mm
            .validate_positive_into(errors, "printer.print_volume_mm");
        self.obstruction.validate_into(errors);
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrinterObstructionSettings {
    pub printhead_clearance_height_mm: f64,
    pub printhead_clearance_angle_degrees: f64,
}

impl Default for PrinterObstructionSettings {
    fn default() -> Self {
        Self {
            printhead_clearance_height_mm: 5.0,
            printhead_clearance_angle_degrees: 55.0,
        }
    }
}

impl PrinterObstructionSettings {
    fn validate_into(&self, errors: &mut Vec<String>) {
        push_non_negative(
            errors,
            "printer.obstruction.printhead_clearance_height_mm",
            self.printhead_clearance_height_mm,
        );
        if !(0.0..90.0).contains(&self.printhead_clearance_angle_degrees)
            || !self.printhead_clearance_angle_degrees.is_finite()
        {
            errors.push(
                "printer.obstruction.printhead_clearance_angle_degrees must be in [0, 90)"
                    .to_string(),
            );
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaterialSettings {
    pub filament_diameter_mm: f64,
    pub nozzle_temperature_c: u16,
    pub bed_temperature_c: u16,
    pub fan_speed_percent: u8,
}

impl Default for MaterialSettings {
    fn default() -> Self {
        Self {
            filament_diameter_mm: 1.75,
            nozzle_temperature_c: 215,
            bed_temperature_c: 60,
            fan_speed_percent: 100,
        }
    }
}

impl MaterialSettings {
    fn validate_into(&self, errors: &mut Vec<String>) {
        push_positive(
            errors,
            "material.filament_diameter_mm",
            self.filament_diameter_mm,
        );
        if self.fan_speed_percent > 100 {
            errors.push("material.fan_speed_percent must be between 0 and 100".to_string());
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FieldSettings {
    pub method: FieldMethod,
    pub anisotropic_rate: Dimensions3,
}

impl Default for FieldSettings {
    fn default() -> Self {
        Self {
            method: FieldMethod::Trapezoid,
            anisotropic_rate: Dimensions3 {
                x: 3.7,
                y: 3.7,
                z: 1.0,
            },
        }
    }
}

impl FieldSettings {
    fn validate_into(&self, errors: &mut Vec<String>) {
        self.anisotropic_rate
            .validate_positive_into(errors, "field.anisotropic_rate");
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldMethod {
    Anisotropic,
    Trapezoid,
}

impl FieldMethod {
    pub fn name(self) -> &'static str {
        match self {
            Self::Anisotropic => "anisotropic",
            Self::Trapezoid => "trapezoid",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dimensions3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Dimensions3 {
    pub fn uniform(value: f64) -> Self {
        Self {
            x: value,
            y: value,
            z: value,
        }
    }

    fn validate_positive_into(self, errors: &mut Vec<String>, name: &str) {
        push_positive(errors, &format!("{name}.x"), self.x);
        push_positive(errors, &format!("{name}.y"), self.y);
        push_positive(errors, &format!("{name}.z"), self.z);
    }
}

pub fn infill_line_spacing_mm(extrusion_width_mm: f64, infill_percentage: f64) -> Option<f64> {
    if infill_percentage <= 0.0 {
        return None;
    }
    Some(extrusion_width_mm / (infill_percentage / 100.0))
}

pub fn settings_path() -> Result<PathBuf, String> {
    let config_dir = platform_config_dir()
        .ok_or_else(|| "could not determine platform config directory".to_string())?;
    Ok(settings_path_in_config_dir(config_dir))
}

pub fn settings_path_in_config_dir(config_dir: impl AsRef<Path>) -> PathBuf {
    config_dir
        .as_ref()
        .join(APP_DIR_NAME)
        .join(SETTINGS_FILE_NAME)
}

pub fn load_settings(path: impl AsRef<Path>) -> Result<SlicerSettings, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let settings: SlicerSettings = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    settings.validate().map_err(|errors| {
        format!(
            "invalid settings in {}: {}",
            path.display(),
            errors.join("; ")
        )
    })?;
    Ok(settings)
}

pub fn load_settings_or_default(path: impl AsRef<Path>) -> Result<SlicerSettings, String> {
    let path = path.as_ref();
    if path.exists() {
        load_settings(path)
    } else {
        Ok(SlicerSettings::default())
    }
}

pub fn save_settings(path: impl AsRef<Path>, settings: &SlicerSettings) -> Result<(), String> {
    settings
        .validate()
        .map_err(|errors| format!("invalid settings: {}", errors.join("; ")))?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("failed to serialize settings: {error}"))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn platform_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA").map(PathBuf::from)
    }

    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library").join("Application Support"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    {
        None
    }
}

fn push_finite(errors: &mut Vec<String>, name: &str, value: f64) {
    if !value.is_finite() {
        errors.push(format!("{name} must be finite"));
    }
}

fn push_positive(errors: &mut Vec<String>, name: &str, value: f64) {
    if value <= 0.0 || !value.is_finite() {
        errors.push(format!("{name} must be finite and greater than zero"));
    }
}

fn push_non_negative(errors: &mut Vec<String>, name: &str, value: f64) {
    if value < 0.0 || !value.is_finite() {
        errors.push(format!(
            "{name} must be finite and greater than or equal to zero"
        ));
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(settings.field.method, FieldMethod::Trapezoid);
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
}
