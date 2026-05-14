use std::path::PathBuf;

use shockwave_config::{SlicerSettings, load_settings};

#[derive(Clone, Debug)]
pub struct Config {
    pub input: PathBuf,
    pub output_prefix: PathBuf,
    pub settings: SlicerSettings,
}

pub fn parse_args(args: Vec<String>) -> Result<Config, String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Err(usage());
    }

    let input = PathBuf::from(&args[0]);
    let mut output_prefix = PathBuf::from("occupancy");
    let mut settings_path = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                index += 1;
                settings_path = Some(
                    args.get(index)
                        .map(PathBuf::from)
                        .ok_or_else(|| "--config requires a settings JSON path".to_string())?,
                );
            }
            "--output" | "-o" => {
                index += 1;
                output_prefix = args
                    .get(index)
                    .map(PathBuf::from)
                    .ok_or_else(|| "--output requires a path".to_string())?;
            }
            flag if is_deprecated_slicer_flag(flag) => {
                return Err(format!(
                    "{flag} is deprecated; put slicer settings in the JSON passed with --config\n\n{}",
                    usage()
                ));
            }
            flag => {
                return Err(format!("unknown argument `{flag}`\n\n{}", usage()));
            }
        }
        index += 1;
    }

    let settings_path = settings_path.ok_or_else(|| {
        "missing required --config <settings.json>; slicer settings must come from JSON".to_string()
    })?;
    let settings = load_settings(settings_path)?;

    Ok(Config {
        input,
        output_prefix,
        settings,
    })
}

fn is_deprecated_slicer_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--voxel"
            | "--size"
            | "--padding-voxels"
            | "--origin"
            | "--field"
            | "--field-rate"
            | "--field-method"
            | "--kernel"
            | "--max-unreached-below"
            | "--unreached-cone-angle"
            | "--iso-spacing"
            | "--gcode"
            | "--export-ply"
            | "--wall-count"
            | "--extrusion-width"
            | "--filament-diameter"
            | "--bed-temperature"
            | "--nozzle-temperature"
            | "--fan-speed"
            | "--global-z-offset"
            | "--infill-spacing"
            | "--infill-percentage"
    )
}

fn usage() -> String {
    "usage: field-gen <input.stl> --config <settings.json> [--output <prefix>]\n\
\n\
STL coordinates are assumed to be millimeters. Slicer settings are loaded exclusively from the JSON config file.\n\
Deprecated slicer-setting flags such as --voxel, --field-method, --iso-spacing, --gcode, and --wall-count are no longer accepted."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shockwave_config::{Dimensions3, save_settings, settings_path_in_config_dir};

    #[test]
    fn requires_settings_json() {
        let error = parse_args(vec!["part.stl".to_string()]).unwrap_err();

        assert!(error.contains("missing required --config"));
    }

    #[test]
    fn loads_settings_json() {
        let settings_path = settings_path_in_config_dir(unique_temp_path("cli-config"));
        let mut settings = SlicerSettings::default();
        settings.slicing.layer_height_mm = 0.3;
        settings.field.voxel_size_mm = Dimensions3::uniform(0.8);
        settings.output.export_ply = true;
        save_settings(&settings_path, &settings).unwrap();

        let config = parse_args(vec![
            "part.stl".to_string(),
            "--config".to_string(),
            settings_path.display().to_string(),
            "--output".to_string(),
            "out/part".to_string(),
        ])
        .unwrap();

        assert_eq!(config.input, PathBuf::from("part.stl"));
        assert_eq!(config.output_prefix, PathBuf::from("out/part"));
        assert_eq!(config.settings.slicing.layer_height_mm, 0.3);
        assert_eq!(
            config.settings.field.voxel_size_mm,
            Dimensions3::uniform(0.8)
        );
        assert!(config.settings.output.export_ply);
    }

    #[test]
    fn rejects_deprecated_slicer_flags() {
        let error = parse_args(vec![
            "part.stl".to_string(),
            "--voxel".to_string(),
            "1".to_string(),
            "1".to_string(),
            "1".to_string(),
        ])
        .unwrap_err();

        assert!(error.contains("--voxel is deprecated"));
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
    }
}
