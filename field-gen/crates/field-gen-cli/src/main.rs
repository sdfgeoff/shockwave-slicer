mod cli;

use std::env;
use std::time::{Duration, Instant};

use cli::parse_args;
use shockwave_slicer::{CancellationToken, SliceProgress};
use shockwave_slicer_io::{SliceDebugOutput, SliceJobOutput, SliceJobRequest, run_slice_job};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let total_start = Instant::now();
    let config = parse_args(env::args().skip(1).collect())?;
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
    let cancellation = CancellationToken::default();
    let output = run_slice_job(
        &request,
        &config.settings,
        &mut progress,
        &mut timing,
        &cancellation,
    )?;
    print_summary(&output);
    log_timing("total", total_start.elapsed());
    Ok(())
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
