mod cli;

use std::env;
use std::fs;
use std::time::{Duration, Instant};

use cli::parse_args;
use shockwave_clip::{TriangleSolid, clip_mesh_to_solid};
use shockwave_core::geometry::{Triangle, mesh_bounds};
use shockwave_core::grid::{Grid, GridSpec, build_grid};
use shockwave_iso::{Isosurface, IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_output::{
    Metadata, MetadataDocument, build_atlas, metadata_json, write_obj, write_occupancy_bmp,
    write_ply_binary,
};
use shockwave_stl::parse_stl;
use shockwave_voxel::field::{
    AnisotropicEuclideanPropagation, Field, expand_field, propagate_field,
};
use shockwave_voxel::voxelize::generate_occupancy;

const FIELD_EXTENSION_VOXELS: usize = 2;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let total_start = Instant::now();
    let config = parse_args(env::args().skip(1).collect())?;
    let triangles = load_mesh(&config)?;
    let (grid, occupancy, field) = voxelize(&config, &triangles)?;
    let paths = write_outputs(&config, &triangles, &occupancy, &field, grid)?;
    print_summary(&triangles, &occupancy, grid, &paths);
    log_timing("total", total_start.elapsed());
    Ok(())
}

fn load_mesh(config: &cli::Config) -> Result<Vec<Triangle>, String> {
    let read_start = Instant::now();
    let bytes = fs::read(&config.input)
        .map_err(|error| format!("failed to read {}: {error}", config.input.display()))?;
    log_timing("read stl", read_start.elapsed());

    let parse_start = Instant::now();
    let triangles = parse_stl(&bytes)?;
    log_timing("parse stl", parse_start.elapsed());
    if triangles.is_empty() {
        return Err("STL did not contain any triangles".to_string());
    }
    Ok(triangles)
}

fn voxelize(
    config: &cli::Config,
    triangles: &[Triangle],
) -> Result<(Grid, Vec<u8>, Option<Field>), String> {
    let bounds = mesh_bounds(triangles);
    let grid_start = Instant::now();
    let grid = build_grid(
        GridSpec {
            voxel_size: config.voxel_size,
            requested_size: config.requested_size,
            padding_voxels: config.padding_voxels,
            origin: config.origin,
        },
        bounds,
    )?;
    log_timing("build grid", grid_start.elapsed());

    let occupancy_start = Instant::now();
    let occupancy = generate_occupancy(triangles, grid);
    log_timing("generate occupancy", occupancy_start.elapsed());

    let field = if config.field_enabled {
        let propagation = AnisotropicEuclideanPropagation::new(config.field_rate);
        let propagation_start = Instant::now();
        let mut field = propagate_field(&occupancy, grid, &propagation)?;
        log_timing("propagate field", propagation_start.elapsed());

        let expansion_start = Instant::now();
        expand_field(&mut field, grid, FIELD_EXTENSION_VOXELS, &propagation);
        log_timing("expand field", expansion_start.elapsed());
        Some(field)
    } else {
        None
    };
    Ok((grid, occupancy, field))
}

struct OutputPaths {
    volume: std::path::PathBuf,
    image: std::path::PathBuf,
    mesh: Option<std::path::PathBuf>,
    mesh_ply: Option<std::path::PathBuf>,
    clipped_mesh: Option<std::path::PathBuf>,
    clipped_mesh_ply: Option<std::path::PathBuf>,
    metadata: std::path::PathBuf,
}

fn write_outputs(
    config: &cli::Config,
    triangles: &[Triangle],
    occupancy: &[u8],
    field: &Option<Field>,
    grid: Grid,
) -> Result<OutputPaths, String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let volume_path = config.output_prefix.with_extension("occ");
    let image_path = config.output_prefix.with_extension("bmp");
    let mesh_path = field
        .as_ref()
        .map(|_| config.output_prefix.with_extension("obj"));
    let mesh_ply_path = field
        .as_ref()
        .map(|_| config.output_prefix.with_extension("ply"));
    let clipped_mesh_path = field
        .as_ref()
        .map(|_| suffixed_output_path(config, "clipped", "obj"));
    let clipped_mesh_ply_path = field
        .as_ref()
        .map(|_| suffixed_output_path(config, "clipped", "ply"));
    let metadata_path = config.output_prefix.with_extension("json");

    let occ_start = Instant::now();
    fs::write(&volume_path, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", volume_path.display()))?;
    log_timing("write occ", occ_start.elapsed());

    let bmp_start = Instant::now();
    write_occupancy_bmp(&image_path, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", image_path.display()))?;
    log_timing("write bmp", bmp_start.elapsed());

    if let (Some(field), Some(mesh_path)) = (field.as_ref(), mesh_path.as_ref()) {
        let extract_start = Instant::now();
        let mesh = extract_regular_isosurfaces(field, grid, config.iso_spacing)?;
        log_timing("extract isosurfaces", extract_start.elapsed());
        eprintln!(
            "timing: isosurface set produced {} surfaces, {} vertices and {} triangles",
            mesh.surfaces.len(),
            mesh.vertex_count(),
            mesh.triangle_count()
        );

        let obj_start = Instant::now();
        write_obj(mesh_path, &mesh)
            .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
        log_timing("write obj", obj_start.elapsed());

        if let Some(mesh_ply_path) = mesh_ply_path.as_ref() {
            let ply_start = Instant::now();
            write_ply_binary(mesh_ply_path, &mesh)
                .map_err(|error| format!("failed to write {}: {error}", mesh_ply_path.display()))?;
            log_timing("write ply", ply_start.elapsed());
        }

        if let (Some(clipped_mesh_path), Some(clipped_mesh_ply_path)) =
            (clipped_mesh_path.as_ref(), clipped_mesh_ply_path.as_ref())
        {
            let clip_start = Instant::now();
            let clipped_mesh = clip_isosurfaces_to_solid(&mesh, triangles);
            log_timing("clip isosurfaces", clip_start.elapsed());
            eprintln!(
                "timing: clipped isosurface set produced {} surfaces, {} vertices and {} triangles",
                clipped_mesh.surfaces.len(),
                clipped_mesh.vertex_count(),
                clipped_mesh.triangle_count()
            );

            let clipped_obj_start = Instant::now();
            write_obj(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            log_timing("write clipped obj", clipped_obj_start.elapsed());

            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_ply_path, &clipped_mesh).map_err(|error| {
                format!(
                    "failed to write {}: {error}",
                    clipped_mesh_ply_path.display()
                )
            })?;
            log_timing("write clipped ply", clipped_ply_start.elapsed());
        }
    }

    let metadata_start = Instant::now();
    fs::write(
        &metadata_path,
        metadata_json(&MetadataDocument {
            metadata: Metadata {
                input: &config.input.display().to_string(),
                voxel_size: config.voxel_size,
                padding_voxels: config.padding_voxels,
                field_enabled: config.field_enabled,
                field_rate: config.field_rate,
                field_extension_voxels: FIELD_EXTENSION_VOXELS,
                iso_spacing: config.iso_spacing,
            },
            bounds,
            grid,
            atlas,
            volume_path: &volume_path,
            image_path: &image_path,
            mesh_path: mesh_path.as_deref(),
            mesh_ply_path: mesh_ply_path.as_deref(),
            clipped_mesh_path: clipped_mesh_path.as_deref(),
            clipped_mesh_ply_path: clipped_mesh_ply_path.as_deref(),
            field: field.as_ref(),
            occupied_count,
            voxel_count: occupancy.len(),
        }),
    )
    .map_err(|error| format!("failed to write {}: {error}", metadata_path.display()))?;
    log_timing("write metadata", metadata_start.elapsed());

    Ok(OutputPaths {
        volume: volume_path,
        image: image_path,
        mesh: mesh_path,
        mesh_ply: mesh_ply_path,
        clipped_mesh: clipped_mesh_path,
        clipped_mesh_ply: clipped_mesh_ply_path,
        metadata: metadata_path,
    })
}

fn clip_isosurfaces_to_solid(surfaces: &IsosurfaceSet, triangles: &[Triangle]) -> IsosurfaceSet {
    let solid = TriangleSolid::new(triangles.to_vec());
    IsosurfaceSet {
        surfaces: surfaces
            .surfaces
            .iter()
            .map(|surface| Isosurface {
                level: surface.level,
                value: surface.value,
                mesh: clip_mesh_to_solid(&surface.mesh, &solid),
            })
            .collect(),
    }
}

fn suffixed_output_path(config: &cli::Config, suffix: &str, extension: &str) -> std::path::PathBuf {
    let mut path = config.output_prefix.clone();
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("occupancy");
    path.set_file_name(format!("{stem}-{suffix}"));
    path.set_extension(extension);
    path
}

fn print_summary(triangles: &[Triangle], occupancy: &[u8], grid: Grid, paths: &OutputPaths) {
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
    if let Some(mesh) = &paths.mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(mesh) = &paths.mesh_ply {
        println!("Wrote {}", mesh.display());
    }
    if let Some(mesh) = &paths.clipped_mesh {
        println!("Wrote {}", mesh.display());
    }
    if let Some(mesh) = &paths.clipped_mesh_ply {
        println!("Wrote {}", mesh.display());
    }
    println!("Wrote {}", paths.metadata.display());
}

fn log_timing(label: &str, duration: Duration) {
    eprintln!("timing: {label}: {:.3}s", duration.as_secs_f64());
}
