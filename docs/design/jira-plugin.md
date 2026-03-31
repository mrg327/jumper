# Phase 1: JIRA Plugin

## Objective

Implement a JIRA Cloud integration as a full-screen plugin for jm. The plugin enables viewing, transitioning, editing, commenting on, and creating JIRA issues — all from within the TUI. Data always comes from the API; nothing is persisted locally.

## Prerequisites

- Phase 0 complete: plugin system rewrite with `ScreenPlugin` trait (see `plugin-system-rewrite.md`)
- JIRA Cloud instance with REST API v3 access
- API token with appropriate scopes

## Design Principles

1. **Least privilege** — no delete operations, assignee is read-only on existing issues, auto-discover editable fields from JIRA
2. **Always fresh** — data comes from the JIRA API, never saved locally. Auto-refresh every 60s.
3. **Self-contained** — no interaction with local jm stores (projects, issues, journal)
4. **Non-blocking** — API calls run in a background thread; TUI remains responsive
5. **Dynamic** — workflow statuses, required fields, and editable fields are discovered from JIRA, not hardcoded

## Authentication & Configuration

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `JIRA_API_TOKEN` | Yes | Atlassian API token (personal access token) |

### Config File (`~/.jm/config.yaml`)

```yaml
plugins:
  enabled: [pomodoro, notifications, clock, jira]

  jira:
    url: "https://myorg.atlassian.net"
    email: "matt@company.com"
    refresh_interval_secs: 60
    # Optional: explicit custom field IDs (auto-discovered if omitted)
    story_points_field: "customfield_10016"
    sprint_field: "customfield_10020"
```

### Startup Validation

On plugin initialization (`on_enter`):
1. Check `JIRA_API_TOKEN` env var is set → error modal if missing
2. Check `jira.url` and `jira.email` in config → error modal if missing
3. Call `GET /rest/api/3/myself` to validate credentials and retrieve the user's `accountId`. This serves as both the connectivity check and the auth identity lookup. Store the `accountId` for use in JQL queries.
4. If validation fails, show a blocking error modal with actionable message

The `accountId` from `/myself` is used in JQL: `assignee = '<accountId>'` instead of `currentUser()`, which avoids ambiguity with service accounts and ensures correct results.

## API Layer

### HTTP Client

- **Library**: `ureq` — synchronous HTTP client, ~15 crate dependencies vs ~80 for reqwest, no tokio dependency
- **Threading**: All API calls run in a dedicated background thread
- **Communication**: `mpsc::channel` between TUI thread and API thread
- **Connection pooling**: Single `ureq::Agent` per thread lifetime (reuse for connection pooling)

```
┌──────────────┐   Command channel    ┌──────────────┐
│  TUI Thread  │ ──────────────────► │  API Thread  │
│              │                      │              │
│  render()    │   Result channel     │  ureq::Agent │
│  handle_key()│ ◄────────────────── │  (sync HTTP) │
│  on_tick()   │                      │              │
└──────────────┘                      └──────────────┘
```

### Command Types

```rust
enum JiraCommand {
    /// Fetch all issues assigned to the user.
    /// Includes a generation counter to prevent stale overwrites.
    /// The background thread pages through ALL results from /search
    /// (incrementing startAt) and sends the full Vec<JiraIssue> once complete.
    FetchMyIssues { generation: u64 },

    /// Fetch available transitions for an issue (lazy — only on detail open or 's')
    FetchTransitions { issue_key: String },

    /// Transition an issue to a new status, with optional required fields
    TransitionIssue {
        issue_key: String,
        transition_id: String,
        fields: Option<serde_json::Value>,
    },

    /// Update a field on an issue
    UpdateField { issue_key: String, field_id: String, value: serde_json::Value },

    /// Add a comment to an issue (body is ADF JSON, converted from plain text via $EDITOR)
    AddComment { issue_key: String, body: serde_json::Value },

    /// Fetch comments for an issue
    FetchComments { issue_key: String },

    /// Fetch createmeta (required fields) for a project + issue type
    FetchCreateMeta { project_key: String, issue_type_id: String },

    /// Create a new issue
    CreateIssue { project_key: String, fields: serde_json::Value },

    /// Fetch editable fields metadata for an issue (lazy — only on detail open)
    FetchEditMeta { issue_key: String },

    /// Fetch custom field definitions for discovery
    FetchFields,

    /// Cooperative shutdown signal
    Shutdown,
}
```

### Result Types

```rust
enum JiraResult {
    /// Full issue list with generation counter to prevent stale overwrites.
    /// Only applied if generation matches the current expected generation.
    Issues { generation: u64, issues: Vec<JiraIssue> },
    Transitions(String, Vec<JiraTransition>),  // issue_key, transitions
    TransitionComplete(String),                 // issue_key
    TransitionFailed(String, JiraError),        // issue_key, error (for optimistic UI revert)
    FieldUpdated(String, String),               // issue_key, field_id
    CommentAdded(String),                       // issue_key
    Comments(String, Vec<JiraComment>),         // issue_key, comments
    CreateMeta(CreateMetaResponse),
    IssueCreated(String),                       // new issue key
    EditMeta(String, Vec<EditableField>),       // issue_key, editable fields
    Fields(Vec<JiraFieldDef>),                  // custom field definitions
    Error(JiraError),
}
```

### API Endpoints Used

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/rest/api/3/myself` | GET | Validate credentials, retrieve `accountId` (startup) |
| `/rest/api/3/search` | GET | Search issues (JQL: `assignee = '<accountId>'`). Paginated — fetch ALL pages. |
| `/rest/api/3/field` | GET | Discover custom field IDs (story points, sprint) |
| `/rest/api/3/issue/{key}` | GET | Get issue details |
| `/rest/api/3/issue/{key}/transitions` | GET | Get available transitions (includes `fields` for required transition fields) |
| `/rest/api/3/issue/{key}/transitions` | POST | Execute a transition (with optional required fields) |
| `/rest/api/3/issue/{key}` | PUT | Update issue fields |
| `/rest/api/3/issue/{key}/comment` | GET | Get issue comments |
| `/rest/api/3/issue/{key}/comment` | POST | Add a comment |
| `/rest/api/3/issue/{key}/editmeta` | GET | Get editable field metadata |
| `/rest/api/3/issue/createmeta/{projectKey}/issuetypes` | GET | Get issue types for project |
| `/rest/api/3/issue/createmeta/{projectKey}/issuetypes/{issueTypeId}` | GET | Get required fields |
| `/rest/api/3/issue` | POST | Create issue |
| `/rest/api/3/status` | GET | Get all statuses (for workflow discovery) |

### Endpoints NOT Used (Least Privilege)

- `DELETE /rest/api/3/issue/{key}` — **Never called. No delete capability.**
- `PUT /rest/api/3/issue/{key}/assignee` — **Never called. Assignee is read-only on existing issues.**

### Authentication

All requests include:
```
Authorization: Basic base64(email:api_token)
Content-Type: application/json
```

### Rate Limiting

JIRA Cloud has rate limits (varies by plan). The API layer should:
- Respect `Retry-After` headers on 429 responses
- Show a toast when rate-limited ("JIRA rate limit, retrying in Xs")
- Never send concurrent requests for the same resource

## Data Model

### Core Types

```rust
/// A JIRA issue as displayed in the TUI
pub struct JiraIssue {
    pub key: String,              // e.g., "HMI-103"
    pub summary: String,
    pub status: JiraStatus,
    pub priority: Option<String>, // "High", "Medium", etc.
    pub issue_type: String,       // "Bug", "Story", "Task", etc.
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub created: String,          // ISO 8601
    pub updated: String,          // ISO 8601
    pub description: Option<String>, // Plain text (converted from ADF)
    pub sprint: Option<String>,   // Sprint name
    pub epic: Option<EpicInfo>,
    pub story_points: Option<f64>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub project_key: String,      // "HMI"
    pub project_name: String,     // "HMI Framework"
}

pub struct JiraStatus {
    pub name: String,             // "In Progress"
    pub category: StatusCategory, // maps to kanban column grouping
}

/// JIRA has 4 status categories: new, indeterminate, done, undefined.
/// We map these to 3 display categories. `undefined` maps to ToDo.
/// Use #[serde(other)] or manual deserialization with fallback for unknown values.
pub enum StatusCategory {
    ToDo,          // JIRA: "new" or "undefined"
    InProgress,    // JIRA: "indeterminate"
    Done,          // JIRA: "done"
}

pub struct EpicInfo {
    pub key: String,
    pub name: String,
}

/// An allowed value for a select/multi-select field.
/// Display `name` to user, send `{ "id": "..." }` in write bodies.
pub struct AllowedValue {
    pub id: String,
    pub name: String,
}

pub struct JiraTransition {
    pub id: String,
    pub name: String,             // "Start Progress", "Done", etc.
    pub to_status: JiraStatus,
    pub required_fields: Vec<TransitionField>, // from transitions API `fields` object
}

/// A required field attached to a transition (e.g., Resolution for "Done").
pub struct TransitionField {
    pub field_id: String,
    pub name: String,
    pub field_type: FieldType,
    pub allowed_values: Vec<AllowedValue>,
    /// True when the transition requires a comment instead of (or in addition to) structured fields.
    pub is_comment: bool,
}

pub struct JiraComment {
    pub id: String,
    pub author: String,
    pub created: String,          // ISO 8601
    pub body: String,             // Plain text (converted from ADF)
}

pub struct EditableField {
    pub field_id: String,         // "summary", "customfield_10016" (NOT "description")
    pub name: String,             // "Summary", "Story Points"
    pub field_type: FieldType,
    pub required: bool,
    // Display `name` to user, send `{ "id": "..." }` in write bodies
    pub allowed_values: Option<Vec<AllowedValue>>, // For select fields
}

// NOTE: "description" is excluded from editable fields. ADF round-tripping is lossy
// (rich formatting → plain text → simple ADF paragraph), so description is read-only.
// Only simple fields (summary, priority, etc.) are editable via editmeta.

pub enum FieldType {
    Text,
    TextArea,
    Number,
    Select,
    MultiSelect,
    Date,
    /// For field types the TUI cannot render/edit. Show as read-only text.
    /// If a required unsupported field is encountered during creation, show error:
    /// "Required field X has unsupported type — create in JIRA web UI."
    Unsupported,
}

/// Custom field definition from GET /rest/api/3/field
pub struct JiraFieldDef {
    pub id: String,               // "customfield_10016"
    pub name: String,             // "Story Points"
    pub custom: bool,
}

/// Error returned from the JIRA REST API.
/// Covers both top-level error_messages and per-field errors.
pub struct JiraError {
    pub status_code: u16,
    pub error_messages: Vec<String>,
    pub field_errors: HashMap<String, String>,
}

impl JiraError {
    pub fn display(&self) -> String {
        let mut parts = self.error_messages.clone();
        for (field, msg) in &self.field_errors {
            parts.push(format!("{}: {}", field, msg));
        }
        parts.join("\n")
    }
}

/// Parsed result of the createmeta endpoint.
/// `project`, `issuetype`, and `reporter` are already filtered out —
/// only the remaining required + optional fields are included.
pub struct CreateMetaResponse {
    pub fields: Vec<EditableField>,  // required + optional fields for the project/issue-type
}
```

### Plugin Struct

The top-level struct that implements `ScreenPlugin`. All fields are private to `mod.rs`.

```rust
pub struct JiraPlugin {
    // Config
    config: JiraConfig,
    account_id: Option<String>,   // from /myself, cached after first load

    // Background thread communication
    command_tx: Option<mpsc::Sender<JiraCommand>>,
    result_rx: Option<mpsc::Receiver<JiraResult>>,
    shutdown_flag: Option<Arc<AtomicBool>>,
    thread_handle: Option<std::thread::JoinHandle<()>>,

    // Data (from API, never persisted)
    issues: Vec<JiraIssue>,
    field_defs: Vec<JiraFieldDef>,  // from /field discovery
    story_points_field: Option<String>,
    sprint_field: Option<String>,

    // Board state
    board: BoardState,             // from horizontal-scroll-spec
    project_filter: Option<String>,
    show_done: bool,

    // Modal state (plugin-owned modals)
    modal: Option<JiraModal>,

    // Loading/refresh state
    loading: bool,
    refreshing: bool,
    generation: u64,               // for stale refresh detection
    last_sync: Option<Instant>,

    // Error state
    last_error: Option<String>,
}

enum JiraModal {
    IssueDetail {
        issue_key: String,
        fields: Option<Vec<EditableField>>,
        transitions: Option<Vec<JiraTransition>>,
        comments: Option<Vec<JiraComment>>,
        scroll_offset: usize,
        field_cursor: usize,
    },
    TransitionPicker {
        issue_key: String,
        transitions: Vec<JiraTransition>,
        cursor: usize,
    },
    TransitionFields {
        issue_key: String,
        transition: JiraTransition,
        form: FormState,
    },
    CreateForm {
        project_key: String,
        issue_type_id: String,
        fields: Vec<EditableField>,
        form: FormState,
    },
    ErrorModal {
        title: String,
        message: String,
    },
}
```

### ADF Handling

JIRA Cloud v3 uses Atlassian Document Format (ADF) for rich text fields (description, comments). The plugin converts between ADF and plain text:

**ADF → Plain text (display)**:
- Paragraphs → newline-separated text
- Headings → "# heading text"
- Bullet lists → "- item"
- Ordered lists → "1. item"
- Code blocks → indented text
- Links → "text (url)"
- All other nodes → extract text content, strip formatting

**Plain text → ADF (write)**:
- Wrap user input in a single ADF paragraph node
- No attempt to parse markdown or other formatting
- Simple and predictable: what you type is what gets stored

```rust
// Minimal ADF document with a single paragraph
fn text_to_adf(text: &str) -> serde_json::Value {
    json!({
        "version": 1,
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": text
            }]
        }]
    })
}
```

## Plugin Architecture

### Plugin-Owned Modals

All modals (detail view, transition picker, field editor, creation form, error dialogs) are managed **internally by the plugin**. They are NOT created via the App's modal system. The plugin renders its own modal overlays during `render()` and handles their input during `handle_key()`.

This keeps the JIRA plugin fully self-contained and avoids coupling to the App's modal infrastructure.

### PluginAction Return Type

`handle_key()` returns `PluginAction`, not the full `Action` enum:

```rust
pub enum PluginAction {
    /// Key was not handled by the plugin
    None,
    /// Plugin wants to close / return to dashboard
    Back,
    /// Show a toast message
    Toast(String),
}
```

The plugin never dispatches arbitrary `Action` variants. All internal state transitions (opening modals, navigating fields, etc.) are handled within the plugin's own state machine.

### Background Thread Specifics

- **Single `ureq::Agent`** per thread lifetime — reused for connection pooling
- **Cooperative cancellation**: `AtomicBool` flag or `JiraCommand::Shutdown` to signal the thread to exit
- **Channel draining**: The result-processing loop uses `while let Ok(result) = try_recv()` to drain all pending results per tick, not just one
- **Thread spawn guard**: `on_enter()` checks `JoinHandle::is_finished()` before spawning a new background thread to avoid duplicates
- **Panic detection**: If `try_recv()` returns `TryRecvError::Disconnected`, the background thread has panicked. Show a reconnect prompt to the user.
- **Generation counter**: Each `FetchMyIssues` command carries a monotonically increasing generation ID. Results with stale generation IDs are discarded to prevent out-of-order overwrites.

## UI Design

### Entry Point

**Keybinding**: `J` (uppercase) from the dashboard

### Screen Layout: Kanban Board

The plugin sidebar is **hidden** when the JIRA screen is active — the kanban board takes the full terminal width.

Columns are per-status (one column per distinct JIRA workflow status), NOT grouped by status category. When there are more columns than fit on screen, the board **scrolls horizontally**. `h`/`l` navigates columns AND scrolls the viewport to keep the selected column visible.

```
┌─ JIRA: HMI  ↻ 14:25  ─────────────────────────────────────────┐
│ Open         │ In Progress  │ Code Review  │ QA         │ UAT   │◄ scroll
│              │              │              │            │       │
│ HMI-110      │ HMI-103      │ HMI-102      │            │       │
│  Fix crash   │  Nav focus   │  Unit tests  │            │       │
│  Bug · P1    │  Story · P2  │  Task · P3   │            │       │
│              │              │              │            │       │
│ HMI-115      │ HMI-107      │              │            │       │
│  Tab order   │  CSS states  │              │            │       │
│  Story · P2  │  Sub-task    │              │            │       │
│              │              │              │            │       │
│              │              │              │            │       │
│              │              │              │            │       │
│              │              │              │            │       │
├─────────────────────────────────────────────────────────────────┤
│ hjkl:nav  s:transition  c:comment  Enter:detail  p:proj  R:ref │
│ n:new  D:toggle-done  Esc:back           Last sync: 14:25:03   │
└─────────────────────────────────────────────────────────────────┘
```

**Key features:**
- Header shows current project filter and refresh indicator (stale-data indicator if last refresh failed)
- One column per distinct JIRA workflow status (per-status, not grouped by category)
- Horizontal scroll when columns exceed terminal width; `h`/`l` scrolls the viewport
- Each issue card shows: key, summary (truncated), issue type, priority
- Selected issue is highlighted
- Footer shows key hints and last sync timestamp
- Done column hidden by default, toggle with `D`

### Issue Detail Modal

This modal is managed internally by the plugin (not the App's modal system).

Transitions and editmeta are fetched **lazily** — only when the user opens the detail modal or presses `s`. Results are cached per-issue and invalidated on refresh.

```
┌─ HMI-103: Fix navigation focus ring ───────────────────────────┐
│                                                                 │
│  Status:      In Progress                      [s:transition]   │
│  *Priority:   High                             [e:edit]         │
│  Assignee:    matt.johnson                     (read-only)      │
│  Reporter:    sarah.chen                       (read-only)      │
│  Type:        Story                            (read-only)      │
│  *Points:     3                                [e:edit]         │
│  Sprint:      Sprint 24                        (read-only)      │
│  Epic:        Navigation Rework (NAV-1)        (read-only)      │
│  Labels:      frontend, accessibility          (read-only)      │
│  Components:  hmi-nav                          (read-only)      │
│  Created:     2026-03-15                                        │
│  Updated:     2026-03-25 14:20                                  │
│                                                                 │
│  Description:                                  (read-only)      │
│  The navigation bar focus ring is not visible when using        │
│  keyboard navigation. Need to add CSS outline styles for        │
│  :focus and :focus-visible pseudo-classes on all NavBar items.  │
│                                                                 │
│─────────────────────────────────────────────────────────────────│
│  Comments (3)                                  [c:add]          │
│                                                                 │
│  matt.johnson · 2h ago                                          │
│  Started work on the focus ring. Using CSS outline instead      │
│  of box-shadow for better accessibility.                        │
│                                                                 │
│  sarah.chen · 1d ago                                            │
│  Can we also fix the tab order while we're at it? The skip      │
│  nav link is in the wrong position.                             │
│                                                                 │
│  matt.johnson · 3d ago                                          │
│  Reproducing on Firefox and Chrome. Safari has different        │
│  focus behavior — will test separately.                         │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  j/k:navigate  e:edit  s:transition  c:comment  Esc:close      │
└─────────────────────────────────────────────────────────────────┘
```

**Key features:**
- Fields marked with `*` are editable (discovered from JIRA editmeta API)
- **Description is always read-only** — ADF round-tripping is lossy (rich formatting would be destroyed). Do not support description editing via the TUI.
- Required fields highlighted with accent color
- Editable fields show `[e:edit]` hint, read-only show `(read-only)`
- Status has its own keybinding `s` for transitions (fetched lazily)
- Comments section is scrollable, shows most recent first
- Navigate fields with `j/k`, press `e` to edit the selected field
- Esc closes the modal and returns to the kanban board
- When a refresh completes while the detail modal is open: if the viewed issue no longer exists, close the modal with a toast; if it still exists, update the data in place

### Transition Picker Modal

This modal is managed internally by the plugin (not the App's modal system).

```
┌─ Transition HMI-103 ──────────────┐
│                                    │
│  Current: In Progress              │
│                                    │
│  Available transitions:            │
│  > Start Review   → Code Review   │
│    Block          → Blocked        │
│    Done           → Done           │
│    Back to Todo   → To Do          │
│                                    │
│  Enter: apply  Esc: cancel         │
└────────────────────────────────────┘
```

**Transition flow with required fields:**

After the user selects a transition, check the `fields` object from the transitions API response. If the selected transition has required fields:

1. Present a field input form (same form pattern as issue creation) for the required fields before executing the transition
2. User fills required fields (e.g., Resolution for "Done" transition)
3. POST the transition with the filled fields

If no required fields, execute the transition immediately.

**Optimistic UI:** After sending the transition command, optimistically move the issue to the target column locally. If the API returns an error (`TransitionFailed`), revert the issue to its original column and show a blocking error modal.

### Issue Creation Flow

This flow is managed internally by the plugin (not the App's modal system).

**Step 1: Select Project**
```
┌─ New Issue: Select Project ────────┐
│                                    │
│  > HMI — HMI Framework            │
│    INFRA — Infrastructure          │
│    PLAT — Platform Services        │
│                                    │
│  Enter: select  Esc: cancel        │
└────────────────────────────────────┘
```

**Step 2: Select Issue Type** (fetched from JIRA createmeta for the selected project)
```
┌─ New Issue: HMI — Issue Type ──────┐
│                                    │
│  > Bug                             │
│    Story                           │
│    Task                            │
│    Sub-task                        │
│                                    │
│  Enter: select  Esc: cancel        │
└────────────────────────────────────┘
```

**Step 3: Form Modal** — All required fields visible at once

After project and issue type are selected, a **form modal** is shown with all required fields (discovered from createmeta) visible at once. `j`/`k` navigates between fields, `Enter` edits the focused field. Submit all fields at once.

```
┌─ New Issue: HMI / Bug ────────────────────────────────────────┐
│                                                                │
│  * Summary:      Fix crash when pressing Back                  │
│  * Priority:     > High                                        │
│  * Component:    (none)                                        │
│    Labels:       (none)                                        │
│                                                                │
│  j/k:navigate  Enter:edit field  S:submit  Esc:cancel          │
└────────────────────────────────────────────────────────────────┘
```

Each field is presented based on its type:
- **Text** → inline text input on Enter
- **Select** → selection list popup (with allowed values from JIRA)
- **Number** → text input with validation
- **TextArea** → opens `$EDITOR` (see multi-line input below)
- **Unsupported** → shown as read-only. If the field is required, show error: "Required field X has unsupported type — create in JIRA web UI."

On API error, **preserve all filled fields** and let the user fix the issue without re-entering everything. The issue is automatically assigned to the configured user. A toast confirms creation: "Created HMI-116: Fix crash when pressing Back".

### Comment Input via $EDITOR

Comments and multi-line text fields use `$EDITOR` for input. The app already has editor launch code (`app.rs:167-196`).

**Flow:**
1. User presses `c` to add a comment
2. App writes a temp file (empty, or with template text)
3. App suspends TUI, opens `$EDITOR` on the temp file
4. User writes comment, saves, and exits editor
5. App reads the temp file content
6. If content is non-empty, convert plain text to ADF and POST as comment
7. If content is empty (user quit without writing), cancel

This avoids the need for a multi-line input widget in the TUI and gives the user their full editor for composing comments.

### Error Modal

This modal is managed internally by the plugin (not the App's modal system).

```
┌─ JIRA Error ───────────────────────┐
│                                    │
│  ✖ Failed to transition HMI-103    │
│                                    │
│  Status: 400 Bad Request           │
│  "Transition 'Done' requires       │
│  field 'Resolution' to be set"     │
│                                    │
│  Enter: dismiss                    │
└────────────────────────────────────┘
```

### Loading State

When data is being fetched:

```
┌─ JIRA  ↻ Loading... ──────────────────────────────────────────┐
│                                                                │
│                                                                │
│                                                                │
│                       Loading issues...                        │
│                                                                │
│                                                                │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

When refreshing (data already on screen):

```
┌─ JIRA: HMI  ↻ Refreshing... ─────────────────────────────────┐
│ To Do        │ In Progress  │ ...                              │
│ (existing data continues to display)                           │
```

## Keybindings

### Kanban Board

| Key | Action |
|-----|--------|
| `h` / `Left` | Move to previous column (scrolls viewport if needed) |
| `l` / `Right` | Move to next column (scrolls viewport if needed) |
| `j` / `Down` | Move to next issue in column |
| `k` / `Up` | Move to previous issue in column |
| `g` | Jump to top of column |
| `G` | Jump to bottom of column |
| `Enter` | Open issue detail modal (lazy-fetches transitions + editmeta) |
| `s` | Show transition picker for selected issue (lazy-fetches transitions) |
| `c` | Comment on selected issue (opens `$EDITOR`) |
| `n` | Create new issue (form modal) |
| `p` | Cycle project filter |
| `D` | Toggle Done column visibility |
| `R` | Manual refresh |
| `Esc` / `q` | Back to dashboard |

### Issue Detail Modal

| Key | Action |
|-----|--------|
| `j` / `Down` | Navigate to next field |
| `k` / `Up` | Navigate to previous field |
| `e` | Edit selected field (if editable; description is always read-only) |
| `s` | Transition issue status (lazy-fetches transitions if not cached) |
| `c` | Add comment (opens `$EDITOR`) |
| `Esc` | Close modal, return to kanban |

### Transition Picker Modal

| Key | Action |
|-----|--------|
| `j` / `Down` | Next transition |
| `k` / `Up` | Previous transition |
| `Enter` | Apply selected transition |
| `Esc` | Cancel |

## Error Handling

Errors are handled differently depending on whether the action was user-initiated or automatic:

- **User-initiated actions** (transition, edit, create, comment): **Blocking error modal** that must be dismissed before continuing. This ensures errors from explicit user actions are never missed.
- **Auto-refresh failures**: **Non-blocking toast** + stale-data indicator in the header (e.g., "JIRA [stale]"). The existing data remains visible and usable.

### Error Categories

| Category | Example | Behavior |
|----------|---------|----------|
| Auth failure | 401 Unauthorized | Blocking modal: check JIRA_API_TOKEN and jira.email |
| Permission denied | 403 Forbidden | Blocking modal: check JIRA permissions for the project |
| Not found | 404 on issue | Blocking modal: issue may have been deleted in JIRA |
| Validation error | 400 on create/edit | Blocking modal: show field-level error from JIRA response |
| Rate limited | 429 Too Many Requests | Auto-retry after `Retry-After` header delay; toast notification |
| Network error (user action) | Connection refused / timeout | Blocking modal: check JIRA URL and network |
| Network error (auto-refresh) | Connection refused / timeout | Non-blocking toast + stale-data indicator |
| Config error | Missing env var or config | Blocking modal: show which config is missing |

### Error Modal Content (Blocking)

Each blocking error modal shows:
1. **Action that failed** — "Failed to transition HMI-103"
2. **HTTP status** — "Status: 400 Bad Request"
3. **Error message** — parsed from JIRA's response body
4. **Dismiss instruction** — "Enter: dismiss"

### Stale-Data Indicator (Non-Blocking)

When an auto-refresh fails, the header changes to show a stale indicator:
```
┌─ JIRA: HMI [stale]  ↻ 14:25  ────────────────────────────────┐
```
A toast is shown briefly: "JIRA refresh failed — showing cached data". The indicator clears on the next successful refresh.

## Refresh Behavior

- **Initial load**: Fetch all assigned issues when JIRA screen is opened (`on_enter`). The background thread pages through ALL results from `/search` (incrementing `startAt`) and sends the full `Vec<JiraIssue>` once complete.
- **Auto-refresh**: Every 60 seconds (configurable via `refresh_interval_secs`)
- **Manual refresh**: Press `R` to force immediate refresh
- **Post-write refresh**: After any write operation (transition, edit, create, comment), trigger a refresh with a **500ms delay** (JIRA has eventual consistency — immediate reads may return stale data)
- **Refresh deduplication**: A `refreshing: bool` flag prevents overlapping refreshes. Skip auto-refresh if a refresh is already in-flight.
- **Generation counter**: Each `FetchMyIssues` command carries a `generation: u64`. Results are only applied if the generation matches the current expected generation, preventing stale overwrites from slow responses.
- **Visual indicator**: Spinner in header during refresh ("↻ Refreshing...")
- **Last sync**: Timestamp shown in footer ("Last sync: 14:25:03")
- **Background**: Refresh happens in background thread; stale data remains visible until new data arrives
- **Stale detail modal**: When a refresh completes and the detail modal is open, check if the viewed issue still exists in the new data. If not, close the modal with a toast ("Issue no longer assigned to you"). If it still exists, update the displayed data in place.

## Permissions Summary

### What the Plugin CAN Do

- View issues assigned to the user
- Transition issues through available workflow statuses
- Edit simple fields that JIRA reports as editable for the user (description excluded — read-only due to ADF lossy round-trip)
- Add comments to issues
- Create new issues (always assigned to the configured user)

### What the Plugin CANNOT Do

- **Delete issues** — no delete API calls, ever
- **Reassign issues** — assignee is read-only on existing issues
- **Modify issues in projects the user is not a member of** — project list derived from assigned issues
- **Create issues in projects the user is not a member of** — only projects with existing assignments shown
- **Modify local jm data** — plugin is self-contained

## File Structure

```
crates/jm-tui/src/plugins/jira/
├── mod.rs          # JiraPlugin struct implementing ScreenPlugin
├── api.rs          # JIRA Cloud REST v3 client + background thread
├── models.rs       # JiraIssue, JiraStatus, JiraTransition, etc.
├── board.rs        # Kanban board rendering (full-screen)
├── detail.rs       # Issue detail modal rendering + field navigation
├── create.rs       # Issue creation flow (project/type select + form modal)
├── adf.rs          # ADF ↔ plain text conversion
└── config.rs       # JiraConfig struct + validation
```

### Dependencies (new Cargo.toml additions)

```toml
[dependencies]
ureq = { version = "3", features = ["json"] }
serde_json = "1.0"
base64 = "0.22"
```

`ureq` is a synchronous HTTP client with ~15 transitive crate dependencies (vs ~80 for `reqwest`). No tokio runtime needed. All HTTP calls are made from the dedicated background thread using a single `ureq::Agent` instance for connection pooling.

## Config Schema

### Full Config Example

```yaml
plugins:
  enabled: [pomodoro, notifications, clock, jira]

  jira:
    # Required
    url: "https://myorg.atlassian.net"
    email: "matt@company.com"

    # Optional (with defaults)
    refresh_interval_secs: 60   # Auto-refresh interval (default: 60)

    # Optional: custom field IDs for story points and sprint.
    # If not set, the plugin calls GET /rest/api/3/field and searches by name
    # (case-insensitive 'story point', 'sprint'). If ambiguous, omit from display.
    story_points_field: "customfield_10016"
    sprint_field: "customfield_10020"
```

### Rust Config Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraConfig {
    pub url: String,
    pub email: String,
    #[serde(default = "default_refresh")]
    pub refresh_interval_secs: u64,
    /// Custom field ID for story points (e.g., "customfield_10016").
    /// If not set, auto-discovered via GET /rest/api/3/field.
    #[serde(default)]
    pub story_points_field: Option<String>,
    /// Custom field ID for sprint (e.g., "customfield_10020").
    /// If not set, auto-discovered via GET /rest/api/3/field.
    #[serde(default)]
    pub sprint_field: Option<String>,
}

fn default_refresh() -> u64 { 60 }
```

### Custom Field Discovery

If `story_points_field` or `sprint_field` are not set in the config, the plugin calls `GET /rest/api/3/field` on startup and searches for fields whose name contains "story point" or "sprint" (case-insensitive). If exactly one match is found, use that field ID. If multiple matches are found (ambiguous), omit the field from display rather than guessing wrong.

## Phases / Milestones

### Phase 1a: Foundation

- JiraPlugin struct implementing ScreenPlugin with `PluginAction` return type
- Config parsing and validation (including optional `story_points_field`, `sprint_field`)
- `ureq`-based API client with background thread (single `ureq::Agent`, cooperative cancellation)
- Authentication: `/rest/api/3/myself` call on startup to validate credentials and retrieve `accountId`
- Custom field discovery via `GET /rest/api/3/field` (if not configured)
- Fetch assigned issues endpoint with full pagination (all pages)
- Per-status kanban board rendering with horizontal scroll
- Project filter cycling
- Loading state, refresh with generation counter, stale-data indicator
- Sidebar hidden when JIRA screen is active

### Phase 1b: Issue Interaction

- Issue detail modal (plugin-owned) with field display; description as read-only
- Dynamic status discovery (per-status columns)
- Transition picker modal (plugin-owned) with required field detection
- Optimistic UI for transitions (revert on error)
- Status transitions from both board and detail (lazy-fetched, cached)
- Blocking error modals for user-initiated actions; non-blocking toasts for auto-refresh failures
- ADF → plain text conversion for descriptions

### Phase 1c: Editing & Comments

- Editable field discovery (editmeta API, lazy-fetched on detail open)
- Field editing for simple types (text, select, number); `FieldType::Unsupported` for unknown types
- Description excluded from editable fields (ADF lossy round-trip)
- Required field highlighting (color)
- Comment viewing in detail modal
- Comment creation via `$EDITOR` (suspend TUI, open editor, read back, convert to ADF)
- Plain text → ADF conversion for writes
- Write-then-refresh with 500ms delay (JIRA eventual consistency)

### Phase 1d: Issue Creation

- Createmeta API integration (required field discovery)
- Form modal creation flow (all required fields visible at once, j/k navigation)
- Dynamic field inputs per type; unsupported required fields show "create in JIRA web UI" error
- On API error, preserve filled fields and let user fix
- Auto-assign to configured user
- Post-create refresh (with delay) and toast notification

### Phase 1e: Polish

- Keyboard hint bar for all states
- Column width balancing with horizontal scroll for many statuses
- Issue card truncation and formatting
- Relative time display for comments ("2h ago", "1d ago")
- Rate limit handling with retry
- Stale detail modal handling (close if issue removed, update in place otherwise)
- Thread panic detection (`TryRecvError::Disconnected`) with reconnect prompt
- Edge cases: empty projects, no assigned issues, network offline
