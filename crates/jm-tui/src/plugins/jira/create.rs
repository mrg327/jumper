//! Issue creation flow for the JIRA plugin.
//!
//! Renders the project selection, issue type selection, and create form modals.
//! The creation flow is a three-step wizard: select project, select issue type,
//! then fill in required and optional fields.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::plugins::PluginAction;
use crate::theme;

use super::api::JiraCommand;
use super::detail::{build_field_json, render_form_fields};
use super::models::*;
use super::{FormState, JiraModal, JiraPlugin};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn centered_rect_abs(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

// ── Select project rendering ────────────────────────────────────────────────

pub(crate) fn render_select_project(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::SelectProject { projects, cursor }) = &plugin.modal else { return; };
    let lh = projects.len().min(12) as u16;
    let mh = (lh + 5).min(area.height.saturating_sub(4));
    let mw = 40u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);
    let block = Block::default().title(" New Issue: Select Project ").borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);
    if inner.height < 3 { return; }
    let ch = inner.height.saturating_sub(1) as usize;
    let mut y = inner.y;
    for (idx, (key, name)) in projects.iter().enumerate() {
        if idx >= ch { break; }
        let sel = idx == *cursor;
        let st = if sel { theme::selected() } else { Style::default() };
        let p = if sel { "> " } else { "  " };
        let d = if name.is_empty() { key.clone() } else { format!("{key} \u{2014} {name}") };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("{p}{d}"), st))), Rect::new(inner.x, y, inner.width, 1));
        y += 1;
    }
    let fy = inner.y + inner.height - 1;
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled("  Enter", theme::accent()), Span::styled(": select  ", theme::dim()), Span::styled("Esc", theme::accent()), Span::styled(": cancel", theme::dim())])), Rect::new(inner.x, fy, inner.width, 1));
}

// ── Select issue type rendering ─────────────────────────────────────────────

pub(crate) fn render_select_issue_type(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::SelectIssueType { project_key, issue_types, cursor }) = &plugin.modal else { return; };
    if issue_types.is_empty() {
        let ma = centered_rect_abs(40, 6, area);
        frame.render_widget(Clear, ma);
        let block = Block::default().title(format!(" New Issue: {project_key} \u{2014} Issue Type ")).borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
        let inner = block.inner(ma);
        frame.render_widget(block, ma);
        frame.render_widget(Paragraph::new("  Loading issue types...").style(theme::dim()), Rect::new(inner.x, inner.y + 1, inner.width, 1));
        return;
    }
    let lh = issue_types.len().min(12) as u16;
    let mh = (lh + 5).min(area.height.saturating_sub(4));
    let mw = 40u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);
    let block = Block::default().title(format!(" New Issue: {project_key} \u{2014} Issue Type ")).borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);
    if inner.height < 3 { return; }
    let ch = inner.height.saturating_sub(1) as usize;
    let mut y = inner.y;
    for (idx, it) in issue_types.iter().enumerate() {
        if idx >= ch { break; }
        let sel = idx == *cursor;
        let st = if sel { theme::selected() } else { Style::default() };
        let p = if sel { "> " } else { "  " };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("{p}{}", it.name), st))), Rect::new(inner.x, y, inner.width, 1));
        y += 1;
    }
    let fy = inner.y + inner.height - 1;
    frame.render_widget(Paragraph::new(Line::from(vec![Span::styled("  Enter", theme::accent()), Span::styled(": select  ", theme::dim()), Span::styled("Esc", theme::accent()), Span::styled(": cancel", theme::dim())])), Rect::new(inner.x, fy, inner.width, 1));
}

// ── Create form rendering ───────────────────────────────────────────────────

pub(crate) fn render_create_form(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let Some(JiraModal::CreateForm { project_key, issue_type_id, fields, form }) = &plugin.modal else { return; };
    let fc = fields.len();
    let mh = ((fc as u16) + 6).min(area.height.saturating_sub(4));
    let mw = 60u16.min(area.width.saturating_sub(4));
    let ma = centered_rect_abs(mw, mh, area);
    frame.render_widget(Clear, ma);
    let title = format!(" New Issue: {} / {} ", project_key, issue_type_id);
    let block = Block::default().title(title).borders(Borders::ALL).border_style(Style::default().fg(theme::MODAL_BORDER));
    let inner = block.inner(ma);
    frame.render_widget(block, ma);
    if inner.height < 2 { return; }
    render_form_fields(frame, inner, fields, form);
    let fy = inner.y + inner.height - 1;
    let footer = match form {
        FormState::Submitting => Line::from(Span::styled("  Creating issue...", theme::dim())),
        _ => Line::from(vec![Span::styled("  j/k", theme::accent()), Span::styled(":nav  ", theme::dim()), Span::styled("Enter", theme::accent()), Span::styled(":edit  ", theme::dim()), Span::styled("S", theme::accent()), Span::styled(":submit  ", theme::dim()), Span::styled("Esc", theme::accent()), Span::styled(":cancel", theme::dim())]),
    };
    frame.render_widget(Paragraph::new(footer), Rect::new(inner.x, fy, inner.width, 1));
}

// ── Key handlers ────────────────────────────────────────────────────────────

pub(crate) fn handle_select_project_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let pc = { let Some(JiraModal::SelectProject { projects, .. }) = &plugin.modal else { return PluginAction::None; }; projects.len() };
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(JiraModal::SelectProject { cursor, projects }) = &mut plugin.modal { if !projects.is_empty() && *cursor + 1 < projects.len() { *cursor += 1; } }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(JiraModal::SelectProject { cursor, .. }) = &mut plugin.modal { if *cursor > 0 { *cursor -= 1; } }
            PluginAction::None
        }
        KeyCode::Enter => {
            if pc == 0 { return PluginAction::None; }
            let pk = { let Some(JiraModal::SelectProject { projects, cursor }) = &plugin.modal else { return PluginAction::None; }; projects[*cursor].0.clone() };
            plugin.modal = Some(JiraModal::SelectIssueType { project_key: pk.clone(), issue_types: Vec::new(), cursor: 0 });
            if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::FetchIssueTypes { project_key: pk }).ok(); }
            PluginAction::None
        }
        KeyCode::Esc => { plugin.modal = None; PluginAction::None }
        _ => PluginAction::None,
    }
}

pub(crate) fn handle_select_issue_type_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let (pk, tc) = { let Some(JiraModal::SelectIssueType { project_key, issue_types, .. }) = &plugin.modal else { return PluginAction::None; }; (project_key.clone(), issue_types.len()) };
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(JiraModal::SelectIssueType { cursor, issue_types, .. }) = &mut plugin.modal { if !issue_types.is_empty() && *cursor + 1 < issue_types.len() { *cursor += 1; } }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(JiraModal::SelectIssueType { cursor, .. }) = &mut plugin.modal { if *cursor > 0 { *cursor -= 1; } }
            PluginAction::None
        }
        KeyCode::Enter => {
            if tc == 0 { return PluginAction::None; }
            let sel = { let Some(JiraModal::SelectIssueType { issue_types, cursor, .. }) = &plugin.modal else { return PluginAction::None; }; issue_types[*cursor].clone() };
            plugin.modal = Some(JiraModal::CreateForm { project_key: pk.clone(), issue_type_id: sel.id.clone(), fields: Vec::new(), form: FormState::Submitting });
            if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::FetchCreateMeta { project_key: pk, issue_type_id: sel.id }).ok(); }
            PluginAction::None
        }
        KeyCode::Esc => {
            let pl: Vec<(String, String)> = { let mut v: Vec<_> = plugin.issues.iter().map(|i| (i.project_key.clone(), i.project_name.clone())).collect(); v.sort(); v.dedup(); v };
            plugin.modal = Some(JiraModal::SelectProject { projects: pl, cursor: 0 });
            PluginAction::None
        }
        _ => PluginAction::None,
    }
}

pub(crate) fn handle_create_form_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let (pk, itid, field_count, form_kind) = {
        let Some(JiraModal::CreateForm { project_key, issue_type_id, fields, form }) = &plugin.modal else { return PluginAction::None; };
        let fk = match form {
            FormState::Navigating { .. } => "nav",
            FormState::EditingText { .. } => "edit",
            FormState::SelectOpen { .. } => "select",
            FormState::MultiSelectOpen { .. } => "multi",
            FormState::Submitting => "submit",
            FormState::ValidationError { .. } => "valerr",
        };
        (project_key.clone(), issue_type_id.clone(), fields.len(), fk)
    };

    match form_kind {
        "nav" | "valerr" => handle_create_nav(key, plugin, &pk, &itid, field_count),
        "edit" => super::detail::handle_generic_form_edit(key, plugin, field_count),
        "select" => super::detail::handle_generic_form_select(key, plugin, field_count),
        "multi" => super::detail::handle_generic_form_multi(key, plugin, field_count),
        "submit" => { if key.code == KeyCode::Esc { plugin.modal = None; } PluginAction::None }
        _ => PluginAction::None,
    }
}

fn handle_create_nav(key: KeyEvent, plugin: &mut JiraPlugin, pk: &str, itid: &str, field_count: usize) -> PluginAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { if field_count > 0 && *cursor + 1 < field_count { *cursor += 1; } } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { if *cursor > 0 { *cursor -= 1; } } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Char('g') => {
            if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { *cursor = 0; } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Char('G') => {
            if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal {
                match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => { *cursor = field_count.saturating_sub(1); } _ => {} }
            }
            PluginAction::None
        }
        KeyCode::Enter => {
            let cur = { let Some(JiraModal::CreateForm { form, .. }) = &plugin.modal else { return PluginAction::None; }; match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => *cursor, _ => return PluginAction::None } };
            if let Some(JiraModal::CreateForm { project_key, fields, form, .. }) = &mut plugin.modal {
                if let Some(action) = super::detail::enter_form_field_edit(fields, form, cur, project_key) { return action; }
            }
            PluginAction::None
        }
        KeyCode::Char('S') => {
            // Check for unsupported required fields
            let unsup = { let Some(JiraModal::CreateForm { fields, .. }) = &plugin.modal else { return PluginAction::None; }; fields.iter().find(|(f, _)| f.required && f.field_type == FieldType::Unsupported).map(|(f, _)| f.name.clone()) };
            if let Some(name) = unsup { return PluginAction::Toast(format!("Required field '{name}' has unsupported type \u{2014} create in JIRA web UI")); }

            // Check for missing required fields
            let mi = { let Some(JiraModal::CreateForm { fields, .. }) = &plugin.modal else { return PluginAction::None; }; fields.iter().enumerate().find(|(_, (f, v))| f.required && v.is_none() && f.field_type != FieldType::Unsupported).map(|(i, _)| i) };
            if let Some(idx) = mi {
                if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal { match form { FormState::Navigating { cursor, .. } | FormState::ValidationError { cursor, .. } => *cursor = idx, _ => {} } }
                return PluginAction::None;
            }

            let mut fj = { let Some(JiraModal::CreateForm { fields, .. }) = &plugin.modal else { return PluginAction::None; }; build_field_json(fields) };
            if let serde_json::Value::Object(map) = &mut fj { map.insert("project".into(), serde_json::json!({ "key": pk })); map.insert("issuetype".into(), serde_json::json!({ "id": itid })); }
            if let Some(JiraModal::CreateForm { form, .. }) = &mut plugin.modal { *form = FormState::Submitting; }
            plugin.refreshing = true;
            if let Some(tx) = &plugin.command_tx { tx.send(JiraCommand::CreateIssue { project_key: pk.into(), fields: fj }).ok(); }
            PluginAction::None
        }
        KeyCode::Esc => { plugin.modal = None; PluginAction::None }
        _ => PluginAction::None,
    }
}
