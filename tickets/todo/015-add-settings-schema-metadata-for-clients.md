# Add Settings Schema Metadata For Clients

## Goal

Add lightweight metadata for slicer-facing settings so clients can present names, units, and basic ranges without duplicating that knowledge.

## Rationale

The JSON settings schema is the single source of configuration. A future local app should not maintain a separate hardcoded map of units and basic validation hints.

## Scope

- Add metadata helpers in or near `shockwave-config`.
- Cover slicer-facing settings such as:
  - layer height;
  - voxel size;
  - print volume;
  - temperatures;
  - fan speed;
  - wall count;
  - infill percentage;
  - global Z offset;
  - obstruction parameters.
- Include stable setting paths matching the JSON structure.
- Include units and basic min/max/default display hints where useful.
- Keep validation logic authoritative in existing settings validation.

## Out Of Scope

- Dynamic GUI generation.
- Tauri bindings.
- User profile or recent-file state.

## Verification

- Tests that metadata paths correspond to real serialized settings paths.
- Tests that default values still serialize and parse correctly.
- `cargo test --manifest-path field-gen/Cargo.toml`
