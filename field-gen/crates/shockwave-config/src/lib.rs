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
    pub output: OutputSettings,
}

impl Default for SlicerSettings {
    fn default() -> Self {
        Self {
            slicing: SlicingSettings::default(),
            printer: PrinterSettings::default(),
            material: MaterialSettings::default(),
            field: FieldSettings::default(),
            output: OutputSettings::default(),
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
        self.output.validate_into(&mut errors);

        if self.output.gcode && !self.field.enabled {
            errors.push("output.gcode requires field.enabled".to_string());
        }
        if self.output.export_ply && !self.field.enabled {
            errors.push("output.export_ply requires field.enabled".to_string());
        }

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
    pub origin_mm: Option<Dimensions3>,
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
            origin_mm: None,
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
        if let Some(origin) = self.origin_mm {
            origin.validate_finite_into(errors, "slicing.origin_mm");
        }
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
    pub enabled: bool,
    pub voxel_size_mm: Dimensions3,
    pub method: FieldMethod,
    pub anisotropic_rate: Dimensions3,
    pub kernel_path: Option<PathBuf>,
}

impl Default for FieldSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            voxel_size_mm: Dimensions3::uniform(0.4),
            method: FieldMethod::Trapezoid,
            anisotropic_rate: Dimensions3 {
                x: 3.7,
                y: 3.7,
                z: 1.0,
            },
            kernel_path: None,
        }
    }
}

impl FieldSettings {
    fn validate_into(&self, errors: &mut Vec<String>) {
        self.voxel_size_mm
            .validate_positive_into(errors, "field.voxel_size_mm");
        self.anisotropic_rate
            .validate_positive_into(errors, "field.anisotropic_rate");
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputSettings {
    pub export_ply: bool,
    pub gcode: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            export_ply: false,
            gcode: true,
        }
    }
}

impl OutputSettings {
    fn validate_into(&self, _errors: &mut Vec<String>) {}
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

    fn validate_finite_into(self, errors: &mut Vec<String>, name: &str) {
        push_finite(errors, &format!("{name}.x"), self.x);
        push_finite(errors, &format!("{name}.y"), self.y);
        push_finite(errors, &format!("{name}.z"), self.z);
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
mod tests;
