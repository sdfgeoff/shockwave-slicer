# Slicer GUI Design Notes

Date: 2026-05-14

## Confirmed Decisions

- The slicing engine must be separate from the GUI.
- The existing debug viewers are not intended to become the production slicer UI.
- Slicing must run in the background.
- Settings should persist as JSON.
- The CLI should eventually ingest the same JSON settings file, with explicit CLI arguments overriding JSON values.
- Settings should use slicer-facing names rather than internal implementation names.
- Settings should use OS config directories. On Linux this should resolve to `~/.config/shockwave-slicer/settings.json`.
- `global_z_offset` is a final G-code Z offset for bed-level compensation, not a field-generation parameter.
- Bed dimensions / print volume are visualization-only in v1.
- STL model placement should be preserved as-is for v1.
- Progress should use one progress bar, with each slicing phase weighted equally for now.
- Initial phases: parse STL, voxelize, propagate field, extract layers, clip layers, generate paths, write G-code.
- v1 flow: select STL, choose output G-code file, slice, write directly to that path.
- No G-code visualization is required in v1.
- Future UI should allow visualizing G-code and possibly the field.
- GUI should not depend on the Makefile.
- Settings should be validated before slicing starts.

## Current Codebase Facts

- The Rust workspace is under `field-gen/`.
- Existing engine crates include `shockwave-core`, `shockwave-stl`, `shockwave-voxel`, `shockwave-iso`, `shockwave-clip`, `shockwave-path`, and `shockwave-gcode`.
- `field-gen-cli` still owns end-to-end orchestration, CLI parsing, progress output, and output writing.
- A production GUI should move orchestration and slicer-facing config into a reusable slicer crate.

## GUI Toolkit Discussion

### Iced

Iced is attractive because it is pure Rust and supports a one-language build/development model. It is likely the strongest candidate if avoiding web frontend tooling is a hard requirement.

Concerns:

- 3D model rendering on a print bed will likely need custom `wgpu` integration or an embedded scene/render widget.
- File dialogs, app persistence, native packaging, and platform polish are less turnkey than Tauri.
- The UI model is Rust-native, which is good for typed state, but less convenient for rich 3D viewport and CSS-like layout iteration.

Recommendation: viable, but only if we accept doing more custom 3D/viewer work.

### Tauri

Tauri gives a Rust backend with a local webview frontend. It is not a web app in the hosted sense; it is a local desktop app using OS webviews. Tauri's docs expose webview/window APIs and are explicitly built around Rust plus webview integration.

Pros:

- Strong fit for a local app with Rust backend and frontend UI.
- Three.js/Babylon/etc. can handle STL preview and print-bed rendering quickly.
- Easy UI iteration compared with native Rust GUI layout.
- Clear path to background slicing, progress events, file dialogs, settings persistence, and later G-code/field visualization.

Concerns:

- Adds JavaScript/TypeScript frontend tooling unless kept deliberately minimal.
- Webview rendering behavior varies by OS.
- More moving parts than pure Rust.

Recommendation: best fit if web frontend tooling is acceptable.

### Slint

Slint is a native declarative GUI toolkit with Rust integration. Its site describes it as a declarative GUI for Rust and notes a small runtime footprint.

Pros:

- Native-ish app without web frontend tooling.
- More structured/polished than egui-style immediate mode.
- Better fit than egui for a conventional settings-heavy application.

Concerns:

- 3D viewport story is less obvious than a webview + Three.js.
- More custom rendering integration likely needed.
- Smaller ecosystem for slicer-style 3D preview workflows.

Recommendation: worth considering if avoiding web tech is more important than fastest 3D UI delivery.

### Dioxus Desktop

Dioxus offers Rust-authored UI across desktop/web/mobile. Its site positions it as a fullstack cross-platform Rust framework. The desktop renderer docs say Dioxus Desktop uses a native WebView and is built off Tauri.

Pros:

- Rust-authored frontend with webview rendering.
- Potentially less TypeScript/JS than Tauri.
- Uses web layout/rendering, so 3D web rendering remains plausible.

Concerns:

- Desktop path still relies on webview/Tauri/Wry concepts.
- Smaller desktop-app ecosystem than Tauri directly.
- If we need Tauri-level native integration anyway, using Tauri directly may be simpler.

Recommendation: interesting, but probably not the simplest v1 choice.

## Current Recommendation

The two serious options are:

- **Tauri + TypeScript/JS + Three.js** if we prioritize shortest path to a capable 3D slicer UI.
- **Iced + custom wgpu viewport** if we prioritize pure Rust and one-command builds over UI/3D implementation speed.

My recommendation remains Tauri unless pure Rust is a hard requirement. Iced is the strongest pure-Rust alternative, but it moves complexity from build tooling into viewport/rendering/tooling code.

## Toolkit Decision

Decision: use **Iced** for the production slicer GUI.

Rationale:

- Pure Rust is preferred for the slicer application.
- One-command Cargo builds are valuable.
- Avoiding a web frontend is preferred for long-term maintainability.
- Custom `wgpu` rendering is acceptable for the 3D viewport.
- Iced is mature enough for conventional application UI, and the project can own the specialized 3D rendering path.

Implications:

- The GUI crate should be a normal Rust binary crate in the workspace.
- The 3D viewport should be isolated behind a small rendering module so UI state, slicer orchestration, and `wgpu` rendering do not become tangled.
- The initial model viewport can be simple: STL mesh, print bed plane/grid, camera orbit/pan/zoom, and bounds visualization.
- Later G-code/field visualization should reuse the same viewport/rendering abstraction rather than adding a separate viewer stack.

## Workspace Decision

Decision: add the GUI as a separate crate in the current `field-gen` workspace.

Rationale:

- A single flat workspace is easier to manage than nested workspaces.
- The GUI should remain separate from the engine crates and CLI crate.
- Keeping it in the same workspace lets it depend directly on existing crates while the engine boundary is refined.

## Engine Boundary Decision

Decision: create intermediate crates rather than putting GUI orchestration directly in the CLI or building one giant slicer crate.

Likely crate split, created as needed rather than scaffolded upfront:

- `shockwave-config`: persistent JSON settings schema, defaults, config-dir path resolution, load/save/merge behavior.
- `shockwave-slicer`: pure slicing pipeline and orchestration over existing engine crates.
- `shockwave-slicer-io`: filesystem/job runner shared by CLI and GUI.
- `shockwave-scene-tree`: user scene graph / build-plate model ownership, created when the GUI needs scene ownership.
- `shockwave-render`: reusable 3D viewport/rendering code for the GUI, created when the shader viewport is implemented.
- `shockwave-slicer-gui`: Iced application shell, user interaction, file dialogs, background slicing task, and wiring to config/slicer/render crates.

Rationale:

- The CLI should not be the engine API.
- The GUI should not own slicing logic.
- `shockwave-mesh` should stay low-level geometry; scene/application state belongs in `shockwave-scene-tree`.
- Rendering will be substantial and should not be buried inside the GUI app crate.
- File import, rendering, config, and slicing orchestration can split further as concrete needs appear.

Open implementation caution:

- Avoid over-splitting before APIs exist. Add crates when there is an immediate consumer and a clear ownership boundary.
- Do not create empty placeholder crates. Each crate should arrive as part of a ticket that moves or implements concrete behavior.

## Scene Tree Decision

Decision: prefer `shockwave-scene-tree` over a generic `shockwave-model` crate, but create it only when a feature needs it.

Rationale:

- `shockwave-mesh` already owns low-level geometry.
- The missing abstraction is application scene state: selected/imported objects, source paths, transforms, bounds, and eventually multiple objects on a build plate.
- A scene tree can grow toward placement, grouping, object enable/disable, per-object settings, and future import formats without contaminating mesh geometry types.

Minimum v1 shape:

```rust
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub active_object: Option<ObjectId>,
}

pub struct SceneObject {
    pub id: ObjectId,
    pub name: String,
    pub source_path: PathBuf,
    pub mesh: Mesh,
    pub transform: Transform3,
    pub bounds_local: Bounds,
}
```

For v1 the scene may only contain zero or one object in normal use, but the API should not encode that limitation. The default transform is identity. Slicing should use transformed geometry.

## Settings Schema Decision

Decision: persistent settings JSON should use nested slicer-facing sections.

Path:

- Use OS config directories.
- On Linux this should resolve to `~/.config/shockwave-slicer/settings.json`.

Important naming decisions:

- `layer_height` is the user-facing name for current isosurface spacing.
- Infill should be configured as `infill_percentage`, not `infill_spacing`, even if v1 converts percentage to a simple spacing internally.
- Field maximum size should derive from printer bed / print volume rather than being duplicated in the field section.
- Reachability/obstruction parameters are physical printer parameters and belong under `printer.obstruction`.

Proposed v1 schema:

```json
{
  "version": 1,
  "slicing": {
    "layer_height": 0.25,
    "wall_count": 2,
    "extrusion_width": 0.4,
    "infill_percentage": 15.0
  },
  "printer": {
    "bed_width": 220.0,
    "bed_depth": 220.0,
    "max_height": 250.0,
    "global_z_offset": 0.0,
    "obstruction": {
      "printhead_clearance_angle": 45.0,
      "printhead_clearance_height": 5.0
    }
  },
  "material": {
    "filament_diameter": 1.75,
    "nozzle_temperature": 215,
    "bed_temperature": 60,
    "fan_speed_percent": 100
  },
  "field": {
    "voxel_size": [0.4, 0.4, 0.4],
    "field_method": "trapezoid"
  }
}
```

Implementation notes:

- v1 maps `infill_percentage` to simple line spacing internally:
  `line_spacing_mm = extrusion_width_mm / (infill_percentage / 100)`.
- `infill_percentage = 0` disables infill.
- This is a provisional implementation detail. The JSON should continue exposing percentage rather than spacing.
- `shockwave-config` owns the user-facing settings datatypes.
- `shockwave-slicer` may depend on `shockwave-config` for these datatypes rather than inventing duplicate settings.
- CLI JSON ingestion should load this schema, then explicit CLI arguments should override values.

## Progress API Decision

Decision: the slicer engine reports progress as a fixed phase enum with `0.0..1.0` subprogress per phase.

Example:

```rust
pub enum SlicePhase {
    LoadModel,
    Voxelize,
    PropagateField,
    ExtractLayers,
    ClipLayers,
    GeneratePaths,
    WriteGcode,
}

pub struct SliceProgress {
    pub phase: SlicePhase,
    pub phase_progress: f32,
    pub message: String,
}
```

The GUI computes one progress bar as:

```text
overall = (phase_index + phase_progress) / phase_count
```

For phases without real progress, emit `0.0` then `1.0`. Propagation should reuse its real progress data.

## Cancellation And Output Safety Decision

Decision: v1 should support cooperative cancellation.

Rationale:

- Rust does not provide safe arbitrary thread termination.
- Forced thread termination could leave locks held, shared state inconsistent, resources leaked, or output files partially written.
- Cooperative cancellation gives predictable cleanup points.

Implementation shape:

```rust
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}
```

The slicer should check cancellation:

- Before each major phase.
- Inside long loops where practical.
- Inside field propagation progress handling.
- Before committing final output.

Cancellation should return a typed `SliceError::Cancelled`.

Output safety:

- Write G-code to a temporary file first.
- Only rename to the selected output path after successful completion.
- Cancellation or errors should not leave a convincing partial G-code file at the final path.

## G-code Preamble Decision

Decision: v1 should not expose arbitrary custom start/end G-code.

Structured preamble settings:

- `material.bed_temperature`
- `material.nozzle_temperature`
- `material.fan_speed_percent`

Hardcoded for now:

- Homing / lift sequence.
- Prime extrusion length and feedrate.
- Start/end G-code templates.

Rationale:

- Structured temperature/fan settings are easy to validate and safe to expose.
- Arbitrary start/end G-code is useful later, but not necessary for v1.

## Viewport Scope Decision

Decision: v0 does not need STL/model preview.

Minimum v0 viewport:

- An Iced widget / viewport area.
- Custom rendering path capable of displaying a test shader.
- A single triangle is sufficient.

Deferred until after refactor/settings/slicing flow:

- STL preview.
- Print bed grid.
- Camera controls.
- G-code visualization.
- Field visualization.

Rationale:

- The immediate goal is to prove GUI architecture, settings persistence, background slicing, and custom rendering integration.
- Full visualization should be built after the engine/config/refactor boundaries are established.

## Initial Implementation Order Decision

Decision: build the new crates in this order:

1. `shockwave-config`: settings schema, defaults, OS config path, load/save.
2. `shockwave-slicer`: job/settings mapping, progress/cancellation/output temp-file API, initially wrapping existing pipeline logic.
3. `shockwave-scene-tree`: minimal scene/object/identity transform types.
4. `shockwave-render`: minimal test triangle renderer.
5. `shockwave-slicer-gui`: Iced app shell wiring settings, background slicing, file selection, progress, save output, and render widget.

Rationale:

- The GUI should stay thin.
- Settings and slicer orchestration should stabilize before building UI around them.
- Rendering can start as a proof of integration and grow later.

## Slicer Crate Split Decision

Decision: split slicing into a pure pipeline crate and an IO/job-runner crate.

Crates:

- `shockwave-slicer`: core slicing pipeline.
- `shockwave-slicer-io`: application-facing filesystem/job orchestration shared by CLI and GUI.

`shockwave-slicer` owns:

- User-facing settings datatypes from `shockwave-config` and any resolved engine settings derived from them.
- Geometry/scene input, not raw file paths.
- Voxelization.
- Field propagation.
- Isosurface extraction and clipping.
- Path generation.
- G-code generation into an abstract `std::io::Write`.
- Progress callback and cancellation checks.

`shockwave-slicer-io` owns:

- Loading model files from paths.
- Writing G-code through a temporary file and renaming on success.
- Mapping settings + scene + output path into a slice job.
- Filesystem/path errors.
- CLI/GUI shared job orchestration that is not pure engine logic.

Rationale:

- Engine tests can avoid filesystem dependencies.
- CLI and GUI can share safe output behavior.
- Filesystem policy does not contaminate the pure slicing pipeline.
- Future batch/project-file behavior has a natural home.

## Core Rename Decision

Decision: rename `shockwave-core` to `shockwave-math` early in the refactor.

Rationale:

- The crate currently owns low-level geometry/grid primitives.
- `core` risks becoming a junk drawer.
- `math` creates a clearer ownership boundary: vectors, bounds, triangles, grids, transforms, and numeric primitives.

Implementation note:

- Do this as a standalone mechanical commit.
- Avoid behavior changes in the rename commit.

## CLI Refactor Strategy

Decision: refactor the CLI before GUI work.

Order:

1. Add regression coverage around current CLI/G-code behavior before moving code.
2. Rename `shockwave-core` to `shockwave-math` as its own mechanical commit.
3. Add `shockwave-config` when implementing persistent settings / JSON config.
4. Add `shockwave-slicer` while moving pure slicing pipeline logic out of `field-gen-cli`.
5. Add `shockwave-slicer-io` when moving filesystem job behavior, model loading, temp-file output, and CLI/GUI shared IO policy.
6. Refactor `field-gen-cli` to call the new crates while preserving current flags.
7. Add CLI JSON ingestion early, with explicit CLI arguments overriding JSON settings.
8. Add progress callback, cancellation token, and temp-file output semantics.
9. Start GUI work only after the CLI is using the shared slicer stack.

## Sources Checked

- Tauri webview/window API: https://tauri.app/reference/javascript/api/namespacewebviewwindow/
- Iced Rust docs: https://docs.iced.rs/iced/
- Slint docs/site: https://docs.slint.dev/index.html and https://slint.rs/
- Dioxus site/docs: https://dioxus.dev/ and https://docs.rs/dioxus-desktop/latest/dioxus-desktop
