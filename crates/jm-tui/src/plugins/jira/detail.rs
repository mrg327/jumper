//! Issue detail modal rendering for the JIRA plugin.
//!
//! Renders the issue detail modal overlay, transition picker, and transition
//! fields form. All modals are plugin-owned overlays, not part of the App's
//! modal system.

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::plugins::PluginAction;
use crate::theme;

use super::api::JiraCommand;
use super::models::*;
use super::{DetailEditState, DetailFocus, FieldValue, FormState, JiraModal, JiraPlugin};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn centered_rect(pct_width: u16, height: u16, area: Rect) -> Rect {
    let width = (area.width * pct_width / 100).min(area.width);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height.min(area.height))
}

fn centered_rect_abs(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 3 {
        s.chars().take(max).collect()
    } else {
        let t: String = s.chars().take(max - 3).collect();
        format!("{t}...")
    }
}

/// Convert a character index to a byte index in a string.
/// Returns the byte position of the nth character, or the string's byte length
/// if n >= char count.
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

// ── Detail modal rendering ──────────────────────────────────────────────────

/// Render the issue detail modal overlay.
pub(crate) fn render_detail(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::IssueDetail {
        issue_key,
        fields,
        comments,
        scroll_offset,
        field_cursor,
        focus,
        edit_state,
        ..
    }) = &plugin.modal
    else {
        return;
    };

    let issue = plugin.issues.iter().find(|i| i.key == *issue_key);

    let modal_height = area.height.saturating_sub(4);
    let modal_area = centered_rect(70, modal_height, area);
    frame.render_widget(Clear, modal_area);

    let title = if let Some(iss) = issue {
        format!(" {}: {} ", issue_key, truncate_str(&iss.summary, 50))
    } else {
        format!(" {} ", issue_key)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let content_height = inner.height.saturating_sub(1) as usize;
    let footer_y = inner.y + inner.height - 1;

    // Build content rows
    let mut rows: Vec<DetailRow> = Vec::new();
    if let Some(iss) = issue {
        add_field_row(&mut rows, "Status", &iss.status.name, false, false);
        add_field_row(&mut rows, "Priority", &iss.priority.clone().unwrap_or_default(), is_field_editable(fields, "priority"), false);
        add_field_row(&mut rows, "Assignee", &iss.assignee.clone().unwrap_or_else(|| "Unassigned".into()), false, true);
        add_field_row(&mut rows, "Reporter", &iss.reporter.clone().unwrap_or_default(), false, true);
        add_field_row(&mut rows, "Type", &iss.issue_type, false, true);
        if let Some(pts) = iss.story_points {
            let s = if pts.fract() == 0.0 { format!("{}", pts as i64) } else { format!("{pts}") };
            add_field_row(&mut rows, "Points", &s, is_field_editable(fields, "story_points"), false);
        }
        if let Some(sprint) = &iss.sprint {
            add_field_row(&mut rows, "Sprint", sprint, false, true);
        }
        if let Some(epic) = &iss.epic {
            add_field_row(&mut rows, "Epic", &format!("{} ({})", epic.name, epic.key), false, true);
        }
        if !iss.labels.is_empty() {
            add_field_row(&mut rows, "Labels", &iss.labels.join(", "), is_field_editable(fields, "labels"), false);
        }
        if !iss.components.is_empty() {
            add_field_row(&mut rows, "Components", &iss.components.join(", "), is_field_editable(fields, "components"), false);
        }
        add_field_row(&mut rows, "Created", &iss.created, false, true);
        add_field_row(&mut rows, "Updated", &iss.updated, false, true);
        if let Some(desc) = &iss.description {
            rows.push(DetailRow::Separator("Description".into()));
            for line in desc.lines() { rows.push(DetailRow::Text(line.into())); }
        }
    }
    if fields.is_none() {
        rows.push(DetailRow::Text(String::new()));
        rows.push(DetailRow::Text("Loading field metadata...".into()));
    }
    let cc = comments.as_ref().map(|c| c.len()).unwrap_or(0);
    rows.push(DetailRow::Separator(format!("Comments ({cc})")));
    if let Some(coms) = comments {
        for c in coms {
            rows.push(DetailRow::Comment { author: c.author.clone(), created: c.created.clone(), body: c.body.clone() });
        }
    } else {
        rows.push(DetailRow::Text("Loading comments...".into()));
    }

    // Render
    let mut y = inner.y;
    let mut rendered = 0usize;
    let mut fidx = 0usize;
    let label_w: u16 = 14;

    for row in &rows {
        if rendered >= *scroll_offset + content_height { break; }
        let rl = match row { DetailRow::Comment { body, .. } => 2 + body.lines().count().max(1), _ => 1 };
        if rendered + rl <= *scroll_offset { rendered += rl; if matches!(row, DetailRow::Field { .. }) { fidx += 1; } continue; }
        if y >= inner.y + content_height as u16 { break; }

        match row {
            DetailRow::Field { label, value, editable, read_only } => {
                let is_sel = *focus == DetailFocus::Fields && fidx == *field_cursor;
                let rs = if is_sel { theme::selected() } else { Style::default() };
                let ls = format!("{:>w$}:  ", label, w = label_w as usize);

                // Inline text edit
                if let Some(DetailEditState::EditingText { buffer, cursor_pos, .. }) = edit_state {
                    if is_sel {
                        let line = Line::from(vec![Span::styled(&ls, theme::dim()), Span::styled(buffer.clone(), Style::default().bg(theme::SELECTED_BG))]);
                        frame.render_widget(Paragraph::new(line), Rect::new(inner.x, y, inner.width, 1));
                        frame.set_cursor_position(Position::new(inner.x + label_w + 3 + *cursor_pos as u16, y));
                        y += 1; rendered += 1; fidx += 1; continue;
                    }
                }
                // Select dropdown
                if let Some(DetailEditState::SelectOpen { options, cursor, .. }) = edit_state {
                    if is_sel {
                        let spans = vec![Span::styled(&ls, theme::dim()), Span::styled(value.clone(), rs)];
                        let line = Line::from(spans);
                        frame.render_widget(Paragraph::new(line), Rect::new(inner.x, y, inner.width, 1));
                        y += 1; rendered += 1;
                        let dh = options.len().min(8).min(content_height.saturating_sub(rendered)) as u16;
                        let dx = inner.x + label_w + 3;
                        let dw = inner.width.saturating_sub(label_w + 3);
                        if dh > 0 {
                            let da = Rect::new(dx, y, dw, dh);
                            frame.render_widget(Clear, da);
                            for (oi, opt) in options.iter().enumerate() {
                                if oi >= dh as usize { break; }
                                let os = if oi == *cursor { theme::selected() } else { Style::default() };
                                let p = if oi == *cursor { "> " } else { "  " };
                                frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("{p}{}", opt.name), os))), Rect::new(dx, y + oi as u16, dw, 1));
                            }
                            y += dh; rendered += dh as usize;
                        }
                        fidx += 1; continue;
                    }
                }

                let mut spans = vec![Span::styled(ls, theme::dim()), Span::styled(value.clone(), rs)];
                if is_sel {
                    if *editable { spans.push(Span::styled("  [e:edit]", Style::default().fg(theme::TEXT_ACCENT))); }
                    else if *read_only { spans.push(Span::styled("  (read-only)", theme::dim())); }
                }
                frame.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, y, inner.width, 1));
                y += 1; rendered += 1; fidx += 1;
            }
            DetailRow::Separator(lbl) => {
                let sl = Line::from(vec![
                    Span::styled("\u{2500}\u{2500}\u{2500} ", theme::dim()),
                    Span::styled(lbl.clone(), theme::bold()),
                    Span::styled(" \u{2500}".repeat((inner.width as usize).saturating_sub(lbl.len() + 5)), theme::dim()),
                ]);
                frame.render_widget(Paragraph::new(sl), Rect::new(inner.x, y, inner.width, 1));
                y += 1; rendered += 1;
            }
            DetailRow::Text(t) => {
                let st = if t.contains("Loading") { theme::dim() } else { Style::default() };
                frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("  {t}"), st))), Rect::new(inner.x, y, inner.width, 1));
                y += 1; rendered += 1;
            }
            DetailRow::Comment { author, created, body } => {
                if y < inner.y + content_height as u16 {
                    let hl = Line::from(vec![Span::styled(format!("  {author}"), theme::accent()), Span::styled(format!(" \u{00b7} {created}"), theme::dim())]);
                    frame.render_widget(Paragraph::new(hl), Rect::new(inner.x, y, inner.width, 1));
                    y += 1;
                }
                let bw = inner.width.saturating_sub(4) as usize;
                for bl in body.lines() {
                    if y >= inner.y + content_height as u16 { break; }
                    frame.render_widget(Paragraph::new(Line::from(Span::raw(format!("    {}", truncate_str(bl, bw))))), Rect::new(inner.x, y, inner.width, 1));
                    y += 1;
                }
                if y < inner.y + content_height as u16 { y += 1; }
                rendered = (y - inner.y) as usize;
            }
        }
    }

    let fl = Line::from(vec![
        Span::styled(" j/k", theme::accent()), Span::styled(":navigate  ", theme::dim()),
        Span::styled("e", theme::accent()), Span::styled(":edit  ", theme::dim()),
        Span::styled("s", theme::accent()), Span::styled(":transition  ", theme::dim()),
        Span::styled("c", theme::accent()), Span::styled(":comment  ", theme::dim()),
        Span::styled("Tab", theme::accent()), Span::styled(":section  ", theme::dim()),
        Span::styled("Esc", theme::accent()), Span::styled(":close", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(fl), Rect::new(inner.x, footer_y, inner.width, 1));
}

enum DetailRow { Field { label: String, value: String, editable: bool, read_only: bool }, Separator(String), Text(String), Comment { author: String, created: String, body: String } }

fn add_field_row(rows: &mut Vec<DetailRow>, label: &str, value: &str, editable: bool, read_only: bool) {
    rows.push(DetailRow::Field { label: label.into(), value: value.into(), editable, read_only: if editable { false } else { read_only } });
}

fn is_field_editable(fields: &Option<Vec<EditableField>>, hint: &str) -> bool {
    fields.as_ref().map_or(false, |fs| fs.iter().any(|f| f.field_id == hint || f.name.to_lowercase().contains(hint)))
}

// ── Transition picker rendering ─────────────────────────────────────────────

pub(crate) fn render_transition_picker(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::TransitionPicker { issue_key, transitions, cursor }) = &plugin.modal else { return; };
    let current_status = plugin.issues.iter().find(|i| i.key == *issue_key).map(|i| i.status.name.clone()).unwrap_or_else(|| "Unknown".into());
    let ch = transitions.len() + 4;
    let mh = (ch as u16 + 4).min(area.height.saturating_sub(4));
    let mw = 40u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);
    let block = Block::default().title(format!(" Transition {} ", issue_key)).borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);
    if inner.height < 3 { return; }
    let mut y = inner.y;
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled("  Current: ", theme::dim()), Span::styled(&current_status, theme::accent())])), Rect::new(inner.x, y, inner.width, 1));
    y += 2;
    if transitions.is_empty() {
        frame.render_widget(Paragraph::new("  Loading transitions...").style(theme::dim()), Rect::new(inner.x, y, inner.width, 1));
    } else {
        for (idx, tr) in transitions.iter().enumerate() {
            if y >= inner.y + inner.height - 1 { break; }
            let sel = idx == *cursor;
            let st = if sel { theme::selected() } else { Style::default() };
            let p = if sel { "> " } else { "  " };
            let l = Line::from(vec![Span::styled(p, st), Span::styled(&tr.name, st), Span::styled(format!(" \u{2192} {}", tr.to_status.name), if sel { theme::dim().bg(theme::SELECTED_BG) } else { theme::dim() })]);
            frame.render_widget(Paragraph::new(l), Rect::new(inner.x, y, inner.width, 1));
            y += 1;
        }
    }
    let fy = inner.y + inner.height - 1;
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled("  Enter", theme::accent()), Span::styled(": apply  ", theme::dim()), Span::styled("Esc", theme::accent()), Span::styled(": back", theme::dim())])), Rect::new(inner.x, fy, inner.width, 1));
}

// ── Transition fields form rendering ────────────────────────────────────────

pub(crate) fn render_transition_fields(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::TransitionFields { issue_key, transition, fields, form }) = &plugin.modal else { return; };
    let fc = fields.len();
    let mh = (fc as u16 + 6).min(area.height.saturating_sub(4));
    let mw = 60u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);
    let title = format!(" Transition {} \u{2192} {} ", issue_key, transition.to_status.name);
    let block = Block::default().title(title).borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);
    if inner.height < 2 { return; }
    render_form_fields(frame, inner, fields, form);
    let fy = inner.y + inner.height - 1;
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled("  Enter", theme::accent()), Span::styled(": apply  ", theme::dim()), Span::styled("Esc", theme::accent()), Span::styled(": cancel", theme::dim())])), Rect::new(inner.x, fy, inner.width, 1));
}

/// Shared form field rendering used by both transition fields and create form.
pub(super) fn render_form_fields(frame: &mut Frame, inner: Rect, fields: &[(EditableField, Option<FieldValue>)], form: &FormState) {
    let ch = inner.height.saturating_sub(1) as usize;
    let lw = fields.iter().map(|(f, _)| f.name.len()).max().unwrap_or(8).min(20);
    let fc = match form {
        FormState::Navigating { cursor, .. } | FormState::EditingText { cursor, .. } => *cursor,
        FormState::SelectOpen { field_cursor, .. } | FormState::MultiSelectOpen { field_cursor, .. } => *field_cursor,
        FormState::ValidationError { cursor, .. } => *cursor,
        FormState::Submitting => usize::MAX,
    };
    let errs = if let FormState::ValidationError { errors, .. } = form { Some(errors) } else { None };

    for (idx, (field, value)) in fields.iter().enumerate() {
        if idx >= ch { break; }
        let y = inner.y + idx as u16;
        let is_sel = idx == fc;
        let rs = if is_sel { theme::selected() } else { Style::default() };
        let has_err = errs.and_then(|e| e.get(&field.field_id)).is_some();
        let prefix = if has_err { Span::styled("!", Style::default().fg(theme::TEXT_ERROR)) }
            else if field.field_type == FieldType::Unsupported { Span::styled("~", theme::dim()) }
            else if field.required { if value.is_some() { Span::styled("*", theme::accent()) } else { Span::styled("*", Style::default().fg(theme::TEXT_ERROR)) } }
            else { Span::raw(" ") };
        let ls = format!("{:>w$}:  ", field.name, w = lw);
        let vd = match (&field.field_type, value) {
            (FieldType::Unsupported, _) => "(unsupported type)".into(),
            (_, Some(FieldValue::Text(s))) => s.clone(),
            (_, Some(FieldValue::Number(n))) => if n.fract() == 0.0 { format!("{}", *n as i64) } else { format!("{n}") },
            (_, Some(FieldValue::Select(id))) => field.allowed_values.as_ref().and_then(|v| v.iter().find(|a| a.id == *id).map(|a| a.name.clone())).unwrap_or_else(|| id.clone()),
            (_, Some(FieldValue::MultiSelect(ids))) => if ids.is_empty() { "[empty]".into() } else { field.allowed_values.as_ref().map(|v| ids.iter().filter_map(|id| v.iter().find(|a| a.id == *id).map(|a| a.name.as_str())).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| ids.join(", ")) },
            (_, Some(FieldValue::Date(d))) => d.clone(),
            (_, None) => "[empty]".into(),
        };

        if let FormState::EditingText { cursor: ec, buffer, cursor_pos } = form {
            if idx == *ec {
                let spans = vec![prefix.clone(), Span::styled(&ls, theme::dim()), Span::styled(buffer.clone(), Style::default().bg(theme::SELECTED_BG))];
                frame.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, y, inner.width, 1));
                frame.set_cursor_position(Position::new(inner.x + 1 + lw as u16 + 3 + *cursor_pos as u16, y));
                continue;
            }
        }

        let vs = if vd == "[empty]" { if is_sel { theme::empty_hint().bg(theme::SELECTED_BG) } else { theme::empty_hint() } }
            else if field.field_type == FieldType::Unsupported { theme::dim() }
            else { rs };
        let mut spans = vec![prefix, Span::styled(ls, theme::dim()), Span::styled(vd, vs)];
        if matches!(field.field_type, FieldType::Select | FieldType::MultiSelect) {
            spans.push(Span::styled(" [\u{25bc}]", if is_sel { theme::accent() } else { theme::dim() }));
        }
        if let Some(em) = errs.and_then(|e| e.get(&field.field_id)) {
            spans.push(Span::styled(format!(" \u{2500} {em}"), Style::default().fg(theme::TEXT_ERROR)));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, y, inner.width, 1));
    }

    // Select dropdown overlay
    if let FormState::SelectOpen { field_cursor: fci, dropdown_cursor: dci } = form {
        if let Some((field, _)) = fields.get(*fci) {
            if let Some(allowed) = &field.allowed_values {
                let dy = inner.y + *fci as u16 + 1;
                let dh = allowed.len().min(8).min((inner.y + inner.height).saturating_sub(dy) as usize) as u16;
                let dx = inner.x + lw as u16 + 4;
                let dw = inner.width.saturating_sub(lw as u16 + 4);
                if dh > 0 {
                    frame.render_widget(Clear, Rect::new(dx, dy, dw, dh));
                    for (oi, opt) in allowed.iter().enumerate() {
                        if oi >= dh as usize { break; }
                        let os = if oi == *dci { theme::selected() } else { Style::default() };
                        let p = if oi == *dci { ">" } else { " " };
                        frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("{p} {}", opt.name), os))), Rect::new(dx, dy + oi as u16, dw, 1));
                    }
                }
            }
        }
    }

    // Multi-select dropdown overlay
    if let FormState::MultiSelectOpen { field_cursor: fci, dropdown_cursor: dci, checked } = form {
        if let Some((field, _)) = fields.get(*fci) {
            if let Some(allowed) = &field.allowed_values {
                let dy = inner.y + *fci as u16 + 1;
                let dh = allowed.len().min(8).min((inner.y + inner.height).saturating_sub(dy) as usize) as u16;
                let dx = inner.x + lw as u16 + 4;
                let dw = inner.width.saturating_sub(lw as u16 + 4);
                if dh > 0 {
                    frame.render_widget(Clear, Rect::new(dx, dy, dw, dh));
                    for (oi, opt) in allowed.iter().enumerate() {
                        if oi >= dh as usize { break; }
                        let ic = checked.contains(&oi);
                        let icu = oi == *dci;
                        let os = if icu { theme::selected() } else { Style::default() };
                        let cb = if ic { "[x]" } else { "[ ]" };
                        let cs = if ic { if icu { theme::accent().bg(theme::SELECTED_BG) } else { theme::accent() } } else { os };
                        frame.render_widget(Paragraph::new(Line::from(vec![Span::styled(format!(" {cb} "), cs), Span::styled(&opt.name, os)])), Rect::new(dx, dy + oi as u16, dw, 1));
                    }
                }
            }
        }
    }

    if matches!(form, FormState::Submitting) {
        let sy = inner.y + fields.len().min(ch) as u16;
        if sy < inner.y + inner.height {
            frame.render_widget(Paragraph::new("  Submitting...").style(theme::dim()), Rect::new(inner.x, sy, inner.width, 1));
        }
    }
}

// ── Key handlers ────────────────────────────────────────────────────────────

pub(crate) fn handle_detail_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    if matches!(&plugin.modal, Some(JiraModal::IssueDetail { edit_state: Some(_), .. })) {
        return handle_detail_edit_key(key, plugin);
    }

    let (issue_key, field_count) = {
        let Some(JiraModal::IssueDetail { issue_key, .. }) = &plugin.modal else { return PluginAction::None; };
        let ik = issue_key.clone();
        let fc = count_detail_fields_by_key(plugin, &ik);
        (ik, fc)
    };

    // Handle keys that replace/take the modal
    match key.code {
        KeyCode::Char('e') => {
            if matches!(&plugin.modal, Some(JiraModal::IssueDetail { focus: DetailFocus::Fields, .. })) {
                start_detail_field_edit(plugin);
            }
            return PluginAction::None;
        }
        KeyCode::Char('s') => {
            let cur = plugin.modal.take();
            plugin.previous_modal = cur;
            plugin.modal = Some(JiraModal::TransitionPicker { issue_key: issue_key.clone(), transitions: Vec::new(), cursor: 0 });
            if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::FetchTransitions { issue_key }).ok(); }
            return PluginAction::None;
        }
        KeyCode::Char('c') => return PluginAction::LaunchEditor { content: String::new(), context: format!("comment:{}", issue_key) },
        KeyCode::Esc => { plugin.modal = None; return PluginAction::None; }
        _ => {}
    }

    // Navigation
    let Some(JiraModal::IssueDetail { scroll_offset, field_cursor, focus, .. }) = &mut plugin.modal else { return PluginAction::None; };
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => match focus {
            DetailFocus::Fields => { if *field_cursor + 1 < field_count { *field_cursor += 1; } else { *focus = DetailFocus::Comments; } }
            DetailFocus::Comments => { *scroll_offset += 1; }
        },
        KeyCode::Char('k') | KeyCode::Up => match focus {
            DetailFocus::Fields => { if *field_cursor > 0 { *field_cursor -= 1; } }
            DetailFocus::Comments => { if *scroll_offset > 0 { *scroll_offset -= 1; } else { *focus = DetailFocus::Fields; *field_cursor = field_count.saturating_sub(1); } }
        },
        KeyCode::Tab => { *focus = match *focus { DetailFocus::Fields => DetailFocus::Comments, DetailFocus::Comments => DetailFocus::Fields }; }
        _ => {}
    }
    PluginAction::None
}

fn count_detail_fields_by_key(plugin: &JiraPlugin, issue_key: &str) -> usize {
    let Some(iss) = plugin.issues.iter().find(|i| i.key == issue_key) else { return 0; };
    let mut c = 5; // Status, Priority, Assignee, Reporter, Type
    if iss.story_points.is_some() { c += 1; }
    if iss.sprint.is_some() { c += 1; }
    if iss.epic.is_some() { c += 1; }
    if !iss.labels.is_empty() { c += 1; }
    if !iss.components.is_empty() { c += 1; }
    c + 2 // Created, Updated
}

fn start_detail_field_edit(plugin: &mut JiraPlugin) {
    let (issue_key, field_cursor, editable_fields) = {
        let Some(JiraModal::IssueDetail { issue_key, fields, field_cursor, .. }) = &plugin.modal else { return; };
        let Some(ef) = fields else { return; };
        (issue_key.clone(), *field_cursor, ef.clone())
    };
    let Some(iss) = plugin.issues.iter().find(|i| i.key == issue_key) else { return; };
    let field_names = build_field_names(iss);
    let Some(cursor_name) = field_names.get(field_cursor) else { return; };
    let Some(editable) = editable_fields.iter().find(|f| f.name.to_lowercase() == cursor_name.to_lowercase() || f.field_id == cursor_name.to_lowercase()) else { return; };

    let new_es = match editable.field_type {
        FieldType::Text | FieldType::Number | FieldType::Date => {
            let cv = get_current_field_value(iss, cursor_name);
            Some(DetailEditState::EditingText { field_id: editable.field_id.clone(), buffer: cv.clone(), cursor_pos: cv.chars().count() })
        }
        FieldType::Select => editable.allowed_values.as_ref().map(|allowed| {
            let cv = get_current_field_value(iss, cursor_name);
            let cp = allowed.iter().position(|v| v.name == cv).unwrap_or(0);
            DetailEditState::SelectOpen { field_id: editable.field_id.clone(), options: allowed.clone(), cursor: cp }
        }),
        _ => None,
    };
    if let Some(JiraModal::IssueDetail { edit_state, .. }) = &mut plugin.modal { *edit_state = new_es; }
}

fn build_field_names(iss: &JiraIssue) -> Vec<String> {
    let mut n = vec!["Status".into(), "Priority".into(), "Assignee".into(), "Reporter".into(), "Type".into()];
    if iss.story_points.is_some() { n.push("Points".into()); }
    if iss.sprint.is_some() { n.push("Sprint".into()); }
    if iss.epic.is_some() { n.push("Epic".into()); }
    if !iss.labels.is_empty() { n.push("Labels".into()); }
    if !iss.components.is_empty() { n.push("Components".into()); }
    n.push("Created".into()); n.push("Updated".into());
    n
}

fn get_current_field_value(iss: &JiraIssue, name: &str) -> String {
    match name {
        "Status" => iss.status.name.clone(), "Priority" => iss.priority.clone().unwrap_or_default(),
        "Assignee" => iss.assignee.clone().unwrap_or_default(), "Reporter" => iss.reporter.clone().unwrap_or_default(),
        "Type" => iss.issue_type.clone(),
        "Points" => iss.story_points.map(|p| if p.fract() == 0.0 { format!("{}", p as i64) } else { format!("{p}") }).unwrap_or_default(),
        "Sprint" => iss.sprint.clone().unwrap_or_default(), "Labels" => iss.labels.join(", "),
        "Components" => iss.components.join(", "), "Created" => iss.created.clone(), "Updated" => iss.updated.clone(),
        _ => String::new(),
    }
}

fn handle_detail_edit_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let (edit_type, field_id, issue_key) = {
        let Some(JiraModal::IssueDetail { issue_key, edit_state: Some(es), .. }) = &plugin.modal else { return PluginAction::None; };
        match es {
            DetailEditState::EditingText { field_id, .. } => ("text", field_id.clone(), issue_key.clone()),
            DetailEditState::SelectOpen { field_id, .. } => ("select", field_id.clone(), issue_key.clone()),
        }
    };

    if edit_type == "text" {
        let Some(JiraModal::IssueDetail { edit_state: Some(DetailEditState::EditingText { buffer, cursor_pos, .. }), .. }) = &mut plugin.modal else { return PluginAction::None; };
        match key.code {
            KeyCode::Char(c) => { buffer.insert(char_to_byte_idx(buffer, *cursor_pos), c); *cursor_pos += 1; }
            KeyCode::Backspace => { if *cursor_pos > 0 { buffer.remove(char_to_byte_idx(buffer, *cursor_pos - 1)); *cursor_pos -= 1; } }
            KeyCode::Delete => { if *cursor_pos < buffer.chars().count() { buffer.remove(char_to_byte_idx(buffer, *cursor_pos)); } }
            KeyCode::Left => { if *cursor_pos > 0 { *cursor_pos -= 1; } }
            KeyCode::Right => { if *cursor_pos < buffer.chars().count() { *cursor_pos += 1; } }
            KeyCode::Home => *cursor_pos = 0,
            KeyCode::End => *cursor_pos = buffer.chars().count(),
            KeyCode::Enter => {
                let v = buffer.clone();
                plugin.refreshing = true;
                if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::UpdateField { issue_key, field_id, value: serde_json::json!(v) }).ok(); }
                if let Some(JiraModal::IssueDetail { edit_state, .. }) = &mut plugin.modal { *edit_state = None; }
            }
            KeyCode::Esc => { if let Some(JiraModal::IssueDetail { edit_state, .. }) = &mut plugin.modal { *edit_state = None; } }
            _ => {}
        }
    } else {
        let Some(JiraModal::IssueDetail { edit_state: Some(DetailEditState::SelectOpen { options, cursor, .. }), .. }) = &mut plugin.modal else { return PluginAction::None; };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => { if *cursor + 1 < options.len() { *cursor += 1; } }
            KeyCode::Char('k') | KeyCode::Up => { if *cursor > 0 { *cursor -= 1; } }
            KeyCode::Enter => {
                let sid = options.get(*cursor).map(|v| v.id.clone());
                if let Some(id) = sid {
                    plugin.refreshing = true;
                    if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::UpdateField { issue_key, field_id, value: serde_json::json!({ "id": id }) }).ok(); }
                }
                if let Some(JiraModal::IssueDetail { edit_state, .. }) = &mut plugin.modal { *edit_state = None; }
            }
            KeyCode::Esc => { if let Some(JiraModal::IssueDetail { edit_state, .. }) = &mut plugin.modal { *edit_state = None; } }
            _ => {}
        }
    }
    PluginAction::None
}

// ── Transition picker key handler ───────────────────────────────────────────

pub(crate) fn handle_transition_picker_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let issue_key = { let Some(JiraModal::TransitionPicker { issue_key, .. }) = &plugin.modal else { return PluginAction::None; }; issue_key.clone() };

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(JiraModal::TransitionPicker { cursor, transitions, .. }) = &mut plugin.modal {
                if !transitions.is_empty() && *cursor + 1 < transitions.len() { *cursor += 1; }
            }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(JiraModal::TransitionPicker { cursor, .. }) = &mut plugin.modal { if *cursor > 0 { *cursor -= 1; } }
            PluginAction::None
        }
        KeyCode::Enter => {
            let selected = { let Some(JiraModal::TransitionPicker { transitions, cursor, .. }) = &plugin.modal else { return PluginAction::None; }; transitions.get(*cursor).cloned() };
            let Some(sel) = selected else { return PluginAction::None; };
            if !sel.required_fields.is_empty() {
                let has_comment = sel.required_fields.iter().any(|f| f.is_comment);
                let non_comment_fields: Vec<&TransitionField> = sel.required_fields.iter().filter(|f| !f.is_comment).collect();
                if has_comment && non_comment_fields.is_empty() {
                    // Only required field is a comment — launch $EDITOR
                    return PluginAction::LaunchEditor {
                        content: String::new(),
                        context: format!("transition_comment:{}:{}", issue_key, sel.id),
                    };
                }
                // Build form fields, filtering out comment fields
                let ff: Vec<(EditableField, Option<FieldValue>)> = sel.required_fields.iter().filter(|tf| !tf.is_comment).map(|tf| {
                    (EditableField { field_id: tf.field_id.clone(), name: tf.name.clone(), field_type: tf.field_type.clone(), required: true, allowed_values: if tf.allowed_values.is_empty() { None } else { Some(tf.allowed_values.clone()) } }, None)
                }).collect();
                let cur = plugin.modal.take();
                plugin.previous_modal = cur;
                plugin.modal = Some(JiraModal::TransitionFields { issue_key: issue_key.clone(), transition: sel, fields: ff, form: FormState::Navigating { cursor: 0, scroll_offset: 0 } });
            } else {
                // Save original status for revert
                if let Some(issue) = plugin.issues.iter().find(|i| i.key == issue_key) {
                    plugin.pending_transitions.insert(issue_key.clone(), issue.status.clone());
                }
                // Optimistic UI: move issue to target column
                if let Some(issue) = plugin.issues.iter_mut().find(|i| i.key == issue_key) {
                    issue.status = sel.to_status.clone();
                }
                plugin.rebuild_columns();
                plugin.refreshing = true;
                if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::TransitionIssue { issue_key: issue_key.clone(), transition_id: sel.id.clone(), fields: None }).ok(); }
                plugin.modal = plugin.previous_modal.take();
                return PluginAction::Toast(format!("Transitioning {} \u{2192} {}", issue_key, sel.to_status.name));
            }
            PluginAction::None
        }
        KeyCode::Esc => { plugin.modal = plugin.previous_modal.take(); PluginAction::None }
        _ => PluginAction::None,
    }
}

// ── Transition fields key handler ───────────────────────────────────────────

pub(crate) fn handle_transition_fields_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let (issue_key, transition_id, to_status, to_status_full, field_count, form_kind) = {
        let Some(JiraModal::TransitionFields { issue_key, transition, fields, form }) = &plugin.modal else { return PluginAction::None; };
        let fk = match form {
            FormState::Navigating { .. } | FormState::ValidationError { .. } => "nav",
            FormState::EditingText { .. } => "edit",
            FormState::SelectOpen { .. } => "select",
            FormState::MultiSelectOpen { .. } => "multi",
            FormState::Submitting => "submit",
        };
        (issue_key.clone(), transition.id.clone(), transition.to_status.name.clone(), transition.to_status.clone(), fields.len(), fk)
    };

    match form_kind {
        "nav" => handle_tf_nav(key, plugin, &issue_key, &transition_id, &to_status, &to_status_full, field_count),
        "edit" => handle_generic_form_edit(key, plugin, field_count),
        "select" => handle_generic_form_select(key, plugin, field_count),
        "multi" => handle_generic_form_multi(key, plugin, field_count),
        _ => PluginAction::None,
    }
}

fn handle_tf_nav(key: KeyEvent, plugin: &mut JiraPlugin, issue_key: &str, transition_id: &str, to_status: &str, to_status_full: &JiraStatus, field_count: usize) -> PluginAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(JiraModal::TransitionFields { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { if *cursor + 1 < field_count { *cursor += 1; } } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(JiraModal::TransitionFields { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { if *cursor > 0 { *cursor -= 1; } } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Enter => {
            let cur = { let Some(JiraModal::TransitionFields { form, .. }) = &plugin.modal else { return PluginAction::None; }; match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => *cursor, _ => return PluginAction::None } };
            if let Some(JiraModal::TransitionFields { fields, form, .. }) = &mut plugin.modal {
                if let Some(action) = enter_form_field_edit(fields, form, cur, issue_key) { return action; }
            }
            PluginAction::None
        }
        KeyCode::Char('S') | KeyCode::Char('s') => {
            let mi = { let Some(JiraModal::TransitionFields { fields, .. }) = &plugin.modal else { return PluginAction::None; }; fields.iter().enumerate().find(|(_, (f, v))| f.required && v.is_none()).map(|(i, _)| i) };
            if let Some(idx) = mi {
                if let Some(JiraModal::TransitionFields { form, .. }) = &mut plugin.modal { match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => *cursor = idx, _ => {} } }
                return PluginAction::None;
            }
            let fj = { let Some(JiraModal::TransitionFields { fields, .. }) = &plugin.modal else { return PluginAction::None; }; build_field_json(fields) };
            // Save original status for revert
            if let Some(issue) = plugin.issues.iter().find(|i| i.key == issue_key) {
                plugin.pending_transitions.insert(issue_key.to_string(), issue.status.clone());
            }
            // Optimistic UI: move issue to target column
            if let Some(issue) = plugin.issues.iter_mut().find(|i| i.key == issue_key) {
                issue.status = to_status_full.clone();
            }
            plugin.rebuild_columns();
            plugin.refreshing = true;
            if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::TransitionIssue { issue_key: issue_key.into(), transition_id: transition_id.into(), fields: Some(fj) }).ok(); }
            plugin.modal = None; plugin.previous_modal = None;
            PluginAction::Toast(format!("Transitioning {} \u{2192} {}", issue_key, to_status))
        }
        KeyCode::Esc => { plugin.modal = plugin.previous_modal.take(); PluginAction::None }
        _ => PluginAction::None,
    }
}

// Generic form key handlers shared between transition fields and create form

pub(super) fn handle_generic_form_edit(key: KeyEvent, plugin: &mut JiraPlugin, field_count: usize) -> PluginAction {
    let cur = {
        let f = match &plugin.modal {
            Some(JiraModal::TransitionFields { form, .. }) | Some(JiraModal::CreateForm { form, .. }) => Some(form),
            _ => None,
        };
        match f { Some(FormState::EditingText { cursor, .. }) => *cursor, _ => return PluginAction::None }
    };
    let (form, fields) = match &mut plugin.modal {
        Some(JiraModal::TransitionFields { form, fields, .. }) | Some(JiraModal::CreateForm { form, fields, .. }) => (form, fields),
        _ => return PluginAction::None,
    };
    let FormState::EditingText { buffer, cursor_pos, .. } = form else { return PluginAction::None; };
    match key.code {
        KeyCode::Char(c) => { buffer.insert(char_to_byte_idx(buffer, *cursor_pos), c); *cursor_pos += 1; }
        KeyCode::Backspace => { if *cursor_pos > 0 { buffer.remove(char_to_byte_idx(buffer, *cursor_pos - 1)); *cursor_pos -= 1; } }
        KeyCode::Delete => { if *cursor_pos < buffer.chars().count() { buffer.remove(char_to_byte_idx(buffer, *cursor_pos)); } }
        KeyCode::Left => { if *cursor_pos > 0 { *cursor_pos -= 1; } }
        KeyCode::Right => { if *cursor_pos < buffer.chars().count() { *cursor_pos += 1; } }
        KeyCode::Home => *cursor_pos = 0,
        KeyCode::End => *cursor_pos = buffer.chars().count(),
        KeyCode::Enter => {
            let val = buffer.clone();
            let ft = &fields[cur].0.field_type;
            let fv = match ft {
                FieldType::Number => match val.parse::<f64>() { Ok(n) => Some(FieldValue::Number(n)), Err(_) => return PluginAction::None },
                FieldType::Date => Some(FieldValue::Date(val)),
                _ => if val.is_empty() { None } else { Some(FieldValue::Text(val)) },
            };
            fields[cur].1 = fv;
            *form = FormState::Navigating { cursor: (cur + 1).min(field_count.saturating_sub(1)), scroll_offset: 0 };
        }
        KeyCode::Esc => { *form = FormState::Navigating { cursor: cur, scroll_offset: 0 }; }
        _ => {}
    }
    PluginAction::None
}

pub(super) fn handle_generic_form_select(key: KeyEvent, plugin: &mut JiraPlugin, field_count: usize) -> PluginAction {
    let (form, fields) = match &mut plugin.modal {
        Some(JiraModal::TransitionFields { form, fields, .. }) | Some(JiraModal::CreateForm { form, fields, .. }) => (form, fields),
        _ => return PluginAction::None,
    };
    let FormState::SelectOpen { field_cursor, dropdown_cursor } = form else { return PluginAction::None; };
    let fc = *field_cursor;
    let oc = fields[fc].0.allowed_values.as_ref().map(|v| v.len()).unwrap_or(0);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => { if *dropdown_cursor + 1 < oc { *dropdown_cursor += 1; } }
        KeyCode::Char('k') | KeyCode::Up => { if *dropdown_cursor > 0 { *dropdown_cursor -= 1; } }
        KeyCode::Enter => {
            let dc = *dropdown_cursor;
            if let Some(allowed) = &fields[fc].0.allowed_values { if let Some(s) = allowed.get(dc) { fields[fc].1 = Some(FieldValue::Select(s.id.clone())); } }
            *form = FormState::Navigating { cursor: (fc + 1).min(field_count.saturating_sub(1)), scroll_offset: 0 };
        }
        KeyCode::Esc => { *form = FormState::Navigating { cursor: fc, scroll_offset: 0 }; }
        _ => {}
    }
    PluginAction::None
}

pub(super) fn handle_generic_form_multi(key: KeyEvent, plugin: &mut JiraPlugin, field_count: usize) -> PluginAction {
    let (form, fields) = match &mut plugin.modal {
        Some(JiraModal::TransitionFields { form, fields, .. }) | Some(JiraModal::CreateForm { form, fields, .. }) => (form, fields),
        _ => return PluginAction::None,
    };
    let FormState::MultiSelectOpen { field_cursor, dropdown_cursor, checked } = form else { return PluginAction::None; };
    let fc = *field_cursor;
    let oc = fields[fc].0.allowed_values.as_ref().map(|v| v.len()).unwrap_or(0);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => { if *dropdown_cursor + 1 < oc { *dropdown_cursor += 1; } }
        KeyCode::Char('k') | KeyCode::Up => { if *dropdown_cursor > 0 { *dropdown_cursor -= 1; } }
        KeyCode::Char(' ') => { let dc = *dropdown_cursor; if checked.contains(&dc) { checked.remove(&dc); } else { checked.insert(dc); } }
        KeyCode::Enter => {
            if let Some(allowed) = &fields[fc].0.allowed_values {
                let ids: Vec<String> = checked.iter().filter_map(|&i| allowed.get(i).map(|v| v.id.clone())).collect();
                fields[fc].1 = Some(FieldValue::MultiSelect(ids));
            }
            *form = FormState::Navigating { cursor: (fc + 1).min(field_count.saturating_sub(1)), scroll_offset: 0 };
        }
        KeyCode::Esc => { *form = FormState::Navigating { cursor: fc, scroll_offset: 0 }; }
        _ => {}
    }
    PluginAction::None
}

pub(super) fn enter_form_field_edit(fields: &mut [(EditableField, Option<FieldValue>)], form: &mut FormState, cursor: usize, issue_context: &str) -> Option<PluginAction> {
    let Some((field, value)) = fields.get(cursor) else { return None; };
    match field.field_type {
        FieldType::Text | FieldType::Number | FieldType::Date => {
            let c = match value { Some(FieldValue::Text(s)) => s.clone(), Some(FieldValue::Number(n)) => if n.fract() == 0.0 { format!("{}", *n as i64) } else { format!("{n}") }, Some(FieldValue::Date(d)) => d.clone(), _ => String::new() };
            *form = FormState::EditingText { cursor, buffer: c.clone(), cursor_pos: c.chars().count() };
            None
        }
        FieldType::TextArea => {
            let content = match value { Some(FieldValue::Text(s)) => s.clone(), _ => String::new() };
            Some(PluginAction::LaunchEditor {
                content,
                context: format!("textarea:{}:{}", issue_context, field.field_id),
            })
        }
        FieldType::Select => {
            let ci = match value { Some(FieldValue::Select(id)) => Some(id.as_str()), _ => None };
            let dc = field.allowed_values.as_ref().and_then(|v| ci.and_then(|id| v.iter().position(|a| a.id == id))).unwrap_or(0);
            *form = FormState::SelectOpen { field_cursor: cursor, dropdown_cursor: dc };
            None
        }
        FieldType::MultiSelect => {
            let ci: HashSet<String> = match value { Some(FieldValue::MultiSelect(ids)) => ids.iter().cloned().collect(), _ => HashSet::new() };
            let ch: HashSet<usize> = field.allowed_values.as_ref().map(|v| v.iter().enumerate().filter(|(_, a)| ci.contains(&a.id)).map(|(i, _)| i).collect()).unwrap_or_default();
            *form = FormState::MultiSelectOpen { field_cursor: cursor, dropdown_cursor: 0, checked: ch };
            None
        }
        FieldType::Unsupported => { None }
    }
}

pub(super) fn build_field_json(fields: &[(EditableField, Option<FieldValue>)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (field, value) in fields {
        if let Some(val) = value {
            let jv = match val {
                FieldValue::Text(s) => serde_json::json!(s),
                FieldValue::Number(n) => serde_json::json!(n),
                FieldValue::Select(id) => serde_json::json!({ "id": id }),
                FieldValue::MultiSelect(ids) => serde_json::json!(ids.iter().map(|id| serde_json::json!({ "id": id })).collect::<Vec<_>>()),
                FieldValue::Date(d) => serde_json::json!(d),
            };
            map.insert(field.field_id.clone(), jv);
        }
    }
    serde_json::Value::Object(map)
}

// ── Error modal rendering ───────────────────────────────────────────────────

pub(crate) fn render_error_modal(frame: &mut Frame, area: Rect, title: &str, message: &str) {
    use ratatui::widgets::Wrap;

    let msg_lines = message.lines().count().max(1);
    // Height: 1 (title border) + msg_lines + 1 (blank) + 1 (footer) + 1 (border) + padding
    let mh = (msg_lines as u16 + 6).min(area.height.saturating_sub(4));
    let mw = 60u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);

    if inner.height < 3 {
        return;
    }

    // Error message text (wrapped)
    let msg_area = Rect::new(inner.x + 1, inner.y + 1, inner.width.saturating_sub(2), inner.height.saturating_sub(3));
    let msg_para = Paragraph::new(message)
        .style(Style::default().fg(theme::TEXT_ERROR))
        .wrap(Wrap { trim: false });
    frame.render_widget(msg_para, msg_area);

    // Footer
    let footer_y = inner.y + inner.height - 1;
    let footer = Line::from(vec![
        Span::styled("  Enter", theme::accent()),
        Span::styled(": dismiss  ", theme::dim()),
        Span::styled("Esc", theme::accent()),
        Span::styled(": dismiss", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(footer), Rect::new(inner.x, footer_y, inner.width, 1));
}
