# Shockwave Layers

Tools for generating and inspecting voxel fields from STL meshes.

## Voxel Atlas Image Format

`field-gen` writes a browser-loadable BMP slice atlas next to the raw occupancy file and JSON metadata.

Each atlas cell is one Z slice. Cells are arranged row-major:

```text
slice = cell_x + cell_y * image_grid_columns
```

Within each slice, voxel X maps to pixel X and voxel Y maps to pixel Y. The raw occupancy layout is `x-fastest-u8`:

```text
index = x + y * width + z * width * height
```

## Channel Layout

Generated BMP images use this channel convention:

```text
R: normalized propagated field value, 0..255
G: occupancy mask, 0 empty, 255 occupied
B: reserved, currently 0
A: not present in the 24-bit BMP output; viewers should treat pixels as opaque
```

When field generation is disabled, older/generated occupancy-only BMPs may have the same occupancy value in `R`, `G`, and `B`.

The field value is normalized by `field_max_distance` in the JSON metadata. This means the 8-bit BMP is convenient for preview and WebGL viewing, but it is not a high-precision field storage format. Future TIFF or raw higher-bit-depth output should preserve the same semantic channels unless the metadata says otherwise.

## Metadata

The JSON sidecar records the values needed to interpret the atlas:

```text
dimensions: [x, y, z] voxel dimensions
voxel_size_mm: physical voxel size in millimeters
origin_mm: grid origin in STL/model coordinates
image_grid: [columns, rows] atlas cell grid
image_size_px: atlas pixel size
field_enabled: whether R contains a propagated field
field_rate: anisotropic propagation rates used by field-gen
field_max_distance: value used to normalize R into 0..255
```

STL coordinates are assumed to be millimeters.

## Viewer Interpretation

`field-viewer` has a `Field + Occupancy` data mode matching the generated format. In 3D, it samples `R` as the field and uses `G` as the occupancy/validity mask. Field interpolation is occupancy-aware so empty voxels do not create false intermediate isosurfaces at mesh boundaries.
