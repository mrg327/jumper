//! Context-sensitive keybinding footer bar.
//!
//! Renders a single-line hint bar at the bottom of the terminal showing the
//! 6-8 most relevant keybindings for the current context. Hints change
//! dynamically based on screen, focus state, and whether a modal is open.
//! Inspired by zellij and lazygit's dynamic footer.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::events::{Focus, ScreenId};
use crate::theme;

/// Render the keybinding hint bar at the bottom of the screen.
///
/// `screen` is the current screen, `focus` is the current focus state,
/// `has_modal` indicates if a modal is open on top.
/// `status_spans` are rendered right-aligned (switch count, session timer, etc.)
/// `plugin_hints` are the key hints from the active screen plugin, if any.
pub fn render(
    screen: &ScreenId,
    focus: &Focus,
    has_modal: bool,
    is_kanban: bool,
    status_spans: &[Span],
    plugin_hints: Option<Vec<(&'static str, &'static str)>>,
    frame: &mut Frame,
    area: Rect,
) {
    let hints = get_hints(screen, focus, has_modal, is_kanban, plugin_hints);

    // Render as a single line: "key:action  key:action  key:action"
    // Keys in bold/accent, descriptions in dim gray.
    let mut spans = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::TEXT_ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(":{}", desc),
            Style::default().fg(theme::TEXT_DIM),
        ));
    }

    // Right-align status spans if present
    if !status_spans.is_empty() {
        // Calculate used width by left-side hints
        let hints_width: usize = spans.iter().map(|s| s.width()).sum();
        let status_width: usize = status_spans.iter().map(|s| s.width()).sum();
        let area_width = area.width as usize;

        if hints_width + status_width + 2 < area_width {
            let padding = area_width - hints_width - status_width;
            spans.push(Span::raw(" ".repeat(padding)));
            spans.extend_from_slice(status_spans);
        }
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line);
    frame.render_widget(para, area);
}

fn get_hints(
    screen: &ScreenId,
    focus: &Focus,
    has_modal: bool,
    is_kanban: bool,
    plugin_hints: Option<Vec<(&'static str, &'static str)>>,
) -> Vec<(&'static str, &'static str)> {
    // Modal open — only submit/cancel are relevant.
    if has_modal {
        return vec![
            ("Enter", "submit"),
            ("Esc", "cancel"),
        ];
    }

    // Sidebar focused — show sidebar/plugin controls.
    if let Focus::Sidebar(_) = focus {
        return vec![
            ("Tab", "back"),
            ("Space", "start/pause"),
            ("+/-", "adjust"),
            ("r", "reset"),
            ("R", "reset all"),
        ];
    }

    // Screen-specific hints for the main panel.
    match screen {
        ScreenId::Plugin(_) => {
            // Return hints provided by the active screen plugin, or a minimal fallback.
            plugin_hints.unwrap_or_else(|| vec![("Esc", "back")])
        }
        ScreenId::Dashboard if is_kanban => vec![
            ("h/l", "column"),
            ("j/k", "nav"),
            ("Enter", "open"),
            ("K", "list view"),
            ("w", "work"),
            ("?", "help"),
        ],
        ScreenId::Dashboard => vec![
            ("j/k", "nav"),
            ("Enter", "open"),
            ("w", "work"),
            ("s", "switch"),
            ("m", "meeting"),
            ("n", "note"),
            ("b", "block"),
            ("/", "search"),
            ("W", "weekly"),
            ("?", "help"),
        ],
        ScreenId::ProjectView(_) => vec![
            ("Esc", "back"),
            ("e", "edit"),
            ("i", "issue"),
            ("s", "cycle"),
            ("c", "close"),
            ("n", "note"),
            ("N", "note→issue"),
            ("b", "block"),
            ("o", "editor"),
        ],
        ScreenId::Switch(_) => vec![
            ("Enter", "next"),
            ("Esc", "cancel"),
        ],
        ScreenId::Review => vec![
            ("j/k", "nav"),
            ("Tab", "section"),
            ("Esc", "back"),
        ],
        ScreenId::Search => vec![
            ("Enter", "open"),
            ("Esc", "back"),
            ("j/k", "results"),
        ],
        ScreenId::People => vec![
            ("j/k", "nav"),
            ("Esc", "back"),
        ],
        ScreenId::IssueBoard => vec![
            ("h/l", "column"),
            ("j/k", "nav"),
            ("Enter", "advance"),
            ("S", "reverse"),
            ("c", "close"),
            ("p", "filter"),
            ("D", "done col"),
            ("Esc", "back"),
        ],
        ScreenId::Weekly => vec![
            ("Tab", "section"),
            ("j/k", "nav"),
            ("Esc", "back"),
        ],
    }
}
