# jm — Job Manager TUI

Personal task/project management TUI designed for software engineers juggling parallel projects. **Core value**: context-switch capture — when switching projects, the tool prompts you to note where you left off, blockers, and next steps.

## Quick Start

```bash
uv sync              # Install dependencies
uv run jm            # Launch TUI
uv run jm --dump     # Export state to stdout
uv run jm note "x"   # Quick note without TUI
uv run jm status     # One-line status
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
├── widgets/               # Reusable Textual widgets
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
| q | Quit |

## Anti-Requirements (Do NOT Add)

- Cloud sync, authentication, multi-user
- Databases (SQLite, etc.)
- Web UI
- Complex dependency chains
- Auto-save timers or background processes
- Anything requiring friction

## For AI Agents

- Use `uv run jm --dump` to inspect current state
- Models have `to_markdown()` / `from_markdown()` methods for testing
- All storage is file-based; no server startup needed
- Textual uses `asyncio`; pytest configured with `asyncio_mode = "auto"`
- Entry point: `src/jm/cli.py:main()`
