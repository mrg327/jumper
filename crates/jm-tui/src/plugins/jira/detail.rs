//! Issue detail modal rendering for the JIRA plugin.
//!
//! Renders the issue detail modal overlay, transition picker, and transition
//! fields form. Placeholder implementation — bodies will be filled in by Agent 2.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::plugins::PluginAction;

use super::JiraPlugin;

/// Render the issue detail modal overlay.
pub(crate) fn render_detail(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Render the transition picker modal overlay.
pub(crate) fn render_transition_picker(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Render the transition fields form modal overlay.
pub(crate) fn render_transition_fields(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Handle key events in the issue detail modal.
pub(crate) fn handle_detail_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}

/// Handle key events in the transition picker.
pub(crate) fn handle_transition_picker_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}

/// Handle key events in the transition fields form.
pub(crate) fn handle_transition_fields_key(
    key: KeyEvent,
    plugin: &mut JiraPlugin,
) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}
