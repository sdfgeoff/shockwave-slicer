mod job;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use shockwave_math::geometry::{Bounds, Triangle};
use shockwave_path::LayerToolpaths;
use shockwave_slicer::{SliceSettings, write_gcode};

pub use job::{SliceDebugOutput, SliceJobOutput, SliceJobRequest, run_slice_debug_outputs};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceOutputPaths {
    pub volume: PathBuf,
    pub image: PathBuf,
    pub mesh: Option<PathBuf>,
    pub clipped_mesh: Option<PathBuf>,
    pub gcode: Option<PathBuf>,
    pub metadata: PathBuf,
}

impl SliceOutputPaths {
    pub fn from_prefix(
        prefix: &Path,
        has_field: bool,
        export_ply: bool,
        gcode_enabled: bool,
    ) -> Self {
        Self {
            volume: prefix.with_extension("occ"),
            image: prefix.with_extension("bmp"),
            mesh: (has_field && export_ply).then(|| prefix.with_extension("ply")),
            clipped_mesh: (has_field && export_ply)
                .then(|| suffixed_output_path(prefix, "clipped", "ply")),
            gcode: gcode_enabled.then(|| prefix.with_extension("gcode")),
            metadata: prefix.with_extension("json"),
        }
    }
}

pub fn load_stl_model(path: impl AsRef<Path>) -> Result<Vec<Triangle>, String> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let triangles = shockwave_stl::parse_stl(&bytes)?;
    if triangles.is_empty() {
        return Err("STL did not contain any triangles".to_string());
    }
    Ok(triangles)
}

pub fn write_gcode_atomically(
    path: impl AsRef<Path>,
    layers: &[LayerToolpaths],
    bounds: Bounds,
    settings: &SliceSettings,
) -> Result<(), String> {
    write_atomically(path, |writer| {
        write_gcode(writer, layers, bounds, settings).map_err(|error| error.to_string())
    })
}

pub fn write_atomically(
    path: impl AsRef<Path>,
    write: impl FnOnce(&mut dyn Write) -> Result<(), String>,
) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    let temp_path = temporary_path(path);
    let result = (|| {
        let mut file = fs::File::create(&temp_path)
            .map_err(|error| format!("failed to create {}: {error}", temp_path.display()))?;
        write(&mut file)?;
        file.flush()
            .map_err(|error| format!("failed to flush {}: {error}", temp_path.display()))?;
        fs::rename(&temp_path, path).map_err(|error| {
            format!(
                "failed to rename {} to {}: {error}",
                temp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn suffixed_output_path(prefix: &Path, suffix: &str, extension: &str) -> PathBuf {
    let mut path = prefix.to_path_buf();
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("occupancy");
    path.set_file_name(format!("{stem}-{suffix}"));
    path.set_extension(extension);
    path
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temp_path = path.to_path_buf();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    temp_path.set_file_name(format!("{file_name}.tmp-{}", std::process::id()));
    temp_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_compatible_output_paths_from_prefix() {
        let paths = SliceOutputPaths::from_prefix(Path::new("/tmp/part"), true, true, true);

        assert_eq!(paths.volume, PathBuf::from("/tmp/part.occ"));
        assert_eq!(paths.image, PathBuf::from("/tmp/part.bmp"));
        assert_eq!(paths.mesh, Some(PathBuf::from("/tmp/part.ply")));
        assert_eq!(
            paths.clipped_mesh,
            Some(PathBuf::from("/tmp/part-clipped.ply"))
        );
        assert_eq!(paths.gcode, Some(PathBuf::from("/tmp/part.gcode")));
        assert_eq!(paths.metadata, PathBuf::from("/tmp/part.json"));
    }

    #[test]
    fn atomic_write_replaces_final_file_on_success() {
        let path = unique_temp_path("atomic-success.txt");

        write_atomically(&path, |writer| {
            writer.write_all(b"new").map_err(|error| error.to_string())
        })
        .unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "new");
    }

    #[test]
    fn atomic_write_does_not_leave_final_file_on_error() {
        let path = unique_temp_path("atomic-error.txt");
        let _ = fs::remove_file(&path);

        let error = write_atomically(&path, |writer| {
            writer
                .write_all(b"partial")
                .map_err(|error| error.to_string())?;
            Err("cancelled".to_string())
        })
        .unwrap_err();

        assert_eq!(error, "cancelled");
        assert!(!path.exists());
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", std::process::id()))
    }
}
