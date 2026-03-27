//! Plugin system — two independent traits for sidebar widgets and full-screen plugins.
//! No base `Plugin` trait. `SidebarPlugin` and `ScreenPlugin` stand alone.

mod clock;
mod notifications;
mod pomodoro;
mod sidebar;
pub mod about;

#[allow(unused_imports)]
pub use clock::ClockPlugin;
#[allow(unused_imports)]
pub use notifications::NotificationsPlugin;
#[allow(unused_imports)]
pub use pomodoro::PomodoroPlugin;
pub use sidebar::PluginSidebar;
#[allow(unused_imports)]
pub use about::AboutPlugin;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Frame;

/// A plugin that renders in the sidebar (small widget).
/// Independent trait — no supertrait.
pub trait SidebarPlugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }
    fn on_notify(&mut self, _message: &str) {}
    fn on_key(&mut self, _key: KeyEvent) -> bool { false }
    fn height(&self) -> u16 { 3 }
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Actions a screen plugin can request from the app.
/// Deliberately limited — screen plugins do NOT have access to the full Action enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginAction {
    None,
    Back,
    Toast(String),
}

/// A plugin that renders as a full screen.
/// Independent trait — no supertrait.
/// Uses Frame (not Buffer) for its render signature.
pub trait ScreenPlugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> PluginAction;
    fn on_enter(&mut self);
    fn on_leave(&mut self);
    fn key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
}
