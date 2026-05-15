# Expose Basic GUI Settings

## Goal

Add hand-written controls for the initial slicer settings needed for normal use.

## Rationale

The GUI should edit the real `SlicerSettings` object directly. A generic settings model is unnecessary at this stage.

## Scope

- Add controls for:
  - layer height;
  - voxel size;
  - print volume;
  - wall count;
  - infill percentage;
  - filament diameter;
  - nozzle temperature;
  - bed temperature;
  - fan speed;
  - global Z offset;
  - printhead obstruction height and angle.
- Validate through `SlicerSettings::validate`.
- Save edited settings through `shockwave-config`.
- Keep field/kernel advanced settings hidden or read-only unless needed.

## Out Of Scope

- Generic dynamic settings UI.
- Preset/profile management.
- Arbitrary start/end G-code editing.

## Verification

- Edited settings persist to JSON.
- Invalid values are surfaced without crashing.
- `cargo test --manifest-path field-gen/Cargo.toml`
