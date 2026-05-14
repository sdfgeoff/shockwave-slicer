# Add `shockwave-scene-tree` When GUI Needs Scene State

## Goal

Create `shockwave-scene-tree` only when the GUI needs persistent scene/object state beyond a selected input path.

## Rationale

`shockwave-mesh` owns low-level geometry. The missing abstraction is build-plate scene state: objects, source paths, transforms, active selection, and future multi-model support.

## Scope

- Add `shockwave-scene-tree` crate when implementing GUI scene ownership.
- Define:
  - `Scene`;
  - `SceneObject`;
  - `ObjectId`;
  - identity `Transform3` or use a math transform type if it already exists.
- Store:
  - object name;
  - source path;
  - mesh;
  - local bounds;
  - transform.
- Keep v0/v1 capable of zero or one object, but do not encode single-object-only assumptions into the API.

## Out Of Scope

- Multi-object placement UI.
- Grouping.
- Per-object slicer overrides.
- Import formats beyond what the immediate GUI needs.

## Verification

- Unit tests for adding/removing/active object selection.
- Unit tests for identity transform and bounds behavior.
- `cargo test --manifest-path field-gen/Cargo.toml`
