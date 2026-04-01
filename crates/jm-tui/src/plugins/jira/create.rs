//! Issue creation flow for the JIRA plugin.
//!
//! Renders the project selection, issue type selection, and create form modals.
//! Placeholder implementation — bodies will be filled in by Agent 2.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::plugins::PluginAction;

use super::JiraPlugin;

/// Render the project selection modal.
pub(crate) fn render_select_project(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Render the issue type selection modal.
pub(crate) fn render_select_issue_type(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Render the create form modal.
pub(crate) fn render_create_form(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Handle key events in project selection.
pub(crate) fn handle_select_project_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}

/// Handle key events in issue type selection.
pub(crate) fn handle_select_issue_type_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}

/// Handle key events in the create form.
pub(crate) fn handle_create_form_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}
