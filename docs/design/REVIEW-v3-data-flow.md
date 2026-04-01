# Data Flow Review v3 — JIRA Plugin End-to-End Trace

**Reviewer role**: State management architect. Zero bias toward approval.
**Date**: 2026-03-27
**Question answered**: Is every data flow path complete? Every state transition defined? Every command/result pair matched? Every error path handled?

---

## Scope

Traced against:
- `jira-plugin.md` — struct, modals, commands, results, refresh, ADF
- `jira-api-reference.md` — request/response shapes, pagination
- `form-modal-spec.md` — FormState, FieldValue, data flow
- `horizontal-scroll-spec.md` — BoardState, scroll preservation
- `plugin-architecture.md` — PluginAction, lifecycle
- `REVIEW-v2-concurrency.md` — four CONCERN areas (C1–C4)
- `REVIEW-v2-spec-completeness.md` — 26 unguided decisions

Previous reviews are checked: have their identified gaps been closed in the current spec documents? They have not — the reviewed spec documents remain unchanged. This review identifies new data-flow gaps on top of the pre-existing C1–C4 and spec-completeness blockers and assesses whether the spec is end-to-end complete as a data-flow specification.

---

## Flow 1: Open JIRA Screen → Fetch Issues → Render Board

### Happy Path

1. User presses `J` on dashboard → `App` sets `screen = ScreenId::Plugin("jira")` → calls `plugin.on_enter()`.
2. `on_enter()` spawns background thread with fresh `mpsc` channels. Sends `JiraCommand::FetchFields` (if custom field IDs not in config) and `JiraCommand::FetchMyIssues { generation: 0 }`. Sets `loading = true`.
3. Background thread calls `GET /rest/api/3/field`. Sends `JiraResult::Fields(Vec<JiraFieldDef>)`.
4. Background thread calls paginated `GET /rest/api/3/search`. Sends `JiraResult::Issues { generation: 0, issues: Vec<JiraIssue> }`.
5. `on_tick()` drains channel. On `Fields`: populates `story_points_field`/`sprint_field`. On `Issues`: sets `issues`, clears `loading`, sets `last_sync`, updates `BoardState`.
6. `render()` calls `render_board()` with populated `issues`. Board shows columns.

**GAP F1.1 — Sequencing of `FetchFields` vs. `FetchMyIssues`**

The spec says custom field IDs are needed to include them in the `/search` `fields` query param. But it does not define what happens when `FetchMyIssues` completes before `FetchFields`. If `story_points_field` and `sprint_field` are `None` when the search is sent, the fields query param will omit them. The first `Issues` result will lack story points and sprint data. The spec does not specify whether to delay `FetchMyIssues` until `Fields` completes, or whether to re-fetch after `Fields` arrives. A correct implementation must queue `FetchMyIssues` after `Fields` completes (if auto-discovery is needed), but the spec does not say this.

**Impact**: Story points and sprint will not appear on issue cards on first load when auto-discovery is used. A silent data gap, not a crash.

**GAP F1.2 — `myself` call is missing from the command/result protocol**

The spec describes `GET /rest/api/3/myself` as a startup validation step in `on_enter()`. However, there is no `JiraCommand::ValidateAuth` or `JiraResult::AuthValidated(accountId)` variant. The spec says to store `accountId` in `JiraPlugin.account_id`. There is no defined path from the background thread back to the plugin struct for the `accountId` from the `myself` call. If `myself` runs inside `on_enter()` on the TUI thread (synchronously before the background thread spawns), it blocks the TUI. If it runs in the background thread, there is no result variant to carry it back. This is a structural gap: the command/result protocol cannot express auth validation.

**Impact**: Either the TUI blocks on startup (bad), or `accountId` never reaches the plugin struct through the defined channel protocol, or an undocumented out-of-band mechanism is needed. The JQL query `assignee = '<accountId>'` cannot be constructed without this value reaching the background thread.

### Error Path

`on_enter()` startup validation failure (missing env var, missing config): spec says show a blocking error modal. The result path is: `on_enter()` directly sets `modal = Some(JiraModal::ErrorModal { ... })`. This is fine as a spec statement but:

**GAP F1.3 — No `ErrorModal` keybinding closes it and also prevents further operation**

The spec states ErrorModal shows "Enter: dismiss." When dismissed, `modal = None`. But the board state has `loading = true` and no data. The user sees a blank loading screen with no way to retry unless they press `R` (manual refresh). Pressing `R` will call `FetchMyIssues`, which requires `accountId` (which validation failed to populate). The refresh will produce a malformed JQL query. No spec description of this recovery path exists.

---

## Flow 2: Navigate Board → Select Issue → Open Detail Modal → Lazy-Fetch Transitions + EditMeta

### Happy Path

1. User navigates with `hjkl`. `BoardState.selected_col` and `selected_row` updated per the horizontal-scroll-spec rules.
2. User presses `Enter`. Plugin sets `modal = Some(JiraModal::IssueDetail { issue_key, fields: None, transitions: None, comments: None, scroll_offset: 0, field_cursor: 0 })`.
3. Plugin sends `JiraCommand::FetchTransitions { issue_key }`, `JiraCommand::FetchEditMeta { issue_key }`, `JiraCommand::FetchComments { issue_key }`.
4. `on_tick()` receives `JiraResult::Transitions(key, transitions)` → sets `modal.transitions = Some(transitions)`.
5. `on_tick()` receives `JiraResult::EditMeta(key, fields)` → sets `modal.fields = Some(fields)`.
6. `on_tick()` receives `JiraResult::Comments(key, comments)` → sets `modal.comments = Some(comments)`.
7. Modal renders updated content as each arrives (loading spinners per section until data arrives).

**GAP F2.1 — No loading state for lazy-loaded modal sections**

When `fields`, `transitions`, and `comments` are all `None`, the detail modal renders with nothing in those sections. The spec defines no per-section loading indicator (e.g., "Loading..." in the transitions slot, spinner in the comments area). Users will see fields disappear then reappear. The spec says nothing about rendering partial state.

**GAP F2.2 — Issue key matching when applying lazy results**

`on_tick()` receives `JiraResult::Transitions("HMI-103", transitions)`. The spec says to update the modal with this data. But it does not specify what to do if the modal was closed between when the command was sent and when the result arrives. The spec must say: "discard lazy-fetch results if `modal` is no longer `Some(IssueDetail { issue_key: key, .. })` where `key` matches the result's issue key." Without this check, results for a previously-viewed issue are applied to whatever modal is currently open (which may be a different issue's detail, a transition picker, or a create form).

**Impact**: Data from issue A's `FetchEditMeta` applied to issue B's detail modal, or applied to a TransitionPicker modal — corrupting displayed field data.

**GAP F2.3 — `FetchComments` is not in the spec's "Lazy fetch on detail open" summary**

The spec's detail modal description says transitions and editmeta are fetched lazily "only when the user opens the detail modal or presses `s`." Comments are mentioned in `JiraCommand::FetchComments` and `JiraResult::Comments` but are never explicitly said to be triggered on detail open. They are also not mentioned as lazy or eager anywhere in the flow description. The command exists, the result handler presumably exists, but the trigger is not specified.

---

## Flow 3: Press `s` → Transition Picker → Select Transition → Handle Required Fields → POST → Optimistic Move → Refresh

### Happy Path

1. User presses `s` on board or in detail modal.
2. If `transitions` are already cached (from detail open), skip fetch. Otherwise, send `JiraCommand::FetchTransitions { issue_key }`.
3. Wait for `JiraResult::Transitions`. Set `modal = Some(JiraModal::TransitionPicker { issue_key, transitions, cursor: 0 })`.
4. User navigates list, presses `Enter`. Selected `JiraTransition` checked for `required_fields`.
5. If `required_fields` is empty: send `JiraCommand::TransitionIssue { issue_key, transition_id, fields: None }`. Apply optimistic move. Set `refreshing = true`.
6. If `required_fields` is non-empty: set `modal = Some(JiraModal::TransitionFields { issue_key, transition, fields: required_fields.iter().map(|f| (f.into(), None)).collect(), form: FormState::Navigating { cursor: 0 } })`.
7. User fills fields. Presses `Enter`. Plugin builds `fields` JSON. Sends `JiraCommand::TransitionIssue { issue_key, transition_id, fields: Some(...) }`. Applies optimistic move.
8. `on_tick()` receives `JiraResult::TransitionComplete(key)` → sets `pending_refresh_at = Instant::now() + 500ms` (per C3 fix).
9. `on_tick()` fires after 500ms → sends `JiraCommand::FetchMyIssues { generation: G+1 }`.
10. `on_tick()` receives `JiraResult::Issues { generation, issues }` → applies to board.

**GAP F3.1 — Transition picker loading state (confirmed from REVIEW-v2-spec-completeness 1b.5)**

When `s` is pressed and transitions are not cached, there is a latency window (network call) where the user has pressed `s` but the picker is not yet shown. The spec defines no intermediate state. The modal is `None` until `Transitions` arrives. During this window, the user's keystrokes go to the board-level handler. If the user presses `s` again, a second `FetchTransitions` is sent. The spec does not define deduplication for in-flight lazy fetches.

**GAP F3.2 — TransitionField form uses `Enter` for submit, not `S` (spec inconsistency)**

`form-modal-spec.md` says: "Submit key is `Enter` (not `S`) — there's usually only 1-2 fields." But `JiraModal::TransitionFields` uses the same `FormState` type as `CreateForm`, which has `Submitting` state. If `Enter` in `EditingText` state saves the current field and moves to the next (standard EditingText behavior), how does the user submit the form if there is only one field? The user presses `Enter` to save the field → `FormState::Navigating`. Now `Enter` in Navigating moves to... the next field (but there isn't one)? Or submits? The spec does not clarify this distinction for the single-field transition case.

**GAP F3.3 — Optimistic move data structure not specified**

The spec says "optimistically move the issue to the target column locally." `BoardState` stores `Vec<StatusColumn>` where each column has a status name and a list of issues. An optimistic move requires: (1) removing the issue from its current column, (2) inserting it in the target column, (3) storing the original column name for potential revert. No field in `JiraPlugin` is defined for storing the pre-transition status. `optimistic_transitions: HashMap<String, JiraStatus>` is implied but not specified. (This is REVIEW-v2-spec-completeness 1b.3 — still open.)

**GAP F3.4 — `TransitionFailed` revert data flow**

On `JiraResult::TransitionFailed(key, error)`: the spec says revert the issue to its original column and show a blocking error modal. Revert requires the stored pre-transition status (GAP F3.3). Without the storage field, revert is impossible to implement. Additionally, the spec does not specify whether `refreshing` is cleared on `TransitionFailed`. If `refreshing = true` was set when the transition was sent (per C2 fix), and `TransitionFailed` does not clear it, all subsequent auto-refreshes are permanently suppressed.

**GAP F3.5 — Comment-required transition (`is_comment: true`) has no data flow**

`TransitionField` has an `is_comment: bool` field. The JIRA API reference shows that comment-required transitions use a different POST body shape (`"update": { "comment": [...] }` instead of `"fields": { ... }`). The spec says `is_comment` exists in the struct but defines no handling for it in the form or submission path. When `is_comment = true`: does a `$EDITOR` open? Does the form show a TextArea field? Is the POST body constructed differently? None of this is specified.

**Impact**: Transitions requiring a comment will either produce a malformed API request or silently omit the comment, causing the transition to fail with a 400 error.

---

## Flow 4: Press `c` → `$EDITOR` → `text_to_adf` → POST Comment → Refresh

### Happy Path

1. User presses `c` on board or in detail modal.
2. Plugin returns `PluginAction::LaunchEditor { content: String::new(), context: format!("comment:{}", issue_key) }`.
3. App writes temp file, suspends TUI, opens `$EDITOR`, resumes TUI, calls `plugin.on_editor_complete(content, "comment:HMI-103")`.
4. `on_editor_complete` calls `text_to_adf(content)`. Sends `JiraCommand::AddComment { issue_key, body: adf_json }`.
5. `on_tick()` receives `JiraResult::CommentAdded(key)`. Triggers post-write refresh.

**GAP F4.1 — `PluginAction::LaunchEditor` is not in the spec's `PluginAction` definition in `jira-plugin.md`**

`jira-plugin.md` defines `PluginAction` with three variants: `None`, `Back`, `Toast(String)`. `LaunchEditor` is absent from this definition. `plugin-architecture.md` includes `LaunchEditor` in its version. These are inconsistent. An agent reading only `jira-plugin.md` will not know to use `LaunchEditor`. (This is REVIEW-v2-spec-completeness issue 1/10 — still open.)

**GAP F4.2 — Empty content handling is defined, but `$EDITOR` not-saved path is ambiguous**

The spec says "If content is empty (user quit without writing), cancel." This handles the case where the user opens `$EDITOR` and saves an empty file. It does not handle the case where the user opens `$EDITOR`, the editor crashes, or the temp file cannot be read. `on_editor_complete` is called with whatever is in the temp file — which may be the initial content string (empty) or garbage if the file was not created. The spec should specify: treat any read error as empty → cancel.

**GAP F4.3 — `CommentAdded` result does not update the detail modal's comment cache**

After `JiraResult::CommentAdded(key)` arrives, the spec triggers a board refresh. The refresh fetches `FetchMyIssues`, which does NOT re-fetch comments. If the detail modal is still open with `comments` cached from the initial lazy load, the new comment will not appear until the user closes and reopens the detail modal. The spec says nothing about invalidating the comment cache on `CommentAdded`. Should `FetchComments` be triggered again after `CommentAdded`? Unspecified.

---

## Flow 5: Press `n` → Select Project → Select Issue Type → Fetch CreateMeta → Form Modal → Fill Fields → POST Create → Refresh

### Happy Path

1. User presses `n`. Plugin shows project selection list (from `issues` — unique `project_key` values).
2. User selects project. Plugin shows issue type list. But where does the issue type list come from?

**GAP F5.1 — Issue type fetch path is undefined**

Step 2 shows "Select Issue Type (fetched from JIRA createmeta for the selected project)." The `JiraCommand::FetchCreateMeta { project_key, issue_type_id }` requires BOTH a `project_key` AND an `issue_type_id`. But to get the `issue_type_id`, the user must first see a list of issue types — which comes from `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes`. There is no `JiraCommand` variant for this endpoint. The command for `FetchCreateMeta` fetches fields for a specific issue type, not the list of issue types. The intermediate step "show issue type selector" has no command/result pair.

**Impact**: The create flow cannot be implemented without an undocumented API call for issue type listing, or a completely missing command/result variant. This is a structural dead end in the data flow.

**GAP F5.2 — Project list derivation vs. stale data**

The project selector shows unique `project_key` values from `issues`. If the user has no assigned issues in a project, that project is not available for creation. This is by design (per permissions summary). But no spec text says "derive from `issues`" — it is implied by the keybinding description mentioning "select project." An agent must infer this.

**GAP F5.3 — `FormState::Submitting` → `JiraResult::IssueCreated` path: what closes the form?**

On `IssueCreated(key)`: `form-modal-spec.md` says "Close form. Show toast 'Created HMI-116'. Trigger board refresh." But the spec does not describe where this logic lives. `on_tick()` processes `JiraResult::IssueCreated`. At that point, `modal = Some(JiraModal::CreateForm { ... })` and `form.state = FormState::Submitting`. The `on_tick()` result handler must set `modal = None` and return a toast. But `on_tick()` returns `Vec<String>` for notifications, not `PluginAction::Toast`. There is no mechanism in `on_tick()` to enqueue a toast visible to the App. Only `handle_key()` can return `PluginAction::Toast`. This is a structural gap: success toasts from `on_tick()` have no delivery path.

**Impact**: "Created HMI-116" toast silently never appears. The only notification mechanism for background-thread results that need to surface as toasts is the `on_tick()` notification return path, but `PluginRegistry::tick_screen` forwards those to the sidebar notification center — not to the app's toast widget.

**GAP F5.4 — Validation error path from `JiraResult::Error` during create**

On `JiraResult::Error(e)` during create: form-modal-spec.md says "Parse JIRA error response. Map field-level errors to form fields via field ID." The `JiraError` struct has `field_errors: HashMap<String, String>`. The spec says transition to `FormState::ValidationError { cursor, errors }`. But `on_tick()` receives the `Error` result — how does it know the error was from a `CreateIssue` command vs. a different in-flight command? Multiple commands can be in-flight. `JiraResult::Error` carries no command context. An agent cannot reliably map an `Error` result to the correct modal state without correlating it to the command that caused it.

**Impact**: If a `FetchTransitions` fails with an error while a create form is open, the error may be incorrectly treated as a create validation error, setting `ValidationError` state on the form.

---

## Flow 6: Press `e` on Editable Field in Detail → Edit → PUT Update → Refresh

### Happy Path

1. User presses `e` on a field marked `[e:edit]` (editable per `EditableField` from editmeta).
2. For `Text`/`Number` field: an inline edit mode opens within the detail modal.
3. For `Select` field: a dropdown appears.
4. User confirms value. Plugin sends `JiraCommand::UpdateField { issue_key, field_id, value: serde_json::Value }`.
5. `on_tick()` receives `JiraResult::FieldUpdated(key, field_id)`. Triggers post-write refresh.

**GAP F6.1 — Inline edit UI for detail modal fields is not specified**

The detail modal spec defines `field_cursor: usize` and says `e` edits the selected field. But there is no edit state defined within `JiraModal::IssueDetail`. `FormState` is used by `CreateForm` and `TransitionFields`, not by `IssueDetail`. The detail modal has no `FormState` field. So when `e` is pressed: how does the inline text input or dropdown appear? What state variable tracks that we are in edit mode? What key exits edit mode? The spec defines none of this for the detail modal case.

**Impact**: The detail modal's edit path has no state machine definition. An agent must invent edit state storage for the detail modal from scratch.

**GAP F6.2 — `FieldUpdated` result does not update the local issue data**

On `JiraResult::FieldUpdated(key, field_id)`: the spec triggers a board refresh. Until that refresh completes (500ms delay + network latency), the detail modal continues to show the old value for the updated field. The spec describes no local optimistic update for field edits (only for transitions). This is a visible UX gap but a spec omission rather than a correctness bug — the refresh will eventually correct it.

**GAP F6.3 — `FieldValue` → `serde_json::Value` conversion for `UpdateField`**

`JiraCommand::UpdateField` takes `value: serde_json::Value`. The detail modal uses `EditableField.allowed_values` to populate a dropdown. But the edit input for the detail modal does not use `FormState` or `FieldValue`. The spec does not define how to convert a user-selected allowed value into the `serde_json::Value` for the `UpdateField` command body. For select fields, the correct format is `{ "id": "..." }`. For number fields, a bare number. The conversion rules are only defined in `form-modal-spec.md`'s `FieldValue` section, which is coupled to `FormState`. There is no equivalent for the detail modal's ad-hoc edit path.

---

## Flow 7: Auto-Refresh Every 60s → Background Fetch → Generation Check → Update Board

### Happy Path

1. `on_tick()` checks if `Instant::now() - last_sync >= refresh_interval`. If yes and `refreshing == false`: increment `generation`, send `JiraCommand::FetchMyIssues { generation }`, set `refreshing = true`.
2. Background thread fetches all pages of `/search`. Accumulates into `Vec<JiraIssue>`. Sends single `JiraResult::Issues { generation, issues }`.
3. `on_tick()` drains channel. On `Issues`: if `generation == expected_generation`, apply to board, clear `refreshing`, update `last_sync`. If generation mismatch, discard result **and clear `refreshing`** (per C1 fix).
4. Board preserves scroll position per horizontal-scroll-spec's "Scroll State Preservation Across Refresh" algorithm.

**GAP F7.1 — `refreshing` cleared on generation mismatch: C1 is NOT fixed in current spec docs**

REVIEW-v2-concurrency identified this as C1 and required a spec fix. The current `jira-plugin.md` text says: "discard the data BUT still clear `refreshing = false`." This text IS present in the refresh behavior section at line 879: "When a `JiraResult::Issues { generation, .. }` arrives with a stale generation (older than current), discard the data BUT still clear `refreshing = false`." **C1 is fixed in the spec text.** No gap.

**GAP F7.2 — `pending_refresh_at` timer: C3 is NOT fixed in current spec docs**

REVIEW-v2-concurrency required the spec to say: "On write result, record `pending_refresh_at = Instant::now() + 500ms`. In `on_tick()`, if deadline is past and `refreshing` is false, send `FetchMyIssues`." The current spec says: "The 500ms delay between a write completion and the post-write refresh MUST be implemented as a TUI-side timer checked in `on_tick()` — e.g., `pending_refresh_at: Option<Instant>`." This description IS present at line 877. **C3 is fixed in the spec text.** However, `pending_refresh_at` is NOT a field in the `JiraPlugin` struct definition (lines 319–352). The struct has no such field. Agent must add it without spec guidance on the type.

**GAP F7.3 — Auto-refresh while detail modal is open: partial spec coverage**

The spec says at lines 883–884: "When a refresh completes and the detail modal is open, check if the viewed issue still exists in the new data. If not, close the modal with a toast ('Issue no longer assigned to you'). If it still exists, update the displayed data in place." The "update displayed data in place" path is not specified. The detail modal caches `fields`, `transitions`, and `comments` (all lazy-loaded). When an `Issues` refresh arrives, should those caches be invalidated? If the issue's status changed, the `transitions` cache may be stale (workflows can have different transitions per status). The spec says to update displayed data but does not define which cache fields to invalidate.

**GAP F7.4 — Board column disappearance on refresh**

The horizontal-scroll-spec defines scroll preservation steps 5–6 as: "If the status no longer exists, clamp `selected_col` to `columns.len() - 1`." But if the "Done" column is hidden (`show_done = true` → filtered out), and the selected issue moves to Done in the refresh, it disappears from the visible columns. `selected_row` now points past the end of its column. The clamping logic covers this, but the hidden-Done-column case adds complexity: after clamping, the selected issue may be in a column the user cannot see. No spec text addresses this interaction between `show_done` filtering and scroll preservation.

---

## Flow 8: Network Error During Any Operation → Error Modal or Toast

### Error modal path (user-initiated actions)

`JiraResult::Error(e)` arrives during a user-initiated operation. Plugin sets `modal = Some(JiraModal::ErrorModal { title, message })`.

**GAP F8.1 — `JiraResult::Error` cannot be attributed to a specific operation**

As noted in F5.4: `JiraResult::Error` carries only the error, not the command that caused it. Multiple commands can be in-flight simultaneously (e.g., `FetchTransitions` + `FetchEditMeta` + `FetchComments` sent on detail open). If any of these fail, `Error` arrives and the plugin cannot distinguish "transitions fetch failed" from "editmeta fetch failed." All three failures produce identical `Error` results. The spec defines no command correlation ID or result tagging.

**Impact**: An editmeta fetch failure during detail open produces the same `Error` as a transition failure. Both trigger the error modal path, but the error message ("Failed to load edit metadata" vs. "Failed to load transitions") cannot be constructed without knowing the command. Also: if `FetchComments` fails but `FetchTransitions` succeeds, should the modal still open? The spec has no partial-failure handling.

**GAP F8.2 — Error modal dismissal path for auth failures**

On 401 error (auth failure): blocking modal with message about checking `JIRA_API_TOKEN`. User dismisses. `modal = None`. The board is in `loading = true` state (if this was during initial load) or in stale state (if during refresh). No spec for retry or "re-authenticate" flow. No keyboard shortcut to re-trigger auth validation.

**GAP F8.3 — Stale-data indicator is not a field in `JiraPlugin`**

The spec describes a `[stale]` indicator in the header for auto-refresh failures. But `JiraPlugin` has no `is_stale: bool` field defined in the struct (lines 319–352). The struct has `last_error: Option<String>` which could be used, but the spec treats it as an error field, not a stale indicator. An agent must add a field not in the struct definition.

---

## Flow 9: Rate Limit 429 → Retry After Delay

### Background thread path

1. `ureq` call returns 429. Background thread extracts `Retry-After` header. Produces `JiraError::RateLimited { retry_after_secs }`.
2. Background thread sends... what? There is no `JiraResult::RateLimited` variant. The only result variant for errors is `JiraResult::Error(JiraError)`.
3. TUI receives `JiraResult::Error`. Classifies as rate-limited based on `JiraError::status_code == 429`.
4. Spec says: "Show a toast when rate-limited ('JIRA rate limit, retrying in Xs')."

**GAP F9.1 — Rate limit retry is not in the background thread's command loop**

The spec says the background thread should "Respect `Retry-After` headers on 429 responses" and "Auto-retry after `Retry-After` header delay." But the background thread sends `JiraResult::Error(RateLimited { ... })` to the TUI thread. The TUI then shows a toast. Nothing in the spec says the background thread retries the failed command after the delay. If the background thread sends the error and moves on, the command that was rate-limited is silently dropped. The TUI has no mechanism to replay the failed command.

**REVIEW-v2-spec-completeness 1e.4** identified this gap: "A sleep of 30+ seconds would block the polling loop. No guidance on how to implement interruptible sleep." This gap is still open. The background thread cannot block on `thread::sleep(retry_after)` because that blocks the `recv_timeout` loop and prevents shutdown flag detection. The spec does not describe the interruptible retry mechanism.

**Impact**: Rate-limited commands are silently dropped. User sees a toast but the action they requested (transition, comment, create) is not retried. If auto-refresh is rate-limited, the board becomes permanently stale until the user manually presses `R`.

**GAP F9.2 — 429 during user-initiated action should be blocking modal, not toast**

The error handling table says user-initiated actions get blocking modals; auto-refresh failures get toasts. Rate limit 429 can happen during either. The spec says "Show a toast" for rate limits universally, which contradicts the general rule for user-initiated actions. If the user initiates a transition and it is rate-limited, the user gets a toast (non-blocking) and has no idea whether the transition will be retried or was dropped.

---

## Cross-Cutting Data Flow Gaps

### C-GAP-1 — Toast delivery mechanism from `on_tick()` is broken

This is the most structurally important gap. The flow is:

1. `JiraResult::IssueCreated(key)` arrives in `on_tick()`.
2. Form should close. Toast "Created HMI-116" should appear.
3. `on_tick()` returns `Vec<String>` — forwarded to `PluginSidebar.push_notification()`.
4. But `push_notification` goes to the notification center, not the app's toast widget.
5. There is no path from `on_tick()` to `PluginAction::Toast`.

The only way to emit a toast is through `handle_key()` returning `PluginAction::Toast`. Background-thread results processed in `on_tick()` have no toast delivery path.

This affects:
- "Created HMI-116" toast after create (F5.3)
- "Issue no longer assigned to you" toast on refresh with stale modal (F7.3)
- Rate limit toast (F9.2)
- Any other success/error toast triggered by background results

**The spec must add a toast queue to `JiraPlugin`** — e.g., `pending_toasts: Vec<String>` — and define that `on_tick()` returns these via the notification return value (which must then be routed to the app toast, not just the sidebar). Or `PluginAction` must be extended to allow `on_tick()` to return actions. Currently neither mechanism exists.

### C-GAP-2 — `JiraResult::Error` context ambiguity (aggregates F5.4, F8.1)

The command/result protocol is asymmetric: every command has a unique type, but all errors collapse to `JiraResult::Error(JiraError)`. With multiple in-flight commands (detail open triggers 3 simultaneous fetches), error attribution is impossible. The spec needs either: (a) command-tagged error results (`Error { command_tag: CommandTag, error: JiraError }`), or (b) a policy that only one command is in-flight at a time (which contradicts the detail-open behavior of sending 3 simultaneous fetches).

### C-GAP-3 — Missing command variant for issue type listing (Flow 5)

`GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes` has no `JiraCommand` variant and no `JiraResult` variant. The create flow explicitly requires this endpoint (Step 2 of the creation UI). This is not a minor gap — it is a missing API call that makes the create flow structurally incomplete.

### C-GAP-4 — `pending_refresh_at` and `is_stale` fields missing from `JiraPlugin` struct

Two fields referenced in spec behavior sections are absent from the struct definition:
- `pending_refresh_at: Option<Instant>` — needed for 500ms post-write refresh delay (C3 fix)
- Stale indicator field — needed for auto-refresh failure display

### C-GAP-5 — `account_id` population path for background thread

The background thread constructs the JQL query `assignee = '<accountId>'`. The `accountId` is stored in `JiraPlugin.account_id: Option<String>`. The background thread is spawned with closures that capture data from `JiraPlugin`, or it is passed in through the command. Neither mechanism is specified. The `JiraCommand::FetchMyIssues` command does not carry `account_id`. The background thread cannot use `account_id` from `JiraPlugin` directly (different thread). The spec must define how `account_id` reaches the background thread for use in JQL construction.

---

## Command/Result Pair Audit

| JiraCommand | JiraResult(s) Expected | Matched? |
|-------------|------------------------|----------|
| FetchMyIssues { generation } | Issues { generation, issues } | Yes |
| FetchTransitions { issue_key } | Transitions(key, transitions) | Yes |
| TransitionIssue { key, id, fields } | TransitionComplete(key) or TransitionFailed(key, error) | Yes |
| UpdateField { key, field_id, value } | FieldUpdated(key, field_id) | Yes |
| AddComment { key, body } | CommentAdded(key) | Yes |
| FetchComments { key } | Comments(key, comments) | Yes |
| FetchCreateMeta { project_key, issue_type_id } | CreateMeta(response) | Yes |
| CreateIssue { project_key, fields } | IssueCreated(key) | Yes |
| FetchEditMeta { key } | EditMeta(key, fields) | Yes |
| FetchFields | Fields(field_defs) | Yes |
| Shutdown | (none — thread exits) | N/A |
| **MISSING: FetchIssueTypes { project_key }** | **MISSING: IssueTypes(key, types)** | **NO** |

All defined commands have matched result variants. The structural gap is the missing command for issue type listing (C-GAP-3).

Every `JiraResult` variant has a receiving path in `on_tick()` per the spec. However, the `Error` variant context ambiguity (C-GAP-2) means error results cannot be reliably attributed to their source command.

---

## State Transition Completeness

### `FormState` State Machine

All transitions are defined for `Navigating`, `EditingText`, `SelectOpen`, `MultiSelectOpen`, `Submitting`, and `ValidationError`. Two gaps:

1. **`EditingText` → `Submitting` path missing for single-field transition form**: In `JiraModal::TransitionFields`, pressing `Enter` on the only field saves the value (EditingText → Navigating), but the submit trigger is also `Enter` (not `S`). The transition from `Navigating` → `Submitting` when the field count is 1 and Enter is pressed requires detecting "all fields filled and cursor is on last field." Not specified.

2. **`ValidationError` field correction does not clear individual errors**: spec says same as Navigating but shows errors. When the user corrects field `f` and presses Enter (EditingText → Navigating), field `f`'s error marker should clear. The spec says the state remains `ValidationError` until `S` is pressed again. But it does not specify whether correcting a field removes its `!` marker immediately or only after re-submission. An agent will make an arbitrary choice.

### `JiraModal` State Machine

| From State | Trigger | To State | Defined? |
|------------|---------|----------|----------|
| None | `Enter` on board | IssueDetail | Yes |
| None | `s` on board | IssueDetail → TransitionPicker (after fetch) | Partial — loading window undefined |
| None | `n` | Project select → Type select → CreateForm | Partial — type select has no state variant |
| IssueDetail | `s` | TransitionPicker | Yes |
| IssueDetail | `e` | edit state ??? | Not defined |
| IssueDetail | `Esc` | None | Yes |
| TransitionPicker | `Enter` (no fields) | None (sends command) | Yes |
| TransitionPicker | `Enter` (with fields) | TransitionFields | Yes |
| TransitionPicker | `Esc` | IssueDetail (or None?) | **Ambiguous** |
| TransitionFields | submit | None (sends command) | Yes |
| TransitionFields | `Esc` | TransitionPicker or None? | **Ambiguous** |
| CreateForm | `S` | Submitting (within form) | Yes |
| CreateForm | `Esc` | None | Yes |
| ErrorModal | `Enter` | None | Yes |

**GAP SM-1 — TransitionPicker `Esc` destination is ambiguous**

If the user opens the transition picker from the board (not from detail modal), pressing `Esc` should return to the board (modal = None). If opened from inside the detail modal (which is itself a modal), `Esc` should return to the detail modal. The spec has no "previous modal" stack or history. `JiraModal` is a flat enum — there is no nesting. An agent implementing "return to detail modal from transition picker" must store the previous modal state manually.

**GAP SM-2 — Issue type selection step has no `JiraModal` variant**

The create flow has three steps: project select, issue type select, then form. Steps 1 and 2 are rendered as selection lists. But `JiraModal` has only `CreateForm` — there is no `ProjectSelect` or `IssueTypeSelect` variant. Steps 1 and 2 have no corresponding state in the modal enum.

---

## Dead Ends — Fetched But Never Used

| Data | Fetched By | Consumed By | Dead End? |
|------|-----------|-------------|-----------|
| `JiraFieldDef` (from `FetchFields`) | `JiraResult::Fields` | `story_points_field`, `sprint_field` discovery | No — used in `FetchMyIssues` query param |
| `JiraTransition.required_fields` | `FetchTransitions` | `TransitionFields` modal | Partial — `is_comment` field exists but has no handler (GAP F3.5) |
| `EditableField.allowed_values` | `FetchEditMeta` | Detail modal `e` edit | Partial — edit state for detail modal undefined (GAP F6.1) |
| `JiraComment.id` | `FetchComments` | Detail modal render | **Dead end** — `id` is stored but never used. No comment editing or deletion. |
| `CreateMetaResponse.fields` | `FetchCreateMeta` | `CreateForm.fields` | Yes, consumed |
| `JiraTransition.to_status` | `FetchTransitions` | Transition picker display | Yes |

`JiraComment.id` is fetched and stored but has no use in the plugin. This is a minor dead field (comments are read-only), but it is wasted data.

---

## Summary Table

| Flow | Verdict | Critical Gaps |
|------|---------|---------------|
| 1. Open screen → fetch → render | PARTIAL | FetchFields/FetchMyIssues sequencing; `myself` has no result variant |
| 2. Detail modal lazy fetch | PARTIAL | Result attribution to stale modals; FetchComments trigger undefined |
| 3. Transition flow | FAIL | Missing issue type command; is_comment unhandled; optimistic revert storage undefined |
| 4. Comment via $EDITOR | PARTIAL | LaunchEditor missing from jira-plugin.md PluginAction; comment cache not invalidated |
| 5. Create issue | FAIL | FetchIssueTypes command missing; toast delivery path broken; Error context ambiguity |
| 6. Edit field in detail | FAIL | Inline edit state for detail modal completely undefined |
| 7. Auto-refresh | PASS | C1 and C3 text fixes present; pending_refresh_at missing from struct |
| 8. Network error handling | PARTIAL | Error context ambiguity; stale field missing from struct |
| 9. Rate limit retry | FAIL | Retry mechanism unspecified; blocking vs. toast classification contradicts rule |
| Cross-cutting | FAIL | Toast delivery from on_tick() broken; command/result context; missing struct fields |

---

## Verdict

**REJECT**

### Issues Preventing Implementation

The spec set has nine data-flow failures that would produce either compile errors, structural dead ends, or silent wrong behavior:

**P0 — Toast delivery from `on_tick()` is architecturally broken (C-GAP-1)**

`on_tick()` returns `Vec<String>` for sidebar notifications. There is no mechanism to emit a `PluginAction::Toast` from a background result. All success toasts from write operations (create, transition, comment) and all runtime events (issue disappeared from board, stale data) use this path. Without a toast queue on the plugin struct and a corresponding drain in `handle_key()` or a `PluginAction::Tick(Vec<PluginAction>)` mechanism, none of these toasts will reach the user. Required fix: define `pending_toasts: Vec<String>` on `JiraPlugin`, populate in `on_tick()`, drain in `handle_key()` returning `Toast(msg)` or drain via the notification path in a way that reaches the app toast widget.

**P1 — Missing `FetchIssueTypes` command/result pair (C-GAP-3)**

The create flow Step 2 requires `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes`. No `JiraCommand` or `JiraResult` variant exists for this. The creation flow cannot complete. Required fix: add `JiraCommand::FetchIssueTypes { project_key: String }` and `JiraResult::IssueTypes(String, Vec<IssueTypeSummary>)`, plus the intermediate modal state variants for project/type selection.

**P2 — `JiraModal` has no state for project/type selection steps (GAP SM-2)**

Steps 1 and 2 of the create flow have no `JiraModal` variant. Agent must invent them. Required fix: add `ProjectSelect { projects: Vec<ProjectSummary>, cursor: usize }` and `IssueTypeSelect { project_key: String, types: Vec<IssueTypeSummary>, cursor: usize }` variants to `JiraModal`.

**P3 — Detail modal edit path has no state definition (GAP F6.1)**

`JiraModal::IssueDetail` has no edit state. When `e` is pressed, no spec describes what state change occurs, what UI appears, or what keys govern the edit. Required fix: add an `editing` field to `IssueDetail` with defined state (likely a variant covering text editing buffer and cursor position for a given field), or document that detail-field editing reuses `FormState` inline.

**P4 — `myself` call has no command/result path (GAP F1.2)**

`accountId` from `GET /rest/api/3/myself` cannot reach the background thread through the defined channel protocol. Required fix: either define `JiraCommand::ValidateAuth` and `JiraResult::AuthValidated(String)`, or specify that `accountId` is passed to the background thread closure at spawn time (captured from the synchronous `myself` call in `on_enter()`, accepting the brief TUI block). The synchronous-call-in-`on_enter()` approach is simpler but must be explicitly specified as acceptable.

**P5 — Rate limit retry mechanism is unspecified (GAP F9.1)**

The background thread cannot block on `thread::sleep(retry_secs)` without blocking the shutdown loop. No interruptible sleep pattern is specified. Required fix: specify the chunked-sleep approach (poll `shutdown_flag` every 100ms in a loop until `retry_secs` elapsed), or specify that the 429 error is sent to the TUI which queues a retry via `pending_refresh_at` timer.

**P6 — `is_comment: true` transition fields unhandled (GAP F3.5)**

Transitions requiring a comment use a different POST body shape. `TransitionField.is_comment` exists but has no specified handling. Required fix: specify that when `is_comment == true`, the `TransitionFields` form renders a TextArea field (or triggers `$EDITOR` directly), and that the POST body uses `"update": { "comment": [...] }` instead of `"fields": { ... }`.

**P7 — `JiraResult::Error` context ambiguity (C-GAP-2)**

Multiple simultaneous in-flight commands (detail open: 3 commands) produce indistinguishable `Error` results. Required fix: add command context to errors (e.g., `JiraResult::FetchFailed { command_context: String, error: JiraError }`) or adopt a correlation ID scheme.

### Issues That Are Spec Gaps (Not Implementation-Blocking But Must Be Fixed)

- `pending_refresh_at: Option<Instant>` missing from `JiraPlugin` struct (C3 fix mentioned in text but not in struct)
- `is_stale: bool` (or equivalent) missing from `JiraPlugin` struct
- `optimistic_transitions: HashMap<String, JiraStatus>` (or equivalent) missing from struct for revert on `TransitionFailed`
- `TransitionPicker Esc` → return to IssueDetail path unspecified
- Comment cache invalidation after `CommentAdded` not specified
- FetchFields/FetchMyIssues ordering for custom field discovery not specified
- `LaunchEditor` absent from `jira-plugin.md`'s `PluginAction` definition (still matches REVIEW-v2-spec-completeness issue 1/10)

### Issues Carried Forward from Previous Reviews (Still Unresolved in Spec Docs)

- **C4 (rapid open/close Disconnected)**: Present in REVIEW-v2-concurrency, not fixed in spec
- **theme::selection() compile error**: Present in REVIEW-v2-spec-completeness issue 11, not fixed in spec
- **$EDITOR LaunchEditor handler**: Present in REVIEW-v2-spec-completeness issues 1/10, not fixed in spec
- **form-modal-spec footer rect on border row**: Present in REVIEW-v2-spec-completeness issue 1d.7, not fixed in spec

The core data-flow architecture (mpsc channel topology, generation counter, BoardState scroll, FormState machine, FieldValue type) is sound and well-specified. The critical failures are concentrated in four areas: toast delivery from background results, the missing create-flow command pairs, the detail modal edit state, and rate limit retry. All are fixable without architectural rework. Fix the P0–P7 items above, close the struct field gaps, and resubmit.
