use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill, Theme};
use rfd::FileDialog;
use shockwave_config::{SlicerSettings, load_settings_or_default, save_settings, settings_path};

pub fn run() -> iced::Result {
    iced::application(ShockwaveGui::new, update, view)
        .theme(theme)
        .centered()
        .run()
}

#[derive(Debug)]
struct ShockwaveGui {
    settings: SlicerSettings,
    settings_path: Option<PathBuf>,
    input_path: Option<PathBuf>,
    output_prefix: Option<PathBuf>,
    status: String,
}

impl ShockwaveGui {
    fn new() -> Self {
        let mut app = Self {
            settings: SlicerSettings::default(),
            settings_path: None,
            input_path: None,
            output_prefix: None,
            status: "Loading settings".to_string(),
        };
        app.load_settings();
        app
    }

    fn load_settings(&mut self) {
        match settings_path() {
            Ok(path) => {
                let existed = path.exists();
                match load_settings_or_default(&path) {
                    Ok(settings) => {
                        self.settings = settings;
                        self.settings_path = Some(path.clone());
                        self.status = if existed {
                            format!("Loaded settings from {}", path.display())
                        } else {
                            format!("Using default settings; will save to {}", path.display())
                        };
                    }
                    Err(error) => {
                        self.settings_path = Some(path);
                        self.status = error;
                    }
                }
            }
            Err(error) => {
                self.status = error;
            }
        }
    }

    fn save_settings(&mut self) {
        let Some(path) = self.settings_path.as_ref() else {
            self.status = "No settings path is available".to_string();
            return;
        };

        match save_settings(path, &self.settings) {
            Ok(()) => {
                self.status = format!("Saved settings to {}", path.display());
            }
            Err(error) => {
                self.status = error;
            }
        }
    }

    fn select_stl(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("STL mesh", &["stl"])
            .set_title("Select STL to slice")
            .pick_file()
        {
            if self.output_prefix.is_none() {
                self.output_prefix = Some(path.with_extension(""));
            }
            self.status = format!("Selected STL {}", path.display());
            self.input_path = Some(path);
        }
    }

    fn select_output(&mut self) {
        let dialog = FileDialog::new()
            .add_filter("G-code", &["gcode"])
            .set_title("Choose G-code output");
        let dialog = match self.default_output_directory() {
            Some(directory) => dialog.set_directory(directory),
            None => dialog,
        };

        if let Some(path) = dialog.save_file() {
            self.output_prefix = Some(output_prefix_from_gcode_path(&path));
            self.status = format!("Selected output {}", path.display());
        }
    }

    fn default_output_directory(&self) -> Option<&Path> {
        self.output_prefix
            .as_ref()
            .and_then(|path| path.parent())
            .or_else(|| self.input_path.as_ref().and_then(|path| path.parent()))
    }

    fn can_slice(&self) -> bool {
        self.input_path.is_some() && self.output_prefix.is_some()
    }
}

#[derive(Clone, Debug)]
enum Message {
    SaveSettings,
    SelectStl,
    SelectOutput,
    Slice,
}

fn update(state: &mut ShockwaveGui, message: Message) {
    match message {
        Message::SaveSettings => state.save_settings(),
        Message::SelectStl => state.select_stl(),
        Message::SelectOutput => state.select_output(),
        Message::Slice => {
            state.status = "Slicing is not wired yet".to_string();
        }
    }
}

fn view(state: &ShockwaveGui) -> Element<'_, Message> {
    let settings_path = state
        .settings_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "Unavailable".to_string());
    let input_path = display_path(state.input_path.as_ref(), "No STL selected");
    let gcode_path = state
        .output_prefix
        .as_ref()
        .map(|path| path.with_extension("gcode"))
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "No output selected".to_string());

    container(
        column![
            text("Shockwave Slicer").size(32),
            text(&state.status).size(16),
            row![text("Settings path:"), text(settings_path)].spacing(8),
            row![
                button("Select STL").on_press(Message::SelectStl),
                text(input_path)
            ]
            .spacing(12),
            row![
                button("Save/Export to").on_press(Message::SelectOutput),
                text(gcode_path)
            ]
            .spacing(12),
            button("Slice").on_press_maybe(state.can_slice().then_some(Message::Slice)),
            row![
                text(format!(
                    "Layer height: {:.3} mm",
                    state.settings.slicing.layer_height_mm
                )),
                text(format!(
                    "Voxel: {:.3} x {:.3} x {:.3} mm",
                    state.settings.field.voxel_size_mm.x,
                    state.settings.field.voxel_size_mm.y,
                    state.settings.field.voxel_size_mm.z
                )),
            ]
            .spacing(16),
            button("Save settings").on_press(Message::SaveSettings),
        ]
        .spacing(12),
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}

fn theme(_state: &ShockwaveGui) -> Theme {
    Theme::TokyoNight
}

fn display_path(path: Option<&PathBuf>, fallback: &str) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn output_prefix_from_gcode_path(path: &Path) -> PathBuf {
    if path.extension().and_then(|extension| extension.to_str()) == Some("gcode") {
        path.with_extension("")
    } else {
        path.to_path_buf()
    }
}
