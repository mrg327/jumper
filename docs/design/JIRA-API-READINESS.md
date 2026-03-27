# JIRA API Readiness Assessment

Assessment of whether the jira-plugin.md design doc contains sufficient API-specific detail for an autonomous coding agent to implement the API layer correctly without trial-and-error against a live JIRA Cloud instance.

Assessed against: JIRA Cloud REST API v3 (platform REST API, not Forge/Connect).

---

## 1. Endpoint-by-Endpoint Assessment

### 1.1 `GET /rest/api/3/myself`

- **Request completeness**: GREEN
- **Response mapping**: YELLOW
- **Missing details**: The doc says "retrieve the user's `accountId`" but does not specify the full response shape. An agent needs to know the exact JSON field path.

The actual response includes many fields. The agent only needs `accountId`, but should know the shape to write the deserialization struct:

```json
{
  "self": "https://myorg.atlassian.net/rest/api/3/user?accountId=5b10a2844c20165700ede21g",
  "accountId": "5b10a2844c20165700ede21g",
  "accountType": "atlassian",
  "emailAddress": "matt@company.com",
  "avatarUrls": { "48x48": "...", "24x24": "...", "16x16": "...", "32x32": "..." },
  "displayName": "Matt Johnson",
  "active": true,
  "timeZone": "America/Chicago",
  "locale": "en_US"
}
```

The agent needs to extract: `accountId` (String). Deserialize with `#[serde(rename_all = "camelCase")]` or just pull `accountId` from a `serde_json::Value`.

---

### 1.2 `GET /rest/api/3/search`

- **Request completeness**: YELLOW
- **Response mapping**: RED
- **Missing details**: This is the most critical endpoint and has the largest gaps.

**Request gaps:**
- The doc says `JQL: assignee = '<accountId>'` but does not specify the full set of query parameters. The agent needs to know:
  - `jql` (String) -- the JQL query
  - `startAt` (int, default 0) -- pagination offset
  - `maxResults` (int, default 50, max 100) -- page size
  - `fields` (comma-separated String) -- which fields to return. Without this, the response is enormous with every field. The agent MUST specify fields to get story points, sprint, epic, etc.
  - `expand` (String) -- not needed for this use case

The agent should use a `fields` parameter to request only the fields it needs. Otherwise responses are bloated and may include hundreds of custom fields.

**Recommended request:**
```
GET /rest/api/3/search?jql=assignee%3D%27{accountId}%27%20AND%20statusCategory%20!%3D%20Done&startAt=0&maxResults=100&fields=summary,status,priority,issuetype,assignee,reporter,created,updated,description,labels,components,project,parent,customfield_10016,customfield_10020
```

Note: The JQL in the doc uses `assignee = '<accountId>'` but does not mention filtering out Done issues. The kanban board hides Done by default, but the search fetches ALL assigned issues. The doc should clarify whether Done issues are fetched (and filtered client-side) or excluded via JQL.

**Response gaps (RED):** The doc does not show the actual `/search` response shape. This is the most complex response in the entire integration and the most likely source of implementation errors. The actual response:

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
          "self": "https://...",
          "id": "10001",
          "name": "In Progress",
          "statusCategory": {
            "self": "https://...",
            "id": 4,
            "key": "indeterminate",
            "colorName": "blue",
            "name": "In Progress"
          }
        },
        "priority": {
          "self": "https://...",
          "id": "2",
          "name": "High",
          "iconUrl": "https://..."
        },
        "issuetype": {
          "self": "https://...",
          "id": "10001",
          "name": "Story",
          "subtask": false,
          "iconUrl": "https://...",
          "hierarchyLevel": 0
        },
        "assignee": {
          "self": "https://...",
          "accountId": "5b10a2844c20165700ede21g",
          "displayName": "Matt Johnson",
          "active": true,
          "avatarUrls": { "48x48": "...", "24x24": "...", "16x16": "...", "32x32": "..." }
        },
        "reporter": {
          "self": "https://...",
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
                {
                  "type": "text",
                  "text": "The navigation bar focus ring is not visible..."
                }
              ]
            }
          ]
        },
        "labels": ["frontend", "accessibility"],
        "components": [
          {
            "self": "https://...",
            "id": "10000",
            "name": "hmi-nav"
          }
        ],
        "project": {
          "self": "https://...",
          "id": "10000",
          "key": "HMI",
          "name": "HMI Framework",
          "projectTypeKey": "software",
          "avatarUrls": { "48x48": "...", "24x24": "...", "16x16": "...", "32x32": "..." }
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

**Critical mapping details the agent needs:**

1. **`status.statusCategory.key`** is the field that maps to `StatusCategory`. The possible values are `"new"`, `"indeterminate"`, `"done"`, and `"undefined"`. The doc mentions these 4 categories but does not say to use the `key` field (not `name`, not `id`).

2. **`priority`** is an object `{ "name": "High" }`, not a bare string. The agent must extract `.priority.name`. Priority can also be `null` if unset.

3. **`assignee` and `reporter`** are objects with `displayName`, or `null` if unassigned. Extract `.displayName`.

4. **`components`** is an array of objects `[{ "name": "hmi-nav" }]`, not strings. Extract `.name` from each.

5. **`description`** is ADF (a JSON object), NOT a string. It can also be `null`. The doc mentions ADF conversion but does not show that description arrives as a nested JSON object in the search response.

6. **`customfield_10020` (sprint)** is an **array** of sprint objects, not a single object. The active sprint is the one with `"state": "active"`. If the array is empty or the field is null, there is no sprint.

7. **`customfield_10016` (story points)** is a bare number (float), or `null`. NOT an object.

8. **Epic**: The doc defines `EpicInfo { key, name }` but does not specify how to extract epic data from the search response. In JIRA Cloud v3, the epic link is typically the `parent` field (for next-gen projects) or a custom field like `customfield_10014` (for classic projects). The `parent` field looks like:
   ```json
   "parent": {
     "id": "10019",
     "key": "NAV-1",
     "self": "https://...",
     "fields": {
       "summary": "Navigation Rework",
       "status": { ... },
       "priority": { ... },
       "issuetype": {
         "id": "10000",
         "name": "Epic",
         "subtask": false,
         "hierarchyLevel": 1
       }
     }
   }
   ```
   The agent must check if `parent.fields.issuetype.name == "Epic"` (or `hierarchyLevel == 1`) to determine if the parent is an epic. The doc should specify this extraction logic. For company-managed projects using the legacy epic link custom field, the approach is different. This is a significant gap.

9. **Pagination**: The doc says "Paginated -- fetch ALL pages" and "incrementing startAt" but does not specify: check `startAt + issues.len() < total` to know if there are more pages. `maxResults` should be set to 100 (the API maximum for cloud).

---

### 1.3 `GET /rest/api/3/field`

- **Request completeness**: GREEN (no query params needed)
- **Response mapping**: YELLOW
- **Missing details**: See Section 4 below for full details.

---

### 1.4 `GET /rest/api/3/issue/{key}`

- **Request completeness**: YELLOW
- **Response mapping**: YELLOW
- **Missing details**: The doc lists this endpoint but never describes when it is called -- the `JiraCommand` enum does not have a `FetchIssue` variant. The search response already includes issue details, so this endpoint may be unnecessary. If it IS needed (e.g., to get fresh data for a single issue), the agent needs to know:
  - Query param `fields` should be specified (same as search)
  - Response shape is the same as a single element of the search `issues` array (i.e., `{ "id": "...", "key": "...", "fields": { ... } }`)
  - Query param `expand=transitions` can be used to get transitions in the same call, avoiding a second request

**Verdict**: The agent may be confused about whether to use this endpoint at all, since there is no command for it.

---

### 1.5 `GET /rest/api/3/issue/{key}/transitions`

- **Request completeness**: GREEN
- **Response mapping**: RED
- **Missing details**: The actual response shape is not documented. See Section 5 below for full details. The `fields` object on transitions (for required fields) is particularly complex and undocumented.

Actual response:
```json
{
  "transitions": [
    {
      "id": "21",
      "name": "Start Review",
      "to": {
        "self": "https://...",
        "id": "10002",
        "name": "Code Review",
        "statusCategory": {
          "self": "https://...",
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
        "self": "https://...",
        "id": "10003",
        "name": "Done",
        "statusCategory": {
          "self": "https://...",
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
          "schema": {
            "type": "resolution",
            "system": "resolution"
          },
          "name": "Resolution",
          "hasDefaultValue": false,
          "operations": ["set"],
          "allowedValues": [
            { "self": "https://...", "id": "1", "name": "Done", "description": "Work has been completed." },
            { "self": "https://...", "id": "2", "name": "Won't Do", "description": "..." },
            { "self": "https://...", "id": "3", "name": "Duplicate", "description": "..." }
          ]
        }
      }
    }
  ]
}
```

Key points the doc misses:
- The target status is in `.to`, not `.to_status`
- The `fields` object is a **map** keyed by field ID (e.g., `"resolution"`), not an array
- Each field value contains `required`, `schema`, `name`, `allowedValues`, `operations`
- The `allowedValues` items are objects with `id` and `name`, and the POST body must reference the value by `id`, not `name`

---

### 1.6 `POST /rest/api/3/issue/{key}/transitions`

- **Request completeness**: RED
- **Response mapping**: GREEN (204 No Content on success)
- **Missing details**: The doc does not show the POST body shape.

Actual POST body:
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

Critical details:
- The transition ID is wrapped in `{ "transition": { "id": "..." } }`, not sent as a bare field
- Fields with `allowedValues` must be set as `{ "id": "..." }`, not as a bare string like `"Done"`
- The `fields` key is a sibling of `transition`, not nested inside it
- If there are no required fields, the `fields` key can be omitted entirely
- Success returns **204 No Content** (empty body), not 200

---

### 1.7 `PUT /rest/api/3/issue/{key}`

- **Request completeness**: RED
- **Response mapping**: GREEN (204 No Content on success)
- **Missing details**: The doc says `UpdateField { issue_key, field_id, value }` but does not show the actual PUT body structure.

Actual PUT body:
```json
{
  "fields": {
    "summary": "Updated summary text",
    "priority": { "id": "2" },
    "customfield_10016": 5.0,
    "labels": ["frontend", "accessibility", "new-label"]
  }
}
```

Critical details:
- All field updates are wrapped in a top-level `"fields"` object
- Simple text fields (summary) are bare strings
- Select fields (priority) require `{ "id": "..." }` or `{ "name": "..." }` -- using `id` is more reliable
- Number fields (story points) are bare numbers
- Array fields (labels) must send the COMPLETE array, not a diff
- Multi-select fields (components) are arrays of objects: `[{ "id": "..." }]`
- Success returns **204 No Content** (empty body)
- The `notifyUsers` query parameter can be set to `false` to suppress email notifications: `PUT /rest/api/3/issue/{key}?notifyUsers=false`

The `JiraCommand::UpdateField` sends a single field at a time, which is fine (the PUT body can contain just one field).

---

### 1.8 `GET /rest/api/3/issue/{key}/comment`

- **Request completeness**: YELLOW
- **Response mapping**: RED
- **Missing details**: Pagination params and response shape are not documented.

Query parameters:
- `startAt` (int, default 0)
- `maxResults` (int, default 50)
- `orderBy` -- use `"-created"` for newest first

Actual response:
```json
{
  "startAt": 0,
  "maxResults": 50,
  "total": 3,
  "comments": [
    {
      "self": "https://...",
      "id": "10023",
      "author": {
        "self": "https://...",
        "accountId": "5b10a2844c20165700ede21g",
        "displayName": "Matt Johnson",
        "active": true,
        "avatarUrls": { ... }
      },
      "body": {
        "version": 1,
        "type": "doc",
        "content": [
          {
            "type": "paragraph",
            "content": [
              {
                "type": "text",
                "text": "Started work on the focus ring."
              }
            ]
          }
        ]
      },
      "created": "2026-03-25T12:20:00.000+0000",
      "updated": "2026-03-25T12:20:00.000+0000"
    }
  ]
}
```

Critical details:
- `author` is an object with `displayName`, not a bare string. The `JiraComment` struct has `author: String` but the agent needs to know to extract `.author.displayName`.
- `body` is ADF (JSON object), NOT a string. The `JiraComment` struct has `body: String` (plain text), so the agent must convert ADF to text during deserialization.
- Comments may need pagination if there are many (>50). The doc does not address this.
- The `orderBy` parameter uses the format `"-created"` (minus prefix for descending), or `"+created"` for ascending. This parameter is optional; default order is chronological.

---

### 1.9 `POST /rest/api/3/issue/{key}/comment`

- **Request completeness**: YELLOW
- **Response mapping**: GREEN (returns the created comment)
- **Missing details**: The doc says the body is "ADF JSON" but does not show the actual POST body.

Actual POST body:
```json
{
  "body": {
    "version": 1,
    "type": "doc",
    "content": [
      {
        "type": "paragraph",
        "content": [
          {
            "type": "text",
            "text": "This is my comment text."
          }
        ]
      }
    ]
  }
}
```

The doc's `text_to_adf()` function generates the correct inner structure, but the agent needs to know that the POST body wraps it as `{ "body": <adf_document> }`. This is implied by `AddComment { body: serde_json::Value }` but not explicitly stated.

Success returns **201 Created** with the full comment object in the response body.

---

### 1.10 `GET /rest/api/3/issue/{key}/editmeta`

- **Request completeness**: GREEN
- **Response mapping**: RED
- **Missing details**: The response shape is not documented at all.

Actual response:
```json
{
  "fields": {
    "summary": {
      "required": true,
      "schema": {
        "type": "string",
        "system": "summary"
      },
      "name": "Summary",
      "operations": ["set"],
      "allowedValues": null
    },
    "priority": {
      "required": false,
      "schema": {
        "type": "priority",
        "system": "priority"
      },
      "name": "Priority",
      "operations": ["set"],
      "allowedValues": [
        { "self": "https://...", "id": "1", "name": "Highest", "iconUrl": "..." },
        { "self": "https://...", "id": "2", "name": "High", "iconUrl": "..." },
        { "self": "https://...", "id": "3", "name": "Medium", "iconUrl": "..." },
        { "self": "https://...", "id": "4", "name": "Low", "iconUrl": "..." },
        { "self": "https://...", "id": "5", "name": "Lowest", "iconUrl": "..." }
      ]
    },
    "labels": {
      "required": false,
      "schema": {
        "type": "array",
        "items": "string",
        "system": "labels"
      },
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
      "schema": {
        "type": "any",
        "system": "description"
      },
      "name": "Description",
      "operations": ["set"]
    }
  }
}
```

Critical details for mapping to `EditableField`:
- The response is a **map** keyed by field ID, not an array. The agent must iterate the map entries.
- `schema.type` determines `FieldType`:
  - `"string"` -> `FieldType::Text` (unless it has `"custom": "...textarea..."`, then `TextArea`)
  - `"number"` -> `FieldType::Number`
  - `"priority"`, `"resolution"`, `"option"` with `allowedValues` -> `FieldType::Select`
  - `"array"` with `items: "string"` (like labels) -> could be `FieldType::MultiSelect` or `Text` depending on UI approach
  - `"any"` for description -> the doc says to exclude description, so filter by field ID
- The doc says to exclude `"description"` from editable fields. The agent should filter out field ID `"description"` explicitly.
- `allowedValues` items are objects with `id` and `name`. The `EditableField.allowed_values` is `Option<Vec<String>>` -- the agent needs to know whether to store names or IDs (should store both, or at minimum store names for display and IDs for the PUT body).
- The `operations` array tells you what kind of updates are allowed (`"set"`, `"add"`, `"remove"`). The agent should check for `"set"` to confirm the field is editable.

---

### 1.11 `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes`

- **Request completeness**: GREEN
- **Response mapping**: RED
- **Missing details**: Response shape not documented.

Actual response:
```json
{
  "values": [
    {
      "self": "https://...",
      "id": "10001",
      "name": "Story",
      "subtask": false,
      "iconUrl": "https://...",
      "hierarchyLevel": 0
    },
    {
      "self": "https://...",
      "id": "10002",
      "name": "Bug",
      "subtask": false,
      "iconUrl": "https://..."
    },
    {
      "self": "https://...",
      "id": "10003",
      "name": "Task",
      "subtask": false,
      "iconUrl": "https://..."
    },
    {
      "self": "https://...",
      "id": "10004",
      "name": "Sub-task",
      "subtask": true,
      "iconUrl": "https://..."
    }
  ]
}
```

Note: The response is wrapped in `"values"`, not `"issueTypes"`. This is a v3 API change that catches many implementors. The old createmeta endpoint (deprecated) used a different structure. The agent needs to know the wrapper key is `"values"`.

The agent also needs the `id` for each issue type, to use in the next call and in the create issue POST body.

---

### 1.12 `GET /rest/api/3/issue/createmeta/{projectKey}/issuetypes/{issueTypeId}`

- **Request completeness**: YELLOW
- **Response mapping**: RED
- **Missing details**: Response shape not documented. See Section 6 below.

Query parameters:
- `startAt` (int, default 0) -- this endpoint is paginated
- `maxResults` (int, default 50)

Actual response:
```json
{
  "maxResults": 50,
  "startAt": 0,
  "total": 8,
  "values": [
    {
      "required": true,
      "schema": {
        "type": "string",
        "system": "summary"
      },
      "name": "Summary",
      "fieldId": "summary",
      "hasDefaultValue": false,
      "operations": ["set"]
    },
    {
      "required": true,
      "schema": {
        "type": "issuetype",
        "system": "issuetype"
      },
      "name": "Issue Type",
      "fieldId": "issuetype",
      "hasDefaultValue": false,
      "operations": ["set"],
      "allowedValues": [
        { "self": "https://...", "id": "10001", "name": "Story", "subtask": false }
      ]
    },
    {
      "required": true,
      "schema": {
        "type": "project",
        "system": "project"
      },
      "name": "Project",
      "fieldId": "project",
      "hasDefaultValue": false,
      "operations": ["set"]
    },
    {
      "required": false,
      "schema": {
        "type": "priority",
        "system": "priority"
      },
      "name": "Priority",
      "fieldId": "priority",
      "hasDefaultValue": true,
      "operations": ["set"],
      "allowedValues": [
        { "self": "https://...", "id": "1", "name": "Highest" },
        { "self": "https://...", "id": "2", "name": "High" },
        { "self": "https://...", "id": "3", "name": "Medium" },
        { "self": "https://...", "id": "4", "name": "Low" },
        { "self": "https://...", "id": "5", "name": "Lowest" }
      ]
    },
    {
      "required": false,
      "schema": {
        "type": "array",
        "items": "string",
        "system": "labels"
      },
      "name": "Labels",
      "fieldId": "labels",
      "hasDefaultValue": false,
      "operations": ["add", "set", "remove"]
    },
    {
      "required": false,
      "schema": {
        "type": "any",
        "system": "description"
      },
      "name": "Description",
      "fieldId": "description",
      "hasDefaultValue": false,
      "operations": ["set"]
    }
  ]
}
```

Critical details:
- Response is **paginated** with `"values"` wrapper. If a project/issue type combo has many fields (custom fields), the agent must paginate.
- Each field has `fieldId` (the key to use in the create POST body) and `name` (display name).
- `required: true` fields MUST be included in the POST body. But some "required" fields like `project` and `issuetype` are set automatically by the plugin (not user input). The agent should filter out `project`, `issuetype`, and `reporter` from user-facing required fields.
- `hasDefaultValue: true` means JIRA will fill in a default if omitted, even if `required: true`. Priority often has a default.
- `allowedValues` items use `id` and `name`. The POST body needs `{ "id": "..." }`.

---

### 1.13 `POST /rest/api/3/issue`

- **Request completeness**: RED
- **Response mapping**: YELLOW
- **Missing details**: The doc says `CreateIssue { project_key, fields: serde_json::Value }` but does not show the actual POST body.

Actual POST body:
```json
{
  "fields": {
    "project": {
      "key": "HMI"
    },
    "issuetype": {
      "id": "10002"
    },
    "summary": "Fix crash when pressing Back",
    "priority": {
      "id": "2"
    },
    "assignee": {
      "accountId": "5b10a2844c20165700ede21g"
    },
    "labels": ["frontend"],
    "components": [
      { "id": "10000" }
    ],
    "description": {
      "version": 1,
      "type": "doc",
      "content": [
        {
          "type": "paragraph",
          "content": [
            { "type": "text", "text": "Description text here." }
          ]
        }
      ]
    }
  }
}
```

Critical details:
- `project` uses `{ "key": "HMI" }`, not a bare string
- `issuetype` uses `{ "id": "10002" }`, not `{ "name": "Bug" }` (id is more reliable)
- `assignee` uses `{ "accountId": "..." }` -- the doc says "auto-assign to configured user" but does not clarify this uses the `accountId` from the `/myself` call
- Select fields (priority) use `{ "id": "..." }`
- Array-of-object fields (components) use `[{ "id": "..." }]`
- Description is ADF if provided

Success returns **201 Created** with:
```json
{
  "id": "10024",
  "key": "HMI-116",
  "self": "https://myorg.atlassian.net/rest/api/3/issue/10024"
}
```

The agent needs the `key` from the response for the toast message.

---

### 1.14 `GET /rest/api/3/status`

- **Request completeness**: GREEN
- **Response mapping**: YELLOW
- **Missing details**: Listed in the endpoint table as "Get all statuses (for workflow discovery)" but never referenced in the command types or any workflow. The `JiraCommand` enum has no `FetchStatuses` variant.

Actual response:
```json
[
  {
    "self": "https://...",
    "id": "1",
    "name": "Open",
    "description": "",
    "statusCategory": {
      "self": "https://...",
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

Note: This returns ALL statuses across all projects. It can be useful for pre-loading status metadata, but the search response already includes status details per issue. The agent may not need this endpoint at all. The doc should clarify if and when this is called.

---

## 2. Authentication Details

**Assessment: YELLOW**

The doc specifies:
```
Authorization: Basic base64(email:api_token)
Content-Type: application/json
```

This is correct but incomplete. The agent needs to know:

1. **Base64 encoding**: The format is `base64("{email}:{api_token}")` -- the colon is a literal colon joining the two values, then the whole string is base64-encoded. Example: `base64("matt@company.com:myapitoken123")` = `"bWF0dEBjb21wYW55LmNvbTpteWFwaXRva2VuMTIz"`.

2. **Header format**: `Authorization: Basic bWF0dEBjb21wYW55LmNvbTpteWFwaXRva2VuMTIz` -- there is a space between "Basic" and the encoded string.

3. **Accept header**: Requests should also include `Accept: application/json`. While not strictly required for most endpoints, it is best practice and ensures the response format. Some endpoints may return XML otherwise.

4. **User-Agent header**: Not required, but a custom User-Agent (e.g., `jm-tui/0.1`) is good practice and helps Atlassian support debug issues.

5. **API token source**: The doc correctly says `JIRA_API_TOKEN` env var. The agent should be told that this is an Atlassian API token generated at https://id.atlassian.com/manage-profile/security/api-tokens -- it is NOT an OAuth token, not a PAT (Personal Access Token in the Data Center sense), and not a session cookie.

6. **ureq specifics**: With ureq v3, setting headers looks like:
   ```rust
   let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", email, token));
   agent.get(&url)
       .header("Authorization", &format!("Basic {}", auth))
       .header("Content-Type", "application/json")
       .header("Accept", "application/json")
       .call()?;
   ```
   The doc should note that ureq v3 has a different API surface than v2 (method chaining changed). Since the doc specifies `ureq = { version = "3" }`, the agent needs v3-compatible examples.

**Recommendation**: Add a concrete Rust code snippet showing the auth header construction with ureq v3 and the `base64` crate.

---

## 3. ADF (Atlassian Document Format)

**Assessment: YELLOW for ADF->plaintext, GREEN for plaintext->ADF**

### Plaintext to ADF (GREEN)

The doc provides a working `text_to_adf()` function. This is sufficient for the agent. One minor gap: multi-paragraph text. If the user types multiple paragraphs (separated by blank lines) in $EDITOR, should each paragraph become its own ADF paragraph node? The current implementation wraps everything in one paragraph, which means newlines become literal `\n` in a single text node. This is functional but ugly in JIRA's web UI. Consider:

```rust
fn text_to_adf(text: &str) -> serde_json::Value {
    let paragraphs: Vec<Value> = text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| json!({
            "type": "paragraph",
            "content": [{ "type": "text", "text": p.trim() }]
        }))
        .collect();

    json!({
        "version": 1,
        "type": "doc",
        "content": paragraphs
    })
}
```

This is a minor enhancement, not a blocker.

### ADF to Plaintext (YELLOW)

The doc lists the conversion rules (paragraphs, headings, lists, code blocks, links) but does not provide example ADF structures for each node type. An agent implementing the recursive ADF walker needs to know the exact JSON shapes. Here they are:

**Simple paragraph:**
```json
{
  "type": "paragraph",
  "content": [
    { "type": "text", "text": "Hello world" }
  ]
}
```

**Paragraph with inline formatting (bold, italic, link):**
```json
{
  "type": "paragraph",
  "content": [
    { "type": "text", "text": "This is " },
    {
      "type": "text",
      "text": "bold text",
      "marks": [{ "type": "strong" }]
    },
    { "type": "text", "text": " and " },
    {
      "type": "text",
      "text": "italic",
      "marks": [{ "type": "em" }]
    },
    { "type": "text", "text": " and " },
    {
      "type": "text",
      "text": "a link",
      "marks": [{
        "type": "link",
        "attrs": { "href": "https://example.com" }
      }]
    }
  ]
}
```
Output: `This is bold text and italic and a link (https://example.com)`

**Heading (level 1-6):**
```json
{
  "type": "heading",
  "attrs": { "level": 2 },
  "content": [
    { "type": "text", "text": "My Heading" }
  ]
}
```
Output: `## My Heading`

**Bullet list:**
```json
{
  "type": "bulletList",
  "content": [
    {
      "type": "listItem",
      "content": [
        {
          "type": "paragraph",
          "content": [
            { "type": "text", "text": "First item" }
          ]
        }
      ]
    },
    {
      "type": "listItem",
      "content": [
        {
          "type": "paragraph",
          "content": [
            { "type": "text", "text": "Second item" }
          ]
        }
      ]
    }
  ]
}
```
Output:
```
- First item
- Second item
```

**Ordered list:**
```json
{
  "type": "orderedList",
  "attrs": { "order": 1 },
  "content": [
    {
      "type": "listItem",
      "content": [
        {
          "type": "paragraph",
          "content": [
            { "type": "text", "text": "First item" }
          ]
        }
      ]
    }
  ]
}
```
Output: `1. First item`

**Code block:**
```json
{
  "type": "codeBlock",
  "attrs": { "language": "rust" },
  "content": [
    { "type": "text", "text": "fn main() {\n    println!(\"hello\");\n}" }
  ]
}
```
Output (indented):
```
    fn main() {
        println!("hello");
    }
```

**Mention (user @-mention):**
```json
{
  "type": "mention",
  "attrs": {
    "id": "5b10a2844c20165700ede21g",
    "text": "@Matt Johnson",
    "accessLevel": ""
  }
}
```
Output: `@Matt Johnson`

**Inline card (JIRA issue link):**
```json
{
  "type": "inlineCard",
  "attrs": {
    "url": "https://myorg.atlassian.net/browse/HMI-103"
  }
}
```
Output: `HMI-103` (extract issue key from URL) or just the URL.

**Table:**
```json
{
  "type": "table",
  "attrs": { "isNumberColumnEnabled": false, "layout": "default" },
  "content": [
    {
      "type": "tableRow",
      "content": [
        {
          "type": "tableHeader",
          "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Header" }] }]
        }
      ]
    },
    {
      "type": "tableRow",
      "content": [
        {
          "type": "tableCell",
          "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Cell" }] }]
        }
      ]
    }
  ]
}
```
Output: Tables are complex; the doc's rule "extract text content, strip formatting" would flatten this to `Header\nCell`. Acceptable for a TUI.

**Key implementation note**: ADF is a recursive tree. The conversion function should be a recursive walker that pattern-matches on `node["type"]` and recurses into `node["content"]` (an array of child nodes). Text leaf nodes have `"type": "text"` and a `"text"` field. Non-text leaf nodes (mentions, inline cards, media, emoji, hardBreak) need special handling. Unknown node types should be handled gracefully (extract any nested text, or skip).

**Hard break:**
```json
{ "type": "hardBreak" }
```
Output: `\n`

**The agent needs to know that `"content"` is optional on any node** -- some nodes (like `hardBreak`, `mention`, `inlineCard`, `rule`) have no content array. The walker must handle this.

---

## 4. Custom Field Discovery

**Assessment: YELLOW**

### `GET /rest/api/3/field` response shape:

```json
[
  {
    "id": "summary",
    "key": "summary",
    "name": "Summary",
    "custom": false,
    "orderable": true,
    "navigable": true,
    "searchable": true,
    "clauseNames": ["summary"],
    "schema": {
      "type": "string",
      "system": "summary"
    }
  },
  {
    "id": "customfield_10016",
    "key": "customfield_10016",
    "name": "Story Points",
    "custom": true,
    "orderable": true,
    "navigable": true,
    "searchable": true,
    "clauseNames": ["cf[10016]", "Story Points"],
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
    "orderable": true,
    "navigable": true,
    "searchable": true,
    "clauseNames": ["cf[10020]", "Sprint"],
    "schema": {
      "type": "array",
      "items": "json",
      "custom": "com.pyxis.greenhopper.jira:gh-sprint",
      "customId": 10020
    }
  }
]
```

Key details:
- The response is a **flat array** (not paginated, not wrapped in an object). Can be hundreds of entries on a real instance.
- `custom: true` identifies custom fields.
- For story points: look for `name` containing "story point" (case-insensitive) AND `schema.custom` containing `"float"` to disambiguate from other fields with similar names.
- For sprint: look for `schema.custom` containing `"gh-sprint"` -- this is more reliable than name matching since "Sprint" is a common word. Alternatively, match `name` case-insensitively.
- The doc's approach (case-insensitive name match) is acceptable but could be made more robust with schema type matching.

### Sprint field value on an issue:

The doc does not describe what the sprint field value looks like on an issue. This is important:

```json
"customfield_10020": [
  {
    "id": 37,
    "name": "Sprint 24",
    "state": "active",
    "boardId": 5,
    "startDate": "2026-03-18T00:00:00.000Z",
    "endDate": "2026-04-01T00:00:00.000Z"
  },
  {
    "id": 35,
    "name": "Sprint 22",
    "state": "closed",
    "boardId": 5,
    "startDate": "2026-02-18T00:00:00.000Z",
    "endDate": "2026-03-04T00:00:00.000Z"
  }
]
```

- It is an **array** of sprint objects (an issue can be in multiple sprints historically).
- The `state` field values are: `"active"`, `"closed"`, `"future"`.
- To get the current sprint name: find the sprint with `"state": "active"`. If none is active, use `"future"` if present, otherwise the most recent `"closed"`.
- The field can be `null` if no sprint is assigned.

The `JiraIssue` struct has `sprint: Option<String>` -- the agent needs to know to extract the active sprint's `name`.

---

## 5. Transition Fields

**Assessment: RED**

This is a significant gap. The doc mentions "required fields" for transitions but does not show:

### What the transitions response `fields` object looks like:

See Section 1.5 above for the full response. Key additional details:

**The `fields` map within a transition can be empty** (`{}`) when no fields are required. The agent must handle this case (empty map = no required fields = execute transition immediately).

**Common required transition fields:**

1. **Resolution** (on "Done" transitions):
```json
"resolution": {
  "required": true,
  "schema": { "type": "resolution", "system": "resolution" },
  "name": "Resolution",
  "hasDefaultValue": false,
  "operations": ["set"],
  "allowedValues": [
    { "self": "https://...", "id": "1", "name": "Done" },
    { "self": "https://...", "id": "2", "name": "Won't Do" },
    { "self": "https://...", "id": "3", "name": "Duplicate" },
    { "self": "https://...", "id": "4", "name": "Cannot Reproduce" }
  ]
}
```

2. **Comment** (sometimes required on certain transitions):
```json
"comment": {
  "required": true,
  "schema": { "type": "comments-page", "system": "comment" },
  "name": "Comment",
  "hasDefaultValue": false,
  "operations": ["add"]
}
```

### POST body with transition fields:

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

For a transition with a required comment:
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
            "content": [{
              "type": "paragraph",
              "content": [{ "type": "text", "text": "Closing as completed." }]
            }]
          }
        }
      }
    ]
  }
}
```

Note the asymmetry: regular fields go in `"fields"`, but comments go in `"update"` with an `"add"` operation. This is a common source of bugs. The agent needs to know about this distinction.

### Mapping transition `fields` to `EditableField`:

The `JiraTransition` struct has `required_fields: Vec<EditableField>`. The agent needs to:
1. Iterate the `fields` map from the transition response
2. Filter to only `required: true` fields
3. Map `schema.type` to `FieldType`
4. Extract `allowedValues` names for select fields
5. Store the field ID as the key (e.g., `"resolution"`)

The doc's `EditableField` struct stores `allowed_values: Option<Vec<String>>` but the POST body needs `{ "id": "..." }`, not a name string. The agent will need to maintain a mapping from display name to ID, or change the struct to store both.

---

## 6. Createmeta

**Assessment: RED**

See Section 1.11 and 1.12 above for the response shapes.

### Additional critical details:

**Required vs optional fields:**
- `required: true` means the field MUST be in the POST body (unless `hasDefaultValue: true`, in which case JIRA fills a default).
- The agent should present `required: true` fields first in the form, with a visual indicator.
- Some "required" fields are not user-facing: `project`, `issuetype`, and `reporter` are set by the plugin. The form should exclude these.

**Allowed values structure:**
- Select fields have `allowedValues` as an array of objects: `[{ "id": "...", "name": "..." }]`
- The form should display `name`, but the POST body uses `{ "id": "..." }`
- Some fields (like labels) have `autoCompleteUrl` instead of `allowedValues` -- these are free-text with autocomplete, not a fixed list. The TUI should treat these as text input.

**Pagination of createmeta fields:**
- The per-issuetype endpoint is paginated. A project with many custom fields may return multiple pages. The agent must paginate this endpoint.
- `startAt` + `maxResults` pattern, same as search.

**The project list for the creation flow:**
- The doc says "only projects with existing assignments shown" -- the project list is derived from the distinct `project_key` values of assigned issues. No additional API call is needed for the project list.
- The agent must extract `{ key, name }` from the issue data and deduplicate.

---

## 7. Overall Assessment

### API Readiness Score: 3/14 endpoints are GREEN

| # | Endpoint | Request | Response | Overall |
|---|----------|---------|----------|---------|
| 1 | `GET /myself` | GREEN | YELLOW | YELLOW |
| 2 | `GET /search` | YELLOW | RED | RED |
| 3 | `GET /field` | GREEN | YELLOW | YELLOW |
| 4 | `GET /issue/{key}` | YELLOW | YELLOW | YELLOW |
| 5 | `GET /issue/{key}/transitions` | GREEN | RED | RED |
| 6 | `POST /issue/{key}/transitions` | RED | GREEN | RED |
| 7 | `PUT /issue/{key}` | RED | GREEN | RED |
| 8 | `GET /issue/{key}/comment` | YELLOW | RED | RED |
| 9 | `POST /issue/{key}/comment` | YELLOW | GREEN | YELLOW |
| 10 | `GET /issue/{key}/editmeta` | GREEN | RED | RED |
| 11 | `GET /createmeta/{proj}/issuetypes` | GREEN | RED | RED |
| 12 | `GET /createmeta/{proj}/issuetypes/{id}` | YELLOW | RED | RED |
| 13 | `POST /issue` | RED | YELLOW | RED |
| 14 | `GET /status` | GREEN | YELLOW | YELLOW |

- **GREEN**: 3 (request-only endpoints with simple shapes: `/myself` request, `/field` request, `/status` request)
- **YELLOW**: 4
- **RED**: 7

### Critical Gaps That MUST Be Added Before Implementation

1. **`GET /search` response shape** -- This is THE most critical gap. Every field extraction (status category key, priority as object, assignee as object, components as object array, description as ADF, sprint as array of sprint objects, epic via parent field) will be wrong without explicit documentation. An agent will assume flat string fields and produce broken deserialization code.

2. **POST body shapes for all write endpoints** -- The transition POST body (`{ "transition": { "id": "..." }, "fields": {...} }`), the issue update PUT body (`{ "fields": {...} }`), and the issue create POST body (`{ "fields": { "project": { "key": "..." }, ... } }`) all have specific wrapper structures. Without these, the agent will send malformed requests.

3. **Object-vs-primitive field values** -- Throughout the API, some fields are bare values (`"summary": "text"`, `"customfield_10016": 3.0`, `"labels": ["a","b"]`) and others are objects (`"priority": { "id": "2" }`, `"assignee": { "accountId": "..." }`, `"components": [{ "id": "..." }]`). This inconsistency is the #1 source of bugs in JIRA integrations. The doc must specify which fields use which format for both reads AND writes.

4. **Editmeta and createmeta response shapes** -- Both return field metadata in a specific structure (map vs paginated array) that determines how the agent builds its `EditableField` list. Without these, the deserialization will fail.

5. **Sprint field extraction** -- The sprint custom field is an array of sprint objects with a `state` field. The agent must find the active sprint. This is not obvious from the data model alone.

6. **Epic extraction logic** -- The doc has `EpicInfo { key, name }` but never explains how to extract it. The `parent` field approach (check if parent's issuetype is "Epic") is not mentioned.

7. **Comment field in transitions** -- The `"update"` key (vs `"fields"`) for comments on transitions is a well-known JIRA API gotcha that will cause a 400 error if the agent puts comments in `"fields"`.

8. **`allowedValues` ID-vs-name mapping** -- The `EditableField.allowed_values` stores `Vec<String>` but the write APIs need object IDs. The agent needs guidance on storing both display names and IDs, or the struct needs to be `Vec<(String, String)>` (id, name pairs).

### Recommended Reference Material

The following should be created as a separate reference file (e.g., `docs/design/jira-api-reference.md`) for agents to use during implementation:

1. **Atlassian REST API v3 docs**:
   - Search: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/#api-rest-api-3-search-get
   - Issue CRUD: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/#api-rest-api-3-issue-post
   - Transitions: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/#api-rest-api-3-issue-issueidorkey-transitions-post
   - Edit meta: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/#api-rest-api-3-issue-issueidorkey-editmeta-get
   - Createmeta: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/#api-rest-api-3-issue-createmeta-projectidorkey-issuetypes-get
   - Comments: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-comments/
   - Fields: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-fields/#api-rest-api-3-field-get

2. **ADF specification**: https://developer.atlassian.com/cloud/jira/platform/apis/document/structure/

3. **ureq v3 migration guide / API docs**: https://docs.rs/ureq/3/ureq/ -- The API changed significantly from v2 to v3 (builder pattern, error handling).

4. **A concrete JSON reference file** containing: one full `/search` response (with all field types populated), one `/transitions` response (with and without required fields), one `/editmeta` response, one createmeta response. These can be anonymized but must have realistic structure. This file would be the single most valuable addition -- agents can use it as a test fixture and deserialization reference.

### Summary

The design doc is strong on architecture, UI design, keybindings, error handling strategy, and threading model. It is weak on the specific JSON shapes that flow over the wire. The Rust data model structs (`JiraIssue`, `JiraTransition`, etc.) describe what the agent WANTS to have, but do not describe what the JIRA API actually RETURNS. The mapping from API response to Rust struct is where every bug will live, and that mapping is currently left for the agent to figure out through API documentation or trial-and-error.

**Bottom line**: An agent implementing this today would produce code that compiles and has correct architecture, but would fail at runtime on deserialization of nearly every endpoint response. Adding the response shapes documented in this assessment to the design doc (or a companion reference file) would bring the readiness score from 3/14 to 13/14 GREEN.
