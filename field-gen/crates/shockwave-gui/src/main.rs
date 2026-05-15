mod app;
mod gpu_common;
mod gpu_mesh_pipeline;
mod gpu_preview;
mod gpu_scene_preview;
mod gpu_toolpath_preview;
mod settings_form;

fn main() -> iced::Result {
    app::run()
}
