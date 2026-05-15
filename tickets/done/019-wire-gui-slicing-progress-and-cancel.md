# Wire GUI Slicing Progress And Cancel

## Goal

Run the shared slicer job from the GUI and display progress with cancellation.

## Rationale

This makes the GUI useful before visualization work starts, while validating that the shared job API is suitable for an app frontend.

## Scope

- Call `shockwave_slicer_io::run_slice_job` directly from the GUI.
- Run slicing on a background thread or task so the UI stays responsive.
- Display current slicing phase, message, and progress.
- Add a Cancel button using `CancellationToken`.
- Display success or error status.
- Use the current `SlicerSettings` object as the job config.

## Out Of Scope

- Canvas rendering.
- G-code preview.
- Settings editor beyond already-loaded settings.
- Killing worker threads forcibly.

## Verification

- A small STL can be sliced from the GUI.
- Progress updates appear during the run.
- Cancel stops a run and does not report success.
- `cargo test --manifest-path field-gen/Cargo.toml`
