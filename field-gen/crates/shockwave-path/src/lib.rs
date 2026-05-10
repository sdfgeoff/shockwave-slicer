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

    pub fn length_mm(&self) -> f64 {
        let open_length: f64 = self
            .points
            .windows(2)
            .map(|segment| distance(segment[0].position, segment[1].position))
            .sum();
        if self.closed && self.points.len() > 2 {
            open_length
                + distance(
                    self.points[0].position,
                    self.points.last().unwrap().position,
                )
        } else {
            open_length
        }
    }

    pub fn estimated_volume_mm3(&self) -> f64 {
        let open_volume: f64 = self
            .points
            .windows(2)
            .map(|segment| segment_volume(segment[0], segment[1]))
            .sum();
        if self.closed && self.points.len() > 2 {
            open_volume + segment_volume(*self.points.last().unwrap(), self.points[0])
        } else {
            open_volume
        }
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

    pub fn estimated_volume_mm3(&self) -> f64 {
        self.paths.iter().map(Toolpath::estimated_volume_mm3).sum()
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

pub fn perimeter_toolpaths_from_boundary(
    mesh: &Mesh,
    offsets_mm: &[f64],
    options: ContourOptions,
) -> Result<Vec<Toolpath>, String> {
    let distance = shockwave_geodesic::distance_from_boundary(mesh)?;
    perimeter_toolpaths_from_distance(mesh, &distance, offsets_mm, options)
}

pub fn perimeter_layer_from_boundary(
    mesh: &Mesh,
    field_value: f64,
    offsets_mm: &[f64],
    options: ContourOptions,
) -> Result<LayerToolpaths, String> {
    if !field_value.is_finite() {
        return Err("layer field value must be finite".to_string());
    }

    Ok(LayerToolpaths {
        field_value,
        paths: perimeter_toolpaths_from_boundary(mesh, offsets_mm, options)?,
    })
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
        paths.push(vec![segment.a, segment.b]);
    }
    merge_paths(&mut paths, options.join_tolerance_mm);

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

fn merge_paths(paths: &mut Vec<Vec<Vec3>>, tolerance: f64) {
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for a_index in 0..paths.len() {
            for b_index in a_index + 1..paths.len() {
                if merge_path_pair(paths, a_index, b_index, tolerance) {
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
}

fn merge_path_pair(
    paths: &mut Vec<Vec<Vec3>>,
    a_index: usize,
    b_index: usize,
    tolerance: f64,
) -> bool {
    let a_first = paths[a_index][0];
    let a_last = *paths[a_index].last().unwrap();
    let b_first = paths[b_index][0];
    let b_last = *paths[b_index].last().unwrap();

    let mut b = if points_close(a_last, b_first, tolerance) {
        let mut path = paths.remove(b_index);
        path.remove(0);
        path
    } else if points_close(a_last, b_last, tolerance) {
        let mut path = paths.remove(b_index);
        path.reverse();
        path.remove(0);
        path
    } else if points_close(a_first, b_last, tolerance) {
        let mut path = paths.remove(b_index);
        path.pop();
        path.append(&mut paths[a_index]);
        paths[a_index] = path;
        return true;
    } else if points_close(a_first, b_first, tolerance) {
        let mut path = paths.remove(b_index);
        path.reverse();
        path.pop();
        path.append(&mut paths[a_index]);
        paths[a_index] = path;
        return true;
    } else {
        return false;
    };

    paths[a_index].append(&mut b);
    true
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

fn distance(a: Vec3, b: Vec3) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dz = b.z - a.z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn segment_volume(a: PathPoint, b: PathPoint) -> f64 {
    let width = (a.extrusion_width_mm + b.extrusion_width_mm) * 0.5;
    let height = (a.layer_height_mm + b.layer_height_mm) * 0.5;
    distance(a.position, b.position) * width * height
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

    #[test]
    fn computes_path_length_and_volume() {
        let path = Toolpath {
            points: vec![
                PathPoint {
                    position: vertex(0.0, 0.0, 0.0),
                    extrusion_width_mm: 0.4,
                    layer_height_mm: 0.2,
                },
                PathPoint {
                    position: vertex(3.0, 4.0, 0.0),
                    extrusion_width_mm: 0.4,
                    layer_height_mm: 0.2,
                },
            ],
            role: ToolpathRole::Perimeter,
            closed: false,
        };

        assert_eq!(path.length_mm(), 5.0);
        assert!((path.estimated_volume_mm3() - 0.4).abs() < 1.0e-9);
    }

    #[test]
    fn extracts_closed_perimeter_from_boundary_distance() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(2.0, 0.0, 0.0),
                vertex(2.0, 2.0, 0.0),
                vertex(0.0, 2.0, 0.0),
                vertex(1.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        };

        let paths =
            perimeter_toolpaths_from_boundary(&mesh, &[0.5], ContourOptions::default()).unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed);
        assert!(paths[0].length_mm() > 0.0);
    }

    #[test]
    fn builds_perimeter_layer_from_boundary_distance() {
        let mesh = Mesh {
            vertices: vec![
                vertex(0.0, 0.0, 0.0),
                vertex(2.0, 0.0, 0.0),
                vertex(2.0, 2.0, 0.0),
                vertex(0.0, 2.0, 0.0),
                vertex(1.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        };

        let layer =
            perimeter_layer_from_boundary(&mesh, 4.0, &[0.5], ContourOptions::default()).unwrap();

        assert_eq!(layer.field_value, 4.0);
        assert_eq!(layer.path_count(), 1);
        assert!(layer.estimated_volume_mm3() > 0.0);
    }
}
