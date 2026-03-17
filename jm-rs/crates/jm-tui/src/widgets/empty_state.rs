use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::theme;

/// Render a dim, italic hint message centered in the area.
/// Used when a section has no content to teach the user how to populate it.
#[allow(dead_code)]
pub fn render_empty_hint(frame: &mut Frame, area: Rect, message: &str) {
    let para = Paragraph::new(message)
        .style(theme::empty_hint())
        .alignment(Alignment::Center);
    frame.render_widget(para, area);
}

/// Return a Line with the empty hint styled for use in paragraph builders.
#[allow(dead_code)]
pub fn empty_hint_line(message: &str) -> Line<'static> {
    Line::from(Span::styled(message.to_string(), theme::empty_hint()))
}

/// Common empty state messages.
#[allow(dead_code)]
pub mod messages {
    pub const NO_PROJECTS: &str = "No projects yet. Press a to create one.";
    pub const NO_BLOCKERS: &str = "No open blockers.";
    pub const NO_JOURNAL: &str = "No entries today. Press w to start working.";
    pub const NO_FOCUS: &str = "No focus set. Press e to set one.";
    pub const NO_DECISIONS: &str = "No decisions logged.";
    pub const NO_LOG: &str = "No entries. Press n to add a note.";
    pub const NO_PEOPLE: &str = "No stakeholders tracked yet.";
    pub const NO_PENDING: &str = "No pending items.";
    pub const NO_RESULTS: &str = "No results found.";
    pub const NO_NOTIFICATIONS: &str = "No notifications.";
}
