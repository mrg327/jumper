# Phase 0 Adversarial Review: Plugin System Rewrite

Reviewer perspective: senior Rust developer challenging implementation feasibility, migration risks, and Rust-specific gotchas. Taste decisions are not challenged.

---

## 1. There Is No Screen Stack

**Severity: RED Blocker**

The spec repeatedly references `self.screen_stack.push()`, `self.screen_stack.last()`, and `self.screen_stack.pop()` (spec lines 212-252). This data structure does not exist.

The actual app uses a single flat field:

```
// app.rs line 39
pub screen: ScreenId,
```

Screen transitions are flat assignments (`self.screen = ScreenId::Dashboard`, `self.screen = ScreenId::IssueBoard`, etc. — see app.rs lines 516, 547, 551, 557, 587, 592, 724, 783, 804, 838). There is no stack, no push, no pop. The `Action::PushScreen` handler (app.rs line 720-724) simply overwrites `self.screen`:

```rust
Action::PushScreen(ref screen) => {
    if let ScreenId::ProjectView(slug) = screen {
        self.project_view = project_view::init(slug);
    }
    self.screen = screen.clone();
}
```

And `Action::PopScreen` (line 726) delegates to `handle_back()`, which unconditionally sets `self.screen = ScreenId::Dashboard` (line 804).

**Why this matters**: The entire screen plugin lifecycle design assumes stack-based navigation. `on_enter` on push, `on_leave` on pop, returning to the previous screen after closing a plugin — none of this works with a flat `self.screen` field. The `Action::Back` handler (line 795-808) always returns to Dashboard, so pressing Esc in a plugin screen opened from ProjectView would not return to ProjectView.

**Resolution approaches** (pick one):
1. **Introduce an actual screen stack** as a prerequisite task (task 0). Change `pub screen: ScreenId` to `pub screen_stack: Vec<ScreenId>` with `screen_stack.last()` as the current screen. This is a significant refactor touching every `self.screen` assignment and every `match &self.screen` in render/handle_key. Roughly 15+ call sites.
2. **Keep the flat model** and accept that plugin screens always return to Dashboard. This avoids the refactor but limits future composability (e.g., opening a JIRA screen from ProjectView). The spec's `on_leave` hook still works; `on_enter` still works; you just lose "return to previous screen."
3. **Hybrid**: add a `previous_screen: Option<ScreenId>` field specifically for plugin screens. Minimal refactor, but a special case that adds tech debt.

Option 1 is the cleanest but adds significant scope to Phase 0. Option 2 is pragmatic and honest.

---

## 2. Trait Object Split: `on_tick()` and the Notification Forwarding Loop

**Severity: RED Blocker**

### 2a. The current notification forwarding system

In `sidebar.rs` lines 132-166, `on_tick()` does a two-pass operation:

1. **Pass 1**: Iterates `self.plugins` (as `Vec<Box<dyn Plugin>>`), calls `plugin.on_tick()` on each, and collects notification messages. It identifies the NotificationsPlugin by index via `p.name() == "Notifications"` (line 137).
2. **Pass 2**: Forwards collected messages from non-notification plugins to the NotificationsPlugin via `notif_plugin.on_notify(msg)` (lines 157-163).

This works because all plugins live in a single `Vec<Box<dyn Plugin>>`, so the sidebar can iterate them, identify the notification plugin, and forward messages — all through the same trait.

### 2b. After the split

After the rewrite, sidebar plugins are `Vec<Box<dyn SidebarPlugin>>` and screen plugins are `Vec<Box<dyn ScreenPlugin>>`. The `PluginRegistry::on_tick()` (spec line 164-173) calls `self.sidebar.on_tick()` first, then iterates screen plugins calling `screen.on_tick()`.

The problem: **screen plugin notifications are never forwarded to the NotificationsPlugin**. The spec's `on_tick()` returns `Vec<String>` from both, but the forwarding into NotificationsPlugin only happens inside `PluginSidebar::on_tick()`. Screen plugin messages are collected at the registry level and returned, but nobody calls `on_notify()` on the NotificationsPlugin with them.

The `PluginRegistry::on_tick()` as spec'd returns `Vec<String>` — these become toasts in `app.rs` line 764-768. But the NotificationsPlugin's internal list (the persistent notification center) never sees screen plugin messages.

**Resolution**: After collecting screen plugin messages in `PluginRegistry::on_tick()`, forward them into `self.sidebar` via the existing `push_notification()` method (sidebar.rs line 170-182). Add something like:

```rust
for msg in &screen_notifications {
    self.sidebar.push_notification(msg);
}
```

This is straightforward but needs to be explicit in the spec.

### 2c. Trait object coercion

The spec mentions `SidebarPlugin: Plugin` as a supertrait relationship. In Rust, `Box<dyn SidebarPlugin>` cannot be coerced to `Box<dyn Plugin>`. This means you cannot store both sidebar and screen plugins in a single `Vec<Box<dyn Plugin>>` and iterate them uniformly. The spec already accounts for this by keeping them separate — this is fine. But it means **any code that wants to call `Plugin` methods on all plugins must enumerate both collections**. The registry's `on_tick()` does this correctly. Just be aware this pattern must be repeated for any future "broadcast to all plugins" operation.

---

## 3. `PluginSidebar::new()` Responsibilities Must Be Split

**Severity: YELLOW Needs Resolution**

The current `PluginSidebar::new()` (sidebar.rs lines 22-61) does more than just instantiate plugins:

1. Reads `config.plugins.pomodoro` for work/break minutes (line 24)
2. Reads `config.plugins.notifications` for reminders (line 25)
3. Parses `NaiveTime` from reminder strings (lines 28-36)
4. Iterates `config.plugins.enabled` to decide which plugins to instantiate (lines 40-53)
5. Constructs each plugin with the correct config arguments
6. Returns `Self { plugins, focused_idx: 0 }`

The spec says the registry handles instantiation and the sidebar gets a `new_from(plugins: Vec<Box<dyn SidebarPlugin>>)` constructor. This means responsibilities 1-5 must move to `PluginRegistry::new()`. The registry needs access to `Config` and must replicate the reminder-parsing logic.

**Specific concern**: The reminder parsing (converting `Vec<ReminderConfig>` to `Vec<(NaiveTime, String)>`) is currently co-located with plugin creation. When moved to the registry, ensure the `NotificationsPlugin::new()` signature doesn't change — it already takes `Vec<(NaiveTime, String)>`, so the parsing just moves up one level. This is mechanical but must not be forgotten.

**Action item**: The task list should explicitly note that Task 5 (Create PluginRegistry) subsumes the config-reading and parsing logic from Task 6 (Refactor PluginSidebar). These are not independent — Task 6 depends on Task 5 being complete, and Task 5 must include the config parsing logic. The current ordering (5 then 6) is correct, but they should ideally be done as a single atomic commit to avoid a broken intermediate state.

---

## 4. `Focus::Sidebar(usize)` Index Stability

**Severity: GREEN Minor**

The app uses `Focus::Sidebar(idx)` where `idx` is an index into `self.plugins.plugins` (the `Vec<Box<dyn Plugin>>` inside PluginSidebar). After the rewrite, this indexes into `Vec<Box<dyn SidebarPlugin>>` inside PluginSidebar. Since screen plugins live in a separate `Vec` on the registry, the index space is unaffected. A PluginSidebar with 3 sidebar plugins still has indices 0, 1, 2 regardless of how many screen plugins exist.

**No issue here** — the spec's split naturally preserves index correctness. Just calling this out because it looked like a risk on first read and is worth confirming during implementation.

---

## 5. `Action::OpenPlugin(String)` Dispatch Path

**Severity: YELLOW Needs Resolution**

The spec adds `Action::OpenPlugin(String)` to the Action enum. This is dispatched from the dashboard's key handler when a plugin keybinding is pressed (spec line 260-266). The action is then handled in the central `update()` function.

### 5a. Where does `OpenPlugin` get created?

The spec shows it created in the dashboard key handler. But dashboard key handling is in `screens/dashboard.rs`, not `app.rs`. The dashboard's `handle_key()` function returns an `Action`. For the dashboard to emit `Action::OpenPlugin("jira".to_string())`, it needs to know which screen plugins exist and what their keybindings are.

Currently, `dashboard::handle_key()` (in dashboard.rs) takes `(&mut DashboardState, KeyEvent)` and has no access to the plugin registry or config. The keybinding check shown in the spec requires access to `self.plugin_registry`, which lives on `App`, not on `DashboardState`.

**Resolution options**:
1. Put the plugin keybinding check in `app.rs`'s `handle_key()` method, between the sidebar check and the screen dispatch (app.rs line 450-470). This is the path of least resistance — `app.rs` already has access to `self.plugin_registry` and `self.config`.
2. Pass plugin keybinding info into `dashboard::handle_key()`. This changes the function signature and adds coupling.

Option 1 is cleaner and avoids touching the dashboard module. The check would go right before `match &self.screen { ... }` at line 452.

### 5b. Where does `OpenPlugin` get handled?

It must be added as an arm in `update()` (app.rs line 498). The handler should:
1. Check that the named plugin exists in the registry
2. Call `plugin.on_enter()`
3. Set `self.screen = ScreenId::Plugin(name)` (or push to stack if stack is implemented)

**Important**: The `update()` match is exhaustive with a `_ => {}` fallback (line 771). Adding `Action::OpenPlugin(name)` is straightforward, but the exhaustive match means the compiler will not catch a missing arm — you must add it manually.

---

## 6. Keyhints Lifetime Issue for Screen Plugins

**Severity: YELLOW Needs Resolution**

The current `get_hints()` in `keyhints.rs` (line 65) returns `Vec<(&'static str, &'static str)>`. Every hint string is a string literal with `'static` lifetime. This is because the match arms return `vec![("j/k", "nav"), ...]` — all `&'static str`.

The spec defines `ScreenPlugin::key_hints()` as returning `Vec<(&str, &str)>` (spec line 104, architecture doc line 109). The implied lifetime is the lifetime of `&self` — `Vec<(&'a str, &'a str)>` where `'a` is the borrow on the plugin.

**The problem**: `keyhints::render()` currently takes `screen: &ScreenId` and calls `get_hints()` internally. It has no access to the plugin registry. To render plugin hints, it either:

1. Needs access to the `PluginRegistry` (or the specific `ScreenPlugin`) to call `key_hints()`, or
2. The caller (`App::render()`) must call `plugin.key_hints()` and pass the result into `keyhints::render()`.

Option 2 is simpler. But it means changing the `keyhints::render()` signature to accept an optional `Vec<(&str, &str)>` override. The lifetime works out because `App::render()` holds `&self` which borrows the registry and the plugin, so the `&str` references live long enough for the render call.

**Alternatively**: screen plugins could return `Vec<(&'static str, &'static str)>` (with `'static` lifetimes). For a simple plugin like About, this is trivially true since all hints are string literals. For a JIRA plugin, hints would also be literals. If all plugin hints are compile-time strings, change the trait signature to `Vec<(&'static str, &'static str)>` and the existing keyhints infrastructure works without modification — just add a `ScreenId::Plugin(_)` arm that returns the plugin's hints.

**Recommended**: Use `'static` lifetimes in the trait. There is no realistic case where a plugin's keybinding descriptions would be dynamically generated at runtime.

---

## 7. `ScreenId::Plugin(String)` in the Match Arms

**Severity: YELLOW Needs Resolution**

Adding `ScreenId::Plugin(String)` to the `ScreenId` enum will cause **exhaustive match failures** across the codebase. Every `match &self.screen { ... }` must handle the new variant. Let me enumerate every match site:

| File | Line | Context |
|------|------|---------|
| `app.rs` | 264-356 | `render()` — screen rendering dispatch |
| `app.rs` | 452-470 | `handle_key()` — key handling dispatch |
| `app.rs` | 474-479 | `current_project()` |
| `app.rs` | 485-493 | `targeted_project_slug()` |
| `app.rs` | 564-568 | Help modal screen name |
| `app.rs` | 779-791 | `handle_select()` |
| `app.rs` | 795-808 | `handle_back()` |
| `app.rs` | 812-841 | `handle_start_work()` |
| `keyhints.rs` | 86-151 | `get_hints()` |

That is at least **9 match sites** that must handle `ScreenId::Plugin(_)`. The compiler will enforce this (since ScreenId derives no `#[non_exhaustive]`), which is good — but this is easily 50+ lines of changes across multiple files, and each site needs correct behavior:

- **render**: delegate to `plugin.render()`
- **handle_key**: delegate to `plugin.handle_key()`
- **current_project**: return `None`
- **targeted_project_slug**: return `None`
- **Help modal**: use plugin name or "plugin"
- **handle_select**: no-op or delegate
- **handle_back**: call `on_leave()`, return to previous screen
- **handle_start_work**: probably no-op (can't start work from a plugin screen?)
- **get_hints**: return plugin's `key_hints()`

**This is the largest single source of churn in Phase 0**. Task 7 ("Add ScreenId::Plugin(String) to events.rs") is trivially one line, but it immediately breaks compilation until all 9+ match sites are updated. Tasks 9-12 (wiring rendering, key handling, lifecycle, hints) each address a subset of these sites, but the spec doesn't call out the non-obvious sites (`current_project`, `targeted_project_slug`, `handle_start_work`, `handle_select`, Help modal).

**Recommendation**: Add a task "7b: Add `_ => {}` / `_ => None` wildcard arms to all existing ScreenId matches, then incrementally replace with real implementations in tasks 9-12." This allows compilation after task 7 and prevents accidentally missing a match site.

---

## 8. `PluginRegistry` vs `App.plugins` Field Rename

**Severity: YELLOW Needs Resolution**

The app currently has `pub plugins: PluginSidebar` (app.rs line 41). The spec says to replace this with `pub plugin_registry: PluginRegistry`. Every reference to `self.plugins` must change to `self.plugin_registry.sidebar`.

Current references to `self.plugins` in app.rs:
- Line 100: `let plugins = PluginSidebar::new(&config);`
- Line 131: `plugins,`
- Line 366-367: `self.plugins.render(...)`
- Line 445: `self.plugins.handle_key(idx, key);`
- Line 597: `self.plugins.plugin_count()`
- Line 598: `Focus::Sidebar(0)`
- Line 765: `self.plugins.on_tick()`

After the rewrite:
- `self.plugins.render(...)` becomes `self.plugin_registry.sidebar.render(...)`
- `self.plugins.handle_key(...)` becomes `self.plugin_registry.sidebar.handle_key(...)`
- `self.plugins.plugin_count()` becomes `self.plugin_registry.sidebar.plugin_count()`
- `self.plugins.on_tick()` becomes `self.plugin_registry.on_tick()` (registry level, not sidebar)

The tick change is important: the registry's `on_tick()` ticks both sidebar AND screen plugins and handles cross-forwarding. If you accidentally change it to `self.plugin_registry.sidebar.on_tick()`, screen plugins never get ticked.

**Recommendation**: This rename should be a single atomic task/commit. Don't leave intermediate states where `self.plugins` and `self.plugin_registry` coexist.

---

## 9. Task Ordering and Dependency Analysis

**Severity: YELLOW Needs Resolution**

The spec lists 18 tasks. Here is the actual dependency graph:

```
Task 1: Define trait hierarchy (no deps)
  |
  +---> Tasks 2,3,4: Migrate Clock, Notifications, Pomodoro (depend on 1)
  |       |
  |       +---> Task 6: Refactor PluginSidebar to Vec<Box<dyn SidebarPlugin>> (depends on 2,3,4)
  |               |
  |               +---> Task 5: Create PluginRegistry (depends on 6)
  |                       |
  |                       +---> Task 8: Add Action::OpenPlugin (no dep on 5, but needed by 10)
  |                       +---> Task 7: Add ScreenId::Plugin (no dep on 5, but needed by 9,10,11,12)
  |                       |
  |                       +---> Task 9: Wire rendering (depends on 5,7)
  |                       +---> Task 10: Wire key handling (depends on 5,7,8)
  |                       +---> Task 11: Wire lifecycle (depends on 5,7,8)
  |                       +---> Task 12: Wire key hints (depends on 7)
  |                               |
  |                               +---> Task 13: Implement AboutPlugin (depends on 1)
  |                                       |
  |                                       +---> Task 14: Register AboutPlugin (depends on 5,13)
  |                                       +---> Task 15: Add keybinding (depends on 10,14)
  |
  +---> Task 16: Update Config (can be done in parallel with 2-6)
  +---> Task 17: Run tests (depends on all)
  +---> Task 18: Manual testing (depends on 17)
```

**Ordering issue**: The spec lists Task 5 (PluginRegistry) before Task 6 (Refactor PluginSidebar). But the registry takes ownership of the sidebar via `PluginSidebar::new_from()`. The sidebar must be refactored to accept `Vec<Box<dyn SidebarPlugin>>` before the registry can construct it. **Task 6 must come before Task 5**, or they must be done atomically.

**Minimum compilable change**: Tasks 1 + 2 + 3 + 4 + 6 can be done as one commit. This changes the trait and all impls but keeps the same `PluginSidebar::new()` signature (internally creating `Vec<Box<dyn SidebarPlugin>>` instead of `Vec<Box<dyn Plugin>>`). Everything compiles, all tests pass, zero behavioral change.

**Parallelizable**: Tasks 2, 3, 4 are fully independent of each other. Tasks 7 and 8 are independent of each other. Task 13 (AboutPlugin impl) can be written in parallel with tasks 5-12 as long as the trait hierarchy exists.

---

## 10. The AboutPlugin as Architecture Verification

**Severity: YELLOW Needs Resolution**

The spec proposes an AboutPlugin as the demo screen plugin. It verifies:
- Screen plugin registration
- `on_enter()` / `on_leave()` lifecycle
- Key handling returning `Action::Back`
- Key hints rendering
- Full-screen rendering

What it does NOT verify:
- **`on_tick()` for screen plugins**: The AboutPlugin does not override `needs_timer()`, so it returns `false`. No screen plugin ticking is exercised. A timer-based screen plugin (like JIRA with refresh) would be the first to exercise this path, meaning the tick forwarding code in `PluginRegistry::on_tick()` is untested until Phase 1.
- **`on_notify()` for screen plugins**: The AboutPlugin has no reason to receive notifications. The forwarding path from sidebar plugins to screen plugins is untested.
- **Background threads / async work**: The architecture doc describes a background thread pattern (architecture doc lines 256-274). The AboutPlugin doesn't use it, so no validation that `on_leave()` actually has time to clean up threads.
- **Plugin-initiated modals**: The architecture doc mentions `Action::ShowModal(ModalKind)` (architecture doc line 141). The AboutPlugin doesn't test this. Can a screen plugin actually open a modal? The current modal system (`self.modal_stack.push(Modal::...)`) requires constructing specific `Modal` variants. How does a plugin create a modal without access to the modal types?
- **Plugin-initiated toasts**: Can a screen plugin's `handle_key()` return `Action::Toast("message".into())` and have it display? This should work (the update loop handles `Action::Toast`), but the AboutPlugin doesn't test it.
- **Multiple screen plugins**: With only one screen plugin, we don't test name collision handling, keybinding conflicts, or the `get_screen(name)` lookup under multiple entries.

**Recommendation**: Don't add a second demo plugin (that's scope creep), but DO:
1. Add a unit test that constructs a `PluginRegistry` with a mock `ScreenPlugin` that returns `needs_timer() = true` and verify its `on_tick()` is called.
2. Add a manual test step: verify `Action::Toast` works from the AboutPlugin (add a keybinding like `t` that returns `Action::Toast("test")` — this can be removed later).

---

## 11. The `handle_back()` Function and Plugin Lifecycle

**Severity: RED Blocker**

The current `handle_back()` (app.rs lines 795-808):

```rust
fn handle_back(&mut self) {
    if let Focus::Sidebar(_) = self.focus {
        self.focus = Focus::Main;
        return;
    }
    // When leaving the review screen, mark today as reviewed.
    if matches!(self.screen, ScreenId::Review) {
        let _ = self.last_review_store.mark_reviewed_today();
    }
    self.screen = ScreenId::Dashboard;
    self.focus = Focus::Main;
    dashboard::refresh(&mut self.dashboard, &self.project_store);
}
```

This function must be extended to handle `ScreenId::Plugin(name)`:
1. Look up the plugin by name in the registry
2. Call `plugin.on_leave()`
3. Set `self.screen = ScreenId::Dashboard` (or pop stack if implemented)

**The borrow checker issue**: To call `on_leave()`, you need `&mut self.plugin_registry`. But `self.screen` is also borrowed (to match the `Plugin(name)` variant and extract `name`). In Rust, you cannot hold an immutable borrow of `self.screen` while taking a mutable borrow of `self.plugin_registry`.

Concretely:

```rust
// This won't compile:
if let ScreenId::Plugin(name) = &self.screen {
    // `name` borrows self.screen, which borrows self
    if let Some(plugin) = self.plugin_registry.get_screen_mut(name) {
        // get_screen_mut borrows &mut self.plugin_registry, which borrows &mut self
        // ERROR: cannot borrow `self` as mutable because it is already borrowed as immutable
        plugin.on_leave();
    }
}
```

**Resolution**: Clone the name first:

```rust
let plugin_name = if let ScreenId::Plugin(name) = &self.screen {
    Some(name.clone())
} else {
    None
};
if let Some(name) = plugin_name {
    if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
        plugin.on_leave();
    }
}
self.screen = ScreenId::Dashboard;
```

This is a common Rust pattern but easy to miss. The same issue applies in the render path and key handling path — anywhere you match `&self.screen` to extract the plugin name and then need `&mut self.plugin_registry`. The render path uses `&self` throughout so it's fine (no mutable borrow needed). The key handling path in `handle_key()` has the same issue and needs the same clone-first pattern.

---

## 12. `PluginRegistry.screens`: Vec vs HashMap

**Severity: GREEN Minor**

The spec (lines 133-136) defines:

```rust
pub struct PluginRegistry {
    pub sidebar: PluginSidebar,
    pub screens: Vec<Box<dyn ScreenPlugin>>,
}
```

But the architecture doc (line 192) defines:

```rust
pub screens: HashMap<String, Box<dyn ScreenPlugin>>,
```

These are inconsistent. The spec's `get_screen(name)` does a linear scan of the Vec. The architecture doc's HashMap does O(1) lookup.

For the expected number of screen plugins (1-5), the performance difference is irrelevant. The Vec is simpler and avoids duplicating the name (it's already in `plugin.name()`). But the inconsistency between the two docs should be resolved before implementation.

**Recommendation**: Use `Vec<Box<dyn ScreenPlugin>>` as the spec says. It's simpler, and `get_screen()` is called at most once per key event / render frame.

---

## 13. The `ScreenPlugin::render` Takes `&self` — Mutable State Updates During Render

**Severity: GREEN Minor**

Both `SidebarPlugin::render(&self, ...)` and `ScreenPlugin::render(&self, ...)` take `&self`. This means render cannot mutate plugin state. This is consistent with the current design (the existing `Plugin::render(&self, ...)` is also immutable).

However, some TUI patterns require updating derived/cached state during render (e.g., computing visible rows based on terminal height, storing scroll position that depends on content height). If a screen plugin needs this, it must use `Cell` or `RefCell` for interior mutability.

This is fine for Phase 0 (the AboutPlugin has no mutable render state), but worth noting for the JIRA plugin in Phase 1 which will almost certainly need it (scroll position in a board view, visible issue count based on terminal height).

**No action needed now**, but the trait could proactively use `&mut self` for `render()` to avoid `RefCell` in Phase 1. The existing sidebar plugins would need trivial signature changes (add `mut` to the `self` parameter).

---

## 14. Sidebar Visibility During Plugin Screens

**Severity: YELLOW Needs Resolution**

The current render pipeline (app.rs lines 250-260) always splits the layout for the sidebar when `self.sidebar_visible` is true, regardless of which screen is active. This means if a screen plugin is rendered, the sidebar will still appear on the right, consuming 22 columns.

Is this the desired behavior? The spec says screen plugins get "full screen area" (acceptance criteria line 365: "Demo screen renders correctly (full screen area)"). But with the sidebar visible, the plugin only gets `terminal_width - 22` columns.

**Options**:
1. Hide the sidebar automatically when a plugin screen is active
2. Let the sidebar coexist with plugin screens (like it does with all other screens)
3. Let the plugin decide (add `fn wants_sidebar(&self) -> bool` to the trait)

The acceptance criterion says "full screen area", which implies option 1. But this is a taste decision so I won't prescribe — just flagging that the current render pipeline won't give full-screen without a code change, and the spec doesn't address this.

---

## 15. Config Parsing for Screen Plugins: Task 16 Is Underspecified

**Severity: GREEN Minor**

Task 16 says "Update Config struct for screen plugin configuration." The current `Config` struct (in `jm-core/src/config.rs`) has:

```rust
pub struct PluginsConfig {
    pub enabled: Vec<String>,
    pub pomodoro: PomodoroConfig,
    pub notifications: NotificationsConfig,
}
```

Adding screen plugin config (e.g., JIRA URL, keybindings) requires extending this struct. But for Phase 0, the only screen plugin is About, which has no configuration. Task 16 either:
1. Does nothing meaningful (About has no config), or
2. Adds a generic `HashMap<String, serde_yaml::Value>` for future plugin configs, or
3. Is deferred to Phase 1 when JIRA actually needs config.

**Recommendation**: Defer to Phase 1. Don't add dead config infrastructure. The About plugin needs no configuration. The keybinding for About can be hardcoded (as the spec acknowledges on line 268: "Initially, keybindings are hardcoded per plugin").

---

## 16. The `Stores` Pattern and Plugin Data Access

**Severity: GREEN Minor**

The architecture doc (lines 116-128) says screen plugins are "self-contained" and don't access core stores. This is a clean design, but the `app.rs` currently passes stores directly to screen render/handle functions (e.g., `issue_board::render(&self.issue_board_state, &self.project_store, ...)`).

Screen plugins will NOT have this luxury — they get `render(&self, area, buf)` with no stores. This is fine for Phase 0 (About doesn't need stores) and fine for JIRA (it has its own API client), but if a future plugin needs to read local jm data (e.g., a "Project Statistics" screen plugin), the architecture would need revisiting.

No action needed for Phase 0. Just flagging for awareness.

---

## Summary: Recommended Task Ordering

Based on the above analysis, here is the corrected task ordering with dependency annotations:

| Order | Task | Depends On | Notes |
|-------|------|------------|-------|
| **0** | **Decide on screen stack vs flat screen** | — | Blocker: must resolve before tasks 9-11 |
| 1 | Define trait hierarchy in `plugins/mod.rs` | — | |
| 2-4 | Migrate Clock, Notifications, Pomodoro | 1 | Parallelizable |
| 5 | Refactor PluginSidebar to `Vec<Box<dyn SidebarPlugin>>` | 2,3,4 | **Was task 6 in spec** |
| 6 | Create PluginRegistry (move config parsing here) | 5 | **Was task 5 in spec** |
| 7 | Add `ScreenId::Plugin(String)` + wildcard arms | — | Add `_ =>` arms to all 9+ match sites first |
| 8 | Add `Action::OpenPlugin(String)` | — | |
| 9 | Rename `App.plugins` to `App.plugin_registry` | 6 | **New task** — single atomic rename |
| 10 | Wire rendering for screen plugins | 6,7,9 | |
| 11 | Wire key handling (in `app.rs`, not dashboard) | 6,7,8,9 | |
| 12 | Wire lifecycle (on_enter/on_leave with clone pattern) | 6,7,8,9 | Address borrow checker issue |
| 13 | Wire key hints | 7 | Decide on `'static` vs `'a` lifetime |
| 14 | Implement AboutPlugin | 1 | Can be done in parallel with 5-13 |
| 15 | Register AboutPlugin + add keybinding | 6,11,14 | |
| 16 | ~~Update Config~~ | — | **Defer to Phase 1** |
| 17 | Run tests | All | |
| 18 | Manual testing | 17 | |

**Critical path**: Task 0 (stack decision) -> 1 -> 2/3/4 -> 5 -> 6 -> 9 -> 10/11/12 -> 15 -> 17 -> 18
