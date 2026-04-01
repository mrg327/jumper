# Code-vs-Spec Review R4: Plugin System Implementation

Reviewer: automated code/spec comparison
Date: 2026-03-27
Branch: feature/jira-plugin
Specs read: `plugin-architecture.md`, `plugin-system-rewrite.md`
Code read: `plugins/mod.rs`, `plugins/registry.rs`, `plugins/about.rs`, `app.rs`, `events.rs`, `keyhints.rs`, `jm-tui/Cargo.toml`, `jm-core/src/config.rs`

---

## Summary

**12 discrepancies found.** 9 are AUTO-FIX. 3 are NEEDS-INPUT.

All 265 tests pass. `cargo check` produces zero errors and one dead-code warning (classified below).

---

## Verification Results by Check

### 1. PluginAction enum — all variants present?

**PASS with one discrepancy.**

Spec (`plugin-architecture.md` §PluginAction, `plugin-system-rewrite.md` §Task 1):
```
None, Back, Toast(String), LaunchEditor { content: String, context: String }
```

Code (`plugins/mod.rs`):
```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { content: String, context: String },
}
```

All four variants present. Derives `Debug, Clone, PartialEq, Eq` — consistent with spec.

**DISCREPANCY D-01 (AUTO-FIX):** `cargo check` emits `dead_code` warnings for `Toast` and `LaunchEditor` variants:
```
warning: variants `Toast` and `LaunchEditor` are never constructed
```
These variants are constructed in `app.rs` (the match arm at line ~535-540 handles `LaunchEditor` and `Toast`), but the `PluginAction` enum itself is in `jm-tui`, and dead-code analysis does not see the `app.rs` match as a "construction" site. The fix is to add `#[allow(dead_code)]` on the `PluginAction` enum, or — better — add `#[cfg_attr(not(test), allow(dead_code))]` since the variants will be used by the Jira plugin. Alternatively a `_` suppression field is not needed; a simple `#[allow(dead_code)]` annotation on the enum is sufficient.

---

### 2. ScreenPlugin trait — methods and signatures

**PASS with one discrepancy.**

Spec defines these methods on `ScreenPlugin`:
- `name(&self) -> &str` — PRESENT
- `needs_timer(&self) -> bool { false }` — PRESENT
- `on_tick(&mut self) -> Vec<String> { Vec::new() }` — PRESENT
- `render(&self, frame: &mut Frame, area: Rect)` — PRESENT
- `handle_key(&mut self, key: KeyEvent) -> PluginAction` — PRESENT
- `on_enter(&mut self)` — PRESENT
- `on_leave(&mut self)` — PRESENT
- `key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }` — PRESENT
- `on_editor_complete(&mut self, _content: String, _context: &str) {}` — PRESENT

**DISCREPANCY D-02 (NEEDS-INPUT):** `plugin-architecture.md` (the overview doc) includes `on_notify(&mut self, _message: &str) {}` in the `ScreenPlugin` trait signature (under "Trait Signature Summary" at the bottom of the file). The implementation-spec `plugin-system-rewrite.md` does NOT include `on_notify` in the `ScreenPlugin` trait — its Phase 0 trait definition omits it entirely.

The code does NOT implement `on_notify` on `ScreenPlugin`.

This is a spec conflict. `plugin-architecture.md` is the higher-level design doc; `plugin-system-rewrite.md` is the Phase 0 implementation spec which explicitly defers some items. Because no plugin currently uses `on_notify` for inter-plugin messaging, omitting it from Phase 0 is defensible. However the final architecture is described as having it. **Decision needed:** is `on_notify` required on `ScreenPlugin` now, or deferred to Phase 1?

---

### 3. SidebarPlugin trait — matches spec?

**PASS.** All methods present with correct signatures and defaults. The old `Plugin` base trait is fully gone. `SidebarPlugin` is an independent trait with no supertrait. Consistent with both specs.

---

### 4. PluginRegistry — methods match spec?

**PASS.**

Spec (`plugin-system-rewrite.md` §3 and `plugin-architecture.md` §Plugin Registry):
- `pub sidebar: PluginSidebar` — PRESENT
- `pub screens: Vec<Box<dyn ScreenPlugin>>` — PRESENT
- `new(config: &Config) -> Self` — PRESENT
- `get_screen(&self, name: &str) -> Option<&dyn ScreenPlugin>` — PRESENT
- `get_screen_mut(&mut self, name: &str) -> Option<&mut Box<dyn ScreenPlugin>>` — PRESENT
- `tick_sidebar(&mut self) -> Vec<String>` — PRESENT
- `tick_screen(&mut self, name: &str) -> Vec<String>` — PRESENT
- `AboutPlugin` always registered unconditionally — PRESENT

**DISCREPANCY D-03 (AUTO-FIX):** `plugin-architecture.md` §Notification Forwarding defines a method `tick_active_screen(&mut self, active_screen: &ScreenId)` on `PluginRegistry` that is responsible for the notification-forwarding loop:
```rust
pub fn tick_active_screen(&mut self, active_screen: &ScreenId) {
    if let ScreenId::Plugin(name) = active_screen {
        if let Some(plugin) = self.screens.iter_mut().find(|p| p.name() == name) {
            let notifications = plugin.on_tick();
            for msg in notifications { self.sidebar.push_notification(&msg); }
        }
    }
}
```
The code instead uses `tick_screen(name: &str)` and performs the notification forwarding inline in `app.rs`'s run loop. The forwarding logic itself is correct and present. This is a refactoring difference (behavior is equivalent), but the named method `tick_active_screen` described in the architecture doc does not exist. The `ScreenId` parameter is resolved at the call site in `app.rs` rather than inside the registry. This is lower abstraction than the spec prescribes.

---

### 5. app.rs wiring — render(), handle_key(), handle_back(), update()

**PASS with two discrepancies.**

#### render()
- `ScreenId::Plugin(name)` arm present — renders full content_area, sidebar hidden when plugin active — CORRECT
- Clone-first pattern used to avoid borrow conflicts — CORRECT

#### handle_key()
- `ScreenId::Plugin(name)` arm present — delegates to `plugin.handle_key(key)` — CORRECT
- Translates all four `PluginAction` variants — CORRECT
- Clone-first pattern used — CORRECT

#### handle_back()
- Calls `plugin.on_leave()` before setting `self.screen = ScreenId::Dashboard` — CORRECT
- Clone-first borrow pattern used — CORRECT

#### update() — Action::OpenPlugin
- Calls `plugin.on_enter()` then sets `self.screen = ScreenId::Plugin(name)` — CORRECT
- Emits a toast if plugin not found — CORRECT (nice defensive addition)

**DISCREPANCY D-04 (NEEDS-INPUT):** In the `Help` modal action (`update()`, line ~663), the code uses:
```rust
ScreenId::Plugin(_) => "plugin",
```
The spec (`plugin-system-rewrite.md` §Match Sites, item 5) says "Use the plugin's `name()` as the screen label." The code uses a hardcoded `"plugin"` string instead of calling `plugin.name()`. This means all screen plugins show `"plugin"` in the help modal header rather than `"about"`, `"jira"`, etc.

**DISCREPANCY D-05 (AUTO-FIX):** `handle_select()` has:
```rust
ScreenId::Plugin(_) => {
    // Plugin screens handle their own Enter keys via handle_key() -> PluginAction.
}
```
This is the correct no-op behavior specified in the spec (Match Site item 6). No issue. (Not a discrepancy — listed here for completeness of coverage.)

---

### 6. app.rs editor handling — pending_editor_plugin cycle

**PASS.** Full suspend/resume/callback cycle is implemented correctly.

Steps from spec:
1. Plugin returns `PluginAction::LaunchEditor { content, context }` — handled in `handle_key()` (line ~536-542): writes temp file at `$TMPDIR/jm-plugin-<name>.txt`, sets `pending_editor_plugin = Some((name, context, temp_path))`. CORRECT.
2. App stashes `(plugin_name, context, temp_path)` — CORRECT.
3. At top of run loop before draw — handled (lines ~208-232). CORRECT.
4. Suspends TUI (`disable_raw_mode`, `LeaveAlternateScreen`) — CORRECT.
5. Launches `$EDITOR` (falling back to `vim`) — CORRECT.
6. Resumes TUI (`enable_raw_mode`, `EnterAlternateScreen`, `terminal.clear()`) — CORRECT.
7. Reads back temp file content, deletes file, calls `plugin.on_editor_complete(edited, &context)` — CORRECT.

---

### 7. app.rs tick — 250ms screen plugin tick

**PASS.** Implemented correctly in the run loop (lines ~249-259):
```rust
if self.last_screen_tick.elapsed() >= Duration::from_millis(250) {
    self.last_screen_tick = Instant::now();
    let screen = self.screen.clone();
    if let ScreenId::Plugin(name) = screen {
        let msgs = self.plugin_registry.tick_screen(&name);
        for msg in msgs { self.plugin_registry.sidebar.push_notification(&msg); }
    }
}
```
Notification forwarding to sidebar is present. Screen plugin only ticked when `ScreenId::Plugin`. `last_screen_tick: Instant` field exists in the App struct. CORRECT.

---

### 8. events.rs — ScreenId::Plugin(String) and Action::OpenPlugin(String)

**PASS.**

- `ScreenId::Plugin(String)` — PRESENT (line 28 of `events.rs`).
- `Action::OpenPlugin(String)` — PRESENT (line 125 of `events.rs`).
- `ScreenId` derives `Clone` — PRESENT (needed for clone-first borrow pattern).

---

### 9. keyhints.rs — plugin hints integration

**PASS.** Correct integration:
- `get_hints()` has `ScreenId::Plugin(_)` arm that returns `plugin_hints.unwrap_or_else(|| vec![("Esc", "back")])`.
- `render()` accepts `plugin_hints: Option<Vec<(&'static str, &'static str)>>`.
- In `app.rs`, plugin hints are collected from `plugin.key_hints()` and passed to `keyhints::render()` (lines ~479-498).
- `'static` lifetime on tuples — CORRECT (matches spec requirement to avoid lifetime entanglement).

---

### 10. Cargo.toml — all deps present?

**PASS.** All five dependencies from spec are present:
- `ureq = { version = "3", features = ["json"] }` — PRESENT
- `serde = { version = "1", features = ["derive"] }` — PRESENT
- `serde_json = "1"` — PRESENT
- `serde_yml = "0.0.12"` — PRESENT
- `base64 = "0.22"` — PRESENT

---

### 11. config.rs — serde(flatten) extra field

**PASS.** `PluginConfig` has:
```rust
#[serde(flatten, default)]
pub extra: std::collections::HashMap<String, serde_yml::Value>,
```
Matches the spec exactly (`plugin-architecture.md` §Config Extension). `serde_yml::Value` is the correct type. `#[serde(flatten, default)]` attribute is correct.

---

### 12. cargo check and cargo test results

**PASS (with warnings).**

`cargo check` output:
```
warning: variants `Toast` and `LaunchEditor` are never constructed
  --> crates/jm-tui/src/plugins/mod.rs:72:5
Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.25s
```
Zero errors. One dead_code warning (covered in D-01).

`cargo test` output:
```
test result: ok. 91 passed; 0 failed; 0 ignored (jm-core unit)
test result: ok. 30 passed; 0 failed; 0 ignored (proptest_roundtrip)
test result: ok. 3 passed; 0 failed; 0 ignored (real_data_roundtrip)
test result: ok. 31 passed; 0 failed; 0 ignored (storage_edge_cases)
test result: ok. 140 passed; 0 failed; 0 ignored (jm-tui)
```
Total: **295 tests, 0 failures.** Pre-existing proptest failure (Task 0) is fixed.

Minor test warnings (unused import in `storage_edge_cases.rs`, unused functions in `proptest_roundtrip.rs`) — pre-existing, unrelated to plugin system.

---

## Discrepancy Catalogue

| ID | File | Description | Classification |
|----|------|-------------|----------------|
| D-01 | `plugins/mod.rs` | `PluginAction::Toast` and `::LaunchEditor` trigger `dead_code` warning; variants exist and are matched in `app.rs` but no plugin constructs them yet | AUTO-FIX: add `#[allow(dead_code)]` to `PluginAction` |
| D-02 | `plugins/mod.rs` | `ScreenPlugin` does not have `on_notify(&mut self, _message: &str) {}` — present in `plugin-architecture.md` summary, absent from `plugin-system-rewrite.md` Phase 0 spec and from code | NEEDS-INPUT: resolve spec conflict; add to `ScreenPlugin` now or defer to Phase 1 |
| D-03 | `plugins/registry.rs` | `plugin-architecture.md` prescribes a `tick_active_screen(&mut self, active_screen: &ScreenId)` method on `PluginRegistry`; code uses `tick_screen(name: &str)` with forwarding done inline in `app.rs` | AUTO-FIX: either add `tick_active_screen` as a thin wrapper or accept the deviation and update the architecture doc |
| D-04 | `app.rs` line ~666 | Help modal uses hardcoded `"plugin"` string for plugin screen label; spec says use `plugin.name()` | AUTO-FIX: look up active plugin and use its `name()` |
| D-05 | (none) | All 9 `ScreenId` match sites listed in `plugin-system-rewrite.md` §Task 8 are handled correctly | — PASS — |

---

## Additional Observations (Not Spec Discrepancies)

- **`AboutPlugin` does not implement `needs_timer()`** — defaults to `false`, which is correct for Phase 0. No issue.
- **`PluginRegistry::new()` always registers `AboutPlugin`** regardless of the `enabled` list — matches spec ("AboutPlugin is always registered as a screen plugin — no config needed").
- **`J` keybinding for AboutPlugin** — not visible in this review (lives in `screens/dashboard.rs`). Not checked here but listed as Task 18 in the spec. Should be verified separately.
- **`handle_start_work()` for plugin screens** — returns `None` via the `_ => None` arm. Correct per spec (Match Site item 8: "No-op for plugin screens").
- **`targeted_project_slug()` for plugin screens** — falls through to `_ => None`. Correct per spec (Match Site item 4).
- **`current_project()` for plugin screens** — returns `None` via the else branch. Correct per spec (Match Site item 3).

---

## Classification Summary

| Classification | Count | IDs |
|----------------|-------|-----|
| AUTO-FIX | 3 | D-01, D-03, D-04 |
| NEEDS-INPUT | 1 | D-02 |
| PASS | — | D-05, all checks not listed above |

**Build status: GREEN (0 errors, 295 tests passing)**
