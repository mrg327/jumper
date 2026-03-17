//! Confirmation dialog for destructive actions (delete project, etc.).
//!
//! Small centered popup with [Yes] / [No] buttons.
//! y / Enter confirms; n / Escape cancels.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::events::Action;
use crate::theme;

// ── State ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConfirmModal {
    pub title: String,
    pub message: String,
    /// The key that confirms; defaults to 'y'.
    #[allow(dead_code)]
    pub confirm_key: char,
    /// `true` = Yes button is highlighted, `false` = No button is highlighted.
    pub selected: bool,
}

impl ConfirmModal {
    pub fn new(title: &str, message: &str) -> Self {
        Self {
            title: title.to_string(),
            message: message.to_string(),
            confirm_key: 'y',
            selected: false, // default focus on No — safer for destructive actions
        }
    }

    // ── Key handling ──────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            // Confirm
            KeyCode::Char('y') | KeyCode::Enter if self.selected => {
                Action::SubmitInput("confirm".to_string())
            }
            KeyCode::Char('y') => Action::SubmitInput("confirm".to_string()),

            // Cancel
            KeyCode::Char('n') | KeyCode::Esc => Action::PopModal,

            // Toggle selection with arrow keys or h/l
            KeyCode::Left | KeyCode::Char('h') => {
                self.selected = true; // left = Yes
                Action::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected = false; // right = No
                Action::None
            }

            // Enter on the currently selected button
            KeyCode::Enter => {
                if self.selected {
                    Action::SubmitInput("confirm".to_string())
                } else {
                    Action::PopModal
                }
            }

            _ => Action::None,
        }
    }

    // ── Render ────────────────────────────────────────────────────────

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup_area = crate::modals::centered_rect(50, 25, area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::MODAL_BORDER));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Layout: blank row, message row, blank row, buttons row, blank row, footer.
        let [_, message_area, _, buttons_area, _, footer_area] = Layout::vertical([
            Constraint::Length(1), // top padding
            Constraint::Length(1), // message
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Min(0),    // flexible spacer
            Constraint::Length(1), // footer hint
        ])
        .areas(inner);

        // Message text.
        let message_para = Paragraph::new(self.message.as_str());
        frame.render_widget(message_para, message_area);

        // Buttons: [Yes]  [No]  — center them.
        let yes_style = if self.selected {
            theme::selected()
        } else {
            Style::default().fg(theme::TEXT_DIM)
        };
        let no_style = if !self.selected {
            theme::selected()
        } else {
            Style::default().fg(theme::TEXT_DIM)
        };

        let buttons_line = Line::from(vec![
            Span::raw("  "),
            Span::styled("[ Yes ]", yes_style),
            Span::raw("   "),
            Span::styled("[ No ]", no_style),
        ]);

        // Center horizontally.
        let buttons_width = "  [ Yes ]   [ No ]".len() as u16;
        let left_pad = buttons_area
            .width
            .saturating_sub(buttons_width)
            / 2;
        let centered_buttons_area = Rect {
            x: buttons_area.x + left_pad,
            width: buttons_area.width.saturating_sub(left_pad),
            ..buttons_area
        };

        let buttons_para = Paragraph::new(buttons_line);
        frame.render_widget(buttons_para, centered_buttons_area);

        // Footer hint.
        let footer = Paragraph::new("y: confirm  n/Esc: cancel  ←/→: toggle")
            .style(theme::dim());
        frame.render_widget(footer, footer_area);
    }
}
