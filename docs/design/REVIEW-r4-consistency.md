# Cross-Document Consistency Audit — Round 4

**Date**: 2026-03-27
**Scope**: All spec docs + actual code files
**Method**: File-by-file reads plus targeted grep verification

---

## Files Audited

### Spec Documents
- `docs/design/plugin-architecture.md`
- `docs/design/jira-plugin.md`
- `docs/design/jira-api-reference.md`
- `docs/design/form-modal-spec.md`
- `docs/design/horizontal-scroll-spec.md`

### Code Files
- `crates/jm-tui/src/plugins/mod.rs`
- `crates/jm-tui/src/plugins/registry.rs`
- `crates/jm-tui/src/plugins/about.rs`
- `crates/jm-tui/src/app.rs`
- `crates/jm-tui/src/events.rs`
- `crates/jm-tui/src/theme.rs`
- `crates/jm-tui/Cargo.toml`
- `crates/jm-core/src/config.rs`

---

## Verification Checklist

### 1. PluginAction Variants

**plugin-architecture.md** (lines 134-146 and summary at 631-636):
```
None, Back, Toast(String), LaunchEditor { content: String, context: String }
```

**jira-plugin.md** (lines 470-483):
```
None, Back, Toast(String), LaunchEditor { content: String, context: String }
```

**mod.rs** (lines 66-78):
```
None, Back, Toast(String), LaunchEditor { content: String, context: String }
```

Also: `mod.rs` adds `#[derive(Debug, Clone, PartialEq, Eq)]` — not in either spec doc.

**FINDING 1-A (AUTO-FIX)**: The spec docs do not show any derives on `PluginAction`. The code has `#[derive(Debug, Clone, PartialEq, Eq)]`. No inconsistency in behavior, but spec is incomplete. The spec should document the required derives to avoid an agent omitting them. The test file in `registry.rs` exercises `Clone`, `PartialEq`, and `Debug` on `PluginAction`.

**FINDING 1-B (AUTO-FIX)**: `plugin-architecture.md` says `enum PluginAction` (line 134) but does not include any visibility modifier (`pub`). `mod.rs` correctly uses `pub enum PluginAction`. Spec should show `pub enum` to be consistent with how an agent would write it.

---

### 2. ScreenPlugin Trait Methods

**plugin-architecture.md** ScreenPlugin trait (lines 91-126 and summary at 619-629):
```
name(&self) -> &str
render(&self, frame: &mut Frame, area: Rect)
handle_key(&mut self, key: KeyEvent) -> PluginAction
on_enter(&mut self)
on_leave(&mut self)
on_tick(&mut self) -> Vec<String>     { Vec::new() }   [250ms tick]
on_notify(&mut self, _message: &str)  {}
key_hints(&self) -> Vec<(&str, &str)> { Vec::new() }
on_editor_complete(&mut self, _content: String, _context: &str) {}
```

**mod.rs** (lines 89-125):
```
name(&self) -> &str
needs_timer(&self) -> bool            { false }  ← EXTRA METHOD
on_tick(&mut self) -> Vec<String>     { Vec::new() }
render(&self, frame: &mut Frame, area: Rect)
handle_key(&mut self, key: KeyEvent) -> PluginAction
on_enter(&mut self)
on_leave(&mut self)
key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
on_editor_complete(&mut self, _content: String, _context: &str) {}
```
Note: `on_notify` is **absent** from the code.

**FINDING 2-A (NEEDS-INPUT)**: `ScreenPlugin` in `mod.rs` has a `needs_timer(&self) -> bool` method that is **not in `plugin-architecture.md`**. The spec omits it. However the registry (`registry.rs` line 103) gates `on_tick()` on `plugin.needs_timer()`, so the method is actively used. The spec must be updated to add this method.

**FINDING 2-B (AUTO-FIX)**: `ScreenPlugin` in `plugin-architecture.md` (line 117) and its summary (line 627) define `key_hints` as `Vec<(&str, &str)>` with bare (non-static) lifetime. The actual code in `mod.rs` (line 118) uses `Vec<(&'static str, &'static str)>`. The `about.rs` implementation (line 107) also uses `&'static str`. The spec should be updated to `'static` to match the code — using a non-static lifetime causes borrowing issues when returning refs from the plugin struct.

**FINDING 2-C (AUTO-FIX)**: `ScreenPlugin` in `plugin-architecture.md` (lines 117-118 and 627) includes `on_notify(&mut self, _message: &str) {}` as a default method. This method is **absent** from `mod.rs`. The code does not implement this method on `ScreenPlugin`. Two options: (a) the spec is aspirational and the method should be added to the code, or (b) the method was dropped from the design and the spec should remove it. Since `SidebarPlugin` in `mod.rs` still has `on_notify` (line 56), the omission from `ScreenPlugin` looks intentional. The spec still documents it.

---

### 3. SidebarPlugin Trait

**plugin-architecture.md** and **mod.rs** are consistent for `SidebarPlugin`. No discrepancies found.

---

### 4. PluginRegistry

**plugin-architecture.md** (line 382-393) documents a `tick_active_screen(active_screen: &ScreenId)` method.

**registry.rs** (line 101) implements `tick_screen(name: &str)` instead — takes a name string, not a `ScreenId`.

**FINDING 4-A (AUTO-FIX)**: `plugin-architecture.md` defines `tick_active_screen(&mut self, active_screen: &ScreenId)`, but the actual implementation is `tick_screen(&mut self, name: &str)`. The method name and signature differ. The code's approach (passing a `&str` name) is simpler and avoids importing `ScreenId` into the plugin module. The spec should be updated to match the code.

---

### 5. JiraModal Variants

**jira-plugin.md** (lines 369-407) defines:
```
IssueDetail { issue_key, fields, transitions, comments, scroll_offset, field_cursor, edit_state }
TransitionPicker { issue_key, transitions, cursor }
TransitionFields { issue_key, transition, fields: Vec<(EditableField, Option<FieldValue>)>, form: FormState }
SelectProject { projects: Vec<(String, String)>, cursor }
SelectIssueType { project_key, issue_types, cursor }
CreateForm { project_key, issue_type_id, fields: Vec<(EditableField, Option<FieldValue>)>, form: FormState }
ErrorModal { title, message }
```

The `IssueDetail` variant in **jira-plugin.md** line 651 adds a `focus: DetailFocus` field:
```
focus: DetailFocus,  // which section has focus
```
But the `IssueDetail` struct definition at lines 370-379 does NOT include `focus`. It appears in the rendering section's state comment block (line 651) but not in the enum variant definition.

**FINDING 5-A (AUTO-FIX)**: `JiraModal::IssueDetail` variant definition (lines 370-379) is missing the `focus: DetailFocus` field that is documented in the detail modal rendering section (line 651). The `DetailFocus` enum is defined at lines 655-658. The field must be added to the `IssueDetail` variant struct.

**FINDING 5-B (AUTO-FIX)**: `JiraPlugin` struct definition (lines 331-367) does not include the `previous_modal` field. The transition picker stacking logic (lines 778-795) requires `previous_modal: Option<Box<JiraModal>>`. The struct definition must be updated to add this field.

---

### 6. JiraCommand vs JiraResult — Matched Pairs

**JiraCommand variants** (jira-plugin.md lines 76-119):
- `FetchMyIssues { generation }` → `JiraResult::Issues { generation, issues }`  ✓
- `FetchTransitions { issue_key }` → `JiraResult::Transitions(String, Vec<JiraTransition>)`  ✓
- `TransitionIssue { issue_key, transition_id, fields }` → `JiraResult::TransitionComplete(String)` + `JiraResult::TransitionFailed(String, JiraError)`  ✓
- `UpdateField { issue_key, field_id, value }` → `JiraResult::FieldUpdated(String, String)`  ✓
- `AddComment { issue_key, body }` → `JiraResult::CommentAdded(String)`  ✓
- `FetchComments { issue_key }` → `JiraResult::Comments(String, Vec<JiraComment>)`  ✓
- `FetchCreateMeta { project_key, issue_type_id }` → `JiraResult::CreateMeta(CreateMetaResponse)`  ✓
- `CreateIssue { project_key, fields }` → `JiraResult::IssueCreated(String)`  ✓
- `FetchEditMeta { issue_key }` → `JiraResult::EditMeta(String, Vec<EditableField>)`  ✓
- `FetchFields` → `JiraResult::Fields(Vec<JiraFieldDef>)`  ✓
- `FetchIssueTypes { project_key }` → `JiraResult::IssueTypes(String, Vec<JiraIssueType>)`  ✓
- `Shutdown` → no result (cooperative signal)  ✓

**jira-api-reference.md** (lines 1136-1141) says to REMOVE two commands:
- `FetchIssue { issue_key }` — this command does NOT exist in `jira-plugin.md`. The API ref mentions removing it, but it was never present. This note is stale/misleading.
- `FetchStatus` / `GET /rest/api/3/status` — same situation: `jira-plugin.md` has no `FetchStatus` command.

**FINDING 6-A (AUTO-FIX)**: The "Endpoints to REMOVE" section at the end of `jira-api-reference.md` (lines 1136-1141) refers to removing `GET /rest/api/3/issue/{key}` and `GET /rest/api/3/status` from `JiraCommand`. Neither command exists in the current `jira-plugin.md` command enum. The removal note is stale and should be deleted to avoid confusing an implementer.

**FINDING 6-B (NEEDS-INPUT)**: `JiraResult::Error { context, error }` (line 141 in jira-plugin.md) has a `JiraError` type in the error field. The `JiraError` struct in jira-plugin.md (lines 302-316) has `status_code: u16`, `error_messages: Vec<String>`, and `field_errors: HashMap<String, String>`. However `jira-api-reference.md` defines a different `JiraError` enum with variants `RateLimited { retry_after_secs: u64 }` and `Api { status, detail: JiraErrorResponse }` (lines 62-68). These are two different type definitions with the same name. A decision is needed: should `JiraError` be a struct (as in jira-plugin.md) or an enum (as in jira-api-reference.md)?

---

### 7. FieldType Variants

**jira-plugin.md** (lines 273-284):
```
Text, TextArea, Number, Select, MultiSelect, Date, Unsupported
```

**form-modal-spec.md** (lines 103-111, table):
```
Text, Number, Select, MultiSelect, TextArea, Date, Unsupported
```
(same variants, different order — order is irrelevant in Rust enums)

**jira-api-reference.md** (lines 734-740, editmeta mapping):
```
schema.type mapping:
  "string" → Text
  "number" → Number
  "priority", "resolution", "option" (with allowedValues) → Select
  "array" with items: "string" → Text
  "user", "version", "array" with object items → Unsupported
  Anything else → Unsupported
```
Note: `MultiSelect` is NOT mapped by editmeta. The `"array"` type with object items maps to `Unsupported`, not `MultiSelect`. Components and labels are handled as `Text` (comma-separated) or `Unsupported`. The `MultiSelect` variant exists in `FieldType` but has no mapping entry in the editmeta schema type table.

**FINDING 7-A (NEEDS-INPUT)**: `FieldType::MultiSelect` is defined and used in the form modal spec, but `jira-api-reference.md` does not define when `schema.type` maps to `MultiSelect`. The editmeta table maps `"array"` with object items to `Unsupported`. There is no documented path from an editmeta field to `MultiSelect`. A decision is needed: should `"array"` with `allowedValues` map to `MultiSelect`? If so, the API reference schema mapping table needs updating.

**FINDING 7-B (AUTO-FIX)**: `TransitionField` in `jira-plugin.md` (lines 244-251) includes `field_type: FieldType`. The `TransitionField` in `jira-api-reference.md` (lines 479-483) does NOT include `field_type` — only `field_id`, `name`, `allowed_values`. This is a structural difference. The transitions API response does not separate `field_type` in the same way editmeta does. The API reference's `TransitionField` is the ground truth for deserialization; the field_type would need to be derived from the response. The spec needs to clarify whether `field_type` is present or derived.

---

### 8. Keybinding Consistency

#### Board-level keybindings

**jira-plugin.md** (lines 950-964):
- `h/l/j/k/g/G/Enter/s/c/n/p/D/R` — present
- `Esc / q` → Back to dashboard

**horizontal-scroll-spec.md** (lines 362-376):
- Same keys with same actions
- `Esc / q` → Back to dashboard

**horizontal-scroll-spec.md** footer visual (line 316):
```
│ hjkl:nav  s:transition  c:comment  Enter:detail  p:proj  R:refresh│
│ n:new  D:toggle-done  J:back           Last sync: 14:25:03       │
```

**FINDING 8-A (AUTO-FIX)**: The footer visual in `horizontal-scroll-spec.md` (line 316) shows `J:back` as a keybinding hint in the footer bar. But per `plugin-architecture.md` (line 467), `J` (uppercase) opens the JIRA plugin from the Dashboard. `J` should NOT be listed as a "back" key inside the JIRA screen — the back action is `Esc / q`. The `jira-plugin.md` keybinding table correctly does NOT include `J` as a back key. This footer visual line is wrong and will confuse the implementer.

#### Issue Detail keybindings

**jira-plugin.md** detail modal section mentions that `e` edits a field, but the detail modal rendering section (line 675) notes that when `field_cursor` moves past the last field, focus shifts to comments. Neither document specifies `Tab` as a way to switch `DetailFocus`. That is consistent.

**form-modal-spec.md** (lines 295-335) keybinding tables are internally consistent.

---

### 9. Theme References

All theme references in spec docs were grep-verified against `theme.rs`:

| Reference | Exists in theme.rs | Notes |
|-----------|-------------------|-------|
| `theme::accent()` | YES (line 112) | Returns `Style` |
| `theme::dim()` | YES (line 108) | Returns `Style` |
| `theme::selected()` | YES (line 120) | Returns `Style` |
| `theme::TEXT_ERROR` | YES (line 25) | `Color::Red` constant |
| `theme::TEXT_DIM` | YES (line 23) | `Color::DarkGray` constant |
| `theme::TEXT_ACCENT` | YES (line 24) | `Color::Cyan` constant |
| `theme::PRIORITY_HIGH` | YES (line 15) | `Color::Red` |
| `theme::PRIORITY_MEDIUM` | YES (line 16) | `Color::Yellow` |
| `theme::PRIORITY_LOW` | YES (line 17) | `Color::Blue` |

**FINDING 9-A (AUTO-FIX)**: `horizontal-scroll-spec.md` (line 214) says:
> "Low", "Lowest" → `theme::PRIORITY_LOW` (dim)

The comment "(dim)" is factually wrong. `theme::PRIORITY_LOW` is `Color::Blue`, not `DarkGray`/dim. This parenthetical will mislead an implementer into thinking `PRIORITY_LOW` renders in a dim/gray color when it actually renders blue. The comment should be removed or changed to `(blue)`.

**FINDING 9-B (NEEDS-INPUT)**: `plugin-architecture.md` (line 535) background thread example still uses `ureq::agent()` (ureq v2 API):
```rust
let client = ureq::agent();
```
The correct ureq v3 constructor is `ureq::Agent::new()`. This is documented as correct in `jira-api-reference.md` (line 30) and `Cargo.toml` specifies `ureq = "3"`. Any agent using the `plugin-architecture.md` background thread template as a copy-paste starting point will get a compile error. This issue was flagged in multiple previous reviews but has not been fixed.

---

### 10. Struct Field Consistency Across Documents

#### `JiraPlugin` struct

**jira-plugin.md** (lines 331-367) defines `JiraPlugin`. As noted in FINDING 5-B, `previous_modal` is missing. Additionally:

The `board: BoardState` field (line 348) references a `BoardState` type. `horizontal-scroll-spec.md` (lines 24-37) defines `BoardState` with:
```
columns, selected_col, scroll_offset, selected_row
```
But `horizontal-scroll-spec.md` (line 235-241) also adds `col_scroll_offsets: Vec<usize>` to `BoardState` in the vertical scroll section. These are two code blocks in the same document defining the same struct. The second definition must be treated as an extension of the first.

**FINDING 10-A (AUTO-FIX)**: `horizontal-scroll-spec.md` defines `BoardState` in two separate blocks (lines 24-37 and lines 235-241). The second block adds `col_scroll_offsets: Vec<usize>`. These should be a single, complete definition to avoid an implementer missing the second field. Merge into one definition block.

#### `TransitionField` struct

**jira-plugin.md** (lines 244-251):
```rust
pub struct TransitionField {
    pub field_id: String,
    pub name: String,
    pub field_type: FieldType,
    pub allowed_values: Vec<AllowedValue>,
    pub is_comment: bool,
}
```

**jira-api-reference.md** "Data Model Corrections" section (lines 1128-1133):
```rust
struct TransitionField {
    field_id: String,
    name: String,
    allowed_values: Vec<AllowedValue>,
    is_comment: bool,  // true if field_id == "comment"
}
```

The api-reference version is **missing `field_type: FieldType`**. This is the same issue as FINDING 7-B — a deliberate design choice vs. an oversight needs to be resolved.

#### `EditableField` struct

**jira-plugin.md** (lines 260-267):
```rust
pub field_id: String,
pub name: String,
pub field_type: FieldType,
pub required: bool,
pub allowed_values: Option<Vec<AllowedValue>>,
```

**jira-api-reference.md** editmeta struct (lines 745-751):
```rust
field_id: String,
name: String,
field_type: FieldType,
required: bool,
allowed_values: Option<Vec<AllowedValue>>,
```

These are consistent. No issue.

#### `AllowedValue` struct

**jira-plugin.md** (implied from usage): `id: String, name: String`
**jira-api-reference.md** (lines 485-488 and 1099-1103): `id: String, name: String`

Consistent. No issue.

---

### 11. PluginConfig / Config Consistency

**plugin-architecture.md** (lines 322-334) shows `PluginConfig`:
```rust
pub enabled: Vec<String>,
pub pomodoro: Option<PomodoroConfig>,
pub notifications: Option<NotificationsConfig>,
#[serde(flatten)]
pub extra: HashMap<String, serde_yml::Value>,
```

**config.rs** (lines 8-19) actual implementation:
```rust
pub enabled: Vec<String>,
pub notifications: NotificationsConfig,    // NOT Option<>
pub pomodoro: PomodoroConfig,              // NOT Option<>
#[serde(flatten, default)]
pub extra: HashMap<String, serde_yml::Value>,
```

**FINDING 11-A (AUTO-FIX)**: `plugin-architecture.md` shows `pomodoro` and `notifications` fields as `Option<PomodoroConfig>` and `Option<NotificationsConfig>`. The actual `config.rs` uses non-optional types with `#[serde(default)]` on the struct and `impl Default`. The spec is stale and should show the non-optional fields with default-derived structs, to match the code.

**FINDING 11-B (AUTO-FIX)**: `plugin-architecture.md` shows `#[serde(flatten)]` without `default`. `config.rs` uses `#[serde(flatten, default)]`. Missing `default` means if the `extra` key is absent in YAML, deserialization may fail depending on serde_yml version. The spec should show `, default`.

---

### 12. File Layout

**plugin-architecture.md** (lines 580-601) defines expected file structure including:
```
└── jira/
    ├── mod.rs
    ├── api.rs
    ├── models.rs
    ├── board.rs
    ├── detail.rs
    └── config.rs
```

This is a design doc for future implementation. The `jira/` directory does not yet exist (consistent with the feature being planned). No discrepancy — this is forward-looking documentation.

---

### 13. `on_editor_complete` Lifecycle

**plugin-architecture.md** (lines 191-198) example:
```rust
fn on_editor_complete(&mut self, content: String, context: &str) {
```

**mod.rs** (line 124):
```rust
fn on_editor_complete(&mut self, _content: String, _context: &str) {}
```

These are consistent (default no-op, parameters underscore-prefixed in the default but correct in the override pattern). No issue.

---

### 14. `jira-plugin.md` Prerequisites Reference

**jira-plugin.md** (line 9):
> Phase 0 complete: plugin system rewrite with `ScreenPlugin` trait (see `plugin-system-rewrite.md`)

The referenced `plugin-system-rewrite.md` exists at `docs/design/plugin-system-rewrite.md`. However `plugin-architecture.md` is now the canonical architecture document and supersedes `plugin-system-rewrite.md`. The reference in jira-plugin.md points to the older document.

**FINDING 14-A (AUTO-FIX)**: `jira-plugin.md` line 9 references `plugin-system-rewrite.md` as the authoritative plugin architecture document. The canonical document is now `plugin-architecture.md`. The prerequisite reference should point to `plugin-architecture.md`.

---

## Summary Table

| # | Finding | Location | Classification | Description |
|---|---------|----------|---------------|-------------|
| 1-A | Missing `#[derive]` on `PluginAction` in spec | `plugin-architecture.md` | AUTO-FIX | Add `#[derive(Debug, Clone, PartialEq, Eq)]` to the `PluginAction` enum in the spec |
| 1-B | Missing `pub` on `PluginAction` in spec | `plugin-architecture.md` lines 134, 631 | AUTO-FIX | Change `enum PluginAction` to `pub enum PluginAction` in both places |
| 2-A | `needs_timer()` missing from `ScreenPlugin` spec | `plugin-architecture.md` | NEEDS-INPUT | Code has `fn needs_timer(&self) -> bool { false }` on `ScreenPlugin`; spec omits it entirely; registry gates ticking on this method — spec must add it |
| 2-B | `key_hints` lifetime mismatch | `plugin-architecture.md` lines 120, 627 | AUTO-FIX | Spec shows `Vec<(&str, &str)>`; code and impl use `Vec<(&'static str, &'static str)>` — update spec to `'static` |
| 2-C | `on_notify` on `ScreenPlugin` in spec but not in code | `plugin-architecture.md` lines 117, 627 | NEEDS-INPUT | Decide: add `on_notify` back to the `ScreenPlugin` impl in code, or remove it from the spec |
| 4-A | `tick_active_screen` vs `tick_screen` method name | `plugin-architecture.md` line 382 vs `registry.rs` line 101 | AUTO-FIX | Spec shows `tick_active_screen(active_screen: &ScreenId)` but code implements `tick_screen(name: &str)` — update spec to match code |
| 5-A | `focus: DetailFocus` missing from `IssueDetail` variant | `jira-plugin.md` lines 370-379 | AUTO-FIX | The `IssueDetail` enum variant body is missing `focus: DetailFocus` which is documented and used in the rendering section (line 651) |
| 5-B | `previous_modal` field missing from `JiraPlugin` struct | `jira-plugin.md` lines 331-367 | AUTO-FIX | The transition stacking logic (lines 778-795) requires `previous_modal: Option<Box<JiraModal>>` but it is absent from the struct definition |
| 6-A | Stale "Endpoints to REMOVE" section | `jira-api-reference.md` lines 1136-1141 | AUTO-FIX | The commands it says to remove (`FetchIssue`, `FetchStatus`) do not exist in `jira-plugin.md`'s command enum — the note is stale and should be deleted |
| 6-B | `JiraError` is struct vs enum | `jira-plugin.md` lines 302-316 vs `jira-api-reference.md` lines 62-68 | NEEDS-INPUT | `jira-plugin.md` defines `JiraError` as a plain struct; `jira-api-reference.md` defines it as an enum with `RateLimited` and `Api` variants — one must be chosen |
| 7-A | `MultiSelect` FieldType has no API schema mapping | `jira-api-reference.md` lines 734-740 | NEEDS-INPUT | `FieldType::MultiSelect` exists in the enum but no `schema.type` maps to it in the editmeta or createmeta tables — the mapping must be added or the variant justified |
| 7-B | `TransitionField.field_type` present in jira-plugin.md but absent from jira-api-reference.md | `jira-plugin.md` line 247 vs `jira-api-reference.md` lines 479-483 | NEEDS-INPUT | One document includes `field_type: FieldType` on `TransitionField`, the other omits it — decide whether to include it |
| 8-A | `J:back` in horizontal-scroll-spec footer visual | `horizontal-scroll-spec.md` line 316 | AUTO-FIX | Footer shows `J:back` but `J` opens JIRA from the dashboard — it is not a back key inside the JIRA screen; replace with `Esc:back` |
| 9-A | `PRIORITY_LOW` comment says "(dim)" but color is Blue | `horizontal-scroll-spec.md` line 214 | AUTO-FIX | `theme::PRIORITY_LOW = Color::Blue`; the spec annotation `(dim)` is wrong — change to `(blue)` |
| 9-B | `ureq::agent()` v2 API in background thread example | `plugin-architecture.md` line 535 | AUTO-FIX | Change `let client = ureq::agent()` to `let client = ureq::Agent::new()` — the v2 constructor does not exist in ureq v3 |
| 10-A | `BoardState` defined in two separate blocks | `horizontal-scroll-spec.md` lines 24-37 and 235-241 | AUTO-FIX | Merge the two `BoardState` struct blocks into one complete definition including `col_scroll_offsets: Vec<usize>` |
| 11-A | `pomodoro`/`notifications` as `Option<T>` in spec vs non-optional in code | `plugin-architecture.md` lines 326-328 | AUTO-FIX | Change `Option<PomodoroConfig>` and `Option<NotificationsConfig>` to `PomodoroConfig` and `NotificationsConfig` with `#[serde(default)]` |
| 11-B | Missing `default` on `#[serde(flatten)]` for `extra` | `plugin-architecture.md` line 332 | AUTO-FIX | Change `#[serde(flatten)]` to `#[serde(flatten, default)]` to match `config.rs` |
| 14-A | Stale prerequisite reference to `plugin-system-rewrite.md` | `jira-plugin.md` line 9 | AUTO-FIX | Change reference from `plugin-system-rewrite.md` to `plugin-architecture.md` |

---

## Counts

- **AUTO-FIX**: 13 items (1-A, 1-B, 4-A, 5-A, 5-B, 6-A, 8-A, 9-A, 9-B, 10-A, 11-A, 11-B, 14-A)
- **NEEDS-INPUT**: 5 items (2-A, 2-B resolved to AUTO-FIX, 2-C, 6-B, 7-A, 7-B)

Revised final counts:
- **AUTO-FIX**: 14 (adding 2-B)
- **NEEDS-INPUT**: 4 (2-A, 2-C, 6-B, 7-A, 7-B = 5 items, but 2-A has a known correct answer)

Corrected:
- **AUTO-FIX**: 13 items
- **NEEDS-INPUT**: 5 items

---

## Priority Order for Fixes

The following AUTO-FIX items would directly cause compile errors or silently wrong behavior if an implementer copies from the spec:

1. **9-B** — `ureq::agent()` compile error in `plugin-architecture.md`
2. **2-B** — `key_hints` lifetime compile error if non-static lifetime is used
3. **5-A** — Missing `focus: DetailFocus` in `IssueDetail` variant — struct incomplete
4. **5-B** — Missing `previous_modal` field — transition stacking will not compile
5. **8-A** — `J:back` in footer is functionally wrong
6. **All others** — documentation quality issues, no compile impact
