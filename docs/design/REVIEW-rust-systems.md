# Rust Systems Review — JIRA Plugin Implementation Readiness

**Reviewer role**: Senior Rust systems engineer. No bias toward approval.
**Review date**: 2026-03-27
**Scope**: Docs 1–6 (design specs) + Files 7–11 (implemented Phase 0 code)

---

## Assessment

### 1. Trait Foundation

**Verdict: PASS**

The implemented `ScreenPlugin` trait in `plugins/mod.rs` matches the spec exactly. `render(&self, frame: &mut Frame, area: Rect)`, `handle_key(&mut self, key: KeyEvent) -> PluginAction`, `on_enter`, `on_leave`, `on_tick`, `key_hints` — all present with the correct signatures.

One deviation from `plugin-architecture.md` spec is harmless: the architecture doc shows `on_notify` on `ScreenPlugin`, but the implemented trait omits it. This is fine — the JIRA plugin spec never uses it, and adding it later is backwards-compatible since it has a default impl.

The `PluginRegistry::new` in `registry.rs` currently hardcodes `AboutPlugin` and has no arm for `"jira"`. Agents will need to add the JIRA registration arm there. This is clearly implied by the design and is not a gap — it is straightforward.

The `key_hints()` return type is `Vec<(&'static str, &'static str)>` in the implemented trait — the spec agrees. This matters because agents need to store keybinding strings as `'static` (e.g., string literals, not `format!()` output), which is correctly specified.

### 2. serde Deserialization

**Verdict: CONCERN**

The API reference provides Rust deserialization structs for most shapes. However, several gaps would cause compile errors or silent runtime failures:

**a) `sprint` field is a `serde_json::Value` problem in disguise.** The search response shows `customfield_10020` as an array of sprint objects with `id: integer` (not string). The spec says to deserialize this field dynamically because the field ID is not known at compile time. The document correctly shows it as a raw array, and the extraction algorithm is documented. However, there is no Rust deserialization struct for the sprint object itself. An agent must invent:

```rust
#[derive(Deserialize)]
struct SprintValue {
    name: String,
    state: String,
}
```

This is derivable from the JSON example, but it is not given. Minor concern.

**b) The `fields` map in the search response is a `HashMap<String, serde_json::Value>`.** The story points and sprint custom fields have dynamic IDs, so the outer `fields` object cannot be a statically-typed struct. The spec does not show an explicit deserialization struct for the `issues[*].fields` object. An agent must decide: deserialize the whole issue as `serde_json::Value` and then extract fields by path, OR use a hybrid struct with `#[serde(flatten)]` and `serde_json::Value` for the remainder. This is a non-trivial design decision that the spec leaves entirely open. If an agent writes a fully-typed struct without accounting for dynamic field IDs, it will fail to compile or will silently drop sprint/story-points data.

**c) The `transitions.fields` object is a map keyed by field ID.** The spec correctly warns it is a map, not an array. It also correctly notes that an agent should use `HashMap<String, TransitionFieldMeta>` — but `TransitionFieldMeta` is only sketched in the api-reference, not given a complete `#[derive(Deserialize)]` struct with all fields that appear in the JSON. Specifically, `hasDefaultValue` and `operations` (array of strings) appear in the JSON but are absent from the sketch. If an agent uses `#[serde(deny_unknown_fields)]` this will panic; without that it is fine. This is acceptable.

**d) `JiraIssue.epic` extraction requires checking `issuetype.hierarchyLevel == 1` OR `issuetype.name == "Epic"` on the parent.** The parent object shape is documented (with the nested `fields.issuetype`) but no deserialization struct is given. An agent must navigate the `serde_json::Value` tree for this OR write a nested struct. This is workable but underdocumented.

**e) `StatusCategory` deserialization is specified via `#[serde(other)]` but the spec does not give the concrete `#[derive(Deserialize)]` impl.** The enum uses the string value of `.statusCategory.key`. An agent must know to use `#[serde(rename = "new")]`, etc. or use a `from_str` approach. The spec says "Use `#[serde(other)]` or manual deserialization with fallback" which is correct guidance but not a complete struct.

**f) `ureq v3` returns `ureq::Response` which has changed API vs v2.** The spec specifies `ureq = { version = "3", features = ["json"] }`. In ureq v3, reading the response body is done differently from v2. The api-reference code sample uses `.call()?` and implies chaining onto the response — but ureq v3 separates the status check from body reading. The correct v3 pattern is:

```rust
let response = agent.get(&url).header(...).call()?;
let body: MyStruct = response.into_json()?;
```

The code sample in the api-reference only shows `.call()?` and stops there. Agents need to know that `call()` returns `Result<Response, ureq::Error>` and that `.into_json::<T>()` (or `.into_body().read_to_string()`) reads the body. The sample omits the body-reading step entirely for the auth request. This is a **concrete gap** — without it, agents will write code that makes the request but never reads the response.

### 3. ureq Integration

**Verdict: CONCERN**

The spec correctly identifies `ureq = { version = "3", features = ["json"] }` as the dependency. However:

**a) Missing body-reading step in the auth code example.** See section 2f above.

**b) Error handling with ureq v3 is not documented.** In ureq v3, `call()` returns `Err(ureq::Error)` for both network errors and HTTP errors (4xx, 5xx). The `ureq::Error` enum has variants `ureq::Error::Status(u16, Response)` for HTTP errors and `ureq::Error::Io(...)` for network errors. An agent that only pattern-matches on `?` propagation will convert all errors to `anyhow::Error` and lose the HTTP status code needed for the 401/403/404/429 handling described in the spec. The spec mentions "respect `Retry-After` headers on 429" but does not show how to extract headers from a ureq v3 error response. The correct pattern:

```rust
match agent.get(&url).call() {
    Ok(response) => { /* happy path */ }
    Err(ureq::Error::Status(429, response)) => {
        let retry_after = response.header("Retry-After")...;
    }
    Err(ureq::Error::Status(code, response)) => { /* other HTTP errors */ }
    Err(e) => { /* network error */ }
}
```

This pattern is not shown anywhere in the docs. An agent using `?` throughout will lose the ability to read error bodies for JIRA's `{"errorMessages": [...], "errors": {...}}` JSON, which is required for the blocking error modal content.

**c) ureq v3 `Agent` construction.** The spec shows `let client = ureq::agent()` which is the v2 API. In ureq v3 it is `ureq::Agent::new_with_config(...)` or `ureq::Agent::new()`. The spec's example code will not compile against ureq v3.

**d) `base64` crate usage.** The spec correctly shows `base64::engine::general_purpose::STANDARD.encode(...)` which matches base64 v0.22. This is correct.

### 4. Threading Model

**Verdict: PASS**

The threading design is thoroughly specified:
- `mpsc::channel` with `Sender<JiraCommand>` / `Receiver<JiraResult>`
- `AtomicBool` shutdown flag with `Ordering::Relaxed` (acceptable for this use case)
- `recv_timeout(Duration::from_millis(100))` polling loop in the background thread
- `while let Ok(result) = try_recv()` drain pattern in `on_tick()`
- `JoinHandle::is_finished()` respawn guard on `on_enter()`
- `TryRecvError::Disconnected` → thread panicked, show reconnect prompt
- Generation counter (`u64`) to prevent stale overwrites

The `AtomicBool` OR `JiraCommand::Shutdown` dual-shutdown mechanism is slightly redundant but both are documented clearly. An agent can implement either or both.

The `JiraCommand::Shutdown` variant appears in the enum but no code shows the background thread acting on it (vs. the `AtomicBool` check). This is minor — the spec text says "cooperative shutdown signal" for both, and the pseudocode loop checks the `AtomicBool`. An agent will implement this correctly.

One real concern: the spec says `JoinHandle::is_finished()` for the respawn guard. This requires storing the `JoinHandle` on the `JiraPlugin` struct. The `JiraPlugin` struct is never defined with its fields in the spec — only described narratively. An agent must infer the full struct definition from context. The required fields include: `command_tx: Option<mpsc::Sender<JiraCommand>>`, `result_rx: Option<mpsc::Receiver<JiraResult>>`, `shutdown: Arc<AtomicBool>`, `thread_handle: Option<JoinHandle<()>>`, `account_id: Option<String>`, `issues: Vec<JiraIssue>`, `loading: bool`, `board_state: BoardState`, `modal: Option<JiraModal>`, `generation: u64`, `last_refresh: Instant`, `refreshing: bool`, `stale: bool`, `config: JiraConfig`. None of this struct definition is written down. This is a **significant gap** for agents working on `jira/mod.rs`.

### 5. Error Handling

**Verdict: CONCERN**

The `JiraError` type appears throughout `JiraResult` variants (`TransitionFailed(String, JiraError)`, `Error(JiraError)`) but is never defined. The api-reference gives `JiraErrorResponse` as the deserialization struct for the API response body, but `JiraError` is a different type that the agent must define to carry context to the TUI thread. An agent must invent:

```rust
pub struct JiraError {
    pub status: Option<u16>,
    pub message: String,
}
```

or something equivalent. The type is used in the spec but never defined. This is a **concrete gap** — different agents will invent incompatible definitions.

The error handling strategy (blocking modal for user actions, toast for auto-refresh) is well-specified in `jira-plugin.md`. The HTTP status codes are listed. But the translation from `ureq::Error` to `JiraError` is not shown (see section 3b).

The `JiraErrorResponse.errors` is `HashMap<String, String>`. This is correct for field-level validation errors. However, the combine-into-display-string algorithm ("join `error_messages` + join `errors` values") is underspecified. Does the join use newlines? Semicolons? This matters for the blocking error modal. Minor.

### 6. Cargo.toml Dependencies

**Verdict: FAIL**

The spec says to add:
```toml
ureq = { version = "3", features = ["json"] }
serde_json = "1.0"
base64 = "0.22"
```

However:
- `serde` with `features = ["derive"]` is not in the current `Cargo.toml` and is not listed as a new dep. The JIRA models need `#[derive(Deserialize)]`. `serde_json` implies serde transitively, but the `derive` feature is not enabled transitively. An agent will get compile errors on `#[derive(Deserialize)]` unless `serde = { version = "1", features = ["derive"] }` is added explicitly.
- `serde_json` is not currently in `jm-tui/Cargo.toml`. It IS in `jm-core/Cargo.toml` (inferred from usage), but it is not available to `jm-tui` without being declared.
- There is no mention of whether `ureq` v3 requires any additional system dependencies (TLS). ureq v3's default TLS backend is `rustls`. This compiles without system dependencies, which is correct. Not a blocker but worth noting for CI environments.
- The `std::sync::atomic` types and `std::thread` and `std::sync::Arc` are from std — no extra crates needed. This is fine.

**Critical gap**: `serde` with `derive` feature is missing from the listed new dependencies.

---

## Gaps Found

Ranked by severity:

### CRITICAL — would cause compile failure or runtime panic

1. **`JiraPlugin` struct fields not defined anywhere.** The struct is referenced throughout all specs but never laid out with typed fields. Agents working on `jira/mod.rs` must guess all field names and types from narrative prose. High risk of divergent implementations between agents working on different files.

2. **`JiraError` type never defined.** Used in `JiraResult::TransitionFailed(String, JiraError)` and `JiraResult::Error(JiraError)`. Agents must independently invent this type, risking incompatible definitions between `api.rs` and `mod.rs`.

3. **`serde` derive feature missing from dependency list.** Without `serde = { version = "1", features = ["derive"] }` in `jm-tui/Cargo.toml`, all `#[derive(Deserialize)]` annotations on JIRA models will fail to compile.

4. **ureq v3 API mismatch in code sample.** `ureq::agent()` is the v2 constructor; v3 uses `ureq::Agent::new()`. The auth code sample also omits the body-reading step after `.call()?`. Code copied from this sample will not compile against ureq v3.

### HIGH — would cause silent data loss or runtime errors at non-trivial inputs

5. **No Rust struct for dynamic `fields` map in search response.** The `issues[*].fields` object cannot be a typed struct because story points and sprint use dynamic field IDs. The spec gives no guidance on using `serde_json::Value` for the outer struct. Agents are likely to write a typed struct that silently drops custom field data.

6. **ureq v3 error handling pattern not shown.** `call()` on 4xx/5xx returns `Err(ureq::Error::Status(code, response))`. Without this pattern, agents cannot extract HTTP status codes for error categorization, cannot read JIRA error response bodies for the blocking modal, and cannot check `Retry-After` headers for 429 handling.

7. **`JiraConfig` uses `serde_yml::Value` extraction path in `PluginRegistry::new` — but the registry never does this.** The architecture doc specifies config extraction via `config.extra.get("jira")`, but the actual `PluginRegistry::new` in `registry.rs` does not have an `extra` field lookup — it needs to be added. Agents must reconcile the spec with the actual `Config` struct. If `Config` does not have an `extra: HashMap<String, serde_yml::Value>` field, the whole config extraction pattern breaks.

### MEDIUM — would require design decisions not answerable from the spec

8. **Sprint deserialization struct not given.** Agent must invent the `SprintValue` struct from the JSON example.

9. **`StatusCategory` enum deserialization strategy (`#[serde(other)]` usage) not concretely shown.** Agent must know how to implement `#[serde(other)]` on a non-unit variant or use a `TryFrom<String>` approach.

10. **`text_to_adf` in `jira-plugin.md` (single-paragraph) conflicts with the more complete multi-paragraph version in `jira-api-reference.md`.** Both are named `text_to_adf`. An agent reading only one doc will implement differently from one reading both.

11. **`JiraModal` enum variants** (used in plugin-architecture.md as an example) are sketched but not finalized. `IssueDetail(Issue)`, `CreateIssue`, `ConfirmTransition(Issue, String)` are shown but the full set needed for Phase 1 (transition picker, required fields form, creation wizard, error modal) is not enumerated. Agents will invent different variant names.

---

## Final Verdict

**REJECT**

The spec set is strong on architecture, lifecycle, keybindings, UI layout, and endpoint-level API details. The implemented Phase 0 code is solid and provides a correct foundation. However, the spec cannot be handed to agents in its current state because of four issues that will cause compile failures or non-trivially incorrect behavior:

1. The `JiraPlugin` struct definition is entirely absent — the most-touched file in the implementation (`jira/mod.rs`) has no typed blueprint.
2. `JiraError` is referenced but never defined, guaranteeing inter-agent type incompatibility.
3. The ureq v3 API surface in the code examples is wrong (constructor + missing body-read step), which will produce non-compiling code that is hard to debug without knowing the ureq v3 changelog.
4. `serde` with `derive` feature is missing from the dependency additions, causing every `#[derive(Deserialize)]` to fail.

**Minimum fixes required before approving for agent handoff:**

- Add a complete `JiraPlugin` struct definition with all field types to `jira-plugin.md` or a new `jira-struct-reference.md`.
- Define `JiraError` (with `status: Option<u16>` and `message: String` fields minimum).
- Fix the ureq v3 code examples: correct constructor (`Agent::new()`), show the body-read pattern (`response.into_json::<T>()?`), and show the error-match pattern for 4xx/5xx.
- Add `serde = { version = "1", features = ["derive"] }` to the dependency list.
- Clarify the `Config.extra` / `serde_yml::Value` extraction path or show the actual `Config` struct that agents must work against.
