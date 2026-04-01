//! Kanban board rendering for the JIRA plugin.
//!
//! Renders the full-screen kanban board with horizontal scrolling columns.
//! Placeholder implementation — bodies will be filled in by Agent 2.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::plugins::PluginAction;

use super::JiraPlugin;

/// Render the kanban board (full screen, no modal active).
pub(crate) fn render(frame: &mut Frame, area: Rect, plugin: &JiraPlugin) {
    let _ = (frame, area, plugin);
}

/// Handle key events on the kanban board (no modal active).
pub(crate) fn handle_key(key: KeyEvent, plugin: &mut JiraPlugin) -> PluginAction {
    let _ = (key, plugin);
    PluginAction::None
}
