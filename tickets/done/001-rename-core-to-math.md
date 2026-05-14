# Rename `shockwave-math` To `shockwave-math`

## Goal

Rename the low-level math/geometry/grid crate from `shockwave-math` to `shockwave-math`.

## Rationale

`shockwave-math` currently owns low-level primitives such as vectors, bounds, triangles, and grids. The name `core` risks becoming a junk drawer. `math` creates a clearer boundary for geometry, grid/index math, transforms, and numeric primitives.

## Scope

- Rename crate directory `field-gen/crates/shockwave-math` to `field-gen/crates/shockwave-math`.
- Update package name in `Cargo.toml`.
- Update all workspace members and dependencies.
- Update Rust imports from `shockwave_math` to `shockwave_math`.
- Run tests.
- Commit as a standalone mechanical change.

## Out Of Scope

- Behavior changes.
- Moving non-math logic into or out of the crate.
- Adding new math features.

## Verification

- `cargo test --manifest-path field-gen/Cargo.toml`
- Diff should be mechanical rename/import changes only.
