# Parameterize G-code Material Settings

## Goal

Move hardcoded temperature/fan values into structured settings and use them during G-code emission.

## Rationale

The current preamble is hardcoded. v1 should expose structured material settings without allowing arbitrary start/end G-code yet.

## Scope

- Use `material.bed_temperature` for `M190`.
- Use `material.nozzle_temperature` for `M109`.
- Use `material.fan_speed_percent` to emit fan control:
  - `M106 S...` for values above zero;
  - `M107` for zero.
- Keep prime extrusion hardcoded for now.
- Keep homing/lift/start/end templates controlled by code.
- Apply `printer.global_z_offset` at G-code emission time only.

## Out Of Scope

- Arbitrary start/end G-code.
- Prime-line settings.
- Firmware profiles beyond current Marlin flavor.

## Verification

- Unit tests for temperature substitution.
- Unit tests for fan percent to `S0..255` mapping.
- Unit tests for global Z offset affecting emitted Z only.
- `cargo test --manifest-path field-gen/Cargo.toml`
