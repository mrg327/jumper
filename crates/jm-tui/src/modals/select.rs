//! List-selection modal.
//!
//! Shows a numbered list of items; j/k (or arrow keys) navigate,
//! Enter selects, Escape cancels.  The caller identifies which flow
//! triggered the modal via `SelectAction`.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::events::Action;
use crate::theme;

// ── What to do on selection ───────────────────────────────────────────

/// Identifies which flow this modal feeds.
#[derive(Debug, Clone)]
pub enum SelectAction {
    /// User is choosing which blocker to unblock.
    ChooseBlocker,
    /// User is choosing the source blocker to move (step 1 of MoveBlocker).
    MoveBlockerSource,
    /// User is choosing the destination project (step 2 of MoveBlocker).
    /// Carries the blocker index that was selected in step 1.
    MoveBlockerDest(usize),
    /// User is picking a parent issue for a sub-issue.
    PickParentIssue,
    /// User is picking an issue to cycle status forward.
    PickIssueToCycle,
    /// User is picking an issue to cycle status backward.
    PickIssueToCycleReverse,
    /// User is picking an issue to close.
    PickIssueToClose,
}

// ── State ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SelectModal {
    pub title: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub on_submit: SelectAction,
}

impl SelectModal {
    pub fn new(title: &str, items: Vec<String>, on_submit: SelectAction) -> Self {
        Self {
            title: title.to_string(),
            items,
            selected: 0,
            on_submit,
        }
    }

    // ── Key handling ─────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.items.is_empty() {
                    self.selected = (self.selected + 1).min(self.items.len() - 1);
                }
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Enter => {
                // Submit with index encoded as a string so we can reuse SubmitInput.
                Action::SubmitInput(self.selected.to_string())
            }
            KeyCode::Esc => Action::PopModal,
            _ => Action::None,
        }
    }

    // ── Render ───────────────────────────────────────────────────────

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup_area = crate::modals::centered_rect(70, 60, area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::MODAL_BORDER));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Reserve bottom row for footer hint.
        let [list_area, footer_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.selected {
                    theme::selected()
                } else {
                    Style::default()
                };
                ListItem::new(Span::styled(format!("  {item}"), style))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, list_area);

        let footer = Paragraph::new("j/k: navigate  Enter: select  Esc: cancel")
            .style(theme::dim());
        frame.render_widget(footer, footer_area);
    }
}
