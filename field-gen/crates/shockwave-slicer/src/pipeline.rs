use std::io::Write;

use shockwave_iso::{IsosurfaceSet, extract_regular_isosurfaces};
use shockwave_math::geometry::{Triangle, mesh_bounds};
use shockwave_math::grid::{Grid, GridSpec, build_grid};
use shockwave_path::LayerToolpaths;
use shockwave_voxel::field::Field;
use shockwave_voxel::voxelize::generate_occupancy;

use crate::error::{SliceError, SliceResult};
use crate::field::{align_field_to_model_floor, propagate_field};
use crate::gcode::write_gcode;
use crate::layers::{clip_isosurfaces_to_solid, toolpaths_from_isosurfaces};
use crate::progress::{CancellationToken, SlicePhase, SliceProgress};
use crate::settings::SliceSettings;

#[derive(Clone, Debug)]
pub struct SliceModelOutput {
    pub grid: Grid,
    pub occupancy: Vec<u8>,
    pub field: Option<Field>,
    pub isosurfaces: Option<IsosurfaceSet>,
    pub clipped_isosurfaces: Option<IsosurfaceSet>,
    pub layers: Vec<LayerToolpaths>,
}

pub fn voxelize(
    settings: &SliceSettings,
    triangles: &[Triangle],
    progress: &mut impl FnMut(SliceProgress),
) -> SliceResult<(Grid, Vec<u8>, Option<Field>)> {
    let bounds = mesh_bounds(triangles);
    progress_event(progress, SlicePhase::Voxelize, 0.0, "building voxel grid");
    let grid = build_grid(
        GridSpec {
            voxel_size: settings.voxel_size,
            requested_size: settings.requested_size,
            padding_voxels: settings.padding_voxels,
            origin: settings.origin,
        },
        bounds,
    )?;
    progress_event(progress, SlicePhase::Voxelize, 0.5, "generating occupancy");
    let occupancy = generate_occupancy(triangles, grid);

    let field = if settings.field_enabled {
        progress_event(
            progress,
            SlicePhase::PropagateField,
            0.0,
            "propagating field",
        );
        let mut field = propagate_field(settings, &occupancy, grid, progress)?;
        align_field_to_model_floor(&mut field, &occupancy, grid, bounds);
        Some(field)
    } else {
        None
    };
    progress_event(progress, SlicePhase::Voxelize, 1.0, "voxelization complete");
    Ok((grid, occupancy, field))
}

pub fn slice_model(
    writer: &mut impl Write,
    triangles: &[Triangle],
    settings: &SliceSettings,
    progress: &mut impl FnMut(SliceProgress),
    cancellation: &CancellationToken,
) -> SliceResult<SliceModelOutput> {
    check_cancelled(cancellation)?;
    let (grid, occupancy, field) = voxelize(settings, triangles, progress)?;
    check_cancelled(cancellation)?;

    let mut isosurfaces = None;
    let mut clipped_isosurfaces = None;
    let mut layers = Vec::new();
    if let Some(field) = &field {
        progress_event(
            progress,
            SlicePhase::ExtractLayers,
            0.0,
            "extracting isosurfaces",
        );
        let mesh = extract_regular_isosurfaces(field, grid, settings.iso_spacing)?;
        progress_event(
            progress,
            SlicePhase::ExtractLayers,
            1.0,
            "extracted isosurfaces",
        );
        check_cancelled(cancellation)?;

        progress_event(
            progress,
            SlicePhase::ClipLayers,
            0.0,
            "clipping isosurfaces",
        );
        let clipped_mesh = clip_isosurfaces_to_solid(&mesh, triangles);
        progress_event(progress, SlicePhase::ClipLayers, 1.0, "clipped isosurfaces");
        check_cancelled(cancellation)?;

        progress_event(progress, SlicePhase::GeneratePaths, 0.0, "generating paths");
        layers = toolpaths_from_isosurfaces(&clipped_mesh, settings, field, grid)?;
        progress_event(progress, SlicePhase::GeneratePaths, 1.0, "generated paths");
        check_cancelled(cancellation)?;

        progress_event(progress, SlicePhase::WriteGcode, 0.0, "writing gcode");
        write_gcode(writer, &layers, mesh_bounds(triangles), settings)?;
        progress_event(progress, SlicePhase::WriteGcode, 1.0, "wrote gcode");
        isosurfaces = Some(mesh);
        clipped_isosurfaces = Some(clipped_mesh);
    }

    Ok(SliceModelOutput {
        grid,
        occupancy,
        field,
        isosurfaces,
        clipped_isosurfaces,
        layers,
    })
}

fn check_cancelled(cancellation: &CancellationToken) -> SliceResult<()> {
    if cancellation.is_cancelled() {
        Err(SliceError::Cancelled)
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

#[cfg(test)]
mod tests {
    use shockwave_config::FieldMethod;
    use shockwave_math::geometry::Vec3;

    use super::*;
    use crate::field::FieldPropagation;

    #[test]
    fn reports_progress_phases_in_order() {
        let mut progress = Vec::new();
        let mut gcode = Vec::new();
        let output = slice_model(
            &mut gcode,
            &cube_triangles(v(10.0, 20.0, 0.0), v(18.0, 28.0, 5.0)),
            &test_settings(),
            &mut |event| progress.push(event.phase),
            &CancellationToken::default(),
        )
        .unwrap();

        let gcode = String::from_utf8(gcode).unwrap();
        assert!(!output.layers.is_empty());
        assert!(gcode.contains("M190 S60 ; set bed temperature and wait for it to be reached"));
        assert!(gcode.contains("G28 ; home all axes"));
        assert!(gcode.contains("M109 S215 ; Nozzle temperature"));
        assert!(gcode.contains("G1 E20 F200"));
        assert!(gcode.contains(";LAYER_CHANGE"));
        assert!(gcode.contains("; layer field_value=1.000000"));
        assert_order(&gcode, ";TYPE:Perimeter", ";TYPE:Infill");
        let first_print_move = first_print_move(&gcode).expect("first print move");
        assert!(first_print_move.x > 9.0);
        assert!(first_print_move.y > 19.0);
        assert!((first_print_move.z - 1.0).abs() < 1.0e-6);

        assert!(
            phase_index(&progress, SlicePhase::Voxelize)
                < phase_index(&progress, SlicePhase::PropagateField)
        );
        assert!(
            phase_index(&progress, SlicePhase::PropagateField)
                < phase_index(&progress, SlicePhase::ExtractLayers)
        );
        assert!(
            phase_index(&progress, SlicePhase::ExtractLayers)
                < phase_index(&progress, SlicePhase::ClipLayers)
        );
        assert!(
            phase_index(&progress, SlicePhase::ClipLayers)
                < phase_index(&progress, SlicePhase::GeneratePaths)
        );
        assert!(
            phase_index(&progress, SlicePhase::GeneratePaths)
                < phase_index(&progress, SlicePhase::WriteGcode)
        );
    }

    #[test]
    fn cancellation_is_checked_between_phases() {
        let cancellation = CancellationToken::default();
        let mut gcode = Vec::new();
        let mut progress = |event: SliceProgress| {
            if event.phase == SlicePhase::Voxelize && event.phase_progress >= 1.0 {
                cancellation.cancel();
            }
        };

        let error = slice_model(
            &mut gcode,
            &cube_triangles(v(0.0, 0.0, 0.0), v(4.0, 4.0, 3.0)),
            &test_settings(),
            &mut progress,
            &cancellation,
        )
        .unwrap_err();

        assert!(matches!(error, SliceError::Cancelled));
    }

    fn phase_index(phases: &[SlicePhase], phase: SlicePhase) -> usize {
        phases
            .iter()
            .position(|candidate| *candidate == phase)
            .unwrap_or_else(|| panic!("missing phase {phase:?}"))
    }

    fn assert_order(text: &str, before: &str, after: &str) {
        let before_index = text
            .find(before)
            .unwrap_or_else(|| panic!("missing {before}"));
        let after_index = text
            .find(after)
            .unwrap_or_else(|| panic!("missing {after}"));
        assert!(
            before_index < after_index,
            "expected {before} before {after}"
        );
    }

    fn first_print_move(gcode: &str) -> Option<Vec3> {
        gcode
            .lines()
            .find(|line| line.starts_with("G1 X"))
            .and_then(parse_gcode_position)
    }

    fn parse_gcode_position(line: &str) -> Option<Vec3> {
        let mut position = Vec3 {
            x: f64::NAN,
            y: f64::NAN,
            z: f64::NAN,
        };
        for word in line.split_whitespace() {
            let Some((axis, value)) = word.split_at_checked(1) else {
                continue;
            };
            let Ok(value) = value.parse::<f64>() else {
                continue;
            };
            match axis {
                "X" => position.x = value,
                "Y" => position.y = value,
                "Z" => position.z = value,
                _ => {}
            }
        }
        position.x.is_finite().then_some(position)
    }

    fn test_settings() -> SliceSettings {
        let field_rate = v(1.0, 1.0, 1.0);
        SliceSettings {
            voxel_size: v(1.0, 1.0, 1.0),
            requested_size: Some(v(16.0, 16.0, 12.0)),
            padding_voxels: 2,
            origin: None,
            field_enabled: true,
            propagation: FieldPropagation::from_method(FieldMethod::Anisotropic, field_rate),
            field_rate,
            max_unreached_below_mm: 20.0,
            unreached_cone_angle_degrees: 0.0,
            iso_spacing: 1.0,
            wall_count: 2,
            extrusion_width_mm: 0.4,
            filament_diameter_mm: 1.75,
            bed_temperature_c: 60,
            nozzle_temperature_c: 215,
            fan_speed_percent: 100,
            global_z_offset_mm: 0.0,
            infill_spacing_mm: Some(2.0),
        }
    }

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    fn tri(a: Vec3, b: Vec3, c: Vec3) -> Triangle {
        Triangle {
            vertices: [a, b, c],
        }
    }

    fn cube_triangles(min: Vec3, max: Vec3) -> Vec<Triangle> {
        vec![
            tri(
                v(min.x, min.y, min.z),
                v(max.x, max.y, min.z),
                v(max.x, min.y, min.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, max.y, min.z),
                v(max.x, max.y, min.z),
            ),
            tri(
                v(min.x, min.y, max.z),
                v(max.x, min.y, max.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, max.z),
                v(max.x, max.y, max.z),
                v(min.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(max.x, min.y, min.z),
                v(max.x, min.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(max.x, min.y, max.z),
                v(min.x, min.y, max.z),
            ),
            tri(
                v(min.x, max.y, min.z),
                v(max.x, max.y, max.z),
                v(max.x, max.y, min.z),
            ),
            tri(
                v(min.x, max.y, min.z),
                v(min.x, max.y, max.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, min.y, max.z),
                v(min.x, max.y, max.z),
            ),
            tri(
                v(min.x, min.y, min.z),
                v(min.x, max.y, max.z),
                v(min.x, max.y, min.z),
            ),
            tri(
                v(max.x, min.y, min.z),
                v(max.x, max.y, min.z),
                v(max.x, max.y, max.z),
            ),
            tri(
                v(max.x, min.y, min.z),
                v(max.x, max.y, max.z),
                v(max.x, min.y, max.z),
            ),
        ]
    }
}
