# JIRA Plugin Design — Veteran Integration Review (v2)

**Reviewer**: Developer with 3+ production JIRA Cloud integrations shipped
**Date**: 2026-03-27
**Docs reviewed**:
- `jira-plugin.md` — full plugin design
- `jira-api-reference.md` — API JSON shapes (primary focus)
- `form-modal-spec.md` — creation/transition form UX
- `REVIEW-api-integration.md` — v1 review (previous reviewer's findings)

**Single question driving every verdict**: Will this break against a real JIRA Cloud instance?

---

## Context: What the v1 Review Caught (and Whether It Was Fixed)

The v1 reviewer found 10 issues. Before going issue-by-issue on new findings, I verified the status of those 10 items in the current docs.

| v1 Issue | Status in current docs |
|----------|------------------------|
| #1 Pagination using estimated `total` | **FIXED** — `jira-api-reference.md` now says `!page.is_empty() && page.len() >= max_results`, with explicit note that `total` is an estimate |
| #2 `reporter` filtered but not injected | **FIXED** — both docs now say to silently inject `reporter: { accountId }` always |
| #3 Sprint field might be scalar, not array | **NOT FIXED** — still documented as an array only; no `serde_json::Value` handling noted |
| #4 `Accept: application/json` missing from ureq snippet | **FIXED** — the code example now includes `.header("Accept", "application/json")` |
| #5 `Retry-After` format (seconds vs HTTP-date) | **PARTIALLY FIXED** — code now parses as `u64`, but no max-retry cap or scope clarification |
| #6 `orderedList` counter not shown | **NOT FIXED** — still says "number each" with no implementation hint |
| #7 Non-JSON error bodies (CDN HTML 503) | **NOT FIXED** — `unwrap_or_default()` on parse failure; no raw-body fallback |
| #8 `hierarchyLevel` may be absent in search response | **NOT FIXED** — documented as a bare integer, no `Option<i32>` |
| #9 ADF `mediaSingle`/`taskList`/`expand` unhandled | **NOT FIXED** — no mention in the `_` arm |
| #10 `comment` type in transition required fields | **NOT FIXED** — `TransitionField.is_comment` exists in the struct but how to detect it from the API response is not specified |

Four of the ten v1 issues were fixed. Six remain open.

---

## Area-by-Area Verdicts

### 1. Auth Flow — PASS

Basic auth with `email:api_token` base64-encoded is correct for JIRA Cloud API tokens. The `/myself` call for `accountId` is the right approach. Using explicit `accountId` in JQL (`assignee = '<accountId>'`) instead of `currentUser()` is correct — `currentUser()` is unreliable with API token auth in some configurations.

The ureq v3 code snippet is accurate: `ureq::Agent::new()`, `.call()?` for network errors only, explicit `response.status()` check. The `Accept` header is now present.

No blockers.

---

### 2. Search Response Deserialization — CONCERN

The JSON shape for `/search` is correct for the common case. Specific issues:

**a. Sprint field — FAIL (carried from v1 #3, not fixed)**

The sprint field `customfield_10020` is documented as always being an array:
```json
"customfield_10020": [{ "id": 37, "name": "Sprint 24", "state": "active", ... }]
```

On next-gen (team-managed) projects and some company-managed configurations, JIRA returns the sprint field as a single object, not an array. If the implementer deserializes this as `Vec<SprintObject>`, serde will return a deserialization error on the single-object variant. This is not a hypothetical edge case — next-gen projects are common in organizations that adopted JIRA Cloud after 2019.

**Fix required**: Deserialize sprint as `serde_json::Value`, then inspect with `Value::as_array()` / `Value::as_object()`, and normalise both into a `Vec` before extracting the active sprint.

**b. `hierarchyLevel` in parent.fields.issuetype — CONCERN (carried from v1 #8, not fixed)**

The extraction table checks `parent.fields.issuetype.hierarchyLevel == 1` for epic detection. In the abbreviated `parent` sub-object returned by `/search`, `hierarchyLevel` is frequently absent. A strict `== 1` check against a missing field returns `false`, so the fallback `name == "Epic"` fires — but that fallback fails on non-English JIRA instances ("Épico", "Epic リンク", etc.).

No panic; just silent `None` for epic on non-English instances. Document `hierarchyLevel` as `Option<i32>` and document the locale limitation.

**c. `null` vs absent fields — PASS**

The docs correctly note that `priority`, `assignee`, `reporter`, `description`, `sprint`, and `parent` can all be `null`. The field extraction table accounts for `None` on all of these. No issue.

**d. `components` as empty array — PASS**

`"components": []` is valid and the extraction logic (`[*].name`) degrades to an empty vec. Fine.

**e. Issues with no `fields` key — not addressed**

This is theoretical but real: malformed or restricted issues in JIRA can return an issue object with `fields: {}` (empty map). The per-field extraction will return `None` / empty for everything, which is safe as long as the Rust structs use `Option` for all non-key fields. The current `JiraIssue` struct does use `Option` appropriately for nullable fields. The `summary` field is `String` (not `Option<String>`) which would panic on a missing summary. Real JIRA issues always have a summary, so this is low risk but worth noting.

---

### 3. Transition Flow — CONCERN

**a. Overall structure — PASS**

The GET transitions → present picker → check required fields → POST with `fields` or `update.comment` flow is correct. The `fields`-vs-`update` asymmetry for comments is documented and the example bodies are accurate.

**b. `comment` field detection — CONCERN (carried from v1 #10, partially addressed)**

The `TransitionField` struct has `is_comment: bool`, noted as "true if field_id == 'comment'". This is correct — a transition required field with `field_id == "comment"` needs to go into the `update.comment` array, not `fields`. However, the `TransitionField` also has `field_type: FieldType` — and the docs don't say what `FieldType` to assign when `field_id == "comment"`. The schema for a comment field in the transitions API looks like:
```json
"comment": {
  "required": true,
  "schema": { "type": "comment", "system": "comment" },
  "name": "Comment",
  "operations": ["add"]
}
```
`schema.type == "comment"` does not map to any `FieldType` variant. If the implementer hits this in the `schema.type → FieldType` mapping table (which only covers `"string"`, `"number"`, `"priority"`, `"resolution"`, `"option"`, `"array"`, `"user"`, etc.), they will fall through to `Unsupported`. `Unsupported` fields show as read-only in the form. A required unsupported transition field shows an error message: "Required field X has unsupported type — create in JIRA web UI." That is wrong UX for a comment field.

**Fix**: Add an explicit rule: when `field_id == "comment"`, set `is_comment = true` regardless of `field_type`; open `$EDITOR` instead of showing it as `Unsupported`.

**c. `hasScreen: true` vs required fields — CONCERN (new)**

The transitions response includes `hasScreen: true/false`. The docs treat `fields: {}` (empty) as "no required fields" and non-empty `fields` as "required fields". This is correct for the simple case, but `hasScreen: true` with `fields: {}` (populated dynamically at POST time, not returned in GET) is a real pattern on some workflows — the screen exists but the fields are not enumerated in the expand. In this case the POST will succeed if no fields are required, but the implementer has no way to know ahead of time. The current approach will silently succeed or fail with a 400 if the screen has required fields that were not returned by `expand=transitions.fields`.

This is rare and works correctly on most standard workflows. Worth a note but not a blocker for initial implementation.

**d. Transition POST returns 204 — PASS**

Documented correctly. `204 No Content` with no body. The ureq code needs `response.into_body()` to be dropped (not read), or read and discarded. The docs don't show the 204 handling code explicitly, but the general error-handling pattern in the ureq snippet covers `200..=299` as success. Fine.

---

### 4. Issue Creation — PASS (with one concern)

**a. `createmeta` endpoint path — PASS**

Using `/rest/api/3/issue/createmeta/{projectKey}/issuetypes` (the v3 paginated path, not the older `/rest/api/3/issue/createmeta?projectKeys=...`) is correct.

**b. `"values"` wrapper — PASS**

Both endpoints correctly use `"values"` as the array key (not `"issueTypes"`). This is a common mistake and it's correctly documented.

**c. Field wrapping — PASS**

`project: { key }`, `issuetype: { id }`, `priority: { id }`, `assignee: { accountId }`, `components: [{ id }]`, `reporter: { accountId }`. All correct. The Common Field Value Formats table is the best part of this reference doc.

**d. `reporter` silently injected — PASS (v1 #2 fixed)**

Always inject `reporter: { accountId }` regardless of whether it appears as required in createmeta. Correct.

**e. `assignee` silently injected — CONCERN (new)**

The plugin always self-assigns new issues (`assignee: { accountId: user.accountId }`). The docs say "The issue is automatically assigned to the configured user." However, not all JIRA project configurations allow setting `assignee` at creation. On some project permission schemes, only project leads or admins can set `assignee`. The create call with `assignee` on such a project returns a 400 or silently ignores the assignee.

**This will not crash**, because the error path is handled. But the UX will be confusing — the user thinks they created an assigned issue, but JIRA may have created it unassigned. Since this is a personal tool and the user is presumably a member of their own projects, probability is low. Still worth a comment in the code.

**f. Description excluded — PASS**

Correct design decision. ADF round-tripping from plain text is lossy. Excluding description from creation avoids destroying existing rich-text on subsequent edits.

---

### 5. ADF Handling — CONCERN

**a. Common node types — PASS**

`paragraph`, `heading`, `bulletList`, `orderedList`, `listItem`, `codeBlock`, `blockquote`, `text`, `hardBreak`, `mention`, `inlineCard`, `emoji`, `rule`, `table` are all documented. The `_` fallback recurse-into-content pattern prevents panics on unknown types.

**b. `orderedList` counter not threaded through recursion — FAIL (carried from v1 #6, not fixed)**

The pseudocode says:
```
"orderedList" => join children with "\n"  (number each)
```
"Number each" requires a mutable counter. A pure recursive `adf_to_text(node)` function has no access to sibling index. The pseudocode does not show how to thread the counter. An implementer who writes a clean recursive function will produce:
```
- Item one
- Item two
- Item three
```
instead of:
```
1. Item one
2. Item two
3. Item three
```
This is not a correctness blocker (ordered lists still display as lists, just unnumbered), but it is a spec defect that will produce wrong output for any issue or comment containing a numbered list. In technical docs (bug reports, acceptance criteria), numbered lists are common.

**Fix**: Add an explicit note that `orderedList` must pass an index to each `listItem` child, or show a non-recursive iteration pattern.

**c. `mediaSingle` / `taskList` / `expand` — CONCERN (carried from v1 #9, not fixed)**

These node types are unmentioned. The `_` fallback recurse-into-content handles them without panic. The practical impact:
- `mediaSingle` wrapping a `media` node: `media` has no `content` array, only `attrs`. The `_` fallback recurses into a missing content array, producing an empty string. Images are silently dropped.
- `taskList` / `taskItem`: recurse into content, text extracted, but no checkbox indicator. Renders as a plain list.
- `expand`: recurse into content, loses the expand title (stored in `attrs.title`, not `content`).

For a TUI this is acceptable. However the spec should explicitly say "the following nodes are handled via the catch-all (text extracted, structure lost): `mediaSingle`, `media`, `expand`, `nestedExpand`, `panel`, `taskList`, `taskItem`." This prevents implementers from adding incorrect special-case code that panics on missing fields.

**d. `link` mark — PASS**

Correctly documented: extract `attrs.href`, append `(url)` to the text. Good.

**e. `text` node `marks` field absent — PASS**

Implied optional. Fine.

**f. Multi-line text in `text_to_adf` — CONCERN (new)**

The `text_to_adf` function for comments splits on `\n\n` to create multiple paragraphs, but individual lines within a paragraph are not handled — a paragraph with embedded `\n` (single newline, not double) will produce a single `text` node with a literal `\n` in it. ADF does not allow literal newlines in text nodes; they must be `hardBreak` nodes. If a user pastes multi-line content from their editor without a blank line between lines, those newlines will be sent as literal `\n` in a `text` node. JIRA's API will accept this (it won't 400), but the comment will render with the newlines collapsed in the web UI.

Not a 400 error, but a rendering defect for any comment with single-newline line breaks (common in editors that use soft wrapping).

---

### 6. Custom Field Discovery — PASS

Story points: `custom == true && name.to_lowercase().contains("story point") && schema.custom.contains("float")`. Sprint: `schema.custom.contains("gh-sprint")`. Both are correct and documented. The `schema` field is `Option<FieldSchema>` (handled). The fallback to config-provided field IDs is correct.

One minor issue not blocking: if a JIRA instance uses a third-party story points app (e.g., Zenhub, Linear-style fields), the story points field will have a different schema string and will not be discovered. The user must set `story_points_field` manually in config. This is the correct design — just document that auto-discovery covers Atlassian's built-in and Agile Board story points fields.

---

### 7. Pagination — PASS

v1 #1 was fixed. The current termination condition (`!page.is_empty() && page.len() >= max_results`) is correct and matches the search-specific note. The `total` field is correctly noted as an estimate. Createmeta is correctly marked as paginated. `/field` is correctly marked as non-paginated. Comments are correctly handled.

No issues.

---

### 8. Rate Limiting — CONCERN

The ureq code example correctly reads `Retry-After` as `u64` seconds (v1 #5 partially fixed). However:

**a. No maximum retry count**

If JIRA keeps returning 429 (e.g., a rogue auto-refresh loop or a poorly-configured rate limit), the design has no cap on retries. A background thread that retries indefinitely on 429 will hold the channel open, consume resources, and never surface a "JIRA is unavailable" state to the user.

**Fix**: Retry at most 3 times per command. After 3 failures, send `JiraResult::Error` to the TUI thread and let the user take action.

**b. Retry scope not bounded to auto-refresh**

The design says rate-limit retry should show a toast and retry. But user-initiated actions (transition, create, edit) should not silently retry — the user needs to know the action is pending. The error handling table distinguishes user-initiated vs auto-refresh, but the rate-limit retry logic is not tied to that distinction.

**Fix**: For user-initiated actions, retry once (with the toast), then show a blocking error if still rate-limited. For auto-refresh, retry per the documented flow.

---

### 9. Error Responses — CONCERN

**a. Non-JSON error bodies — CONCERN (carried from v1 #7, not fixed)**

The error-handling code uses:
```rust
let err: JiraErrorResponse = response.into_body().read_json()
    .unwrap_or_default();
```
`unwrap_or_default()` on a `JiraErrorResponse` with `#[serde(default)]` on all fields produces an empty struct with no messages. So a 503 HTML page from Cloudflare (which is a real occurrence when JIRA's CDN is having issues) silently produces an empty error: the user sees "JIRA error (503)" with no message. That's better than a panic, but it's confusing.

**Fix**: On parse failure, read the body as raw bytes/string and include the first 200 chars in the error message.

**b. 5xx status codes not in the documented list**

The error table lists 400, 401, 403, 404, 429. It does not list 500 or 503. The `status => { ... }` catch-all in the ureq snippet handles them at the code level, but the design doc doesn't tell the implementer what UX to show for a 500. Should it be treated like a network error (non-blocking for auto-refresh, blocking for user actions)? This needs one line of clarification.

**c. 401 vs 403 semantics — PASS**

401 = bad credentials; 403 = authenticated but no permission. Both are documented. The error messages from JIRA's body are surfaced, which is sufficient.

---

### 10. Edge Cases

**a. Empty projects (no assigned issues) — PASS**

`/search` returns `{ "issues": [] }`. The pagination loop exits immediately on the first empty page. The kanban board renders with no cards. This is handled.

**b. Zero assigned issues after filtering — PASS**

If the project filter removes all issues from a non-empty result set, the board shows empty columns. Fine.

**c. Issues with all-null optional fields — PASS**

`JiraIssue` uses `Option` for priority, assignee, reporter, description, sprint, epic, story_points. All null-returning fields map to `None`. No crash.

**d. Sprint array with no active sprint — PASS**

Documented: find `"state": "active"`, fall back to `"future"`, then most recent `"closed"`. If none found, `None`. Correct.

**e. Issues without epics — PASS**

`parent` absent or `parent` pointing to a non-Epic → `epic: None`. Handled.

**f. Issues with sub-tasks as `parent` — CONCERN**

A sub-task has its parent Story in the `parent` field, not an epic. The `hierarchyLevel` check (`== 1`) should prevent sub-tasks from being treated as epics. But if `hierarchyLevel` is absent (v1 #8, still unfixed), the fallback `name == "Epic"` fires — and a Story parent named "Epic" (unlikely but possible with custom issue type names) would be misclassified.

Since `hierarchyLevel` is not `Option<i32>` in the current spec, an absent `hierarchyLevel` will cause a deserialization issue unless the struct handles it. This is the same root issue as v1 #8.

**g. JQL special characters in `accountId` — PASS**

JIRA `accountId` values are alphanumeric with no special JQL characters. The current quoting (`assignee = '<accountId>'`) is correct. No injection risk.

**h. `assignee` filter returning issues from JIRA Service Management — CONCERN (new)**

If the user works on JIRA Service Management (JSM) projects, the `/search` query with `assignee = '<accountId>'` will return JSM tickets. JSM issues have different field schemas — in particular, `priority` may be absent and `status` may have custom categories not in the standard set. The `StatusCategory` enum uses `#[serde(other)]` fallback mapping to `ToDo`, which handles unknown categories safely. The `priority: Option<String>` handles missing priority. These are fine.

However, JSM issues may have a `requestType` field and different `issuetype` names. The board will display them alongside software issues, which may be unexpected. This is a UX concern, not a crash.

**i. Issues returned with `key: null` — not addressed**

Extremely rare but possible during JIRA migrations. `JiraIssue.key` is `String` (not `Option`), so a null key would cause a deserialization error. In practice JIRA never returns null keys on stable instances. Acceptable risk.

---

## Issues That Will Cause a 400/500 Error on a Real JIRA Instance

| # | Scenario | Root Cause | Severity |
|---|----------|------------|----------|
| A | Sprint field on next-gen project | Sprint returned as object, not array; deserialization error | HIGH — silent failure or panic on many modern JIRA orgs |
| B | Comment-type transition required field | Classified as `Unsupported`, shows error instead of opening editor | MEDIUM — wrong UX but no API error; the user can't complete the transition from the TUI |
| C | Ordered lists in ADF render with `- ` prefix | Counter not threaded; every item says `- N` or just `- ` | LOW — cosmetic/display defect, no API error |
| D | 503 CDN error surfaces no message | `unwrap_or_default()` on non-JSON body | LOW — confusing but not a crash |

No issues that will cause a 400 POST error from the API itself (the v1 reporter and pagination fixes resolved those). The remaining HIGH issue (sprint field) causes a deserialization error on the client side, not a server 400.

---

## Final Verdict

**REJECT — two targeted fixes required before implementation**

The v1 reviewer's most critical corrections (pagination, reporter injection, Accept header) were incorporated. The spec is now substantially correct. However two issues remain that will cause real failures against a non-trivial JIRA Cloud instance:

**1. Sprint field deserialization (HIGH)**
Documented as always-array; real JIRA Cloud on next-gen projects returns a scalar object. This will produce a serde deserialization error and the issue list will fail to load for any user who has next-gen project issues. Next-gen is the default for new JIRA Cloud projects since 2019.

Fix in `jira-api-reference.md`: change the sprint extraction note to deserialize as `serde_json::Value` and handle both array and object variants.

**2. `orderedList` counter threading (MEDIUM-spec-defect)**
The pseudocode says "number each" but provides no mechanism for tracking the index in a recursive descent. Every implementer who writes a clean recursive `adf_to_text` will produce unordered output for ordered lists. This is a spec defect that guarantees wrong output.

Fix in `jira-api-reference.md`: replace "number each" with a concrete implementation note (either use `enumerate()` in a non-recursive list-child loop, or pass a `&mut usize` counter).

**Lower-priority items to fix before coding (won't cause API errors but will cause runtime confusion):**

3. `hierarchyLevel` should be `Option<i32>` — field may be absent in search response parent sub-object
4. ADF: document the catch-all arm explicitly (`mediaSingle`, `media`, `panel`, `expand`, `taskList`, `taskItem`, `nestedExpand`)
5. Rate limiting: add max retry count (3), and distinguish user-initiated vs auto-refresh retry behavior
6. Non-JSON error bodies: fall back to raw body text instead of empty error message

**Once items 1 and 2 are fixed, this spec is implementable against a real JIRA Cloud instance. All the hard parts (field wrapping rules, transitions fields/update asymmetry, reporter injection, pagination termination, custom field discovery) are correct.**
