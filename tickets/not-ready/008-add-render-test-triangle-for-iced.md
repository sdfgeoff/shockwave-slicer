# Add Minimal Iced/WGPU Render Integration

## Goal

Create rendering infrastructure only when implementing the first GUI viewport: a test shader rendering a single triangle inside an Iced application.

## Rationale

v0 does not need STL preview. The first goal is to prove that the GUI can host custom rendering code cleanly.

## Scope

- Add `shockwave-render` when implementing real render code.
- Implement a minimal renderer capable of drawing one triangle with a test shader.
- Keep renderer independent from slicer and config crates.
- Expose a small API suitable for an Iced widget/viewport.

## Out Of Scope

- STL/model preview.
- Print bed grid.
- Camera controls.
- G-code visualization.
- Field visualization.

## Verification

- Renderer compiles in the workspace.
- GUI can display the test triangle.
- Avoid screenshot/browser automation requirements.
