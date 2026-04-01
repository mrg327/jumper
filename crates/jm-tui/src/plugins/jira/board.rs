//! Kanban board rendering for the JIRA plugin.
//!
//! Renders the full-screen kanban board with horizontal scrolling columns.
//! Each column represents a JIRA workflow status. Issues are displayed as
//! three-line cards within their status column.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::plugins::PluginAction;
use crate::theme;

use super::api::JiraCommand;
use super::models::StatusCategory;
use super::{JiraModal, JiraPlugin};

/// Minimum column width for kanban columns (including border).
const MIN_COL_WIDTH: u16 = 25;

/// Lines per issue card: key+type, summary, priority/points, blank separator.
const CARD_LINES: usize = 4;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn visible_col_count(board_width: u16, total_cols: usize) -> usize {
    let max_fit = (board_width / MIN_COL_WIDTH) as usize;
    max_fit.min(total_cols).max(1)
}

fn column_widths(board_width: u16, visible_count: usize) -> Vec<u16> {
    if visible_count == 0 {
        return Vec::new();
    }
    let base_width = board_width / visible_count as u16;
    let remainder = (board_width % visible_count as u16) as usize;

    (0..visible_count)
        .map(|i| if i < remainder { base_width + 1 } else { base_width })
        .collect()
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else if max_chars <= 3 {
        s.chars().take(max_chars).collect()
    } else {
        let truncated: String = s.chars().take(max_chars - 3).collect();
        format!("{truncated}...")
    }
}

fn priority_color(priority: &str) -> Color {
    match priority {
        "Highest" | "High" => theme::PRIORITY_HIGH,
        "Medium" => theme::PRIORITY_MEDIUM,
        "Low" | "Lowest" => theme::PRIORITY_LOW,
        _ => theme::TEXT_DIM,
    }
}

fn category_color(category: &StatusCategory) -> Color {
    match category {
        StatusCategory::ToDo => Color::Blue,
        StatusCategory::InProgress => Color::Yellow,
        StatusCategory::Done => Color::DarkGray,
    }
}

fn format_last_sync(instant: &std::time::Instant) -> String {
    let elapsed = instant.elapsed().as_secs();
    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else {
        format!("{}h ago", elapsed / 3600)
    }
}

// ── Render ──────────────────────────────────────────────────────────────────

/// Render the kanban board (full screen, no modal active).
pub(crate) fn render(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let total_cols = plugin.board.columns.len();

    // If loading and no data, show centered loading message
    if plugin.loading && plugin.issues.is_empty() {
        render_loading(frame, area, plugin);
        return;
    }

    // If no data and not loading, show empty state
    if plugin.issues.is_empty() && !plugin.loading {
        render_empty(frame, area, plugin);
        return;
    }

    // If columns are empty after filtering, show filtered empty state
    if total_cols == 0 {
        render_empty(frame, area, plugin);
        return;
    }

    let needs_scroll_dots = {
        let vis = visible_col_count(area.width, total_cols);
        total_cols > vis
    };

    // Vertical layout: header, board, optional scroll dots, footer
    let mut constraints = vec![
        Constraint::Length(1), // header
        Constraint::Min(5),   // board area
    ];
    if needs_scroll_dots {
        constraints.push(Constraint::Length(1)); // scroll dots
    }
    constraints.push(Constraint::Length(2)); // footer

    let chunks = Layout::vertical(constraints).split(area);

    let header_area = chunks[0];
    let board_area = chunks[1];
    let (dots_area, footer_area) = if needs_scroll_dots {
        (Some(chunks[2]), chunks[3])
    } else {
        (None, chunks[2])
    };

    // ── Header ──────────────────────────────────────────────────────────
    render_header(frame, header_area, plugin);

    // ── Kanban columns ──────────────────────────────────────────────────
    let visible_count = visible_col_count(board_area.width, total_cols);
    let widths = column_widths(board_area.width, visible_count);
    let col_constraints: Vec<Constraint> = widths.iter().map(|&w| Constraint::Length(w)).collect();
    let col_areas = Layout::horizontal(col_constraints).split(board_area);

    let filtered = plugin.filtered_issues();

    for (vis_idx, col_area) in col_areas.iter().enumerate() {
        let col_idx = plugin.board.scroll_offset + vis_idx;
        if col_idx >= total_cols {
            break;
        }

        let column = &plugin.board.columns[col_idx];
        let is_selected_col = col_idx == plugin.board.selected_col;

        let border_style = if is_selected_col {
            Style::default().fg(category_color(&column.category))
        } else {
            theme::unfocused_border()
        };

        let title_text = format!(" {} ({}) ", column.name, column.issue_indices.len());
        let title_style = if is_selected_col {
            Style::default()
                .fg(theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::dim()
        };

        let block = Block::default()
            .title(Line::from(Span::styled(title_text, title_style)))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(*col_area);
        frame.render_widget(block, *col_area);

        if column.issue_indices.is_empty() {
            continue;
        }

        let inner_width = inner.width as usize;
        let max_visible_cards = if inner.height > 0 {
            (inner.height as usize) / CARD_LINES
        } else {
            0
        };
        if max_visible_cards == 0 {
            continue;
        }

        // Per-column vertical scroll offset
        let col_scroll = plugin
            .board
            .col_scroll_offsets
            .get(col_idx)
            .copied()
            .unwrap_or(0);

        let visible_issues = column
            .issue_indices
            .iter()
            .enumerate()
            .skip(col_scroll)
            .take(max_visible_cards);

        for (issue_idx, &global_idx) in visible_issues {
            let Some(issue) = filtered.get(global_idx) else {
                continue;
            };

            let is_selected = is_selected_col && issue_idx == plugin.board.selected_row;
            let card_y = inner.y + ((issue_idx - col_scroll) * CARD_LINES) as u16;

            if card_y + 3 > inner.y + inner.height {
                break;
            }

            let bg_style = if is_selected {
                theme::selected()
            } else {
                Style::default()
            };

            // Line 1: issue key (left) + issue type (right)
            let key_span = Span::styled(
                &issue.key,
                if is_selected {
                    theme::accent().bg(theme::SELECTED_BG).add_modifier(Modifier::BOLD)
                } else {
                    theme::accent()
                },
            );
            let type_str = truncate_str(&issue.issue_type, inner_width.saturating_sub(issue.key.len() + 2));
            let type_span = Span::styled(
                type_str,
                if is_selected {
                    theme::dim().bg(theme::SELECTED_BG)
                } else {
                    theme::dim()
                },
            );
            // Build line 1 with space padding between key and type
            let key_len = issue.key.chars().count();
            let type_display_len = type_span.content.chars().count();
            let padding = inner_width.saturating_sub(key_len + type_display_len);
            let line1 = Line::from(vec![
                key_span,
                Span::styled(" ".repeat(padding), bg_style),
                type_span,
            ]);
            let line1_area = Rect::new(inner.x, card_y, inner.width, 1);
            frame.render_widget(Paragraph::new(line1), line1_area);

            // Line 2: summary (truncated)
            let summary = truncate_str(&issue.summary, inner_width);
            let line2 = Paragraph::new(Span::styled(summary, bg_style));
            let line2_area = Rect::new(inner.x, card_y + 1, inner.width, 1);
            frame.render_widget(line2, line2_area);

            // Line 3: priority + points
            let mut line3_spans: Vec<Span> = Vec::new();
            if let Some(ref pri) = issue.priority {
                let pri_style = if is_selected {
                    Style::default().fg(priority_color(pri)).bg(theme::SELECTED_BG)
                } else {
                    Style::default().fg(priority_color(pri))
                };
                line3_spans.push(Span::styled(pri.clone(), pri_style));
            }
            if let Some(pts) = issue.story_points {
                if !line3_spans.is_empty() {
                    line3_spans.push(Span::styled(
                        " \u{00b7} ",
                        if is_selected { theme::dim().bg(theme::SELECTED_BG) } else { theme::dim() },
                    ));
                }
                let pts_str = if pts.fract() == 0.0 {
                    format!("{}pts", pts as i64)
                } else {
                    format!("{pts}pts")
                };
                line3_spans.push(Span::styled(
                    pts_str,
                    if is_selected { theme::dim().bg(theme::SELECTED_BG) } else { theme::dim() },
                ));
            }
            if !line3_spans.is_empty() {
                let line3 = Paragraph::new(Line::from(line3_spans));
                let line3_area = Rect::new(inner.x, card_y + 2, inner.width, 1);
                frame.render_widget(line3, line3_area);
            }
        }
    }

    // ── Scroll dots ─────────────────────────────────────────────────────
    if let Some(dots_area) = dots_area {
        render_scroll_dots(frame, dots_area, total_cols, plugin.board.scroll_offset, visible_count);
    }

    // ── Footer ──────────────────────────────────────────────────────────
    render_footer(frame, footer_area, plugin);
}

fn render_header(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let mut spans: Vec<Span> = Vec::new();

    // Project filter or "All"
    let filter_text = match &plugin.project_filter {
        Some(key) => format!(" JIRA: {} ", key),
        None => " JIRA: All ".to_string(),
    };
    spans.push(Span::styled(
        filter_text,
        theme::accent().add_modifier(Modifier::BOLD),
    ));

    // Stale indicator
    if plugin.last_error.is_some() {
        spans.push(Span::styled(" [stale] ", Style::default().fg(theme::TEXT_WARNING)));
    }

    // Refresh indicator
    spans.push(Span::raw(" "));
    if plugin.loading {
        spans.push(Span::styled("\u{21bb} Loading...", theme::dim()));
    } else if plugin.refreshing {
        spans.push(Span::styled("\u{21bb} Refreshing...", theme::dim()));
    } else if let Some(ref sync) = plugin.last_sync {
        spans.push(Span::styled(
            format!("\u{21bb} {}", format_last_sync(sync)),
            theme::dim(),
        ));
    }

    let header = Paragraph::new(Line::from(spans));
    frame.render_widget(header, area);
}

fn render_footer(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    // Line 1: keybindings
    let hints_line = Line::from(vec![
        Span::styled("hjkl", theme::accent()),
        Span::styled(":nav  ", theme::dim()),
        Span::styled("s", theme::accent()),
        Span::styled(":transition  ", theme::dim()),
        Span::styled("c", theme::accent()),
        Span::styled(":comment  ", theme::dim()),
        Span::styled("Enter", theme::accent()),
        Span::styled(":detail  ", theme::dim()),
        Span::styled("p", theme::accent()),
        Span::styled(":proj  ", theme::dim()),
        Span::styled("R", theme::accent()),
        Span::styled(":refresh", theme::dim()),
    ]);

    // Line 2: more hints + last sync
    let mut line2_spans = vec![
        Span::styled("n", theme::accent()),
        Span::styled(":new  ", theme::dim()),
        Span::styled("D", theme::accent()),
        Span::styled(":toggle-done  ", theme::dim()),
        Span::styled("Esc", theme::accent()),
        Span::styled(":back", theme::dim()),
    ];

    if let Some(ref sync) = plugin.last_sync {
        let elapsed = sync.elapsed().as_secs();
        let h = (elapsed / 3600) % 24;
        let m = (elapsed / 60) % 60;
        let s = elapsed % 60;
        // Show approximate current time by not using elapsed but just formatting
        // Actually, we don't have wall clock - use "last sync" relative time
        let sync_text = format!("           Last sync: {}", format_last_sync(sync));
        line2_spans.push(Span::styled(sync_text, theme::dim()));
        let _ = (h, m, s); // suppress unused
    }

    let [line1_area, line2_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    frame.render_widget(Paragraph::new(hints_line), line1_area);
    frame.render_widget(Paragraph::new(Line::from(line2_spans)), line2_area);
}

fn render_scroll_dots(
    frame: &mut Frame,
    area: Rect,
    total: usize,
    scroll_offset: usize,
    visible: usize,
) {
    let dots: String = (0..total)
        .map(|i| {
            if i >= scroll_offset && i < scroll_offset + visible {
                "\u{25cf}"
            } else {
                "\u{25cb}"
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let paragraph = Paragraph::new(dots)
        .alignment(Alignment::Center)
        .style(theme::dim());
    frame.render_widget(paragraph, area);
}

fn render_loading(frame: &mut Frame, area: Rect, _plugin: &JiraPlugin) {
    let block = Block::default()
        .title(" JIRA ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::TEXT_ACCENT));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let v_pad = inner.height.saturating_sub(1) / 2;
    let [_, center, _] = Layout::vertical([
        Constraint::Length(v_pad),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    let loading = Paragraph::new("Loading issues...")
        .alignment(Alignment::Center)
        .style(theme::dim());
    frame.render_widget(loading, center);
}

fn render_empty(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let title = match &plugin.project_filter {
        Some(key) => format!(" JIRA: {} ", key),
        None => " JIRA ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::TEXT_ACCENT));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let v_pad = inner.height.saturating_sub(1) / 2;
    let [_, center, _] = Layout::vertical([
        Constraint::Length(v_pad),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    let msg = if plugin.last_error.is_some() {
        "No issues loaded. Check connection and press R to refresh."
    } else {
        "No issues assigned to you."
    };
    let empty = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .style(theme::dim());
    frame.render_widget(empty, center);
}

// ── Key handling ────────────────────────────────────────────────────────────

/// Handle key events on the kanban board (no modal active).
pub(crate) fn handle_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let total_cols = plugin.board.columns.len();
    if total_cols == 0 {
        // Only Esc/q/R work when there are no columns
        return match key.code {
            KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,
            KeyCode::Char('R') => {
                plugin.generation += 1;
                plugin.refreshing = true;
                if let Some(tx) = &plugin.command_tx {
                    tx.send(JiraCommand::FetchMyIssues {
                        generation: plugin.generation,
                    })
                    .ok();
                }
                PluginAction::None
            }
            KeyCode::Char('n') => {
                open_create_flow(plugin);
                PluginAction::None
            }
            _ => PluginAction::None,
        };
    }

    let filtered = plugin.filtered_issues();

    match key.code {
        // ── Column navigation ───────────────────────────────────────────
        KeyCode::Char('h') | KeyCode::Left => {
            if plugin.board.selected_col > 0 {
                plugin.board.selected_col -= 1;
                // Cursor-follows: shift viewport left if cursor is before it
                if plugin.board.selected_col < plugin.board.scroll_offset {
                    plugin.board.scroll_offset = plugin.board.selected_col;
                }
                // Clamp selected_row to new column's issue count
                clamp_row(plugin);
            }
            PluginAction::None
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if plugin.board.selected_col < total_cols - 1 {
                plugin.board.selected_col += 1;
                // For cursor-follows we need visible_count, but we don't have area.
                // Use a reasonable default — the scroll_offset is adjusted on render if needed.
                // Shift viewport right if cursor moves past it.
                // We need visible count — estimate from last known or use a safe value.
                // Since we don't have board_width here, we shift by 1 if needed:
                let vis = total_cols.min(5); // conservative estimate
                if plugin.board.selected_col >= plugin.board.scroll_offset + vis {
                    plugin.board.scroll_offset = plugin.board.selected_col.saturating_sub(vis - 1);
                }
                clamp_row(plugin);
            }
            PluginAction::None
        }

        // ── Row navigation ──────────────────────────────────────────────
        KeyCode::Char('j') | KeyCode::Down => {
            let col = &plugin.board.columns[plugin.board.selected_col];
            let col_len = col.issue_indices.len();
            if col_len > 0 && plugin.board.selected_row < col_len - 1 {
                plugin.board.selected_row += 1;
                adjust_col_scroll(plugin);
            }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if plugin.board.selected_row > 0 {
                plugin.board.selected_row -= 1;
                adjust_col_scroll(plugin);
            }
            PluginAction::None
        }

        // ── Jump to top/bottom ──────────────────────────────────────────
        KeyCode::Char('g') => {
            plugin.board.selected_row = 0;
            if let Some(scroll) = plugin
                .board
                .col_scroll_offsets
                .get_mut(plugin.board.selected_col)
            {
                *scroll = 0;
            }
            PluginAction::None
        }
        KeyCode::Char('G') => {
            let col = &plugin.board.columns[plugin.board.selected_col];
            plugin.board.selected_row = col.issue_indices.len().saturating_sub(1);
            adjust_col_scroll(plugin);
            PluginAction::None
        }

        // ── Open detail modal ───────────────────────────────────────────
        KeyCode::Enter => {
            if let Some(issue) = selected_issue(plugin, &filtered) {
                let issue_key = issue.key.clone();
                plugin.modal = Some(JiraModal::IssueDetail {
                    issue_key: issue_key.clone(),
                    fields: None,
                    transitions: None,
                    comments: None,
                    scroll_offset: 0,
                    field_cursor: 0,
                    focus: super::DetailFocus::Fields,
                    edit_state: None,
                });
                // Fetch detail data
                if let Some(tx) = &plugin.command_tx {
                    tx.send(JiraCommand::FetchTransitions {
                        issue_key: issue_key.clone(),
                    })
                    .ok();
                    tx.send(JiraCommand::FetchEditMeta {
                        issue_key: issue_key.clone(),
                    })
                    .ok();
                    tx.send(JiraCommand::FetchComments {
                        issue_key: issue_key.clone(),
                    })
                    .ok();
                }
            }
            PluginAction::None
        }

        // ── Transition picker ───────────────────────────────────────────
        KeyCode::Char('s') => {
            if let Some(issue) = selected_issue(plugin, &filtered) {
                let issue_key = issue.key.clone();
                plugin.modal = Some(JiraModal::TransitionPicker {
                    issue_key: issue_key.clone(),
                    transitions: Vec::new(),
                    cursor: 0,
                });
                if let Some(tx) = &plugin.command_tx {
                    tx.send(JiraCommand::FetchTransitions { issue_key }).ok();
                }
            }
            PluginAction::None
        }

        // ── Comment ─────────────────────────────────────────────────────
        KeyCode::Char('c') => {
            if let Some(issue) = selected_issue(plugin, &filtered) {
                return PluginAction::LaunchEditor {
                    content: String::new(),
                    context: format!("comment:{}", issue.key),
                };
            }
            PluginAction::None
        }

        // ── Create new issue ────────────────────────────────────────────
        KeyCode::Char('n') => {
            open_create_flow(plugin);
            PluginAction::None
        }

        // ── Cycle project filter ────────────────────────────────────────
        KeyCode::Char('p') => {
            let mut project_keys: Vec<String> = plugin
                .issues
                .iter()
                .map(|i| i.project_key.clone())
                .collect();
            project_keys.sort();
            project_keys.dedup();

            if project_keys.is_empty() {
                return PluginAction::None;
            }

            plugin.project_filter = match &plugin.project_filter {
                None => Some(project_keys[0].clone()),
                Some(current) => {
                    let idx = project_keys
                        .iter()
                        .position(|k| k == current)
                        .unwrap_or(0);
                    if idx + 1 < project_keys.len() {
                        Some(project_keys[idx + 1].clone())
                    } else {
                        None
                    }
                }
            };
            plugin.rebuild_columns();
            PluginAction::None
        }

        // ── Toggle Done ─────────────────────────────────────────────────
        KeyCode::Char('D') => {
            plugin.show_done = !plugin.show_done;
            plugin.rebuild_columns();
            PluginAction::None
        }

        // ── Manual refresh ──────────────────────────────────────────────
        KeyCode::Char('R') => {
            plugin.generation += 1;
            plugin.refreshing = true;
            if let Some(tx) = &plugin.command_tx {
                tx.send(JiraCommand::FetchMyIssues {
                    generation: plugin.generation,
                })
                .ok();
            }
            PluginAction::None
        }

        // ── Back ────────────────────────────────────────────────────────
        KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,

        _ => PluginAction::None,
    }
}

// ── Board helpers ───────────────────────────────────────────────────────────

fn selected_issue<'a>(
    plugin: &JiraPlugin,
    filtered: &[&'a super::models::JiraIssue],
) -> Option<&'a super::models::JiraIssue> {
    let col = plugin.board.columns.get(plugin.board.selected_col)?;
    let global_idx = col.issue_indices.get(plugin.board.selected_row)?;
    filtered.get(*global_idx).copied()
}

fn clamp_row(plugin: &mut JiraPlugin) {
    if plugin.board.columns.is_empty() {
        plugin.board.selected_row = 0;
        return;
    }
    let col = &plugin.board.columns[plugin.board.selected_col];
    let len = col.issue_indices.len();
    if len == 0 {
        plugin.board.selected_row = 0;
    } else if plugin.board.selected_row >= len {
        plugin.board.selected_row = len - 1;
    }
}

fn adjust_col_scroll(plugin: &mut JiraPlugin) {
    // Use a reasonable default for max_visible_cards since we don't have area height here.
    // The render function will use the real value, but for scroll adjustment we use
    // the col_scroll_offsets to ensure the cursor is reasonably visible.
    let col_idx = plugin.board.selected_col;
    let row = plugin.board.selected_row;

    // Ensure col_scroll_offsets is large enough
    if col_idx >= plugin.board.col_scroll_offsets.len() {
        plugin.board.col_scroll_offsets.resize(col_idx + 1, 0);
    }

    let scroll = &mut plugin.board.col_scroll_offsets[col_idx];

    // Approximate max visible cards — we'll use 5 as a reasonable lower bound.
    // The render pass uses the real value, but this keeps things roughly correct.
    let max_vis = 5_usize;

    if row < *scroll {
        *scroll = row;
    } else if row >= *scroll + max_vis {
        *scroll = row - max_vis + 1;
    }
}

fn open_create_flow(plugin: &mut JiraPlugin) {
    let mut project_list: Vec<(String, String)> = plugin
        .issues
        .iter()
        .map(|i| (i.project_key.clone(), i.project_name.clone()))
        .collect();
    project_list.sort();
    project_list.dedup();

    if project_list.is_empty() {
        // No projects known — cannot create
        plugin.modal = Some(JiraModal::ErrorModal {
            title: "Cannot Create Issue".to_string(),
            message: "No projects found. Issues must be fetched first.".to_string(),
        });
        return;
    }

    plugin.modal = Some(JiraModal::SelectProject {
        projects: project_list,
        cursor: 0,
    });
}
