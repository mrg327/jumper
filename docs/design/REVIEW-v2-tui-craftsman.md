# TUI Craftsman Review v2

Reviewer role: TUI developer with multiple ratatui applications shipped. Zero approval bias.
Single question: will this render correctly and feel good to use?

Files reviewed: `jira-plugin.md`, `form-modal-spec.md`, `horizontal-scroll-spec.md`,
`plugin-architecture.md`, plus existing code in `screens/issue_board.rs`, `modals/input.rs`,
`modals/select.rs`, `plugins/about.rs`, `theme.rs`, `text_utils.rs`.

Previous review: `REVIEW-tui-ux.md` (REJECT). This review assesses whether those gaps were fixed.

---

## Status of v1 REJECT Gaps

| v1 Gap | Fixed? |
|--------|--------|
| `$EDITOR` path broken — no `PluginAction::LaunchEditor` | **FIXED** — `plugin-architecture.md` now has `LaunchEditor { content, context }`, `on_editor_complete()` lifecycle method, and the full App-side handler (write temp file, suspend TUI, launch editor, resume, call callback). `form-modal-spec.md` references `PluginAction::LaunchEditor` for `TextArea` fields. |
| `theme::selection()` does not exist | **FIXED** — `horizontal-scroll-spec.md` now uses `theme::selected()`. Grep confirms no remaining `theme::selection()` in spec files. |
| `theme::error()` does not exist as a function | **FIXED** — `form-modal-spec.md` uses `theme::TEXT_ERROR` (correct constant). No `theme::error()` found in specs. |
| `centered_rect` API mismatch (pixel vs percentage) | **FIXED** — `form-modal-spec.md` explicitly defines a new pixel-absolute `centered_rect(width, height, area)` helper and says "Do NOT use `crate::modals::centered_rect()` which takes percentages." The function signature is provided. |
| No `frame.set_cursor_position()` spec in `EditingText` | **FIXED** — `form-modal-spec.md` now has the explicit `frame.set_cursor_position(Position::new(field_value_x + cursor_pos, row_y))` call in the rendering snippet, matching the `input.rs` pattern. |
| Form height formula 2 rows too short (`field_count + 4`) | **FIXED** — now `field_count + 6` with the derivation explained. |
| No scroll state for detail modal | **PARTIALLY FIXED** — `IssueDetail` variant now has `scroll_offset: usize` and `field_cursor: usize`. Still missing a rendering algorithm (see below). |
| No visible cursor in detail modal field navigation | **NOT FIXED** — state variable exists but rendering spec absent (see below). |
| Layered plugin-internal modal routing unspecified | **PARTIALLY FIXED** — `JiraModal` now has `TransitionFields` as its own variant (not nested inside `IssueDetail`). Key routing remains underspecified for the stacking case. |
| Form footer rendered outside `inner` area | **FIXED** — snippet now correctly uses `inner.y + inner.height - 1`. |
| Ambiguous selected card rendering (`>` marker "Or" background) | **NOT FIXED** — `horizontal-scroll-spec.md` still says "Or a `>` marker on line 1". |

---

## Area-by-Area Assessment

### 1. Form Modal State Machine — PASS (with one remaining concern)

The state machine is complete. All states, all transitions, all key bindings are documented. The terminal cursor call is now specified. The height formula is correct. The `centered_rect` naming issue is resolved.

**Remaining concern — MultiSelect toggle UI is still absent.**

`form-modal-spec.md` line 93 says `MultiSelect` uses "Inline dropdown with checkboxes (Space to toggle, Enter to confirm)" but there is no rendering spec for what a checked vs unchecked item looks like inside the dropdown, no description of how `SelectOpen` state accumulates the selected set for a multi-select field (the state machine only has a single `dropdown_cursor: usize`, not a `selected_ids: Vec<String>`), and no `Space` key documented in the `SelectOpen` keybinding table.

If any required JIRA field is `MultiSelect`, the form will be visually broken and the state machine insufficient. This is a real risk — Labels and Components are frequently `MultiSelect` fields in JIRA.

**Remaining concern — field list scroll within form is still absent.**

`form-modal-spec.md` says "if more fields than fit, the field list scrolls internally" but there is no `scroll_offset` variable in `FormState`, no algorithm for which fields are visible, and no cursor-follows behavior described. An agent will implement this independently with no guarantee the cursor stays visible. This was flagged in v1 and is still open.

**Remaining concern — dropdown `Clear` widget is not mentioned.**

The dropdown rendering spec describes position and dimensions but does not say to render a `Clear` widget before the dropdown border. Without `Clear`, the dropdown border will overdraw the underlying text but leave background color artifacts from the field rows it overlaps. The existing `input.rs` and `select.rs` both call `frame.render_widget(Clear, popup_area)` as the first rendering step. The dropdown spec should follow suit.

**Remaining concern — `label_col_width` is never defined.**

The `render_form` snippet uses `label_col_width` to calculate `field_value_x` for cursor positioning:
```rust
let field_value_x = inner.x + label_col_width;
```
This variable is never specified anywhere. An agent must guess: is it the length of the longest field name? A fixed value like 12? The alignment column width affects both the visual layout (all values must line up) and the cursor position calculation. If an agent uses a wrong value, the blinking cursor will appear in the wrong column.

**Minor — `ValidationError → Navigating` transition on field edit still unspecified.** Does correcting a `!`-marked field clear the error immediately (on keystroke) or only on the next `S` submit? The state diagram says "same as Navigating, but error markers shown" but does not say when the `!` prefix is removed.

---

### 2. Horizontal Scroll — PASS (with two remaining concerns)

The core algorithm is solid. The viewport math is correct. `theme::selected()` is now correct. Scroll state preservation (key-follows-not-index) is fully specified.

**Remaining concern — selected card rendering is still ambiguous.**

`horizontal-scroll-spec.md` section "Selected Card" still says:
> - Inverted/highlighted background (`theme::selected()`)
> - Or a `>` marker on line 1: `>HMI-103  Story`

The "Or" is fatal for consistency. The existing `issue_board.rs` uses `theme::selected()` (background highlight). An agent that chooses the `>` marker will diverge from app convention silently. One approach must be mandated. Recommendation: drop the "Or" line, keep only `theme::selected()`.

**Remaining concern — vertical scroll within a column has no spec.**

When a column has more issues than visible rows (e.g., "Done" column with 20+ issues), the column needs a vertical scroll mechanism. The spec calculates `(column_height - 1) / 4` max visible cards but never defines a `col_scroll_offset` state variable per column or describes how it tracks `selected_row`. An agent using ratatui's `List` widget with `ListState` would need to know to use `ListState::select()` and pass it to `List::render_stateful`. An agent building a manual render loop needs a per-column scroll offset. Neither approach is specified. This is the most likely source of a runtime bug on data-heavy boards — the selected card scrolls off screen with no recovery.

**Minor — dots row always reserves 1 row.** The `Layout::vertical` spec unconditionally includes `Constraint::Length(1)` for the dots row, but the comment says "only if scrolling." When all columns fit (no horizontal scroll), this wastes 1 board row. The spec should resolve this: either always reserve the row or show how to build the layout conditionally. This is cosmetic but visible on small terminals.

---

### 3. Issue Detail Modal — CONCERN

`scroll_offset` and `field_cursor` are now in the `IssueDetail` struct. But the rendering spec for the detail modal was never written.

**Critical gap — no rendering algorithm for the detail modal.**

There is no `detail.rs` spec at all. No description of how many rows the modal occupies, how the fields are laid out, how `scroll_offset` is applied, how `field_cursor` is rendered (which row gets `theme::selected()` background?), or how `j`/`k` update `field_cursor` and potentially advance `scroll_offset`.

The mockup in `jira-plugin.md` shows a full-screen-width modal with ~30 rows of content including description and comments. On a 24-row terminal, this content cannot fit. The spec says "scrollable" but provides no algorithm.

The `help.rs` pattern (`.skip(scroll_offset).take(visible_rows)`) is referenced in `TUI-READINESS.md` but not in any JIRA spec file. An agent implementing `detail.rs` will have to re-derive the scroll approach independently. Given that fields and comments are two logically separate regions (fields are navigable with `e` for edit, comments are display-only), having a single `scroll_offset` for the whole modal means either:
- `j`/`k` scroll the entire content (but then `field_cursor` cannot point to a field that is scrolled off screen, making `e` ambiguous), or
- `j`/`k` navigate `field_cursor` within the fields section and comments scroll independently.

Neither interpretation is specified.

**Gap — cursor rendering in detail modal is unspecified.**

`field_cursor` is in the struct but there is no description of how the selected field is highlighted. Does it get `theme::selected()` background? A `>` prefix? No rendering cue at all? Without a visible cursor, pressing `e` to edit is usable only if the user memorizes which field number they are on.

**Gap — label alignment column width is not specified.**

The mockup shows right-aligned labels (≈12 chars wide) with values starting at a consistent column. The spec says nothing about what this column width is. The existing `help.rs` uses a fixed `key_col_width = 12u16`. The detail modal labels are longer ("Components", "Story Points" — up to 12 chars). An agent must guess whether to use the longest label name dynamically or a fixed constant.

**Gap — comments rendering format is not specified.**

The mockup shows author name, relative time ("2h ago"), and body text. The spec says "shows most recent first" but does not describe:
- How to calculate "2h ago" from an ISO 8601 timestamp
- Whether the author/time line uses a different style than the body
- How multi-line comment bodies are handled (wrap to full modal width? indent under the author?)
- Whether comments participate in the `scroll_offset` mechanism

An agent will produce independently correct-looking but unpredictable output.

---

### 4. $EDITOR Integration — PASS

This was the only hard FAIL in v1. It is now fully resolved.

`plugin-architecture.md` specifies:
1. `PluginAction::LaunchEditor { content: String, context: String }` in the enum
2. `on_editor_complete(&mut self, content: String, context: &str)` default no-op on `ScreenPlugin`
3. The full App-side handler: write temp file to `$TMPDIR/jm-plugin-<name>.txt`, stash in `pending_editor_plugin`, detect at top of run loop, suspend (`disable_raw_mode` + `LeaveAlternateScreen`), launch `$EDITOR`, resume (`enable_raw_mode` + `EnterAlternateScreen` + `terminal.clear()`), read temp file, delete it, call `on_editor_complete`
4. An `on_editor_complete` usage example for the comment case

The `form-modal-spec.md` correctly specifies that `TextArea` fields return `PluginAction::LaunchEditor` and receive the result via `on_editor_complete`. The context string `"comment:HMI-103"` pattern is documented.

**One minor gap**: The App-side code snippet in `plugin-architecture.md` uses `name.clone()` in `pending_editor_plugin = Some((name.clone(), context, temp_path))` but `name` is not in scope in the snippet — it assumes `self.screen = ScreenId::Plugin(name)` which is a `String`, and the code silently borrows it. This is a snippet-quality issue, not a spec gap — an agent will figure it out from context.

---

### 5. Theme References — PASS

All three compile-time errors from v1 are resolved:
- `theme::selection()` → `theme::selected()`: fixed in `horizontal-scroll-spec.md`
- `theme::error()`: not present in any spec; `form-modal-spec.md` correctly uses `theme::TEXT_ERROR`
- `centered_rect` disambiguation: explicit in `form-modal-spec.md`

**Remaining concern — priority level colors for Highest/Lowest are not in `theme.rs`.**

`horizontal-scroll-spec.md` says "Highest/High=red, Medium=yellow, Low/Lowest=dim" but `theme.rs` only defines `PRIORITY_HIGH`, `PRIORITY_MEDIUM`, `PRIORITY_LOW`. JIRA has 5 priority levels. An agent must map "Highest" to `PRIORITY_HIGH` (red) and "Lowest" to `TEXT_DIM` (dim) without any spec guidance. This produces inconsistent behavior across implementations. The spec should either add constants to `theme.rs` or list the exact mapping.

---

### 6. Plugin-Owned Modal Rendering — PASS

The architecture is clear and the pattern is working code (`about.rs` as reference). The `render()` + `Clear` + `Block::bordered()` + inner layout pattern is well-established. `JiraModal` enum is defined with appropriate variants.

**Remaining concern — modal key routing for layered states.**

The `JiraModal` enum has `IssueDetail`, `TransitionPicker`, `TransitionFields`, `CreateForm`, `ErrorModal` as flat variants. The intended flow is:
1. Board → open `IssueDetail` (press Enter)
2. `IssueDetail` → open `TransitionPicker` (press `s`)
3. `TransitionPicker` → open `TransitionFields` (press Enter on a transition with required fields)

But `JiraModal` is `Option<JiraModal>` — a single optional variant. Moving from `IssueDetail` to `TransitionPicker` replaces the modal rather than stacking it. When the user cancels the `TransitionPicker` (Esc), what do they return to? The board? Or the detail modal they came from?

The spec says `Esc` on the transition picker returns to the kanban board (the keybinding table for `TransitionPicker` says "Esc: cancel"). But the user likely expects to return to the detail modal. This is a UX decision that must be made explicitly — and it is not made in any spec. If returning to detail is correct, `JiraModal` needs to store return state or become a stack, and the transition picker's Esc behavior must be documented.

---

### 7. Loading States — PASS

Both loading states are fully specified with spinners:
- Initial load: centered "⠙ Loading issues..." with the braille cycle sequence documented
- Refresh (data on screen): header changes to "JIRA: HMI ↻ Refreshing..."
- Submitting state in form: spinner replaces footer row with "⠙ Creating issue..."

The spinner character set (`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`) is correct UTF-8, renders 1 cell wide in ratatui. The 250ms tick cycle is specified. No issues.

---

### 8. Scroll State Preservation — PASS

`horizontal-scroll-spec.md` has a complete 7-step algorithm for state preservation across refresh using `(selected_issue_key, selected_status_name)` as the identity pair — not index. The edge cases (status no longer exists, issue no longer exists in column) are handled with explicit clamp steps. The `generation` counter for stale-refresh detection is defined in the `JiraPlugin` struct. This is solid.

---

## Rendering Issues an Agent Will Produce

Listed in rough order of impact:

1. **Column vertical scroll off-screen**: With no `col_scroll_offset` per column and no `ListState` guidance, a column with 15 issues will render all 15 cards — most of them below the visible area — with no scrolling to keep the selected card on screen. The card at `selected_row=10` in a 5-row visible column will be invisible, and `j`/`k` will appear to do nothing.

2. **Detail modal cursor invisible**: `field_cursor` is stored but never rendered visually. The user presses `j`, nothing appears to move, then presses `e` and something unexpected happens. The `e` keybinding is broken UX without a visible cursor.

3. **Detail modal content cut off**: Without a rendering algorithm, the agent will either render all fields in a tall fixed-height modal (which may exceed terminal height, causing ratatui to silently clip it) or stop rendering at an arbitrary row. Either way, content past row ~20 is inaccessible.

4. **MultiSelect fields display stale values**: The `SelectOpen` state only has `dropdown_cursor: usize`, no accumulator for checked items. An agent will render the dropdown as a single-select and ignore multi-select semantics, silently sending wrong JSON to the JIRA API.

5. **`label_col_width` guessed wrong**: If the agent uses the wrong column width for label/value alignment, the terminal cursor position in `EditingText` state will be offset left or right, making inline editing look broken even though it technically works.

6. **Transition picker Esc returns to board not detail**: The spec says `Esc: cancel` on the transition picker without specifying what "cancel" means navigationally. If the user was in the detail modal when they pressed `s`, they expect `Esc` to bring them back to the detail — not the board. An agent following the spec literally will close back to the board, destroying the context.

7. **`>` marker selected card**: If an agent picks the `>` marker option (still offered with "Or"), the card will have `>HMI-103  Story` on line 1 — disrupting the alignment of the key and type display that the layout is designed around. This is cosmetic but immediately visible.

---

## Verdict by Area

| Area | Verdict |
|------|---------|
| Form modal state machine | PASS (MultiSelect and field-list scroll are concerns, not blockers for basic text/select fields) |
| Horizontal scroll | PASS (column vertical scroll is the main unresolved gap) |
| Issue detail modal | CONCERN (scroll algorithm and cursor rendering are absent) |
| $EDITOR integration | PASS (fully fixed from v1) |
| Theme references | PASS (all compile errors resolved; Highest/Lowest colors are minor) |
| Plugin-owned modal rendering | PASS (modal key routing for layered states needs one design decision) |
| Loading states | PASS |
| Scroll state preservation | PASS |

---

## Final Verdict

**CONDITIONAL APPROVE** for Phases 1a (API + board rendering) and 1b (issue detail + transitions, minus comments).

**REJECT** for Phase 1c (comments via $EDITOR) — actually this is now PASS, $EDITOR is resolved.

**REJECT** still applies for the issue detail modal as a complete feature. The struct fields are there but the rendering spec is absent. An agent implementing `detail.rs` will produce something functional-looking but with an invisible cursor and broken scroll — making `j/k/e` navigation unusable in practice.

### Minimum changes needed before full APPROVE

1. **Write a `detail-modal-spec.md`** (or add a "Detail Modal Rendering" section to `jira-plugin.md`) that specifies:
   - Total modal height and width (e.g., `min(terminal_width - 4, 80)` wide, `min(terminal_height - 2, area.height)` tall)
   - How `scroll_offset` is applied: `.skip(scroll_offset).take(visible_rows)` over a flattened list of all content rows (fields + separator + comments)
   - How `field_cursor` is rendered: recommend `theme::selected()` background on the entire field row when that row is the selected navigable field
   - `j`/`k` behavior: advance `field_cursor` through navigable fields only (not through read-only rows or comment rows), and advance `scroll_offset` when `field_cursor` moves past the visible region
   - Label alignment column: define a fixed `LABEL_COL_WIDTH: u16 = 14` or equivalent constant
   - Comments rendering: author/time in `theme::dim()`, body in default style, one blank row between comments

2. **Resolve the "Or" in horizontal-scroll-spec.md Selected Card** — drop the `>` marker option, mandate `theme::selected()` background only.

3. **Add per-column scroll state to the board**: Add a `col_row_offsets: Vec<usize>` (one entry per column) to `BoardState` and specify that `j`/`k` updates `selected_row` and adjusts `col_row_offsets[selected_col]` to keep the card visible. Alternatively, specify `ListState`-based rendering and how to update it.

4. **Add `MultiSelect` toggle state to `FormState`**: Add a `checked_ids: Vec<String>` to `SelectOpen` (used only for multi-select fields), document `Space` key in the keybinding table, show what checked vs unchecked items look like (e.g., `[x] High` vs `[ ] Low`).

5. **Define `label_col_width`** in the form modal rendering section — either as a constant or as "max label name length + 2, rounded up to nearest 4".

6. **Specify transition picker Esc navigation** — does it return to the detail modal (if opened from detail) or the board (if opened from board via `s`)? If context-sensitive, specify how the return target is stored.
