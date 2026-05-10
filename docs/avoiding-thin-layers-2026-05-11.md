# Avoiding Very Thin Layers

Date: 2026-05-11

The propagated field defines printability/order. Isosurfaces extracted from that field are candidate layer surfaces. In regions where propagation is non-uniform or constrained, multiple candidate isosurfaces may become very close together. This can produce many very thin layers.

At this stage, this is mostly a print-time optimization problem rather than a correctness blocker. Very thin layers are inefficient, but they can still represent printable material. Avoiding them should be handled downstream of propagation and mostly downstream of raw isosurface extraction.

## Problem

When the field changes rapidly across a small physical distance, fixed field-spacing isosurfaces become physically close together. This can happen near:

- Reachability constraint transitions.
- Fallback reseeding regions.
- Areas where vertical propagation is halted or delayed.
- Areas where the scalar field is locally discontinuous or steep.

Discarding whole layers is too blunt because only parts of a candidate layer may be too thin. Other regions of the same layer may have a reasonable physical separation from neighboring layers.

Simply not printing thin regions is also not sufficient, because it can leave voids in the model. If material is not printed on one candidate layer, that material should generally be accounted for later.

## Layer Band Concept

A useful slicer primitive may be a layer band rather than only an isosurface.

Conceptually:

```text
Layer {
  surface: deposition surface at field = k
  thickness: local physical layer thickness over the surface
  material_demand: local material volume/area required by this field interval
  accumulated_debt: material carried forward from earlier underprinted regions
  printable_mask: local decision about whether this region should be printed now
}
```

It may be better to represent this as a surface plus local thickness/material data rather than as two separate surfaces. The upper/current surface is the nozzle path surface, while thickness describes the material assigned to that layer.

## Why Voxel Bands Alone Are Not Enough

The layer spacing may be denser than the voxel spacing. If layer validity is decided only by voxel-center membership in a field interval, decisions collapse to voxel resolution and lose useful sub-voxel information from the trilinear field and isosurface extractor.

Thin-layer detection should therefore use geometric/continuous estimates where possible, such as:

- Distance to the previous accepted layer along a local direction.
- Field gradient magnitude near the surface.
- Local field interval thickness, approximately `spacing / |grad(field)|`.
- Local volume density on the layer surface.

Voxel-space data can still be useful for conservative volume accounting, but it should not be the only layer-validity signal.

## Iterative Material Accounting Idea

One possible approach is to keep a separate leftover-material field and process candidate layers iteratively.

Sketch:

1. Initialize a `leftover_material` field.
2. Extract the isosurface at `field = spacing * layer`.
3. Estimate the local layer volume requirement using the derivative of the field.
4. Add any accumulated leftover material to the current layer's local volume requirement.
5. If a local region has less than a minimum useful printable volume/thickness, do not print it on this layer.
6. Add the unprinted material back into the leftover field so it can be consumed by later layers.

This treats very thin regions as material scheduling/deposition optimization rather than as missing geometry.

## Voxel/Mesh Space Tension

There is an unresolved representation issue between voxel volume and mesh/surface pathing.

Voxel space is good for:

- Conservative material accounting.
- Tracking unassigned volume.
- Avoiding accidental loss of material.
- Staying aligned with the source scalar field.

Surface space is good for:

- Nozzle path generation.
- Local layer thickness.
- Perimeter and bead planning.
- Geometric distance to adjacent layers.

A pragmatic split may be:

- Store source-of-truth unassigned material in voxel/cell space.
- Extract a candidate layer surface for the nozzle path.
- Project or splat relevant voxel/cell volume onto the surface for toolpath planning.
- If the surface region is too thin or otherwise not worth printing, leave that material in the voxel accumulator.

This avoids losing material while still allowing path generation to happen on the actual deposition surface.

## Local Thickness Estimate

For a scalar field `f`, candidate layers are separated by a field interval `df = spacing`. If the field is locally smooth, physical thickness can be approximated by:

```text
local_thickness ~= df / |grad(f)|
```

This estimate should be treated carefully near:

- Field discontinuities.
- Fallback seeds.
- Occupancy boundaries.
- Regions where `|grad(f)|` is very small or noisy.

In these cases, local geometric distance to neighboring candidate/accepted layers may be more reliable than gradient-only thickness.

## Top Surface Handling

Carrying unprinted material forward works only while there are later layers available to receive it. Top surfaces may need special handling.

Possible later strategies:

- Force remaining material into the final few layers within bead-height limits.
- Add explicit non-planar finishing layers.
- Use smaller bead heights or widths for final detail.
- Treat top-surface precision as a separate pass from general thin-layer optimization.

This can likely be deferred. The immediate goal is to avoid wasting time on many tiny intermediate layers, not to solve final top-surface quality completely.

## Current Position

For now, very thin layers are acceptable. They increase print time, but they are not a showstopper.

The important design decision is that avoiding thin layers should not change field propagation. The field should continue to represent printability/order. Thin-layer handling belongs in a later layer-planning/toolpath stage that can account for material volume and decide where/when to deposit it.

## Possible Future Pipeline

```text
propagation field
  -> candidate isosurfaces
  -> layer-band analysis
  -> local thickness/material demand
  -> carry-forward material accounting
  -> printable region selection
  -> perimeter/bead/toolpath generation
  -> G-code
```

The first useful implementation step would be diagnostic rather than corrective: compute local thickness estimates for candidate layers and report how much of each layer falls below a configurable minimum printable thickness.

