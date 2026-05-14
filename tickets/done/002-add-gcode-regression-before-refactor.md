# Add G-code Regression Coverage Before Slicer Refactor

## Goal

Add regression tests that capture current G-code behavior before moving CLI orchestration into new slicer crates.

## Rationale

The CLI currently owns end-to-end slicing behavior. Before extracting pipeline code, we need tests that detect unintended G-code changes.

## Scope

- Add deterministic test coverage for generating G-code from a small fixture or synthetic model.
- Prefer stable assertions over brittle full-file comparisons if necessary.
- Cover key behaviors:
  - preamble is emitted;
  - first layer field/Z alignment is correct for a simple vertical model;
  - perimeters emit before infill;
  - inner perimeters emit before outer perimeters;
  - infill respects perimeter clearance;
  - X/Y model coordinates are preserved;
  - global Z offset behavior once that setting exists.

## Out Of Scope

- Refactoring CLI orchestration.
- Adding JSON config.
- Adding GUI code.

## Verification

- `cargo test --manifest-path field-gen/Cargo.toml`
