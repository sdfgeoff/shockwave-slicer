# shockwave-slicer-io

Filesystem-facing slicer IO helpers.

This crate owns reusable IO behavior shared by CLI and future GUI code: loading model files, deriving output paths, and writing final files safely. It should stay separate from pure slicing algorithms and from CLI-specific presentation.
