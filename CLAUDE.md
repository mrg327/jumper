# jm — Job Manager TUI

Personal task/project management TUI designed for software engineers juggling parallel projects. **Core value**: context-switch capture — when switching projects, the tool prompts you to note where you left off, blockers, and next steps.

## Quick Start

```bash
cargo build --release    # Build release binary
./build-install.sh       # Build + install to ~/.local/bin/jm
cargo test               # Run all tests
jm                       # Launch TUI
jm --dump                # Export state to stdout
```

## Tech Stack

- **Language**: Rust (edition 2024)
- **TUI Framework**: ratatui + crossterm
- **CLI**: clap (derive)
- **Serialization**: serde + serde_yaml (YAML frontmatter in markdown files)
- **Storage**: Markdown files with YAML frontmatter (human-readable, git-friendly, no DB)
- **Data Directory**: `~/.jm/`

## Project Structure

Cargo workspace with two crates:

```
crates/
├── jm-core/                   # Library: models, storage, config, export
│   └── src/
│       ├── lib.rs             # Public API (re-exports modules)
│       ├── config.rs          # Config from ~/.jm/config.yaml
│       ├── export.rs          # Screen dump for agent consumption
│       ├── crosslinks.rs      # [[slug]] cross-reference finder
│       ├── time.rs            # Session time tracking + aggregation
│       ├── models/
│       │   ├── mod.rs         # Project, Blocker, LogEntry, JournalEntry, Person, etc.
│       │   ├── project.rs     # Project with markdown round-trip
│       │   ├── journal.rs     # Daily journal (timestamped entries)
│       │   ├── person.rs      # Stakeholder tracking from @mentions
│       │   ├── inbox.rs       # Quick-capture inbox items
│       │   └── issue.rs       # Hierarchical issues (parent/child)
│       └── storage/
│           ├── mod.rs         # Store trait re-exports
│           ├── store.rs       # ProjectStore, JournalStore, PeopleStore, etc.
│           └── search.rs      # Full-text search across markdown files
└── jm-tui/                    # Binary: TUI app + CLI commands
    └── src/
        ├── main.rs            # Entry point, terminal setup, CLI dispatch
        ├── cli.rs             # Clap CLI definition (all subcommands)
        ├── app.rs             # Main TUI app loop
        ├── events.rs          # Event handling, Focus, ScreenId
        ├── keyhints.rs        # Dynamic keybinding footer bar
        ├── theme.rs           # Color constants
        ├── text_utils.rs      # Text wrapping/formatting utilities
        ├── screens/
        │   ├── dashboard.rs   # Main project list + kanban view
        │   ├── project_view.rs # Single project details
        │   ├── switch.rs      # Context-switch capture modal
        │   ├── issue_board.rs # Cross-project issue kanban board
        │   ├── weekly.rs      # Weekly review with activity chart
        │   ├── review.rs      # Morning review screen
        │   ├── search.rs      # Full-text search
        │   └── people.rs      # Stakeholder view
        ├── modals/
        │   ├── input.rs       # Text input modal
        │   ├── select.rs      # Selection list modal
        │   ├── confirm.rs     # Confirmation dialog
        │   └── help.rs        # Help/keybinding reference
        ├── plugins/
        │   ├── mod.rs         # Plugin registry
        │   ├── sidebar.rs     # Sidebar container widget
        │   ├── clock.rs       # Clock plugin
        │   ├── notifications.rs # Notification center
        │   └── pomodoro.rs    # Pomodoro timer
        └── widgets/
            ├── empty_state.rs # Empty state placeholder
            └── toast.rs       # Toast notification widget
```

## Data Format

All data stored as markdown with YAML frontmatter in `~/.jm/`:

- **Projects**: `~/.jm/projects/<slug>.md`
- **Journal**: `~/.jm/journal/YYYY-MM-DD.md` (one per day)
- **People**: `~/.jm/people.md` (tracking @mentions)
- **Issues**: `~/.jm/issues/<slug>.md` (per-project issue tracker)
- **Inbox**: `~/.jm/inbox.md` (quick-capture)
- **Config**: `~/.jm/config.yaml` (user settings, defaults provided)
- **Active State**: `~/.jm/.active` (single line: current project slug)

Models use `to_markdown()` and `from_markdown()` for serialization.

## Key Design Decisions

1. **Zero-friction**: Every interaction in seconds. All input fields optional/skippable via Enter.
2. **Context-switch capture**: The killer feature — prompt when switching projects for blockers, next steps.
3. **Agent-readable**: `jm --dump` and Ctrl+E export produce clean, ANSI-free text.
4. **Vim-inspired keybindings**: j/k for nav, single-letter commands.
5. **Markdown storage**: Human-readable, git-friendly, no database dependency.

## Dashboard Keybindings

| Key | Action |
|-----|--------|
| j/k | Navigate up/down |
| h/l | Navigate columns (kanban mode) |
| Enter | Open project |
| K | Toggle kanban/list view |
| w | Start working |
| s | Switch context (capture prompt) |
| n | Quick note |
| b | Log blocker |
| d | Log decision |
| / | Search |
| r | Morning review |
| p | People view |
| a | Add project |
| Ctrl+E | Export screen |
| m | Quick meeting switch |
| f | Stop / break / done for day |
| I | Issue board (cross-project kanban) |
| W | Weekly review |
| Tab | Focus plugin sidebar |
| q | Quit |
| ? | Help (all keybindings) |

## Issue Board Keybindings

| Key | Action |
|-----|--------|
| h/l | Navigate columns |
| j/k | Navigate within column |
| Enter/s | Advance issue status |
| S | Reverse issue status |
| c | Close issue (set to Done) |
| p | Cycle project filter |
| D | Toggle Done column |
| o | Open project view |
| g/G | Jump to top/bottom |
| Esc | Back to dashboard |

## Weekly Review Keybindings

| Key | Action |
|-----|--------|
| Tab | Cycle sections |
| j/k | Navigate within section |
| g/G | Jump to top/bottom |
| W | Toggle back to dashboard |
| Esc/q | Back to dashboard |

## Project View Keybindings

| Key | Action |
|-----|--------|
| Esc/q | Back to dashboard |
| e | Edit current focus |
| x | Pin active issue |
| i | Add issue |
| N | Note to issue |
| s | Cycle status |
| c | Close issue |
| n | Quick note |
| b | Log blocker |
| o | Open in external editor |

## CLI Commands

| Command | Description |
|---------|-------------|
| `jm` | Launch TUI |
| `jm --dump` | Export state to stdout |
| `jm --dump -o file` | Export to file |
| `jm note "text"` | Quick note on active project |
| `jm block "text"` | Log blocker on active project |
| `jm work <slug>` | Start working on project |
| `jm switch <slug>` | Switch active project (with context capture) |
| `jm switch <slug> --no-capture` | Silent switch (for scripting) |
| `jm status` | One-line status |
| `jm list` | List all projects (slug, status, priority, name) |
| `jm list --status active` | Filter by status |
| `jm add "Name"` | Create new project |
| `jm done` | Log "done for day", clear active |
| `jm done --reflect "..." --tomorrow "..."` | Done with reflection |
| `jm break [15min\|lunch\|eod]` | Take a break |
| `jm time [YYYY-MM-DD]` | Show time tracked (today or given date) |
| `jm standup [YYYY-MM-DD]` | Generate standup report from journal + git |
| `jm inbox "text"` | Capture quick thought to inbox |
| `jm refs <slug>` | Show cross-references for a project |
| `jm set-status <slug> <status>` | Change project status |
| `jm set-priority <slug> <pri>` | Change project priority |
| `jm issue "title" [--project slug] [--parent N]` | Add issue to project |
| `jm issues [--project slug] [--status X] [--all]` | List issues |
| `jm issue-status <slug> <id> <status>` | Change issue status |

## Plugin System

The dashboard has a sidebar with extensible plugins.

Built-in plugins: Clock, Notifications, Pomodoro.

Plugin config in `~/.jm/config.yaml`:
```yaml
plugins:
  enabled: [pomodoro, notifications, clock]
  pomodoro:
    work_minutes: 25
  notifications:
    reminders:
      - time: "09:00"
        message: "Morning review"
```

## Testing

```bash
cargo test                        # All tests
cargo test -p jm-core             # Core library tests only
cargo test -p jm-core proptest    # Property-based round-trip tests
```

Test files in `crates/jm-core/tests/`:
- `proptest_roundtrip.rs` — property-based markdown round-trip tests
- `real_data_roundtrip.rs` — real data round-trip verification
- `storage_edge_cases.rs` — storage edge case coverage

## Anti-Requirements (Do NOT Add)

- Cloud sync, authentication, multi-user
- Databases (SQLite, etc.)
- Web UI
- Complex dependency chains
- Anything requiring friction

## For AI Agents

- Use `jm --dump` (or `cargo run -- --dump`) to inspect current state
- Models have `to_markdown()` / `from_markdown()` methods for testing
- All storage is file-based; no server startup needed
- Entry point: `crates/jm-tui/src/main.rs`
- Core library API: `crates/jm-core/src/lib.rs`
