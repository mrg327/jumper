# Form Modal UX Specification

This document specifies the form modal widget used for JIRA issue creation and transition field entry. This is a **new widget pattern** with no direct analog in the existing codebase.

## Overview

The form modal is a centered overlay that displays all fields at once in a vertical list. The user navigates between fields with `j`/`k`, edits inline with `Enter`, and submits the entire form with `S`.

```
┌─ New Issue: HMI / Bug ──────────────────────────┐
│                                                   │
│  *Summary:    [Fix crash on back button___]       │
│  *Priority:   High                       [▼]     │
│   Labels:     frontend, accessibility             │
│   Component:  hmi-nav                    [▼]     │
│   Points:     3                                   │
│  ~Fix Version: (unsupported type)                 │
│                                                   │
│  j/k:nav  Enter:edit  S:submit  Esc:cancel        │
└───────────────────────────────────────────────────┘
```

## State Machine

```rust
enum FormState {
    /// Navigating between fields. j/k moves cursor, Enter enters edit mode.
    Navigating { cursor: usize },

    /// Editing a text or number field inline. Typing modifies the value.
    EditingText { cursor: usize, buffer: String, cursor_pos: usize },

    /// A select dropdown is open for the focused field.
    SelectOpen { field_cursor: usize, dropdown_cursor: usize },

    /// A multi-select dropdown is open. Space toggles items, Enter confirms.
    MultiSelectOpen {
        field_cursor: usize,
        dropdown_cursor: usize,
        checked: HashSet<usize>,  // indices of selected items in allowed_values
    },

    /// Form submitted, waiting for API response.
    Submitting,

    // NOTE: FormState tracks UI state only (cursor, edit buffer, dropdown).
    // Actual field values are stored in a parallel Vec alongside the FormState:
    //   values: Vec<(EditableField, Option<FieldValue>)>
    // This Vec lives in the JiraModal variant (CreateForm or TransitionFields),
    // NOT inside FormState. FormState.cursor indexes into this Vec.

    /// API returned validation errors. Fields marked with errors.
    ValidationError { cursor: usize, errors: HashMap<String, String> },
}
```

### State Transitions

```
Navigating
  │
  ├── j/k               → Navigating (move cursor)
  ├── Enter (text)      → EditingText (activate inline edit)
  ├── Enter (select)    → SelectOpen (show dropdown)
  ├── Enter (multiselect) → MultiSelectOpen { checked = currently-selected indices }
  ├── Enter (unsupported) → no-op (field is disabled)
  ├── S                 → Submitting (send create request)
  ├── Esc               → close form, return PluginAction::Back
  │
EditingText
  │
  ├── typing       → EditingText (update buffer)
  ├── Enter        → Navigating (save value, move to next field)
  ├── Esc          → Navigating (discard changes, stay on field)
  │
SelectOpen
  │
  ├── j/k          → SelectOpen (move dropdown cursor)
  ├── Enter        → Navigating (select value, save, move to next field)
  ├── Esc          → Navigating (cancel, keep previous value)
  │
MultiSelectOpen
  │
  ├── j/k          → MultiSelectOpen (move dropdown cursor)
  ├── Space        → MultiSelectOpen (toggle checked state of current item)
  ├── Enter        → Navigating (confirm selections, save as comma-separated names)
  ├── Esc          → Navigating (cancel, keep previous selections)
  │
Submitting
  │
  ├── API success  → close form, toast "Created HMI-116"
  ├── API error    → ValidationError (mark fields, jump to first error)
  │
ValidationError
  │
  ├── (same as Navigating, but error markers shown)
  ├── S            → Submitting (retry)
  ├── Esc          → close form
```

## Field Types and Edit Behavior

| FieldType | Display (Navigating) | Edit Mode | Value in POST |
|-----------|---------------------|-----------|---------------|
| `Text` | Value or `[empty]` | Inline text input, cursor visible | `"string"` |
| `Number` | Value or `[empty]` | Inline text input with number validation | `number` |
| `Select` | Selected value name + `[▼]` | Inline dropdown below field | `{ "id": "..." }` |
| `MultiSelect` | Comma-separated names | Inline dropdown with checkboxes (Space to toggle, Enter to confirm) | `[{ "id": "..." }, ...]` |
| `TextArea` | First line + `...` | Opens `$EDITOR` via `PluginAction::LaunchEditor`. Result returned via `on_editor_complete()`. | ADF JSON (via `text_to_adf()`) |
| `Date` | `YYYY-MM-DD` or `[empty]` | Inline text input with date validation (`YYYY-MM-DD` format) | `"YYYY-MM-DD"` |
| `Unsupported` | `(unsupported type)` in dim | Not editable (Enter is no-op) | Omitted from POST |

## Field Indicators (Color-Coded Prefixes)

Each field row has a prefix character indicating its state:

| Prefix | Color | Meaning |
|--------|-------|---------|
| `*` | Green (`theme::accent()`) | Required field, has a value |
| `*` | Red (`theme::TEXT_ERROR`) | Required field, empty (needs attention) |
| (space) | Normal | Optional field |
| `~` | Dim (`theme::dim()`) | Unsupported field type (disabled) |
| `!` | Red (`theme::TEXT_ERROR`) | Validation error from API |

### Field Row Layout

```
 {prefix}{name}:{padding}{value}{suffix}
```

- `prefix`: 1 char (`*`, `~`, `!`, or space)
- `name`: field display name, right-padded to align colons
- `value`: current value (or placeholder text in dim)
- `suffix`: `[▼]` for select fields (dim when not focused)
- Selected row: highlighted background (`theme::selected()`)
- Error text: shown after the value in red: `─ error message`

### Example Rendering

```
 *Summary:      [Fix crash on back___]          ← EditingText, cursor visible
 *Priority:     High                    [▼]     ← Navigating, has value (green *)
  Labels:       frontend, a11y                  ← Optional, has value
 *Component:    [select...]             [▼]     ← Required, empty (red *)
  Points:       [empty]                         ← Optional, empty (dim placeholder)
 ~Fix Version:  (unsupported type)              ← Disabled (dim ~)
 !Team:         devops ─ Not a valid team       ← Validation error (red !)
```

## Inline Text Editing

When `Enter` is pressed on a `Text` or `Number` field:

1. Field value area becomes an editable text input
2. Cursor appears at the end of the current value (or at position 0 if empty)
3. Standard text input keys work:
   - Typing inserts characters
   - Backspace/Delete remove characters
   - Home/End move cursor
   - Left/Right move cursor within the value
4. `Enter` saves the value and moves cursor to the next field
5. `Esc` discards changes and returns to Navigating on the same field

For `Number` fields: validate on save. If not a valid number, show inline error and stay in edit mode.

The text input area is bounded to the right side of the form (from the colon to the right border minus padding). The value is truncated with `...` if it exceeds the area when not editing.

### Terminal Cursor Positioning

When in `EditingText` state, the plugin must call:

```rust
frame.set_cursor_position(Position::new(
    field_value_x + cursor_pos as u16,
    field_row_y,
))
```

after rendering the field row. This shows the blinking terminal cursor at the correct position within the text input. Without this call the cursor remains at (0, 0), which looks broken. `field_value_x` is the x-coordinate where the field's value area begins (after the label and colon), and `cursor_pos` is the byte offset of the edit cursor within the buffer.

## Inline Select Dropdown

When `Enter` is pressed on a `Select` field:

1. A bordered dropdown list appears directly below the field row
2. The dropdown overlays fields below it (they are temporarily hidden)
3. Navigation:
   - `j`/`k` or `Up`/`Down` move the selection within the dropdown
   - `Enter` selects the highlighted value and closes the dropdown
   - `Esc` closes the dropdown without changing the value
4. The dropdown shows all `allowed_values` from the field metadata
5. The currently selected value (if any) is pre-highlighted
6. If the list exceeds the available vertical space, it scrolls internally

### Dropdown Layout

```
  >Priority:     [select...]
                 ┌───────────────┐
                 │ Highest       │
                 │>High          │  ← highlighted (accent color)
                 │ Medium        │
                 │ Low           │
                 │ Lowest        │
                 └───────────────┘
```

- Dropdown width: max of (field value area width, longest option + 2)
- Dropdown height: min of (option count, available rows below field, 8)
- Dropdown position: starts at the column of the value area, row below the field
- If not enough room below, show above the field

## Inline MultiSelect Dropdown

When `Enter` is pressed on a `MultiSelect` field:

1. The field value area changes to `[select multiple...]` as a placeholder
2. A bordered checkbox list appears directly below the field row, overlaying fields below
3. Navigation:
   - `j`/`k` or `Up`/`Down` move `dropdown_cursor` within the list
   - `Space` toggles the checkbox for the item at `dropdown_cursor` (adds/removes its index from `checked`)
   - `Enter` confirms all checked items and closes the dropdown; saves as comma-separated `AllowedValue.name` strings for display, and the full `Vec<AllowedValue.id>` for the POST body
   - `Esc` closes the dropdown without changing the current selections
4. The dropdown is pre-populated from the current field value: any already-selected IDs set their corresponding indices in `checked`
5. If the list exceeds the available vertical space, it scrolls internally

### MultiSelect Dropdown Layout

```
  Component:  [select multiple...]
              ┌─────────────────┐
              │ [x] hmi-nav     │  ← checked (accent color)
              │ [ ] hmi-core    │  ← unchecked
              │ [x] platform    │  ← checked (accent color)
              │ [ ] tools       │  ← unchecked
              └─────────────────┘
```

- `[x]` rendered in `theme::accent()` (cyan) for checked items
- `[ ]` rendered in normal color for unchecked items
- The row at `dropdown_cursor` has `theme::selected()` background regardless of checked state
- Dropdown width: max of (field value area width, longest option name + 6 for `[ ] ` prefix and padding)
- Dropdown height: min of (option count, available rows below field, 8)
- Dropdown position: same rules as `SelectOpen` (below field, flip above if insufficient room)
- On `Enter`: collect all checked items' `AllowedValue.id` values for the POST body as `[{ "id": "..." }, ...]`

## Form Submission Flow

1. User presses `S`
2. **Pre-validation**: Check all required fields (`*` prefix) have values. If any are empty, jump cursor to the first empty required field and flash the row red. Do NOT send the request.
3. **Build POST body**: Construct `{ "fields": { ... } }` from filled values. Use `AllowedValue.id` for select fields. Set `assignee.accountId` from the cached `/myself` response. Set `project.key` and `issuetype.id` from the wizard steps.
4. **Send**: Transition to `Submitting` state. Show spinner on the submit line.
5. **On success**: Close form. Show toast "Created HMI-116". Trigger board refresh.
6. **On error**: Parse JIRA error response. Map field-level errors to form fields via field ID. Set `!` prefix and error message on each. Jump cursor to first error field. User can fix and press `S` again.

### Submitting State UI

```
  *Summary:     Fix crash on back
  *Priority:    High
   Labels:      frontend

  ⠙ Creating issue...                    ← spinner replaces footer
```

## Form for Transition Required Fields

The same form widget is reused for transition fields (e.g., Resolution when transitioning to Done). The differences:

- Title: "Transition HMI-103 → Done" instead of "New Issue"
- Fields come from `JiraTransition.required_fields` instead of createmeta
- Submit key is `Enter` (not `S`) — there's usually only 1-2 fields
- Esc cancels the transition
- On success, the transition POST is executed with the filled fields

```
┌─ Transition HMI-103 → Done ─────────────────┐
│                                               │
│  *Resolution:  Done                  [▼]     │
│                                               │
│  Enter:apply  Esc:cancel                      │
└───────────────────────────────────────────────┘
```

## Sizing and Positioning

- **Width**: min(terminal_width - 4, 60). Centered horizontally.
- **Height**: field_count + 6 (2 border rows + 1 blank top padding + field rows + 1 blank bottom padding + 1 footer). Max: terminal_height - 4. If more fields than fit, the field list scrolls internally.
- **Position**: Centered on screen using a **new** pixel-absolute `centered_rect(width, height, area)` helper. Do NOT use `crate::modals::centered_rect()` which takes percentages.
- **Background**: Clear the area behind the modal (render `Clear` widget first, then border, then content).

## Keybindings Summary

### Navigating State

| Key | Action |
|-----|--------|
| `j` / `Down` | Move cursor to next field |
| `k` / `Up` | Move cursor to previous field |
| `Enter` | Enter edit mode for focused field |
| `S` | Submit form (pre-validate first) |
| `Esc` | Cancel and close form |
| `g` | Jump to first field |
| `G` | Jump to last field |

### EditingText State

| Key | Action |
|-----|--------|
| Typing | Insert characters |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character after cursor |
| `Left`/`Right` | Move cursor within value |
| `Home`/`End` | Jump to start/end of value |
| `Enter` | Save value, move to next field |
| `Esc` | Discard changes, return to Navigating |

### SelectOpen State

| Key | Action |
|-----|--------|
| `j` / `Down` | Next option |
| `k` / `Up` | Previous option |
| `Enter` | Select highlighted option, close dropdown |
| `Esc` | Cancel, close dropdown |

### MultiSelectOpen State

| Key | Action |
|-----|--------|
| `j` / `Down` | Move dropdown cursor to next item |
| `k` / `Up` | Move dropdown cursor to previous item |
| `Space` | Toggle checked state of item at dropdown cursor |
| `Enter` | Confirm all checked items, close dropdown |
| `Esc` | Cancel, close dropdown (keep previous selections) |

## Implementation Notes

### Rendering

The form is rendered inside the plugin's `render()` method as an overlay:

```rust
fn render_form(&self, frame: &mut Frame, area: Rect) {
    let form_area = centered_rect(60, field_count + 6, area);

    // Clear background
    frame.render_widget(Clear, form_area);

    // Draw border with title
    let block = Block::bordered().title(format!(" New Issue: {} / {} ", project, issue_type));
    let inner = block.inner(form_area);
    frame.render_widget(block, form_area);

    // Calculate label column width: max field name length + 2 (for ": " suffix).
    // This aligns value areas across all fields regardless of name length.
    let label_col_width = fields.iter()
        .map(|(f, _)| f.name.len())
        .max()
        .unwrap_or(10) as u16 + 2;

    // Render each field row
    for (i, field) in fields.iter().enumerate() {
        let row_area = Rect { y: inner.y + i as u16, height: 1, ..inner };
        self.render_field_row(frame, row_area, field, i == cursor, &state, label_col_width);
    }

    // Position the terminal cursor when editing a text field
    if let FormState::EditingText { cursor: field_idx, cursor_pos, .. } = &self.form_state {
        let row_y = inner.y + *field_idx as u16;
        let field_value_x = inner.x + label_col_width; // x where value area starts
        frame.set_cursor_position(Position::new(
            field_value_x + *cursor_pos as u16,
            row_y,
        ));
    }

    // If SelectOpen, render dropdown overlay
    if let FormState::SelectOpen { field_cursor, dropdown_cursor } = &self.form_state {
        self.render_dropdown(frame, inner, *field_cursor, *dropdown_cursor);
    }

    // If MultiSelectOpen, render checkbox dropdown overlay
    if let FormState::MultiSelectOpen { field_cursor, dropdown_cursor, checked } = &self.form_state {
        self.render_multiselect_dropdown(frame, inner, *field_cursor, *dropdown_cursor, checked);
    }

    // Render footer with keybindings (inside inner area, NOT on the border row)
    let footer_area = Rect { y: inner.y + inner.height - 1, height: 1, ..inner };
    // ...
}
```

### Data Flow

```
User fills fields
       ↓
JiraModal::CreateForm / TransitionFields stores Vec<(EditableField, Option<FieldValue>)>
       ↓
On submit: build serde_json::Value from field values
       ↓
Send JiraCommand::CreateIssue { project_key, fields }
       ↓
Background thread executes POST /rest/api/3/issue
       ↓
JiraResult::IssueCreated(key) or JiraResult::Error(e)
       ↓
On success: close form, toast, refresh board
On error: parse errors, mark fields, stay on form
```

### FieldValue

```rust
/// Represents a field value in the form. Used alongside EditableField
/// in a parallel Vec: Vec<(EditableField, Option<FieldValue>)>
/// This Vec lives in JiraModal::CreateForm or JiraModal::TransitionFields,
/// NOT inside FormState (which only tracks UI cursor/edit state).
pub enum FieldValue {
    /// Text or TextArea field value
    Text(String),
    /// Number field value (story points, etc.)
    Number(f64),
    /// Single-select field value (stores the AllowedValue id)
    Select(String),
    /// Multi-select field values (stores AllowedValue ids)
    MultiSelect(Vec<String>),
    /// Date field value
    Date(String),  // "YYYY-MM-DD" format
}
```

When building the POST body, convert FieldValue to serde_json::Value:
- `FieldValue::Text(s)` → `json!(s)` (bare string)
- `FieldValue::Number(n)` → `json!(n)` (bare number)
- `FieldValue::Select(id)` → `json!({ "id": id })` (object with id)
- `FieldValue::MultiSelect(ids)` → `json!(ids.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>())`
- `FieldValue::Date(d)` → `json!(d)` (bare string)

### `centered_rect` Helper

Reuse the existing pattern from `modals/`:

```rust
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
```
