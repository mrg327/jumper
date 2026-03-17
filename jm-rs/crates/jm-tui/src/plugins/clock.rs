//! Clock plugin — shows current time and date.

use chrono::Local;
use crossterm::event::KeyEvent;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theme;
use super::Plugin;

pub struct ClockPlugin;

impl ClockPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for ClockPlugin {
    fn name(&self) -> &str {
        "Clock"
    }

    fn needs_timer(&self) -> bool {
        true
    }

    /// 2 lines of content + top/bottom border = 4 rows total.
    fn height(&self) -> u16 {
        4
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let now = Local::now();
        let time_str = now.format("%H:%M").to_string();
        let date_str = now.format("%a %b %d").to_string();

        let block = Block::default()
            .title("Clock")
            .borders(Borders::ALL)
            .border_style(theme::unfocused_border());

        let inner = block.inner(area);
        block.render(area, buf);

        // Stack two centered lines in the inner area.
        if inner.height >= 1 {
            let time_line = Paragraph::new(time_str)
                .style(theme::accent())
                .alignment(Alignment::Center);
            time_line.render(
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
        }

        if inner.height >= 2 {
            let date_line = Paragraph::new(date_str)
                .style(theme::dim())
                .alignment(Alignment::Center);
            date_line.render(
                Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );
        }
    }

    fn on_tick(&mut self) -> Vec<String> {
        Vec::new() // re-render handles the update
    }

    fn on_key(&mut self, _key: KeyEvent) -> bool {
        false
    }
}
