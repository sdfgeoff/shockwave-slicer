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

When field generation is enabled, the propagated field is computed through occupied voxels first, then extended two voxels into neighboring empty space. The occupancy channel remains the authoritative object mask. The field extension exists so viewers can estimate derivatives at the occupancy boundary without sampling missing field values.

## Explicit Growth Kernels

`field-gen` can propagate the field with an explicit JSON kernel:

```bash
field-gen input.stl --voxel 1 1 1 --kernel kernel.json --iso-spacing 0.5 --output output/prefix
```

The kernel replaces `--field-rate` propagation and implies `--field`. Its `moves` array defines directed graph edges from each voxel:

```json
{
  "version": 1,
  "type": "explicit",
  "units": "voxels",
  "path_check": "swept_occupied",
  "moves": [
    { "offset": [1, 0, 0], "cost": 0.25 },
    { "offset": [0, 0, 1], "cost": 1.0 }
  ]
}
```

`offset` values are integer voxel offsets and `cost` is the field-distance added by that move. `path_check` may be `endpoint_occupied` or `swept_occupied`. With `swept_occupied`, long moves are rejected if the segment crosses empty voxels during the occupied-volume propagation pass.

## Propagation Constraints

Field propagation also applies printer reachability constraints before isosurface extraction:

```bash
field-gen input.stl --voxel 1 1 1 --kernel kernel.json \
  --max-unreached-below 5 \
  --unreached-cone-angle 80
```

`--max-unreached-below` prevents a voxel from being reached while any other unreached occupied voxel is more than the configured distance below it. `--unreached-cone-angle` reserves an upward cone above every unreached occupied voxel until that voxel is reached. The angle is measured from vertical; the default is `80` degrees.

## Metadata

The JSON sidecar records the values needed to interpret the atlas:

```text
dimensions: [x, y, z] voxel dimensions
voxel_size_mm: physical voxel size in millimeters
origin_mm: grid origin in STL/model coordinates
image_grid: [columns, rows] atlas cell grid
image_size_px: atlas pixel size
field_enabled: whether R contains a propagated field
field_method: anisotropic or explicit-kernel
kernel_file: source JSON kernel when field_method is explicit-kernel
field_rate: anisotropic propagation rates used by field-gen
max_unreached_below_mm: propagation height clearance limit
unreached_cone_angle_degrees: unreached-point access cone angle from vertical
field_extension_voxels: number of voxels the field was extended beyond occupancy
iso_spacing: distance between exported isosurface levels, when field output is enabled
field_max_distance: value used to normalize R into 0..255
```

STL coordinates are assumed to be millimeters.

## Viewer Interpretation

`field-viewer` has a `Field + Occupancy` data mode matching the generated format. In 3D, it samples `R` as the field, uses `G` as the occupancy clipping mask, and lights the rendered surface from the combined threshold/occupancy predicate.
