# Team Plan: Parallel Implementation of Plugin Rewrite + JIRA Plugin

## 1. Agent Roles

### Agent A: "Trait Architect" (Plugin System Core)

**Responsibility:** Define the new trait hierarchy, build `PluginRegistry`, and wire screen plugins into `app.rs`. This agent owns the structural spine that all other agents depend on.

**Exclusive write access:**
- `crates/jm-tui/src/plugins/mod.rs` (trait definitions, `PluginAction` enum)
- `crates/jm-tui/src/plugins/registry.rs` (new file)
- `crates/jm-tui/src/app.rs` (all match-site additions, `plugin_registry` rename, lifecycle wiring)
- `crates/jm-tui/src/events.rs` (`ScreenId::Plugin(String)`, `Action::OpenPlugin(String)`)
- `crates/jm-tui/src/keyhints.rs` (`ScreenId::Plugin` arm)

**Dependencies from other agents:** None. This is the root of the dependency graph.

**Start time:** Immediately (Day 1).

---

### Agent B: "Sidebar Migrator" (Existing Plugin Migration)

**Responsibility:** Migrate the three existing sidebar plugins from `impl Plugin` to `impl SidebarPlugin`, refactor `PluginSidebar` to accept `Vec<Box<dyn SidebarPlugin>>`, and create the `AboutPlugin` demo screen plugin.

**Exclusive write access:**
- `crates/jm-tui/src/plugins/clock.rs`
- `crates/jm-tui/src/plugins/notifications.rs`
- `crates/jm-tui/src/plugins/pomodoro.rs`
- `crates/jm-tui/src/plugins/sidebar.rs`
- `crates/jm-tui/src/plugins/about.rs` (new file)

**Dependencies from other agents:**
- Needs Agent A's `SidebarPlugin` and `ScreenPlugin` trait definitions from `mod.rs` (Tasks 1).
- Needs Agent A's `PluginAction` enum definition.

**Start time:** After Agent A completes Task 1 (trait definitions). Agent B can begin preparing the migration pattern immediately by reading the current code, but cannot compile until the traits exist. In practice, Agent A's Task 1 is small (30 minutes), so Agent B starts ~30 minutes into Day 1.

---

### Agent C: "Proptest Fixer" (Test Gate Unblocking)

**Responsibility:** Fix the pre-existing `prop_project_name_with_yaml_special_chars` proptest failure. This is entirely in `jm-core` and has zero overlap with the plugin work.

**Exclusive write access:**
- `crates/jm-core/src/models/project.rs` (specifically the `yaml_string()` function and/or `from_markdown` parsing)
- `crates/jm-core/tests/proptest_roundtrip.rs` (if the test strategy needs tightening)

**Dependencies from other agents:** None. Completely independent.

**Start time:** Immediately (Day 1). Can run fully in parallel with all other agents.

---

### Agent D: "JIRA Data Layer" (API Client, Models, Config)

**Responsibility:** Build the JIRA plugin's data layer: config parsing, data models, ADF conversion, and the background-thread API client. These are all new files in the `jira/` module directory with no dependencies on existing TUI code beyond the `ScreenPlugin` trait signature.

**Exclusive write access:**
- `crates/jm-tui/src/plugins/jira/` (entire new directory)
  - `config.rs`
  - `models.rs`
  - `adf.rs`
  - `api.rs`
- `crates/jm-tui/Cargo.toml` (adding `ureq`, `serde_json`, `base64` dependencies)
- `crates/jm-core/src/config.rs` (adding `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig`)

**Dependencies from other agents:**
- Needs Agent A's `ScreenPlugin` trait and `PluginAction` enum from `mod.rs` (for the `impl ScreenPlugin` block in `jira/mod.rs` -- but Agent D can stub this and defer the impl).
- The models, api, adf, and config files have **zero** dependency on the trait system and can be built from Day 1.

**Start time:** Immediately for `models.rs`, `config.rs`, `adf.rs`, `api.rs`. The `jira/mod.rs` file (which implements `ScreenPlugin`) waits until Agent A's Task 1 is done.

---

### Agent E: "JIRA UI Layer" (Board, Detail, Create Screens)

**Responsibility:** Build the JIRA plugin's TUI rendering: kanban board, issue detail modal, transition picker, creation flow, and error modals. These are rendering-only files that depend on the models from Agent D.

**Exclusive write access:**
- `crates/jm-tui/src/plugins/jira/board.rs`
- `crates/jm-tui/src/plugins/jira/detail.rs`
- `crates/jm-tui/src/plugins/jira/create.rs`
- `crates/jm-tui/src/plugins/jira/mod.rs` (shared with Agent D for the struct definition -- see Integration Points)

**Dependencies from other agents:**
- Needs Agent D's `models.rs` types (`JiraIssue`, `JiraStatus`, `StatusCategory`, `JiraTransition`, `EditableField`, etc.)
- Needs Agent D's `api.rs` command/result enums (`JiraCommand`, `JiraResult`) for understanding what data is available.
- Needs Agent A's `ScreenPlugin` trait for `jira/mod.rs`.

**Start time:** Can begin `board.rs` rendering logic on Day 1 using stub types (copy the struct definitions from the design doc). Full integration after Agent D delivers `models.rs` (likely Day 1-2). The `jira/mod.rs` state machine (handle_key, render dispatch) starts after both Agent A and Agent D have their core pieces done.

---

## 2. Dependency Graph

```
Day 1 Start
  |
  +---> Agent C: Fix proptest (independent) -----> DONE (no downstream deps)
  |
  +---> Agent A: Task 1 (traits + PluginAction) ---+
  |     [~30 min]                                   |
  |                                                 |
  |     +-------------------------------------------+
  |     |                    |
  |     v                    v
  |   Agent B: Migrate     Agent A continues:
  |   sidebar plugins      Task 7-9 (ScreenId,
  |   + AboutPlugin        Action, match arms)
  |     |                    |
  |     v                    v
  |   Agent B: Done        Agent A: Tasks 10-15
  |   (sidebar works)      (app.rs wiring)
  |                          |
  |                          v
  +---> Agent D: models,   Agent A: Tasks 16-18
  |     config, adf,       (AboutPlugin registration,
  |     api.rs             J keybinding)
  |     [starts Day 1]       |
  |       |                  v
  |       |              ** MERGE POINT 1: Phase 0 Complete **
  |       |              (Agent A + B + C all done)
  |       |
  |       v
  |     Agent D: jira/mod.rs (impl ScreenPlugin)
  |     [needs Agent A traits]
  |       |
  |       v
  +---> Agent E: board.rs, detail.rs, create.rs
        [needs Agent D models]
          |
          v
        ** MERGE POINT 2: Phase 1a Complete **
        (Agent D + E integrate jira/mod.rs)
          |
          v
        Phase 1b-1e (sequential within JIRA plugin,
        but board/detail/create can parallelize)
          |
          v
        ** MERGE POINT 3: Phase 1 Complete **
```

### Critical Path

The longest sequential chain is:

```
Agent A: Task 1 (traits)
  -> Agent A: Tasks 7-15 (app.rs wiring, ScreenId, match arms)
    -> Agent A: Tasks 16-18 (AboutPlugin, J key, registration)
      -> Phase 0 gate (merge + test)
        -> Agent D: jira/mod.rs (impl ScreenPlugin)
          -> Agent E: integrate board/detail/create into mod.rs
            -> Phase 1 gate
```

**Estimated critical path: Agent A's work through Phase 0 gate (~1-2 days), then JIRA integration (~3-5 days).**

### Maximum Parallelism

At peak, all 5 agents can work simultaneously:
- Agent A: wiring app.rs
- Agent B: migrating sidebar plugins
- Agent C: fixing proptest
- Agent D: building JIRA data layer
- Agent E: building JIRA rendering (with stub types)

This is achievable from Day 1 with a 30-minute stagger for agents that need the trait definitions.

---

## 3. Phase 0 Work Breakdown

### Task-to-Agent Assignment

| Task | Agent | Can Start | Depends On | Est. Size |
|------|-------|-----------|------------|-----------|
| **T0**: Fix proptest failure | C | Day 1 | Nothing | Small |
| **T1**: Define SidebarPlugin + ScreenPlugin + PluginAction in mod.rs | A | Day 1 | Nothing | Small |
| **T2**: Migrate ClockPlugin to SidebarPlugin | B | After T1 | T1 | Small |
| **T3**: Migrate NotificationsPlugin to SidebarPlugin | B | After T1 | T1 | Small |
| **T4**: Migrate PomodoroPlugin to SidebarPlugin | B | After T1 | T1 | Small |
| **T5**: Refactor PluginSidebar to Vec<Box<dyn SidebarPlugin>> | B | After T1 | T1 | Medium |
| **T6**: Create PluginRegistry in registry.rs | A | After T1, T5 | T1 | Medium |
| **T7**: Add ScreenId::Plugin(String) to events.rs | A | After T1 | T1 | Small |
| **T8**: Add Plugin(_) wildcard arms to all 9+ match sites | A | After T7 | T7 | Medium |
| **T9**: Add Action::OpenPlugin(String) to events.rs | A | Day 1 | Nothing | Tiny |
| **T10**: Rename self.plugins to self.plugin_registry in app.rs | A | After T6 | T6 | Small |
| **T11**: Handle Action::OpenPlugin in update() | A | After T9, T10 | T9, T10 | Small |
| **T12**: Modify handle_back() for plugin on_leave() | A | After T8 | T8 | Small |
| **T13**: Wire screen plugin rendering | A | After T8, T10 | T8, T10 | Medium |
| **T14**: Wire screen plugin key handling | A | After T8, T10 | T8, T10 | Medium |
| **T15**: Wire key hints for screen plugins in keyhints.rs | A | After T7 | T7 | Small |
| **T16**: Implement AboutPlugin demo | B | After T1 | T1 | Small |
| **T17**: Register AboutPlugin in PluginRegistry | A | After T6, T16 | T6, T16 | Tiny |
| **T18**: Add J keybinding for AboutPlugin | A | After T9 | T9 | Tiny |
| **T19**: Write unit tests for PluginRegistry + AboutPlugin | A or B | After T17 | T17 | Small |
| **T20**: Run full test suite | All | After T0-T19 | All | N/A |
| **T21**: Manual testing | All | After T20 | T20 | N/A |

### Parallel Execution Timeline

```
Serial Step 1 (can all run in parallel):
  Agent A: T1 (traits), T9 (Action::OpenPlugin)
  Agent C: T0 (proptest fix)
  Agent D: begins models.rs, config.rs, adf.rs (Phase 1 early start)

Serial Step 2 (after T1):
  Agent A: T7 (ScreenId::Plugin)
  Agent B: T2, T3, T4 (migrate all 3 sidebar plugins -- parallel within B)

Serial Step 3 (after T7):
  Agent A: T8 (wildcard match arms)
  Agent B: T5 (refactor PluginSidebar), T16 (AboutPlugin)

Serial Step 4 (after T5, T8):
  Agent A: T6 (PluginRegistry)
  Agent B: done with sidebar work

Serial Step 5 (after T6):
  Agent A: T10 (rename), T11 (OpenPlugin handler), T12 (handle_back)

Serial Step 6 (after T10):
  Agent A: T13 (rendering), T14 (key handling), T15 (key hints)

Serial Step 7 (after T13, T14, T16):
  Agent A: T17 (register AboutPlugin), T18 (J keybinding)

Serial Step 8:
  Agent A or B: T19 (unit tests)
  All: T20 (full test suite), T21 (manual testing)
```

**Minimum serial steps: 8** (but many are small, total elapsed time ~1-2 days with focused agents).

---

## 4. Phase 1 Work Breakdown

### Sub-Phase to Agent Mapping

| Sub-Phase | Primary Agent | Secondary Agent | Notes |
|-----------|---------------|-----------------|-------|
| **1a: Foundation** | D (api, config, models) + E (board rendering) | A (registration in PluginRegistry) | D and E can parallelize: D builds data layer, E builds board rendering with stub data |
| **1b: Issue Interaction** | E (detail modal, transition picker) | D (transition API endpoints) | E builds the UI; D adds FetchTransitions/TransitionIssue to api.rs |
| **1c: Editing & Comments** | E (field editing UI, comment view) | D (editmeta API, comment API, ADF) | Agent D already built adf.rs early; E integrates it |
| **1d: Issue Creation** | E (create flow UI, form modal) | D (createmeta API, CreateIssue command) | E builds the multi-step form; D adds the API calls |
| **1e: Polish** | E (UI polish, edge cases) | D (rate limiting, error handling) | Both agents refine their respective layers |

### File-Level Parallelism (Phase 1a specifically)

These files can all be built simultaneously from Day 1:

```
Agent D (data layer):          Agent E (UI layer):
  models.rs  ────────────────>   board.rs (uses model types)
  config.rs                      detail.rs (uses model types)
  adf.rs                         create.rs (uses model types)
  api.rs  ───────────────────>   mod.rs (state machine uses api commands)
```

The arrow indicates a compile-time dependency. Agent E can begin with type stubs copied from the design doc and swap in real imports once Agent D's files compile.

### Phase 1 Parallel Tracks

```
Track 1 (Agent D):                    Track 2 (Agent E):
  models.rs (Day 1)                     board.rs with stubs (Day 1)
  config.rs (Day 1)
  adf.rs (Day 1)                        detail.rs with stubs (Day 2)
  api.rs background thread (Day 1-2)
  jira/mod.rs struct + ScreenPlugin     create.rs (Day 3)
    impl skeleton (Day 2)
                                      ** INTEGRATION: plug real types into
                                         board/detail/create (Day 2-3) **

  FetchTransitions endpoint (Day 3)     Transition picker modal (Day 3)
  EditMeta endpoint (Day 3)             Field editing UI (Day 4)
  Comment endpoints (Day 3)             Comment view/creation (Day 4)
  CreateMeta endpoint (Day 4)           Creation flow (Day 4-5)

  Rate limiting, retry (Day 5)          Polish, edge cases (Day 5)
```

---

## 5. Integration Points

### Pre-Work Agreements (Before Any Code)

These interfaces must be locked down before parallel work begins:

1. **`SidebarPlugin` trait signature** -- exact method names, argument types, return types. Agent A defines this in Task 1; Agents B relies on it.

2. **`ScreenPlugin` trait signature** -- specifically `render(&self, frame: &mut Frame, area: Rect)` and `handle_key(&mut self, key: KeyEvent) -> PluginAction`. Agent A defines; Agents D and E rely on it.

3. **`PluginAction` enum** -- the three variants (`None`, `Back`, `Toast(String)`). Must be agreed before Agent A wires app.rs and before Agent E writes handle_key logic.

4. **`ScreenId::Plugin(String)` variant** -- the exact variant shape. Agent A defines; the string parameter is the plugin's `name()` return value.

5. **`PluginRegistry` public API** -- `get_screen(&self, name: &str)`, `get_screen_mut(&mut self, name: &str)`, `tick_sidebar()`, `tick_screen(&mut self, name: &str)`. Agent A defines; future registration code depends on it.

6. **JIRA model types** -- `JiraIssue`, `JiraStatus`, `StatusCategory`, `JiraTransition`, `EditableField`, `JiraComment`, `JiraFieldDef`. Agent D defines in `models.rs`; Agent E consumes in `board.rs`, `detail.rs`, `create.rs`. The design doc already specifies these precisely.

7. **`JiraCommand` / `JiraResult` enums** -- Agent D defines in `api.rs`; Agent E's `jira/mod.rs` state machine sends commands and processes results.

8. **`PluginConfig.extra` field** -- Agent D adds `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig` in jm-core. This is a one-line change that must happen before the JIRA config can be deserialized.

### Compile-Time Integration Checks

The Rust compiler enforces most integration contracts:

- **Trait method signatures:** If Agent B implements `SidebarPlugin` with the wrong method signature, the compiler rejects it immediately.
- **`PluginAction` exhaustive matching:** If Agent A matches on `PluginAction` in `app.rs`, and Agent E returns a variant that doesn't exist, the compiler catches it.
- **`ScreenId` match exhaustiveness:** Any unhandled `ScreenId::Plugin(_)` arm produces a compiler error in every match block.
- **Type mismatches:** If Agent E uses `JiraIssue` fields that Agent D didn't define, compilation fails.
- **Missing imports:** If Agent D changes a type name in `models.rs`, all consumers in Agent E's files fail to compile.

### Manual Testing Checkpoints

1. **Phase 0 Gate (all agents sync):**
   - Launch TUI, verify sidebar plugins render correctly.
   - Press Tab to focus sidebar, navigate between plugins, verify Pomodoro starts/pauses.
   - Press J to open AboutPlugin. Verify full-screen render, sidebar hidden.
   - Press Esc to return. Verify sidebar reappears, dashboard state preserved.
   - Run `cargo test` -- all tests pass including proptest fix.
   - Run `./build-install.sh` to update installed binary.

2. **Phase 1a Gate (Agents D + E sync):**
   - Configure JIRA in `~/.jm/config.yaml` with real credentials.
   - Press J from dashboard. Verify loading state appears.
   - Verify kanban board populates with real JIRA issues.
   - Navigate h/j/k/l, verify column/row selection.
   - Press p to cycle project filter.
   - Press Esc to return to dashboard.

3. **Phase 1b Gate:**
   - Press Enter on an issue. Verify detail modal appears with correct data.
   - Press s on board or in detail. Verify transition picker with real JIRA workflows.
   - Execute a transition. Verify optimistic UI update and API confirmation.

4. **Phase 1c Gate:**
   - In detail modal, verify editable fields have `[e:edit]` hints.
   - Edit a text field. Verify API update succeeds.
   - Press c to add a comment. Verify $EDITOR opens, comment posts to JIRA.

5. **Phase 1d Gate:**
   - Press n on board. Verify project selection, issue type selection, form modal.
   - Fill required fields and submit. Verify issue created in JIRA.

---

## 6. Risk Assessment

### Most Likely to Block Others: Agent A

Agent A's work is the critical path. Every other agent depends on the trait definitions (Task 1), and the full Phase 0 gate depends on Agent A completing all 15+ tasks. If Agent A falls behind, Agents B, D, and E are all stalled.

**Mitigation:** Agent A's Task 1 (trait definitions) is small and well-specified by the design doc. Ship it within the first 30 minutes so other agents unblock immediately. The remaining tasks (app.rs wiring) are mechanical and well-documented in the design doc with exact code patterns.

### Most Likely Merge Conflicts: `crates/jm-tui/src/plugins/mod.rs`

This file is touched by:
- Agent A: replaces the `Plugin` trait with `SidebarPlugin` + `ScreenPlugin` + `PluginAction`, adds `mod about;`, `mod registry;`, `mod jira;`
- Agent B: depends on the new trait being in place (but doesn't write to mod.rs)
- Agent D: needs `mod jira;` added to mod.rs

**Mitigation:** Agent A owns mod.rs exclusively. Agent D creates the `jira/` directory and files, but Agent A (or a merge coordinator) adds the `mod jira;` line. This is a one-line merge that should be done at the Phase 0 -> Phase 1 transition.

### Second Most Likely Merge Conflicts: `crates/jm-tui/src/plugins/jira/mod.rs`

Both Agent D and Agent E need to write to this file:
- Agent D: defines `JiraPlugin` struct, `impl ScreenPlugin`, background thread management
- Agent E: needs to add rendering dispatch calls and modal state to the same struct

**Mitigation:** Assign `jira/mod.rs` ownership to one agent (recommended: Agent D for the skeleton and Agent E for the render/key-handling body). Alternatively, Agent D writes the struct definition and method stubs, and Agent E fills in the render and handle_key implementations. The struct fields and method bodies can be cleanly separated if the initial skeleton is agreed upon.

### Biggest Unknown: Borrow Checker Battles in `app.rs`

The design doc calls out the "clone-first pattern" for `ScreenId::Plugin(name)` to avoid borrow conflicts between `self.screen` and `self.plugin_registry`. In practice, there may be additional borrow checker issues when:
- Rendering a screen plugin while also needing `&self` for other state
- Handling keys where `&mut self.plugin_registry` conflicts with other `&mut self` borrows
- The tick system needing to forward notifications from screen plugins to sidebar plugins

Agent A will encounter these issues first when wiring app.rs. The design doc provides specific patterns, but real-world borrow checker errors often require creative restructuring.

**Mitigation:** Agent A should add wildcard/stub arms first (Task 8) to get the code compiling, then incrementally wire real behavior. Each match site can be tested independently. The clone-first pattern is reliable; the main risk is missing a clone site.

### Secondary Risk: JIRA API Variance

The JIRA plugin depends on specific API response shapes. Different JIRA Cloud instances may have different custom fields, workflow configurations, and permission schemes. Agent D builds the API layer against the design doc's specifications, but real-world testing may reveal:
- Unexpected field types in `editmeta` responses
- Workflow transitions with unusual required fields
- Rate limiting behavior that differs from documentation

**Mitigation:** Agent D should build the API layer with robust error handling from the start (`JiraError` type, graceful handling of unexpected response shapes). The `FieldType::Unsupported` variant is already designed for this. Real JIRA instance testing should happen as early as Phase 1a.

### Third Risk: Config Schema Change in jm-core

Agent D needs to add `#[serde(flatten)] pub extra: HashMap<String, serde_yml::Value>` to `PluginConfig` in `crates/jm-core/src/config.rs`. This change affects a shared crate that other agents don't touch, but it could potentially break existing config parsing if `serde(flatten)` captures keys that were previously silently ignored.

**Mitigation:** Test the config change with the existing `config.yaml` format first. The `serde(flatten)` directive should only capture keys not already defined as named fields (`enabled`, `notifications`, `pomodoro`), so existing configs should parse identically. Add a unit test in jm-core to verify this.
