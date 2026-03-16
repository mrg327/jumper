"""Screen dump export — clean text for AI agent consumption.

Ctrl+E in the TUI or `jm --dump` from CLI produces this output.
No ANSI codes, no Rich markup, just parseable plain text.
"""

from __future__ import annotations

from datetime import date
from pathlib import Path

from jm.config import load_config
from jm.storage.store import ActiveProjectStore, JournalStore, PeopleStore, ProjectStore


def generate_dump(
    project_store: ProjectStore,
    journal_store: JournalStore,
    people_store: PeopleStore,
    active_store: ActiveProjectStore,
) -> str:
    """Generate a clean text dump of current jm state.

    Returns ANSI-free plain text suitable for AI agent consumption.
    """
    lines: list[str] = []
    today = date.today()
    today_str = today.strftime("%a %b %d, %Y")

    # Header
    lines.append(f"jm -- Job Manager{' ' * 28}{today_str}")
    lines.append("")

    # Projects
    projects = project_store.list_projects()
    status_order = {"active": 0, "blocked": 1, "parked": 2, "done": 3}
    projects.sort(key=lambda p: (status_order.get(p.status, 99), p.name))

    lines.append(f"ACTIVE PROJECTS ({len(projects)})")
    if projects:
        lines.append(f"  {'Project':<20} {'Status':<10} {'Pri':<6} Current Focus")
        for p in projects:
            pri = {"high": "high", "medium": "med", "low": "low"}.get(
                p.priority, p.priority
            )
            focus = p.current_focus[:40] if p.current_focus else ""
            lines.append(f"  {p.name:<20} {p.status:<10} {pri:<6} {focus}")
    else:
        lines.append("  No projects yet")
    lines.append("")

    # Blockers
    blocker_items: list[str] = []
    for p in projects:
        for b in p.blockers:
            if not b.resolved:
                days = ""
                if b.since:
                    delta = (today - b.since).days
                    days = f" ({delta} days)"
                person = f" {b.person}" if b.person else ""
                blocker_items.append(
                    f"  {p.name}: {b.description}{person}{days}"
                )

    lines.append(f"BLOCKERS ({len(blocker_items)})")
    if blocker_items:
        lines.extend(blocker_items)
    else:
        lines.append("  No open blockers")
    lines.append("")

    # Today's log
    journal = journal_store.today()
    lines.append("TODAY'S LOG")
    if journal.entries:
        for entry in journal.entries:
            if entry.entry_type == "Switched":
                lines.append(f"  {entry.time}  Switched -> {entry.project}")
            elif entry.entry_type == "Done":
                lines.append(f"  {entry.time}  Done for day")
            else:
                lines.append(
                    f"  {entry.time}  {entry.entry_type} {entry.project}"
                )
    else:
        lines.append("  No entries yet today")
    lines.append("")

    # Active project
    active_slug = active_store.get_active()
    if active_slug:
        active_project = project_store.get_project(active_slug)
        name = active_project.name if active_project else active_slug
        # Find when the active project was last started
        last_start = ""
        for entry in reversed(journal.entries):
            if entry.entry_type in ("Started", "Switched") and name in entry.project:
                last_start = f" (since {entry.time})"
                break
        lines.append(f"ACTIVE: {name}{last_start}")
    else:
        lines.append("ACTIVE: none")

    return "\n".join(lines)


def export_to_file(
    project_store: ProjectStore,
    journal_store: JournalStore,
    people_store: PeopleStore,
    active_store: ActiveProjectStore,
    output_path: Path | None = None,
) -> Path:
    """Export dump to file. Returns the path written to."""
    text = generate_dump(project_store, journal_store, people_store, active_store)

    if output_path is None:
        config = load_config()
        output_path = Path(
            config.get("export_path", "~/.jm/screen.txt")
        ).expanduser()

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(text, encoding="utf-8")
    return output_path


def dump_to_stdout(
    project_store: ProjectStore,
    journal_store: JournalStore,
    people_store: PeopleStore,
    active_store: ActiveProjectStore,
) -> None:
    """Print dump to stdout (for `jm --dump`)."""
    print(generate_dump(project_store, journal_store, people_store, active_store))
