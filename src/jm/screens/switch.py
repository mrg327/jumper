"""Context-switch capture screen — the killer feature of jm.

When the user presses 's' on the dashboard, this screen captures what they
were doing before allowing them to switch to another project.  All prompts
are optional (Enter skips), and the captured data is written to BOTH the
project file AND the daily journal on confirm.
"""

from __future__ import annotations

import re
from datetime import date, datetime
from typing import TYPE_CHECKING, Optional

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Input, Label, OptionList, Static
from textual.widgets.option_list import Option

from jm.models import Blocker, JournalEntry, LogEntry

if TYPE_CHECKING:
    from jm.storage.store import ActiveProjectStore, JournalStore, ProjectStore


class SwitchScreen(Screen):
    """Context-switch capture screen — the killer feature."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(
        self,
        project_store: ProjectStore,
        journal_store: JournalStore,
        active_store: ActiveProjectStore,
        callback,  # noqa: ANN001 — called with True/False when done
    ) -> None:
        super().__init__()
        self.project_store = project_store
        self.journal_store = journal_store
        self.active_store = active_store
        self.callback = callback

        # Load current state
        self.current_slug = active_store.get_active()
        self.current_project = None
        if self.current_slug:
            self.current_project = project_store.get_project(self.current_slug)

        # Capture data
        self.left_off = ""
        self.blocker = ""
        self.next_step = ""
        self.target_slug = ""

    # ------------------------------------------------------------------
    # Layout
    # ------------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            # Header info
            project_name = (
                self.current_project.name
                if self.current_project
                else "(no active project)"
            )
            yield Label(
                f"SWITCHING FROM: {project_name}", id="switch-title"
            )

            # Show original focus if available
            if self.current_project and self.current_project.current_focus:
                yield Static(
                    f'  Started with focus: "{self.current_project.current_focus}"',
                    id="original-focus",
                )

            # Capture prompts — all optional
            yield Label(
                "Where did you leave off? (Enter to skip)",
                classes="prompt-label",
            )
            yield Input(
                id="left-off-input",
                placeholder="e.g., checked compositor, it's adding 4ms delay",
            )

            yield Label(
                "Anything blocking? (Enter to skip)", classes="prompt-label"
            )
            yield Input(
                id="blocker-input",
                placeholder="e.g., need @carol's spec to know if 4ms is acceptable",
            )

            yield Label(
                "Next step when you come back? (Enter to skip)",
                classes="prompt-label",
            )
            yield Input(
                id="next-step-input",
                placeholder="e.g., test with compositor bypass flag",
            )

            # Project selector
            yield Label("Switch to:", classes="prompt-label")
            yield OptionList(id="project-selector")
        yield Footer()

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def on_mount(self) -> None:
        # Populate project selector with all projects except current
        option_list = self.query_one("#project-selector", OptionList)
        projects = self.project_store.list_projects()
        for project in projects:
            if project.slug != self.current_slug:
                option_list.add_option(Option(project.name, id=project.slug))

        # Focus the first input
        self.query_one("#left-off-input", Input).focus()

        # Disable inputs that aren't active yet
        self.query_one("#blocker-input", Input).disabled = True
        self.query_one("#next-step-input", Input).disabled = True
        self.query_one("#project-selector", OptionList).disabled = True

    # ------------------------------------------------------------------
    # Input flow  (left_off -> blocker -> next_step -> target selector)
    # ------------------------------------------------------------------

    def on_input_submitted(self, event) -> None:  # noqa: ANN001
        input_id = event.input.id
        value = event.value.strip()

        if input_id == "left-off-input":
            self.left_off = value
            blocker_input = self.query_one("#blocker-input", Input)
            blocker_input.disabled = False
            blocker_input.focus()

        elif input_id == "blocker-input":
            self.blocker = value
            next_input = self.query_one("#next-step-input", Input)
            next_input.disabled = False
            next_input.focus()

        elif input_id == "next-step-input":
            self.next_step = value
            selector = self.query_one("#project-selector", OptionList)
            selector.disabled = False
            selector.focus()

    # ------------------------------------------------------------------
    # Target selection
    # ------------------------------------------------------------------

    def on_option_list_option_selected(self, event) -> None:  # noqa: ANN001
        """User selected a target project — execute the switch."""
        self.target_slug = str(event.option.id)
        self._execute_switch()

    # ------------------------------------------------------------------
    # Core logic — write to project file + journal
    # ------------------------------------------------------------------

    def _execute_switch(self) -> None:
        """Save capture data and switch to new project."""
        now = datetime.now()
        time_str = now.strftime("%H:%M")
        today = date.today()

        # 1. Update the current project's log and blockers
        if self.current_project:
            log_lines: list[str] = []
            if self.left_off:
                log_lines.append(f"Left off: {self.left_off}")
            if self.next_step:
                log_lines.append(f"Next step: {self.next_step}")

            if log_lines:
                # Find or create today's log entry
                today_log: Optional[LogEntry] = None
                for entry in self.current_project.log:
                    if entry.date == today:
                        today_log = entry
                        break
                if today_log is None:
                    today_log = LogEntry(date=today)
                    self.current_project.log.insert(0, today_log)
                today_log.lines.extend(log_lines)

            # Add blocker if specified
            if self.blocker:
                person: Optional[str] = None
                mention_match = re.search(r"@(\w+)", self.blocker)
                if mention_match:
                    person = f"@{mention_match.group(1)}"

                self.current_project.blockers.append(
                    Blocker(
                        description=self.blocker, person=person, since=today
                    )
                )

            # Update current focus with next_step for when we resume
            if self.next_step:
                self.current_project.current_focus = self.next_step

            self.project_store.save_project(self.current_project)

        # 2. Write journal entry for the switch
        target_project = self.project_store.get_project(self.target_slug)
        target_name = (
            target_project.name if target_project else self.target_slug
        )
        current_name = (
            self.current_project.name if self.current_project else ""
        )

        details: dict[str, str] = {}
        if self.left_off:
            details["left_off"] = self.left_off
        if self.blocker:
            details["blocker"] = self.blocker
        if self.next_step:
            details["next_step"] = self.next_step

        switch_entry = JournalEntry(
            time=time_str,
            entry_type="Switched",
            project=(
                f"{current_name} \u2192 {target_name}"
                if current_name
                else target_name
            ),
            details=details,
        )
        self.journal_store.append(switch_entry)

        # 3. Write "Started" journal entry for the new project
        start_entry = JournalEntry(
            time=time_str,
            entry_type="Started",
            project=target_name,
            details={},
        )
        self.journal_store.append(start_entry)

        # 4. Set new active project
        self.active_store.set_active(self.target_slug)

        # 5. Return to dashboard
        self.app.pop_screen()
        self.callback(True)

    # ------------------------------------------------------------------
    # Cancel
    # ------------------------------------------------------------------

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)
