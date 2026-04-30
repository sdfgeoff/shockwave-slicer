use std::fs;
use std::path::Path;

use shockwave_core::geometry::{Bounds, Vec3};
use shockwave_core::grid::Grid;
use shockwave_iso::IsosurfaceSet;
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
    pub field_rate: Vec3,
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
        concat!(
            "{{\n",
            "  \"input\": \"{}\",\n",
            "  \"units\": \"mm\",\n",
            "  \"layout\": \"x-fastest-u8\",\n",
            "  \"occupancy_file\": \"{}\",\n",
            "  \"image_file\": \"{}\",\n",
            "  \"isosurface_file\": {},\n",
            "  \"clipped_isosurface_file\": {},\n",
            "  \"mesh_format\": \"binary_little_endian_ply\",\n",
            "  \"image_format\": \"bmp-r-field-g-occupancy-slice-atlas\",\n",
            "  \"image_grid\": [{}, {}],\n",
            "  \"image_size_px\": [{}, {}],\n",
            "  \"dimensions\": [{}, {}, {}],\n",
            "  \"voxel_size_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"padding_voxels\": {},\n",
            "  \"field_enabled\": {},\n",
            "  \"field_rate\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"field_extension_voxels\": {},\n",
            "  \"iso_spacing\": {},\n",
            "  \"field_max_distance\": {},\n",
            "  \"origin_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"actual_size_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"model_bounds_min_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"model_bounds_max_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"occupied_voxels\": {},\n",
            "  \"total_voxels\": {}\n",
            "}}\n"
        ),
        json_escape(document.metadata.input),
        json_escape(&document.volume_path.display().to_string()),
        json_escape(&document.image_path.display().to_string()),
        path_json(document.mesh_path),
        path_json(document.clipped_mesh_path),
        document.atlas.columns,
        document.atlas.rows,
        document.atlas.width,
        document.atlas.height,
        document.grid.dims[0],
        document.grid.dims[1],
        document.grid.dims[2],
        document.metadata.voxel_size.x,
        document.metadata.voxel_size.y,
        document.metadata.voxel_size.z,
        document.metadata.padding_voxels,
        document.metadata.field_enabled,
        document.metadata.field_rate.x,
        document.metadata.field_rate.y,
        document.metadata.field_rate.z,
        if document.metadata.field_enabled {
            document.metadata.field_extension_voxels
        } else {
            0
        },
        if document.metadata.field_enabled {
            format!("{:.9}", document.metadata.iso_spacing)
        } else {
            "null".to_string()
        },
        field_max_distance_json(document.field),
        document.grid.origin.x,
        document.grid.origin.y,
        document.grid.origin.z,
        document.grid.actual_size.x,
        document.grid.actual_size.y,
        document.grid.actual_size.z,
        document.bounds.min.x,
        document.bounds.min.y,
        document.bounds.min.z,
        document.bounds.max.x,
        document.bounds.max.y,
        document.bounds.max.z,
        document.occupied_count,
        document.voxel_count,
    )
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn path_json(path: Option<&Path>) -> String {
    path.map(|path| format!("\"{}\"", json_escape(&path.display().to_string())))
        .unwrap_or_else(|| "null".to_string())
}

fn field_max_distance_json(field: Option<&Field>) -> String {
    field
        .map(|field| format!("{:.9}", field.max_distance))
        .unwrap_or_else(|| "null".to_string())
}
