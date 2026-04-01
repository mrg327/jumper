# TUI Polish Review v3

Reviewer role: TUI rendering specialist — ratatui, crossterm, terminal layout. Zero approval bias.
Single question: will this render correctly and behave correctly when an agent implements it from these specs alone?

Files reviewed: `jira-plugin.md`, `form-modal-spec.md`, `horizontal-scroll-spec.md`,
`plugin-architecture.md`, plus existing code `screens/issue_board.rs`, `modals/input.rs`,
`modals/select.rs`, `plugins/about.rs`, `theme.rs`.

Previous review: `REVIEW-v2-tui-craftsman.md` (CONDITIONAL APPROVE). This review tracks which v2 gaps were fixed.

---

## Status of v2 Required Fixes

v2 listed six minimum required changes before full APPROVE. Status of each:

| v2 Required Fix | Fixed? |
|-----------------|--------|
| Write `detail-modal-spec.md` (or add Detail Modal Rendering section) | **FIXED** — `jira-plugin.md` now has a "Detail Modal Rendering" section with `DetailFocus` enum, two-section layout (fields + comments), field navigation algorithm, comment rendering format, cursor-follows scroll algorithm, and rendering pseudocode. |
| Resolve "Or" in Selected Card (drop `>` marker, mandate `theme::selected()`) | **FIXED** — `horizontal-scroll-spec.md` now specifies the selected card renders with `theme::selected()` background. The "Or" option is gone. |
| Add per-column scroll state to the board | **FIXED** — `horizontal-scroll-spec.md` now defines `col_scroll_offsets: Vec<usize>` in `BoardState`, specifies the cursor-follows algorithm for `j`/`k`, and handles `g`/`G` and column-change preservation. |
| Add `MultiSelect` toggle state to `FormState` | **FIXED** — `form-modal-spec.md` now has a full `MultiSelectOpen` state, `checked: HashSet<usize>`, complete key table (j/k/Space/Enter/Esc), visual rendering spec (`[x]`/`[ ]` with accent/normal color), and the `Enter` → collect `AllowedValue.id` values flow. |
| Define `label_col_width` in form modal rendering | **NOT FIXED** — see below. |
| Specify transition picker Esc navigation | **NOT FIXED** — see below. |

---

## Area-by-Area Assessment

### 1. Form Modal State Machine — PASS (with one remaining defect)

The state machine is complete and correct. `MultiSelectOpen` is now a proper first-class state with `checked: HashSet<usize>`. The visual spec for `[x]`/`[ ]` is present. The `Space` key is documented. Pre-validation for required fields is specified. The `Submitting` state UI is shown.

**Remaining defect — `label_col_width` still undefined.**

The rendering pseudocode in `form-modal-spec.md` uses:
```rust
let field_value_x = inner.x + label_col_width;
```
`label_col_width` is never assigned a value or derivation. This variable controls two things simultaneously: the visual alignment of all field values (they must left-align at a consistent column), and the terminal cursor x-position when in `EditingText` state. If an agent guesses wrong, the blinking cursor is misaligned and inline editing looks broken.

The existing `input.rs` and text-based modals sidestep this by rendering a single-field layout with `Constraint::Length(4)` + `Constraint::Min(0)` splits. The form modal has a multi-field grid layout with no analog in the existing codebase, so an agent cannot copy the pattern — it must be specified.

Fix: add to the "Rendering" subsection of `form-modal-spec.md`:
```rust
// Alignment column: widest field name + 2 for prefix + 1 for colon + 1 padding
let label_col_width = fields.iter()
    .map(|(f, _)| f.name.len())
    .max()
    .unwrap_or(8)
    + 4;  // 1 prefix char + name + 1 colon + 1 space
```
Or: mandate a fixed constant `FORM_LABEL_COL: u16 = 16` and note that field names over 13 chars are truncated.

**Remaining minor — field list scroll within the form is still absent.**

`form-modal-spec.md` says "if more fields than fit, the field list scrolls internally" but `FormState::Navigating { cursor: usize }` has no scroll offset. Without a `scroll_offset` in `FormState`, the render loop will either clip silently at `inner.height` (making fields past the bottom permanently inaccessible) or render outside the modal bounds (writing past the border row). This affects JIRA projects with many create-meta fields. An agent will implement this independently and the result is unpredictable.

Fix: add `scroll_offset: usize` to `FormState::Navigating` and `FormState::ValidationError`, and specify the same cursor-follows algorithm already present in the other scroll specs.

**Minor — `ValidationError → edit field` transition unspecified.**

The state diagram says `ValidationError` behaves "same as Navigating, but error markers shown." It does not say whether entering `EditingText` on an error field clears the `!` prefix immediately (on `Enter`) or only on the next `S` submit. Both are defensible; the spec should pick one. The existing `input.rs` has no error-clearing concept, so an agent has no reference.

---

### 2. Horizontal Scroll — PASS

All three v2 concerns are resolved:
- Selected card rendering: `theme::selected()` background mandated, `>` marker option removed.
- Column vertical scroll: `col_scroll_offsets: Vec<usize>` in `BoardState`, cursor-follows algorithm for `j`/`k`, `g`/`G` behavior, column-change preservation (cursor returns to last position per column).
- Dots row `Layout` conditional: the spec still unconditionally includes `Constraint::Length(1)` for the dots row but adds "Only rendered when `total_columns > visible_columns`." This is a minor cosmetic issue — the row is wasted on wide terminals when all columns fit. Not a blocking concern.

**One new edge case — `col_scroll_offsets` length vs column index.**

`BoardState.col_scroll_offsets` is indexed by `selected_col` (the full column index, not the viewport-relative index). The spec says "Initialize all entries to 0." If columns are added dynamically from JIRA (new workflow statuses appear after a refresh), the `Vec` will be shorter than `selected_col`. The spec should say: when applying new data from a refresh, resize `col_scroll_offsets` to match the new column count (appending 0 for new columns, dropping removed ones by index).

This is a minor data-integrity issue, not a render issue. A well-written agent will notice and handle it; a less careful one will panic on out-of-bounds access.

---

### 3. Issue Detail Modal — PASS (with two implementation traps)

The v2 critical gap is resolved. The spec now has: `DetailFocus` enum, two-section layout, field navigation with `theme::selected()` background, cursor-follows `scroll_offset` algorithm, comment rendering format (author/time on one line, body wrapped, blank separator), and rendering pseudocode.

**Implementation trap — rendering pseudocode has a typo.**

The pseudocode signature in `jira-plugin.md` line 618:
```rust
fn render_detail_modal(&self, frame: &mut Frame, area: Rect, modal: &JiraModa) {
```
`JiraModa` is missing the final `l` — it should be `JiraModal`. An agent implementing from this signature verbatim will get a compile error. Minor, but this is a spec document; typos in Rust snippets cause compile errors.

**Implementation trap — scroll pseudocode doesn't account for field section scroll offset vs comment section scroll.**

The rendering pseudocode iterates field rows with `.skip(scroll_offset)`:
```rust
for (i, field_row) in all_field_rows.iter().enumerate().skip(scroll_offset) {
```
But then unconditionally renders the separator and comments without skipping. When `scroll_offset` is large enough that field rows push the separator and comments out of the field-section range, the separator and comments must also be skipped by `scroll_offset - field_row_count` rows. The pseudocode does not handle this. An agent following the pseudocode literally will render the separator at row 0 when all field rows have scrolled off screen, and then render comments on top of it.

The correct approach is to build a unified flat list of all rows (field rows + separator row + comment rows), then apply `.skip(scroll_offset).take(visible_rows)` over the flat list. The spec says "apply `scroll_offset` to keep the selected field row visible" but the pseudocode implementation is incorrect for this. The `help.rs` pattern referenced in v2 does this correctly with a unified list.

**Minor — label alignment column width unspecified for detail modal.**

The mockup shows right-aligned labels ("Status:", "Priority:", "Components:", "Story Points:") but specifies no column width constant. The existing `help.rs` uses `key_col_width = 12`. "Story Points:" is 12 chars. "Components:" is 10. An agent must choose a width. If they use `help.rs`'s value of 12, "Story Points:" will be truncated or overflow. The spec should state a constant (e.g., `DETAIL_LABEL_COL: u16 = 14`).

**Minor — "relative time" format not specified.**

The comment rendering spec says:
```
author · relative_time
```
but does not say how to compute `relative_time` from an ISO 8601 timestamp. Without chrono (which is a dependency of jm-core), an agent must either add a dependency or implement a simplified calculation. The options — "2h ago", "1d ago", "3d ago" — require knowing the current time. An agent should be told: use `Instant::now()` (or `chrono::Utc::now()` since `chrono` is already in scope via `jm-core`) to compute elapsed time. A two-line spec would prevent this being solved with a wrong approach.

---

### 4. Transition Picker Esc Navigation — NOT FIXED

v2 identified this as a required fix. It is still unresolved.

**Critical UX gap — transition picker Esc returns to board, not detail modal.**

`JiraModal` is `Option<JiraModal>` — a flat single-slot. The intended flow is:
1. User opens detail modal (Enter on board) → `modal = Some(IssueDetail { ... })`
2. User presses `s` inside detail modal → `modal = Some(TransitionPicker { ... })`
3. User presses Esc in transition picker → what?

Option A: `modal = None` (board). Destroys the detail view the user came from. Requires re-opening detail to continue viewing the issue.

Option B: `modal = Some(IssueDetail { ... })` (restore previous modal). Requires storing the previous modal state so it can be restored on Esc.

The transition picker keybinding table says "Esc: cancel" without specifying the return target. `handle_modal_key` for the picker variant must either store a `return_to: Box<JiraModal>` or be handled by the parent (detail modal) calling the picker as a sub-modal.

An agent implementing this will make a choice. Most likely they choose Option A (simpler) because `JiraModal` has no stack. Option A is the wrong UX choice — the user expects to return to the detail after canceling a transition.

Fix options:
- Add `return_modal: Option<Box<JiraModal>>` to `TransitionPicker` variant
- Change `JiraModal` to a stack: `modal_stack: Vec<JiraModal>`
- Specify that Esc on `TransitionPicker` always returns to the board (and accept the UX downgrade), and document this decision explicitly

One of these three must be chosen. The current spec leaves an agent to guess.

---

### 5. Theme References — PASS

All v1 compile errors remain fixed. v2's concern about `PRIORITY_HIGH`/`PRIORITY_LOW` mapping for JIRA's 5 levels ("Highest", "High", "Medium", "Low", "Lowest") is addressed in `horizontal-scroll-spec.md`:
> "Highest/High=PRIORITY_HIGH (red), Medium=PRIORITY_MEDIUM (yellow), Low/Lowest=PRIORITY_LOW (dim)."

`PRIORITY_LOW` in `theme.rs` is `Color::Blue`, not dim (`Color::DarkGray`). The spec says "Low/Lowest → dim" but `theme::PRIORITY_LOW = Color::Blue`. An agent will use `PRIORITY_LOW` (blue) but the spec says it should be "dim." These are different colors. The spec should say `theme::PRIORITY_LOW` (blue), not "dim," or specify `theme::TEXT_DIM` explicitly.

Note: `theme.rs` has no `dim()` function that returns a `Color` constant — `theme::dim()` returns a `Style`. The spec should reference `theme::PRIORITY_LOW` directly to avoid ambiguity.

---

### 6. Plugin-Owned Modal Rendering — PASS (architecture correct; one trap)

`AboutPlugin` is the reference implementation. The pattern (render overlay in `render()`, route keys in `handle_key()`, `Clear` + `Block::bordered()` + inner layout) is clean and correctly demonstrated.

`JiraModal` variants now include `IssueDetail` with `fields: Option<Vec<EditableField>>`. The lazy-loading pattern (send `FetchEditMeta` in `on_enter` for detail, receive via `on_tick`, populate `fields`) is specified.

**One issue — `PluginAction` enum in `jira-plugin.md` differs from `plugin-architecture.md`.**

`jira-plugin.md` line 435 defines `PluginAction` as:
```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
}
```
`plugin-architecture.md` defines it as:
```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { content: String, context: String },
}
```

`jira-plugin.md`'s copy is missing `LaunchEditor`. The jira-plugin spec also references `PluginAction::LaunchEditor` for `TextArea` fields (form-modal-spec.md line 109) and comment input. If an agent implements `PluginAction` from the definition in `jira-plugin.md`, the `LaunchEditor` variant will not exist and the `$EDITOR` integration will fail to compile.

Fix: remove the `PluginAction` enum definition from `jira-plugin.md` entirely. It is already correctly defined in `plugin-architecture.md` as the canonical definition. Duplicate definitions with different contents are a maintenance hazard.

---

### 7. Loading States — PASS

No new issues. Spinner spec, braille characters, 250ms tick rate, both states (initial load vs. in-flight refresh), and submitting-form spinner are all specified. The `pending_refresh_at: Option<Instant>` pattern for post-write delay is specified and correct.

---

### 8. $EDITOR Integration — PASS

Fully resolved in v1→v2. No regression. The `on_editor_complete` lifecycle method, temp file path, TUI suspend/resume sequence, and context string pattern are all correctly specified.

---

### 9. Background Thread — PASS

Thread spawn guard, `AtomicBool` shutdown, `try_recv` drain loop, `Disconnected` → panic detection, generation counter, and stale-generation discard (including clearing `refreshing = false` on stale results) are all correctly specified. The post-write 500ms delay timer pattern is correct.

---

## New Defects Not in Previous Reviews

### A. Rendering Pseudocode Bug in Detail Modal (scroll logic broken)

Described above in §3. The `.skip(scroll_offset)` on field rows without corresponding skip on separator and comments rows produces incorrect output whenever any field rows have scrolled off screen. This will produce doubled/overlapping content: the separator renders at `row_y = inner.y` when all fields have scrolled past, and comments render on top of it.

Severity: **HIGH** — visible layout corruption on any issue with a moderately long field list.

### B. `PluginAction` Enum Divergence

Described above in §6. Two copies of the `PluginAction` enum exist with different variants. `LaunchEditor` is missing from the `jira-plugin.md` copy.

Severity: **HIGH** — compile error if agent uses the wrong copy.

### C. `label_col_width` Still Undefined (form modal)

Described above in §1. Was flagged in v2. Not fixed. The cursor position in `EditingText` will be offset unless the agent guesses the right value.

Severity: **MEDIUM** — visual bug (cursor in wrong column), functionality still works.

### D. Form Field List Has No Scroll State

Described above in §1. `FormState` has no `scroll_offset`. Fields past the visible modal height are silently inaccessible.

Severity: **MEDIUM** — data loss risk on complex issue types with many fields.

### E. `col_scroll_offsets` Not Resized on Refresh

Described above in §2. The Vec length must track the column count. JIRA workflow changes during a session will cause out-of-bounds indexing.

Severity: **LOW** — only triggered by mid-session workflow changes; easily panic-avoided with `.get(col).copied().unwrap_or(0)`.

### F. `PRIORITY_LOW` Color Mismatch

Described above in §5. Spec says "dim" (DarkGray), code defines `PRIORITY_LOW = Blue`. Minor visual inconsistency.

Severity: **LOW** — cosmetic.

---

## Rendering Issues an Agent Will Produce (Updated)

| # | Issue | Probability | Severity |
|---|-------|-------------|----------|
| 1 | Detail modal scroll logic broken — separator renders at row 0 when fields scroll off screen | High | High |
| 2 | `PluginAction::LaunchEditor` compile error if agent reads `jira-plugin.md` definition | Medium | High |
| 3 | Transition picker Esc returns to board instead of detail modal | High | Medium (UX) |
| 4 | Form `label_col_width` guessed wrong — terminal cursor offset in EditingText | Medium | Medium |
| 5 | Form field list not scrollable — fields past visible height silently inaccessible | High | Medium |
| 6 | `col_scroll_offsets` out-of-bounds panic on workflow change during session | Low | Low |
| 7 | Priority "Low/Lowest" rendered in blue instead of dim | High | Low (cosmetic) |

---

## Verdict by Area

| Area | v2 Verdict | v3 Verdict | Change |
|------|------------|------------|--------|
| Form modal state machine | PASS (with concerns) | PASS (with defects) | Concerns reduced; `label_col_width` and field-scroll still open |
| Horizontal scroll | PASS (with concerns) | PASS | All concerns resolved |
| Issue detail modal | CONCERN | PASS (with traps) | Major gap fixed; pseudocode has a scroll bug |
| $EDITOR integration | PASS | PASS | No change |
| Theme references | PASS | PASS (minor color mismatch) | New: `PRIORITY_LOW` vs "dim" divergence |
| Plugin-owned modal rendering | PASS | PASS (with one defect) | New: `PluginAction` enum divergence |
| Loading states | PASS | PASS | No change |
| Scroll state preservation | PASS | PASS | No change |
| Transition picker Esc nav | REJECT | REJECT | Still not fixed |

---

## Final Verdict

**CONDITIONAL APPROVE** — upgrades from v2 were substantial. The detail modal rendering spec is now present and mostly correct, column vertical scroll is fully specified, `MultiSelect` is complete. These were the hardest gaps.

**REJECT on two specific flows:**

1. **Transition picker Esc navigation** — spec says "cancel" without specifying return target. An agent will return to the board; the user expects to return to the detail modal. This is observable, reproducible bad UX on every transition cancel.

2. **Detail modal scroll rendering pseudocode** — the pseudocode has a structural bug: separator and comment rows are not offset by `scroll_offset`, causing visible layout corruption when field rows scroll off screen. The pseudocode will be copied by an agent and produce broken output.

### Minimum changes needed before full APPROVE

1. **Fix detail modal rendering pseudocode** — replace the per-section `.skip()` approach with a unified flat row list approach:
   - Build `all_rows: Vec<ContentRow>` (field rows, separator, comment rows)
   - Apply `.enumerate().skip(scroll_offset).take(visible_rows)` over `all_rows`
   - This is what `help.rs` does and it is correct

2. **Specify transition picker Esc return target** — choose one:
   - Option A: Add `return_modal: Option<Box<JiraModal>>` to `TransitionPicker` variant
   - Option B: Add a `modal_stack: Vec<JiraModal>` to `JiraPlugin`
   - Option C: Explicitly specify "Esc on transition picker always returns to board" and document this as the intentional design decision (no stack)

3. **Define `label_col_width` in form modal** — add one of:
   - `let label_col_width = fields.iter().map(|(f,_)| f.name.len()).max().unwrap_or(8) + 4;`
   - Or: `const FORM_LABEL_COL: u16 = 16;`

4. **Remove duplicate `PluginAction` definition from `jira-plugin.md`** — keep only the definition in `plugin-architecture.md`, which includes `LaunchEditor`. Reference it from `jira-plugin.md` rather than redefining it.

5. **Add `scroll_offset` to `FormState::Navigating`** — needed for forms with more fields than fit in the modal.

6. **Fix `PRIORITY_LOW` color description** — change "Low/Lowest → dim" to "Low/Lowest → `theme::PRIORITY_LOW`" (blue).
