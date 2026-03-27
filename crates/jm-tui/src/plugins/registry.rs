//! `PluginRegistry` — manages both sidebar and screen plugins.
//!
//! The registry is the single source of truth for all plugins.
//! - Sidebar plugins are stored inside `PluginSidebar` and tick at 1s.
//! - Screen plugins are stored in `Vec<Box<dyn ScreenPlugin>>` and tick at 250ms (active only).

use chrono::NaiveTime;

use jm_core::config::Config;

use super::{
    AboutPlugin, ClockPlugin, NotificationsPlugin, PluginSidebar, PomodoroPlugin, ScreenPlugin,
    SidebarPlugin,
};

/// Manages both sidebar and screen plugins.
///
/// Screen plugins are stored in a `Vec` (not `HashMap`) and looked up by linear scan —
/// the number of screen plugins will always be small (typically 1-3).
pub struct PluginRegistry {
    /// The sidebar container, which holds all `SidebarPlugin` instances.
    pub sidebar: PluginSidebar,
    /// All registered screen plugins (full-screen views).
    pub screens: Vec<Box<dyn ScreenPlugin>>,
}

impl PluginRegistry {
    /// Construct the registry from the user's config.
    ///
    /// - Sidebar plugins are instantiated for each name in `config.plugins.enabled`.
    /// - `AboutPlugin` is always registered as a screen plugin (no config needed).
    pub fn new(config: &Config) -> Self {
        let pomo_cfg = &config.plugins.pomodoro;
        let notif_cfg = &config.plugins.notifications;

        // Parse scheduled reminders from config.
        let reminders: Vec<(NaiveTime, String)> = notif_cfg
            .reminders
            .iter()
            .filter_map(|r| {
                NaiveTime::parse_from_str(&r.time, "%H:%M")
                    .ok()
                    .map(|t| (t, r.message.clone()))
            })
            .collect();

        let mut sidebar_plugins: Vec<Box<dyn SidebarPlugin>> = Vec::new();

        for name in &config.plugins.enabled {
            match name.as_str() {
                "pomodoro" => sidebar_plugins.push(Box::new(PomodoroPlugin::new(
                    pomo_cfg.work_minutes,
                    pomo_cfg.short_break_minutes,
                    pomo_cfg.long_break_minutes,
                    pomo_cfg.sessions_before_long,
                ))),
                "notifications" => {
                    sidebar_plugins.push(Box::new(NotificationsPlugin::new(reminders.clone())))
                }
                "clock" => sidebar_plugins.push(Box::new(ClockPlugin::new())),
                _ => {} // unknown plugin names are silently skipped
            }
        }

        // AboutPlugin is always registered as a screen plugin — no config needed.
        let screen_plugins: Vec<Box<dyn ScreenPlugin>> =
            vec![Box::new(AboutPlugin::new())];

        Self {
            sidebar: PluginSidebar::new_from(sidebar_plugins),
            screens: screen_plugins,
        }
    }

    /// Find a screen plugin by name (immutable reference).
    ///
    /// Uses linear scan — the number of screen plugins is always small.
    pub fn get_screen(&self, name: &str) -> Option<&dyn ScreenPlugin> {
        self.screens.iter().find(|p| p.name() == name).map(|p| &**p)
    }

    /// Find a screen plugin by name (mutable reference).
    ///
    /// Uses linear scan — the number of screen plugins is always small.
    pub fn get_screen_mut(&mut self, name: &str) -> Option<&mut Box<dyn ScreenPlugin>> {
        self.screens.iter_mut().find(|p| p.name() == name)
    }

    /// Tick all sidebar plugins (1s interval, always ticked).
    ///
    /// Returns notification messages emitted by plugins.
    pub fn tick_sidebar(&mut self) -> Vec<String> {
        self.sidebar.on_tick()
    }

    /// Tick only the named screen plugin (250ms interval, active screen only).
    ///
    /// If the plugin does not need a timer, returns an empty vec.
    /// Notification messages returned here should be forwarded to the sidebar via
    /// `self.sidebar.push_notification()`.
    pub fn tick_screen(&mut self, name: &str) -> Vec<String> {
        if let Some(plugin) = self.get_screen_mut(name) {
            if plugin.needs_timer() {
                return plugin.on_tick();
            }
        }
        Vec::new()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use jm_core::config::Config;

    fn default_config() -> Config {
        Config::default()
    }

    #[test]
    fn registry_constructs_without_panic() {
        let config = default_config();
        let _registry = PluginRegistry::new(&config);
    }

    #[test]
    fn about_plugin_always_registered() {
        let config = default_config();
        let registry = PluginRegistry::new(&config);
        assert!(
            registry.get_screen("about").is_some(),
            "AboutPlugin should always be registered"
        );
    }

    #[test]
    fn get_screen_returns_none_for_unknown() {
        let config = default_config();
        let registry = PluginRegistry::new(&config);
        assert!(registry.get_screen("nonexistent").is_none());
    }

    #[test]
    fn get_screen_mut_allows_mutation() {
        let config = default_config();
        let mut registry = PluginRegistry::new(&config);
        let plugin = registry.get_screen_mut("about");
        assert!(plugin.is_some(), "should find about plugin mutably");
    }

    #[test]
    fn tick_screen_returns_empty_for_no_timer() {
        let config = default_config();
        let mut registry = PluginRegistry::new(&config);
        // AboutPlugin does not need a timer — tick should return empty.
        let msgs = registry.tick_screen("about");
        assert!(msgs.is_empty());
    }

    #[test]
    fn plugin_action_enum_derives() {
        use crate::plugins::PluginAction;

        let a = PluginAction::None;
        let b = a.clone();
        assert_eq!(a, b);

        let toast = PluginAction::Toast("hello".to_string());
        assert_ne!(toast, PluginAction::Back);

        // Debug should not panic
        let _ = format!("{:?}", PluginAction::Back);
    }
}
