# Improve Progress And Cancellation Granularity

## Goal

Make progress and cancellation useful during long-running slice phases, not only between major phases.

## Rationale

The GUI needs responsive progress and cancellation while propagation, isosurface extraction, clipping, and path generation are running. Long phases should not appear frozen.

## Scope

- Audit each long-running phase for progress and cancellation hooks:
  - voxelization;
  - field propagation;
  - isosurface extraction;
  - clipping;
  - path generation;
  - output writing.
- Preserve the existing `SliceProgress` model unless a small extension is clearly needed.
- Keep progress callback-based and independent from cancellation.
- Add cancellation checks inside long loops where it is safe to stop.
- Keep CLI progress output readable.

## Out Of Scope

- Tauri event emission.
- GUI progress bar implementation.
- Thread cancellation or forced thread termination.

## Verification

- Tests for cancellation during at least one long-running phase, not only between phases.
- Existing progress phase ordering tests still pass.
- `cargo test --manifest-path field-gen/Cargo.toml`
