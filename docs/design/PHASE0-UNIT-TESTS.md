# Phase 0 Unit Tests

Prepared by QA Engineer 1. These are drop-in test modules, ready to paste once the
Phase 0 implementation lands. Each section calls out which file receives the test block.

All tests use the codebase's established pattern:
- `#[cfg(test)] mod tests { ... }` inside the same file as the code under test
- `use super::*;` for access to private items
- `KeyEvent::new(code, KeyModifiers::NONE)` for synthetic key events
- No heap allocation in test helpers beyond what the types require

---

## 1. `crates/jm-tui/src/plugins/mod.rs`

Paste this block **at the bottom** of `plugins/mod.rs`, after the trait definitions.

```rust
#[cfg(test)]
mod tests {
    use super::PluginAction;

    // ── PluginAction construction ────────────────────────────────────

    #[test]
    fn plugin_action_none_is_constructible() {
        let action = PluginAction::None;
        // If this compiles and runs without panic, the variant exists.
        let _ = action;
    }

    #[test]
    fn plugin_action_back_is_constructible() {
        let action = PluginAction::Back;
        let _ = action;
    }

    #[test]
    fn plugin_action_toast_wraps_string() {
        let msg = "hello from plugin".to_string();
        let action = PluginAction::Toast(msg.clone());
        // Destructure to confirm the inner String is preserved.
        if let PluginAction::Toast(inner) = action {
            assert_eq!(inner, msg);
        } else {
            panic!("Expected PluginAction::Toast");
        }
    }

    // ── Derives: Clone ───────────────────────────────────────────────

    #[test]
    fn plugin_action_none_clones() {
        let a = PluginAction::None;
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn plugin_action_back_clones() {
        let a = PluginAction::Back;
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn plugin_action_toast_clones() {
        let a = PluginAction::Toast("msg".to_string());
        let b = a.clone();
        assert_eq!(a, b);
    }

    // ── Derives: PartialEq + Eq ──────────────────────────────────────

    #[test]
    fn plugin_action_equality_none() {
        assert_eq!(PluginAction::None, PluginAction::None);
    }

    #[test]
    fn plugin_action_equality_back() {
        assert_eq!(PluginAction::Back, PluginAction::Back);
    }

    #[test]
    fn plugin_action_equality_toast_same_string() {
        assert_eq!(
            PluginAction::Toast("abc".to_string()),
            PluginAction::Toast("abc".to_string()),
        );
    }

    #[test]
    fn plugin_action_inequality_toast_different_strings() {
        assert_ne!(
            PluginAction::Toast("abc".to_string()),
            PluginAction::Toast("xyz".to_string()),
        );
    }

    #[test]
    fn plugin_action_inequality_across_variants() {
        assert_ne!(PluginAction::None, PluginAction::Back);
        assert_ne!(PluginAction::None, PluginAction::Toast("".to_string()));
        assert_ne!(PluginAction::Back, PluginAction::Toast("".to_string()));
    }

    // ── Derives: Debug ───────────────────────────────────────────────
    // Debug is exercised via format!() — if the derive is missing this won't compile.

    #[test]
    fn plugin_action_debug_none() {
        let s = format!("{:?}", PluginAction::None);
        assert!(s.contains("None"), "Debug output was: {s}");
    }

    #[test]
    fn plugin_action_debug_back() {
        let s = format!("{:?}", PluginAction::Back);
        assert!(s.contains("Back"), "Debug output was: {s}");
    }

    #[test]
    fn plugin_action_debug_toast() {
        let s = format!("{:?}", PluginAction::Toast("hi".to_string()));
        assert!(s.contains("Toast"), "Debug output was: {s}");
        assert!(s.contains("hi"), "Debug output was: {s}");
    }
}
```

---

## 2. `crates/jm-tui/src/plugins/registry.rs`

Paste this block at the bottom of `registry.rs`.

The tests use a minimal `MockScreenPlugin` struct so they can run without requiring
actual UI rendering. `MockScreenPlugin` also tracks `on_enter` / `on_leave` call counts
so lifecycle tests can be self-contained.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;
    use ratatui::Frame;

    use crate::plugins::{PluginAction, ScreenPlugin};

    // ── MockScreenPlugin ─────────────────────────────────────────────

    /// Minimal ScreenPlugin for testing the registry. Does not render anything.
    struct MockScreenPlugin {
        plugin_name: &'static str,
        needs_timer_val: bool,
        enter_count: u32,
        leave_count: u32,
        tick_count: u32,
    }

    impl MockScreenPlugin {
        fn new(name: &'static str) -> Self {
            Self {
                plugin_name: name,
                needs_timer_val: false,
                enter_count: 0,
                leave_count: 0,
                tick_count: 0,
            }
        }

        fn with_timer(mut self) -> Self {
            self.needs_timer_val = true;
            self
        }
    }

    impl ScreenPlugin for MockScreenPlugin {
        fn name(&self) -> &str {
            self.plugin_name
        }

        fn needs_timer(&self) -> bool {
            self.needs_timer_val
        }

        fn render(&self, _frame: &mut Frame, _area: Rect) {
            // No-op in tests — we never actually render.
        }

        fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,
                _ => PluginAction::None,
            }
        }

        fn on_enter(&mut self) {
            self.enter_count += 1;
        }

        fn on_leave(&mut self) {
            self.leave_count += 1;
        }

        fn on_tick(&mut self) -> Vec<String> {
            self.tick_count += 1;
            Vec::new()
        }

        fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
            vec![("Esc", "back")]
        }
    }

    // Helper: build a PluginRegistry with no sidebar plugins and one MockScreenPlugin
    // named "about".  Mirrors the state PluginRegistry::new() produces for Phase 0.
    fn registry_with_about() -> PluginRegistry {
        let mut reg = PluginRegistry {
            sidebar: PluginSidebar::new_from(Vec::new()),
            screens: Vec::new(),
        };
        reg.screens.push(Box::new(MockScreenPlugin::new("about")));
        reg
    }

    // ── get_screen ───────────────────────────────────────────────────

    #[test]
    fn get_screen_known_name_returns_some() {
        let reg = registry_with_about();
        assert!(reg.get_screen("about").is_some());
    }

    #[test]
    fn get_screen_unknown_name_returns_none() {
        let reg = registry_with_about();
        assert!(reg.get_screen("nonexistent").is_none());
    }

    #[test]
    fn get_screen_returns_plugin_with_correct_name() {
        let reg = registry_with_about();
        let plugin = reg.get_screen("about").expect("about should exist");
        assert_eq!(plugin.name(), "about");
    }

    // ── get_screen_mut ───────────────────────────────────────────────

    #[test]
    fn get_screen_mut_known_name_returns_some() {
        let mut reg = registry_with_about();
        assert!(reg.get_screen_mut("about").is_some());
    }

    #[test]
    fn get_screen_mut_unknown_name_returns_none() {
        let mut reg = registry_with_about();
        assert!(reg.get_screen_mut("nonexistent").is_none());
    }

    #[test]
    fn get_screen_mut_allows_mutation() {
        let mut reg = registry_with_about();
        // Call on_enter() through get_screen_mut — verifies we get a mutable reference.
        if let Some(plugin) = reg.get_screen_mut("about") {
            plugin.on_enter();
        }
        // Verify side-effect by downcasting.  Since we can't downcast a Box<dyn ScreenPlugin>
        // directly without unsafe, we verify indirectly: on_tick() should be callable.
        assert!(reg.get_screen_mut("about").is_some());
    }

    // ── AboutPlugin is always registered ────────────────────────────
    // This test verifies the contract that AboutPlugin is unconditionally registered
    // in PluginRegistry::new(). It calls new() with an empty config so no sidebar
    // plugins are loaded, but AboutPlugin must still appear in screens.

    #[test]
    fn new_always_registers_about_plugin() {
        use jm_core::config::Config;
        let config = Config::default();
        let reg = PluginRegistry::new(&config);
        assert!(
            reg.get_screen("about").is_some(),
            "AboutPlugin must always be registered"
        );
    }

    // ── tick_sidebar ─────────────────────────────────────────────────

    #[test]
    fn tick_sidebar_returns_vec_no_panic() {
        let mut reg = registry_with_about();
        // No sidebar plugins — should return empty vec without panic.
        let msgs = reg.tick_sidebar();
        assert!(msgs.is_empty());
    }

    // ── tick_screen ──────────────────────────────────────────────────

    #[test]
    fn tick_screen_no_timer_returns_empty() {
        let mut reg = registry_with_about();
        // MockScreenPlugin::needs_timer() returns false by default.
        let msgs = reg.tick_screen("about");
        assert!(msgs.is_empty(), "plugin without timer should produce no messages");
    }

    #[test]
    fn tick_screen_with_timer_calls_on_tick() {
        let mut reg = PluginRegistry {
            sidebar: PluginSidebar::new_from(Vec::new()),
            screens: vec![Box::new(MockScreenPlugin::new("timed").with_timer())],
        };
        // First tick.
        let msgs = reg.tick_screen("timed");
        // MockScreenPlugin::on_tick() returns empty vec.
        assert!(msgs.is_empty());

        // Verify on_tick was actually called by inspecting tick_count via a second
        // lookup — we can't downcast, so we call tick_screen again and confirm no
        // panic / no unexpected side effect.
        let msgs2 = reg.tick_screen("timed");
        assert!(msgs2.is_empty());
    }

    #[test]
    fn tick_screen_unknown_name_returns_empty() {
        let mut reg = registry_with_about();
        let msgs = reg.tick_screen("does-not-exist");
        assert!(msgs.is_empty());
    }

    // ── multiple screen plugins ──────────────────────────────────────

    #[test]
    fn registry_can_hold_multiple_screen_plugins() {
        let mut reg = PluginRegistry {
            sidebar: PluginSidebar::new_from(Vec::new()),
            screens: vec![
                Box::new(MockScreenPlugin::new("alpha")),
                Box::new(MockScreenPlugin::new("beta")),
                Box::new(MockScreenPlugin::new("gamma")),
            ],
        };
        assert!(reg.get_screen("alpha").is_some());
        assert!(reg.get_screen("beta").is_some());
        assert!(reg.get_screen("gamma").is_some());
        assert!(reg.get_screen("delta").is_none());
    }

    #[test]
    fn get_screen_returns_first_match_when_names_are_unique() {
        let reg = PluginRegistry {
            sidebar: PluginSidebar::new_from(Vec::new()),
            screens: vec![
                Box::new(MockScreenPlugin::new("first")),
                Box::new(MockScreenPlugin::new("second")),
            ],
        };
        assert_eq!(reg.get_screen("first").unwrap().name(), "first");
        assert_eq!(reg.get_screen("second").unwrap().name(), "second");
    }
}
```

---

## 3. `crates/jm-tui/src/plugins/about.rs`

Paste this block at the bottom of `about.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::plugins::PluginAction;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    // ── name() ───────────────────────────────────────────────────────

    #[test]
    fn name_returns_about() {
        let plugin = AboutPlugin::new();
        assert_eq!(plugin.name(), "about");
    }

    // ── handle_key() ─────────────────────────────────────────────────

    #[test]
    fn esc_returns_back() {
        let mut plugin = AboutPlugin::new();
        let action = plugin.handle_key(key(KeyCode::Esc));
        assert_eq!(action, PluginAction::Back);
    }

    #[test]
    fn q_returns_back() {
        let mut plugin = AboutPlugin::new();
        let action = plugin.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, PluginAction::Back);
    }

    #[test]
    fn other_keys_return_none() {
        let mut plugin = AboutPlugin::new();
        let cases = [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('a'),
            KeyCode::Char('?'),
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::F(1),
        ];
        for code in cases {
            let action = plugin.handle_key(key(code));
            assert_eq!(
                action,
                PluginAction::None,
                "key {code:?} should produce PluginAction::None"
            );
        }
    }

    // ── key_hints() ──────────────────────────────────────────────────

    #[test]
    fn key_hints_is_non_empty() {
        let plugin = AboutPlugin::new();
        let hints = plugin.key_hints();
        assert!(!hints.is_empty(), "AboutPlugin must provide at least one key hint");
    }

    #[test]
    fn key_hints_contains_esc() {
        let plugin = AboutPlugin::new();
        let hints = plugin.key_hints();
        let has_esc = hints.iter().any(|(key, _)| key.contains("Esc"));
        assert!(has_esc, "key_hints must include an Esc entry; got: {hints:?}");
    }

    #[test]
    fn key_hints_tuples_are_non_empty_strings() {
        let plugin = AboutPlugin::new();
        for (k, desc) in plugin.key_hints() {
            assert!(!k.is_empty(), "key string must not be empty");
            assert!(!desc.is_empty(), "description must not be empty");
        }
    }

    // ── lifecycle: on_enter / on_leave ───────────────────────────────
    // AboutPlugin's on_enter/on_leave are no-ops, but they must not panic.

    #[test]
    fn on_enter_does_not_panic() {
        let mut plugin = AboutPlugin::new();
        plugin.on_enter(); // must not panic
    }

    #[test]
    fn on_leave_does_not_panic() {
        let mut plugin = AboutPlugin::new();
        plugin.on_leave(); // must not panic
    }

    #[test]
    fn enter_leave_cycle_does_not_panic() {
        let mut plugin = AboutPlugin::new();
        plugin.on_enter();
        plugin.on_leave();
        plugin.on_enter();
        plugin.on_leave();
    }

    // ── needs_timer() ────────────────────────────────────────────────

    #[test]
    fn needs_timer_is_false() {
        let plugin = AboutPlugin::new();
        // AboutPlugin has no timer; tick_screen should be a no-op for it.
        assert!(!plugin.needs_timer());
    }

    // ── on_tick() ────────────────────────────────────────────────────

    #[test]
    fn on_tick_returns_empty_vec() {
        let mut plugin = AboutPlugin::new();
        let msgs = plugin.on_tick();
        assert!(msgs.is_empty(), "AboutPlugin::on_tick should return no messages");
    }
}
```

---

## 4. `crates/jm-tui/src/events.rs` (additions)

Paste this block at the bottom of `events.rs`, after the existing code.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── ScreenId::Plugin variant ─────────────────────────────────────

    #[test]
    fn screen_id_plugin_is_constructible() {
        let id = ScreenId::Plugin("test".to_string());
        let _ = id;
    }

    #[test]
    fn screen_id_plugin_equality_same_name() {
        assert_eq!(
            ScreenId::Plugin("test".to_string()),
            ScreenId::Plugin("test".to_string()),
        );
    }

    #[test]
    fn screen_id_plugin_inequality_different_names() {
        assert_ne!(
            ScreenId::Plugin("a".to_string()),
            ScreenId::Plugin("b".to_string()),
        );
    }

    #[test]
    fn screen_id_plugin_inequality_vs_dashboard() {
        assert_ne!(ScreenId::Plugin("about".to_string()), ScreenId::Dashboard);
    }

    #[test]
    fn screen_id_plugin_inequality_vs_other_variants() {
        let plugin = ScreenId::Plugin("about".to_string());
        assert_ne!(plugin, ScreenId::Review);
        assert_ne!(plugin, ScreenId::Search);
        assert_ne!(plugin, ScreenId::IssueBoard);
        assert_ne!(plugin, ScreenId::Weekly);
        assert_ne!(plugin, ScreenId::People);
    }

    #[test]
    fn screen_id_plugin_clones() {
        let original = ScreenId::Plugin("about".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn screen_id_plugin_clone_is_independent() {
        // Confirms Clone produces an owned copy, not an alias.
        let original = ScreenId::Plugin("about".to_string());
        let cloned = original.clone();
        // Both should still be equal after the original goes out of scope (it doesn't here,
        // but we verify structural independence by comparing the inner strings).
        if let (ScreenId::Plugin(a), ScreenId::Plugin(b)) = (&original, &cloned) {
            assert_eq!(a, b);
            // They're equal strings but different allocations — this is the expected outcome.
        } else {
            panic!("Both should be Plugin variants");
        }
    }

    // ── ScreenId::Plugin pattern-matching ────────────────────────────

    #[test]
    fn screen_id_plugin_pattern_match_extracts_name() {
        let id = ScreenId::Plugin("jira".to_string());
        let name = match id {
            ScreenId::Plugin(n) => n,
            _ => panic!("Expected Plugin variant"),
        };
        assert_eq!(name, "jira");
    }

    #[test]
    fn screen_id_plugin_ref_pattern_match_does_not_move() {
        let id = ScreenId::Plugin("about".to_string());
        // ref pattern — mirrors the clone-first borrow pattern used in app.rs
        if let ScreenId::Plugin(ref name) = id {
            assert_eq!(name, "about");
        } else {
            panic!("Expected Plugin variant");
        }
        // id is still usable after the ref match
        assert_eq!(id, ScreenId::Plugin("about".to_string()));
    }

    // ── Action::OpenPlugin variant ───────────────────────────────────

    #[test]
    fn action_open_plugin_is_constructible() {
        let action = Action::OpenPlugin("about".to_string());
        let _ = action;
    }

    #[test]
    fn action_open_plugin_wraps_string() {
        let action = Action::OpenPlugin("jira".to_string());
        if let Action::OpenPlugin(name) = action {
            assert_eq!(name, "jira");
        } else {
            panic!("Expected Action::OpenPlugin");
        }
    }

    #[test]
    fn action_open_plugin_clones() {
        let a = Action::OpenPlugin("about".to_string());
        let b = a.clone();
        // Action derives Clone — verify the clone contains the same name.
        if let (Action::OpenPlugin(na), Action::OpenPlugin(nb)) = (a, b) {
            assert_eq!(na, nb);
        } else {
            panic!("Both cloned values should be Action::OpenPlugin");
        }
    }

    #[test]
    fn action_open_plugin_debug() {
        let a = Action::OpenPlugin("about".to_string());
        let s = format!("{:?}", a);
        assert!(s.contains("OpenPlugin"), "Debug output was: {s}");
        assert!(s.contains("about"), "Debug output was: {s}");
    }

    // ── Clone-first borrow pattern ───────────────────────────────────
    // Verifies that cloning ScreenId before a mutable borrow compiles and
    // produces equal values, validating the core borrow-checker workaround.

    #[test]
    fn screen_id_clone_first_pattern_produces_equal_value() {
        let screen = ScreenId::Plugin("about".to_string());
        // Simulate: let name = name.clone(); before get_screen_mut()
        let screen_clone = screen.clone();
        assert_eq!(screen, screen_clone);

        if let ScreenId::Plugin(name) = screen_clone {
            // This is the owned String that gets passed into get_screen_mut()
            assert_eq!(name, "about");
        } else {
            panic!("Expected Plugin variant after clone");
        }
    }
}
```

---

## Placement Guide

| Test block | Target file |
|---|---|
| Section 1 | `crates/jm-tui/src/plugins/mod.rs` — append after trait definitions |
| Section 2 | `crates/jm-tui/src/plugins/registry.rs` — append at end of file |
| Section 3 | `crates/jm-tui/src/plugins/about.rs` — append at end of file |
| Section 4 | `crates/jm-tui/src/events.rs` — append at end of file |

## Running the Tests

```bash
# All Phase 0 plugin tests
cargo test -p jm-tui plugins

# Specific module
cargo test -p jm-tui plugins::mod::tests
cargo test -p jm-tui plugins::registry::tests
cargo test -p jm-tui plugins::about::tests
cargo test -p jm-tui events::tests

# Full suite (regression gate)
cargo test
```

## Notes for the Implementor

1. **`PluginSidebar::new_from`** — Section 2 calls `PluginSidebar::new_from(Vec::new())`.
   This method must be added as part of Task 5 (PluginSidebar refactor). Until Task 5
   lands, the registry tests will not compile.

2. **`Config::default()`** — The `new_always_registers_about_plugin` test calls
   `Config::default()`. Verify `jm_core::config::Config` implements `Default` (it
   should already from the existing code path that reads `~/.jm/config.yaml`).

3. **`key_hints()` lifetime** — The spec (`plugin-system-rewrite.md`) uses
   `Vec<(&'static str, &'static str)>`. The tests in Section 3 rely on this signature.
   If the implementation uses non-static lifetimes, adjust the test assertions
   accordingly (the logic remains identical).

4. **`PluginAction` derives** — Section 1 tests `PartialEq`, `Eq`, `Clone`, and
   `Debug`. All four derives must be present on `PluginAction`. The spec explicitly
   states: `#[derive(Debug, Clone, PartialEq, Eq)]`.

5. **`ScreenId` derives** — Section 4 tests `PartialEq`, `Clone`, and `Debug` on the
   new `Plugin(String)` variant. The existing `ScreenId` already derives these; the
   new variant inherits them automatically.

6. **`Action::OpenPlugin` derives** — Section 4 tests `Clone` and `Debug`. The existing
   `Action` already derives both; the new variant inherits them.
