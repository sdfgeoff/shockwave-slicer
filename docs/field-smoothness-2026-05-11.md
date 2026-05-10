# Field Smoothness Notes

Date: 2026-05-11

The current field propagation appears close enough for the present stage, but the output field visualization and exported isosurfaces still show small stair-step artifacts. This note captures the likely causes and future options.

## Current Assessment

The artifacts are probably not caused by one single bug. They come from several parts of the pipeline:

- The propagated field is a discrete shortest-path field over voxel centers. Even the native trapezoid propagation method is ultimately represented as a finite set of integer voxel moves, so grid/metrication artifacts are expected.
- The BMP preview stores the propagated field in an 8-bit normalized red channel. For fields with large `field_max_distance`, each visible step may represent a fairly large field-distance interval.
- The isosurface extractor already uses a surface-nets style approach with edge crossings and projection onto the trilinear cell field. It is not simply placing vertices at voxel centers, but it can only be as smooth as the sampled scalar field.
- Reachability constraints and fallback reseeding can create real discontinuities or flat regions. Those may be correct from a printability/order perspective, but they will not always look like smooth distance-field layers.

## Likely Sources

### Propagation Metrication

The field is generated with Dijkstra-style propagation over a voxel graph. This means the field is only an approximation of a continuous front. Larger kernels improve angular resolution, but they do not remove the underlying graph nature of the solve.

The native trapezoid method helps because it samples the intended growth shape at the current voxel size, rather than relying on a pre-authored JSON kernel. However, it still emits discrete moves, so stair-stepping can remain visible.

### Visualization Quantization

The BMP output is useful for inspection, but it is not high precision. The red channel maps the full field range to `0..255`, so visual banding can appear even if the internal `f64` field is smoother.

This means viewer artifacts should not be treated as definitive evidence that the in-memory field is equally stepped.

### Isosurface Extraction

The extractor builds one vertex per active cell and projects that vertex onto the trilinear field in that cell. This gives better geometry than a pure voxel-center surface, but the topology and vertex density are still tied to the voxel grid.

If the underlying field changes in grid-aligned increments, the extracted mesh will reflect that.

### Constraint Discontinuities

The max-unreached-below constraint, cone constraint, and fallback reseeding are intentionally non-smooth operations. They model printability constraints rather than physical distance alone.

These operations can introduce real kinks or discontinuities in the field. Some of those may be desirable because they represent a change in print order or reachability.

## Future Options

### 1. Add High-Precision Field Output

Export the scalar field in a high-precision format, such as raw `f32`, raw `f64`, or eventually TIFF/EXR-style image data.

This would let the viewer inspect the real field instead of the 8-bit BMP approximation. It would also make it easier to determine whether artifacts are caused by visualization or by propagation.

This is probably the best first diagnostic step.

### 2. Add Field Smoothness Diagnostics

Add debug reporting for:

- Neighbor field deltas.
- Gradient magnitude distribution.
- Per-slice min/max field values.
- Fallback seed locations and distances.
- Counts of voxels affected by reachability constraints.

This would make it easier to distinguish propagation artifacts from constraint artifacts.

### 3. Add Constrained Mesh Smoothing

After isosurface extraction, apply tangential or Taubin-style mesh smoothing, then project vertices back onto the same isovalue using the trilinear field.

This would improve mesh/toolpath smoothness without changing the underlying printability field. The projection step is important because smoothing alone would drift vertices away from the requested layer.

This is likely a pragmatic improvement for exported meshes.

### 4. Add Scalar Field Regularization

Apply a limited smoothing pass to the scalar field after propagation, constrained to occupied connected regions and avoiding smoothing across occupancy boundaries.

This is easier than replacing the propagation solver, but it is risky because it can blur printability constraints. It should be treated as an optional post-process, not as the source of truth, unless carefully validated.

### 5. Use a Continuous Front Solver

Replace or augment graph-Dijkstra propagation with an eikonal-style solver such as fast marching or ordered-upwind methods.

This would directly target metrication artifacts and produce smoother fields, but it is a larger algorithmic change. Ordered-upwind methods may be relevant if the propagation remains strongly anisotropic or direction-dependent.

## Recommended Path

For now, keep the current propagation model.

If smoothness becomes a blocker, the recommended order is:

1. Add high-precision field export/import for debugging.
2. Add field smoothness diagnostics.
3. Add constrained mesh smoothing with projection back to the isosurface.
4. Consider field regularization only if diagnostics show local noise rather than meaningful constraints.
5. Consider a continuous front solver only if graph propagation remains the limiting factor.

