//! Morning review screen.
//! Shows yesterday's commitments (next_step from switch captures), journal
//! entries, open blockers across all projects, and stale projects.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use jm_core::models::{Blocker, DailyJournal, Priority, Project};

use crate::events::Action;
use crate::theme;

// ── State ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewSection {
    Commitments,
    Yesterday,
    Blockers,
    StaleProjects,
}

impl ReviewSection {
    fn next(&self) -> ReviewSection {
        match self {
            ReviewSection::Commitments => ReviewSection::Yesterday,
            ReviewSection::Yesterday => ReviewSection::Blockers,
            ReviewSection::Blockers => ReviewSection::StaleProjects,
            ReviewSection::StaleProjects => ReviewSection::Commitments,
        }
    }

    fn prev(&self) -> ReviewSection {
        match self {
            ReviewSection::Commitments => ReviewSection::StaleProjects,
            ReviewSection::Yesterday => ReviewSection::Commitments,
            ReviewSection::Blockers => ReviewSection::Yesterday,
            ReviewSection::StaleProjects => ReviewSection::Blockers,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReviewState {
    pub selected: usize,
    pub section: ReviewSection,
}

// ── Init ─────────────────────────────────────────────────────────────

pub fn init() -> ReviewState {
    ReviewState {
        selected: 0,
        section: ReviewSection::Commitments,
    }
}

// ── Key handling ─────────────────────────────────────────────────────

pub fn handle_key(state: &mut ReviewState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Action::Back,

        KeyCode::Tab => {
            state.section = state.section.next();
            state.selected = 0;
            Action::None
        }

        KeyCode::BackTab => {
            state.section = state.section.prev();
            state.selected = 0;
            Action::None
        }

        KeyCode::Char('j') | KeyCode::Down => {
            state.selected = state.selected.saturating_add(1);
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
            state.selected = usize::MAX;
            Action::None
        }

        _ => Action::None,
    }
}

// ── Rendering ────────────────────────────────────────────────────────

pub fn render(
    state: &ReviewState,
    yesterday: Option<&DailyJournal>,
    blockers: &[(String, Blocker)],
    stale: &[Project],
    frame: &mut Frame,
    area: Rect,
) {
    // Extract commitments from yesterday's journal (next_step values from
    // Switched entries and EOD reflections from Done/Reflection entries).
    let commitments = extract_commitments(yesterday);
    let eod_reflection = extract_eod_reflection(yesterday);

    // Layout: top row = commitments | eod-reflection (if any), then
    // yesterday | blockers | stale in a second row.
    // If there is no EOD reflection, commitments take the full top row.
    let (top_area, bottom_area) = {
        let rows = Layout::vertical([
            Constraint::Ratio(2, 5),
            Constraint::Ratio(3, 5),
        ])
        .split(area);
        (rows[0], rows[1])
    };

    // Top: commitments (+ optional EOD reflection alongside)
    if eod_reflection.is_some() {
        let [commit_area, eod_area] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
                .areas(top_area);
        render_commitments(state, &commitments, yesterday, frame, commit_area);
        render_eod_reflection(eod_reflection.as_ref(), frame, eod_area);
    } else {
        render_commitments(state, &commitments, yesterday, frame, top_area);
    }

    // Bottom: yesterday entries | blockers | stale
    let bottom_chunks = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(bottom_area);

    render_yesterday(state, yesterday, frame, bottom_chunks[0]);
    render_blockers(state, blockers, frame, bottom_chunks[1]);
    render_stale(state, stale, frame, bottom_chunks[2]);
}

// ── Commitment extraction ─────────────────────────────────────────────

/// A commitment captured during a context switch: project + next_step text.
pub struct Commitment {
    pub project: String,
    pub next_step: String,
}

/// Gather all `next_step` values from Switched entries in yesterday's journal.
fn extract_commitments(yesterday: Option<&DailyJournal>) -> Vec<Commitment> {
    let Some(journal) = yesterday else {
        return Vec::new();
    };
    journal
        .entries
        .iter()
        .filter(|e| e.entry_type == "Switched")
        .filter_map(|e| {
            let next_step = e.details.get("next_step")?;
            if next_step.is_empty() {
                return None;
            }
            // Project label: extract the "from" project from "A → B"
            let project = if let Some(arrow_pos) = e.project.find('\u{2192}') {
                e.project[..arrow_pos].trim().to_string()
            } else {
                e.project.clone()
            };
            Some(Commitment {
                project,
                next_step: next_step.clone(),
            })
        })
        .collect()
}

/// Extract EOD reflection from Done/Reflection entries.
/// Returns a tuple (shipped, tomorrow) if found.
fn extract_eod_reflection(yesterday: Option<&DailyJournal>) -> Option<(String, String)> {
    let journal = yesterday?;
    for entry in &journal.entries {
        if entry.entry_type == "Reflection" || entry.entry_type == "Done" {
            let shipped = entry
                .details
                .get("shipped")
                .cloned()
                .unwrap_or_default();
            let tomorrow = entry
                .details
                .get("tomorrow")
                .cloned()
                .unwrap_or_default();
            if !shipped.is_empty() || !tomorrow.is_empty() {
                return Some((shipped, tomorrow));
            }
        }
    }
    None
}

// ── Section renderers ────────────────────────────────────────────────

fn render_commitments(
    state: &ReviewState,
    commitments: &[Commitment],
    yesterday: Option<&DailyJournal>,
    frame: &mut Frame,
    area: Rect,
) {
    let is_focused = state.section == ReviewSection::Commitments;
    let border_style = if is_focused {
        Style::default().fg(theme::BORDER_FOCUSED)
    } else {
        Style::default().fg(theme::BORDER_UNFOCUSED)
    };

    let date_label = yesterday
        .map(|j| format!(" Yesterday's Commitments — {} ", j.date))
        .unwrap_or_else(|| " Yesterday's Commitments ".to_string());
    let title = Span::styled(date_label, Style::default().fg(theme::TEXT_ACCENT));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if commitments.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No commitments recorded yesterday.",
                theme::dim(),
            )),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = commitments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let selected = is_focused && i == state.selected;
            let project_span = Span::styled(
                format!("{}: ", c.project),
                Style::default().fg(theme::TEXT_ACCENT),
            );
            let step_style = if selected {
                theme::selected()
            } else {
                Style::default()
            };
            let step_span = Span::styled(&c.next_step, step_style);
            ListItem::new(Line::from(vec![project_span, step_span]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_eod_reflection(
    reflection: Option<&(String, String)>,
    frame: &mut Frame,
    area: Rect,
) {
    let border_style = Style::default().fg(theme::BORDER_UNFOCUSED);
    let title = Span::styled(
        " End-of-Day Reflection ",
        Style::default().fg(theme::TEXT_ACCENT),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match reflection {
        None => {
            frame.render_widget(
                Paragraph::new(Span::styled("No reflection recorded.", theme::dim())),
                inner,
            );
        }
        Some((shipped, tomorrow)) => {
            let mut lines: Vec<Line> = Vec::new();
            if !shipped.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Shipped: ", theme::bold()),
                    Span::raw(shipped.clone()),
                ]));
                lines.push(Line::raw(""));
            }
            if !tomorrow.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Tomorrow: ", theme::bold()),
                    Span::raw(tomorrow.clone()),
                ]));
            }
            if lines.is_empty() {
                frame.render_widget(
                    Paragraph::new(Span::styled("No reflection recorded.", theme::dim())),
                    inner,
                );
            } else {
                frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), inner);
            }
        }
    }
}

fn render_yesterday(
    state: &ReviewState,
    yesterday: Option<&DailyJournal>,
    frame: &mut Frame,
    area: Rect,
) {
    let is_focused = state.section == ReviewSection::Yesterday;
    let border_style = if is_focused {
        Style::default().fg(theme::BORDER_FOCUSED)
    } else {
        Style::default().fg(theme::BORDER_UNFOCUSED)
    };

    let date_label = yesterday
        .map(|j| format!(" Activity — {} ", j.date))
        .unwrap_or_else(|| " Activity ".to_string());
    let title = Span::styled(date_label, Style::default().fg(theme::TEXT_ACCENT));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match yesterday {
        None => {
            frame.render_widget(
                Paragraph::new(Span::styled("No journal entry found.", theme::dim())),
                inner,
            );
        }
        Some(journal) if journal.entries.is_empty() => {
            frame.render_widget(
                Paragraph::new(Span::styled("No entries recorded.", theme::dim())),
                inner,
            );
        }
        Some(journal) => {
            let items: Vec<ListItem> = journal
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let header = if entry.entry_type == "Done" {
                        format!("{} — Done for day", entry.time)
                    } else if entry.project.is_empty() {
                        format!("{} — {}", entry.time, entry.entry_type)
                    } else {
                        format!("{} — {}: {}", entry.time, entry.entry_type, entry.project)
                    };

                    let style = if is_focused && i == state.selected {
                        theme::selected()
                    } else {
                        Style::default()
                    };

                    let mut lines = vec![Line::from(Span::styled(header, style))];

                    // Show key detail fields beneath the header.
                    for (k, v) in &entry.details {
                        if v.is_empty() {
                            continue;
                        }
                        let detail = format!("  {k}: {v}");
                        lines.push(Line::from(Span::styled(detail, theme::dim())));
                    }

                    ListItem::new(lines)
                })
                .collect();

            frame.render_widget(List::new(items), inner);
        }
    }
}

fn render_blockers(
    state: &ReviewState,
    blockers: &[(String, Blocker)],
    frame: &mut Frame,
    area: Rect,
) {
    let is_focused = state.section == ReviewSection::Blockers;
    let border_style = if is_focused {
        Style::default().fg(theme::BORDER_FOCUSED)
    } else {
        Style::default().fg(theme::BORDER_UNFOCUSED)
    };

    let count = blockers.len();
    let title_text = if count == 0 {
        " Open Blockers ".to_string()
    } else {
        format!(" Open Blockers ({count}) ")
    };
    let title_color = if count > 0 {
        theme::TEXT_WARNING
    } else {
        theme::TEXT_ACCENT
    };
    let title = Span::styled(title_text, Style::default().fg(title_color));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if blockers.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No open blockers.", theme::dim())),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = blockers
        .iter()
        .enumerate()
        .map(|(i, (project_name, blocker))| {
            let style = if is_focused && i == state.selected {
                theme::selected()
            } else {
                Style::default()
            };

            let project_span = Span::styled(
                format!("{project_name}: "),
                Style::default().fg(theme::TEXT_ACCENT),
            );
            let desc_span = Span::styled(&blocker.description, style);

            let mut spans = vec![project_span, desc_span];

            if let Some(person) = &blocker.person {
                spans.push(Span::styled(
                    format!(" {person}"),
                    Style::default().fg(theme::PERSON_COLOR),
                ));
            }

            if let Some(since) = blocker.since {
                spans.push(Span::styled(
                    format!(" (since {since})"),
                    theme::dim(),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_stale(
    state: &ReviewState,
    stale: &[Project],
    frame: &mut Frame,
    area: Rect,
) {
    let is_focused = state.section == ReviewSection::StaleProjects;
    let border_style = if is_focused {
        Style::default().fg(theme::BORDER_FOCUSED)
    } else {
        Style::default().fg(theme::BORDER_UNFOCUSED)
    };

    let count = stale.len();
    let title_text = if count == 0 {
        " Stale Projects (7+ days) ".to_string()
    } else {
        format!(" Stale Projects — {count} not updated in 7+ days ")
    };
    let title_color = if count > 0 {
        theme::TEXT_WARNING
    } else {
        theme::TEXT_ACCENT
    };
    let title = Span::styled(title_text, Style::default().fg(title_color));

    // Reserve one line at the bottom for the keybind hint.
    let outer_chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(outer_chunks[0]);
    frame.render_widget(block, outer_chunks[0]);

    if stale.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("All projects up to date.", theme::dim())),
            inner,
        );
    } else {
        let items: Vec<ListItem> = stale
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let selected = is_focused && i == state.selected;

                let priority_span = match p.priority {
                    Priority::High   => Span::styled("! ", Style::default().fg(theme::PRIORITY_HIGH)),
                    Priority::Low    => Span::styled("· ", theme::dim()),
                    Priority::Medium => Span::raw("  "),
                };

                let name_span = if selected {
                    Span::styled(&p.name, theme::selected())
                } else {
                    Span::raw(&p.name)
                };

                let date_span = if let Some(last_entry) = p.log.first() {
                    Span::styled(
                        format!("  last: {}", last_entry.date),
                        theme::dim(),
                    )
                } else {
                    Span::styled("  (no log)", theme::dim())
                };

                ListItem::new(Line::from(vec![priority_span, name_span, date_span]))
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    // Global keybind hint at the very bottom of the screen
    let kb = "j/k: navigate  Tab: next section  Escape: back";
    frame.render_widget(
        Paragraph::new(Span::styled(kb, theme::dim())),
        outer_chunks[1],
    );
}
