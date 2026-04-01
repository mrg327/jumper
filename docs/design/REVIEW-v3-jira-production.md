# JIRA Plugin Design — Production Integration Review (v3)

**Reviewer**: Production JIRA Cloud integration engineer. Zero bias toward approval.
**Date**: 2026-03-27
**Docs reviewed**:
- `jira-plugin.md` — full plugin design (primary)
- `jira-api-reference.md` — API JSON shapes
- `form-modal-spec.md` — form modal UX
- `REVIEW-v2-jira-veteran.md` — previous API review (v2)
- `REVIEW-v2-concurrency.md` — previous concurrency review (v2)

**Single question driving every verdict**: Will this break against a real JIRA Cloud instance?

---

## Verification of v2 Issues

### v2-veteran findings (6 open items after v1 fixes)

| v2 Issue | Severity | Status in current docs |
|----------|----------|------------------------|
| A — Sprint scalar/array deserialization | HIGH | **FIXED** — `jira-api-reference.md` now documents `extract_sprint_name(value: &serde_json::Value)` handling both array and object cases |
| B — Comment transition field mapped to Unsupported | MEDIUM | **FIXED** — `is_comment: bool` in `TransitionField`; `jira-api-reference.md` Data Model section says `is_comment: true` when `field_id == "comment"` |
| C — orderedList counter not threaded | LOW | **FIXED** — `jira-api-reference.md` ADF section now shows `.enumerate()` loop with explicit index |
| D — Non-JSON error bodies surface empty message | LOW | **NOT FIXED** — `unwrap_or_default()` is still the only error-body parsing strategy; no raw-body fallback documented |
| E — hierarchyLevel as Option<i32> | LOW | **NOT FIXED** — field extraction table still shows bare `== 1` check with no `Option` handling |
| F — mediaSingle/taskList/expand catch-all undocumented | LOW | **NOT FIXED** — no explicit enumeration of nodes handled by the `_` catch-all arm |

### v2-concurrency findings (4 critical issues: C1–C4)

| v2 Issue | Severity | Status in current docs |
|----------|----------|------------------------|
| C1 — refreshing never clears on generation mismatch | CRITICAL | **FIXED** — `jira-plugin.md` Refresh Behavior now says "discard the data BUT still clear `refreshing = false`" |
| C2 — Optimistic state clobbered by auto-refresh during write latency | CRITICAL | **FIXED** — `jira-plugin.md` Transition section now says "Set `refreshing = true` immediately when sending any write command (TransitionIssue, UpdateField, CreateIssue, AddComment) — not just when sending FetchMyIssues" |
| C3 — 500ms delay mechanism unspecified | CRITICAL | **FIXED** — `jira-plugin.md` Refresh Behavior now explicitly says "MUST be implemented as a TUI-side timer checked in `on_tick()`" with `pending_refresh_at: Option<Instant>` example |
| C4 — Rapid open/close produces spurious Disconnected error | HIGH | **FIXED** — `jira-plugin.md` Background Thread Specifics now gives a three-step on_enter sequence: (1) if shutdown_flag set, join the old thread; (2) if is_finished, clean up handle; (3) create fresh channels and spawn |

All four concurrency issues are resolved. Three of the six v2-veteran issues are resolved. Three low-severity items remain open (D, E, F).

---

## New Area-by-Area Verdicts

### 1. Authentication — PASS

Basic auth (`base64(email:api_token)`), `Accept: application/json`, ureq v3 constructor, explicit `response.status()` check for 4xx/5xx — all correct.

`/myself` for `accountId` with JQL `assignee = '<accountId>'` (not `currentUser()`) is the correct and robust approach.

No blockers.

---

### 2. Search Response Shape — PASS (one residual concern)

The `/search` response shape is accurate. All field extraction rules are correct:
- `priority.name` — extracted from nested object, null-safe
- `assignee.displayName` — extracted from nested object, null-safe
- `status.statusCategory.key` — extracted correctly; `"new"`/`"undefined"` → ToDo, `"indeterminate"` → InProgress, `"done"` → Done
- `components[*].name` — array extraction, degrades to empty vec
- `parent` → epic detection via `hierarchyLevel == 1` OR `name == "Epic"` fallback
- Sprint — `serde_json::Value` with both array and object variants handled

**Residual concern (low): `hierarchyLevel` absent in parent sub-object (v2-E, still open)**

The parent sub-object returned by `/search` frequently omits `hierarchyLevel`. The spec still shows a bare `== 1` check with no `Option` handling. Since the fallback `name == "Epic"` fires when the first check fails, this does not panic — it silently returns `None` for the epic on non-English JIRA instances where `issuetype.name` is not `"Epic"`. This affects localized JIRA instances only. The code will not crash; the `epic` field will be `None` when it should have a value.

**Action**: Deserialize `hierarchyLevel` as `Option<i32>`. Document that non-English instances will show no epic link.

---

### 3. Transitions Flow — PASS (one residual concern)

**POST body structure — PASS**

The `fields` vs `update.comment` asymmetry is documented and the example bodies are correct:
- Regular required fields → `"fields": { "resolution": { "id": "1" } }`
- Comment-required transition → `"update": { "comment": [{ "add": { "body": <ADF> } }] }`

**Comment field detection — PASS (v2-B fixed)**

`is_comment = true` when `field_id == "comment"`, which triggers `$EDITOR` flow instead of the Unsupported path.

**Residual concern (medium): `hasScreen: true` with empty `fields` object**

Documented in v2 as a corner case and still unresolved. A transition with `hasScreen: true` and `fields: {}` (the screen fields are not enumerated in the GET response) will be treated as requiring no fields and the POST will proceed. If the screen has required fields that JIRA does not enumerate in `expand=transitions.fields`, the POST returns a 400. The error modal will show correctly, but the user cannot complete the transition from the TUI.

This is documented in v2 as "rare and works correctly on most standard workflows" and is acceptable for an initial implementation. It is not a new defect.

**`204 No Content` handling — PASS**

Success check is `200..=299` range; `204` falls within that. No body to read. The ureq code correctly handles this with the `200..=299` arm.

---

### 4. Field Updates (PUT) — PASS

`PUT /rest/api/3/issue/{key}?notifyUsers=false` with `{ "fields": { ... } }` wrapper is correct.

Field write formats match the Common Field Value Formats table:
- `summary` → bare string
- `priority` → `{ "id": "2" }`
- `customfield_10016` (story points) → bare number
- `labels` → full string array (replace, not diff)
- `components` → `[{ "id": "..." }]`

`204 No Content` on success is correct.

**One implementation trap not in the spec**: labels field behaves as full replacement. A user editing labels in the form must see the current labels pre-populated, or they will unknowingly delete existing labels. The form modal spec uses `Option<FieldValue>` initialized from the current issue's labels — this is the right approach if implemented correctly, but the spec does not say "pre-populate form fields from the current issue data". This is an implementer assumption, not a spec defect. Worth a comment in the code.

---

### 5. Comments — PASS

**GET `/rest/api/3/issue/{key}/comment`**
- `orderBy=-created` for newest-first: correct.
- Paginated with standard pattern: correct.
- `body` is ADF, converted via `adf_to_text()`: correct.
- `author.displayName` extraction: correct.

**POST `/rest/api/3/issue/{key}/comment`**

ADF body with `version: 1`, `type: "doc"`, `content: [paragraph, ...]` is correct. The `text_to_adf()` for multi-paragraph content splits on `\n\n` — correct. Single-newline handling (v2-veteran multi-line concern) produces text nodes with `\n` literals, which JIRA's API accepts without a 400 but renders collapsed in the web UI. This is a display cosmetic defect, not an API error. Acceptable for a TUI tool.

**Response**: `201 Created` with the comment object. The spec correctly handles this.

---

### 6. Issue Creation — PASS

**createmeta path**: `/rest/api/3/issue/createmeta/{projectKey}/issuetypes` — correct v3 path.

**`"values"` wrapper**: correct (NOT `"issueTypes"`). Both endpoints use `"values"`.

**Required fields filter**: `project`, `issuetype`, `reporter` are filtered from the user form and injected automatically. Correct.

**POST body field formats**: All correct per the Common Field Value Formats table.

**`reporter` silently injected**: always inject `reporter: { accountId }` regardless of whether it appears in createmeta. Correct and necessary for business/service-desk project types.

**`assignee` silently injected**: as noted in v2, some project permission schemes will ignore or 400 on `assignee` at create time. The error path handles this. No change needed.

**Response**: `201 Created` with `{ "id": "...", "key": "...", "self": "..." }`. Extract `.key` for toast. Correct.

---

### 7. ADF Handling — PASS (two residual concerns)

**Core algorithm — PASS**

All major node types covered: `paragraph`, `heading`, `bulletList`, `orderedList` (now with `.enumerate()` per v2-C fix), `listItem`, `codeBlock`, `blockquote`, `text`, `hardBreak`, `mention`, `inlineCard`, `emoji`, `rule`, `table`.

Null-safe attribute access via `.get("attrs").and_then(...)` is documented. The `_` fallback recurses into `content` if present.

**`text_to_adf` — PASS**

For write operations (comments, transition comments): wraps user text in ADF paragraph nodes. Multi-paragraph via `\n\n` split. This produces valid ADF that JIRA Cloud will accept.

**Residual concern (low): catch-all arm not enumerated (v2-F, still open)**

Nodes handled silently by `_` fallback are not documented. `mediaSingle`, `media`, `taskList`, `taskItem`, `expand`, `panel`, `nestedExpand` will produce empty string output for `media` (which has no `content` array) and partial text output for the others. An implementer adding a new `match` arm for these node types to "fix" them may accidentally introduce panics by using `node["attrs"]["key"]` instead of the null-safe `.get()` chain.

**Action**: Add one sentence in the `_` arm pseudocode: "The following node types are known to reach this arm and are intentionally handled by falling back to content recursion: `mediaSingle`, `media`, `panel`, `expand`, `nestedExpand`, `taskList`, `taskItem`. Do not add special-case arms for these — they will produce either empty string (`media`) or plain-text fallback without structure, which is acceptable for TUI display."

---

### 8. Custom Field Discovery — PASS

Story points: `custom == true && name.to_lowercase().contains("story point") && schema.custom.contains("float")` — correct. Disambiguates from similarly-named fields.

Sprint: `schema.custom.contains("gh-sprint")` — correct and more reliable than name matching.

Config override: user can specify `story_points_field` and `sprint_field` explicitly, bypassing discovery. Correct fallback for non-standard field setups (Zenhub, etc.).

`schema` is `Option<FieldSchema>` in the `JiraFieldDef` struct — null-safe if schema is absent on some custom field types.

No issues.

---

### 9. Pagination — PASS

Termination condition: `!page.is_empty() && page.len() >= max_results`. Correctly avoids using estimated `total`.

All paginated endpoints identified: `/search`, `/issue/createmeta/{key}/issuetypes/{id}`, `/issue/{key}/comment`.

`/rest/api/3/field` is correctly identified as non-paginated (flat array response).

No issues.

---

### 10. Rate Limiting — CONCERN (partially open from v2)

`Retry-After` header parsed as `u64` seconds: correct.

**Open from v2: no maximum retry count**

The spec still has no cap on retry attempts. A pathological 429 loop (e.g., auto-refresh firing repeatedly against a heavily throttled tenant) will keep the background thread in a spin state consuming resources and never surfacing a terminal error. The spec says "retry after Retry-After header delay" with no exit condition.

**Severity**: LOW in practice — real JIRA Cloud rate limits are per-minute with resets, not persistent. But the spec is incomplete.

**Required fix**: Add "Retry at most 3 times per command. On the fourth 429, send `JiraResult::Error` to the TUI thread."

**Open from v2: retry scope not distinguished for user actions vs auto-refresh**

User-initiated write commands (transition, create, edit) should not silently retry — the user needs to know the action is delayed. Auto-refresh can silently retry. The spec does not make this distinction in the rate-limit retry logic.

**Required fix**: "For user-initiated commands, retry once with a toast, then send `JiraResult::Error` on a second 429. For `FetchMyIssues`, use the full 3-retry policy."

---

### 11. Error Responses — CONCERN (one open)

**400/401/403/404 — PASS**

Field-level errors from `{ "errorMessages": [], "errors": { "field": "msg" } }` are correctly deserialized. `#[serde(default)]` on both fields handles absent keys. The `JiraError.display()` method combines both maps into a readable string.

**429 — PASS** (with rate-limit retry concerns noted above)

**5xx — CONCERN (open from v2)**

The error handling table documents 400, 401, 403, 404, 429 but not 500 or 503. The spec says these fall through to the `status => { ... }` catch-all and produce a `JiraResult::Error`. The UX should be identical to network errors (non-blocking toast for auto-refresh, blocking modal for user actions). This is implied but not stated.

**Residual concern (low): Non-JSON error bodies (v2-D, still open)**

`unwrap_or_default()` on a failed JSON parse produces an empty `JiraErrorResponse`. A 503 from Cloudflare CDN (HTML body) shows the user only the HTTP status with no message. The spec still has no raw-body fallback.

**Required fix**: On `read_json()` parse failure, read the body as a `String` (with a byte limit, e.g., 500 bytes) and use it as the error message. This gives the user actionable information ("Service Unavailable — Cloudflare...") instead of a blank error.

---

### 12. Concurrency Model — PASS (all v2-C issues resolved)

All four critical concurrency issues from v2 are fixed:

- **C1 (refreshing deadlock)**: Stale-generation Issues results now always clear `refreshing`. Auto-refresh unblocked.
- **C2 (optimistic state clobbered)**: `refreshing = true` set immediately on any write command send, not just on FetchMyIssues send. Auto-refresh suppressed during write latency.
- **C3 (500ms delay mechanism)**: TUI-side `pending_refresh_at: Option<Instant>` timer in `on_tick()` is now specified. Background thread sleep is explicitly prohibited.
- **C4 (rapid open/close Disconnected error)**: `on_enter()` now joins the old thread before spawning a new one when `shutdown_flag` is set. Channel disconnection race eliminated.

The channel topology (command: TUI→thread, result: thread→TUI), single `ureq::Agent` per thread lifetime, unbounded `mpsc` channel, and `try_recv()` drain loop are all sound.

---

### 13. Form Modal — PASS

State machine (`Navigating`, `EditingText`, `SelectOpen`, `MultiSelectOpen`, `Submitting`, `ValidationError`) is complete and transitions are fully specified.

`FieldValue` enum separates UI state (in `FormState`) from data state (in `Vec<(EditableField, Option<FieldValue>)>`). This is the correct design — `FormState` is cheap to clone or replace without losing user-entered data.

Terminal cursor positioning (`frame.set_cursor_position()`) during `EditingText` is documented.

Pre-submission validation (check all required fields before sending) is correct. On API error, fields are preserved and re-presented with error markers.

`S` to submit for creation, `Enter` to submit for transition required fields: distinction is documented.

No issues.

---

### 14. Edge Cases — PASS

All edge cases from v2 are handled:

| Scenario | Verdict |
|----------|---------|
| Zero assigned issues | PASS — `/search` returns empty array, pagination exits, board shows empty |
| All optional fields null | PASS — `JiraIssue` uses `Option` throughout for nullable fields |
| Sprint array with no active sprint | PASS — fall-through to `"future"`, then most recent, then `None` |
| Issues without epics | PASS — `parent` absent or non-Epic parent → `epic: None` |
| Sub-task with Story as parent | PASS — `hierarchyLevel` check (or Epic name fallback) prevents misclassification; low-risk |
| JQL special characters in accountId | PASS — alphanumeric, no injection risk |
| JSM issues in search results | PASS — no crash; display-only UX concern |
| `summary: null` on malformed issue | LOW RISK — `JiraIssue.summary: String` (not Option); real JIRA never returns null summary |

---

## Summary: What v2 Left Open and Current Status

| Item | v2 Severity | v3 Status |
|------|-------------|-----------|
| Sprint scalar vs array | HIGH | **FIXED** |
| Comment field in transition → Unsupported | MEDIUM | **FIXED** |
| orderedList counter not threaded | LOW | **FIXED** |
| C1 — refreshing deadlock on generation mismatch | CRITICAL | **FIXED** |
| C2 — optimistic state clobbered by auto-refresh | CRITICAL | **FIXED** |
| C3 — 500ms delay mechanism unspecified | CRITICAL | **FIXED** |
| C4 — rapid open/close Disconnected error | HIGH | **FIXED** |
| Rate limiting: no max retry count | LOW | **OPEN** |
| Rate limiting: no user-action vs auto-refresh distinction | LOW | **OPEN** |
| Non-JSON error bodies (5xx from CDN) | LOW | **OPEN** |
| `hierarchyLevel` as Option<i32> | LOW | **OPEN** |
| Catch-all ADF nodes not enumerated | LOW | **OPEN** |

---

## New Issues Not in v2

### N1 — `text_to_adf` single-newline collapse (cosmetic, no API error)

From v2-veteran but carried as low: a paragraph written in `$EDITOR` with single-newline line breaks will have those newlines embedded in the ADF `text` node. JIRA Cloud accepts this without a 400, but renders the newlines collapsed in the web UI. No API failure, purely cosmetic. Does not require a spec fix but worth a code comment.

### N2 — Labels write is full replacement, form must pre-populate

`PUT /rest/api/3/issue/{key}` with `labels` sends the complete array (no diff). If the form opens with an empty labels field for a labeled issue, saving will delete all existing labels. The spec does not say "pre-populate form fields from the current issue data when editing". The form modal spec's `Option<FieldValue>` starts as `None` until the user edits — this means editing a labeled issue would clear its labels if the user does not re-enter them.

**Severity**: MEDIUM for field edits in the detail modal (UpdateField path), LOW for issue creation (no existing labels to lose).

**Required fix**: In the detail modal, when the user presses `e` to edit a field, the `EditableField` form must pre-populate the current value from `JiraIssue`. For labels specifically, pre-populate with the current comma-separated list. The form must start in `Navigating` state with the existing value, not in an empty state. The spec describes the form but does not say how it initializes field values when launched from the detail view.

### N3 — `TransitionField.field_type` not specified for comment field type

The `TransitionField` struct has both `field_type: FieldType` and `is_comment: bool`. When `field_id == "comment"`, what should `field_type` be? The API returns `schema.type == "comment"` which maps to `Unsupported` via the existing mapping table. But `is_comment = true` overrides the display behavior (opens `$EDITOR` instead of Unsupported).

This is internally consistent, but an implementer writing `render_field_row()` for the transition form may check `field_type` before `is_comment`, rendering a "Required field X has unsupported type" error instead of opening the editor.

**Required fix**: Document explicitly: "When `is_comment` is true, ignore `field_type` for rendering and input purposes. Always open `$EDITOR` for comment fields regardless of `field_type`."

---

## Issues That Will Cause Failures on a Real JIRA Cloud Instance

| # | Scenario | Root Cause | Severity |
|---|----------|------------|----------|
| N2 | Editing labels in detail modal clears all existing labels | Form pre-population not specified | MEDIUM — data loss for the user's label data |
| Rate limit | Infinite retry loop on persistent 429 | No max retry count in spec | LOW — background thread resource leak, user never sees terminal error |
| Non-JSON body | CDN 503 surfaces empty error message | `unwrap_or_default()` on parse failure | LOW — confusing UX, no data loss |

No issues that will cause a server-side 400 error on the JIRA API itself. The field format table, reporter injection, pagination termination, and ADF handling are all correct.

---

## Final Verdict

**CONDITIONAL APPROVE — two targeted fixes before implementation**

The bulk of the spec is correct and production-ready. All critical concurrency issues and the HIGH sprint-field deserialization issue from v2 are resolved. The existing API surface (auth, search, transitions, editmeta, createmeta, create, comment POST) will work against a real JIRA Cloud instance with no structural API errors.

Two issues need spec fixes before the implementer starts:

**1. Labels (and multi-value fields) pre-population in the detail edit form (MEDIUM — N2)**

The spec does not specify that launching a field editor from the detail modal pre-populates with the current field value. Without this, editing any field in the detail view will clear existing values on save. For labels this is silent data loss. Fix: add one paragraph to `jira-plugin.md` and `form-modal-spec.md` stating that `Vec<(EditableField, Option<FieldValue>)>` is initialized from the current `JiraIssue` data when editing existing fields from the detail view.

**2. `is_comment` render override in transition form (MEDIUM — N3)**

`TransitionField.is_comment` overrides `field_type` for rendering, but this is not stated. An implementer checking `field_type` first will display a broken "unsupported type" error for comment-required transitions. Fix: one sentence in `jira-plugin.md` under the TransitionField struct: "When `is_comment` is true, always use the `$EDITOR` flow regardless of `field_type`."

**Lower-priority items (fix before shipping, not before coding):**

3. Rate limiting: add max 3 retries, distinguish user-action vs auto-refresh retry
4. Non-JSON error bodies: raw-body fallback on JSON parse failure
5. `hierarchyLevel` as `Option<i32>` with note on localized instances
6. Catch-all ADF arm: enumerate known-fallback node types explicitly

**The spec is complete enough to begin implementation with fixes 1 and 2 in hand. All the hard parts — auth, field write formats, transitions fields/update asymmetry, reporter injection, pagination termination, sprint scalar/array handling, custom field discovery, optimistic UI race protection, 500ms delay timer mechanism — are correct and production-grade.**
