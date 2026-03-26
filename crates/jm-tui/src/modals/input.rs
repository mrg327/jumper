//! Reusable text input modal.
//! Used for QuickNote, QuickBlocker, QuickDecision, EditFocus, EditTags,
//! EditTarget, AddProject, Unblock, MoveBlocker, and CommandMode.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::events::Action;
use crate::text_utils::{char_to_byte_idx, wrapped_cursor_position};
use crate::theme;

// ── What to do on submit ──────────────────────────────────────────────

/// Identifies which operation triggered this modal.
/// The caller uses this to route the submitted string correctly.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum InputAction {
    AddProject,
    QuickNote,
    QuickBlocker,
    QuickDecision,
    EditFocus,
    EditTags,
    EditTarget,
    Unblock,
    MoveBlocker,
    CommandMode,
    /// First step of EOD reflection: "What did you ship today?"
    EodReflectShipped,
    /// Second step of EOD reflection: "Most important thing for tomorrow?"
    EodReflectTomorrow(String), // carries the shipped text from step 1
    /// Add a new top-level issue
    AddIssue,
    /// Add a sub-issue (carries parent issue ID)
    AddSubIssue(u32),
    /// Quick-meeting mode: capture meeting note on the meetings project
    MeetingNote,
}

// ── State ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InputModal {
    pub title: String,
    pub prompt: String,
    pub input: String,
    pub cursor_pos: usize,
    pub on_submit: InputAction,
}

impl InputModal {
    /// Create a new, empty input modal.
    pub fn new(title: &str, prompt: &str, on_submit: InputAction) -> Self {
        Self {
            title: title.to_string(),
            prompt: prompt.to_string(),
            input: String::new(),
            cursor_pos: 0,
            on_submit,
        }
    }

    /// Create an input modal pre-populated with `initial` text.
    /// The cursor is placed at the end of the initial text.
    pub fn with_initial(title: &str, prompt: &str, initial: &str, on_submit: InputAction) -> Self {
        let input = initial.to_string();
        let cursor_pos = input.chars().count();
        Self {
            title: title.to_string(),
            prompt: prompt.to_string(),
            input,
            cursor_pos,
            on_submit,
        }
    }

    // ── Key handling ──────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            // Submit
            KeyCode::Enter => Action::SubmitInput(self.input.clone()),

            // Cancel
            KeyCode::Esc => Action::PopModal,

            // Cursor movement
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                Action::None
            }
            KeyCode::Right => {
                let len = self.input.chars().count();
                if self.cursor_pos < len {
                    self.cursor_pos += 1;
                }
                Action::None
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                Action::None
            }
            KeyCode::End => {
                self.cursor_pos = self.input.chars().count();
                Action::None
            }

            // Delete char before cursor
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    let byte_pos = char_to_byte_idx(&self.input, self.cursor_pos - 1);
                    let ch_len = self.input[byte_pos..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    self.input.drain(byte_pos..byte_pos + ch_len);
                    self.cursor_pos -= 1;
                }
                Action::None
            }

            // Delete char at cursor
            KeyCode::Delete => {
                let len = self.input.chars().count();
                if self.cursor_pos < len {
                    let byte_pos = char_to_byte_idx(&self.input, self.cursor_pos);
                    let ch_len = self.input[byte_pos..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    self.input.drain(byte_pos..byte_pos + ch_len);
                }
                Action::None
            }

            // Ctrl+W — delete word before cursor
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_word_before_cursor();
                Action::None
            }

            // Ctrl+U — clear to start of line
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.cursor_pos > 0 {
                    let byte_end = char_to_byte_idx(&self.input, self.cursor_pos);
                    self.input.drain(..byte_end);
                    self.cursor_pos = 0;
                }
                Action::None
            }

            // Printable characters
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let byte_pos = char_to_byte_idx(&self.input, self.cursor_pos);
                self.input.insert(byte_pos, c);
                self.cursor_pos += 1;
                Action::None
            }

            _ => Action::None,
        }
    }

    // ── Render ────────────────────────────────────────────────────────

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup_area = crate::modals::centered_rect(60, 50, area);

        // Clear the background.
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::MODAL_BORDER));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Split inner area: prompt row, input area (4 lines for wrapping), spacer, footer.
        let [prompt_area, input_area, _, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(inner);

        // Prompt line.
        let prompt_para = Paragraph::new(self.prompt.as_str())
            .style(Style::default().fg(theme::TEXT_DIM));
        frame.render_widget(prompt_para, prompt_area);

        // Input with word wrapping.
        let input_line = build_input_line(&self.input, self.cursor_pos);
        frame.render_widget(
            Paragraph::new(input_line).wrap(ratatui::widgets::Wrap { trim: false }),
            input_area,
        );

        // Compute cursor position accounting for word-wrap.
        let inner_width = input_area.width as usize;
        if inner_width > 0 {
            let (crow, ccol) = wrapped_cursor_position(&self.input, inner_width, self.cursor_pos);
            let cursor_x = input_area.x + ccol;
            let cursor_y = input_area.y + crow;
            if cursor_x < input_area.x + input_area.width
                && cursor_y < input_area.y + input_area.height
            {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }

        // Footer hint.
        let footer = Paragraph::new("Enter: submit  Escape: cancel")
            .style(Style::default().fg(theme::TEXT_DIM));
        frame.render_widget(footer, footer_area);
    }

    // ── Private helpers ───────────────────────────────────────────────

    fn delete_word_before_cursor(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        // Find the start of the word before the cursor.
        let chars: Vec<char> = self.input.chars().collect();
        let mut pos = self.cursor_pos;

        // Skip trailing spaces.
        while pos > 0 && chars[pos - 1] == ' ' {
            pos -= 1;
        }
        // Skip word characters.
        while pos > 0 && chars[pos - 1] != ' ' {
            pos -= 1;
        }

        let byte_start = char_to_byte_idx(&self.input, pos);
        let byte_end = char_to_byte_idx(&self.input, self.cursor_pos);
        self.input.drain(byte_start..byte_end);
        self.cursor_pos = pos;
    }
}

// ── Utilities ─────────────────────────────────────────────────────────

/// Build the input `Line` with a block-cursor indicator inserted at
/// `cursor_pos`.  Characters to the left are plain, the cursor character
/// (or a space) gets `SELECTED_BG` highlight, and characters to the right
/// are plain again.
fn build_input_line(input: &str, cursor_pos: usize) -> Line<'static> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    let before: String = chars[..cursor_pos.min(len)].iter().collect();
    let at_cursor: String = if cursor_pos < len {
        chars[cursor_pos].to_string()
    } else {
        " ".to_string() // phantom space at end
    };
    let after: String = if cursor_pos + 1 < len {
        chars[cursor_pos + 1..].iter().collect()
    } else {
        String::new()
    };

    Line::from(vec![
        Span::raw(before),
        Span::styled(
            at_cursor,
            Style::default()
                .bg(theme::MODAL_BORDER)
                .fg(ratatui::style::Color::Black),
        ),
        Span::raw(after),
    ])
}

