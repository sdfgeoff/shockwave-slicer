# Path Geometry Quality Notes

Date: 2026-05-12

The current G-code pathing is useful as an integration test, but path geometry quality needs more work before it should be considered slicer-grade. This note captures the main issues and likely next improvements.

## Current State

The current pathing pipeline is:

```text
clipped isosurface mesh
  -> mesh boundary distance
  -> contour-parallel perimeter paths
  -> simple world-space grid infill paths
  -> Marlin G-code
```

The implementation is intentionally simple:

- Boundary distance is computed with mesh-edge Dijkstra, not true fast marching.
- Perimeters are contours of that mesh-edge distance field.
- Infill is generated as world-space vertical grid lines clipped by the boundary-distance field.
- Path joining is tolerance-based and does not yet have full topology awareness.

This is enough to exercise the end-to-end data flow, but it will produce artifacts on complex clipped isosurfaces.

## Boundary Loop Extraction

Boundary edges are currently used implicitly through distance-from-boundary. For better pathing, boundary loops should be extracted explicitly.

Useful next steps:

- Build ordered boundary loops from boundary edges.
- Split loops by connected component.
- Distinguish outer boundaries from holes where possible.
- Preserve loop orientation consistently.
- Add tests for multiple components and holes.

Explicit boundary loops will help with seam placement, perimeter ordering, infill masking, and diagnostics.

## Geodesic Distance Quality

Mesh-edge Dijkstra is biased by triangle edge directions. On high-density meshes it may be acceptable, but it is still not a true surface distance.

Potential improvements:

- Keep mesh-edge Dijkstra as a fast baseline.
- Add fast marching or heat-method distance on triangle meshes.
- Compare contour smoothness against the current edge-Dijkstra field.
- Consider smoothing the distance field before contour extraction, while preserving boundary conditions.

The distance field quality directly controls perimeter smoothness.

## Contour Extraction

Current contour extraction intersects scalar values over mesh triangles and then joins segment endpoints by position tolerance.

Known weak points:

- Degenerate triangles or contours touching vertices can create tiny fragments.
- Segment joining can become ambiguous at non-manifold or nearly coincident geometry.
- Closed loops are detected only by endpoint proximity.
- Paths are not simplified or smoothed.
- Paths are not guaranteed to have consistent winding.

Useful next steps:

- Add a contour graph representation instead of ad-hoc segment merging.
- Remove very short segments and near-duplicate points.
- Add loop orientation/winding checks.
- Add optional path simplification.
- Add on-surface smoothing that keeps points on the layer mesh.

## Perimeter Ordering

Current G-code ordering is a simple greedy nearest-start heuristic. It does not rotate closed loops to minimize travel and does not reason about inside/outside ordering.

Useful next steps:

- Rotate closed loops so the seam/start point is nearest the previous endpoint.
- Prefer inner-to-outer or outer-to-inner ordering intentionally.
- Track connected components.
- Add configurable seam placement.

## Infill Geometry

Current infill is a simple world-space grid clipped by boundary distance. It is not yet aware of:

- Holes beyond the boundary-distance mask.
- Pattern continuity across layers.
- Alternating angles.
- Gyroid or other 3D anchored patterns.
- Perimeter/infill overlap compensation.
- Thin-region gap fill.

It is useful for testing, but it is not a final infill strategy.

## Testing Strategy

Good tests should avoid requiring the full voxelization pipeline.

Recommended synthetic tests:

- Square mesh with one boundary loop.
- Mesh with a hole.
- Multiple disconnected mesh components.
- Sloped/non-planar mesh where paths should retain 3D coordinates.
- Degenerate contour cases touching vertices or edges.
- Golden G-code snippets for layer/type/travel markers.

The full STL pipeline should remain a smoke/integration test rather than the primary pathing test harness.

