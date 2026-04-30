use crate::geometry::{Bounds, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub origin: Vec3,
    pub dims: [usize; 3],
    pub voxel_size: Vec3,
    pub actual_size: Vec3,
}

impl Grid {
    pub fn voxel_count(self) -> usize {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    pub fn slice_len(self) -> usize {
        self.dims[0] * self.dims[1]
    }

    pub fn index(self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.dims[0] + z * self.slice_len()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridSpec {
    pub voxel_size: Vec3,
    pub requested_size: Option<Vec3>,
    pub padding_voxels: usize,
    pub origin: Option<Vec3>,
}

pub fn build_grid(spec: GridSpec, bounds: Bounds) -> Result<Grid, String> {
    let model_size = bounds.max.sub(bounds.min);
    let padding_size = Vec3 {
        x: spec.padding_voxels as f64 * spec.voxel_size.x * 2.0,
        y: spec.padding_voxels as f64 * spec.voxel_size.y * 2.0,
        z: spec.padding_voxels as f64 * spec.voxel_size.z * 2.0,
    };
    let padded_size = Vec3 {
        x: model_size.x + padding_size.x,
        y: model_size.y + padding_size.y,
        z: model_size.z + padding_size.z,
    };
    let size = if let Some(maximum_size) = spec.requested_size {
        Vec3 {
            x: padded_size.x.min(maximum_size.x),
            y: padded_size.y.min(maximum_size.y),
            z: padded_size.z.min(maximum_size.z),
        }
    } else {
        padded_size
    };
    let dims = [
        ceil_to_usize(size.x / spec.voxel_size.x, "x dimension")?,
        ceil_to_usize(size.y / spec.voxel_size.y, "y dimension")?,
        ceil_to_usize(size.z / spec.voxel_size.z, "z dimension")?,
    ];
    let actual_size = Vec3 {
        x: dims[0] as f64 * spec.voxel_size.x,
        y: dims[1] as f64 * spec.voxel_size.y,
        z: dims[2] as f64 * spec.voxel_size.z,
    };
    let model_center = Vec3 {
        x: (bounds.min.x + bounds.max.x) * 0.5,
        y: (bounds.min.y + bounds.max.y) * 0.5,
        z: (bounds.min.z + bounds.max.z) * 0.5,
    };
    let origin = spec.origin.unwrap_or(Vec3 {
        x: model_center.x - actual_size.x * 0.5,
        y: model_center.y - actual_size.y * 0.5,
        z: model_center.z - actual_size.z * 0.5,
    });

    Ok(Grid {
        origin,
        dims,
        voxel_size: spec.voxel_size,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn padded_grid_shrinks_below_requested_maximum() {
        let spec = GridSpec {
            voxel_size: v(1.0, 1.0, 1.0),
            requested_size: Some(v(128.0, 128.0, 128.0)),
            padding_voxels: 3,
            origin: None,
        };
        let bounds = Bounds {
            min: v(0.0, 0.0, 0.0),
            max: v(10.0, 10.0, 10.0),
        };

        let grid = build_grid(spec, bounds).unwrap();

        assert_eq!(grid.dims, [16, 16, 16]);
        assert_eq!(grid.origin.x, -3.0);
        assert_eq!(grid.origin.y, -3.0);
        assert_eq!(grid.origin.z, -3.0);
        assert_eq!(grid.actual_size.x, 16.0);
        assert_eq!(grid.actual_size.y, 16.0);
        assert_eq!(grid.actual_size.z, 16.0);
    }

    #[test]
    fn padded_grid_clamps_to_requested_maximum() {
        let spec = GridSpec {
            voxel_size: v(0.4, 0.4, 0.4),
            requested_size: Some(v(100.0, 100.0, 100.0)),
            padding_voxels: 3,
            origin: None,
        };
        let bounds = Bounds {
            min: v(0.0, 0.0, 0.0),
            max: v(120.0, 120.0, 120.0),
        };

        let grid = build_grid(spec, bounds).unwrap();

        assert_eq!(grid.dims, [250, 250, 250]);
        assert_eq!(grid.actual_size.x, 100.0);
        assert_eq!(grid.actual_size.y, 100.0);
        assert_eq!(grid.actual_size.z, 100.0);
        assert_eq!(grid.origin.x, 10.0);
        assert_eq!(grid.origin.y, 10.0);
        assert_eq!(grid.origin.z, 10.0);
    }

    #[test]
    fn explicit_origin_is_respected_when_grid_clamps() {
        let spec = GridSpec {
            voxel_size: v(1.0, 1.0, 1.0),
            requested_size: Some(v(100.0, 100.0, 100.0)),
            padding_voxels: 3,
            origin: Some(v(-20.0, -10.0, 5.0)),
        };
        let bounds = Bounds {
            min: v(0.0, 0.0, 0.0),
            max: v(120.0, 120.0, 120.0),
        };

        let grid = build_grid(spec, bounds).unwrap();

        assert_eq!(grid.dims, [100, 100, 100]);
        assert_eq!(grid.origin.x, -20.0);
        assert_eq!(grid.origin.y, -10.0);
        assert_eq!(grid.origin.z, 5.0);
    }
}
