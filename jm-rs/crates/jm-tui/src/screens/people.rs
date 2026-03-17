//! Stakeholder view — master/detail layout showing people and their pending items.
//!
//! Layout:
//! ┌─ People ────────────────┬─ @carol ─────────────────────────────┐
//! │ ▶ @carol (1 pending)    │ Role: Display Systems Lead           │
//! │   @bob (1 pending)      │ Projects: HMI Framework              │
//! │   @alice (0 pending)    │                                      │
//! │                          │ Pending:                             │
//! │                          │ ⊘ spec clarification (3 days)       │
//! └──────────────────────────┴──────────────────────────────────────┘

use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use jm_core::models::Person;
use jm_core::storage::PeopleStore;

use crate::events::Action;
use crate::theme;

// ── State ───────────────────────────────────────────────────────────

pub struct PeopleState {
    pub selected: usize,
    pub people: Vec<Person>,
}

// ── Init ────────────────────────────────────────────────────────────

pub fn init(people_store: &PeopleStore) -> PeopleState {
    let file = people_store.load();
    PeopleState {
        selected: 0,
        people: file.people,
    }
}

// ── Refresh ─────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn refresh(state: &mut PeopleState, people_store: &PeopleStore) {
    let file = people_store.load();
    state.people = file.people;
    // Clamp selection after reload
    if !state.people.is_empty() && state.selected >= state.people.len() {
        state.selected = state.people.len() - 1;
    }
}

// ── Render ──────────────────────────────────────────────────────────

pub fn render(state: &PeopleState, frame: &mut Frame, area: Rect) {
    // Master (left) / Detail (right) split — 28 cols for the list
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(0),
        ])
        .split(area);

    render_list(state, frame, chunks[0]);
    render_detail(state, frame, chunks[1]);
}

// ── Master list ──────────────────────────────────────────────────────

fn render_list(state: &PeopleState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" People ")
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
        .border_style(theme::focused_border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.people.is_empty() {
        let para = Paragraph::new("  No people tracked yet.\n  Use @mentions in notes.")
            .style(theme::empty_hint());
        frame.render_widget(para, inner);
        return;
    }

    let items: Vec<ListItem> = state
        .people
        .iter()
        .enumerate()
        .map(|(i, person)| build_person_list_item(person, i, state.selected))
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn build_person_list_item(person: &Person, index: usize, selected: usize) -> ListItem<'static> {
    let is_selected = index == selected;
    let selector = if is_selected { "▶ " } else { "  " };
    let pending_count = person.pending.len();

    let pending_label = match pending_count {
        0 => "(0 pending)".to_string(),
        1 => "(1 pending)".to_string(),
        n => format!("({n} pending)"),
    };

    let handle_style = if is_selected {
        theme::person_style()
            .patch(Style::default().bg(theme::SELECTED_BG))
            .add_modifier(Modifier::BOLD)
    } else {
        theme::person_style()
    };

    let pending_style = if pending_count > 0 {
        if is_selected {
            Style::default()
                .fg(theme::TEXT_WARNING)
                .bg(theme::SELECTED_BG)
        } else {
            Style::default().fg(theme::TEXT_WARNING)
        }
    } else {
        if is_selected {
            theme::dim().patch(Style::default().bg(theme::SELECTED_BG))
        } else {
            theme::dim()
        }
    };

    let selector_style = if is_selected {
        Style::default()
            .fg(theme::TEXT_ACCENT)
            .bg(theme::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        theme::dim()
    };

    let bg = if is_selected {
        Style::default().bg(theme::SELECTED_BG)
    } else {
        Style::default()
    };

    let line = Line::from(vec![
        Span::styled(selector.to_string(), selector_style),
        Span::styled(person.handle.clone(), handle_style),
        Span::styled(" ".to_string(), bg),
        Span::styled(pending_label, pending_style),
    ]);

    ListItem::new(line)
}

// ── Detail panel ─────────────────────────────────────────────────────

fn render_detail(state: &PeopleState, frame: &mut Frame, area: Rect) {
    let (title, content) = if state.people.is_empty() {
        (" People ".to_string(), Vec::new())
    } else if let Some(person) = state.people.get(state.selected) {
        let title = format!(" {} ", person.handle);
        let content = build_detail_lines(person);
        (title, content)
    } else {
        (" People ".to_string(), Vec::new())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.people.is_empty() {
        let para = Paragraph::new("No stakeholders recorded.")
            .style(theme::empty_hint());
        frame.render_widget(para, inner);
        return;
    }

    let para = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

fn build_detail_lines(person: &Person) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    // Role
    if person.role.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Role: ", theme::bold()),
            Span::styled("—", theme::dim()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Role: ", theme::bold()),
            Span::raw(person.role.clone()),
        ]));
    }

    lines.push(Line::from(""));

    // Projects
    if person.projects.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Projects: ", theme::bold()),
            Span::styled("none", theme::dim()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Projects: ", theme::bold()),
            Span::raw(person.projects.join(", ")),
        ]));
    }

    lines.push(Line::from(""));

    // Pending items
    lines.push(Line::from(Span::styled("Pending:", theme::bold())));

    if person.pending.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No pending items",
            theme::empty_hint(),
        )));
    } else {
        let today = Local::now().date_naive();
        for item in &person.pending {
            let age_label = if let Some(since) = item.since {
                let days = (today - since).num_days();
                match days {
                    0 => " (today)".to_string(),
                    1 => " (1 day)".to_string(),
                    n => format!(" ({n} days)"),
                }
            } else {
                String::new()
            };

            let age_style = if let Some(since) = item.since {
                let days = (today - since).num_days();
                if days >= 7 {
                    Style::default().fg(theme::TEXT_ERROR)
                } else if days >= 3 {
                    Style::default().fg(theme::TEXT_WARNING)
                } else {
                    theme::dim()
                }
            } else {
                theme::dim()
            };

            let project_label = if let Some(ref proj) = item.project {
                format!(" [{proj}]")
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::styled("  ⊘ ", Style::default().fg(theme::TEXT_ERROR)),
                Span::raw(item.description.clone()),
                Span::styled(age_label, age_style),
                Span::styled(project_label, theme::tag_style()),
            ]));
        }
    }

    lines
}

// ── Key handling ────────────────────────────────────────────────────

pub fn handle_key(state: &mut PeopleState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Action::Back,

        KeyCode::Char('j') | KeyCode::Down => {
            if !state.people.is_empty() {
                state.selected = (state.selected + 1).min(state.people.len() - 1);
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
            Action::None
        }
        KeyCode::Char('g') => {
            state.selected = 0;
            Action::None
        }
        KeyCode::Char('G') => {
            if !state.people.is_empty() {
                state.selected = state.people.len() - 1;
            }
            Action::None
        }

        _ => Action::None,
    }
}
