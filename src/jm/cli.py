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
    subparsers.add_parser("work", help="Start working on a project")

    args = parser.parse_args()

    if args.dump:
        print("jm — Job Manager\n\nNo projects yet.")
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
