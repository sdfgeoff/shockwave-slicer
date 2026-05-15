use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use iced::widget::{button, column, container, progress_bar, row, scrollable, text};
use iced::{Element, Fill, Subscription, Theme, time};
use rfd::FileDialog;
use shockwave_config::{SlicerSettings, load_settings_or_default, save_settings, settings_path};
use shockwave_math::geometry::Triangle;
use shockwave_path::LayerToolpaths;
use shockwave_slicer::{CancellationToken, SliceProgress};
use shockwave_slicer_io::{
    SliceDebugOutput, SliceJobOutput, SliceJobRequest, load_stl_model, run_slice_job,
};

use crate::settings_form::{SettingsForm, SettingsMessage};
use crate::{gpu_preview, gpu_toolpath_preview, preview_canvas};

pub fn run() -> iced::Result {
    iced::application(ShockwaveGui::new, update, view)
        .subscription(subscription)
        .theme(theme)
        .centered()
        .run()
}

#[derive(Debug)]
struct ShockwaveGui {
    settings: SlicerSettings,
    settings_form: SettingsForm,
    settings_path: Option<PathBuf>,
    input_path: Option<PathBuf>,
    output_prefix: Option<PathBuf>,
    preview_triangles: Vec<Triangle>,
    preview_layers: Vec<LayerToolpaths>,
    slice_job: Option<SliceJobState>,
    status: String,
}

#[derive(Debug)]
struct SliceJobState {
    cancellation: CancellationToken,
    receiver: Receiver<SliceJobEvent>,
    progress: Option<SliceProgress>,
}

#[derive(Debug)]
enum SliceJobEvent {
    Progress(SliceProgress),
    Finished(Result<SliceJobOutput, String>),
}

impl ShockwaveGui {
    fn new() -> Self {
        let mut app = Self {
            settings: SlicerSettings::default(),
            settings_form: SettingsForm::from_settings(&SlicerSettings::default()),
            settings_path: None,
            input_path: None,
            output_prefix: None,
            preview_triangles: Vec::new(),
            preview_layers: Vec::new(),
            slice_job: None,
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
                        self.settings_form = SettingsForm::from_settings(&self.settings);
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
        if let Err(errors) = self.settings_form.apply_to_settings(&mut self.settings) {
            self.status = format!("Invalid settings: {}", errors.join("; "));
            return;
        }

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
            match load_stl_model(&path) {
                Ok(triangles) => {
                    self.status = format!(
                        "Selected STL {} ({} triangles)",
                        path.display(),
                        triangles.len()
                    );
                    self.preview_triangles = triangles;
                    self.preview_layers.clear();
                    self.input_path = Some(path);
                }
                Err(error) => {
                    self.status = error;
                    self.preview_triangles.clear();
                    self.preview_layers.clear();
                    self.input_path = Some(path);
                }
            }
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
        self.input_path.is_some() && self.output_prefix.is_some() && self.slice_job.is_none()
    }

    fn start_slice(&mut self) {
        let Some(input) = self.input_path.clone() else {
            self.status = "Select an STL before slicing".to_string();
            return;
        };
        let Some(output_prefix) = self.output_prefix.clone() else {
            self.status = "Select an output path before slicing".to_string();
            return;
        };
        if let Err(errors) = self.settings_form.apply_to_settings(&mut self.settings) {
            self.status = format!("Invalid settings: {}", errors.join("; "));
            return;
        }
        if let Err(errors) = self.settings.validate() {
            self.status = format!("Invalid settings: {}", errors.join("; "));
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let settings = self.settings.clone();
        let request = SliceJobRequest {
            input,
            output_prefix,
            debug_output: SliceDebugOutput {
                export_ply: settings.output.export_ply,
                gcode: settings.output.gcode,
            },
            kernel_path: settings.field.kernel_path.clone(),
        };

        thread::spawn(move || {
            let progress_sender = sender.clone();
            let mut progress = move |event| {
                let _ = progress_sender.send(SliceJobEvent::Progress(event));
            };
            let mut timing = ignore_timing;
            let result = run_slice_job(
                &request,
                &settings,
                &mut progress,
                &mut timing,
                &worker_cancellation,
            );
            let _ = sender.send(SliceJobEvent::Finished(result));
        });

        self.status = "Slicing started".to_string();
        self.slice_job = Some(SliceJobState {
            cancellation,
            receiver,
            progress: None,
        });
    }

    fn cancel_slice(&mut self) {
        if let Some(job) = &self.slice_job {
            job.cancellation.cancel();
            self.status = "Cancelling slice".to_string();
        }
    }

    fn poll_slice_events(&mut self) {
        let Some(job) = self.slice_job.as_mut() else {
            return;
        };

        let mut finished = None;
        while let Ok(event) = job.receiver.try_recv() {
            match event {
                SliceJobEvent::Progress(progress) => {
                    self.status = progress.message.clone();
                    job.progress = Some(progress);
                }
                SliceJobEvent::Finished(result) => {
                    finished = Some(result);
                    break;
                }
            }
        }

        if let Some(result) = finished {
            self.slice_job = None;
            match result {
                Ok(output) => {
                    self.status =
                        format!("Slice complete: wrote {}", output.paths.metadata.display());
                    self.preview_layers = output.layers;
                }
                Err(error) => {
                    self.status = error;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Message {
    SaveSettings,
    SelectStl,
    SelectOutput,
    Slice,
    CancelSlice,
    PollSlice,
    Settings(SettingsMessage),
}

fn update(state: &mut ShockwaveGui, message: Message) {
    match message {
        Message::SaveSettings => state.save_settings(),
        Message::SelectStl => state.select_stl(),
        Message::SelectOutput => state.select_output(),
        Message::Slice => state.start_slice(),
        Message::CancelSlice => state.cancel_slice(),
        Message::PollSlice => state.poll_slice_events(),
        Message::Settings(message) => {
            state.settings_form.update(message);
        }
    }
}

fn subscription(state: &ShockwaveGui) -> Subscription<Message> {
    if state.slice_job.is_some() {
        time::every(Duration::from_millis(100)).map(|_| Message::PollSlice)
    } else {
        Subscription::none()
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

    let content = column![
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
        row![
            button("Slice").on_press_maybe(state.can_slice().then_some(Message::Slice)),
            button("Cancel")
                .on_press_maybe(state.slice_job.is_some().then_some(Message::CancelSlice)),
        ]
        .spacing(12),
        slice_progress_view(state),
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
        text("Preview").size(24),
        preview_canvas::scene_view(
            &state.preview_triangles,
            &state.preview_layers,
            state.settings.printer.print_volume_mm
        ),
        text("GPU STL Preview").size(24),
        gpu_preview::scene_view(
            &state.preview_triangles,
            state.settings.printer.print_volume_mm
        ),
        text("GPU G-code Preview").size(24),
        gpu_toolpath_preview::scene_view(
            &state.preview_layers,
            state.settings.printer.print_volume_mm
        ),
        text("Settings").size(24),
        state.settings_form.view().map(Message::Settings),
        button("Save settings").on_press(Message::SaveSettings),
    ]
    .spacing(12);

    container(scrollable(content))
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}

fn theme(_state: &ShockwaveGui) -> Theme {
    Theme::TokyoNight
}

fn slice_progress_view(state: &ShockwaveGui) -> Element<'_, Message> {
    let Some(job) = &state.slice_job else {
        return text("No slice running").into();
    };
    let Some(progress) = &job.progress else {
        return progress_bar(0.0..=1.0, 0.0).into();
    };
    column![
        text(format!("{:?}: {}", progress.phase, progress.message)),
        progress_bar(0.0..=1.0, progress.phase_progress.clamp(0.0, 1.0)),
    ]
    .spacing(6)
    .into()
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

fn ignore_timing(_: &str, _: Duration) {}
