use crate::geometry::{Triangle, Vec3};

pub fn parse_stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
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
