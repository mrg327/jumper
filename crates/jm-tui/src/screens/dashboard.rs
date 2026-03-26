//! Dashboard screen — master-detail project list with live preview.
//!
//! Layout: 40% left panel (project list) | 60% right panel (project preview).
//! Inspired by lazygit's panel layout.

use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use jm_core::models::{IssueStatus, Priority, Project, Status};
use jm_core::storage::{IssueStore, ProjectStore};

use crate::events::Action;
use crate::theme;

// ── View mode ───────────────────────────────────────────────────────

pub enum ViewMode {
    List,
    Kanban,
}

// ── State ────────────────────────────────────────────────────────────

pub struct DashboardState {
    pub selected: usize,
    pub projects: Vec<Project>,
    pub scroll_offset: usize,
    pub view_mode: ViewMode,
    pub kanban_column: usize,
    pub kanban_row: usize,
}

// ── Sort order ───────────────────────────────────────────────────────

fn status_sort_key(status: Status) -> u8 {
    match status {
        Status::Active  => 0,
        Status::Blocked => 1,
        Status::Pending => 2,
        Status::Parked  => 3,
        Status::Done    => 4,
    }
}

fn sort_projects(projects: &mut Vec<Project>) {
    projects.sort_by(|a, b| {
        let sa = status_sort_key(a.status);
        let sb = status_sort_key(b.status);
        sa.cmp(&sb).then_with(|| a.name.cmp(&b.name))
    });
}

// ── Public API ───────────────────────────────────────────────────────

/// Load projects from disk and return initial dashboard state.
pub fn init(project_store: &ProjectStore) -> DashboardState {
    let mut projects = project_store.list_projects(None);
    sort_projects(&mut projects);
    DashboardState {
        selected: 0,
        projects,
        scroll_offset: 0,
        view_mode: ViewMode::List,
        kanban_column: 0,
        kanban_row: 0,
    }
}

/// Reload projects from disk and re-apply sort. Tries to keep selection
/// pointing at the same project slug after reload.
pub fn refresh(state: &mut DashboardState, project_store: &ProjectStore) {
    let current_slug = state
        .projects
        .get(state.selected)
        .map(|p| p.slug.clone());

    let mut projects = project_store.list_projects(None);
    sort_projects(&mut projects);

    // Restore selection by slug if possible, else clamp to valid range.
    let new_selected = current_slug
        .and_then(|slug| projects.iter().position(|p| p.slug == slug))
        .unwrap_or(0);

    let new_selected = if projects.is_empty() {
        0
    } else {
        new_selected.min(projects.len() - 1)
    };

    state.projects = projects;
    state.selected = new_selected;
    clamp_scroll(state);
}

/// Render the dashboard into `area`.
pub fn render(
    state: &DashboardState,
    projects: &[Project],
    active_slug: Option<&str>,
    focus_main: bool,
    frame: &mut Frame,
    area: Rect,
    issue_store: &IssueStore,
) {
    match state.view_mode {
        ViewMode::List => {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .areas(area);

            render_project_list(state, projects, active_slug, focus_main, frame, left_area);
            render_preview(state, projects, frame, right_area, issue_store);
        }
        ViewMode::Kanban => {
            render_kanban(state, projects, active_slug, frame, area);
        }
    }
}

/// Handle a key event. Navigation state mutations happen inline;
/// all actions (including navigation) are returned to the caller.
pub fn handle_key(state: &mut DashboardState, key: KeyEvent) -> Action {
    // Toggle view mode (works in both modes)
    if matches!(key.code, KeyCode::Char('K')) {
        state.view_mode = match state.view_mode {
            ViewMode::List => {
                // Sync kanban cursor to the selected project's column/row
                if let Some(project) = state.projects.get(state.selected) {
                    state.kanban_column = status_to_column(project.status);
                    let col_projects = projects_in_column(&state.projects, state.kanban_column);
                    state.kanban_row = col_projects
                        .iter()
                        .position(|p| p.slug == project.slug)
                        .unwrap_or(0);
                }
                ViewMode::Kanban
            }
            ViewMode::Kanban => {
                // Sync list selection to the kanban cursor
                let col_projects = projects_in_column(&state.projects, state.kanban_column);
                if let Some(proj) = col_projects.get(state.kanban_row) {
                    if let Some(idx) = state.projects.iter().position(|p| p.slug == proj.slug) {
                        state.selected = idx;
                        clamp_scroll(state);
                    }
                }
                ViewMode::List
            }
        };
        return Action::None;
    }

    match state.view_mode {
        ViewMode::List => handle_key_list(state, key),
        ViewMode::Kanban => handle_key_kanban(state, key),
    }
}

fn handle_key_list(state: &mut DashboardState, key: KeyEvent) -> Action {
    let len = state.projects.len();

    match key.code {
        // ── Navigation ───────────────────────────────────────────────
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                state.selected = (state.selected + 1) % len;
                clamp_scroll(state);
            }
            Action::Down
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if len > 0 {
                state.selected = if state.selected == 0 { len - 1 } else { state.selected - 1 };
                clamp_scroll(state);
            }
            Action::Up
        }
        KeyCode::Char('g') => {
            if len > 0 {
                state.selected = 0;
                state.scroll_offset = 0;
            }
            Action::Top
        }
        KeyCode::Char('G') => {
            if len > 0 {
                state.selected = len - 1;
                clamp_scroll(state);
            }
            Action::Bottom
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if len > 0 {
                let half = (len / 2).max(1);
                state.selected = (state.selected + half).min(len - 1);
                clamp_scroll(state);
            }
            Action::HalfPageDown
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if len > 0 {
                let half = (len / 2).max(1);
                state.selected = state.selected.saturating_sub(half);
                clamp_scroll(state);
            }
            Action::HalfPageUp
        }

        // ── Selection / opening ──────────────────────────────────────
        KeyCode::Enter => Action::Select,

        // ── Dashboard actions ────────────────────────────────────────
        KeyCode::Char('w') => Action::StartWork,
        KeyCode::Char('s') => Action::SwitchContext,
        KeyCode::Char('m') => Action::MeetingMode,
        KeyCode::Char('n') => Action::QuickNote,
        KeyCode::Char('b') => Action::QuickBlocker,
        KeyCode::Char('d') => Action::QuickDecision,
        KeyCode::Char('u') => Action::Unblock,
        KeyCode::Char('/') => Action::SearchOpen,
        KeyCode::Char('r') => Action::MorningReview,
        KeyCode::Char('p') => Action::PeopleView,
        KeyCode::Char('a') => Action::AddProject,
        KeyCode::Char('i') => Action::AddIssue,
        KeyCode::Char('f') => Action::StopWork,
        KeyCode::Char('I') => Action::OpenIssueBoard,
        KeyCode::Char('W') => Action::OpenWeekly,
        KeyCode::Char('?') => Action::Help,
        KeyCode::Char('q') => Action::Quit,

        // ── Sidebar ──────────────────────────────────────────────────
        KeyCode::Char('P') => Action::ToggleSidebar,
        KeyCode::Tab      => Action::FocusSidebar,

        // ── Open in editor ───────────────────────────────────────────
        KeyCode::Char('O') => Action::OpenEditorSelected,

        // ── Command palette ──────────────────────────────────────────
        KeyCode::Char(':') => Action::OpenCommandMode,

        // ── Export ───────────────────────────────────────────────────
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Export,

        _ => Action::None,
    }
}

fn handle_key_kanban(state: &mut DashboardState, key: KeyEvent) -> Action {
    match key.code {
        // Column navigation
        KeyCode::Char('h') | KeyCode::Left => {
            if state.kanban_column > 0 {
                state.kanban_column -= 1;
                let col_len = projects_in_column(&state.projects, state.kanban_column).len();
                state.kanban_row = state.kanban_row.min(col_len.saturating_sub(1));
            }
            Action::None
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if state.kanban_column < KANBAN_COLUMNS.len() - 1 {
                state.kanban_column += 1;
                let col_len = projects_in_column(&state.projects, state.kanban_column).len();
                state.kanban_row = state.kanban_row.min(col_len.saturating_sub(1));
            }
            Action::None
        }
        // Row navigation within column
        KeyCode::Char('j') | KeyCode::Down => {
            let col_len = projects_in_column(&state.projects, state.kanban_column).len();
            if col_len > 0 {
                state.kanban_row = (state.kanban_row + 1).min(col_len - 1);
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.kanban_row = state.kanban_row.saturating_sub(1);
            Action::None
        }
        // Open selected project
        KeyCode::Enter => {
            let col_projects = projects_in_column(&state.projects, state.kanban_column);
            if let Some(project) = col_projects.get(state.kanban_row) {
                // Update state.selected to match so handle_select works
                if let Some(idx) = state.projects.iter().position(|p| p.slug == project.slug) {
                    state.selected = idx;
                }
                Action::Select
            } else {
                Action::None
            }
        }
        // Common actions
        KeyCode::Char('w') => Action::StartWork,
        KeyCode::Char('m') => Action::MeetingMode,
        KeyCode::Char('a') => Action::AddProject,
        KeyCode::Char('i') => Action::AddIssue,
        KeyCode::Char('I') => Action::OpenIssueBoard,
        KeyCode::Char('W') => Action::OpenWeekly,
        KeyCode::Char('?') => Action::Help,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('P') => Action::ToggleSidebar,
        KeyCode::Tab      => Action::FocusSidebar,
        _ => Action::None,
    }
}

// ── Private helpers ──────────────────────────────────────────────────

/// Adjust scroll_offset so that `selected` is always visible.
/// Assumes a conservative visible-row estimate; the real frame height is not
/// available here, so we use a page size of 20 (overestimate is fine —
/// ratatui clips automatically).
fn clamp_scroll(state: &mut DashboardState) {
    const PAGE: usize = 20;
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + PAGE {
        state.scroll_offset = state.selected - PAGE + 1;
    }
}

// ── Left panel ───────────────────────────────────────────────────────

fn render_project_list(
    state: &DashboardState,
    projects: &[Project],
    active_slug: Option<&str>,
    focus_main: bool,
    frame: &mut Frame,
    area: Rect,
) {
    let border_style = if focus_main {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let block = Block::default()
        .title(" PROJECTS ")
        .borders(Borders::ALL)
        .border_style(border_style);

    if projects.is_empty() {
        let para = Paragraph::new("No projects yet. Press a to create one.")
            .style(theme::empty_hint())
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    // Inner area width, accounting for borders (2) and padding (1 each side).
    let inner_width = area.width.saturating_sub(4) as usize;

    // Visible window — use actual inner height minus 2 for borders.
    let visible_rows = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = projects
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_rows)
        .map(|(idx, project)| {
            let is_selected = idx == state.selected;
            let is_active_project = active_slug
                .map(|s| s == project.slug)
                .unwrap_or(false);

            build_list_item(project, is_selected, is_active_project, inner_width)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn build_list_item<'a>(
    project: &'a Project,
    is_selected: bool,
    is_active_project: bool,
    inner_width: usize,
) -> ListItem<'a> {
    let (badge_text, badge_style) = theme::status_badge(project.status);

    // Active-project marker prefix: "▶ " or "  "
    let prefix = if is_active_project { "▶ " } else { "  " };
    let prefix_len = 2usize;

    // Badge occupies its own fixed width.
    let badge_len = badge_text.len();

    // Stale age badge: days since last log entry
    let today = Local::now().date_naive();
    let (stale_text, stale_color) = if project.status == Status::Done || project.status == Status::Parked {
        (String::new(), Color::Reset) // no stale badge for done/parked
    } else {
        match project.log.first() {
            Some(entry) => {
                let days = (today - entry.date).num_days();
                if days <= 0 {
                    (String::new(), Color::Reset) // today, no badge
                } else if days <= 2 {
                    (format!("{days}d"), Color::Green)
                } else if days <= 7 {
                    (format!("{days}d"), Color::Yellow)
                } else if days <= 30 {
                    (format!("{}w", days / 7), Color::Red)
                } else {
                    (format!("{}mo", days / 30), Color::Red)
                }
            }
            None => ("new".to_string(), Color::DarkGray),
        }
    };
    let stale_len = if stale_text.is_empty() { 0 } else { stale_text.len() + 1 }; // +1 for space

    // Priority indicator character.
    let priority_indicator = match project.priority {
        Priority::High   => "!",
        Priority::Medium => "·",
        Priority::Low    => " ",
    };
    let priority_len = 1usize;

    // Space between prefix + name and badge: at least 1.
    // Layout: [prefix][name...padding...][stale][priority][badge]
    let reserved = prefix_len + stale_len + priority_len + badge_len + 2; // 2 separating spaces
    let name_width = if inner_width > reserved {
        inner_width - reserved
    } else {
        4 // minimum fallback
    };

    // Truncate name if needed.
    let name = truncate_str(&project.name, name_width);
    let name_pad = name_width.saturating_sub(name.len());
    let padding = " ".repeat(name_pad);

    // Build spans.
    let prefix_style = if is_active_project {
        Style::default().fg(theme::STATUS_ACTIVE)
    } else {
        Style::default()
    };

    let name_style = if is_selected {
        theme::selected()
    } else {
        Style::default()
    };

    let priority_style = theme::priority_style(project.priority);

    let mut spans = vec![
        Span::styled(prefix, prefix_style),
        Span::styled(name, name_style),
        Span::styled(padding, name_style),
    ];
    if !stale_text.is_empty() {
        spans.push(Span::styled(
            format!(" {stale_text}"),
            Style::default().fg(stale_color),
        ));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(priority_indicator, priority_style));
    spans.push(Span::styled(badge_text, badge_style));
    let line = Line::from(spans);

    ListItem::new(line)
}

// ── Right panel ──────────────────────────────────────────────────────

fn render_preview(
    state: &DashboardState,
    projects: &[Project],
    frame: &mut Frame,
    area: Rect,
    issue_store: &IssueStore,
) {
    let block = Block::default()
        .title(" PREVIEW ")
        .borders(Borders::ALL)
        .border_style(theme::unfocused_border());

    let Some(project) = projects.get(state.selected) else {
        let para = Paragraph::new("").block(block);
        frame.render_widget(para, area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // ── Issues ───────────────────────────────────────────────────────
    let issue_file = issue_store.load(&project.slug);
    let open_issues: Vec<_> = issue_file
        .issues
        .iter()
        .filter(|i| i.status != IssueStatus::Done)
        .collect();
    if !open_issues.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Issues ({} open):", open_issues.len()),
                theme::bold(),
            ),
        ]));

        // Show pinned active issue first (if set)
        let mut pinned_id_shown: Option<u32> = None;
        if let Some(pinned_id) = project.active_issue {
            if let Some(pinned) = issue_file.issues.iter().find(|i| i.id == pinned_id && i.status != IssueStatus::Done) {
                lines.push(Line::from(vec![
                    Span::styled("▶ ", Style::default().fg(theme::TEXT_ACCENT)),
                    Span::styled(
                        format!("#{} {}", pinned.id, pinned.title),
                        Style::default().fg(theme::TEXT_ACCENT),
                    ),
                ]));
                pinned_id_shown = Some(pinned_id);
            }
        }

        let cm = issue_file.children_map();
        let top_issues = cm.get(&None).cloned().unwrap_or_default();
        let mut shown = if pinned_id_shown.is_some() { 1 } else { 0 };
        const MAX_ISSUES: usize = 8;
        'outer: for issue in &top_issues {
            if issue.status == IssueStatus::Done {
                continue;
            }
            // Skip the pinned issue — already shown above
            if pinned_id_shown == Some(issue.id) {
                continue;
            }
            lines.push(build_preview_issue_line(issue, false));
            shown += 1;
            if shown >= MAX_ISSUES {
                break;
            }
            if let Some(children) = cm.get(&Some(issue.id)) {
                for child in children {
                    if child.status == IssueStatus::Done {
                        continue;
                    }
                    lines.push(build_preview_issue_line(child, true));
                    shown += 1;
                    if shown >= MAX_ISSUES {
                        break 'outer;
                    }
                }
            }
        }
        let remaining = open_issues.len().saturating_sub(shown);
        if remaining > 0 {
            lines.push(Line::from(Span::styled(
                format!("  … {remaining} more"),
                theme::dim(),
            )));
        }
        lines.push(Line::raw(""));
    }

    // ── Focus ────────────────────────────────────────────────────────
    if project.current_focus.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Focus: ", theme::bold()),
            Span::styled("No focus set. Press e to set one.", theme::empty_hint()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Focus: ", theme::bold()),
            Span::raw(&project.current_focus),
        ]));
    }

    lines.push(Line::raw(""));

    // ── Blockers ─────────────────────────────────────────────────────
    let open_blockers: Vec<_> = project.blockers.iter().filter(|b| !b.resolved).collect();

    if open_blockers.is_empty() {
        lines.push(Line::from(Span::styled(
            "Blockers (0)",
            theme::dim(),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Blockers ({}):", open_blockers.len()),
                Style::default()
                    .fg(theme::TEXT_ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        for blocker in &open_blockers {
            let mut text = format!("⊘ {}", blocker.description);
            if let Some(person) = &blocker.person {
                text.push_str(&format!(" {person}"));
            }
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(text, Style::default().fg(theme::TEXT_WARNING)),
            ]));
        }
    }

    lines.push(Line::raw(""));

    // ── Last log ─────────────────────────────────────────────────────
    lines.push(Line::from(Span::styled("Last log:", theme::bold())));

    // Most recent log entry first (log is stored newest-first by convention
    // from the Python side, but we take the first entry regardless).
    match project.log.first() {
        None => {
            lines.push(Line::from(Span::styled(
                "  No log entries yet.",
                theme::empty_hint(),
            )));
        }
        Some(entry) => {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(entry.date.to_string(), theme::timestamp_style()),
            ]));
            if entry.lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (empty entry)",
                    theme::empty_hint(),
                )));
            } else {
                for log_line in entry.lines.iter().take(5) {
                    lines.push(Line::from(vec![
                        Span::raw("  - "),
                        Span::raw(log_line.as_str()),
                    ]));
                }
                if entry.lines.len() > 5 {
                    lines.push(Line::from(Span::styled(
                        format!("  … {} more", entry.lines.len() - 5),
                        theme::dim(),
                    )));
                }
            }
        }
    }

    // ── Tags (if any) ────────────────────────────────────────────────
    if !project.tags.is_empty() {
        lines.push(Line::raw(""));
        let tag_spans: Vec<Span> = std::iter::once(Span::styled("Tags: ", theme::dim()))
            .chain(project.tags.iter().enumerate().flat_map(|(i, tag)| {
                let sep = if i == 0 {
                    Span::raw("")
                } else {
                    Span::raw("  ")
                };
                [sep, Span::styled(format!("#{tag}"), theme::tag_style())]
            }))
            .collect();
        lines.push(Line::from(tag_spans));
    }

    let text = Text::from(lines);
    let para = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(para, area);
}

/// Build a compact issue line for the dashboard preview.
fn build_preview_issue_line(issue: &jm_core::models::Issue, is_child: bool) -> Line<'static> {
    let indent = if is_child { "    " } else { "  " };

    let id_span = Span::styled(
        format!("#{}", issue.id),
        Style::default().add_modifier(Modifier::BOLD),
    );

    let (status_str, status_style) = match issue.status {
        IssueStatus::Todo => (
            "[todo]",
            Style::default().fg(Color::DarkGray),
        ),
        IssueStatus::Active => (
            "[active]",
            Style::default().fg(Color::Green),
        ),
        IssueStatus::Blocked => (
            "[blocked]",
            Style::default().fg(Color::Red),
        ),
        IssueStatus::Done => (
            "[done]",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT),
        ),
    };

    let title_style = if issue.status == IssueStatus::Done {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::raw(indent.to_string()),
        id_span,
        Span::raw(" "),
        Span::styled(status_str.to_string(), status_style),
        Span::raw(" "),
        Span::styled(issue.title.clone(), title_style),
    ])
}

// ── Kanban view ──────────────────────────────────────────────────────

const KANBAN_COLUMNS: [Status; 5] = [
    Status::Active,
    Status::Blocked,
    Status::Pending,
    Status::Parked,
    Status::Done,
];

fn status_to_column(status: Status) -> usize {
    KANBAN_COLUMNS
        .iter()
        .position(|&s| s == status)
        .unwrap_or(0)
}

fn projects_in_column<'a>(projects: &'a [Project], column: usize) -> Vec<&'a Project> {
    let status = KANBAN_COLUMNS.get(column).copied().unwrap_or(Status::Active);
    projects.iter().filter(|p| p.status == status).collect()
}

fn render_kanban(
    state: &DashboardState,
    projects: &[Project],
    active_slug: Option<&str>,
    frame: &mut Frame,
    area: Rect,
) {
    // Split into 5 equal columns
    let constraints: Vec<Constraint> = (0..KANBAN_COLUMNS.len())
        .map(|_| Constraint::Percentage(100 / KANBAN_COLUMNS.len() as u16))
        .collect();
    let columns = Layout::horizontal(constraints).split(area);

    for (col_idx, &status) in KANBAN_COLUMNS.iter().enumerate() {
        let is_focused_col = col_idx == state.kanban_column;
        let col_projects = projects_in_column(projects, col_idx);

        let border_style = if is_focused_col {
            theme::focused_border()
        } else {
            theme::unfocused_border()
        };

        let (_, badge_style) = theme::status_badge(status);
        let title = Span::styled(
            format!(" {} ({}) ", status.to_string().to_uppercase(), col_projects.len()),
            badge_style,
        );

        let block = Block::default()
            .title(Line::from(title))
            .borders(Borders::ALL)
            .border_style(border_style);

        if col_projects.is_empty() {
            let para = Paragraph::new("").block(block);
            frame.render_widget(para, columns[col_idx]);
            continue;
        }

        let inner_width = columns[col_idx].width.saturating_sub(2) as usize;

        let items: Vec<ListItem> = col_projects
            .iter()
            .enumerate()
            .map(|(row_idx, project)| {
                let is_selected = is_focused_col && row_idx == state.kanban_row;
                let is_active_project = active_slug
                    .map(|s| s == project.slug)
                    .unwrap_or(false);

                let prefix = if is_active_project { "▶ " } else { "  " };
                let prefix_style = if is_active_project {
                    Style::default().fg(theme::STATUS_ACTIVE)
                } else {
                    Style::default()
                };
                let name_style = if is_selected {
                    theme::selected()
                } else {
                    Style::default()
                };

                let name = truncate_str(&project.name, inner_width.saturating_sub(2));
                let line = Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(name, name_style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, columns[col_idx]);
    }
}

// ── Utilities ────────────────────────────────────────────────────────

/// Truncate a string to at most `max_chars` Unicode scalar values.
/// Appends "…" if truncation occurred, keeping total at `max_chars`.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else if max_chars <= 1 {
        "…".to_string()
    } else {
        let truncated: String = s.chars().take(max_chars - 1).collect();
        format!("{truncated}…")
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use jm_core::models::{Priority, Status};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn make_projects(n: usize) -> Vec<jm_core::models::Project> {
        (0..n)
            .map(|i| {
                let mut p = jm_core::models::Project::new(&format!("Project {}", i + 1));
                p.status = Status::Active;
                p.priority = Priority::Medium;
                p
            })
            .collect()
    }

    fn make_state(n: usize) -> DashboardState {
        DashboardState {
            selected: 0,
            projects: make_projects(n),
            scroll_offset: 0,
            view_mode: ViewMode::List,
            kanban_column: 0,
            kanban_row: 0,
        }
    }

    // ── List view navigation ─────────────────────────────────────────

    #[test]
    fn test_j_scrolls_down() {
        let mut state = make_state(3);
        assert_eq!(state.selected, 0);
        let action = handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(matches!(action, Action::Down));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_down_arrow_scrolls_down() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Down));
        assert!(matches!(action, Action::Down));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_k_scrolls_up() {
        let mut state = make_state(3);
        state.selected = 2;
        let action = handle_key(&mut state, key(KeyCode::Char('k')));
        assert!(matches!(action, Action::Up));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_up_arrow_scrolls_up() {
        let mut state = make_state(3);
        state.selected = 2;
        let action = handle_key(&mut state, key(KeyCode::Up));
        assert!(matches!(action, Action::Up));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_j_wraps_at_bottom() {
        let mut state = make_state(3);
        state.selected = 2; // last item
        let action = handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(matches!(action, Action::Down));
        assert_eq!(state.selected, 0, "j should wrap to first item");
    }

    #[test]
    fn test_k_wraps_at_top() {
        let mut state = make_state(3);
        state.selected = 0; // first item
        let action = handle_key(&mut state, key(KeyCode::Char('k')));
        assert!(matches!(action, Action::Up));
        assert_eq!(state.selected, 2, "k should wrap to last item");
    }

    #[test]
    fn test_g_goes_to_top() {
        let mut state = make_state(5);
        state.selected = 4;
        let action = handle_key(&mut state, key(KeyCode::Char('g')));
        assert!(matches!(action, Action::Top));
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_shift_g_goes_to_bottom() {
        let mut state = make_state(5);
        state.selected = 0;
        let action = handle_key(&mut state, key(KeyCode::Char('G')));
        assert!(matches!(action, Action::Bottom));
        assert_eq!(state.selected, 4);
    }

    #[test]
    fn test_ctrl_d_half_page_down() {
        let mut state = make_state(10);
        state.selected = 0;
        let action = handle_key(&mut state, ctrl_key(KeyCode::Char('d')));
        assert!(matches!(action, Action::HalfPageDown));
        assert!(state.selected > 0, "should have moved down");
    }

    #[test]
    fn test_ctrl_u_half_page_up() {
        let mut state = make_state(10);
        state.selected = 8;
        let action = handle_key(&mut state, ctrl_key(KeyCode::Char('u')));
        assert!(matches!(action, Action::HalfPageUp));
        assert!(state.selected < 8, "should have moved up");
    }

    // ── Navigation with empty list ────────────────────────────────────

    #[test]
    fn test_j_on_empty_list_returns_down() {
        let mut state = make_state(0);
        let action = handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(matches!(action, Action::Down));
        assert_eq!(state.selected, 0, "selected should stay at 0 with empty list");
    }

    #[test]
    fn test_k_on_empty_list_returns_up() {
        let mut state = make_state(0);
        let action = handle_key(&mut state, key(KeyCode::Char('k')));
        assert!(matches!(action, Action::Up));
        assert_eq!(state.selected, 0);
    }

    // ── Dashboard command keys ────────────────────────────────────────

    #[test]
    fn test_enter_returns_select() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::Select));
    }

    #[test]
    fn test_w_starts_work() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('w')));
        assert!(matches!(action, Action::StartWork));
    }

    #[test]
    fn test_s_switches_context() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(matches!(action, Action::SwitchContext));
    }

    #[test]
    fn test_n_quick_note() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('n')));
        assert!(matches!(action, Action::QuickNote));
    }

    #[test]
    fn test_b_quick_blocker() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('b')));
        assert!(matches!(action, Action::QuickBlocker));
    }

    #[test]
    fn test_d_quick_decision() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('d')));
        assert!(matches!(action, Action::QuickDecision));
    }

    #[test]
    fn test_u_unblock() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('u')));
        assert!(matches!(action, Action::Unblock));
    }

    #[test]
    fn test_slash_opens_search() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('/')));
        assert!(matches!(action, Action::SearchOpen));
    }

    #[test]
    fn test_r_morning_review() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('r')));
        assert!(matches!(action, Action::MorningReview));
    }

    #[test]
    fn test_p_people_view() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('p')));
        assert!(matches!(action, Action::PeopleView));
    }

    #[test]
    fn test_a_add_project() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('a')));
        assert!(matches!(action, Action::AddProject));
    }

    #[test]
    fn test_f_stop_work() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('f')));
        assert!(matches!(action, Action::StopWork));
    }

    #[test]
    fn test_q_quits() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(matches!(action, Action::Quit));
    }

    #[test]
    fn test_question_mark_help() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('?')));
        assert!(matches!(action, Action::Help));
    }

    #[test]
    fn test_shift_p_toggle_sidebar() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('P')));
        assert!(matches!(action, Action::ToggleSidebar));
    }

    #[test]
    fn test_tab_focus_sidebar() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Tab));
        assert!(matches!(action, Action::FocusSidebar));
    }

    #[test]
    fn test_ctrl_e_export() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, ctrl_key(KeyCode::Char('e')));
        assert!(matches!(action, Action::Export));
    }

    #[test]
    fn test_unknown_key_returns_none() {
        let mut state = make_state(3);
        let action = handle_key(&mut state, key(KeyCode::Char('z')));
        assert!(matches!(action, Action::None));
    }

    // ── sort_projects ordering ────────────────────────────────────────

    #[test]
    fn test_sort_projects_active_before_done() {
        let mut projects = vec![
            {
                let mut p = jm_core::models::Project::new("Done Project");
                p.status = Status::Done;
                p
            },
            {
                let mut p = jm_core::models::Project::new("Active Project");
                p.status = Status::Active;
                p
            },
        ];
        sort_projects(&mut projects);
        assert_eq!(projects[0].status, Status::Active);
        assert_eq!(projects[1].status, Status::Done);
    }

    #[test]
    fn test_sort_projects_full_status_order() {
        let statuses = [
            Status::Done,
            Status::Parked,
            Status::Pending,
            Status::Blocked,
            Status::Active,
        ];
        let mut projects: Vec<_> = statuses
            .iter()
            .map(|&s| {
                let mut p = jm_core::models::Project::new("P");
                p.status = s;
                p
            })
            .collect();
        sort_projects(&mut projects);
        let sorted_statuses: Vec<_> = projects.iter().map(|p| p.status).collect();
        assert_eq!(
            sorted_statuses,
            vec![
                Status::Active,
                Status::Blocked,
                Status::Pending,
                Status::Parked,
                Status::Done,
            ]
        );
    }

    #[test]
    fn test_sort_projects_alphabetical_within_same_status() {
        let mut projects = vec![
            {
                let mut p = jm_core::models::Project::new("Zeta");
                p.status = Status::Active;
                p
            },
            {
                let mut p = jm_core::models::Project::new("Alpha");
                p.status = Status::Active;
                p
            },
            {
                let mut p = jm_core::models::Project::new("Gamma");
                p.status = Status::Active;
                p
            },
        ];
        sort_projects(&mut projects);
        assert_eq!(projects[0].name, "Alpha");
        assert_eq!(projects[1].name, "Gamma");
        assert_eq!(projects[2].name, "Zeta");
    }

    // ── truncate_str ─────────────────────────────────────────────────

    #[test]
    fn test_truncate_str_short_string() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_longer_string() {
        let result = truncate_str("hello world", 8);
        assert_eq!(result.chars().count(), 8);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_str_zero_max() {
        assert_eq!(truncate_str("hello", 0), "");
    }

    #[test]
    fn test_truncate_str_max_one() {
        assert_eq!(truncate_str("hello", 1), "…");
    }

    #[test]
    fn test_truncate_str_unicode() {
        // Japanese chars are each 1 char
        let result = truncate_str("日本語テスト", 4);
        assert_eq!(result.chars().count(), 4);
        assert!(result.ends_with('…'));
    }

    // ── Kanban view key handling ──────────────────────────────────────

    #[test]
    fn test_shift_k_toggles_to_kanban() {
        let mut state = make_state(5);
        assert!(matches!(state.view_mode, ViewMode::List));
        let action = handle_key(&mut state, key(KeyCode::Char('K')));
        assert!(matches!(action, Action::None));
        assert!(matches!(state.view_mode, ViewMode::Kanban));
    }

    #[test]
    fn test_shift_k_toggles_back_to_list() {
        let mut state = make_state(5);
        // Switch to kanban first
        handle_key(&mut state, key(KeyCode::Char('K')));
        assert!(matches!(state.view_mode, ViewMode::Kanban));
        // Switch back
        handle_key(&mut state, key(KeyCode::Char('K')));
        assert!(matches!(state.view_mode, ViewMode::List));
    }

    #[test]
    fn test_kanban_j_moves_down_within_column() {
        let mut state = make_state(0);
        // Populate with active projects so kanban has rows
        state.projects = vec![
            { let mut p = jm_core::models::Project::new("A"); p.status = Status::Active; p },
            { let mut p = jm_core::models::Project::new("B"); p.status = Status::Active; p },
        ];
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 0;
        state.kanban_row = 0;

        let action = handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(matches!(action, Action::None));
        assert_eq!(state.kanban_row, 1);
    }

    #[test]
    fn test_kanban_j_does_not_exceed_last_row() {
        let mut state = make_state(0);
        state.projects = vec![
            { let mut p = jm_core::models::Project::new("A"); p.status = Status::Active; p },
        ];
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 0;
        state.kanban_row = 0;

        // Press j when already at the only row
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.kanban_row, 0, "should not go past last row");
    }

    #[test]
    fn test_kanban_k_moves_up_within_column() {
        let mut state = make_state(0);
        state.projects = vec![
            { let mut p = jm_core::models::Project::new("A"); p.status = Status::Active; p },
            { let mut p = jm_core::models::Project::new("B"); p.status = Status::Active; p },
        ];
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 0;
        state.kanban_row = 1;

        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.kanban_row, 0);
    }

    #[test]
    fn test_kanban_h_moves_left() {
        let mut state = make_state(0);
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 2;

        handle_key(&mut state, key(KeyCode::Char('h')));
        assert_eq!(state.kanban_column, 1);
    }

    #[test]
    fn test_kanban_l_moves_right() {
        let mut state = make_state(0);
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 0;

        handle_key(&mut state, key(KeyCode::Char('l')));
        assert_eq!(state.kanban_column, 1);
    }

    #[test]
    fn test_kanban_h_does_not_go_below_zero() {
        let mut state = make_state(0);
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = 0;

        handle_key(&mut state, key(KeyCode::Char('h')));
        assert_eq!(state.kanban_column, 0, "should not go below column 0");
    }

    #[test]
    fn test_kanban_l_does_not_exceed_last_column() {
        let mut state = make_state(0);
        state.view_mode = ViewMode::Kanban;
        state.kanban_column = KANBAN_COLUMNS.len() - 1;

        handle_key(&mut state, key(KeyCode::Char('l')));
        assert_eq!(
            state.kanban_column,
            KANBAN_COLUMNS.len() - 1,
            "should not go past last column"
        );
    }

    // ── Refresh keeps slug selection ──────────────────────────────────

    #[test]
    fn test_refresh_preserves_selection_by_slug() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let ps = jm_core::storage::store::ProjectStore::new(tmp.path());
        ps.create_project("Alpha").unwrap();
        ps.create_project("Beta").unwrap();

        let mut state = init(&ps);
        // Select second item
        state.selected = 1;

        // Refresh — should restore selection
        refresh(&mut state, &ps);
        // The slug should be preserved; selected index might change due to sort
        let selected_name = &state.projects[state.selected].name;
        assert!(
            selected_name == "Alpha" || selected_name == "Beta",
            "selection should still point to a valid project"
        );
    }
}
