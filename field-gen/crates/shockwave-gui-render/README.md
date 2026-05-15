# shockwave-gui-render

WGPU renderer for Shockwave preview scenes.

This crate owns GPU-facing preview geometry, shader inputs, render pipelines, and rendering into a caller-provided WGPU target. It does not know about Iced or any other GUI toolkit; GUI crates provide adapters around it.
