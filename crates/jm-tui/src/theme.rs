//! ANSI base-16 semantic color mapping.
//! Uses only the 16 ANSI colors so the app respects the user's terminal theme.

use jm_core::models::{Priority, Status};
use ratatui::style::{Color, Modifier, Style};

// ── Semantic colors ─────────────────────────────────────────────────

pub const STATUS_ACTIVE: Color = Color::Green;
pub const STATUS_BLOCKED: Color = Color::Red;
pub const STATUS_PARKED: Color = Color::Yellow;
pub const STATUS_PENDING: Color = Color::Blue;
pub const STATUS_DONE: Color = Color::DarkGray;

pub const PRIORITY_HIGH: Color = Color::Red;
pub const PRIORITY_MEDIUM: Color = Color::Yellow;
pub const PRIORITY_LOW: Color = Color::Blue;

pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_UNFOCUSED: Color = Color::DarkGray;

pub const TEXT_PRIMARY: Color = Color::Reset;
pub const TEXT_DIM: Color = Color::DarkGray;
pub const TEXT_ACCENT: Color = Color::Cyan;
pub const TEXT_ERROR: Color = Color::Red;
pub const TEXT_SUCCESS: Color = Color::Green;
pub const TEXT_WARNING: Color = Color::Yellow;

pub const TAG_COLOR: Color = Color::Magenta;
pub const TIMESTAMP_COLOR: Color = Color::Cyan;
pub const PERSON_COLOR: Color = Color::Blue;

pub const TOAST_BG: Color = Color::DarkGray;
pub const MODAL_BORDER: Color = Color::Cyan;
pub const SELECTED_BG: Color = Color::DarkGray;

// ── Style helpers ───────────────────────────────────────────────────

#[allow(dead_code)] // utility kept for potential future use
pub fn status_style(status: Status) -> Style {
    let color = match status {
        Status::Active => STATUS_ACTIVE,
        Status::Blocked => STATUS_BLOCKED,
        Status::Pending => STATUS_PENDING,
        Status::Parked => STATUS_PARKED,
        Status::Done => STATUS_DONE,
    };
    Style::default().fg(Color::Black).bg(color)
}

pub fn status_badge(status: Status) -> (&'static str, Style) {
    match status {
        Status::Active => (
            " ACTIVE ",
            Style::default()
                .fg(Color::Black)
                .bg(STATUS_ACTIVE)
                .add_modifier(Modifier::BOLD),
        ),
        Status::Blocked => (
            " BLOCKED ",
            Style::default()
                .fg(Color::Black)
                .bg(STATUS_BLOCKED)
                .add_modifier(Modifier::BOLD),
        ),
        Status::Pending => (
            " PENDING ",
            Style::default()
                .fg(Color::Black)
                .bg(STATUS_PENDING)
                .add_modifier(Modifier::BOLD),
        ),
        Status::Parked => (
            " PARKED ",
            Style::default()
                .fg(Color::Black)
                .bg(STATUS_PARKED)
                .add_modifier(Modifier::BOLD),
        ),
        Status::Done => (
            " DONE ",
            Style::default()
                .fg(Color::White)
                .bg(STATUS_DONE)
                .add_modifier(Modifier::BOLD),
        ),
    }
}


pub fn priority_style(priority: Priority) -> Style {
    match priority {
        Priority::High => Style::default().fg(PRIORITY_HIGH).add_modifier(Modifier::BOLD),
        Priority::Medium => Style::default().fg(PRIORITY_MEDIUM),
        Priority::Low => Style::default().fg(PRIORITY_LOW),
    }
}

pub fn focused_border() -> Style {
    Style::default().fg(BORDER_FOCUSED)
}

pub fn unfocused_border() -> Style {
    Style::default().fg(BORDER_UNFOCUSED)
}

pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn accent() -> Style {
    Style::default().fg(TEXT_ACCENT)
}

pub fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    Style::default().bg(SELECTED_BG).add_modifier(Modifier::BOLD)
}

pub fn empty_hint() -> Style {
    Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC)
}

pub fn toast_style() -> Style {
    Style::default().fg(Color::White).bg(TOAST_BG)
}

pub fn tag_style() -> Style {
    Style::default().fg(TAG_COLOR)
}

pub fn person_style() -> Style {
    Style::default().fg(PERSON_COLOR)
}

pub fn timestamp_style() -> Style {
    Style::default().fg(TIMESTAMP_COLOR)
}
