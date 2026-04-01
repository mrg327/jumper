# Horizontal Scroll Kanban Specification

This document specifies the horizontal scrolling behavior for the JIRA plugin's kanban board when there are more workflow status columns than fit on screen.

## Overview

JIRA workflows can have 5-10+ statuses. At 25 chars minimum per column, a standard terminal can display 3-5 columns at once. When the total column count exceeds the visible window, the board scrolls horizontally with cursor-follows behavior.

## Core Concepts

```
Full column set (9 statuses):
[Backlog] [Todo] [In Prog] [Review] [QA] [Staging] [UAT] [Done] [Closed]

Terminal fits 5 columns at min_width=25:
         ┌─── viewport (5 cols) ───┐
[Backlog] [Todo] [In Prog] [Review] [QA] [Staging] [UAT] [Done] [Closed]
                                     ↑
                              selected column
```

### State

```rust
struct BoardState {
    /// Total number of status columns
    columns: Vec<StatusColumn>,

    /// Index of the selected column (0-based, in the full column set)
    selected_col: usize,

    /// Index of the first visible column (horizontal scroll offset)
    scroll_offset: usize,

    /// Row cursor within the selected column
    selected_row: usize,

    /// Per-column vertical scroll offsets. Indexed by full column index (not viewport-relative).
    /// Initialize all entries to 0. Resize (via .resize(new_len, 0)) whenever columns change
    /// after a refresh. Access defensively: col_scroll_offsets.get(col).copied().unwrap_or(0).
    col_scroll_offsets: Vec<usize>,
}
```

## Layout Algorithm

### Step 1: Determine Visible Column Count

```rust
const MIN_COL_WIDTH: u16 = 25;  // minimum usable width per column (including border)

fn visible_col_count(board_width: u16, total_cols: usize) -> usize {
    let max_fit = (board_width / MIN_COL_WIDTH) as usize;
    max_fit.min(total_cols).max(1)  // at least 1 column visible
}
```

Examples:
- 120-col terminal → `120 / 25 = 4` columns visible
- 80-col terminal → `80 / 25 = 3` columns visible
- 200-col terminal → `200 / 25 = 8` columns visible

### Step 2: Distribute Width Among Visible Columns

Visible columns share the board width equally. Any remainder pixels go to the leftmost columns:

```rust
fn column_widths(board_width: u16, visible_count: usize) -> Vec<u16> {
    let base_width = board_width / visible_count as u16;
    let remainder = (board_width % visible_count as u16) as usize;

    (0..visible_count)
        .map(|i| if i < remainder { base_width + 1 } else { base_width })
        .collect()
}
```

Use `Constraint::Length(width)` for each column in `Layout::horizontal()`.

### Step 3: Determine Visible Slice

The visible columns are `columns[scroll_offset..scroll_offset + visible_count]`.

## Scroll Behavior: Cursor-Follows

The selected column is **always visible**. When the cursor moves past the viewport edge, the viewport shifts to keep it visible.

### `h` (move left)

```rust
fn move_left(&mut self) {
    if self.selected_col > 0 {
        self.selected_col -= 1;
        // If cursor moved before the viewport, shift viewport left
        if self.selected_col < self.scroll_offset {
            self.scroll_offset = self.selected_col;
        }
    }
}
```

### `l` (move right)

```rust
fn move_right(&mut self, visible_count: usize) {
    if self.selected_col < self.columns.len() - 1 {
        self.selected_col += 1;
        // If cursor moved past the viewport, shift viewport right
        if self.selected_col >= self.scroll_offset + visible_count {
            self.scroll_offset = self.selected_col - visible_count + 1;
        }
    }
}
```

### Visual Example

```
9 total columns, viewport shows 5:

Initial state (selected_col=0, scroll_offset=0):
  [>Backlog] [Todo] [In Prog] [Review] [QA]
  ○ ○ ○ ○ ○ ● ● ● ●

Press l 4 times (selected_col=4, scroll_offset=0):
  [Backlog] [Todo] [In Prog] [Review] [>QA]
  ○ ○ ○ ○ ○ ● ● ● ●

Press l (selected_col=5, scroll_offset=1):
  [Todo] [In Prog] [Review] [QA] [>Staging]
  ● ○ ○ ○ ○ ● ● ● ●

Press l (selected_col=6, scroll_offset=2):
  [In Prog] [Review] [QA] [Staging] [>UAT]
  ● ● ○ ○ ○ ○ ● ● ●

Press h (selected_col=5, scroll_offset=2):
  [In Prog] [Review] [QA] [>Staging] [UAT]
  ● ● ○ ○ ○ ○ ● ● ●

Press h 3 times (selected_col=2, scroll_offset=2):
  [>In Prog] [Review] [QA] [Staging] [UAT]
  ● ● ○ ○ ○ ○ ● ● ●

Press h (selected_col=1, scroll_offset=1):
  [>Todo] [In Prog] [Review] [QA] [Staging]
  ● ○ ○ ○ ○ ○ ● ● ●
```

## Scroll Position Indicator (Dots)

A row of dots below the kanban columns indicates which columns are currently visible.

### Layout

```
│ Review   │ QA       │>Staging  │ Done    │
└──────────┴──────────┴──────────┴─────────┘
                ○ ○ ● ● ● ● ○ ○ ○
```

- One dot per column in the full column set
- `●` (filled) = column is in the viewport
- `○` (empty) = column is off-screen
- Dots are centered horizontally below the board
- Only rendered when `total_columns > visible_columns` (no dots if everything fits)

### Rendering

```rust
fn render_scroll_dots(
    frame: &mut Frame,
    area: Rect,       // single row below the board
    total: usize,
    scroll_offset: usize,
    visible: usize,
) {
    let dots: String = (0..total)
        .map(|i| {
            if i >= scroll_offset && i < scroll_offset + visible { "●" } else { "○" }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let paragraph = Paragraph::new(dots)
        .alignment(Alignment::Center)
        .style(theme::dim());
    frame.render_widget(paragraph, area);
}
```

## Issue Card Format (Three-Line)

Each issue in a column is displayed as a three-line card:

```
Line 1: {key}  {issue_type}
Line 2: {summary truncated to column width}
Line 3: {priority} · {story_points}pts
```

### Card Rendering

Within a column of width W (minus 2 for borders = W-2 usable):

```
 HMI-103  Story          ← key left-aligned, type right-aligned, dim
 Fix navigation focu...  ← summary, truncated with "..."
 High · 3pts             ← priority colored by level, points dim
```

- **Line 1**: Issue key in accent color. Issue type in dim, right-aligned.
- **Line 2**: Summary, truncated to `col_width - 2` chars with `...` if longer.
- **Line 3**: Priority name colored using existing theme constants. Separator ` · `. Story points with "pts" suffix in dim. If no points, just priority. If no priority, just points. If neither, empty line.

Priority coloring uses existing theme constants:
- "Highest", "High" → theme::PRIORITY_HIGH (red)
- "Medium" → theme::PRIORITY_MEDIUM (yellow)
- "Low", "Lowest" → theme::PRIORITY_LOW (blue — `Color::Blue` in theme.rs)

JIRA has 5 priority levels but theme.rs has 3 constants. Map Highest→HIGH and Lowest→LOW.

### Selected Card

The selected card (at `selected_col`, `selected_row`) has:
- Inverted/highlighted background (`theme::selected()`) — this is the **only** selection indicator. Do NOT add a `>` marker prefix; use background highlight only for consistency with the rest of the TUI.

### Card Spacing

Cards are separated by 1 blank line within each column. This means each card occupies 4 rows (3 content + 1 separator). Maximum visible cards per column: `column_height.saturating_sub(1) / 4` (minus 1 for the column header; `saturating_sub` prevents u16 underflow when `column_height` is 0 on tiny terminals).

### Column Vertical Scroll

When a column has more issues than visible rows (i.e., `issue_count * 4 > column_height.saturating_sub(1)`), the column scrolls vertically with cursor-follows behavior.

#### State

`col_scroll_offsets: Vec<usize>` is part of `BoardState` (see the full struct definition in the Core Concepts section above). It is indexed by `selected_col` (full column index, not viewport-relative). Initialize all entries to `0`.

#### Derived constants

```rust
// Minus 1 for column header row; divided by 4 for 3-line card + 1 separator line.
// saturating_sub prevents u16 underflow when column_height is 0 (very small terminal).
let max_visible_cards = column_height.saturating_sub(1) / 4;
```

#### Behavior

- `j`/`k` moves `selected_row` within the selected column as normal
- After each `j`/`k`, adjust `col_scroll_offsets[selected_col]` to keep `selected_row` visible:
  - If `selected_row < col_scroll_offsets[selected_col]`
    → `col_scroll_offsets[selected_col] = selected_row`
  - If `selected_row >= col_scroll_offsets[selected_col] + max_visible_cards`
    → `col_scroll_offsets[selected_col] = selected_row - max_visible_cards + 1`
- Render issues starting from `col_scroll_offsets[col]`, up to `max_visible_cards` issues per column
- `g` sets `selected_row = 0` and `col_scroll_offsets[selected_col] = 0`
- `G` sets `selected_row = last_issue_index` and
  `col_scroll_offsets[selected_col] = last_issue_index.saturating_sub(max_visible_cards - 1)`

#### Reset on column change

When `h`/`l` moves to a different column, **preserve** that column's `selected_row` and `col_scroll_offsets[col]` from the last time it was visited. Do not reset them. This means the cursor returns to where the user left off in each column.

### Column Header

```
┌─ In Progress (3) ─────┐
```

Column header shows the status name and issue count. The selected column header uses accent color. Other headers use dim color.

## Scroll State Preservation Across Refresh

When a refresh completes with new data:

1. Save `(selected_issue_key, selected_status_name)` before applying new data
2. Apply new data (columns may change — new statuses, removed statuses)
3. Find the column with `name == selected_status_name`. Set `selected_col` to its index.
4. Within that column, find the issue with `key == selected_issue_key`. Set `selected_row` to its index.
5. If the status no longer exists, clamp `selected_col` to `columns.len() - 1`.
6. If the issue no longer exists in the column, clamp `selected_row` to the column's issue count.
7. Recalculate `scroll_offset` to ensure `selected_col` is visible.
8. **Resize `col_scroll_offsets`** to match the new column count: append `0` for new columns, truncate for removed columns. Use `col_scroll_offsets.resize(new_column_count, 0)`. Do this before step 3 so the index into `col_scroll_offsets` is always valid. When rendering, always access as `col_scroll_offsets.get(col).copied().unwrap_or(0)` as a defensive fallback.

## No-Scroll Case

When `total_columns <= visible_count_at_min_width`:
- No horizontal scrolling needed
- Columns expand to fill available width equally (like existing issue board)
- Scroll dots are NOT rendered
- `scroll_offset` stays at 0

## Full Board Layout

```
┌─ JIRA: HMI  ↻ 14:25  ─────────────────────────────────────────────┐
│ To Do (2)    │ In Prog (3) │>Review (1)  │ QA (0)     │ Done (5)  │
│              │             │             │            │           │
│ HMI-110  Bug │ HMI-103 Sty │>HMI-102 Tsk│            │ HMI-99 Sty│
│ Fix crash    │ Nav focus   │ Unit tests  │            │ Perf fix  │
│ P1           │ High · 3pts │ Med · 2pts  │            │ Done      │
│              │             │             │            │           │
│ HMI-115  Sty │ HMI-107 Sub │             │            │ HMI-98 Bug│
│ Tab order    │ CSS states  │             │            │ Init fix  │
│ High · 2pts  │ Med         │             │            │ Low · 1pt │
│              │             │             │            │           │
│              │ HMI-112 Tsk │             │            │ ...       │
│              │ Docs update │             │            │           │
│              │ Low · 1pt   │             │            │           │
├──────────────┴─────────────┴─────────────┴────────────┴───────────┤
│                        ● ● ● ● ● ○ ○ ○                           │
├───────────────────────────────────────────────────────────────────┤
│ hjkl:nav  s:transition  c:comment  Enter:detail  p:proj  R:refresh│
│ n:new  D:toggle-done  Esc:back         Last sync: 14:25:03       │
└───────────────────────────────────────────────────────────────────┘
```

### Vertical Layout Split

```rust
let chunks = Layout::vertical([
    Constraint::Length(1),    // Header (project name, refresh indicator)
    Constraint::Min(5),       // Board area (columns + cards)
    Constraint::Length(1),    // Scroll dots (only if scrolling)
    Constraint::Length(2),    // Footer (keybindings + last sync)
]).split(area);
```

The board area is then split horizontally for columns:

```rust
let visible_cols = visible_col_count(board_area.width, total_columns);
let widths = column_widths(board_area.width, visible_cols);
let constraints: Vec<Constraint> = widths.iter().map(|&w| Constraint::Length(w)).collect();
let col_areas = Layout::horizontal(constraints).split(board_area);
```

## Loading State

On initial load (no data yet):

```
┌─ JIRA ───────────────────────────────────────┐
│                                               │
│                                               │
│            ⠙ Loading issues...                │
│                                               │
│                                               │
│                                               │
├───────────────────────────────────────────────┤
│ Esc:back                                      │
└───────────────────────────────────────────────┘
```

Spinner cycles through braille characters: `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏` on each 250ms tick.

## Keybindings (Board)

| Key | Action |
|-----|--------|
| `h` / `Left` | Select previous column (scroll viewport if needed) |
| `l` / `Right` | Select next column (scroll viewport if needed) |
| `j` / `Down` | Select next issue in column |
| `k` / `Up` | Select previous issue in column |
| `g` | Jump to first issue in column |
| `G` | Jump to last issue in column |
| `Enter` | Open issue detail modal |
| `s` | Transition selected issue (show transition picker) |
| `c` | Comment on selected issue (open $EDITOR) |
| `n` | Create new issue |
| `p` | Cycle project filter |
| `D` | Toggle Done column visibility |
| `R` | Manual refresh |
| `Esc` / `q` | Back to dashboard |
