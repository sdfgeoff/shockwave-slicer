# Extract Pure Slicing Pipeline Into `shockwave-slicer`

## Goal

Create `shockwave-slicer` while moving pure slicing pipeline orchestration out of `field-gen-cli`.

## Rationale

The GUI must not depend on CLI internals. The CLI and GUI should share one tested slicing pipeline.

## Scope

- Add `shockwave-slicer` crate when moving real pipeline behavior.
- Depend on `shockwave-config` for user-facing settings datatypes where appropriate.
- Move or expose pipeline steps:
  - voxelization;
  - field propagation;
  - field floor alignment;
  - isosurface extraction;
  - clipping;
  - path generation;
  - G-code generation into an abstract `std::io::Write`.
- Define:
  - `SlicePhase`;
  - `SliceProgress`;
  - progress callback API;
  - `CancellationToken`;
  - typed `SliceError`.
- Preserve existing behavior initially.

## API Direction

`shockwave-slicer` should operate on already-loaded geometry/scene data, not raw file paths. Filesystem policy belongs in `shockwave-slicer-io`.

Progress shape:

```rust
pub enum SlicePhase {
    LoadModel,
    Voxelize,
    PropagateField,
    ExtractLayers,
    ClipLayers,
    GeneratePaths,
    WriteGcode,
}

pub struct SliceProgress {
    pub phase: SlicePhase,
    pub phase_progress: f32,
    pub message: String,
}
```

## Out Of Scope

- File loading by path.
- Temp-file output policy.
- GUI code.
- Scene tree unless the pipeline needs it immediately.

## Verification

- Existing G-code regression tests still pass.
- New tests for progress phase ordering.
- New tests for cancellation between phases.
- `cargo test --manifest-path field-gen/Cargo.toml`
