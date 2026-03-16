import argparse
import sys


def main():
    parser = argparse.ArgumentParser(prog="jm", description="Job Manager TUI")
    parser.add_argument("--dump", action="store_true", help="Export current state to stdout")
    parser.add_argument("--version", action="version", version="jm 0.1.0")

    subparsers = parser.add_subparsers(dest="command")
    subparsers.add_parser("note", help="Add a quick note")
    subparsers.add_parser("block", help="Log a blocker")
    subparsers.add_parser("switch", help="Switch active project")
    subparsers.add_parser("status", help="Show current status")
    work_parser = subparsers.add_parser("work", help="Start working on a project")
    work_parser.add_argument("project_name", nargs="?", help="Project slug")

    args = parser.parse_args()

    if args.dump:
        print("jm — Job Manager\n\nNo projects yet.")
        return

    if args.command == "work":
        from datetime import datetime

        from jm.models import JournalEntry
        from jm.storage.store import create_stores

        project_store, journal_store, _, active_store = create_stores()

        if not getattr(args, "project_name", None):
            print("Usage: jm work <project-slug>")
            return

        slug = args.project_name
        project = project_store.get_project(slug)
        if not project:
            print(f"Project '{slug}' not found.")
            return

        active_store.set_active(slug)
        time_str = datetime.now().strftime("%H:%M")
        journal_store.append(JournalEntry(
            time=time_str,
            entry_type="Started",
            project=project.name,
            details={"focus": project.current_focus} if project.current_focus else {},
        ))
        print(f"Now working on: {project.name}")
        return

    if args.command:
        print(f"Command '{args.command}' not yet implemented.")
        return

    # Default: launch TUI
    from jm.app import JMApp

    app = JMApp()
    app.run()


if __name__ == "__main__":
    main()
