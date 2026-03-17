//! Project detail view — shows all sections of a single project.
//!
//! Layout (scrollable):
//!   Title bar: name, status badge, priority
//!   Tags / Created / Target metadata row
//!   ── Focus ──
//!   ── Blockers (N) ──
//!   ── Decisions ──
//!   ── Log ──

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use jm_core::models::Project;

use crate::events::Action;
use crate::theme;

// ── State ────────────────────────────────────────────────────────────

pub struct ProjectViewState {
    #[allow(dead_code)]
    pub slug: String,
    pub scroll_offset: usize,
    #[allow(dead_code)]
    pub selected_section: usize, // 0=focus, 1=blockers, 2=decisions, 3=log
}

// ── Public API ───────────────────────────────────────────────────────

pub fn init(slug: &str) -> ProjectViewState {
    ProjectViewState {
        slug: slug.to_string(),
        scroll_offset: 0,
        selected_section: 0,
    }
}

pub fn render(
    state: &ProjectViewState,
    project: &Project,
    references: &[(String, String)],
    frame: &mut Frame,
    area: Rect,
) {
    let lines = build_lines(project, references, area.width as usize);

    // Outer block with title bar containing name + status badge + priority
    let (badge_text, badge_style) = theme::status_badge(project.status);
    let pri_style = theme::priority_style(project.priority);
    let priority_str = project.priority.to_string();

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(&project.name, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ─── "),
        Span::styled(badge_text, badge_style),
        Span::raw(" "),
        Span::styled(priority_str, pri_style),
        Span::raw(" "),
    ]);

    let hint = Line::from(vec![Span::styled(
        " e:focus  S:status  P:priority  t:tags  T:target  n:note  b:blocker  d:decision  u:unblock  m:move  x:delete  Esc:back ",
        theme::dim(),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::focused_border())
        .title(title)
        .title_bottom(hint);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset as u16, 0));

    frame.render_widget(paragraph, inner);
}

pub fn handle_key(
    state: &mut ProjectViewState,
    key: KeyEvent,
    project: &Project,
) -> Action {
    match key.code {
        KeyCode::Esc => Action::Back,

        // Project edits
        KeyCode::Char('e') => Action::EditFocus,
        KeyCode::Char('S') => Action::CycleStatus,
        KeyCode::Char('P') => Action::CyclePriority,
        KeyCode::Char('t') => Action::EditTags,
        KeyCode::Char('T') => Action::EditTarget,
        KeyCode::Char('x') => Action::DeleteProject,
        KeyCode::Char('m') => Action::MoveBlocker,
        KeyCode::Char('n') => Action::QuickNote,
        KeyCode::Char('b') => Action::QuickBlocker,
        KeyCode::Char('o') => Action::OpenEditor,
        // Ctrl+D/U must come before bare d/u
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_add(10);
            Action::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_sub(10);
            Action::None
        }
        KeyCode::Char('d') => Action::QuickDecision,
        KeyCode::Char('u') => Action::Unblock,

        // Scroll
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll_offset = state.scroll_offset.saturating_add(1);
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1);
            Action::None
        }
        KeyCode::Char('g') => {
            state.scroll_offset = 0;
            Action::None
        }
        KeyCode::Char('G') => {
            let total = count_lines(project);
            state.scroll_offset = total.saturating_sub(1);
            Action::None
        }

        _ => Action::None,
    }
}

// ── Line builders ────────────────────────────────────────────────────

/// Build the full scrollable content as a `Vec<Line>`.
fn build_lines<'a>(project: &'a Project, references: &[(String, String)], _width: usize) -> Vec<Line<'a>> {
    let mut lines: Vec<Line<'a>> = Vec::new();

    // ── Metadata row ────────────────────────────────────────────────
    push_metadata_rows(&mut lines, project);

    lines.push(Line::raw(""));

    // ── Focus section ───────────────────────────────────────────────
    push_section_header(&mut lines, "Focus", 0);
    if project.current_focus.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  No focus set. Press e to set one.",
            theme::empty_hint(),
        )]));
    } else {
        for text_line in project.current_focus.split('\n') {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(text_line.to_string()),
            ]));
        }
    }

    lines.push(Line::raw(""));

    // ── Blockers section ────────────────────────────────────────────
    let _open_count = project.blockers.iter().filter(|b| !b.resolved).count();
    let blocker_header = if project.blockers.is_empty() {
        "Blockers".to_string()
    } else {
        format!("Blockers ({})", project.blockers.len())
    };
    push_section_header_str(&mut lines, &blocker_header, 1);

    if project.blockers.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  No open blockers.",
            theme::empty_hint(),
        )]));
    } else {
        for blocker in &project.blockers {
            lines.push(build_blocker_line(blocker));
        }
    }

    lines.push(Line::raw(""));

    // ── Decisions section ───────────────────────────────────────────
    push_section_header(&mut lines, "Decisions", 2);
    if project.decisions.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  No decisions logged.",
            theme::empty_hint(),
        )]));
    } else {
        for decision in &project.decisions {
            // Date: Choice
            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                decision.date.to_string(),
                theme::timestamp_style(),
            ));
            spans.push(Span::styled(": ", theme::dim()));
            spans.push(Span::raw(decision.choice.clone()));
            lines.push(Line::from(spans));

            // Alternatives
            if !decision.alternatives.is_empty() {
                let alts = decision.alternatives.join(", ");
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("Alternatives: ", theme::dim()),
                    Span::raw(alts),
                ]));
            }
        }
    }

    lines.push(Line::raw(""));

    // ── Log section ─────────────────────────────────────────────────
    push_section_header(&mut lines, "Log", 3);
    if project.log.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  No entries. Press n to add a note.",
            theme::empty_hint(),
        )]));
    } else {
        for entry in &project.log {
            // Date header
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(entry.date.to_string(), theme::timestamp_style()),
            ]));
            for log_line in &entry.lines {
                lines.push(Line::from(render_with_links(&format!("   - {log_line}"))));
            }
        }
    }

    lines.push(Line::raw(""));

    // ── Referenced by section ───────────────────────────────────────
    if !references.is_empty() {
        let header = format!("Referenced by ({})", references.len());
        push_section_header_str(&mut lines, &header, 4);
        for (slug, context) in references {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[[{slug}]]"),
                    Style::default().fg(theme::TEXT_ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(": ", theme::dim()),
                Span::raw(truncate_context(context, 60)),
            ]));
        }
        lines.push(Line::raw(""));
    }

    lines
}

/// Metadata rows: tags, created, target.
fn push_metadata_rows<'a>(lines: &mut Vec<Line<'a>>, project: &'a Project) {
    // Tags row
    if !project.tags.is_empty() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("Tags: ", theme::dim()));
        for (i, tag) in project.tags.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(", "));
            }
            spans.push(Span::styled(tag.clone(), theme::tag_style()));
        }
        lines.push(Line::from(spans));
    }

    // Created / Target row
    let mut date_spans: Vec<Span<'static>> = Vec::new();
    date_spans.push(Span::styled("Created: ", theme::dim()));
    date_spans.push(Span::styled(
        project.created.to_string(),
        theme::timestamp_style(),
    ));
    if let Some(target) = project.target {
        date_spans.push(Span::styled("  Target: ", theme::dim()));
        date_spans.push(Span::styled(target.to_string(), theme::timestamp_style()));
    }
    lines.push(Line::from(date_spans));
}

/// Push a styled section divider line with a static string label.
fn push_section_header(lines: &mut Vec<Line<'_>>, label: &'static str, _section_idx: usize) {
    lines.push(Line::from(vec![
        Span::styled("── ", theme::dim()),
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            " ──────────────────────────────────────────────────────────",
            theme::dim(),
        ),
    ]));
}

/// Push a styled section divider with a runtime (owned) string label.
fn push_section_header_str(lines: &mut Vec<Line<'_>>, label: &str, _section_idx: usize) {
    lines.push(Line::from(vec![
        Span::styled("── ", theme::dim()),
        Span::styled(
            label.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ──────────────────────────────────────────────────────────",
            theme::dim(),
        ),
    ]));
}

/// Build a single rendered line for a blocker.
fn build_blocker_line(blocker: &jm_core::models::Blocker) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    if blocker.resolved {
        // ✓ dim + strikethrough description (resolved)
        spans.push(Span::styled("  ✓ ", theme::dim()));
        spans.push(Span::styled(
            blocker.description.clone(),
            Style::default()
                .fg(ratatui::style::Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT),
        ));
        if let Some(ref person) = blocker.person {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(person.clone(), theme::person_style()));
        }
        if let Some(resolved_date) = blocker.resolved_date {
            spans.push(Span::styled(
                format!(" (resolved {})", resolved_date),
                theme::dim(),
            ));
        }
    } else {
        // ⊘ red (open/unresolved)
        spans.push(Span::styled(
            "  ⊘ ",
            Style::default().fg(ratatui::style::Color::Red),
        ));
        spans.push(Span::styled(
            blocker.description.clone(),
            Style::default().fg(ratatui::style::Color::Reset),
        ));
        if let Some(ref person) = blocker.person {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(person.clone(), theme::person_style()));
        }
        if let Some(since) = blocker.since {
            spans.push(Span::styled(
                format!(" (since {})", since),
                theme::timestamp_style(),
            ));
        }
    }

    Line::from(spans)
}

/// Render text with `[[slug]]` crosslinks highlighted.
fn render_with_links(text: &str) -> Vec<Span<'static>> {
    let parts = jm_core::crosslinks::split_with_links(text);
    parts
        .into_iter()
        .map(|(s, is_link)| {
            if is_link {
                Span::styled(s, Style::default().fg(theme::TEXT_ACCENT).add_modifier(Modifier::BOLD))
            } else {
                Span::raw(s)
            }
        })
        .collect()
}

/// Truncate a context string to a max length.
fn truncate_context(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}\u{2026}")
    }
}

/// Rough line count for G (go-to-bottom) scroll target.
fn count_lines(project: &Project) -> usize {
    let mut count = 0;

    // metadata: tags row + dates row + blank
    if !project.tags.is_empty() {
        count += 1;
    }
    count += 2; // dates + blank

    // Focus section header + content
    count += 1;
    count += if project.current_focus.is_empty() {
        1
    } else {
        project.current_focus.split('\n').count().max(1)
    };
    count += 1; // blank

    // Blockers
    count += 1; // header
    count += if project.blockers.is_empty() {
        1
    } else {
        project.blockers.len()
    };
    count += 1;

    // Decisions
    count += 1;
    count += if project.decisions.is_empty() {
        1
    } else {
        project.decisions.iter().fold(0, |acc, d| {
            acc + 1 + if d.alternatives.is_empty() { 0 } else { 1 }
        })
    };
    count += 1;

    // Log
    count += 1;
    count += if project.log.is_empty() {
        1
    } else {
        project
            .log
            .iter()
            .fold(0, |acc, e| acc + 1 + e.lines.len())
    };
    count += 1;

    count
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

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn make_project() -> Project {
        let mut p = Project::new("Test Project");
        p.status = Status::Active;
        p.priority = Priority::Medium;
        p
    }

    fn make_state() -> ProjectViewState {
        init("test-project")
    }

    // ── Navigation ───────────────────────────────────────────────────

    #[test]
    fn test_escape_returns_back() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Esc), &project);
        assert!(matches!(action, Action::Back));
    }

    #[test]
    fn test_j_scrolls_down() {
        let mut state = make_state();
        let project = make_project();
        let before = state.scroll_offset;
        let action = handle_key(&mut state, key(KeyCode::Char('j')), &project);
        assert!(matches!(action, Action::None));
        assert_eq!(state.scroll_offset, before + 1);
    }

    #[test]
    fn test_down_arrow_scrolls_down() {
        let mut state = make_state();
        let project = make_project();
        handle_key(&mut state, key(KeyCode::Down), &project);
        assert_eq!(state.scroll_offset, 1);
    }

    #[test]
    fn test_k_scrolls_up() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 5;
        let action = handle_key(&mut state, key(KeyCode::Char('k')), &project);
        assert!(matches!(action, Action::None));
        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_k_does_not_scroll_below_zero() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 0;
        handle_key(&mut state, key(KeyCode::Char('k')), &project);
        assert_eq!(state.scroll_offset, 0, "scroll_offset must not underflow");
    }

    #[test]
    fn test_up_arrow_does_not_scroll_below_zero() {
        let mut state = make_state();
        let project = make_project();
        handle_key(&mut state, key(KeyCode::Up), &project);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_g_resets_scroll_to_top() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 10;
        let action = handle_key(&mut state, key(KeyCode::Char('g')), &project);
        assert!(matches!(action, Action::None));
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_ctrl_d_scrolls_down_10() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 5;
        let action = handle_key(&mut state, ctrl_key(KeyCode::Char('d')), &project);
        assert!(matches!(action, Action::None));
        assert_eq!(state.scroll_offset, 15);
    }

    #[test]
    fn test_ctrl_u_scrolls_up_10() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 15;
        let action = handle_key(&mut state, ctrl_key(KeyCode::Char('u')), &project);
        assert!(matches!(action, Action::None));
        assert_eq!(state.scroll_offset, 5);
    }

    #[test]
    fn test_ctrl_u_saturates_at_zero() {
        let mut state = make_state();
        let project = make_project();
        state.scroll_offset = 3;
        handle_key(&mut state, ctrl_key(KeyCode::Char('u')), &project);
        assert_eq!(state.scroll_offset, 0);
    }

    // ── Edit actions ─────────────────────────────────────────────────

    #[test]
    fn test_e_returns_edit_focus() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('e')), &project);
        assert!(matches!(action, Action::EditFocus));
    }

    #[test]
    fn test_shift_s_returns_cycle_status() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('S')), &project);
        assert!(matches!(action, Action::CycleStatus));
    }

    #[test]
    fn test_shift_p_returns_cycle_priority() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('P')), &project);
        assert!(matches!(action, Action::CyclePriority));
    }

    #[test]
    fn test_t_returns_edit_tags() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('t')), &project);
        assert!(matches!(action, Action::EditTags));
    }

    #[test]
    fn test_shift_t_returns_edit_target() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('T')), &project);
        assert!(matches!(action, Action::EditTarget));
    }

    #[test]
    fn test_x_returns_delete_project() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('x')), &project);
        assert!(matches!(action, Action::DeleteProject));
    }

    #[test]
    fn test_m_returns_move_blocker() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('m')), &project);
        assert!(matches!(action, Action::MoveBlocker));
    }

    #[test]
    fn test_n_returns_quick_note() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('n')), &project);
        assert!(matches!(action, Action::QuickNote));
    }

    #[test]
    fn test_b_returns_quick_blocker() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('b')), &project);
        assert!(matches!(action, Action::QuickBlocker));
    }

    #[test]
    fn test_d_returns_quick_decision_bare() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('d')), &project);
        assert!(matches!(action, Action::QuickDecision));
    }

    #[test]
    fn test_u_returns_unblock() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('u')), &project);
        assert!(matches!(action, Action::Unblock));
    }

    #[test]
    fn test_unknown_key_returns_none() {
        let mut state = make_state();
        let project = make_project();
        let action = handle_key(&mut state, key(KeyCode::Char('z')), &project);
        assert!(matches!(action, Action::None));
    }
}
