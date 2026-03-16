"""Quick-input modal screens for note, blocker, decision, and unblock actions.

Each screen is a lightweight modal with a single input field (or option list for
unblock). All are submittable with Enter and cancelable with Escape.
"""

from __future__ import annotations

import re
from datetime import date, datetime
from typing import Optional

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Center, Vertical
from textual.screen import ModalScreen
from textual.widgets import Input, Label, Static

from jm.models import Blocker, Decision, LogEntry, JournalEntry, Person, PendingItem
from jm.storage.store import ProjectStore, JournalStore, PeopleStore, ActiveProjectStore


class QuickNoteScreen(ModalScreen):
    """Quick note on active project — single input, Enter to submit."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, callback):  # noqa: ANN001
        super().__init__()
        self.project_name = project_name
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Note on {self.project_name}:")
                yield Input(id="note-input", placeholder="Quick thought...")

    def on_input_submitted(self, event) -> None:  # noqa: ANN001
        text = event.value.strip()
        self.app.pop_screen()
        self.callback(text if text else None)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)


class QuickBlockerScreen(ModalScreen):
    """Log a blocker — single input with @mention detection."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, callback):  # noqa: ANN001
        super().__init__()
        self.project_name = project_name
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Blocker on {self.project_name}:")
                yield Input(
                    id="blocker-input",
                    placeholder="e.g., waiting on @carol for spec review",
                )

    def on_input_submitted(self, event) -> None:  # noqa: ANN001
        text = event.value.strip()
        self.app.pop_screen()
        self.callback(text if text else None)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)


class QuickDecisionScreen(ModalScreen):
    """Log a decision — single input."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, callback):  # noqa: ANN001
        super().__init__()
        self.project_name = project_name
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Decision on {self.project_name}:")
                yield Input(
                    id="decision-input",
                    placeholder="Chose X over Y because Z",
                )

    def on_input_submitted(self, event) -> None:  # noqa: ANN001
        text = event.value.strip()
        self.app.pop_screen()
        self.callback(text if text else None)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)


class UnblockScreen(ModalScreen):
    """Show open blockers for active project, select one to resolve."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, project, project_store: ProjectStore, callback):  # noqa: ANN001
        super().__init__()
        self.project = project
        self.project_store = project_store
        self.callback = callback
        self.open_blockers = [b for b in project.blockers if not b.resolved]

    def compose(self) -> ComposeResult:
        from textual.widgets import OptionList
        from textual.widgets.option_list import Option

        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Resolve blocker on {self.project.name}:")
                if self.open_blockers:
                    yield OptionList(id="blocker-list")
                else:
                    yield Static("No open blockers")

    def on_mount(self) -> None:
        if self.open_blockers:
            from textual.widgets import OptionList
            from textual.widgets.option_list import Option

            option_list = self.query_one("#blocker-list", OptionList)
            for i, b in enumerate(self.open_blockers):
                person = f" {b.person}" if b.person else ""
                option_list.add_option(Option(f"{b.description}{person}", id=str(i)))

    def on_option_list_option_selected(self, event) -> None:  # noqa: ANN001
        idx = int(str(event.option.id))
        blocker = self.open_blockers[idx]
        blocker.resolved = True
        blocker.resolved_date = date.today()
        self.project_store.save_project(self.project)
        self.app.pop_screen()
        self.callback(blocker.description)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)
