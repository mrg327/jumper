use std::time::{Duration, Instant};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::theme;

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            created: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    #[allow(dead_code)]
    pub fn with_duration(message: &str, secs: u64) -> Self {
        Self {
            message: message.to_string(),
            created: Instant::now(),
            duration: Duration::from_secs(secs),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }

    /// Render the toast as a single line overlaid at the bottom of the area.
    /// Shows just above the footer keybinding bar.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if area.height < 2 {
            return;
        }
        let toast_area = Rect {
            x: area.x + 1,
            y: area.y + area.height - 2,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let text = format!("  {}  ", self.message);
        let para = Paragraph::new(text)
            .style(theme::toast_style())
            .alignment(Alignment::Center);
        frame.render_widget(para, toast_area);
    }
}
