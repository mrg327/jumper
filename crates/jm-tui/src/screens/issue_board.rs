//! Issue Board — cross-project kanban board for all open issues.
//!
//! Layout: kanban columns (left) | detail panel (right).
//! Columns: Todo | Active | Blocked (Done hidden by default, toggle with D).
//! Issues shown with [project-slug] prefix. h/l moves between columns,
//! j/k navigates within a column. Enter advances status, S reverses.
//! I toggles back to dashboard (same key that opens the board).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use jm_core::models::{Issue, IssueStatus, Project};
use jm_core::storage::{IssueStore, ProjectStore};

use crate::events::Action;
use crate::theme;

// ── Columns ─────────────────────────────────────────────────────────

const COLUMNS_DEFAULT: [IssueStatus; 3] = [IssueStatus::Todo, IssueStatus::Active, IssueStatus::Blocked];
const COLUMNS_WITH_DONE: [IssueStatus; 4] = [IssueStatus::Todo, IssueStatus::Active, IssueStatus::Blocked, IssueStatus::Done];

// ── State ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IssueBoardState {
    /// All issues flattened from every project, each paired with its project slug.
    pub items: Vec<BoardItem>,
    /// Current column index.
    pub column: usize,
    /// Current row index within the focused column.
    pub row: usize,
    /// Whether to show the Done column.
    pub show_done: bool,
    /// Optional project filter (slug). None = show all.
    pub project_filter: Option<String>,
    /// All project slugs that have issues (for cycling the filter).
    pub project_slugs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BoardItem {
    pub slug: String,
    pub issue: Issue,
}

// ── Init / Refresh ──────────────────────────────────────────────────

pub fn init(issue_store: &IssueStore) -> IssueBoardState {
    let mut state = IssueBoardState {
        items: Vec::new(),
        column: 0,
        row: 0,
        show_done: false,
        project_filter: None,
        project_slugs: Vec::new(),
    };
    load_items(&mut state, issue_store);
    state
}

pub fn refresh(state: &mut IssueBoardState, issue_store: &IssueStore) {
    let old_id = selected_item(state).map(|it| (it.slug.clone(), it.issue.id));
    load_items(state, issue_store);
    // Try to restore cursor to the same issue after reload.
    if let Some((slug, id)) = old_id {
        let cols = columns(state.show_done);
        if let Some(&status) = cols.get(state.column) {
            let col_items = items_in_column(&state.items, status, &state.project_filter);
            if let Some(pos) = col_items.iter().position(|it| it.slug == slug && it.issue.id == id) {
                state.row = pos;
                return;
            }
        }
    }
    clamp_row(state);
}

fn load_items(state: &mut IssueBoardState, issue_store: &IssueStore) {
    let files = issue_store.load_all();
    let mut items = Vec::new();
    let mut slugs = Vec::new();
    for file in files {
        if !file.issues.is_empty() {
            slugs.push(file.project_slug.clone());
        }
        for issue in file.issues {
            items.push(BoardItem {
                slug: file.project_slug.clone(),
                issue,
            });
        }
    }
    slugs.sort();
    slugs.dedup();
    state.items = items;
    state.project_slugs = slugs;
}

// ── Helpers ─────────────────────────────────────────────────────────

fn columns(show_done: bool) -> &'static [IssueStatus] {
    if show_done {
        &COLUMNS_WITH_DONE
    } else {
        &COLUMNS_DEFAULT
    }
}

fn items_in_column<'a>(
    items: &'a [BoardItem],
    status: IssueStatus,
    filter: &Option<String>,
) -> Vec<&'a BoardItem> {
    items
        .iter()
        .filter(|it| it.issue.status == status)
        .filter(|it| match filter {
            Some(slug) => it.slug == *slug,
            None => true,
        })
        .collect()
}

fn selected_item(state: &IssueBoardState) -> Option<&BoardItem> {
    let cols = columns(state.show_done);
    let status = cols.get(state.column)?;
    let col_items = items_in_column(&state.items, *status, &state.project_filter);
    col_items.get(state.row).copied()
}

fn clamp_row(state: &mut IssueBoardState) {
    let cols = columns(state.show_done);
    if state.column >= cols.len() {
        state.column = cols.len().saturating_sub(1);
    }
    let status = cols[state.column];
    let col_len = items_in_column(&state.items, status, &state.project_filter).len();
    if col_len == 0 {
        state.row = 0;
    } else if state.row >= col_len {
        state.row = col_len - 1;
    }
}

// ── Key handling ────────────────────────────────────────────────────

pub fn handle_key(state: &mut IssueBoardState, key: KeyEvent) -> Action {
    let cols = columns(state.show_done);
    match key.code {
        // Toggle back to dashboard (same key that opens the board)
        KeyCode::Char('I') | KeyCode::Esc | KeyCode::Char('q') => Action::Back,

        // Column navigation
        KeyCode::Char('h') | KeyCode::Left => {
            if state.column > 0 {
                state.column -= 1;
                clamp_row(state);
            }
            Action::None
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if state.column < cols.len() - 1 {
                state.column += 1;
                clamp_row(state);
            }
            Action::None
        }

        // Row navigation
        KeyCode::Char('j') | KeyCode::Down => {
            let status = cols[state.column];
            let col_len = items_in_column(&state.items, status, &state.project_filter).len();
            if col_len > 0 && state.row < col_len - 1 {
                state.row += 1;
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if state.row > 0 {
                state.row -= 1;
            }
            Action::None
        }
        KeyCode::Char('g') => {
            state.row = 0;
            Action::None
        }
        KeyCode::Char('G') => {
            let status = cols[state.column];
            let col_len = items_in_column(&state.items, status, &state.project_filter).len();
            state.row = col_len.saturating_sub(1);
            Action::None
        }

        // Half page
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let status = cols[state.column];
            let col_len = items_in_column(&state.items, status, &state.project_filter).len();
            state.row = (state.row + 10).min(col_len.saturating_sub(1));
            Action::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.row = state.row.saturating_sub(10);
            Action::None
        }

        // Advance status (move issue right)
        KeyCode::Enter | KeyCode::Char('s') => {
            if let Some(item) = selected_item(state) {
                let new_status = item.issue.status.cycle();
                return Action::Toast(format!(
                    "issue_board_set_status:{}:{}:{}",
                    item.slug, item.issue.id, new_status
                ));
            }
            Action::None
        }

        // Reverse status (move issue left)
        KeyCode::Char('S') => {
            if let Some(item) = selected_item(state) {
                let new_status = item.issue.status.cycle_reverse();
                return Action::Toast(format!(
                    "issue_board_set_status:{}:{}:{}",
                    item.slug, item.issue.id, new_status
                ));
            }
            Action::None
        }

        // Close issue
        KeyCode::Char('c') => {
            if let Some(item) = selected_item(state) {
                if item.issue.status != IssueStatus::Done {
                    return Action::Toast(format!(
                        "issue_board_set_status:{}:{}:done",
                        item.slug, item.issue.id
                    ));
                }
            }
            Action::None
        }

        // Toggle Done column
        KeyCode::Char('D') => {
            state.show_done = !state.show_done;
            clamp_row(state);
            Action::None
        }

        // Cycle project filter
        KeyCode::Char('p') => {
            if state.project_slugs.is_empty() {
                return Action::None;
            }
            state.project_filter = match &state.project_filter {
                None => Some(state.project_slugs[0].clone()),
                Some(current) => {
                    let idx = state
                        .project_slugs
                        .iter()
                        .position(|s| s == current)
                        .unwrap_or(0);
                    if idx + 1 < state.project_slugs.len() {
                        Some(state.project_slugs[idx + 1].clone())
                    } else {
                        None // cycle back to "all"
                    }
                }
            };
            state.row = 0;
            clamp_row(state);
            Action::None
        }

        // Open project view for selected issue
        KeyCode::Char('o') => {
            if let Some(item) = selected_item(state) {
                return Action::PushScreen(crate::events::ScreenId::ProjectView(
                    item.slug.clone(),
                ));
            }
            Action::None
        }

        KeyCode::Char('?') => Action::Help,

        _ => Action::None,
    }
}

// ── Render ──────────────────────────────────────────────────────────

pub fn render(
    state: &IssueBoardState,
    project_store: &ProjectStore,
    frame: &mut Frame,
    area: Rect,
) {
    let cols = columns(state.show_done);
    let num_cols = cols.len() as u16;

    // Filter label at top
    let filter_label = match &state.project_filter {
        Some(slug) => format!(" ISSUE BOARD [{}] ", slug),
        None => " ISSUE BOARD (all projects) ".to_string(),
    };

    let [header_area, body_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(filter_label, theme::accent().add_modifier(Modifier::BOLD)),
    ]));
    frame.render_widget(header, header_area);

    // Split body: kanban columns (left) | detail panel (right)
    let [board_area, detail_area] = Layout::horizontal([
        Constraint::Percentage(65),
        Constraint::Percentage(35),
    ])
    .areas(body_area);

    // ── Kanban columns ──────────────────────────────────────────────
    let constraints: Vec<Constraint> = (0..num_cols)
        .map(|_| Constraint::Percentage(100 / num_cols))
        .collect();
    let col_areas = Layout::horizontal(constraints).split(board_area);

    for (col_idx, &status) in cols.iter().enumerate() {
        let is_focused = col_idx == state.column;
        let col_items = items_in_column(&state.items, status, &state.project_filter);

        let border_style = if is_focused {
            theme::focused_border()
        } else {
            theme::unfocused_border()
        };

        let title_style = issue_status_title_style(status);
        let title = Span::styled(
            format!(" {} ({}) ", status_label(status), col_items.len()),
            title_style,
        );

        let block = Block::default()
            .title(Line::from(title))
            .borders(Borders::ALL)
            .border_style(border_style);

        if col_items.is_empty() {
            let para = Paragraph::new("")
                .style(theme::empty_hint())
                .block(block);
            frame.render_widget(para, col_areas[col_idx]);
            continue;
        }

        let inner_width = col_areas[col_idx].width.saturating_sub(2) as usize;

        let items: Vec<ListItem> = col_items
            .iter()
            .enumerate()
            .map(|(row_idx, item)| {
                let is_selected = is_focused && row_idx == state.row;
                build_issue_item(item, is_selected, inner_width)
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, col_areas[col_idx]);
    }

    // ── Detail panel ────────────────────────────────────────────────
    render_detail(state, project_store, frame, detail_area);
}

fn render_detail(
    state: &IssueBoardState,
    project_store: &ProjectStore,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default()
        .title(" DETAIL ")
        .borders(Borders::ALL)
        .border_style(theme::unfocused_border());

    let Some(item) = selected_item(state) else {
        let para = Paragraph::new("No issue selected.")
            .style(theme::empty_hint())
            .block(block);
        frame.render_widget(para, area);
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Issue header
    lines.push(Line::from(vec![
        Span::styled(
            format!("#{}", item.issue.id),
            theme::accent().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(item.issue.title.clone(), theme::bold()),
    ]));

    // Status badge
    let status_style = issue_status_title_style(item.issue.status);
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme::dim()),
        Span::styled(format!(" {} ", status_label(item.issue.status)), status_style),
    ]));

    // Created date
    lines.push(Line::from(vec![
        Span::styled("Created: ", theme::dim()),
        Span::styled(
            item.issue.created.to_string(),
            theme::timestamp_style(),
        ),
    ]));

    // Closed date (if done)
    if let Some(closed) = item.issue.closed {
        lines.push(Line::from(vec![
            Span::styled("Closed: ", theme::dim()),
            Span::styled(closed.to_string(), theme::timestamp_style()),
        ]));
    }

    // External ref
    if !item.issue.r#ref.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Ref: ", theme::dim()),
            Span::styled(item.issue.r#ref.clone(), theme::tag_style()),
        ]));
    }

    // Notes
    if !item.issue.notes.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Notes: ", theme::dim()),
            Span::raw(item.issue.notes.clone()),
        ]));
    }

    // Parent issue (if sub-issue)
    if let Some(parent_id) = item.issue.parent_id {
        let parent_title = state
            .items
            .iter()
            .find(|it| it.slug == item.slug && it.issue.id == parent_id)
            .map(|it| it.issue.title.as_str())
            .unwrap_or("?");
        lines.push(Line::from(vec![
            Span::styled("Parent: ", theme::dim()),
            Span::styled(format!("#{parent_id}"), theme::accent()),
            Span::raw(format!(" {parent_title}")),
        ]));
    }

    // Sub-issues
    let children: Vec<&BoardItem> = state
        .items
        .iter()
        .filter(|it| it.slug == item.slug && it.issue.parent_id == Some(item.issue.id))
        .collect();
    if !children.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("Sub-issues ({}):", children.len()),
            theme::bold(),
        )));
        for child in &children {
            let child_status_style = issue_status_title_style(child.issue.status);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(" {} ", status_label(child.issue.status)),
                    child_status_style,
                ),
                Span::raw(format!(" #{} {}", child.issue.id, child.issue.title)),
            ]));
        }
    }

    // ── Project context ─────────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("─── Project: ", theme::dim()),
        Span::styled(item.slug.clone(), theme::accent().add_modifier(Modifier::BOLD)),
        Span::styled(" ───", theme::dim()),
    ]));

    if let Some(project) = project_store.get_project(&item.slug) {
        render_project_context(&project, &mut lines);
    }

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_project_context(project: &Project, lines: &mut Vec<Line<'static>>) {
    // Project name + status
    let (badge_text, badge_style) = theme::status_badge(project.status);
    lines.push(Line::from(vec![
        Span::styled(project.name.clone(), theme::bold()),
        Span::raw(" "),
        Span::styled(badge_text, badge_style),
    ]));

    // Current focus
    if !project.current_focus.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Focus: ", theme::dim()),
            Span::raw(project.current_focus.clone()),
        ]));
    }

    // Blockers
    let open_blockers: Vec<_> = project.blockers.iter().filter(|b| !b.resolved).collect();
    if !open_blockers.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("Blockers ({}):", open_blockers.len()),
            Style::default()
                .fg(theme::TEXT_ERROR)
                .add_modifier(Modifier::BOLD),
        )));
        for b in &open_blockers {
            let mut text = format!("  ⊘ {}", b.description);
            if let Some(person) = &b.person {
                text.push_str(&format!(" {person}"));
            }
            lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(theme::TEXT_WARNING),
            )));
        }
    }

    // Recent log (last 3 entries)
    if !project.log.is_empty() {
        lines.push(Line::from(Span::styled("Recent log:", theme::dim())));
        for entry in project.log.iter().take(3) {
            let date_str = entry.date.to_string();
            for (i, line) in entry.lines.iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {date_str} "),
                            theme::timestamp_style(),
                        ),
                        Span::raw(line.clone()),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("             "),
                        Span::raw(line.clone()),
                    ]));
                }
            }
        }
    }
}

fn build_issue_item<'a>(item: &'a BoardItem, is_selected: bool, inner_width: usize) -> ListItem<'a> {
    let slug_display = format!("[{}]", item.slug);
    let slug_len = slug_display.len() + 1; // +1 for space

    let title_max = inner_width.saturating_sub(slug_len);
    let title = truncate_str(&item.issue.title, title_max);

    let name_style = if is_selected {
        theme::selected()
    } else {
        Style::default()
    };

    let slug_style = if is_selected {
        Style::default()
            .fg(theme::TEXT_DIM)
            .bg(theme::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM)
    };

    // Sub-issue indicator
    let prefix = if item.issue.parent_id.is_some() {
        "  └ "
    } else {
        ""
    };

    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix, Style::default().fg(theme::TEXT_DIM)));
    }
    spans.push(Span::styled(slug_display, slug_style));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(title, name_style));

    // Show ref if present
    if !item.issue.r#ref.is_empty() {
        spans.push(Span::styled(
            format!(" ({})", item.issue.r#ref),
            Style::default().fg(theme::TAG_COLOR),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn status_label(status: IssueStatus) -> &'static str {
    match status {
        IssueStatus::Todo => "TODO",
        IssueStatus::Active => "ACTIVE",
        IssueStatus::Blocked => "BLOCKED",
        IssueStatus::Done => "DONE",
    }
}

fn issue_status_title_style(status: IssueStatus) -> Style {
    let color = match status {
        IssueStatus::Todo => theme::STATUS_PENDING, // blue
        IssueStatus::Active => theme::STATUS_ACTIVE,  // green
        IssueStatus::Blocked => theme::STATUS_BLOCKED, // red
        IssueStatus::Done => theme::STATUS_DONE,     // gray
    };
    Style::default()
        .fg(Color::Black)
        .bg(color)
        .add_modifier(Modifier::BOLD)
}

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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_item(slug: &str, id: u32, title: &str, status: IssueStatus) -> BoardItem {
        BoardItem {
            slug: slug.to_string(),
            issue: Issue {
                id,
                title: title.to_string(),
                status,
                parent_id: None,
                created: NaiveDate::from_ymd_opt(2026, 3, 20).unwrap(),
                closed: None,
                notes: String::new(),
                r#ref: String::new(),
            },
        }
    }

    fn make_state() -> IssueBoardState {
        IssueBoardState {
            items: vec![
                make_item("proj-a", 1, "Task A1", IssueStatus::Todo),
                make_item("proj-a", 2, "Task A2", IssueStatus::Active),
                make_item("proj-b", 3, "Task B1", IssueStatus::Todo),
                make_item("proj-b", 4, "Task B2", IssueStatus::Blocked),
                make_item("proj-a", 5, "Task A3", IssueStatus::Done),
            ],
            column: 0,
            row: 0,
            show_done: false,
            project_filter: None,
            project_slugs: vec!["proj-a".to_string(), "proj-b".to_string()],
        }
    }

    #[test]
    fn test_column_navigation() {
        let mut state = make_state();
        assert_eq!(state.column, 0);

        handle_key(&mut state, key(KeyCode::Char('l')));
        assert_eq!(state.column, 1);

        handle_key(&mut state, key(KeyCode::Char('l')));
        assert_eq!(state.column, 2);

        // Should not go past last column (3 columns: 0,1,2)
        handle_key(&mut state, key(KeyCode::Char('l')));
        assert_eq!(state.column, 2);

        handle_key(&mut state, key(KeyCode::Char('h')));
        assert_eq!(state.column, 1);
    }

    #[test]
    fn test_row_navigation() {
        let mut state = make_state();
        // Column 0 = Todo: 2 items (Task A1, Task B1)
        assert_eq!(state.row, 0);

        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.row, 1);

        // Should not go past last row
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.row, 1);

        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.row, 0);
    }

    #[test]
    fn test_top_bottom() {
        let mut state = make_state();

        handle_key(&mut state, key(KeyCode::Char('G')));
        assert_eq!(state.row, 1); // 2 items in Todo column

        handle_key(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.row, 0);
    }

    #[test]
    fn test_toggle_done() {
        let mut state = make_state();
        assert!(!state.show_done);
        assert_eq!(columns(state.show_done).len(), 3);

        handle_key(&mut state, key(KeyCode::Char('D')));
        assert!(state.show_done);
        assert_eq!(columns(state.show_done).len(), 4);
    }

    #[test]
    fn test_project_filter_cycle() {
        let mut state = make_state();
        assert!(state.project_filter.is_none());

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.project_filter, Some("proj-a".to_string()));

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.project_filter, Some("proj-b".to_string()));

        handle_key(&mut state, key(KeyCode::Char('p')));
        assert!(state.project_filter.is_none()); // back to all
    }

    #[test]
    fn test_enter_advances_status() {
        let mut state = make_state();
        let action = handle_key(&mut state, key(KeyCode::Enter));
        // Should emit a toast with the set_status command
        match action {
            Action::Toast(msg) => {
                assert!(msg.starts_with("issue_board_set_status:"));
                assert!(msg.contains("proj-a:1:active")); // Todo -> Active
            }
            _ => panic!("expected Toast action"),
        }
    }

    #[test]
    fn test_back_action() {
        let mut state = make_state();
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, Action::Back));
    }

    #[test]
    fn test_i_toggles_back() {
        let mut state = make_state();
        let action = handle_key(&mut state, key(KeyCode::Char('I')));
        assert!(matches!(action, Action::Back));
    }

    #[test]
    fn test_items_in_column_with_filter() {
        let state = make_state();
        let filter = Some("proj-a".to_string());
        let todo_items = items_in_column(&state.items, IssueStatus::Todo, &filter);
        assert_eq!(todo_items.len(), 1);
        assert_eq!(todo_items[0].issue.id, 1);
    }

    #[test]
    fn test_clamp_row_on_column_switch() {
        let mut state = make_state();
        // Move to row 1 in Todo column (2 items)
        state.row = 1;
        // Switch to Active column (1 item) — row should clamp
        state.column = 1;
        clamp_row(&mut state);
        assert_eq!(state.row, 0);
    }
}
