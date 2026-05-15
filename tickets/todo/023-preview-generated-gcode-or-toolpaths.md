# Preview Generated G-code Or Toolpaths

## Goal

Preview generated paths after slicing.

## Rationale

The eventual slicer needs visual inspection of emitted paths. For the first version, the preview should use the most structured data available rather than reparsing raw G-code if possible.

## Scope

- Prefer previewing `LayerToolpaths` from the slicer job output.
- If necessary, add structured path data to the shared slice job output.
- Render per-layer or all-layer toolpaths in the canvas.
- Add basic layer navigation if it is low-cost.
- Keep G-code file emission unchanged.

## Out Of Scope

- Full G-code simulator.
- Extrusion width rendering.
- Time estimates.
- Material/temperature visualization.

## Verification

- A sliced model shows generated paths in the preview.
- Preview path count roughly matches generated layer/toolpath counts.
- `cargo test --manifest-path field-gen/Cargo.toml`
