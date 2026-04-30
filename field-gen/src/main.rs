mod cli;
mod field;
mod geometry;
mod grid;
mod output;
mod stl;
mod voxelize;

use std::env;
use std::fs;

use cli::parse_args;
use field::{AnisotropicEuclideanPropagation, propagate_field};
use geometry::mesh_bounds;
use grid::build_grid;
use output::{build_atlas, metadata_json, write_occupancy_bmp};
use stl::parse_stl;
use voxelize::generate_occupancy;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = parse_args(env::args().skip(1).collect())?;
    let triangles = load_mesh(&config)?;
    let (grid, occupancy, field) = voxelize(&config, &triangles)?;
    let paths = write_outputs(&config, &triangles, &occupancy, &field, grid)?;
    print_summary(&triangles, &occupancy, grid, &paths);
    Ok(())
}

fn load_mesh(config: &cli::Config) -> Result<Vec<geometry::Triangle>, String> {
    let bytes = fs::read(&config.input)
        .map_err(|error| format!("failed to read {}: {error}", config.input.display()))?;
    let triangles = parse_stl(&bytes)?;
    if triangles.is_empty() {
        return Err("STL did not contain any triangles".to_string());
    }
    Ok(triangles)
}

fn voxelize(
    config: &cli::Config,
    triangles: &[geometry::Triangle],
) -> Result<(grid::Grid, Vec<u8>, Option<field::Field>), String> {
    let bounds = mesh_bounds(triangles);
    let grid = build_grid(config, bounds)?;
    let occupancy = generate_occupancy(triangles, grid);
    let field = if config.field_enabled {
        Some(propagate_field(
            &occupancy,
            grid,
            &AnisotropicEuclideanPropagation::new(config.field_rate),
        )?)
    } else {
        None
    };
    Ok((grid, occupancy, field))
}

struct OutputPaths {
    volume: std::path::PathBuf,
    image: std::path::PathBuf,
    metadata: std::path::PathBuf,
}

fn write_outputs(
    config: &cli::Config,
    triangles: &[geometry::Triangle],
    occupancy: &[u8],
    field: &Option<field::Field>,
    grid: grid::Grid,
) -> Result<OutputPaths, String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let volume_path = config.output_prefix.with_extension("occ");
    let image_path = config.output_prefix.with_extension("bmp");
    let metadata_path = config.output_prefix.with_extension("json");

    fs::write(&volume_path, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", volume_path.display()))?;
    write_occupancy_bmp(&image_path, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", image_path.display()))?;
    fs::write(
        &metadata_path,
        metadata_json(
            config,
            bounds,
            grid,
            atlas,
            &volume_path,
            &image_path,
            field.as_ref(),
            occupied_count,
            occupancy.len(),
        ),
    )
    .map_err(|error| format!("failed to write {}: {error}", metadata_path.display()))?;

    Ok(OutputPaths {
        volume: volume_path,
        image: image_path,
        metadata: metadata_path,
    })
}

fn print_summary(
    triangles: &[geometry::Triangle],
    occupancy: &[u8],
    grid: grid::Grid,
    paths: &OutputPaths,
) {
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();
    println!("Loaded {} triangles", triangles.len());
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
    println!("Occupied: {occupied_count} / {}", occupancy.len());
    println!("Wrote {}", paths.volume.display());
    println!("Wrote {}", paths.image.display());
    println!("Wrote {}", paths.metadata.display());
}
