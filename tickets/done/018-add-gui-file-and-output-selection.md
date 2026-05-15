# Add GUI File And Output Selection

## Goal

Allow the user to select an STL file and choose where generated output should be written.

## Rationale

The first useful slicer workflow needs explicit input and output paths before slicing can be wired in.

## Scope

- Add an STL file picker.
- Add an output prefix or export destination selector.
- Add disabled/enabled state for Slice based on valid input/output paths.
- Add a Save/Export button or dialog placeholder that matches the intended workflow.
- Use a native file dialog crate if that is the simplest practical route.

## Out Of Scope

- Running the slicer.
- Model preview.
- G-code preview.
- Recent files.

## Verification

- Selected input and output paths are displayed in the UI.
- Slice action remains disabled until required paths are available.
- `cargo test --manifest-path field-gen/Cargo.toml`
