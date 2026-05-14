# Move Kernel JSON Loading Out Of CLI

## Goal

Move explicit field propagation kernel JSON parsing/loading into shared slicing setup code.

## Rationale

`field.kernel_path` is part of the config schema, not CLI behavior. Any caller using `SlicerSettings` should get the same field propagation behavior.

## Scope

- Move explicit kernel parsing from `field-gen-cli` into a shared crate.
- Keep support for current explicit kernel format:
  - `type: "explicit"`;
  - `path_check`;
  - `moves[].offset`;
  - `moves[].cost`.
- Convert `SlicerSettings` into `SliceSettings` through shared code.
- Make `field.kernel_path` override `field.method` consistently for CLI and future app callers.
- Preserve existing error messages or improve them if the shared API makes that clearer.

## Out Of Scope

- New kernel formats.
- Kernel authoring UI.
- Generated/native kernels beyond the formats already supported.

## Verification

- Add tests for valid explicit kernel JSON.
- Add tests for invalid kernel type, invalid path check, malformed offsets, and missing moves.
- Existing config and CLI tests still pass.
- `cargo test --manifest-path field-gen/Cargo.toml`
