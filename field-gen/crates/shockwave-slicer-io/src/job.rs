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
    CancellationToken, FIELD_EXTENSION_VOXELS, SlicePhase, SliceProgress, SliceSettings,
    clip_isosurfaces_to_solid, toolpaths_from_isosurfaces, voxelize,
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
    pub layers: Vec<LayerToolpaths>,
}

pub fn run_slice_job(
    request: &SliceJobRequest,
    settings: &SlicerSettings,
    progress: &mut impl FnMut(SliceProgress),
    timing: &mut impl FnMut(&str, Duration),
    cancellation: &CancellationToken,
) -> Result<SliceJobOutput, String> {
    check_cancelled(cancellation)?;
    progress_event(progress, SlicePhase::LoadModel, 0.0, "loading model");
    let load_start = Instant::now();
    let triangles = load_stl_model(&request.input)?;
    timing("load stl", load_start.elapsed());
    progress_event(progress, SlicePhase::LoadModel, 1.0, "loaded model");
    check_cancelled(cancellation)?;

    let runtime_settings = runtime_slice_settings(settings, timing)?;
    check_cancelled(cancellation)?;
    let request = SliceJobRequest {
        input: request.input.clone(),
        output_prefix: request.output_prefix.clone(),
        debug_output: SliceDebugOutput {
            export_ply: settings.output.export_ply,
            gcode: settings.output.gcode,
        },
        kernel_path: settings.field.kernel_path.clone(),
    };
    run_slice_debug_outputs(
        &request,
        &runtime_settings,
        &triangles,
        progress,
        timing,
        cancellation,
    )
}

pub fn run_slice_debug_outputs(
    request: &SliceJobRequest,
    settings: &SliceSettings,
    triangles: &[Triangle],
    progress: &mut impl FnMut(SliceProgress),
    timing: &mut impl FnMut(&str, Duration),
    cancellation: &CancellationToken,
) -> Result<SliceJobOutput, String> {
    check_cancelled(cancellation)?;
    let voxelize_start = Instant::now();
    let (grid, occupancy, field) =
        voxelize(settings, triangles, progress).map_err(|error| error.to_string())?;
    timing("voxelize", voxelize_start.elapsed());
    check_cancelled(cancellation)?;

    let (paths, layers) = write_outputs(
        request,
        settings,
        triangles,
        &occupancy,
        &field,
        grid,
        progress,
        timing,
        cancellation,
    )?;
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    Ok(SliceJobOutput {
        paths,
        grid,
        occupied_count,
        voxel_count: occupancy.len(),
        triangle_count: triangles.len(),
        layers,
    })
}

fn write_outputs(
    request: &SliceJobRequest,
    settings: &SliceSettings,
    triangles: &[Triangle],
    occupancy: &[u8],
    field: &Option<Field>,
    grid: Grid,
    progress: &mut impl FnMut(SliceProgress),
    timing: &mut impl FnMut(&str, Duration),
    cancellation: &CancellationToken,
) -> Result<(SliceOutputPaths, Vec<LayerToolpaths>), String> {
    let bounds = mesh_bounds(triangles);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let paths = SliceOutputPaths::from_prefix(
        &request.output_prefix,
        field.is_some(),
        request.debug_output.export_ply,
        request.debug_output.gcode,
    );
    let mut layers = Vec::new();

    let occ_start = Instant::now();
    fs::write(&paths.volume, occupancy)
        .map_err(|error| format!("failed to write {}: {error}", paths.volume.display()))?;
    timing("write occ", occ_start.elapsed());
    check_cancelled(cancellation)?;

    let bmp_start = Instant::now();
    write_occupancy_bmp(&paths.image, occupancy, field.as_ref(), grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", paths.image.display()))?;
    timing("write bmp", bmp_start.elapsed());
    check_cancelled(cancellation)?;

    if let Some(field) = field
        .as_ref()
        .filter(|_| request.debug_output.export_ply || request.debug_output.gcode)
    {
        progress_event(
            progress,
            SlicePhase::ExtractLayers,
            0.0,
            "extracting isosurfaces",
        );
        check_cancelled(cancellation)?;
        let mesh = extract_isosurfaces_with_timing(field, grid, settings.iso_spacing, timing)?;
        progress_event(
            progress,
            SlicePhase::ExtractLayers,
            1.0,
            "extracted isosurfaces",
        );
        check_cancelled(cancellation)?;

        if let Some(mesh_path) = paths.mesh.as_ref() {
            let ply_start = Instant::now();
            write_ply_binary(mesh_path, &mesh)
                .map_err(|error| format!("failed to write {}: {error}", mesh_path.display()))?;
            timing("write ply", ply_start.elapsed());
            check_cancelled(cancellation)?;
        }

        progress_event(
            progress,
            SlicePhase::ClipLayers,
            0.0,
            "clipping isosurfaces",
        );
        check_cancelled(cancellation)?;
        let clipped_mesh = clip_isosurfaces_with_timing(&mesh, triangles, timing);
        progress_event(progress, SlicePhase::ClipLayers, 1.0, "clipped isosurfaces");
        check_cancelled(cancellation)?;

        if let Some(clipped_mesh_path) = paths.clipped_mesh.as_ref() {
            let clipped_ply_start = Instant::now();
            write_ply_binary(clipped_mesh_path, &clipped_mesh).map_err(|error| {
                format!("failed to write {}: {error}", clipped_mesh_path.display())
            })?;
            timing("write clipped ply", clipped_ply_start.elapsed());
            check_cancelled(cancellation)?;
        }

        if let Some(gcode_path) = paths.gcode.as_ref() {
            progress_event(progress, SlicePhase::GeneratePaths, 0.0, "generating paths");
            check_cancelled(cancellation)?;
            let path_start = Instant::now();
            layers = toolpaths_from_isosurfaces(&clipped_mesh, settings, field, grid)
                .map_err(|error| error.to_string())?;
            timing("generate perimeter paths", path_start.elapsed());
            progress_event(progress, SlicePhase::GeneratePaths, 1.0, "generated paths");
            check_cancelled(cancellation)?;

            progress_event(progress, SlicePhase::WriteGcode, 0.0, "writing gcode");
            write_gcode_with_timing(gcode_path, &layers, settings, triangles, timing)?;
            progress_event(progress, SlicePhase::WriteGcode, 1.0, "wrote gcode");
            check_cancelled(cancellation)?;
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

    Ok((paths, layers))
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
    layers: &[LayerToolpaths],
    settings: &SliceSettings,
    triangles: &[Triangle],
    timing: &mut impl FnMut(&str, Duration),
) -> Result<(), String> {
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

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err("cancelled".to_string())
    } else {
        Ok(())
    }
}

fn progress_event(
    progress: &mut impl FnMut(SliceProgress),
    phase: SlicePhase,
    phase_progress: f32,
    message: &str,
) {
    progress(SliceProgress {
        phase,
        phase_progress,
        message: message.to_string(),
    });
}
