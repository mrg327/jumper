# Interaction Test Plan — Phase 0 Plugin System Rewrite

**Scope:** Keybindings, focus management, and navigation flow for `ScreenId::Plugin(String)` and the `AboutPlugin` demo screen.

**Prepared by:** UX Engineer 2
**Based on:** `docs/design/plugin-system-rewrite.md`, `crates/jm-tui/src/app.rs`, `crates/jm-tui/src/events.rs`, `crates/jm-tui/src/screens/dashboard.rs`, `crates/jm-tui/src/keyhints.rs`

---

## Conventions

Each test case uses:

- **Precondition** — starting state
- **Input sequence** — keystrokes in order
- **Expected result** — observable outcome
- **Verification method** — how to confirm it (manual visual check, unit test, or assertion in code)

Pass/fail is recorded as `[ ]` (not yet tested) / `[x]` (passed) / `[!]` (failed, see notes).

---

## 1. Navigation Flow Tests

### 1.1 — Dashboard → J → AboutPlugin → Esc → Dashboard

| Field | Value |
|---|---|
| Precondition | App running, `screen = ScreenId::Dashboard`, `focus = Focus::Main`, no modal open |
| Input sequence | `J` then `Esc` |
| Expected result | After `J`: screen = `ScreenId::Plugin("about")`, full-screen About content rendered, sidebar hidden. After `Esc`: screen = `ScreenId::Dashboard`, sidebar visible again, dashboard list rendered. |
| Verification | Manual: watch screen transitions. Unit: assert `app.screen == ScreenId::Dashboard` after Esc dispatch. |

- [ ] Pass

---

### 1.2 — Dashboard → J → AboutPlugin → q → Dashboard

| Field | Value |
|---|---|
| Precondition | App running, `screen = ScreenId::Dashboard`, `focus = Focus::Main`, no modal open |
| Input sequence | `J` then `q` |
| Expected result | Same as 1.1 — `q` must also trigger `PluginAction::Back` from `AboutPlugin::handle_key`. The app returns to Dashboard without quitting (q on Dashboard quits; q on AboutPlugin goes back). |
| Verification | Manual: confirm app does NOT quit. Unit: assert `app.screen == ScreenId::Dashboard` and `app.should_quit == false`. |

- [ ] Pass

**Implementation note:** `q` must be handled inside `AboutPlugin::handle_key` returning `PluginAction::Back`, NOT forwarded to the global quit handler. The global key handler only reaches `dashboard::handle_key` when `self.screen == ScreenId::Dashboard`. When the screen is `ScreenId::Plugin(_)`, all keys go through `plugin.handle_key()` first — so `q` on AboutPlugin is safe to map to Back.

---

### 1.3 — Tab (sidebar focus) then J: expected behavior

| Field | Value |
|---|---|
| Precondition | App running, `screen = ScreenId::Dashboard`, sidebar visible, `focus = Focus::Main` |
| Input sequence | `Tab` (focus sidebar), then `J` |
| Expected result | After `Tab`: `focus = Focus::Sidebar(0)`. After `J`: the sidebar's `handle_key` receives `J` and passes it to `plugins.handle_key(0, key)`. The `J` is consumed by the sidebar as an unknown key — `PluginAction::None` is returned and focus remains `Focus::Sidebar(0)`. AboutPlugin does NOT open. |
| Verification | Manual: confirm About screen does not appear after Tab+J. The `J` key has no meaning to any existing sidebar plugin, so it is silently swallowed. |

- [ ] Pass

**Rationale:** In `app.rs` `handle_key()`, the sidebar focus branch (`Focus::Sidebar(idx)`) is checked before the screen dispatch. When focus is Sidebar, only `Esc`/`Tab` return from the sidebar and unfocus it; all other keys go to `plugins.handle_key(idx, key)`. The dashboard key handler (which maps `J` → `OpenPlugin`) is never reached.

---

### 1.4 — Cross-screen navigation: non-Dashboard → back → J → AboutPlugin

| Field | Value |
|---|---|
| Precondition | App on IssueBoard screen (`screen = ScreenId::IssueBoard`) |
| Input sequence | `Esc` (back to Dashboard), then `J` |
| Expected result | After `Esc`: screen = `ScreenId::Dashboard`. After `J`: screen = `ScreenId::Plugin("about")`. |
| Verification | Manual: navigate to issue board, return, open About. |

- [ ] Pass

---

### 1.5 — AboutPlugin → J is a no-op inside About

| Field | Value |
|---|---|
| Precondition | App on AboutPlugin screen |
| Input sequence | `J` |
| Expected result | AboutPlugin returns `PluginAction::None`. Screen does not change. No recursive open. |
| Verification | Manual: confirm nothing happens. `AboutPlugin::handle_key` has no branch for `J`, so it falls through to the wildcard `_ => PluginAction::None`. |

- [ ] Pass

---

## 2. Keybinding Tests

### 2.1 — Uppercase J opens AboutPlugin from Dashboard

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `focus = Focus::Main`, no modal |
| Input | `J` (KeyCode::Char('J'), no modifiers) |
| Expected result | `dashboard::handle_key` returns `Action::OpenPlugin("about".to_string())`. App transitions to `ScreenId::Plugin("about")`. |
| Verification | Unit test in `dashboard.rs` or `app.rs`: dispatch `KeyCode::Char('J')` → assert action == `Action::OpenPlugin("about")`. Manual: confirm About screen appears. |

- [ ] Pass

---

### 2.2 — Lowercase j does NOT open AboutPlugin (navigates down)

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, at least one project in list |
| Input | `j` (KeyCode::Char('j'), no modifiers) |
| Expected result | `dashboard::handle_key` returns `Action::Down`. Selection moves down by one. Screen remains `ScreenId::Dashboard`. |
| Verification | Unit test: dispatch `KeyCode::Char('j')` → assert action == `Action::Down`, `app.screen == ScreenId::Dashboard`. |

- [ ] Pass

---

### 2.3 — J from ProjectView: expected behavior

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::ProjectView("some-slug")` |
| Input | `J` |
| Expected result | `project_view::handle_key` does not handle `J` (no binding for it there). It falls through to `_ => Action::None`. Screen remains ProjectView. AboutPlugin does NOT open. |
| Verification | Read `project_view.rs` to confirm no `J` handler. Manual: open a project, press `J`, confirm nothing happens. |

- [ ] Pass

**Note for implementors:** `J` is intentionally wired only in `dashboard::handle_key`. If future phases want `J` accessible globally, a pre-screen dispatch branch in `app.rs::handle_key` would be needed. Phase 0 does not do this.

---

### 2.4 — J from IssueBoard: expected behavior

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::IssueBoard` |
| Input | `J` |
| Expected result | `issue_board::handle_key` does not handle `J`. Falls to `Action::None`. About screen does NOT open. |
| Verification | Manual: go to issue board, press `J`, nothing happens. |

- [ ] Pass

---

### 2.5 — J from WeeklyReview: expected behavior

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Weekly` |
| Input | `J` |
| Expected result | `weekly::handle_key` does not handle `J`. Falls to `Action::None`. About screen does NOT open. |
| Verification | Manual: open weekly review, press `J`, nothing happens. |

- [ ] Pass

---

### 2.6 — Esc from AboutPlugin returns to Dashboard (flat navigation)

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `Esc` |
| Expected result | `AboutPlugin::handle_key` returns `PluginAction::Back`. `app.handle_back()` is called. `on_leave()` is called on AboutPlugin. `screen` is set to `ScreenId::Dashboard` (NOT the previous screen — there is no stack). `focus` is set to `Focus::Main`. |
| Verification | Manual. Unit: confirm `app.screen == ScreenId::Dashboard` after dispatch. Confirm no `screen_stack` field exists on App. |

- [ ] Pass

---

### 2.7 — q from AboutPlugin returns to Dashboard (not quit)

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `q` |
| Expected result | Same result as 2.6 — returns to Dashboard, does NOT set `app.should_quit = true`. |
| Verification | Unit: assert `app.should_quit == false` and `app.screen == ScreenId::Dashboard`. |

- [ ] Pass

---

## 3. Focus Management Tests

### 3.1 — Tab on AboutPlugin screen: no sidebar open

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `Tab` |
| Expected result | `AboutPlugin::handle_key` receives `Tab`. About does not define a handler for `Tab`, so it returns `PluginAction::None`. `focus` remains `Focus::Main`. No sidebar focus transition occurs. Sidebar is not rendered (hidden during plugin screen). |
| Verification | Manual: confirm Tab does nothing on About screen. |

- [ ] Pass

**Note:** `Action::FocusSidebar` is only reachable from `dashboard::handle_key` (and kanban handler) via `Tab`. Since all keys on the About screen go through `plugin.handle_key()`, Tab never reaches the sidebar focus logic.

---

### 3.2 — Focus state after returning from AboutPlugin

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `Esc` |
| Expected result | `handle_back()` sets `self.focus = Focus::Main`. Dashboard receives focus, border rendered in focused style. |
| Verification | Manual: verify dashboard panel border is highlighted after return. Unit: assert `app.focus == Focus::Main`. |

- [ ] Pass

---

### 3.3 — Sidebar was focused before pressing J

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `focus = Focus::Sidebar(0)` (Tab was pressed) |
| Input | `J` |
| Expected result | `handle_key` reaches the sidebar-focused branch BEFORE the screen dispatch. `J` is passed to `plugins.handle_key(0, key)`. Sidebar consumes it as a no-op. AboutPlugin does NOT open. `focus` remains `Focus::Sidebar(0)`. |
| Verification | Manual: press Tab to focus sidebar, then press J — About screen must not appear. |

- [ ] Pass

**Implementation note:** This is the same behavior as test 1.3. The sidebar focus gate in `handle_key` (lines ~439-448 in current `app.rs`) fires before the screen match. No changes needed for correct behavior — Phase 0 just needs to not break this gate.

---

### 3.4 — Focus state is NOT carried into AboutPlugin

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `focus = Focus::Sidebar(0)` |
| Input | `Tab` (unfocus sidebar), then `J` |
| Expected result | After Tab: `focus = Focus::Main` (sidebar unfocused via `handle_back`). After J: screen = `ScreenId::Plugin("about")`, `focus` remains `Focus::Main`. |
| Verification | Unit: assert `app.focus == Focus::Main` when `app.screen == ScreenId::Plugin("about")`. |

- [ ] Pass

---

## 4. Lifecycle Tests (observable behavior)

### 4.1 — on_enter() is called when J is pressed

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard` |
| Input | `J` |
| Expected result | `Action::OpenPlugin("about")` is processed by `update()`. Before setting `self.screen`, `plugin_registry.get_screen_mut("about")` is called and `plugin.on_enter()` is invoked. For `AboutPlugin`, `on_enter()` is a no-op, but the call itself must happen (future plugins depend on this ordering guarantee). |
| Verification | Add a call counter or log in `on_enter()` during development. Unit test: mock plugin or instrument AboutPlugin to set a flag; assert flag is true after J dispatch. |

- [ ] Pass

---

### 4.2 — on_leave() is called when Esc is pressed

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `Esc` |
| Expected result | `handle_back()` calls `on_leave()` on the AboutPlugin BEFORE setting `self.screen = ScreenId::Dashboard`. The ordering is: (1) identify current screen as Plugin, (2) call `on_leave()`, (3) set screen to Dashboard. |
| Verification | Same instrumentation approach as 4.1. Unit test: assert `on_leave()` was called before screen transition. |

- [ ] Pass

---

### 4.3 — Rapid open/close does not corrupt state

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard` |
| Input sequence | `J`, `Esc`, `J`, `Esc`, `J`, `Esc` (three rapid open/close cycles) |
| Expected result | After each cycle, `screen = ScreenId::Dashboard`, `focus = Focus::Main`, no panics, no zombie plugin state. Dashboard renders correctly with projects visible. |
| Verification | Manual: perform rapid open/close, confirm stable rendering. The clone-first borrow pattern in `handle_back()` must not accumulate side effects. |

- [ ] Pass

---

### 4.4 — AboutPlugin state is preserved across open/close cycles

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard` |
| Input sequence | `J`, `Esc`, `J` (open, close, reopen) |
| Expected result | AboutPlugin renders the same content on second open. No new allocation of AboutPlugin on each open — the existing instance in `plugin_registry.screens` is reused. `on_enter()` is called on each open. |
| Verification | Manual: confirm content identical on second open. By design: `AboutPlugin` is registered once in `PluginRegistry::new()` and never recreated. |

- [ ] Pass

---

## 5. Key Hints Bar Tests

### 5.1 — Dashboard shows normal dashboard hints

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `focus = Focus::Main`, no modal |
| Expected result | Footer bar shows: `j/k:nav  Enter:open  w:work  s:switch  m:meeting  n:note  b:block  /:search  W:weekly  ?:help` |
| Verification | Manual: launch app, confirm footer bar content. |

- [ ] Pass

---

### 5.2 — AboutPlugin shows "Esc: back" hint

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Expected result | Footer bar shows at minimum `Esc:back`. If `q` is also listed in `AboutPlugin::key_hints()`, it should appear too. Dashboard hints must NOT be visible. |
| Verification | Manual: open About, inspect footer. The `keyhints::get_hints()` function matches `ScreenId::Plugin(ref name)` and calls `plugin.key_hints()` — result replaces all other hints. |

- [ ] Pass

**Implementation note:** `AboutPlugin::key_hints()` per spec returns `vec![("Esc", "back")]`. If `q` should also appear in the hint bar (it is a valid exit key), implementors should add `("q", "back")` to the vec. This is a UX decision: document it here so the implementor makes it explicit rather than leaving it undocumented.

**Recommendation:** Add both `("Esc", "back")` and `("q", "back")` to `AboutPlugin::key_hints()` so users know q works. This matches the pattern used in the Switch screen where both `Enter` and `Esc` are shown.

---

### 5.3 — Returning to Dashboard restores dashboard hints

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")`, About hints showing |
| Input | `Esc` |
| Expected result | Immediately after return, footer renders full dashboard hint set (test 5.1). No leftover About hints visible. |
| Verification | Manual: watch footer bar change on Esc. |

- [ ] Pass

---

### 5.4 — Hint bar does not flicker during transition

| Field | Value |
|---|---|
| Precondition | App running |
| Input sequence | `J`, `Esc` (one open/close cycle) |
| Expected result | Footer bar transitions cleanly. No frame where it shows a blank or incorrect hint set. Because rendering is synchronous within a single frame draw, and `screen` is updated atomically before the next `terminal.draw()` call, there should be no intermediate blank frame. |
| Verification | Manual: observe at normal terminal speed. If flicker is observed, check that the screen field update and the render call are on the same frame cycle. |

- [ ] Pass

---

### 5.5 — Sidebar focused hint set is unaffected by AboutPlugin addition

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `focus = Focus::Sidebar(0)` |
| Expected result | Footer shows sidebar hints: `Tab:back  Space:start/pause  +/-:adjust  r:reset  R:reset all`. This code path in `get_hints()` fires before the screen match and must remain unchanged. |
| Verification | Manual: Tab to sidebar, confirm footer. |

- [ ] Pass

---

## 6. Regression Tests

### 6.1 — Existing dashboard keybindings still work

Test each of the following from `screen = ScreenId::Dashboard`, `focus = Focus::Main`:

| Key | Expected Action | [ ] |
|-----|----------------|-----|
| `j` | Navigate down (selection moves) | [ ] |
| `k` | Navigate up (selection moves) | [ ] |
| `Enter` | Opens ProjectView for selected project | [ ] |
| `w` | `Action::StartWork` → start work or open Switch | [ ] |
| `s` | Opens Switch screen | [ ] |
| `m` | `Action::MeetingMode` → meeting modal | [ ] |
| `n` | Opens QuickNote input modal | [ ] |
| `b` | Opens QuickBlocker input modal | [ ] |
| `d` | Opens QuickDecision input modal | [ ] |
| `/` | Opens Search screen | [ ] |
| `r` | Opens Review screen | [ ] |
| `p` | Opens People screen | [ ] |
| `a` | Opens AddProject input modal | [ ] |
| `I` | Opens IssueBoard screen | [ ] |
| `W` | Opens Weekly screen | [ ] |
| `K` | Toggles Kanban/List view | [ ] |
| `g` | Jumps to first project | [ ] |
| `G` | Jumps to last project | [ ] |
| `?` | Opens Help modal | [ ] |
| `q` | Quits app | [ ] |
| `P` | Toggles sidebar visibility | [ ] |
| `Tab` | Focuses sidebar | [ ] |
| `O` | Opens selected project in external editor | [ ] |
| `:` | Opens command palette | [ ] |
| `Ctrl+E` | Exports screen | [ ] |
| `Ctrl+D` | Half-page down | [ ] |
| `Ctrl+U` | Half-page up | [ ] |

**After Phase 0 implementation:** run through this table to confirm no key was accidentally captured by the new `J` handler or by the new `ScreenId::Plugin(_)` match arms.

---

### 6.2 — Sidebar keybindings still work after PluginSidebar → SidebarPlugin migration

| Test | Expected | [ ] |
|------|---------|-----|
| Tab from dashboard focuses sidebar | `focus = Focus::Sidebar(0)` | [ ] |
| Esc from sidebar unfocuses | `focus = Focus::Main` | [ ] |
| Tab from sidebar unfocuses | `focus = Focus::Main` (same as Esc, per current code) | [ ] |
| Space on Pomodoro starts/pauses | Timer state changes | [ ] |
| `+` on Pomodoro increases work time | Duration changes | [ ] |
| `-` on Pomodoro decreases work time | Duration changes | [ ] |
| `r` on Pomodoro resets current | Timer resets to full | [ ] |
| `R` on Pomodoro resets all | All timers reset | [ ] |
| `c` on Notifications clears | Notification list emptied | [ ] |
| Clock renders current time | Time shown in sidebar | [ ] |
| Sidebar renders correctly with `sidebar_visible = true` | Plugins visible | [ ] |
| Sidebar hidden with `P` key | Sidebar disappears | [ ] |
| `P` again restores sidebar | Sidebar reappears | [ ] |

---

### 6.3 — Sidebar is hidden when AboutPlugin is active

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, `sidebar_visible = true` |
| Input | `J` |
| Expected result | While `screen = ScreenId::Plugin("about")`, sidebar is NOT rendered. Full terminal width is given to the plugin. Specifically: the render loop for `ScreenId::Plugin(_)` must NOT split the layout for a sidebar panel, regardless of `self.sidebar_visible`. |
| Verification | Manual: open About with sidebar visible, confirm no sidebar column on the right. |

- [ ] Pass

---

### 6.4 — Sidebar visibility flag is NOT changed by AboutPlugin

| Field | Value |
|---|---|
| Precondition | `sidebar_visible = true` |
| Input sequence | `J` (open About), `Esc` (return to Dashboard) |
| Expected result | `sidebar_visible` is still `true` after returning. The sidebar reappears on Dashboard. |
| Verification | Manual: open About, return, confirm sidebar is back. Unit: assert `app.sidebar_visible == true`. |

- [ ] Pass

---

### 6.5 — All existing screen transitions still work

| Transition | How to trigger | Expected | [ ] |
|------------|---------------|---------|-----|
| Dashboard → ProjectView | `Enter` on selected project | ProjectView renders | [ ] |
| ProjectView → Dashboard | `Esc` | Returns to Dashboard | [ ] |
| Dashboard → IssueBoard | `I` | Issue kanban renders | [ ] |
| IssueBoard → Dashboard | `Esc` | Returns to Dashboard | [ ] |
| Dashboard → Weekly | `W` | Weekly review renders | [ ] |
| Weekly → Dashboard | `Esc` or `q` | Returns to Dashboard | [ ] |
| Dashboard → Review | `r` | Morning review renders | [ ] |
| Review → Dashboard | `Esc` | Returns to Dashboard | [ ] |
| Dashboard → Search | `/` | Search screen renders | [ ] |
| Search → Dashboard | `Esc` | Returns to Dashboard | [ ] |
| Dashboard → People | `p` | People screen renders | [ ] |
| People → Dashboard | `Esc` | Returns to Dashboard | [ ] |
| Dashboard → Switch | `s` | Switch wizard renders | [ ] |
| Switch → Dashboard | `Esc` (cancel) | Returns to Dashboard | [ ] |

---

### 6.6 — Toast notifications still appear during plugin screen

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")`, Pomodoro timer active |
| Expected result | If Pomodoro fires a notification while About is open, the toast overlay still appears on top of the About screen. Toast rendering is independent of screen type (it's drawn in step 4 of the render pipeline regardless of current screen). |
| Verification | Manual: start a Pomodoro timer, open About screen, wait for notification. |

- [ ] Pass

---

### 6.7 — Help modal (?): screen name label for plugin screens

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Plugin("about")` |
| Input | `?` |
| Expected result | Per the spec, `AboutPlugin::handle_key` handles all keys. `?` is not in AboutPlugin's handler, so it returns `PluginAction::None`. The Help modal does NOT open from a plugin screen (plugins handle their own help if desired). |
| Verification | Manual: press `?` while on About screen — nothing should happen. |

- [ ] Pass

**Note:** If a future screen plugin wants to expose `?` for help, it should return `PluginAction::None` for `?` and render help inline, or handle it internally. The app's `Action::Help` path is not reachable from plugin screens.

---

## 7. Edge Cases and Boundary Conditions

### 7.1 — Unknown plugin name in ScreenId::Plugin

| Field | Value |
|---|---|
| Scenario | Somehow `self.screen = ScreenId::Plugin("nonexistent")` is set (e.g., future state deserialization bug) |
| Expected result | `plugin_registry.get_screen("nonexistent")` returns `None`. Render loop renders nothing (or a blank area). Key handler receives no plugin, returns `PluginAction::None`. No panic. |
| Verification | Unit test: construct `ScreenId::Plugin("nonexistent".to_string())` and call render/key-handler paths — assert no panic. |

- [ ] Pass

---

### 7.2 — Modal open while on AboutPlugin screen

| Field | Value |
|---|---|
| Scenario | A toast fires `Action::Toast(...)` while `screen = ScreenId::Plugin("about")` — this pushes nothing, but if somehow a modal is on the stack |
| Expected result | Modal takes priority in `handle_key()` (modal check runs before screen dispatch). The plugin's `handle_key` is NOT called while a modal is open. |
| Verification | The existing modal priority check in `handle_key()` at line ~434 in `app.rs` already handles this — no new code needed. Verify by inspection that the modal check comes before the `ScreenId::Plugin(_)` dispatch. |

- [ ] Pass

---

### 7.3 — J pressed while modal is open on Dashboard

| Field | Value |
|---|---|
| Precondition | `screen = ScreenId::Dashboard`, a modal is on the modal stack |
| Input | `J` |
| Expected result | Modal handles the key (or ignores it). `dashboard::handle_key` is NOT called. About screen does NOT open. |
| Verification | Manual: open any modal (e.g., `?` for Help), press `J` — About must not open. |

- [ ] Pass

---

## 8. Test Execution Order

Run tests in this recommended order to catch regressions early:

1. **6.1** — Confirm all dashboard keys still work before testing new ones
2. **6.2** — Confirm sidebar migration didn't break existing plugins
3. **2.1** and **2.2** — Confirm J/j distinction is correct
4. **1.1** — Basic open/close smoke test
5. **1.2** — q-to-back confirmation
6. **5.2** — Hint bar shows correct content in About
7. **5.3** — Hint bar restores on return
8. **4.3** — Rapid open/close stability
9. **1.3** and **3.3** — Sidebar-focus guard
10. **4.1** and **4.2** — Lifecycle hook ordering
11. **6.3** and **6.4** — Sidebar hidden/restored
12. **6.5** — All existing screen transitions
13. **7.1** and **7.2** — Edge cases

---

## 9. Automation Notes

The following tests are good candidates for unit tests in `crates/jm-tui/`:

| Test | Suggested location |
|------|--------------------|
| 2.1, 2.2 — J/j dispatch | `crates/jm-tui/src/screens/dashboard.rs` test module |
| 2.6, 2.7 — Esc/q on AboutPlugin | `crates/jm-tui/src/plugins/about.rs` test module |
| 4.1, 4.2 — lifecycle hook ordering | `crates/jm-tui/src/plugins/about.rs` or `registry.rs` test module |
| 7.1 — unknown plugin name | `crates/jm-tui/src/plugins/registry.rs` test module |
| 3.2, 3.4 — Focus::Main after return | Integration test in `app.rs` or a dedicated test file |

Tests involving full render output (hint bar content, sidebar visibility) are best verified manually or with a headless terminal test harness if one is added later.

---

*End of interaction test plan.*
