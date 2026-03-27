//! Plugin sidebar — stacks all plugins vertically and manages focus / ticks.

use crossterm::event::KeyEvent;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

use crate::theme;
use super::SidebarPlugin;

// ── Sidebar ──────────────────────────────────────────────────────────────────

pub struct PluginSidebar {
    pub plugins: Vec<Box<dyn SidebarPlugin>>,
    #[allow(dead_code)]
    pub focused_idx: usize,
}

impl PluginSidebar {
    /// Build the sidebar from a pre-constructed list of plugins.
    /// The registry (or caller) is responsible for instantiating the plugins.
    pub fn new_from(plugins: Vec<Box<dyn SidebarPlugin>>) -> Self {
        Self {
            plugins,
            focused_idx: 0,
        }
    }

    /// Render all plugins stacked inside the sidebar area.
    ///
    /// `focused` — whether the sidebar pane itself has keyboard focus.
    /// `focused_plugin_idx` — which plugin within the sidebar is active
    ///   (only matters when `focused == true`).
    pub fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        focused: bool,
        focused_plugin_idx: Option<usize>,
    ) {
        // Outer sidebar block.
        let border_style = if focused {
            theme::focused_border()
        } else {
            theme::unfocused_border()
        };

        let outer_block = Block::default()
            .title("Plugins")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = outer_block.inner(area);
        outer_block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Distribute vertical space to each plugin according to its height().
        // Plugins that don't fit are simply skipped.
        let mut y_offset = inner.y;

        for (idx, plugin) in self.plugins.iter().enumerate() {
            let remaining = inner.y + inner.height - y_offset;
            if remaining == 0 {
                break;
            }

            let plugin_height = plugin.height().min(remaining);
            let plugin_area = Rect {
                x: inner.x,
                y: y_offset,
                width: inner.width,
                height: plugin_height,
            };

            // When focused, highlight the active plugin's border by temporarily
            // overriding the border color after rendering.
            plugin.render(plugin_area, buf);

            // If this plugin is the focused one, redraw just its top border line
            // with the focused accent color to give a visual selection cue.
            if focused && focused_plugin_idx == Some(idx) {
                // Overwrite the top border row of the plugin area.
                for x in plugin_area.x..plugin_area.x + plugin_area.width {
                    if let Some(cell) = buf.cell_mut(Position::new(x, plugin_area.y)) {
                        cell.set_style(theme::focused_border());
                    }
                }
            }

            y_offset += plugin_height;
        }
    }

    /// Tick all timer-based plugins and return any notification messages they emit.
    pub fn on_tick(&mut self) -> Vec<String> {
        // Pass 1: tick every plugin and collect (plugin_index, messages) pairs.
        let notifications_plugin_idx = self
            .plugins
            .iter()
            .position(|p| p.name() == "Notifications");

        let mut all_messages: Vec<String> = Vec::new();
        let mut forwarded: Vec<String> = Vec::new();

        for (idx, plugin) in self.plugins.iter_mut().enumerate() {
            if !plugin.needs_timer() {
                continue;
            }
            let msgs = plugin.on_tick();
            for msg in msgs {
                // Messages from any plugin other than Notifications get forwarded there.
                if notifications_plugin_idx != Some(idx) {
                    forwarded.push(msg.clone());
                }
                all_messages.push(msg);
            }
        }

        // Pass 2: forward collected messages into the NotificationsPlugin.
        for msg in &forwarded {
            if let Some(notif_idx) = notifications_plugin_idx {
                if let Some(notif_plugin) = self.plugins.get_mut(notif_idx) {
                    notif_plugin.on_notify(msg);
                }
            }
        }

        all_messages
    }

    /// Push a message into the NotificationsPlugin if it is enabled.
    #[allow(dead_code)]
    pub fn push_notification(&mut self, message: &str) {
        for plugin in &mut self.plugins {
            // We rely on the plugin name to find the notification center.
            if plugin.name() == "Notifications" {
                // SidebarPlugin trait is object-safe — we tunnel the message via on_notify,
                // which NotificationsPlugin overrides to push into its queue.
                plugin.on_notify(message);
                break;
            }
        }
    }

    /// Forward a key event to the currently focused plugin.
    /// Returns `true` if the key was consumed.
    pub fn handle_key(&mut self, idx: usize, key: KeyEvent) -> bool {
        if let Some(plugin) = self.plugins.get_mut(idx) {
            plugin.on_key(key)
        } else {
            false
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}
