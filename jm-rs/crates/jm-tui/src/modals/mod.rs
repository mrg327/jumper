//! Modal popup stack system (gitui pattern).
//! Modals render as centered overlays with dim backgrounds.
//! Escape always pops the top modal.

mod input;
mod help;
mod confirm;
mod select;

pub use input::{InputModal, InputAction};
pub use help::HelpModal;
pub use confirm::ConfirmModal;
pub use select::{SelectModal, SelectAction};

use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use crate::events::Action;

/// A modal instance on the popup stack.
#[derive(Debug, Clone)]
pub enum Modal {
    Help(HelpModal),
    Input(InputModal),
    Confirm(ConfirmModal),
    Select(SelectModal),
}

impl Modal {
    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        match self {
            Modal::Help(m) => &m.title,
            Modal::Input(m) => &m.title,
            Modal::Confirm(m) => &m.title,
            Modal::Select(m) => &m.title,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match self {
            Modal::Help(m) => m.handle_key(key),
            Modal::Input(m) => m.handle_key(key),
            Modal::Confirm(m) => m.handle_key(key),
            Modal::Select(m) => m.handle_key(key),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        match self {
            Modal::Help(m) => m.render(frame, area),
            Modal::Input(m) => m.render(frame, area),
            Modal::Confirm(m) => m.render(frame, area),
            Modal::Select(m) => m.render(frame, area),
        }
    }
}

/// Render a centered popup area with a percentage of the parent.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Render a dim overlay behind modals.
pub fn render_dim_overlay(frame: &mut Frame, area: Rect) {
    let overlay = ratatui::widgets::Block::default()
        .style(Style::default().bg(Color::Black));
    frame.render_widget(overlay, area);
}
