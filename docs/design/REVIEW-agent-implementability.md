# Agent Implementability Review: Phase 1 JIRA Plugin

**Review date**: 2026-03-27
**Reviewer role**: Expert in AI-assisted development and prompt engineering for autonomous coding agents
**Scope**: Phase 1 (sub-phases 1a through 1e) against the full specification set

**Documents reviewed**:
- `plugin-architecture.md`
- `plugin-system-rewrite.md`
- `jira-plugin.md`
- `jira-api-reference.md`
- `form-modal-spec.md`
- `horizontal-scroll-spec.md`
- `TEAM-PLAN.md`
- `PHASE0-READINESS.md`
- `JIRA-API-READINESS.md`
- `TUI-READINESS.md`
- Implemented Phase 0 code (`plugins/mod.rs`, `plugins/registry.rs`, `plugins/about.rs`)

---

## Assessment

### 1. Ambiguity Level (unguided decisions per sub-phase)

---

#### Phase 1a: Foundation

**Unguided decisions an agent must make:**

1. **`$EDITOR` integration** — The design doc says comments use `$EDITOR` but gives no guidance on how a `ScreenPlugin` triggers this. The plugin only has `handle_key()` returning `PluginAction`, yet suspending the TUI requires calling `disable_raw_mode` + `LeaveAlternateScreen` from `App::run()`. The `PluginAction` enum has only three variants (`None`, `Back`, `Toast`). An agent must either (a) add `PluginAction::LaunchEditor` and modify `app.rs`, or (b) somehow work around this — neither option is documented. This affects 1c, 1d, and 1a if the agent starts wiring `JiraPlugin` from the beginning.

2. **`PluginRegistry` registration of `JiraPlugin`** — The registry currently hard-codes `AboutPlugin` only. The agent must add `JiraPlugin` registration with config extraction from `PluginConfig.extra`. The design doc describes the pattern, but the agent must also add `mod jira;` to `plugins/mod.rs`, decide where the `JiraPlugin::new(config)` call goes, and handle the case where `JIRA_API_TOKEN` is missing at registration vs. at `on_enter`.

3. **`PluginConfig.extra` field** — Agent D must add `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `jm-core/src/config.rs`. The spec describes this change and provides the exact code. Low ambiguity here, but the agent must know this is in `jm-core`, a different crate from `jm-tui`.

4. **Sprint field extraction from `/search`** — The sprint custom field (`customfield_10020`) returns an **array** of sprint objects, and the agent must filter for `state: "active"`. The `jira-plugin.md` does not explain this; the `jira-api-reference.md` does document it (item 6 in the JIRA API readiness critical gaps section). The agent must read and cross-reference both documents.

5. **Epic extraction logic** — `JiraIssue.epic: Option<EpicInfo>` is defined but the extraction path is not specified in `jira-plugin.md`. The `jira-api-reference.md` documents that the `parent` field must be checked for `issuetype.name == "Epic"` or `hierarchyLevel == 1`. An agent reading only `jira-plugin.md` will produce broken epic extraction.

6. **`GET /rest/api/3/status` endpoint** — Listed in the endpoints table but has no `JiraCommand` variant and no description of when it is called. An agent may implement it unnecessarily or skip it and rely on statuses derived from the `/search` response. The correct answer (derive statuses from issue data) is not stated.

7. **Multi-line card vs single-line card rendering** — The `horizontal-scroll-spec.md` specifies three-line cards. The `issue_board.rs` existing code uses single-line items. The agent on Phase 1a must choose the right approach and implement multi-line `ListItem` height accounting — a non-trivial ratatui pattern.

8. **Done column filtering** — `/search` fetches ALL assigned issues. The doc says Done is hidden by default (toggle `D`). Whether Done filtering happens in JQL or client-side is not explicit. An agent may filter via JQL (wrong: prevents toggling without a re-fetch) or filter client-side (correct). The spec says "fetches all" but does not state this explicitly as the reason.

**Unguided decisions count for 1a: ~8**
**Rating: CONCERN**

---

#### Phase 1b: Issue Interaction

**Unguided decisions an agent must make:**

1. **Transitions `fields` object structure** — The `jira-plugin.md` mentions "required fields from the transitions API `fields` object." The `jira-api-reference.md` documents that `fields` is a **map keyed by field ID** (not an array), with each value containing `required`, `schema`, `name`, `allowedValues`. Without the reference doc, the agent will produce broken deserialization. With the reference doc, it is specified. **Addressed by `jira-api-reference.md`.**

2. **Optimistic UI state machine** — On transition, move the issue locally, send the command, then revert on `TransitionFailed`. The agent must store the pre-transition status to enable revert. The spec describes the behavior but does not show the state fields needed (e.g., `optimistic_transitions: HashMap<String, JiraStatus>`). The agent must derive the data structure.

3. **Detail modal scroll vs cursor interaction** — `j`/`k` navigates fields and `e` edits. But when there are more fields + comments than fit on screen, does `j` past the bottom scroll the viewport, or does a separate scroll key exist? The `TUI-READINESS.md` identifies this as unspecified. The `jira-plugin.md` does not resolve it. **Not addressed by new docs.**

4. **Field navigation cursor in detail modal** — What is the visual indicator for the focused field? The spec shows `[e:edit]` hints but does not specify the focused-row highlight style. The `TUI-READINESS.md` raises this gap. **Not addressed.**

5. **Stale detail modal on refresh** — When a refresh arrives and the modal is open, the spec says update in place if the issue still exists. The agent must locate the issue in the new data set by key and update the detail state. Straightforward but requires explicit state management (`current_detail_issue_key: Option<String>`).

6. **`allowedValues` ID vs name storage** — `EditableField.allowed_values: Option<Vec<String>>` stores names (for display) but the transition POST body needs IDs. An agent will hit a runtime failure when constructing the transition POST body. The `JIRA-API-READINESS.md` documents this gap but the main `jira-plugin.md` and `jira-api-reference.md` do not resolve it with a concrete struct change. **Partially addressed: identified in readiness doc, not fixed in spec.**

**Unguided decisions count for 1b: ~6**
**Rating: CONCERN**

---

#### Phase 1c: Editing and Comments

**Unguided decisions an agent must make:**

1. **`PluginAction::LaunchEditor`** — Same fundamental problem as noted in 1a. The comment flow requires `$EDITOR`. Without adding `LaunchEditor` to `PluginAction` and modifying `app.rs`, comments cannot be implemented. This is a design gap that requires touching Agent A's domain (`app.rs`, `events.rs`). The `TUI-READINESS.md` documents this clearly and provides the solution path (`LaunchEditor { path, callback_id }`), but the `plugin-architecture.md` and `jira-plugin.md` do not include this variant. **Identified in readiness doc, not fixed in spec.**

2. **Field editing inline vs popup** — The `form-modal-spec.md` specifies inline text editing with a cursor-in-place approach. The `TUI-READINESS.md` previously flagged this as needing spec. The `form-modal-spec.md` does address this with a complete state machine (`FormState` enum, `EditingText`, `SelectOpen`). **Addressed by `form-modal-spec.md`.**

3. **`editMeta` response structure** — The `jira-api-reference.md` provides the full response shape (a map keyed by field ID). The agent has what it needs to parse editmeta. **Addressed.**

4. **Comment `orderBy` parameter** — The `jira-api-reference.md` documents `orderBy: "-created"` for newest-first ordering. Without this, comments appear in chronological order, which the mockup shows as reverse-chronological. **Addressed by reference doc.**

5. **ADF multi-paragraph conversion for writes** — The `jira-plugin.md` provides `text_to_adf()` but wraps everything in one paragraph, making multi-line editor input appear as a single block in JIRA. The `jira-api-reference.md` suggests splitting on `\n\n`. Minor UX issue, not a correctness blocker.

6. **`notifyUsers=false` query param on PUT** — The `jira-api-reference.md` mentions this optional param. Including it is good practice (suppresses JIRA notification emails on field edits), but the spec does not require it. Low-impact unguided decision.

**Unguided decisions count for 1c: ~4 (down from previous assessment)**
**Rating: CONCERN** (driven primarily by the `LaunchEditor` gap)

---

#### Phase 1d: Issue Creation

**Unguided decisions an agent must make:**

1. **`LaunchEditor` still unresolved** — If Phase 1c did not add `LaunchEditor` to `PluginAction`, TextArea fields in the creation form cannot open `$EDITOR`. The creation form can still work if TextArea fields are shown as `FieldType::Unsupported`, but this limits functionality.

2. **Createmeta `values` wrapper** — The `jira-api-reference.md` documents that the createmeta response is wrapped in `"values"`, not `"issueTypes"`. This is a v3 API change that will cause silent deserialization failure (empty list) if the agent uses the wrong key. **Addressed by reference doc.**

3. **Filtering `project`, `issuetype`, `reporter` from user-facing fields** — The `jira-api-reference.md` explicitly calls this out. Without it, the form will show these fields to the user and they will be confused. **Addressed by reference doc.**

4. **Project list derivation** — The spec says derive the project list from the distinct `project_key` values of already-loaded issues. This is mentioned in `jira-api-reference.md` but not in `jira-plugin.md`. An agent reading only the plugin spec may attempt an API call to enumerate projects. **Addressed in reference doc, not in main spec.**

5. **`form-modal-spec.md` form state machine** — The `form-modal-spec.md` provides a complete `FormState` enum, field row layout, dropdown positioning, submission flow, and validation error display. This is well-specified. **Addressed.**

6. **`$EDITOR` for TextArea fields in form** — Same issue as 1c. The form modal spec says TextArea opens `$EDITOR` but there is no mechanism for a screen plugin to do this.

**Unguided decisions count for 1d: ~4**
**Rating: CONCERN** (creation form itself is well-specified; gaps are infrastructure-level)

---

#### Phase 1e: Polish

**Unguided decisions an agent must make:**

1. **Relative time display** — "2h ago", "1d ago". No existing utility in the codebase. Agent must implement from scratch using `chrono`. Low complexity but unguided.

2. **Spinner cycle characters** — The `horizontal-scroll-spec.md` specifies the exact braille spinner sequence: `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`. **Addressed.**

3. **Thread panic detection** — `TryRecvError::Disconnected` indicates a panicked background thread. The spec says show a reconnect prompt but does not specify what "reconnect prompt" looks like or how the user dismisses it. This is probably just an error modal, but it is not shown in the UI mockups.

4. **Rate limit retry** — The spec says respect `Retry-After` headers on 429. With ureq (sync HTTP), the background thread would need to sleep. The thread uses `recv_timeout(100ms)` in its loop, so it can check the shutdown flag during the wait. Implementation pattern is not shown.

5. **Column width balancing edge case** — Columns with very long status names (JIRA allows arbitrary status names). The `horizontal-scroll-spec.md` specifies the algorithm but doesn't cap status name display in column headers. Minor truncation decision.

**Unguided decisions count for 1e: ~5**
**Rating: PASS** (these are minor polish decisions, not architectural)

---

### 2. Agent Task Sizing

**Assessment: PASS with one CONCERN**

Sub-phases 1a through 1d are well-sized for autonomous agents. Each represents 1-3 days of focused work on a coherent capability slice:

- **1a** (board + API foundation): ~2-3 days. Appropriately sized — an agent can implement the full data pipeline and rendering without needing the detail/create features.
- **1b** (transitions + detail modal): ~2-3 days. Bounded by the transition picker + detail modal scope.
- **1c** (editing + comments): ~2-3 days. Comment flow is well-bounded. Field editing reuses the form spec.
- **1d** (issue creation): ~2-3 days. The form modal spec is complete enough.
- **1e** (polish): ~1-2 days. Intentionally small.

**Concern**: The `$EDITOR` / `PluginAction::LaunchEditor` issue cuts across 1c and 1d. If an agent implementing 1c discovers the gap and adds `LaunchEditor` to `PluginAction`, it must also modify `app.rs` — which is Agent A's domain. This creates a cross-phase, cross-agent coordination point that is not documented in the `TEAM-PLAN.md` or anywhere in the specs.

**Concern**: Phase 1a is the largest sub-phase and includes both the full API data layer (models, auth, pagination, background thread) AND the kanban board rendering (horizontal scroll, multi-line cards, loading state). Depending on how JIRA API deserialization goes, 1a could exceed 3 days. Consider explicitly splitting 1a into "data layer" and "board rendering" parallel tracks as the TEAM-PLAN.md suggests for Agents D and E.

---

### 3. File Ownership (Parallelizability)

**Assessment: PASS with acknowledged conflicts**

The TEAM-PLAN.md's file ownership model is sound. Most files have clear single owners:

| Agent | Files | Conflict Risk |
|-------|-------|---------------|
| A | `app.rs`, `events.rs`, `keyhints.rs`, `plugins/mod.rs`, `registry.rs` | **Low** — owns all wiring files |
| B | `clock.rs`, `notifications.rs`, `pomodoro.rs`, `sidebar.rs`, `about.rs` | **None** — fully isolated |
| C | `jm-core/models/project.rs`, proptest files | **None** — different crate |
| D | `jira/config.rs`, `jira/models.rs`, `jira/adf.rs`, `jira/api.rs`, `Cargo.toml`, `jm-core/config.rs` | **Low** — new files except config.rs |
| E | `jira/board.rs`, `jira/detail.rs`, `jira/create.rs`, `jira/mod.rs` | **Low** — new files |

**Shared file conflict: `jira/mod.rs`** — Both Agent D and Agent E need this file. The TEAM-PLAN.md acknowledges this and recommends Agent D write the struct definition + method stubs and Agent E fill the render and handle_key bodies. This is workable if the stub boundaries are agreed upon before parallel work starts. The risk is Agent E trying to compile `jira/mod.rs` before Agent D has finished the struct definition.

**Shared file conflict: `plugins/mod.rs`** — Agent A owns this. Agent D needs `mod jira;` added. The TEAM-PLAN.md calls this out as a one-line merge. Low risk.

**Hidden conflict: `app.rs` for `LaunchEditor`** — If Agent E (implementing comments in 1c) discovers the `PluginAction::LaunchEditor` gap and needs to add a new variant plus handle it in `app.rs`, this touches Agent A's domain. Since Agent A will have long since finished their work when Phase 1c executes, this creates an unplanned return to a "done" file. This is the main parallelism risk not addressed by the current plan.

**Overall: parallelism is feasible as designed for Phase 0 and Phase 1a. The `LaunchEditor` gap threatens Phase 1c/1d parallelism if not resolved in the specs before agents start.**

---

### 4. Reference Material Completeness

**Assessment: PASS for most; FAIL for one critical gap**

The readiness assessments identified specific gaps. Here is whether each was addressed:

| Gap Identified | New Doc That Addresses It | Status |
|----------------|--------------------------|--------|
| Horizontal scroll algorithm | `horizontal-scroll-spec.md` | **RESOLVED** — algorithm, state struct, visual examples, layout constraints all specified |
| Form modal UX (inline edit, select popup, state machine) | `form-modal-spec.md` | **RESOLVED** — `FormState` enum, per-field-type behavior, sizing, positioning |
| Multi-line issue card format | `horizontal-scroll-spec.md` section "Issue Card Format" | **RESOLVED** — three-line card layout, line contents, priority coloring |
| `/search` response shape | `jira-api-reference.md` | **RESOLVED** — full response JSON with annotations |
| `/transitions` response + POST body | `jira-api-reference.md` | **RESOLVED** — map structure, `to` field, POST body wrapper |
| `/editmeta` response shape | `jira-api-reference.md` | **RESOLVED** — map structure, schema type mapping |
| Createmeta `values` wrapper | `jira-api-reference.md` | **RESOLVED** |
| POST body shapes for all write endpoints | `jira-api-reference.md` | **RESOLVED** |
| ureq v3 auth header pattern | `jira-api-reference.md` | **RESOLVED** — concrete Rust code provided |
| Sprint field extraction (array, `state: "active"`) | `jira-api-reference.md` | **RESOLVED** |
| `$EDITOR` requires `PluginAction::LaunchEditor` | `TUI-READINESS.md` identifies it, provides solution | **NOT RESOLVED in specs** — identified but not incorporated into `plugin-architecture.md` or `PluginAction` definition |
| `allowedValues` ID vs name in `EditableField` | `JIRA-API-READINESS.md` documents gap | **NOT RESOLVED** — struct still shows `Option<Vec<String>>` without a companion ID list |
| Detail modal scroll + field navigation mechanics | `TUI-READINESS.md` identifies it | **NOT RESOLVED** — no spec update |
| Epic extraction via `parent` field | `jira-api-reference.md` | **RESOLVED** |

**Summary**: 11 of 14 identified gaps are resolved. Three remain open, two of which are implementation-blockers for Phase 1c/1d.

---

### 5. Implicit Knowledge

**Assessment: CONCERN**

The following knowledge is required but appears in no document:

**ratatui specifics:**

- `List` with multi-line `ListItem`s: ratatui calculates item height from the number of `Line`s in the `Text`. An agent that uses single-line `ListItem`s will render only the first line of three-line cards. The `horizontal-scroll-spec.md` specifies the card format but does not state "use multi-line `ListItem`" explicitly nor reference the ratatui API for this. The existing `issue_board.rs` uses single-line items, so copy-paste will produce the wrong output.

- `frame.render_widget(Clear, area)` before rendering a modal overlay: both `TUI-READINESS.md` and the existing modal code demonstrate this, but it is not in `plugin-architecture.md`. An agent who skips `Clear` will have transparent modals (showing the board underneath). The `form-modal-spec.md` does include this in its rendering pseudocode.

- `Layout::horizontal(constraints).split(area)` vs `.areas(area)`: The codebase uses both. `.areas()` is the newer ratatui API that returns a fixed-size array. `.split()` returns a `Rc<[Rect]>`. The `horizontal-scroll-spec.md` uses `.split()` for the variable-length column case (correct) and `.areas()` elsewhere. An agent may be confused about which to use.

**Rust/async patterns:**

- `mpsc::channel` vs `mpsc::sync_channel`: The spec says to use `mpsc::channel` (unbounded). For a real JIRA plugin, an unbounded channel is fine since the background thread sends slowly (network-bound). But an agent familiar with Tokio may try to use async channels. The spec explicitly says `ureq` (synchronous) and thread-based, which is clear.

- `AtomicBool` with `Ordering::Relaxed` vs `Ordering::SeqCst` for the shutdown flag: The spec uses `Relaxed` in its example. For a simple "check if we should stop" flag, `Relaxed` is correct and the spec example is correct.

- `JoinHandle::is_finished()` for the thread spawn guard: This is a stable API since Rust 1.61. The spec cites it correctly.

**The most dangerous implicit knowledge gap**: An agent building `api.rs` with ureq v3 may encounter ureq's error type changes from v2. In ureq v3, HTTP errors (4xx, 5xx) are NOT returned as `Err(...)` by default — the `call()` returns `Ok(Response)` for all HTTP responses, and the agent must check `response.status()`. The `jira-api-reference.md` does not mention this v3 behavior change. An agent testing with a 401 response will think the request succeeded.

---

### 6. Failure Modes

**Assessment: CONCERN**

**Most likely failure modes and their safeguards:**

| Failure Mode | Probability | Safeguard |
|-------------|-------------|-----------|
| `ureq v3` error handling — treating 4xx/5xx as success | **HIGH** | No spec coverage. Agent will ship an auth failure silently appearing as "no issues assigned." |
| Sprint field as array (not single object) | **HIGH** | `jira-api-reference.md` documents this. Safeguard: present. |
| `allowedValues` storing names not IDs — transition POST body uses name strings instead of `{ "id" }` | **HIGH** | Gap not resolved in specs. Will cause 400 errors on transitions. |
| Multi-line `ListItem` for cards — agent uses single-line, only first line renders | **HIGH** | `horizontal-scroll-spec.md` specifies 3-line format but does not call out ratatui API. |
| `$EDITOR` comment flow — no mechanism to launch editor from ScreenPlugin | **HIGH** | `TUI-READINESS.md` identifies and provides solution. Not incorporated in specs. Phase 1c will stall. |
| Detail modal field navigation scroll — undefined behavior | **MEDIUM** | `TUI-READINESS.md` provides a recommendation. Not in specs. Agent will produce inconsistent UX. |
| Epic extraction — agent invents an approach | **MEDIUM** | `jira-api-reference.md` documents the `parent` field approach. Safeguard: present. |
| Borrow checker battles in `app.rs` (clone-first pattern) | **MEDIUM** | Well-documented in `plugin-system-rewrite.md` with exact code patterns. Strong safeguard. |
| `jira/mod.rs` dual-ownership merge conflict (Agents D+E) | **MEDIUM** | Acknowledged in TEAM-PLAN.md; mitigation is method stubs. Workable. |

**Compiler as safeguard:** The Rust compiler catches trait method signature mismatches, exhaustive match failures on `ScreenId::Plugin`, and type mismatches between `JiraCommand`/`JiraResult` and their uses. This provides a strong compile-time gate for integration contracts. However, the compiler does NOT catch:
- Wrong JSON field extraction paths (runtime deserialization failures)
- Wrong HTTP status codes (ureq v3 behavior)
- Wrong field value formats in API calls (400 errors at runtime)

**Test suite as safeguard:** There are no tests for the JIRA plugin planned against a mock or real JIRA API. The only test gate is `cargo test` (which runs core unit tests and proptest). This provides zero coverage for API integration correctness. An agent can produce code that compiles and passes all tests but fails against a real JIRA instance.

---

### 7. Integration Testing

**Assessment: FAIL**

The TEAM-PLAN.md defines manual testing checkpoints at each gate (Phase 0 gate, 1a gate, 1b gate, etc.). These require a human with a real JIRA instance. There is **no automated integration test plan**.

Specific gaps:

1. **No mock JIRA server**: The specs do not describe a mock server for API testing. An agent cannot run integration tests without credentials.

2. **No test fixtures**: The `jira-api-reference.md` provides example JSON responses but does not package them as test fixture files that the agent could use in unit tests for the deserialization layer (`api.rs`, `models.rs`).

3. **No unit tests for API deserialization**: The plan calls for agents to write "unit tests for `PluginRegistry` and `AboutPlugin`" (Phase 0, Task 19) but says nothing about unit tests for `JiraIssue` deserialization, ADF conversion, `EditableField` mapping, or the command/result channel protocol. These are the highest-risk code paths.

4. **No test for the background thread protocol**: The `JiraCommand`/`JiraResult` channel protocol cannot be tested without a real thread. The spec provides no guidance on how to test this.

5. **Phase 1 gate tests are all manual**: TEAM-PLAN.md sections 2-5 ("Phase 1a Gate", "Phase 1b Gate", etc.) describe manual TUI interaction sequences requiring a human, a keyboard, and live JIRA credentials. A CI pipeline cannot reproduce these.

**What a minimal test plan would require:**
- JSON test fixtures for each API response (can be derived from the `jira-api-reference.md` examples)
- Unit tests in `jira/models.rs` validating deserialization of each fixture
- Unit tests in `jira/adf.rs` for ADF-to-plaintext conversion
- Unit tests for `JiraConfig` validation (missing env vars, missing config fields)
- A `cfg(test)` mock channel in `api.rs` that sends pre-loaded fixture responses

None of this is planned, documented, or described anywhere in the spec set.

---

## Gaps Found

### Blocking Gaps (will cause Phase 1 implementation failure)

**GAP-1: `PluginAction::LaunchEditor` is not in the spec**

The `$EDITOR` flow for comments (Phase 1c) and TextArea fields (Phase 1d) requires the plugin to suspend the TUI. `ScreenPlugin` can only return `PluginAction` values. The current `PluginAction` enum (`None`, `Back`, `Toast`) has no mechanism for this. The `TUI-READINESS.md` identifies this gap and provides the exact solution:

```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { path: PathBuf, callback_id: String }, // MISSING
}
```

The `app.rs` event loop must handle this variant the same way it handles `pending_editor_slug`. After the editor exits, `app.rs` must deliver the result back to the plugin (via `on_notify` or a new `on_editor_result` method). Without this, Phase 1c and 1d cannot be completed.

**Required fix**: Add `LaunchEditor` to `PluginAction` in `plugin-architecture.md`, define the callback delivery mechanism, and update `TEAM-PLAN.md` to assign the `app.rs` change to a specific agent/phase.

---

**GAP-2: `EditableField.allowed_values` stores names but writes need IDs**

The `EditableField` struct has `allowed_values: Option<Vec<String>>`. This is used for display (show option names in the form). But when building the transition POST body or field update PUT body, the API requires `{ "id": "..." }`, not a bare string name.

For example, a transition to "Done" requires `"resolution": { "id": "1" }`, not `"resolution": "Done"`. An agent following the current struct definition will send `"resolution": "Done"` and receive a 400 error.

**Required fix**: Change `allowed_values` to store both id and name:
```rust
pub allowed_values: Option<Vec<AllowedValue>>,
// where:
pub struct AllowedValue { pub id: String, pub name: String }
```
Update all references in `jira-plugin.md`, `form-modal-spec.md`, and `jira-api-reference.md`.

---

**GAP-3: ureq v3 error handling behavior is undocumented**

In ureq v3, HTTP error responses (401, 403, 404, 400, 429, 500) are returned as `Ok(Response)` by the `.call()` method, not as `Err(...)`. The agent must check `response.status()` and handle non-2xx responses explicitly. The `jira-api-reference.md` shows `agent.get(&url)...call()?` with the `?` operator, which will only propagate network-level errors (DNS, connection refused), not HTTP-level errors.

An agent following the example code will treat 401 Unauthorized as a successful response with an empty body, producing silent failures (no issues returned, no error shown).

**Required fix**: Add to `jira-api-reference.md` a code snippet showing correct ureq v3 error handling:
```rust
let response = agent.get(&url)...call()?;
if response.status() != 200 {
    return Err(JiraError::HttpError { status: response.status(), body: response.into_string()? });
}
```

---

### Significant Gaps (will cause Phase 1 quality failures but not compile failures)

**GAP-4: Detail modal scroll mechanics unspecified**

`jira-plugin.md` says "Navigate fields with j/k" and "Comments section is scrollable" but does not specify:
- Single scroll offset for the whole modal vs. separate field/comment regions
- Whether `j` past the last visible field auto-scrolls the viewport
- The visual indicator for the focused field

The `TUI-READINESS.md` provides a recommendation (single `scroll_offset`, `help.rs` pattern) but this is in a readiness assessment, not a design spec. An agent may not find it.

**Required fix**: Add a "Detail Modal Layout" section to `jira-plugin.md` with: layout rect dimensions, scroll behavior, field cursor visual, and an explicit reference to `help.rs:78-120` as the pattern to copy.

---

**GAP-5: Multi-line ratatui `ListItem` pattern not called out**

The `horizontal-scroll-spec.md` specifies three-line issue cards but does not say "use multi-line `ListItem`" or provide the ratatui API for doing so. The existing `issue_board.rs` uses single-line items. An agent copying the existing code will render only the first line of each card.

**Required fix**: Add to `horizontal-scroll-spec.md`:
```rust
// Each card is a multi-line ListItem with fixed height of 3 content lines + 1 blank = 4 rows
let card = ListItem::new(vec![
    Line::from(vec![key_span, type_span]),  // Line 1
    Line::from(summary_span),               // Line 2
    Line::from(vec![priority_span, pts_span]), // Line 3
]);
```
Or explicitly reference `ratatui::widgets::ListItem::new(Text)` with multi-line support.

---

**GAP-6: `GET /rest/api/3/status` endpoint — called or not?**

Listed in the endpoints table in `jira-plugin.md` but has no corresponding `JiraCommand` variant. An agent will be confused about whether to implement it. The `jira-api-reference.md` calls this out but does not resolve it.

**Required fix**: Either add `JiraCommand::FetchStatuses` with a clear use case, or explicitly remove the endpoint from the table and add a note: "Statuses are derived from issue data; this endpoint is not used."

---

## Final Verdict

### REJECT

**Confidence in >80% success rate**: No.

With the three blocking gaps in their current state:
- **GAP-1** (`LaunchEditor`) means Phase 1c and 1d cannot be completed without an agent making an undocumented architectural change to `PluginAction` and `app.rs`. The probability an agent gets this right on first try is around 50% — the solution is documented in `TUI-READINESS.md` but requires modifying a file in a different agent's domain.
- **GAP-2** (`allowedValues` IDs) means transitions will return 400 errors on any select-type required field. With the most common transition (Done → requires Resolution) failing, the core value proposition of Phase 1b is broken. Probability of agent getting this right unprompted: ~30%.
- **GAP-3** (ureq v3 error handling) means auth failures and all HTTP errors are silently swallowed. An agent testing against a real JIRA instance will see empty boards with no error, spend significant effort debugging, and likely conclude the API client or config is wrong. This will consume 1-2 days of an agent's effort on a problem that a 5-line code snippet would prevent.

**Estimated current success rate**: 55-65% for Phase 1a (board renders, data loads, navigation works), dropping to 30-40% for Phase 1b-1d (transitions work, editing works, creation works). The overall Phase 1 success rate (compiles, works against a real JIRA instance, correct UI) is below the 80% threshold.

### What needs to happen before APPROVE

1. **Fix GAP-1**: Add `PluginAction::LaunchEditor` to `plugin-architecture.md` with the delivery mechanism. Assign the `app.rs` change to Agent A or an explicit Phase 1c infrastructure task.

2. **Fix GAP-2**: Update `EditableField.allowed_values` to `Option<Vec<AllowedValue>>` with a struct that holds both `id` and `name`. Update `form-modal-spec.md` and `jira-api-reference.md` to be consistent.

3. **Fix GAP-3**: Add a ureq v3 error handling code example to `jira-api-reference.md` showing how to check `response.status()` for non-2xx responses.

4. **Fix GAP-4**: Add detail modal scroll mechanics to `jira-plugin.md` (or a new `detail-modal-spec.md` parallel to `form-modal-spec.md`).

5. **Fix GAP-5**: Add ratatui multi-line `ListItem` example to `horizontal-scroll-spec.md`.

6. **Optionally fix GAP-6**: Clarify whether `GET /rest/api/3/status` is needed.

7. **Add a minimal integration test plan**: Even without a mock server, add unit tests for `JiraIssue` deserialization using the JSON fixtures from `jira-api-reference.md`. These tests can be written by an agent and run in CI without live credentials.

If items 1-3 are fixed (the true blockers), the estimated success rate rises to 75-80%. Items 4-5 push it above 80%. Items 6-7 are recommended but not blocking approval.

---

### Summary Table

| Area | Rating | Notes |
|------|--------|-------|
| Ambiguity level (1a) | CONCERN | ~8 unguided decisions, most mitigated by reference docs |
| Ambiguity level (1b) | CONCERN | `allowedValues` ID gap blocks transitions |
| Ambiguity level (1c) | CONCERN | `LaunchEditor` gap blocks comments |
| Ambiguity level (1d) | CONCERN | `LaunchEditor` gap; form modal well-specified otherwise |
| Ambiguity level (1e) | PASS | Minor polish decisions |
| Agent task sizing | PASS | Sub-phases are appropriately sized |
| File ownership / parallelism | PASS | Clean ownership; `LaunchEditor` creates one cross-agent touch point |
| Reference material completeness | CONCERN | 11/14 gaps resolved; 3 remain open, 2 are blockers |
| Implicit knowledge | CONCERN | ureq v3 error behavior is the critical missing item |
| Failure modes | CONCERN | High-probability failures in ureq error handling and allowedValues IDs |
| Integration testing | FAIL | No automated tests planned; all validation requires live JIRA instance |
