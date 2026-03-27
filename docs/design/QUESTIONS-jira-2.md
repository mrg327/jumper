# JIRA Plugin Design Review: TUI Feasibility Analysis

Review of `jira-plugin.md` and `plugin-architecture.md` from the perspective of ratatui rendering, modal interaction patterns, and the gap between the existing codebase and what the design requires.

---

## 1. Dynamic Kanban Column Rendering

### 1.1 Column Count vs. Terminal Width

**Issue**: The existing `issue_board.rs` uses a fixed set of 3-4 columns (`COLUMNS_DEFAULT` / `COLUMNS_WITH_DONE` at line 21-22) and divides space with `Constraint::Percentage(100 / num_cols)`. The JIRA plugin needs dynamic columns based on workflow statuses. Real JIRA workflows commonly have 5-8 statuses (To Do, In Progress, Code Review, QA, UAT, Staging, Done, etc.). Some teams have 10+.

**Constraint**: A standard 80-column terminal divided by 8 columns gives 10 characters per column. Subtracting 2 for borders leaves 8 usable characters -- not enough to display an issue key ("HMI-103" = 7 chars) plus any summary text. Even at 120 columns (common for dev terminals), 8 columns yields 15 chars each, which is tight for "HMI-103 Fix nav..." format shown in the design mockup.

**What the codebase does today**: `issue_board.rs` lines 331-334 build constraints as `Constraint::Percentage(100 / num_cols)` and calls `Layout::horizontal(constraints).split(board_area)`. This works for 3-4 columns because each gets 25-33% of the width. At 7+ columns, ratatui's percentage-based layout starts rounding down in ways that lose pixels or produce uneven columns.

**Questions**:
- What is the maximum number of columns the kanban board should support before introducing horizontal scrolling? The design doc shows 5 columns in the mockup but does not discuss the upper bound.
- Should columns with zero issues collapse to a narrow "header-only" width (e.g., 3 chars) to free space for populated columns? The existing board gives equal width to empty and full columns.
- Horizontal scrolling in a kanban board is unusual in TUIs. If the board scrolls horizontally, `h/l` navigation becomes overloaded (column nav vs. scroll). Is there a plan?

**Suggestion**: Group statuses by `StatusCategory` (ToDo, InProgress, Done) as the design already mentions, but use this as the default column layout (3 columns, matching the existing board). Add a "detailed view" toggle that expands to per-status columns. This limits the common case to 3 columns while still exposing workflow detail when needed.

**Severity**: :yellow_circle: Needs Resolution -- the mockup shows 5 columns which is fine, but the "dynamic" promise means the code must handle adversarial workflows.

### 1.2 Column Width Allocation Algorithm

**Issue**: Equal-width columns waste space when issue distribution is uneven (e.g., 15 issues in "In Progress" but 0 in "QA"). The existing board does not solve this -- it uses equal percentages.

**Questions**:
- Should column width be proportional to issue count? Min-width-constrained proportional? Fixed minimum + flexible remainder?
- The design mockup shows a "Done" column that is narrower than others. Is this intentional or an artifact of the ASCII art?

**Constraint**: ratatui's `Layout` computes constraints once per `split()` call. Using `Constraint::Min(min_col_width)` combined with `Constraint::Fill(1)` for flexible columns would work, but you cannot mix `Percentage` and `Min` in the same direction without `Constraint::Ratio` math getting complex. The cleanest approach is `Constraint::Min(min)` for all columns plus one `Constraint::Fill(1)` for the focused column, but ratatui does not support "give extra space to one specific column" out of the box.

**Severity**: :yellow_circle: Needs Resolution -- affects readability for any workflow with more than 4 statuses.

### 1.3 Render Performance for Dynamic Layouts

**Issue**: `render()` is called every frame (~100ms based on the `event::poll(Duration::from_millis(100))` in `app.rs` line 200). Computing dynamic column widths, filtering issues into columns, and building `ListItem` vectors every frame is wasteful if the data has not changed.

**What the codebase does today**: `issue_board::render()` recomputes `items_in_column()` on every frame (line 338). This is fine for small local issue sets but could be noticeable with 30+ JIRA issues across 8 dynamic columns, each requiring key+summary truncation calculations.

**Suggestion**: Cache the column layout (which issues belong to which column, truncated display strings) and only recompute when the underlying `Vec<JiraIssue>` changes (after a refresh). A simple generation counter or `data_version: u64` flag would suffice.

**Severity**: :green_circle: Minor -- unlikely to cause visible lag at realistic issue counts (30-50), but worth noting for the per-frame cost.

---

## 2. Modal-from-Plugin Problem

### 2.1 Modal Result Routing Back to Plugin

**Issue**: This is the deepest architectural gap in the design. Today, modals are owned by `App` (the `modal_stack: Vec<Modal>` in `app.rs` line 40). When a modal submits, it returns `Action::SubmitInput(String)` which flows to `App::handle_submit_input()` (line 1252). That method peeks at the top modal's `InputAction` or `SelectAction` enum to decide what to do with the result. All routing is hardcoded in `App`.

A `ScreenPlugin` returns `Action` from `handle_key()`. If it returns `Action::ShowModal(...)`, the App can push the modal. But when the modal submits, `App::handle_submit_input()` runs -- and it has no way to forward the result back to the plugin. The plugin's `handle_key()` is not called during modal interaction (line 434: modal takes priority over screen).

**Concrete example**: User presses `s` on the JIRA board to transition an issue. The plugin wants to show a select modal with available transitions. It returns some action to trigger the modal. The App shows it. User picks "Start Review". The modal returns `Action::SubmitInput("0")`. `App::handle_submit_input()` runs. Now what? The App does not know this was a JIRA transition picker. It does not have the transition list. It cannot call the plugin's API.

**How the codebase solves this today**: It does not -- every modal result handler is a method on `App` that directly accesses stores. See how `SelectAction::PickParentIssue` in `handle_submit_select()` (line 1661) immediately calls `self.issue_store.load()` and pushes another modal. This pattern requires the state machine to live in `App`, not in the plugin.

**Possible approaches**:
1. **Plugin-owned modals**: The plugin renders its own modals within its `render()` call, using `Clear` + centered rect (same technique as the existing modals). The plugin handles all key events during its modal state internally, never involving the App's modal stack. This is the cleanest separation but means the plugin reimplements modal rendering.
2. **Action::PluginModalResult**: Add a new action variant that carries the plugin name and the result string back. The App routes it to the plugin via a new method like `on_modal_result(&mut self, result: String)` on the `ScreenPlugin` trait. This adds a trait method but keeps the App's modal infrastructure reusable.
3. **Callback closure**: Store a `Box<dyn FnOnce(String)>` in the modal that the App invokes on submit. This is un-Rustic given the borrow checker implications with mutable plugin references.

**Recommendation**: Approach 1 (plugin-owned modals) is the most pragmatic. The JIRA plugin already needs custom modals (transition picker with "current status" display, detail modal with scrollable fields, creation wizard). These are too specialized for the generic `SelectModal`. Rendering them within the plugin's `render()` avoids the routing problem entirely. The plugin tracks its own `modal_state: Option<JiraModal>` enum and handles keys accordingly before delegating to the board.

**Severity**: :red_circle: Blocker -- without resolving this, no modal-based interaction works for the plugin.

### 2.2 Multi-Step Modal Chains (Creation Wizard)

**Issue**: The creation flow (Project -> Issue Type -> Summary -> Required Fields) is a sequential state machine where each step depends on the previous step's result. Between steps, there may be an async API call (fetching createmeta after project+issue type selection). The design says "one modal per field" with "dynamically discovered" required fields.

**State machine complexity**: The plugin needs to maintain:
```
CreateState {
    project_key: Option<String>,
    issue_type_id: Option<String>,
    summary: Option<String>,
    fields_to_fill: Vec<EditableField>,    // from API
    fields_filled: Vec<(String, Value)>,    // accumulated
    current_field_index: usize,
}
```

This state must survive across modal dismissals. If modals are plugin-owned (approach 1 above), this is straightforward -- the state lives in the plugin struct. If modals are App-owned, the state must be stashed somewhere the routing code can find it, similar to `unblock_open_indices` / `unblock_slug` / `move_blocker_source_idx` fields in `App` (lines 67-74). Those ad-hoc fields are already messy for 2-step flows; a 5-8 step creation wizard would be worse.

**The async gap**: Between "select issue type" and "show required fields," the plugin must call `FetchCreateMeta`. This is an API call that runs on the background thread. The modal sequence is: show select -> user picks -> dismiss modal -> send API command -> wait for result (show loading) -> show next modal. The current modal system has no concept of "waiting" between modals. The plugin would need a `CreatingState::WaitingForMeta` phase where it renders a loading indicator instead of a modal.

**Questions**:
- If the JIRA API returns a validation error after all fields are filled and the create call is made, can the user go back and fix one field? The design does not mention a "back" capability in the wizard. Forcing the user to restart from step 1 after filling 5 fields would be frustrating.
- What if a required field's `allowed_values` is a list of 200 items (e.g., a "Component" field with many options)? The `SelectModal` has no search/filter capability. The existing `switch.rs` screen has fuzzy filtering for project selection -- should this be reusable?

**Severity**: :red_circle: Blocker -- the multi-step state machine with interspersed async calls is the hardest part of the plugin to get right. Needs explicit design for the state transitions.

---

## 3. Loading and Refresh UX

### 3.1 Column Layout Shift on Refresh

**Issue**: If a refresh returns data with a different set of workflow statuses (e.g., an admin added a new status column while you were looking at the board), the entire kanban layout changes. The number of columns changes, column widths redistribute, and the user's cursor position (`column: usize, row: usize`) may now point to a different column or be out of bounds.

**What the codebase does today**: `issue_board::refresh()` (line 63) saves the currently selected item's `(slug, id)` before reloading, then tries to find it again after reload. If found, the cursor position is restored. If not, `clamp_row()` ensures indices are in bounds. This works because the columns are fixed -- `column: usize` always maps to the same status.

**The JIRA problem**: If columns are dynamic and the column set changes, `column: usize = 2` might have been "Code Review" before refresh and "QA" after. The cursor should follow the status, not the index. The plugin needs to save `(selected_issue_key, selected_column_status_name)` and restore by searching for the status name in the new column list.

**Severity**: :yellow_circle: Needs Resolution -- cursor jump after refresh would be disorienting.

### 3.2 Concurrent Operation State

**Issue**: The plugin uses `mpsc::channel` for API communication. The design lists `is_loading` conceptually but does not define how to track multiple in-flight operations. Consider: user presses `R` (manual refresh) and while that's pending, presses `s` to transition an issue. Now there are two pending operations: `FetchMyIssues` and `TransitionIssue`. When the transition completes, it triggers a post-write refresh (`FetchMyIssues` again). Now there are potentially two `FetchMyIssues` in flight.

**Questions**:
- Should transitions be disabled while a refresh is in flight? That would hurt responsiveness.
- Should the plugin track each pending operation separately (e.g., `pending_ops: HashSet<OperationKind>`)? Or just a simple `loading: bool`?
- What happens if a refresh result arrives that is staler than the last one applied? (e.g., slow refresh #1 completes after fast refresh #2). Need generation counters or request IDs.

**Suggestion**: Use a monotonic request ID. Each command sent to the API thread carries an ID. Results carry the same ID. The plugin ignores results with an ID older than the last applied result. This prevents stale data from overwriting fresh data.

**Severity**: :yellow_circle: Needs Resolution -- race conditions between concurrent operations will cause subtle bugs.

### 3.3 Optimistic UI Updates

**Issue**: After transitioning an issue, the design says "post-write refresh" fetches fresh data. But the API round-trip (transition POST + refresh GET) takes 500ms-2s depending on network. During that time, the user sees the issue in its old column. This is jarring -- you pressed "Start Review" but the issue still sits in "In Progress" for 1-2 seconds.

**What the codebase does today**: In `issue_board.rs`, status changes are synchronous file operations (`self.issue_store.set_status()` at app.rs line 754) that complete instantly. The board refresh happens in the same frame. There is no latency gap.

**Suggestion**: After a successful transition command is sent (not completed), optimistically move the issue to the target column in the local state. If the API returns an error, revert. This requires the plugin to know the target status of each transition (which is available from `JiraTransition::to_status`).

**Severity**: :yellow_circle: Needs Resolution -- without optimistic updates, every write operation will feel sluggish compared to the local issue board.

---

## 4. Scroll State and Focus Management

### 4.1 Detail Modal Over Changing Board Data

**Issue**: The issue detail modal is described as an overlay. If the user opens the detail modal for HMI-103 and an auto-refresh fires (every 60s), the board data underneath changes but the modal still shows the old data. Worse, if HMI-103's status changed on the server (a colleague transitioned it), the modal shows stale field values.

**Questions**:
- Should the detail modal subscribe to refresh events and update its displayed fields?
- If the detail modal is plugin-owned (per recommendation in section 2.1), the plugin controls when to re-render it. But the plugin would need to detect "the issue I'm showing has changed" after each refresh and update the modal content.
- If the user is in the middle of editing a field (text input open) and a refresh changes that field's value on the server, what happens? The user's input should not be discarded. But submitting the edit would overwrite a newer server value.

**Severity**: :yellow_circle: Needs Resolution -- stale data in modals is confusing but probably acceptable for the first version. However, the "edit overwrites newer server value" case needs at least a documented decision.

### 4.2 Scroll Within Detail Modal

**Issue**: The detail modal (design mockup lines 303-340) has two scrollable regions: the fields section (top) and the comments section (bottom). The user navigates fields with `j/k` and scrolls comments within the comments section.

**Constraint**: ratatui does not have a built-in "scrollable region" widget. Scroll is implemented by slicing the content and rendering a window. The existing `project_view.rs` uses `scroll_offset: usize` to offset the rendered paragraph. But the detail modal needs two independent scroll states.

**Questions**:
- When `j/k` is on the fields section, pressing `j` past the last field should... move focus to the comments section? Or stop?
- Once in the comments section, does `j/k` scroll individual comments or navigate between comments?
- How does the user get back to the fields section from comments? `k` at the top of comments jumps to the last field?

**Implementation approach**: Track `detail_focus: enum { Fields(usize), Comments(usize) }`. Fields section: `j/k` moves the field cursor. Past the last field, focus moves to `Comments(0)`. Comments section: `j/k` scrolls the comment viewport. At the top of comments, `k` returns to `Fields(last)`.

**Severity**: :green_circle: Minor -- standard TUI pattern, but worth specifying the exact navigation behavior before implementation.

---

## 5. Text Input for Descriptions and Comments

### 5.1 Multi-Line Text Input

**Issue**: The existing `InputModal` (`modals/input.rs`) is a single-line text input. It has word wrapping for display (line 203: `Paragraph::new(input_line).wrap(...)`) but the actual input model is a single string without newline handling. There is no Enter-to-newline; Enter submits.

The JIRA plugin needs multi-line input for:
- Comments (design mockup shows multi-line with Ctrl+Enter to submit)
- Description editing (TextArea field type)

**What needs to be built**: A multi-line text editor in a TUI. This requires:
- Newline insertion on Enter (Ctrl+Enter or a different key to submit)
- Cursor tracking as `(row, col)` instead of a flat `cursor_pos: usize`
- Vertical scrolling when content exceeds the input area height
- Line-aware cursor movement (up/down move between lines, Home/End go to line start/end)
- Word wrap display with correct cursor positioning (the existing `wrapped_cursor_position` in `text_utils.rs` handles this for single-line display wrapping but not for multi-line editing with real newlines)

**Complexity**: Multi-line text editing in a TUI is a significant feature. It is essentially a mini text editor. Libraries like `tui-textarea` exist but add a dependency. Rolling a custom one is 200-400 lines of code with edge cases around UTF-8 handling, wrapping, and scroll.

**Suggestion**: For v1, use the existing single-line `InputModal` for comments (comments are often short). For description editing, open `$EDITOR` (the app already has this pattern -- see `pending_editor_slug` in `app.rs` line 78 and the editor launch code at lines 167-196). Write the ADF-converted plain text to a temp file, open $EDITOR, read back, convert to ADF, and submit. This sidesteps the multi-line TUI input problem entirely while giving the user a proper editor for long text.

**Severity**: :red_circle: Blocker -- the design specifies multi-line comment input with Ctrl+Enter. Either build it, use a library, or change the design to use $EDITOR for long text.

### 5.2 ADF Description Length in Detail Modal

**Issue**: JIRA descriptions can be very long (thousands of characters). The detail modal has limited vertical space. The design mockup shows 3 lines of description text.

**Questions**:
- Is the description truncated to N lines with a "... (more)" indicator?
- Or is the entire fields section scrollable (as discussed in 4.2)?
- The `Paragraph` widget with `Wrap { trim: false }` will render all the text but it will overflow the allocated area. ratatui clips at the area boundary, so excess text is simply invisible. This is fine if the user can scroll. But the scroll mechanism needs to know the total content height, which requires computing the wrapped line count.

**Severity**: :green_circle: Minor -- scrollable field section handles this. Just needs implementation.

---

## 6. The Sequential Creation Wizard

### 6.1 Many Required Fields

**Issue**: The design says "one modal per field" for required fields. Some JIRA projects have many required fields. I have seen JIRA project configurations with 8-12 required fields on issue creation (Summary, Description, Priority, Component, Fix Version, Sprint, Story Points, Acceptance Criteria, Team, etc.).

Going through 8+ sequential single-field modals is tedious. Each one requires: read prompt, type/select, press Enter. At 3-5 seconds per field, that is 24-40 seconds of modal cycling. Compare to JIRA's web UI which shows all fields on one page.

**Alternative approaches**:
- **Form modal**: A single modal with all required fields rendered as a vertical form. `j/k` navigates between fields, `Enter` edits the focused field. This is more complex to build but much faster to use.
- **Minimal wizard**: Only prompt for Summary (and issue type if needed). Create the issue with only required fields filled, then let the user edit other fields via the detail modal. JIRA's API will reject the create if truly required fields are missing, but many "required" fields have defaults.
- **Two-phase**: Prompt for Summary + Description (the two most common fields), create with those, then toast "Created HMI-116 -- edit to add more fields."

**Questions**:
- Has the user tested how many required fields typical projects in their JIRA instance have? If it is consistently just Summary + Issue Type, the sequential approach is fine.
- Is there a way to query which fields have defaults (and thus can be skipped even if "required")?

**Severity**: :yellow_circle: Needs Resolution -- risk of UX tedium for heavily-configured JIRA projects.

### 6.2 No Back Navigation in Wizard

**Issue**: The creation wizard is forward-only. If the user fills 5 fields and then realizes they selected the wrong issue type in step 2, they must Esc (cancel everything) and restart.

**Additional concern**: If the JIRA API returns a 400 validation error on the create call (after all fields are filled), the accumulated field values are lost. The user must re-enter everything.

**Suggestion**: At minimum, persist the `CreateState` across a failed create attempt so the user can retry without re-entering. Better: allow Esc on any step to go back one step instead of cancelling the entire wizard. This requires the state machine to support backward transitions, which complicates it but improves usability significantly.

**Severity**: :yellow_circle: Needs Resolution -- losing 5+ fields of input on error is a UX failure that contradicts the "zero-friction" design principle.

### 6.3 Wizard State Ownership

**Issue**: Where does the creation wizard state live? If plugin-owned modals (section 2.1 recommendation), the plugin struct holds `create_state: Option<CreateWizardState>`. This is clean. But if using the App's modal stack, the accumulated state needs to be threaded through `InputAction` / `SelectAction` variants, which means adding variants like `InputAction::JiraCreateField { accumulated: Vec<(String, Value)>, remaining: Vec<EditableField> }`. This is ugly and couples `InputAction` to JIRA-specific types.

**This reinforces the recommendation for plugin-owned modals.**

**Severity**: :red_circle: Blocker (if not using plugin-owned modals) / :green_circle: Minor (if using plugin-owned modals).

---

## 7. Performance and API Call Budget

### 7.1 Initial Load API Calls

**Issue**: On entering the JIRA screen (`on_enter`), the plugin needs:
1. `GET /rest/api/3/search` -- fetch assigned issues (1 call, returns up to 50 issues by default)
2. `GET /rest/api/3/status` -- get all statuses for workflow column discovery (1 call)

That is 2 calls for initial load, which is fine.

However, the design also mentions:
- `editmeta` (per-issue) -- needed to know which fields are editable in the detail modal
- `transitions` (per-issue) -- needed for the transition picker

**Questions**:
- Are editmeta and transitions fetched lazily (when the user opens a detail modal / presses `s`)? The design says "Fetch available transitions for an issue" as a command type but does not specify when it fires.
- If they are fetched eagerly for all issues on initial load, that is 30+ API calls (1 per issue for transitions, 1 per issue for editmeta). At ~200ms per call sequentially, that is 12+ seconds. Even parallelized, JIRA rate limits will throttle this.
- The `/search` endpoint returns most fields inline. Does it return the status category? If so, the `/status` call may be redundant for column grouping.

**Recommendation**: Fetch transitions and editmeta lazily -- only when the user opens a detail modal or presses `s`. Cache per-issue so subsequent accesses are instant. Invalidate cache on refresh.

**Severity**: :yellow_circle: Needs Resolution -- eager fetching of per-issue metadata would make initial load unacceptably slow.

### 7.2 Pagination

**Issue**: The `/search` endpoint returns paginated results (default 50, max 100 per page). If the user has more than 50 assigned issues, the initial fetch only gets the first page.

**Questions**:
- Does the plugin need to handle pagination (fetch all pages)?
- Or is 50 issues sufficient for the "my assigned issues" use case?
- The design does not mention pagination at all.

**Severity**: :green_circle: Minor -- most users have fewer than 50 assigned issues, but worth a note for heavy-load users.

---

## 8. ScreenPlugin Trait Gaps

### 8.1 No `on_tick` Result Consumption

**Issue**: The `ScreenPlugin` trait has `on_tick()` inherited from the base `Plugin` trait, which returns `Vec<String>` (notification messages). But for the JIRA plugin, `on_tick()` is where it should check the `mpsc::Receiver` for API results. The return type `Vec<String>` is for notification messages to other plugins, not for "I got new data, please re-render."

**Current pattern**: In `app.rs` lines 207-211, the tick fires every second and calls `self.plugins.on_tick()`. The sidebar plugins return notification strings. But a screen plugin needs to mutate its own state (update `issues`, clear `loading` flag) during `on_tick()`. Since `on_tick()` takes `&mut self`, the plugin can mutate. The re-render happens automatically because `render()` is called every frame. So the return value is irrelevant for data updates -- the plugin just mutates internally.

But: the current `App::run()` loop only ticks sidebar plugins via `self.plugins.on_tick()` (line 765). There is no call to tick screen plugins. The `PluginRegistry` proposed in the architecture doc would need to tick the active screen plugin separately.

**Questions**:
- Is the 1-second tick granularity sufficient for checking API results? If an API call completes in 200ms, the user waits up to 1 second to see the result. The poll interval in the event loop is 100ms (line 200) -- should screen plugins be polled on every iteration (every 100ms) instead of every 1s?

**Severity**: :yellow_circle: Needs Resolution -- the tick integration for screen plugins is not wired up and the 1s granularity is too slow for responsive API result handling.

### 8.2 No `ShowModal` Action Defined

**Issue**: The architecture doc mentions `Action::ShowModal(ModalKind)` but the actual `Action` enum in `events.rs` has no such variant. There is `PushModal(ModalId)` but `ModalId` is a closed enum of known modal types (Help, AddProject, QuickNote, etc.) with no plugin-extensible variant.

If using plugin-owned modals (recommendation from section 2.1), this is a non-issue -- the plugin never asks the App to show a modal. But if the design changes to use App-owned modals, `ModalId` and `Modal` need plugin-extensible variants.

**Severity**: :green_circle: Minor (if using plugin-owned modals) / :red_circle: Blocker (if using App-owned modals).

---

## 9. Keybinding Conflicts and Footer

### 9.1 Dynamic Key Hints from Plugin

**Issue**: The `keyhints.rs` module (line 65, `get_hints()`) has a hardcoded `match screen` with static hint vectors for each `ScreenId`. There is no `ScreenId::Plugin(_)` case. The `ScreenPlugin` trait has `key_hints(&self) -> Vec<(&str, &str)>` which is the right approach, but `keyhints::render()` does not call it. The App's render method calls `keyhints::render()` directly (line 419).

**Integration needed**: When the active screen is a plugin, the keyhints system must delegate to `plugin.key_hints()` instead of the hardcoded match. This requires either:
- Adding a `ScreenId::Plugin(_)` match arm that looks up the plugin and calls `key_hints()`
- Or bypassing `keyhints::render()` entirely for plugin screens and letting the plugin render its own footer

**Additional complication**: The JIRA plugin's key hints change based on internal state (board vs. detail modal vs. creation wizard vs. transition picker). A static `key_hints()` return is insufficient. The trait should probably return hints based on the plugin's current state, which it can since `&self` is available.

**Severity**: :yellow_circle: Needs Resolution -- but straightforward to implement.

### 9.2 Ctrl+J Entry Keybinding

**Issue**: The design specifies `Ctrl+J` as the entry keybinding from the dashboard. The current dashboard's `handle_key` in `dashboard.rs` does not handle `Ctrl+J`. The architecture doc proposes reading the keybinding from config.

**Questions**:
- Is `Ctrl+J` safe from conflicts? `Ctrl+J` is equivalent to pressing Enter in many terminals (it sends the same byte as newline/CR). This could cause issues in terminal emulators that map Ctrl+J to Enter.
- Should the keybinding be a single-key (like `J` uppercase) instead? The existing app uses single-letter keys for screen navigation (`I` for issue board, `W` for weekly, `r` for review).

**Severity**: :yellow_circle: Needs Resolution -- `Ctrl+J` may not be reliably distinguishable from Enter in all terminal emulators.

---

## 10. Error Modal as Blocking

**Issue**: The design says "All errors show a blocking error modal that must be dismissed before continuing." If the JIRA API is flaky and returns errors frequently, the user is stuck dismissing error modals. This is especially problematic during auto-refresh: if the network drops, every 60-second refresh fires an error modal that blocks the UI.

**Suggestion**: Use blocking error modals only for user-initiated actions (transition, edit, create, comment). For auto-refresh failures, show a non-blocking toast ("JIRA refresh failed -- check network") and keep the stale data visible. The user can press `R` to retry manually.

**Severity**: :yellow_circle: Needs Resolution -- blocking modals on auto-refresh failure would make the tool unusable on unreliable networks.

---

## Summary by Severity

### :red_circle: Blockers (must resolve before implementation)

| # | Issue | Section |
|---|-------|---------|
| 1 | Modal result routing back to plugin | 2.1 |
| 2 | Multi-step creation wizard state machine with async gaps | 2.2 |
| 3 | Multi-line text input for comments/descriptions | 5.1 |

### :yellow_circle: Needs Resolution (must resolve before shipping)

| # | Issue | Section |
|---|-------|---------|
| 4 | Dynamic column count upper bound and overflow strategy | 1.1 |
| 5 | Column width allocation algorithm | 1.2 |
| 6 | Column layout shift on refresh (cursor follows status, not index) | 3.1 |
| 7 | Concurrent operation state and stale result handling | 3.2 |
| 8 | Optimistic UI updates for transitions | 3.3 |
| 9 | Stale data in detail modal during refresh | 4.1 |
| 10 | Many required fields creating tedious wizard UX | 6.1 |
| 11 | No back navigation in creation wizard | 6.2 |
| 12 | Lazy vs. eager fetching of per-issue metadata | 7.1 |
| 13 | Screen plugin tick integration (wiring + granularity) | 8.1 |
| 14 | Dynamic key hints integration for plugin screens | 9.1 |
| 15 | Ctrl+J keybinding reliability across terminals | 9.2 |
| 16 | Blocking error modals on auto-refresh failure | 10 |

### :green_circle: Minor / Nice-to-have

| # | Issue | Section |
|---|-------|---------|
| 17 | Per-frame render caching for dynamic layouts | 1.3 |
| 18 | Detail modal scroll navigation specification | 4.2 |
| 19 | ADF description length handling | 5.2 |
| 20 | Pagination for >50 assigned issues | 7.2 |
| 21 | ShowModal action variant (moot if plugin-owned modals) | 8.2 |

---

## Key Architectural Recommendation

**Use plugin-owned modals.** The JIRA plugin should manage its own modal state internally via a `JiraFocus` or `JiraModal` enum within the plugin struct. The plugin's `render()` method renders the board, detail modal, transition picker, creation wizard, and error dialogs based on this internal state. The plugin's `handle_key()` routes keys to the appropriate handler based on the current modal state.

This solves issues #1, #2, #3 (partially), and #18 simultaneously. It avoids polluting the core `Modal`/`InputAction`/`SelectAction` enums with JIRA-specific variants. It keeps the plugin truly self-contained, which aligns with the architecture doc's "self-contained" principle.

The cost is that the plugin must reimplement modal rendering (centered rect, clear background, border, key hints). But the existing `modals::centered_rect()` helper and rendering patterns are simple enough to copy -- each modal is ~30 lines of rendering code.
