//! Pomodoro timer plugin with a work/break state machine.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theme;
use super::SidebarPlugin;

// ── State machine ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PomodoroState {
    Idle,
    Work,
    ShortBreak,
    LongBreak,
    /// Wraps the state that was active when pause was triggered.
    Paused(Box<PomodoroState>),
}

// ── Plugin struct ────────────────────────────────────────────────────────────

pub struct PomodoroPlugin {
    pub state: PomodoroState,
    pub remaining_secs: u32,
    pub session_count: u32,
    pub work_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub sessions_before_long: u32,
    pub last_tick: Option<Instant>,
}

impl PomodoroPlugin {
    pub fn new(
        work_minutes: u32,
        short_break_minutes: u32,
        long_break_minutes: u32,
        sessions_before_long: u32,
    ) -> Self {
        Self {
            state: PomodoroState::Idle,
            remaining_secs: work_minutes * 60,
            session_count: 0,
            work_minutes,
            short_break_minutes,
            long_break_minutes,
            sessions_before_long,
            last_tick: None,
        }
    }

    /// Seconds for the given running state.
    fn default_secs_for(&self, state: &PomodoroState) -> u32 {
        match state {
            PomodoroState::Work => self.work_minutes * 60,
            PomodoroState::ShortBreak => self.short_break_minutes * 60,
            PomodoroState::LongBreak => self.long_break_minutes * 60,
            _ => self.work_minutes * 60,
        }
    }

    /// Transition to `next`, resetting the timer and returning a notification.
    fn transition(&mut self, next: PomodoroState) -> String {
        let msg = match &next {
            PomodoroState::Work => {
                format!("Work session {} started. Focus!", self.session_count + 1)
            }
            PomodoroState::ShortBreak => "Short break — stretch and breathe.".to_string(),
            PomodoroState::LongBreak => "Long break — great work! Recharge.".to_string(),
            PomodoroState::Idle => {
                "Pomodoro cycle complete. Well done!".to_string()
            }
            PomodoroState::Paused(_) => String::new(),
        };
        self.remaining_secs = self.default_secs_for(&next);
        self.state = next;
        self.last_tick = None;
        msg
    }

    /// Determine the next state after a timer expires.
    fn next_after_work(&self) -> PomodoroState {
        let next_session = self.session_count + 1;
        if next_session % self.sessions_before_long == 0 {
            PomodoroState::LongBreak
        } else {
            PomodoroState::ShortBreak
        }
    }

    fn format_time(secs: u32) -> String {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

// ── Plugin impl ──────────────────────────────────────────────────────────────

impl SidebarPlugin for PomodoroPlugin {
    fn name(&self) -> &str {
        "Pomodoro"
    }

    fn needs_timer(&self) -> bool {
        true
    }

    /// 4 lines of content + border = 6 rows.
    fn height(&self) -> u16 {
        6
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Pomodoro")
            .borders(Borders::ALL)
            .border_style(theme::unfocused_border());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }

        let (label, time_display, style) = match &self.state {
            PomodoroState::Idle => (
                "IDLE".to_string(),
                format!("Ready ({}:00)", self.work_minutes),
                theme::dim(),
            ),
            PomodoroState::Work => (
                format!(
                    "WORK {}/{}",
                    self.session_count + 1,
                    self.sessions_before_long
                ),
                Self::format_time(self.remaining_secs),
                Style::default().fg(theme::TEXT_SUCCESS),
            ),
            PomodoroState::ShortBreak => (
                "SHORT BREAK".to_string(),
                Self::format_time(self.remaining_secs),
                Style::default().fg(theme::TEXT_WARNING),
            ),
            PomodoroState::LongBreak => (
                "LONG BREAK".to_string(),
                Self::format_time(self.remaining_secs),
                Style::default().fg(theme::TEXT_WARNING),
            ),
            PomodoroState::Paused(inner_state) => {
                let inner_label = match inner_state.as_ref() {
                    PomodoroState::Work => "WORK",
                    PomodoroState::ShortBreak => "SHORT BRK",
                    PomodoroState::LongBreak => "LONG BRK",
                    _ => "IDLE",
                };
                (
                    format!("PAUSED ({})", inner_label),
                    Self::format_time(self.remaining_secs),
                    theme::dim(),
                )
            }
        };

        // Line 1: state label with tomato emoji
        if inner.height >= 1 {
            let label_text = format!("🍅 {}", label);
            Paragraph::new(label_text)
                .style(style)
                .alignment(Alignment::Center)
                .render(
                    Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
                    buf,
                );
        }

        // Line 2: countdown
        if inner.height >= 2 {
            Paragraph::new(time_display)
                .style(style.add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .render(
                    Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
                    buf,
                );
        }

        // Line 3: session dots
        if inner.height >= 3 {
            let total = self.sessions_before_long as usize;
            let done = (self.session_count % self.sessions_before_long) as usize;
            let dots: String = (0..total)
                .map(|i| if i < done { '●' } else { '○' })
                .collect();
            Paragraph::new(dots)
                .style(theme::dim())
                .alignment(Alignment::Center)
                .render(
                    Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: 1 },
                    buf,
                );
        }

        // Line 4: hint
        if inner.height >= 4 {
            let hint = match &self.state {
                PomodoroState::Idle => "Spc:start",
                PomodoroState::Paused(_) => "Spc:resume",
                _ => "Spc:pause",
            };
            Paragraph::new(hint)
                .style(theme::dim())
                .alignment(Alignment::Center)
                .render(
                    Rect { x: inner.x, y: inner.y + 3, width: inner.width, height: 1 },
                    buf,
                );
        }
    }

    fn on_tick(&mut self) -> Vec<String> {
        // Only tick when actively running.
        let is_running = matches!(
            &self.state,
            PomodoroState::Work | PomodoroState::ShortBreak | PomodoroState::LongBreak
        );
        if !is_running {
            self.last_tick = None;
            return Vec::new();
        }

        let now = Instant::now();
        let elapsed = match self.last_tick {
            Some(prev) => {
                let e = prev.elapsed().as_secs() as u32;
                if e == 0 {
                    self.last_tick = Some(now);
                    return Vec::new();
                }
                e
            }
            None => {
                self.last_tick = Some(now);
                return Vec::new();
            }
        };

        self.last_tick = Some(now);

        let to_subtract = elapsed.min(self.remaining_secs);
        self.remaining_secs -= to_subtract;

        if self.remaining_secs > 0 {
            return Vec::new();
        }

        // Timer expired — transition.
        let (notification, next_state) = match &self.state.clone() {
            PomodoroState::Work => {
                self.session_count += 1;
                let msg = "🍅 Work session complete! Time for a break.".to_string();
                let next = self.next_after_work();
                (msg, next)
            }
            PomodoroState::ShortBreak => {
                let msg = "☕ Break over — back to work!".to_string();
                (msg, PomodoroState::Work)
            }
            PomodoroState::LongBreak => {
                let msg = "🎉 Long break complete. Resetting Pomodoro.".to_string();
                (msg, PomodoroState::Idle)
            }
            _ => return Vec::new(),
        };

        let transition_msg = self.transition(next_state);
        let mut notifications = vec![notification];
        if !transition_msg.is_empty() {
            notifications.push(transition_msg);
        }
        notifications
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Toggle start / pause
            KeyCode::Char(' ') => {
                match self.state.clone() {
                    PomodoroState::Idle => {
                        self.transition(PomodoroState::Work);
                    }
                    PomodoroState::Paused(inner) => {
                        // Resume — restore inner state and restart last_tick tracking.
                        self.state = *inner;
                        self.last_tick = Some(Instant::now());
                    }
                    running => {
                        self.state = PomodoroState::Paused(Box::new(running));
                        self.last_tick = None;
                    }
                }
                true
            }

            // Add 5 minutes
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.remaining_secs += 5 * 60;
                true
            }

            // Subtract 5 minutes (minimum 60 seconds)
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let min_secs = 60u32;
                self.remaining_secs =
                    self.remaining_secs.saturating_sub(5 * 60).max(min_secs);
                true
            }

            // Reset current timer to default
            KeyCode::Char('r') => {
                let base = match &self.state {
                    PomodoroState::Work => self.work_minutes * 60,
                    PomodoroState::ShortBreak => self.short_break_minutes * 60,
                    PomodoroState::LongBreak => self.long_break_minutes * 60,
                    PomodoroState::Paused(inner) => match inner.as_ref() {
                        PomodoroState::Work => self.work_minutes * 60,
                        PomodoroState::ShortBreak => self.short_break_minutes * 60,
                        PomodoroState::LongBreak => self.long_break_minutes * 60,
                        _ => self.work_minutes * 60,
                    },
                    PomodoroState::Idle => self.work_minutes * 60,
                };
                self.remaining_secs = base;
                self.last_tick = None;
                true
            }

            // Full reset
            KeyCode::Char('R') => {
                self.state = PomodoroState::Idle;
                self.remaining_secs = self.work_minutes * 60;
                self.session_count = 0;
                self.last_tick = None;
                true
            }

            _ => false,
        }
    }
}
