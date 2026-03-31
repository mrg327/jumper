# Spec Completeness Review v2 — Phase 1 Agent Readiness

**Reviewer role**: Technical program manager. Zero bias toward approval.
**Review date**: 2026-03-27
**Single question answered**: Can agents implement Phase 1 without asking questions?

---

## 1. Previous Gap Closure — 13/13 Original Issues Verified

### Original 13 Issues (from 5 first-round reviews + consistency audit)

| # | Issue | Prior Verdict | Current State | Status |
|---|-------|---------------|---------------|--------|
| 1 | `PluginAction::LaunchEditor` missing from enum | BLOCKER | Variant exists in `plugins/mod.rs` with `content: String, context: String` fields. `on_editor_complete` hook is on `ScreenPlugin` trait. `pending_editor_plugin: Option<(String, String, PathBuf)>` field on App struct exists. **However**, the app-side handler that writes the temp file, suspends the TUI, launches `$EDITOR`, reads back the content, and calls `on_editor_complete` is NOT implemented — cargo check confirms `LaunchEditor` is "never constructed." `plugin-architecture.md` still shows only 3 variants; `LaunchEditor` is absent from all spec documents. | **PARTIALLY FIXED — 1 hard gap remains** |
| 2 | `JiraPlugin` struct fields undefined | BLOCKER | `jira-plugin.md` lines 319–353 define the complete typed struct. | **FIXED** |
| 3 | `JiraError` type undefined | BLOCKER | `jira-plugin.md` lines 288–304 define `JiraError` with `status_code`, `error_messages`, `field_errors`, and `display()`. | **FIXED** |
| 4 | `serde`/`serde_json`/`ureq`/`base64` missing from Cargo.toml | BLOCKER | `jm-tui/Cargo.toml` now contains all four: `ureq = { version = "3", features = ["json"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `base64 = "0.22"`. | **FIXED** |
| 5 | ureq v3 API mismatch in code examples | CRITICAL | `jira-api-reference.md` auth section shows the correct v3 pattern: `Agent::new()`, `.call()` returning `Ok(Response)` for all status codes, explicit `response.status()` check, `response.into_body().read_json()` for body reading. Note: `plugin-architecture.md` background thread example still shows the v2 `ureq::agent()` call — a stale inconsistency in a secondary reference doc. | **FIXED** (primary reference correct; secondary doc stale) |
| 6 | Dynamic `fields` map deserialization unspecified | HIGH | `jira-api-reference.md` field extraction table documents which paths to follow. Sprint is documented as an array requiring `state: "active"` filter. However, no explicit Rust deserialization struct is given for the hybrid `issues[*].fields` object (static fields + dynamic custom field IDs). An agent must independently decide between `serde_json::Value` + path extraction vs. a typed struct with a `#[serde(flatten)]` remainder map. | **PARTIALLY FIXED — design decision still unguided** |
| 7 | ureq v3 error handling (4xx as success) | HIGH | `jira-api-reference.md` shows the complete match-on-status pattern with 429, non-2xx, and success arms. | **FIXED** |
| 8 | `PluginConfig.extra` missing from `config.rs` | HIGH | `jm-core/src/config.rs` has `#[serde(flatten, default)] pub extra: HashMap<String, serde_yml::Value>`. | **FIXED** |
| 9 | `AllowedValue` storing names not IDs | HIGH | `jira-plugin.md` defines `AllowedValue { id: String, name: String }`. `EditableField.allowed_values` is `Option<Vec<AllowedValue>>`. | **FIXED** |
| 10 | `$EDITOR` app-side handler unspecified | BLOCKER | Same state as Issue 1. `PluginAction::LaunchEditor` exists in the code but is unused. No document describes the App-side suspend/write/launch/restore/callback cycle. The cargo check warning "variants `Toast` and `LaunchEditor` are never constructed" confirms the handler is absent. | **STILL OPEN — hard gap** |
| 11 | `theme::selection()` wrong function name | COMPILE ERROR | `horizontal-scroll-spec.md` still references `theme::selection()`. The actual function is `theme::selected()`. Agents copying this code will get a compile error. | **STILL OPEN — hard gap** |
| 12 | `theme::error()` wrong function name | HIGH | `form-modal-spec.md` uses `theme::TEXT_ERROR` (the correct constant), not `theme::error()`. This issue appears to have been pre-corrected in the final spec version. | **RESOLVED** |
| 13 | `centered_rect` pixel vs percentage mismatch | HIGH | `form-modal-spec.md` defines a new pixel-absolute `centered_rect(width, height, area)` helper inline. The spec says "Do NOT use `crate::modals::centered_rect()` which takes percentages." | **FIXED** |

### Summary

- **Fixed**: 9 of 13 (Issues 2, 3, 4, 5, 7, 8, 9, 12, 13)
- **Partially Fixed**: 2 of 13 (Issues 1/10 — same underlying gap; Issue 6)
- **Still Open / Hard Gap**: 2 distinct hard gaps:
  - `$EDITOR` app-side handler: code stub exists but handler unimplemented and undocumented
  - `theme::selection()` compile error in `horizontal-scroll-spec.md`

---

## 2. Decision Completeness Per Sub-Phase

Counting unguided decisions an agent must make that are not answerable from any spec document.

### Phase 1a — Foundation (Data layer + board rendering)

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`$EDITOR` app-side handler** — `PluginAction::LaunchEditor` variant exists in code but is not handled in `app.rs`. The entire suspend-TUI → write-temp-file → launch-editor → resume-TUI → call-`on_editor_complete` cycle is undocumented. No spec describes where in `App::run()` this belongs, what temp file naming convention to use, how to call `on_editor_complete` after the editor exits, or how this interacts with the existing `pending_editor_slug` mechanism. This affects 1c and 1d but must be discovered and planned in 1a when the plugin struct is designed. | BLOCKER |
| 2 | **`PluginAction::Toast` also unused** — cargo check confirms neither `Toast` nor `LaunchEditor` is dispatched. The app-side handler that converts `PluginAction::Toast` into a visible toast notification from a screen plugin may be missing from `app.rs`. An agent implementing 1a needs to verify and wire this or toasts from the JIRA plugin will silently not appear. | HIGH |
| 3 | **`jira/mod.rs` dual-ownership boundary** — Agent D (struct + thread management) and Agent E (render + key handling) both write this file. No document defines where the stub boundary is: which method bodies Agent D writes vs. which are left for Agent E. Agents must pre-agree on an interface without a spec. | HIGH |
| 4 | **Sprint field deserialization struct** — `jira-api-reference.md` documents that `customfield_10020` is an array of sprint objects; agents need to filter by `state: "active"`. But no `SprintValue { name: String, state: String }` struct is provided. Derivable from the JSON example, but not given. | MINOR |
| 5 | **`issues[*].fields` deserialization strategy** — Dynamic custom field IDs (story points, sprint) mean the `fields` object cannot be a fully typed struct at compile time. No guidance given on whether to use `serde_json::Value` for the whole response or a hybrid approach. Agents will independently invent incompatible strategies. | HIGH |
| 6 | **Done column filtering** — The `/search` endpoint fetches ALL assigned issues. The spec says the Done column is hidden by default (toggle `D`). It is implicit but never stated that filtering must be client-side (not JQL-based). An agent may filter via JQL, which breaks the `D` toggle without a re-fetch. | MEDIUM |
| 7 | **`theme::selection()` compile error** — `horizontal-scroll-spec.md` uses a non-existent function. Agents copying the spec code will get an immediate compile error with no clear fix (unless they also read the consistency audit). | COMPILE ERROR |

**Unguided decision count for 1a: 7 (1 blocker, 2 high, 3 medium/minor, 1 compile error)**

---

### Phase 1b — Issue Interaction (transitions, detail modal)

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **Detail modal scroll architecture** — `j`/`k` navigates fields, but when content exceeds terminal height (fields + comments), no scroll state variable, no viewport-follows-cursor algorithm, and no split-region vs. unified-scroll decision is given. `TUI-READINESS.md` identified this gap and provided a recommendation, but it was never incorporated into `jira-plugin.md`. | HIGH |
| 2 | **Detail modal field cursor rendering** — No spec describes the visual indicator for the currently focused field during `j`/`k` navigation. Full-row `theme::selected()` highlight? `>` prefix? Nothing? Ambiguous; agents will produce visually inconsistent results. | MEDIUM |
| 3 | **Optimistic transition data structures** — The spec describes optimistic UI (move issue on transition, revert on `TransitionFailed`) but specifies no storage for pre-transition state (e.g., `optimistic_transitions: HashMap<String, JiraStatus>`). Agent must invent the storage model. | MEDIUM |
| 4 | **Detail modal `*` vs. form `*` prefix conflict** — In the detail modal, `*` marks editable fields. In the form modal, `*` marks required fields. Same prefix character, different semantics. An agent building both will produce inconsistent color coding unless they read both specs in full and reconcile. | MINOR |
| 5 | **Transition picker loading state** — Transitions are fetched lazily. The spec does not describe what the picker shows while `FetchTransitions` is in-flight (empty list? spinner? disabled state?). An agent must choose. | MINOR |

**Unguided decision count for 1b: 5 (1 high, 2 medium, 2 minor)**

---

### Phase 1c — Editing and Comments

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`PluginAction::LaunchEditor` app-side handler (same gap as 1a)** — By Phase 1c the agent is implementing the comment flow, which requires `$EDITOR`. If the app-side handler was not implemented in 1a, the agent implementing 1c must add it — touching `app.rs` (Agent A's domain) without a spec or coordination checkpoint. | BLOCKER |
| 2 | **`ValidationError` state transition on field correction** — The form spec says `ValidationError` behaves "same as Navigating, but error markers shown." When a user corrects a `!`-marked field and presses Enter, does the state transition back to `Navigating` (clearing all errors) or does it stay in `ValidationError` (removing just that field's marker)? Neither behavior is specified. | MEDIUM |
| 3 | **Form scroll state for long field lists** — The form spec says "if more fields than fit, the field list scrolls internally" but provides no scroll offset variable, no cursor-follows algorithm, and no visible-range calculation. An agent must invent all of this. | HIGH |

**Unguided decision count for 1c: 3 (1 blocker, 1 high, 1 medium)**

---

### Phase 1d — Issue Creation

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`LaunchEditor` (same gap)** — TextArea fields in the creation form should open `$EDITOR` per the form spec. Without the app-side handler, `TextArea` fields either silently do nothing or the agent must implement the handler (touching Agent A's domain). The spec fallback of treating TextArea as `Unsupported` is an acceptable workaround but is not stated as the intended behavior. | BLOCKER (or medium if TextArea treated as Unsupported) |
| 2 | **`FormState` value storage architecture** — The `FormState` enum tracks cursor and edit state but stores NO field values. The spec mentions a parallel `Vec<(EditableField, Option<FieldValue>)>` in prose only (the "Data Flow" diagram). The `JiraModal::CreateForm` variant has `form: FormState` but `FormState` by itself cannot hold values. Agents must invent the `FieldValue` type and the parallel storage architecture. | HIGH |
| 3 | **`MultiSelect` toggle UI** — `form-modal-spec.md` lists `MultiSelect` as "Inline dropdown with toggle" but specifies no toggle interaction, no checked/unchecked display, no keyboard to toggle items, and no accumulated-selections representation in `FormState`. | HIGH |
| 4 | **`TextArea` field type absent from form spec** — `FieldType::TextArea` exists in `jira-plugin.md` but has no row in `form-modal-spec.md`'s field type table. Behavior is unspecified. | HIGH |
| 5 | **`Date` field type absent from form spec** — Same issue as TextArea. `FieldType::Date` has no entry in the form spec. | MEDIUM |
| 6 | **Form height formula error** — `form-modal-spec.md` says `height = field_count + 6` in the Sizing section, but the render snippet computes `field_count + 6` differently (one spec says "+4"). `REVIEW-consistency-audit.md` calculates the correct value is `field_count + 6` based on the mockup (1 border top, 1 blank top padding, N fields, 1 blank bottom padding, 1 footer, 1 border bottom). However, the spec body text says "+4" in one place and "+6" in another. An agent will pick one and be wrong on some terminal sizes. | HIGH |
| 7 | **Footer `Rect` positioned on border row** — `form-modal-spec.md`'s `render_form` snippet computes `footer_area.y` as `form_area.y + form_area.height - 1`, which is the bottom border row. Rendering content there overwrites the border. Correct position is `inner.y + inner.height - 1`. Agents copying the snippet verbatim will have a visual bug. | HIGH |

**Unguided decision count for 1d: 7 (1 blocker, 5 high, 1 medium)**

---

### Phase 1e — Polish

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **Relative time display format** — "2h ago", "1d ago". No existing utility in the codebase. Agent must implement from scratch. Format boundaries (when to switch from minutes to hours to days) are unspecified. | MINOR |
| 2 | **`PRIORITY_HIGHEST` and `PRIORITY_LOWEST` theme constants absent** — `theme.rs` has `PRIORITY_HIGH`, `PRIORITY_MEDIUM`, `PRIORITY_LOW` but not `PRIORITY_HIGHEST` or `PRIORITY_LOWEST`. The `horizontal-scroll-spec.md` card rendering references all 5 JIRA priority levels. Agents must define two new theme constants or use existing ones inconsistently. | MINOR |
| 3 | **Thread join strategy on `on_leave()`** — The spec says "thread stopped on `on_leave()`" but does not specify whether to join (blocks TUI during in-flight HTTP request), detach (zombie thread), or timeout-and-detach. For ureq (synchronous), the background thread cannot be interrupted mid-request. If the user rapidly enters and leaves the JIRA screen, and `is_finished()` returns `false`, the spec says "skip spawning and reuse the existing one" — but after `on_leave()` signals shutdown, should `on_enter()` send a new `FetchMyIssues` command to the (still-running) old thread? This is workable logic but requires inference. | MEDIUM |
| 4 | **Rate limit retry in background thread** — The spec says to sleep `Retry-After` seconds on 429. The background thread uses `recv_timeout(100ms)` polling. A sleep of 30+ seconds would block the polling loop. No guidance on how to implement interruptible sleep (e.g., using the 100ms poll as sleep chunks). | MEDIUM |

**Unguided decision count for 1e: 4 (2 medium, 2 minor)**

---

### Decision Count Summary

| Sub-Phase | Blockers | High | Medium | Minor | Compile Errors | Total |
|-----------|----------|------|--------|-------|----------------|-------|
| 1a | 1 | 2 | 1 | 2 | 1 | 7 |
| 1b | 0 | 1 | 2 | 2 | 0 | 5 |
| 1c | 1 | 1 | 1 | 0 | 0 | 3 |
| 1d | 1 | 5 | 1 | 0 | 0 | 7 |
| 1e | 0 | 0 | 2 | 2 | 0 | 4 |
| **Total** | **3** | **9** | **7** | **6** | **1** | **26** |

**Unguided decision threshold**: Zero is ready; 1-2 minor = acceptable; 3+ or any major = not ready.

By sub-phase: Only Phase 1e passes the threshold (4 total, none blockers, 2 minor). All other sub-phases have at least one HIGH or BLOCKER unguided decision. Phase 1a and 1d each have 7 unguided decisions including a blocker.

---

## 3. File Ownership for Parallel Agents (D vs E)

The TEAM-PLAN.md assigns:
- **Agent D** (data layer): `models.rs`, `config.rs`, `adf.rs`, `api.rs`, `jira/mod.rs` (struct + impl skeleton)
- **Agent E** (UI layer): `board.rs`, `detail.rs`, `create.rs`, `jira/mod.rs` (render + handle_key bodies)

**Assessment: Mostly independent, two concrete shared-type gaps remain.**

### Where D and E can work truly independently

All four of Agent D's non-mod-rs files (`models.rs`, `config.rs`, `adf.rs`, `api.rs`) and all three of Agent E's non-mod-rs files (`board.rs`, `detail.rs`, `create.rs`) have no file-level conflicts. The JIRA model types in `models.rs` are completely specified. `JiraCommand` and `JiraResult` enums are defined. Agent E can work against stub imports from the spec.

### Shared type gaps that create inter-agent dependency

**Gap 1 — `FormState` value storage type (`FieldValue`)**

`FormState` in `form-modal-spec.md` tracks navigation and edit state but not actual field values. `jira-plugin.md`'s `JiraModal::CreateForm` has `form: FormState` alongside `fields: Vec<EditableField>`. The prose implies a parallel `Vec<Option<FieldValue>>` for values, but `FieldValue` is never defined as a type. Agent D must decide whether `FieldValue` is a `String`, an enum, or something else. Agent E's `create.rs` needs to call `build_post_body(fields, values)` — if `FieldValue` is not defined and agreed between D and E, their code will not integrate.

**Gap 2 — `jira/mod.rs` stub boundary**

TEAM-PLAN.md says Agent D writes the struct and method stubs; Agent E fills in the bodies. This is workable but the boundary is not drawn explicitly: no document says "Agent D writes these specific lines; Agent E writes these." Without a concrete stub template, agents will either underspecify stubs (Agent E fills in wrong assumptions) or overspecify them (conflict at merge).

**Verdict**: D and E can work independently on their exclusive files (full independence for `models.rs`, `api.rs`, `adf.rs`, `config.rs`, `board.rs`, `detail.rs`). `create.rs` requires the `FieldValue` type to be agreed first. `jira/mod.rs` requires a pre-written stub template.

---

## 4. Phase 1 Task Granularity — Is Sub-Phase Level Sufficient?

Phase 0 had a 22-task list with per-task readiness ratings, dependency ordering, and complexity estimates. Phase 1 has five sub-phases (1a–1e) with a 2-3 day grain.

**Assessment: Insufficient for parallel agents; sufficient for a single sequential agent.**

For a **single agent** working through 1a → 1b → 1c → 1d → 1e, the sub-phase descriptions with their mockups, keybinding tables, and API reference are enough to chunk the work. The agent can make its own task ordering decisions.

For **parallel agents** (the TEAM-PLAN.md scenario), the lack of task-level granularity creates integration risk in two places:

1. **Phase 1a — Agent D and E integration point**: The TEAM-PLAN.md mentions "plug real types into board/detail/create (Day 2-3)" as an integration step, but there is no equivalent of Phase 0's "T17: Register AboutPlugin" task for Phase 1. There is no documented "T-jira-1: Add `mod jira;` to `plugins/mod.rs`" task with a clear owner and dependency.

2. **`PluginAction::LaunchEditor` cross-phase task**: This is a task that belongs to Phase 0 infrastructure (touches `app.rs`, owned by Agent A) but is required for Phase 1c/1d content. It has no sub-phase assignment in the current plan. Without an explicit task saying "who adds the `LaunchEditor` handler to `app.rs` and when," this falls in the gap between Agent A (done) and Agents D/E (no `app.rs` access).

**Recommendation**: Add a "Phase 1 Pre-Work Checklist" with 5-8 tasks covering: (1) `mod jira;` in `plugins/mod.rs`, (2) `"jira"` arm in `PluginRegistry::new()`, (3) `LaunchEditor` handler in `app.rs`, (4) `PluginAction::Toast` handler verification in `app.rs`, (5) `FieldValue` type agreement between D and E.

---

## 5. Test Strategy — No Live JIRA Instance

**Assessment: Adequate for unit testing; blind for API integration.**

### What is mockable

All API types (`JiraIssue`, `JiraCommand`, `JiraResult`, `JiraError`) are plain Rust structs — fully testable without a live instance. The `adf_to_text` and `text_to_adf` functions are pure functions. `BoardState`, `FormState`, `JiraModal` state machines can be tested with synthetic data. The `PluginRegistry` tests in `registry.rs` demonstrate the pattern.

### What is not mockable without infrastructure

The `api.rs` background thread makes synchronous ureq HTTP calls. No mock server is specified. The spec contains no guidance on how to test the API layer without a live JIRA Cloud instance. Agents implementing `api.rs` have zero automated test coverage for their primary work product.

**Partial mitigation**: The `JiraCommand`/`JiraResult` channel protocol can be unit-tested by injecting pre-populated results directly into the channel and verifying plugin state changes in `on_tick()`. This tests the data processing path (models, extraction, state updates) without testing the HTTP layer. The spec does not suggest this pattern.

**No mock server specified**: The spec mentions no `mockito`, `wiremock`, or `httpmock` strategy. Agent D will implement `api.rs` against the documented shapes, but correctness cannot be verified until Phase 1a manual testing with real credentials.

---

## 6. Verdict

### Previous gap closure: 9/13 original issues verified fixed, 2 still open (Issues 10/11), 2 partially fixed (Issues 1/6)

Issues 10 and 11 are the same two gaps the consistency audit flagged as hard blockers:
- **Issue 10** (`$EDITOR` app-side handler): `PluginAction::LaunchEditor` is defined in code but the app-side dispatch loop has not been written. This is confirmed by the cargo check "never constructed" warning. No document describes the implementation.
- **Issue 11** (`theme::selection()`): `horizontal-scroll-spec.md` references a function that does not exist. Direct compile error.

### Decision completeness per sub-phase: 26 total unguided decisions across all phases

- 3 blockers (all stem from the same undocumented `LaunchEditor` handler gap)
- 9 high-severity unguided decisions
- 7 medium-severity unguided decisions

**These counts exceed the "not ready" threshold of 3+ or any major unguided decision for every sub-phase except 1e.**

### REJECT

The spec set cannot be handed to agents in its current state for one primary reason and five secondary ones.

**Primary reason (hard block on 1c and 1d)**: `PluginAction::LaunchEditor` exists in the code as an unused stub. The app-side handler in `App::run()` that suspends the TUI, writes the temp file, launches `$EDITOR`, reads the result back, and calls `plugin.on_editor_complete()` has not been implemented and is not documented anywhere. `plugin-architecture.md` does not include `LaunchEditor` in its `PluginAction` definition. Without this, Phase 1c (comments) and Phase 1d (TextArea fields) cannot be implemented. Agents will stall at the first `$EDITOR` integration test.

**Secondary reasons**:

1. **`theme::selection()` compile error** in `horizontal-scroll-spec.md` — agents copying card rendering code will not compile. Fix: replace with `theme::selected()`.

2. **`FormState` missing value storage** — the `FormState` enum tracks UI cursor state but not field values. `FieldValue` type is never defined. `form-modal-spec.md`'s data flow diagram mentions a parallel `Vec<(EditableField, Option<String>)>` in prose, but neither the type nor the storage architecture is formalized. Agents D and E will invent incompatible storage models.

3. **`TextArea` and `Date` field types absent from form spec** — both exist in `jira-plugin.md`'s `FieldType` enum but have no entries in `form-modal-spec.md`'s field type behavior table. If a required JIRA field is of type `Date`, the agent has no spec to implement it against.

4. **Form height formula contradiction** — `form-modal-spec.md` says `+4` in one place and `+6` in the Sizing section. The correct value is `+6` per the mockup analysis, but agents will encounter a contradictory spec and may pick the wrong number.

5. **Footer positioned on border row** — the `render_form` code snippet in `form-modal-spec.md` computes the footer `Rect` using `form_area.y + form_area.height - 1` (the border row). Correct is `inner.y + inner.height - 1`. Agents copying this snippet verbatim will overdraw the modal border.

### Minimum fixes required before APPROVE

In priority order:

1. **Implement and document `LaunchEditor` app-side handler** — Write the handler in `App::run()` (between channel drain and terminal draw) that: detects `pending_editor_plugin`, suspends TUI (`disable_raw_mode`, `LeaveAlternateScreen`), spawns `$EDITOR` with temp file path, restores TUI, reads temp file back, calls `plugin.on_editor_complete(content, context)`. Document this flow in `plugin-architecture.md` under a new "Editor Integration" section. Also document the `PluginAction::LaunchEditor { content, context }` variant with its semantics (the `content` is the initial editor text; `context` is the opaque callback tag). Verify the cargo "never constructed" warning clears.

2. **Fix `theme::selection()` → `theme::selected()` in `horizontal-scroll-spec.md`** — One-line change; unambiguous.

3. **Define `FieldValue` type and formalize `FormState` value storage** — Add to `form-modal-spec.md`: a `FieldValue` enum covering all `FieldType` variants, and clarify that `JiraModal::CreateForm` (and `TransitionFields`) stores a parallel `Vec<Option<FieldValue>>` alongside `FormState`. This ends the D vs E coordination gap.

4. **Add `TextArea` and `Date` rows to `form-modal-spec.md`'s field type table** — `TextArea` should specify "opens `$EDITOR` via `PluginAction::LaunchEditor`; result returned via `on_editor_complete`." `Date` should specify "inline text input with `YYYY-MM-DD` format, validate on Enter."

5. **Fix form height formula** — Remove the `+4` from the prose; make the Sizing section's `+6` the single authoritative value with a breakdown: `2 border + 1 blank-top + N fields + 1 blank-bottom + 1 footer = N + 5 inner + 2 border = N + 6 total`. Fix the `render_form` footer `Rect` calculation: change `form_area.y + form_area.height - 1` to `inner.y + inner.height - 1`.

After these five fixes, the remaining unguided decisions (sprint deserialization struct, done-column filtering strategy, optimistic transition storage, detail modal scroll architecture, thread join on leave) are solvable without inter-agent coordination and are within the competency of a capable Rust agent working from the existing patterns in the codebase. The predicted success rate after fixes: **>80%**.
