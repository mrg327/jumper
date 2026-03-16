"""Work/Resume screen — shows context from last time a project was worked on."""

from __future__ import annotations

from datetime import date, datetime
from typing import Optional

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Label, Static

from jm.models import JournalEntry
from jm.storage.store import JournalStore, ProjectStore


def find_last_switch_away(journal_store: JournalStore, project_name: str) -> Optional[JournalEntry]:
    """Find the most recent 'Switched' entry that switched AWAY from this project.

    Search today's journal first, then previous workdays (up to 14 days back).
    """
    # Check today
    today_journal = journal_store.today()
    for entry in reversed(today_journal.entries):
        if entry.entry_type == "Switched" and "\u2192" in entry.project:
            from_project = entry.project.split("\u2192")[0].strip()
            if from_project.lower() == project_name.lower():
                return entry

    # Check previous days (up to 14 days back)
    from datetime import timedelta
    today = date.today()
    for days_back in range(1, 15):
        check_date = today - timedelta(days=days_back)
        prev = journal_store.get_day(check_date)
        if prev:
            for entry in reversed(prev.entries):
                if entry.entry_type == "Switched" and "\u2192" in entry.project:
                    from_project = entry.project.split("\u2192")[0].strip()
                    if from_project.lower() == project_name.lower():
                        return entry

    return None


class ResumeScreen(Screen):
    """Shows context from last time this project was worked on."""

    BINDINGS = [
        Binding("escape", "dismiss", "Continue"),
        Binding("enter", "dismiss", "Continue"),
        Binding("y", "keep_blocker", "Keep Blocker"),
        Binding("n", "resolve_blocker", "Resolved"),
    ]

    def __init__(
        self,
        project,  # Project object
        last_switch: JournalEntry,
        project_store: ProjectStore,
        callback,
    ):
        super().__init__()
        self.project = project
        self.last_switch = last_switch
        self.project_store = project_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            yield Label(f"RESUMING: {self.project.name}", id="resume-title")

            details = self.last_switch.details

            if details.get("left_off"):
                yield Label("Last time:", classes="prompt-label")
                yield Static(f'  "{details["left_off"]}"', id="left-off-text")

            if details.get("next_step"):
                yield Label("Next step was:", classes="prompt-label")
                yield Static(f'  "{details["next_step"]}"', id="next-step-text")

            if details.get("blocker"):
                yield Label("Blocker noted:", classes="prompt-label")
                yield Static(f'  "{details["blocker"]}"', id="blocker-text")

                # Check if there's a matching unresolved blocker
                has_unresolved = any(
                    not b.resolved for b in self.project.blockers
                )
                if has_unresolved:
                    yield Static("  Still blocked? [y] yes  [n] resolved", id="blocker-prompt")

            yield Static("")
            yield Static("  Press Enter to continue working", id="continue-hint")
        yield Footer()

    def action_dismiss(self) -> None:
        self.app.pop_screen()
        self.callback()

    def action_keep_blocker(self) -> None:
        # Blocker still active -- just continue
        self.action_dismiss()

    def action_resolve_blocker(self) -> None:
        # Mark first unresolved blocker as resolved
        for blocker in self.project.blockers:
            if not blocker.resolved:
                blocker.resolved = True
                blocker.resolved_date = date.today()
                break
        self.project_store.save_project(self.project)
        self.app.pop_screen()
        self.callback()
