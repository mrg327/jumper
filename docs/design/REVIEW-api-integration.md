# API Integration Review: JIRA Plugin

**Reviewer perspective**: Senior API integration engineer, JIRA Cloud v3 specialist
**Date**: 2026-03-27
**Docs reviewed**:
- `jira-plugin.md` — Plugin design
- `jira-api-reference.md` — API JSON shapes
- `form-modal-spec.md` — Creation flow

---

## Assessment

### 1. Endpoint Completeness — PASS

All endpoints needed for the described feature set are listed in both `jira-plugin.md` and `jira-api-reference.md`. URLs, methods, and purpose are correctly documented. No obviously missing endpoints. The `GET /rest/api/3/status` endpoint is correctly flagged as optional with a sound rationale (derive columns from issue data instead).

One minor gap: `GET /rest/api/3/issue/{key}/editmeta` is marked deprecated in JIRA Cloud in favour of `GET /rest/api/3/issue/{key}?expand=editmeta`, but the old path still responds correctly on current Cloud instances as of early 2026. Not a runtime blocker, but worth noting as a future-proofing concern.

### 2. JSON Accuracy — CONCERN

Most shapes are accurate. The following specific issues were found:

**a. Sprint field value format — CONCERN**

The API reference documents `customfield_10020` as:
```json
"customfield_10020": [{ "id": 37, "name": "Sprint 24", "state": "active", ... }]
```
This is correct for the Greenhopper sprint field on most Cloud instances. However, on some Cloud configurations (especially Company-managed projects with next-gen boards) the sprint field can be absent entirely or return a single object instead of an array. The current documentation assumes an array; no fallback for a scalar value is documented. If an implementer hits a scalar sprint value and tries to iterate it as an array, a runtime deserialization panic is the result.

**Recommendation**: Deserialize sprint as `serde_json::Value`, inspect whether it is an array or object, handle both, and treat anything else as absent.

**b. `parent` field used as epic proxy — CONCERN**

The docs use `.fields.parent` to detect epics, checking `hierarchyLevel == 1`. This is brittle in two ways:

- In JIRA's next-gen (team-managed) projects, the hierarchy is flatter and levels differ from classic projects. `hierarchyLevel` for Epic is `1` in classic but can be `0` or `2` in next-gen. The fallback `name == "Epic"` is safer but not reliable either (custom issue type names).
- Before Jira Cloud 2022, the epic link was a separate custom field (`customfield_10014`), not encoded in `parent`. If the target instance is on an older schema, `parent` may be present but point to a parent Story (sub-task relationship), not an Epic. The `hierarchyLevel` check partially mitigates this, but the fallback to `name == "Epic"` will silently fail on non-English JIRA locales where the issue type is called "Épico", "叙事", etc.

**Recommendation**: Accept the current approach for an initial implementation but treat `EpicInfo` extraction as best-effort and document the limitations.

**c. `statusCategory.key` values — PASS (but note one omission)**

The docs correctly document `"new"`, `"indeterminate"`, `"done"`, and `"undefined"`. The `StatusCategory` enum maps `undefined` → `ToDo` with `#[serde(other)]` fallback. This is correct and complete.

**d. `issuetype.hierarchyLevel` in search response — CONCERN**

The `fields` extraction table documents `parent.fields.issuetype.hierarchyLevel` for epic detection, but the `issuetype` object nested inside `parent.fields` in the search response does NOT always include `hierarchyLevel`. The full `GET /rest/api/3/issue/{key}` response includes it, but inside the abbreviated `parent` sub-object returned by `/search`, many Cloud instances omit it. The `name == "Epic"` fallback partially saves this, but implementers need to use `Option<i32>` for `hierarchyLevel` and not assume its presence.

**e. `customfield_10016` story points — PASS**

Documented as bare `f64`. Correct. The float schema match for disambiguation is a good approach.

### 3. Authentication — PASS

Basic Auth with `email:api_token` encoded as base64 is correct for JIRA Cloud. The Rust snippet using `base64::engine::general_purpose::STANDARD` with the `base64 = "0.22"` crate is correct API for that version. The note that tokens are generated at `id.atlassian.com` is correct.

One important gotcha is documented correctly: the `accountId` from `/myself` should be used in JQL rather than `currentUser()`. This matters because `currentUser()` resolves based on the authenticated user's cookie session, not the API token in some configurations. Using the explicit `accountId` is the right choice.

**Missing**: The `Accept: application/json` header is documented in the auth section but the ureq code snippet only shows `Content-Type`. An implementer following the code example alone would omit `Accept`. Some JIRA endpoints (notably `/rest/api/3/field`) return 406 or malformed responses without `Accept: application/json`. The spec text says to include it but the code example contradicts this. **Fix the code snippet.**

### 4. Pagination — CONCERN

**a. Loop termination condition — CONCERN**

The pagination spec says:
> Fetch all pages: loop while `startAt + page_size < total`, incrementing `startAt` by `maxResults` each iteration.

And in the search section:
> **Pagination**: Loop while `startAt + issues.len() < total`.

These two conditions are inconsistent. The first uses `page_size` (the requested `maxResults`); the second uses `issues.len()` (the actual returned count). The second is correct: JIRA may return fewer items than `maxResults` on the last page, and the last page's `issues.len()` will be less than `maxResults`. Using `page_size` in the termination condition is correct only if `page_size == issues.len()` on every page except the last — which is only guaranteed if `total` is exact. In practice JIRA's `total` is an estimate for large result sets (it says so in the API docs: `"Note: the total field is an estimate"`). The safe termination condition is: **stop when the returned page is empty or smaller than `maxResults`**, not purely when `startAt + count >= total`.

**Recommendation**: Use `if issues.is_empty() || issues.len() < max_results { break; }` as the primary termination, with the `total` check as a secondary guard.

**b. createmeta pagination — PASS**

Endpoint 11 is correctly documented as paginated with `startAt`/`maxResults`/`total` using `"values"` as the array key.

**c. `GET /rest/api/3/field` not paginated — PASS**

Correctly documented as a flat array with no pagination. This is accurate.

**d. Comment pagination — PASS**

Documented as paginated, with `orderBy=-created`. The `orderBy` parameter is supported on this endpoint. Correct.

### 5. ADF Handling — CONCERN

The `adf_to_text` algorithm is documented and covers the common node types. However there are gaps that will produce silent incorrect output or panics if unhandled:

**a. Missing node types — CONCERN**

The following ADF node types appear in real JIRA Cloud data but are not in the documented match arms:

- `"mediaSingle"` / `"media"` — image/attachment embeds. Common in descriptions. Unhandled means the `_` fallback recurses into `content`, but `media` nodes have no `content`, only `attrs`. Result: empty string (silent data loss, no panic).
- `"expand"` — collapsible section. Has `content` with a `title` attrs field. Will recurse into content but loses the expand title.
- `"panel"` — info/warning/note panels. Common for notes. Has `content`. Will recurse correctly but loses panel type context.
- `"taskList"` / `"taskItem"` — checklist items. Has `content`. Without a specific handler, the `_` fallback recurses but produces no prefix marker.
- `"nestedExpand"` — nested collapsible. Same issue as `expand`.

None of these cause panics given the documented `_` fallback, but they produce output that loses structure silently. For a TUI display this is acceptable. The spec should note this explicitly so implementers don't add defensive panics or unwrap calls on the unknown node type.

**b. `listItem` rendering with nested paragraph — CONCERN**

The algorithm says:
```
"listItem" => "- " + join children
```
But in real ADF, `listItem` children are always wrapped in a `paragraph` (or `codeBlock`) node. So a `listItem` actually renders as `"- " + adf_to_text(paragraph_child)` which produces `"- " + "Item text"` = `"- Item text"`. This is correct as long as `paragraph` joins its children with `""`. However, the algorithm for `paragraph` is `join children with ""`, which means a `listItem` containing a `paragraph` containing text produces the right result.

The subtle bug is with `orderedList`. The algorithm says "number each" but the implementation needs to carry an index counter through the iteration of `listItem` siblings. The pseudocode does not show how the counter is threaded through the recursive descent. An implementer who writes a pure recursive function without mutable state will produce `"- "` for all ordered list items. This needs explicit documentation or a concrete implementation hint.

**c. `link` mark handling — PASS**

The docs correctly note to append the URL in parens for `link` marks on `text` nodes.

**d. Nested `marks` array — PASS**

The `"marks"` field is documented as an array on text nodes. The instruction to ignore all marks except `link` is correct for plain-text output. The `marks` field may be absent on text nodes without formatting — implementers must treat it as optional, which is implied but not explicitly stated. Minor.

**e. Empty `content` array — PASS**

The doc says: "Content is optional on any node. hardBreak, mention, inlineCard, emoji, rule have NO content array. Always check before recursing." This is correct and sufficient.

### 6. Custom Field Discovery — PASS

The discovery logic using `schema.custom` contains `"gh-sprint"` is the most reliable approach and matches what Atlassian documents. The story points disambiguation via `schema.custom` containing `"float"` is also correct and handles the common case where organizations have multiple custom number fields.

One edge case not documented: some organizations use a "Story point estimate" field (often `customfield_10016`) alongside the legacy "Story Points" field from an older scheme (`customfield_10028`). Both match the `float` schema check. The "if multiple matches, omit from display" policy is safe and correct.

The discovery being conditional on config absence (use config if set, fall back to discovery) is the right design.

### 7. Transition Fields — PASS

The comment-in-update-key gotcha is documented explicitly and correctly:

> Regular fields go in `"fields"`, but comments go in `"update"` with an `"add"` operation.

The example request bodies are correct. The `fields` map (keyed by field ID, not array) from the transitions response is correctly described.

One minor gap: the docs do not document what to do when a required transition field has type `"comment"` in the field schema (i.e., when `fields["comment"]` appears with `required: true`). This happens in some workflows. The `TransitionField` struct does not model this. The workaround is to treat `"comment"` type fields as `FieldType::Unsupported` and show an error, or to add a special case that opens `$EDITOR` like the standalone comment flow. Not documenting this leaves implementers to discover the discrepancy at runtime. Low probability on most JIRA instances, but worth a note.

### 8. Issue Creation — PASS (with one concern)

The createmeta → create flow is correct:
- Use `createmeta/{projectKey}/issuetypes` to list issue types (correct `"values"` wrapper noted)
- Use `createmeta/{projectKey}/issuetypes/{id}` for required fields (correctly noted as paginated)
- Filter out `project`, `issuetype`, `reporter` from the user form
- Set `assignee.accountId` from `/myself`
- Use `{ "id": "..." }` for select fields

**One concern**: The `reporter` filter is correct for most cases, but JIRA Cloud sometimes returns `reporter` as a required field from createmeta AND does not auto-set it from the API token identity on all project configurations (particularly service-desk and business projects where reporter tracking differs). If `reporter` is required and filtered out, the create call will return a 400. The doc says to filter it out unconditionally, which will fail on those project types. **Recommendation**: Filter `reporter` from display but silently set it to `accountId` from `/myself` in the POST body if it appears as required in createmeta.

### 9. Rate Limiting — CONCERN

The `jira-plugin.md` documents:
> Respect `Retry-After` headers on 429 responses. Show a toast. Never send concurrent requests for the same resource.

The `jira-api-reference.md` documents `429` in the error status code table.

However, neither document specifies:

- **What the `Retry-After` header value format is.** JIRA Cloud returns `Retry-After` in seconds (an integer string, e.g., `"30"`), not an HTTP-date. Implementers must parse it as `u64` seconds. This is not obvious to everyone (HTTP spec allows both formats; JIRA uses only seconds).
- **Maximum retry count.** No cap on retries is defined. A stuck rate-limit loop could spin indefinitely.
- **Whether to retry on network errors vs. rate-limit errors.** These should behave differently: rate-limit = wait then retry; network error = show error immediately (per the error handling table, which correctly treats these differently for user-initiated vs. auto-refresh, but the retry-after retry loop is not bound to auto-refresh only).

These gaps will not cause a panic but will cause confusing behavior that requires fixing in production.

### 10. Error Responses — PASS (with one gap)

The error response shapes are documented for:
- Standard `errorMessages` + `errors` format
- Field-level validation errors in `errors` map
- HTTP status semantics (400, 401, 403, 404, 429)

The Rust deserialization struct is correct.

**One gap**: JIRA occasionally returns non-JSON error bodies (e.g., a 503 HTML page from Cloudflare/Atlassian CDN, or a 500 with a plain text message). The docs do not document how to handle deserialization failures of the error body. An implementer who does `response.into_json::<JiraErrorResponse>()` unconditionally on non-2xx responses will panic or return an opaque error when the body is not JSON. **Recommendation**: On non-2xx, attempt to parse as `JiraErrorResponse`; on parse failure, surface the raw response body text as the error message.

---

## Gaps Found

**Numbered by severity:**

1. **(Medium) Pagination termination is inconsistently specified.** The general pattern uses `startAt + page_size < total`; the search-specific note uses `issues.len()`. The `total` field is documented by Atlassian as an estimate. The safe termination is `page.is_empty() || page.len() < max_results`. Fix the general pattern to match the search-specific note, and add a note that `total` is an estimate.

2. **(Medium) `reporter` in createmeta should be set silently, not just filtered.** If a project requires `reporter` (business/service-desk projects), the create call fails with a 400. Silently injecting `reporter: { accountId: ... }` when it appears in createmeta required fields fixes this without exposing it to the user.

3. **(Medium) Sprint field may not always be an array.** Some Cloud configurations return a scalar object. Deserialize as `serde_json::Value` and handle both array and object cases.

4. **(Medium) `Accept: application/json` missing from ureq code example.** The spec text says to include it; the code snippet does not. Some endpoints (especially `/rest/api/3/field`) can behave incorrectly without it.

5. **(Low) `Retry-After` header format not specified.** JIRA uses integer seconds. Add a note that it must be parsed as `u64`, not as an HTTP-date.

6. **(Low) `orderedList` rendering requires a mutable counter.** The pseudocode says "number each" but does not show how to thread a counter through the recursive function. Add a concrete implementation note.

7. **(Low) Non-JSON error bodies not handled.** CDN/gateway errors return HTML. Add a fallback to surface raw response text when JSON parse fails on error responses.

8. **(Low) `parent.fields.issuetype.hierarchyLevel` may be absent in search response.** Deserialize as `Option<i32>`. Document the fallback chain: `hierarchyLevel == 1` → `name == "Epic"` → `None`.

9. **(Low) ADF `mediaSingle`/`taskList`/`expand` nodes not documented.** The `_` fallback handles them without panic, but the omission should be noted so implementers don't add incorrect logic. Add a comment in the pseudocode: "the _ arm covers mediaSingle, expand, panel, taskList, taskItem, nestedExpand — text content is extracted but structural context is lost."

10. **(Low) `comment` type in transition required fields not documented.** If a transition's `fields` object contains a field with `schema.type == "comment"`, the `fields`-vs-`update` key distinction applies and the current struct does not model it. Add a note to treat `"comment"` fields as a special case that uses the `update.comment` path, or classify as `Unsupported` with an explicit error message.

---

## Final Verdict

**REJECT — revise and re-submit**

The documentation is substantially correct and far above average for an API integration spec. Most of the shapes are accurate, authentication is correct, the comment-in-update gotcha is captured, the `Accept` header is mentioned (if not in the code example), ADF conversion covers the common cases, and the error model is sound.

However, there are two issues that will cause silent runtime failures on real JIRA Cloud instances with non-trivial configurations:

- **Pagination termination using an estimated `total`** will silently miss issues on large workloads.
- **`reporter` hard-filtered from the POST body** will cause 400 errors on any business or service-desk project that marks reporter as required.

Both are straightforward to fix. Once those two gaps are addressed and the `Accept` header is added to the code example, this spec is ready to implement against.

**After fixing items 1–4 above, re-review is not required — those are mechanical fixes. The overall API model is sound.**
