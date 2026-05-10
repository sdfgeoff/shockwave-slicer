use shockwave_core::geometry::Vec3;
use shockwave_geodesic::GeodesicField;
use shockwave_mesh::Mesh;

#[derive(Clone, Copy, Debug)]
pub struct PathPoint {
    pub position: Vec3,
    pub extrusion_width_mm: f64,
    pub layer_height_mm: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Toolpath {
    pub points: Vec<PathPoint>,
    pub role: ToolpathRole,
    pub closed: bool,
}

impl Toolpath {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToolpathRole {
    #[default]
    Perimeter,
    Infill,
    Travel,
}

#[derive(Clone, Debug, Default)]
pub struct LayerToolpaths {
    pub field_value: f64,
    pub paths: Vec<Toolpath>,
}

impl LayerToolpaths {
    pub fn path_count(&self) -> usize {
        self.paths.len()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ContourOptions {
    pub extrusion_width_mm: f64,
    pub layer_height_mm: f64,
    pub join_tolerance_mm: f64,
}

impl Default for ContourOptions {
    fn default() -> Self {
        Self {
            extrusion_width_mm: 0.4,
            layer_height_mm: 0.2,
            join_tolerance_mm: 1.0e-6,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    a: Vec3,
    b: Vec3,
}

pub fn perimeter_toolpaths_from_distance(
    mesh: &Mesh,
    distance: &GeodesicField,
    offsets_mm: &[f64],
    options: ContourOptions,
) -> Result<Vec<Toolpath>, String> {
    let mut paths = Vec::new();
    for offset in offsets_mm {
        paths.extend(contour_toolpaths(
            mesh,
            &distance.distances,
            *offset,
            ToolpathRole::Perimeter,
            options,
        )?);
    }

    Ok(paths)
}

pub fn contour_toolpaths(
    mesh: &Mesh,
    values: &[f64],
    iso_value: f64,
    role: ToolpathRole,
    options: ContourOptions,
) -> Result<Vec<Toolpath>, String> {
    if values.len() != mesh.vertices.len() {
        return Err("scalar field length does not match mesh vertex count".to_string());
    }
    if !iso_value.is_finite() {
        return Err("contour value must be finite".to_string());
    }
    if options.extrusion_width_mm <= 0.0 || options.layer_height_mm <= 0.0 {
        return Err("contour extrusion width and layer height must be positive".to_string());
    }

    let mut segments = Vec::new();
    for triangle in &mesh.triangles {
        for vertex in triangle {
            if *vertex >= mesh.vertices.len() {
                return Err("mesh triangle references an out-of-bounds vertex".to_string());
            }
        }

        if let Some(segment) = triangle_contour_segment(mesh, values, *triangle, iso_value) {
            segments.push(segment);
        }
    }

    Ok(join_segments(segments, role, options))
}

fn triangle_contour_segment(
    mesh: &Mesh,
    values: &[f64],
    triangle: [usize; 3],
    iso_value: f64,
) -> Option<Segment> {
    let edges = [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
    ];
    let mut points = Vec::new();

    for [a, b] in edges {
        let value_a = values[a];
        let value_b = values[b];
        if !value_a.is_finite() || !value_b.is_finite() || value_a == value_b {
            continue;
        }

        let min_value = value_a.min(value_b);
        let max_value = value_a.max(value_b);
        if iso_value < min_value || iso_value >= max_value {
            continue;
        }

        let t = (iso_value - value_a) / (value_b - value_a);
        points.push(lerp(mesh.vertices[a], mesh.vertices[b], t));
    }

    dedup_points(&mut points);
    (points.len() == 2).then_some(Segment {
        a: points[0],
        b: points[1],
    })
}

fn join_segments(
    segments: Vec<Segment>,
    role: ToolpathRole,
    options: ContourOptions,
) -> Vec<Toolpath> {
    let mut paths: Vec<Vec<Vec3>> = Vec::new();

    for segment in segments {
        let mut merged = false;
        for path in &mut paths {
            if points_close(*path.last().unwrap(), segment.a, options.join_tolerance_mm) {
                path.push(segment.b);
                merged = true;
                break;
            }
            if points_close(*path.last().unwrap(), segment.b, options.join_tolerance_mm) {
                path.push(segment.a);
                merged = true;
                break;
            }
            if points_close(path[0], segment.b, options.join_tolerance_mm) {
                path.insert(0, segment.a);
                merged = true;
                break;
            }
            if points_close(path[0], segment.a, options.join_tolerance_mm) {
                path.insert(0, segment.b);
                merged = true;
                break;
            }
        }

        if !merged {
            paths.push(vec![segment.a, segment.b]);
        }
    }

    paths
        .into_iter()
        .filter(|points| points.len() >= 2)
        .map(|points| {
            let closed = points_close(
                points[0],
                *points.last().unwrap(),
                options.join_tolerance_mm,
            );
            Toolpath {
                points: points
                    .into_iter()
                    .map(|position| PathPoint {
                        position,
                        extrusion_width_mm: options.extrusion_width_mm,
                        layer_height_mm: options.layer_height_mm,
                    })
                    .collect(),
                role,
                closed,
            }
        })
        .collect()
}

fn dedup_points(points: &mut Vec<Vec3>) {
    let mut index = 0;
    while index < points.len() {
        let duplicate = points[..index]
            .iter()
            .any(|point| points_close(*point, points[index], 1.0e-9));
        if duplicate {
            points.swap_remove(index);
        } else {
            index += 1;
        }
    }
}

fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    Vec3 {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        z: a.z + (b.z - a.z) * t,
    }
}

fn points_close(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dz = b.z - a.z;
    dx * dx + dy * dy + dz * dz <= tolerance * tolerance
}

#[cfg(test)]
mod tests {
    use shockwave_geodesic::GeodesicField;
    use shockwave_mesh::Mesh;

    use super::*;

    fn vertex(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn extracts_contour_polyline_from_scalar_field() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(1.0, 0.0, 0.0),
                vertex(1.0, 1.0, 0.0),
                vertex(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let values = vec![0.0, 1.0, 1.0, 0.0];

        let paths = contour_toolpaths(
            &mesh,
            &values,
            0.5,
            ToolpathRole::Perimeter,
            ContourOptions::default(),
        )
        .unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].points.len(), 3);
        for point in &paths[0].points {
            assert!((point.position.x - 0.5).abs() < 1.0e-9);
        }
    }

    #[test]
    fn creates_perimeters_from_geodesic_distance_offsets() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(1.0, 0.0, 0.0),
                vertex(1.0, 1.0, 0.0),
                vertex(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let distance = GeodesicField {
            distances: vec![0.0, 1.0, 1.0, 0.0],
        };

        let paths =
            perimeter_toolpaths_from_distance(&mesh, &distance, &[0.5], ContourOptions::default())
                .unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].role, ToolpathRole::Perimeter);
    }
}
