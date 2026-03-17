//! Help modal showing all keybindings for the current screen.
//!
//! Two-column layout: key on the left (dim), description on the right (normal).
//! Supports scrolling with j/k and g/G.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::events::Action;
use crate::theme;

// ── State ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HelpModal {
    pub title: String,
    pub screen: String,
    pub scroll: usize,
}

impl HelpModal {
    pub fn new(screen: &str) -> Self {
        let title = match screen {
            "project_view" => "Help — Project View",
            _ => "Help — Dashboard",
        };
        Self {
            title: title.to_string(),
            screen: screen.to_string(),
            scroll: 0,
        }
    }

    // ── Key handling ──────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        let entries = self.entries();
        let total = entries.len();

        match key.code {
            // Dismiss
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Action::PopModal,

            // Scroll down
            KeyCode::Char('j') | KeyCode::Down => {
                if self.scroll + 1 < total {
                    self.scroll += 1;
                }
                Action::None
            }

            // Scroll up
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::None
            }

            // Top
            KeyCode::Char('g') => {
                self.scroll = 0;
                Action::None
            }

            // Bottom
            KeyCode::Char('G') => {
                self.scroll = total.saturating_sub(1);
                Action::None
            }

            _ => Action::None,
        }
    }

    // ── Render ────────────────────────────────────────────────────────

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup_area = crate::modals::centered_rect(70, 80, area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::MODAL_BORDER));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Split inner: content area + footer hint row.
        let [content_area, footer_area] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(inner);

        // Build all rows, skip the scrolled-off ones.
        let entries = self.entries();
        let visible_height = content_area.height as usize;
        let key_col_width = 12u16; // fixed width for the key column

        let [key_col_area, desc_col_area] = Layout::horizontal([
            Constraint::Length(key_col_width),
            Constraint::Min(0),
        ])
        .areas(content_area);

        let visible: Vec<&(&str, &str)> = entries
            .iter()
            .skip(self.scroll)
            .take(visible_height)
            .collect();

        // Render key column (dim).
        let key_lines: Vec<Line> = visible
            .iter()
            .map(|(key, _)| Line::from(Span::styled(*key, theme::dim())))
            .collect();
        let key_para = Paragraph::new(Text::from(key_lines));
        frame.render_widget(key_para, key_col_area);

        // Render description column (normal).
        let desc_lines: Vec<Line> = visible
            .iter()
            .map(|(_, desc)| {
                if desc.is_empty() {
                    // Section header — render as bold accent.
                    Line::from(Span::styled("", Style::default()))
                } else {
                    Line::from(Span::raw(*desc))
                }
            })
            .collect();
        let desc_para = Paragraph::new(Text::from(desc_lines)).wrap(Wrap { trim: false });
        frame.render_widget(desc_para, desc_col_area);

        // Scroll indicator + footer hint.
        let total = entries.len();
        let scroll_info = if total > visible_height {
            format!(
                " {}/{} ",
                self.scroll + 1,
                total
            )
        } else {
            String::new()
        };

        let footer_text = format!(
            "{}j/k: scroll  g/G: top/bottom  Esc/q: close",
            scroll_info
        );
        let footer = Paragraph::new(footer_text).style(theme::dim());
        frame.render_widget(footer, footer_area);
    }

    // ── Keybinding tables ─────────────────────────────────────────────

    /// Returns a flat list of (key, description) pairs for the current screen.
    /// Empty description marks a section separator / blank line.
    fn entries(&self) -> Vec<(&'static str, &'static str)> {
        match self.screen.as_str() {
            "project_view" => project_view_entries(),
            _ => dashboard_entries(),
        }
    }
}

fn dashboard_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        // Navigation
        ("j / k",        "Navigate down / up"),
        ("g / G",        "Jump to first / last"),
        ("Ctrl+D",       "Half page down"),
        ("Ctrl+U",       "Half page up"),
        ("",             ""),
        // Actions
        ("Enter",        "Open project"),
        ("w",            "Start working"),
        ("s",            "Switch context (capture prompt)"),
        ("n",            "Quick note"),
        ("b",            "Log blocker"),
        ("d",            "Log decision"),
        ("u",            "Unblock"),
        ("",             ""),
        // Views
        ("/",            "Search"),
        ("r",            "Morning review"),
        ("p",            "People view"),
        ("I",            "Inbox"),
        ("K",            "Toggle kanban / list view"),
        ("a",            "Add project"),
        ("f",            "Stop work"),
        ("",             ""),
        // Sidebar
        ("P",            "Toggle sidebar"),
        ("Tab",          "Focus sidebar"),
        ("",             ""),
        // Misc
        ("Ctrl+E",       "Export screen"),
        ("q",            "Quit"),
        ("?",            "This help"),
    ]
}

fn project_view_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        // Navigation
        ("Escape",       "Back to dashboard"),
        ("j / k",        "Scroll down / up"),
        ("g / G",        "Scroll to top / bottom"),
        ("",             ""),
        // Editing
        ("e",            "Edit current focus"),
        ("S",            "Cycle status (active/blocked/pending/parked/done)"),
        ("P",            "Cycle priority (high/medium/low)"),
        ("t",            "Edit tags"),
        ("T",            "Edit target date"),
        ("",             ""),
        // Notes & blockers
        ("n",            "Quick note"),
        ("b",            "Log blocker"),
        ("d",            "Log decision"),
        ("u",            "Unblock"),
        ("m",            "Move / edit blocker"),
        ("",             ""),
        // Cross-links
        ("",             "Use [[slug]] in notes to cross-link projects"),
        ("",             ""),
        // Danger
        ("x",            "Delete project (confirmation required)"),
    ]
}
