use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shockwave_config::SlicerSettings;
use shockwave_iso::{IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_math::geometry::{Triangle, mesh_bounds};
use shockwave_math::grid::Grid;
use shockwave_output::{
    Metadata, MetadataDocument, build_atlas, metadata_json, write_occupancy_bmp, write_ply_binary,
};
use shockwave_path::LayerToolpaths;
use shockwave_slicer::{
    FIELD_EXTENSION_VOXELS, SliceProgress, SliceSettings, clip_isosurfaces_to_solid,
    toolpaths_from_isosurfaces, voxelize,
};
use shockwave_voxel::field::Field;

use crate::runtime_slice_settings;
use crate::{SliceOutputPaths, load_stl_model, write_gcode_atomically};

#[derive(Clone, Debug)]
pub struct SliceDebugOutput {
    pub export_ply: bool,
    pub gcode: bool,
}

#[derive(Clone, Debug)]
pub struct SliceJobRequest {
    pub input: PathBuf,
    pub output_prefix: PathBuf,
    pub debug_output: SliceDebugOutput,
    pub kernel_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct SliceJobOutput {
    pub paths: SliceOutputPaths,
    pub grid: Grid,
    pub occupied_count: usize,
    pub voxel_count: usize,
    pub triangle_count: usize,
}

pub fn run_slice_job(
    request: &SliceJobRequest,
    settings: &SlicerSettings,
    progress: &mut impl FnMut(SliceProgress),
    timing: &mut impl FnMut(&str, Duration),
) -> Result<SliceJobOutput, String> {
    let load_start = Instant::now();
    let triangles = load_stl_model(&request.input)?;
    timing("load stl", load_start.elapsed());
    let runtime_settings = runtime_slice_settings(settings, timing)?;
    let request = SliceJobRequest {
        input: request.input.clone(),
        output_prefix: request.output_prefix.clone(),
        debug_output: SliceDebugOutput {
            export_ply: settings.output.export_ply,
            gcode: settings.output.gcode,
        },
        kernel_path: settings.field.kernel_path.clone(),
    };
    run_slice_debug_outputs(&request, &runtime_settings, &triangles, progress, timing)
}

pub fn run_slice_debug_outputs(
    request: &SliceJobRequest,
    settings: &SliceSettings,
    triangles: &[Triangle],
    progress: &mut impl FnMut(SliceProgress),
    timing: &mut impl FnMut(&str, Duration),
) -> Result<SliceJobOutput, String> {
    let voxelize_start = Instant::now();
    let (grid, occupancy, field) =
        voxelize(settings, triangles, progress).map_err(|error| error.to_string())?;
    timing("voxelize", voxelize_start.elapsed());

    let paths = write_outputs(
        request, settings, triangles, &occupancy, &field, grid, timing,
    )?;
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    Ok(SliceJobOutput {
        paths,
        grid,
        occupied_count,
        voxel_count: occupancy.len(),
        triangle_count: triangles.len(),
    })
}

fn write_outputs(
    request: &SliceJobRequest,
    settings: &SliceSettings,
    triangles: &[Triangle],
    occupancy: &[u8],
    field: &Option<Field>,
    grid: Grid,
    timing: &mut impl FnMut(&str, Duration),
) -> Result<SliceOutputPaths, String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let paths = SliceOutputPaths::from_prefix(
        &request.output_prefix,
        field.is_some(),
        request.debug_output.export_ply,
        request.debug_output.gcode,
    );

    let occ_start = Instant::now();
    fs::write(&paths.volume, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", paths.volume.display()))?;
    timing("write occ", occ_start.elapsed());

    let bmp_start = Instant::now();
    write_occupancy_bmp(&paths.image, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", paths.image.display()))?;
    timing("write bmp", bmp_start.elapsed());

    if let Some(field) = field
        .as_ref()
        .filter(|_| request.debug_output.export_ply || request.debug_output.gcode)
    {
        let mesh = extract_isosurfaces_with_timing(field, grid, settings.iso_spacing, timing)?;

        if let Some(mesh_path) = paths.mesh.as_ref() {
            let ply_start = Instant::now();
            write_ply_binary(mesh_path, &mesh)
                .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
            timing("write ply", ply_start.elapsed());
        }

        let clipped_mesh = clip_isosurfaces_with_timing(&mesh, triangles, timing);

        if let Some(clipped_mesh_path) = paths.clipped_mesh.as_ref() {
            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            timing("write clipped ply", clipped_ply_start.elapsed());
        }

        if let Some(gcode_path) = paths.gcode.as_ref() {
            write_gcode_with_timing(
                gcode_path,
                &clipped_mesh,
                settings,
                triangles,
                field,
                grid,
                timing,
            )?;
        }
    }

    let metadata_start = Instant::now();
    fs::write(
        &paths.metadata,
        metadata_json(&MetadataDocument {
            metadata: Metadata {
                input: &request.input.display().to_string(),
                voxel_size: settings.voxel_size,
                padding_voxels: settings.padding_voxels,
                field_enabled: settings.field_enabled,
                field_method: settings.field_method_name(),
                kernel_path: request
                    .kernel_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .as_deref(),
                field_rate: settings.field_rate,
                max_unreached_below_mm: settings.max_unreached_below_mm,
                unreached_cone_angle_degrees: settings.unreached_cone_angle_degrees,
                field_extension_voxels: FIELD_EXTENSION_VOXELS,
                iso_spacing: settings.iso_spacing,
            },
            bounds,
            grid,
            atlas,
            volume_path: &paths.volume,
            image_path: &paths.image,
            mesh_path: paths.mesh.as_deref(),
            clipped_mesh_path: paths.clipped_mesh.as_deref(),
            field: field.as_ref(),
            occupied_count,
            voxel_count: occupancy.len(),
        }),
    )
    .map_err(|error| format!("failed to write {}: {error}", paths.metadata.display()))?;
    timing("write metadata", metadata_start.elapsed());

    Ok(paths)
}

fn extract_isosurfaces_with_timing(
    field: &Field,
    grid: Grid,
    iso_spacing: f64,
    timing: &mut impl FnMut(&str, Duration),
) -> Result<IsosurfaceSet, String> {
    let start = Instant::now();
    let mesh = extract_regular_isosurfaces(field, grid, iso_spacing)?;
    timing("extract isosurfaces", start.elapsed());
    eprintln!(
        "timing: isosurface set produced {} surfaces, {} vertices and {} triangles",
        mesh.surfaces.len(),
        mesh.vertex_count(),
        mesh.triangle_count()
    );
    Ok(mesh)
}

fn clip_isosurfaces_with_timing(
    mesh: &IsosurfaceSet,
    triangles: &[Triangle],
    timing: &mut impl FnMut(&str, Duration),
) -> IsosurfaceSet {
    let start = Instant::now();
    let clipped_mesh = clip_isosurfaces_to_solid(mesh, triangles);
    timing("clip isosurfaces", start.elapsed());
    eprintln!(
        "timing: clipped isosurface set produced {} surfaces, {} vertices and {} triangles",
        clipped_mesh.surfaces.len(),
        clipped_mesh.vertex_count(),
        clipped_mesh.triangle_count()
    );
    clipped_mesh
}

fn write_gcode_with_timing(
    gcode_path: &Path,
    clipped_mesh: &IsosurfaceSet,
    settings: &SliceSettings,
    triangles: &[Triangle],
    field: &Field,
    grid: Grid,
    timing: &mut impl FnMut(&str, Duration),
) -> Result<(), String> {
    let path_start = Instant::now();
    let layers = toolpaths_from_isosurfaces(clipped_mesh, settings, field, grid)
        .map_err(|error| error.to_string())?;
    timing("generate perimeter paths", path_start.elapsed());
    eprintln!(
        "timing: generated {} toolpath layers with {} paths",
        layers.len(),
        layers.iter().map(LayerToolpaths::path_count).sum::<usize>()
    );

    let gcode_start = Instant::now();
    write_gcode_atomically(gcode_path, &layers, mesh_bounds(triangles), settings)?;
    timing("write gcode", gcode_start.elapsed());
    Ok(())
}
