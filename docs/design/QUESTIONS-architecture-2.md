# Plugin Architecture Review -- Rust Implementation Concerns

Adversarial review of `plugin-architecture.md` focusing on Rust-specific implementation feasibility, borrow checker implications, and gaps between the proposed design and the existing codebase.

---

## 1. Trait Hierarchy and Downcasting

**Severity: 🔴 Blocker**

The design proposes a base `Plugin` trait with `SidebarPlugin: Plugin` and `ScreenPlugin: Plugin` sub-traits, then stores them in a single `PluginRegistry` that owns both. But how does the registry know which concrete sub-trait a `Box<dyn Plugin>` implements?

In Rust you cannot downcast from `dyn Plugin` to `dyn SidebarPlugin` without `Any`. The registry pseudocode in the doc already splits them into two collections at registration time:

```rust
pub struct PluginRegistry {
    pub sidebar: PluginSidebar,                       // Vec<Box<dyn SidebarPlugin>>
    pub screens: HashMap<String, Box<dyn ScreenPlugin>>,
}
```

If they are always stored separately, the base `Plugin` trait serves no runtime purpose -- it is only a shared interface for `name()`, `needs_timer()`, `on_tick()`, and `on_notify()`. There is never a `Vec<Box<dyn Plugin>>` that mixes both kinds.

**Questions:**

- If the two collections are always separate, is the trait hierarchy adding anything beyond code organization? A `SidebarPlugin` and `ScreenPlugin` could simply be two unrelated traits that happen to share some method signatures.
- If you do want a unified tick loop (tick ALL plugins regardless of kind), you need one homogeneous collection. That means either (a) `Vec<Box<dyn Plugin>>` with `kind()` + `Any` downcasting, or (b) an enum wrapper like `enum AnyPlugin { Sidebar(Box<dyn SidebarPlugin>), Screen(Box<dyn ScreenPlugin>) }` and delegate `on_tick()` through the wrapper. Which approach is intended?
- The `PluginKind` enum returned by `fn kind(&self) -> PluginKind` on the base trait implies runtime type discrimination, which is a code smell if you already have separate collections. It only makes sense if you have a single heterogeneous collection and need to branch at call sites. Decide which model you want and cut the other.

**Suggestion:** Drop the base `Plugin` trait entirely. Make `SidebarPlugin` and `ScreenPlugin` two independent traits. Use an `enum AnyPlugin` wrapper for the unified tick/notify loop if needed. This avoids the supertrait footgun and is more idiomatic Rust.

---

## 2. Render Signature Mismatch: `&mut Buffer` vs `&mut Frame`

**Severity: 🔴 Blocker**

The design proposes:

```rust
fn render(&self, area: Rect, buf: &mut Buffer);  // for both SidebarPlugin and ScreenPlugin
```

But every existing screen in the codebase takes `frame: &mut Frame`, not `buf: &mut Buffer`:

- `dashboard::render(state, ..., frame: &mut Frame, area: Rect, ...)`
- `issue_board::render(state, ..., frame: &mut Frame, area: Rect)`
- `weekly::render(state, ..., frame: &mut Frame, area: Rect)`
- `project_view::render(state, ..., frame: &mut Frame, area: Rect)`
- `search::render(state, frame: &mut Frame, area: Rect)`

The existing sidebar plugins use `buf: &mut Buffer` because they are called via `self.plugins.render(sidebar_area, frame.buffer_mut(), ...)` in `app.rs` line 367. This works for small widgets, but `Frame` provides `render_widget()` which handles `StatefulWidget`, scroll state, and other ratatui abstractions that `Buffer` alone does not.

A `ScreenPlugin` that wants to use `frame.render_widget()`, `frame.render_stateful_widget()`, or `frame.area()` cannot do so with just a `&mut Buffer`. You would have to manually call `Widget::render(widget, area, buf)` everywhere, which is more verbose and loses access to Frame-level features.

**Questions:**

- Should `ScreenPlugin::render` take `frame: &mut Frame` instead of `buf: &mut Buffer` to match the existing screen pattern? This would make screen plugins first-class citizens in the rendering pipeline.
- If you use `Frame`, the trait method becomes `fn render(&self, frame: &mut Frame, area: Rect)`, which is not object-safe if `Frame` has a generic lifetime or backend parameter. Check the ratatui 0.29 API: `Frame<'a>` has a single lifetime. This is fine for trait objects as long as you write `fn render(&self, frame: &mut Frame<'_>, area: Rect)`.
- Sidebar plugins can stay with `buf: &mut Buffer` since they are small and never need Frame features.

**Suggestion:** Use `Frame` for `ScreenPlugin::render` and `Buffer` for `SidebarPlugin::render`. This matches the existing codebase split exactly.

---

## 3. `&self` Render vs Interior Mutability

**Severity: 🟡 Needs Resolution**

The design proposes `fn render(&self, ...)` for both plugin types. The existing sidebar plugins also use `fn render(&self, ...)`, so this is consistent for sidebar widgets.

However, screens in the current codebase take state by shared reference (`&DashboardState`, `&IssueBoardState`, etc.) as free function arguments -- they never mutate during render. This works because state mutations happen in `handle_key` and `update`, not during rendering (TEA architecture).

A `ScreenPlugin` that follows the same TEA discipline has no problem with `&self` for render. But what about a JIRA plugin that:
- Checks an `mpsc::Receiver` for background data during render to show a "loading" spinner?
- Updates scroll position during render based on content height?

`mpsc::Receiver::try_recv()` requires `&self` (it is internally synchronized), so checking for background data in `on_tick()` and storing results in the struct (mutated via `&mut self` in `on_tick`) is the correct pattern. Scroll state is updated in `handle_key(&mut self)`, not during render.

**Questions:**

- Is the contract clear that `render(&self)` must be pure -- no side effects, no state mutation? The existing codebase enforces this implicitly through free functions that borrow state immutably. With a trait object, this becomes a harder-to-violate guarantee (you'd need `Cell`/`RefCell` to cheat). Should the doc state this explicitly?
- If a plugin author reaches for `RefCell<T>` inside a plugin struct to mutate in render, that's a design smell. The doc should explicitly state that data fetching happens in `on_tick()` or `on_enter()`, never in `render()`.

---

## 4. `HashMap<String, Box<dyn ScreenPlugin>>` Iteration Order and Borrow Issues

**Severity: 🟡 Needs Resolution**

The design stores screen plugins in `HashMap<String, Box<dyn ScreenPlugin>>`.

**Iteration order:** `HashMap` iteration order is non-deterministic. For screen plugins this may not matter since typically only one screen plugin is active at a time. But if you iterate all screen plugins for `on_tick()` (which you should -- a JIRA plugin might want to refresh data in the background even when not visible), the tick order is unpredictable. Probably fine, but worth stating.

**Borrow checker concern:** The `App` struct owns the `PluginRegistry`. When you need to call `screen_plugin.render()` inside `App::render(&self, ...)`, you need `&self.plugin_registry.screens["jira"]`. When you need to call `screen_plugin.handle_key()`, you need `&mut self.plugin_registry.screens["jira"]`. Both are fine individually.

The real problem arises when `handle_key` returns an `Action` that the `App::update()` function needs to process, and that processing needs mutable access to the same plugin (e.g., `Action::Toast` triggering `on_notify` on the same plugin). The current codebase avoids this because `update()` takes `&mut self` and accesses stores, not the screen that generated the action. But if `on_notify` is added as a cross-plugin communication channel, you'd need `&mut` on plugin A while iterating to find plugin B. The existing sidebar code solves this with the two-pass approach in `PluginSidebar::on_tick()` (lines 132-166 of sidebar.rs) -- collect first, then forward. The same pattern must be used for screen plugins.

**Questions:**

- Should the registry use `IndexMap<String, Box<dyn ScreenPlugin>>` (already a dependency via `jm-core`) for deterministic iteration order?
- How will the tick loop work? Will it iterate over sidebar plugins AND screen plugins? The current `Action::Tick` handler in `app.rs` only calls `self.plugins.on_tick()` (the sidebar). The registry needs a unified tick method.
- Is `on_notify` actually needed for screen plugins? The doc lists it in the lifecycle but gives no concrete use case. If screen plugins are self-contained (no cross-plugin data), `on_notify` on screen plugins is dead code.

---

## 5. Cross-Crate Config Boundary

**Severity: 🟡 Needs Resolution**

Plugin-specific configs (e.g., JIRA URL, email, refresh interval) need to be parsed from `~/.jm/config.yaml`. But the `Config` struct lives in `jm-core`, which must not know about JIRA or any specific plugin.

The current `PluginConfig` struct in `jm-core/src/config.rs` has typed fields for each built-in plugin:

```rust
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub notifications: NotificationsConfig,
    pub pomodoro: PomodoroConfig,
}
```

Adding `pub jira: JiraConfig` to `PluginConfig` in `jm-core` would couple the core crate to TUI plugin knowledge, violating the architecture. There is no `serde_yml::Value` or `HashMap<String, Value>` escape hatch currently.

**Questions:**

- The simplest solution: add a `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` field to `PluginConfig`. This captures any unknown keys as raw YAML values. Each plugin can then deserialize its own config from the `Value`. Does this require adding `serde_yml` as a dependency of `jm-core`? It's already a dependency (see Cargo.toml), so `serde_yml::Value` is available.
- Alternatively, screen plugins could load their own config files from `~/.jm/plugins/<name>.yaml`, completely bypassing the core config. This is simpler but means config is split across two locations.
- A third option: keep `PluginConfig` in `jm-core` as-is. Move the full config parsing for TUI-specific plugins into `jm-tui`. The TUI crate already depends on `jm-core`, so it can re-parse the YAML file with its own extended struct. But then config is parsed twice.

**Suggestion:** The `#[serde(flatten)]` approach with `HashMap<String, serde_yml::Value>` is the most idiomatic serde pattern for this. It requires zero new dependencies and lets each plugin deserialize its own config block from the raw value.

---

## 6. Background Thread Lifecycle

**Severity: 🟡 Needs Resolution**

The design proposes that screen plugins spawn a background thread in `on_enter()` and stop it in `on_leave()`. Communication is via `mpsc::Sender` (commands to the thread) and `mpsc::Receiver` (results from the thread).

**Thread ownership:** The plugin struct owns both the `Sender` (to send commands) and the `Receiver` (to receive results). It also needs a `JoinHandle<()>` to join/abort the thread on leave. The background thread holds the other ends: a `Receiver` for commands and a `Sender` for results.

**Rapid open/close:** If the user opens the JIRA screen (spawning a thread), immediately presses Esc (calling `on_leave`), then reopens it (calling `on_enter` again), you get:

1. First thread may still be running when `on_leave()` is called.
2. `on_leave()` drops the command `Sender`, which causes the thread's `Receiver` to return `Err(RecvError)` on the next `recv()`. The thread must handle this gracefully (exit its loop).
3. `on_enter()` creates new channels and spawns a new thread.
4. The old `JoinHandle` is dropped without `join()`. The old thread is detached and may still be sending HTTP requests.

**Questions:**

- Does the design require `on_leave()` to `join()` the thread (blocking the TUI until the thread finishes)? That defeats the purpose of background work. It would freeze the UI if an HTTP request is in-flight.
- Should there be an `AtomicBool` or `CancellationToken` (from `tokio-util`, but this is sync) pattern? A simple `Arc<AtomicBool>` shared between the plugin and the thread, set to `true` on `on_leave()`, checked by the thread before each operation. This allows cooperative cancellation without blocking.
- Detached threads that outlive the screen are a resource leak. If the user rapidly toggles in and out 10 times, you could have 10 detached threads making API calls. The design needs to specify whether `on_enter()` is idempotent (reuse existing thread if alive) or always spawns fresh.
- `mpsc::Receiver::try_recv()` in `on_tick()` to check for results -- this is fine. But `on_tick()` is only called once per second. Is that fast enough for a responsive UI when data arrives? ratatui redraws at ~10fps (the 100ms poll in `app.rs` line 200), but `on_tick` is gated to 1-second intervals.

**Suggestion:** Use `Arc<AtomicBool>` for cancellation. Make `on_enter()` check if a thread is already alive (via `JoinHandle::is_finished()`) before spawning. Consider checking the `Receiver` in the render loop (via a non-blocking `try_recv`) in addition to `on_tick` for faster UI updates, OR reduce the tick interval for active screen plugins.

---

## 7. Existing Screen Pattern vs Plugin Screens

**Severity: 🔴 Blocker**

This is the most consequential design decision. The existing screens (Dashboard, IssueBoard, Weekly, etc.) follow this pattern:

- **Free functions** in a module: `init()`, `handle_key()`, `render()`, `refresh()`
- **State struct** stored as a field on `App`: `app.issue_board_state`, `app.weekly_state`, etc.
- **Data access** via store references passed as arguments: `render(state, &project_store, frame, area)`
- **Lifecycle** managed by `App::update()`: `app.screen = ScreenId::IssueBoard; app.issue_board_state = issue_board::init(&issue_store);`

Screen plugins in the design follow a completely different pattern:

- **Trait object** in a registry: `Box<dyn ScreenPlugin>`
- **State encapsulated** inside the trait object (not on App)
- **No data access** to core stores (self-contained)
- **Lifecycle** via `on_enter()` / `on_leave()` callbacks

These two systems would coexist in the app. The `App::render()` function currently has a `match self.screen` with 8 arms. Adding `ScreenId::Plugin(name)` adds a 9th arm that delegates to the registry. Similarly, `App::handle_key()` needs a new arm. `App::update()` needs to handle `Action::PushScreen(ScreenId::Plugin(name))` by calling `on_enter()`.

**Questions:**

- The `match self.screen` dispatch in `render()` and `handle_key()` already has access to `&self` / `&mut self` (the full App). Screen plugins in the registry only get `&self` / `&mut self` on the plugin. This means the App must do the dispatch plumbing. Is this acceptable? It's ~10 lines of new code in each of `render()`, `handle_key()`, and `update()`. Not a blocker by itself, but it means the "plugin" is not truly pluggable -- you still need to modify `app.rs` to add the dispatch arm for the `Plugin(_)` variant. Actually, that arm is generic (`Plugin(name)` -> look up in registry), so it only needs to be written once. Fine.
- The bigger question: should existing screens (IssueBoard, Weekly, etc.) eventually be migrated to the `ScreenPlugin` trait? If not, you permanently maintain two parallel screen systems. If yes, the `ScreenPlugin` trait needs to support store access, which contradicts the "self-contained" design. The doc should take a position.
- The keybinding footer (`key_hints`) is currently computed in `keyhints.rs` with a match on `ScreenId`. Plugin screens return their own hints via `fn key_hints() -> Vec<(&str, &str)>`. How do these integrate into the existing `keyhints::render()` function? The doc's `ScreenId::Plugin(String)` variant needs a code path in keyhints that queries the active plugin for its hints.

**Suggestion:** Add a section to the design doc explicitly stating that existing screens remain as-is (free function pattern) and plugin screens are a parallel system for external integrations only. If the intent is to eventually unify, that's a much larger refactor and should be scoped separately.

---

## 8. `ScreenId::Plugin(String)` -- Clone, PartialEq, and Match Ergonomics

**Severity: 🟢 Minor/Nice-to-have**

The existing `ScreenId` derives `Debug, Clone, PartialEq`. Adding `Plugin(String)` preserves all three derives.

However, matching on `ScreenId::Plugin(ref name) if name == "jira"` is less ergonomic than `ScreenId::Jira`. More critically, every time you push a plugin screen, you allocate a `String`. The existing variants are zero-cost enums.

**Questions:**

- Is there a bounded set of screen plugins (known at compile time)? If so, an enum variant per plugin (`ScreenId::Jira`, `ScreenId::GitHub`) avoids the String allocation and gives exhaustive match checking. But this sacrifices the "dynamic" plugin model.
- If dynamic is required, consider `Plugin(&'static str)` if plugin names are always string literals, or `Plugin(PluginId)` where `PluginId` is a newtype around an interned string index. Probably over-engineering for 1-3 plugins.

---

## 9. Plugin Keybinding Registration

**Severity: 🟡 Needs Resolution**

The design says each plugin registers a keybinding in config:

```yaml
plugins:
  jira:
    keybinding: "Ctrl+J"
```

The dashboard's key handler must check for registered plugin keybindings.

**Questions:**

- Who parses `"Ctrl+J"` into a `KeyEvent`? There is no existing keybinding parser in the codebase. The current key handling is all hardcoded `KeyCode::Char('I')` patterns. You need a function `parse_keybinding("Ctrl+J") -> KeyEvent`. This is non-trivial -- modifier combinations (`Ctrl`, `Shift`, `Alt`), special keys (`F1`, `Home`, etc.), and edge cases (`Ctrl+Shift+K`). Is this in scope for the initial implementation?
- What happens if a plugin keybinding conflicts with an existing dashboard keybinding? `Ctrl+J` doesn't conflict today, but there's no validation. A plugin configured with `keybinding: "s"` would shadow the switch-context key.
- Currently, `dashboard::handle_key()` is a free function in `screens/dashboard.rs` that has no access to the plugin registry. To check plugin keybindings, either (a) the dashboard function needs the registry passed as a parameter, or (b) the check happens in `App::handle_key()` before delegating to the screen, or (c) the dashboard returns a new action like `Action::UnhandledKey(KeyEvent)` and the app checks plugin bindings as a fallback. Option (b) is cleanest.

**Suggestion:** Handle plugin keybindings in `App::handle_key()`, after the screen's handler returns `Action::None` (key not consumed). This is a fallback layer. Defer the keybinding parser to the JIRA plugin implementation doc, and hardcode `Ctrl+J` in the initial implementation.

---

## 10. Testing Strategy

**Severity: 🟡 Needs Resolution**

The existing TUI test strategy is:

- **Logic tests only**: `dashboard.rs`, `issue_board.rs`, `switch.rs`, `project_view.rs` all have `#[cfg(test)] mod tests` that test `handle_key()` return values and state mutations.
- **No rendering tests**: No test creates a `Buffer` or `Frame` and asserts on rendered output. No `TestBackend` usage.
- **Core library tests**: `jm-core` has property-based roundtrip tests for models and storage edge case tests.

**Questions:**

- How do you test a `dyn ScreenPlugin`? The `render` method requires a `Rect` and either `Buffer` or `Frame`. You can create a `Buffer::empty(Rect::new(0, 0, 80, 24))` for testing render output, but nobody does this today. Is it worth starting?
- The more testable parts are `handle_key` (returns `Action`) and state changes via `on_enter`/`on_leave`/`on_tick`. These can be tested by constructing the plugin, calling methods, and asserting on returned actions and internal state. This matches the existing pattern.
- For a JIRA plugin with HTTP calls, `handle_key` and `render` tests are straightforward (mock the data, test the UI logic). The background thread and API client need separate unit tests. Is the design expecting integration tests with a mock JIRA server? The existing codebase has no HTTP testing infrastructure.
- `dyn ScreenPlugin` is not `Send` or `Sync` (the traits don't require it). If the background thread pattern requires the plugin to be `Send`, the trait bounds must be `ScreenPlugin: Plugin + Send`. But the plugin is only ever accessed from the TUI thread. The `Sender`/`Receiver` types handle cross-thread communication. So `Send` is not required on the trait itself, only on the data sent through channels.

---

## 11. The `Action` Enum Extensibility

**Severity: 🟢 Minor/Nice-to-have**

The design says screen plugins return `Action` from `handle_key()`. The current `Action` enum is defined in `events.rs` with ~40 variants, all specific to the core app screens.

**Questions:**

- If a JIRA plugin needs a plugin-specific action (e.g., "refresh board", "assign issue"), does it add variants to the `Action` enum? That couples `events.rs` to plugin knowledge. The current workaround visible in the codebase is `Action::Toast(String)` with encoded commands (`"issue_board_set_status:slug:id:status"` on line 747-758 of app.rs). This is fragile string parsing.
- A cleaner approach: `Action::PluginAction(String, Box<dyn Any>)` or a dedicated `Action::PluginCommand { plugin: String, payload: String }`. But `Box<dyn Any>` is not `Clone` or `Debug`, and `Action` derives both.
- For the self-contained plugin model (plugin handles its own state), most plugin key events return `Action::None` (handled internally) or `Action::Back` (close screen). Plugin-specific actions that only affect plugin state stay inside `handle_key(&mut self)`. Only cross-cutting concerns (toast, back, push screen) need to be `Action` variants. This is probably sufficient for v1.

---

## Summary

| # | Issue | Severity |
|---|-------|----------|
| 1 | Trait hierarchy serves no runtime purpose if collections are separate | 🔴 Blocker |
| 2 | `render` takes `Buffer` but screens use `Frame` | 🔴 Blocker |
| 3 | `render(&self)` purity contract should be explicit | 🟡 Needs Resolution |
| 4 | HashMap iteration order + borrow checker for cross-plugin notify | 🟡 Needs Resolution |
| 5 | Plugin-specific config parsing across crate boundary | 🟡 Needs Resolution |
| 6 | Background thread lifecycle on rapid open/close | 🟡 Needs Resolution |
| 7 | Two parallel screen systems (free functions vs trait objects) | 🔴 Blocker |
| 8 | `Plugin(String)` allocation and match ergonomics | 🟢 Minor |
| 9 | Keybinding parsing and conflict resolution | 🟡 Needs Resolution |
| 10 | No rendering test infrastructure; testing dyn trait objects | 🟡 Needs Resolution |
| 11 | Action enum extensibility for plugin-specific commands | 🟢 Minor |
