//! Live search screen with 200ms debounce.
//!
//! Layout:
//! ┌─ Search ─────────────────────────────────────────────┐
//! │ / debugging render█                                   │
//! ├──────────────────────────────────────────────────────┤
//! │ 3 results                                             │
//! │ ▶ [project] hmi-framework:12  debugging render loop  │
//! │   [journal] 2026-03-15:5      render loop debugging  │
//! │   [project] test-infra:8      render test framework  │
//! └──────────────────────────────────────────────────────┘

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use jm_core::storage::{SearchEngine, SearchFilter, SearchResult};

use crate::events::Action;
use crate::theme;

// ── Debounce duration ───────────────────────────────────────────────

const DEBOUNCE_MS: u128 = 200;

// ── State ───────────────────────────────────────────────────────────

pub struct SearchState {
    pub query: String,
    pub cursor_pos: usize,
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub last_keypress: Instant,
    pub needs_search: bool,
}

// ── Init ────────────────────────────────────────────────────────────

pub fn init() -> SearchState {
    SearchState {
        query: String::new(),
        cursor_pos: 0,
        results: Vec::new(),
        selected: 0,
        last_keypress: Instant::now(),
        needs_search: false,
    }
}

// ── Render ──────────────────────────────────────────────────────────

pub fn render(state: &SearchState, frame: &mut Frame, area: Rect) {
    // Split: input row at top, results below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search input
            Constraint::Min(0),    // results
        ])
        .split(area);

    render_input(state, frame, chunks[0]);
    render_results(state, frame, chunks[1]);
}

fn render_input(state: &SearchState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build the prompt + query text with a cursor block appended
    // We show: "/ <query>█" where █ is the cursor position indicator
    let before_cursor = &state.query[..state.cursor_pos];
    let after_cursor = &state.query[state.cursor_pos..];

    let cursor_char = if after_cursor.is_empty() {
        " "
    } else {
        &after_cursor[..after_cursor
            .char_indices()
            .nth(1)
            .map(|(i, _)| i)
            .unwrap_or(after_cursor.len())]
    };

    let rest_after = if after_cursor.is_empty() {
        ""
    } else {
        &after_cursor[cursor_char.len()..]
    };

    let line = Line::from(vec![
        Span::styled("/ ", theme::dim()),
        Span::styled(before_cursor.to_string(), theme::accent()),
        Span::styled(
            cursor_char.to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(theme::TEXT_ACCENT),
        ),
        Span::styled(rest_after.to_string(), theme::accent()),
    ]);

    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}

fn render_results(state: &SearchState, frame: &mut Frame, area: Rect) {
    let result_count = state.results.len();
    let count_line = if result_count == 0 {
        if state.query.is_empty() {
            "Type to search".to_string()
        } else if state.needs_search {
            "Searching…".to_string()
        } else {
            "No results".to_string()
        }
    } else if result_count == 1 {
        "1 result".to_string()
    } else {
        format!("{result_count} results")
    };

    // Header line + list rows
    let block = Block::default()
        .title(format!(" {count_line} "))
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(theme::focused_border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.results.is_empty() {
        let hint = if state.query.is_empty() {
            "  Start typing to search across all projects and journal entries"
        } else {
            "  No matches found"
        };
        let para = Paragraph::new(hint).style(theme::empty_hint());
        frame.render_widget(para, inner);
        return;
    }

    // Determine scroll offset so selected item is always visible
    let visible_height = inner.height as usize;
    let offset = scroll_offset(state.selected, visible_height, result_count);

    let items: Vec<ListItem> = state
        .results
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(i, result)| build_result_item(result, i, state.selected, &state.query))
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Build a single result row with highlighted match text.
fn build_result_item(
    result: &SearchResult,
    index: usize,
    selected: usize,
    query: &str,
) -> ListItem<'static> {
    let is_selected = index == selected;
    let selector = if is_selected { "▶ " } else { "  " };

    // Badge: [project] or [journal] or [people]
    let badge_style = match result.file_type.as_str() {
        "project" => Style::default().fg(Color::Green),
        "journal" => Style::default().fg(Color::Cyan),
        "people" => Style::default().fg(Color::Blue),
        _ => theme::dim(),
    };
    let badge = format!("[{}]", result.file_type);

    // Location: slug:line or date:line
    let location = if result.file_type == "project" {
        format!("{}:{}", result.project_slug, result.line_number)
    } else {
        // For journal files, derive date from filename stem
        let stem = result
            .file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?");
        format!("{stem}:{}", result.line_number)
    };

    // Build spans for the matching line with highlighted match region
    let line_text = result.line_text.trim().to_string();
    let match_spans = highlight_match(&line_text, result.match_start, result.match_end, query);

    let base_style = if is_selected {
        theme::selected()
    } else {
        Style::default()
    };

    let mut spans: Vec<Span> = Vec::new();

    let selector_style = if is_selected {
        Style::default()
            .fg(theme::TEXT_ACCENT)
            .bg(theme::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        theme::dim()
    };
    spans.push(Span::styled(selector.to_string(), selector_style));
    spans.push(Span::styled(format!("{badge:<10}"), badge_style.patch(if is_selected { Style::default().bg(theme::SELECTED_BG) } else { Style::default() })));
    spans.push(Span::styled(
        format!("{location:<22}  "),
        theme::dim().patch(if is_selected { Style::default().bg(theme::SELECTED_BG) } else { Style::default() }),
    ));

    // Append highlighted match spans
    for span in match_spans {
        let s = if is_selected {
            Span::styled(span.content.into_owned(), span.style.patch(Style::default().bg(theme::SELECTED_BG)))
        } else {
            span
        };
        spans.push(s);
    }

    ListItem::new(Line::from(spans)).style(base_style)
}

/// Split `text` into spans, highlighting the matched region.
/// Falls back gracefully if byte indices are out of range.
fn highlight_match(text: &str, match_start: usize, match_end: usize, _query: &str) -> Vec<Span<'static>> {
    let len = text.len();
    if match_start >= len || match_end > len || match_start >= match_end {
        // No valid match range — return plain text
        return vec![Span::raw(text.to_string())];
    }

    let before = text[..match_start].to_string();
    let matched = text[match_start..match_end].to_string();
    let after = text[match_end..].to_string();

    let mut spans = Vec::new();
    if !before.is_empty() {
        spans.push(Span::styled(before, Style::default().fg(theme::TEXT_PRIMARY)));
    }
    spans.push(Span::styled(
        matched,
        Style::default()
            .fg(Color::Black)
            .bg(theme::TEXT_ACCENT)
            .add_modifier(Modifier::BOLD),
    ));
    if !after.is_empty() {
        spans.push(Span::styled(after, Style::default().fg(theme::TEXT_PRIMARY)));
    }
    spans
}

/// Compute scroll offset so `selected` is always in the visible window.
fn scroll_offset(selected: usize, visible: usize, total: usize) -> usize {
    if visible == 0 || total == 0 {
        return 0;
    }
    if selected < visible {
        0
    } else {
        // Keep selected item at the bottom of the window
        let max_offset = total.saturating_sub(visible);
        (selected + 1).saturating_sub(visible).min(max_offset)
    }
}

// ── Key handling ────────────────────────────────────────────────────

pub fn handle_key(state: &mut SearchState, key: KeyEvent) -> Action {
    match key.code {
        // ── Exit ──────────────────────────────────────────────────
        KeyCode::Esc => return Action::Back,

        // ── Open selected result ───────────────────────────────────
        KeyCode::Enter => {
            if let Some(result) = state.results.get(state.selected) {
                if result.file_type == "project" && !result.project_slug.is_empty() {
                    let slug = result.project_slug.clone();
                    return Action::PushScreen(crate::events::ScreenId::ProjectView(slug));
                }
            }
            return Action::None;
        }

        // ── Navigation (when not editing — j/k/g/G) ───────────────
        KeyCode::Char('j') | KeyCode::Down => {
            if !state.results.is_empty() {
                state.selected = (state.selected + 1).min(state.results.len() - 1);
            }
            return Action::None;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
            return Action::None;
        }
        KeyCode::Char('g') => {
            state.selected = 0;
            return Action::None;
        }
        KeyCode::Char('G') => {
            if !state.results.is_empty() {
                state.selected = state.results.len() - 1;
            }
            return Action::None;
        }

        // ── Text editing ───────────────────────────────────────────
        KeyCode::Char(c) => {
            // Reject Ctrl+<char> combos (except Ctrl+A/E which are cursor moves)
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'a' => {
                        state.cursor_pos = 0;
                        return Action::None;
                    }
                    'e' => {
                        state.cursor_pos = state.query.len();
                        return Action::None;
                    }
                    _ => return Action::None,
                }
            }
            state.query.insert(state.cursor_pos, c);
            state.cursor_pos += c.len_utf8();
            mark_dirty(state);
        }
        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                // Find the char boundary just before cursor
                let mut new_pos = state.cursor_pos - 1;
                while !state.query.is_char_boundary(new_pos) {
                    new_pos -= 1;
                }
                state.query.remove(new_pos);
                state.cursor_pos = new_pos;
                mark_dirty(state);
            }
        }
        KeyCode::Delete => {
            if state.cursor_pos < state.query.len() {
                state.query.remove(state.cursor_pos);
                mark_dirty(state);
            }
        }
        KeyCode::Home => {
            state.cursor_pos = 0;
        }
        KeyCode::End => {
            state.cursor_pos = state.query.len();
        }
        KeyCode::Left => {
            if state.cursor_pos > 0 {
                let mut new_pos = state.cursor_pos - 1;
                while !state.query.is_char_boundary(new_pos) {
                    new_pos -= 1;
                }
                state.cursor_pos = new_pos;
            }
        }
        KeyCode::Right => {
            if state.cursor_pos < state.query.len() {
                let next = state.query[state.cursor_pos..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| state.cursor_pos + i)
                    .unwrap_or(state.query.len());
                state.cursor_pos = next;
            }
        }

        _ => {}
    }

    Action::None
}

/// Mark state as needing a search, recording keypress time for debounce.
fn mark_dirty(state: &mut SearchState) {
    state.needs_search = true;
    state.last_keypress = Instant::now();
    // Reset selection on new query
    state.selected = 0;
}

// ── Debounced search ────────────────────────────────────────────────

/// Called from the event loop on every tick / event.
/// Fires the search if debounce has elapsed and state is dirty.
pub fn maybe_search(state: &mut SearchState, engine: &SearchEngine) {
    if !state.needs_search {
        return;
    }
    if state.last_keypress.elapsed().as_millis() < DEBOUNCE_MS {
        return;
    }

    // Debounce elapsed — run the search
    state.needs_search = false;

    if state.query.trim().is_empty() {
        state.results.clear();
        state.selected = 0;
        return;
    }

    state.results = engine.search(&SearchFilter {
        query: state.query.clone(),
        ..Default::default()
    });

    // Clamp selection
    if !state.results.is_empty() && state.selected >= state.results.len() {
        state.selected = state.results.len() - 1;
    }
}
