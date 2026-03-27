# Phase 0: Implementation Progress Tracker

**Branch:** `feature/jira-plugin`
**Phase:** 0 — Plugin System Rewrite
**Last Updated:** 2026-03-27

---

## 1. Team Roster

| Agent | Role | Responsibilities |
|-------|------|-----------------|
| **core-architect** | Senior Rust Dev 1 | Traits (`plugins/mod.rs`), `PluginRegistry` (`plugins/registry.rs`), `app.rs` refactor, `events.rs` changes, `keyhints.rs` key hint wiring |
| **plugin-migrator** | Senior Rust Dev 2 | Plugin migrations (`clock.rs`, `notifications.rs`, `pomodoro.rs`), `PluginSidebar` refactor (`sidebar.rs`), `AboutPlugin` implementation (`about.rs`) |
| **proptest-intern** | Proptest Intern | Fix `prop_project_name_with_yaml_special_chars` proptest failure in `jm-core` |
| **ux-engineer-1** | UX Engineer 1 | Rendering review — verify sidebar hidden when screen plugin active, full-screen layout, key hints display |
| **ux-engineer-2** | UX Engineer 2 | Interaction review — verify lifecycle hooks, navigation behavior, `J` keybinding, Esc back flow |
| **qa-engineer-1** | QA Engineer 1 | Unit tests for `PluginRegistry` and `AboutPlugin`, test suite wiring |
| **qa-engineer-2** | QA Engineer 2 | Regression testing — all sidebar plugins, full test suite pass verification |

---

## 2. Task Tracking Table

| # | Task Name | Assigned To | Status | Dependencies | Notes |
|---|-----------|-------------|--------|--------------|-------|
| 0 | Fix pre-existing proptest failure | proptest-intern | In Progress | — | Fix `prop_project_name_with_yaml_special_chars` in `jm-core`; blocks test gate for Tasks 20–21 |
| 1 | Define new trait hierarchy in `plugins/mod.rs` | core-architect | In Progress | — | Drop `Plugin` base trait; define `SidebarPlugin`, `ScreenPlugin`, `PluginAction` as independent types |
| 2 | Migrate `ClockPlugin` to `SidebarPlugin` | plugin-migrator | In Progress | 1 | Mechanical rename; no behavioral change |
| 3 | Migrate `NotificationsPlugin` to `SidebarPlugin` | plugin-migrator | In Progress | 1 | Mechanical rename; no behavioral change |
| 4 | Migrate `PomodoroPlugin` to `SidebarPlugin` | plugin-migrator | In Progress | 1 | Mechanical rename; no behavioral change |
| 5 | Refactor `PluginSidebar` to `Vec<Box<dyn SidebarPlugin>>` | plugin-migrator | In Progress | 1, 2, 3, 4 | Replace `new()` with `new_from()`; must precede Task 6 |
| 6 | Create `PluginRegistry` in `plugins/registry.rs` | core-architect | In Progress | 1, 5 | `Vec<Box<dyn ScreenPlugin>>` for screens; separate tick methods for sidebar (1s) and active screen (250ms) |
| 7 | Add `ScreenId::Plugin(String)` to `events.rs` | core-architect | In Progress | — | Derive `Clone` on `ScreenId` if not already present |
| 8 | Add `ScreenId::Plugin(_)` wildcard arms to all match sites | core-architect | In Progress | 7 | 9 sites: `render()`, `handle_key()`, `current_project()`, `targeted_project_slug()`, help modal name, `handle_select()`, `handle_back()`, `handle_start_work()` in `app.rs`; `get_hints()` in `keyhints.rs` |
| 9 | Add `Action::OpenPlugin(String)` to `events.rs` | core-architect | In Progress | — | Enables dashboard to request a screen plugin open |
| 10 | Rename `self.plugins` to `self.plugin_registry` in `app.rs` | core-architect | In Progress | 6 | 7 call sites; replace `PluginSidebar` type with `PluginRegistry` in App struct |
| 11 | Handle `Action::OpenPlugin` in `update()` | core-architect | In Progress | 6, 9, 10 | Look up plugin, call `on_enter()`, set `self.screen = ScreenId::Plugin(name)` |
| 12 | Modify `handle_back()` to call `on_leave()` for plugin screens | core-architect | In Progress | 7, 10 | Clone-first pattern to avoid borrow conflicts; set screen to Dashboard after |
| 13 | Wire screen plugin rendering into `app.rs` | core-architect | In Progress | 6, 7, 10 | Full-screen render; hide sidebar when screen plugin is active |
| 14 | Wire screen plugin key handling into `app.rs` | core-architect | In Progress | 6, 7, 10 | Delegate to `handle_key()`, translate `PluginAction`; clone-first pattern |
| 15 | Wire key hints for screen plugins into `keyhints.rs` | core-architect | In Progress | 7 | Match `ScreenId::Plugin(ref name)`, return plugin's `key_hints()` |
| 16 | Implement `AboutPlugin` demo | plugin-migrator | In Progress | 1 | New file `plugins/about.rs`; implements `ScreenPlugin` directly; renders version info centered; Esc/q → `PluginAction::Back` |
| 17 | Register `AboutPlugin` in `PluginRegistry` | core-architect | In Progress | 6, 16 | Always registered; no config needed |
| 18 | Add `J` keybinding for `AboutPlugin` | core-architect | In Progress | 9, 16, 17 | Hardcoded uppercase `J` in dashboard key handling → `Action::OpenPlugin("about".to_string())` |
| 19 | Write unit tests for `PluginRegistry` and `AboutPlugin` | qa-engineer-1 | Not Started | 6, 16 | Test lookup by name, tick behavior, `PluginAction` handling |
| 20 | Run full test suite — all tests pass | qa-engineer-2 | Not Started | 0, 19, all prior | Includes proptest fix from Task 0 |
| 21 | Manual testing — sidebar plugins work, About screen works | qa-engineer-2 | Not Started | 20 | Verify sidebar hidden when About active; `on_leave()` called on back; all sidebar plugins functional |

**Status key:** `Not Started` | `In Progress` | `Complete` | `Blocked`

---

## 3. File Ownership Matrix

Parallel work is safe because agents own non-overlapping files. The only expected conflict is `plugins/mod.rs` (see Merge Plan).

| File | Owner | Notes |
|------|-------|-------|
| `crates/jm-tui/src/plugins/mod.rs` | **core-architect** | Defines `SidebarPlugin`, `ScreenPlugin`, `PluginAction`; plugin-migrator reads this but does not own it |
| `crates/jm-tui/src/plugins/registry.rs` | **core-architect** | New file; no conflict risk |
| `crates/jm-tui/src/app.rs` | **core-architect** | All structural changes; plugin-migrator does not touch this file |
| `crates/jm-tui/src/events.rs` | **core-architect** | `ScreenId::Plugin`, `Action::OpenPlugin` |
| `crates/jm-tui/src/keyhints.rs` | **core-architect** | Key hints wiring for screen plugins |
| `crates/jm-tui/src/plugins/sidebar.rs` | **plugin-migrator** | `PluginSidebar` refactor; `new_from()` |
| `crates/jm-tui/src/plugins/clock.rs` | **plugin-migrator** | `SidebarPlugin` migration |
| `crates/jm-tui/src/plugins/notifications.rs` | **plugin-migrator** | `SidebarPlugin` migration |
| `crates/jm-tui/src/plugins/pomodoro.rs` | **plugin-migrator** | `SidebarPlugin` migration |
| `crates/jm-tui/src/plugins/about.rs` | **plugin-migrator** | New file; `AboutPlugin` implementation |
| `crates/jm-core/tests/proptest_roundtrip.rs` | **proptest-intern** | Proptest fix only |
| `crates/jm-core/src/models/project.rs` | **proptest-intern** | Fix only if root cause is in serialization |

### Shared Read-Only Reference (no edits)

| File | Read By |
|------|---------|
| `crates/jm-tui/src/plugins/mod.rs` | plugin-migrator (reads trait definitions to implement them) |
| `crates/jm-tui/src/plugins/registry.rs` | plugin-migrator (reads `PluginRegistry::new()` signature) |

---

## 4. Merge Plan

Merges target `feature/jira-plugin` in the following order. Each merge must compile before proceeding to the next.

### Step 1: Merge proptest-intern worktree → `feature/jira-plugin`

- **Expected conflicts:** None (proptest-intern only touches `jm-core` test files)
- **Verification:** `cargo test -p jm-core proptest` passes

### Step 2: Merge core-architect worktree → `feature/jira-plugin`

- **Expected conflicts:** None at this stage (plugin-migrator not yet merged)
- **Verification:** `cargo build` succeeds (app.rs will have wildcard arms; sidebar plugins will fail to compile until Step 3)
- **Note:** `app.rs` will not fully compile until plugin-migrator's `SidebarPlugin` implementations are present. Wildcard arms in Task 8 prevent exhaustiveness errors, but `PluginSidebar::new_from()` is called by `PluginRegistry::new()` — plugin-migrator must merge before the binary links.

### Step 3: Merge plugin-migrator worktree → `feature/jira-plugin`

- **Expected conflicts:**
  - `plugins/mod.rs` — **CONFLICT EXPECTED**: core-architect dropped the old `Plugin` trait; plugin-migrator's migrations reference the new `SidebarPlugin` trait. **Resolution: take core-architect's version of `mod.rs` in its entirety.** Plugin-migrator's `about.rs` is a new file with no conflict.
- **Resolution steps:**
  1. Accept core-architect's `plugins/mod.rs` (it has the correct trait definitions that plugin-migrator implemented against)
  2. Confirm plugin-migrator's `about.rs` is present as a new file
  3. Confirm plugin-migrator's `sidebar.rs`, `clock.rs`, `notifications.rs`, `pomodoro.rs` changes are preserved
- **Verification:** `cargo build` succeeds with no errors

### Step 4: Full build and test on merged result

- `cargo build --release`
- `cargo test` (all tests pass, including proptest fix)
- QA Engineer 1 runs unit tests (Task 19)

### Step 5: QA sign-off

- QA Engineer 2 runs regression and manual testing (Tasks 20–21)
- All acceptance criteria checked off

---

## 5. Acceptance Criteria Checklist

QA engineers use this checklist to sign off on Phase 0. All items must be checked before merging to `main`.

### Regression

- [ ] Clock plugin renders correctly in sidebar
- [ ] Notifications plugin receives and displays messages, clears on `c`, reminders fire
- [ ] Pomodoro plugin starts/pauses/resets, transitions between states, emits notifications
- [ ] Sidebar focus (`Tab`) works: navigate between plugins, `Esc` to unfocus
- [ ] Sidebar toggle works
- [ ] Toast notifications from plugin ticks still appear
- [ ] All existing `cargo test` pass (including proptest fix from Task 0)

### New Functionality

- [ ] `SidebarPlugin` and `ScreenPlugin` are independent traits with no supertrait
- [ ] `PluginRegistry` manages both plugin types
- [ ] `ScreenId::Plugin(String)` variant added and handled via flat `screen` field (no stack)
- [ ] Demo `AboutPlugin` can be opened via `J` keybinding from dashboard
- [ ] Demo screen renders correctly (full screen area, version info centered)
- [ ] Sidebar is hidden when AboutPlugin screen is active
- [ ] Demo screen handles keys (`Esc` returns to dashboard, `q` returns to dashboard)
- [ ] Demo screen lifecycle: `on_enter` called on open, `on_leave` called on close
- [ ] `handle_back()` calls `on_leave()` for plugin screens
- [ ] Key hints render correctly for the demo screen (`Esc` → `back`)
- [ ] Demo screen does not interfere with sidebar plugins

### Code Quality

- [ ] No `unsafe` code introduced
- [ ] No new `unwrap()` on fallible operations
- [ ] Existing sidebar plugin code changes are minimal (trait split only — no behavioral changes)
- [ ] New public items have doc comments

---

## 6. Known Risks

### Risk 1: `plugins/mod.rs` merge conflict (Medium — Mitigated)

**Description:** Both core-architect and plugin-migrator work from the same starting state of `plugins/mod.rs`. Core-architect drops the `Plugin` trait and defines `SidebarPlugin` / `ScreenPlugin`. Plugin-migrator implements `SidebarPlugin` for all three existing plugins and `ScreenPlugin` for `AboutPlugin` — these implementations live in separate files, not in `mod.rs` itself.

**Mitigation:** Plugin-migrator works from the spec's exact trait definitions (copied verbatim in the Phase 0 design doc). If plugin-migrator's worktree has any re-export additions to `mod.rs` (e.g., `pub mod about`), those lines must be manually added to core-architect's version during merge. Resolution: take core-architect's full `mod.rs`, then add plugin-migrator's `pub mod about;` line.

### Risk 2: `app.rs` does not compile until after Step 3 merge (Low — Expected)

**Description:** After merging core-architect but before merging plugin-migrator, `PluginRegistry::new()` calls `PluginSidebar::new_from()`, and the migrated sidebar plugins (`ClockPlugin`, `NotificationsPlugin`, `PomodoroPlugin`) must implement `SidebarPlugin`. Until plugin-migrator's implementations are merged, the binary will not link.

**Mitigation:** This is expected and acceptable — Steps 2 and 3 should be performed in rapid succession. Do not run QA between these two steps.

### Risk 3: 250ms screen plugin tick requires event loop changes (Medium)

**Description:** The existing tick system fires at 1s intervals. The Phase 0 spec requires screen plugins to tick at 250ms. This may require changes to the `crossterm` event polling timeout or a separate timer in `app.rs`.

**Mitigation:** Core-architect owns `app.rs` and the tick system. If the event loop uses `poll(Duration::from_millis(1000))`, it must be reduced to 250ms (or a sub-tick counter added). Sidebar plugin behavior is unchanged since `tick_sidebar()` is still called at the logical 1s mark using a counter. Verify no sidebar plugin relies on wall-clock 1s accuracy.

### Risk 4: Borrow checker conflicts in `app.rs` (Low — Documented)

**Description:** Multiple sites in `app.rs` borrow `self.screen` and `self.plugin_registry` simultaneously, which violates Rust's aliasing rules.

**Mitigation:** The spec explicitly documents the clone-first pattern for `handle_back()` and `handle_key()`. Core-architect must apply this pattern at all affected sites. The pattern is: `let screen = self.screen.clone(); if let ScreenId::Plugin(name) = screen { ... }`.
