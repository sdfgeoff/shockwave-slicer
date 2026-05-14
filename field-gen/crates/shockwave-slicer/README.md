# shockwave-slicer

Core slicing pipeline.

This crate owns the pure slicing workflow over already-loaded geometry: voxelization, field propagation, layer extraction, clipping, path generation, progress reporting, cancellation checks, and G-code writing to an abstract writer. It should avoid direct filesystem policy and GUI concerns.
