//! Context-switch capture screen.
//! The killer feature: prompts the user to record where they left off,
//! any blockers, and the next step before switching projects.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use jm_core::models::Project;

use crate::events::Action;
use crate::text_utils::{char_to_byte_idx, next_word_boundary, prev_word_boundary};
use crate::theme;

// ── State ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SwitchStep {
    LeftOff,       // "Where did you leave off?"
    Blocker,       // "Any blockers?" (optional, Enter skips)
    NextStep,      // "What's the next step?"
    SelectProject, // Pick target project (only when target_slug is None)
    Done,
}

#[derive(Debug, Clone)]
pub struct SwitchState {
    pub step: SwitchStep,
    pub target_slug: Option<String>,
    pub left_off: String,
    pub blocker: String,
    pub next_step: String,
    pub input_buffer: String,
    pub cursor_pos: usize,
    /// Selected index within the *filtered* project list (SelectProject step only).
    pub list_selected: usize,
    /// Case-insensitive substring filter for the project selector.
    pub filter_buffer: String,
}

// ── Init ─────────────────────────────────────────────────────────────

pub fn init(target_slug: Option<&str>) -> SwitchState {
    SwitchState {
        step: SwitchStep::LeftOff,
        target_slug: target_slug.map(|s| s.to_string()),
        left_off: String::new(),
        blocker: String::new(),
        next_step: String::new(),
        input_buffer: String::new(),
        cursor_pos: 0,
        list_selected: 0,
        filter_buffer: String::new(),
    }
}

// ── Key handling ─────────────────────────────────────────────────────

pub fn handle_key(
    state: &mut SwitchState,
    key: KeyEvent,
    projects: &[Project],
) -> Action {
    if key.code == KeyCode::Esc {
        if state.step == SwitchStep::SelectProject {
            // If the filter is non-empty, clear it first (don't cancel outright).
            if !state.filter_buffer.is_empty() {
                state.filter_buffer.clear();
                state.list_selected = 0;
                return Action::None;
            }
            // Otherwise, if context was captured, offer to save it.
            let has_context = !state.left_off.is_empty()
                || !state.blocker.is_empty()
                || !state.next_step.is_empty();
            if has_context {
                return Action::SaveContextOnly;
            }
        }
        return Action::Cancel;
    }

    match &state.step {
        SwitchStep::LeftOff | SwitchStep::Blocker | SwitchStep::NextStep => {
            handle_text_key(state, key)
        }
        SwitchStep::SelectProject => {
            let filtered = filtered_projects(projects, &state.filter_buffer);
            handle_list_key(state, key, projects, &filtered)
        }
        SwitchStep::Done => Action::None,
    }
}

/// Handle a key event for text-input steps.
fn handle_text_key(state: &mut SwitchState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Enter => {
            // Commit the current buffer value to the right field and advance.
            let value = state.input_buffer.trim().to_string();
            match state.step {
                SwitchStep::LeftOff => {
                    state.left_off = value;
                    advance_step(state);
                }
                SwitchStep::Blocker => {
                    state.blocker = value; // empty is fine — optional
                    advance_step(state);
                }
                SwitchStep::NextStep => {
                    state.next_step = value;
                    advance_step(state);
                    // If we have reached Done (target_slug was pre-set), emit.
                    if state.step == SwitchStep::Done {
                        return Action::SwitchComplete;
                    }
                }
                _ => {}
            }
            Action::None
        }

        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                // Remove the character just before the cursor.
                let byte_pos = char_to_byte_idx(&state.input_buffer, state.cursor_pos - 1);
                state.input_buffer.remove(byte_pos);
                state.cursor_pos -= 1;
            }
            Action::None
        }

        KeyCode::Delete => {
            if state.cursor_pos < state.input_buffer.chars().count() {
                let byte_pos = char_to_byte_idx(&state.input_buffer, state.cursor_pos);
                state.input_buffer.remove(byte_pos);
            }
            Action::None
        }

        KeyCode::Left => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Jump to start of previous word
                state.cursor_pos = prev_word_boundary(&state.input_buffer, state.cursor_pos);
            } else if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
            }
            Action::None
        }

        KeyCode::Right => {
            let len = state.input_buffer.chars().count();
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                state.cursor_pos = next_word_boundary(&state.input_buffer, state.cursor_pos);
            } else if state.cursor_pos < len {
                state.cursor_pos += 1;
            }
            Action::None
        }

        KeyCode::Home => {
            state.cursor_pos = 0;
            Action::None
        }

        KeyCode::End => {
            state.cursor_pos = state.input_buffer.chars().count();
            Action::None
        }

        KeyCode::Char(c) => {
            let byte_pos = char_to_byte_idx(&state.input_buffer, state.cursor_pos);
            state.input_buffer.insert(byte_pos, c);
            state.cursor_pos += 1;
            Action::None
        }

        _ => Action::None,
    }
}

/// Handle a key event during the SelectProject step.
/// `filtered` is a pre-computed slice of projects matching the current filter.
fn handle_list_key(
    state: &mut SwitchState,
    key: KeyEvent,
    all_projects: &[Project],
    filtered: &[&Project],
) -> Action {
    let flen = filtered.len();

    match key.code {
        // ── Navigation ───────────────────────────────────────────────
        KeyCode::Char('j') | KeyCode::Down => {
            if flen > 0 {
                state.list_selected = (state.list_selected + 1).min(flen - 1);
            }
            Action::None
        }

        KeyCode::Char('k') | KeyCode::Up => {
            if state.list_selected > 0 {
                state.list_selected -= 1;
            }
            Action::None
        }

        KeyCode::Char('g') => {
            state.list_selected = 0;
            Action::None
        }

        KeyCode::Char('G') => {
            if flen > 0 {
                state.list_selected = flen - 1;
            }
            Action::None
        }

        // ── Selection ────────────────────────────────────────────────
        KeyCode::Enter => {
            if let Some(project) = filtered.get(state.list_selected) {
                state.target_slug = Some(project.slug.clone());
                state.step = SwitchStep::Done;
                return Action::SwitchComplete;
            }
            Action::None
        }

        // ── Filter editing ───────────────────────────────────────────
        KeyCode::Backspace => {
            if !state.filter_buffer.is_empty() {
                state.filter_buffer.pop();
                // Clamp selection to new filtered length
                let new_filtered = filtered_projects(all_projects, &state.filter_buffer);
                if !new_filtered.is_empty() && state.list_selected >= new_filtered.len() {
                    state.list_selected = new_filtered.len() - 1;
                }
            }
            Action::None
        }

        KeyCode::Char(c) => {
            // Reject control chars used for navigation (g/G/j/k already handled above).
            // Any other printable char appends to the filter.
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                state.filter_buffer.push(c);
                // Reset selection when filter changes
                state.list_selected = 0;
            }
            Action::None
        }

        _ => Action::None,
    }
}

// ── Step advancement ─────────────────────────────────────────────────

/// Advance to the next step, resetting the input buffer.
fn advance_step(state: &mut SwitchState) {
    state.input_buffer.clear();
    state.cursor_pos = 0;

    state.step = match state.step {
        SwitchStep::LeftOff => SwitchStep::Blocker,
        SwitchStep::Blocker => SwitchStep::NextStep,
        SwitchStep::NextStep => {
            if state.target_slug.is_none() {
                SwitchStep::SelectProject
            } else {
                SwitchStep::Done
            }
        }
        SwitchStep::SelectProject => SwitchStep::Done,
        SwitchStep::Done => SwitchStep::Done,
    };
}

// ── Filtering ────────────────────────────────────────────────────────

/// Return projects whose name contains `filter` (case-insensitive substring).
/// When `filter` is empty, returns all projects.
pub fn filtered_projects<'a>(projects: &'a [Project], filter: &str) -> Vec<&'a Project> {
    if filter.is_empty() {
        return projects.iter().collect();
    }
    let lower = filter.to_lowercase();
    projects
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&lower))
        .collect()
}

// ── Rendering ────────────────────────────────────────────────────────

pub fn render(
    state: &SwitchState,
    projects: &[Project],
    current_project: Option<&str>,
    frame: &mut Frame,
    area: Rect,
) {
    // Dim the background.
    let overlay = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(overlay, area);

    // Centered card: 60 % wide, 50 % tall.
    let card = centered_rect(60, 50, area);
    frame.render_widget(Clear, card);

    match state.step {
        SwitchStep::SelectProject => render_project_selector(state, projects, frame, card),
        _ => render_input_step(state, current_project, frame, card),
    }
}

/// Render the text-input card for LeftOff / Blocker / NextStep.
fn render_input_step(
    state: &SwitchState,
    current_project: Option<&str>,
    frame: &mut Frame,
    area: Rect,
) {
    let (step_num, total_steps, prompt, hint) = step_metadata(&state.step);

    // Outer card border
    let subtitle = current_project
        .map(|p| format!(" switching from {p} "))
        .unwrap_or_default();
    let title = format!(" Context Switch{subtitle}");
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::MODAL_BORDER))
        .title(Span::styled(title, Style::default().fg(theme::TEXT_ACCENT)));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Layout inside card:
    //   [0] step indicator (1 line)
    //   [1] spacer (1 line)
    //   [2] prompt label (1 line)
    //   [3] hint (1 line — optional, shown for Blocker)
    //   [4] spacer (1 line)
    //   [5] input box (3 lines)
    //   [6] spacer (fill)
    //   [7] keybind hint (1 line)
    let chunks = Layout::vertical([
        Constraint::Length(1), // step indicator
        Constraint::Length(1), // spacer
        Constraint::Length(1), // prompt
        Constraint::Length(1), // sub-hint
        Constraint::Length(1), // spacer
        Constraint::Length(3), // input box
        Constraint::Min(0),    // spacer (fill)
        Constraint::Length(1), // keybind hint
    ])
    .split(inner);

    // Step indicator: "Step 1/3"
    let step_text = format!("Step {step_num}/{total_steps}");
    frame.render_widget(
        Paragraph::new(step_text).style(theme::dim()),
        chunks[0],
    );

    // Prompt label
    frame.render_widget(
        Paragraph::new(Span::styled(prompt, theme::bold())),
        chunks[2],
    );

    // Sub-hint (only for Blocker step)
    if !hint.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(hint, theme::dim())),
            chunks[3],
        );
    }

    // Input box
    render_input_box(state, frame, chunks[5]);

    // Keybind hint
    let kb = if state.step == SwitchStep::Blocker {
        "Enter: next (skip if empty)  Escape: cancel"
    } else {
        "Enter: next  Escape: cancel"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(kb, theme::dim())),
        chunks[7],
    );
}

/// Render the text input field with a block border and cursor.
fn render_input_box(state: &SwitchState, frame: &mut Frame, area: Rect) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));
    let inner = input_block.inner(area);
    frame.render_widget(input_block, area);

    // Build spans: text before cursor, cursor char (or space), text after cursor.
    let buf = &state.input_buffer;
    let pos = state.cursor_pos;
    let chars: Vec<char> = buf.chars().collect();
    let len = chars.len();

    let before: String = chars[..pos].iter().collect();
    let cursor_ch: String = if pos < len {
        chars[pos].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if pos + 1 < len {
        chars[pos + 1..].iter().collect()
    } else {
        String::new()
    };

    let spans = vec![
        Span::raw(before),
        Span::styled(
            cursor_ch,
            Style::default().bg(theme::TEXT_ACCENT).fg(Color::Black),
        ),
        Span::raw(after),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Render the project-selector list with fuzzy filter.
fn render_project_selector(
    state: &SwitchState,
    projects: &[Project],
    frame: &mut Frame,
    area: Rect,
) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::MODAL_BORDER))
        .title(Span::styled(
            " Context Switch — Select Target ",
            Style::default().fg(theme::TEXT_ACCENT),
        ));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let filtered = filtered_projects(projects, &state.filter_buffer);

    // Layout: instruction line, filter line, list, keybind hint
    let chunks = Layout::vertical([
        Constraint::Length(1), // instruction
        Constraint::Length(1), // filter row
        Constraint::Min(1),    // list
        Constraint::Length(1), // keybind hint
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Switch to which project?",
            theme::bold(),
        )),
        chunks[0],
    );

    // Filter row: "Filter: abc_" or "(type to filter)" when empty
    let filter_line = if state.filter_buffer.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", theme::dim()),
            Span::styled("(type to filter)", theme::dim()),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", theme::dim()),
            Span::styled(state.filter_buffer.clone(), Style::default().fg(theme::TEXT_ACCENT)),
            Span::styled("_", Style::default().fg(theme::TEXT_ACCENT)),
        ])
    };
    frame.render_widget(Paragraph::new(filter_line), chunks[1]);

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let priority_marker = match p.priority {
                jm_core::models::Priority::High => Span::styled("! ", Style::default().fg(theme::PRIORITY_HIGH)),
                jm_core::models::Priority::Low => Span::styled("· ", theme::dim()),
                jm_core::models::Priority::Medium => Span::raw("  "),
            };
            let status_marker = match p.status {
                jm_core::models::Status::Blocked => Span::styled("⊘ ", Style::default().fg(theme::STATUS_BLOCKED)),
                jm_core::models::Status::Parked => Span::styled("◆ ", Style::default().fg(theme::STATUS_PARKED)),
                jm_core::models::Status::Done => Span::styled("✓ ", Style::default().fg(theme::STATUS_DONE)),
                _ => Span::raw("  "),
            };
            let name_span = if i == state.list_selected {
                Span::styled(&p.name, theme::selected())
            } else {
                Span::raw(&p.name)
            };
            ListItem::new(Line::from(vec![priority_marker, status_marker, name_span]))
        })
        .collect();

    if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No matches.", theme::dim())),
            chunks[2],
        );
    } else {
        frame.render_widget(List::new(items), chunks[2]);
    }

    let has_context = !state.left_off.is_empty()
        || !state.blocker.is_empty()
        || !state.next_step.is_empty();
    let escape_hint = if !state.filter_buffer.is_empty() {
        "j/k/g/G: navigate  Enter: select  Esc: clear filter"
    } else if has_context {
        "j/k/g/G: navigate  Enter: select  Esc: save context & stop"
    } else {
        "j/k/g/G: navigate  Enter: select  Esc: cancel"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(escape_hint, theme::dim())),
        chunks[3],
    );
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Return (step_num, total_steps, prompt_text, sub_hint) for a text-input step.
fn step_metadata(step: &SwitchStep) -> (u8, u8, &'static str, &'static str) {
    match step {
        SwitchStep::LeftOff => (1, 3, "Where did you leave off?", ""),
        SwitchStep::Blocker => (2, 3, "Any blockers?", "(Enter to skip)"),
        SwitchStep::NextStep => (3, 3, "What is the next step?", ""),
        _ => (0, 0, "", ""),
    }
}

/// Compute a centered rect within `area` at the given percentage sizes.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use jm_core::models::{Priority, Project, Status};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn make_projects(names: &[&str]) -> Vec<Project> {
        names
            .iter()
            .map(|&n| {
                let mut p = Project::new(n);
                p.status = Status::Active;
                p.priority = Priority::Medium;
                p
            })
            .collect()
    }

    // ── init ─────────────────────────────────────────────────────────

    #[test]
    fn test_init_starts_at_left_off_step() {
        let state = init(None);
        assert_eq!(state.step, SwitchStep::LeftOff);
        assert!(state.input_buffer.is_empty());
        assert!(state.left_off.is_empty());
        assert!(state.blocker.is_empty());
        assert!(state.next_step.is_empty());
        assert_eq!(state.cursor_pos, 0);
        assert_eq!(state.list_selected, 0);
        assert!(state.target_slug.is_none());
    }

    #[test]
    fn test_init_with_target_slug() {
        let state = init(Some("my-project"));
        assert_eq!(state.target_slug, Some("my-project".to_string()));
    }

    // ── Escape behavior ───────────────────────────────────────────────

    #[test]
    fn test_escape_at_left_off_cancels() {
        let mut state = init(None);
        let projects: Vec<Project> = Vec::new();
        let action = handle_key(&mut state, key(KeyCode::Esc), &projects);
        assert!(matches!(action, Action::Cancel));
    }

    #[test]
    fn test_escape_at_blocker_cancels() {
        let mut state = init(None);
        state.step = SwitchStep::Blocker;
        let action = handle_key(&mut state, key(KeyCode::Esc), &[]);
        assert!(matches!(action, Action::Cancel));
    }

    #[test]
    fn test_escape_at_select_project_with_context_saves() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.left_off = "working on the render loop".to_string();
        let action = handle_key(&mut state, key(KeyCode::Esc), &[]);
        assert!(matches!(action, Action::SaveContextOnly));
    }

    #[test]
    fn test_escape_at_select_project_without_context_cancels() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        // No context captured
        let action = handle_key(&mut state, key(KeyCode::Esc), &[]);
        assert!(matches!(action, Action::Cancel));
    }

    #[test]
    fn test_escape_at_select_project_with_filter_clears_filter_first() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.filter_buffer = "abc".to_string();
        let action = handle_key(&mut state, key(KeyCode::Esc), &[]);
        // Filter should be cleared, not cancel
        assert!(matches!(action, Action::None));
        assert!(state.filter_buffer.is_empty());
    }

    // ── Text input: character insertion ──────────────────────────────

    #[test]
    fn test_char_appended_to_input_buffer() {
        let mut state = init(None);
        handle_key(&mut state, char_key('h'), &[]);
        handle_key(&mut state, char_key('i'), &[]);
        assert_eq!(state.input_buffer, "hi");
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn test_backspace_removes_last_char() {
        let mut state = init(None);
        handle_key(&mut state, char_key('h'), &[]);
        handle_key(&mut state, char_key('i'), &[]);
        assert_eq!(state.input_buffer, "hi");

        handle_key(&mut state, key(KeyCode::Backspace), &[]);
        assert_eq!(state.input_buffer, "h");
        assert_eq!(state.cursor_pos, 1);
    }

    #[test]
    fn test_backspace_at_beginning_does_nothing() {
        let mut state = init(None);
        assert_eq!(state.cursor_pos, 0);
        handle_key(&mut state, key(KeyCode::Backspace), &[]);
        assert_eq!(state.input_buffer, "");
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn test_delete_removes_char_at_cursor() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 0;

        handle_key(&mut state, key(KeyCode::Delete), &[]);
        assert_eq!(state.input_buffer, "ello");
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn test_home_moves_cursor_to_start() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 5;

        handle_key(&mut state, key(KeyCode::Home), &[]);
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn test_end_moves_cursor_to_end() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 2;

        handle_key(&mut state, key(KeyCode::End), &[]);
        assert_eq!(state.cursor_pos, 5);
    }

    #[test]
    fn test_left_arrow_moves_cursor_left() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 3;

        handle_key(&mut state, key(KeyCode::Left), &[]);
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn test_left_arrow_at_start_stays() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 0;

        handle_key(&mut state, key(KeyCode::Left), &[]);
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn test_right_arrow_moves_cursor_right() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 2;

        handle_key(&mut state, key(KeyCode::Right), &[]);
        assert_eq!(state.cursor_pos, 3);
    }

    #[test]
    fn test_right_arrow_at_end_stays() {
        let mut state = init(None);
        state.input_buffer = "hello".to_string();
        state.cursor_pos = 5;

        handle_key(&mut state, key(KeyCode::Right), &[]);
        assert_eq!(state.cursor_pos, 5);
    }

    // ── Step advancement via Enter ────────────────────────────────────

    #[test]
    fn test_enter_at_left_off_advances_to_blocker() {
        let mut state = init(None);
        state.input_buffer = "working on render loop".to_string();

        let action = handle_key(&mut state, key(KeyCode::Enter), &[]);
        assert!(matches!(action, Action::None));
        assert_eq!(state.step, SwitchStep::Blocker);
        assert_eq!(state.left_off, "working on render loop");
        assert!(state.input_buffer.is_empty(), "buffer should be cleared after Enter");
    }

    #[test]
    fn test_enter_at_blocker_advances_to_next_step() {
        let mut state = init(None);
        state.step = SwitchStep::Blocker;
        state.input_buffer = "waiting on @carol".to_string();

        let action = handle_key(&mut state, key(KeyCode::Enter), &[]);
        assert!(matches!(action, Action::None));
        assert_eq!(state.step, SwitchStep::NextStep);
        assert_eq!(state.blocker, "waiting on @carol");
    }

    #[test]
    fn test_enter_at_blocker_with_empty_buffer_still_advances() {
        let mut state = init(None);
        state.step = SwitchStep::Blocker;
        // Empty buffer — blocker is optional

        let action = handle_key(&mut state, key(KeyCode::Enter), &[]);
        assert!(matches!(action, Action::None));
        assert_eq!(state.step, SwitchStep::NextStep);
        assert_eq!(state.blocker, "");
    }

    #[test]
    fn test_enter_at_next_step_with_target_completes() {
        let mut state = init(Some("target-proj"));
        state.step = SwitchStep::NextStep;
        state.input_buffer = "fix render bug".to_string();

        let action = handle_key(&mut state, key(KeyCode::Enter), &[]);
        assert!(matches!(action, Action::SwitchComplete));
        assert_eq!(state.step, SwitchStep::Done);
        assert_eq!(state.next_step, "fix render bug");
    }

    #[test]
    fn test_enter_at_next_step_without_target_goes_to_select() {
        let mut state = init(None); // no target slug
        state.step = SwitchStep::NextStep;
        state.input_buffer = "fix render bug".to_string();

        let action = handle_key(&mut state, key(KeyCode::Enter), &[]);
        assert!(matches!(action, Action::None));
        assert_eq!(state.step, SwitchStep::SelectProject);
    }

    // ── SelectProject list navigation ────────────────────────────────

    #[test]
    fn test_j_moves_down_in_project_list() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        let projects = make_projects(&["Alpha", "Beta", "Gamma"]);

        handle_key(&mut state, char_key('j'), &projects);
        assert_eq!(state.list_selected, 1);
    }

    #[test]
    fn test_j_does_not_go_past_last_project() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        let projects = make_projects(&["Alpha", "Beta"]);
        state.list_selected = 1; // already at last

        handle_key(&mut state, char_key('j'), &projects);
        assert_eq!(state.list_selected, 1, "should not go past last project");
    }

    #[test]
    fn test_k_moves_up_in_project_list() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.list_selected = 2;
        let projects = make_projects(&["Alpha", "Beta", "Gamma"]);

        handle_key(&mut state, char_key('k'), &projects);
        assert_eq!(state.list_selected, 1);
    }

    #[test]
    fn test_k_does_not_go_below_zero() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.list_selected = 0;
        let projects = make_projects(&["Alpha", "Beta"]);

        handle_key(&mut state, char_key('k'), &projects);
        assert_eq!(state.list_selected, 0);
    }

    #[test]
    fn test_g_goes_to_first_project() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.list_selected = 3;
        let projects = make_projects(&["A", "B", "C", "D"]);

        handle_key(&mut state, char_key('g'), &projects);
        assert_eq!(state.list_selected, 0);
    }

    #[test]
    fn test_shift_g_goes_to_last_project() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.list_selected = 0;
        let projects = make_projects(&["A", "B", "C", "D"]);

        handle_key(&mut state, key(KeyCode::Char('G')), &projects);
        assert_eq!(state.list_selected, 3);
    }

    #[test]
    fn test_enter_at_select_project_completes_switch() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        state.list_selected = 1;
        let projects = make_projects(&["Alpha", "Beta", "Gamma"]);

        let action = handle_key(&mut state, key(KeyCode::Enter), &projects);
        assert!(matches!(action, Action::SwitchComplete));
        assert_eq!(state.target_slug, Some("beta".to_string()));
        assert_eq!(state.step, SwitchStep::Done);
    }

    #[test]
    fn test_enter_at_select_project_with_empty_list_is_noop() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        let projects: Vec<Project> = Vec::new();

        let action = handle_key(&mut state, key(KeyCode::Enter), &projects);
        assert!(matches!(action, Action::None));
    }

    // ── Filter buffer in SelectProject ────────────────────────────────

    #[test]
    fn test_typing_in_select_step_appends_to_filter() {
        let mut state = init(None);
        state.step = SwitchStep::SelectProject;
        let projects = make_projects(&["Alpha", "Beta"]);

        handle_key(&mut state, char_key('a'), &projects);
        assert_eq!(state.filter_buffer, "a");
    }

    #[test]
    fn test_filtered_projects_empty_filter_returns_all() {
        let projects = make_projects(&["Alpha", "Beta", "Gamma"]);
        let result = filtered_projects(&projects, "");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filtered_projects_filters_by_name_substring() {
        let projects = make_projects(&["Alpha Project", "Beta Service", "Alpha Backend"]);
        let result = filtered_projects(&projects, "alpha");
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|p| p.name.to_lowercase().contains("alpha")));
    }

    #[test]
    fn test_filtered_projects_case_insensitive() {
        let projects = make_projects(&["UPPERCASE", "lowercase", "MiXeD"]);
        let result = filtered_projects(&projects, "upper");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "UPPERCASE");
    }

    #[test]
    fn test_filtered_projects_no_match_returns_empty() {
        let projects = make_projects(&["Alpha", "Beta"]);
        let result = filtered_projects(&projects, "zzz");
        assert!(result.is_empty());
    }

    // ── advance_step logic ────────────────────────────────────────────

    #[test]
    fn test_advance_step_left_off_to_blocker() {
        let mut state = init(None);
        advance_step(&mut state);
        assert_eq!(state.step, SwitchStep::Blocker);
    }

    #[test]
    fn test_advance_step_blocker_to_next_step() {
        let mut state = init(None);
        state.step = SwitchStep::Blocker;
        advance_step(&mut state);
        assert_eq!(state.step, SwitchStep::NextStep);
    }

    #[test]
    fn test_advance_step_next_step_to_select_when_no_target() {
        let mut state = init(None);
        state.step = SwitchStep::NextStep;
        advance_step(&mut state);
        assert_eq!(state.step, SwitchStep::SelectProject);
    }

    #[test]
    fn test_advance_step_next_step_to_done_when_target_set() {
        let mut state = init(Some("target-slug"));
        state.step = SwitchStep::NextStep;
        advance_step(&mut state);
        assert_eq!(state.step, SwitchStep::Done);
    }

    #[test]
    fn test_advance_step_clears_input_buffer() {
        let mut state = init(None);
        state.input_buffer = "some text".to_string();
        state.cursor_pos = 9;
        advance_step(&mut state);
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_pos, 0);
    }

    // ── Unicode input ─────────────────────────────────────────────────

    #[test]
    fn test_unicode_chars_in_input_buffer() {
        let mut state = init(None);
        for c in "日本語".chars() {
            handle_key(&mut state, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), &[]);
        }
        assert_eq!(state.input_buffer, "日本語");
        assert_eq!(state.cursor_pos, 3);
    }

    #[test]
    fn test_backspace_on_unicode() {
        let mut state = init(None);
        state.input_buffer = "日本語".to_string();
        state.cursor_pos = 3;

        handle_key(&mut state, key(KeyCode::Backspace), &[]);
        assert_eq!(state.input_buffer, "日本");
        assert_eq!(state.cursor_pos, 2);
    }
}

