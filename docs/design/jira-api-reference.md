# JIRA Cloud REST API v3 — Implementation Reference

This document provides exact JSON request/response shapes for every JIRA API endpoint used by the jm JIRA plugin. It is the definitive reference for implementing `jira/api.rs` and `jira/models.rs`.

**Target**: JIRA Cloud REST API v3 (`https://{instance}.atlassian.net/rest/api/3/...`)

---

## Authentication

All requests use Basic Auth with email + API token:

```
Authorization: Basic base64("{email}:{api_token}")
Content-Type: application/json
Accept: application/json
```

**Rust (ureq v3 + base64 crate):**

> **Dependencies** — add these to `Cargo.toml` for the plugin crate:
> ```toml
> ureq = "3"
> base64 = "0.22"
> serde = { version = "1", features = ["derive"] }
> serde_json = "1.0"
> ```
> `serde_json` alone does **not** enable the `derive` feature for `serde`. You must list `serde` explicitly with `features = ["derive"]` or every `#[derive(Deserialize)]` annotation will fail to compile.
>
> **ureq v3 constructor**: use `ureq::Agent::new()` or `ureq::AgentBuilder::new().build()` (NOT `ureq::agent()` — that is the v2 API).

```rust
use base64::Engine;

let auth = base64::engine::general_purpose::STANDARD
    .encode(format!("{}:{}", email, api_token));

// ureq v3: .call()? propagates network/IO errors only.
// 4xx/5xx responses are returned as Ok(Response) with a non-2xx status —
// they are NOT Err. Always check response.status() explicitly.
let response = agent
    .get(&url)
    .header("Authorization", &format!("Basic {}", auth))
    .header("Content-Type", "application/json")
    .header("Accept", "application/json")
    .call()?;  // ? only catches network errors

match response.status() {
    200..=299 => {
        // Read the success body as JSON
        let body: MyResponseType = response.into_body().read_json()?;
        Ok(body)
    }
    429 => {
        // Rate limited — honour Retry-After header (seconds to wait)
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        Err(JiraError::RateLimited { retry_after_secs: retry_after })
    }
    status => {
        // Read error body as JiraErrorResponse
        let err: JiraErrorResponse = response.into_body().read_json()
            .unwrap_or_default();
        Err(JiraError::Api { status, detail: err })
    }
}
```

The API token is generated at https://id.atlassian.com/manage-profile/security/api-tokens. It is NOT an OAuth token or PAT.

---

## Pagination Pattern

Several endpoints are paginated. The pattern is always:

```
?startAt=0&maxResults=100
```

Response includes:
```json
{ "startAt": 0, "maxResults": 100, "total": 247, ... }
```

**Safe termination**: loop while `!page.is_empty() && page.len() >= max_results`, incrementing `startAt` by `max_results` each iteration. Stop when the page is empty or shorter than `max_results` (indicates the last page).

**Do NOT use `total` for loop termination.** The `total` field is an estimate on large workloads — using it as a hard bound can silently drop the last page. It is still useful for progress indicators (e.g., "fetched N of ~total issues").

---

## Common Field Value Formats

**CRITICAL**: JIRA mixes bare values and object values inconsistently. This table is the source of truth for reads AND writes:

| Field | Read Format (GET response) | Write Format (POST/PUT body) |
|-------|---------------------------|------------------------------|
| `summary` | `"text"` (string) | `"text"` (string) |
| `description` | ADF JSON object or `null` | ADF JSON object (read-only in our plugin) |
| `status` | `{ "id": "...", "name": "...", "statusCategory": {...} }` | N/A (changed via transitions) |
| `priority` | `{ "id": "2", "name": "High" }` or `null` | `{ "id": "2" }` |
| `issuetype` | `{ "id": "...", "name": "Story", "subtask": false }` | `{ "id": "10001" }` |
| `assignee` | `{ "accountId": "...", "displayName": "..." }` or `null` | `{ "accountId": "..." }` |
| `reporter` | `{ "accountId": "...", "displayName": "..." }` or `null` | `{ "accountId": "..." }` (silently inject in create POST; required for business/service-desk project types) |
| `labels` | `["frontend", "a11y"]` (string array) | `["frontend", "a11y"]` (full replacement) |
| `components` | `[{ "id": "...", "name": "hmi-nav" }]` | `[{ "id": "10000" }]` |
| `project` | `{ "id": "...", "key": "HMI", "name": "..." }` | `{ "key": "HMI" }` |
| `story_points` | `3.0` (bare number) or `null` | `5.0` (bare number) |
| `sprint` | `[{ "id": 37, "name": "Sprint 24", "state": "active", ... }]` or `null` | N/A (not editable from this plugin) |
| `parent` (epic) | `{ "key": "NAV-1", "fields": { "summary": "...", "issuetype": {...} } }` or absent | N/A (not editable) |
| `resolution` | `{ "id": "1", "name": "Done" }` or `null` | `{ "id": "1" }` |

**Rule of thumb**: Simple values (string, number, string-array) are bare. Entity references (priority, assignee, component, resolution, project, issuetype) are always objects with `id`.

---

## Endpoints

### 1. `GET /rest/api/3/myself`

**Purpose**: Validate credentials, retrieve `accountId` for JQL queries.

**Request**: No params.
```
GET /rest/api/3/myself
```

**Response** (200 OK):
```json
{
  "self": "https://myorg.atlassian.net/rest/api/3/user?accountId=5b10a2844c20165700ede21g",
  "accountId": "5b10a2844c20165700ede21g",
  "accountType": "atlassian",
  "emailAddress": "matt@company.com",
  "displayName": "Matt Johnson",
  "active": true,
  "timeZone": "America/Chicago",
  "locale": "en_US"
}
```

**Extract**: `accountId` (String). Use in JQL as `assignee = '<accountId>'`.

**Rust struct** (only deserialize what you need):
```rust
#[derive(Deserialize)]
struct MyselfResponse {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
}
```

---

### 2. `GET /rest/api/3/search`

**Purpose**: Fetch all issues assigned to the user. THE primary data source.

**Request**:
```
GET /rest/api/3/search?jql=assignee%3D'{accountId}'&startAt=0&maxResults=100&fields=summary,status,priority,issuetype,assignee,reporter,created,updated,description,labels,components,project,parent,{story_points_field},{sprint_field}
```

Query params:
- `jql`: `assignee = '{accountId}'` — uses accountId from `/myself`, NOT `currentUser()`
- `startAt`: pagination offset (0-based)
- `maxResults`: page size (max 100 for Cloud)
- `fields`: comma-separated list of field IDs to return. ALWAYS specify this — without it, the response includes ALL fields (hundreds of custom fields).

**Response** (200 OK):
```json
{
  "startAt": 0,
  "maxResults": 100,
  "total": 47,
  "issues": [
    {
      "id": "10023",
      "key": "HMI-103",
      "self": "https://myorg.atlassian.net/rest/api/3/issue/10023",
      "fields": {
        "summary": "Fix navigation focus ring",
        "status": {
          "id": "10001",
          "name": "In Progress",
          "statusCategory": {
            "id": 4,
            "key": "indeterminate",
            "colorName": "blue",
            "name": "In Progress"
          }
        },
        "priority": {
          "id": "2",
          "name": "High",
          "iconUrl": "https://..."
        },
        "issuetype": {
          "id": "10001",
          "name": "Story",
          "subtask": false,
          "hierarchyLevel": 0
        },
        "assignee": {
          "accountId": "5b10a2844c20165700ede21g",
          "displayName": "Matt Johnson",
          "active": true
        },
        "reporter": {
          "accountId": "5b10ac8d82e05b22cc7d4ef5",
          "displayName": "Sarah Chen",
          "active": true
        },
        "created": "2026-03-15T09:30:00.000+0000",
        "updated": "2026-03-25T14:20:00.000+0000",
        "description": {
          "version": 1,
          "type": "doc",
          "content": [
            {
              "type": "paragraph",
              "content": [
                { "type": "text", "text": "The navigation bar focus ring is not visible..." }
              ]
            }
          ]
        },
        "labels": ["frontend", "accessibility"],
        "components": [
          { "id": "10000", "name": "hmi-nav" }
        ],
        "project": {
          "id": "10000",
          "key": "HMI",
          "name": "HMI Framework",
          "projectTypeKey": "software"
        },
        "parent": {
          "id": "10019",
          "key": "NAV-1",
          "fields": {
            "summary": "Navigation Rework",
            "status": { "name": "In Progress", "statusCategory": { "key": "indeterminate" } },
            "priority": { "name": "High" },
            "issuetype": { "id": "10000", "name": "Epic", "subtask": false, "hierarchyLevel": 1 }
          }
        },
        "customfield_10016": 3.0,
        "customfield_10020": [
          {
            "id": 37,
            "name": "Sprint 24",
            "state": "active",
            "boardId": 5,
            "startDate": "2026-03-18T00:00:00.000Z",
            "endDate": "2026-04-01T00:00:00.000Z"
          }
        ]
      }
    }
  ]
}
```

**Field extraction rules**:

| JiraIssue field | JSON path | Extraction logic |
|-----------------|-----------|------------------|
| `key` | `.key` | Direct string |
| `summary` | `.fields.summary` | Direct string |
| `status.name` | `.fields.status.name` | String from nested object |
| `status.category` | `.fields.status.statusCategory.key` | Map: `"new"`/`"undefined"` → ToDo, `"indeterminate"` → InProgress, `"done"` → Done |
| `priority` | `.fields.priority.name` | String from nested object, or `None` if `null` |
| `issue_type` | `.fields.issuetype.name` | String from nested object |
| `assignee` | `.fields.assignee.displayName` | String from nested object, or `None` if `null` |
| `reporter` | `.fields.reporter.displayName` | String from nested object, or `None` if `null` |
| `created` | `.fields.created` | ISO 8601 string |
| `updated` | `.fields.updated` | ISO 8601 string |
| `description` | `.fields.description` | ADF JSON → plain text via `adf_to_text()`, or `None` if `null` |
| `labels` | `.fields.labels` | Direct `Vec<String>` |
| `components` | `.fields.components[*].name` | Extract `.name` from each object in the array |
| `project_key` | `.fields.project.key` | String from nested object |
| `project_name` | `.fields.project.name` | String from nested object |
| `story_points` | `.fields.{story_points_field}` | Bare `f64` or `null` (field ID is dynamic) |
| `sprint` | `.fields.{sprint_field}` | Array of sprint objects. Find one with `"state": "active"`. Extract `.name`. If none active, try `"future"`, then most recent `"closed"`. `null` if field absent. |
| `epic` | `.fields.parent` | Check if `.fields.parent.fields.issuetype.hierarchyLevel == 1` OR `.fields.parent.fields.issuetype.name == "Epic"`. If so, extract `EpicInfo { key: parent.key, name: parent.fields.summary }`. Otherwise `None`. |

**Pagination**: Loop while `!issues.is_empty() && issues.len() >= max_results`. See Pagination Pattern section above for rationale.

**Custom field IDs**: `story_points_field` and `sprint_field` are dynamic. Either from config or from field discovery (endpoint 3). Include them in the `fields` query param.

---

### 3. `GET /rest/api/3/field`

**Purpose**: Discover custom field IDs for story points and sprint.

**Request**: No params.
```
GET /rest/api/3/field
```

**Response** (200 OK) — flat array, NOT paginated:
```json
[
  {
    "id": "summary",
    "key": "summary",
    "name": "Summary",
    "custom": false,
    "schema": { "type": "string", "system": "summary" }
  },
  {
    "id": "customfield_10016",
    "key": "customfield_10016",
    "name": "Story Points",
    "custom": true,
    "schema": {
      "type": "number",
      "custom": "com.atlassian.jira.plugin.system.customfieldtypes:float",
      "customId": 10016
    }
  },
  {
    "id": "customfield_10020",
    "key": "customfield_10020",
    "name": "Sprint",
    "custom": true,
    "schema": {
      "type": "array",
      "items": "json",
      "custom": "com.pyxis.greenhopper.jira:gh-sprint",
      "customId": 10020
    }
  }
]
```

**Discovery logic**:
- **Story Points**: Find where `custom == true` AND (`name` case-insensitively contains "story point" AND `schema.custom` contains `"float"`). The `schema.custom` match disambiguates from similarly-named fields.
- **Sprint**: Find where `custom == true` AND `schema.custom` contains `"gh-sprint"`. More reliable than name matching.

**Rust struct**:
```rust
#[derive(Deserialize)]
struct JiraFieldDef {
    id: String,
    name: String,
    custom: bool,
    schema: Option<FieldSchema>,
}

#[derive(Deserialize)]
struct FieldSchema {
    #[serde(rename = "type")]
    field_type: String,
    custom: Option<String>,  // e.g., "com.pyxis.greenhopper.jira:gh-sprint"
}
```

---

### 4. `GET /rest/api/3/issue/{key}/transitions`

**Purpose**: Get available workflow transitions for an issue. Called lazily when user opens detail or presses `s`.

**Request**:
```
GET /rest/api/3/issue/HMI-103/transitions?expand=transitions.fields
```

The `expand=transitions.fields` query param is required to get required field metadata in the response.

**Response** (200 OK):
```json
{
  "transitions": [
    {
      "id": "21",
      "name": "Start Review",
      "to": {
        "id": "10002",
        "name": "Code Review",
        "statusCategory": {
          "id": 4,
          "key": "indeterminate",
          "colorName": "blue",
          "name": "In Progress"
        }
      },
      "hasScreen": false,
      "isGlobal": false,
      "isInitial": false,
      "isConditional": false,
      "fields": {}
    },
    {
      "id": "31",
      "name": "Done",
      "to": {
        "id": "10003",
        "name": "Done",
        "statusCategory": {
          "id": 3,
          "key": "done",
          "colorName": "green",
          "name": "Done"
        }
      },
      "hasScreen": true,
      "isGlobal": false,
      "isInitial": false,
      "isConditional": false,
      "fields": {
        "resolution": {
          "required": true,
          "schema": { "type": "resolution", "system": "resolution" },
          "name": "Resolution",
          "hasDefaultValue": false,
          "operations": ["set"],
          "allowedValues": [
            { "id": "1", "name": "Done", "description": "Work has been completed." },
            { "id": "2", "name": "Won't Do", "description": "..." },
            { "id": "3", "name": "Duplicate", "description": "..." },
            { "id": "4", "name": "Cannot Reproduce", "description": "..." }
          ]
        }
      }
    }
  ]
}
```

**Key mapping details**:
- Target status is in `.to` (NOT `.to_status` as the data model struct might suggest)
- `fields` is a **map** keyed by field ID, NOT an array
- Empty `fields: {}` means no required fields — execute transition immediately
- Non-empty `fields` with `required: true` entries means the user must fill those before the transition can execute
- `allowedValues` items have BOTH `id` and `name` — display `name`, send `id`

**Mapping to `JiraTransition`**:
```rust
struct JiraTransition {
    id: String,                           // from .id
    name: String,                         // from .name
    to_status: JiraStatus,                // from .to.name + .to.statusCategory.key
    required_fields: Vec<TransitionField>, // from .fields (filter to required: true)
}

struct TransitionField {
    field_id: String,                      // map key (e.g., "resolution")
    name: String,                          // from .name
    allowed_values: Vec<AllowedValue>,     // from .allowedValues
}

struct AllowedValue {
    id: String,    // needed for POST body
    name: String,  // needed for display
}
```

---

### 5. `POST /rest/api/3/issue/{key}/transitions`

**Purpose**: Execute a status transition, optionally with required fields.

**Request** — simple transition (no required fields):
```json
POST /rest/api/3/issue/HMI-103/transitions

{
  "transition": {
    "id": "21"
  }
}
```

**Request** — transition with resolution field:
```json
{
  "transition": {
    "id": "31"
  },
  "fields": {
    "resolution": {
      "id": "1"
    }
  }
}
```

**Request** — transition with required comment:
```json
{
  "transition": {
    "id": "41"
  },
  "update": {
    "comment": [
      {
        "add": {
          "body": {
            "version": 1,
            "type": "doc",
            "content": [
              { "type": "paragraph", "content": [{ "type": "text", "text": "Closing as completed." }] }
            ]
          }
        }
      }
    ]
  }
}
```

**IMPORTANT**: Regular fields go in `"fields"`, but comments go in `"update"` with an `"add"` operation. This asymmetry is a common bug source.

**Response**: `204 No Content` (empty body) on success.

---

### 6. `PUT /rest/api/3/issue/{key}`

**Purpose**: Update one or more fields on an issue.

**Request**:
```json
PUT /rest/api/3/issue/HMI-103?notifyUsers=false

{
  "fields": {
    "summary": "Updated summary text",
    "priority": { "id": "2" },
    "customfield_10016": 5.0,
    "labels": ["frontend", "accessibility", "new-label"]
  }
}
```

- Use `?notifyUsers=false` to suppress email notifications for minor edits
- All updates wrapped in top-level `"fields"` object
- Can update one field at a time or multiple in one call
- Array fields (labels) must send the COMPLETE array, not a diff
- Select fields use `{ "id": "..." }`
- Number fields are bare numbers
- String fields are bare strings

**Response**: `204 No Content` (empty body) on success.

---

### 7. `GET /rest/api/3/issue/{key}/comment`

**Purpose**: Fetch comments for an issue.

**Request**:
```
GET /rest/api/3/issue/HMI-103/comment?startAt=0&maxResults=50&orderBy=-created
```

- `orderBy=-created` for newest first (minus prefix = descending)

**Response** (200 OK):
```json
{
  "startAt": 0,
  "maxResults": 50,
  "total": 3,
  "comments": [
    {
      "id": "10023",
      "author": {
        "accountId": "5b10a2844c20165700ede21g",
        "displayName": "Matt Johnson",
        "active": true
      },
      "body": {
        "version": 1,
        "type": "doc",
        "content": [
          { "type": "paragraph", "content": [{ "type": "text", "text": "Started work on the focus ring." }] }
        ]
      },
      "created": "2026-03-25T12:20:00.000+0000",
      "updated": "2026-03-25T12:20:00.000+0000"
    }
  ]
}
```

**Key details**:
- `author` is an object — extract `.displayName`
- `body` is ADF — convert to plain text via `adf_to_text()`
- Paginated — use standard pagination pattern if >50 comments

---

### 8. `POST /rest/api/3/issue/{key}/comment`

**Purpose**: Add a comment to an issue.

**Request**:
```json
POST /rest/api/3/issue/HMI-103/comment

{
  "body": {
    "version": 1,
    "type": "doc",
    "content": [
      {
        "type": "paragraph",
        "content": [
          { "type": "text", "text": "This is my comment." }
        ]
      }
    ]
  }
}
```

For multi-paragraph comments (from `$EDITOR`), split on blank lines:
```rust
fn text_to_adf(text: &str) -> serde_json::Value {
    let paragraphs: Vec<Value> = text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| json!({
            "type": "paragraph",
            "content": [{ "type": "text", "text": p.trim() }]
        }))
        .collect();

    json!({ "version": 1, "type": "doc", "content": paragraphs })
}
```

**Response**: `201 Created` with the full comment object (same shape as GET response items).

---

### 9. `GET /rest/api/3/issue/{key}/editmeta`

**Purpose**: Discover which fields the user can edit on a specific issue. Called lazily when detail modal opens.

**Request**:
```
GET /rest/api/3/issue/HMI-103/editmeta
```

**Response** (200 OK):
```json
{
  "fields": {
    "summary": {
      "required": true,
      "schema": { "type": "string", "system": "summary" },
      "name": "Summary",
      "operations": ["set"]
    },
    "priority": {
      "required": false,
      "schema": { "type": "priority", "system": "priority" },
      "name": "Priority",
      "operations": ["set"],
      "allowedValues": [
        { "id": "1", "name": "Highest", "iconUrl": "..." },
        { "id": "2", "name": "High", "iconUrl": "..." },
        { "id": "3", "name": "Medium", "iconUrl": "..." },
        { "id": "4", "name": "Low", "iconUrl": "..." },
        { "id": "5", "name": "Lowest", "iconUrl": "..." }
      ]
    },
    "labels": {
      "required": false,
      "schema": { "type": "array", "items": "string", "system": "labels" },
      "name": "Labels",
      "autoCompleteUrl": "https://...",
      "operations": ["add", "set", "remove"]
    },
    "customfield_10016": {
      "required": false,
      "schema": {
        "type": "number",
        "custom": "com.atlassian.jira.plugin.system.customfieldtypes:float",
        "customId": 10016
      },
      "name": "Story Points",
      "operations": ["set"]
    },
    "description": {
      "required": false,
      "schema": { "type": "any", "system": "description" },
      "name": "Description",
      "operations": ["set"]
    }
  }
}
```

**Key details**:
- Response is a **map** keyed by field ID — iterate entries
- **Exclude** `description` (read-only in our plugin, ADF lossy)
- Only include fields with `"set"` in `operations`
- `schema.type` → `FieldType` mapping:
  - `"string"` → `Text`
  - `"number"` → `Number`
  - `"priority"`, `"resolution"`, `"option"` (with `allowedValues`) → `Select`
  - `"array"` with `items: "string"` (labels) → `Text` (comma-separated input)
  - `"user"`, `"version"`, `"array"` with object items → `Unsupported`
  - Anything else → `Unsupported`
- Fields with `autoCompleteUrl` but no `allowedValues` → treat as free text

**Mapping to `EditableField`**:
```rust
struct EditableField {
    field_id: String,                        // map key
    name: String,                            // from .name
    field_type: FieldType,                   // from schema.type mapping above
    required: bool,                          // from .required
    allowed_values: Option<Vec<AllowedValue>>, // from .allowedValues (id + name pairs)
}

struct AllowedValue {
    id: String,
    name: String,
}
```

**NOTE**: The `allowed_values` MUST store both `id` and `name`. Display `name` to user, send `{ "id": "..." }` in PUT body.

---

### 10. `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes`

**Purpose**: Get available issue types for a project during issue creation.

**Request**:
```
GET /rest/api/3/issue/createmeta/HMI/issuetypes
```

**Response** (200 OK):
```json
{
  "values": [
    { "id": "10001", "name": "Story", "subtask": false, "hierarchyLevel": 0 },
    { "id": "10002", "name": "Bug", "subtask": false },
    { "id": "10003", "name": "Task", "subtask": false },
    { "id": "10004", "name": "Sub-task", "subtask": true }
  ]
}
```

**NOTE**: Response wrapper is `"values"`, NOT `"issueTypes"`. This is a v3 API change.

Extract `id` and `name` for each. The `id` is needed for the next call and the create POST body.

---

### 11. `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes/{issueTypeId}`

**Purpose**: Get required/optional fields for creating an issue of a specific type.

**Request**:
```
GET /rest/api/3/issue/createmeta/HMI/issuetypes/10002?startAt=0&maxResults=50
```

**Response** (200 OK) — **paginated**:
```json
{
  "startAt": 0,
  "maxResults": 50,
  "total": 8,
  "values": [
    {
      "required": true,
      "schema": { "type": "string", "system": "summary" },
      "name": "Summary",
      "fieldId": "summary",
      "hasDefaultValue": false,
      "operations": ["set"]
    },
    {
      "required": true,
      "schema": { "type": "issuetype", "system": "issuetype" },
      "name": "Issue Type",
      "fieldId": "issuetype",
      "hasDefaultValue": false,
      "operations": ["set"],
      "allowedValues": [
        { "id": "10002", "name": "Bug", "subtask": false }
      ]
    },
    {
      "required": true,
      "schema": { "type": "project", "system": "project" },
      "name": "Project",
      "fieldId": "project",
      "hasDefaultValue": false,
      "operations": ["set"]
    },
    {
      "required": false,
      "schema": { "type": "priority", "system": "priority" },
      "name": "Priority",
      "fieldId": "priority",
      "hasDefaultValue": true,
      "operations": ["set"],
      "allowedValues": [
        { "id": "1", "name": "Highest" },
        { "id": "2", "name": "High" },
        { "id": "3", "name": "Medium" },
        { "id": "4", "name": "Low" },
        { "id": "5", "name": "Lowest" }
      ]
    },
    {
      "required": false,
      "schema": { "type": "array", "items": "string", "system": "labels" },
      "name": "Labels",
      "fieldId": "labels",
      "hasDefaultValue": false,
      "operations": ["add", "set", "remove"]
    },
    {
      "required": false,
      "schema": { "type": "any", "system": "description" },
      "name": "Description",
      "fieldId": "description",
      "hasDefaultValue": false,
      "operations": ["set"]
    }
  ]
}
```

**Key details**:
- Each field has `fieldId` — this is the key for the create POST body
- **Filter out** auto-set fields from user form: `project`, `issuetype`, `reporter`
- `hasDefaultValue: true` means JIRA fills a default even if `required: true` (e.g., Priority often defaults to Medium)
- `allowedValues` items have `id` and `name` — same `AllowedValue` struct
- Fields with `autoCompleteUrl` instead of `allowedValues` → free text input
- **Paginated** — must handle `total > maxResults`
- `schema.type` → `FieldType` mapping is same as editmeta

---

### 12. `POST /rest/api/3/issue`

**Purpose**: Create a new issue.

**Request**:
```json
POST /rest/api/3/issue

{
  "fields": {
    "project": { "key": "HMI" },
    "issuetype": { "id": "10002" },
    "summary": "Fix crash when pressing Back",
    "priority": { "id": "2" },
    "assignee": { "accountId": "5b10a2844c20165700ede21g" },
    "reporter": { "accountId": "5b10a2844c20165700ede21g" },
    "labels": ["frontend"],
    "components": [{ "id": "10000" }]
  }
}
```

- `project` uses `{ "key": "..." }`
- `issuetype` uses `{ "id": "..." }` (NOT `{ "name": "Bug" }`)
- `assignee` uses `{ "accountId": "..." }` from `/myself` response
- **`reporter` must be silently injected** with the user's `accountId` from `/myself`. Some project types (business, service desk) require this field even though it is not shown to the user in the create form. Omitting it on those project types returns a 400 error. Always include it.
- Select fields use `{ "id": "..." }`
- Only include fields that the user actually filled in
- Description is excluded (read-only in our plugin)

**Response** (201 Created):
```json
{
  "id": "10024",
  "key": "HMI-116",
  "self": "https://myorg.atlassian.net/rest/api/3/issue/10024"
}
```

Extract `.key` for the toast message: "Created HMI-116".

---

### 13. `GET /rest/api/3/status` (optional)

**Purpose**: Get all statuses for workflow discovery. **May not be needed** — the search response already includes status info per issue.

**Request**: No params.

**Response** (200 OK) — flat array:
```json
[
  {
    "id": "1",
    "name": "Open",
    "statusCategory": {
      "id": 2,
      "key": "new",
      "colorName": "blue-gray",
      "name": "To Do"
    },
    "scope": {
      "type": "PROJECT",
      "project": { "id": "10000" }
    }
  }
]
```

**Recommendation**: Derive the status list and kanban columns from the issues themselves (unique `status.name` values, grouped by `status.statusCategory.key`). Only call this endpoint if you need statuses with zero issues in them (empty columns).

---

## ADF (Atlassian Document Format) Reference

ADF is a recursive JSON tree. Every node has a `type` field. Block nodes have a `content` array of child nodes. Text nodes have a `text` field.

### Conversion Algorithm: `adf_to_text(node) -> String`

```
fn adf_to_text(node: &Value) -> String {
    match node["type"].as_str() {
        "doc"         => join children with "\n\n"
        "paragraph"   => join children with ""  (inline concatenation)
        "heading"     => "#".repeat(level) + " " + join children
        "bulletList"  => join children with "\n"  (each child is a listItem)
        "orderedList" => join children with "\n"  (number each)
        "listItem"    => "- " + join children     (or "N. " for ordered)
        "codeBlock"   => "    " + indent each line of text content
        "blockquote"  => "> " + prefix each line
        "text"        => node["text"]  (leaf node)
        "hardBreak"   => "\n"          (leaf node, no content)
        "mention"     => node["attrs"]["text"]  (e.g., "@Matt Johnson")
        "inlineCard"  => node["attrs"]["url"]   (or extract issue key from URL)
        "emoji"       => node["attrs"]["shortName"]  (e.g., ":thumbsup:")
        "rule"        => "---"         (horizontal rule, no content)
        "table"       => flatten all cells' text with spaces
        _             => recurse into content if present, else ""
    }
}
```

**Critical**: `"content"` is optional on any node. `hardBreak`, `mention`, `inlineCard`, `emoji`, `rule` have NO content array. Always check before recursing.

### ADF Node Examples

**Paragraph with inline marks** (bold, italic, link):
```json
{
  "type": "paragraph",
  "content": [
    { "type": "text", "text": "Normal " },
    { "type": "text", "text": "bold", "marks": [{ "type": "strong" }] },
    { "type": "text", "text": " and " },
    { "type": "text", "text": "linked text", "marks": [{ "type": "link", "attrs": { "href": "https://example.com" } }] }
  ]
}
```
→ `"Normal bold and linked text (https://example.com)"`

For plain text output: ignore `marks` except for `link` (append URL in parens).

**Heading**:
```json
{ "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "My Heading" }] }
```
→ `"## My Heading"`

**Bullet list**:
```json
{
  "type": "bulletList",
  "content": [
    { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Item one" }] }] },
    { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Item two" }] }] }
  ]
}
```
→ `"- Item one\n- Item two"`

**Code block**:
```json
{ "type": "codeBlock", "attrs": { "language": "rust" }, "content": [{ "type": "text", "text": "fn main() {}" }] }
```
→ `"    fn main() {}"`

---

## Error Responses

All JIRA API errors follow this format:

```json
{
  "errorMessages": ["Issue Does Not Exist"],
  "errors": {}
}
```

Or for field-level validation errors:
```json
{
  "errorMessages": [],
  "errors": {
    "summary": "You must specify a summary of the issue.",
    "resolution": "Resolution is required when moving to Done."
  }
}
```

HTTP status codes:
- `400` — Bad request (validation error, missing required field)
- `401` — Unauthorized (bad token or email)
- `403` — Forbidden (no permission for this project/action)
- `404` — Not found (issue deleted, project doesn't exist)
- `429` — Rate limited (check `Retry-After` header for seconds to wait)

**Rust deserialization**:
```rust
#[derive(Deserialize)]
struct JiraErrorResponse {
    #[serde(rename = "errorMessages", default)]
    error_messages: Vec<String>,
    #[serde(default)]
    errors: HashMap<String, String>,
}
```

Combine into a display string: join `error_messages` + join `errors` values.

---

## Data Model Corrections

Based on API response shapes, these `jira-plugin.md` structs need adjustment:

### `AllowedValue` — NEW struct needed

```rust
/// Used in EditableField and TransitionField for select-type fields.
/// Display `name` to user, send `{ "id": "..." }` in write bodies.
struct AllowedValue {
    id: String,
    name: String,
}
```

### `EditableField` — fix `allowed_values` type

```rust
struct EditableField {
    field_id: String,
    name: String,
    field_type: FieldType,
    required: bool,
    allowed_values: Option<Vec<AllowedValue>>,  // was Vec<String>, needs id+name pairs
}
```

### `JiraTransition` — fix field name and add required fields struct

```rust
struct JiraTransition {
    id: String,
    name: String,
    to_status: JiraStatus,               // mapped from .to (not .to_status)
    required_fields: Vec<TransitionField>, // from .fields map, filtered to required
}

struct TransitionField {
    field_id: String,
    name: String,
    allowed_values: Vec<AllowedValue>,
    is_comment: bool,  // true if field_id == "comment" (uses "update" key in POST)
}
```

### Endpoints to REMOVE from `JiraCommand`

- `GET /rest/api/3/issue/{key}` — not needed, search response has all data
- `GET /rest/api/3/status` — derive from issue data, not a separate call

If individual issue refresh is ever needed, add it later.
