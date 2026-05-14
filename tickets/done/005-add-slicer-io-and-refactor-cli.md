# Add `shockwave-slicer-io` And Refactor CLI To Shared Pipeline

## Goal

Create `shockwave-slicer-io` when moving filesystem/job-runner behavior out of `field-gen-cli`, then refactor the CLI to call the shared slicer stack.

## Rationale

The CLI and GUI both need file loading, safe output writing, and job orchestration. That should not live in the CLI.

## Scope

- Add `shockwave-slicer-io` crate when moving real IO behavior.
- Own:
  - loading STL/model files from paths;
  - writing G-code through a temporary file;
  - renaming temp output to final output only on success;
  - mapping config + input path + output path/prefix into a slice job;
  - filesystem/path errors.
- Refactor `field-gen-cli` to call `shockwave-config`, `shockwave-slicer`, and `shockwave-slicer-io`.
- Preserve existing CLI flags and output behavior.
- Keep `--output <prefix>` compatible with existing CLI output naming.
- Add `--config <settings.json>` support early.
- Explicit CLI arguments override JSON settings.

## Output Behavior

- CLI continues to produce debug outputs (`.occ`, `.bmp`, `.json`, optional `.ply`) as today.
- GUI will also produce debug outputs initially; do not gate them yet.
- G-code final output should avoid convincing partial files by using temp-file then rename.

## Out Of Scope

- GUI code.
- Recent-file or user-session state.
- Arbitrary custom start/end G-code.

## Verification

- Existing CLI commands still work.
- Tests for JSON config load + explicit CLI override precedence.
- Tests for temp-file output success and cancellation/error behavior.
- Existing G-code regression tests still pass.
- `cargo test --manifest-path field-gen/Cargo.toml`
