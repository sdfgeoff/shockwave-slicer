# Add Single Slice Job API

## Goal

Expose one high-level slice job API that covers loading the STL, slicing, writing debug outputs, and writing G-code.

## Rationale

The future GUI needs one stable entry point. Lower-level crates should remain available, but app-level callers should not have to manually reproduce the CLI workflow.

## Scope

- Add a public `run_slice_job(...)` style API in the shared IO/job crate.
- Accept already-loaded `SlicerSettings` rather than CLI-specific arguments.
- Accept progress and cancellation independently.
- Return structured output paths and summary data rather than printing directly.
- Keep timing/logging optional or callback-driven so CLI can print it without forcing GUI behavior.
- Ensure this API is the path used by the CLI.

## Out Of Scope

- Any Tauri-specific event emission.
- Any frontend-friendly DTOs beyond plain Rust API types.
- GUI progress display.

## Verification

- Unit or integration test using a small generated mesh or existing lightweight fixture.
- CLI code path uses the shared API.
- `cargo test --manifest-path field-gen/Cargo.toml`
