# jm — Job Manager TUI

Personal task/project management TUI designed for software engineers juggling parallel projects. **Core value**: context-switch capture — when switching projects, the tool prompts you to note where you left off, blockers, and next steps.

## Quick Start

```bash
uv sync              # Install dependencies
uv run jm            # Launch TUI
uv run jm --dump     # Export state to stdout
uv run jm note "x"   # Quick note without TUI
uv run jm status     # One-line status
uv run jm list       # List all projects with slugs
uv run jm add "Name" # Create project from CLI
uv run jm done       # Log "done for day", clear active
```

## Testing

```bash
uv run pytest        # Run all tests
uv run pytest -v     # Verbose
uv run pytest -x     # Stop on first failure
```

## Tech Stack

- **Language**: Python 3.12+
- **TUI Framework**: Textual (rich terminal UI), Rich (rendering)
- **Storage**: Markdown files with YAML frontmatter (human-readable, git-friendly, no DB)
- **Package Manager**: uv
- **Data Directory**: ~/.jm/

## Project Structure

```
src/jm/
├── app.py                 # Textual App root
├── cli.py                 # CLI entry point (argparse)
├── config.py              # Config from ~/.jm/config.yaml with defaults
├── export.py              # Screen dump for agent consumption
├── models/
│   ├── project.py         # Project dataclass (blockers, decisions, log)
│   ├── blocker.py         # Blocker dataclass
│   ├── task.py            # Task dataclass
│   ├── journal.py         # Daily journal (timestamped entries)
│   └── person.py          # Stakeholder tracking from @mentions
├── screens/
│   ├── dashboard.py       # Main project list view
│   ├── project_view.py    # Single project details
│   ├── switch.py          # Context-switch capture modal
│   ├── review.py          # Morning review screen
│   ├── search.py          # Full-text search
│   └── people.py          # Stakeholder view
├── storage/
│   ├── store.py           # ProjectStore, JournalStore, PeopleStore (file CRUD)
│   └── search.py          # Full-text search across markdown files
├── plugins/
│   ├── __init__.py        # Plugin registry with auto-discovery
│   ├── base.py            # JMPlugin base class, PluginTick, PluginNotification
│   ├── clock.py           # Clock plugin (time + date)
│   ├── notifications.py   # Notification center (scheduled reminders, plugin alerts)
│   └── pomodoro.py        # Pomodoro timer (work/break cycles)
├── widgets/
│   ├── plugin_sidebar.py  # Sidebar container for plugins
│   └── quick_input.py     # Quick input modals
├── styles/
│   └── app.tcss           # Textual CSS
└── __main__.py            # Entry point
```

## Data Format

All data stored as markdown with YAML frontmatter in `~/.jm/`:

- **Projects**: `~/.jm/projects/<slug>.md`
- **Journal**: `~/.jm/journal/YYYY-MM-DD.md` (one per day)
- **People**: `~/.jm/people.md` (tracking @mentions)
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
| Enter | Open project |
| w | Start working |
| s | Switch context (capture prompt) |
| n | Quick note |
| b | Log blocker |
| u | Unblock |
| d | Log decision |
| / | Search |
| r | Morning review |
| p | People view |
| a | Add project |
| Ctrl+E | Export screen |
| f | Done for day |
| Tab | Focus plugin sidebar |
| q | Quit |
| ? | Help (all keybindings) |

## Project View Keybindings

| Key | Action |
|-----|--------|
| Escape | Back to dashboard |
| e | Edit current focus |
| S | Cycle status (active/blocked/parked/done) |
| P | Cycle priority (high/medium/low) |
| t | Edit tags |
| T | Edit target date |
| m | Move/edit blocker |
| x | Delete project (with confirmation) |

## CLI Commands

| Command | Description |
|---------|-------------|
| `jm` | Launch TUI |
| `jm --dump` | Export state to stdout |
| `jm --dump -o file` | Export to file |
| `jm note "text"` | Quick note on active project |
| `jm block "text"` | Log blocker on active project |
| `jm work <slug>` | Start working on project |
| `jm switch <slug>` | Switch active project (non-interactive) |
| `jm status` | One-line status |
| `jm list` | List all projects (slug, status, priority, name) |
| `jm list --status active` | Filter by status |
| `jm add "Name"` | Create new project |
| `jm done` | Log "done for day", clear active |
| `jm set-status <slug> <status>` | Change project status |
| `jm set-priority <slug> <pri>` | Change project priority |

## Plugin System

The dashboard has a sidebar with extensible plugins. To create a new plugin:
1. Create a `.py` file in `src/jm/plugins/`
2. Define a class extending `JMPlugin` from `jm.plugins.base`
3. Set `PLUGIN_NAME`, `PLUGIN_DESCRIPTION`, and optionally `NEEDS_TIMER = True`
4. Implement `compose()` to render the widget
5. Override `on_plugin_tick()` for per-second updates (if `NEEDS_TIMER = True`)
6. Call `self.notify_user(msg)` to push notifications to the notification center

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

## Anti-Requirements (Do NOT Add)

- Cloud sync, authentication, multi-user
- Databases (SQLite, etc.)
- Web UI
- Complex dependency chains
- Anything requiring friction

## For AI Agents

- Use `uv run jm --dump` to inspect current state
- Models have `to_markdown()` / `from_markdown()` methods for testing
- All storage is file-based; no server startup needed
- Textual uses `asyncio`; pytest configured with `asyncio_mode = "auto"`
- Entry point: `src/jm/cli.py:main()`
