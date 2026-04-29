use std::path::PathBuf;

use crate::geometry::Vec3;

#[derive(Clone, Debug)]
pub struct Config {
    pub input: PathBuf,
    pub output_prefix: PathBuf,
    pub voxel_size: Vec3,
    pub requested_size: Option<Vec3>,
    pub padding_voxels: usize,
    pub origin: Option<Vec3>,
}

pub fn parse_args(args: Vec<String>) -> Result<Config, String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Err(usage());
    }

    let input = PathBuf::from(&args[0]);
    let mut output_prefix = PathBuf::from("occupancy");
    let mut voxel_size = None;
    let mut requested_size = None;
    let mut padding_voxels = 3;
    let mut origin = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--voxel" => {
                voxel_size = Some(parse_vec3_flag("--voxel", &args, &mut index)?);
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

    let voxel_size = voxel_size.ok_or_else(|| "--voxel x y z is required".to_string())?;
    validate_positive_vec3("--voxel", voxel_size)?;
    if let Some(size) = requested_size {
        validate_positive_vec3("--size", size)?;
    }

    Ok(Config {
        input,
        output_prefix,
        voxel_size,
        requested_size,
        padding_voxels,
        origin,
    })
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

fn usage() -> String {
    "usage: field-gen <input.stl> --voxel <x-mm> <y-mm> <z-mm> [--size <x-mm> <y-mm> <z-mm>] [--padding-voxels <n>] [--origin <x-mm> <y-mm> <z-mm>] [--output <prefix>]\n\
\n\
STL coordinates are assumed to be millimeters. If --size is provided, it is treated as a maximum grid size.\n\
By default, the grid fits the STL bounds plus 3 voxels of padding on each side.\n\
Voxel size takes priority: grid dimensions are ceil(size / voxel), so actual size may expand slightly."
        .to_string()
}
