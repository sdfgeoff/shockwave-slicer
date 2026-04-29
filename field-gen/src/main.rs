use std::env;
use std::fs;
use std::path::PathBuf;

const EPSILON: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
            z: self.z.max(other.z),
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

#[derive(Clone, Debug)]
struct Triangle {
    vertices: [Vec3; 3],
}

#[derive(Clone, Copy, Debug)]
struct Vec2 {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct Segment2 {
    a: Vec2,
    b: Vec2,
}

#[derive(Clone, Debug)]
struct Config {
    input: PathBuf,
    output_prefix: PathBuf,
    voxel_size: Vec3,
    requested_size: Option<Vec3>,
    origin: Option<Vec3>,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct Grid {
    origin: Vec3,
    dims: [usize; 3],
    voxel_size: Vec3,
    actual_size: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct Atlas {
    columns: usize,
    rows: usize,
    width: usize,
    height: usize,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = parse_args(env::args().skip(1).collect())?;
    let bytes = fs::read(&config.input)
        .map_err(|error| format!("failed to read {}: {error}", config.input.display()))?;
    let triangles = parse_stl(&bytes)?;

    if triangles.is_empty() {
        return Err("STL did not contain any triangles".to_string());
    }

    let bounds = mesh_bounds(&triangles);
    let grid = build_grid(&config, bounds)?;
    let occupancy = generate_occupancy(&triangles, grid);
    let atlas = build_atlas(grid);
    let occupied_count = occupancy.iter().filter(|value| **value != 0).count();

    let volume_path = config.output_prefix.with_extension("occ");
    let image_path = config.output_prefix.with_extension("bmp");
    let metadata_path = config.output_prefix.with_extension("json");
    fs::write(&volume_path, &occupancy)
        .map_err(|error| format!("failed to write {}: {error}", volume_path.display()))?;
    write_occupancy_bmp(&image_path, &occupancy, grid, atlas)
        .map_err(|error| format!("failed to write {}: {error}", image_path.display()))?;
    fs::write(
        &metadata_path,
        metadata_json(
            &config,
            bounds,
            grid,
            atlas,
            &volume_path,
            &image_path,
            occupied_count,
            occupancy.len(),
        ),
    )
    .map_err(|error| format!("failed to write {}: {error}", metadata_path.display()))?;

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
    println!("Wrote {}", volume_path.display());
    println!("Wrote {}", image_path.display());
    println!("Wrote {}", metadata_path.display());

    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Config, String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Err(usage());
    }

    let input = PathBuf::from(&args[0]);
    let mut output_prefix = PathBuf::from("occupancy");
    let mut voxel_size = None;
    let mut requested_size = None;
    let mut origin = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--voxel" => {
                voxel_size = Some(parse_vec3_flag("--voxel", &args, &mut index)?);
            }
            "--size" => {
                requested_size = Some(parse_vec3_flag("--size", &args, &mut index)?);
            }
            "--origin" => {
                origin = Some(parse_vec3_flag("--origin", &args, &mut index)?);
            }
            "--output" | "-o" => {
                index += 1;
                output_prefix = args
                    .get(index)
                    .map(PathBuf::from)
                    .ok_or_else(|| "--output requires a path".to_string())?;
            }
            flag => {
                return Err(format!("unknown argument `{flag}`\n\n{}", usage()));
            }
        }
        index += 1;
    }

    let voxel_size = voxel_size.ok_or_else(|| "--voxel x y z is required".to_string())?;
    validate_positive_vec3("--voxel", voxel_size)?;
    if let Some(size) = requested_size {
        validate_positive_vec3("--size", size)?;
    }

    Ok(Config {
        input,
        output_prefix,
        voxel_size,
        requested_size,
        origin,
    })
}

fn parse_vec3_flag(flag: &str, args: &[String], index: &mut usize) -> Result<Vec3, String> {
    let start = *index + 1;
    let values = args
        .get(start..start + 3)
        .ok_or_else(|| format!("{flag} requires three numeric values"))?;
    *index += 3;

    Ok(Vec3 {
        x: values[0]
            .parse()
            .map_err(|_| format!("{flag} x value must be numeric"))?,
        y: values[1]
            .parse()
            .map_err(|_| format!("{flag} y value must be numeric"))?,
        z: values[2]
            .parse()
            .map_err(|_| format!("{flag} z value must be numeric"))?,
    })
}

fn validate_positive_vec3(name: &str, value: Vec3) -> Result<(), String> {
    if value.x <= 0.0 || value.y <= 0.0 || value.z <= 0.0 {
        return Err(format!("{name} values must be greater than zero"));
    }
    Ok(())
}

fn usage() -> String {
    "usage: field-gen <input.stl> --voxel <x-mm> <y-mm> <z-mm> [--size <x-mm> <y-mm> <z-mm>] [--origin <x-mm> <y-mm> <z-mm>] [--output <prefix>]\n\
\n\
STL coordinates are assumed to be millimeters. If --size is omitted, the grid fits the STL bounds.\n\
Voxel size takes priority: grid dimensions are ceil(size / voxel), so actual size may expand."
        .to_string()
}

fn parse_stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    if let Some(triangles) = parse_binary_stl(bytes)? {
        return Ok(triangles);
    }

    parse_ascii_stl(bytes)
}

fn parse_binary_stl(bytes: &[u8]) -> Result<Option<Vec<Triangle>>, String> {
    if bytes.len() < 84 {
        return Ok(None);
    }

    let triangle_count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let expected_len = 84usize
        .checked_add(
            triangle_count
                .checked_mul(50)
                .ok_or_else(|| "binary STL triangle count is too large".to_string())?,
        )
        .ok_or_else(|| "binary STL size overflow".to_string())?;

    if expected_len != bytes.len() {
        return Ok(None);
    }

    let mut triangles = Vec::with_capacity(triangle_count);
    let mut offset = 84;
    for _ in 0..triangle_count {
        offset += 12;
        let v0 = read_binary_vertex(bytes, offset)?;
        let v1 = read_binary_vertex(bytes, offset + 12)?;
        let v2 = read_binary_vertex(bytes, offset + 24)?;
        triangles.push(Triangle {
            vertices: [v0, v1, v2],
        });
        offset += 38;
    }

    Ok(Some(triangles))
}

fn read_binary_vertex(bytes: &[u8], offset: usize) -> Result<Vec3, String> {
    let read_f32 = |start: usize| -> Result<f64, String> {
        let value = bytes
            .get(start..start + 4)
            .ok_or_else(|| "binary STL ended unexpectedly".to_string())?;
        Ok(f32::from_le_bytes(value.try_into().unwrap()) as f64)
    };

    Ok(Vec3 {
        x: read_f32(offset)?,
        y: read_f32(offset + 4)?,
        z: read_f32(offset + 8)?,
    })
}

fn parse_ascii_stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "STL is neither valid binary STL nor UTF-8 ASCII STL".to_string())?;
    let mut vertices = Vec::new();

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() != Some("vertex") {
            continue;
        }

        let x = parse_ascii_coord(parts.next(), "x")?;
        let y = parse_ascii_coord(parts.next(), "y")?;
        let z = parse_ascii_coord(parts.next(), "z")?;
        vertices.push(Vec3 { x, y, z });
    }

    if vertices.len() % 3 != 0 {
        return Err("ASCII STL has a vertex count that is not divisible by 3".to_string());
    }

    Ok(vertices
        .chunks_exact(3)
        .map(|chunk| Triangle {
            vertices: [chunk[0], chunk[1], chunk[2]],
        })
        .collect())
}

fn parse_ascii_coord(value: Option<&str>, axis: &str) -> Result<f64, String> {
    value
        .ok_or_else(|| format!("ASCII STL vertex missing {axis} coordinate"))?
        .parse()
        .map_err(|_| format!("ASCII STL vertex has invalid {axis} coordinate"))
}

fn mesh_bounds(triangles: &[Triangle]) -> Bounds {
    let mut min = triangles[0].vertices[0];
    let mut max = triangles[0].vertices[0];

    for triangle in triangles {
        for vertex in triangle.vertices {
            min = min.min(vertex);
            max = max.max(vertex);
        }
    }

    Bounds { min, max }
}

fn build_grid(config: &Config, bounds: Bounds) -> Result<Grid, String> {
    let model_size = bounds.max.sub(bounds.min);
    let requested_size = config.requested_size.unwrap_or(model_size);
    let size = Vec3 {
        x: requested_size.x.max(model_size.x),
        y: requested_size.y.max(model_size.y),
        z: requested_size.z.max(model_size.z),
    };
    let dims = [
        ceil_to_usize(size.x / config.voxel_size.x, "x dimension")?,
        ceil_to_usize(size.y / config.voxel_size.y, "y dimension")?,
        ceil_to_usize(size.z / config.voxel_size.z, "z dimension")?,
    ];
    let actual_size = Vec3 {
        x: dims[0] as f64 * config.voxel_size.x,
        y: dims[1] as f64 * config.voxel_size.y,
        z: dims[2] as f64 * config.voxel_size.z,
    };
    let model_center = Vec3 {
        x: (bounds.min.x + bounds.max.x) * 0.5,
        y: (bounds.min.y + bounds.max.y) * 0.5,
        z: (bounds.min.z + bounds.max.z) * 0.5,
    };
    let origin = config.origin.unwrap_or(Vec3 {
        x: model_center.x - actual_size.x * 0.5,
        y: model_center.y - actual_size.y * 0.5,
        z: model_center.z - actual_size.z * 0.5,
    });

    Ok(Grid {
        origin,
        dims,
        voxel_size: config.voxel_size,
        actual_size,
    })
}

fn ceil_to_usize(value: f64, label: &str) -> Result<usize, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{label} is invalid"));
    }

    let ceiled = value.ceil();
    if ceiled > usize::MAX as f64 {
        return Err(format!("{label} is too large"));
    }
    Ok(ceiled as usize)
}

fn generate_occupancy(triangles: &[Triangle], grid: Grid) -> Vec<u8> {
    let voxel_count = grid.dims[0] * grid.dims[1] * grid.dims[2];
    let mut occupancy = vec![0; voxel_count];

    for z in 0..grid.dims[2] {
        let z_position = grid.origin.z + (z as f64 + 0.5) * grid.voxel_size.z;
        let segments = slice_segments(triangles, z_position);

        for y in 0..grid.dims[1] {
            let y_position = grid.origin.y + (y as f64 + 0.5) * grid.voxel_size.y;
            let mut crossings = row_crossings(&segments, y_position);
            if crossings.len() < 2 {
                continue;
            }

            crossings.sort_by(|a, b| a.total_cmp(b));
            dedupe_sorted_f64(&mut crossings, 1.0e-7);

            for interval in crossings.chunks_exact(2) {
                let left = interval[0].min(interval[1]);
                let right = interval[0].max(interval[1]);
                let start_x = voxel_index_at_or_after(left, grid.origin.x, grid.voxel_size.x);
                let end_x = voxel_index_before(right, grid.origin.x, grid.voxel_size.x);
                let start_x = start_x.min(grid.dims[0]);
                let end_x = end_x.min(grid.dims[0]);

                for x in start_x..end_x {
                    let index = x + y * grid.dims[0] + z * grid.dims[0] * grid.dims[1];
                    occupancy[index] = 255;
                }
            }
        }
    }

    occupancy
}

fn slice_segments(triangles: &[Triangle], z: f64) -> Vec<Segment2> {
    triangles
        .iter()
        .filter_map(|triangle| triangle_z_intersection(triangle, z))
        .collect()
}

fn triangle_z_intersection(triangle: &Triangle, z: f64) -> Option<Segment2> {
    let vertices = triangle.vertices;
    let edges = [
        (vertices[0], vertices[1]),
        (vertices[1], vertices[2]),
        (vertices[2], vertices[0]),
    ];
    let mut points = Vec::with_capacity(2);

    for (a, b) in edges {
        let a_offset = a.z - z;
        let b_offset = b.z - z;

        if a_offset.abs() <= EPSILON && b_offset.abs() <= EPSILON {
            continue;
        }

        if (a_offset <= EPSILON && b_offset > EPSILON)
            || (b_offset <= EPSILON && a_offset > EPSILON)
        {
            let t = (z - a.z) / (b.z - a.z);
            points.push(Vec2 {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            });
        }
    }

    dedupe_vec2(&mut points, 1.0e-8);

    if points.len() == 2 && distance_squared_2d(points[0], points[1]) > 1.0e-16 {
        Some(Segment2 {
            a: points[0],
            b: points[1],
        })
    } else {
        None
    }
}

fn row_crossings(segments: &[Segment2], y: f64) -> Vec<f64> {
    let mut crossings = Vec::new();

    for segment in segments {
        let y0 = segment.a.y;
        let y1 = segment.b.y;

        if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
            let t = (y - y0) / (y1 - y0);
            crossings.push(segment.a.x + (segment.b.x - segment.a.x) * t);
        }
    }

    crossings
}

fn voxel_index_at_or_after(position: f64, origin: f64, voxel_size: f64) -> usize {
    let value = ((position - origin) / voxel_size - 0.5).ceil();
    if value <= 0.0 { 0 } else { value as usize }
}

fn voxel_index_before(position: f64, origin: f64, voxel_size: f64) -> usize {
    let value = ((position - origin) / voxel_size - 0.5).ceil();
    if value <= 0.0 { 0 } else { value as usize }
}

fn dedupe_sorted_f64(values: &mut Vec<f64>, epsilon: f64) {
    let mut write_index = 0;
    for read_index in 0..values.len() {
        if write_index == 0 || (values[read_index] - values[write_index - 1]).abs() > epsilon {
            values[write_index] = values[read_index];
            write_index += 1;
        }
    }
    values.truncate(write_index);
}

fn dedupe_vec2(points: &mut Vec<Vec2>, epsilon: f64) {
    let mut unique = Vec::with_capacity(points.len());

    for point in points.iter().copied() {
        if !unique
            .iter()
            .any(|existing| distance_squared_2d(point, *existing) <= epsilon * epsilon)
        {
            unique.push(point);
        }
    }

    *points = unique;
}

fn distance_squared_2d(a: Vec2, b: Vec2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn build_atlas(grid: Grid) -> Atlas {
    let columns = (grid.dims[2] as f64).sqrt().ceil() as usize;
    let rows = grid.dims[2].div_ceil(columns);

    Atlas {
        columns,
        rows,
        width: grid.dims[0] * columns,
        height: grid.dims[1] * rows,
    }
}

fn write_occupancy_bmp(
    path: &PathBuf,
    occupancy: &[u8],
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
            let value = if z < grid.dims[2] {
                let voxel_index = x + y * grid.dims[0] + z * grid.dims[0] * grid.dims[1];
                occupancy[voxel_index]
            } else {
                0
            };

            bytes.push(value);
            bytes.push(value);
            bytes.push(value);
        }

        bytes.extend(std::iter::repeat_n(0, padding));
    }

    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn metadata_json(
    config: &Config,
    bounds: Bounds,
    grid: Grid,
    atlas: Atlas,
    volume_path: &PathBuf,
    image_path: &PathBuf,
    occupied_count: usize,
    voxel_count: usize,
) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"input\": \"{}\",\n",
            "  \"units\": \"mm\",\n",
            "  \"layout\": \"x-fastest-u8\",\n",
            "  \"occupancy_file\": \"{}\",\n",
            "  \"image_file\": \"{}\",\n",
            "  \"image_format\": \"bmp-grayscale-slice-atlas\",\n",
            "  \"image_grid\": [{}, {}],\n",
            "  \"image_size_px\": [{}, {}],\n",
            "  \"dimensions\": [{}, {}, {}],\n",
            "  \"voxel_size_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"origin_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"actual_size_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"model_bounds_min_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"model_bounds_max_mm\": [{:.9}, {:.9}, {:.9}],\n",
            "  \"occupied_voxels\": {},\n",
            "  \"total_voxels\": {}\n",
            "}}\n"
        ),
        json_escape(&config.input.display().to_string()),
        json_escape(&volume_path.display().to_string()),
        json_escape(&image_path.display().to_string()),
        atlas.columns,
        atlas.rows,
        atlas.width,
        atlas.height,
        grid.dims[0],
        grid.dims[1],
        grid.dims[2],
        grid.voxel_size.x,
        grid.voxel_size.y,
        grid.voxel_size.z,
        grid.origin.x,
        grid.origin.y,
        grid.origin.z,
        grid.actual_size.x,
        grid.actual_size.y,
        grid.actual_size.z,
        bounds.min.x,
        bounds.min.y,
        bounds.min.z,
        bounds.max.x,
        bounds.max.y,
        bounds.max.z,
        occupied_count,
        voxel_count,
    )
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn grid(origin: Vec3, dims: [usize; 3], voxel_size: Vec3) -> Grid {
        Grid {
            origin,
            dims,
            voxel_size,
            actual_size: Vec3 {
                x: dims[0] as f64 * voxel_size.x,
                y: dims[1] as f64 * voxel_size.y,
                z: dims[2] as f64 * voxel_size.z,
            },
        }
    }

    #[test]
    fn cube_occupies_all_voxel_centers_inside_it() {
        let triangles = cube_triangles(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0));
        let occupancy = generate_occupancy(
            &triangles,
            grid(v(0.0, 0.0, 0.0), [2, 2, 2], v(5.0, 5.0, 5.0)),
        );

        assert_eq!(occupancy, vec![255; 8]);
    }

    #[test]
    fn cube_leaves_voxel_centers_outside_it_empty() {
        let triangles = cube_triangles(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0));
        let occupancy = generate_occupancy(
            &triangles,
            grid(v(-5.0, -5.0, -5.0), [4, 4, 4], v(5.0, 5.0, 5.0)),
        );
        let occupied_count = occupancy.iter().filter(|value| **value == 255).count();

        assert_eq!(occupied_count, 8);
    }

    #[test]
    fn grid_size_expands_to_voxel_multiple() {
        let config = Config {
            input: PathBuf::from("mesh.stl"),
            output_prefix: PathBuf::from("out"),
            voxel_size: v(0.4, 0.4, 0.4),
            requested_size: Some(v(100.0, 100.0, 100.0)),
            origin: None,
        };
        let bounds = Bounds {
            min: v(0.0, 0.0, 0.0),
            max: v(10.0, 10.0, 10.0),
        };

        let grid = build_grid(&config, bounds).unwrap();

        assert_eq!(grid.dims, [250, 250, 250]);
        assert_eq!(grid.actual_size.x, 100.0);
        assert_eq!(grid.actual_size.y, 100.0);
        assert_eq!(grid.actual_size.z, 100.0);
    }
}
