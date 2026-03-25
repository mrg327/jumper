//! Plugin system — trait-based, static registration.
//! Plugins render in the sidebar and can push notifications.

mod clock;
mod notifications;
mod pomodoro;
mod sidebar;

pub use clock::ClockPlugin;
pub use notifications::NotificationsPlugin;
pub use pomodoro::PomodoroPlugin;
pub use sidebar::PluginSidebar;

use crossterm::event::KeyEvent;
use ratatui::prelude::*;

/// Trait for sidebar plugins.
pub trait Plugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn height(&self) -> u16 { 3 }
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn on_tick(&mut self) -> Vec<String> { Vec::new() } // returns notification messages
    fn on_key(&mut self, _key: KeyEvent) -> bool { false } // true = consumed
    /// Called by the sidebar to push an external message into this plugin.
    /// Only meaningful for the NotificationsPlugin; others can ignore it.
    fn on_notify(&mut self, _message: &str) {}
}
