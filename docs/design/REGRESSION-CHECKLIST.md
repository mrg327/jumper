# Phase 0 Plugin System Rewrite — Regression Test Checklist

QA Engineer 2 | Prepared against commit baseline `318a931`

This checklist verifies that nothing existing breaks during the Phase 0 rewrite.
Check each box after manual or automated verification.

---

## 1. Sidebar Plugin Regression

### 1.1 Clock Plugin

> Source of truth: `crates/jm-tui/src/plugins/clock.rs`
> Clock is stateless. Its only job is rendering the current time from `chrono::Local::now()`.

- [ ] Clock plugin appears in the sidebar when `"clock"` is listed in `config.plugins.enabled`
- [ ] Clock renders HH:MM time (24-hour format via `%H:%M`) on line 1 of its inner area
- [ ] Clock renders day+month+date (e.g. `Fri Mar 27`) on line 2 via `%a %b %d`
- [ ] Time text is styled with `theme::accent()` (cyan/highlight)
- [ ] Date text is styled with `theme::dim()` (gray)
- [ ] Both lines are center-aligned within the plugin's inner area
- [ ] Clock block title reads `"Clock"` with `Borders::ALL`
- [ ] Clock height is exactly 4 rows (2 content + 2 border)
- [ ] Clock updates every second (re-render reflects new time after 1s)
- [ ] Clock does NOT consume any key events (all keys pass through to the next handler)
- [ ] Clock does NOT emit any notification messages from `on_tick()`
- [ ] Clock `needs_timer()` returns `true` — tick system must call it every second

### 1.2 Notifications Plugin

> Source of truth: `crates/jm-tui/src/plugins/notifications.rs`
> Notifications collects messages from other plugins via `on_notify()` and fires scheduled reminders.

- [ ] Notifications plugin appears in the sidebar when `"notifications"` is listed in `config.plugins.enabled`
- [ ] Block title reads `"Notifs (N)"` where N is the current notification count
- [ ] When notification list is empty, shows `"No notifications"` centered in dim style
- [ ] Notifications are listed newest-first (most recent at top — inserted at index 0)
- [ ] Each notification line shows `"HH:MM message"` format
- [ ] Maximum 5 notifications visible at once (overflow is truncated)
- [ ] Maximum 10 notifications stored (oldest are dropped when limit is reached)
- [ ] Exact duplicate messages arriving in the same tick are deduplicated (no double entry)
- [ ] Plugin height is dynamic: `3 + min(count, 5)` rows (border + header + up to 5 items)
- [ ] `'c'` key clears all notifications (list becomes empty)
- [ ] `'c'` key returns `true` (key consumed), no other key returns `true`
- [ ] `needs_timer()` returns `true`

**Reminders:**
- [ ] Reminders configured in `config.plugins.notifications.reminders` are parsed at startup
- [ ] A reminder fires when `now` is within 0–59 seconds after its scheduled `HH:MM` time
- [ ] A reminder that fires once is added to `fired_today` and does NOT fire again the same day
- [ ] Fired reminder message is pushed into the notification list
- [ ] Fired reminder message is returned from `on_tick()` so the sidebar can forward it to toast

**Cross-plugin routing (sidebar behavior):**
- [ ] Messages emitted by Pomodoro's `on_tick()` are forwarded into `NotificationsPlugin::on_notify()`
- [ ] Messages emitted by other plugins (non-Notifications) are forwarded to Notifications
- [ ] Notifications plugin does NOT forward its own messages back to itself

**Auto-expiry:**
- [ ] Each tick decrements `expires_in` by 1 for every notification
- [ ] Notifications with `expires_in == 0` are removed from the list on the next tick
- [ ] New notifications start with `expires_in = 1800` (30-minute window)

### 1.3 Pomodoro Plugin

> Source of truth: `crates/jm-tui/src/plugins/pomodoro.rs`
> Pomodoro is a state machine: Idle → Work → (Short|Long)Break → Work → ... → Idle.

- [ ] Pomodoro plugin appears in the sidebar when `"pomodoro"` is listed in `config.plugins.enabled`
- [ ] Plugin height is exactly 6 rows (4 content lines + 2 border)
- [ ] Block title reads `"Pomodoro"`
- [ ] `needs_timer()` returns `true`

**Rendering by state:**
- [ ] `Idle`: label `"🍅 IDLE"`, time shows `"Ready (N:00)"` where N is `work_minutes`, dim style
- [ ] `Work`: label `"🍅 WORK S/T"` (S = session+1, T = sessions_before_long), green style
- [ ] `Work`: countdown shows `MM:SS` in bold
- [ ] `ShortBreak`: label `"🍅 SHORT BREAK"`, yellow/warning style, countdown shows `MM:SS`
- [ ] `LongBreak`: label `"🍅 LONG BREAK"`, yellow/warning style, countdown shows `MM:SS`
- [ ] `Paused(Work)`: label `"🍅 PAUSED (WORK)"`, dim style, countdown frozen at pause time
- [ ] `Paused(ShortBreak)`: label `"🍅 PAUSED (SHORT BRK)"`, dim style
- [ ] `Paused(LongBreak)`: label `"🍅 PAUSED (LONG BRK)"`, dim style
- [ ] Line 3: session dots — `●` for completed sessions, `○` for remaining, count = `sessions_before_long`
- [ ] Session dots reflect progress modulo `sessions_before_long` (e.g., after 4 sessions in a cycle of 4, dots reset)
- [ ] Line 4 hint: `"Spc:start"` when Idle, `"Spc:resume"` when Paused, `"Spc:pause"` when running

**Key handling:**
- [ ] `Space` in Idle: transitions to Work state, resets timer to `work_minutes * 60`
- [ ] `Space` in Work/ShortBreak/LongBreak: pauses, wraps state as `Paused(current_state)`
- [ ] `Space` in Paused: resumes inner state, restarts `last_tick` tracking
- [ ] `+` or `=`: adds 5 minutes (300 seconds) to `remaining_secs`
- [ ] `-` or `_`: subtracts 5 minutes (300 seconds), minimum floor of 60 seconds
- [ ] `r`: resets `remaining_secs` to the configured default for the current state (or inner state if Paused)
- [ ] `r` in Idle: resets to `work_minutes * 60`
- [ ] `R` (uppercase): full reset — back to Idle, `session_count = 0`, timer reset to `work_minutes * 60`
- [ ] All handled keys return `true` (consumed); all others return `false`

**Timer tick behavior:**
- [ ] Timer only counts down when state is Work, ShortBreak, or LongBreak (not Idle, not Paused)
- [ ] First tick after entering a running state initializes `last_tick` and returns no messages
- [ ] Subsequent ticks subtract elapsed wall-clock seconds from `remaining_secs`
- [ ] If `remaining_secs` would go to 0, timer expires and transitions to next state

**State transitions on expiry:**
- [ ] Work expires: `session_count` increments, transitions to ShortBreak (unless `session_count % sessions_before_long == 0`, then LongBreak)
- [ ] Work expiry notification: `"🍅 Work session complete! Time for a break."` emitted
- [ ] Work expiry also emits the transition message (e.g., `"Short break — stretch and breathe."`)
- [ ] ShortBreak expires: transitions back to Work, emits `"☕ Break over — back to work!"`
- [ ] LongBreak expires: transitions to Idle, emits `"🎉 Long break complete. Resetting Pomodoro."`
- [ ] LongBreak expiry also emits `"Work session N started. Focus!"` (from `transition()`)
- [ ] All expiry notifications are returned from `on_tick()` as `Vec<String>` (forwarded to Notifications)

**Config from `config.yaml`:**
- [ ] `work_minutes` configures work session length (default 25)
- [ ] `short_break_minutes` configures short break length (default 5)
- [ ] `long_break_minutes` configures long break length (default 15)
- [ ] `sessions_before_long` configures cycle length (default 4)

---

## 2. Sidebar Container Regression

> Source of truth: `crates/jm-tui/src/plugins/sidebar.rs`
> The sidebar is a 22-column right panel managed by `PluginSidebar`.

**Layout and rendering:**
- [ ] Sidebar renders at 22 columns wide (right panel of dashboard layout)
- [ ] Sidebar has an outer `"Plugins"` block with `Borders::ALL`
- [ ] Outer border is `theme::focused_border()` when focused, `theme::unfocused_border()` otherwise
- [ ] Plugins are stacked vertically within the sidebar's inner area
- [ ] A plugin that would overflow the available vertical space is skipped (not rendered, no panic)
- [ ] Plugins render in the order defined by `config.plugins.enabled`

**Focus and keyboard routing:**
- [ ] `Tab` on the dashboard moves focus from `Focus::Main` to `Focus::Sidebar(0)` (first plugin)
- [ ] While `Focus::Sidebar(idx)` is active, `j` or arrow-down advances to the next plugin index
- [ ] While `Focus::Sidebar(idx)` is active, `k` or arrow-up retreats to the previous plugin index
- [ ] Focus wraps or clamps at the boundaries (does not panic on index 0 or last index)
- [ ] `Esc` while sidebar is focused returns focus to `Focus::Main`
- [ ] `Tab` while sidebar is focused cycles back to `Focus::Main`
- [ ] Key events while `Focus::Sidebar(idx)` are forwarded to `sidebar.handle_key(idx, key)`
- [ ] `handle_key()` returns `true` if the plugin consumed the key, `false` otherwise
- [ ] Unconsumed sidebar keys do NOT propagate to dashboard actions

**Focus highlight:**
- [ ] When `Focus::Sidebar(idx)` is active, the focused plugin's top border row is redrawn with `theme::focused_border()` color
- [ ] When focus is on plugin 0, only plugin 0's top border is highlighted
- [ ] When focus moves to plugin 1, plugin 1's border is highlighted and plugin 0's is not

**Sidebar toggle:**
- [ ] `P` key on the dashboard fires `Action::ToggleSidebar`
- [ ] `Action::ToggleSidebar` toggles `app.sidebar_visible` between `true` and `false`
- [ ] When `sidebar_visible` is `false`, the sidebar panel is not rendered (full width for main content)
- [ ] When `sidebar_visible` is `true`, the sidebar panel is rendered at 22 columns
- [ ] Toggling sidebar to hidden while sidebar is focused resets focus to `Focus::Main`

**Tick routing:**
- [ ] `on_tick()` is called on the sidebar every 1 second
- [ ] `on_tick()` only calls `plugin.on_tick()` for plugins where `needs_timer()` returns `true`
- [ ] Messages from non-Notifications plugins returned by `on_tick()` are forwarded to NotificationsPlugin via `on_notify()`
- [ ] The full set of all plugin messages (including Notifications' own) is returned to the app for toast display
- [ ] Toast notifications from plugin ticks appear as on-screen toasts in the TUI

---

## 3. Dashboard Regression

> Source of truth: `crates/jm-tui/src/screens/dashboard.rs` and `CLAUDE.md`

**Project list rendering:**
- [ ] Project list renders on the left 40% of the dashboard area
- [ ] Active project is marked with `"▶ "` prefix
- [ ] Project status badge is shown for each project
- [ ] Projects are sorted: Active → Blocked → Pending → Parked → Done, then alphabetically within each group
- [ ] Stale age badge appears for Active/Blocked/Pending projects (not for Done/Parked)
- [ ] When no projects exist, shows `"No projects yet. Press a to create one."` placeholder
- [ ] Selection highlight moves with `j`/`k`
- [ ] Scrolling works: large project lists scroll to keep selection visible

**Right panel / preview:**
- [ ] Right panel (60% width) shows project detail preview for selected project
- [ ] Preview updates as selection changes

**Kanban view:**
- [ ] `K` toggles between list and kanban view
- [ ] Kanban shows columns for Active, Blocked, Pending, Parked, Done statuses
- [ ] `h`/`l` navigate between kanban columns
- [ ] `j`/`k` navigate within a kanban column
- [ ] `K` again returns to list view, preserving selection at the kanban-selected project

**All list-view keybindings (from `CLAUDE.md`):**
- [ ] `j` / Down — navigate down
- [ ] `k` / Up — navigate up
- [ ] `g` — jump to top (first project)
- [ ] `G` — jump to bottom (last project)
- [ ] `Ctrl+D` — half-page down
- [ ] `Ctrl+U` — half-page up
- [ ] `Enter` — open project (navigate to ProjectView)
- [ ] `K` — toggle kanban/list view
- [ ] `w` — start working on selected project
- [ ] `s` — switch context (opens Switch screen)
- [ ] `m` — quick meeting mode switch
- [ ] `n` — quick note on active project
- [ ] `b` — log blocker on active project
- [ ] `d` — log decision on active project
- [ ] `u` — unblock (resolve a blocker)
- [ ] `/` — open search screen
- [ ] `r` — morning review screen
- [ ] `p` — people view screen
- [ ] `a` — add new project (modal)
- [ ] `i` — add issue (modal)
- [ ] `f` — stop work / break / done for day (StopWork action)
- [ ] `I` — open issue board screen
- [ ] `W` — open weekly review screen
- [ ] `?` — open help modal (all keybindings)
- [ ] `q` — quit application
- [ ] `P` — toggle plugin sidebar visibility
- [ ] `Tab` — focus plugin sidebar
- [ ] `O` — open selected project in external editor
- [ ] `:` — open command palette
- [ ] `Ctrl+E` — export screen dump to stdout

**All kanban-view keybindings:**
- [ ] `h` / Left — navigate columns left
- [ ] `l` / Right — navigate columns right
- [ ] `j` / Down — navigate rows down within column
- [ ] `k` / Up — navigate rows up within column
- [ ] `Enter` — open selected project
- [ ] `K` — return to list view
- [ ] `w` — start work
- [ ] `m` — meeting mode
- [ ] `a` — add project
- [ ] `i` — add issue
- [ ] `I` — open issue board
- [ ] `W` — open weekly review
- [ ] `?` — help modal
- [ ] `q` — quit
- [ ] `P` — toggle sidebar
- [ ] `Tab` — focus sidebar

**Screen transitions from dashboard:**
- [ ] `Enter` → ProjectView for selected project
- [ ] `s` or `w` → Switch screen opens with context capture
- [ ] `/` → Search screen opens
- [ ] `r` → Review screen opens
- [ ] `p` → People screen opens
- [ ] `I` → IssueBoard screen opens
- [ ] `W` → Weekly screen opens

**Key hints footer:**
- [ ] Footer shows correct hints for list view dashboard
- [ ] Footer shows different hints when kanban view is active
- [ ] Footer shows `"Enter:submit  Esc:cancel"` when a modal is open
- [ ] Footer shows sidebar hints (`"Tab:back  Space:start/pause  +/-:adjust  r:reset  R:reset all"`) when sidebar is focused

**Export:**
- [ ] `Ctrl+E` triggers `Action::Export`
- [ ] Export produces ANSI-free text output

---

## 4. All Screen Regression

### 4.1 ProjectView Screen

> Source of truth: `crates/jm-tui/src/screens/project_view.rs`

- [ ] Opens correctly when `Enter` is pressed on a project in the dashboard
- [ ] Displays project name, status, priority, tags, and target date
- [ ] Displays current focus/goal text
- [ ] Displays log entries (notes, blockers, decisions)
- [ ] Displays list of issues for this project
- [ ] Active (pinned) issue is highlighted
- [ ] `Esc` or `q` returns to dashboard
- [ ] `e` — opens edit focus modal
- [ ] `x` — pins/unpins the active issue
- [ ] `i` — opens add issue modal
- [ ] `N` — note-to-issue: adds a note to the selected issue
- [ ] `s` — cycles project status (Active → Blocked → Pending → Parked → Done)
- [ ] `c` — closes (marks Done) the selected issue
- [ ] `n` — quick note on this project
- [ ] `b` — log blocker on this project
- [ ] `o` — opens project file in `$EDITOR`
- [ ] Key hints footer shows ProjectView-specific hints

### 4.2 IssueBoard Screen

> Source of truth: `crates/jm-tui/src/screens/issue_board.rs` and `CLAUDE.md`

- [ ] Opens correctly from dashboard via `I` key
- [ ] Displays issues in kanban columns (Backlog, In Progress, In Review, Done)
- [ ] Issues are grouped by status column
- [ ] `h`/`l` — navigate between columns
- [ ] `j`/`k` — navigate within column
- [ ] `g`/`G` — jump to top/bottom within column
- [ ] `Enter` or `s` — advance issue status forward
- [ ] `S` — reverse issue status backward
- [ ] `c` — close issue (set to Done)
- [ ] `p` — cycle project filter (all → per-project → all)
- [ ] `D` — toggle Done column visibility
- [ ] `o` — open the issue's parent project in ProjectView
- [ ] `Esc` returns to dashboard
- [ ] Key hints footer shows IssueBoard-specific hints

### 4.3 WeeklyReview Screen

> Source of truth: `crates/jm-tui/src/screens/weekly.rs` and `CLAUDE.md`

- [ ] Opens correctly from dashboard via `W` key
- [ ] Displays weekly activity chart (last 7 days of journal activity)
- [ ] Displays completed issues section
- [ ] Displays open blockers section
- [ ] `Tab` — cycles between sections
- [ ] `j`/`k` — navigate items within the active section
- [ ] `g`/`G` — jump to top/bottom within section
- [ ] `W` key (uppercase) — toggles back to dashboard
- [ ] `Esc` or `q` — returns to dashboard
- [ ] Key hints footer shows Weekly-specific hints

### 4.4 Search Screen

> Source of truth: `crates/jm-tui/src/screens/search.rs`

- [ ] Opens correctly from dashboard via `/` key
- [ ] Text input field is focused and accepts typed characters
- [ ] Search results update as user types
- [ ] Results show project name and matching excerpt
- [ ] `j`/`k` — navigate through search results
- [ ] `Enter` — opens the selected result's project in ProjectView
- [ ] `Esc` — returns to dashboard without opening anything
- [ ] Key hints footer shows Search-specific hints

### 4.5 People Screen

> Source of truth: `crates/jm-tui/src/screens/people.rs`

- [ ] Opens correctly from dashboard via `p` key
- [ ] Lists all stakeholders extracted from `@mentions` in project/journal files
- [ ] Shows last seen date and associated projects for each person
- [ ] `j`/`k` — navigate the list
- [ ] `Esc` — returns to dashboard
- [ ] Key hints footer shows People-specific hints

### 4.6 MorningReview Screen

> Source of truth: `crates/jm-tui/src/screens/review.rs`

- [ ] Opens correctly from dashboard via `r` key
- [ ] Shows active projects, blockers, and agenda for the day
- [ ] `j`/`k` — navigate items
- [ ] `Tab` — cycle sections
- [ ] `Esc` — returns to dashboard

### 4.7 Switch Screen (Context-Switch Capture)

> Source of truth: `crates/jm-tui/src/screens/switch.rs`

- [ ] Opens correctly when `s` is pressed on dashboard (and when `w` triggers a context switch)
- [ ] Prompts for: current work state / blockers / next steps
- [ ] `Enter` advances through each step
- [ ] `Esc` at any step cancels and optionally saves context-only (no project switch)
- [ ] On completion (`SwitchComplete`), switches active project and logs context entry
- [ ] `Esc` at SelectProject step with captured context fires `Action::SaveContextOnly`

---

## 5. Automated Test Regression (`cargo test`)

> Run with: `cargo test` or `cargo test -p jm-core`

### 5.1 Property-Based Tests

> File: `crates/jm-core/tests/proptest_roundtrip.rs`

- [ ] `prop_project_roundtrip` passes (Project `to_markdown` → `from_markdown` is stable)
- [ ] `prop_project_name_with_yaml_special_chars` passes (THIS IS THE PRE-EXISTING FAILING TEST — Task 0 must fix it before this can be checked)
- [ ] `prop_journal_roundtrip` passes
- [ ] `prop_people_roundtrip` passes
- [ ] All proptest strategies use `arb_name()` which generates YAML-safe names (`[a-zA-Z][a-zA-Z0-9_-]{0,48}`)
- [ ] No proptest cases fail with a regex/strategy error

### 5.2 Real Data Round-Trip Tests

> File: `crates/jm-core/tests/real_data_roundtrip.rs`

- [ ] All real-data fixture round-trips pass
- [ ] No regressions introduced by any model changes

### 5.3 Storage Edge Case Tests

> File: `crates/jm-core/tests/storage_edge_cases.rs`

- [ ] All edge case tests pass
- [ ] Missing file, empty file, and malformed frontmatter cases are handled without panic

### 5.4 Overall Test Gate

- [ ] `cargo test` exits 0 with no test failures
- [ ] `cargo test -p jm-core` exits 0
- [ ] `cargo test -p jm-tui` exits 0 (if TUI tests exist)
- [ ] Zero new compiler warnings introduced by the rewrite (compare `cargo build 2>&1 | grep warning` before and after)
- [ ] No `unused import` warnings in any migrated plugin file
- [ ] No `unused variable` warnings from renamed fields (e.g., `self.plugins` → `self.plugin_registry`)

---

## 6. Build Regression

- [ ] `cargo build` (debug) succeeds with exit code 0
- [ ] `cargo build --release` (release) succeeds with exit code 0
- [ ] `./build-install.sh` succeeds and installs binary to `~/.local/bin/jm`
- [ ] Installed binary at `~/.local/bin/jm` exists and is executable
- [ ] `jm --version` or `jm --help` runs without immediate crash
- [ ] `jm` launches the TUI without immediate crash
- [ ] `jm --dump` produces ANSI-free text output to stdout
- [ ] `jm status` returns one-line status without crash
- [ ] `jm list` lists projects without crash
- [ ] `jm note "test regression"` writes a note without crash (requires active project set)

---

## 7. New Feature Smoke Tests (Phase 0 Acceptance)

These verify the new functionality introduced by the rewrite does not itself regress after implementation.

### 7.1 Trait Split

- [ ] `SidebarPlugin` and `ScreenPlugin` are separate traits with no shared supertrait
- [ ] `ClockPlugin` implements `SidebarPlugin` (not the old `Plugin` trait)
- [ ] `NotificationsPlugin` implements `SidebarPlugin`
- [ ] `PomodoroPlugin` implements `SidebarPlugin`
- [ ] `PluginAction` enum has exactly three variants: `None`, `Back`, `Toast(String)`

### 7.2 PluginRegistry

- [ ] `PluginRegistry` struct exists in `plugins/registry.rs`
- [ ] `App.plugin_registry` field replaces the old `App.plugins` field
- [ ] All 7 former `self.plugins` call sites in `app.rs` have been updated to `self.plugin_registry`
- [ ] `PluginRegistry::tick_sidebar()` ticks sidebar at 1s interval
- [ ] `PluginRegistry::tick_screen()` ticks only the active screen plugin at 250ms interval
- [ ] Inactive screen plugins receive no ticks

### 7.3 ScreenId::Plugin Variant

- [ ] `ScreenId::Plugin(String)` variant exists in `events.rs`
- [ ] `ScreenId` derives `Clone` (required for the clone-first borrow pattern)
- [ ] All match/if-let sites on `ScreenId` in `app.rs` handle the `Plugin(_)` arm (no exhaustiveness errors)
- [ ] `get_hints()` in `keyhints.rs` handles `ScreenId::Plugin(_)` and returns the plugin's `key_hints()`

### 7.4 AboutPlugin Demo Screen

- [ ] `AboutPlugin` exists in `plugins/about.rs`
- [ ] `J` (uppercase, Shift+J) on the dashboard opens the About screen
- [ ] About screen renders version/build info centered in the full terminal area
- [ ] Sidebar is NOT shown when About screen is active (full-width rendering)
- [ ] `Esc` on the About screen calls `on_leave()` and returns to dashboard
- [ ] `q` on the About screen also calls `on_leave()` and returns to dashboard
- [ ] `on_enter()` is called when the About screen is opened
- [ ] `on_leave()` is called when the About screen is closed (via Esc, q, or any `PluginAction::Back`)
- [ ] Key hints footer shows `"Esc:back"` when About screen is active
- [ ] `handle_back()` in `app.rs` uses the clone-first pattern (no borrow checker errors)
- [ ] `handle_key()` in `app.rs` for `ScreenId::Plugin(_)` uses the clone-first pattern

---

## Testing Notes

**Order of operations for manual testing:**

1. Run `cargo test` first — fix any automated failures before manual testing.
2. Launch `jm` TUI and verify the dashboard loads.
3. Check each plugin section (1.1–1.3) by looking at the sidebar while idle.
4. Test sidebar focus: `Tab` → navigate plugins → `Esc`.
5. Test pomodoro: focus it with `Tab`+`j/k`, then `Space` to start, wait a few ticks, `Space` to pause, `r` to reset.
6. Test notifications: trigger a pomodoro cycle transition and confirm the message appears in Notifications.
7. Walk through all dashboard keybindings.
8. Open each screen (`Enter`, `I`, `W`, `/`, `r`, `p`) and verify basic rendering and `Esc` returns.
9. Press `J` to open the About screen; confirm sidebar is hidden; press `Esc`.
10. Run `./build-install.sh` and confirm the installed binary works.

**Borrow-checker verification:** After Phase 0, confirm that `cargo build` produces zero errors related to "cannot borrow `*self` as mutable" in `app.rs` — the clone-first pattern must be applied correctly at `handle_back()` and `handle_key()`.
