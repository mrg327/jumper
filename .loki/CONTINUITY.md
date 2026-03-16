# Loki Mode — Working Memory

## Current State
- **Phase:** 8 — COMPLETE
- **Status:** ALL PHASES DONE
- **Started:** 2026-03-16

## Completion Promise — FULFILLED
Build `jm` — a pip-installable Job Manager TUI using Textual + Rich + markdown storage.
All 8 phases per LOKI_MODE_SPEC.md, from scaffold to plugin sidebar.

## Key Decisions
- Use `uv` for ALL Python operations (no pip, no manual venvs)
- Textual TUI framework + Rich for rendering
- Markdown files with YAML frontmatter for all data storage
- Context-switch capture is the killer feature
- Plugin system: auto-discovery from src/jm/plugins/ package
- Plugin sidebar: right column of dashboard, Tab/Shift-Tab focus cycling

## Mistakes & Learnings
- CLI internal functions import `create_stores` locally, so patches must target `jm.storage.store.create_stores`
- Rich markup in Static widgets: square brackets `[text]` are interpreted as markup tags — use parentheses
- Plugin `_update_display()` must guard `query_one()` calls with try/except for unit-testability outside Textual app context
- `notify_user()` in base plugin must also be guarded since `post_message()` requires a mounted widget tree

## Phase Progress
- [x] Phase 0: Scaffold & Foundation
- [x] Phase 1: Core Storage & Models
- [x] Phase 2: TUI Screens — Dashboard & Navigation
- [x] Phase 3: Context-Switch Engine
- [x] Phase 4: Search, People & Review
- [x] Phase 5: Agent Export & CLI Mode
- [x] Phase 6: Polish, Testing & Packaging (147 tests)
- [x] Phase 7: UX Polish, Editing & Bug Fixes (190 tests)
- [x] Phase 8: Plugin Sidebar & Notification Center (233 tests)
  - [x] 8.1 Plugin Infrastructure (base class, registry, sidebar widget, dashboard layout, CSS, config)
  - [x] 8.2 Built-in Plugins (clock, notifications, pomodoro)
  - [x] 8.3 Tests (43 new tests covering plugin lifecycle, state machines, config, discovery)
