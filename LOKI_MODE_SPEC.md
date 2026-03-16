# Loki Mode Spec: `jm` — Job Manager TUI

## Overview

This spec defines the multi-agent autonomous loop for building `jm` from zero to a fully tested, pip-installable personal task management TUI. The loop is designed around **parallel agent dispatch** with quality gates between phases.

### Guiding Constraints (from user interview)

- Every interaction must take seconds, not minutes
- Context-switch capture is the killer feature — get this right first
- No overhead: if using the tool feels like work, the user will abandon it in days
- The tool must be useful even with minimal input
- Not just code work: meetings, slide decks, research, decisions
- Must work in WSL + Windows Terminal
- Agent-readable: `Ctrl+E` screen dump + `jm --dump` CLI export

---

## Phase 0: Scaffold & Foundation

**Goal:** Establish project structure, dependencies, data model, and a runnable skeleton.

### Agent 0.1 — Project Scaffold (general-purpose)

Create the Python project structure:

```
/mnt/c/projects/job-mgmt/
├── pyproject.toml              # pip-installable, entry point: jm
├── src/
│   └── jm/
│       ├── __init__.py
│       ├── __main__.py         # CLI entry: python -m jm
│       ├── app.py              # Textual App root
│       ├── cli.py              # argparse: --dump, subcommands
│       ├── config.py           # Config loading (~/.jm/config.yaml)
│       ├── models/
│       │   ├── __init__.py
│       │   ├── project.py      # Project dataclass + markdown serde
│       │   ├── task.py         # Task dataclass + markdown serde
│       │   ├── journal.py      # Daily journal entry model
│       │   ├── person.py       # Stakeholder model
│       │   └── blocker.py      # Blocker model
│       ├── storage/
│       │   ├── __init__.py
│       │   ├── store.py        # Read/write markdown files
│       │   └── search.py       # Full-text search across all markdown
│       ├── screens/
│       │   ├── __init__.py
│       │   ├── dashboard.py    # Main dashboard screen
│       │   ├── project_view.py # Single project detail screen
│       │   ├── switch.py       # Context-switch capture screen
│       │   ├── review.py       # Morning review screen
│       │   ├── search.py       # Search results screen
│       │   └── people.py       # People/stakeholder view
│       ├── widgets/
│       │   ├── __init__.py
│       │   ├── project_table.py
│       │   ├── blocker_list.py
│       │   ├── journal_panel.py
│       │   ├── quick_input.py  # Minimal-friction text input modal
│       │   └── status_bar.py
│       ├── export.py           # Rich Console screen dump (agent export)
│       └── styles/
│           └── app.tcss        # Textual CSS
├── tests/
│   ├── __init__.py
│   ├── test_models.py
│   ├── test_storage.py
│   ├── test_search.py
│   ├── test_export.py
│   └── test_screens.py        # Textual pilot tests
├── CLAUDE.md                   # Agent instructions for working in this repo
└── README.md
```

**Outputs:** Runnable `pip install -e .` with `jm` entry point that launches an empty Textual app.

### Agent 0.2 — Data Model Design (general-purpose)

Define the markdown file format for each entity. Key decisions:

**Project file** (`~/.jm/projects/<slug>.md`):
```markdown
---
name: HMI Framework
status: active          # active | blocked | parked | done
priority: high          # high | medium | low
tags: [performance, q2-goal]
created: 2026-03-16
target: 2026-05-01      # soft deadline (optional)
---

## Current Focus
Debugging render loop — vsync timing issue on target hardware.

## Blockers
- [ ] Waiting on @carol for display spec clarification (since 2026-03-14)
- [x] ~~Build server access~~ (resolved 2026-03-15)

## Decisions
- **2026-03-12:** Chose double-buffering over triple — lower latency, acceptable tearing risk
  - Alternatives: triple buffering (safer but +8ms latency), dirty-rect (too complex for deadline)

## Log
### 2026-03-16
- Started investigating vsync timing
- Next: check if compositor is introducing the delay

### 2026-03-14
- Got prototype running on target hardware
- Render loop is 2ms over budget
```

**Journal file** (`~/.jm/journal/2026-03-16.md`):
```markdown
---
date: 2026-03-16
---

## 09:15 — Started: HMI Framework
Focus: debugging render loop

## 11:30 — Switched: HMI Framework → Test Infra
Left off: checking vsync timing, compositor might be the issue
Blocker: waiting on @carol for display spec
Next step: read compositor docs before next attempt

## 11:30 — Started: Test Infra
Focus: PR review feedback from @bob

## 14:00 — Note: Test Infra
Decision: keeping pytest over switching to unittest — team familiarity wins

## 16:30 — Done for day
Active: Test Infra (PR out for re-review), HMI Framework (parked on blocker)
```

**People file** (`~/.jm/people.md`):
```markdown
## @carol
- Role: Display Systems Lead
- Projects: HMI Framework
- Pending: spec clarification (asked 2026-03-14)

## @bob
- Role: Test Infra reviewer
- Projects: Test Infra
- Pending: PR re-review
```

**Config** (`~/.jm/config.yaml`):
```yaml
data_dir: ~/.jm
statuses: [active, blocked, parked, done]
priorities: [high, medium, low]
categories: [feature, bug, meeting, research, decision]
editor: $EDITOR
export_path: ~/.jm/screen.txt
```

**Outputs:** Model dataclasses with `to_markdown()` / `from_markdown()` serde, YAML frontmatter parsing, config loader.

### Agent 0.3 — CLAUDE.md (general-purpose)

Write the `CLAUDE.md` for this repo so future agent sessions can work effectively. Must include:
- Project purpose and architecture overview
- Tech stack (Textual + Rich + markdown storage)
- How to run (`pip install -e .`, then `jm`)
- How to test (`pytest`)
- Key design decisions and anti-requirements
- File structure guide
- The "low-friction" design philosophy

**Gate 0 → 1:** `pip install -e .` succeeds, `jm` launches an empty Textual app, `jm --dump` prints placeholder text, all model serde tests pass.

---

## Phase 1: Core Storage & Models

**Goal:** Working markdown read/write layer with full-text search.

### Agent 1.1 — Storage Layer (general-purpose)

Implement `storage/store.py`:
- `ProjectStore`: CRUD for project markdown files
  - `list_projects()` → sorted by last modified
  - `get_project(slug)` → parsed Project model
  - `save_project(project)` → write markdown
  - `create_project(name, **kwargs)` → create with defaults
- `JournalStore`: Append-only daily journal
  - `today()` → current journal file, create if missing
  - `append(entry)` → add timestamped entry
  - `get_day(date)` → parsed journal
- `PeopleStore`: Read/write people.md
- Auto-create `~/.jm/` directory structure on first run

### Agent 1.2 — Search Engine (general-purpose)

Implement `storage/search.py`:
- Full-text search across all markdown files in `~/.jm/`
- Filter by: project, tag, person (@mentions), date range, category
- Return results with file, line number, and context snippet
- Use simple substring/regex matching (no external deps like whoosh)
- Index should be fast enough for <1000 files without caching

### Agent 1.3 — Model Tests (general-purpose, worktree)

Write comprehensive tests for:
- Markdown → Model → Markdown round-trip fidelity
- Frontmatter parsing edge cases (missing fields, extra fields)
- Journal append ordering
- Search across multiple files with filters
- Config loading with defaults

**Gate 1 → 2:** All storage tests pass. Can create a project, append journal entries, search across files, round-trip markdown without data loss.

---

## Phase 2: TUI Screens — Dashboard & Navigation

**Goal:** The main dashboard screen with project table, blocker summary, and keyboard navigation.

### Agent 2.1 — Dashboard Screen (general-purpose)

Implement `screens/dashboard.py`:

```
╔══════════════════════════════════════════════════════════════════╗
║  jm — Job Manager                            Mon Mar 16, 2026  ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  ACTIVE PROJECTS                                    [4 active]   ║
║  ┌─────────────────┬──────────┬──────┬────────────────────────┐  ║
║  │ Project         │ Status   │ Pri  │ Current Focus          │  ║
║  ├─────────────────┼──────────┼──────┼────────────────────────┤  ║
║  │ ● HMI Framework │ active   │ high │ debugging render loop  │  ║
║  │ ● Test Infra    │ blocked  │ med  │ PR out, waiting @bob   │  ║
║  │ ● OTA Dashboard │ active   │ med  │ wireframe review       │  ║
║  │ ○ Logging Spike │ parked   │ low  │ parked, low priority   │  ║
║  └─────────────────┴──────────┴──────┴────────────────────────┘  ║
║                                                                  ║
║  BLOCKERS                                           [2 open]     ║
║  ⊘ HMI Framework: waiting on @carol for spec (2 days)           ║
║  ⊘ Test Infra: waiting on @bob for PR review (3 days)           ║
║                                                                  ║
║  TODAY'S LOG                                                     ║
║  09:15  Started HMI Framework                                    ║
║  11:30  Switched → Test Infra                                    ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║  [w]ork  [s]witch  [n]ote  [b]lock  [d]ecide  [/]search  [?]   ║
╚══════════════════════════════════════════════════════════════════╝
```

Key bindings (vim-inspired):
| Key | Action |
|-----|--------|
| `j/k` | Navigate project list up/down |
| `Enter` | Open project detail view |
| `w` | Start working on selected project |
| `s` | Switch context (triggers capture prompt) |
| `n` | Quick note on active project |
| `b` | Log a blocker |
| `u` | Unblock (mark blocker resolved) |
| `d` | Log a decision |
| `/` | Open search |
| `r` | Morning review mode |
| `p` | People view |
| `a` | Add new project |
| `Ctrl+E` | Export screen dump |
| `q` | Quit |
| `?` | Show all keybindings |

### Agent 2.2 — Project Detail Screen (general-purpose)

Implement `screens/project_view.py`:
- Shows full project markdown rendered in panels
- Sections: Current Focus, Blockers, Decisions, Log (scrollable)
- Inline editing: press `e` on a section to edit in a text area
- `Escape` returns to dashboard

### Agent 2.3 — Textual CSS Styling (general-purpose)

Implement `styles/app.tcss`:
- Color scheme: muted/professional (not garish)
- Status colors: active=green, blocked=red, parked=yellow, done=dim
- Priority indicators: high=bold, medium=normal, low=dim
- Responsive layout: works in 80-col and wider terminals
- Panel borders, section separators, footer help bar

**Gate 2 → 3:** Dashboard renders with real data from `~/.jm/`. Can navigate with keyboard. Project detail view opens and shows content. Textual pilot tests pass for navigation flow.

---

## Phase 3: Context-Switch Engine (The Killer Feature)

**Goal:** The `switch` workflow that captures state before allowing context change.

### Agent 3.1 — Switch Screen (general-purpose)

Implement `screens/switch.py`:

This is the highest-priority feature. When user presses `s`:

```
╔═══════════════════════════════════════════════════════╗
║  SWITCHING FROM: HMI Framework                        ║
╠═══════════════════════════════════════════════════════╣
║                                                       ║
║  You started at 09:15 with focus:                     ║
║  "debugging render loop — vsync timing issue"         ║
║                                                       ║
║  Where did you leave off? (1 line, Enter to skip)     ║
║  > checked compositor, it's adding 4ms delay_         ║
║                                                       ║
║  Anything blocking? (Enter to skip)                   ║
║  > need @carol's spec to know if 4ms is acceptable_   ║
║                                                       ║
║  Next step when you come back? (Enter to skip)        ║
║  > test with compositor bypass flag_                   ║
║                                                       ║
║  ─────────────────────────────────────────────────    ║
║  Switch to:                                           ║
║  > Test Infra          ← (Tab to cycle projects)      ║
║    OTA Dashboard                                      ║
║    Logging Spike                                      ║
║                                                       ║
║  [Enter] Switch  [Esc] Cancel                         ║
╚═══════════════════════════════════════════════════════╝
```

Behavior:
1. Shows what you started with (from the `work` command / last journal entry)
2. Three optional prompts — all skippable with Enter (zero required fields)
3. Every field entered is appended to BOTH the project file AND the daily journal
4. Project selector at bottom with Tab cycling
5. On confirm: updates active project, appends "Started: X" to journal
6. On cancel: returns to dashboard, nothing saved

### Agent 3.2 — Work & Resume (general-purpose)

Implement the "start working" flow:
- `w` on dashboard or `jm work <project>` from CLI
- Sets active project in `~/.jm/.active` (single line: project slug)
- Appends "Started: <project>" with timestamp to daily journal
- When resuming a project, shows the LAST switch-away note:
  ```
  Resuming: HMI Framework
  Last time (Mar 14): "checked compositor, it's adding 4ms delay"
  Next step was: "test with compositor bypass flag"
  Blocker: "need @carol's spec" — still open?  [y]es [n]o resolved
  ```
- This is the payoff — the reason capturing on switch is worth it

### Agent 3.3 — Quick Input Widgets (general-purpose)

Implement `widgets/quick_input.py`:
- Minimal modal for single-line input (note, blocker, decision)
- Pre-filled context (e.g., "Note on HMI Framework:")
- Enter submits, Escape cancels
- For blockers: auto-detect @mentions and link to people.md
- For decisions: prompt format "Chose X over Y because Z" (optional template)
- All inputs append to both project file and daily journal

**Gate 3 → 4:** Full switch workflow works end-to-end. Can start work, switch with capture prompts, resume and see previous notes. Journal entries are created correctly. Textual pilot tests verify the switch → capture → resume cycle.

---

## Phase 4: Search, People & Review

**Goal:** Full-text search, people tracking, and morning review screen.

### Agent 4.1 — Search Screen (general-purpose)

Implement `screens/search.py`:
- Fuzzy search via Textual's command palette pattern
- `/` opens search bar at top of screen
- Live results as you type (debounced)
- Results show: file, project, date, matching line with highlight
- Filter chips: `@person`, `#tag`, `project:name`, `status:blocked`
- Enter on result navigates to that project/journal entry

### Agent 4.2 — People Screen (general-purpose)

Implement `screens/people.py`:
- Table of all @mentioned people across project files
- For each person: name, role, projects, pending items
- "Waiting on" view: all open blockers grouped by person
- "Asked by" view: requests/action items grouped by requester
- Quick action: press `Enter` on a pending item to resolve it

### Agent 4.3 — Morning Review Screen (general-purpose)

Implement `screens/review.py`:

```
╔══════════════════════════════════════════════════════════════╗
║  MORNING REVIEW — Mon Mar 16, 2026                          ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  YESTERDAY (Fri Mar 13)                                      ║
║  ┌────────────────────────────────────────────────────────┐  ║
║  │ 09:15  Started HMI Framework                           │  ║
║  │ 11:30  Switched → Test Infra                           │  ║
║  │        Left off: "compositor adding 4ms delay"         │  ║
║  │ 14:00  Decision: keeping pytest over unittest          │  ║
║  │ 16:30  Done for day                                    │  ║
║  └────────────────────────────────────────────────────────┘  ║
║                                                              ║
║  BLOCKERS SINCE LAST REVIEW                     [2 open]     ║
║  ⊘ @carol: display spec (2 days) — still open?  [y/n]       ║
║  ⊘ @bob: PR review (3 days) — still open?  [y/n]            ║
║                                                              ║
║  STALE PROJECTS (not touched in 7+ days)                     ║
║  ○ Logging Spike — last touched Mar 6 — [p]ark / [a]ctivate ║
║                                                              ║
║  TODAY'S FOCUS (pick up to 3)                                ║
║  [x] HMI Framework — "test with compositor bypass flag"      ║
║  [ ] Test Infra — "address PR feedback"                      ║
║  [ ] OTA Dashboard                                           ║
║                                                              ║
║  [Enter] Start day  [Esc] Back to dashboard                  ║
╚══════════════════════════════════════════════════════════════╝
```

Features:
- Shows previous day's journal (or last workday if Monday)
- Prompts to resolve/keep stale blockers (y/n inline)
- Flags stale projects (configurable threshold, default 7 days)
- Focus picker: select up to 3 projects for today (optional)
- On "Start day": writes focus to today's journal, returns to dashboard

**Gate 4 → 5:** Search returns results across all files with filters. People view aggregates @mentions correctly. Morning review shows previous day's journal, stale blockers, and stale projects.

---

## Phase 5: Agent Export & CLI Mode

**Goal:** The dual export system for AI agent consumption.

### Agent 5.1 — Screen Dump Export (general-purpose)

Implement `export.py`:
- `Ctrl+E` in any screen triggers export
- Builds a Rich Console with `record=True`
- Renders current screen state (dashboard, project, whatever is visible) as Rich renderables
- Calls `console.save_text("~/.jm/screen.txt")`
- Shows brief toast notification: "Exported to ~/.jm/screen.txt"
- The text file must be clean, unstyled, and parseable by an AI agent

Example output (`screen.txt`):
```
jm — Job Manager                            Mon Mar 16, 2026

ACTIVE PROJECTS (4)
  Project           Status    Pri   Current Focus
  HMI Framework     active    high  debugging render loop
  Test Infra        blocked   med   PR out, waiting @bob
  OTA Dashboard     active    med   wireframe review
  Logging Spike     parked    low   parked, low priority

BLOCKERS (2)
  HMI Framework: waiting on @carol for spec (2 days)
  Test Infra: waiting on @bob for PR review (3 days)

TODAY'S LOG
  09:15  Started HMI Framework
  11:30  Switched -> Test Infra

ACTIVE: Test Infra (since 11:30)
```

### Agent 5.2 — CLI Non-Interactive Mode (general-purpose)

Implement CLI subcommands that work WITHOUT launching the TUI:
```bash
jm --dump                   # Export current state to stdout (same as Ctrl+E output)
jm --dump -o file.txt       # Export to specific file
jm note "quick thought"     # Append note to active project without launching TUI
jm block "waiting on X"     # Log blocker without TUI
jm switch <project>         # Non-interactive switch (no prompts, just switches active)
jm status                   # One-line: "Active: Test Infra (since 11:30), 2 blockers"
```

These CLI commands are critical for agent integration — an AI agent can read state with `jm --dump` and write with `jm note`.

**Gate 5 → 6:** `jm --dump` produces clean text. `jm note "test"` appends without launching TUI. Screen dump from TUI matches CLI dump content. Export test verifies text is ANSI-free.

---

## Phase 6: Polish, Testing & Packaging

**Goal:** Production quality, full test coverage, pip-installable.

### Agent 6.1 — Integration Tests (general-purpose, worktree)

Full end-to-end tests using Textual's pilot testing:
- Launch app → navigate to project → press `s` → fill switch prompts → verify journal entry
- Morning review → resolve blocker → verify people.md updated
- Search → enter query → verify results → navigate to result
- Export → verify screen.txt content
- CLI mode tests: `jm note`, `jm --dump`, `jm status`

### Agent 6.2 — Edge Cases & Resilience (general-purpose, worktree)

- First run with no `~/.jm/` directory (auto-create)
- Empty state: no projects, no journal (helpful onboarding message)
- Corrupt markdown files (graceful parse failure, not crash)
- Very long project names, notes, tag lists
- Concurrent access (not critical for single-user, but don't corrupt files)
- WSL path handling (Windows ↔ Linux path edge cases)

### Agent 6.3 — Packaging & Install (general-purpose)

- `pyproject.toml` with all dependencies pinned
- Entry point: `jm` command
- `pip install .` works globally
- `pip install -e .` works for development
- Verify install in a clean venv

**Gate 6 → Done:** All tests pass. `pip install .` in clean venv works. `jm` launches dashboard. `jm --dump` produces output. Full switch cycle works. Morning review works.

---

## Agent Parallelization Map

```
Phase 0:  [0.1 Scaffold] ──→ ┐
          [0.2 Data Model] ──→├──→ Gate 0
          [0.3 CLAUDE.md]  ──→┘

Phase 1:  [1.1 Storage] ─────→┐
          [1.2 Search]  ─────→├──→ Gate 1
          [1.3 Tests]   ─────→┘

Phase 2:  [2.1 Dashboard] ───→┐
          [2.2 Project View] ─→├──→ Gate 2
          [2.3 CSS Styling]  ─→┘

Phase 3:  [3.1 Switch Screen] → ┐
          [3.2 Work/Resume]   → ├──→ Gate 3
          [3.3 Quick Input]   → ┘

Phase 4:  [4.1 Search Screen] → ┐
          [4.2 People Screen] → ├──→ Gate 4
          [4.3 Review Screen] → ┘

Phase 5:  [5.1 Screen Dump]  → ┐
          [5.2 CLI Mode]     → ├──→ Gate 5
                                ┘

Phase 6:  [6.1 Integration Tests] → ┐
          [6.2 Edge Cases]         → ├──→ Gate 6 → DONE
          [6.3 Packaging]          → ┘
```

Within each phase, all agents run in **parallel** (worktree isolation where noted).
Phases run **sequentially** — each gate must pass before the next phase starts.

---

## Quality Gates — Verification Protocol

Each gate is verified by a **review agent** (separate from the build agents):

1. **Run tests:** `pytest -v` — all must pass
2. **Run app:** Launch `jm`, verify it doesn't crash, basic navigation works
3. **Run export:** `jm --dump` produces valid output
4. **Code review:** Check for:
   - No hardcoded paths (use config)
   - No required fields in user-facing prompts (everything skippable)
   - Keyboard bindings match spec
   - Markdown round-trip fidelity
   - ANSI-free export output
5. **Friction check:** Can a new note be captured in <5 seconds? Can a switch be completed in <30 seconds?

---

## Error Recovery

If an agent fails:
1. Check error output — if it's a dependency/import issue, fix and retry
2. If it's a design conflict with another agent's output, the review agent resolves
3. If a gate fails, only re-run the failing agent(s), not the entire phase
4. If 3 retries fail on the same agent, pause and surface to user for guidance

---

## Tooling: uv

**Use `uv` for ALL Python operations.** No `pip`, `pip3`, `python -m venv`, `virtualenv`, or `pip install` — always `uv`.

| Task | Command |
|------|---------|
| Init project | `uv init` (if no pyproject.toml) |
| Add dependency | `uv add textual rich pyyaml python-frontmatter` |
| Add dev dependency | `uv add --dev pytest pytest-asyncio` |
| Install from pyproject.toml | `uv sync` |
| Editable install | `uv sync` (editable is the default for the project) |
| Run the app | `uv run jm` |
| Run tests | `uv run pytest` |
| Run any Python script | `uv run python script.py` |
| Create venv (if needed) | `uv venv` |

`uv` manages the virtualenv, lockfile (`uv.lock`), and dependency resolution automatically. Do not manually create or activate venvs — `uv run` handles it.

---

## Dependencies

```
textual>=1.0.0
rich>=14.0.0
pyyaml>=6.0
python-frontmatter>=1.1.0
```

No other external dependencies. Keep it minimal.

---

## Phase 7: UX Polish, Editing & Bug Fixes

**Goal:** Fix all known bugs, add editing capabilities, and polish the UX to match the low-friction design philosophy.

**Context:** After the initial build (Phases 0–6), a review identified 20 issues ranging from data-loss bugs to UX friction. This phase fixes them in priority order.

### Agent 7.1 — Critical Bug Fixes (general-purpose)

Fix these bugs that cause data loss or broken rendering:

1. **Person data wipe on blocker creation** — `_handle_blocker` in `dashboard.py` calls `add_or_update_person(Person(...))` which **replaces** the entire person record, wiping their existing role, projects, and other pending items. Fix `PeopleStore.add_or_update_person()` to **merge** instead of replace: append new pending items, union project lists, preserve existing role unless a new one is provided.

2. **Rich markup injection in blocker panel** — `dashboard.py:132` renders `f"[{count} open]\n..."` but Textual's `Static.update()` interprets `[2 open]` as Rich markup (broken tag). Escape the brackets or use a different format like `(2 open)`.

3. **`action_work` writes journal before resume screen** — The "Started" journal entry is written immediately, but if the user is shown the resume screen and decides to dismiss it, the entry is already committed. The journal append should happen after the resume screen is dismissed, or at minimum the "Started" entry should always be written (it's not cancelable, just informational — this is acceptable if the user presses `w` they intend to work).

4. **`find_last_switch_away` only checks 1 previous day** — Currently calls `get_previous_workday()` once. If the last switch was 3+ days ago, resume context is lost. Should iterate through multiple previous days (up to 14 days back, matching `get_previous_workday`'s own search depth).

### Agent 7.2 — Editing Capabilities (general-purpose)

This is the core new feature: the ability to **edit existing data**, especially moving blockers between projects.

#### Blocker editing (highest priority — user explicitly requested)

**From project view** (`e` on a blocker, or new `m` for move):
- Show a blocker editor screen that allows:
  - Edit description text
  - **Move to different project** — pick from project list
  - Change the @person
  - Mark as resolved (alternative to `u` on dashboard)
  - Delete entirely

**Blocker move flow:**
```
╔════════════════════════════════════════════════╗
║  EDIT BLOCKER                                   ║
╠════════════════════════════════════════════════╣
║                                                 ║
║  Project: HMI Framework                         ║
║                                                 ║
║  Description:                                   ║
║  > waiting on @carol for display spec_           ║
║                                                 ║
║  Move to project:                               ║
║  > (keep current)                               ║
║    Test Infra                                   ║
║    OTA Dashboard                                ║
║                                                 ║
║  [Enter] Save  [d] Delete  [Esc] Cancel         ║
╚════════════════════════════════════════════════╝
```

When moved: remove from source project, add to target project, log the move in both project files and the journal.

#### Project metadata editing

**From project view**, add these keybindings:
| Key | Action |
|-----|--------|
| `e` | Edit current focus (already exists) |
| `S` | Change status (cycle: active → blocked → parked → done) |
| `P` | Change priority (cycle: high → medium → low) |
| `t` | Edit tags |
| `T` | Edit target date |
| `x` | Delete project (with confirmation) |

#### Blocker editing from dashboard

**From dashboard blocker panel**, pressing `Enter` or `e` on a blocker should open the edit screen described above. The dashboard doesn't currently have a way to select individual blockers — add a secondary navigation mode or integrate with the unblock flow.

#### Decision editing

**From project view**, pressing `Enter` on a decision should allow editing the text and alternatives.

#### CLI editing commands

```bash
jm edit <project-slug>           # Open project in $EDITOR (or TUI edit)
jm move-blocker <project> <idx>  # Move blocker at index to different project (interactive picker)
jm set-status <project> <status> # Change project status
jm set-priority <project> <pri>  # Change project priority
```

### Agent 7.3 — UX Improvements (general-purpose)

Fix UX friction and missing features:

1. **Switch screen: parallel inputs instead of serial** — All three capture fields (left off, blocker, next step) should be visible and navigable simultaneously with Tab/Shift-Tab. Remove the progressive disable/enable pattern. The project selector should also be visible from the start. This reduces the minimum keypresses from 4 Enters to 1.

2. **Use `ModalScreen` for dialogs** — `AddProjectScreen`, `QuickNoteScreen`, `QuickBlockerScreen`, `QuickDecisionScreen`, `UnblockScreen`, and `EditFocusScreen` should extend `ModalScreen` instead of `Screen` so the dashboard remains visible underneath. This provides visual context and feels more polished.

3. **Status colors in DataTable** — The CSS defines `.status-active`, `.status-blocked`, etc. but the dashboard never applies them. Use Rich markup or Textual's `Text` class to colorize the status column: green=active, red=blocked, yellow=parked, dim=done.

4. **Empty-state onboarding** — When launching with no projects, show a centered message: "No projects yet. Press `a` to create your first project, or `?` for help." instead of empty table headers.

5. **Help screen (`?`)** — Implement the keybindings reference screen. Show all bindings in a clean table. Currently stubbed as "not yet implemented".

6. **"Done for day" command** — Add `Ctrl+D` or `f` keybinding to log a "Done for day" journal entry. Add `jm done` CLI command.

7. **`jm add <name>` CLI command** — Create projects from CLI without launching TUI.

8. **`jm list` CLI command** — Show all projects with their slugs and statuses. Essential for scripting (`jm work` needs a slug).

9. **Search debounce** — Add ~200ms debounce on `on_input_changed` using `set_timer` to avoid re-searching on every keystroke.

10. **Journal "Started:" format** — Dashboard journal panel shows `"Started HMI Framework"` but should show `"Started: HMI Framework"` with the colon, matching the spec format.

11. **People screen: resolve pending items** — Pressing Enter on a pending item should resolve it (remove from pending list). Currently read-only.

12. **Dashboard triple data load** — `_refresh_data()` calls `list_projects()` in `_refresh_projects()` and again in `_refresh_blockers()`. Load once, pass the list.

### Agent 7.4 — Tests for Phase 7 (general-purpose, worktree)

Write tests covering:
- Person merge (not replace) on `add_or_update_person`
- Blocker move between projects (remove from source, add to target)
- Project status/priority cycling
- Blocker editing (description, person, resolution)
- `jm list`, `jm add`, `jm done` CLI commands
- Empty-state onboarding message
- Rich markup escaping in panels
- `find_last_switch_away` searching multiple previous days

**Gate 7 → Done:** All bugs fixed. Blockers can be moved between projects. Project metadata is editable. Switch screen uses parallel inputs. All new tests pass alongside existing 147 tests.

---

### Updated Agent Parallelization Map

```
Phase 0–6:  (completed — see above)

Phase 7:  [7.1 Bug Fixes]      → ┐
          [7.2 Editing]         → ├──→ Gate 7 → DONE
          [7.3 UX Improvements] → │
          [7.4 Tests]           → ┘
```

---

## Success Criteria

The tool is done when:
1. `uv sync && uv run jm` launches the TUI dashboard
2. A user can: create a project, start work, capture a note, switch context (with prompted capture), resume (seeing previous notes), run morning review, search across all data, export screen for an agent
3. A user can: **edit project metadata** (status, priority, tags, focus, target date), **edit blockers** (description, person, move to different project, delete), **edit decisions**
4. `jm --dump` produces clean text readable by Claude Code
5. All tests pass (including Phase 7 tests)
6. The entire switch cycle (capture → switch → resume) takes <30 seconds of user time
7. A note can be captured in <5 seconds of user time
8. **No data loss bugs** — person records merge, blocker moves are atomic, journal entries are consistent
9. CLI covers full CRUD: `jm add`, `jm list`, `jm note`, `jm block`, `jm status`, `jm done`, `jm work`, `jm switch`, `jm --dump`
