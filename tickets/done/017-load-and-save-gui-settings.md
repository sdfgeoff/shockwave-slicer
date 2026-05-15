# Load And Save GUI Settings

## Goal

Load and save slicer settings from the OS config location using `shockwave-config`.

## Rationale

The GUI should use the same settings schema and defaults as the CLI. This should be proven before adding controls that edit individual settings.

## Scope

- Load settings from `shockwave_config::settings_path()`.
- Use `load_settings_or_default` when the settings file does not exist.
- Save settings back through `save_settings`.
- Display a small status line showing whether settings were loaded or defaulted.
- Keep the GUI using `SlicerSettings` directly.

## Out Of Scope

- Generic settings metadata.
- Full settings editor.
- User profile or recent-file state.

## Verification

- App starts when no settings file exists.
- App can save a settings file to the expected OS config path.
- `cargo test --manifest-path field-gen/Cargo.toml`
