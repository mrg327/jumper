import argparse
import sys
from datetime import date, datetime
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(prog="jm", description="Job Manager TUI")
    parser.add_argument("--dump", action="store_true", help="Export current state to stdout")
    parser.add_argument("-o", "--output", help="Export to file instead of stdout (use with --dump)")
    parser.add_argument("--version", action="version", version="jm 0.1.0")

    subparsers = parser.add_subparsers(dest="command")

    # note command
    note_parser = subparsers.add_parser("note", help="Add a quick note to active project")
    note_parser.add_argument("text", nargs="+", help="Note text")

    # block command
    block_parser = subparsers.add_parser("block", help="Log a blocker on active project")
    block_parser.add_argument("text", nargs="+", help="Blocker description")

    # switch command
    switch_parser = subparsers.add_parser("switch", help="Switch active project (non-interactive)")
    switch_parser.add_argument("project_name", help="Project slug to switch to")

    # status command
    subparsers.add_parser("status", help="Show current status (one-line)")

    # work command
    work_parser = subparsers.add_parser("work", help="Start working on a project")
    work_parser.add_argument("project_name", nargs="?", help="Project slug")

    args = parser.parse_args()

    # Handle --dump
    if args.dump:
        from jm.export import dump_to_stdout, export_to_file
        from jm.storage.store import create_stores

        stores = create_stores()
        if args.output:
            path = export_to_file(*stores, output_path=Path(args.output))
            print(f"Exported to {path}")
        else:
            dump_to_stdout(*stores)
        return

    # Handle subcommands
    if args.command == "note":
        _cmd_note(" ".join(args.text))
        return

    if args.command == "block":
        _cmd_block(" ".join(args.text))
        return

    if args.command == "switch":
        _cmd_switch(args.project_name)
        return

    if args.command == "status":
        _cmd_status()
        return

    if args.command == "work":
        _cmd_work(args.project_name)
        return

    # Default: launch TUI
    from jm.app import JMApp

    app = JMApp()
    app.run()


def _cmd_note(text: str) -> None:
    """Append note to active project and journal."""
    from jm.models import JournalEntry, LogEntry
    from jm.storage.store import create_stores

    project_store, journal_store, _, active_store = create_stores()

    slug = active_store.get_active()
    if not slug:
        print("No active project. Run: jm work <project-slug>")
        sys.exit(1)

    project = project_store.get_project(slug)
    if not project:
        print(f"Project '{slug}' not found.")
        sys.exit(1)

    # Add to project log
    today = date.today()
    today_log = None
    for entry in project.log:
        if entry.date == today:
            today_log = entry
            break
    if today_log is None:
        today_log = LogEntry(date=today)
        project.log.insert(0, today_log)
    today_log.lines.append(text)
    project_store.save_project(project)

    # Add to journal
    time_str = datetime.now().strftime("%H:%M")
    journal_store.append(JournalEntry(
        time=time_str, entry_type="Note", project=project.name,
        details={"note": text},
    ))

    print(f"Note added to {project.name}: {text}")


def _cmd_block(text: str) -> None:
    """Log a blocker on active project."""
    import re

    from jm.models import Blocker, JournalEntry
    from jm.storage.store import create_stores

    project_store, journal_store, _, active_store = create_stores()

    slug = active_store.get_active()
    if not slug:
        print("No active project. Run: jm work <project-slug>")
        sys.exit(1)

    project = project_store.get_project(slug)
    if not project:
        print(f"Project '{slug}' not found.")
        sys.exit(1)

    # Extract @mention
    person = None
    mention_match = re.search(r"@(\w+)", text)
    if mention_match:
        person = f"@{mention_match.group(1)}"

    project.blockers.append(Blocker(description=text, person=person, since=date.today()))
    project_store.save_project(project)

    # Journal
    time_str = datetime.now().strftime("%H:%M")
    journal_store.append(JournalEntry(
        time=time_str, entry_type="Note", project=project.name,
        details={"blocker": text},
    ))

    print(f"Blocker logged on {project.name}: {text}")


def _cmd_switch(slug: str) -> None:
    """Non-interactive switch -- just set active project."""
    from jm.models import JournalEntry
    from jm.storage.store import create_stores

    project_store, journal_store, _, active_store = create_stores()

    project = project_store.get_project(slug)
    if not project:
        print(f"Project '{slug}' not found.")
        sys.exit(1)

    old_slug = active_store.get_active()
    old_name = ""
    if old_slug:
        old_project = project_store.get_project(old_slug)
        old_name = old_project.name if old_project else old_slug

    active_store.set_active(slug)

    time_str = datetime.now().strftime("%H:%M")

    if old_name:
        journal_store.append(JournalEntry(
            time=time_str, entry_type="Switched",
            project=f"{old_name} \u2192 {project.name}", details={},
        ))

    journal_store.append(JournalEntry(
        time=time_str, entry_type="Started",
        project=project.name, details={},
    ))

    print(f"Switched to: {project.name}")


def _cmd_status() -> None:
    """One-line status output."""
    from jm.storage.store import create_stores

    project_store, journal_store, _, active_store = create_stores()

    active_slug = active_store.get_active()
    if not active_slug:
        print("No active project")
        return

    project = project_store.get_project(active_slug)
    if not project:
        print(f"Active: {active_slug} (project file missing)")
        return

    # Count open blockers
    open_blockers = sum(1 for b in project.blockers if not b.resolved)

    # Find when started
    journal = journal_store.today()
    started = ""
    for entry in reversed(journal.entries):
        if entry.entry_type in ("Started", "Switched") and project.name in entry.project:
            started = f" (since {entry.time})"
            break

    blocker_str = f", {open_blockers} blockers" if open_blockers else ""
    focus_str = f" -- {project.current_focus}" if project.current_focus else ""

    print(f"Active: {project.name}{started}{blocker_str}{focus_str}")


def _cmd_work(slug: str | None) -> None:
    """Start working on a project (non-interactive)."""
    from jm.models import JournalEntry
    from jm.storage.store import create_stores

    project_store, journal_store, _, active_store = create_stores()

    if not slug:
        print("Usage: jm work <project-slug>")
        sys.exit(1)

    project = project_store.get_project(slug)
    if not project:
        print(f"Project '{slug}' not found.")
        sys.exit(1)

    active_store.set_active(slug)
    time_str = datetime.now().strftime("%H:%M")
    journal_store.append(JournalEntry(
        time=time_str, entry_type="Started",
        project=project.name,
        details={"focus": project.current_focus} if project.current_focus else {},
    ))
    print(f"Now working on: {project.name}")


if __name__ == "__main__":
    main()
