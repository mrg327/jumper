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

    /// Form submitted, waiting for API response.
    Submitting,

    /// API returned validation errors. Fields marked with errors.
    ValidationError { cursor: usize, errors: HashMap<String, String> },
}
```

### State Transitions

```
Navigating
  │
  ├── j/k          → Navigating (move cursor)
  ├── Enter (text) → EditingText (activate inline edit)
  ├── Enter (select) → SelectOpen (show dropdown)
  ├── Enter (unsupported) → no-op (field is disabled)
  ├── S            → Submitting (send create request)
  ├── Esc          → close form, return PluginAction::Back
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
| `MultiSelect` | Comma-separated names | Inline dropdown with toggle | `[{ "id": "..." }, ...]` |
| `Unsupported` | `(unsupported type)` in dim | Not editable (Enter is no-op) | Omitted from POST |

## Field Indicators (Color-Coded Prefixes)

Each field row has a prefix character indicating its state:

| Prefix | Color | Meaning |
|--------|-------|---------|
| `*` | Green (`theme::accent()`) | Required field, has a value |
| `*` | Red (`theme::error()`) | Required field, empty (needs attention) |
| (space) | Normal | Optional field |
| `~` | Dim (`theme::dim()`) | Unsupported field type (disabled) |
| `!` | Red (`theme::error()`) | Validation error from API |

### Field Row Layout

```
 {prefix}{name}:{padding}{value}{suffix}
```

- `prefix`: 1 char (`*`, `~`, `!`, or space)
- `name`: field display name, right-padded to align colons
- `value`: current value (or placeholder text in dim)
- `suffix`: `[▼]` for select fields (dim when not focused)
- Selected row: highlighted background (`theme::selection()`)
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
- **Height**: field_count + 4 (title + padding + footer). Max: terminal_height - 4. If more fields than fit, the field list scrolls internally.
- **Position**: Centered on screen using `centered_rect()` pattern from existing modals.
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

## Implementation Notes

### Rendering

The form is rendered inside the plugin's `render()` method as an overlay:

```rust
fn render_form(&self, frame: &mut Frame, area: Rect) {
    let form_area = centered_rect(60, field_count + 4, area);

    // Clear background
    frame.render_widget(Clear, form_area);

    // Draw border with title
    let block = Block::bordered().title(format!(" New Issue: {} / {} ", project, issue_type));
    let inner = block.inner(form_area);
    frame.render_widget(block, form_area);

    // Render each field row
    for (i, field) in fields.iter().enumerate() {
        let row_area = Rect { y: inner.y + i as u16, height: 1, ..inner };
        self.render_field_row(frame, row_area, field, i == cursor, &state);
    }

    // If SelectOpen, render dropdown overlay
    if let FormState::SelectOpen { field_cursor, dropdown_cursor } = &self.form_state {
        self.render_dropdown(frame, inner, *field_cursor, *dropdown_cursor);
    }

    // Render footer with keybindings
    let footer_area = Rect { y: form_area.y + form_area.height - 1, height: 1, ..form_area };
    // ...
}
```

### Data Flow

```
User fills fields
       ↓
FormState tracks all values: Vec<(EditableField, Option<String>)>
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

### `centered_rect` Helper

Reuse the existing pattern from `modals/`:

```rust
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
```
