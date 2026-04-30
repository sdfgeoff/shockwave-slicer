//! Isosurface extraction for propagated voxel fields.
//!
//! Field values are sampled at voxel centers. The surface-net extractor uses
//! neighboring voxel-center samples as the corners of each interpolation cell,
//! then places vertices in model-space millimeter coordinates.

mod interpolate;
pub mod mesh;
mod surface_nets;

pub use mesh::{Isosurface, IsosurfaceSet, Mesh};
pub use surface_nets::extract_regular_isosurfaces;
