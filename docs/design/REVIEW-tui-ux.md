# TUI/UX Spec Review

Reviewer role: senior ratatui developer, no approval bias.
Files reviewed: `jira-plugin.md`, `form-modal-spec.md`, `horizontal-scroll-spec.md`,
`plugin-architecture.md`, plus existing code in `screens/issue_board.rs`,
`modals/input.rs`, `modals/select.rs`, `plugins/about.rs`, `theme.rs`, `text_utils.rs`,
`app.rs`, `events.rs`, and `plugins/mod.rs`.

---

## Assessment

### 1. Form Modal State Machine — CONCERN

The state machine itself is complete and all transitions are enumerated. The
problems are in the rendering details.

**Cursor rendering in `EditingText` is underspecified.** The spec says "cursor
appears at the end of the current value" and shows a `[Fix crash on back___]`
placeholder, but never specifies *how* to draw the cursor character. The existing
`input.rs` uses a block-cursor technique (`build_input_line`) that inserts a
highlighted `Span` at the cursor position, then calls
`frame.set_cursor_position()` to position the terminal cursor. The spec says
nothing about which approach to use, and specifically does not mention calling
`frame.set_cursor_position()` inside the form. An agent will likely render the
cursor highlight but forget the actual terminal cursor, causing the blinking
cursor to sit at `(0,0)` on the screen while focus is inside a field — visually
broken.

**`centered_rect` signature mismatch.** The form spec gives this helper:

```rust
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    ...
}
```

But the existing codebase version in `modals/mod.rs` takes `(percent_x: u16,
percent_y: u16, area: Rect)` — percentage-based, not absolute pixels. Both
`input.rs` and `select.rs` call `crate::modals::centered_rect(60, 50, area)` /
`crate::modals::centered_rect(70, 60, area)` using percentages. The spec
defines a *pixel-absolute* `centered_rect`. An agent that reuses the existing
helper will silently pass pixel counts as percentage arguments, producing a
modal of the wrong size (a 60px wide form becomes a 60% wide form, and a 14px
height becomes 14% — roughly 3 rows on an 80x24 terminal, not enough to display
all fields). The spec must either specify which helper to call, or note that a
new absolute-pixel variant is needed.

**The form height formula is wrong.** The spec says `height: field_count + 4`
(title + padding + footer). Ratatui's `Block::bordered()` consumes 2 rows (top
and bottom border), and the inner content needs at minimum 1 row per field plus
1 footer row plus 1 padding row — which is `field_count + 2` inner rows, plus 2
for the border = `field_count + 4`. This is correct *only* if there are exactly
0 rows of top padding, which contradicts the mockup showing a blank row above the
first field. The mockup has 1 blank row at top and 1 at bottom, meaning the
actual content height is `field_count + 2` inner rows plus 2 blank rows =
`field_count + 4` inner rows, which with 2 border rows = `field_count + 6`.
Depending on how an agent reads this, forms will be 2 rows too short, cutting
off either the last field or the footer.

**Dropdown overlay positioning is underdefined.** The spec states the dropdown
"overlays fields below it (they are temporarily hidden)" and shows a list
starting at "the column of the value area, row below the field." In ratatui,
overlays are drawn by rendering a `Clear` widget before the dropdown content.
The spec does not mention this. More critically, it does not specify the absolute
`Rect` calculation for the dropdown. The value column offset (where the dropdown
left-aligns) is never calculated from the form layout — an agent must guess the
column width of the label-prefix area to determine where the value column starts.
This is a real rendering bug risk.

**No spec for `MultiSelect` toggle UI.** The field type table lists `MultiSelect`
as "Inline dropdown with toggle," but there is no description of how a user
toggles items on/off, how checked/unchecked items are displayed inside the
dropdown, or how selected items are accumulated in the `FormState`. This is a
complete rendering gap if `MultiSelect` fields appear in required JIRA fields.

**`ValidationError` state is treated as identical to `Navigating` in the state
diagram.** The spec says "same as Navigating, but error markers shown." However,
the `!` prefix replaces the previous prefix character. The transition
`ValidationError → Navigating` on field edit is never specified. When the user
corrects a field marked `!` and presses `Enter`, does it stay in
`ValidationError` (with the `!` removed on that field) or transition to
`Navigating`? Ambiguous.

**Scroll behavior inside the field list is completely absent.** The spec says "if
more fields than fit, the field list scrolls internally" but gives no state
variable for the scroll offset, no algorithm for which fields are visible, and no
description of how the viewport follows the cursor when `j`/`k` navigates past
the visible range. Agents writing this feature will produce independent
implementations that likely do not keep the cursor visible.

---

### 2. Horizontal Scroll Kanban — CONCERN

The core algorithm (viewport, cursor-follows) is solid and the pseudocode is
correct.

**The vertical layout spec allocates a fixed 1-row scroll-dots area
unconditionally.** The spec gives:

```rust
let chunks = Layout::vertical([
    Constraint::Length(1),  // Header
    Constraint::Min(5),     // Board
    Constraint::Length(1),  // Scroll dots (only if scrolling)
    Constraint::Length(2),  // Footer
]).split(area);
```

The comment says "only if scrolling" but the `Layout` always reserves the row.
When there are few columns and no scrolling, the dots row is empty but still
consumes 1 row of board height. The spec should either always include the dots
row (consuming 1 row always) or conditionally build the `Layout` with a
`Constraint::Length(0)` or `Constraint::Length(if scrolling { 1 } else { 0 })`.
The current spec is contradictory — an agent will have to guess. On an 80x24
terminal this wastes 1 of the 17 usable board rows, which is noticeable.

**The `column_widths` function distributes remainder to leftmost columns, but the
dot indicator marks ALL visible columns as filled.** This is correct semantically
but the visual alignment between dots and column borders will be off by ~1
character on odd-remainder widths because each dot is `● ` (2 chars with
space) but columns are not all equal width. This is cosmetic but will look wrong
on close inspection.

**The "selected card" spec is ambiguous between two rendering approaches.** The
spec says: "Inverted/highlighted background (`theme::selection()`) *Or* a `>`
marker on line 1". Offering both options with "Or" leaves agents to implement
either one. The existing `issue_board.rs` uses `theme::selected()` (background
highlight), not a `>` marker. An agent that chooses the `>` marker diverges from
app convention without being wrong. One approach should be mandated.

**`theme::selection()` is not a real function.** The spec references
`theme::selection()` in two places. The actual theme module exports
`theme::selected()` (not `selection()`). This will cause a compilation error.

**Card spacing and column height math is correct but relies on `List` widget
behavior the spec does not mention.** Each "card" is 4 rows (3 lines + 1 blank
separator), and the spec calculates max visible cards as `(column_height - 1) /
4`. However, the existing code in `issue_board.rs` uses `List::new(items)` where
each `ListItem` is a *single line*. The three-line card format will require either
multi-line `ListItem` values or a manual paragraph-per-card render loop.
The spec doesn't say which approach to take. Multi-line `ListItem` works in
ratatui (each `ListItem` can hold a `Text` with multiple `Line`s), but the
behavior when the list scrolls (for columns with many issues) is not specified.
Does the list need a `ListState` for scrolling? The spec says nothing.

**The `Done` column toggle (`D` key) interacts with the `selected_col` index in
an unspecified way.** When `Done` is hidden and `selected_col` is beyond the
last visible column, what happens? The spec says to clamp on refresh but says
nothing about the `D` toggle itself. The existing `issue_board.rs` calls
`clamp_row(state)` on `D`, but the JIRA spec doesn't mention an equivalent.

---

### 3. Issue Detail Modal — CONCERN

The mockup and field list are thorough. The problems are in scrolling and scroll
state.

**No scroll state is specified.** The detail modal contains fields plus a
potentially long comment thread. The spec says "Comments section is scrollable"
and "navigate fields with `j`/`k`," but there is no `scroll_offset` or
`viewport_start` state variable defined anywhere. An agent has no guidance on
whether field navigation scrolls the whole modal view or whether fields and
comments are two independent scrollable regions. This is a fundamental rendering
architecture gap.

**The `j`/`k` key navigates "fields," but field count is dynamic.** The list of
fields shown in the mockup (Status, Priority, Assignee, Reporter, Type, Points,
Sprint, Epic, Labels, Components, Created, Updated, Description, Comments) is 14
rows. On a 24-row terminal with header/footer consuming ~3 rows, the inner area
is ~19 rows — the field list fits. But when comments are long, a combined
scrollable region requires knowing total item count. This is not specified.

**The "editable" field marker `*` conflicts with the form modal spec.** In the
form spec, `*` (red) means "required field, empty" and `*` (green) means
"required field, has value." In the detail modal spec, `*` means "editable
field" (not "required"). An agent building both components from these specs may
implement inconsistent color semantics for `*`.

**No cursor rendering spec for the detail modal navigation.** When `j`/`k`
navigates among fields, how is the selected field highlighted? The mockup shows
no cursor indicator. Is it a full-row background highlight (like
`theme::selected()`)? A `>` marker? Nothing at all? The keybinding table says
`e` edits the selected field, but without a visible cursor the user cannot see
which field is selected.

**The field layout uses fixed-width label columns in the mockup but no width is
specified.** The mockup right-aligns values starting at column ~15:
```
  Status:      In Progress
  *Priority:   High
```
The alignment column is never specified. Agents will implement different label
widths, producing inconsistent alignment. The existing code (e.g.,
`render_detail` in `issue_board.rs`) does not align label/value columns — it
simply renders `Span::styled("Status: ", dim())` followed by the value. The JIRA
detail modal appears to need column-aligned rendering that the codebase has no
existing helper for.

---

### 4. Transition Picker Modal — PASS (with minor note)

The transition picker is a simple vertical list — essentially a specialized
`SelectModal`. The state machine (j/k, Enter, Esc) is complete, the layout is
clear, and the "required fields flow" (invoking the form modal) is described.

**Minor**: The spec does not describe how the transition picker handles the case
where `FetchTransitions` is still in-flight when `s` is pressed. It says
transitions are "lazy-fetched," but there is no loading state or disabled state
for the picker while waiting. An agent will either show an empty list or block —
neither is specified.

---

### 5. $EDITOR Integration — FAIL

The spec says (jira-plugin.md §Comment Input):

> The app already has editor launch code (`app.rs:167-196`).

This is the core problem. The `$EDITOR` launch mechanism in `app.rs` is
**inaccessible to a `ScreenPlugin`**.

The existing mechanism works via the `pending_editor_slug: Option<String>` field
on `App`, set by `Action::OpenEditor` / `Action::OpenEditorSelected`, and
consumed at the top of `App::run()` before each draw. This is App-private state.

`ScreenPlugin::handle_key()` returns only `PluginAction { None, Back,
Toast(String) }`. There is no `PluginAction::OpenEditor(PathBuf)` variant. There
is no mechanism for a plugin to set `app.pending_editor_slug`. The `PluginAction`
enum is intentionally narrow and is defined in `plugins/mod.rs` — the plugin
cannot return an action that triggers the editor launch.

The spec says "Flow: 1. User presses `c` 2. App writes a temp file..." implying
the *App* handles the temp file. But the App's key handler for plugin screens is:

```rust
match plugin.handle_key(key) {
    PluginAction::None => Action::None,
    PluginAction::Back => { ... }
    PluginAction::Toast(msg) => Action::Toast(msg),
}
```

There is no path from `PluginAction` to `App::pending_editor_slug`. The
`jira-plugin.md` spec references app internals the plugin cannot reach without a
new `PluginAction` variant (e.g., `PluginAction::OpenEditor { path: PathBuf,
on_complete: ... }`) or a callback mechanism that does not exist.

This gap affects both comment creation (`c` key on board and detail modal) and
any `TextArea` field in the creation form. The spec says TextArea fields "open
`$EDITOR`" — but this has the same broken path.

**What is missing**: The spec must either (a) add `PluginAction::OpenEditor(PathBuf)`
and specify how `App` handles it (writes the temp file, launches editor, reads
back, calls a plugin callback with the content), or (b) describe an
`on_editor_complete(content: String)` lifecycle method on `ScreenPlugin`, or
(c) specify that the plugin does the terminal suspend/restore itself via
`crossterm` directly without going through the App. Option (c) would work
technically but is inconsistent with the "plugin is self-contained" design — and
the spec explicitly says "The app already has editor launch code" implying it
should go through the app.

---

### 6. Theme Consistency — CONCERN

**Missing theme functions.** The specs reference theme helpers that do not exist
in `theme.rs`:

| Referenced in spec | Actual in theme.rs |
|---|---|
| `theme::selection()` | `theme::selected()` |
| `theme::error()` | `theme::TEXT_ERROR` (constant, not a function) |
| `theme::dim()` | `theme::dim()` — exists, correct |
| `theme::accent()` | `theme::accent()` — exists, correct |

`theme::selection()` appears twice in `horizontal-scroll-spec.md`. `theme::error()`
appears in `form-modal-spec.md` for the red required-field prefix. An agent
copying these names verbatim will get compilation errors.

**Priority color mapping.** The card spec says:
> "Priority name (colored: Highest/High=red, Medium=yellow, Low/Lowest=dim)"

JIRA uses 5 priority levels (Highest, High, Medium, Low, Lowest). The existing
`theme.rs` only has 3: `PRIORITY_HIGH`, `PRIORITY_MEDIUM`, `PRIORITY_LOW`. The
mapping for "Highest" and "Lowest" is not specified and there is no
`PRIORITY_HIGHEST` or `PRIORITY_LOWEST` constant. An agent will have to guess.

**`selected()` vs background highlight.** `theme::selected()` uses
`SELECTED_BG = Color::DarkGray`. On dark terminals (common for developers), a
DarkGray background on dark-terminal background produces very low contrast. This
is an existing app concern and not introduced by the spec, but spec code
examples should note the potential visibility issue.

---

### 7. Key Handling — CONCERN

**`c` key is used for conflicting actions.** In `issue_board.rs`, `c` closes an
issue (`CloseIssue`). In the JIRA board spec, `c` means "Comment on selected
issue." This is fine because they are different screens, but the JIRA plugin also
implements `c` as "close issue" in its detail modal header key hint (`c:comment`
vs the board's close-issue `c`). Actually, rereading the spec: `c` on the JIRA
board means "Comment," and `c` on the existing jm IssueBoard means "Close." This
is not a conflict since they are different screens, but if a developer reads both
specs side-by-side they will be confused. A comment noting the intentional
difference would prevent bugs.

**`q` key on JIRA board conflicts with quitting the app.** In the existing app,
`q` from the dashboard quits jm entirely. The JIRA board spec lists `Esc / q` as
"Back to dashboard." The existing issue board (`issue_board.rs:153`) already
handles `q` as `Action::Back` at the screen level. The JIRA plugin's
`handle_key()` must consume `q` and return `PluginAction::Back` — but the app's
plugin key routing is:

```rust
match plugin.handle_key(key) {
    PluginAction::Back => { self.handle_back(); Action::None }
    ...
}
```

The global `q`→Quit handler fires *after* screen-level handling only if the
screen returns `Action::None`. Looking at `app.rs`, the Dashboard handler
includes `KeyCode::Char('q') => Action::Quit`. For plugin screens, the key goes
to `plugin.handle_key()` which returns `PluginAction` — the `q`→Quit path is
never reached. So this is fine, but the spec should confirm this explicitly since
it is non-obvious.

**Ambiguous key routing in the detail modal + form modal stack.** When the form
modal for "required transition fields" is open inside the detail modal (which is
open on the JIRA board), the key routing is:

```
App::handle_key
  → JiraPlugin::handle_key
    → JiraPlugin::handle_modal_key (detail modal is open)
      → Detail modal should route to form modal if form is open
```

The spec (plugin-architecture.md) shows a single `modal: Option<JiraModal>` but
the JIRA flow requires layered modals: detail → transition picker → form. None
of the specs define the modal stack depth inside the plugin or how keys are
routed through 2+ plugin-internal modal layers. The `JiraModal` enum in the
architecture doc shows only `IssueDetail`, `CreateIssue`, `ConfirmTransition` —
not a form modal on top of a detail modal on top of the board. This needs a
sub-modal field or a small internal stack.

**Kanban board: `g`/`G` navigate within a column, but these are not listed in
`jira-plugin.md` keybinding table.** They ARE listed in `horizontal-scroll-spec.md`.
The two docs are inconsistent. An agent reading only `jira-plugin.md` will omit
`g`/`G` support.

---

### 8. ratatui Compatibility — CONCERN

**`Layout::horizontal(constraints).split(board_area)` returns an `Rc<[Rect]>`,
not a `Vec<Rect>`.** The code in `horizontal-scroll-spec.md` does:

```rust
let col_areas = Layout::horizontal(constraints).split(board_area);
```

then presumably indexes `col_areas[i]`. In ratatui 0.27+, `.split()` returns
`Rc<[Rect]>` which supports indexing, so this is fine. However the spec and
existing code also uses `.areas()` (the destructuring variant). The spec uses
`.split()` for dynamic constraint counts (correct — `.areas()` only works for
fixed-size arrays). This is not a bug, just worth noting an agent should not
use `.areas()` here.

**The form modal's `centered_rect` takes `u16` height but ratatui `Rect` fields
are `u16`.** No overflow risk for realistic field counts.

**`Paragraph::new(dots).alignment(Alignment::Center)` for the scroll dots row
is correct ratatui usage.** No issue.

**Multi-line `ListItem` in the card rendering.** The spec calls for 3-line cards
with a blank separator. Using `ListItem::new(Text::from(vec![line1, line2, line3,
Line::raw("")]))` is valid in ratatui. However, `List` does not natively scroll
to keep the selected item visible when using multi-line items and there is no
`ListState` specified. For columns with many issues (more cards than visible
rows), the selected card may scroll off screen with no guidance on how to handle
this. The spec says nothing about vertical scrolling within a column.

**The spinner character set (`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`) are multi-byte UTF-8
characters (3 bytes each) but each displays as 1 terminal cell.** Ratatui's
`Paragraph` handles these correctly. No issue.

---

## Gaps Found

### Critical (will produce broken or non-compiling code)

1. **`$EDITOR` launch path is broken.** `PluginAction` has no variant to trigger
   editor launch. The plugin cannot set `app.pending_editor_slug`. Comment
   creation and `TextArea` fields have no working implementation path.
   A new `PluginAction::OpenEditor(PathBuf)` variant (or equivalent mechanism)
   must be added to `PluginAction` and handled in `App`.

2. **`theme::selection()` does not exist.** Two spec files reference this.
   Should be `theme::selected()`. Will not compile.

3. **`theme::error()` does not exist as a function.** Should be
   `Style::default().fg(theme::TEXT_ERROR)`. Form modal indicator code will
   not compile as written.

4. **`centered_rect` API mismatch.** The form spec defines an absolute-pixel
   version; the codebase has a percentage version. Calling the existing helper
   with pixel values will produce a nearly-invisible modal.

### Significant (will produce visually broken UI)

5. **No `frame.set_cursor_position()` spec for form's `EditingText`.** The
   terminal cursor will sit at `(0,0)` instead of inside the field. Users will
   see a stray blinking cursor at the top-left of the terminal.

6. **Form height formula is 2 rows too short.** `field_count + 4` doesn't
   account for the blank padding rows shown in the mockup. Should be
   `field_count + 6` (2 border rows + 2 padding rows + 1 footer + 1 header
   inside the block).

7. **No scroll state for detail modal.** `j`/`k` navigation target is undefined
   (fields only? whole content? two regions?). No `scroll_offset` field defined.
   Agents will produce incompatible implementations.

8. **No cursor indicator in detail modal field navigation.** Users cannot see
   which field is selected. The `e` key is useless without visible selection.

9. **Layered plugin-internal modal routing is unspecified.** When the transition
   form is open inside the transition picker inside the detail modal, there is no
   defined `JiraModal` state for this. Keys will not route correctly.

10. **Selected card rendering approach is ambiguous** (`>` marker vs background
    highlight). Diverges from app convention if agent picks the wrong one.

### Minor (cosmetic or edge-case)

11. **`MultiSelect` field toggle UI is completely unspecified.** If a required
    JIRA field is multi-select, the form will be broken.

12. **`g`/`G` navigation missing from `jira-plugin.md` keybinding table** but
    present in `horizontal-scroll-spec.md`. Inconsistency will cause omissions.

13. **Column vertical scroll (many issues in one column) has no spec.** `ListState`
    and scroll-to-cursor behavior are not defined for the card list.

14. **`Done` column `D` toggle has no cursor clamping spec** for the JIRA board
    (unlike `issue_board.rs` which calls `clamp_row` on `D`).

15. **`ValidationError` → `Navigating` transition on field edit is not specified.**
    Does correcting a `!`-marked field clear the error immediately or only on
    next submit?

16. **Priority levels Highest/Lowest have no color constants in `theme.rs`.**

17. **Transition picker loading state (while `FetchTransitions` is in-flight)
    is not specified.**

18. **The form spec `render_form` snippet renders footer at `form_area.y +
    form_area.height - 1`** — this is outside the block's inner area (the block
    border occupies the last row). Footer should render at
    `inner.y + inner.height - 1`, not relative to `form_area`.

---

## Final Verdict

**REJECT**

The specs are incomplete in ways that will produce broken TUI implementations.
The `$EDITOR` integration is architecturally broken (Gap #1) — the primary
comment input mechanism has no implementation path from `PluginAction`. Two theme
function names will cause compilation failures (#2, #3). The form modal
height formula and `centered_rect` mismatch (#4, #6) will produce a visually
wrong modal even if the code compiles. The detail modal has no scroll state and
no visible field cursor (#7, #8), making it unusable.

These are not style preferences — they are missing specs that will cause an
agent to either produce code that doesn't compile or a UI that is broken on first
use.

### Minimum changes needed before APPROVE

1. Add `PluginAction::OpenEditor { temp_path: PathBuf }` to `plugins/mod.rs` and
   specify the App-side handler that suspends the TUI, runs `$EDITOR`, reads the
   temp file back, and calls a new `ScreenPlugin::on_editor_complete(content:
   String)` lifecycle method (or an equivalent callback pattern).

2. Fix `theme::selection()` → `theme::selected()` in both spec files.

3. Fix `theme::error()` → `Style::default().fg(theme::TEXT_ERROR)` in
   `form-modal-spec.md`.

4. Clarify `centered_rect`: either use the existing percentage-based helper with
   correct percentage values, or define a new absolute-pixel helper with a
   different name.

5. Add `frame.set_cursor_position()` call to the `EditingText` rendering spec,
   mirroring the pattern in `input.rs`.

6. Fix the form height formula: document the correct row count including padding.

7. Define `scroll_offset: usize` for the detail modal, specify which axis `j`/`k`
   scrolls, and describe the viewport algorithm.

8. Add a visible field cursor to the detail modal (recommend full-row
   `theme::selected()` background, consistent with existing modals).

9. Define a `JiraModal` sub-state for the case where a form modal is open on top
   of the transition picker (or on top of the detail modal), and specify key routing.

10. Pick one card selection rendering style (recommend background highlight to
    match existing code).
