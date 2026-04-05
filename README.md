# jm — Job Manager TUI

A personal task and project management TUI for software engineers who juggle multiple projects in parallel.

The killer feature is **context-switch capture**: when you switch between projects, `jm` prompts you to note where you left off, what's blocking you, and what to do next. No more "wait, where was I?" after a meeting derails your afternoon.

![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange)
![Storage](https://img.shields.io/badge/Storage-Markdown-blue)
![License](https://img.shields.io/badge/License-Personal_Project-lightgrey)

## Built by AI

This project is an experiment in **Claude-first development**. The entire codebase — architecture, implementation, tests, design docs, and even this README — was written by [Claude Code](https://claude.ai/code) (Anthropic's AI coding agent) with human direction and review.

The goals of this experiment:

1. **Can an AI write software you actually use daily?** This isn't a demo or a toy. `jm` is used every day for real work across real projects.
2. **On-demand capabilities.** Need a new feature? A plugin? A CLI command? Describe it to the agent and it builds it — architecture, implementation, and tests. The plugin system, JIRA integration, and weekly review screen were all created this way.
3. **What does "AI-native" development look like?** The repo includes design specs, multi-agent build plans, and review artifacts that document the AI-driven development process (see `docs/design/`).

Every commit in this repo has a human author but AI-generated code. The human role is product direction, review, and daily use — the agent handles implementation.

## Quick Start

```bash
# Build and install
cargo build --release
./build-install.sh          # Installs to ~/.local/bin/jm

# Or just run it
cargo run

# Launch the TUI
jm
```

## What It Does

**Dashboard** — see all your projects at a glance in list or kanban view.

**Context switching** — press `s` to switch projects. `jm` captures where you left off, blockers, and next steps before moving on.

**Time tracking** — automatic session tracking. See how long you spent on each project today.

**Journal** — daily entries with timestamped notes, blockers, and decisions. One markdown file per day.

**Issues** — lightweight per-project issue tracking with a cross-project kanban board.

**Weekly review** — activity chart and summary across all projects.

**CLI** — quick capture from the terminal without opening the TUI:
```bash
jm note "figured out the auth flow, need to wire up token refresh"
jm block "waiting on API credentials from infra team"
jm switch my-project
jm status
jm standup              # generate standup from yesterday's journal + git
```

## Design Principles

- **Zero friction.** Every interaction takes seconds. All input fields are optional/skippable.
- **Vim keybindings.** `j/k` to navigate, single-letter commands for everything.
- **Markdown storage.** All data lives in `~/.jm/` as markdown files with YAML frontmatter — human-readable, git-friendly, no database.
- **Agent-readable.** `jm --dump` exports clean text for AI agents to consume.

## Tech Stack

| Component | Choice |
|-----------|--------|
| Language | Rust (2024 edition) |
| TUI | ratatui + crossterm |
| CLI | clap (derive API) |
| Storage | Markdown + YAML frontmatter |
| Serialization | serde + serde_yaml |
| Data directory | `~/.jm/` |

## Project Structure

Cargo workspace with two crates:

```
crates/
├── jm-core/       # Library: models, storage, config, export
│   ├── models/    # Project, Journal, Issue, Person, Inbox
│   └── storage/   # File-backed stores, full-text search
└── jm-tui/        # Binary: TUI app, screens, modals, plugins, CLI
    ├── screens/   # Dashboard, project view, issue board, weekly review
    ├── modals/    # Input, select, confirm, help
    ├── plugins/   # Sidebar plugins (clock, pomodoro, notifications)
    └── widgets/   # Toast, empty state
```

## Key Bindings

### Dashboard

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate up/down |
| `h`/`l` | Navigate columns (kanban) |
| `Enter` | Open project |
| `K` | Toggle kanban/list view |
| `w` | Start working on project |
| `s` | Switch context (with capture) |
| `n` | Quick note |
| `b` | Log blocker |
| `a` | Add project |
| `/` | Search |
| `I` | Issue board |
| `W` | Weekly review |
| `?` | Help |
| `q` | Quit |

### CLI Commands

```
jm                          Launch TUI
jm --dump                   Export state (for AI agents)
jm note "text"              Quick note on active project
jm block "text"             Log blocker
jm work <slug>              Start working on project
jm switch <slug>            Switch with context capture
jm status                   One-line status
jm list                     List all projects
jm add "Name"               Create project
jm done                     End of day
jm standup                  Generate standup report
jm inbox "text"             Capture to inbox
jm issue "title"            Add issue
jm issues                   List issues
jm time                     Show time tracked today
```

Full keybinding and command reference available via `jm --help` and `?` in the TUI.

## Plugin System

The dashboard sidebar supports plugins. Built-in:

- **Clock** — current time
- **Pomodoro** — work/break timer
- **Notifications** — scheduled reminders

Plugins are configurable via `~/.jm/config.yaml` and the system is designed for on-demand extension — describe what you need and the agent builds it.

## Testing

```bash
cargo test                    # All tests
cargo test -p jm-core         # Core library only
```

Tests include property-based markdown round-trip testing (proptest) and storage edge case coverage.

## Anti-Requirements

Things this project will never add:

- Cloud sync, auth, or multi-user support
- A database
- A web UI
- Complex dependency management
- Anything that adds friction

## Development

This is a personal tool with an opinionated development workflow:

1. **Describe** what you want (feature, plugin, fix)
2. **Claude Code** designs and implements it
3. **Review** the output, iterate if needed
4. **Use it** — the real test is daily use

Design specs and review artifacts live in `docs/design/`. The original multi-agent build plan is in `LOKI_MODE_SPEC.md`.
