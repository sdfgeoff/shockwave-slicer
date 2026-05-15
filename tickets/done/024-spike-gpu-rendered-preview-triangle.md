# Spike GPU Rendered Preview Triangle

## Goal

Prove that the Iced GUI can host a custom WGPU-rendered primitive inside the normal UI.

## Rationale

The current STL preview is CPU-projected and CPU-rendered through Iced canvas paths. Before replacing it with a real mesh renderer, we need to confirm that custom WGPU rendering integrates cleanly with Iced.

## Scope

- Add a dedicated GPU preview module.
- Add the smallest custom widget needed to reserve a preview rectangle.
- Use `iced_wgpu::Primitive` to render a triangle through a custom WGPU pipeline.
- Integrate the GPU triangle into the GUI alongside or near the existing preview.
- Keep the existing CPU canvas preview intact as fallback.

## Out Of Scope

- STL mesh GPU buffers.
- Depth buffer.
- Camera controls.
- Toolpath GPU rendering.
- Replacing the existing CPU preview.

## Verification

- `cargo test --manifest-path field-gen/Cargo.toml`
- `cargo run --manifest-path field-gen/Cargo.toml -p shockwave-gui` shows the GPU triangle panel.
