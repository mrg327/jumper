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
