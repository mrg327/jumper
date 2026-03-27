# Phase 0: Plugin System Rewrite -- Readiness Assessment

Assessment date: 2026-03-26
Assessed against: `docs/design/plugin-system-rewrite.md` (primary) + codebase

---

## Task-by-Task Assessment

### Task 0: Fix pre-existing proptest failure

- **Readiness: GREEN**
- **What the agent needs to know**: The failing test is `prop_project_name_with_yaml_special_chars` in `crates/jm-core/tests/proptest_roundtrip.rs`. The test generates names with YAML special chars (`'`, `:`, `!`, `?`, `#`, `{}`, `[]`, `@`). The panic is `"String join would overflow memory bounds"` from `libyml` (the serde_yml backend). The minimal failing input is `"aaa  A 0aaa0  A0 A0a0A ? Aa      a"`. The `yaml_string()` helper in `crates/jm-core/src/models/project.rs:597` handles quoting for names with special chars. The `?` character at a specific position triggers a libyml bug when parsing YAML. The fix is either: (a) quote the string in `yaml_string()` when it contains `?` (which it already does -- `?` falls through to the single-quote branch), so the issue is likely in `from_markdown` -> `parse_frontmatter` -> serde_yml parsing of `?`-containing quoted strings; or (b) tighten the proptest regex to avoid triggering a known libyml edge case; or (c) add `?` to the quoting trigger list or use double quotes.
- **Ambiguities**: The design doc says "fix" but does not specify the approach. The agent must decide: fix in `yaml_string()` / `from_markdown()` / proptest regex / or switch to a different YAML quoting strategy. Given the error is in the YAML parser (libyml), the most pragmatic fix is either (a) catching the panic in `from_markdown()` via `catch_unwind`, (b) pre-quoting the name differently, or (c) narrowing the proptest regex. All are valid. The agent has enough info to diagnose and fix.
- **Missing context**: None. The error message, the minimal failing input, and the relevant code (`yaml_string`, `parse_frontmatter`) are all accessible.
- **Estimated complexity**: small

### Task 1: Define new trait hierarchy in `plugins/mod.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: Current `Plugin` trait in `crates/jm-tui/src/plugins/mod.rs` (29 lines). The design doc provides the exact Rust code for `SidebarPlugin`, `ScreenPlugin`, and `PluginAction`. The `render` signature for `ScreenPlugin` uses `Frame` (from `ratatui::Frame`), not `Buffer`. The `key_hints` return type uses `'static` lifetimes in the design doc.
- **Ambiguities**: The plugin-architecture.md reference doc shows `key_hints` returning `Vec<(&str, &str)>` (without `'static`), while plugin-system-rewrite.md specifies `Vec<(&'static str, &'static str)>`. The primary doc (rewrite) wins. Also, the `ScreenPlugin` trait in plugin-architecture.md includes `on_notify(&mut self, _message: &str)` and `needs_timer()` methods, while the rewrite doc's trait also includes them. Minor: the architecture doc mentions an `AnyPlugin` enum wrapper for unified dispatch, but the rewrite doc does not require it for Phase 0. The agent should only implement what the rewrite doc specifies.
- **Missing context**: None. Exact trait signatures are provided.
- **Estimated complexity**: small

### Task 2: Migrate ClockPlugin to `SidebarPlugin`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/plugins/clock.rs` -- currently `impl Plugin for ClockPlugin`. Methods: `name()`, `needs_timer()`, `height()`, `render()`, `on_tick()`, `on_key()`. All methods map 1:1 to `SidebarPlugin` methods. Change `use super::Plugin` to `use super::SidebarPlugin` and rename the impl block.
- **Ambiguities**: None. This is a mechanical rename.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 3: Migrate NotificationsPlugin to `SidebarPlugin`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/plugins/notifications.rs` -- currently `impl Plugin for NotificationsPlugin`. Has all `Plugin` methods plus `on_notify`. All methods map 1:1 to `SidebarPlugin`.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 4: Migrate PomodoroPlugin to `SidebarPlugin`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/plugins/pomodoro.rs` -- currently `impl Plugin for PomodoroPlugin`. Has `name()`, `needs_timer()`, `height()`, `render()`, `on_tick()`, `on_key()`. All methods map 1:1 to `SidebarPlugin`.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 5: Refactor `PluginSidebar` to use `Vec<Box<dyn SidebarPlugin>>`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/plugins/sidebar.rs`. Currently stores `Vec<Box<dyn Plugin>>`. Change to `Vec<Box<dyn SidebarPlugin>>`. The `new()` constructor must be replaced by `new_from(plugins: Vec<Box<dyn SidebarPlugin>>)` -- the construction logic (reading config, instantiating plugins) moves to `PluginRegistry::new()`. The import `use super::Plugin` becomes `use super::SidebarPlugin`. All internal method calls (`plugin.name()`, `plugin.needs_timer()`, etc.) remain identical since `SidebarPlugin` has the same method signatures as `Plugin`.
- **Ambiguities**: The `new()` method currently does config parsing and plugin instantiation. The design says "replace `new()` with `new_from()`." The agent must decide whether to remove `new()` entirely (yes -- the registry takes over) or keep both. The design is clear: remove `new()`, add `new_from()`.
- **Missing context**: None.
- **Estimated complexity**: small

### Task 6: Create `PluginRegistry` in `plugins/registry.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: Design doc provides complete pseudocode for `PluginRegistry` struct, `new()`, `get_screen()`, `get_screen_mut()`, `tick_sidebar()`, `tick_screen()`. The registry must import `Config`, `PluginSidebar`, `SidebarPlugin`, `ScreenPlugin`, and all concrete plugin types. The `new()` body is largely copied from the old `PluginSidebar::new()` (config parsing, plugin instantiation) with the addition of screen plugin registration. The `AboutPlugin` is always registered.
- **Ambiguities**: The tick intervals (1s sidebar, 250ms screen) are described, but the actual interval management is in app.rs's event loop, not in the registry. The registry's `tick_screen()` method is called from app.rs, which must manage its own timer. This is clear from the design.
- **Missing context**: None. Complete pseudocode provided.
- **Estimated complexity**: medium

### Task 7: Add `ScreenId::Plugin(String)` to `events.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/events.rs` line 17: `ScreenId` already derives `Clone` and `PartialEq`. Adding `Plugin(String)` variant is straightforward. Since `String` implements `Clone`, `PartialEq`, and `Debug`, the derives continue to work.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 8: Add `ScreenId::Plugin(_)` wildcard arms to ALL match sites

- **Readiness: YELLOW**
- **What the agent needs to know**: The design doc lists 9 match sites. The agent must find all `match &self.screen` / `match self.screen` patterns in `app.rs` and `keyhints.rs`. Verified sites from code reading:
  1. `render()` -- line 264 match (app.rs)
  2. `handle_key()` -- line 452 match (app.rs)
  3. `current_project()` -- line 474 if-let (app.rs)
  4. `targeted_project_slug()` -- line 485 match (app.rs)
  5. Help modal screen name -- line 564 match (app.rs)
  6. `handle_select()` -- line 779 match (app.rs)
  7. `handle_back()` -- line 796+ (app.rs) -- but this uses `if let` chains, not an exhaustive match
  8. `handle_start_work()` -- line 812 match (app.rs)
  9. `get_hints()` -- line 86 match (keyhints.rs)
  Additionally there are other sites: `handle_command_mode_input()` has `match &self.screen` at lines 2196, 2234, 2261. Also `Action::OpenEditor` at line 636 uses `if let ScreenId::ProjectView(ref slug) = self.screen.clone()`. `Action::DeleteProject` at line 655. `handle_confirm_delete` at line 1856. `should_show_idle_reminder` at line 2088. `PushScreen` at line 721.
- **Ambiguities**: The design doc says "9+ match sites" but the actual number is higher. The doc lists 9 named sites but some are not exhaustive matches (they use `if let` which does not require a new arm). The agent must determine which are exhaustive matches vs. if-let patterns. Only exhaustive matches need a new arm. The if-let patterns (e.g., `handle_back()`, `OpenEditor`, `DeleteProject`) will simply not match `Plugin(_)` and fall through. The agent must audit all sites carefully -- missing even one will cause a compilation error, which is immediately caught, but it could be confusing.
- **Missing context**: The doc says "9+ match sites" without being exhaustive about the "+". An agent will need to compile and see which matches are incomplete. The compiler will catch any misses, so this is self-correcting.
- **Estimated complexity**: medium

### Task 9: Add `Action::OpenPlugin(String)` to `events.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: `crates/jm-tui/src/events.rs` line 55: `Action` enum. Add `OpenPlugin(String)` variant. The enum already has `#[allow(dead_code)]` so no warnings.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 10: Rename `self.plugins` to `self.plugin_registry` in `app.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: The field `pub plugins: PluginSidebar` (line 41) becomes `pub plugin_registry: PluginRegistry`. The design doc says "7 call sites." Actual call sites found by searching `self.plugins`:
  1. Constructor (line 100): `let plugins = PluginSidebar::new(&config);`
  2. Constructor (line 131): `plugins,`
  3. `render()` (line 366): `self.plugins.render(...)`
  4. `handle_key()` (line 445): `self.plugins.handle_key(idx, key);`
  5. `update()` Action::FocusSidebar (line 597): `self.plugins.plugin_count()`
  6. `update()` Action::Tick (line 765): `self.plugins.on_tick()`

  The type import changes from `use crate::plugins::PluginSidebar` to `use crate::plugins::PluginRegistry` (or wherever registry lives). After the rename, calls like `self.plugin_registry.sidebar.render(...)` and `self.plugin_registry.sidebar.handle_key(...)` and `self.plugin_registry.tick_sidebar()`.
- **Ambiguities**: The "7 call sites" in the doc may be slightly off from actual count. The agent must do a project-wide find/replace, which is mechanical.
- **Missing context**: None.
- **Estimated complexity**: small

### Task 11: Handle `Action::OpenPlugin` in `update()`

- **Readiness: GREEN**
- **What the agent needs to know**: In `app.rs` `update()` method, add a match arm for `Action::OpenPlugin(name)`. The design doc provides exact code: look up screen plugin, call `on_enter()`, set `self.screen = ScreenId::Plugin(name)`. Uses `self.plugin_registry.get_screen_mut(&name)`.
- **Ambiguities**: None. Exact code provided.
- **Missing context**: None.
- **Estimated complexity**: small

### Task 12: Modify `handle_back()` to call `on_leave()` for plugin screens

- **Readiness: GREEN**
- **What the agent needs to know**: Current `handle_back()` at line 795 of app.rs. Currently checks for sidebar focus then does `self.screen = ScreenId::Dashboard`. Must add a check before the Dashboard assignment: if current screen is `ScreenId::Plugin(name)`, call `on_leave()` on the plugin. Must use the clone-first pattern (design doc provides exact code) because `self.screen` holds a borrow when we also need `&mut self.plugin_registry`. The existing `handle_back()` also marks review as reviewed -- that logic should remain. The plugin leave should be inserted before the general `self.screen = ScreenId::Dashboard` line.
- **Ambiguities**: None. The design doc provides the exact borrow-checker-safe pattern.
- **Missing context**: None.
- **Estimated complexity**: small

### Task 13: Wire screen plugin rendering into `app.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: In `render()` (line 264 match block), add a `ScreenId::Plugin(ref name)` arm. The design doc specifies: render plugin full-screen, hide sidebar. The sidebar hiding must happen at the layout level -- when screen is `Plugin(_)`, do not split the layout for sidebar. Currently the sidebar split happens at lines 251-260 unconditionally based on `self.sidebar_visible`. The agent must add an additional condition: sidebar is hidden when a screen plugin is active, regardless of `self.sidebar_visible`. The render call is `plugin.render(frame, main_area)` using `Frame` (not `Buffer`). Note: `get_screen()` returns `&dyn ScreenPlugin` which has `render(&self, frame: &mut Frame, area: Rect)`, but `frame` in the render closure is `&mut Frame`, so the agent needs `self.plugin_registry.get_screen(name)` followed by `plugin.render(frame, content_area)`. Important: `self` is borrowed immutably in the render closure (the signature is `fn render(&self, frame: &mut Frame)`), but `get_screen()` returns an immutable reference, so this works.
- **Ambiguities**: The exact mechanism for hiding the sidebar is not spelled out line-by-line. The agent must modify the layout calculation (lines 251-260) to check if the screen is a plugin screen. One approach: `let show_sidebar = self.sidebar_visible && !matches!(self.screen, ScreenId::Plugin(_))`.
- **Missing context**: None. The design doc says "Sidebar is HIDDEN for screen plugins" and provides the render call pattern.
- **Estimated complexity**: small

### Task 14: Wire screen plugin key handling into `app.rs`

- **Readiness: GREEN**
- **What the agent needs to know**: In `handle_key()` (line 452 match block), add `ScreenId::Plugin(ref name)` arm. Must use clone-first pattern because `handle_key()` takes `&mut self` and we need `&mut self.plugin_registry`. Design doc provides exact code: clone name, get_screen_mut, call handle_key, match PluginAction. The `PluginAction::Toast(msg)` should create a `Toast::new(&msg)`. `PluginAction::Back` calls `self.handle_back()`.
- **Ambiguities**: Minor: should the plugin key handling go before or after the sidebar focus check? The sidebar focus check is at line 439 and should remain higher priority. The plugin screen arm should be added in the screen match block at line 452. This is straightforward from the code structure.
- **Missing context**: None.
- **Estimated complexity**: small

### Task 15: Wire key hints for screen plugins into `keyhints.rs`

- **Readiness: YELLOW**
- **What the agent needs to know**: `crates/jm-tui/src/keyhints.rs`, function `get_hints()` at line 65. Currently matches on `ScreenId` variants. Must add `ScreenId::Plugin(ref name)` arm. The problem: `get_hints()` receives `&ScreenId` and `&Focus`, but it does NOT have access to the `PluginRegistry` or the plugin itself. It needs to call `plugin.key_hints()` but has no reference to the plugin.
- **Ambiguities**: The design doc says "Match `ScreenId::Plugin(ref name)` in `get_hints()`, return the plugin's `key_hints()`." But `get_hints()` currently has no access to plugins. The function signature is `fn get_hints(screen: &ScreenId, focus: &Focus, has_modal: bool, is_kanban: bool)`. To return plugin key hints, either: (a) the function signature must change to accept the plugin registry (or a `Vec<(&'static str, &'static str)>` parameter), or (b) the hints must be computed in `app.rs` and passed differently. The agent must decide how to thread this data through. The simplest approach is to add an optional parameter like `plugin_hints: &[(&'static str, &'static str)]` to `get_hints()` and `render()`. This is a design decision not fully specified in the doc.
- **Missing context**: The design doc does not specify how `keyhints.rs` gets access to the plugin's key_hints. The agent must infer a reasonable approach.
- **Estimated complexity**: small

### Task 16: Implement `AboutPlugin` demo

- **Readiness: GREEN**
- **What the agent needs to know**: Create `crates/jm-tui/src/plugins/about.rs`. The design doc provides the complete struct and impl. It implements `ScreenPlugin` directly. Renders "jm v0.1.0" centered with build info. Handles Esc/q to return `PluginAction::Back`. `on_enter()`/`on_leave()` are no-ops. `key_hints()` returns `vec![("Esc", "back")]`. The version can come from `env!("CARGO_PKG_VERSION")` or be hardcoded. Uses ratatui widgets for centered rendering (Paragraph with Alignment::Center, Block, etc.).
- **Ambiguities**: "Build info" is vague -- what to show. The agent can reasonably show the version, the Rust edition, and the build date. This is a demo plugin so the exact content is not critical.
- **Missing context**: None. The design doc provides enough to implement.
- **Estimated complexity**: small

### Task 17: Register `AboutPlugin` in `PluginRegistry`

- **Readiness: GREEN**
- **What the agent needs to know**: In `PluginRegistry::new()` (from Task 6), add `screen_plugins.push(Box::new(AboutPlugin::new()))`. The design doc states "AboutPlugin is always registered (no config needed)." The about module must be declared in `plugins/mod.rs` and the `AboutPlugin` type re-exported or imported in registry.rs.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial

### Task 18: Add `J` keybinding for `AboutPlugin`

- **Readiness: GREEN**
- **What the agent needs to know**: In `crates/jm-tui/src/screens/dashboard.rs` (the dashboard key handler), add `KeyCode::Char('J') => Action::OpenPlugin("about".to_string())`. This is uppercase J (Shift+J), consistent with `I` for IssueBoard and `W` for Weekly.
- **Ambiguities**: The design doc says "In dashboard key handling" but the dashboard key handler is in `crates/jm-tui/src/screens/dashboard.rs`, not `app.rs`. The agent needs to check where `I` and `W` are handled. Looking at the code: `Action::OpenIssueBoard` and `Action::OpenWeekly` are likely returned from `dashboard::handle_key()`. The agent needs to look at `dashboard.rs` to confirm. However, this is easily discoverable from the existing keybinding patterns.
- **Missing context**: The agent needs to read `crates/jm-tui/src/screens/dashboard.rs` to see the key handler, which is not listed in the design doc's file references. This is a minor oversight -- the agent would naturally check it.
- **Estimated complexity**: trivial

### Task 19: Write unit tests for `PluginRegistry` and `AboutPlugin`

- **Readiness: YELLOW**
- **What the agent needs to know**: The design doc says "Test plugin lookup by name, tick behavior, `PluginAction` handling." No specific test file location or test structure is specified. The tests likely go in `crates/jm-tui/src/plugins/registry.rs` as `#[cfg(test)] mod tests { ... }` or in a separate test file. For `PluginRegistry` tests: construct with a default config, verify sidebar plugins are created, verify `get_screen("about")` returns Some. For `AboutPlugin` tests: verify `name()` returns `"about"`, verify `handle_key(Esc)` returns `PluginAction::Back`, verify `key_hints()` is non-empty.
- **Ambiguities**: No test examples provided. The agent must decide: what tests to write, where to put them, how to construct the `Config` for test contexts (the `Config::default()` method exists and includes default plugin config). The `PluginRegistry::new(&Config::default())` should work for tests.
- **Missing context**: No existing test patterns for TUI code -- existing tests are all in `jm-core`. The agent would need to look at test conventions. However, standard Rust inline `#[cfg(test)]` modules are fine.
- **Estimated complexity**: small

### Task 20: Run full test suite -- all tests pass

- **Readiness: GREEN**
- **What the agent needs to know**: `cargo test` runs all tests. The proptest failure from Task 0 must be fixed first. No other known failures. The agent runs `cargo test` and iterates if needed.
- **Ambiguities**: None.
- **Missing context**: None.
- **Estimated complexity**: trivial (assuming Tasks 0-19 are done correctly)

### Task 21: Manual testing -- all sidebar plugins work, About screen works

- **Readiness: YELLOW**
- **What the agent needs to know**: This is a manual verification task. An autonomous agent cannot interact with the TUI. However, it can verify compilation succeeds, tests pass, and the code paths are logically correct through code review.
- **Ambiguities**: "Manual testing" implies a human must run the TUI and verify. An agent can build the binary but cannot click through the UI.
- **Missing context**: This task inherently requires human interaction -- pressing keys, observing renders. The agent can do `cargo build --release` and `./build-install.sh` to produce the binary, then report that manual testing is needed.
- **Estimated complexity**: N/A (human task)

---

## Summary

### Overall readiness score: 18/22 tasks are GREEN

| Task | Name | Readiness | Complexity |
|------|------|-----------|------------|
| 0 | Fix proptest failure | GREEN | small |
| 1 | Define new traits | GREEN | small |
| 2 | Migrate ClockPlugin | GREEN | trivial |
| 3 | Migrate NotificationsPlugin | GREEN | trivial |
| 4 | Migrate PomodoroPlugin | GREEN | trivial |
| 5 | Refactor PluginSidebar | GREEN | small |
| 6 | Create PluginRegistry | GREEN | medium |
| 7 | Add ScreenId::Plugin | GREEN | trivial |
| 8 | Add wildcard match arms | YELLOW | medium |
| 9 | Add Action::OpenPlugin | GREEN | trivial |
| 10 | Rename self.plugins | GREEN | small |
| 11 | Handle Action::OpenPlugin | GREEN | small |
| 12 | Modify handle_back() | GREEN | small |
| 13 | Wire screen plugin rendering | GREEN | small |
| 14 | Wire screen plugin key handling | GREEN | small |
| 15 | Wire key hints | YELLOW | small |
| 16 | Implement AboutPlugin | GREEN | small |
| 17 | Register AboutPlugin | GREEN | trivial |
| 18 | Add J keybinding | GREEN | trivial |
| 19 | Write unit tests | YELLOW | small |
| 20 | Run full test suite | GREEN | trivial |
| 21 | Manual testing | YELLOW | N/A |

### Blocking gaps

None of the YELLOW items are truly blocking -- they represent areas where the agent will need to make minor design decisions or iterate with the compiler:

1. **Task 8 (match arm audit)**: The doc lists 9 sites but the real count is higher. The compiler will catch any missed match arms immediately, making this self-correcting. Not blocking.

2. **Task 15 (key hints threading)**: The `get_hints()` function has no access to the plugin registry. The agent must modify the function signature or pass hints differently. This is a small design decision. Recommendation: add a `plugin_hints: Option<Vec<(&'static str, &'static str)>>` parameter to both `keyhints::render()` and `get_hints()`, computed in `app.rs` where the plugin registry is available.

3. **Task 19 (unit tests)**: No test templates provided. The agent can write standard Rust unit tests. Not blocking.

4. **Task 21 (manual testing)**: Inherently a human task. Not blocking for the agent -- the agent should build the binary and flag this for human verification.

### Recommended pre-work

1. **Read `crates/jm-tui/src/screens/dashboard.rs`** -- needed for Task 18 (J keybinding) and Task 8 (finding all ScreenId match sites in screen modules, not just app.rs). The design doc does not list this file but it is required.

2. **Check for any `match self.screen` or `match &self.screen` patterns in screen modules** beyond `app.rs` and `keyhints.rs` -- if any screen module matches on `ScreenId`, those would also need `Plugin(_)` arms.

3. **Verify `ratatui::Frame` import path** -- the design doc uses `Frame` in the `ScreenPlugin::render` signature. The agent should confirm the correct import: `ratatui::Frame` (ratatui 0.29 re-exports from `ratatui::prelude::*` which is already used throughout the codebase).

4. **For Task 0**: The minimal failing input `"aaa  A 0aaa0  A0 A0a0A ? Aa      a"` contains a `?` character with surrounding whitespace that triggers a libyml scanner bug when the name is single-quoted. The agent should verify whether the `?` handling in `yaml_string()` produces valid YAML by testing the output manually. The likely fix: `yaml_string()` should use double quotes for strings containing `?` followed by a space (which is a YAML flow indicator), or the proptest regex should be narrowed to exclude `?` since it triggers a known upstream bug in libyml.

### Dependency ordering note

Tasks must execute in this order for compilation to succeed at each step:

1. Task 0 (independent, can run first or last)
2. Tasks 1-4 (trait + migration -- must precede 5)
3. Task 5 (sidebar refactor -- must precede 6)
4. Task 6 (registry -- must precede 10, 11)
5. Tasks 7, 9 (enum variants -- must precede 8)
6. Task 8 (wildcard arms -- must follow 7, enables compilation)
7. Tasks 10-14 (app.rs wiring -- must follow 6, 8)
8. Task 15 (key hints -- can follow 8)
9. Tasks 16-17 (AboutPlugin -- can follow 1, 6)
10. Task 18 (keybinding -- must follow 9, 16)
11. Tasks 19-21 (verification -- must follow all above)

The design doc's task numbering already reflects a valid topological order.
