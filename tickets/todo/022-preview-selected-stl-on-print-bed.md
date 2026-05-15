# Preview Selected STL On Print Bed

## Goal

Render the selected STL model on a simple print bed preview.

## Rationale

Users need visual confirmation that they selected the correct model and that its model-space position makes sense before slicing.

## Scope

- Load triangles for preview using existing STL parsing code.
- Render the model in the GUI canvas.
- Render a basic print bed sized from settings.
- Add simple camera controls if practical:
  - orbit;
  - pan;
  - zoom.
- Keep preview independent from running the slicer.

## Out Of Scope

- Model transform/edit controls.
- Field visualization.
- G-code preview.
- Slicing result preview.

## Verification

- Selecting an STL updates the preview.
- Print bed dimensions follow current settings.
- `cargo test --manifest-path field-gen/Cargo.toml`
