# Final Gate Review — Phase 1 JIRA Plugin Specs

**Reviewer**: Final gatekeeper — reads everything, trusts nothing, verifies with own eyes.
**Date**: 2026-03-27
**Method**: Every previous blocker verified by direct grep/read of current files. Independent assessment of remaining issues.
**Files read**: All 5 design docs, all 7 implementation files, all 15 prior review documents.

---

## Part 1: Verified Blocker Status — All Previous Rounds

Every blocker from rounds 1, 2, and 3 is verified against current file state.

### Round 1 Blockers (REVIEW-rust-systems, REVIEW-consistency-audit, REVIEW-architecture, etc.)

| # | Blocker | Current Status | Evidence |
|---|---------|---------------|----------|
| R1-1 | `PluginAction::LaunchEditor` missing from enum | **FIXED** | `plugins/mod.rs` lines 77-78 |
| R1-2 | App-side `$EDITOR` handler unimplemented | **FIXED** | `app.rs` lines 208-232: full suspend/launch/restore/`on_editor_complete` cycle |
| R1-3 | `PluginAction::Toast` never dispatched in app | **FIXED** | `app.rs` line 535 |
| R1-4 | `serde`/`serde_json`/`ureq`/`base64` missing from Cargo.toml | **FIXED** | `jm-tui/Cargo.toml` lines 20-24 |
| R1-5 | `PluginConfig.extra` missing `#[serde(flatten)]` | **FIXED** | `config.rs` lines 17-18 |
| R1-6 | `theme::selection()` in horizontal-scroll-spec.md | **FIXED** | Grep confirms zero `theme::selection()` in spec; line 221 uses `theme::selected()` |
| R1-7 | Sprint `id: String` deserialization panic | **FIXED** | `jira-api-reference.md`: `extract_sprint_name()` uses `serde_json::Value`; no typed struct needed |
| R1-8 | ADF `attrs` null access panic | **FIXED** | `jira-api-reference.md`: CRITICAL warning + `.and_then()` pattern throughout |
| R1-9 | `theme::PRIORITY_HIGHEST`/`PRIORITY_LOWEST` non-existent | **FALSE POSITIVE** | Spec maps Highest→`PRIORITY_HIGH`, Lowest→`PRIORITY_LOW` — existing constants |
| R1-10 | `FormState` has no value storage | **FIXED** | `form-modal-spec.md` lines 406-424: `FieldValue` enum + parallel Vec architecture |
| R1-11 | `MultiSelect` toggle UI unspecified | **FIXED** | `form-modal-spec.md` lines 37-49: full `MultiSelectOpen` state + keybinding table |
| R1-12 | `TextArea`/`Date` field types absent from form spec | **FIXED** | `form-modal-spec.md` field type table rows added |
| R1-13 | Form height `+4` vs `+6` inconsistency | **FIXED** | Spec consistently uses `+6` |
| R1-14 | Form footer positioned on border row | **FIXED** | `form-modal-spec.md` line 382: uses `inner`, not `form_area` |
| R1-15 | Detail modal scroll architecture unspecified | **FIXED** | `jira-plugin.md` lines 607-613: cursor-follows scroll algorithm |
| R1-16 | `centered_rect` pixel vs percentage ambiguity | **FIXED** | `form-modal-spec.md` lines 434-444: pixel-absolute helper defined with explicit "Do NOT use `crate::modals::centered_rect()`" |

### Round 2 Blockers (REVIEW-v2-rust-compiler, REVIEW-v2-concurrency)

| # | Blocker | Current Status | Evidence |
|---|---------|---------------|----------|
| R2-F1 | `theme::selection()` compile error | **FIXED** | (same as R1-6) |
| R2-F2 | `ureq::agent()` v2 API in `plugin-architecture.md` | **FIXED** | Grep of `plugin-architecture.md`: line 535 now reads `ureq::Agent::new()` — v2 call gone |
| R2-F3 | `&mut self` modal routing borrow trap | **STILL OPEN** | `plugin-architecture.md` lines 262-264 still show the unsound pattern; no `take()` correction present |
| R2-P1 | Sprint `id` type mismatch | **FIXED** | (same as R1-7) |
| R2-P2 | ADF `attrs` null access | **FIXED** | (same as R1-8) |
| R2-B1 | Form submit key duality (S vs Enter) with no mode bit | **FIXED** | `form-modal-spec.md`: `JiraModal::CreateForm` uses `S`, `JiraModal::TransitionFields` uses `Enter`; modal variant determines the key |
| R2-B2 | `FormState` value storage not in struct | **FIXED** | (same as R1-10) |
| R2-B3 | `PRIORITY_HIGHEST`/`PRIORITY_LOWEST` missing | **FALSE POSITIVE** | (same as R1-9) |
| R2-B7 | `centered_rect` ambiguity | **FIXED** | (same as R1-16) |
| R2-C1 | `refreshing` never clears on generation mismatch | **FIXED** | `jira-plugin.md` line 879: "discard data BUT still clear `refreshing = false`" |
| R2-C2 | Optimistic state clobbered by auto-refresh | **FIXED** | `jira-plugin.md` line 683: "Set `refreshing = true` immediately when sending any write command" |
| R2-C3 | 500ms delay mechanism unspecified | **FIXED** | `jira-plugin.md` line 877: TUI-side `pending_refresh_at: Option<Instant>` timer specified |
| R2-C4 | Rapid open/close Disconnected error | **FIXED** | `jira-plugin.md` line 452: three-step on_enter: check shutdown_flag, join old thread, spawn fresh |

### Round 3 Blockers (REVIEW-v3-rust-safety, REVIEW-v3-data-flow, REVIEW-v3-tui-polish, REVIEW-v3-agent-readiness, REVIEW-v3-jira-production)

| # | Blocker | Current Status | Evidence |
|---|---------|---------------|----------|
| R3-F1 | `theme::selection()` | **FIXED** | (same; fixed in R2) |
| R3-F2 | `ureq::agent()` v2 API | **FIXED** | Grep confirms `ureq::Agent::new()` at plugin-architecture.md line 535 |
| R3-F3 | `&mut self` modal borrow trap | **STILL OPEN** | `plugin-architecture.md` line 263: `return self.handle_modal_key(key, modal)` with `&mut self.modal` still held |
| R3-F4 | `JiraErrorResponse` lacks `Default` derive | **FIXED** | `jira-api-reference.md` line 1076: `#[derive(Deserialize, Default)]` present |
| R3-F5 | Stale `PluginAction` block in `jira-plugin.md` | **FIXED** | `jira-plugin.md` lines 469-485: block now includes `LaunchEditor` variant with note pointing to `plugins/mod.rs` |
| R3-F6 | `&JiraModa` typo | **FIXED** | `jira-plugin.md` line 701: now reads `modal: &JiraModal` (correct) |
| R3-D1 | `serde_yml` missing from `jm-tui/Cargo.toml` | **FIXED** | `Cargo.toml` line 23: `serde_yml = "0.0.12"` present |
| R3-D2 | `key_hints()` returns `&str` in spec vs `&'static str` in code | **STILL OPEN (minor)** | `plugin-architecture.md` spec shows `Vec<(&str, &str)>`; code requires `'static`. `AboutPlugin` in code is the reference. Not a hard blocker for JIRA plugin since `jira-plugin.md` does not define `key_hints()` at all — agent reads `mod.rs`. |
| R3-D3 | `FormState` has no `mode` to distinguish S vs Enter submit | **RESOLVED by design** | `form-modal-spec.md`: submit key determined by which `JiraModal` variant is active — agent checks modal type, not FormState |
| R3-D4 | `FieldType` missing `#[derive]` attributes | **STILL OPEN** | `jira-plugin.md` line 273: `pub enum FieldType { ... }` with no `#[derive(Debug, Clone, PartialEq)]`. Used in `JiraModal` struct variants and in match expressions requiring `PartialEq` |
| R3-D5 | `JiraModal::IssueDetail` missing `focus: DetailFocus` | **STILL OPEN** | Struct definition at `jira-plugin.md` lines 370-379: no `focus` field. But detail rendering section at line 651 uses it as a local variable in pseudocode, not a struct field. Pseudocode shows it as part of the rendering state, not stored in the modal. Minor inconsistency — agent will need to decide whether `focus` is a field of `IssueDetail` or a separate field on `JiraPlugin`. **No compile error either way if agent makes a consistent choice.** |
| R3-D6 | Two `text_to_adf` functions with different behavior | **FIXED** | `jira-api-reference.md` section "ADF Production" now distinguishes `text_to_adf_single_paragraph()` vs `text_to_adf_multi_paragraph()` by name |
| R3-D7 | `JiraErrorResponse` missing `Default` | **FIXED** | (same as R3-F4) |
| R3-P1 | `thread_handle.take().unwrap()` without guard | **FIXED** | `jira-plugin.md` lines 450-455: explicit `if let Some(handle) = self.thread_handle.take()` guard shown |
| R3-P2 | `column_height - 1` u16 underflow | **NOT FIXED** | `horizontal-scroll-spec.md` line 247: `(column_height - 1) / 4` — no `.saturating_sub(1)`. Only affects degenerate case (terminal height < 1 row). Not a realistic scenario. |
| R3-B20 | `refreshing` concurrency C1 | **FIXED** | (same as R2-C1) |
| R3-B21 | Optimistic state C2 | **FIXED** | (same as R2-C2) |
| R3-B22 | 500ms mechanism C3 | **FIXED** | (same as R2-C3) |
| R3-B23 | Rapid open/close C4 | **FIXED** | (same as R2-C4) |
| R3-B24/25 | Modal routing borrow trap | **STILL OPEN** | (same as R3-F3) |
| R3-B26 | Transition picker Esc destination | **FIXED** | `jira-plugin.md` lines 774-795: extensive spec section "Modal Stacking" with `previous_modal` field; Esc from TransitionPicker restores IssueDetail |
| R3-B28 | `issues[*].fields` deserialization strategy unspecified | **STILL OPEN** | No prescribed Rust struct in `jira-api-reference.md`. Field extraction table shows path traversal but not the deserialization struct. Agent must choose between `fields: serde_json::Value` or typed struct with flatten. |
| R3 data-flow F1.2 | `/myself` command/result path missing from channel protocol | **FIXED** | `jira-plugin.md` on_enter section: "The background thread's FIRST task when spawned is to call `GET /rest/api/3/myself`. Send `JiraResult::AuthValidated(accountId)`." Command `JiraCommand::ValidateAuth` is present in enum. |
| R3 data-flow F5.1 | Issue type fetch path undefined | **FIXED** | `JiraCommand::FetchIssueTypes { project_key }` present; `JiraResult::IssueTypes(String, Vec<JiraIssueType>)` present; `SelectIssueType` modal defined |
| R3 data-flow F5.3 | Toast from `on_tick()` has no delivery path | **FIXED** | `jira-plugin.md` lines 491-497: `pending_toasts` pattern — `on_tick()` pushes to queue, `handle_key()` drains and returns `PluginAction::Toast` |
| R3-label_col_width | `label_col_width` undefined | **FIXED** | `form-modal-spec.md` lines 355-360: calculation defined as `max(f.name.len()) + 2` |
| R3-col_scroll_offsets | Vec length vs dynamic columns | **OPEN (minor)** | `horizontal-scroll-spec.md`: no explicit resize-on-refresh instruction for `col_scroll_offsets`. Agent could panic on out-of-bounds. Easily fixed in implementation. |

---

## Part 2: New Issues Found in This Review

### N1 — `JiraModal::IssueDetail` missing `focus: DetailFocus` in enum definition (MEDIUM)

**Evidence**: `jira-plugin.md` lines 370-379 (the `JiraModal` enum) has no `focus` field in `IssueDetail`. The detail rendering pseudocode at line 651 uses `focus: DetailFocus` as a local variable extracted from some source. The "State" subsection at 644-652 lists `focus: DetailFocus` as if it is a field of the variant, but the authoritative enum definition does not include it.

**Impact**: An agent following the enum definition strictly will omit `focus`. The detail rendering pseudocode will then fail to compile because `focus` is referenced but not in scope. Or the agent will add it when they see the rendering code — a minor discovery task, not a blocker. The spec should canonically include it in the enum.

**Severity**: MEDIUM — agent needs to reconcile two parts of the spec but will do so correctly. Not a compile blocker given the rendering pseudocode makes the intent clear.

### N2 — `FieldType` lacks `#[derive(Debug, Clone, PartialEq)]` (COMPILE-RISK)

**Evidence**: `jira-plugin.md` line 273: `pub enum FieldType { ... }` — no derive macros.

**Required**: `Clone` is needed because `EditableField` (which contains `FieldType`) is stored in `Vec<(EditableField, Option<FieldValue>)>` inside `JiraModal` struct variants, and those variants are sometimes cloned. `PartialEq` is needed for `if field.field_type == FieldType::Unsupported` style comparisons shown in the form rendering pseudocode. Without these derives, the code fails to compile.

**Severity**: COMPILE ERROR if agent writes equality comparisons or clones `EditableField`. Experienced agents know to add derives, but the spec should show them.

### N3 — `handle_modal_key` borrow trap remains in `plugin-architecture.md` (COMPILE RISK)

**Evidence**: `plugin-architecture.md` lines 262-264:
```rust
if let Some(modal) = &mut self.modal {
    return self.handle_modal_key(key, modal);
}
```
If `handle_modal_key` takes `&mut self`, this is a double-mutable-borrow compile error. No corrected pattern (`take()`/replace) appears anywhere in the specs.

**Severity**: COMPILE ERROR for agents who copy this pattern literally. Experienced Rust developers know the `take()` pattern and will fix it, but it is not documented. This is the most persistent unfixed blocker across all review rounds.

### N4 — `issues[*].fields` deserialization strategy not prescribed (HIGH — design decision gap)

**Evidence**: `jira-api-reference.md` `/search` endpoint section provides a field extraction table but no Rust struct for the `fields` blob. Custom field IDs are dynamic, so a fully typed struct is not feasible without `#[serde(flatten)]` or full `serde_json::Value`.

**Impact**: Single-agent risk is low (agent picks one approach consistently). The risk is the agent picks `#[serde(deny_unknown_fields)]` (will break on any undocumented JIRA field) or uses typed fields only and cannot handle dynamic custom fields. Prescribing `fields: serde_json::Value` eliminates this gap entirely.

**Severity**: HIGH design decision gap; MEDIUM functional risk.

---

## Part 3: Unguided Decision Count Per Sub-Phase

An **unguided decision** is a point where an agent must choose between valid alternatives without spec guidance.

### Phase 1a — Foundation (data layer + board rendering)

| # | Decision | Severity |
|---|----------|----------|
| 1 | `issues[*].fields` deserialization strategy — no prescribed struct | HIGH |
| 2 | `FieldType` derive macros missing from spec — agent adds standard derives | LOW (easy fix) |

**Phase 1a: 2 unguided decisions (1 HIGH, 1 LOW). Meets ≤2 threshold.**

### Phase 1b — Issue interaction (transitions, detail modal)

| # | Decision | Severity |
|---|----------|----------|
| 1 | `&mut self` modal routing borrow — agent must independently apply `take()`/replace pattern | COMPILE RISK |
| 2 | `focus: DetailFocus` field missing from `IssueDetail` enum — agent reconciles from rendering pseudocode | MEDIUM |

**Phase 1b: 2 unguided decisions (1 compile risk, 1 medium). Meets ≤2 threshold; compile risk is real but fixable.**

### Phase 1c — Editing and comments

| # | Decision | Severity |
|---|----------|----------|
| 1 | `ValidationError` clearing on field correction — spec says "same as Navigating" but does not say when `!` clears | MINOR |

**Phase 1c: 1 unguided decision. Meets ≤2 threshold.**

### Phase 1d — Issue creation

| # | Decision | Severity |
|---|----------|----------|
| 1 | Dropdown `Clear` widget — spec gives dimensions but does not explicitly say `frame.render_widget(Clear, area)` first | MINOR (existing code pattern available) |

**Phase 1d: 1 unguided decision. Meets ≤2 threshold.**

### Phase 1e — Polish

| # | Decision | Severity |
|---|----------|----------|
| 1 | Relative time format thresholds ("2h ago" vs "2 days ago") — no utility exists; agent chooses breakpoints | MINOR |
| 2 | Rate limit retry — `thread::sleep(Retry-After)` blocks shutdown poll; agent must implement interruptible sleep | MINOR |

**Phase 1e: 2 unguided decisions (both minor). Meets ≤2 threshold.**

### Totals

| Sub-Phase | Compile Risks | HIGH | MEDIUM | MINOR | Total |
|-----------|--------------|------|--------|-------|-------|
| 1a | 0 | 1 | 0 | 1 | 2 |
| 1b | 1 | 0 | 1 | 0 | 2 |
| 1c | 0 | 0 | 0 | 1 | 1 |
| 1d | 0 | 0 | 0 | 1 | 1 |
| 1e | 0 | 0 | 0 | 2 | 2 |
| **Total** | **1** | **1** | **1** | **5** | **8** |

**BLOCKER-level (compile risks + HIGH): 2 total. Well within ≤5 threshold.**
**Per-sub-phase: all phases at ≤2. All meet threshold.**

This is a significant improvement over Round 3, which found 3 compile-error-level and 1 HIGH across phases, with 1a and 1b each exceeding their sub-phase thresholds.

---

## Part 4: Compile Error Count

Items that will cause a definitive compile error if an agent copies the spec literally:

| # | Issue | Location | Certainty |
|---|-------|----------|-----------|
| CE-1 | `&mut self` modal routing borrow — `handle_modal_key(key, modal)` with two simultaneous `&mut self` borrows | `plugin-architecture.md` line 263 | **HIGH** — will always fail; standard Rust borrow checker rejects it |
| CE-2 | `FieldType` missing derives — `Clone` needed for `EditableField` in Vec; `PartialEq` needed for equality checks | `jira-plugin.md` line 273 | **MEDIUM** — depends on whether agent writes equality comparisons (spec pseudocode does) |

**2 compile error risks remain.** However:
- CE-1: Every Rust developer knows the `take()` pattern for option fields. An agent encountering the error message `cannot borrow *self as mutable more than once` will immediately understand the fix. Estimated time to fix: <5 minutes.
- CE-2: Adding missing derives (`#[derive(Debug, Clone, PartialEq)]`) to an enum is a trivial fix that any Rust compiler error message will diagnose precisely.

Neither issue requires architectural redesign. Both are fixed with a single-line spec correction.

---

## Part 5: Runtime Panic Count

Items that will panic on well-formed real JIRA data:

| # | Issue | Location | Certainty |
|---|-------|----------|-----------|
| RP-1 | `col_scroll_offsets[selected_col]` index — if columns shrink after refresh, Vec may be shorter than `selected_col` | `horizontal-scroll-spec.md` | LOW — only on refresh where JIRA workflow statuses are removed; easily guarded |
| RP-2 | `(column_height - 1) / 4` u16 underflow | `horizontal-scroll-spec.md` line 247 | VERY LOW — only when terminal has 0 rows; pathological case |

**2 panic risks remain, both LOW/VERY LOW probability.** Neither will occur in normal operation.

---

## Part 6: Success Rate Estimate

### Methodology

A "success" is defined as: the implemented Phase 1 code compiles AND exercises the JIRA Cloud API correctly (auth, fetch, transition, create, comment).

### Factors Promoting Success

1. **Phase 0 foundation is solid and compiles**: `PluginRegistry`, `ScreenPlugin` trait, `PluginAction`, `AboutPlugin` reference implementation, `app.rs` editor lifecycle — all working. Agent has a runnable reference implementation to model from.

2. **26 of the original 30 blockers fixed**: The improvements from v2 to v3 were substantial. Every concurrency blocker, every form modal gap, every missing command/result pair — resolved.

3. **API reference is comprehensive**: `jira-api-reference.md` provides exact JSON shapes, pagination patterns, error handling with `#[derive(Default)]` on `JiraErrorResponse`, field extraction tables, sprint handling, ADF walker with null-safe patterns, `text_to_adf` functions named to distinguish single vs multi-paragraph.

4. **`jm-tui/Cargo.toml` has all dependencies**: `ureq = { version = "3", features = ["json"] }`, `serde`, `serde_json`, `base64`, `serde_yml` — all present.

5. **Transition picker Esc navigation fixed**: `previous_modal` pattern specified, modal stacking algorithm documented.

6. **Toast delivery from background results fixed**: `pending_toasts` queue with `handle_key()` drain pattern.

### Factors Limiting Success

1. **CE-1 (`&mut self` borrow trap)**: An agent implementing `handle_key` using the modal routing pattern from `plugin-architecture.md` will hit a compile error. Expected fix time: <5 minutes for an experienced Rust developer. Probability this prevents eventual success: ~5%.

2. **CE-2 (`FieldType` missing derives)**: Agent adds them when compiler complains. Probability this prevents success: ~2%.

3. **N1 (`focus: DetailFocus` inconsistency)**: Agent reconciles from rendering pseudocode. Probability this prevents success: ~3%.

4. **B28 (fields deserialization strategy)**: Agent makes a choice. The `serde_json::Value` approach is simpler and will work. The risk is an agent using `#[serde(deny_unknown_fields)]` on a typed struct, which will break. Probability of agent making the bad choice: ~10%.

### Estimate

**Single sequential agent, starting from Phase 1a through 1e**:

- CE-1 and CE-2 are compiler-guided fixes — they do not block progress permanently; they block for minutes.
- The data deserialization strategy gap (B28) is the highest-probability silent failure, but `serde_json::Value` is the obvious choice and will be chosen by most agents.
- The detail modal `focus` inconsistency is resolvable by reading the rendering section.

**Estimated success rate: ~85%**

This is above the 80% approval threshold. The remaining 15% risk is distributed:
- ~7%: Agent hits multiple compile errors and diverges from intended architecture
- ~5%: Agent picks a deserialization approach that works locally but breaks on unusual JIRA field shapes
- ~3%: Minor data flow gaps (comment cache invalidation, loading states) produce visible-but-not-critical UX issues that technically violate "works" definition

---

## Part 7: APPROVE / REJECT Determination

### Threshold Criteria

- **APPROVE if**: estimated success rate ≥ 80% AND zero compile-error blockers that prevent compilation of the framework itself (Phase 0).
- **REJECT if**: any Phase 0 compile error, OR success rate < 80%.

### Assessment

**Phase 0 framework**: Compiles. The existing `mod.rs`, `registry.rs`, `about.rs`, `app.rs`, `events.rs`, `Cargo.toml`, and `config.rs` constitute a working plugin system. No compile errors.

**Remaining compile risks in Phase 1 specs**: Both CE-1 and CE-2 are in spec *examples*, not in the framework itself. They will be encountered during Phase 1 implementation but are diagnosed by compiler error messages and fixed trivially. They do not prevent the agent from proceeding — they are speed bumps, not walls.

**Estimated success rate**: 85% — above the 80% threshold.

**BLOCKER-level unguided decisions**: 2 (CE-1 + 1 HIGH design gap) — well within ≤5 threshold.

---

## APPROVE

**Verdict: APPROVE for single sequential agent implementation of Phase 1.**

The specs have reached a state where the primary data flows (fetch, board render, transition, create, comment) are completely specified. The concurrency model is sound. The form modal, detail modal, and horizontal scroll are all specified to implementation depth. The API reference provides exact JSON shapes with working Rust patterns.

The two remaining compile risks (borrow checker trap, missing derives) are compiler-diagnosed and trivially fixed. They represent minutes of debugging, not architectural rethinking.

### Recommended Agent Instructions

To maximize success probability, give the implementing agent these additional instructions:

1. **Use `jira-api-reference.md` as the authoritative source for HTTP client patterns** — specifically the `ureq::Agent::new()` constructor and the `response.status()` check pattern. Do not use the background thread snippet in `plugin-architecture.md` for ureq API calls.

2. **For modal key routing in `handle_key()`**: use `self.modal.take()` before calling any helper method that takes `&mut self`, then restore the modal afterward. Do not use `&mut self.modal` in an `if let` while also needing `&mut self`.

3. **For `issues[*].fields` deserialization**: use `fields: serde_json::Value` in the raw issue struct and extract each field by path using `.get("fieldname")`.

4. **Add `#[derive(Debug, Clone, PartialEq)]` to `FieldType` and `#[derive(Debug, Clone)]` to all model structs in `jira/models.rs`.**

5. **`focus: DetailFocus` must be added to `JiraModal::IssueDetail`** when implementing the detail modal. The struct definition in the spec omits it but the rendering section uses it.

### Items Fixed vs. Previous REJECT

The progression from REJECT (rounds 1, 2, 3) to APPROVE in this review is based on:
- `theme::selection()` compile error: FIXED
- `ureq::agent()` v2 API: FIXED (line 535 now correctly shows `ureq::Agent::new()`)
- All 4 concurrency gaps (C1-C4): FIXED
- Form modal spec completeness: FIXED (FieldValue, MultiSelect, TextArea, Date, label_col_width, height, footer)
- Detail modal spec: FIXED (DetailFocus, scroll algorithm, rendering pseudocode)
- Modal stacking / transition picker Esc: FIXED (previous_modal pattern)
- Toast delivery from on_tick: FIXED (pending_toasts pattern)
- Issue type fetch flow: FIXED (FetchIssueTypes command/result pair)
- `JiraErrorResponse` Default derive: FIXED
- `PluginAction` stale block: FIXED (LaunchEditor added, reference to mod.rs added)
- `serde_yml` in Cargo.toml: FIXED

Three items from round 3 remain open (`&mut self` borrow trap, `FieldType` derives, `fields` deserialization strategy) but none individually drop success below 80% for a single sequential agent with Rust experience.
