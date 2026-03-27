# TUI Readiness Assessment: JIRA Plugin

Assessment of whether the design doc (`jira-plugin.md`) contains sufficient UI detail for an autonomous coding agent to implement correct, compiling, visually-correct TUI rendering code.

**Evaluator context**: ratatui 0.29, crossterm 0.28, existing codebase patterns in `crates/jm-tui/`.

---

## 1. Kanban Board Rendering

**Verdict: YELLOW -- implementable with one critical gap**

### What can be reused

The codebase has **three** kanban implementations to copy from:

| File | Pattern |
|------|---------|
| `screens/issue_board.rs` | Closest analog: per-status columns, `h`/`l` column nav, `j`/`k` row nav, `D` toggle done, `p` project filter. Full 660-line reference. |
| `screens/dashboard.rs` (kanban mode) | Equal-width columns via `Constraint::Percentage(100 / N)`, `List` widget per column, `Block` with `Borders::ALL`, focused/unfocused border styles. |
| `screens/project_view.rs` (kanban mode) | Same pattern, 4 fixed columns. |

All three use the identical recipe:
```
Layout::horizontal(vec![Constraint::Percentage(100/N); N]).split(area)
```
with `List` + `Block` per column, `theme::focused_border()` / `theme::unfocused_border()`, and `theme::selected()` for the cursor item. An agent can directly copy this pattern.

### Critical gap: horizontal scrolling

**None of the existing kanban boards implement horizontal scrolling.** They all assume a fixed, small number of columns (3-5) that fit the terminal width. The JIRA plugin must handle an arbitrary number of JIRA workflow statuses (commonly 5-10+) that may exceed the terminal width.

The design doc says:

> Columns are per-status (one column per distinct JIRA workflow status). When there are more columns than fit on screen, the board scrolls horizontally. `h`/`l` navigates columns AND scrolls the viewport to keep the selected column visible.

This is **insufficiently specified**. An agent needs answers to:

1. **Column width strategy**: What width does each column get? Fixed pixel count? Percentage of terminal width? `Min(N)` constraint? The existing boards use `Constraint::Percentage(100/N)` which shrinks all columns when N is large -- that is not horizontal scrolling, that is compression.
2. **Viewport window**: How many columns are visible at once? Is it "fit as many `MIN_COL_WIDTH`-wide columns as the terminal allows, then scroll the rest"?
3. **Scroll offset tracking**: The design doc does not mention a `col_scroll_offset` field or how the viewport pans when `h`/`l` moves past the visible window edge.
4. **Rendering strategy**: ratatui has no built-in horizontal scrolling widget. The agent must manually compute a visible column window (`col_scroll_offset..col_scroll_offset + visible_cols`) and render only those columns into the available `Rect`. This is a custom implementation.

**Recommended additions to the design doc**:

```
Horizontal scroll algorithm:
- MIN_COL_WIDTH = 20 (columns never narrower than this)
- visible_cols = terminal_width / MIN_COL_WIDTH (clamped to total_cols)
- Each visible column gets Constraint::Percentage(100 / visible_cols)
- col_scroll_offset tracks the first visible column index
- When selected_column < col_scroll_offset: col_scroll_offset = selected_column
- When selected_column >= col_scroll_offset + visible_cols:
    col_scroll_offset = selected_column - visible_cols + 1
- State fields: col_scroll_offset: usize
```

### Issue card rendering

The ASCII mockup shows 3-line cards (key, summary, type+priority). The existing `build_issue_item` in `issue_board.rs` renders 1-line items. The design doc does not specify whether cards are multi-line `ListItem`s or single-line. The mockup suggests multi-line, but `List` with multi-line `ListItem`s requires height calculation per item. This needs clarification: **single-line items (like existing code) or multi-line cards?**

### Layout constraints

The design doc specifies header, body, and footer areas. This maps cleanly to:
```
Layout::vertical([Length(1), Fill(1), Length(2)]).areas(area)
```
This is well-specified and matches existing patterns (e.g., `issue_board.rs` line 311).

---

## 2. Plugin-Owned Modals

**Verdict: GREEN -- well-specified, strong existing patterns**

### Reusable utilities

The plugin can directly use these public functions from `crates/jm-tui/src/modals/mod.rs`:

| Function | Purpose |
|----------|---------|
| `centered_rect(percent_x, percent_y, area) -> Rect` | Compute centered popup rectangle |
| `render_dim_overlay(frame, area)` | Draw dim background behind modal |

Both are `pub` and take only `Rect`/`Frame` args -- no coupling to the App's modal system.

### Rendering pattern to copy

Every existing modal follows the same recipe:
1. `let popup_area = centered_rect(W, H, area);`
2. `frame.render_widget(Clear, popup_area);`
3. Build a `Block` with `Borders::ALL`, `border_style(theme::MODAL_BORDER)`
4. `let inner = block.inner(popup_area);`
5. `frame.render_widget(block, popup_area);`
6. Use `Layout::vertical(...)` on `inner` for content sections
7. Render footer hint line at the bottom

This pattern is repeated identically in `input.rs`, `select.rs`, `confirm.rs`, and `help.rs`. An agent can copy it verbatim. The key insight is that the plugin calls `frame.render_widget(Clear, popup_area)` to erase the board underneath -- it does NOT need the App's modal stack.

### Modal types needed

| JIRA Modal | Closest existing analog | Copy difficulty |
|------------|------------------------|----------------|
| Error modal | `confirm.rs` (message + dismiss) | Trivial -- simpler than confirm |
| Transition picker | `select.rs` (j/k list, Enter select) | Trivial -- nearly identical |
| Project select (creation step 1) | `select.rs` | Trivial |
| Issue type select (creation step 2) | `select.rs` | Trivial |
| Issue detail modal | No direct analog (see section 3) | **Medium** |
| Form modal (creation step 3) | No direct analog (see section 4) | **Hard** |

### Assessment

The design doc explicitly states "all modals are managed internally by the plugin" and shows the enum pattern (`JiraModal`). Combined with the existing `centered_rect` + `Clear` + `Block` recipe, an agent has enough to implement the simple modals (error, transition picker, selects). The complex modals (detail, form) are assessed separately below.

---

## 3. Issue Detail Modal

**Verdict: YELLOW -- layout is clear, but field navigation and scrolling need more spec**

### What is well-specified

- The ASCII mockup (lines 388-426) is detailed and shows exact field layout, label alignment, read-only vs editable indicators, and the comments section.
- Fields are listed: Status, Priority, Assignee, Reporter, Type, Points, Sprint, Epic, Labels, Components, Created, Updated, Description, Comments.
- Editable fields are marked with `*` and show `[e:edit]`; read-only show `(read-only)`.
- Keybindings are fully specified: `j`/`k` navigate fields, `e` edits, `s` transitions, `c` comments, `Esc` closes.

### What is NOT specified

1. **Field navigation cursor**: The doc says "Navigate fields with j/k" but does not specify:
   - What is the visual indicator for the focused field? Highlight background? `>` prefix? The existing `select.rs` uses `theme::selected()` (DarkGray background + Bold). Should the detail modal do the same?
   - Does Description count as a navigable "field" even though it is read-only?
   - Do the Comments section entries count as navigable items, or is the comments section a single scrollable region?

2. **Scrollable content**: The detail modal may have more content than fits in the viewport (long description + many comments). The doc says "Comments section is scrollable" but does not specify:
   - Is there a single scroll offset for the entire modal, or separate scroll for fields vs. comments?
   - How does the scroll interact with `j`/`k` navigation? Does `j` past the last visible field auto-scroll?
   - What existing pattern to follow? The `project_view.rs` uses `.scroll((offset, 0))` on a `Paragraph` widget, but that is a single `Paragraph` -- not a field-by-field navigator.

3. **Two-region layout**: The mockup shows fields above a horizontal divider and comments below. An agent needs to know:
   - Is this a fixed split (e.g., 60% fields / 40% comments)?
   - Or does it scroll as one unit, with the divider being part of the content?

**Recommended additions to the design doc**:

```
Detail modal layout:
- Single scroll_offset for the entire modal content
- Fields are rendered as Lines in a Vec<Line>, comments appended below
- j/k moves a field_cursor through navigable items (all fields + "Comments" header)
- When field_cursor moves below the visible area, scroll_offset increases
- Focused field gets theme::selected() background
- Description is navigable (for reading) but e does nothing on it
- Comments are NOT individually navigable -- they scroll as a block below fields
- Layout: centered_rect(80, 85, area), then vertical split:
    [fields region: Min(8)] [divider: Length(1)] [comments region: Fill(1)] [footer: Length(1)]
```

### Existing patterns

The `help.rs` modal has a `scroll_offset` and renders scrollable content with `skip(scroll_offset).take(visible_rows)`. This is the closest pattern for the detail modal's scrollable content. An agent should study `help.rs`.

---

## 4. Form Modal (Issue Creation)

**Verdict: RED -- requires building a new widget pattern from scratch**

### What the design doc specifies

- All required fields visible at once (not wizard-style one-at-a-time)
- `j`/`k` navigates between fields
- `Enter` edits the focused field
- `S` submits all fields
- Per-field-type input: Text (inline), Select (popup), Number (text input with validation), TextArea ($EDITOR)
- Required fields marked with `*`
- On API error, preserve filled fields

### What is NOT in the existing codebase

**There is no multi-field form widget anywhere in the codebase.** The existing modals are:
- `InputModal`: single text field
- `SelectModal`: single selection list
- `ConfirmModal`: yes/no buttons
- `HelpModal`: read-only scrollable text

A multi-field form with heterogeneous field types (text, select, number) and per-field edit triggers is a **fundamentally new pattern**. An agent must build:

1. **FormField struct**: `{ name, field_type, value, required, allowed_values, focused }`
2. **Form state machine**: navigating fields with j/k, triggering inline edit (text fields open a mini-input in place), triggering select popup (overlays the form with a select list), handling $EDITOR launch for TextArea fields
3. **Rendering logic**: each field rendered as a line, required fields with `*` prefix, focused field highlighted, current value shown inline
4. **Nested modal within modal**: when editing a Select field, a selection popup appears ON TOP of the form modal. This is a modal-within-a-modal pattern.

### Critical gaps in the design doc

1. **Inline text editing UX**: When the user presses Enter on a Text field, does a cursor appear inline (like editing a cell in a spreadsheet), or does a separate `InputModal`-style popup appear? The doc says "inline text input on Enter" which suggests in-place editing, but this is not how any existing modal works.

2. **Select popup positioning**: When a Select field is activated, where does the popup render? Centered over the form? Anchored to the field? The doc does not specify.

3. **Number input validation**: "Text input with validation" -- when does validation happen? On submit? On each keystroke? What is the error display?

4. **Field value display**: How are multi-select values shown? Comma-separated? How are empty optional fields shown? The mockup shows `(none)` for empty fields.

**Recommended additions to the design doc**:

```
Form modal editing UX:
- Text field: Enter opens an InputModal popup (reuse existing input.rs pattern).
  On submit, value is captured into the form field and the InputModal closes.
  The form modal remains visible underneath.
- Select field: Enter opens a SelectModal popup (reuse existing select.rs pattern).
  On submit, selected value is captured. The SelectModal closes.
- Number field: Same as Text, but validate is_numeric on submit. Show error toast if invalid.
- TextArea field: Enter launches $EDITOR. On return, value is captured.
- Multi-select field: Show as comma-separated. Enter opens SelectModal with toggle behavior.
- Empty values display as "(none)" in dim style.
- Required field label prefix: "* " in theme::TEXT_ACCENT color.
- Submit (S): validate all required fields are filled. If any are empty, show toast
  "Required field X is empty" and move cursor to that field.

Form state machine:
  enum FormState {
      Navigating,           // j/k between fields, Enter to edit, S to submit
      EditingText(usize),   // InputModal open for field at index
      EditingSelect(usize), // SelectModal open for field at index
  }
```

---

## 5. $EDITOR Integration

**Verdict: GREEN -- existing code is directly reusable**

### Existing pattern

`app.rs` lines 167-196 implement the full $EDITOR suspend/resume cycle:
1. Set `pending_editor_slug` field
2. In the main loop (before `terminal.draw`), check the field
3. `crossterm::terminal::disable_raw_mode()`
4. `crossterm::execute!(stdout, LeaveAlternateScreen, Show)`
5. `std::process::Command::new(&editor).arg(&path).status()`
6. `crossterm::terminal::enable_raw_mode()`
7. `crossterm::execute!(stdout, EnterAlternateScreen, Hide)`
8. `terminal.clear()`

### The problem

This code lives inside `App::run()` and uses `terminal: &mut Terminal<impl Backend>`. A `ScreenPlugin` does NOT have access to the terminal -- it only has `Frame` (in `render`) and `KeyEvent` (in `handle_key`). The plugin **cannot** call `disable_raw_mode` or `LeaveAlternateScreen` from within `handle_key()`.

### Solution path

The design doc correctly identifies this: comments use `$EDITOR`. But it does not address the terminal access problem. The solution is:

1. Add a `PluginAction::LaunchEditor(PathBuf)` variant to `PluginAction`
2. The App's event loop handles this variant the same way it handles `pending_editor_slug`
3. After the editor exits, the App reads the temp file and calls `plugin.on_notify("editor_result:contents...")` or a dedicated method

**This is a design gap**, but it is small and the solution is obvious. An agent with access to `app.rs` would likely figure this out. The `PluginAction` enum is explicitly designed to be extended.

**Recommended addition**:

```rust
pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { path: PathBuf, callback_id: String },
}
```

---

## 6. State Machine Complexity

**Verdict: GREEN -- well-documented, existing patterns to study**

### JIRA plugin states

The design doc implicitly defines these states through its flow descriptions:

| State | Described in section |
|-------|---------------------|
| Loading | "Loading State" section with ASCII mockup |
| Board | "Screen Layout: Kanban Board" with full keybinding table |
| DetailModal | "Issue Detail Modal" with keybindings |
| TransitionPicker | "Transition Picker Modal" with keybindings |
| TransitionFieldForm | "Transition flow with required fields" paragraph |
| CreateSelectProject | "Issue Creation Flow: Step 1" with mockup |
| CreateSelectType | "Issue Creation Flow: Step 2" with mockup |
| CreateForm | "Issue Creation Flow: Step 3" with mockup |
| ErrorModal | "Error Modal" with mockup |
| Refreshing | "When refreshing (data already on screen)" |

This maps cleanly to:
```rust
enum JiraScreen {
    Board,
    Loading,
}

enum JiraModal {
    Detail { ... },
    TransitionPicker { ... },
    TransitionFieldForm { ... },
    CreateSelectProject { ... },
    CreateSelectType { ... },
    CreateForm { ... },
    Error { ... },
}
```

### Existing state machine complexity

The most complex existing screen is `app.rs` itself, which manages `ScreenId` + `modal_stack` + `Focus` + per-screen state structs. The `switch.rs` screen has a multi-step flow (SwitchState with prompts). But the JIRA plugin's state machine is more complex than any single existing screen.

However, the design doc's `plugin-architecture.md` provides the exact pattern:
```rust
struct JiraPlugin {
    modal: Option<JiraModal>,
}
// render: draw board, then overlay modal if Some
// handle_key: modal gets first crack, then board
```

This is well-specified enough. The state transitions are implicit in the keybinding tables (e.g., `Enter` on board -> DetailModal, `s` in board or detail -> TransitionPicker, `n` in board -> CreateSelectProject chain).

**One gap**: The transition from TransitionPicker to TransitionFieldForm is described in prose ("if the selected transition has required fields, present a field input form") but not in the keybinding table. An agent would need to read the prose carefully. Consider adding to the transition picker keybinding table:

```
| Enter | Apply transition (if no required fields) or open field form (if required fields) |
```

---

## 7. Overall Assessment

### TUI Readiness Score: 3/6 GREEN, 2/6 YELLOW, 1/6 RED

| Component | Rating | Blocker? |
|-----------|--------|----------|
| Kanban Board Rendering | YELLOW | Horizontal scroll algorithm unspecified |
| Plugin-Owned Modals | GREEN | Strong existing patterns |
| Issue Detail Modal | YELLOW | Field navigation and scroll mechanics need spec |
| Form Modal | RED | New widget pattern, nested modal UX unspecified |
| $EDITOR Integration | GREEN | Needs PluginAction::LaunchEditor but path is obvious |
| State Machine | GREEN | Well-documented, clear patterns |

### Critical Gaps (must fix before agent implementation)

1. **Horizontal scroll algorithm for kanban board**: Specify `MIN_COL_WIDTH`, `col_scroll_offset`, viewport window calculation, and the constraint strategy. Without this, an agent will either compress all columns (broken at 8+ statuses) or guess at an implementation that may not match intent.

2. **Form modal field editing UX**: Specify whether text editing is inline or popup, how select fields open, and the `FormState` enum. This is the single most complex new widget and it has the least specification. An agent will waste significant effort guessing the interaction model.

3. **Issue card format**: Specify whether kanban cards are single-line (like existing `issue_board.rs`) or multi-line (like the ASCII mockup suggests). This determines whether the agent uses simple `ListItem::new(Line::from(...))` or needs multi-line `ListItem` height calculation.

### Patterns to Extract as "Copy This" References

These existing code patterns should be called out explicitly in the design doc as templates:

| Pattern | Source file | What to copy |
|---------|------------|--------------|
| Kanban column rendering | `issue_board.rs:296-378` | Layout::horizontal, List per column, Block with focused/unfocused border, build_issue_item |
| Modal overlay rendering | `modals/input.rs:172-224` | centered_rect, Clear, Block with MODAL_BORDER, Layout::vertical for inner sections |
| Selection list modal | `modals/select.rs:87-128` | j/k navigation, Enter select, List with theme::selected() |
| Scrollable modal content | `modals/help.rs:78-120` | scroll_offset, skip/take visible rows pattern |
| Footer hint bar | `keyhints.rs:18-63` | Span styling for key:desc pairs, right-aligned status |
| $EDITOR suspend/resume | `app.rs:167-196` | disable_raw_mode, LeaveAlternateScreen, Command, re-enable |
| Toast notification | `widgets/toast.rs` | Toast::new, render at bottom of area |
| Theme constants | `theme.rs` | All semantic colors, style helpers (selected, dim, accent, etc.) |

### New Widgets/Patterns Needed

| Widget | Complexity | Notes |
|--------|-----------|-------|
| Horizontal-scrolling kanban | Medium | Viewport windowing over `Layout::horizontal`. No ratatui built-in. Must compute `col_scroll_offset` and render only visible columns. |
| Multi-line issue card | Low | `ListItem` with multiple `Line`s. ratatui supports this natively but the height is fixed per item. |
| Multi-field form | High | New pattern: Vec of FormField, j/k cursor, per-type edit triggers, nested modal overlays. Nothing in the codebase to copy. |
| Scrollable field list in modal | Medium | Hybrid of `help.rs` scroll pattern + `select.rs` cursor highlight. Build field lines, track cursor + scroll offset, render visible window. |
| Loading spinner indicator | Low | Animated "Refreshing..." text in header. Can be a simple frame counter toggling between spinner chars. |

### Recommendation

**Do not hand this to an autonomous agent yet.** Fix the three critical gaps above first. The form modal (RED) is the largest risk -- it requires a novel widget pattern that accounts for nested modal state (editing a select field opens a popup over the form). Without explicit specification of this interaction, an agent will either build something that does not compile (trying to nest the App's modal system) or produce a UX that diverges from intent.

After addressing the gaps, the implementation order should be:
1. Phase 1a (board rendering) -- agent can start immediately after horizontal scroll spec is added
2. Phase 1b (detail modal) -- agent can start after field navigation spec is added
3. Phase 1c/1d (form modal) -- agent should start only after form UX is fully specified
