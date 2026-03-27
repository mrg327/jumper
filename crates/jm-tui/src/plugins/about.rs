//! `AboutPlugin` — demo full-screen plugin showing version and build info.
//!
//! This is the reference implementation of `ScreenPlugin`. Press `J` from the
//! Dashboard to open it; press `Esc` or `q` to return.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{PluginAction, ScreenPlugin};
use crate::theme;

/// Demo screen plugin — shows version and build info.
pub struct AboutPlugin {
    version: String,
}

impl AboutPlugin {
    /// Create a new `AboutPlugin` with the crate version from `Cargo.toml`.
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl Default for AboutPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenPlugin for AboutPlugin {
    fn name(&self) -> &str {
        "about"
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Outer block
        let block = Block::default()
            .title(" About jm ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::TEXT_ACCENT));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 3 {
            return;
        }

        // Centre content vertically
        let v_pad = inner.height.saturating_sub(5) / 2;
        let [_, content, _] = Layout::vertical([
            Constraint::Length(v_pad),
            Constraint::Length(5),
            Constraint::Fill(1),
        ])
        .areas(inner);

        let title = Paragraph::new(format!("jm  v{}", self.version))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme::TEXT_ACCENT)
                    .add_modifier(Modifier::BOLD),
            );

        let subtitle = Paragraph::new("Job Manager — personal task & project TUI")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_DIM));

        let hint = Paragraph::new("Esc / q — back")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_DIM));

        let [title_area, sub_area, _, hint_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(content);

        frame.render_widget(title, title_area);
        frame.render_widget(subtitle, sub_area);
        frame.render_widget(hint, hint_area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,
            _ => PluginAction::None,
        }
    }

    fn on_enter(&mut self) {
        // No-op for About screen — nothing to load or initialise.
    }

    fn on_leave(&mut self) {
        // No-op for About screen — nothing to clean up.
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Esc", "back")]
    }
}
