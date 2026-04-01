# Flow Trace Review — JIRA Plugin Spec (r4)

Reviewer: Claude Sonnet 4.6
Date: 2026-03-27
Documents reviewed:
- `jira-plugin.md` (primary)
- `jira-api-reference.md`
- `form-modal-spec.md`
- `horizontal-scroll-spec.md`
- `plugin-architecture.md`

Each flow is traced step by step. For every step the review asks: Is the state change defined? Is the command defined? Is the result handler defined? Is the error path defined?

Verdict codes:
- **COMPLETE** — all four questions answered
- **GAP** — one or more questions unanswered; severity and classification follow

---

## Flow 1: Open JIRA Screen

**Trigger**: `J` keypress from dashboard

### Step 1.1 — `J` → `Action::OpenPlugin`

- State change: `plugin-architecture.md` §Focus & Navigation defines `ScreenId::Plugin("jira".into())` assignment and calls `plugin.on_enter()`. Dashboard key handler must emit an action that the app converts to `self.screen = ScreenId::Plugin("jira")`.
- Command: `plugin-architecture.md` shows the App-level dispatch, but the exact `Action` variant emitted from the dashboard on `J` is not named. The doc says keybindings are "hardcoded" in `App::handle_key()` after the current screen's handler returns `Action::None`, calling `self.open_plugin_screen("jira")`. The `open_plugin_screen` helper is named but its body is not defined anywhere in the docs.
- Result handler: implicit — sets `self.screen`.
- Error path: not defined for the case where `jira` is not in the `enabled` list or `JiraPlugin::new()` fails at init time.

**GAP (AUTO-FIX)**: `open_plugin_screen("jira")` is named but its body is never defined. Spec should state it sets `self.screen = ScreenId::Plugin(name)` and calls `plugin.on_enter()`. One sentence needed.

### Step 1.2 — `on_enter()`: validate config

- State change: `jira-plugin.md` §Startup Validation lists steps 1–4 (check env var, check config keys, call `/myself`, show error modal if any check fails).
- Command: all three checks happen synchronously in `on_enter()` before the thread is spawned — `jira-plugin.md` §Background Thread Specifics confirms this.
- Result handler: on success, `account_id` is stored.
- Error path: blocking `ErrorModal` shown. However, after showing the error modal, the spec does not state whether `on_enter()` should return early and skip thread spawn. Implied but not explicit.

**GAP (AUTO-FIX)**: Spec should explicitly state that if any validation step fails, `on_enter()` returns early — no thread is spawned and no `FetchFields`/`FetchMyIssues` commands are sent.

### Step 1.3 — `on_enter()`: `GET /rest/api/3/myself`

- State change: stores `accountId` in `self.account_id`.
- Command: synchronous `ureq` call; `jira-api-reference.md` §1 gives exact request/response shape.
- Result handler: `MyselfResponse.account_id` extracted and stored.
- Error path: `jira-plugin.md` says show "blocking error modal with actionable message". `jira-api-reference.md` §Auth shows the `JiraError::Api` variant. The bridge from an API error returned by the synchronous call into `JiraModal::ErrorModal` in the plugin state is not described.

**GAP (AUTO-FIX)**: Spec should describe how a synchronous `/myself` error (before the background thread exists) creates a `JiraModal::ErrorModal`. The current text says "show a blocking error modal" but the mechanism (set `self.modal = Some(JiraModal::ErrorModal {...})` before returning from `on_enter()`) is left implicit.

### Step 1.4 — `FetchFields` (custom field discovery)

- State change: `jira-plugin.md` §Custom Field Discovery: on success sets `self.story_points_field` and `self.sprint_field`. If ambiguous, omit.
- Command: `JiraCommand::FetchFields` sent to background thread immediately after spawn (implied by §Phase 1a and §Background Thread Specifics, but ordering is implicit).
- Result handler: `JiraResult::Fields(Vec<JiraFieldDef>)` — `jira-plugin.md` §Result Types names the variant. The handler logic (name-matching logic) is described in §Custom Field Discovery.
- Error path: not defined. If `FetchFields` returns `JiraResult::Error`, the spec says auto-refresh failures become toasts but `FetchFields` is a startup command, not a refresh. The handling for a failed fields fetch (fields remain unset; story points and sprint columns never show) is described implicitly by "if ambiguous, omit" but a network error is not the same as ambiguity.

**GAP (NEEDS-INPUT)**: What happens if `FetchFields` returns an error? Do we proceed without custom field display (silently degrade) or show a blocking error? The spec has no coverage for this path.

### Step 1.5 — `FetchMyIssues` (initial load)

- State change: `loading = true` set when? Not specified. Implied to be set in `on_enter()` before commanding the thread, but not stated.
- Command: `JiraCommand::FetchMyIssues { generation }`. `jira-plugin.md` §Command Types has the definition.
- Result handler: `JiraResult::Issues { generation, issues }` — drain loop in `on_tick()`. `jira-plugin.md` §Refresh Behavior describes generation-counter logic.
- Error path: `JiraResult::Error` → toast for auto-refresh, blocking modal for user-initiated. The initial load is the first fetch — which category does it fall into? If a blocking modal is shown on first-load failure, the user can only dismiss; they cannot retry without leaving and re-entering. Retry path is not specified.

**GAP (NEEDS-INPUT)**: The initial `FetchMyIssues` (first ever load, `loading = true`) is neither a "user-initiated write" nor a pure "auto-refresh". The error handling category (blocking modal vs. toast + stale indicator) is not specified for the first load.

### Step 1.6 — Render board

- State change: `loading = false` on `JiraResult::Issues` receipt. `jira-plugin.md` §Loading State shows the loading UI.
- Command: none (render only).
- Result handler: n/a.
- Error path: n/a.

**COMPLETE** for render step.

---

## Flow 2: Navigate Board

**Trigger**: `h`, `l`, `j`, `k` keys while on kanban board (no modal open)

### Step 2.1 — `l` (move right to next column)

- State change: `horizontal-scroll-spec.md` §`l` gives exact pseudocode: `selected_col += 1`; if `selected_col >= scroll_offset + visible_count` then `scroll_offset = selected_col - visible_count + 1`.
- Command: none (local state only).
- Result handler: re-render.
- Error path: boundary — spec says `if self.selected_col < self.columns.len() - 1` guards the increment. No error.

**COMPLETE**

### Step 2.2 — `h` (move left)

- State change: `horizontal-scroll-spec.md` §`h` gives exact pseudocode.
- Command: none.
- Result handler: re-render.
- Error path: boundary guard present.

**COMPLETE**

### Step 2.3 — `j` (move down in column)

- State change: `horizontal-scroll-spec.md` §Column Vertical Scroll defines `selected_row` increment and `col_scroll_offsets` adjustment. `g`/`G` jump-to-top/bottom are also specified.
- Command: none.
- Result handler: re-render.
- Error path: boundary — `selected_row` clamped implicitly when `j` is at the last item. The spec describes the cursor-follows algorithm but not the guard for `selected_row` going past the last issue. It says "moves selected_row within the selected column as normal" but no explicit boundary check is written.

**GAP (AUTO-FIX)**: The `j` handler should explicitly state: if `selected_row == column.issues.len().saturating_sub(1)`, do nothing (already at bottom). Currently the spec omits the no-op guard.

### Step 2.4 — `k` (move up in column)

- State change: same spec section as 2.3. Symmetric boundary.
- Command: none.
- Error path: same gap — no explicit guard for `selected_row == 0`.

**GAP (AUTO-FIX)**: Same as 2.3 — explicit `k` boundary guard at top (row 0) missing.

### Step 2.5 — Horizontal scroll: visible column count calculation

- State change: `horizontal-scroll-spec.md` §Step 1 defines `visible_col_count()`.
- Command: none.
- Result handler: columns displayed using `horizontal-scroll-spec.md` §Step 2 (width distribution) and §Step 3 (slice).

**COMPLETE**

### Step 2.6 — Scroll dots rendering

- State change: `horizontal-scroll-spec.md` §Scroll Position Indicator defines `render_scroll_dots()`.
- Error path: scroll dots are not rendered when `total_columns <= visible_count` — specified.

**COMPLETE**

### Step 2.7 — Card rendering

- State change: `horizontal-scroll-spec.md` §Issue Card Format (Three-Line) defines the three-line layout.
- Priority colors: `horizontal-scroll-spec.md` §Priority coloring maps Highest/High/Medium/Low/Lowest to three constants. Defined.
- Error path: column with zero issues — renders empty column with header. Spec covers this in §No-Scroll Case: "empty columns" mentioned but the card rendering when issues list is empty for a column is not explicitly described (is an empty column rendered as just the header? is `selected_row` 0 allowed in an empty column?).

**GAP (NEEDS-INPUT)**: When a column has zero issues, `selected_row` is meaningless. Spec does not state whether an empty column can even be selected, whether `j`/`k` are no-ops in it, or whether the cursor jumps to the nearest non-empty column. Scroll-state preservation on refresh also says "clamp `selected_row` to the column's issue count" — if count is 0, that clamps to 0, which is still an invalid index if there are no issues.

### Step 2.8 — `p`: cycle project filter

- State change: `jira-plugin.md` §Keybindings Kanban Board: `p` → "Cycle project filter". Implementation logic (what the cycle does, what values it cycles through, whether it cycles to "All" then per-project-key or only per-project-key) is not specified beyond the keybinding table entry.

**GAP (AUTO-FIX)**: Cycle project filter logic is incompletely specified. Spec should state: cycle through (All projects, project_key_1, project_key_2, …) where project keys are derived from the distinct `project_key` values in loaded issues; "All" shows everything; cycling past the last project wraps back to "All".

### Step 2.9 — `D`: toggle Done column

- State change: `jira-plugin.md` §UI Design: "Done column hidden by default, toggle with `D`". The field `show_done: bool` is in the plugin struct. What happens to `selected_col` when Done is toggled off and the cursor was on the Done column? Not specified.

**GAP (AUTO-FIX)**: Spec should state: if the currently selected column is "Done" (or any column whose `StatusCategory == Done`) and `D` hides it, clamp `selected_col` to the last visible column.

---

## Flow 3: Open Issue Detail

**Trigger**: `Enter` on a selected issue in the board

### Step 3.1 — Open `JiraModal::IssueDetail`

- State change: `self.modal = Some(JiraModal::IssueDetail { issue_key, fields: None, transitions: None, comments: None, scroll_offset: 0, field_cursor: 0, edit_state: None })`. Described in the IssueDetail variant definition in `jira-plugin.md` §Plugin Struct.
- Command: none yet.

**COMPLETE** for state change.

### Step 3.2 — Lazy-fetch `FetchEditMeta`

- State change: `loading` not relevant here (detail opened while board is already showing). The `fields: None` in the modal variant is the pending-load marker.
- Command: `JiraCommand::FetchEditMeta { issue_key }` — sent upon opening detail. Described in `jira-plugin.md` §Issue Detail Modal: "Transitions and editmeta are fetched lazily — only when the user opens the detail modal". However the exact moment the commands are sent is under-specified: is it during `handle_key()` when `Enter` is pressed, or during the first `render()` call that sees `fields: None`?

**GAP (AUTO-FIX)**: Spec should explicitly state that `FetchEditMeta` and `FetchTransitions` and `FetchComments` are sent from `handle_key()` at the moment `Enter` is pressed (not from `render()`), to prevent duplicate sends on every render frame.

### Step 3.3 — Lazy-fetch `FetchTransitions`

- Command: `JiraCommand::FetchTransitions { issue_key }` — same concern as 3.2.
- Result handler: `JiraResult::Transitions(issue_key, transitions)` — the modal's `transitions` field is updated in `on_tick()` when this result arrives.
- Error path: if `FetchTransitions` returns `JiraResult::Error`, the transitions list stays `None`. The detail modal remains open. The user pressing `s` will re-trigger the fetch or see a spinner? Not specified.

**GAP (NEEDS-INPUT)**: If lazy-fetched `FetchTransitions` fails (network error while detail is open), what does the user see? Does the plugin re-send the command when `s` is pressed and `transitions` is still `None`? Spec should define the retry trigger.

### Step 3.4 — Lazy-fetch `FetchComments`

- Command: `JiraCommand::FetchComments { issue_key }` — same timing concern as 3.2.
- Result handler: `JiraResult::Comments(issue_key, comments)` — modal's `comments` field updated.
- Error path: not defined. Same gap as transitions.

**GAP (AUTO-FIX)**: State that `FetchComments` failure (returns `JiraResult::Error`) leaves `comments: None`, and the Comments section of the detail modal renders as "Comments (failed to load)" or similar. This prevents the section from silently appearing empty.

### Step 3.5 — Render detail modal

- State change: `jira-plugin.md` §Detail Modal Rendering defines full layout, `DetailFocus` enum, `DetailRow` render pseudocode. The flat-list-then-skip scroll approach is fully specified.
- `focus: DetailFocus` field: listed in §Detail Modal Rendering State block but is **not present** in the `JiraModal::IssueDetail` variant defined in §Plugin Struct. The variant lists `scroll_offset`, `field_cursor`, `edit_state` but not `focus`.

**GAP (AUTO-FIX)**: The `focus: DetailFocus` field is missing from the `JiraModal::IssueDetail` enum variant in the Plugin Struct section. It must be added to the variant definition to match the rendering pseudocode that references it.

### Step 3.6 — Field pre-population

- State change: `jira-plugin.md` §Field Pre-population for Edits defines the mapping rules for pre-populating `FieldValue` from `JiraIssue`.
- The spec requires looking up priority name to `AllowedValue.id` "case-insensitively" and matching component names to IDs from `AllowedValue` entries. The `AllowedValue` list for priority comes from `EditableField.allowed_values` (from editmeta). But `editmeta` is lazy-loaded. If the user opens the detail modal and editmeta hasn't arrived yet (`fields: None`), what is pre-populated?

**GAP (NEEDS-INPUT)**: Pre-population requires `EditableField.allowed_values` from editmeta (for ID lookup), but editmeta is lazy-loaded. If `e` is pressed before editmeta arrives, there are no `AllowedValue` IDs to look up. Spec should state: if editmeta has not yet loaded, pressing `e` is a no-op (or shows a "Loading…" message).

---

## Flow 4: Transition Issue

**Trigger**: `s` from kanban board or `s` from IssueDetail modal

### Step 4.1 — `s` pressed: check transition cache

- State change: if `transitions` is cached (not `None`), open `TransitionPicker` directly. If `None`, send `FetchTransitions` and open picker when result arrives.
- Command: `JiraCommand::FetchTransitions { issue_key }` (conditional).
- Result handler: `JiraResult::Transitions` → `on_tick()` sets the modal's `transitions` field; `TransitionPicker` is opened on next tick.
- However: there is a race — `s` is pressed, command sent, user continues to press keys. The spec does not define the intermediate UI state between pressing `s` and `JiraResult::Transitions` arriving. Does the plugin show a loading indicator? Block input?

**GAP (AUTO-FIX)**: Spec should describe the intermediate state when `s` is pressed and `FetchTransitions` is in-flight. Suggestion: set a `pending_transition_open: bool` flag; `on_tick()` opens `TransitionPicker` when both the flag is set and `JiraResult::Transitions` arrives.

### Step 4.2 — `TransitionPicker` navigation and selection

- State change: `jira-plugin.md` §Transition Picker Modal keybindings: `j`/`k` move cursor, `Enter` applies. `previous_modal` saving is fully specified.
- Command: `Enter` → check `transition.required_fields.is_empty()`.
- If empty: go directly to 4.4.
- If non-empty: go to 4.3.

**COMPLETE**

### Step 4.3 — `TransitionFields` form (required fields present)

- State change: `JiraModal::TransitionFields` opened with `previous_modal = TransitionPicker`. Fully specified in §Transition Picker Modal §Transition flow with required fields.
- Command: user fills fields; `Enter` (not `S` — `form-modal-spec.md` §Form for Transition Required Fields specifies `Enter` as submit for TransitionFields) POSTs the transition.
- Result handler: `JiraResult::TransitionComplete` (defined in Result Types).
- Error path: `JiraResult::TransitionFailed` → revert optimistic move, show blocking modal. Defined in §Optimistic UI.

**COMPLETE** (the asymmetry that `TransitionFields` uses `Enter` not `S` for submit is documented in `form-modal-spec.md`).

### Step 4.4 — Comment-type transition field

- State change: `jira-plugin.md` §Comment-Type Transition Fields: `is_comment == true` triggers `PluginAction::LaunchEditor` with context `"transition_comment:<issue_key>:<transition_id>"`.
- Command: `LaunchEditor` returned from `handle_key()`.
- Result handler: `on_editor_complete(content, "transition_comment:HMI-103:31")` — the spec provides this context string format. The `on_editor_complete` handler must build the `"update"."comment"` POST body and dispatch `JiraCommand::TransitionIssue`.
- However: the spec describes how to detect a comment field (`field_id == "comment"`) and how to POST it, but it does not specify the exact code path when a transition has BOTH a comment field AND a normal required field. Does the plugin open the `TransitionFields` form for the normal fields first and then `$EDITOR` for the comment field? Or does it go straight to `$EDITOR`?

**GAP (NEEDS-INPUT)**: When a transition has both a structured required field (e.g., Resolution) and a comment field, the ordering of the two input steps is not specified. Spec should define: (a) show `TransitionFields` form first for structured fields, then launch `$EDITOR` for the comment; or (b) only the comment path is supported (structured + comment simultaneously is treated as comment-only). Given the "Reject" / "Request Changes" use case, option (a) seems correct but is unspecified.

### Step 4.5 — POST transition (`JiraCommand::TransitionIssue`)

- State change: optimistic move: issue moved to target column locally before API responds. `jira-plugin.md` §Optimistic UI specifies this and says set `refreshing = true` immediately.
- Command: `JiraCommand::TransitionIssue { issue_key, transition_id, fields }`.
- Result handler: `JiraResult::TransitionComplete(issue_key)` → clear `refreshing`, schedule post-write refresh (500ms delay via `pending_refresh_at`).
- Error path: `JiraResult::TransitionFailed(issue_key, error)` → revert issue to original column, show blocking `ErrorModal`. All defined.

**COMPLETE**

### Step 4.6 — Post-transition refresh

- State change: `jira-plugin.md` §Refresh Behavior: "500ms delay" via `pending_refresh_at: Option<Instant>` in `on_tick()`. When fired, sends `FetchMyIssues(generation+1)`.
- Command: `JiraCommand::FetchMyIssues { generation }`.
- Result handler: same as Flow 8.

**COMPLETE** (mechanism fully specified).

---

## Flow 5: Edit Field in Detail

**Trigger**: `e` pressed on an editable field in `IssueDetail`

### Step 5.1 — `e` pressed: open `DetailEditState`

- State change: `jira-plugin.md` §DetailEditState: text/number fields → `EditingText`, select fields → `SelectOpen`.
- Pre-population: current value extracted from `JiraIssue` struct. Rules in §Field Pre-population for Edits.
- Gap from step 3.6 applies here: if editmeta hasn't loaded, `e` behavior is undefined.

**GAP**: Covered under Flow 3 step 3.6 gap (not duplicated here).

### Step 5.2 — Inline edit: `EditingText`

- State change: `DetailEditState::EditingText { field_id, buffer, cursor_pos }` — buffer pre-populated.
- `Enter` saves and sends `JiraCommand::UpdateField`.
- `Esc` cancels, sets `edit_state = None`.
- Terminal cursor positioning: `jira-plugin.md` §Detail Modal Rendering references the same `frame.set_cursor_position()` call required by `form-modal-spec.md` §Terminal Cursor Positioning. The detail modal spec does not repeat this requirement — it is present only in `form-modal-spec.md`.

**GAP (AUTO-FIX)**: The detail modal rendering spec should explicitly state that `frame.set_cursor_position()` must be called when `edit_state` is `EditingText`, mirroring the requirement in `form-modal-spec.md`. Currently only the form modal spec mentions this.

### Step 5.3 — Inline edit: `SelectOpen`

- State change: `DetailEditState::SelectOpen { field_id, options, cursor }` — pre-populated from issue's current value.
- `Enter` selects, sends `JiraCommand::UpdateField`.
- `Esc` cancels.
- The dropdown for `SelectOpen` in the detail modal is not given a rendering spec. `form-modal-spec.md` §Inline Select Dropdown specifies the dropdown for the form modal context. The detail modal context is different (the dropdown appears inside an overlay-over-board, not inside a form). The spec does not describe dropdown positioning within the detail modal.

**GAP (AUTO-FIX)**: Spec should state that `SelectOpen` in the detail modal uses the same dropdown layout rules as in `form-modal-spec.md` §Inline Select Dropdown, positioned below the selected field row within the modal's inner area.

### Step 5.4 — `PUT /rest/api/3/issue/{key}` (UpdateField)

- State change: after `Enter`, `edit_state = None`; `JiraCommand::UpdateField` sent.
- Command: `jira-plugin.md` §Command Types defines `UpdateField { issue_key, field_id, value }`. `jira-api-reference.md` §6 gives exact request shape.
- Result handler: `JiraResult::FieldUpdated(issue_key, field_id)`. The handler must update the local issue data from the in-flight value to match what was PUT. However the `FieldUpdated` result only carries `issue_key` and `field_id`, not the new value. The spec does not describe how the local issue cache is updated on `FieldUpdated` (does it just trigger a refresh, or update in place using the value that was sent?).
- Error path: `JiraResult::Error` → blocking modal (user-initiated action). Defined.

**GAP (AUTO-FIX)**: The `FieldUpdated` result handler is underspecified. It carries only `(issue_key, field_id)`, not the new value. Spec should state: on `FieldUpdated`, schedule a post-write refresh (same 500ms `pending_refresh_at` mechanism as transitions). Do NOT attempt to update the local cache from the sent value — just refresh. This avoids having to reconstruct `JiraIssue` field values from `FieldValue`.

---

## Flow 6: Add Comment

**Trigger**: `c` from kanban board or `c` from `IssueDetail`

### Step 6.1 — `c` pressed: return `PluginAction::LaunchEditor`

- State change: `plugin-architecture.md` §Editor Integration: plugin returns `LaunchEditor { content: "", context: "comment:HMI-103" }`.
- The context string format for comments is given in `plugin-architecture.md` §on_editor_complete Lifecycle: `"comment:HMI-103"`. This is consistent with `jira-plugin.md` §Comment Input via $EDITOR.

**COMPLETE**

### Step 6.2 — App writes temp file, stashes `pending_editor_plugin`

- State change: `plugin-architecture.md` §How It Works: app writes `$TMPDIR/jm-plugin-jira.txt`, sets `self.pending_editor_plugin = Some(("jira", context, temp_path))`.
- At the top of the run loop, before the next draw, the app detects this and suspends the TUI.

**COMPLETE**

### Step 6.3 — TUI suspend → `$EDITOR` launch → TUI resume

- State change: `plugin-architecture.md` §How It Works steps 4–5: `disable_raw_mode`, `LeaveAlternateScreen`, launch editor, `enable_raw_mode`, `EnterAlternateScreen`, `terminal.clear()`.
- The spec references `app.rs:167-196` for the existing code. This is a real implementation reference, not a specification. If those lines change, the spec becomes stale.

**GAP (AUTO-FIX)**: The spec should not reference implementation line numbers (`app.rs:167-196`). It should instead describe the required behavior inline so it does not go stale when the file changes.

### Step 6.4 — App reads temp file, calls `on_editor_complete`

- State change: temp file deleted. `plugin.on_editor_complete(content, "comment:HMI-103")` called.
- Command: `on_editor_complete` checks `content.is_empty()` → if empty, cancel. If non-empty, call `text_to_adf(content)` and dispatch `JiraCommand::AddComment`.
- The multi-paragraph `text_to_adf` variant (splitting on `\n\n`) is in `jira-api-reference.md` §8. The simpler single-paragraph version is in `jira-plugin.md` §ADF Handling. These two versions contradict each other — the simpler version creates only a single paragraph regardless of blank lines.

**GAP (AUTO-FIX)**: There are two `text_to_adf` implementations in the specs: a single-paragraph version in `jira-plugin.md` and a multi-paragraph (split on `\n\n`) version in `jira-api-reference.md`. Spec should canonicalize to the multi-paragraph version (from `jira-api-reference.md`) and remove or update the single-paragraph version in `jira-plugin.md`.

### Step 6.5 — POST comment

- Command: `JiraCommand::AddComment { issue_key, body }`.
- Result handler: `JiraResult::CommentAdded(issue_key)`. Triggers post-write refresh.
- Error path: `JiraResult::Error` → blocking modal. Defined.
- One gap: after `CommentAdded`, does the open `IssueDetail` modal's comments list get refreshed? The spec says `FetchComments` is lazy on detail open, and post-write refresh calls `FetchMyIssues`. `FetchMyIssues` does not re-fetch comments — it fetches issues via `/search`. So after adding a comment, the comments section in the still-open detail modal will not show the new comment until the modal is closed and re-opened.

**GAP (NEEDS-INPUT)**: After `CommentAdded`, the IssueDetail modal's comment list is stale. Spec should state: on `CommentAdded`, if `IssueDetail` for that issue is open, also send `FetchComments { issue_key }` to update the comments section in place.

---

## Flow 7: Create Issue

**Trigger**: `n` from kanban board

### Step 7.1 — Open `SelectProject`

- State change: `JiraModal::SelectProject { projects, cursor: 0 }`. Projects derived from distinct `project_key` values in `self.issues`.
- If `self.issues` is empty (no loaded issues), the project list is empty. The user cannot create an issue. This case is not handled.

**GAP (AUTO-FIX)**: Spec should define what happens when the user presses `n` but `self.issues` is empty (no distinct project keys available). Either show an error toast ("No projects available — board is empty") or disable the `n` keybinding with a hint.

### Step 7.2 — `Enter` on project → `FetchIssueTypes`

- State change: `cursor` position is captured; `project_key` recorded. `JiraModal::SelectIssueType { project_key, issue_types: vec![], cursor: 0 }` opened immediately? Or opened when `IssueTypes` result arrives?
- Command: `JiraCommand::FetchIssueTypes { project_key }`.
- Result handler: `JiraResult::IssueTypes(project_key, issue_types)` → `jira-plugin.md` §Issue Creation Flow says "open `SelectIssueType` when `IssueTypes` arrives".
- Intermediate state: between pressing `Enter` and result arriving, the modal is not specified. Is `SelectProject` still shown? Or is a loading spinner shown?

**GAP (AUTO-FIX)**: Spec should define the intermediate modal state between pressing `Enter` on a project and receiving `JiraResult::IssueTypes`. Either keep `SelectProject` with a "Loading…" footer, or open a new `Loading` intermediate state.

### Step 7.3 — `Enter` on issue type → `FetchCreateMeta`

- Same gap pattern as 7.2 for `FetchCreateMeta` latency.
- Command: `JiraCommand::FetchCreateMeta { project_key, issue_type_id }`.
- Result handler: `JiraResult::CreateMeta(CreateMetaResponse)` → open `CreateForm`.

**GAP (AUTO-FIX)**: Same intermediate state gap as 7.2.

### Step 7.4 — `CreateForm` filling

- State change: `JiraModal::CreateForm { project_key, issue_type_id, fields, form }`. Fields from `CreateMetaResponse`, pre-filtered to exclude `project`, `issuetype`, `reporter` (defined in `jira-api-reference.md` §11 and `jira-plugin.md` §Issue Creation Flow).
- Unsupported required field: spec says show error "Required field X has unsupported type — create in JIRA web UI." — but how is this error shown? As a `ValidationError` state in the form? As an `ErrorModal` immediately on form open? Not specified.

**GAP (AUTO-FIX)**: Spec should state when and how the "Required field X has unsupported type" error is surfaced: either (a) immediately on `CreateForm` open as an `ErrorModal` before the form is shown, preventing the user from filling anything; or (b) as a non-editable field row with the error text in place, allowing the user to fill other fields and only fail at submit time.

### Step 7.5 — `S` submit

- State change: `FormState::Submitting`. Spinner shown in footer.
- Command: `JiraCommand::CreateIssue { project_key, fields }`. Fields include injected `assignee` and `reporter` (from `account_id`) and `project.key` and `issuetype.id`.
- Result handler: `JiraResult::IssueCreated(key)` → close form, toast "Created HMI-116: Fix crash...", post-write refresh.
- Error path: `JiraResult::Error` → `FormState::ValidationError` with field-level errors marked. User can fix and press `S` again. Fully specified in `form-modal-spec.md` §Form Submission Flow.
- The toast message format: `jira-plugin.md` says "Created HMI-116: Fix crash when pressing Back" (full summary), `jira-api-reference.md` §12 says "Extract `.key` for the toast message: 'Created HMI-116'" (key only). Contradiction.

**GAP (AUTO-FIX)**: Two contradictory toast formats for `IssueCreated`: one includes the summary ("Created HMI-116: Fix crash when pressing Back"), the other is key-only ("Created HMI-116"). Since `JiraResult::IssueCreated` carries only the issue key (not the summary), the key-only format is what's implementable without an extra field. Either (a) canonicalize to key-only, or (b) add the summary to `JiraResult::IssueCreated(String, String)`.

---

## Flow 8: Auto-Refresh

**Trigger**: 60s timer fires in `on_tick()`

### Step 8.1 — Timer check

- State change: `jira-plugin.md` §Refresh Behavior: auto-refresh every `refresh_interval_secs` (default 60). Timer is checked in `on_tick()`. The exact timer mechanism (how the 60s interval is tracked) is not specified — is it an `Instant` stored in `last_sync`? A separate `next_refresh_at: Option<Instant>` field?

**GAP (AUTO-FIX)**: The spec defines `last_sync: Option<Instant>` in the plugin struct, but the trigger logic is described as "every 60 seconds" without specifying how the interval is tracked. Spec should state: use `last_sync` to compute elapsed time; if `Instant::now() - last_sync >= refresh_interval`, trigger refresh. Or define a dedicated `next_refresh_at` field analogous to `pending_refresh_at`.

### Step 8.2 — Check `refreshing` flag

- State change: if `refreshing == true`, skip. Prevents overlapping refreshes. Defined in §Refresh Behavior.

**COMPLETE**

### Step 8.3 — Send `FetchMyIssues(generation)`

- State change: `refreshing = true`, `generation += 1`, command sent.
- Command: `JiraCommand::FetchMyIssues { generation }`.

**COMPLETE**

### Step 8.4 — `on_tick()` drains result

- State change: `while let Ok(result) = result_rx.try_recv()` — defined in `plugin-architecture.md` §Result Processing in on_tick().
- Drain pattern is fully specified.

**COMPLETE**

### Step 8.5 — Check generation

- State change: `jira-plugin.md` §Refresh Behavior: "Results are only applied if the generation matches the current expected generation". On stale generation: discard data BUT still clear `refreshing = false`.
- This means `refreshing = false` is cleared in two places: (a) on matching generation → apply data, clear refreshing; (b) on stale generation → discard data, still clear refreshing.

**COMPLETE** (both cases explicitly specified).

### Step 8.6 — Update board, preserve scroll state

- State change: `horizontal-scroll-spec.md` §Scroll State Preservation Across Refresh: steps 1–7 are fully specified. Save `(selected_issue_key, selected_status_name)`, apply new data, re-find positions.

**COMPLETE**

### Step 8.7 — Stale detail modal handling

- State change: `jira-plugin.md` §Issue Detail Modal: "if the viewed issue no longer exists, close the modal with a toast". Defined.

**COMPLETE**

### Step 8.8 — Auto-refresh error

- State change: `JiraResult::Error` from a background-initiated fetch → non-blocking toast + stale-data indicator. Defined in §Error Handling.
- `refreshing = false` on error? Not explicitly stated. If `refreshing` is not cleared on error, future auto-refreshes are blocked forever.

**GAP (AUTO-FIX)**: Spec should explicitly state that `refreshing = false` is set on `JiraResult::Error` (in addition to on `JiraResult::Issues`). Currently the stale-generation case is covered but the error case is silent.

---

## Flow 9: Error Handling

**Trigger**: API error received from background thread

### Step 9.1 — User-initiated error → blocking modal

- State change: `JiraResult::Error { context, error }` received in `on_tick()` for a user-initiated action → `self.modal = Some(JiraModal::ErrorModal { title, message })`. Defined in §Error Handling.
- Error categories: `jira-plugin.md` §Error Categories table covers 401, 403, 404, 400, 429, network errors, config errors.
- How does `on_tick()` know whether an error was user-initiated or auto-refresh? The `context` string is the only signal (e.g., `"fetch_transitions:HMI-103"` vs. `"auto_refresh"`). The spec does not define the set of context string values that are classified as "user-initiated" vs. "auto-refresh".

**GAP (AUTO-FIX)**: Spec should define which context prefix strings classify an error as user-initiated (→ blocking modal) vs. auto-initiated (→ toast). For example: context starting with `"auto_refresh"` or `"fetch_my_issues"` → toast; all others → blocking modal. Without this, the implementation must guess which contexts warrant blocking modals.

### Step 9.2 — Error modal: Enter to dismiss

- State change: `Enter` on `ErrorModal` → `self.modal = None`. Defined implicitly by `jira-plugin.md` §Error Modal showing "Enter: dismiss".
- After dismissal, where does focus go? If the error was during a transition (modal was on `TransitionPicker`), `previous_modal` may still be set. Does dismissing an `ErrorModal` restore `previous_modal`?

**GAP (AUTO-FIX)**: Spec should state what happens after `ErrorModal` is dismissed: (a) `modal = None` → back to board, OR (b) `modal = previous_modal.take()` → restore prior modal state (if applicable). Currently the `previous_modal` mechanism is only described in the context of `TransitionPicker`/`TransitionFields`. The `ErrorModal` dismiss path does not reference it.

### Step 9.3 — Rate-limit (429): toast + retry

- State change: `jira-plugin.md` §Rate Limiting: show toast "JIRA rate limit, retrying in Xs", retry after `Retry-After` seconds. `jira-api-reference.md` §Auth shows `JiraError::RateLimited { retry_after_secs }`.
- How is the retry implemented? The spec says "Respect `Retry-After` headers" and "Never send concurrent requests for the same resource." The background thread would need to `thread::sleep(retry_after_secs)` or use a timer. Sleeping in the background thread is explicitly prohibited by `jira-plugin.md` §Refresh Behavior ("Do NOT use `thread::sleep()` in the background thread").

**GAP (NEEDS-INPUT)**: The rate-limit retry mechanism is contradictory: `jira-plugin.md` §Rate Limiting says "respect `Retry-After` headers" and the refresh spec says "do NOT use `thread::sleep()` in the background thread." If the background thread cannot sleep, it cannot implement the retry delay. Spec should clarify: either (a) the background thread MAY sleep for rate-limit retries only (special case), or (b) the background thread sends a `JiraResult::Error(RateLimited{secs})` back to the TUI thread, which schedules a re-send via `pending_refresh_at`-style timer.

### Step 9.4 — Panic detection (`TryRecvError::Disconnected`)

- State change: `jira-plugin.md` §Background Thread Specifics: "show a reconnect prompt". The specific UI element (ErrorModal? toast?) is not defined.

**GAP (AUTO-FIX)**: Spec should state: on `TryRecvError::Disconnected`, show an `ErrorModal` with title "JIRA connection lost" and message "Background thread crashed. Press Enter to reconnect." Enter should re-invoke the startup sequence (or navigate back to dashboard). Currently "reconnect prompt" is unspecified.

---

## Flow 10: `$EDITOR` Lifecycle

**Trigger**: `PluginAction::LaunchEditor` returned from `handle_key()`

### Step 10.1 — Plugin returns `LaunchEditor { content, context }`

- State change: `plugin-architecture.md` §How It Works: app sets `self.pending_editor_plugin = Some((name, context, temp_path))`.
- Content is written to `$TMPDIR/jm-plugin-jira.txt`. One file per plugin name means if two editors were somehow launched sequentially (impossible in current design since TUI is suspended), they'd share a path. Not a real issue but not noted.

**COMPLETE**

### Step 10.2 — App detects pending at top of run loop, suspends TUI

- State change: `disable_raw_mode()`, `LeaveAlternateScreen`. Defined.
- "At the top of the run loop" — `plugin-architecture.md` §How It Works step 4. The exact check point (before draw, after event poll) is defined.

**COMPLETE**

### Step 10.3 — Launch `$EDITOR`

- Command: spawn `$EDITOR` (fallback to `vim`) as a subprocess blocking the run loop.
- `$EDITOR` environment variable: if unset and `vim` is also not found, what happens? The spec says "falling back to `vim`" but does not define behavior if `vim` is absent.

**GAP (AUTO-FIX)**: Spec should define: if `$EDITOR` is unset and `vim` is not found on `$PATH`, show a toast "EDITOR not set — install vim or set $EDITOR" and cancel the operation (do not suspend TUI).

### Step 10.4 — Resume TUI

- State change: `enable_raw_mode()`, `EnterAlternateScreen`, `terminal.clear()`. Defined in `plugin-architecture.md`.

**COMPLETE**

### Step 10.5 — Read temp file, call `on_editor_complete`

- State change: file content read, file deleted, `plugin.on_editor_complete(content, context)` called.
- Empty content handling: `jira-plugin.md` §Comment Input via $EDITOR step 7: "If content is empty, cancel." Defined.

**COMPLETE**

### Step 10.6 — Plugin processes result

- State change: `on_editor_complete` converts to ADF, dispatches `JiraCommand::AddComment` (for comment context) or `JiraCommand::TransitionIssue` (for transition_comment context).
- Context string `"transition_comment:<issue_key>:<transition_id>"` is defined in §Comment-Type Transition Fields. The `on_editor_complete` handler must parse this format. Parsing logic is not specified (only the format string is given).

**GAP (AUTO-FIX)**: Spec should provide the context string parsing logic for `on_editor_complete`: `if let Some(rest) = context.strip_prefix("transition_comment:") { let parts: Vec<&str> = rest.splitn(2, ':').collect(); let (issue_key, transition_id) = (parts[0], parts[1]); ... }`. The current spec gives the format but not the parsing.

---

## Summary Table

| Flow | Step | Status | Classification |
|------|------|--------|----------------|
| 1 | 1.1 `open_plugin_screen` body | GAP | AUTO-FIX |
| 1 | 1.2 `on_enter()` early return on failure | GAP | AUTO-FIX |
| 1 | 1.3 Sync error → `ErrorModal` mechanism | GAP | AUTO-FIX |
| 1 | 1.4 `FetchFields` error handling | GAP | NEEDS-INPUT |
| 1 | 1.5 Initial load error category | GAP | NEEDS-INPUT |
| 2 | 2.3 `j` boundary guard missing | GAP | AUTO-FIX |
| 2 | 2.4 `k` boundary guard missing | GAP | AUTO-FIX |
| 2 | 2.7 Empty column behavior | GAP | NEEDS-INPUT |
| 2 | 2.8 Project filter cycle logic | GAP | AUTO-FIX |
| 2 | 2.9 `D` toggle when cursor on Done | GAP | AUTO-FIX |
| 3 | 3.2 When lazy-fetch commands are sent | GAP | AUTO-FIX |
| 3 | 3.3 `FetchTransitions` failure in detail | GAP | NEEDS-INPUT |
| 3 | 3.4 `FetchComments` failure display | GAP | AUTO-FIX |
| 3 | 3.5 `focus: DetailFocus` missing from struct | GAP | AUTO-FIX |
| 3 | 3.6 `e` before editmeta loads | GAP | NEEDS-INPUT |
| 4 | 4.1 `s` intermediate state (in-flight) | GAP | AUTO-FIX |
| 4 | 4.4 Comment + structured field ordering | GAP | NEEDS-INPUT |
| 5 | 5.2 Cursor positioning in detail modal | GAP | AUTO-FIX |
| 5 | 5.3 `SelectOpen` dropdown positioning in detail | GAP | AUTO-FIX |
| 5 | 5.4 `FieldUpdated` local cache update | GAP | AUTO-FIX |
| 6 | 6.3 Line number reference to `app.rs` | GAP | AUTO-FIX |
| 6 | 6.4 `text_to_adf` two contradictory versions | GAP | AUTO-FIX |
| 6 | 6.5 Comment not refreshed in open detail | GAP | NEEDS-INPUT |
| 7 | 7.1 `n` with empty issues list | GAP | AUTO-FIX |
| 7 | 7.2 `FetchIssueTypes` intermediate state | GAP | AUTO-FIX |
| 7 | 7.3 `FetchCreateMeta` intermediate state | GAP | AUTO-FIX |
| 7 | 7.4 Unsupported required field error surface | GAP | AUTO-FIX |
| 7 | 7.5 Toast format contradiction | GAP | AUTO-FIX |
| 8 | 8.1 Auto-refresh timer mechanism | GAP | AUTO-FIX |
| 8 | 8.8 `refreshing = false` on error | GAP | AUTO-FIX |
| 9 | 9.1 Context strings for user-initiated vs. auto | GAP | AUTO-FIX |
| 9 | 9.2 `ErrorModal` dismiss and `previous_modal` | GAP | AUTO-FIX |
| 9 | 9.3 Rate-limit retry vs. no-sleep rule | GAP | NEEDS-INPUT |
| 9 | 9.4 Panic reconnect prompt UI | GAP | AUTO-FIX |
| 10 | 10.3 `$EDITOR` not found | GAP | AUTO-FIX |
| 10 | 10.6 Context string parsing in `on_editor_complete` | GAP | AUTO-FIX |

**Totals**: 36 gaps — 27 AUTO-FIX, 9 NEEDS-INPUT

**COMPLETE steps** (no gaps): Flow 2 (h/l nav, scroll dots), Flow 4 (TransitionPicker nav, TransitionFields form, post-transition refresh), Flow 8 (refreshing flag, drain loop, generation check, scroll preservation, stale detail modal), Flow 10 (LaunchEditor return, run-loop detect, TUI resume, read+call on_editor_complete).

---

## NEEDS-INPUT Items (Require Design Decisions)

These 9 items require author decisions before the spec can be finalized. They cannot be resolved by inference from existing spec text.

1. **Flow 1 / 1.4** — `FetchFields` network error: silent degrade (no story-points/sprint shown) or blocking modal?
2. **Flow 1 / 1.5** — First-ever `FetchMyIssues` failure: blocking modal or toast + stale indicator?
3. **Flow 2 / 2.7** — Empty column behavior: can an empty column be selected? Are `j`/`k` no-ops? Does cursor auto-skip to the nearest non-empty column?
4. **Flow 3 / 3.3** — `FetchTransitions` failure while detail is open: retry on next `s` press? Show error in detail modal? Retry automatically?
5. **Flow 3 / 3.6** — `e` pressed before editmeta loads: no-op with hint, spinner, or error toast?
6. **Flow 4 / 4.4** — Transition with both a structured required field and a comment field: which input step comes first? Are both steps required?
7. **Flow 6 / 6.5** — After `CommentAdded`, should `FetchComments` be re-sent to update the open detail modal's comment list?
8. **Flow 9 / 9.3** — Rate-limit retry: may the background thread sleep for this specific case, or must the retry be TUI-timer-driven?
9. **Flow 10 / 10.3** (already marked AUTO-FIX above, but the fallback behavior if both `$EDITOR` and `vim` are absent is a product decision): cancel silently, toast, or modal?
