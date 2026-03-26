//! Weekly review screen — 7-day activity chart + issues closed + time per project.
//!
//! Layout:
//!   Top 50%:  GitHub-style activity bar chart (7 days, oldest left → newest right)
//!   Bot 50%:  Two columns — Issues Closed (left) | Time Per Project (right)

use chrono::{Datelike, Duration, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use jm_core::storage::{IssueStore, JournalStore};
use jm_core::time as jm_time;

use crate::events::Action;
use crate::theme;

// ── State ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum WeeklySection {
    Chart,
    IssuesClosed,
    TimePerProject,
}

impl WeeklySection {
    fn next(&self) -> WeeklySection {
        match self {
            WeeklySection::Chart => WeeklySection::IssuesClosed,
            WeeklySection::IssuesClosed => WeeklySection::TimePerProject,
            WeeklySection::TimePerProject => WeeklySection::Chart,
        }
    }

    fn prev(&self) -> WeeklySection {
        match self {
            WeeklySection::Chart => WeeklySection::TimePerProject,
            WeeklySection::IssuesClosed => WeeklySection::Chart,
            WeeklySection::TimePerProject => WeeklySection::IssuesClosed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DayData {
    pub date: NaiveDate,
    pub entry_count: usize,
    pub session_minutes: u64,
    #[allow(dead_code)] // available for future chart enhancements
    pub notes_count: usize,
    pub switches_count: usize,
}

#[derive(Debug, Clone)]
pub struct WeeklyState {
    pub section: WeeklySection,
    pub selected: usize,
    pub week_data: Vec<DayData>,
    /// (slug, id, title)
    pub issues_closed: Vec<(String, u32, String)>,
    /// (project_name, minutes)
    pub time_per_project: Vec<(String, u64)>,
}

// ── Init ─────────────────────────────────────────────────────────────

pub fn init(journal_store: &JournalStore, issue_store: &IssueStore) -> WeeklyState {
    let today = Local::now().date_naive();
    let now_time = Local::now().time();
    let week_start = today - Duration::days(6);

    // ── Build per-day data ───────────────────────────────────────────
    let mut week_data: Vec<DayData> = Vec::new();
    // Accumulate time per project across all 7 days.
    let mut time_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for offset in 0..7u64 {
        let date = week_start + Duration::days(offset as i64);
        let day_data = if let Some(journal) = journal_store.get_day(date) {
            let sessions = jm_time::compute_sessions(&journal);
            let agg = jm_time::aggregate_sessions(&sessions, now_time);

            let total_minutes: i64 = agg.iter().map(|(_, d)| d.num_minutes()).sum();
            let notes_count = journal
                .entries
                .iter()
                .filter(|e| e.entry_type == "Note")
                .count();
            let switches_count = journal
                .entries
                .iter()
                .filter(|e| e.entry_type == "Switched")
                .count();

            // Accumulate into global time map
            for (project, dur) in &agg {
                let mins = dur.num_minutes().max(0) as u64;
                *time_map.entry(project.clone()).or_insert(0) += mins;
            }

            DayData {
                date,
                entry_count: journal.entries.len(),
                session_minutes: total_minutes.max(0) as u64,
                notes_count,
                switches_count,
            }
        } else {
            DayData {
                date,
                entry_count: 0,
                session_minutes: 0,
                notes_count: 0,
                switches_count: 0,
            }
        };
        week_data.push(day_data);
    }

    // ── Issues closed in the last 7 days ────────────────────────────
    let all_issue_files = issue_store.load_all();
    let mut issues_closed: Vec<(String, u32, String)> = Vec::new();
    for issue_file in &all_issue_files {
        for issue in &issue_file.issues {
            if issue
                .closed
                .map(|d| d >= week_start)
                .unwrap_or(false)
            {
                issues_closed.push((
                    issue_file.project_slug.clone(),
                    issue.id,
                    issue.title.clone(),
                ));
            }
        }
    }
    // Sort by project slug, then issue id
    issues_closed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // ── Time per project, sorted descending ─────────────────────────
    let mut time_per_project: Vec<(String, u64)> = time_map.into_iter().collect();
    time_per_project.sort_by(|a, b| b.1.cmp(&a.1));

    WeeklyState {
        section: WeeklySection::Chart,
        selected: 0,
        week_data,
        issues_closed,
        time_per_project,
    }
}

// ── Key handling ─────────────────────────────────────────────────────

pub fn handle_key(state: &mut WeeklyState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('W') => Action::Back,

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

pub fn render(state: &WeeklyState, frame: &mut Frame, area: Rect) {
    let [top_area, bottom_area] =
        Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(area);

    render_chart(state, frame, top_area);

    let [issues_area, time_area] =
        Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(bottom_area);

    render_issues_closed(state, frame, issues_area);
    render_time_per_project(state, frame, time_area);
}

// ── Chart section ────────────────────────────────────────────────────

fn render_chart(state: &WeeklyState, frame: &mut Frame, area: Rect) {
    let is_focused = state.section == WeeklySection::Chart;
    let border_style = if is_focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let total_entries: usize = state.week_data.iter().map(|d| d.entry_count).sum();
    let total_switches: usize = state.week_data.iter().map(|d| d.switches_count).sum();
    let total_minutes: u64 = state.week_data.iter().map(|d| d.session_minutes).sum();

    let title = Span::styled(
        " Activity (7 days) ",
        Style::default().fg(theme::TEXT_ACCENT),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Reserve bottom lines: 1 for day labels, 1 for counts, 1 for summary
    // Remaining height is available for bars
    let inner_height = inner.height as usize;
    if inner_height < 4 {
        // Too small to render
        return;
    }
    let bar_height = inner_height.saturating_sub(3); // labels row + count row + summary row

    let [bars_and_labels_area, summary_area] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Compute max entry_count for scaling
    let max_count = state
        .week_data
        .iter()
        .map(|d| d.entry_count)
        .max()
        .unwrap_or(0);

    // Render summary line
    let total_dur = chrono::Duration::minutes(total_minutes as i64);
    let time_str = jm_time::format_duration(total_dur);
    let summary = format!(
        "Total: {total_entries} entries | {total_switches} switches | {time_str} tracked"
    );
    frame.render_widget(
        Paragraph::new(Span::styled(summary, theme::dim())),
        summary_area,
    );

    // Each day column gets equal width
    let num_days = state.week_data.len();
    if num_days == 0 || bars_and_labels_area.width == 0 {
        return;
    }

    // Build per-column areas
    let col_constraints: Vec<Constraint> = (0..num_days)
        .map(|_| Constraint::Ratio(1, num_days as u32))
        .collect();
    let col_areas = Layout::horizontal(col_constraints).split(bars_and_labels_area);

    for (i, day) in state.week_data.iter().enumerate() {
        let col = col_areas[i];
        let has_activity = day.entry_count > 0;

        // Compute bar fill
        let filled_rows = if max_count == 0 || bar_height == 0 {
            0
        } else {
            ((day.entry_count as f64 / max_count as f64) * bar_height as f64).round() as usize
        };

        // Layout within column: bar area + label line + count line
        let [bar_area, label_area, count_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(col);

        // Render bar (top-to-bottom: empty rows first, then filled rows)
        let bar_height_actual = bar_area.height as usize;
        let empty_rows = bar_height_actual.saturating_sub(filled_rows);
        let mut bar_lines: Vec<Line> = Vec::new();

        // Empty rows (spaces)
        for _ in 0..empty_rows {
            bar_lines.push(Line::from(Span::raw(" ")));
        }
        // Filled rows (block chars)
        let bar_style = if has_activity {
            Style::default().fg(theme::TEXT_ACCENT)
        } else {
            theme::dim()
        };
        for _ in 0..filled_rows {
            bar_lines.push(Line::from(Span::styled("█", bar_style)));
        }

        frame.render_widget(
            Paragraph::new(bar_lines).alignment(Alignment::Center),
            bar_area,
        );

        // Day label
        let weekday_abbr = weekday_abbr(day.date);
        let label_style = if has_activity {
            Style::default().fg(theme::TEXT_ACCENT)
        } else {
            theme::dim()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(weekday_abbr, label_style))
                .alignment(Alignment::Center),
            label_area,
        );

        // Count below label
        let count_str = if day.entry_count > 0 {
            day.entry_count.to_string()
        } else {
            "-".to_string()
        };
        let count_style = if has_activity {
            Style::default()
        } else {
            theme::dim()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(count_str, count_style)).alignment(Alignment::Center),
            count_area,
        );
    }
}

// ── Issues closed section ────────────────────────────────────────────

fn render_issues_closed(state: &WeeklyState, frame: &mut Frame, area: Rect) {
    let is_focused = state.section == WeeklySection::IssuesClosed;
    let border_style = if is_focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let count = state.issues_closed.len();
    let title_text = if count == 0 {
        " Issues Closed ".to_string()
    } else {
        format!(" Issues Closed ({count}) ")
    };
    let title = Span::styled(title_text, Style::default().fg(theme::TEXT_ACCENT));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.issues_closed.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No issues closed this week.", theme::dim())),
            inner,
        );
        return;
    }

    let max_visible = inner.height as usize;
    let offset = if is_focused && state.selected >= max_visible {
        state.selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = state
        .issues_closed
        .iter()
        .enumerate()
        .skip(offset)
        .take(max_visible)
        .map(|(i, (slug, id, title))| {
            let selected = is_focused && i == state.selected;
            let slug_span = Span::styled(
                format!("[{slug}] "),
                Style::default().fg(theme::TEXT_ACCENT),
            );
            let issue_span = Span::styled(format!("#{id} {title}"), if selected {
                theme::selected()
            } else {
                Style::default()
            });
            ListItem::new(Line::from(vec![slug_span, issue_span]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

// ── Time per project section ─────────────────────────────────────────

fn render_time_per_project(state: &WeeklyState, frame: &mut Frame, area: Rect) {
    let is_focused = state.section == WeeklySection::TimePerProject;
    let border_style = if is_focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let title = Span::styled(
        " Time Per Project ",
        Style::default().fg(theme::TEXT_ACCENT),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.time_per_project.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No time tracked this week.", theme::dim())),
            inner,
        );
        return;
    }

    let max_visible = inner.height as usize;
    let offset = if is_focused && state.selected >= max_visible {
        state.selected - max_visible + 1
    } else {
        0
    };

    // Compute right-aligned time strings
    let inner_width = inner.width as usize;

    let items: Vec<ListItem> = state
        .time_per_project
        .iter()
        .enumerate()
        .skip(offset)
        .take(max_visible)
        .map(|(i, (project, minutes))| {
            let selected = is_focused && i == state.selected;
            let dur = chrono::Duration::minutes(*minutes as i64);
            let time_str = jm_time::format_duration(dur);

            // Pad name to fill available width so time appears right-aligned
            let name_max = inner_width.saturating_sub(time_str.len() + 1);
            let name_trunc = if project.len() > name_max {
                &project[..name_max]
            } else {
                project.as_str()
            };
            let padding = " ".repeat(inner_width.saturating_sub(name_trunc.len() + time_str.len()));

            let base_style = if selected { theme::selected() } else { Style::default() };
            let time_style = if selected {
                theme::selected()
            } else {
                Style::default().fg(theme::TEXT_ACCENT)
            };

            let line = Line::from(vec![
                Span::styled(name_trunc.to_string(), base_style),
                Span::styled(padding, base_style),
                Span::styled(time_str, time_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

// ── Helpers ──────────────────────────────────────────────────────────

fn weekday_abbr(date: NaiveDate) -> &'static str {
    match date.weekday() {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    }
}
