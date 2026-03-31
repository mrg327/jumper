# Rust Compiler Review v2 — JIRA Plugin Specs

**Reviewer role**: Rust compiler engineer. Zero bias toward approval. Single question: will this compile and run correctly?
**Date**: 2026-03-27
**Scope**: All five design specs, Phase 0 implemented code (mod.rs, registry.rs, about.rs, app.rs, events.rs, Cargo.toml, config.rs), plus the two prior review documents.

---

## What Changed Since the Prior Reviews

The previous reviews (REVIEW-rust-systems.md + REVIEW-consistency-audit.md) identified several hard blockers. I verify each before adding new findings:

### Previously-blocked item #1 — `serde`/`serde_json`/`ureq`/`base64` missing from Cargo.toml

**Current state**: `jm-tui/Cargo.toml` now reads:
```toml
ureq = { version = "3", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
```

**FIXED.** All four dependencies are present with correct versions and features. `serde` has `derive` enabled. `ureq = "3"` with the `json` feature. This resolves the previous FAIL verdict on Cargo.toml.

### Previously-blocked item #2 — `PluginAction::LaunchEditor` undocumented

**Current state**: The variant exists in `plugins/mod.rs` lines 77-78. The app-side handler is fully implemented in `app.rs` lines 536-542 (write temp file, store `pending_editor_plugin`) and lines 208-232 (suspend TUI, launch `$EDITOR`, read back, call `on_editor_complete`). The `plugin-architecture.md` spec documents the full lifecycle in the "Editor Integration" section (lines 176-220).

**FIXED.** The cargo check warning from the previous audit no longer applies — the handler is now wired. Both `Toast` and `LaunchEditor` are dispatched in the `ScreenId::Plugin` arm at lines 529-543.

### Previously-blocked item #3 — `theme::selection()` wrong name

**Current state**: `horizontal-scroll-spec.md` "Selected Card" section still says `theme::selection()`. The actual function in `theme.rs` line 120 is `theme::selected()`. `theme::selection` does not exist.

**STILL OPEN — COMPILE ERROR** if an agent copies this literally.

---

## Area-by-Area Assessment

---

### 1. Cargo.toml — PASS

All required dependencies are present:
- `ureq = { version = "3", features = ["json"] }` — correct version, correct feature flag
- `serde = { version = "1", features = ["derive"] }` — `derive` feature present; `#[derive(Deserialize, Serialize)]` will compile
- `serde_json = "1"` — present; `serde_json::Value` and `json!()` macro available
- `base64 = "0.22"` — present; `base64::engine::general_purpose::STANDARD.encode()` matches this version

`std::sync::{Arc, AtomicBool, Ordering}`, `std::thread::JoinHandle`, `std::sync::mpsc` are all from `std` — no extra crates needed.

`tempfile = "3"` is in dev-dependencies only, which is appropriate.

No missing dependency that would cause a compile error on Phase 1 code.

---

### 2. Trait Signatures — PASS with one documentation inconsistency

The implemented `ScreenPlugin` trait in `plugins/mod.rs` is the ground truth. Key signatures:

```rust
fn render(&self, frame: &mut Frame, area: Rect);
fn handle_key(&mut self, key: KeyEvent) -> PluginAction;
fn on_enter(&mut self);
fn on_leave(&mut self);
fn on_tick(&mut self) -> Vec<String> { Vec::new() }
fn key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
fn on_editor_complete(&mut self, _content: String, _context: &str) {}
```

All of these match what the JIRA plugin spec requires. One documentation inconsistency remains:

- `plugin-architecture.md` shows `key_hints()` returning `Vec<(&str, &str)>` (no `'static`), but the implemented trait uses `Vec<(&'static str, &'static str)>`. This is a **docs inconsistency only** — the code is correct. An agent reading only `plugin-architecture.md` and trying to return format strings from `key_hints()` will get a lifetime compile error. **CONCERN** (doc vs code mismatch; code wins but agents reading the wrong doc will hit an error).

- `plugin-architecture.md` shows `on_notify` on `ScreenPlugin`. The implemented trait omits it. Since it has a default impl in the spec, absence is backwards-compatible. No compile issue.

The `AboutPlugin` in `about.rs` is a correct reference implementation that agents can model their JIRA plugin after. Trait bounds match. `needs_timer()` is on the implemented trait (line 94) but absent from the `plugin-architecture.md` spec — agents adding a timer to JIRA plugin will find it in the code.

---

### 3. PluginAction Completeness — PASS

```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { content: String, context: String },
}
```

The JIRA plugin needs all four variants:
- `None` — for unhandled keys
- `Back` — Esc/q return to dashboard
- `Toast(String)` — creation success, refresh failure notices
- `LaunchEditor { content, context }` — for comment input and TextArea fields

The app-side conversion is complete and correct (lines 529-543 in `app.rs`):
- `None` → `Action::None`
- `Back` → calls `handle_back()`, returns `Action::None`
- `Toast(msg)` → `Action::Toast(msg)` (which is processed by the update loop)
- `LaunchEditor` → writes temp file, stores `pending_editor_plugin`, returns `Action::None`

One edge case: if the temp file write in `LaunchEditor` fails (`std::fs::write(...).ok()` discards the error), the editor launches with an empty file silently. This is acceptable for a TUI tool but could confuse users. Not a compile error; a minor UX issue.

No missing `PluginAction` variant for the JIRA plugin requirements.

---

### 4. Type Compatibility — PASS with one concern

The Rust struct definitions in `jira-plugin.md` use types that exist in the dependency tree:

| Type | Source | Available? |
|------|--------|------------|
| `HashMap<String, String>` | `std::collections::HashMap` | YES (prelude-adjacent) |
| `serde_json::Value` | `serde_json` crate | YES |
| `Arc<AtomicBool>` | `std::sync` | YES |
| `std::sync::mpsc::Sender<T>` | `std::sync::mpsc` | YES |
| `std::thread::JoinHandle<()>` | `std::thread` | YES |
| `std::time::Instant` | `std::time` | YES |
| `Vec<JiraIssue>` | defined in models.rs | YES (agent must define) |
| `BoardState` | defined in horizontal-scroll-spec | YES (agent must define) |
| `FormState` | defined in form-modal-spec | YES (agent must define) |

**CONCERN**: `JiraPlugin` contains `board: BoardState`. `BoardState` is defined only in `horizontal-scroll-spec.md`. When the JIRA plugin module is split across `jira/mod.rs`, `jira/board.rs`, `jira/models.rs`, etc., agents must ensure `BoardState` is defined before use and imported correctly. The spec says to put it in `board.rs` but `JiraPlugin` (in `mod.rs`) holds it by value. Normal Rust module resolution handles this — it is just a declaration-order dependency agents must be aware of.

**CONCERN — `FieldType` enum not `#[derive(PartialEq)]`**: The spec defines `FieldType` without derives. The form rendering code will need to `match` on it (fine without PartialEq) but pattern matching is sufficient. No issue unless an agent writes `==` comparisons — possible but not specified.

The `JiraPlugin` struct as defined in `jira-plugin.md` (lines 319-352) is complete with all field types. No ambiguity in the primary plugin struct definition.

---

### 5. serde Deserialization Feasibility — CONCERN

This is the most technically dense area. Several issues with the JSON→Rust mapping:

**5a. PASS: Simple fields.** `summary` (String), `created`/`updated` (String), `labels` (`Vec<String>`) deserialize cleanly with standard derives.

**5b. CONCERN: Dynamic custom field IDs.** `story_points_field` and `sprint_field` are runtime-determined, so the `fields` object in the search response cannot be a fully-typed struct. The spec documents extraction via `serde_json::Value` path traversal, but provides no concrete Rust struct for the outer `fields` object. An agent must choose between two approaches:

Option A — deserialize `fields` as `serde_json::Value`:
```rust
#[derive(Deserialize)]
struct RawIssue {
    key: String,
    fields: serde_json::Value,
}
```

Option B — use a typed struct with `#[serde(flatten)]` for the remainder:
```rust
#[derive(Deserialize)]
struct IssueFields {
    summary: String,
    status: RawStatus,
    // ... known fields ...
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}
```

Either works. Neither is prescribed. Agents will make different choices. This is a **design-decision gap** but not a compile blocker — both compile.

**5c. CONCERN: Sprint field shape.** The JSON shows `customfield_10020` as an array of objects: `[{ "id": 37, "name": "Sprint 24", "state": "active", ... }]`. Note `id` is a **number** (37), not a string. An agent writing:
```rust
#[derive(Deserialize)]
struct SprintValue {
    id: String,  // WRONG — will panic on valid JIRA data
    name: String,
    state: String,
}
```
will get a runtime deserialization error (`invalid type: integer, expected a string`). The correct type is `id: u32` or `id: i64`. No struct is given in the spec; the field extraction table only shows the output (`Option<String>` sprint name), not the intermediate deserialization struct. **This is a real runtime panic risk on well-formed JIRA responses.**

**5d. PASS: StatusCategory enum.** `jira-plugin.md` says "Use `#[serde(other)]` or manual deserialization with fallback for unknown values." The JIRA API uses `statusCategory.key` values like `"new"`, `"indeterminate"`, `"done"`, `"undefined"`. A correct implementation:
```rust
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum StatusCategoryKey {
    New,
    Indeterminate,
    Done,
    #[serde(other)]
    Undefined,
}
```
This compiles and handles unknown keys. The spec guidance is correct. Minor gap: no example derive block shown; agents must know `#[serde(other)]` requires a unit variant with no data, which is documented in serde docs but not here.

**5e. CONCERN: `transitions.fields` map deserialization.** The API returns `fields` as a JSON object keyed by field ID. Each value is a field metadata object. The spec says `fields` is a map, not an array (noted prominently). Correct deserialization:
```rust
#[derive(Deserialize)]
struct TransitionsResponse {
    transitions: Vec<RawTransition>,
}

#[derive(Deserialize)]
struct RawTransition {
    id: String,
    name: String,
    to: RawStatus,
    fields: HashMap<String, TransitionFieldMeta>,
}
```
`TransitionFieldMeta` needs at minimum `required: bool`, `name: String`, `allowedValues: Option<Vec<AllowedValue>>`, and `schema: FieldSchema`. The spec's `TransitionField` struct in `jira-plugin.md` has `is_comment: bool` but this is not derivable from the JSON — it must be inferred from field type. An agent adding `#[serde(deny_unknown_fields)]` will panic on the `hasDefaultValue` and `operations` JSON fields that have no matching Rust field. Since the spec does NOT say to use `deny_unknown_fields`, and the default serde behavior is to ignore unknown fields, this is safe as long as agents use the default.

**5f. PASS: `editmeta.fields` map.** The JSON is a map keyed by field ID. Same `HashMap<String, EditFieldMeta>` approach applies. Documented correctly.

**5g. PASS: `createmeta` response.** Uses `"values"` key (not `"issueTypes"`) — this is explicitly called out in the spec. Agents who read the spec will get this right.

**5h. CONCERN: ADF deserialization.** The spec specifies `adf_to_text()` as a tree-walking algorithm over `serde_json::Value`. The algorithm is documented with a pseudocode table. However, ADF nodes can have `attrs` (e.g., heading level, link URL, mention text). The pseudocode uses `node["attrs"]["level"]` for headings. If `attrs` is absent, `node["attrs"]["level"]` returns `serde_json::Value::Null`, and `.as_u64()` returns `None`. The spec implies this is handled gracefully but does not show the null-propagation path explicitly. An agent using `node["attrs"]["level"].as_u64().unwrap()` will panic on every paragraph (which has no `attrs`). **Real panic risk on well-formed JIRA data.** The fix is `.unwrap_or(1)` or a checked path. Documented in pseudocode style only; not as compilable Rust.

---

### 6. Borrow Checker Patterns — PASS

The clone-first pattern for `ScreenId::Plugin` is correctly documented and implemented. `app.rs` lines 409, 481, 525 all show `let name = name.clone()` before the second borrow. `AboutPlugin` demonstrates the pattern compiles.

The `JiraPlugin` with `Option<mpsc::Sender<JiraCommand>>` and `Option<mpsc::Receiver<JiraResult>>` fields avoids the borrow checker issue of holding channel endpoints alongside the struct. Wrapping in `Option` is the canonical pattern. The shutdown flag as `Option<Arc<AtomicBool>>` follows the same pattern.

The `JiraModal` enum uses struct variants with named fields (e.g., `IssueDetail { issue_key: String, ... }`). These own their data. No lifetime parameters needed. No borrow checker issue.

One subtle concern: `JiraModal::IssueDetail` contains `transitions: Option<Vec<JiraTransition>>` where `JiraTransition` contains `Vec<TransitionField>` which contains `Vec<AllowedValue>`. This is a deeply owned tree — it will clone verbosely but will not cause borrow errors.

`handle_key` receives `&mut self` and the modal borrow pattern from `plugin-architecture.md`:
```rust
if let Some(modal) = &mut self.modal {
    return self.handle_modal_key(key, modal);
}
```
This will fail to compile if `handle_modal_key` takes `&mut self` (two `&mut self` borrows simultaneously). The spec shows this pattern but does not address the borrow issue. The correct approach is to `take()` the modal, process, and put it back, or to restructure. This is a **borrow checker trap for agents copying the pattern literally.**

---

### 7. Thread Safety — PASS with one documentation inconsistency

**Channels**: `mpsc::Sender<JiraCommand>` is `Send + Sync` (Sender is Clone + Send). `mpsc::Receiver<JiraResult>` is `Send` but not `Sync` — only one thread should own the receiver, which matches the design (TUI thread owns `result_rx`). All correct.

**AtomicBool**: `Arc<AtomicBool>` with `Ordering::Relaxed` for shutdown flag is correct for a simple "stop running" signal where the ordering of memory operations between the flag write and subsequent reads doesn't need to be sequentially consistent. No data races.

**Background thread function signature**:
```rust
fn background_main(
    shutdown: Arc<AtomicBool>,
    commands: mpsc::Receiver<Command>,
    results: mpsc::Sender<ApiResult>,
)
```
All three types are `Send`. The background thread owns all three. The TUI thread owns the inverse ends (`command_tx: Sender`, `result_rx: Receiver`). This is sound.

**`recv_timeout` usage**: `commands.recv_timeout(Duration::from_millis(100))` is correct API for `mpsc::Receiver`. Returns `Ok(T)`, `Err(RecvTimeoutError::Timeout)`, or `Err(RecvTimeoutError::Disconnected)`. The spec handles all three cases.

**Documentation inconsistency**: `plugin-architecture.md` line 535 still shows `let client = ureq::agent()` (ureq v2 API). The authoritative `jira-api-reference.md` correctly shows `ureq::Agent::new()` (v3 API). The v2 `ureq::agent()` function does not exist in ureq v3 and will cause a compile error if copied from the wrong doc. **CONCERN** — agents reading `plugin-architecture.md` for the background thread template will get a non-compiling constructor.

---

## Consolidated Issue List

### FAIL — Would cause compile error

| # | Issue | Location | Fix |
|---|-------|----------|-----|
| F1 | `theme::selection()` does not exist (only `theme::selected()`) | `horizontal-scroll-spec.md` "Selected Card" section | Replace `theme::selection()` with `theme::selected()` |
| F2 | `ureq::agent()` is v2 API; `ureq::agent()` does not compile against ureq v3 | `plugin-architecture.md` background thread example (line 535) | Replace with `ureq::Agent::new()`. Note: `jira-api-reference.md` is already correct; this is only the secondary doc |
| F3 | `handle_modal_key(key, modal)` with `&mut self` receiver is a two-mutable-borrow trap | `plugin-architecture.md` modal routing pattern | Agents must restructure: either `take()` modal before calling, or pass fields explicitly |

### CONCERN — Would cause runtime panic on well-formed JIRA data

| # | Issue | Location | Risk |
|---|-------|----------|------|
| P1 | Sprint `id` field is a JSON integer (37), not a string — `id: String` struct field will panic during deserialization | `jira-api-reference.md` sprint value shape | Use `id: u32` or `id: serde_json::Value`; no struct given so agents must infer |
| P2 | ADF walker using `node["attrs"]["level"].as_u64().unwrap()` will panic on nodes without `attrs` | ADF algorithm in `jira-api-reference.md` | Must use `.unwrap_or(N)` everywhere attrs are accessed |

### CONCERN — Design decision gap; different agents will write incompatible code

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| D1 | No prescribed Rust struct for `issues[*].fields` with dynamic custom field IDs | `jira-api-reference.md` search endpoint | Agents diverge between typed struct + flatten vs raw `serde_json::Value` |
| D2 | `key_hints()` lifetime: spec shows `&str`, code requires `&'static str` | `plugin-architecture.md` vs `plugins/mod.rs` | Agent writing `format!()` output in `key_hints()` gets lifetime error |

### CONCERN — Spec inconsistency that will cause incorrect behavior

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| B1 | Form submit key: creation form uses `S`, transition form uses `Enter` — `FormState` enum has no mode bit to differentiate | `form-modal-spec.md` | Agent implementing the form state machine cannot know which key to handle |
| B2 | `FormState` enum tracks cursor and edit state but NOT field values; storage architecture (parallel `Vec`) is described only in prose, not in the struct | `form-modal-spec.md` | Agent may put values inside `FormState` enum variants, causing wrong architecture |
| B3 | `theme::PRIORITY_HIGHEST` and `theme::PRIORITY_LOWEST` referenced for JIRA 5-level priority but do not exist in `theme.rs` | `horizontal-scroll-spec.md` issue card rendering | Compile error unless agent defines these constants or uses `TEXT_ERROR` directly |
| B4 | Form height formula: spec prose says `field_count + 4` but correct calculation with blank padding rows is `field_count + 6` | `form-modal-spec.md` | Form rendered 2 rows too short, cutting off content |
| B5 | `render_form` footer positioned at `form_area.y + form_area.height - 1` (the border row) instead of `inner.y + inner.height - 1` | `form-modal-spec.md` snippet | Footer renders over the border, visual corruption |
| B6 | `TextArea` and `Date` field types defined in `jira-plugin.md` `FieldType` enum but absent from `form-modal-spec.md` field type table | `form-modal-spec.md` | Agent implementing the form will not handle `TextArea` (→ `$EDITOR`) or `Date` (→ inline validation) fields |
| B7 | `centered_rect` in form spec is a pixel-absolute function; the existing `crate::modals::centered_rect()` takes percentages. Spec does not warn agents to avoid the existing function | `form-modal-spec.md` | Agent calling existing function with pixel values gets wrong positioning |
| B8 | `g`/`G` keybindings in horizontal-scroll-spec but absent from jira-plugin.md board keybinding table | Cross-document | Incomplete keybinding implementation |

---

## Verdict by Area

| Area | Verdict | Key Finding |
|------|---------|-------------|
| Cargo.toml | **PASS** | All deps present with correct features |
| Trait signatures (ScreenPlugin) | **PASS** | Code is ground truth; one doc vs code lifetime mismatch |
| PluginAction completeness | **PASS** | All four variants needed; all wired in app.rs |
| Type compatibility | **PASS** | All types available in dep tree; one BoardState module ordering note |
| serde deserialization | **CONCERN** | Sprint `id: u32 vs String` panic risk; ADF attrs null risk |
| Borrow checker patterns | **CONCERN** | `&mut self` modal routing pattern is unsound as written |
| Thread safety | **PASS** | Channel + AtomicBool + JoinHandle usage is correct |

---

## Final Verdict: REJECT

The Phase 0 foundation compiles correctly and is solid. The `PluginAction::LaunchEditor` handler is fully implemented and wired — the previous REJECT reasons 2 and 4 are resolved. However, three items remain that would cause compile errors or runtime panics in agent-written Phase 1 code:

**Compile errors an agent will hit:**

1. **`theme::selection()` in `horizontal-scroll-spec.md`** — this function does not exist. Agents implementing the kanban board from this spec will get `error[E0425]: cannot find function selection in module theme`.

2. **`theme::PRIORITY_HIGHEST` / `theme::PRIORITY_LOWEST`** — five-level JIRA priority coloring is specified but these constants do not exist in `theme.rs`. Code implementing the issue card priority coloring for Highest/Lowest priority issues will fail to compile.

3. **`ureq::agent()` in `plugin-architecture.md`** — agents using the background thread template from this doc will not compile against ureq v3. The correct source (`jira-api-reference.md`) is right, but having a wrong example in a primary reference doc is a real risk.

**Runtime panics on well-formed JIRA responses:**

4. **Sprint `id` type mismatch** — if an agent creates a sprint deserialization struct with `id: String`, it will panic when JIRA returns the integer sprint ID (37). The spec shows the JSON with an integer but provides no Rust struct, leaving agents to guess the type.

5. **ADF `attrs` null access** — the ADF walker pseudocode pattern `node["attrs"]["level"]` will produce `Null` for non-heading nodes. Any `.unwrap()` call on this path panics on every paragraph, comment, and description in the JIRA dataset.

**Minimum fixes required to APPROVE:**

1. Fix `horizontal-scroll-spec.md`: change `theme::selection()` → `theme::selected()`.
2. Add `pub const PRIORITY_HIGHEST: Color = Color::Red;` and `pub const PRIORITY_LOWEST: Color = Color::DarkGray;` to `theme.rs` (or update the spec to use `PRIORITY_HIGH` and `PRIORITY_LOW` as the extreme values).
3. Fix `plugin-architecture.md` background thread example: change `ureq::agent()` → `ureq::Agent::new()`.
4. Add a concrete `SprintValue` deserialization struct to `jira-api-reference.md` with `id: u32` (not `String`).
5. Update the ADF algorithm pseudocode in `jira-api-reference.md` to show null-safe attribute access (`.get("attrs").and_then(|a| a.get("level")).and_then(|v| v.as_u64()).unwrap_or(1)`).

The following items are important but do not individually block compilation — they should be fixed before agent handoff but will not cause hard compile failures:

- Document the pixel-absolute `centered_rect` is new and distinct from `crate::modals::centered_rect()`.
- Add `TextArea` and `Date` rows to the `form-modal-spec.md` field type table.
- Fix form height formula (`+4` → `+6`) and footer positioning (`form_area` → `inner`).
- Resolve `FormState` value storage architecture explicitly in the enum definition, not just in prose.
- Add a `mode: FormMode` or equivalent to `FormState` to resolve the `S`-submits vs `Enter`-submits ambiguity between creation and transition forms.
- Add `g`/`G` to `jira-plugin.md` board keybinding table.
