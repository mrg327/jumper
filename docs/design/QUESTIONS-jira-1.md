# JIRA Plugin Design Review: Questions & Concerns

Adversarial review of `jira-plugin.md` and `plugin-architecture.md`, cross-referenced against the existing codebase and JIRA Cloud REST API v3 specifics.

---

## 1. JIRA Cloud REST API v3 Issues

### 1.1 JQL `currentUser()` does not work with Basic Auth (email + API token)

**Severity: Red — Blocker**

The doc specifies JQL `assignee = currentUser()` in the `/rest/api/3/search` endpoint. The `currentUser()` JQL function resolves the caller's identity from the authentication context. With OAuth 2.0 (3LO), this works because the token is tied to a specific Atlassian account. However, with Basic auth using `email:api_token`, behavior has been inconsistent across JIRA Cloud versions.

The safer approach — and the one that works reliably with Basic auth — is to use the account ID:

- First call `/rest/api/3/myself` to get the authenticated user's `accountId`
- Then use `assignee = "<accountId>"` in JQL

Alternatively, `assignee = "email@example.com"` works in some instances but Atlassian has been deprecating email-based lookups in JQL in favor of account IDs (GDPR compliance changes from 2019 onward). Relying on email in JQL is fragile.

**Recommendation**: Add a startup step in the API layer: call `/rest/api/3/myself`, cache the `accountId`, and use it in all JQL queries. This endpoint also serves as the connectivity check mentioned in "Startup Validation" step 3, killing two birds with one stone.

### 1.2 The `createmeta` endpoint is correctly identified, but the response shape needs attention

**Severity: Yellow — Needs Resolution**

The doc correctly uses the non-deprecated per-project endpoint paths:
- `/rest/api/3/issue/createmeta/{projectKey}/issuetypes`
- `/rest/api/3/issue/createmeta/{projectKey}/issuetypes/{issueTypeId}`

Good — the old `/rest/api/3/issue/createmeta` endpoint (with `expand=projects.issuetypes.fields`) was deprecated in JIRA Cloud and returns 404 on newer instances.

However, the doc does not specify how to extract fields from the response. The per-issue-type endpoint returns fields as a map of field IDs to field metadata objects. The `CreateMetaResponse` type is referenced in `JiraResult::CreateMeta(CreateMetaResponse)` but never defined. What fields does it contain? It should at minimum include:
- `fields: HashMap<String, FieldMeta>` where `FieldMeta` includes `required`, `schema`, `allowedValues`, `name`
- The `schema.type` and `schema.custom` values for determining which `FieldType` variant to use

**Recommendation**: Define `CreateMetaResponse` explicitly. Clarify how it maps to the `EditableField` struct and `FieldType` enum.

### 1.3 Status transitions may require fields — this is not handled in the transition flow

**Severity: Red — Blocker**

The doc shows a transition picker modal that lets users select a transition and apply it. The error modal example even shows the exact scenario: "Transition 'Done' requires field 'Resolution' to be set." But the design does not provide a flow for actually setting those fields.

In JIRA Cloud, the `GET /rest/api/3/issue/{key}/transitions` response includes a `fields` object per transition that lists fields that are required or available for that specific transition. Many real-world JIRA workflows require:
- **Resolution** when transitioning to Done (e.g., "Fixed", "Won't Fix", "Duplicate")
- **Comment** on certain transitions (e.g., "Reject" requiring a reason)
- **Custom fields** on specific transitions (e.g., "QA Sign-off" requiring a test result)

Simply POSTing `{ "transition": { "id": "31" } }` without the required fields will return a 400 error.

**Recommendation**: After the user selects a transition, check if that transition has required fields (from the transitions response). If so, present field input modals (same pattern as issue creation) before executing the POST. The transition POST body supports an `update` or `fields` object for this purpose. This is not a Phase 1e polish item — it blocks basic usage of any JIRA instance with a non-trivial workflow.

### 1.4 ADF write path is lossy and may cause data corruption on description edits

**Severity: Yellow — Needs Resolution**

The doc specifies that writing descriptions/comments uses `text_to_adf()` which wraps everything in a single paragraph node. For **new comments**, this is acceptable — comments are append-only and the user typed the text, so a single paragraph is a faithful representation.

For **description edits**, this is problematic:
1. User opens issue detail, sees description rendered as plain text (ADF -> plain text, with heading markers, bullet lists, etc.)
2. User presses `e` to edit description
3. The edit modal pre-fills with the plain-text version of the existing description
4. User makes a small change and submits
5. `text_to_adf()` wraps the entire text in a single paragraph, destroying all original formatting (headings, lists, code blocks, tables, images, mentions, links, etc.)

A description that was a rich 50-line document with code blocks and tables becomes a single flat paragraph. This is silent data loss.

**Recommendation**: Two options:
- **Option A (conservative)**: Do not support description editing. Only support editing simple text fields (summary, custom text fields). Show description as read-only. This is the safest approach for Phase 1.
- **Option B (append-only)**: When "editing" a description, append the user's text as a new ADF paragraph node to the existing ADF content, preserving the original structure. This requires fetching the raw ADF from the API (not the plain-text conversion), appending a paragraph, and PUTting the result back.
- **Option C (full round-trip)**: Build a proper ADF parser/serializer. This is a large scope increase and should be a separate phase if pursued at all.

The doc should explicitly state which approach is taken and warn that `text_to_adf()` is only safe for new content (comments, new issue descriptions).

### 1.5 `StatusCategory` is missing the `undefined` category

**Severity: Green — Minor**

JIRA Cloud's status category API returns four possible `statusCategory.key` values: `new`, `indeterminate`, `done`, and `undefined`. The doc maps to three enum variants: `ToDo`, `InProgress`, `Done`.

The `undefined` category is rare but exists. It typically appears for statuses that have not been properly configured in the workflow scheme. In practice, mapping `undefined` to `ToDo` is reasonable, but the doc should acknowledge this explicitly so the deserialization code includes a fallback rather than panicking on an unexpected value.

**Recommendation**: Add a comment in the `StatusCategory` enum or a note in the doc that `undefined` and `new` both map to `ToDo`. Use `#[serde(other)]` or manual deserialization with a default.

---

## 2. reqwest, Threading, and Concurrency

### 2.1 `reqwest::blocking` spawns its own tokio runtime — interaction with `std::thread`

**Severity: Yellow — Needs Resolution**

`reqwest::blocking::Client` internally creates a tokio runtime on the calling thread. When you spawn a `std::thread` and create a `reqwest::blocking::Client` on it, that thread gets its own single-threaded tokio runtime. This works, but there are implications:

1. **One `Client` per thread, created once**: The doc does not specify whether a new `Client` is created per request or shared. Creating a new `Client` per request means a new tokio runtime + TLS session per request. This adds ~50-100ms of overhead per call for the TLS handshake alone. The `Client` (and its connection pool) must be created once when the background thread starts and reused for all requests.

2. **Thread lifetime**: The doc says the thread is spawned on `on_enter()` and stopped on `on_leave()`. But `on_leave()` is only called when the user navigates away. If the user is on the JIRA screen and the background thread panics (network driver crash, TLS library panic, etc.), the `mpsc::Sender` in the thread drops, causing `Receiver::recv()` or `try_recv()` to return `Err(Disconnected)`. The doc does not specify how the TUI detects or handles this. A dead background thread means no more API calls — the plugin is silently broken.

3. **Thread restart**: If the user navigates away (`on_leave()`) and back (`on_enter()`), a new thread is spawned. If `on_leave()` does not join or signal the old thread to stop, there could be two threads running simultaneously, both writing to channels. The doc mentions "cleanup, cancelling background tasks" in `on_leave()` but does not specify the mechanism (shutdown flag, drop the sender, join with timeout?).

**Recommendation**:
- Explicitly state that a single `reqwest::blocking::Client` is created once per thread lifetime and reused.
- Define a shutdown mechanism: an `AtomicBool` or a poison-pill `JiraCommand::Shutdown` variant that causes the thread's loop to exit.
- On `on_enter()`, check if the thread's `JoinHandle::is_finished()` and restart if needed.
- On `on_leave()`, send the shutdown command and `join()` with a timeout (e.g., 5 seconds). If the join times out, detach (the thread will die when the process exits).

### 2.2 `try_recv()` in `on_tick()` only reads one result — must drain the channel

**Severity: Yellow — Needs Resolution**

The plugin architecture doc says results are "checked in `on_tick()`" which is called every 1 second. Looking at the existing codebase, the tick interval is 1 second (app.rs line 208: `Duration::from_secs(1)`), but the event loop polls at 100ms (line 200: `Duration::from_millis(100)`).

If a single `try_recv()` call is made per tick, and the background thread completes multiple requests between ticks (e.g., a transition + auto-refresh that both complete within 1 second), only the first result is processed. The second sits in the channel until the next tick.

Worse: if results accumulate faster than they're consumed (e.g., during rapid user actions), the channel buffer grows and the UI falls behind.

**Recommendation**: Use a `while let Ok(result) = receiver.try_recv()` loop in `on_tick()` to drain all pending results. This is a simple fix but must be called out explicitly in the design.

Additionally, consider whether `on_tick()` is the right place at all. The existing event loop runs at 100ms (the `event::poll` timeout). A dedicated check in the render/update cycle (not gated on the 1-second tick) would reduce latency from up to 1 second to at most 100ms. The `ScreenPlugin::on_tick()` could be split into `on_tick()` (1s, for timers) and `on_poll()` (every frame, for channel draining).

### 2.3 Write-then-refresh race condition

**Severity: Yellow — Needs Resolution**

The doc says: "After any write operation (transition, edit, create, comment), auto-refresh." This means after a `TransitionComplete` result is received, the plugin sends a `FetchMyIssues` command. But what if the 60-second auto-refresh timer fires while a write is in-flight? The sequence could be:

1. User transitions an issue -> `TransitionIssue` sent to background thread
2. 60s timer fires -> `FetchMyIssues` sent to background thread
3. Background thread processes `TransitionIssue`, returns `TransitionComplete`
4. Plugin receives `TransitionComplete`, sends another `FetchMyIssues`
5. Background thread processes first `FetchMyIssues` (from step 2), returns `Issues(...)` — this reflects the OLD state (transition might not be visible yet due to JIRA's eventual consistency)
6. Background thread processes second `FetchMyIssues` (from step 4), returns `Issues(...)` — this reflects the NEW state
7. Plugin updates its cache twice, potentially flickering between old and new state

Additionally, JIRA Cloud has eventual consistency — a transition may take a few hundred milliseconds to be reflected in search results. A refresh immediately after a write may return stale data.

**Recommendation**:
- Add a `refreshing: bool` flag. Skip auto-refresh if a refresh is already in-flight.
- After a write operation, add a short delay (500ms-1s) before the post-write refresh to account for JIRA's eventual consistency.
- Consider a generation counter: each `FetchMyIssues` command carries a monotonic sequence number, and the result includes it. The plugin ignores results with a stale sequence number.

### 2.4 Background thread panic detection

**Severity: Yellow — Needs Resolution**

If the background thread panics (e.g., due to a bug in JSON deserialization, an unexpected API response shape, or a reqwest internal error), the `JoinHandle` captures the panic but the TUI thread has no way to know unless it checks.

The `mpsc::Sender` held by the background thread will be dropped on panic, causing subsequent `Receiver::try_recv()` calls to return `Err(TryRecvError::Disconnected)`. But `Err(TryRecvError::Empty)` (no results yet) and `Err(TryRecvError::Disconnected)` (thread dead) need to be distinguished. If the plugin treats disconnection as "no results yet," it will silently stop working.

**Recommendation**: In the `on_tick()` drain loop, check for `TryRecvError::Disconnected` specifically. When detected, show an error toast/modal ("JIRA connection lost — press R to reconnect") and set a `thread_dead: bool` flag. On `R` (manual refresh), if the thread is dead, attempt to respawn it.

---

## 3. Data Model Gaps

### 3.1 Story points live in an instance-specific custom field

**Severity: Red — Blocker**

The doc defines `story_points: Option<f64>` on `JiraIssue`. JIRA does not have a built-in "story points" field. Story points are stored in a custom field whose ID varies by instance:
- Atlassian-managed instances typically use `customfield_10016`
- Older instances or those with different plugins may use `customfield_10028`, `customfield_10004`, or any other ID
- Some instances use the Atlassian "Story Points" field type, others use a generic Number custom field

There is no reliable way to discover which custom field holds story points without either:
1. **Configuration**: The user specifies the custom field ID in `config.yaml`
2. **Heuristic discovery**: Search `/rest/api/3/field` for a field with `name == "Story Points"` or `name == "Story point estimate"`. But field names are localizable and can be renamed by admins.

The current design has no mechanism for this discovery.

**Recommendation**: Add an optional `story_points_field` to `JiraConfig`:
```yaml
jira:
  story_points_field: "customfield_10016"  # optional, auto-discovered if omitted
```
On startup, if not configured, call `GET /rest/api/3/field` and search for a field whose `name` contains "story point" (case-insensitive). If multiple matches or zero matches, omit story points from the display rather than crashing. Log a warning.

### 3.2 Sprint is a complex object in a custom field, not a simple string

**Severity: Yellow — Needs Resolution**

The doc shows `sprint: Option<String>` as the sprint name. JIRA returns sprint data in a custom field (typically `customfield_10020`, but again instance-specific) as a JSON object (or array of objects for multi-sprint), not a string. The sprint object contains:
```json
{
  "id": 37,
  "name": "Sprint 24",
  "state": "active",
  "startDate": "2026-03-15T00:00:00.000Z",
  "endDate": "2026-03-29T00:00:00.000Z"
}
```

The same custom-field-discovery problem applies here as with story points. Additionally, an issue can belong to multiple sprints (e.g., carried over from a previous sprint), in which case the field is an array.

**Recommendation**: Same approach as story points — optional config, auto-discovery fallback. Extract `name` from the sprint object (or the last/active sprint if multiple). Define the parsing explicitly.

### 3.3 `FieldType` enum is incomplete for real-world JIRA fields

**Severity: Yellow — Needs Resolution**

The doc defines 6 `FieldType` variants: `Text`, `TextArea`, `Number`, `Select`, `MultiSelect`, `Date`. Real JIRA instances commonly have:
- **User picker** (`com.atlassian.jira:user-field`)
- **Version picker** (`com.atlassian.jira:versionpicker`)
- **Cascading select** (`com.atlassian.jira:cascadingselect`)
- **URL** (`com.atlassian.jira:url`)
- **Labels** (array of strings, different from MultiSelect)
- **Radio buttons** (`com.atlassian.jira:radiobuttons`)
- **Checkboxes** (`com.atlassian.jira:checkboxes`)
- **Sprint** (custom type)
- **Tempo/time tracking** fields

When the plugin fetches editmeta or createmeta and encounters an unsupported field type, what happens?

**Recommendation**: Add an `Unsupported` variant to `FieldType`. When rendering an unsupported field in the detail modal, show it as read-only with the raw value as a string. When encountered during issue creation (if it's required), show an error: "Required field 'X' has unsupported type 'Y' — create this issue in JIRA directly." Do not silently skip required fields.

### 3.4 Pagination is not addressed for search results

**Severity: Yellow — Needs Resolution**

The `/rest/api/3/search` endpoint returns paginated results. The default `maxResults` is 50 (and the maximum is typically 100, though it varies). If the user has more than 50 assigned issues, the plugin will silently show only the first page.

The doc does not mention pagination at all.

**Recommendation**: Either:
- **Option A**: Fetch all pages in a loop (send multiple requests with increasing `startAt`), collect all results, then send the full `Vec<JiraIssue>` back to the TUI thread. This keeps the UI simple but may be slow for users with hundreds of assigned issues.
- **Option B**: Set `maxResults=100` and document the limit. Most users with more than 100 assigned issues have a process problem, not a tooling problem.

Option A is preferred. The background thread can handle the pagination transparently.

### 3.5 Epic field extraction is underspecified

**Severity: Green — Minor**

The doc shows `epic: Option<EpicInfo>` with `key` and `name`. In JIRA Cloud, the epic link is stored in `customfield_10014` (instance-specific, again). In newer JIRA Cloud (with next-gen/team-managed projects), epics are represented as parent issues rather than a custom field. The doc does not specify how to distinguish these two representations or how to extract epic data.

**Recommendation**: For classic projects, check the configured or discovered epic link custom field. For next-gen projects, check if `fields.parent` exists and has `fields.issuetype.name == "Epic"`. Handle both cases.

---

## 4. Plugin Architecture Integration Concerns

### 4.1 Modal ownership: who renders plugin modals?

**Severity: Yellow — Needs Resolution**

The existing codebase has a `modal_stack: Vec<Modal>` on the `App` struct. Modals are rendered by the app's render pipeline (app.rs lines 371-375). The `Modal` enum has variants for `Input`, `Select`, `Confirm`, and `Help`.

The JIRA plugin needs its own modals: transition picker, issue detail, comment input, field editor, error display. These are not variants of the existing `Modal` enum. The plugin architecture doc says `handle_key()` returns `Action`, but how does the plugin request a modal?

Two approaches are implied but neither is specified:
- **Plugin-internal modals**: The plugin manages its own modal state and renders modals inside its `render()` call. Key events are handled by the plugin itself. This is self-contained but means the plugin must re-implement modal rendering (centering, border, clear-background, focus management).
- **App-managed modals**: The plugin returns `Action::ShowModal(ModalKind)` and the app manages rendering. But the existing `Modal` and `ModalId` enums would need plugin-specific variants, breaking the self-contained principle.

Looking at the existing `SelectModal` and `InputModal` code, they are fairly generic. The JIRA transition picker is essentially a `SelectModal`. The comment input is an `InputModal`. But the issue detail modal is a complex, custom view that does not fit either pattern.

**Recommendation**: The plugin should manage its own modal state internally. The `ScreenPlugin::render()` method receives the full screen area and can render overlays (modals) on top of the board. Key routing is already handled — when the plugin screen is active, all keys go to `handle_key()`, so the plugin can internally dispatch to modal handlers. Confirm this is the intended approach and document it in the plugin architecture doc.

### 4.2 The existing `Action` enum has no plugin-generic extensibility

**Severity: Yellow — Needs Resolution**

The `ScreenPlugin::handle_key()` signature returns `Action`. Looking at the `Action` enum in `events.rs`, it contains many app-specific variants (`StartWork`, `SwitchContext`, `EditFocus`, etc.). The JIRA plugin only needs a few: `None`, `Back`, `Toast(String)`, and possibly `Tick`.

But if a future plugin needs a new action (e.g., "open URL in browser"), there is no mechanism to add plugin-specific actions without modifying the core `Action` enum, which violates the self-contained principle.

**Recommendation**: Either:
- Accept that `Action` is a closed enum and plugins only use a subset (`None`, `Back`, `Toast`, `PushScreen`, `PopScreen`). Document which `Action` variants are available to plugins.
- Add an `Action::PluginAction(String, serde_json::Value)` variant for extensibility. The app can route these to plugin-specific handlers.

The first option is simpler and probably sufficient for the foreseeable future.

### 4.3 `ScreenId::Plugin(String)` introduces string-based dispatch

**Severity: Green — Minor**

The plugin architecture doc adds `ScreenId::Plugin(String)`. This means screen routing uses a runtime string match rather than compile-time enum matching. Not a problem functionally, but it means typos in plugin names are not caught at compile time. The existing `ScreenId` variants are all statically typed.

**Recommendation**: Acceptable tradeoff for plugin extensibility. Just ensure the string is validated at registration time (plugin name must match the config key).

---

## 5. Dependency and Build Concerns

### 5.1 `reqwest` with blocking + json is a heavy dependency

**Severity: Yellow — Needs Resolution**

The current `jm-tui` Cargo.toml has 7 dependencies. Adding `reqwest` with `blocking` and `json` features pulls in:
- `tokio` (full runtime, required by reqwest::blocking internally)
- `hyper` + `http`
- `rustls` or `native-tls` (depending on feature flags — the doc does not specify which)
- `serde_json` (already needed, minimal cost)
- `base64` (small)
- `h2`, `tower`, `pin-project-lite`, and 30+ transitive crates

This can add 30-60 seconds to a clean build and increase binary size by 3-5 MB. For a personal TUI tool, this is a significant increase.

**Recommendation**:
- Consider `ureq` as an alternative. `ureq` is a synchronous HTTP client that does not depend on tokio. It supports TLS (via rustls or native-tls), JSON via serde, and Basic auth. It is dramatically lighter: ~15 crates vs ~80 for reqwest::blocking. Since the JIRA plugin only needs synchronous HTTP from a background thread, `ureq` is a natural fit.
- If reqwest is chosen, specify TLS backend explicitly. `reqwest` defaults to `native-tls` on most platforms, but `rustls-tls` avoids the OpenSSL dependency on Linux. Add `default-features = false, features = ["blocking", "json", "rustls-tls"]` to be explicit.
- Consider making the JIRA plugin a cargo feature (`jira`) so users who don't need it don't pay the compile-time cost:
  ```toml
  [features]
  jira = ["reqwest", "base64"]
  ```

### 5.2 TLS certificate handling is unspecified

**Severity: Green — Minor**

Corporate environments (like automotive) frequently use custom CA certificates or TLS inspection proxies. If the user's JIRA instance is behind such a proxy, reqwest/ureq will reject the certificate by default.

**Recommendation**: Add an optional `tls_ca_cert` config option or respect the `SSL_CERT_FILE` / `REQUESTS_CA_BUNDLE` environment variables. `reqwest` supports this via `ClientBuilder::add_root_certificate()`. Document this for corporate users.

---

## 6. UX Edge Cases

### 6.1 Stale issue reference after refresh

**Severity: Yellow — Needs Resolution**

The plugin caches `Vec<JiraIssue>`. A refresh replaces this entire vector. If the user has the issue detail modal open for `HMI-103` and a refresh completes that removes `HMI-103` (e.g., it was reassigned to someone else), the detail modal now references an issue that no longer exists in the cache.

Similarly, if the issue's fields changed during the refresh, the detail modal shows stale data until the user closes and reopens it.

**Recommendation**: When a refresh completes and the detail modal is open, check if the viewed issue still exists in the new data. If not, close the modal with a toast ("HMI-103 is no longer assigned to you"). If it exists, update the detail modal's data in place.

### 6.2 Project list for issue creation is derived from assigned issues

**Severity: Green — Minor**

The doc says "only projects with existing assignments shown" for issue creation. This means if a user is added to a new JIRA project but has no assigned issues yet, they cannot create issues in that project from the TUI. They would need to go to JIRA's web UI, get an issue assigned, then return to the TUI.

**Recommendation**: Consider also fetching the user's project memberships via `/rest/api/3/project` (or `/rest/api/3/project/search` with `typeKey=software`). This is a nice-to-have for Phase 1e.

### 6.3 No offline/degraded mode

**Severity: Green — Minor**

The doc says "data always comes from the API; nothing is persisted locally." If the network drops while the user is on the JIRA screen, the board goes stale and all operations fail. There is no indication of connectivity status beyond error modals on individual operations.

**Recommendation**: Add a connectivity indicator in the header (e.g., a red indicator if the last refresh failed). Consider caching the last successful response in memory so the board remains visible (with a "stale" warning) even when the network is down.

---

## Summary Table

| # | Issue | Severity | Phase Impact |
|---|-------|----------|-------------|
| 1.1 | `currentUser()` JQL with Basic Auth | Red | Phase 1a |
| 1.2 | `CreateMetaResponse` undefined | Yellow | Phase 1d |
| 1.3 | Transitions requiring fields | Red | Phase 1b |
| 1.4 | ADF lossy write path for description edits | Yellow | Phase 1c |
| 1.5 | Missing `undefined` status category | Green | Phase 1a |
| 2.1 | reqwest Client lifetime and thread management | Yellow | Phase 1a |
| 2.2 | Channel drain in `on_tick()` | Yellow | Phase 1a |
| 2.3 | Write-then-refresh race condition | Yellow | Phase 1b |
| 2.4 | Background thread panic detection | Yellow | Phase 1a |
| 3.1 | Story points custom field discovery | Red | Phase 1a |
| 3.2 | Sprint custom field extraction | Yellow | Phase 1a |
| 3.3 | Incomplete `FieldType` enum | Yellow | Phase 1c |
| 3.4 | Search pagination not addressed | Yellow | Phase 1a |
| 3.5 | Epic field extraction underspecified | Green | Phase 1a |
| 4.1 | Modal ownership unclear | Yellow | Phase 0 (plugin arch) |
| 4.2 | `Action` enum extensibility | Yellow | Phase 0 (plugin arch) |
| 4.3 | String-based screen dispatch | Green | Phase 0 (plugin arch) |
| 5.1 | reqwest dependency weight | Yellow | Phase 1a |
| 5.2 | TLS certificate handling | Green | Phase 1a |
| 6.1 | Stale issue in detail modal after refresh | Yellow | Phase 1b |
| 6.2 | Project list limited to assigned-issue projects | Green | Phase 1d |
| 6.3 | No offline/degraded mode | Green | Phase 1e |

**Blockers (3)**: 1.1, 1.3, 3.1 — these will cause runtime failures or incorrect behavior on real JIRA instances. Resolve before implementation begins.

**Needs Resolution (12)**: Must be addressed in the design doc before the relevant phase starts. Several of these (2.1, 2.2, 2.4) are foundational and affect Phase 1a.

**Minor (7)**: Can be addressed during implementation or deferred to polish.
