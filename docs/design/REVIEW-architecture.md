# Architecture Review: Multi-Phase Plugin System

**Reviewer role**: Senior software architect, no bias toward approval.
**Documents reviewed**: plugin-architecture.md, plugin-system-rewrite.md, jira-plugin.md,
jira-api-reference.md, form-modal-spec.md, horizontal-scroll-spec.md
**Code reviewed**: plugins/mod.rs, plugins/registry.rs, app.rs (Plugin references), events.rs

**Note**: This review post-dates several prior adversarial Q&A documents (QUESTIONS-architecture-1.md,
QUESTIONS-architecture-2.md, QUESTIONS-jira-1.md, QUESTIONS-jira-2.md). Those documents identified
many real problems. This review assesses whether the *final, revised* specifications actually resolved
them, and finds new gaps those reviews did not catch.

---

## Assessment

### 1. Cross-Document Consistency

**PASS** (with one minor discrepancy)

Trait signatures are consistent across all six documents. `ScreenPlugin`, `SidebarPlugin`, and
`PluginAction` are defined identically in `plugin-architecture.md` and `plugin-system-rewrite.md`.
The JIRA plugin doc defers to the architecture doc for traits and does not redefine them.

The one discrepancy: `plugin-architecture.md` (line ~313) describes a registry method named
`tick_active_screen(active_screen: &ScreenId)`. The implemented code and `plugin-system-rewrite.md`
both use `tick_screen(name: &str)`. This is a naming inconsistency in the reference document only;
the implementation is self-consistent.

The `key_hints()` return type is `Vec<(&str, &str)>` in `plugin-architecture.md` (without `'static`)
and `Vec<(&'static str, &'static str)>` in `plugin-system-rewrite.md` and the implemented code.
The implementation uses `'static`, which is correct. The architecture doc is stale on this point.

Keybinding tables are consistent: `J` (uppercase) opens JIRA across all documents.

---

### 2. Phase 0 to Phase 1 Bridge

**PASS**

Phase 0 is fully implemented and verified. The implemented code in `plugins/mod.rs` and
`plugins/registry.rs` exactly matches the `plugin-system-rewrite.md` specification:

- `SidebarPlugin` and `ScreenPlugin` are independent traits with no supertrait.
- `PluginAction { None, Back, Toast(String) }` derives `Debug, Clone, PartialEq, Eq`.
- `PluginRegistry { sidebar: PluginSidebar, screens: Vec<Box<dyn ScreenPlugin>> }`.
- `tick_screen()` gates on `needs_timer()`, returns `Vec<String>`.
- `AboutPlugin` is always registered unconditionally.

The `ScreenPlugin` trait is sufficient for Phase 1. The JIRA plugin needs:

- `on_enter()` to spawn the background thread — provided.
- `on_leave()` to signal thread shutdown — provided.
- `on_tick()` at 250ms to drain the result channel — provided.
- `handle_key()` returning `PluginAction` for back/toast — provided.
- `render()` with `Frame` for full-screen drawing — provided.

The `PluginAction` enum has exactly three variants. This is architecturally intentional:
the JIRA plugin handles all internal state transitions (modal open/close, issue selection,
transition picker, form editing) entirely within `handle_key()` and renders all overlays
within `render()`. No additional `PluginAction` variants are needed for Phase 1.

The `Action::OpenPlugin(String)` variant is present in `events.rs` and correctly handled in
`app.rs` (calls `on_enter()`, sets `ScreenId::Plugin(name)`).

---

### 3. Data Flow Completeness

**CONCERN**

The key-to-UI-update data flow is specified in enough fragments to be reconstructable, but it is
never written end-to-end in a single document. An implementing agent must mentally assemble the
chain from multiple documents. This is a documentation quality issue, not a missing specification,
but it creates implementation risk.

The full chain:

```
user key press
  -> app.rs: handle_key() matches ScreenId::Plugin(name)
  -> plugin.handle_key(key) -> PluginAction
     -> internally: plugin state machine updates (e.g., modal opens, cursor moves)
     -> if write operation: JiraCommand sent via mpsc::Sender to background thread
  -> background thread (ureq::Agent): executes API call
     -> result sent via mpsc::Sender<JiraResult> to TUI thread
  -> app.rs: on_tick() fires every 250ms (screen tick)
     -> plugin_registry.tick_screen(name) calls plugin.on_tick()
     -> plugin.on_tick(): `while let Ok(result) = self.result_rx.try_recv()`
     -> match result: mutate plugin state (board data, modal data, loading flag)
     -> return Vec<String> (notification messages)
  -> app.rs: forwards notifications to plugin_registry.sidebar.push_notification()
  -> next render frame: plugin.render() reads updated state, displays it
```

Every step of this chain is documented somewhere. The gaps are in error handling within
`on_tick()` (what happens to the borrow of `result_rx` when the plugin also holds mutable state?
Resolved by Rust's ownership: `self.result_rx` and `self.board` are different fields).

The 250ms tick rate is documented in `plugin-architecture.md` and `plugin-system-rewrite.md`.
The tick rate in the app event loop (`event::poll(Duration::from_millis(100))` with a
`last_screen_tick` guard at 250ms) matches. The channel drain pattern (`while let Ok(result) =
try_recv()`) is specified in `plugin-architecture.md`, `jira-plugin.md`, and `jira-api-reference.md`.

**One gap**: `on_tick()` is the *only* place the result channel is drained. If the user presses
`s` on a card and the transition completes in 80ms (before the next 250ms tick fires), the
optimistic UI update has already happened, but the board data update waits up to 250ms for the
tick. This is acceptable latency, but it is worth documenting explicitly in `jira/mod.rs`.

**Second gap**: The `$EDITOR` flow for comments suspends the TUI. The specification (jira-plugin.md
lines ~527-539) says the plugin calls the existing editor launch code at `app.rs:167-196`. But
`ScreenPlugin::handle_key()` returns `PluginAction`, not `Action`. There is no `PluginAction::OpenEditor`
variant, and the plugin does not have access to the app's editor launch code. This is a concrete
missing integration point.

The existing editor launch in `app.rs` is a method on `App`, not exposed as a utility function.
To open `$EDITOR` from a screen plugin, the plugin must either:
(a) implement editor launch itself (spawn `$EDITOR` directly, suspend terminal, restore terminal),
(b) return a new `PluginAction::OpenEditor { ... }` variant that the app handles, or
(c) `PluginAction` gains a `SuspendTui(TempFilePath)` variant.

None of these approaches are specified. This affects Phase 1c (comment input).

---

### 4. State Machine Coverage

**CONCERN**

The JIRA plugin state machine is described in prose and modal mockups across `jira-plugin.md`,
`form-modal-spec.md`, and `horizontal-scroll-spec.md`. No single document defines the complete
top-level state machine for `JiraPlugin`. An agent implementing `jira/mod.rs` must infer the
top-level state machine from scattered descriptions.

**Derivable top-level states** (reconstructed from the docs):

```
JiraPluginState {
    Loading,
    Board { board_state: BoardState, show_done: bool, project_filter: Option<String> },
    DetailModal { issue_key: String, detail_data: Option<IssueDetail>, editmeta: Option<...> },
    TransitionPicker { issue_key: String, transitions: Vec<JiraTransition>, cursor: usize },
    TransitionFieldForm { issue_key: String, transition: JiraTransition, form: FormState },
    CreateFlowProjectSelect { cursor: usize },
    CreateFlowTypeSelect { project_key: String, cursor: usize },
    CreateFlowForm { project_key: String, issue_type_id: String, form: FormState },
    ErrorModal { message: String, context: ErrorContext },
}
```

**Missing from the spec**:

- What happens if the user presses `n` to create an issue while a `DetailModal` is open? Is
  `DetailModal` superseded, or does `n` do nothing while a modal is open?
- What is the state when `$EDITOR` returns after a comment? There is no "CommentEditorOpen"
  state — the editor blocks the TUI thread, so this is implicit. But the spec does not say what
  plugin state is maintained during the editor session, or how the plugin knows the comment
  text when control returns.
- `ErrorModal` vs. board-level error: The spec distinguishes "blocking error modal" from
  "non-blocking toast" but does not specify whether an `ErrorModal` state stacks on top of the
  current state or replaces it. If the error fires during `DetailModal`, does dismissing the
  error return to `DetailModal` or to `Board`? The spec does not say.
- Detail modal field navigation: the spec says `j/k` navigates fields and `e` edits the focused
  field. But when a `SelectOpen` dropdown fires inside the detail modal (user presses `e` on an
  editable select field), is this a `FormState::SelectOpen` (from form-modal-spec.md) embedded
  inside the detail modal? Or a different mechanism? The detail modal is not said to use `FormState`.

The `FormState` machine in `form-modal-spec.md` is well-specified for issue creation.
`horizontal-scroll-spec.md` specifies `BoardState` well. The detail modal state machine is
underspecified — `jira-plugin.md` describes the UI but not the edit-state machine for individual
field editing from the detail view.

---

### 5. Concurrency

**PASS** (prior blockers resolved in current docs)

The current specifications address all three original concurrency blockers:

- **Thread respawn guard**: `on_enter()` checks `JoinHandle::is_finished()` before spawning.
  Specified in `jira-plugin.md` (Phase 1a section) and `plugin-architecture.md`.
- **Cooperative shutdown**: `AtomicBool` shared flag OR `JiraCommand::Shutdown` variant.
  Both are specified.
- **Channel drain**: `while let Ok(result) = try_recv()` is specified in `plugin-architecture.md`
  and `jira-plugin.md`.
- **Generation counter**: `FetchMyIssues { generation: u64 }` with result filtering on mismatch.
  Specified.
- **Panic detection**: `TryRecvError::Disconnected` check specified. Shows reconnect prompt.
- **Post-write delay**: 500ms delay before post-write refresh for JIRA eventual consistency.
  Specified.
- **Refresh deduplication**: `refreshing: bool` flag prevents overlapping refreshes. Specified.

One remaining gap: the `AtomicBool` shutdown flag is mentioned but the join strategy on
`on_leave()` is not specified. Does `on_leave()` join the thread (blocking the TUI for up to
the current request timeout), detach (thread becomes a zombie), or use a timeout-and-detach
approach? This matters for the user experience when rapidly entering and leaving the JIRA screen.
The spec says "thread stopped on `on_leave()`" without specifying the mechanism.

For `ureq` with a synchronous blocking HTTP call in-flight, setting `AtomicBool` does not
interrupt the current request. The thread will only see the flag after the current `recv_timeout`
call returns. If a request takes 10-30 seconds (slow network), `on_leave()` returns immediately
but the background thread is still alive for up to 30s. A new `on_enter()` will spawn a second
thread (the `is_finished()` guard will return `false` because it is not finished). This is a
resource leak. The spec does not address it.

**Recommended addition**: Specify that `on_leave()` sets the shutdown flag and does NOT join.
On `on_enter()`, if `is_finished()` is false, do not spawn a new thread — reuse the existing
one (the old thread will see the shutdown flag and exit after its current request). Send a
`FetchMyIssues` command to reinitiate data loading. This avoids both blocking and thread leaks.

---

### 6. Config Integration

**FAIL**

This is the most concrete blocking gap between the design documents and the implemented code.

The `plugin-architecture.md` describes a `serde(flatten)` approach:

```rust
#[derive(Deserialize)]
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub pomodoro: Option<PomodoroConfig>,
    pub notifications: Option<NotificationsConfig>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yml::Value>,
}
```

The actually implemented `PluginConfig` in `crates/jm-core/src/config.rs` is:

```rust
pub struct PluginConfig {
    pub enabled: Vec<String>,
    pub notifications: NotificationsConfig,  // NOT Option
    pub pomodoro: PomodoroConfig,            // NOT Option
    // NO extra field
    // NO serde(flatten)
}
```

There is no `extra: HashMap<String, serde_yml::Value>` field. There is no mechanism for the
JIRA plugin (or any future plugin) to read its config from `config.yaml`. The `PluginRegistry::new()`
only receives `&Config`, which contains no `jira` config data.

This means:

1. If `jira.url`, `jira.email`, etc. are present in `~/.jm/config.yaml`, they are silently
   discarded by the serde deserializer (unknown fields are ignored by default with `serde_yml`).
2. `JiraPlugin::new()` has no `JiraConfig` to receive.
3. The entire config path described in `plugin-architecture.md` and `jira-plugin.md` does not
   exist in the codebase.

**What Phase 1 must do** (and is not currently specified as a Phase 1 task):

Either (a) add `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig`
in `jm-core/src/config.rs`, OR (b) add `pub jira: Option<JiraConfig>` as a named field. The
`serde(flatten)` approach avoids modifying `jm-core` for each new plugin. The named-field approach
is simpler and more discoverable.

The `plugin-system-rewrite.md` (Phase 0 out-of-scope list, item 7) says: "Config parsing for
screen plugins (deferred to Phase 1)". But Phase 1 specifications (`jira-plugin.md`) do not
include a task for modifying `config.rs` in `jm-core`. This is a missing task in the Phase 1
plan. An implementing agent will discover this gap during Phase 1a when trying to call
`JiraPlugin::new(jira_config)`.

---

### 7. File Structure

**PASS**

The proposed `jira/` module structure is:

```
plugins/jira/
├── mod.rs      — JiraPlugin struct implementing ScreenPlugin
├── api.rs      — background thread + ureq client
├── models.rs   — data types
├── board.rs    — kanban board rendering
├── detail.rs   — issue detail modal rendering
├── create.rs   — creation flow
├── adf.rs      — ADF conversion
└── config.rs   — JiraConfig struct
```

This is clear and complete. Responsibilities are properly separated:
- `api.rs` owns the background thread, channel types, and HTTP calls.
- `mod.rs` owns the plugin state machine and coordinates between modules.
- `board.rs`, `detail.rs`, `create.rs` own rendering of their respective views.
- `adf.rs` is a pure conversion utility with no state.
- `config.rs` is isolated so changes to JIRA config don't touch other files.

The `TransitionField` and `AllowedValue` types appear in `jira-api-reference.md` (endpoint 4)
but are not listed in `models.rs` — they need to be added. The `CreateMetaResponse` type
referenced in `JiraResult::CreateMeta(CreateMetaResponse)` is not defined in `models.rs`
or anywhere else. `jira-api-reference.md` provides the response shape for endpoint 11, which
is sufficient for an agent to define the struct, but it must be explicitly added to the
file structure plan.

---

### 8. Missing Specifications

**CONCERN — multiple items**

Items that an implementing agent needs but no document fully covers:

**8.1 `$EDITOR` integration from a ScreenPlugin**
As noted in section 3: there is no specified mechanism for a `ScreenPlugin` to suspend the TUI
and open `$EDITOR`. The existing editor launch code is a method on `App` and returns
`Action::OpenEditor`. Neither `PluginAction` nor `PluginRegistry` has a corresponding mechanism.
This must be resolved before Phase 1c (comment input). Options:

- Add `PluginAction::SuspendForEditor { temp_path: PathBuf, callback_tag: String }` so `app.rs`
  handles the TUI suspension and file writing, then calls back into the plugin with the result.
- Add a utility function `launch_editor(path: &Path) -> io::Result<()>` to `jm-tui` that any
  code (including plugins) can call directly, taking responsibility for suspending the terminal.
- Require the plugin to manage terminal suspension itself (restore raw mode, etc.).

None of these is specified. The third option is most consistent with the "self-contained plugin"
principle but requires documenting the terminal state protocol the plugin must implement.

**8.2 Project list source for issue creation**
The issue creation flow starts with "Select Project". Where does this list come from?
`jira-plugin.md` says "only projects with existing assignments shown — project list derived from
assigned issues." This means the project list is derived from `Vec<JiraIssue>` by unique
`project_key`/`project_name` pairs. This is implied but not stated as a data derivation rule,
and no data structure in the state machine holds `Vec<(String, String)>` (project_key, project_name).
The state machine for the creation flow needs this list.

**8.3 Detail modal edit behavior is underspecified**
The detail modal allows editing editable fields via `e`. The spec shows inline editing in the
creation form (via `FormState`). Is the detail modal edit experience also backed by `FormState`?
Or is it a simpler single-field edit? The creation form spec explicitly says it is reused for
transition required fields, but does NOT say it is reused for detail modal field editing.
If the detail modal uses its own edit mechanism, that mechanism is entirely undocumented.

**8.4 Transition required-field form submit key**
`form-modal-spec.md` says the form for transition fields uses `Enter` (not `S`) to submit:
"Submit key is `Enter` (not `S`) — there's usually only 1-2 fields." This is inconsistent with
`FormState::Navigating` where `Enter` moves to edit mode for the focused field. If `Enter`
submits for transitions but enters edit mode for creation, the same `FormState` machine has
context-dependent key behavior. This is a correctness hazard: the form widget must know whether
it is in "creation mode" (Enter = edit, S = submit) or "transition mode" (Enter = submit).
This dual-mode behavior needs to be explicitly specified in the state machine, not just in a
prose note.

**8.5 `CreateMetaResponse` type is undefined**
`JiraResult::CreateMeta(CreateMetaResponse)` appears in `jira-plugin.md` but `CreateMetaResponse`
is never defined. `jira-api-reference.md` (endpoint 11) provides the JSON shape. An agent can
derive the struct, but the authoritative definition should be in `models.rs`. Gap: the mapping
from `createmeta` response to `Vec<EditableField>` (the same type used by editmeta) needs to
be specified, since the field key name differs (`fieldId` vs map key) and the wrapper key is
`values` not `fields`.

**8.6 Rate limiting behavior for blocking calls**
The rate limiting spec says: "Respect `Retry-After` headers on 429 responses." With `ureq`,
a 429 response is returned as a normal `ureq::Response`. The background thread must parse the
`Retry-After` header, `std::thread::sleep` for the specified duration (blocking the background
thread), then retry. During this sleep, no commands from the TUI thread are processed. If the
user sends commands during the sleep, they queue in the `JiraCommand` channel. This is
acceptable but undocumented. Alternatively, the thread could send a `JiraResult::RateLimited`
notification, return to the command loop, and let a timer in `on_tick()` schedule the retry.
The second approach is more responsive but more complex. The spec must choose.

---

## Gaps Found

### Blocking Implementation Gaps

**GAP-1: Config integration path is not implemented**
`PluginConfig` in `jm-core/src/config.rs` has no `extra` field or `serde(flatten)`.
`JiraConfig` cannot be loaded from `~/.jm/config.yaml` with the current code.
`PluginRegistry::new()` does not instantiate `JiraPlugin`.
This is Phase 1a task 0 that no document assigns.

**GAP-2: `$EDITOR` invocation from ScreenPlugin is unspecified**
No mechanism exists for a screen plugin to suspend the TUI and open `$EDITOR`.
The existing app-level editor launch (`app.rs:167-196`) is not accessible from a `ScreenPlugin`.
This blocks Phase 1c (comment input).

### Specification Completeness Gaps

**GAP-3: `CreateMetaResponse` struct is undefined**
Referenced in `JiraResult` but never defined. Mapping from paginated `createmeta` response to
`Vec<EditableField>` is not specified.

**GAP-4: Detail modal field edit mechanism is unspecified**
Does inline field editing in the detail modal use `FormState`? If yes, how? If no, what
mechanism? The creation form spec does not claim to cover the detail modal edit path.

**GAP-5: `FormState` submit-key duality is unresolved**
`Enter` means "edit focused field" in creation mode and "submit form" in transition-fields mode.
This dual behavior is mentioned in prose but not reflected in the `FormState` state machine.
The machine needs either a mode flag or separate states.

**GAP-6: Thread shutdown join strategy is unspecified**
`on_leave()` sets the shutdown flag but blocking join vs. detach vs. timeout-and-detach is not
specified. With synchronous `ureq` calls that cannot be interrupted, this has material impact
on resource usage when the user rapidly enters and leaves the JIRA screen.

### Minor Gaps

**GAP-7: Top-level `JiraPlugin` state machine is not documented in one place**
Derivable from the docs but not written down. Phase 1a task should include writing this
explicitly in `jira/mod.rs` as a documented enum.

**GAP-8: Project list for creation flow has no named data structure**
Implied to be derived from `Vec<JiraIssue>` but no state field is specified.

**GAP-9: Error modal stacking behavior on modal-over-modal is unspecified**
Does `ErrorModal` return to the previous modal state on dismiss, or always to Board?

**GAP-10: Rate limiting retry strategy (blocking vs. event-driven) is unspecified**
Both are implementable; the choice affects responsiveness during rate-limit backoff.

---

## Final Verdict

**REJECT — with clear path to conditional approval**

The Phase 0 implementation is correct and fully matches its specification. The trait design,
registry, lifecycle hooks, and navigation integration are coherent and well-implemented.
No issues in Phase 0.

Phase 1 cannot be handed to an implementing agent in its current state due to two blocking
gaps and one complete absence of implementation groundwork:

1. **GAP-1 (Config)** is a practical blocker: the agent cannot wire `JiraPlugin::new(config)` into
   `PluginRegistry::new()` without modifying `jm-core/src/config.rs`. This modification is not
   mentioned anywhere in the Phase 1 specifications. It is a missing task.

2. **GAP-2 (`$EDITOR`)** blocks Phase 1c comment input entirely. The spec says to use `$EDITOR`
   but provides no mechanism for a self-contained `ScreenPlugin` to do so. The agent will hit
   this during Phase 1c and have to invent a solution.

3. **GAP-5 (FormState duality)** is a correctness hazard that will produce a subtle bug: pressing
   `Enter` when "navigating" in a transition-required-fields form will edit the field (wrong)
   instead of submitting (correct), unless the form is explicitly told its submit key.

The other gaps (3, 4, 6, 7, 8, 9, 10) are significant but individually manageable by a
senior-level implementing agent making reasonable design decisions. They represent documentation
gaps rather than architectural incoherence.

**To reach APPROVE**, the following must be added to the Phase 1 spec before implementation begins:

- [ ] Add Phase 1a Task 0: modify `jm-core/src/config.rs` to add `pub jira: Option<JiraConfig>`
  to `PluginConfig`. Add corresponding `PluginRegistry::new()` logic to instantiate
  `JiraPlugin` when `config.plugins.jira.is_some()` and "jira" is in `enabled`.
- [ ] Specify the `$EDITOR` invocation mechanism for `ScreenPlugin` (add `PluginAction` variant
  or document direct terminal management).
- [ ] Add a `transition_mode: bool` (or equivalent) field to `FormState` controlling whether
  `Enter` submits or enters edit mode. Reflect this in the state machine diagram.
- [ ] Define `CreateMetaResponse` in `models.rs` spec with field-level mapping to `EditableField`.
- [ ] Specify `on_leave()` thread shutdown strategy (recommend: set flag, do NOT join, use
  `is_finished()` guard on `on_enter()` to reuse the in-flight thread rather than respawning).

The architectural foundations are sound. The trait design is clean, the Phase 0 bridge works,
and the concurrency model is well-thought-out. These are specification completeness issues in
Phase 1, not architectural incoherence.
