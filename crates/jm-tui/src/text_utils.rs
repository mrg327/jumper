//! Shared text-editing utilities used by input modals and multi-step screens.

/// Convert a char-indexed position to the corresponding byte offset in `s`.
pub fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Move cursor to the start of the previous word.
pub fn prev_word_boundary(s: &str, pos: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = pos;
    // Skip whitespace to the left
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    // Skip non-whitespace to the left
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

/// Move cursor to the start of the next word.
pub fn next_word_boundary(s: &str, pos: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = pos;
    // Skip non-whitespace to the right
    while i < len && !chars[i].is_whitespace() {
        i += 1;
    }
    // Skip whitespace to the right
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Given a string, an available `width` (in columns), and a char-indexed
/// `cursor_pos`, return the `(row, col)` terminal position of the cursor after
/// word-wrapping the text to fit `width`.
///
/// Mirrors ratatui's `Wrap { trim: false }`: words that would overflow the
/// current line are moved to the next line; words longer than `width` are
/// hard-wrapped at the boundary.
///
/// Returns `(row, col)` both 0-indexed.
pub fn wrapped_cursor_position(text: &str, width: usize, cursor_pos: usize) -> (u16, u16) {
    if width == 0 {
        return (0, 0);
    }

    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();

    let mut row: u16 = 0;
    let mut col: usize = 0;
    let mut i: usize = 0;

    while i < total {
        if i == cursor_pos {
            return (row, col as u16);
        }

        let ch = chars[i];

        if ch == ' ' {
            col += 1;
            if col >= width {
                row += 1;
                col = 0;
            }
            i += 1;
        } else {
            let word_len = chars[i..].iter().take_while(|&&c| c != ' ').count();

            if col > 0 && col + word_len > width && word_len <= width {
                // Word fits on one line but not the current one — wrap before it.
                row += 1;
                col = 0;
                // Don't advance i; re-process on the new line.
            } else {
                col += 1;
                if col >= width {
                    row += 1;
                    col = 0;
                }
                i += 1;
            }
        }
    }

    (row, col as u16)
}
