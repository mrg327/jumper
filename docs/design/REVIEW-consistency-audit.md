# Consistency Audit Report

**Auditor role**: Consistency auditor — cross-document verification and gap synthesis
**Date**: 2026-03-27
**Scope**: All six design specs, five expert reviews, and implemented Phase 0 code

---

## Executive Summary

The four other review agents all reached REJECT verdicts on the Phase 1 specs. This audit independently verifies the current state of every flagged issue, checks cross-document consistency, and identifies any gaps the reviewers missed.

**Cargo check result**: PASS (1 warning, 0 errors — see Section 4)

**Overall assessment**: The specs are NOT ready for agent implementation in their current state. Three issues are hard blockers that will cause compile failures or fundamental runtime breakage. Twelve more issues are significant enough to cause incorrect behavior. The Phase 0 code is solid and correct; all problems are in the Phase 1 design documents.

---

## Section 1: Status of All 13 Reviewer Issues

The five reviews collectively identified these distinct issues. The following table shows their current status based on the current state of the docs and code.

---

### Issue 1 — `PluginAction::LaunchEditor` missing from the enum

**Reviewer**: REVIEW-tui-ux (Gap #1, Critical), REVIEW-architecture (GAP-2), REVIEW-agent-implementability (GAP-1)

**Current state in code**: `plugins/mod.rs` lines 77–78 already contain:

```rust
LaunchEditor { content: String, context: String },
```

`ScreenPlugin` has an `on_editor_complete(&mut self, _content: String, _context: &str)` default method at line 124. The `app.rs` struct has `pending_editor_plugin: Option<(String, String, std::path::PathBuf)>` at line 84.

**Status: PARTIALLY FIXED**

The `PluginAction::LaunchEditor` variant exists in the code with `content` and `context` fields. However, the design documents (`plugin-architecture.md`, `plugin-system-rewrite.md`) still show only three variants (`None`, `Back`, `Toast`). The docs are out of date relative to the implemented code. Agents reading only the docs will not know about this variant or the `on_editor_complete` callback.

Remaining gap: No document describes the `App`-side handling of `LaunchEditor` — how `app.rs` suspends the TUI, writes the temp file, launches `$EDITOR`, reads the result back, and calls `on_editor_complete`. This is the most critical missing piece for Phase 1c.

---

### Issue 2 — `JiraPlugin` struct fields not defined

**Reviewer**: REVIEW-rust-systems (Critical Gap #1)

**Current state**: `jira-plugin.md` lines 319–353 contain a complete `JiraPlugin` struct definition with all field types.

**Status: FIXED**

The struct is fully defined with typed fields in `jira-plugin.md`. Fields include `config`, `account_id`, `command_tx`, `result_rx`, `shutdown_flag`, `thread_handle`, `issues`, `field_defs`, `story_points_field`, `sprint_field`, `board`, `project_filter`, `show_done`, `modal`, `loading`, `refreshing`, `generation`, `last_sync`, and `last_error`.

---

### Issue 3 — `JiraError` type never defined

**Reviewer**: REVIEW-rust-systems (Critical Gap #2)

**Current state**: `jira-plugin.md` lines 288–304 define:

```rust
pub struct JiraError {
    pub status_code: u16,
    pub error_messages: Vec<String>,
    pub field_errors: HashMap<String, String>,
}
```

**Status: FIXED**

The type is defined with `display()` helper included. Both `api.rs` (which constructs errors) and `mod.rs` (which receives them in `JiraResult`) have a consistent type to work from.

---

### Issue 4 — `serde` derive feature missing from Cargo.toml

**Reviewer**: REVIEW-rust-systems (Critical Gap #3)

**Current state**: `jm-tui/Cargo.toml` contains only: `jm-core`, `anyhow`, `chrono`, `clap`, `crossterm`, `ratatui`, `regex`. No `serde`, no `serde_json`, no `ureq`, no `base64`.

**Status: STILL OPEN**

None of the JIRA dependencies have been added to `Cargo.toml`. When Phase 1 begins, adding `serde = { version = "1", features = ["derive"] }`, `serde_json`, `ureq`, and `base64` is required. The `jira-api-reference.md` lists these dependencies in its authentication section, but they are absent from the actual manifest.

---

### Issue 5 — ureq v3 API mismatch in code examples

**Reviewer**: REVIEW-rust-systems (Critical Gap #4), REVIEW-agent-implementability (GAP-3)

**Current state**: `jira-api-reference.md` Authentication section (lines 19–71) shows the correct ureq v3 pattern including:
- `ureq::Agent::new()` or `ureq::AgentBuilder::new().build()` (not `ureq::agent()`)
- `.call()?` returns `Ok(Response)` for all HTTP responses including 4xx/5xx
- Explicit `response.status()` check with match arms for 200..=299, 429, and other statuses
- `response.into_body().read_json()` for body reading

The note explicitly says: "`ureq::Agent::new()` or `ureq::AgentBuilder::new().build()` (NOT `ureq::agent()` — that is the v2 API)"

**Status: FIXED**

The API reference is correct. The `plugin-architecture.md` background thread example still uses the v2 `ureq::agent()` call, but that is a docs-level inconsistency in a secondary reference document. The authoritative `jira-api-reference.md` is correct.

Remaining inconsistency: `plugin-architecture.md` line 461 still shows `let client = ureq::agent();` — a v2 API call. This will produce a compile error if copied. Minor but should be noted.

---

### Issue 6 — Dynamic `fields` map deserialization in search response

**Reviewer**: REVIEW-rust-systems (High Gap #5)

**Current state**: `jira-api-reference.md` addresses this in the `/search` endpoint section. The field extraction rules table (lines 271–293) documents which JSON paths to use. The sprint field is explicitly documented as a `serde_json::Value` that may be an array and must be handled dynamically. The story points field uses a bare `f64`.

**Status: PARTIALLY FIXED**

The extraction logic is documented. However, no explicit Rust struct is given showing how to deserialize the `fields` object when story_points and sprint field IDs are dynamic. An agent must decide between using `serde_json::Value` for the whole response and extracting by path, or using `#[serde(flatten)]` with a remainder map. Neither pattern is shown explicitly for the `issues[*].fields` object. The `jira-api-reference.md` agent reviewer's recommendation (REVIEW-api-integration item 3) to use `serde_json::Value` for sprint with fallback handling is documented in the review but not incorporated back into the spec.

---

### Issue 7 — ureq v3 error handling pattern for 4xx/5xx

**Reviewer**: REVIEW-rust-systems (High Gap #6), REVIEW-agent-implementability (GAP-3)

**Current state**: `jira-api-reference.md` Auth section lines 40–70 show the complete pattern including the `match response.status()` block, the 429 branch reading `Retry-After`, and the non-2xx branch.

**Status: FIXED**

The correct ureq v3 error handling is documented. 4xx/5xx are returned as `Ok(Response)` and must be checked with `response.status()`. The auth section is the authoritative code example and is correct.

---

### Issue 8 — Config integration path broken (`PluginConfig.extra` missing)

**Reviewer**: REVIEW-architecture (GAP-1/Critical), REVIEW-rust-systems (High Gap #7)

**Current state**: `config.rs` in `jm-core` has:
```rust
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub notifications: NotificationsConfig,
    pub pomodoro: PomodoroConfig,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_yml::Value>,
}
```

**Status: FIXED**

The `extra: HashMap<String, serde_yml::Value>` field IS present in `config.rs` with `#[serde(flatten, default)]`. The architecture doc description matches the implementation. JIRA config can be extracted via `config.plugins.extra.get("jira")`. The `PluginRegistry::new()` still has no arm for `"jira"` (correct — that is Phase 1 work), but the mechanism is in place.

---

### Issue 9 — `AllowedValue` struct: names vs IDs in `EditableField`

**Reviewer**: REVIEW-agent-implementability (GAP-2)

**Current state**: `jira-plugin.md` lines 223–229 define:
```rust
pub struct AllowedValue {
    pub id: String,
    pub name: String,
}
```

`EditableField.allowed_values` is `Option<Vec<AllowedValue>>` (line 261), not `Option<Vec<String>>`. The `jira-api-reference.md` endpoint 4 (transitions) also defines `AllowedValue { id, name }`.

**Status: FIXED**

`AllowedValue` stores both `id` and `name`. The correct POST body format (`{ "id": "..." }`) is documented in the Common Field Value Formats table. The form-modal-spec.md mentions `AllowedValue.id` for POST bodies.

---

### Issue 10 — `$EDITOR` integration mechanism for screen plugins (app-side handler)

**Reviewer**: REVIEW-tui-ux (Gap #1), REVIEW-architecture (GAP-2)

**Current state**: As documented in Issue 1 above, `PluginAction::LaunchEditor { content, context }` exists in `mod.rs` and `on_editor_complete` is on the `ScreenPlugin` trait. The App struct has `pending_editor_plugin: Option<(String, String, PathBuf)>`.

**Status: STILL OPEN (partially)**

The `PluginAction::LaunchEditor` variant exists in the code but is not documented anywhere in the spec files. No document describes:
- How `app.rs` handles `PluginAction::LaunchEditor` (suspend terminal, write temp file, launch editor, restore, call `on_editor_complete`)
- What the `content` and `context` fields mean (initial content and callback tag)
- The exact flow in `App::run()`

The cargo check warning confirms this: `Toast` and `LaunchEditor` variants are "never constructed" — meaning the app-side handling code has not been written yet. The code stub exists but is unconnected to actual editor launch logic.

---

### Issue 11 — `theme::selection()` does not exist (should be `theme::selected()`)

**Reviewer**: REVIEW-tui-ux (Critical Gap #2)

**Current state**: `theme.rs` exports `pub fn selected() -> Style`. There is no `selection()` function.

The `horizontal-scroll-spec.md` "Selected Card" section says: "Inverted/highlighted background (`theme::selection()`)" — this is wrong.

**Status: STILL OPEN**

`horizontal-scroll-spec.md` still references `theme::selection()` which does not exist. Agents copying this code will get a compile error. The correct name is `theme::selected()`.

---

### Issue 12 — `theme::error()` does not exist as a function

**Reviewer**: REVIEW-tui-ux (Critical Gap #3)

**Current state**: `theme.rs` exports `pub const TEXT_ERROR: Color = Color::Red;` (a constant, not a function). There is no `theme::error()` function.

**Status: STILL OPEN**

`form-modal-spec.md` references form field indicators using "Red (`theme::TEXT_ERROR`)" which is correct usage of the constant. However, the field indicator table says `theme::TEXT_ERROR` (the constant) in some places and the prose says to use `Style::default().fg(theme::TEXT_ERROR)`. This is correct. No instance of `theme::error()` as a function call was found in `form-modal-spec.md`. The REVIEW-tui-ux reviewer flagged this based on an earlier version; the current spec appears to use the correct constant form.

**Partially resolved** — verify: `form-modal-spec.md` line 96 says `theme::TEXT_ERROR` (correct constant). No `theme::error()` function call appears in the current `form-modal-spec.md` text. This may have been fixed before the reviews were written against the final spec.

---

### Issue 13 — `centered_rect` API mismatch (pixel vs percentage)

**Reviewer**: REVIEW-tui-ux (Critical Gap #4)

**Current state**: `form-modal-spec.md` lines 316–320 define:
```rust
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    ...
}
```

This is a NEW pixel-absolute helper, not the existing percentage-based helper from `modals/mod.rs`.

**Status: PARTIALLY FIXED**

The spec defines its own pixel-absolute `centered_rect` with a clear signature. However, it does not explicitly say "this is a new function — do NOT use the existing `crate::modals::centered_rect()`." An agent familiar with the codebase may accidentally call the existing percentage-based version with pixel arguments, producing wrong results. The spec should explicitly say: "This is a new helper. Do not use `crate::modals::centered_rect()` which takes percentages."

---

## Section 2: Cross-Document Consistency Check

### 2.1 Trait Signatures: plugin-architecture.md vs mod.rs

| Method | plugin-architecture.md | mod.rs (actual) | Match? |
|--------|------------------------|-----------------|--------|
| `SidebarPlugin::render` | `fn render(&self, area: Rect, buf: &mut Buffer)` | same | YES |
| `SidebarPlugin::on_tick` | `fn on_tick(&mut self) -> Vec<String>` | same | YES |
| `SidebarPlugin::on_key` | `fn on_key(&mut self, key: KeyEvent) -> bool` | same | YES |
| `ScreenPlugin::render` | `fn render(&self, frame: &mut Frame, area: Rect)` | same | YES |
| `ScreenPlugin::handle_key` | `fn handle_key(&mut self, key: KeyEvent) -> PluginAction` | same | YES |
| `ScreenPlugin::key_hints` | `fn key_hints(&self) -> Vec<(&str, &str)>` | `Vec<(&'static str, &'static str)>` | NO — lifetime mismatch |
| `ScreenPlugin::on_notify` | present in architecture.md | ABSENT from mod.rs | INCONSISTENCY |
| `PluginAction` variants | `None, Back, Toast(String)` | `None, Back, Toast(String), LaunchEditor { content, context }` | INCONSISTENCY — LaunchEditor is in code but not in docs |

**Summary**: Two inconsistencies found:
1. `plugin-architecture.md` shows `key_hints()` without `'static` lifetime; the implementation and `plugin-system-rewrite.md` correctly use `'static`. The architecture doc is stale.
2. `plugin-architecture.md` shows `on_notify` on `ScreenPlugin`; the implementation omits it (it has a default impl in the architecture doc, so absence is backwards-compatible).
3. `PluginAction::LaunchEditor` exists in the code but is absent from all design docs.

### 2.2 Data Types: jira-plugin.md vs jira-api-reference.md

| Type/Field | jira-plugin.md | jira-api-reference.md | Consistent? |
|------------|----------------|----------------------|-------------|
| `JiraTransition.to_status` | `to_status: JiraStatus` | endpoint 4 uses `to_status` in the mapping struct | YES |
| `TransitionField` struct | defined in jira-plugin.md with `field_id`, `name`, `field_type`, `allowed_values`, `is_comment` | endpoint 4 shows `field_id`, `name`, `allowed_values` — no `field_type`, no `is_comment` | PARTIAL MISMATCH |
| `AllowedValue` | `{ id: String, name: String }` | `{ id: String, name: String }` | YES |
| `StatusCategory` | `ToDo, InProgress, Done` with mapping | same mapping | YES |
| Sprint field format | `Option<String>` sprint name | array of objects with `state` field | CONSISTENT — field extraction table explains conversion |
| `JiraIssue.epic` | `Option<EpicInfo>` | uses `parent` field check | CONSISTENT — both docs agree |
| `JiraFieldDef` | defined in jira-plugin.md | defined in api-reference endpoint 3 | MINOR INCONSISTENCY — jira-plugin.md has `custom: bool`; api-reference struct has `schema: Option<FieldSchema>` as additional field; these are compatible but the api-reference version is the richer one |

**Summary**: One notable inconsistency: `TransitionField` in `jira-plugin.md` has `field_type: FieldType` and `is_comment: bool`, but the `jira-api-reference.md` endpoint 4 mapping struct does not include these fields. The `jira-api-reference.md` struct is what will be deserialized from the wire format; `field_type` must be derived from `schema.type`, and `is_comment` is a special case documented in prose but not in the struct. An agent must reconcile these.

### 2.3 Keybindings: jira-plugin.md vs form-modal-spec.md vs horizontal-scroll-spec.md

| Key | jira-plugin.md board | horizontal-scroll-spec.md board | Consistent? |
|-----|---------------------|--------------------------------|-------------|
| `h`/`l` | nav columns | nav columns | YES |
| `j`/`k` | nav issues | nav issues | YES |
| `Enter` | open detail | open detail | YES |
| `s` | transition | transition | YES |
| `c` | comment | comment | YES |
| `n` | new issue | new issue | YES |
| `p` | cycle project | cycle project | YES |
| `D` | toggle done | toggle done | YES |
| `R` | refresh | refresh | YES |
| `g`/`G` | **NOT LISTED** | listed | INCONSISTENCY |
| `Esc`/`q` | back | `Esc`/`q` back | YES |

**`g`/`G` gap confirmed**: `jira-plugin.md` kanban board keybinding table omits `g`/`G` while `horizontal-scroll-spec.md` lists them.

| Key | form-modal-spec.md | jira-plugin.md transition form | Consistent? |
|-----|-------------------|-------------------------------|-------------|
| `S` | submit | **NOT `S` — `Enter` submits** | INCONSISTENCY |
| `Enter` (navigate) | enter edit mode | submit | INCONSISTENCY |

**Submit key conflict confirmed**: For creation form, `S` submits and `Enter` edits. For transition fields form, `Enter` submits. The `FormState` state machine does not have a mode flag for this.

### 2.4 Theme References: specs vs theme.rs

| Referenced in specs | Actual in theme.rs | Status |
|--------------------|-------------------|--------|
| `theme::selected()` | exists | OK |
| `theme::selection()` (horizontal-scroll-spec.md) | does NOT exist | BUG — compile error |
| `theme::dim()` | exists | OK |
| `theme::accent()` | exists | OK |
| `theme::TEXT_ERROR` | exists (constant) | OK |
| `theme::TEXT_DIM` | exists (constant) | OK |
| `theme::TEXT_ACCENT` | exists (constant) | OK |
| `PRIORITY_HIGH`, `PRIORITY_MEDIUM`, `PRIORITY_LOW` | exist | OK |
| `PRIORITY_HIGHEST`, `PRIORITY_LOWEST` | do NOT exist | GAP — JIRA has 5 levels |

### 2.5 ScreenId: design docs vs events.rs

`plugin-system-rewrite.md` shows:
```rust
pub enum ScreenId { Dashboard, ProjectView, Switch, Search, IssueBoard, Weekly, Review, People, Plugin(String) }
```

Actual `events.rs`:
```rust
pub enum ScreenId { Dashboard, ProjectView(String), Switch(Option<String>), Review, Search, People, IssueBoard, Weekly, Plugin(String) }
```

**Inconsistency**: `plugin-system-rewrite.md` shows `ProjectView` and `Switch` without type parameters; the actual code has `ProjectView(String)` and `Switch(Option<String>)`. This is a doc staleness issue — the actual implementation is more complete and correct.

### 2.6 Registry Method Names

`plugin-architecture.md` line ~313: `tick_active_screen(active_screen: &ScreenId)`
`plugin-system-rewrite.md` and `registry.rs`: `tick_screen(name: &str)`

**Inconsistency confirmed** (noted by REVIEW-architecture): `plugin-architecture.md` uses a different method name. The implementation is self-consistent; the architecture doc is stale.

---

## Section 3: Additional Gaps Not Caught by Reviewers

### NEW GAP A — `PluginAction::LaunchEditor` has different fields from what TUI-READINESS recommended

The existing code has `LaunchEditor { content: String, context: String }` where `content` is the initial content to put in the temp file and `context` is a callback tag (e.g., `"comment:HMI-103"`). The `REVIEW-agent-implementability.md` recommended `LaunchEditor { path: PathBuf, callback_id: String }`.

The implemented variant uses `content` (the string to write to the temp file) rather than `path` (a pre-created path). This design means the app creates the temp file and manages the path — which is a better design. However, no document describes this convention. An agent implementing the app-side handler must know to: (1) create a temp file, (2) write `content` to it, (3) launch `$EDITOR` with that path, (4) read back the file after editor exits, (5) call `plugin.on_editor_complete(content, context)`.

### NEW GAP B — `PluginAction::LaunchEditor` is an unused warning, confirming no app-side handler exists

The cargo check output shows:
```
warning: variants `Toast` and `LaunchEditor` are never constructed
```

This confirms that while `LaunchEditor` is defined in `mod.rs`, nothing in the current codebase constructs it. The `app.rs` handler for `PluginAction::LaunchEditor` has not been implemented. Phase 1c will stall immediately when an agent tries to test the comment flow.

The `Toast` variant being "never constructed" is also notable — this suggests the app-side handling for `PluginAction::Toast` from plugin screens may also be missing or unused in the current wiring.

### NEW GAP C — Form height formula inconsistency between spec prose and mockup

`form-modal-spec.md` line 217: "Height: `field_count + 4`"
The mockup shows 1 blank row top, 1 blank row bottom, field rows, and 1 footer = `field_count + 3` inner rows + 2 border rows = `field_count + 5` minimum. REVIEW-tui-ux identifies this as `field_count + 6`. The spec prose says `+4`, which will produce a form 2 rows too short when the blank padding rows from the mockup are expected.

This gap remains open — no fix has been applied to the spec.

### NEW GAP D — `render_form` snippet has footer positioned outside inner area

`form-modal-spec.md` lines 286–288:
```rust
let footer_area = Rect { y: form_area.y + form_area.height - 1, height: 1, ..form_area };
```
`form_area` includes the border rows. The inner content area starts at `form_area.y + 1` (top border). The last row of `form_area` is `form_area.y + form_area.height - 1`, which is the BOTTOM BORDER row. Rendering into the border row will overdraw the border. The correct position is `inner.y + inner.height - 1`. REVIEW-tui-ux flagged this (issue 18); it remains unfixed in the spec.

### NEW GAP E — `JiraModal` enum in jira-plugin.md differs from plugin-architecture.md example

`plugin-architecture.md` (example snippet, lines 174–179):
```rust
enum JiraModal {
    IssueDetail(Issue),
    CreateIssue,
    ConfirmTransition(Issue, String),
}
```

`jira-plugin.md` (the definitive struct, lines 354–383):
```rust
enum JiraModal {
    IssueDetail { issue_key, fields, transitions, comments, scroll_offset, field_cursor },
    TransitionPicker { issue_key, transitions, cursor },
    TransitionFields { issue_key, transition, form },
    CreateForm { project_key, issue_type_id, fields, form },
    ErrorModal { title, message },
}
```

The architecture doc shows a simplified/stale example that does not match the full definition in jira-plugin.md. Agents should use the jira-plugin.md version as authoritative. The architecture doc example is illustrative only but may confuse agents.

### NEW GAP F — No `TextArea` field type in `FieldType` enum for form handling

`jira-plugin.md` `FieldType` enum (lines 268–279): `Text`, `TextArea`, `Number`, `Select`, `MultiSelect`, `Date`, `Unsupported`.

`form-modal-spec.md` field types table (lines 82–88): `Text`, `Number`, `Select`, `MultiSelect`, `Unsupported`.

**`TextArea` is listed in `jira-plugin.md`'s `FieldType` enum but is absent from `form-modal-spec.md`'s field type table.** The form spec does not describe how `TextArea` fields behave in the form (should open `$EDITOR`). An agent implementing the form against `form-modal-spec.md` alone will not handle `TextArea` fields correctly.

### NEW GAP G — `Date` field type also missing from form-modal-spec.md

Same as above: `FieldType::Date` exists in `jira-plugin.md` but has no row in the `form-modal-spec.md` field types table. How to render and edit a date field is completely unspecified.

### NEW GAP H — Pagination termination condition is inconsistently stated

`jira-api-reference.md` Pagination Pattern section (lines 90–91):
> loop while `!page.is_empty() && page.len() >= max_results`

`jira-api-reference.md` `/search` endpoint section (line 294):
> **Pagination**: Loop while `!issues.is_empty() && issues.len() >= max_results`

These two statements are consistent with each other but REVIEW-api-integration noted they differ from an earlier version. Checking the current document: both sections use the same correct loop condition. **This gap appears to have been fixed**.

### NEW GAP I — `FormState` has no field for tracking per-field values

`form-modal-spec.md` `FormState` enum:
```rust
enum FormState {
    Navigating { cursor: usize },
    EditingText { cursor: usize, buffer: String, cursor_pos: usize },
    SelectOpen { field_cursor: usize, dropdown_cursor: usize },
    Submitting,
    ValidationError { cursor: usize, errors: HashMap<String, String> },
}
```

`FormState` tracks cursor position and editing state but does NOT contain the actual field values. Where are the field values stored? The `JiraModal::CreateForm` variant has `form: FormState`, but `FormState` itself has no `values: Vec<Option<FieldValue>>`. The prose mentions "FormState tracks all values: `Vec<(EditableField, Option<String>)`" but this is not in the enum definition.

An agent must infer that `FormState` is used alongside a parallel `Vec<(EditableField, Option<FieldValue>)>` in the modal variant, but this architecture is never made explicit. The `JiraModal::TransitionFields` and `JiraModal::CreateForm` variants contain `form: FormState` but this by itself does not hold field values.

### NEW GAP J — `MultiSelect` field type: toggle UI completely unspecified

`form-modal-spec.md` lists `MultiSelect` with "Inline dropdown with toggle" in the edit mode column, but there is no description of the toggle interaction, how checked/unchecked state is displayed, or how multiple selected items are accumulated. REVIEW-tui-ux flagged this (issue 11). It remains unresolved.

---

## Section 4: Cargo Check Results

```
cargo check
    Checking jm-core v0.1.0 (/mnt/c/projects/job-mgmt/crates/jm-core)
    Checking jm-tui v0.1.0 (/mnt/c/projects/job-mgmt/crates/jm-tui)
warning: variants `Toast` and `LaunchEditor` are never constructed
  --> crates/jm-tui/src/plugins/mod.rs:72:5
   |
66 | pub enum PluginAction {
   |          ------------ variants in this enum
...
72 |     Toast(String),
   |     ^^^^^
...
77 |     LaunchEditor { content: String, context: String },
   |     ^^^^^^^^^^^^
   ...
warning: `jm-tui` (bin "jm") generated 1 warning
    Finished `dev` profile [unoptimized + debuginfo] target(s) in ~62s
```

**Result: 0 compile errors, 1 warning**

The warning confirms:
1. `PluginAction::LaunchEditor` is defined but never dispatched — the app-side handler does not exist yet.
2. `PluginAction::Toast` is defined but never dispatched from plugin screens — the plugin screen toast pathway in `app.rs` may not be wired.

The existing Phase 0 code is structurally sound. No errors.

---

## Section 5: Final Assessment

### Issues by Status

| # | Issue | Status | Severity |
|---|-------|--------|----------|
| 1 | `PluginAction::LaunchEditor` missing from docs | PARTIALLY FIXED — in code, not in docs | BLOCKER |
| 2 | `JiraPlugin` struct fields undefined | FIXED | — |
| 3 | `JiraError` type undefined | FIXED | — |
| 4 | `serde`/`serde_json`/`ureq`/`base64` missing from Cargo.toml | STILL OPEN | BLOCKER |
| 5 | ureq v3 API mismatch in code examples | FIXED (in jira-api-reference.md) | — |
| 6 | Dynamic fields map deserialization unspecified | PARTIALLY FIXED | HIGH |
| 7 | ureq v3 error handling pattern | FIXED | — |
| 8 | Config `PluginConfig.extra` missing | FIXED | — |
| 9 | `AllowedValue` stores names not IDs | FIXED | — |
| 10 | `$EDITOR` app-side handler unspecified | STILL OPEN | BLOCKER |
| 11 | `theme::selection()` wrong name | STILL OPEN | COMPILE ERROR |
| 12 | `theme::error()` wrong name | RESOLVED — spec uses `TEXT_ERROR` constant correctly | — |
| 13 | `centered_rect` pixel vs percentage mismatch | PARTIALLY FIXED — spec defines new function but doesn't warn to avoid old one | HIGH |

### New Gaps Found by This Audit

| # | Gap | Severity |
|---|-----|----------|
| A | `LaunchEditor` variant fields undocumented; app-side handler unimplemented (confirmed by warning) | BLOCKER |
| B | `Toast` also never constructed — plugin screen toast pathway may be unimplemented | HIGH |
| C | Form height formula `+4` vs correct `+5` or `+6` | HIGH |
| D | Footer positioned on border row in render_form snippet | HIGH |
| E | `JiraModal` enum in architecture doc is stale example, not authoritative | MEDIUM |
| F | `TextArea` field type missing from form-modal-spec.md | HIGH |
| G | `Date` field type missing from form-modal-spec.md | MEDIUM |
| H | (Previously open) Pagination condition — consistent in current docs | RESOLVED |
| I | `FormState` has no field for actual values — parallel storage architecture undocumented | HIGH |
| J | `MultiSelect` toggle UI completely unspecified | HIGH |

### Are the Docs Ready for Agent Implementation?

**NO.**

Three hard blockers remain:

1. **Cargo.toml**: Phase 1 JIRA crate dependencies (`serde` with derive, `serde_json`, `ureq = "3"`, `base64 = "0.22"`) are not in `jm-tui/Cargo.toml`. An agent starting Phase 1 will get immediate compile errors on the first `#[derive(Deserialize)]`.

2. **`$EDITOR` handler**: `PluginAction::LaunchEditor` exists in code but is unused (cargo check warning confirms this). The app-side handler in `app.rs` that suspends the TUI, writes the temp file, launches `$EDITOR`, reads the result, and calls `on_editor_complete` is not implemented and not documented. Phase 1c (comments) and Phase 1d (TextArea creation fields) are blocked.

3. **`theme::selection()`**: `horizontal-scroll-spec.md` references a function that does not exist. Any agent copying this code will get a compile error. The correct name is `theme::selected()`.

### Minimum Fixes Required Before Handoff

Listed in priority order:

1. Add to `jm-tui/Cargo.toml`:
   ```toml
   ureq = { version = "3", features = ["json"] }
   serde = { version = "1", features = ["derive"] }
   serde_json = "1.0"
   base64 = "0.22"
   ```

2. Fix `horizontal-scroll-spec.md`: replace `theme::selection()` with `theme::selected()`.

3. Add to `plugin-architecture.md` or a dedicated section in `jira-plugin.md`:
   - Document `PluginAction::LaunchEditor { content: String, context: String }`
   - Document `ScreenPlugin::on_editor_complete(&mut self, content: String, context: &str)`
   - Document the `App`-side handler: create temp file, write content, launch editor (suspending TUI), read back, call `on_editor_complete`

4. Add `g`/`G` keybindings to `jira-plugin.md` board keybinding table (already in `horizontal-scroll-spec.md`).

5. Document that `centered_rect` in `form-modal-spec.md` is a new pixel-absolute function separate from the existing `crate::modals::centered_rect()` percentage function.

6. Add `TextArea` and `Date` field types to `form-modal-spec.md` field type table (or explicitly note they should be treated as `Unsupported` for Phase 1).

7. Fix form height formula: change `field_count + 4` to `field_count + 6` (or document the correct calculation with the blank padding rows included).

8. Fix `render_form` footer positioning: change `form_area.y + form_area.height - 1` to `inner.y + inner.height - 1`.

9. Explicitly document `FormState` value storage — specify that field values are stored in a parallel `Vec<(EditableField, Option<FieldValue>)>` in the `JiraModal` variant, not in `FormState` itself.

10. Add a `FormState::transition_mode: bool` field (or separate states) to resolve the `Enter`-submits vs `Enter`-edits ambiguity between creation and transition forms.

### Items That ARE Consistent and Correct

For completeness, the following aspects passed the consistency check:

- `SidebarPlugin` trait: fully consistent between docs and code
- `ScreenPlugin` core trait (render, handle_key, on_enter, on_leave, on_tick): consistent
- `PluginRegistry` struct and methods: fully consistent
- `PluginAction::None`, `Back`, `Toast` variants: consistent across all docs and code
- `ScreenId::Plugin(String)` flat navigation: consistent
- `JiraPlugin` struct fields: consistent between spec and model
- `JiraError` struct: defined and consistent
- `AllowedValue { id, name }`: consistent across all docs
- Authentication pattern (Basic Auth, base64, `accountId` from `/myself`): consistent
- Thread lifecycle (spawn on `on_enter`, shutdown on `on_leave`, `AtomicBool`): consistent
- Channel drain pattern (`while let Ok(result) = try_recv()`): consistent
- Generation counter for stale refresh prevention: consistent
- `BoardState` struct: consistent between `horizontal-scroll-spec.md` and `jira-plugin.md`
- Horizontal scroll algorithm: internally consistent in `horizontal-scroll-spec.md`
- `FormState` state machine (ignoring value-storage and submit-key gaps): consistent
- `PluginConfig.extra: HashMap<String, serde_yml::Value>` with `#[serde(flatten)]`: implemented correctly in `config.rs`
- `J` keybinding for JIRA plugin screen: consistent across all docs
- Endpoint table: all endpoints needed for Phase 1 are listed and consistent between docs
