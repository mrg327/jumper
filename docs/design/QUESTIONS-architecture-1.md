# Plugin Architecture Review: Rust Implementation Concerns

Adversarial review of `docs/design/plugin-architecture.md` against the existing codebase. Each concern references specific files/lines and Rust language constraints.

---

## 1. Trait Object Safety and Sub-Trait Coercion

**Severity: Red - Blocker**

The design proposes a trait hierarchy where `SidebarPlugin: Plugin` and `ScreenPlugin: Plugin`, with the registry holding `Box<dyn SidebarPlugin>` and `Box<dyn ScreenPlugin>`. This is technically object-safe as currently specified -- both sub-traits only add methods with `&self` / `&mut self` receivers and concrete return types. However, there is a **critical coercion problem**.

In Rust, you **cannot** upcast from `Box<dyn SidebarPlugin>` to `Box<dyn Plugin>`. Trait object upcasting (RFC 3324) was stabilized in Rust 1.76, but only for simple cases. The practical issue is: if you hold a `Box<dyn ScreenPlugin>`, you cannot call `Plugin` methods on it through a `dyn Plugin` reference without either:

1. Upcasting (requires `ScreenPlugin: Plugin` to be explicitly listed as a supertrait AND Rust edition/version support), or
2. Duplicating the `Plugin` method calls on the sub-trait objects directly.

The current codebase (`crates/jm-tui/src/plugins/sidebar.rs:16`) uses `Vec<Box<dyn Plugin>>` -- a flat, single-trait approach. The design proposes splitting into two separate trait object collections. This means operations that apply to ALL plugins (like `on_tick()`, `on_notify()`) must be dispatched separately across both collections. The `PluginRegistry` will need to iterate over `sidebar.plugins` AND `screens.values_mut()` for every tick and notification broadcast. This is workable but messy -- and the design document does not acknowledge this duplication.

**Specific question**: The `PluginSidebar::on_tick()` method (`sidebar.rs:132-166`) currently iterates `self.plugins` (all of type `dyn Plugin`) and handles inter-plugin notification forwarding in a single pass. With the split, how does a screen plugin's `on_tick()` notification get forwarded to the sidebar's `NotificationsPlugin`? The registry must bridge both collections. Will the `PluginRegistry` own the notification forwarding logic, or does `PluginSidebar` still do it? This needs explicit design.

**Suggestion**: Consider keeping a single `Vec<Box<dyn Plugin>>` for the base trait operations (tick, notify) and using `downcast` or a `kind()` discriminant to dispatch to sub-trait-specific rendering/key handling. Alternatively, accept the duplication and explicitly document the two-pass tick/notify pattern in the registry.

---

## 2. Borrow Checker Conflict: Mutable Key Handling vs Immutable Rendering

**Severity: Red - Blocker**

The current `App` struct (`app.rs:27-82`) holds plugins as `pub plugins: PluginSidebar`. The render method (`app.rs:243`) takes `&self`, and the key handler (`app.rs:432`) takes `&mut self`. These never overlap because the event loop (`app.rs:198-204`) calls `render()` then `handle_key()` sequentially.

The design proposes `PluginRegistry` holding `HashMap<String, Box<dyn ScreenPlugin>>` for screen plugins. When a `ScreenId::Plugin(name)` screen is active:

- **Rendering** (`render()`, takes `&self` on `App`): needs `&self.registry.screens[name]` to call `plugin.render()`.
- **Key handling** (`handle_key()`, takes `&mut self` on `App`): needs `&mut self.registry.screens[name]` to call `plugin.handle_key()`.

This works fine with the current sequential `render()` -> `handle_key()` pattern. **But** there is a subtler problem: the `handle_key()` method on line 432 borrows `&mut self` (the entire `App`). Inside that method, matching on `self.screen` (an immutable borrow of `self.screen`) while also needing `&mut self.registry.screens` creates a conflict if not structured carefully.

Look at the current pattern (`app.rs:452-469`):
```rust
match &self.screen {
    ScreenId::Dashboard => dashboard::handle_key(&mut self.dashboard, key),
    ScreenId::ProjectView(_) => { ... }
    ...
}
```

This works because each arm borrows a **different field** of `self`. But for `ScreenId::Plugin(name)`, you need to:
1. Read `name` from `&self.screen` (borrows `self.screen`)
2. Get `&mut self.registry.screens[name]` (borrows `self.registry`)

Since `self.screen` and `self.registry` are disjoint fields, this **should** work with Rust's field-level borrow splitting. But `HashMap::get_mut()` takes a `&str`, and `name` is borrowed from `self.screen`. The borrow checker should be fine here because they're disjoint fields, but it depends on the exact code structure. If the `match` arm captures `name` by reference and then indexes into `self.registry.screens`, it will work. If any intermediate step borrows `self` as a whole, it won't.

**Specific question**: What happens when `handle_key()` returns an `Action::PushScreen(ScreenId::Plugin(other_name))`? The `update()` method (`app.rs:498`) would need to call `on_leave()` on the current screen plugin AND `on_enter()` on the new one. That requires two mutable borrows into `self.registry.screens` simultaneously -- which is impossible with `HashMap`. You'd need to either:
- Clone the screen name out of `self.screen` before accessing the registry
- Use `split_at_mut`-style tricks (not available for HashMap)
- Temporarily remove the plugin from the HashMap, call `on_leave()`, re-insert, then get the next one

This plugin-to-plugin navigation case needs an explicit solution.

---

## 3. The `Action` Return Type is a Closed Enum in jm-tui

**Severity: Yellow - Needs Resolution**

`ScreenPlugin::handle_key()` returns `Action` (`events.rs:55-147`). This `Action` enum is defined in `jm-tui`, not `jm-core`. Every plugin-specific action must be added as a variant to this enum.

Currently, the `Action` enum has ~40 variants, all tightly coupled to existing screens (e.g., `CycleIssueStatus`, `PinIssue`, `EditTags`). If the JIRA plugin needs actions like "refresh board", "open issue detail", "post comment", these must either:

1. **Be added to `Action`** -- bloating a TUI-framework enum with plugin-specific concerns. Every new plugin adds variants.
2. **Use existing generic actions** -- `Action::SubmitInput(String)` with string parsing (the design already has `Action::Toast(String)` as precedent). This is brittle.
3. **Be handled internally** -- the plugin handles its own state transitions and only returns `Action::None`, `Action::Back`, or `Action::Toast(...)`.

Option 3 is almost certainly the right answer and is implied by the "self-contained" design. But the design document should be explicit about this. The current `Action` enum does NOT have any `PluginAction(String, Box<dyn Any>)` or similar extensibility variant, and adding one would break the `Clone` derive on `Action` (since `Box<dyn Any>` is not `Clone`).

**Specific question**: If screen plugins are truly self-contained and only emit `Back`/`Toast`/`None`, then why does the trait return `Action` at all instead of a smaller `PluginAction` enum with just `{ None, Back, Toast(String) }`? Returning the full `Action` enum gives plugins the power to emit `Action::Quit`, `Action::SwitchContext`, `Action::PushScreen(...)`, etc. -- is this intentional? If so, it is a capability that should be explicitly stated. If not, a narrower return type prevents accidental misuse and avoids coupling plugins to the TUI's internal action vocabulary.

---

## 4. Screen Navigation is Not a Stack -- `on_enter`/`on_leave` Semantics are Misleading

**Severity: Yellow - Needs Resolution**

The design describes `on_enter()` / `on_leave()` and references a "screen stack" (`plugin-architecture.md:238-241`). But the actual codebase does NOT use a screen stack. It uses a **single `screen: ScreenId` field** (`app.rs:39`). Navigation is a flat replacement:

```rust
// app.rs:724
Action::PushScreen(ref screen) => {
    self.screen = screen.clone();
}
// app.rs:726
Action::PopScreen => self.handle_back(),
```

`handle_back()` (`app.rs:795-808`) unconditionally sets `self.screen = ScreenId::Dashboard`. There is no stack to pop. The `PushScreen` action just replaces the current screen; `PopScreen` always goes to Dashboard.

This means:
- If you navigate Dashboard -> ProjectView -> JIRA Plugin, pressing Back from JIRA goes to **Dashboard**, not back to ProjectView.
- `on_leave()` is only called when the plugin screen is navigated away from. But the design says "pushed onto the screen stack" and "popped from the screen stack" -- these are lies. There is no stack.

**Specific question**: Will the plugin rewrite introduce an actual screen stack, or will it maintain the flat-replacement model? If flat replacement stays, the `on_enter()`/`on_leave()` lifecycle is straightforward (enter on PushScreen, leave on any navigation away). But the design document's stack language is misleading and should be corrected to match the implementation. If an actual stack IS introduced, that's a much larger refactor of the entire screen navigation system, not just the plugin layer.

**Panic safety**: The design asks "what happens if the app panics between enter and leave?" In a single-threaded TUI with no stack, this is moot -- a panic unwinds the entire app. There's no partial state to corrupt. If `on_enter()` spawns a background thread (per the async pattern), the thread will be orphaned on panic, but Rust's drop semantics (if the plugin implements `Drop`) will handle channel cleanup. This is a non-issue as long as plugins don't hold external locks or file handles that need explicit cleanup beyond `Drop`.

---

## 5. Background Thread Pattern: `std::thread` + `mpsc` Blocks on `try_recv`, Not `recv`

**Severity: Yellow - Needs Resolution**

The design (`plugin-architecture.md:256-274`) describes an `mpsc::channel` pattern where the TUI thread sends commands and the background thread sends results. The plugin checks for results in `on_tick()`.

This is a well-established pattern, but the design omits critical details:

**Threading model**: The codebase is fully synchronous -- no tokio, no async runtime. The event loop (`app.rs:200`) uses `crossterm::event::poll()` with a 100ms timeout, and ticks fire every 1 second. This means:

1. The plugin must use `std::sync::mpsc::Receiver::try_recv()` (non-blocking), NOT `recv()` (blocking). `recv()` in `on_tick()` would freeze the entire TUI until data arrives. This is obvious to a Rust developer, but the design should state it explicitly because a blocking `recv()` would be a catastrophic bug with no compile-time warning.

2. **Channel direction**: The design shows bidirectional communication (TUI -> Background for commands, Background -> TUI for results). This requires **two** channels, not one. Or a single channel with an enum that carries both commands and results. The diagram is ambiguous.

3. **Thread ownership**: If `on_enter()` spawns `std::thread::spawn()`, the `JoinHandle` must be stored in the plugin struct. `on_leave()` must signal the thread to stop (via a channel message, `AtomicBool`, or dropping the `Sender`) and then call `join()`. But `join()` is blocking -- if the background thread is stuck in a network call, `on_leave()` will block the TUI. This needs a timeout or a fire-and-forget `drop()` of the handle.

4. **Thread respawn**: If the user navigates to the plugin screen, leaves, and returns, `on_enter()` is called again. If the previous thread is still running (slow network), do you spawn a second thread? The plugin needs guard logic.

**Specific question**: Should the design mandate `try_recv()` and explicitly address the thread lifecycle (spawn, signal, join-or-abandon)? A concrete pattern (e.g., "use an `Arc<AtomicBool>` for shutdown signaling, store `Option<JoinHandle>`, set the flag and drop the handle on leave") would prevent implementation mistakes.

---

## 6. Config Extensibility: `PluginConfig` is a Closed Struct in jm-core

**Severity: Yellow - Needs Resolution**

The `Config` struct (`crates/jm-core/src/config.rs:113-133`) is in `jm-core`. The `PluginConfig` struct (`config.rs:8-15`) has explicitly typed fields:

```rust
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub notifications: NotificationsConfig,
    pub pomodoro: PomodoroConfig,
}
```

Adding a JIRA plugin requires adding a `pub jira: JiraConfig` field to this struct -- **in `jm-core`**, not in `jm-tui`. This contradicts the "self-contained" plugin philosophy. Every new screen plugin forces a change to the core library's config types.

**serde behavior**: With `#[serde(default)]`, unknown YAML keys are silently ignored by default. So adding `jira:` config to `config.yaml` will not cause an error -- it will just be ignored until the struct is updated. But existing users with unknown keys won't get warnings either.

**Alternative approaches**:
1. **`serde_yml::Value` catch-all**: Add `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig`. Plugins deserialize their config from the raw Value at registration time. This keeps `jm-core` unchanged.
2. **Move plugin config to jm-tui**: Only `jm-tui` knows about plugins; the core crate shouldn't own plugin config. But the current architecture has `Config` in `jm-core` and the TUI reads it.
3. **Per-plugin config files**: Each plugin reads its own `~/.jm/plugins/<name>.yaml`. Clean separation but adds file proliferation.

**Specific question**: The design shows `jira` config under the `plugins:` key in `config.yaml`. How is `JiraConfig` deserialized? Does the `PluginConfig` struct grow a new field for every plugin, or is there an extension mechanism? Option 1 (flatten + HashMap) is the lowest-friction Rust solution.

---

## 7. `ScreenId::Plugin(String)` -- String Dispatch Performance and Equality Semantics

**Severity: Green - Minor/Nice-to-have**

The design proposes `ScreenId::Plugin(String)` for dynamic screen identification. The current `ScreenId` enum (`events.rs:17-26`) uses concrete variants with no heap allocation (except `ProjectView(String)` and `Switch(Option<String>)`).

Adding `Plugin(String)` means:
- **Every `match` on `ScreenId`** must handle the `Plugin(String)` arm. There are ~30 match sites in `app.rs` alone.
- **`PartialEq`** on `ScreenId` already derives, so `ScreenId::Plugin("jira".to_string()) == ScreenId::Plugin("jira".to_string())` works correctly. No issue here.
- **Performance**: String comparison for dispatch is negligible -- this is a TUI running at 10fps max, not a hot loop. Not a real concern.
- **`Clone`**: `String::clone()` allocates. In the current code, `self.screen.clone()` is called in several places (e.g., `app.rs:636`). Adding `Plugin(String)` means those clones now potentially allocate. Still negligible for a TUI.

**The real concern** is ergonomic, not performance: every existing `match &self.screen` block must add a `ScreenId::Plugin(name) => { ... }` arm. The design should enumerate which match sites need modification:
- `render()` -- delegate to `registry.screens[name].render()`
- `handle_key()` -- delegate to `registry.screens[name].handle_key()`
- `handle_back()` -- call `on_leave()` before navigating away
- `update()` for `PushScreen(Plugin(..))` -- call `on_enter()`
- Key hints footer -- call `plugin.key_hints()`
- Export -- unclear how plugin screens export

This is not a blocker but is implementation scoping work the design should acknowledge.

---

## 8. Who Calls `on_tick()` for Screen Plugins That Aren't Currently Visible?

**Severity: Yellow - Needs Resolution**

The current tick mechanism (`app.rs:764-768`) calls `self.plugins.on_tick()`, which ticks ALL sidebar plugins regardless of visibility. This is correct -- the Pomodoro timer must keep counting even when the sidebar is hidden.

For screen plugins: should `on_tick()` be called when the plugin screen is NOT active? Consider:
- A JIRA plugin with `needs_timer() -> true` wants to poll its `mpsc::Receiver` for API results.
- The user is on the Dashboard, not the JIRA screen.
- If `on_tick()` is not called, incoming API results pile up in the channel buffer unconsumed. When the user navigates back, `on_tick()` must drain a potentially large backlog.
- If `on_tick()` IS called while inactive, the plugin continues consuming resources (and the results update internal state that nobody is looking at).

The design's lifecycle section (`plugin-architecture.md:207-214`) lists tick as step 4, between render and key events, but doesn't say whether ticking continues when the screen is not active. The background thread pattern (step 5: "Thread is stopped on `on_leave()`") implies NO ticking when inactive, since the thread is stopped. But what if the user wants background refresh? E.g., JIRA data stays fresh even when viewing the dashboard.

**Specific question**: Is `on_tick()` called for screen plugins only when their screen is active, or always? If only when active, does `on_enter()` trigger a full data refresh? If always, the registry tick loop must iterate both sidebar and screen plugins.

---

## 9. The `render()` Method Signature Mismatch: `Widget` vs Direct Buffer

**Severity: Green - Minor/Nice-to-have**

Both `SidebarPlugin::render()` and `ScreenPlugin::render()` take `(area: Rect, buf: &mut Buffer)`. This matches the current `Plugin::render()` signature (`plugins/mod.rs:22`).

But the main screen rendering in `app.rs` uses `Frame`, not raw `Buffer`:
```rust
fn render(&self, frame: &mut Frame) {
    // ...
    dashboard::render(..., frame, content_area, ...);
}
```

The existing sidebar rendering works because `PluginSidebar::render()` takes `(area, buf, ...)` and is called with `frame.buffer_mut()` (`app.rs:367`). Screen plugins would follow the same pattern. This is fine, but screen plugins won't have access to `Frame::area()` to know the terminal size, or `Frame::render_widget()` / `Frame::render_stateful_widget()` helpers. They must compose widgets directly into the buffer.

This is a minor ergonomic concern -- all the existing screen modules (dashboard, project_view, etc.) receive `Frame` and use its convenience methods. Screen plugins would be second-class citizens, working with raw buffers. Consider whether `ScreenPlugin::render()` should take `&mut Frame` and `Rect` instead.

**Counterargument**: Giving plugins `&mut Frame` exposes them to rendering outside their designated area, which the buffer+rect pattern prevents. The `(Rect, &mut Buffer)` signature is actually safer and matches ratatui's `Widget` trait. This is a deliberate constraint, not a bug.

---

## 10. Missing: How Does the Dashboard Discover Plugin Keybindings?

**Severity: Yellow - Needs Resolution**

The design (`plugin-architecture.md:246-252`) says each plugin registers a keybinding in config (e.g., `keybinding: "Ctrl+J"`). The dashboard key handler "checks for registered plugin keybindings and opens the corresponding screen."

But `dashboard::handle_key()` (`app.rs:453`) is a free function in `crates/jm-tui/src/screens/dashboard.rs` that takes `(&mut DashboardState, KeyEvent)` and returns `Action`. It has no access to the `PluginRegistry`, `Config`, or the list of registered keybindings.

For this to work, one of:
1. **Dashboard state grows a plugin keybinding map** -- passed in at init time from config.
2. **The App's `handle_key()` checks plugin keybindings BEFORE delegating to the screen handler** -- this is more natural, since the App owns the registry.
3. **Plugin keybindings are hardcoded in the `ScreenId` match** -- defeats the config-driven design.

Option 2 is cleanest: in `App::handle_key()`, after the modal check and before the screen dispatch, check if the key matches any registered plugin keybinding and return `Action::PushScreen(ScreenId::Plugin(name))`. But the design doesn't specify this, and the current key handling flow (`app.rs:432-471`) has no such hook point.

**Specific question**: Where in the key handling pipeline do plugin keybindings get checked? This determines whether plugins can be opened from ANY screen or only the Dashboard.

---

## Summary

| # | Issue | Severity |
|---|-------|----------|
| 1 | Trait object upcasting and dual-collection tick/notify dispatch | Red - Blocker |
| 2 | Mutable borrow conflict for plugin-to-plugin screen navigation | Red - Blocker |
| 3 | `Action` enum is closed; plugin return type is over-broad | Yellow - Needs Resolution |
| 4 | No screen stack exists; on_enter/on_leave semantics need clarification | Yellow - Needs Resolution |
| 5 | Background thread lifecycle (try_recv, shutdown, respawn) under-specified | Yellow - Needs Resolution |
| 6 | `PluginConfig` in jm-core must grow for every plugin | Yellow - Needs Resolution |
| 7 | `ScreenId::Plugin(String)` match exhaustiveness across ~30 sites | Green - Minor |
| 8 | Tick behavior for inactive screen plugins unspecified | Yellow - Needs Resolution |
| 9 | `render(Rect, &mut Buffer)` vs `render(&mut Frame)` for screen plugins | Green - Minor |
| 10 | Plugin keybinding discovery in dashboard key handler | Yellow - Needs Resolution |
