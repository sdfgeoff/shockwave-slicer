use std::fs;
use std::path::Path;

use serde::ser::{Serialize, SerializeStruct, Serializer};
use shockwave_iso::IsosurfaceSet;
use shockwave_math::geometry::{Bounds, Vec3};
use shockwave_math::grid::Grid;
use shockwave_voxel::field::Field;

#[derive(Clone, Copy, Debug)]
pub struct Atlas {
    pub columns: usize,
    pub rows: usize,
    pub width: usize,
    pub height: usize,
}

pub struct Metadata<'a> {
    pub input: &'a str,
    pub voxel_size: Vec3,
    pub padding_voxels: usize,
    pub field_enabled: bool,
    pub field_method: &'a str,
    pub kernel_path: Option<&'a str>,
    pub field_rate: Vec3,
    pub max_unreached_below_mm: f64,
    pub unreached_cone_angle_degrees: f64,
    pub field_extension_voxels: usize,
    pub iso_spacing: f64,
}

pub struct MetadataDocument<'a> {
    pub metadata: Metadata<'a>,
    pub bounds: Bounds,
    pub grid: Grid,
    pub atlas: Atlas,
    pub volume_path: &'a Path,
    pub image_path: &'a Path,
    pub mesh_path: Option<&'a Path>,
    pub clipped_mesh_path: Option<&'a Path>,
    pub field: Option<&'a Field>,
    pub occupied_count: usize,
    pub voxel_count: usize,
}

pub fn build_atlas(grid: Grid) -> Atlas {
    let columns = (grid.dims[2] as f64).sqrt().ceil() as usize;
    let rows = grid.dims[2].div_ceil(columns);

    Atlas {
        columns,
        rows,
        width: grid.dims[0] * columns,
        height: grid.dims[1] * rows,
    }
}

pub fn write_occupancy_bmp(
    path: &Path,
    occupancy: &[u8],
    field: Option<&Field>,
    grid: Grid,
    atlas: Atlas,
) -> Result<(), String> {
    let row_stride = (atlas.width * 3).div_ceil(4) * 4;
    let pixel_data_size = row_stride
        .checked_mul(atlas.height)
        .ok_or_else(|| "BMP pixel data size overflow".to_string())?;
    let file_size = 14usize
        .checked_add(40)
        .and_then(|header_size| header_size.checked_add(pixel_data_size))
        .ok_or_else(|| "BMP file size overflow".to_string())?;

    if atlas.width > i32::MAX as usize || atlas.height > i32::MAX as usize {
        return Err("BMP dimensions are too large".to_string());
    }
    if file_size > u32::MAX as usize || pixel_data_size > u32::MAX as usize {
        return Err("BMP file is too large".to_string());
    }

    let mut bytes = Vec::with_capacity(file_size);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(54u32).to_le_bytes());
    bytes.extend_from_slice(&(40u32).to_le_bytes());
    bytes.extend_from_slice(&(atlas.width as i32).to_le_bytes());
    bytes.extend_from_slice(&(-(atlas.height as i32)).to_le_bytes());
    bytes.extend_from_slice(&(1u16).to_le_bytes());
    bytes.extend_from_slice(&(24u16).to_le_bytes());
    bytes.extend_from_slice(&(0u32).to_le_bytes());
    bytes.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    bytes.extend_from_slice(&(2_835i32).to_le_bytes());
    bytes.extend_from_slice(&(2_835i32).to_le_bytes());
    bytes.extend_from_slice(&(0u32).to_le_bytes());
    bytes.extend_from_slice(&(0u32).to_le_bytes());

    let padding = row_stride - atlas.width * 3;
    for atlas_y in 0..atlas.height {
        for atlas_x in 0..atlas.width {
            let slice_column = atlas_x / grid.dims[0];
            let slice_row = atlas_y / grid.dims[1];
            let z = slice_row * atlas.columns + slice_column;
            let x = atlas_x % grid.dims[0];
            let y = atlas_y % grid.dims[1];
            let (red, green, blue) = if z < grid.dims[2] {
                let voxel_index = x + y * grid.dims[0] + z * grid.dims[0] * grid.dims[1];
                if let Some(field) = field {
                    (
                        encode_field_distance(field, voxel_index),
                        occupancy[voxel_index],
                        0,
                    )
                } else {
                    let value = occupancy[voxel_index];
                    (value, value, value)
                }
            } else {
                (0, 0, 0)
            };

            bytes.push(blue);
            bytes.push(green);
            bytes.push(red);
        }

        bytes.extend(std::iter::repeat_n(0, padding));
    }

    fs::write(path, bytes).map_err(|error| error.to_string())
}

pub fn write_ply_binary(path: &Path, surfaces: &IsosurfaceSet) -> Result<(), String> {
    let vertex_count = surfaces.vertex_count();
    let face_count = surfaces.triangle_count();
    if vertex_count > u32::MAX as usize {
        return Err("PLY export supports at most u32::MAX vertices".to_string());
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        format!(
            concat!(
                "ply\n",
                "format binary_little_endian 1.0\n",
                "comment shockwave-layers generated isosurfaces\n",
                "element vertex {}\n",
                "property double x\n",
                "property double y\n",
                "property double z\n",
                "element face {}\n",
                "property list uchar uint vertex_indices\n",
                "property uint surface_level\n",
                "property double surface_value\n",
                "end_header\n"
            ),
            vertex_count, face_count
        )
        .as_bytes(),
    );

    for surface in &surfaces.surfaces {
        for vertex in &surface.mesh.vertices {
            bytes.extend_from_slice(&vertex.x.to_le_bytes());
            bytes.extend_from_slice(&vertex.y.to_le_bytes());
            bytes.extend_from_slice(&vertex.z.to_le_bytes());
        }
    }

    let mut vertex_offset = 0usize;
    for surface in &surfaces.surfaces {
        for triangle in &surface.mesh.triangles {
            bytes.push(3);
            for index in triangle {
                let vertex_index = index + vertex_offset;
                bytes.extend_from_slice(&(vertex_index as u32).to_le_bytes());
            }
            bytes.extend_from_slice(&(surface.level as u32).to_le_bytes());
            bytes.extend_from_slice(&surface.value.to_le_bytes());
        }
        vertex_offset += surface.mesh.vertices.len();
    }

    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn encode_field_distance(field: &Field, index: usize) -> u8 {
    let distance = field.distances[index];
    if !distance.is_finite() || field.max_distance <= 0.0 {
        return 0;
    }

    ((distance / field.max_distance).clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn metadata_json(document: &MetadataDocument<'_>) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(document).expect("metadata should be serializable")
    )
}

impl Serialize for MetadataDocument<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let field_enabled = self.metadata.field_enabled;
        let mut state = serializer.serialize_struct("MetadataDocument", 29)?;
        state.serialize_field("input", self.metadata.input)?;
        state.serialize_field("units", "mm")?;
        state.serialize_field("layout", "x-fastest-u8")?;
        state.serialize_field("occupancy_file", &path_string(self.volume_path))?;
        state.serialize_field("image_file", &path_string(self.image_path))?;
        state.serialize_field("isosurface_file", &self.mesh_path.map(path_string))?;
        state.serialize_field(
            "clipped_isosurface_file",
            &self.clipped_mesh_path.map(path_string),
        )?;
        state.serialize_field("mesh_format", "binary_little_endian_ply")?;
        state.serialize_field("image_format", "bmp-r-field-g-occupancy-slice-atlas")?;
        state.serialize_field("image_grid", &[self.atlas.columns, self.atlas.rows])?;
        state.serialize_field("image_size_px", &[self.atlas.width, self.atlas.height])?;
        state.serialize_field("dimensions", &self.grid.dims)?;
        state.serialize_field("voxel_size_mm", &vec3_array(self.metadata.voxel_size))?;
        state.serialize_field("padding_voxels", &self.metadata.padding_voxels)?;
        state.serialize_field("field_enabled", &field_enabled)?;
        state.serialize_field(
            "field_method",
            &field_enabled.then_some(self.metadata.field_method),
        )?;
        state.serialize_field("kernel_file", &self.metadata.kernel_path)?;
        state.serialize_field("field_rate", &vec3_array(self.metadata.field_rate))?;
        state.serialize_field(
            "max_unreached_below_mm",
            &field_enabled.then_some(self.metadata.max_unreached_below_mm),
        )?;
        state.serialize_field(
            "unreached_cone_angle_degrees",
            &field_enabled.then_some(self.metadata.unreached_cone_angle_degrees),
        )?;
        state.serialize_field(
            "field_extension_voxels",
            &if field_enabled {
                self.metadata.field_extension_voxels
            } else {
                0
            },
        )?;
        state.serialize_field(
            "iso_spacing",
            &field_enabled.then_some(self.metadata.iso_spacing),
        )?;
        state.serialize_field(
            "field_max_distance",
            &self.field.map(|field| field.max_distance),
        )?;
        state.serialize_field("origin_mm", &vec3_array(self.grid.origin))?;
        state.serialize_field("actual_size_mm", &vec3_array(self.grid.actual_size))?;
        state.serialize_field("model_bounds_min_mm", &vec3_array(self.bounds.min))?;
        state.serialize_field("model_bounds_max_mm", &vec3_array(self.bounds.max))?;
        state.serialize_field("occupied_voxels", &self.occupied_count)?;
        state.serialize_field("total_voxels", &self.voxel_count)?;
        state.end()
    }
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn vec3_array(value: Vec3) -> [f64; 3] {
    [value.x, value.y, value.z]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::Value;
    use shockwave_math::geometry::{Bounds, Vec3};
    use shockwave_math::grid::Grid;
    use shockwave_voxel::field::Field;

    use super::*;

    #[test]
    fn metadata_json_serializes_valid_json() {
        let volume_path = Path::new("out/part.occ");
        let image_path = Path::new("out/part.bmp");
        let mesh_path = Path::new("out/part.ply");
        let field = Field {
            distances: vec![0.0],
            max_distance: 12.5,
        };
        let document = MetadataDocument {
            metadata: Metadata {
                input: "input/part \"quoted\".stl",
                voxel_size: v(0.4, 0.5, 0.6),
                padding_voxels: 3,
                field_enabled: true,
                field_method: "trapezoid",
                kernel_path: Some("kernels\\field.json"),
                field_rate: v(3.7, 3.7, 1.0),
                max_unreached_below_mm: 5.0,
                unreached_cone_angle_degrees: 55.0,
                field_extension_voxels: 2,
                iso_spacing: 0.25,
            },
            bounds: Bounds {
                min: v(1.0, 2.0, 3.0),
                max: v(4.0, 5.0, 6.0),
            },
            grid: Grid {
                origin: v(-1.0, -2.0, -3.0),
                dims: [2, 3, 4],
                voxel_size: v(0.4, 0.5, 0.6),
                actual_size: v(0.8, 1.5, 2.4),
            },
            atlas: Atlas {
                columns: 2,
                rows: 2,
                width: 4,
                height: 6,
            },
            volume_path,
            image_path,
            mesh_path: Some(mesh_path),
            clipped_mesh_path: None,
            field: Some(&field),
            occupied_count: 7,
            voxel_count: 24,
        };

        let json: Value = serde_json::from_str(&metadata_json(&document)).unwrap();

        assert_eq!(json["input"], "input/part \"quoted\".stl");
        assert_eq!(json["occupancy_file"], "out/part.occ");
        assert_eq!(json["isosurface_file"], "out/part.ply");
        assert_eq!(json["clipped_isosurface_file"], Value::Null);
        assert_eq!(json["dimensions"], serde_json::json!([2, 3, 4]));
        assert_eq!(json["field_method"], "trapezoid");
        assert_eq!(json["field_max_distance"], 12.5);
    }

    #[test]
    fn metadata_json_nulls_field_values_when_field_disabled() {
        let document = MetadataDocument {
            metadata: Metadata {
                input: "input/part.stl",
                voxel_size: v(1.0, 1.0, 1.0),
                padding_voxels: 0,
                field_enabled: false,
                field_method: "trapezoid",
                kernel_path: None,
                field_rate: v(1.0, 1.0, 1.0),
                max_unreached_below_mm: 5.0,
                unreached_cone_angle_degrees: 55.0,
                field_extension_voxels: 2,
                iso_spacing: 0.25,
            },
            bounds: Bounds {
                min: v(0.0, 0.0, 0.0),
                max: v(1.0, 1.0, 1.0),
            },
            grid: Grid {
                origin: v(0.0, 0.0, 0.0),
                dims: [1, 1, 1],
                voxel_size: v(1.0, 1.0, 1.0),
                actual_size: v(1.0, 1.0, 1.0),
            },
            atlas: Atlas {
                columns: 1,
                rows: 1,
                width: 1,
                height: 1,
            },
            volume_path: Path::new("out/part.occ"),
            image_path: Path::new("out/part.bmp"),
            mesh_path: None,
            clipped_mesh_path: None,
            field: None,
            occupied_count: 1,
            voxel_count: 1,
        };

        let json: Value = serde_json::from_str(&metadata_json(&document)).unwrap();

        assert_eq!(json["field_method"], Value::Null);
        assert_eq!(json["max_unreached_below_mm"], Value::Null);
        assert_eq!(json["unreached_cone_angle_degrees"], Value::Null);
        assert_eq!(json["field_extension_voxels"], 0);
        assert_eq!(json["iso_spacing"], Value::Null);
        assert_eq!(json["field_max_distance"], Value::Null);
    }

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }
}
