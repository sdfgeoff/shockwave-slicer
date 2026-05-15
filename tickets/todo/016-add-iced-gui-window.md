# Add Iced GUI Window

## Goal

Create a minimal `shockwave-gui` crate in the existing Rust workspace that opens an Iced desktop window.

## Rationale

This proves the GUI stack builds and runs without introducing slicer behavior, rendering complexity, or settings UI.

## Scope

- Add `shockwave-gui` as a workspace crate.
- Use Iced for the application window.
- Show a simple app title/status view.
- Keep the crate separate from CLI and slicer internals.
- Add a README for the crate describing its high-level responsibility.

## Out Of Scope

- File selection.
- Slicing.
- Settings forms.
- Canvas or 3D rendering.

## Verification

- `cargo run --manifest-path field-gen/Cargo.toml -p shockwave-gui` opens a window.
- `cargo test --manifest-path field-gen/Cargo.toml`
