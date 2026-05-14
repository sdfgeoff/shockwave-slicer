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

`field-gen` takes slicing settings from JSON:

```bash
field-gen input.stl --config field-gen/default-settings.json --output output/prefix
```

The native trapezoid field method uses the same SDF model as `field-gen/generate_trapezoid_sdf_kernel.py`, but samples it directly for the active voxel size instead of relying on a pre-generated JSON file. This avoids accidentally using a kernel authored for one voxel size at a different physical resolution.

To use an explicit growth kernel, set `field.kernel_path` in the slicer settings JSON:

```json
{
  "field": {
    "enabled": true,
    "voxel_size_mm": { "x": 0.4, "y": 0.4, "z": 0.4 },
    "method": "trapezoid",
    "anisotropic_rate": { "x": 3.7, "y": 3.7, "z": 1.0 },
    "kernel_path": "kernel.json"
  }
}
```

The kernel replaces method-based propagation. Its `moves` array defines directed graph edges from each voxel:

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

Field propagation also applies printer reachability constraints before isosurface extraction. These are configured in `printer.obstruction`:

```json
{
  "printer": {
    "obstruction": {
      "printhead_clearance_height_mm": 5.0,
      "printhead_clearance_angle_degrees": 55.0
    }
  }
}
```

`printhead_clearance_height_mm` prevents a voxel from being reached while any other unreached occupied voxel is more than the configured distance below it. `printhead_clearance_angle_degrees` reserves an upward cone above every unreached occupied voxel until that voxel is reached. The angle is measured from vertical. Use `0` to disable the cone constraint.

## Experimental G-code Output

`field-gen` can generate experimental Marlin G-code from the clipped isosurfaces:

```bash
field-gen input.stl --config field-gen/default-settings.json --output output/prefix
```

This currently computes mesh boundary distance on each clipped isosurface, extracts contour-parallel perimeter paths at bead centerline offsets, adds simple world-space grid infill alternating between +45 and -45 degrees by layer, and writes `output/prefix.gcode` when `output.gcode` is true. The number of walls, extrusion width, filament diameter, and infill percentage are configured in the JSON settings. G-code coordinates are shifted so the original STL/model bounds minimum maps to `X=0`, `Y=0`, and `Z=0`.

Extrusion height is estimated per path point from the propagated field gradient as `layer_height_mm / |grad(field)|`. G-code generation fails if the field gradient is undefined or non-finite at a path point; temporary pre-gradient path defaults use `slicing.layer_height_mm`. Travel between paths uses a basic Z-hop to the highest point on the current layer.

It does not yet generate Arachne-style bead variation, support material, robust travel optimization, or mature local non-planar layer-height compensation. Treat it as a pathing integration test, not printer-ready slicer output.

The Makefile generates G-code by default for STLs in `inputs/`:

```bash
make voxels
```

It uses `field-gen/default-settings.json` by default. Override with `make voxels CONFIG=path/to/settings.json`.

## Metadata

The JSON sidecar records the values needed to interpret the atlas:

```text
dimensions: [x, y, z] voxel dimensions
voxel_size_mm: physical voxel size in millimeters
origin_mm: grid origin in STL/model coordinates
image_grid: [columns, rows] atlas cell grid
image_size_px: atlas pixel size
field_enabled: whether R contains a propagated field
field_method: anisotropic, trapezoid, or explicit-kernel
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
