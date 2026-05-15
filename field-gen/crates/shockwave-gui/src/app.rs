use std::path::PathBuf;

use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill, Theme};
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
    status: String,
}

impl ShockwaveGui {
    fn new() -> Self {
        let mut app = Self {
            settings: SlicerSettings::default(),
            settings_path: None,
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
}

#[derive(Clone, Debug)]
enum Message {
    SaveSettings,
}

fn update(state: &mut ShockwaveGui, message: Message) {
    match message {
        Message::SaveSettings => state.save_settings(),
    }
}

fn view(state: &ShockwaveGui) -> Element<'_, Message> {
    let settings_path = state
        .settings_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "Unavailable".to_string());

    container(
        column![
            text("Shockwave Slicer").size(32),
            text(&state.status).size(16),
            row![text("Settings path:"), text(settings_path)].spacing(8),
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
