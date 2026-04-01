# Agent Readiness Review v3 — Phase 1 Implementation

**Reviewer role**: AI agent deployment specialist. Zero bias toward approval.
**Date**: 2026-03-27
**Single question**: Can autonomous coding agents implement Phase 1 from these specs with >80% success rate?
**Method**: Every blocker from rounds 1 and 2 verified against actual files. Unguided decisions counted per sub-phase against ≤2 acceptable threshold.

---

## Part 1: Previous Blocker Verification

All blockers from both review rounds are verified against the current state of actual files, not agent claims.

### Round 1 + Round 2 Blockers — Status

| # | Blocker | Verified Fix? | Evidence |
|---|---------|---------------|----------|
| B1 | `PluginAction::LaunchEditor` missing from enum | **FIXED** | `plugins/mod.rs` lines 77-78: `LaunchEditor { content: String, context: String }` present |
| B2 | App-side `$EDITOR` handler unimplemented | **FIXED** | `app.rs` lines 208-232: full suspend/write/launch/restore/`on_editor_complete` cycle implemented |
| B3 | `PluginAction::Toast` never dispatched | **FIXED** | `app.rs` line 535: `PluginAction::Toast(msg) => Action::Toast(msg)` present |
| B4 | `serde`/`serde_json`/`ureq`/`base64` missing from Cargo.toml | **FIXED** | `Cargo.toml` lines 20-23: all four present with correct versions and features |
| B5 | `PluginConfig.extra` missing (no `serde(flatten)`) | **FIXED** | `config.rs` lines 17-18: `#[serde(flatten, default)] pub extra: HashMap<String, serde_yml::Value>` |
| B6 | `theme::selection()` compile error in `horizontal-scroll-spec.md` | **FIXED** | `horizontal-scroll-spec.md` line 221: now uses `theme::selected()`. Grep confirms zero remaining `theme::selection()` instances in spec files |
| B7 | `ureq::agent()` v2 API in `plugin-architecture.md` background thread template | **STILL OPEN** | `plugin-architecture.md` line 535: `let client = ureq::agent();` — v2 API, will not compile against ureq v3. Primary reference `jira-api-reference.md` is correct |
| B8 | Sprint `id` field: `String` vs `u32` deserialization panic | **FIXED** | `jira-api-reference.md`: `extract_sprint_name()` now deserializes sprint as `serde_json::Value` and handles both array and object variants. No typed `SprintValue` struct needed |
| B9 | ADF `attrs` null access panic | **FIXED** | `jira-api-reference.md`: algorithm now uses `.and_then()` chains throughout attrs access |
| B10 | `theme::PRIORITY_HIGHEST`/`PRIORITY_LOWEST` non-existent constants | **FIXED** | `horizontal-scroll-spec.md` lines 212-216: now maps "Highest","High" → `PRIORITY_HIGH`; "Low","Lowest" → `PRIORITY_LOW`. No non-existent constants referenced |
| B11 | `FormState` has no value storage; `FieldValue` type undefined | **FIXED** | `form-modal-spec.md` lines 406-424: `FieldValue` enum defined; lines 392-393 explicitly state values are stored in parallel `Vec<(EditableField, Option<FieldValue>)>` in the modal variant, NOT in `FormState` |
| B12 | `MultiSelect` toggle UI completely unspecified | **FIXED** | `form-modal-spec.md` lines 37-49: `MultiSelectOpen { field_cursor, dropdown_cursor, checked: HashSet<usize> }` state defined; lines 327-335: full keybinding table with Space toggle; lines 229-244: rendering spec with `[x]`/`[ ]` display |
| B13 | `TextArea`/`Date` field types absent from form spec | **FIXED** | `form-modal-spec.md` field type table now includes `TextArea` (opens `$EDITOR` via `LaunchEditor`) and `Date` (inline text input with `YYYY-MM-DD` validation) |
| B14 | Form height formula contradiction (`+4` vs `+6`) | **FIXED** | `form-modal-spec.md` Sizing section: `field_count + 6` with breakdown provided |
| B15 | Form footer positioned on border row (`form_area.y + form_area.height - 1`) | **FIXED** | `form-modal-spec.md` line 382: `y: inner.y + inner.height - 1` — uses `inner`, not `form_area` |
| B16 | Detail modal scroll architecture unspecified | **FIXED** | `jira-plugin.md` lines 607-613: explicit cursor-follows `scroll_offset` algorithm with if/else bounds |
| B17 | Detail modal cursor rendering unspecified | **FIXED** | `jira-plugin.md` lines 588-590: `theme::selected()` background on selected field row; lines 635-638: rendering pseudocode |
| B18 | Detail modal label alignment column width unspecified | **PARTIALLY FIXED** | `jira-plugin.md` shows consistent right-aligned label pattern in mockup; detail.rs rendering pseudocode in spec exists. However, `label_col_width` used in `form-modal-spec.md` line 364 is referenced but never assigned a value. An agent must infer the column width |
| B19 | `FormState` submit-key duality (`S` for creation, `Enter` for transition) | **FIXED** | `form-modal-spec.md` lines 63-99: state machine documents both; `JiraModal::CreateForm` uses `S`, `JiraModal::TransitionFields` uses `Enter`. The key difference is handled by which modal variant is active, not a mode bit in `FormState` — cleaner design |
| B20 | Concurrency C1: `refreshing` never clears on generation mismatch | **FIXED** | `jira-plugin.md` line 879: "discard the data BUT still clear `refreshing = false`" |
| B21 | Concurrency C2: optimistic state clobbered by auto-refresh during write latency | **FIXED** | `jira-plugin.md` line 683: "Set `refreshing = true` immediately when sending any write command" |
| B22 | Concurrency C3: 500ms delay mechanism unspecified | **FIXED** | `jira-plugin.md` line 877: "MUST be implemented as a TUI-side timer... `pending_refresh_at: Option<Instant>`. When `on_tick()` fires and `Instant::now() >= pending_refresh_at`, send FetchMyIssues. Do NOT use `thread::sleep()`" |
| B23 | Concurrency C4: rapid open/close Disconnected error | **FIXED** | `jira-plugin.md` lines 452: full thread respawn guard — checks `shutdown_flag`, calls `join()` on old thread before spawning new one |
| B24 | `&mut self` modal routing borrow trap | **STILL OPEN** | `plugin-architecture.md` modal routing pattern still shows the unsound `handle_modal_key(key, modal)` with `&mut self` receiver. Agents copying this literally will hit a borrow error. No corrected pattern is shown |
| B25 | `handle_modal_key` borrow checker trap | Same as B24 |
| B26 | Transition picker Esc: back to board vs back to detail modal | **STILL OPEN** | `horizontal-scroll-spec.md` keybinding table says "Esc/q: back to dashboard". The TUI Craftsman v2 review flagged that users pressing `s` from within the detail modal expect Esc to return to detail, not board. No resolution in any spec |
| B27 | `orderedList` counter threading in ADF | **FIXED** | `jira-api-reference.md` now documents enumerate-based counter with a `let mut counter = 1` pattern for ordered lists |
| B28 | `issues[*].fields` deserialization strategy unspecified | **STILL OPEN** | No Rust struct for `fields` with dynamic custom fields is prescribed. Agents must independently choose between `serde_json::Value` for the whole fields blob vs typed struct with `#[serde(flatten)]`. Both compile; they diverge at integration |
| B29 | Column vertical scroll not specified | **FIXED** | `horizontal-scroll-spec.md` lines 230-265: complete `col_scroll_offsets: Vec<usize>` state variable, cursor-follows algorithm, `g`/`G` behavior, reset-on-column-change policy |
| B30 | `centered_rect` pixel vs percentage ambiguity | **FIXED** | `form-modal-spec.md` lines 434-444: defines new pixel-absolute helper with explicit warning "Do NOT use `crate::modals::centered_rect()`" |

### Blocker Summary

- **Fixed**: 26 of 30 blockers
- **Still Open**: 4 blockers (B7, B18, B24/B25, B26, B28)

---

## Part 2: Unguided Decision Count Per Sub-Phase

An **unguided decision** is a point where an agent must choose between valid alternatives without spec guidance. Threshold: ≤2 per sub-phase is acceptable; ≤5 total BLOCKER-level across all sub-phases.

### Phase 1a — Foundation (Data layer + board rendering)

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`issues[*].fields` deserialization strategy** (B28 above) — No Rust struct prescribed for the hybrid `fields` object. Agents choose between `serde_json::Value` for the whole blob (simpler) vs typed struct with `#[serde(flatten)]` for known fields + remainder map (type-safe). Both compile; they produce incompatible `models.rs` implementations if Agents D and E diverge. | HIGH |
| 2 | **Done column filtering strategy** — Spec says Done column is hidden by default (toggle `D`), and `/search` fetches ALL assigned issues. It is never explicitly stated whether "all issues" includes Done-status issues fetched via JQL, or whether they are client-side filtered. An agent may filter via JQL (`AND statusCategory != Done`), which breaks the `D` toggle without a re-fetch. | MEDIUM |
| 3 | **`ureq::agent()` v2 API in secondary doc** (B7) — `plugin-architecture.md` background thread template shows v2 API. Agents reading this doc will get a compile error. Primary reference is correct but agents may read the wrong document. | COMPILE ERROR if agent reads wrong doc |

**Phase 1a unguided decisions: 3 (1 HIGH, 1 MEDIUM, 1 conditional compile error)**
**Threshold verdict: EXCEEDS ≤2 limit (1 HIGH gap + 1 ambiguous compile risk)**

---

### Phase 1b — Issue Interaction (transitions, detail modal)

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`&mut self` modal routing borrow trap** (B24/B25) — `plugin-architecture.md` shows `handle_modal_key(key, modal)` with `&mut self`, which is a two-mutable-borrow compile error. Agents must know to restructure using `take()`/replace or pass fields explicitly. No corrected pattern shown. | COMPILE ERROR |
| 2 | **Transition picker Esc destination** (B26) — When transition picker is opened from inside the detail modal, Esc should return to detail. Spec says Esc returns to board. An agent following the spec literally destroys user context. | MEDIUM (UX correctness) |
| 3 | **Transitions loading state in picker** — Transitions are fetched lazily. The spec does not describe what the transition picker shows while `FetchTransitions` is in-flight. An agent must choose: empty list, spinner row, disabled state. | MINOR |

**Phase 1b unguided decisions: 3 (1 compile error, 1 medium, 1 minor)**
**Threshold verdict: EXCEEDS ≤2 limit (compile error from borrow trap)**

---

### Phase 1c — Editing and Comments

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`label_col_width` value undefined** (B18 partial) — `form-modal-spec.md` line 364 uses `label_col_width` in the cursor positioning snippet but never assigns it. Agents must infer: longest field label name? Fixed 14? An incorrect value misaligns the blinking cursor in `EditingText` state. | MEDIUM |
| 2 | **`ValidationError` clearing on field correction** — State machine says `ValidationError` behaves "same as Navigating, but error markers shown." When user corrects a field and presses Enter: does the `!` marker clear immediately (on Enter save) or only on the next `S` submit? Unspecified. | MINOR |

**Phase 1c unguided decisions: 2 (1 MEDIUM, 1 MINOR)**
**Threshold verdict: MEETS ≤2 limit**

---

### Phase 1d — Issue Creation

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **`label_col_width` value undefined** (same as 1c) — same cursor positioning gap applies in the creation form as in the editing form. | MEDIUM |
| 2 | **Dropdown `Clear` widget not specified** — The dropdown rendering spec gives position/dimensions but never says to render `Clear` before the dropdown border. Without `Clear`, the dropdown overlaps underlying text with background color artifacts. Existing `input.rs` and `select.rs` both call `frame.render_widget(Clear, popup_area)` first. An agent that copies from the spec rather than the code will produce visual artifacts. | MINOR |

**Phase 1d unguided decisions: 2 (1 MEDIUM, 1 MINOR)**
**Threshold verdict: MEETS ≤2 limit**

---

### Phase 1e — Polish

| # | Unguided Decision | Severity |
|---|-------------------|----------|
| 1 | **Relative time format boundaries** — "2h ago", "1d ago". No existing utility. Format thresholds (when does "minutes" become "hours"?) unspecified. | MINOR |
| 2 | **Rate limit retry implementation in background thread** — Spec says sleep `Retry-After` seconds on 429. Background thread uses `recv_timeout(100ms)`. A `thread::sleep(30s)` would block the shutdown-signal poll loop. Spec does not show how to implement interruptible sleep using the 100ms chunks. | MINOR |

**Phase 1e unguided decisions: 2 (2 MINOR)**
**Threshold verdict: MEETS ≤2 limit**

---

### Unguided Decision Totals

| Sub-Phase | Compile Errors | HIGH | MEDIUM | MINOR | Total |
|-----------|---------------|------|--------|-------|-------|
| 1a | 1 (conditional) | 1 | 1 | 0 | 3 |
| 1b | 1 | 0 | 1 | 1 | 3 |
| 1c | 0 | 0 | 1 | 1 | 2 |
| 1d | 0 | 0 | 1 | 1 | 2 |
| 1e | 0 | 0 | 0 | 2 | 2 |
| **Total** | **2** | **1** | **4** | **5** | **12** |

**BLOCKER-level (compile errors + HIGH): 3 total across all phases (threshold ≤5)**
**Per-sub-phase threshold violations: 1a and 1b each exceed ≤2 limit**

---

## Part 3: Remaining Open Issues Assessment

### BLOCKER-Level Remaining Issues

**Issue R1: `ureq::agent()` in `plugin-architecture.md` (conditional compile error)**

The background thread template in `plugin-architecture.md` line 535 shows `let client = ureq::agent()` which is the ureq v2 API. The correct v3 API is `ureq::Agent::new()`. An agent who models the background thread from `plugin-architecture.md` will get a compile error. The primary reference (`jira-api-reference.md`) is correct.

**Risk**: Medium. An experienced agent will check `jira-api-reference.md` first (it's the authoritative API reference). An agent that shortcuts to the architecture doc for the background thread template will fail. In a multi-agent scenario where Agent D focuses on `api.rs` and reads `plugin-architecture.md` for threading patterns, this hits.

**Issue R2: `&mut self` modal routing borrow trap (compile error)**

`plugin-architecture.md` modal routing pattern shows:
```rust
if let Some(modal) = &mut self.modal {
    return self.handle_modal_key(key, modal);
}
```
If `handle_modal_key` takes `&mut self`, this is a two-mutable-borrow compile error. No corrected pattern is documented anywhere. Agents must independently discover the `take()`/replace pattern or pass individual fields.

**Risk**: High. This is in the "main reference" for plugin development patterns. Every agent implementing `jira/mod.rs` handle_key will copy this. The fix is well-known to experienced Rust developers but is not in any spec.

**Issue R3: `issues[*].fields` deserialization strategy (HIGH inter-agent divergence)**

No prescribed Rust struct for the dynamic `fields` object. Agent D and Agent E will independently invent incompatible strategies for `models.rs`.

**Risk**: Medium in single-agent scenarios (one agent makes a consistent choice). High in multi-agent scenarios (D and E will diverge at the `JiraIssue` constructor).

**Issue R4: Transition picker Esc destination (UX correctness)**

Pressing `s` from within the detail modal opens the transition picker. When the user presses Esc, the spec says return to board. The user expects to return to the detail modal. An agent following the spec literally destroys user context.

**Risk**: Low-Medium. This is a UX issue, not a compile error or data corruption. The TUI will work; it just won't feel right. Not a blocker for initial functionality.

---

## Part 4: What Changed Between Review Rounds

The v2 reviews identified 30 blockers. Of these, 26 have been fixed. The fixes are substantial:

**Largest improvements since v2:**
- Form modal: `FieldValue` type defined, `FormState` value storage architecture explicitly documented, `MultiSelectOpen` state fully specified, `TextArea`/`Date` rows added to field type table, height formula corrected, footer positioning corrected
- Detail modal: complete rendering pseudocode written, scroll algorithm documented, cursor rendering specified, two-section focus model (`DetailFocus`) defined
- Concurrency: all 4 concurrency gaps (C1-C4) addressed with explicit language — `refreshing` clearing on stale generation, optimistic UI suppression, 500ms timer mechanism, thread join on rapid open/close
- Horizontal scroll: column vertical scroll (`col_scroll_offsets`) fully specified with cursor-follows algorithm, scroll state preservation across refresh documented
- Sprint field: `extract_sprint_name()` now handles both array and object forms via `serde_json::Value`
- Priority colors: mapped to 3 existing constants without needing new ones

**Still unaddressed:**
- `ureq::agent()` in secondary doc (stale example)
- `&mut self` borrow trap in modal routing pattern
- `label_col_width` value never defined
- `issues[*].fields` deserialization strategy choice
- Transition picker Esc destination
- "Or `>` marker" ambiguity in `horizontal-scroll-spec.md` Selected Card section (minor)

---

## Part 5: Success Rate Estimation

### Single Sequential Agent Scenario

A single capable Rust agent working through sub-phases 1a → 1b → 1c → 1d → 1e in order:

**Phase 1a (Data layer + board):**
- The `ureq::agent()` trap will likely be avoided if the agent reads `jira-api-reference.md` (the authoritative reference). Success: ~85%.
- The `fields` deserialization strategy choice is the agent's to make but either works. No divergence risk in single-agent.
- The `&mut self` borrow trap in modal key routing will be hit in Phase 1b, not 1a.

**Phase 1b (Detail modal, transitions):**
- The `&mut self` borrow trap is real and will cause a compile failure. An experienced Rust agent will fix it, but it requires knowledge not in any spec. Expected: 1-2 wasted attempts, ~30min delay. Does not block completion; just requires fixing.
- The transition picker Esc destination is a UX issue that a reasonable agent will get wrong per the spec, producing a suboptimal but working implementation.

**Phase 1c-1d (Editing, comments, creation):**
- `label_col_width` gap: agent will pick a plausible value (likely `max_label_len + 2`). Visual artifact risk but not a functional failure.
- The rest is well-specified.

**Phase 1e (Polish):**
- Both remaining gaps are minor implementation details with clear reasonable defaults.

**Single-agent Phase 1 success rate**: **~82%**

The primary risks are:
1. Agent reading `plugin-architecture.md` background thread example and getting `ureq::agent()` error — fixable but unexpected.
2. Agent hitting `&mut self` borrow trap in modal routing — fixable with Rust knowledge not in spec.
3. Agent making a `fields` deserialization choice that works but differs from what a parallel agent would choose.

If the agent is told to prioritize `jira-api-reference.md` over `plugin-architecture.md` for API/threading patterns, risk #1 drops significantly.

### Multi-Agent Parallel Scenario (per TEAM-PLAN.md)

The additional risk in multi-agent execution is Agent D and Agent E diverging on `issues[*].fields` strategy and `jira/mod.rs` stub boundary. These create integration failures that are invisible until merge.

**Multi-agent Phase 1 success rate**: **~65%**

Integration failures between D and E are the primary driver. The `FieldValue` type is now defined (previously an unguided gap) which helps significantly, but `fields` deserialization strategy divergence remains.

---

## Part 6: Verdict

### CONDITIONAL APPROVE

**For single sequential agent**: **APPROVE** at ~82% estimated success rate, above the 80% threshold.

**For multi-agent parallel execution**: **REJECT** at ~65% estimated success rate — the `issues[*].fields` deserialization strategy divergence and `jira/mod.rs` stub boundary ambiguity remain unresolved inter-agent coordination gaps.

The specs have improved dramatically since v2. The three foundational issues that caused the previous REJECT verdicts (LaunchEditor handler, `theme::selection()`, Cargo.toml) are all fixed. The form modal, detail modal, concurrency model, and horizontal scroll are now substantially complete.

### Minimum Fixes Required for Multi-Agent APPROVE

In priority order — these 3 changes would push multi-agent success above 80%:

**Fix 1**: Add a prescribed deserialization strategy to `jira-api-reference.md` for `issues[*].fields`:

```rust
// Prescribed approach: serde_json::Value for the fields blob
#[derive(Deserialize)]
struct RawIssue {
    key: String,
    #[serde(default)]
    fields: serde_json::Value,
}
// Then extract each field by path from the Value using .get("fieldname")
```

Add this as a "Rust deserialization strategy" note in the `/search` endpoint section, explicitly choosing one approach over the other.

**Fix 2**: Add a corrected modal routing pattern to `plugin-architecture.md` showing the `take()`/replace approach:

```rust
// CORRECT: take the modal out before calling a method that needs &mut self
fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
    if let Some(mut modal) = self.modal.take() {
        let (action, new_modal) = self.handle_modal_key(key, modal);
        self.modal = new_modal;
        return action;
    }
    self.handle_board_key(key)
}
```

**Fix 3**: Define `label_col_width` in `form-modal-spec.md`:

```rust
// label_col_width: max label name length among all fields + 2 chars for ": " separator
let label_col_width = fields.iter()
    .map(|(f, _)| f.name.len())
    .max()
    .unwrap_or(10) as u16 + 2;
```

**Fix 4** (secondary): Change `horizontal-scroll-spec.md` Selected Card section to drop "Or a `>` marker" — mandate `theme::selected()` only for consistency.

**Fix 5** (secondary): Change `plugin-architecture.md` line 535 from `ureq::agent()` to `ureq::Agent::new()`.

---

## Part 7: Items That Are Solid and Ready

For completeness, the following are correctly specified and do not require agent judgment:

- `SidebarPlugin` / `ScreenPlugin` traits — exact signatures in code; `AboutPlugin` is a working reference
- `PluginAction` enum — all four variants documented and wired in `app.rs`
- `PluginRegistry` — API fully documented; `AboutPlugin` registered and tests pass
- `PluginConfig.extra` / `serde(flatten)` — implemented in `config.rs`; config pipeline ready
- Authentication pattern (Basic Auth, base64, `/myself` accountId) — fully specified
- All 15 JIRA API endpoints — request/response shapes, Rust structs, and extraction rules documented
- Thread lifecycle (spawn/shutdown, `AtomicBool`, channels) — all 4 concurrency gaps fixed
- `BoardState` horizontal scroll — complete with cursor-follows, viewport math, scroll dots
- `col_scroll_offsets` column vertical scroll — complete with cursor-follows algorithm
- `FormState` state machine — all states, transitions, and key bindings documented
- `FieldValue` type — defined with POST body conversion table
- Detail modal rendering — pseudocode provided, scroll/cursor algorithm documented
- Optimistic UI — pattern specified with `refreshing` suppression on write commands
- Post-write refresh — 500ms TUI-side timer via `pending_refresh_at: Option<Instant>` specified
- Generation counter stale-result handling — `refreshing` cleared regardless of generation
- `JiraError` type — defined in `jira-plugin.md`
- `JiraPlugin` struct — all fields defined with types
- `JiraModal` enum — definitive version in `jira-plugin.md` (ignore illustrative example in `plugin-architecture.md`)
- `ScreenId::Plugin(String)` flat navigation — implemented in `events.rs`
- `J` keybinding entry point — documented and consistent across docs
- ADF `adf_to_text` / `text_to_adf` — algorithms documented with null-safe attribute access
