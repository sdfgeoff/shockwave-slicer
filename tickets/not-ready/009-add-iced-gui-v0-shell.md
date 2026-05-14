# Add Iced GUI v0 Shell

## Goal

Create `shockwave-slicer-gui` after the slicer/config/io refactor is usable, and wire a minimal desktop app around it.

## Rationale

The GUI should be a thin Rust application over shared slicer/config/io crates, not a second slicer implementation.

## Scope

- Add `shockwave-slicer-gui` as a separate crate in the current workspace.
- Use Iced.
- Load settings from OS config path.
- Save settings to OS config path.
- Let user select an STL file.
- Let user select output G-code path.
- Show basic settings controls:
  - layer height;
  - global Z offset;
  - nozzle temperature;
  - bed temperature;
  - fan speed;
  - bed width/depth/max height;
  - advanced field/obstruction settings.
- Run slicing in a background thread.
- Show one progress bar using equal phase weighting.
- Support cooperative cancellation.
- Include the minimal render widget/test triangle when `shockwave-render` exists.

## Important Boundaries

- Keep Iced abstractions out of `shockwave-config`, `shockwave-slicer`, and `shockwave-slicer-io`.
- Use background threads/channels for CPU-bound slicing work; do not rely on async as if slicing were IO-bound.
- Do not add recent-file/session state to slicing settings.

## Out Of Scope

- STL preview.
- G-code preview.
- Field preview.
- Placement controls.
- Bed-volume enforcement beyond visualization/warnings.
- Arbitrary start/end G-code.

## Verification

- `cargo run -p shockwave-slicer-gui` opens the app.
- Settings load/save roundtrip works.
- Selecting STL/output and slicing writes G-code and debug outputs.
- Cancel returns a cancelled state and avoids final partial G-code.
- `cargo test --manifest-path field-gen/Cargo.toml`
