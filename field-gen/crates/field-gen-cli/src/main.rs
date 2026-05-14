mod cli;

use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use cli::parse_args;
use serde_json::Value;
use shockwave_config::Dimensions3;
use shockwave_slicer::{FieldPropagation, SliceProgress, SliceSettings};
use shockwave_slicer_io::{SliceDebugOutput, SliceJobOutput, SliceJobRequest, run_slice_job};
use shockwave_voxel::field::{ExplicitKernelPropagation, KernelMove, KernelPathCheck};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let total_start = Instant::now();
    let config = parse_args(env::args().skip(1).collect())?;
    let settings = slicer_settings_from_config(&config)?;
    let request = SliceJobRequest {
        input: config.input.clone(),
        output_prefix: config.output_prefix.clone(),
        debug_output: SliceDebugOutput {
            export_ply: config.settings.output.export_ply,
            gcode: config.settings.output.gcode,
        },
        kernel_path: config.settings.field.kernel_path.clone(),
    };
    let mut progress = stderr_progress();
    let mut timing = log_timing;
    let output = run_slice_job(&request, &settings, &mut progress, &mut timing)?;
    print_summary(&output);
    log_timing("total", total_start.elapsed());
    Ok(())
}

fn slicer_settings_from_config(config: &cli::Config) -> Result<SliceSettings, String> {
    let settings = &config.settings;
    let field_rate = vec3_from_dimensions(settings.field.anisotropic_rate);
    let propagation = if let Some(kernel_path) = &settings.field.kernel_path {
        let load_start = Instant::now();
        let propagation = load_kernel_propagation(kernel_path)?;
        log_timing("load kernel", load_start.elapsed());
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

fn vec3_from_dimensions(value: Dimensions3) -> shockwave_math::geometry::Vec3 {
    shockwave_math::geometry::Vec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn stderr_progress() -> impl FnMut(SliceProgress) {
    let mut last_percent = None;
    move |event| {
        if event.phase_progress >= 1.0 {
            eprintln!("{:?}: complete - {}", event.phase, event.message);
            last_percent = None;
            return;
        }

        let percent = (event.phase_progress * 100.0).floor() as i32;
        if last_percent != Some(percent) && percent % 5 == 0 {
            eprintln!("{:?}: {:>3}% - {}", event.phase, percent, event.message);
            last_percent = Some(percent);
        }
    }
}

fn load_kernel_propagation(path: &Path) -> Result<ExplicitKernelPropagation, String> {
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

fn print_summary(output: &SliceJobOutput) {
    let paths = &output.paths;
    let grid = output.grid;
    println!("Loaded {} triangles", output.triangle_count);
    println!(
        "Grid: {} x {} x {} voxels",
        grid.dims[0], grid.dims[1], grid.dims[2]
    );
    println!(
        "Voxel size: {:.6} x {:.6} x {:.6} mm",
        grid.voxel_size.x, grid.voxel_size.y, grid.voxel_size.z
    );
    println!(
        "Actual size: {:.6} x {:.6} x {:.6} mm",
        grid.actual_size.x, grid.actual_size.y, grid.actual_size.z
    );
    println!(
        "Occupied: {} / {}",
        output.occupied_count, output.voxel_count
    );
    println!("Wrote {}", paths.volume.display());
    println!("Wrote {}", paths.image.display());
    if let Some(mesh) = &paths.mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(mesh) = &paths.clipped_mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(gcode) = &paths.gcode {
        println!("Wrote {}", gcode.display());
    }
    println!("Wrote {}", paths.metadata.display());
}

fn log_timing(label: &str, duration: Duration) {
    eprintln!("timing: {label}: {:.3}s", duration.as_secs_f64());
}
