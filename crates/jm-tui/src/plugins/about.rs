//! About screen plugin — displays jm version and build info.
//!
//! Implements [`ScreenPlugin`] directly (no base trait). Opened via the `J` keybinding
//! from the dashboard. Returns to Dashboard on Esc or q.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::{PluginAction, ScreenPlugin};

/// Version info for display — kept in one place for easy updates.
const JM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Full-screen About plugin.
///
/// Shows jm version info centered on the terminal. No mutable state needed —
/// all content is static. `on_enter` and `on_leave` are no-ops.
pub struct AboutPlugin {
    version: String,
}

impl AboutPlugin {
    /// Create a new AboutPlugin. Always registered — no config required.
    pub fn new() -> Self {
        Self {
            version: JM_VERSION.to_string(),
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
        // Clear the full area first so any underlying content is hidden.
        frame.render_widget(Clear, area);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "jm",
                    Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  v{}", self.version),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Job Manager — personal task & context-switch TUI",
                Style::default().fg(ratatui::style::Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Storage: ~/.jm/   (markdown + YAML frontmatter)",
                Style::default().fg(ratatui::style::Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press q or Esc to return",
                Style::default().fg(ratatui::style::Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" About jm ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ratatui::style::Color::Cyan)),
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,
            _ => PluginAction::None,
        }
    }

    fn on_enter(&mut self) {
        // No-op — About screen has no initialization work.
    }

    fn on_leave(&mut self) {
        // No-op — About screen has no cleanup work.
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Esc", "back"), ("q", "back")]
    }
}
