use std::path::PathBuf;

pub use shockwave_config::FieldMethod;
use shockwave_config::{Dimensions3, SlicerSettings, infill_line_spacing_mm, load_settings};
use shockwave_math::geometry::Vec3;

#[derive(Clone, Debug)]
pub struct Config {
    pub input: PathBuf,
    pub output_prefix: PathBuf,
    pub voxel_size: Vec3,
    pub requested_size: Option<Vec3>,
    pub padding_voxels: usize,
    pub origin: Option<Vec3>,
    pub field_enabled: bool,
    pub field_method: FieldMethod,
    pub field_rate: Vec3,
    pub kernel_path: Option<PathBuf>,
    pub max_unreached_below_mm: f64,
    pub unreached_cone_angle_degrees: f64,
    pub iso_spacing: f64,
    pub export_ply: bool,
    pub gcode_enabled: bool,
    pub wall_count: usize,
    pub extrusion_width_mm: f64,
    pub filament_diameter_mm: f64,
    pub infill_spacing_mm: Option<f64>,
}

pub fn parse_args(args: Vec<String>) -> Result<Config, String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Err(usage());
    }

    let input = PathBuf::from(&args[0]);
    let settings_path = find_settings_path(&args)?;
    let settings = match &settings_path {
        Some(path) => load_settings(path)?,
        None => SlicerSettings::default(),
    };
    let mut output_prefix = PathBuf::from("occupancy");
    let mut voxel_size = vec3_from_dimensions(settings.slicing.voxel_size_mm);
    let mut requested_size = Some(vec3_from_dimensions(settings.printer.print_volume_mm));
    let mut padding_voxels = settings.slicing.padding_voxels;
    let mut origin = None;
    let mut field_enabled = false;
    let mut field_method = settings.field.method;
    let mut field_rate = vec3_from_dimensions(settings.field.anisotropic_rate);
    let mut kernel_path = None;
    let mut max_unreached_below_mm = settings.printer.obstruction.printhead_clearance_height_mm;
    let mut unreached_cone_angle_degrees = settings
        .printer
        .obstruction
        .printhead_clearance_angle_degrees;
    let mut iso_spacing = settings.slicing.layer_height_mm;
    let mut export_ply = false;
    let mut gcode_enabled = false;
    let mut wall_count = settings.slicing.wall_count;
    let mut extrusion_width_mm = settings.slicing.extrusion_width_mm;
    let mut filament_diameter_mm = settings.material.filament_diameter_mm;
    let mut infill_spacing_mm = settings.slicing.infill_line_spacing_mm();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                require_flag_value("--config", &args, index)?;
            }
            "--voxel" => {
                voxel_size = parse_vec3_flag("--voxel", &args, &mut index)?;
            }
            "--size" => {
                requested_size = Some(parse_vec3_flag("--size", &args, &mut index)?);
            }
            "--padding-voxels" => {
                index += 1;
                padding_voxels = args
                    .get(index)
                    .ok_or_else(|| "--padding-voxels requires a non-negative integer".to_string())?
                    .parse()
                    .map_err(|_| "--padding-voxels must be a non-negative integer".to_string())?;
            }
            "--origin" => {
                origin = Some(parse_vec3_flag("--origin", &args, &mut index)?);
            }
            "--field" => {
                field_enabled = true;
            }
            "--field-rate" => {
                field_rate = parse_vec3_flag("--field-rate", &args, &mut index)?;
            }
            "--field-method" => {
                index += 1;
                field_method = parse_field_method(&args, index)?;
                field_enabled = true;
            }
            "--kernel" => {
                index += 1;
                let path = args
                    .get(index)
                    .map(PathBuf::from)
                    .ok_or_else(|| "--kernel requires a JSON kernel path".to_string())?;
                kernel_path = Some(path);
                field_enabled = true;
            }
            "--max-unreached-below" => {
                index += 1;
                max_unreached_below_mm =
                    parse_non_negative_number("--max-unreached-below", &args, index)?;
            }
            "--unreached-cone-angle" => {
                index += 1;
                unreached_cone_angle_degrees =
                    parse_angle_degrees("--unreached-cone-angle", &args, index)?;
            }
            "--iso-spacing" => {
                index += 1;
                iso_spacing = args
                    .get(index)
                    .ok_or_else(|| "--iso-spacing requires a positive numeric value".to_string())?
                    .parse()
                    .map_err(|_| "--iso-spacing must be numeric".to_string())?;
            }
            "--gcode" => {
                gcode_enabled = true;
                field_enabled = true;
            }
            "--export-ply" => {
                export_ply = true;
                field_enabled = true;
            }
            "--wall-count" => {
                index += 1;
                wall_count = args
                    .get(index)
                    .ok_or_else(|| "--wall-count requires a non-negative integer".to_string())?
                    .parse()
                    .map_err(|_| "--wall-count must be a non-negative integer".to_string())?;
            }
            "--extrusion-width" => {
                index += 1;
                extrusion_width_mm = parse_positive_number("--extrusion-width", &args, index)?;
            }
            "--filament-diameter" => {
                index += 1;
                filament_diameter_mm = parse_positive_number("--filament-diameter", &args, index)?;
            }
            "--infill-spacing" => {
                index += 1;
                let spacing = parse_non_negative_number("--infill-spacing", &args, index)?;
                infill_spacing_mm = (spacing > 0.0).then_some(spacing);
            }
            "--infill-percentage" => {
                index += 1;
                let percentage = parse_percentage("--infill-percentage", &args, index)?;
                infill_spacing_mm = infill_line_spacing_mm(extrusion_width_mm, percentage);
            }
            "--output" | "-o" => {
                index += 1;
                output_prefix = args
                    .get(index)
                    .map(PathBuf::from)
                    .ok_or_else(|| "--output requires a path".to_string())?;
            }
            flag => {
                return Err(format!("unknown argument `{flag}`\n\n{}", usage()));
            }
        }
        index += 1;
    }

    validate_positive_vec3("--voxel", voxel_size)?;
    if let Some(size) = requested_size {
        validate_positive_vec3("--size", size)?;
    }
    validate_positive_vec3("--field-rate", field_rate)?;
    if iso_spacing <= 0.0 || !iso_spacing.is_finite() {
        return Err("--iso-spacing must be greater than zero".to_string());
    }
    if gcode_enabled && wall_count == 0 {
        return Err("--wall-count must be greater than zero when --gcode is enabled".to_string());
    }

    Ok(Config {
        input,
        output_prefix,
        voxel_size,
        requested_size,
        padding_voxels,
        origin,
        field_enabled,
        field_method,
        field_rate,
        kernel_path,
        max_unreached_below_mm,
        unreached_cone_angle_degrees,
        iso_spacing,
        export_ply,
        gcode_enabled,
        wall_count,
        extrusion_width_mm,
        filament_diameter_mm,
        infill_spacing_mm,
    })
}

fn find_settings_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut index = 1;
    let mut settings_path = None;
    while index < args.len() {
        if args[index] == "--config" {
            index += 1;
            let path = args
                .get(index)
                .map(PathBuf::from)
                .ok_or_else(|| "--config requires a settings JSON path".to_string())?;
            settings_path = Some(path);
        }
        index += 1;
    }
    Ok(settings_path)
}

fn vec3_from_dimensions(value: Dimensions3) -> Vec3 {
    Vec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn parse_field_method(args: &[String], index: usize) -> Result<FieldMethod, String> {
    match args
        .get(index)
        .ok_or_else(|| "--field-method requires a method name".to_string())?
        .as_str()
    {
        "anisotropic" => Ok(FieldMethod::Anisotropic),
        "trapezoid" => Ok(FieldMethod::Trapezoid),
        value => Err(format!(
            "--field-method must be `anisotropic` or `trapezoid`, got `{value}`"
        )),
    }
}

fn require_flag_value<'a>(flag: &str, args: &'a [String], index: usize) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_vec3_flag(flag: &str, args: &[String], index: &mut usize) -> Result<Vec3, String> {
    let start = *index + 1;
    let values = args
        .get(start..start + 3)
        .ok_or_else(|| format!("{flag} requires three numeric values"))?;
    *index += 3;

    Ok(Vec3 {
        x: values[0]
            .parse()
            .map_err(|_| format!("{flag} x value must be numeric"))?,
        y: values[1]
            .parse()
            .map_err(|_| format!("{flag} y value must be numeric"))?,
        z: values[2]
            .parse()
            .map_err(|_| format!("{flag} z value must be numeric"))?,
    })
}

fn validate_positive_vec3(name: &str, value: Vec3) -> Result<(), String> {
    if value.x <= 0.0 || value.y <= 0.0 || value.z <= 0.0 {
        return Err(format!("{name} values must be greater than zero"));
    }
    Ok(())
}

fn parse_non_negative_number(flag: &str, args: &[String], index: usize) -> Result<f64, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("{flag} requires a non-negative numeric value"))?
        .parse()
        .map_err(|_| format!("{flag} must be numeric"))?;
    if value < 0.0 || !f64::is_finite(value) {
        return Err(format!(
            "{flag} must be a finite number greater than or equal to zero"
        ));
    }
    Ok(value)
}

fn parse_positive_number(flag: &str, args: &[String], index: usize) -> Result<f64, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("{flag} requires a positive numeric value"))?
        .parse()
        .map_err(|_| format!("{flag} must be numeric"))?;
    if value <= 0.0 || !f64::is_finite(value) {
        return Err(format!("{flag} must be a finite number greater than zero"));
    }
    Ok(value)
}

fn parse_angle_degrees(flag: &str, args: &[String], index: usize) -> Result<f64, String> {
    let value = args
        .get(index)
        .ok_or_else(|| format!("{flag} requires an angle in degrees"))?
        .parse()
        .map_err(|_| format!("{flag} must be numeric"))?;
    if !(0.0..90.0).contains(&value) || !f64::is_finite(value) {
        return Err(format!("{flag} must be finite and in the range [0, 90)"));
    }
    Ok(value)
}

fn parse_percentage(flag: &str, args: &[String], index: usize) -> Result<f64, String> {
    let value = parse_non_negative_number(flag, args, index)?;
    if value > 100.0 {
        return Err(format!("{flag} must be less than or equal to 100"));
    }
    Ok(value)
}

fn usage() -> String {
    "usage: field-gen <input.stl> [--config <settings.json>] [--voxel <x-mm> <y-mm> <z-mm>] [--size <x-mm> <y-mm> <z-mm>] [--padding-voxels <n>] [--origin <x-mm> <y-mm> <z-mm>] [--field] [--field-method <anisotropic|trapezoid>] [--field-rate <x> <y> <z>] [--kernel <kernel.json>] [--max-unreached-below <mm>] [--unreached-cone-angle <degrees>] [--iso-spacing <distance>] [--export-ply] [--gcode] [--wall-count <n>] [--extrusion-width <mm>] [--filament-diameter <mm>] [--infill-spacing <mm>] [--infill-percentage <percent>] [--output <prefix>]\n\
\n\
STL coordinates are assumed to be millimeters. Defaults come from SlicerSettings or --config when provided.\n\
If --size is provided, it is treated as a maximum grid size; otherwise printer.print_volume_mm is used.\n\
--field propagates an anisotropic field through occupied voxels from the lowest occupied Z slice.\n\
--field-method trapezoid generates a native radial trapezoid SDF kernel from the active voxel size.\n\
--kernel propagates the field using an explicit JSON kernel instead of --field-rate.\n\
--max-unreached-below prevents reaching high voxels while lower occupied voxels remain unreached.\n\
--unreached-cone-angle reserves access cones above unreached occupied voxels. Use 0 to disable this constraint.\n\
--iso-spacing controls the spacing between exported isosurface levels when --field is enabled.\n\
--export-ply writes unclipped and clipped isosurface PLY files; PLY output is disabled by default.\n\
--gcode writes experimental Marlin G-code from clipped isosurfaces and implies --field.\n\
Use --infill-spacing 0 or --infill-percentage 0 to disable infill.\n\
Voxel size takes priority: grid dimensions are ceil(size / voxel), so actual size may expand slightly."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shockwave_config::{save_settings, settings_path_in_config_dir};

    #[test]
    fn defaults_come_from_slicer_settings() {
        let config = parse_args(vec!["part.stl".to_string()]).unwrap();

        assert_eq!(config.voxel_size.x, 0.4);
        assert_eq!(config.requested_size.unwrap().x, 256.0);
        assert_eq!(config.padding_voxels, 3);
        assert_eq!(config.field_method, FieldMethod::Trapezoid);
        assert_eq!(config.iso_spacing, 0.25);
        assert_eq!(config.wall_count, 6);
        assert_eq!(config.extrusion_width_mm, 0.45);
        assert_eq!(config.infill_spacing_mm, Some(4.0));
    }

    #[test]
    fn loads_settings_json_and_allows_cli_overrides() {
        let settings_path = settings_path_in_config_dir(unique_temp_path("cli-config"));
        let mut settings = SlicerSettings::default();
        settings.slicing.layer_height_mm = 0.3;
        settings.slicing.voxel_size_mm = Dimensions3::uniform(0.8);
        settings.slicing.infill_percentage = 50.0;
        settings.printer.print_volume_mm = Dimensions3 {
            x: 180.0,
            y: 181.0,
            z: 182.0,
        };
        save_settings(&settings_path, &settings).unwrap();

        let config = parse_args(vec![
            "part.stl".to_string(),
            "--config".to_string(),
            settings_path.display().to_string(),
            "--iso-spacing".to_string(),
            "0.2".to_string(),
            "--voxel".to_string(),
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
        ])
        .unwrap();

        assert_eq!(config.iso_spacing, 0.2);
        assert_eq!(config.voxel_size.x, 1.0);
        assert_eq!(config.voxel_size.y, 2.0);
        assert_eq!(config.voxel_size.z, 3.0);
        assert_eq!(config.requested_size.unwrap().x, 180.0);
        assert_eq!(config.infill_spacing_mm, Some(0.9));
    }

    #[test]
    fn infill_percentage_flag_overrides_spacing_from_settings() {
        let config = parse_args(vec![
            "part.stl".to_string(),
            "--extrusion-width".to_string(),
            "0.5".to_string(),
            "--infill-percentage".to_string(),
            "25".to_string(),
        ])
        .unwrap();

        assert_eq!(config.infill_spacing_mm, Some(2.0));
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
    }
}
