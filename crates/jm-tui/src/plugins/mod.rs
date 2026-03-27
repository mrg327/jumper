//! Plugin system — two independent trait types for sidebar widgets and full-screen views.
//!
//! `SidebarPlugin` — small widget rendered in the right sidebar.
//! `ScreenPlugin`  — full-screen view with its own navigation and lifecycle.
//!
//! There is NO shared base trait. Each trait is self-contained.

mod clock;
mod notifications;
mod pomodoro;
mod sidebar;
pub mod about;
pub mod registry;

pub use clock::ClockPlugin;
pub use notifications::NotificationsPlugin;
pub use pomodoro::PomodoroPlugin;
pub use sidebar::PluginSidebar;
pub use about::AboutPlugin;
pub use registry::PluginRegistry;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Frame;

// ── SidebarPlugin ─────────────────────────────────────────────────────────────

/// A plugin that renders as a small widget in the sidebar panel.
///
/// Independent trait — no supertrait. Renders into a `Buffer` (widget-style),
/// which is appropriate for fixed-size sidebar areas.
pub trait SidebarPlugin {
    /// Unique identifier for this plugin (e.g., "clock", "pomodoro").
    fn name(&self) -> &str;

    /// Whether this plugin needs periodic tick events (called every 1s).
    fn needs_timer(&self) -> bool { false }

    /// Vertical height in rows this plugin needs in the sidebar.
    fn height(&self) -> u16 { 3 }

    /// Render the plugin into the given area (widget-style, into Buffer).
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Called every 1 second if `needs_timer()` is true.
    /// Returns notification messages to be forwarded to the notification system.
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }

    /// Handle a key event when this plugin has sidebar focus.
    /// Returns `true` if the key was consumed.
    fn on_key(&mut self, _key: KeyEvent) -> bool { false }

    /// Receive a notification message from another plugin or the system.
    /// Only meaningful for the NotificationsPlugin; others can ignore it.
    fn on_notify(&mut self, _message: &str) {}
}

// ── PluginAction ──────────────────────────────────────────────────────────────

/// Actions a screen plugin can request from the app.
///
/// Deliberately limited — screen plugins do NOT have access to the full `Action` enum.
/// The app translates `PluginAction` values into internal `Action` values as needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginAction {
    /// Key was handled, no app-level action needed.
    None,
    /// Navigate back to Dashboard (calls `on_leave()` then sets screen to Dashboard).
    Back,
    /// Show a toast notification with the given message.
    Toast(String),
}

// ── ScreenPlugin ──────────────────────────────────────────────────────────────

/// A plugin that renders as a full screen.
///
/// Independent trait — no supertrait. Uses `Frame` (not `Buffer`) for its render
/// signature, which is required for full-screen rendering (e.g., setting cursor position).
///
/// Screen plugins manage their own modal state internally — the App's modal system
/// is NOT exposed to plugins.
pub trait ScreenPlugin {
    /// Unique identifier for this plugin (e.g., "about", "jira").
    fn name(&self) -> &str;

    /// Whether this plugin needs periodic tick events (called every 250ms while active).
    fn needs_timer(&self) -> bool { false }

    /// Called every 250ms while this plugin's screen is active.
    /// Only called when the plugin screen is the current screen.
    /// Returns notification messages forwarded to `PluginSidebar` via `push_notification()`.
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }

    /// Render the full screen into the given area.
    /// Uses `Frame` (not `Buffer`) for full-screen rendering.
    fn render(&self, frame: &mut Frame, area: Rect);

    /// Handle a key event. Returns a `PluginAction` for the app to convert.
    fn handle_key(&mut self, key: KeyEvent) -> PluginAction;

    /// Called when the screen becomes active (navigating to this plugin).
    /// Use for initial data loading, API calls, thread spawning, etc.
    fn on_enter(&mut self);

    /// Called when the screen is deactivated (navigating away).
    /// Use for cleanup, stopping background threads, etc.
    fn on_leave(&mut self);

    /// Keybinding hints shown in the footer bar when this screen is active.
    /// Returns `'static` str pairs to avoid lifetime entanglement with the plugin's borrow.
    fn key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
}
