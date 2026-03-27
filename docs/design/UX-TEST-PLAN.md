# UX Test Plan — Phase 0 Plugin System Rewrite

Prepared by: UX Engineer 1
Target: Phase 0 implementation of `SidebarPlugin` / `ScreenPlugin` trait split + `AboutPlugin` demo screen

---

## How to Use This Document

Each test case states: the **element** under test, the **pre-condition**, the **action**, and the **expected visual result**. The manual test script in Section 4 ties everything into a step-by-step walkthrough that can be run top-to-bottom by one person in under 5 minutes on a freshly built binary.

---

## 1. Rendering Test Cases

### 1.1 Sidebar Rendering (Regression)

The sidebar must be visually indistinguishable from the pre-Phase-0 baseline after the `Plugin` → `SidebarPlugin` trait migration.

#### TC-S-01: All three plugins visible in sidebar

| Field | Value |
|-------|-------|
| Pre-condition | Config has `plugins.enabled: [pomodoro, notifications, clock]`; terminal height >= 30 rows |
| Action | Launch `jm`; observe right sidebar panel |
| Expected | Three distinct plugin panels visible stacked top-to-bottom within the "Plugins" bordered block. Pomodoro top, Notifications middle, Clock bottom (order follows config). No panel bleeds outside its allocated height. |
| Theme reference | Outer block border: `BORDER_UNFOCUSED` (DarkGray) when focus is on Main |

#### TC-S-02: Sidebar outer border — unfocused state

| Field | Value |
|-------|-------|
| Pre-condition | App launched; focus is Main (default) |
| Action | Observe the outer "Plugins" block border |
| Expected | Border drawn in `BORDER_UNFOCUSED` (`Color::DarkGray`). Title "Plugins" visible at top-left of block. |

#### TC-S-03: Sidebar outer border — focused state

| Field | Value |
|-------|-------|
| Pre-condition | App launched |
| Action | Press `Tab` to give focus to sidebar |
| Expected | Outer "Plugins" block border switches to `BORDER_FOCUSED` (`Color::Cyan`). No other visual change. |

#### TC-S-04: Focused plugin highlighted within sidebar

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar is focused (Tab pressed once) |
| Action | Observe which plugin is highlighted; press `j` / `k` to cycle |
| Expected | The top border row of the active plugin is overwritten with `BORDER_FOCUSED` (`Color::Cyan`) style, giving a selection indicator. Other plugins show no highlighting. Cycling with j/k moves the highlight. |

#### TC-S-05: Sidebar focus release

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar is focused |
| Action | Press `Tab` again (or `Esc`) to return focus to Main |
| Expected | Outer border reverts to `BORDER_UNFOCUSED`; plugin highlight disappears. Dashboard key bindings resume normally. |

#### TC-S-06: Clock plugin display

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar visible |
| Action | Wait up to 2 seconds; observe Clock plugin panel |
| Expected | Shows current time (HH:MM or HH:MM:SS format). Updates each second. Text drawn in a bordered sub-area. Height does not exceed 4 rows as defined by `height() -> u16 { 4 }`. |

#### TC-S-07: Notifications plugin display

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar visible |
| Action | Observe Notifications panel; optionally press `n` to add a quick note (triggers a notification in some workflows) |
| Expected | Panel renders without panic even with zero notifications. When a notification is present, text appears inside the panel bounds. |

#### TC-S-08: Pomodoro plugin display and interaction

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar visible; sidebar focused at Pomodoro index |
| Action | Press `Space` to start timer; observe countdown; press `Space` again to pause |
| Expected | Timer counts down. Work/Break state label visible. Pause freezes display. All rendering stays inside the 6-row plugin area. No overflow into adjacent panels. |

---

### 1.2 AboutPlugin Screen Rendering

The AboutPlugin is a new `ScreenPlugin`. It must feel like a first-class screen (filling the full terminal area) not a floating widget.

#### TC-A-01: AboutPlugin opens full-screen

| Field | Value |
|-------|-------|
| Pre-condition | Dashboard is visible |
| Action | Press `J` (Shift+J) |
| Expected | The entire terminal area is replaced by the AboutPlugin render. No dashboard content visible. No sidebar visible. AboutPlugin uses the full terminal width and height (the `main_area` passed to `plugin.render()` is the full area, not split). |

#### TC-A-02: Version text is centered

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Observe the screen |
| Expected | "jm v0.1.0" (or current version string) is horizontally and vertically centered within the terminal. In an 80x24 terminal the text should appear near row 12, centered on column 40. In a 200x60 terminal the text should remain visually centered — neither left-justified nor off-screen. |

#### TC-A-03: Text readability and contrast

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Read the screen |
| Expected | All text uses colors from `theme.rs`. Primary content uses `TEXT_PRIMARY` (`Color::Reset`) or `TEXT_ACCENT` (`Color::Cyan`) for emphasis. No raw white-on-white or invisible text. Dark terminal themes and light terminal themes should both produce readable output (all 16 ANSI colors, respecting terminal theme). |

#### TC-A-04: Screen feels like a screen, not a widget

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Compare visually to other full-screen views (IssueBoard, Weekly) |
| Expected | No leftover 22-column sidebar gap on the right. No 40/60 split layout artifact. The render fills edge-to-edge. Border or background (if any) spans the full width. |

---

### 1.3 Sidebar Hiding When AboutPlugin Is Active

Sidebar hiding is a hard requirement in the spec. Any leftover border or column gap is a bug.

#### TC-H-01: Sidebar completely absent during AboutPlugin

| Field | Value |
|-------|-------|
| Pre-condition | Dashboard visible with sidebar showing |
| Action | Press `J` to open AboutPlugin |
| Expected | The "Plugins" bordered block is gone. The right ~22 columns that were previously occupied by the sidebar are now part of the plugin render area. No DarkGray border remnant. No blank column gap. No partial "Plugins" title character. |

#### TC-H-02: No sidebar layout split in render path

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is active |
| Action | Resize terminal; observe re-render |
| Expected | On every resize, the plugin still fills the full width. The layout code does not reintroduce the sidebar split for `ScreenId::Plugin(_)`. |

#### TC-H-03: Sidebar-visible state not permanently changed

| Field | Value |
|-------|-------|
| Pre-condition | Sidebar was visible before opening AboutPlugin |
| Action | Open AboutPlugin (J), then close (Esc) |
| Expected | After returning to dashboard, sidebar is visible again. The `sidebar_visible` flag is not permanently cleared. |

---

### 1.4 Return to Dashboard After AboutPlugin

#### TC-R-01: Dashboard renders correctly after Esc

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Press `Esc` |
| Expected | Full dashboard renders: project list on left (40%), preview on right (60%), sidebar in a separate 22-column panel. All previously visible projects are still listed. Selected row is unchanged from before opening AboutPlugin. No visual artifacts from the AboutPlugin remain. |

#### TC-R-02: Dashboard renders correctly after q

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Press `q` |
| Expected | Same as TC-R-01: returns to dashboard cleanly. (`q` maps to `PluginAction::Back` per the spec.) |

#### TC-R-03: No ghost rendering from AboutPlugin frame

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin displayed version text centered in terminal |
| Action | Press `Esc`; observe carefully |
| Expected | ratatui performs a full buffer diff on the next frame. The terminal should show a clean dashboard with no leftover characters from the AboutPlugin frame. If any cell from the AboutPlugin frame is not covered by the dashboard render, it will be visible as an artifact. Verify no stray characters appear, especially at the center of the screen where AboutPlugin's text was. |

---

### 1.5 Key Hints Bar

The footer bar at the bottom of the screen must reflect the current context at all times.

#### TC-K-01: Dashboard hints while on dashboard

| Field | Value |
|-------|-------|
| Pre-condition | App on dashboard, focus Main, no modal |
| Action | Observe bottom bar |
| Expected | Shows the dashboard hints: `j/k:nav  Enter:open  w:work  s:switch  m:meeting  n:note  b:block  /:search  W:weekly  ?:help`. Keys rendered in `TEXT_ACCENT` (Cyan) + Bold. Descriptions in `TEXT_DIM` (DarkGray). |

#### TC-K-02: AboutPlugin hints while AboutPlugin is active

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open |
| Action | Observe bottom bar |
| Expected | Footer shows only the AboutPlugin's `key_hints()` result: `Esc:back`. Nothing from the dashboard hint set. No stale hints from the previous screen. |

#### TC-K-03: Dashboard hints restored after closing AboutPlugin

| Field | Value |
|-------|-------|
| Pre-condition | AboutPlugin is open; footer shows `Esc:back` |
| Action | Press `Esc` |
| Expected | Footer immediately returns to the full dashboard hint set (TC-K-01). No residual `Esc:back` from the plugin. |

#### TC-K-04: Sidebar hints during sidebar focus

| Field | Value |
|-------|-------|
| Pre-condition | Dashboard visible |
| Action | Press `Tab` |
| Expected | Footer shows sidebar-specific hints: `Tab:back  Space:start/pause  +/-:adjust  r:reset  R:reset all`. This is governed by the `Focus::Sidebar(_)` branch in `keyhints.rs`, which should be unaffected by Phase 0. |

#### TC-K-05: Modal hints during modal

| Field | Value |
|-------|-------|
| Pre-condition | Dashboard visible |
| Action | Press `n` (quick note modal) |
| Expected | Footer shows only `Enter:submit  Esc:cancel`. This is the `has_modal` branch in `keyhints.rs`, unchanged by Phase 0. |

---

## 2. Visual Consistency Checks

These checks compare Phase 0 output against the existing visual design vocabulary. The source of truth is `crates/jm-tui/src/theme.rs`.

### 2.1 Color Consistency

| Element | Expected color | theme.rs constant |
|---------|---------------|-------------------|
| Sidebar border (unfocused) | DarkGray | `BORDER_UNFOCUSED` |
| Sidebar border (focused) | Cyan | `BORDER_FOCUSED` |
| Focused plugin indicator | Cyan | `BORDER_FOCUSED` (used for top-row highlight) |
| Footer key text | Cyan + Bold | `TEXT_ACCENT` |
| Footer description text | DarkGray | `TEXT_DIM` |
| AboutPlugin primary text | Reset (terminal default) | `TEXT_PRIMARY` |
| AboutPlugin accent / version | Cyan | `TEXT_ACCENT` |
| Toast notifications | White on DarkGray | `TOAST_BG` |

All 16 colors used are ANSI base-16; verify no `Color::Rgb(...)` or hardcoded hex values are introduced by Phase 0.

### 2.2 Border and Box-Drawing Consistency

- The "Plugins" sidebar outer block uses `Borders::ALL` (all four sides). This must remain unchanged.
- Each individual plugin sub-block uses its own border (rendered by the plugin's own `render()` call). These are unchanged by the trait migration.
- The AboutPlugin screen: if it renders a border, it must use `Block::default().borders(Borders::ALL)` with a `theme::unfocused_border()` or `theme::focused_border()` style — consistent with `screens/issue_board.rs` and `screens/weekly.rs`.
- No mix of thin/thick Unicode box characters. ratatui uses a single consistent set by default; verify AboutPlugin does not call `.border_type()` with a value that differs from other screens.

### 2.3 Text Alignment and Padding

- Centered text in AboutPlugin: use `Paragraph::new(...).alignment(Alignment::Center)` inside a layout-calculated centered area, or use ratatui's centering helper (`centered_rect`). Do not use manual space-padding to fake centering; it will break on terminal resize.
- Sidebar plugin text: each plugin renders into its allocated `Rect`. Verify no plugin's text exceeds its `height()` declaration. Overflow would corrupt the plugin below it or bleed outside the sidebar block.
- Footer bar: single line, no wrapping. If hint text is too long for a narrow terminal, it will truncate naturally (Paragraph does not wrap by default). Verify no `Wrap { trim: true }` is added to the footer that would cause unexpected word-wrapping.

### 2.4 Screen Transition Cleanliness

ratatui performs differential rendering (buffer diff). Transitions are clean when the new frame fully covers all cells that the previous frame used. Failure modes:

| Failure | Symptom | Cause |
|---------|---------|-------|
| Ghost characters | Letters from AboutPlugin visible on dashboard after Esc | AboutPlugin wrote to cells that dashboard render doesn't cover |
| Sidebar remnant | DarkGray border columns visible during AboutPlugin | Layout split was not suppressed for `ScreenId::Plugin(_)` |
| Blank right panel | 22 blank columns on dashboard after returning | `sidebar_visible` was incorrectly cleared |
| Wrong footer text | Stale hints after screen change | `get_hints()` missing `ScreenId::Plugin(_)` arm or returning wrong data |

---

## 3. Edge Cases to Test

### 3.1 Small Terminal (80x24)

| Test | Steps | Expected |
|------|-------|----------|
| AboutPlugin at minimum size | Resize terminal to 80x24; press J | No panic. Text renders — may be truncated at edges but no out-of-bounds access. If centering calculation produces a zero-width or zero-height Rect, the plugin should no-op gracefully (guard with `if area.width == 0 || area.height == 0 { return; }`). |
| Sidebar at minimum size | Resize to 80x24 | Sidebar still renders with plugins; the 22-column allocation should leave ~58 columns for the main area. Content in both panels may be clipped but no panic. |
| Footer at 80 columns | Resize to 80x24 | Full dashboard hint set may not fit in 80 columns; trailing hints are truncated/omitted. No panic, no wrapping onto a second line. |

### 3.2 Wide Terminal (200x60)

| Test | Steps | Expected |
|------|-------|----------|
| AboutPlugin centering | Resize to 200x60; press J | Version string is visually centered — not left-justified. The centering calculation must use the dynamic `area.width` and `area.height`, not a hardcoded offset. |
| Sidebar width | Resize to 200x60 on dashboard | Sidebar remains 22 columns wide (fixed, not percentage-scaled). The extra width goes to the main area. Verify no layout constraint recalculates sidebar as a larger value. |

### 3.3 Rapid Key Presses

| Test | Steps | Expected |
|------|-------|----------|
| Rapid J/Esc | Hold J and Esc alternately for 1-2 seconds | No panic. No visual glitch. Each press of J calls `on_enter()`; each Esc calls `on_leave()`. Since AboutPlugin's lifecycle methods are no-ops, there is no state to corrupt. Screen transitions should be visually clean each time. |
| Rapid J/q | Press J then q several times quickly | Same as above. |
| Repeated Tab/J | Press Tab (sidebar focus), then J | After Tab, focus is `Focus::Sidebar(0)`. Pressing J should not open AboutPlugin while sidebar is focused — dashboard key handler is not active when `Focus::Sidebar` is set. Verify J is ignored (or consumed by sidebar) rather than opening AboutPlugin. This is a routing logic check with a visual confirmation. |

### 3.4 Sidebar Focus Before Opening About

| Test | Steps | Expected |
|------|-------|----------|
| Tab then J | Press Tab (sidebar focused), then J | If J is routed to the sidebar plugin key handler (e.g., Pomodoro), it is consumed there. AboutPlugin should NOT open. If J is not consumed by any sidebar plugin, it falls through — check whether the app's key dispatch prevents `OpenPlugin` from firing when sidebar is focused. Either behavior (J ignored, J consumed) is acceptable; what is NOT acceptable is the About screen opening unexpectedly while sidebar is focused. |
| Tab + Esc + J | Press Tab, Esc (return focus), J | AboutPlugin opens normally after focus returns to Main. |

### 3.5 Sidebar Toggled Off

| Test | Steps | Expected |
|------|-------|----------|
| AboutPlugin with sidebar hidden | Press P to hide sidebar; press J | AboutPlugin opens; full area used (same as when sidebar is visible, since sidebar is always hidden for screen plugins). No difference in rendering. |
| Return from About with sidebar hidden | Press J; press Esc | Return to dashboard; sidebar remains hidden (P toggled it off before; AboutPlugin does not restore it). |

### 3.6 No Projects / Empty State

| Test | Steps | Expected |
|------|-------|----------|
| AboutPlugin from empty dashboard | Start with no projects; press J | AboutPlugin opens correctly. The plugin does not depend on project data. Returning to dashboard shows empty state. |

---

## 4. Manual Test Script

This script can be followed top-to-bottom by a QA person or an agent with terminal access. Build first with `cargo build --release`, then run `./target/release/jm`.

**Setup requirements:**
- Terminal size: 120x36 or larger (adjust to 80x24 and 200x60 for edge cases)
- Config: `~/.jm/config.yaml` with `plugins.enabled: [pomodoro, notifications, clock]`
- At least 2-3 projects in `~/.jm/projects/` (for a non-empty dashboard)

---

### Step 1: Verify clean launch

**Press:** (none — just launch `jm`)

**Expect:**
- Dashboard visible with project list on the left (~40%) and preview on the right (~60%)
- Right sidebar panel labeled "Plugins" in a DarkGray border
- Three plugin panels stacked: Pomodoro, Notifications, Clock (in config order)
- Footer bar shows: `j/k:nav  Enter:open  w:work  s:switch  m:meeting  n:note  b:block  /:search  W:weekly  ?:help`
- Hint keys are in Cyan+Bold; descriptions are in DarkGray

**Failure indicators:** Missing sidebar, missing plugins, wrong footer text, any panic message

---

### Step 2: Verify sidebar focus (Tab)

**Press:** `Tab`

**Expect:**
- Sidebar outer "Plugins" border turns Cyan
- First plugin (Pomodoro) has its top-border row highlighted in Cyan
- Footer changes to: `Tab:back  Space:start/pause  +/-:adjust  r:reset  R:reset all`

**Press:** `Tab` again (or `Esc`)

**Expect:**
- Sidebar border returns to DarkGray
- Footer returns to dashboard hint set

---

### Step 3: Verify Clock updates

**Press:** (none — wait 3 seconds)

**Expect:**
- Clock plugin panel shows current time
- Time display updates each second (second hand or minute changes if near a minute boundary)
- No flicker in surrounding panels during clock update

---

### Step 4: Open AboutPlugin

**Press:** `J` (Shift+J)

**Expect:**
- Entire terminal area is now the AboutPlugin screen
- Sidebar ("Plugins" block) is completely gone — no border, no title, no blank gap on the right
- Dashboard project list is gone
- Version text (e.g., "jm v0.1.0") is centered horizontally and vertically
- Footer shows only: `Esc:back` (in Cyan+Bold / DarkGray style)
- No stale dashboard content visible anywhere

**Failure indicators:** Sidebar border remnant on right edge; 22-column blank area on right; version text left-aligned; footer still shows dashboard hints; any panic

---

### Step 5: AboutPlugin — verify full-width rendering

**While AboutPlugin is open:**

- Confirm the render uses the full terminal width (text/border reaches both left and right edges if a full-width border is drawn)
- Confirm no vertical split artifact (no thin line dividing the screen at 40%/60% or at the sidebar boundary)

---

### Step 6: Resize during AboutPlugin

**Action:** Resize terminal to approximately 80x24 while AboutPlugin is open

**Expect:**
- No crash
- Version text is still visible and roughly centered
- Footer still shows `Esc:back`

**Action:** Resize back to 120x36

**Expect:**
- Centering recalculates; text returns to center. No leftover characters from the 80-column render.

---

### Step 7: Return to dashboard with Esc

**Press:** `Esc`

**Expect:**
- Full dashboard immediately visible
- Project list on left, preview on right, sidebar on right edge
- Sidebar shows all three plugins (Pomodoro, Notifications, Clock) — same state as before opening AboutPlugin
- Footer shows full dashboard hint set
- No ghost characters at the center of the screen where AboutPlugin's version text was
- Selected project row unchanged from Step 1

**Failure indicators:** Sidebar missing; ghost centered text; wrong footer; project list empty; any panic

---

### Step 8: Open AboutPlugin again and close with q

**Press:** `J`, then `q`

**Expect:**
- Same result as Steps 4-7: AboutPlugin opens, then `q` returns to dashboard cleanly
- Dashboard state unchanged

---

### Step 9: Rapid open/close

**Press:** `J`, `Esc`, `J`, `Esc`, `J`, `Esc` (quickly, in under 3 seconds)

**Expect:**
- No panic at any point
- After the final `Esc`, dashboard renders correctly
- No visual corruption (stale sidebar absence, ghost text, wrong footer)

---

### Step 10: Sidebar focus then J (routing check)

**Press:** `Tab` (focus sidebar)

**Press:** `J`

**Expect:**
- AboutPlugin does NOT open
- J is handled by the sidebar (routed to Pomodoro or ignored if Pomodoro doesn't handle J)
- Sidebar remains focused; footer still shows sidebar hints

**Press:** `Esc` (return sidebar focus to Main)

**Press:** `J`

**Expect:**
- AboutPlugin opens (J now reaches the dashboard key handler)

**Press:** `Esc` to return

---

### Step 11: Hide sidebar then open AboutPlugin

**Press:** `P` (toggle sidebar off)

**Expect:** Sidebar hidden; main area expands to full width

**Press:** `J`

**Expect:**
- AboutPlugin opens; full-width render (same as when sidebar was visible — sidebar is hidden either way for screen plugins)
- No difference in rendering

**Press:** `Esc`

**Expect:**
- Return to dashboard with sidebar STILL hidden (P only toggles; returning from About does not restore it)

**Press:** `P` again to restore sidebar

---

### Step 12: Verify no regressions — existing screens

**Press:** `I` (Issue Board), then `Esc`

**Expect:** Issue Board opens and closes normally; no impact from Phase 0.

**Press:** `W` (Weekly Review), then `Esc`

**Expect:** Weekly Review opens and closes normally.

**Press:** Enter on a project, then `Esc`

**Expect:** Project View opens and closes normally; sidebar visible on return.

---

### Step 13: Verify Pomodoro still works

**Press:** `Tab` (focus sidebar to Pomodoro)

**Press:** `Space` (start timer)

**Expect:** Pomodoro begins counting down. State is maintained across the plugin trait migration.

**Press:** `Space` (pause)

**Expect:** Timer pauses.

**Press:** `Tab` or `Esc` (return focus to main)

---

### Checklist Summary

Copy this checklist to your test run notes and check off each item:

- [ ] TC-S-01: All three sidebar plugins visible on launch
- [ ] TC-S-02: Sidebar border DarkGray when unfocused
- [ ] TC-S-03: Sidebar border Cyan when Tab-focused
- [ ] TC-S-04: Focused plugin has Cyan top-border highlight
- [ ] TC-S-05: Focus returns to Main; border reverts to DarkGray
- [ ] TC-S-06: Clock updates each second
- [ ] TC-S-07: Notifications panel renders without panic
- [ ] TC-S-08: Pomodoro starts, pauses, stays within height bounds
- [ ] TC-A-01: J opens AboutPlugin full-screen
- [ ] TC-A-02: Version text is centered (horizontal + vertical)
- [ ] TC-A-03: Text readable; ANSI colors only; no invisible text
- [ ] TC-A-04: No layout split artifact; fills edge-to-edge
- [ ] TC-H-01: Sidebar completely absent during AboutPlugin
- [ ] TC-H-02: Sidebar absent on terminal resize during AboutPlugin
- [ ] TC-H-03: Sidebar visible again after Esc from AboutPlugin
- [ ] TC-R-01: Dashboard correct after Esc from AboutPlugin
- [ ] TC-R-02: Dashboard correct after q from AboutPlugin
- [ ] TC-R-03: No ghost characters from AboutPlugin frame
- [ ] TC-K-01: Dashboard footer hints correct
- [ ] TC-K-02: AboutPlugin footer shows only `Esc:back`
- [ ] TC-K-03: Dashboard hints restored after Esc from About
- [ ] TC-K-04: Sidebar hints during Tab focus
- [ ] TC-K-05: Modal hints during modal open
- [ ] Edge: 80x24 terminal — no panic, AboutPlugin renders
- [ ] Edge: 200x60 terminal — AboutPlugin text remains centered
- [ ] Edge: Rapid J/Esc — no panic, no visual glitch
- [ ] Edge: Tab+J does not open AboutPlugin while sidebar focused
- [ ] Edge: P (hide sidebar) + J — AboutPlugin still full-width
- [ ] Regression: Issue Board, Weekly Review, Project View unaffected
- [ ] Regression: Pomodoro timer still functional after trait migration

---

## Appendix: Key File Locations

| File | Relevance |
|------|-----------|
| `crates/jm-tui/src/plugins/mod.rs` | Trait definitions (`SidebarPlugin`, `ScreenPlugin`, `PluginAction`) |
| `crates/jm-tui/src/plugins/registry.rs` | `PluginRegistry` (new) |
| `crates/jm-tui/src/plugins/sidebar.rs` | `PluginSidebar` rendering and focus logic |
| `crates/jm-tui/src/plugins/about.rs` | `AboutPlugin` implementation (new) |
| `crates/jm-tui/src/app.rs` | `plugin_registry` field, `handle_back()`, render loop |
| `crates/jm-tui/src/events.rs` | `ScreenId::Plugin(String)` variant, `Action::OpenPlugin` |
| `crates/jm-tui/src/keyhints.rs` | `get_hints()` — must handle `ScreenId::Plugin(_)` |
| `crates/jm-tui/src/screens/dashboard.rs` | `J` keybinding → `Action::OpenPlugin("about")` |
| `crates/jm-tui/src/theme.rs` | All color constants (reference for visual consistency checks) |
