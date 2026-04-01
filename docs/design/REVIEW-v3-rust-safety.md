# Rust Safety Review v3 — JIRA Plugin Specs

**Reviewer role**: Rust safety engineer. Zero bias toward approval. Will this compile and run without panics?
**Date**: 2026-03-27
**Scope**: All five design specs, full Phase 0 implemented code (mod.rs, registry.rs, about.rs, app.rs, events.rs, Cargo.toml, config.rs, theme.rs), plus the two prior review documents (REVIEW-rust-systems.md, REVIEW-consistency-audit.md, REVIEW-v2-rust-compiler.md).

---

## Status of v2 REJECT Items

v2 listed five minimum fixes required to APPROVE. I verify each.

### v2 Fix #1 — `theme::selection()` → `theme::selected()` (horizontal-scroll-spec.md)

**Current state**: The spec at `horizontal-scroll-spec.md` "Selected Card" section still reads:
> `theme::selection()` background

`theme.rs` line 120 defines `pub fn selected() -> Style`. There is no `selection()` function in `theme.rs`.

**STILL OPEN — COMPILE ERROR.**

---

### v2 Fix #2 — `theme::PRIORITY_HIGHEST` / `theme::PRIORITY_LOWEST` missing (horizontal-scroll-spec.md)

**Current state**: `horizontal-scroll-spec.md` "Priority coloring" section states:
> "Highest", "High" → `theme::PRIORITY_HIGH` (red)
> "Medium" → `theme::PRIORITY_MEDIUM` (yellow)
> "Low", "Lowest" → `theme::PRIORITY_LOW` (dim)

This is actually correct usage — the spec maps both Highest→`PRIORITY_HIGH` and Lowest→`PRIORITY_LOW`. There is no reference to `PRIORITY_HIGHEST` or `PRIORITY_LOWEST` constants in the spec. The v2 review incorrectly characterised this as a compile error; re-reading the spec, the constants used (`PRIORITY_HIGH`, `PRIORITY_MEDIUM`, `PRIORITY_LOW`) all exist in `theme.rs` (lines 15-17).

**RESOLVED — was a false positive in v2. The spec uses only existing constants.**

---

### v2 Fix #3 — `ureq::agent()` v2 API in plugin-architecture.md

**Current state**: `plugin-architecture.md` line 535 still shows:
```rust
let client = ureq::agent();
```
`jira-api-reference.md` correctly shows `ureq::Agent::new()` (ureq v3 API) with an explicit note that `ureq::agent()` is the v2 API and must NOT be used.

The two docs conflict. Any agent writing the background thread from `plugin-architecture.md` will get `error[E0425]: cannot find function agent in module ureq`.

**STILL OPEN — COMPILE ERROR.**

---

### v2 Fix #4 — Sprint `id` type mismatch (`id: String` vs JSON integer)

**Current state**: `jira-api-reference.md` line 140 now explicitly states:
> Sprint `id` is an integer (`i64`), not a string. If you define a sprint struct, use `id: i64` or deserialize as `serde_json::Value`.

The spec also provides a complete `extract_sprint_name()` function that uses `serde_json::Value` exclusively — no intermediate sprint struct with an `id` field is required at all. The extraction path avoids the problem entirely.

**RESOLVED — the spec no longer requires a typed sprint struct. The `serde_json::Value` path sidesteps the type mismatch.**

---

### v2 Fix #5 — ADF `attrs` null access panic

**Current state**: `jira-api-reference.md` lines 956-961 now include an explicit CRITICAL warning:
> CRITICAL: Always use `node.get("attrs")` and `node.get("content")` — never index directly with `node["attrs"]` as this returns `Value::Null` on missing keys and will panic on `.as_str().unwrap()`. Use the null-safe pattern: ...

The correct pattern with `unwrap_or(1)` is shown. Additionally, the conversion algorithm pseudocode uses `node["type"].as_str()` which also returns `None` on missing `type` keys — but in a `match` on `Option<&str>`, the `_` arm handles `None` safely without panic.

**RESOLVED — null-safe pattern explicitly documented with working code.**

---

## New Area-by-Area Assessment

---

### 1. Cargo.toml — PASS

All required Phase 1 dependencies present:

| Dependency | Version | Feature | Required for |
|------------|---------|---------|--------------|
| `ureq` | 3 | `json` | HTTP client + `read_json()` |
| `serde` | 1 | `derive` | `#[derive(Deserialize, Serialize)]` |
| `serde_json` | 1 | — | `Value`, `json!()` macro |
| `base64` | 0.22 | — | Basic Auth encoding |

`std::sync::{Arc, Mutex}`, `std::sync::atomic::{AtomicBool, Ordering}`, `std::sync::mpsc`, `std::thread::JoinHandle`, `std::time::Instant` — all from `std`, no extra crates needed.

`serde_yml` is not in `jm-tui/Cargo.toml` directly, but it is used only in `jm-core/config.rs` for the `extra: HashMap<String, serde_yml::Value>` field. When the JIRA plugin deserializes its config via `serde_yml::from_value(raw.clone())`, it will need `serde_yml` at the call site. `serde_yml` is a dependency of `jm-core`, not `jm-tui`. If the deserialization code lives in `jm-tui/src/plugins/jira/config.rs`, this is a **CONCERN**: the agent will need to call `serde_yml::from_value()` but `serde_yml` is not in `jm-tui/Cargo.toml`. The fix is either to add `serde_yml` to `jm-tui` or to have `jm-core` expose a helper. Not a blocker for Phase 0 code, but will be a compile error the moment the JIRA config deserialization is written.

---

### 2. Trait Signatures — PASS

The implemented `ScreenPlugin` trait in `plugins/mod.rs` is the compiled ground truth. All signatures verified:

```rust
fn name(&self) -> &str;
fn needs_timer(&self) -> bool { false }         // present in code, absent from plugin-architecture.md
fn render(&self, frame: &mut Frame, area: Rect);
fn handle_key(&mut self, key: KeyEvent) -> PluginAction;
fn on_enter(&mut self);
fn on_leave(&mut self);
fn on_tick(&mut self) -> Vec<String> { Vec::new() }
fn key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
fn on_editor_complete(&mut self, _content: String, _context: &str) {}
```

`AboutPlugin` in `about.rs` is a correct reference implementation. The `key_hints()` return type is `Vec<(&'static str, &'static str)>`. The `plugin-architecture.md` spec shows `Vec<(&str, &str)>` (no `'static` bound). An agent who reads the spec doc and tries to return a `format!()` string from `key_hints()` will get:

```
error[E0515]: cannot return value referencing local variable
```

**CONCERN** — doc/code lifetime mismatch. Code is the ground truth. Agents must read `mod.rs`, not the spec, for this signature.

The `on_notify` method appears in the spec but not in the implemented `ScreenPlugin` trait. Since the spec default is a no-op, the absence is backwards-compatible — no compile issue.

`jira-plugin.md` line 434-443 shows a `PluginAction` enum that is **missing** the `LaunchEditor` variant:
```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    // LaunchEditor is absent from this block
}
```
This block in the JIRA plugin spec is an outdated copy. The authoritative definition in `plugins/mod.rs` lines 66-78 includes `LaunchEditor { content: String, context: String }`. An agent who copies the `PluginAction` definition from `jira-plugin.md` instead of using the existing one in `mod.rs` would create a duplicate definition with three variants, causing a compile error. **CONCERN** — stale duplicate definition in jira-plugin.md could mislead an agent into redefining PluginAction.

---

### 3. PluginAction Completeness — PASS

The implemented enum has all four variants. The app-side dispatch in `app.rs` lines 529-543 handles all four:
- `None` → `Action::None`
- `Back` → `self.handle_back()`, `Action::None`
- `Toast(msg)` → `Action::Toast(msg)`
- `LaunchEditor { content, context }` → writes temp file, sets `pending_editor_plugin`, `Action::None`

The `pending_editor_plugin` path (app.rs lines 208-232) correctly: suspends TUI, launches editor, resumes TUI, reads back content, calls `plugin.on_editor_complete()`. The `std::fs::write(...).ok()` error discard on line 539 means a temp-file write failure silently launches an editor with empty content — minor UX issue, not a panic.

One new observation: the temp file name is `jm-plugin-{name}.txt`. If two plugin editor sessions were somehow triggered concurrently (not possible in the single-threaded TUI loop), this would collide. Since the TUI loop is single-threaded and sets `pending_editor_plugin` which is immediately consumed, this is safe.

---

### 4. Type Compatibility — PASS with one concern

All types used in the JIRA plugin data model are available:

| Type | Source | Available? |
|------|--------|------------|
| `HashMap<String, String>` | `std::collections` | YES |
| `serde_json::Value` | `serde_json` crate | YES |
| `Arc<AtomicBool>` | `std::sync` | YES |
| `mpsc::Sender<T>` / `Receiver<T>` | `std::sync::mpsc` | YES |
| `std::thread::JoinHandle<()>` | `std::thread` | YES |
| `std::time::Instant` | `std::time` | YES |

**CONCERN**: `JiraPlugin` holds `board: BoardState` and `modal: Option<JiraModal>`. `BoardState` is defined only in `horizontal-scroll-spec.md` and `JiraModal` only in `jira-plugin.md`. Both must be defined before use. The specs document them clearly and the module layout (`jira/board.rs`, `jira/mod.rs`) is prescriptive. No compile blocker if agents follow the layout — just an ordering dependency they must be aware of.

**CONCERN**: `FormState` is defined in `form-modal-spec.md` with a `HashSet<usize>` in `MultiSelectOpen`:
```rust
MultiSelectOpen {
    field_cursor: usize,
    dropdown_cursor: usize,
    checked: HashSet<usize>,
}
```
`HashSet` requires `use std::collections::HashSet`. This is not imported automatically — agents must add the use statement. Not a blocker, but easy to forget.

`FieldType` enum in `jira-plugin.md` has no `#[derive]` attributes shown. It needs at minimum `#[derive(PartialEq)]` to support the `match` expressions in form rendering, and `#[derive(Clone)]` if stored inside `JiraModal` variants (which it is, via `EditableField`). Missing derives will cause compile errors. **CONCERN**.

---

### 5. serde Deserialization Feasibility — PASS with concerns

**5a. PASS: Simple fields.** `summary`, `created`, `updated`, `labels` serialize cleanly.

**5b. PASS: Sprint field.** The v2 concern about `id: String` is now moot. `jira-api-reference.md` provides `extract_sprint_name()` using `serde_json::Value` throughout. No typed sprint struct needed.

**5c. PASS: StatusCategory enum.** The `#[serde(other)]` guidance is correct and sufficient.

**5d. CONCERN: `JiraErrorResponse` default impl.** The spec (jira-api-reference.md line 1078-1083) shows:
```rust
#[derive(Deserialize)]
struct JiraErrorResponse {
    #[serde(rename = "errorMessages", default)]
    error_messages: Vec<String>,
    #[serde(default)]
    errors: HashMap<String, String>,
}
```
The `.unwrap_or_default()` call on the error body parsing (jira-api-reference.md line 67) requires `JiraErrorResponse: Default`. `#[derive(Deserialize)]` does not imply `Default`. Agents must either `#[derive(Default)]` on `JiraErrorResponse` or use a different fallback. If they write `.unwrap_or_default()` without the `Default` derive, they get `error[E0277]: the trait bound JiraErrorResponse: Default is not satisfied`. **CONCERN — missing `#[derive(Default)]` on `JiraErrorResponse`.**

**5e. PASS: `transitions.fields` map.** Deserialization as `HashMap<String, TransitionFieldMeta>` is correct. Spec explicitly notes `fields` is a map, not an array. No `#[serde(deny_unknown_fields)]` is prescribed. Default serde behavior (ignore unknown fields) applies.

**5f. PASS: `createmeta` field key.** Uses `"values"` not `"issueTypes"` — explicitly called out in the spec. Agents who read it will get this right.

**5g. PASS: ADF deserialization.** Null-safe attribute access is now explicitly documented with correct Rust code. The pseudocode `node["type"].as_str()` pattern on `serde_json::Value` returns `None` (not panic) on missing keys, which the `_` match arm handles.

**5h. CONCERN: `text_to_adf` in jira-plugin.md vs jira-api-reference.md shows two different implementations.** `jira-plugin.md` lines 406-419 shows a single-paragraph implementation. `jira-api-reference.md` lines 654-664 shows a multi-paragraph version that splits on `"\n\n"`. These are both correct for their stated purpose (single paragraph for transition comments, multi-paragraph for `$EDITOR` output), but the naming collision — both are called `text_to_adf` — means an agent could implement one and use it where the other is needed. **CONCERN — name collision between single-paragraph and multi-paragraph ADF builders; agents may use the wrong one.**

---

### 6. Borrow Checker Patterns — CONCERN

**6a. Modal routing pattern — borrow conflict (v2 F3, STILL OPEN).** `plugin-architecture.md` shows:
```rust
fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
    if let Some(modal) = &mut self.modal {
        return self.handle_modal_key(key, modal);
    }
    self.handle_board_key(key)
}
```
`self.modal` is borrowed mutably via `&mut self.modal`, then `self.handle_modal_key()` requires a second `&mut self`. The borrow checker rejects this: **cannot borrow `*self` as mutable more than once at a time**.

The two correct patterns for this situation are:

**Pattern A** — `take()` the modal, process, replace:
```rust
fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
    if let Some(mut modal) = self.modal.take() {
        let result = self.handle_modal_key(key, &mut modal);
        // Put it back unless handle_modal_key decided to close it
        if !result.closes_modal {
            self.modal = Some(modal);
        }
        return result.action;
    }
    self.handle_board_key(key)
}
```

**Pattern B** — pass fields explicitly, avoid `&mut self` on the helper:
```rust
fn handle_modal_key(modal: &mut JiraModal, key: KeyEvent, board: &mut BoardState) -> PluginAction {
    // No &mut self receiver — takes only what it needs
}
```

Neither pattern is shown in the specs. Agents who copy the spec pattern literally will get a compile error. **FAIL — the canonical modal routing pattern in plugin-architecture.md is unsound and will not compile.**

**6b. PASS: Clone-before-borrow pattern for ScreenId::Plugin.** The app.rs implementation correctly uses `let name = name.clone()` at lines 409 and 481 and 525 before calling methods that require `&mut self`. This pattern is demonstrated and working.

**6c. PASS: Channel ownership.** `JiraPlugin` holds `command_tx: Option<mpsc::Sender<JiraCommand>>` and `result_rx: Option<mpsc::Receiver<JiraResult>>`. Background thread holds the inverse ends. Wrapping in `Option` allows `take()` for clean shutdown. Borrow-safe.

**6d. CONCERN: JoinHandle join on on_enter().** `jira-plugin.md` line 452 specifies:
```
call thread_handle.take().unwrap().join().ok()
```
The `.unwrap()` on `take()` is safe here because the code first checks `thread_handle.is_some()` in the if condition. But the spec does not show the explicit `is_some()` guard in a code block — only in prose. An agent who writes `self.thread_handle.take().unwrap().join()` without the guard would panic if `thread_handle` is `None`. This is a documentation clarity issue, not a compile error, but it is a real `.unwrap()` panic risk. **CONCERN.**

---

### 7. Thread Safety — PASS

**Channels**: `mpsc::Sender` is `Send + Clone`. `mpsc::Receiver` is `Send` but not `Sync`. The design has exactly one owner per channel endpoint. No `Sync` requirement anywhere. Sound.

**AtomicBool**: `Arc<AtomicBool>` with `Ordering::Relaxed` for a shutdown flag is correct. The only requirement is eventual visibility, not sequential consistency. No data race.

**Background thread function types**: `Arc<AtomicBool>` is `Send + Sync`. `mpsc::Receiver<JiraCommand>` is `Send`. `mpsc::Sender<JiraResult>` is `Send`. All are movable into a `std::thread::spawn` closure. Sound.

**`recv_timeout` usage**: `mpsc::Receiver::recv_timeout(Duration::from_millis(100))` is the correct non-blocking poll with timeout. All three `RecvTimeoutError` arms are handled in the spec pseudocode. Sound.

**`try_recv` drain loop**: `while let Ok(result) = self.result_rx.try_recv()` drains without blocking. `TryRecvError::Empty` breaks the loop. `TryRecvError::Disconnected` is NOT handled in the spec's `on_tick()` example — the loop silently exits on `Err`. An agent who needs to detect thread death must check for `Disconnected` separately. The spec mentions detecting this in prose ("If `try_recv()` returns `TryRecvError::Disconnected`, the background thread has panicked") but the `on_tick()` code example does not handle it. Minor inconsistency — not a panic, just a missing feature.

**ureq v3 constructor**: `jira-api-reference.md` correctly shows `ureq::Agent::new()`. The conflicting `plugin-architecture.md` example using `ureq::agent()` remains a compile error if followed (see Fix #3 above).

---

### 8. Panics — CONCERN

A systematic search for `.unwrap()` calls in spec code blocks:

| Location | Expression | Safe? |
|----------|-----------|-------|
| `jira-api-reference.md` line 61 | `Retry-After` parse `.unwrap_or(60)` | SAFE — explicit fallback |
| `jira-api-reference.md` line 67 | `response.into_body().read_json().unwrap_or_default()` | CONCERN — requires `JiraErrorResponse: Default` |
| `plugin-architecture.md` line 452 spec prose | `thread_handle.take().unwrap().join()` | CONCERN — requires guard before call |
| `jira-plugin.md` line 297-303 | `JiraError::display()` — uses clone(), no unwrap | SAFE |
| `form-modal-spec.md` line 363 | `*field_idx as u16` cursor position cast | CONCERN — if `field_idx` is a `usize` stored in enum variant and the form has 0 fields, `cursor_pos as u16` is fine but `field_value_x + *cursor_pos as u16` could overflow u16 on very wide terminals. Not a panic (u16 wraps on overflow in release mode), but could cause visual corruption. |
| `horizontal-scroll-spec.md` line 247 | `(column_height - 1) / 4` | CONCERN — if `column_height` is 0 (terminal too small), this is a u16 underflow: `0u16 - 1` overflows to `65535`, then `65535 / 4 = 16383` max_visible_cards. No panic (u16 arithmetic), but wildly wrong rendering. The fix is `.saturating_sub(1)`. |
| `horizontal-scroll-spec.md` line 49 | `max_fit.min(total_cols).max(1)` | SAFE — always at least 1 |
| `jira-plugin.md` line 382 | `inner.y + i as u16` in render_form | CONCERN — if `i` is large (many fields) and `inner.y` is near u16::MAX, addition overflows. Pathological case only. `.saturating_add()` would be safer. |

The most actionable panic risks:
1. `JiraErrorResponse` missing `Default` derive — will fail to compile if `.unwrap_or_default()` is used.
2. `thread_handle.take().unwrap()` without an `is_some()` guard — panic if called when None.
3. `column_height - 1` u16 underflow — wrong result, not panic (release mode), but corrupted rendering when terminal is tiny.

---

### 9. Form Modal State Machine — CONCERN

`form-modal-spec.md` has two issues that will produce incorrect behavior or confusing compile errors:

**9a. FormMode disambiguation (v2 B1, STILL OPEN).** The form is used for both issue creation (submit with `S`) and transition fields (submit with `Enter`). `FormState` has no field to distinguish these modes. An agent implementing `handle_key()` on the form will need to add either:
- A `mode: FormMode` field to `FormState`, or
- A `FormMode` parameter to the key handler

The spec says "Submit key is `Enter` (not `S`) — there's usually only 1-2 fields" in the transition section, but `FormState` as defined has no way to encode this. **CONCERN — agents will implement this differently, or will use the same `S` key for both forms, which is wrong UX.**

**9b. Form height formula inconsistency.** Spec prose says height is `field_count + 6` with the breakdown `2 border + 1 blank top + fields + 1 blank bottom + 1 footer`. The code example on line 345 says `centered_rect(60, field_count + 6, area)`. These are consistent with each other. However the "Sizing and Positioning" section says `field_count + 6`. An older part of the spec body (not present in the current version) mentioned `+4` — v2 flagged this. Re-reading the current spec, I only see `+6`. **RESOLVED — current spec is consistent at `+6`.**

**9c. Footer positioning.** The `render_form` pseudocode shows:
```rust
let footer_area = Rect { y: inner.y + inner.height - 1, height: 1, ..inner };
```
`inner` is already the content area inside the block border. `inner.y + inner.height - 1` is the last row of the inner area — correct for a footer inside the block. This is consistent with how ratatui blocks work. **PASS — footer positioning is correct.**

---

### 10. Cross-Document Consistency — CONCERNS

**10a. `DetailFocus` enum missing from `JiraModal`.** `jira-plugin.md` lines 570-575 defines `DetailFocus { Fields, Comments }` and line 569 says it is a field inside `JiraModal::IssueDetail`. But the `JiraModal` enum definition at lines 354-384 does NOT include `focus: DetailFocus` in the `IssueDetail` variant. An agent following the struct definition will omit `focus`, then fail to compile when the rendering pseudocode references `if focus == DetailFocus::Fields`. **CONCERN — missing field in the JiraModal enum.**

**10b. `render_detail_modal` type signature has a typo.** Line 618 shows:
```rust
fn render_detail_modal(&self, frame: &mut Frame, area: Rect, modal: &JiraModa) {
```
`&JiraModa` is missing the final `l` — it should be `&JiraModal`. Agents copying this signature literally will get `error[E0412]: cannot find type JiraModa`. **CONCERN — typo in a function signature in spec code.**

**10c. `scroll_offset` in detail modal rendering does not account for skip offset.** The pseudocode at lines 632-633 does:
```rust
for (i, field_row) in all_field_rows.iter().enumerate().skip(scroll_offset) {
```
`enumerate()` before `skip()` means `i` still starts at 0 for the first visible row — but `is_selected` uses `i == field_cursor`. If `scroll_offset > 0`, a field at `field_cursor = 5` with `scroll_offset = 3` will have `i = 2` when rendered (because `enumerate` counts from 0 in the iterator chain, but `skip` removes items before `enumerate` can count them). The correct call is `skip(scroll_offset).enumerate()` so that `i` matches the actual field index. **CONCERN — off-by-scroll_offset bug in selection highlighting.**

Actually: re-reading the pseudocode, `enumerate()` is called on `iter()` first, producing `(original_index, item)`. Then `.skip(scroll_offset)` drops the first `scroll_offset` `(index, item)` pairs. This means `i` in the loop body IS the original field index, so `i == field_cursor` is correct. This is actually sound. **WITHDRAW — rendering pseudocode is correct.**

---

## Consolidated Issue List

### FAIL — Will cause compile error

| # | Issue | Location | Fix |
|---|-------|----------|-----|
| F1 | `theme::selection()` does not exist; function is `theme::selected()` | `horizontal-scroll-spec.md` "Selected Card" section | Change `theme::selection()` to `theme::selected()` |
| F2 | `ureq::agent()` is ureq v2 API; does not compile against ureq v3 | `plugin-architecture.md` background thread example | Change to `ureq::Agent::new()` |
| F3 | `&mut self.modal` + `self.handle_modal_key(key, modal)` — two simultaneous `&mut self` borrows | `plugin-architecture.md` modal routing pattern | Use `self.modal.take()` before calling the helper, replace afterward |
| F4 | `JiraErrorResponse` lacks `Default` derive; `.unwrap_or_default()` in error handling requires `T: Default` | `jira-api-reference.md` line 67 | Add `#[derive(Default)]` to `JiraErrorResponse` |
| F5 | `jira-plugin.md` lines 434-443 shows a stale `PluginAction` definition without `LaunchEditor`; agents who copy it create a duplicate type definition with wrong variants | `jira-plugin.md` "PluginAction Return Type" section | Remove the entire stale `PluginAction` enum block from jira-plugin.md; refer to `plugins/mod.rs` |
| F6 | `JiraModa` typo in `render_detail_modal` signature (missing final `l`) | `jira-plugin.md` line 618 | Change `&JiraModa` to `&JiraModal` |

### CONCERN — Will cause runtime panic on well-formed data or pathological input

| # | Issue | Location | Risk |
|---|-------|----------|------|
| P1 | `thread_handle.take().unwrap()` in prose for `on_enter()` — panics if None | `jira-plugin.md` line 452 | Show the `if thread_handle.is_some()` guard in code, not prose |
| P2 | `column_height - 1` u16 subtraction on terminal height — underflows when height = 0 | `horizontal-scroll-spec.md` line 247 | Use `column_height.saturating_sub(1)` |

### CONCERN — Design gap causing divergent or incorrect implementations

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| D1 | `serde_yml` missing from `jm-tui/Cargo.toml`; JIRA config deserialization via `serde_yml::from_value()` will fail to compile from `jm-tui` code | `Cargo.toml` + plugin-architecture.md config section | Add `serde_yml` to `jm-tui/Cargo.toml` or expose a helper from `jm-core` |
| D2 | `key_hints()` returns `Vec<(&'static str, &'static str)>` in code but spec shows `Vec<(&str, &str)>` — format strings fail to compile | `plugin-architecture.md` vs `plugins/mod.rs` | Fix spec to show `'static` lifetime |
| D3 | `FormState` has no `mode` field to distinguish S-submits (creation) vs Enter-submits (transition) | `form-modal-spec.md` | Add `mode: FormMode` to `FormState` or pass it as a parameter |
| D4 | `FieldType` enum has no `#[derive]` attributes — needs at minimum `Clone` and `PartialEq` for use in `JiraModal` and matching | `jira-plugin.md` `FieldType` definition | Add `#[derive(Debug, Clone, PartialEq)]` |
| D5 | `JiraModal::IssueDetail` missing `focus: DetailFocus` field compared to "Detail Modal Rendering" section which uses it | `jira-plugin.md` lines 354-384 vs 569-575 | Add `focus: DetailFocus` to the `IssueDetail` variant |
| D6 | Two functions named `text_to_adf` with different behavior (single paragraph vs multi-paragraph split on `\n\n`) — agents will use the wrong one | `jira-plugin.md` line 407 vs `jira-api-reference.md` line 654 | Rename one: `text_to_adf_single()` and `text_to_adf_paragraphs()` |
| D7 | No `Default` derive shown for `JiraErrorResponse` (needed for `.unwrap_or_default()`) | `jira-api-reference.md` error response struct | Add `#[derive(Default)]` to struct definition |

### CONFIRMED FIXED from v2 (no longer blocking)

| # | Issue | Status |
|---|-------|--------|
| v2-F1 | `theme::selection()` → `theme::selected()` | STILL OPEN (re-listed as F1) |
| v2-F2 | `ureq::agent()` v2 API | STILL OPEN (re-listed as F2) |
| v2-F3 | Modal routing `&mut self` double-borrow | STILL OPEN (re-listed as F3) |
| v2-P1 | Sprint `id: String` vs integer | RESOLVED — `extract_sprint_name()` uses `serde_json::Value` |
| v2-P2 | ADF `attrs` null access | RESOLVED — CRITICAL warning + null-safe code in spec |
| v2-B2 | `PRIORITY_HIGHEST`/`PRIORITY_LOWEST` constants | FALSE POSITIVE — spec uses `PRIORITY_HIGH`/`PRIORITY_LOW` which exist |
| v2-B4 | Form height `+4` vs `+6` | RESOLVED — current spec consistently uses `+6` |
| v2-B5 | Footer positioned at `form_area` vs `inner` | RESOLVED — current spec uses `inner` correctly |

---

## Verdict by Area

| Area | Verdict | Key Finding |
|------|---------|-------------|
| Cargo.toml | **CONCERN** | `serde_yml` missing from `jm-tui`; needed for JIRA config deserialization |
| Trait signatures | **CONCERN** | Stale `PluginAction` redefinition in jira-plugin.md; `'static` lifetime mismatch in spec |
| PluginAction completeness | **PASS** | All four variants; all wired in app.rs |
| Type compatibility | **CONCERN** | `FieldType` missing derives; `FormState` needs `HashSet` import; `DetailFocus` missing from `JiraModal` |
| serde deserialization | **CONCERN** | `JiraErrorResponse` missing `Default` derive for `.unwrap_or_default()` |
| Borrow checker patterns | **FAIL** | Modal routing pattern is unsound as written — double `&mut self` borrow |
| Thread safety | **PASS** | Channel + AtomicBool + JoinHandle usage is correct |
| Panics | **CONCERN** | `JoinHandle.take().unwrap()` in prose without guard; u16 underflow in column_height |
| Form modal | **CONCERN** | No `FormMode` to distinguish submit keys; `DetailFocus` missing from struct |
| Cross-document | **FAIL** | `theme::selection()` nonexistent; `ureq::agent()` v2 API; `JiraModa` typo; stale `PluginAction` |

---

## Final Verdict: REJECT

Three items will cause compile errors with certainty in any agent implementation following the specs:

1. **F1** — `theme::selection()` in `horizontal-scroll-spec.md` does not exist. Any agent implementing the kanban board selected-card rendering will get `error[E0425]`.

2. **F2** — `ureq::agent()` in `plugin-architecture.md` is the ureq v2 API. Any agent using the background thread template from this doc gets `error[E0425]`.

3. **F3** — The modal routing pattern (`&mut self.modal` then `self.handle_modal_key(key, modal)`) is a double-mutable-borrow that will not compile. This was in the v2 FAIL list and is unchanged.

Additionally, two new compile-blocking issues were found in this review:

4. **F4** — `JiraErrorResponse` is used with `.unwrap_or_default()` but has no `Default` derive.

5. **F5** — Stale `PluginAction` block in `jira-plugin.md` is missing `LaunchEditor` and will cause a conflict if copied.

6. **F6** — `&JiraModa` typo in the detail modal render function signature.

### Minimum fixes required to APPROVE

1. `horizontal-scroll-spec.md` "Selected Card": change `theme::selection()` → `theme::selected()`.
2. `plugin-architecture.md` background thread example: change `ureq::agent()` → `ureq::Agent::new()`.
3. `plugin-architecture.md` modal routing: replace the double-`&mut self` pattern with an explicit `take()`/replace or free-function pattern.
4. `jira-api-reference.md` `JiraErrorResponse` struct: add `#[derive(Default)]`.
5. `jira-plugin.md` "PluginAction Return Type" section: remove the stale 3-variant enum block entirely; replace with a reference to `plugins/mod.rs`.
6. `jira-plugin.md` line 618: fix `&JiraModa` → `&JiraModal`.
7. `jira-plugin.md` `JiraModal::IssueDetail` variant: add `focus: DetailFocus` field.
8. `jira-plugin.md` `FieldType` enum: add `#[derive(Debug, Clone, PartialEq)]`.
9. `jm-tui/Cargo.toml`: add `serde_yml = "0.9"` (or whichever version matches `jm-core`), OR move JIRA config deserialization into `jm-core`.
10. `form-modal-spec.md`: add `mode: FormMode` field to `FormState`, or explicitly document that the form is parameterized by mode at call sites.

Items D6 (rename `text_to_adf` variants) and P1/P2 (`.unwrap()` guards, u16 saturating_sub) are important quality fixes that will prevent runtime confusion or subtle bugs, but do not individually block compilation in the happy path.
