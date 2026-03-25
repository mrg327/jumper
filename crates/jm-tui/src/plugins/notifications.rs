//! Notification center — collects plugin alerts and scheduled reminders.

use chrono::{Local, NaiveTime};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::theme;
use super::Plugin;

// ── Data types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    /// "HH:MM" when the notification was created.
    pub time: String,
    /// Seconds until auto-dismiss.
    pub expires_in: u32,
}

impl Notification {
    pub fn new(message: impl Into<String>) -> Self {
        let time = Local::now().format("%H:%M").to_string();
        Self {
            message: message.into(),
            time,
            expires_in: 1800, // 30 minutes
        }
    }
}

// ── Plugin struct ────────────────────────────────────────────────────────────

pub struct NotificationsPlugin {
    pub notifications: Vec<Notification>,
    pub max_notifications: usize,
    /// Scheduled reminders: (time, message).
    pub reminders: Vec<(NaiveTime, String)>,
    /// Messages already fired today (to prevent re-firing).
    pub fired_today: Vec<String>,
}

impl NotificationsPlugin {
    pub fn new(reminders: Vec<(NaiveTime, String)>) -> Self {
        Self {
            notifications: Vec::new(),
            max_notifications: 10,
            reminders,
            fired_today: Vec::new(),
        }
    }

    /// Push an external notification (called by the sidebar when a plugin emits one).
    pub fn push(&mut self, message: impl Into<String>) {
        let n = Notification::new(message);
        // Avoid exact duplicates that arrive simultaneously.
        if self.notifications.first().map(|x| x.message.as_str()) == Some(n.message.as_str()) {
            return;
        }
        self.notifications.insert(0, n);
        self.notifications.truncate(self.max_notifications);
    }
}

// ── Plugin impl ──────────────────────────────────────────────────────────────

impl Plugin for NotificationsPlugin {
    fn name(&self) -> &str {
        "Notifications"
    }

    fn needs_timer(&self) -> bool {
        true
    }

    fn height(&self) -> u16 {
        // Border (2) + header line (1) + up to 5 visible notifications.
        let visible = self.notifications.len().min(5) as u16;
        3 + visible
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let count = self.notifications.len();
        let title = format!("Notifs ({})", count);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(theme::unfocused_border());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }

        if self.notifications.is_empty() {
            Paragraph::new("No notifications")
                .style(theme::dim())
                .alignment(Alignment::Center)
                .render(
                    Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
                    buf,
                );
            return;
        }

        let max_visible = inner.height as usize;
        let items: Vec<ListItem> = self
            .notifications
            .iter()
            .take(max_visible)
            .map(|n| {
                let line = format!("{} {}", n.time, n.message);
                ListItem::new(line).style(Style::default().fg(theme::TEXT_PRIMARY))
            })
            .collect();

        Widget::render(List::new(items).style(Style::default()), inner, buf);
    }

    fn on_tick(&mut self) -> Vec<String> {
        let now = Local::now().time();

        // Collect which reminders should fire this tick, without holding
        // an immutable borrow on self while we later mutate.
        let to_fire: Vec<(String, String)> = self
            .reminders
            .iter()
            .filter_map(|(reminder_time, message)| {
                let key = format!("{}|{}", reminder_time.format("%H:%M"), message);
                if self.fired_today.contains(&key) {
                    return None;
                }
                // Fire if now is within a 60-second window after the scheduled time.
                let secs_diff = (now - *reminder_time).num_seconds();
                if (0..60).contains(&secs_diff) {
                    Some((key, message.clone()))
                } else {
                    None
                }
            })
            .collect();

        let mut new_messages: Vec<String> = Vec::new();
        for (key, message) in to_fire {
            self.fired_today.push(key);
            self.push(message.clone());
            new_messages.push(message);
        }

        // Decrement expiry and remove stale notifications.
        for n in &mut self.notifications {
            n.expires_in = n.expires_in.saturating_sub(1);
        }
        self.notifications.retain(|n| n.expires_in > 0);

        new_messages
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') => {
                self.notifications.clear();
                true
            }
            _ => false,
        }
    }

    fn on_notify(&mut self, message: &str) {
        self.push(message);
    }
}
