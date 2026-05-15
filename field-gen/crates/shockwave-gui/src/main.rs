use iced::widget::{column, container, text};
use iced::{Element, Fill, Theme};

fn main() -> iced::Result {
    iced::application(ShockwaveGui::new, update, view)
        .theme(theme)
        .centered()
        .run()
}

#[derive(Debug)]
struct ShockwaveGui {
    status: String,
}

impl ShockwaveGui {
    fn new() -> Self {
        Self {
            status: "Ready".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
enum Message {}

fn update(_state: &mut ShockwaveGui, message: Message) {
    match message {}
}

fn view(state: &ShockwaveGui) -> Element<'_, Message> {
    container(column![
        text("Shockwave Slicer").size(32),
        text(&state.status).size(16),
    ])
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}

fn theme(_state: &ShockwaveGui) -> Theme {
    Theme::TokyoNight
}
