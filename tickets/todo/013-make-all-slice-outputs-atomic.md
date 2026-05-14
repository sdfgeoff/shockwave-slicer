# Make All Slice Outputs Atomic

## Goal

Write every generated output through a temporary file and rename it into place only after successful completion.

## Rationale

G-code is already written atomically, but `.occ`, `.bmp`, `.json`, and optional `.ply` outputs are still written directly. A GUI cancellation or error should not leave convincing partial outputs at final paths.

## Scope

- Reuse or extend `write_atomically`.
- Apply atomic writes to:
  - occupancy `.occ`;
  - visualization `.bmp`;
  - metadata `.json`;
  - optional raw `.ply`;
  - optional clipped `.ply`;
  - G-code `.gcode`.
- Ensure temporary files are cleaned up on write errors.
- Keep final output names unchanged.

## Out Of Scope

- Output enable/disable settings beyond what already exists.
- Changing output formats.
- GUI save dialogs.

## Verification

- Tests for successful atomic writes.
- Tests that failed writes do not leave final files for each output category where practical.
- `cargo test --manifest-path field-gen/Cargo.toml`
