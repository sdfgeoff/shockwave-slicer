# Add `shockwave-config` For Persistent Slicer Settings

## Goal

Create `shockwave-config` when implementing persistent JSON settings and user-facing slicer configuration datatypes.

## Rationale

Settings should be shared by CLI, GUI, and slicer code. The persistent schema should use slicer-facing names, not internal CLI flag names.

## Scope

- Add `shockwave-config` crate to the workspace.
- Define nested settings structs:
  - `SlicerSettings`
  - `SlicingSettings`
  - `PrinterSettings`
  - `PrinterObstructionSettings`
  - `MaterialSettings`
  - `FieldSettings`
- Add defaults matching current intended behavior.
- Add OS config path resolution:
  - Linux target path: `~/.config/shockwave-slicer/settings.json`
  - Use platform config directories rather than hardcoding Linux paths.
- Add JSON load/save helpers.
- Add validation helpers for user-facing settings.
- Define provisional infill mapping:
  - `line_spacing_mm = extrusion_width_mm / (infill_percentage / 100)`
  - `0%` disables infill.

## Settings Decisions

- `layer_height` maps to current isosurface spacing.
- Field max size derives from printer print volume, not a duplicated field setting.
- `printhead_clearance_angle` and `printhead_clearance_height` live under `printer.obstruction`.
- `global_z_offset` is applied only at G-code emission time.
- Temperature and fan settings are structured values; arbitrary start/end G-code is not exposed yet.

## Out Of Scope

- GUI settings forms.
- CLI JSON override behavior, except as tests or API support needed by the next ticket.
- Scene/user recent files.

## Verification

- Unit tests for defaults.
- Unit tests for JSON roundtrip.
- Unit tests for validation errors.
- Unit tests for infill percentage mapping.
- `cargo test --manifest-path field-gen/Cargo.toml`
