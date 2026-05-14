# shockwave-config

Persistent user-facing slicer settings.

This crate owns the JSON settings schema, defaults, validation, config path resolution, and simple load/save helpers. It should describe slicer concepts in stable user-facing terms, not expose internal CLI flag names or pipeline implementation details.
