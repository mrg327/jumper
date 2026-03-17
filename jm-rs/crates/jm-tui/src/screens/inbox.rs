//! Inbox screen — view and manage captured thoughts.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use jm_core::models::InboxItem;
use jm_core::storage::InboxStore;

use crate::events::Action;
use crate::theme;

// ── State ────────────────────────────────────────────────────────────

pub struct InboxState {
    pub items: Vec<InboxItem>,
    pub selected: usize,
    pub scroll_offset: usize,
}

// ── Public API ───────────────────────────────────────────────────────

pub fn init(inbox_store: &InboxStore) -> InboxState {
    let inbox = inbox_store.load();
    // Filter to show only non-refiled items first, then refiled
    let items = inbox.items;
    InboxState {
        items,
        selected: 0,
        scroll_offset: 0,
    }
}

pub fn refresh(state: &mut InboxState, inbox_store: &InboxStore) {
    let inbox = inbox_store.load();
    let current_selected = state.selected;
    state.items = inbox.items;
    state.selected = current_selected.min(state.items.len().saturating_sub(1));
}

pub fn render(state: &InboxState, frame: &mut Frame, area: Rect) {
    let active_count = state.items.iter().filter(|i| i.refiled_to.is_none()).count();
    let title = format!(" INBOX ({active_count}) ");

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    if state.items.is_empty() {
        let para = Paragraph::new("Inbox is empty. Use `jm inbox \"thought\"` to capture ideas.")
            .style(theme::empty_hint())
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let visible_rows = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = state
        .items
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_rows)
        .map(|(idx, item)| {
            let is_selected = idx == state.selected;
            build_item(item, is_selected)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

pub fn handle_key(state: &mut InboxState, key: KeyEvent) -> Action {
    let len = state.items.len();

    match key.code {
        KeyCode::Esc => Action::Back,

        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                state.selected = (state.selected + 1).min(len - 1);
                clamp_scroll(state);
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if len > 0 {
                state.selected = state.selected.saturating_sub(1);
                clamp_scroll(state);
            }
            Action::None
        }
        KeyCode::Char('g') => {
            state.selected = 0;
            state.scroll_offset = 0;
            Action::None
        }
        KeyCode::Char('G') => {
            if len > 0 {
                state.selected = len - 1;
                clamp_scroll(state);
            }
            Action::None
        }

        // Refile to project — signal to app.rs to open refile input modal
        KeyCode::Char('r') => {
            if !state.items.is_empty() && state.items[state.selected].refiled_to.is_none() {
                Action::Toast("refile_inbox_item".to_string())
            } else {
                Action::None
            }
        }

        // Delete
        KeyCode::Char('d') => {
            if !state.items.is_empty() {
                // Return a special action — we'll handle it in app.rs
                Action::Toast("delete_inbox_item".to_string())
            } else {
                Action::None
            }
        }

        _ => Action::None,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn build_item(item: &InboxItem, is_selected: bool) -> ListItem<'static> {
    let name_style = if is_selected {
        theme::selected()
    } else {
        Style::default()
    };

    if let Some(ref slug) = item.refiled_to {
        // Refiled item — dimmed with strikethrough
        let line = Line::from(vec![
            Span::styled("  ", name_style),
            Span::styled(
                item.timestamp.clone(),
                Style::default()
                    .fg(ratatui::style::Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            Span::styled(
                format!(" | {}", item.text),
                Style::default()
                    .fg(ratatui::style::Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            Span::styled(format!(" -> {slug}"), theme::accent()),
        ]);
        ListItem::new(line)
    } else {
        let line = Line::from(vec![
            Span::styled("  ", name_style),
            Span::styled(item.timestamp.clone(), theme::timestamp_style()),
            Span::styled(" | ", theme::dim()),
            Span::styled(item.text.clone(), name_style),
        ]);
        ListItem::new(line)
    }
}

fn clamp_scroll(state: &mut InboxState) {
    const PAGE: usize = 20;
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + PAGE {
        state.scroll_offset = state.selected - PAGE + 1;
    }
}
