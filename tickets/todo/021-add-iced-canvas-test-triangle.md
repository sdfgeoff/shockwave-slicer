# Add Iced Canvas Test Triangle

## Goal

Add a rendering canvas that draws a simple test triangle.

## Rationale

This isolates the rendering risk before STL or G-code preview work. If Iced canvas or custom rendering is not suitable, this is the cheapest point to discover it.

## Scope

- Add a canvas or custom widget area to the GUI.
- Render a plain triangle.
- Keep the rendering code in its own module.
- Preserve existing slicing/settings UI behavior.

## Out Of Scope

- STL preview.
- Camera controls.
- G-code preview.
- Custom WGPU shaders unless required for the triangle.

## Verification

- The triangle appears in the GUI.
- Window resize does not break the UI.
- `cargo test --manifest-path field-gen/Cargo.toml`
