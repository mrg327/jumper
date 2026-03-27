# Phase 0 Review: Plugin System Rewrite — Adversarial Findings

Reviewer context: adversarial Rust review of `docs/design/plugin-system-rewrite.md` and `docs/design/plugin-architecture.md`, cross-referenced against the full existing codebase.

---

## 1. There Is No Screen Stack — The Spec Assumes One

**Severity: RED Blocker**

The spec (lines ~212-253 of `plugin-system-rewrite.md`) uses `self.screen_stack.last()`, `self.screen_stack.push()`, and `self.screen_stack.pop()` throughout. These do not exist.

The actual implementation in `crates/jm-tui/src/app.rs:39` is:

```rust
pub screen: ScreenId,  // single field, NOT a Vec/stack
```

Every screen transition in the codebase is a direct assignment: `self.screen = ScreenId::Dashboard` (app.rs:804), `self.screen = ScreenId::Switch(None)` (app.rs:516), `self.screen = ScreenId::IssueBoard` (app.rs:587), etc. The `handle_back()` method at app.rs:794-808 unconditionally resets to `ScreenId::Dashboard` — it doesn't pop anything.

There IS a `modal_stack: Vec<Modal>` (app.rs:40) which IS a real stack, but this is separate from screen navigation.

**Impact**: The entire "screen stack integration" section of the spec (Section 5) needs rethinking. Two approaches:

1. **Keep single `screen` field**: Plugin screens work exactly like existing screens — `self.screen = ScreenId::Plugin(name)`, and `handle_back()` returns to Dashboard. Simple, consistent with existing code. But this means you cannot navigate PluginScreen -> ProjectView -> back-to-PluginScreen — the intermediate screen replaces the plugin.

2. **Actually introduce a screen stack**: Convert `pub screen: ScreenId` to `pub screen_stack: Vec<ScreenId>`. This requires touching every single `self.screen = ...` assignment in app.rs (at least 14 call sites, grep shows 30+ `self.screen` references). Much larger blast radius than the plugin change itself. Every `match &self.screen` becomes `match self.screen_stack.last()`.

**Mitigation**: For Phase 0, use approach (1). Add a screen stack later if needed. The spec's pseudocode must be rewritten to match the single-field pattern.

---

## 2. `self.plugins` Blast Radius — 4 Direct Access Sites

**Severity: YELLOW Needs Resolution**

Renaming `self.plugins: PluginSidebar` to `self.plugin_registry: PluginRegistry` touches every access to `self.plugins` in app.rs. Exact sites:

| Line | Code | What Changes |
|------|------|-------------|
| app.rs:41 | `pub plugins: PluginSidebar` | Field declaration |
| app.rs:100 | `let plugins = PluginSidebar::new(&config)` | Constructor |
| app.rs:131 | `plugins,` | Struct init |
| app.rs:366-367 | `self.plugins.render(sidebar_area, frame.buffer_mut(), ...)` | Render call |
| app.rs:445 | `self.plugins.handle_key(idx, key)` | Key handling |
| app.rs:597 | `self.plugins.plugin_count() > 0` | Sidebar focus guard |
| app.rs:765 | `self.plugins.on_tick()` | Tick dispatch |

That's 7 sites total. With the `PluginRegistry` wrapper that delegates sidebar operations to `self.plugin_registry.sidebar`, these become:

- `self.plugin_registry.sidebar.render(...)`
- `self.plugin_registry.sidebar.handle_key(...)`
- `self.plugin_registry.sidebar.plugin_count()`
- `self.plugin_registry.on_tick()` (registry handles both types)

The import at app.rs:20 (`use crate::plugins::PluginSidebar`) also needs updating to include `PluginRegistry`.

**Mitigation**: This is manageable (7 call sites). But the task list (Section "Tasks") does not have an explicit task for "Replace `self.plugins` with `self.plugin_registry` in app.rs" — it only says "Wire screen plugin rendering" etc. Add a dedicated refactor task that handles all 7 sites atomically. Any halfway state will not compile.

---

## 3. Render Signature Mismatch: `frame: &mut Frame` vs `area: Rect, buf: &mut Buffer`

**Severity: YELLOW Needs Resolution**

The proposed `ScreenPlugin` trait uses:

```rust
fn render(&self, area: Rect, buf: &mut Buffer);
```

But ALL existing screens render with `frame: &mut Frame`:

- `dashboard.rs:99-107`: `fn render(..., frame: &mut Frame, area: Rect, ...)`
- `search.rs:54`: `fn render(state: &SearchState, frame: &mut Frame, area: Rect)`
- `weekly.rs:199`: `fn render(state: &WeeklyState, frame: &mut Frame, area: Rect)`
- Every modal: `fn render(&self, frame: &mut Frame, area: Rect)`

The existing sidebar plugins (Clock, Pomodoro, Notifications) DO use `fn render(&self, area: Rect, buf: &mut Buffer)` — this matches the `SidebarPlugin` trait. So the existing `Plugin::render` -> `SidebarPlugin::render` migration is clean.

For screen plugins, you can bridge `Frame` and `Buffer` via `frame.buffer_mut()`, which is exactly what app.rs:367 already does for sidebar rendering:

```rust
self.plugins.render(sidebar_area, frame.buffer_mut(), sidebar_focused, focused_idx);
```

And the spec's Section 5 even shows this bridge:

```rust
plugin.render(main_area, frame.buffer_mut());
```

**However**: Receiving `buf: &mut Buffer` instead of `frame: &mut Frame` means screen plugins cannot call `frame.render_widget()` — they must use `Widget::render(widget, area, buf)` directly. This is a real constraint:

- `frame.render_widget(widget, area)` is the idiomatic ratatui pattern and what every existing screen uses.
- `Widget::render(self, area, buf)` consumes `self` — the explicit `Widget::render(widget, area, buf)` form works but is less ergonomic.
- More critically: `frame.set_cursor_position()` is NOT available through `buf`. If a screen plugin ever needs a cursor (e.g., text input for a JIRA search box), it cannot set it through the trait's render signature.

**Mitigation**: Either:
(a) Accept the limitation for Phase 0 (the demo AboutPlugin won't need a cursor), but document that Phase 1 JIRA plugin may need the trait signature changed to `fn render(&self, frame: &mut Frame, area: Rect)` to match existing screens. This would be a breaking trait change.
(b) Use `fn render(&self, frame: &mut Frame, area: Rect)` from the start. This is consistent with existing screens and future-proof. It means sidebar plugins and screen plugins have different render signatures, which is fine — they're different traits.

Recommendation: (b). The cost is zero (it's a new trait), and it avoids a breaking change in Phase 1.

---

## 4. Modal Handling from Screen Plugins: Unresolved Control Flow

**Severity: RED Blocker**

The architecture doc (plugin-architecture.md:137-143) lists `ShowModal(ModalKind)` as a future Action. The Phase 0 spec does NOT include `Action::ShowModal` but the JIRA plugin (Phase 1) will need modals for:

- Issue detail view
- Status transitions (pick from a list)
- Comment/note entry
- Error confirmation dialogs

Currently, modals are tightly coupled to App:

- `self.modal_stack` is `Vec<Modal>` where `Modal` is an enum of `Input`, `Select`, `Confirm`, `Help` (see `crates/jm-tui/src/modals/mod.rs`)
- Modal results flow through `Action::SubmitInput(String)` -> `handle_submit_input()` which dispatches based on `InputAction`/`SelectAction` enums (app.rs:1251-1277)
- The `InputAction` and `SelectAction` enums are defined in the modals module and contain hardcoded variants like `QuickNote`, `AddIssue`, `PickIssueToCycle`, etc.

**The fundamental problem**: When a screen plugin requests a modal via `Action::ShowModal(...)`, the App opens the modal. The user types input and presses Enter. The modal produces `Action::SubmitInput(text)`. The App's `handle_submit_input()` looks at the `InputAction` to route the result. But how does the result get BACK to the plugin?

Options:
1. Plugin-specific `InputAction` variants (e.g., `InputAction::JiraComment`) — breaks "self-contained" plugin design, requires modifying the modals module for every plugin.
2. A callback/closure system — not object-safe, fight with lifetimes.
3. Generic `InputAction::PluginInput { plugin_name: String, context: String }` — the App routes `SubmitInput` back to the named plugin. This works but needs to be designed now.
4. Screen plugins manage their own modals internally, rendering them in their own `render()` call and handling keys before returning `Action` — fully self-contained but duplicates modal infrastructure.

**Phase 0 implication**: The demo `AboutPlugin` doesn't need modals, so this CAN be deferred. But Phase 1 WILL hit this immediately. The spec should explicitly acknowledge this gap and state which approach Phase 1 will take, so Phase 0's trait design doesn't accidentally make the chosen approach impossible.

**Mitigation**: Add to the Phase 0 spec: "Screen plugins that need modal input should manage their own inline modal state and render it within their `render()` call. The App's modal system is not exposed to plugins." This aligns with "self-contained" and avoids coupling. Document this as a design decision, not an oversight.

---

## 5. Config Deserialization: Adding Plugin Config to jm-core's Config Struct

**Severity: YELLOW Needs Resolution**

The architecture doc shows JIRA config under `plugins.jira` in config.yaml. The existing `PluginConfig` struct (`crates/jm-core/src/config.rs:7-15`) has typed fields:

```rust
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub notifications: NotificationsConfig,
    pub pomodoro: PomodoroConfig,
}
```

There is NO `#[serde(flatten)]` or `HashMap<String, Value>` catch-all. Adding `pub jira: JiraConfig` would:

1. Require jm-core to depend on JIRA-specific types — violates "self-contained" plugin design
2. Mean every new screen plugin requires a jm-core change
3. Fail deserialization if `jira:` key exists in config but `JiraConfig` isn't defined yet

**Existing behavior**: unknown keys in the YAML are silently ignored by serde (serde's default for structs is to skip unknown fields). So a user adding `jira:` config today won't break anything. But the plugin can't READ that config through the typed struct.

**Options**:
1. Add `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig`. Plugins can pull their raw config from this map and deserialize it themselves. No jm-core coupling. Serde's flatten with deny_unknown_fields is tricky, but since we don't use deny_unknown_fields, this should work.
2. Each screen plugin reads its own config file (e.g., `~/.jm/plugins/jira.yaml`). Completely decoupled. Plugin gets the data_dir path and handles its own config.
3. The `PluginRegistry::new()` receives the raw `Config` and each plugin's constructor receives `&Config` — the plugin accesses `config.plugins.extra["jira"]`. Still needs option (1).

**Mitigation**: For Phase 0, the AboutPlugin needs no config, so this is not blocking. But the task list item "16. Update Config struct for screen plugin configuration" is underspecified — it needs to say WHICH of these approaches is taken. Recommend option (1) or (2) and specify it now.

---

## 6. `ScreenId::Plugin(String)` and `PartialEq` Derivation

**Severity: GREEN Minor**

`ScreenId` derives `Debug, Clone, PartialEq` (events.rs:16). Adding `Plugin(String)` is fine — `String` implements all three. No issue here.

However, `ScreenId` does NOT derive `Eq` or `Hash`. This means you cannot use `ScreenId` as a HashMap key. The spec's architecture doc shows:

```rust
pub screens: HashMap<String, Box<dyn ScreenPlugin>>
```

...while the rewrite spec shows:

```rust
pub screens: Vec<Box<dyn ScreenPlugin>>
```

These are inconsistent between the two documents. The `Vec` approach is simpler but requires linear scan to find a plugin by name. The `HashMap` approach requires the plugin `name()` as key. For a handful of screen plugins, either works. But the spec should pick one and be consistent.

**Mitigation**: Pick `Vec` for Phase 0 (simpler, matches sidebar pattern). Note the inconsistency.

---

## 7. The `handle_back()` Function Is Not Screen-Aware

**Severity: YELLOW Needs Resolution**

`handle_back()` (app.rs:794-808) currently does:

```rust
fn handle_back(&mut self) {
    if let Focus::Sidebar(_) = self.focus {
        self.focus = Focus::Main;
        return;
    }
    if matches!(self.screen, ScreenId::Review) {
        let _ = self.last_review_store.mark_reviewed_today();
    }
    self.screen = ScreenId::Dashboard;
    self.focus = Focus::Main;
    dashboard::refresh(&mut self.dashboard, &self.project_store);
}
```

When a screen plugin returns `Action::Back`, this function will:
1. NOT call `plugin.on_leave()` — there is no hook for this
2. Directly set `self.screen = ScreenId::Dashboard`
3. Refresh the dashboard

The spec's Section 5 shows lifecycle management where `Action::Back` triggers `on_leave()` before popping. But that code references `self.screen_stack.last()` which doesn't exist (see Issue #1).

**What must happen**: The `handle_back()` method (or the `Action::Back` arm in `update()`) needs to check if the current screen is `ScreenId::Plugin(name)` and, if so, call `plugin.on_leave()` before transitioning to Dashboard.

Currently `Action::Back` at app.rs:512 just calls `self.handle_back()`:

```rust
Action::Back => self.handle_back(),
```

The fix is straightforward — add a `ScreenId::Plugin(name)` check to `handle_back()`. But this is not in the task list.

**Mitigation**: Add explicit task: "Modify `handle_back()` in app.rs to call `on_leave()` when current screen is `ScreenId::Plugin`."

---

## 8. `on_key` Unused Parameter Warning in Trait Default

**Severity: GREEN Minor**

The `SidebarPlugin` trait's default implementation:

```rust
fn on_key(&mut self, key: KeyEvent) -> bool { false }
```

The parameter `key` is not prefixed with `_` in the spec. This will produce a compiler warning. The existing `Plugin` trait at plugins/mod.rs:24 uses `_key` to suppress this. The spec should match: `fn on_key(&mut self, _key: KeyEvent) -> bool { false }`.

Similarly for `Plugin::on_notify(&mut self, _message: &str)`.

---

## 9. Acceptance Criteria Are Not Mechanically Testable

**Severity: YELLOW Needs Resolution**

The acceptance criteria include:

- "Clock plugin renders correctly in sidebar" — What does "correctly" mean? There are zero rendering tests in the TUI crate. All existing tests are key-handling/state-machine tests (e.g., dashboard.rs has ~50 tests, all testing `handle_key` returning correct `Action` values). There is no rendering test infrastructure (no `TestBackend` setup, no buffer assertion helpers).

- "Sidebar focus (Tab) works" — The `Tab` key handling for sidebar is in `dashboard::handle_key`, which returns `Action::FocusSidebar`. This is testable at the Action level but not at the rendering level.

- "Demo screen renders correctly (full screen area)" — Same problem. No rendering test framework.

- "Demo screen lifecycle: on_enter called on open, on_leave called on close" — How do you verify this? The AboutPlugin's on_enter/on_leave are no-ops. You'd need a test plugin with observable side effects.

**Current test coverage**: jm-tui has ~150 tests, all in `screens/` modules, all testing `handle_key() -> Action` mappings. Zero rendering tests. Zero integration tests. Zero plugin tests.

**Mitigation**:

1. For regression criteria: "All existing `cargo test` pass" is the real gate (NOTE: there is already a failing proptest — `prop_project_name_with_yaml_special_chars` panics with "String join would overflow memory bounds"). This pre-existing failure should be noted as a known issue, not a Phase 0 regression.

2. For new functionality: Write unit tests for:
   - `PluginRegistry::new()` produces correct plugin counts
   - `PluginRegistry::get_screen("about")` returns `Some`
   - `PluginRegistry::get_screen("nonexistent")` returns `None`
   - `PluginRegistry::on_tick()` calls both sidebar and screen plugins
   - `AboutPlugin::handle_key(Esc)` returns `Action::Back`

   These are testable without rendering infrastructure.

3. Add explicit task: "Add unit tests for PluginRegistry and AboutPlugin" to the task list.

---

## 10. Thread Safety: `Box<dyn ScreenPlugin>` Is Not `Send + Sync`

**Severity: GREEN Minor (for Phase 0)**

The architecture doc's "Background Work Pattern" section describes spawning background threads from `on_enter()`. `Box<dyn ScreenPlugin>` is NOT `Send + Sync` by default. But this doesn't matter for the described pattern because:

- The plugin itself stays on the main TUI thread
- Background work communicates via `mpsc::Sender/Receiver` which ARE `Send`
- `on_tick()` polls the receiver on the main thread

This works. The trait objects don't need to be `Send`. But if anyone tries to move a `Box<dyn ScreenPlugin>` to another thread (e.g., for parallel rendering), they'll get a compile error. This is fine — just document that plugins live on the main thread.

**For Phase 0**: Not an issue. The AboutPlugin has no background work.

---

## 11. Missing Tasks in the Task List

**Severity: YELLOW Needs Resolution**

The task list (18 items) is missing several integration points:

| Missing Task | Why It Matters |
|---|---|
| Modify `handle_back()` to call `on_leave()` for plugin screens | Lifecycle won't work without this |
| Add `ScreenId::Plugin(String)` to `keyhints.rs` `get_hints()` match | Will cause non-exhaustive match error. Current match at keyhints.rs:86-151 covers all ScreenId variants — adding a new variant without updating this file is a compile error since the match has no wildcard arm |
| Add `ScreenId::Plugin(_)` arm to render match in app.rs:264-356 | Same — the render match is exhaustive, new variant = compile error |
| Add `ScreenId::Plugin(_)` arm to handle_key match in app.rs:452-469 | Same pattern |
| Add `ScreenId::Plugin(_)` to handle_select, handle_back, targeted_project_slug | These match on ScreenId |
| Update existing tests that construct ScreenId | If any test does exhaustive matching on ScreenId variants |
| Write unit tests for PluginRegistry and AboutPlugin | Testability (see Issue #9) |
| Verify `Action::OpenPlugin(String)` is added to the `Action` enum | Task 8 mentions this but it also needs to be handled in `update()` |

The `keyhints.rs` issue is particularly likely to be missed. The match on `ScreenId` at line 86 is exhaustive (no `_ =>` fallback). Adding `Plugin(String)` to `ScreenId` without adding a match arm in `get_hints()` will fail to compile. This is actually a GOOD thing (Rust catches it), but it means task 7 ("Add ScreenId::Plugin(String) to events.rs") cannot be completed in isolation — it immediately breaks keyhints.rs, app.rs render, app.rs handle_key, and any other exhaustive matches on ScreenId.

**Mitigation**: Reorder/group tasks so that adding the ScreenId variant and all its match arms happen in the same atomic step.

---

## 12. `PluginSidebar::new_from` Does Not Exist Yet

**Severity: GREEN Minor**

The spec (Section 3) introduces `PluginSidebar::new_from(plugins: Vec<Box<dyn SidebarPlugin>>)`. The existing `PluginSidebar` (sidebar.rs:21-61) has `pub fn new(config: &Config)` which internally builds the plugin list.

When the `PluginRegistry` takes over plugin construction, the `PluginSidebar` needs to accept pre-built plugins. Two options:

1. Add `new_from()` as proposed
2. Modify `new()` to accept `Vec<Box<dyn SidebarPlugin>>` directly

Either way, the sidebar's internal storage changes from `Vec<Box<dyn Plugin>>` to `Vec<Box<dyn SidebarPlugin>>` (task 6). This is a type change that affects `sidebar.rs:16`:

```rust
pub plugins: Vec<Box<dyn Plugin>>,
```

Every method on `PluginSidebar` that calls trait methods will continue to work IF `SidebarPlugin: Plugin` (which it does — the sub-trait relationship is correctly defined). But `dyn SidebarPlugin` and `dyn Plugin` are different trait objects. You cannot store `Box<dyn SidebarPlugin>` in `Vec<Box<dyn Plugin>>`.

**Specifically**: The `on_tick()` method in sidebar.rs:132-166 iterates `self.plugins` and calls `plugin.needs_timer()` and `plugin.on_tick()`. These are defined on the `Plugin` base trait. With `Vec<Box<dyn SidebarPlugin>>`, you can still call them because `SidebarPlugin: Plugin` means the vtable includes `Plugin`'s methods. Rust handles this correctly for trait objects where the sub-trait has `Plugin` as a supertrait. This WILL work.

The `render()` method calls `plugin.height()` and `plugin.render()` — both on `SidebarPlugin`. Also fine.

The `handle_key()` method calls `plugin.on_key()` — on `SidebarPlugin`. Fine.

The `on_notify()` forwarding calls `plugin.on_notify()` — on `Plugin`. Fine because `SidebarPlugin: Plugin`.

**Mitigation**: Verify with a compile check that `Box<dyn SidebarPlugin>` correctly exposes `Plugin` trait methods. This should work in Rust, but it's worth a quick prototype if anyone is uncertain.

---

## 13. Pre-Existing Test Failure

**Severity: YELLOW Needs Resolution (pre-existing)**

`cargo test` currently fails:

```
FAILED: prop_project_name_with_yaml_special_chars
"String join would overflow memory bounds"
minimal failing input: name = "aaa  A 0aaa0  A0 A0a0A ? Aa      a"
```

This is in `crates/jm-core/tests/proptest_roundtrip.rs` and is unrelated to the plugin rewrite. However, the acceptance criteria state "All existing cargo test pass." This will fail before any Phase 0 code is written.

**Mitigation**: Either fix this proptest issue first, or change the acceptance criterion to "All existing tests pass (excluding known pre-existing failure in prop_project_name_with_yaml_special_chars)."

---

## 14. `Action::OpenPlugin(String)` Handling in `update()`

**Severity: YELLOW Needs Resolution**

The spec adds `Action::OpenPlugin(String)` to the Action enum and shows pseudocode for handling it. But the `update()` function in app.rs:498 is a massive match statement (~300 lines). The new action needs an arm there.

The spec shows:

```rust
Action::OpenPlugin(name) => {
    if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
        plugin.on_enter();
        self.screen_stack.push(ScreenId::Plugin(name));
    }
}
```

Corrected for single-screen architecture:

```rust
Action::OpenPlugin(name) => {
    if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
        plugin.on_enter();
        self.screen = ScreenId::Plugin(name);
    }
}
```

This is straightforward, but the task list doesn't have a dedicated task for "Add Action::OpenPlugin handling to update()". Task 8 says "Add Action::OpenPlugin(String) to events.rs" (the enum definition) but not the handling.

**Mitigation**: Add task: "Handle Action::OpenPlugin in app.rs update() — call on_enter() and set screen."

---

## Summary Table

| # | Issue | Severity | Blocking Phase 0? |
|---|-------|----------|--------------------|
| 1 | No screen stack exists | RED | Yes — spec pseudocode is wrong |
| 2 | self.plugins blast radius (7 sites) | YELLOW | Manageable but needs explicit task |
| 3 | Render signature (Frame vs Buffer) | YELLOW | Design decision needed before coding |
| 4 | Modal control flow for plugins | RED | Not blocking Phase 0, but blocks Phase 1 if unacknowledged |
| 5 | Config deserialization for plugin config | YELLOW | Not blocking Phase 0, needs decision for Phase 1 |
| 6 | ScreenId PartialEq / Vec vs HashMap inconsistency | GREEN | Cosmetic |
| 7 | handle_back() needs on_leave() hook | YELLOW | Lifecycle won't work without fix |
| 8 | on_key unused param warning | GREEN | Cosmetic |
| 9 | Acceptance criteria not mechanically testable | YELLOW | Needs test tasks added |
| 10 | Thread safety of trait objects | GREEN | Fine for Phase 0 |
| 11 | Missing tasks (keyhints, match arms, tests) | YELLOW | Will cause compile errors |
| 12 | SidebarPlugin supertrait vtable | GREEN | Should work, verify |
| 13 | Pre-existing test failure | YELLOW | Blocks "all tests pass" criterion |
| 14 | Action::OpenPlugin handling missing from tasks | YELLOW | Oversight in task list |

### Recommendations Before Starting Implementation

1. **Fix the spec's screen navigation model**: Replace all `screen_stack` references with `self.screen` single-field pattern. This is the biggest correctness gap.
2. **Decide render signature now**: `(frame: &mut Frame, area: Rect)` is future-proof; `(area: Rect, buf: &mut Buffer)` limits cursor support. Pick one for `ScreenPlugin`.
3. **Document modal strategy for Phase 1**: "Plugins manage their own modals internally" or "Generic PluginInput action routes results back." Either works, but the choice constrains the trait design.
4. **Expand the task list**: Add the 8 missing tasks from Issue #11, especially the exhaustive-match updates that will fail to compile.
5. **Fix or acknowledge the pre-existing proptest failure** so the "all tests pass" criterion is achievable.
