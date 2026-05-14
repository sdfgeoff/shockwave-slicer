# field-gen-cli

Command-line entry point for the field generation and slicing pipeline.

This crate owns CLI argument parsing, user-facing command behavior, timing output, and compatibility with the existing `field-gen` command. It should stay thin: shared slicing behavior, file IO helpers, and reusable data structures belong in library crates.
