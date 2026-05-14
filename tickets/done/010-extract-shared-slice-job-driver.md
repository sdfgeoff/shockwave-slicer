# Extract Shared Slice Job Driver

## Goal

Move the remaining slice workflow orchestration out of `field-gen-cli` into shared Rust code that both the CLI and a future local app can call.

## Rationale

The CLI still owns timing, output orchestration, metadata writing, debug output writing, and some pipeline sequencing. A Tauri command should not duplicate this behavior.

## Scope

- Add or extend a shared crate for filesystem-aware slice job orchestration.
- Define a reusable job input type containing:
  - input STL path;
  - output prefix;
  - `SlicerSettings`;
  - progress callback;
  - cancellation token.
- Define a reusable job output type containing the generated output paths and any useful run summary.
- Move CLI orchestration from `field-gen-cli/src/main.rs` into the shared job driver.
- Refactor the CLI so it only parses arguments, loads settings, calls the shared job driver, and reports errors.
- Preserve current CLI output files and behavior.

## Out Of Scope

- Tauri commands.
- GUI state.
- Model preview or scene state.

## Verification

- Existing CLI behavior is preserved.
- Add tests that call the shared job driver without invoking the CLI binary.
- `cargo test --manifest-path field-gen/Cargo.toml`
