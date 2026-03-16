"""Morning review screen — yesterday's journal, stale blockers, focus picker."""

from __future__ import annotations

from datetime import date, datetime, timedelta

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Label, Static, OptionList
from textual.widgets.option_list import Option

from jm.models import JournalEntry
from jm.storage.store import ProjectStore, JournalStore, ActiveProjectStore


class ReviewScreen(Screen):
    """Morning review — yesterday's journal, stale blockers, focus picker."""

    BINDINGS = [
        Binding("escape", "go_back", "Back"),
        Binding("enter", "start_day", "Start Day"),
    ]

    def __init__(
        self,
        project_store: ProjectStore,
        journal_store: JournalStore,
        active_store: ActiveProjectStore,
    ):
        super().__init__()
        self.project_store = project_store
        self.journal_store = journal_store
        self.active_store = active_store
        self.stale_threshold = 7  # days

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            today_str = date.today().strftime("%a %b %d, %Y")
            yield Label(f"MORNING REVIEW — {today_str}", id="review-title")

            # Yesterday's journal
            yield Label("YESTERDAY", classes="section-title")
            yield Static(id="yesterday-panel")

            # Stale blockers
            yield Label("OPEN BLOCKERS", classes="section-title")
            yield Static(id="blockers-panel")

            # Stale projects
            yield Label(
                "STALE PROJECTS (not touched in 7+ days)", classes="section-title"
            )
            yield Static(id="stale-panel")

            # Focus picker
            yield Label(
                "TODAY'S FOCUS (select a project to start)", classes="section-title"
            )
            yield OptionList(id="focus-picker")
        yield Footer()

    def on_mount(self) -> None:
        self._render_yesterday()
        self._render_blockers()
        self._render_stale()
        self._render_focus_picker()

    def _render_yesterday(self) -> None:
        panel = self.query_one("#yesterday-panel", Static)
        prev = self.journal_store.get_previous_workday()

        if not prev:
            panel.update("  No previous journal found")
            return

        day_str = prev.date.strftime("%a %b %d")
        lines = [f"  {day_str}:"]
        for entry in prev.entries:
            if entry.entry_type == "Done":
                lines.append(f"  {entry.time}  Done for day")
            elif entry.entry_type == "Switched":
                lines.append(f"  {entry.time}  Switched \u2192 {entry.project}")
                if entry.details.get("left_off"):
                    lines.append(
                        f"           Left off: \"{entry.details['left_off']}\""
                    )
            else:
                lines.append(f"  {entry.time}  {entry.entry_type} {entry.project}")
                if entry.details.get("decision"):
                    lines.append(
                        f"           Decision: {entry.details['decision']}"
                    )

        panel.update("\n".join(lines))

    def _render_blockers(self) -> None:
        panel = self.query_one("#blockers-panel", Static)
        projects = self.project_store.list_projects()

        blocker_lines = []
        for project in projects:
            for blocker in project.blockers:
                if not blocker.resolved:
                    days = ""
                    if blocker.since:
                        delta = (date.today() - blocker.since).days
                        days = f" ({delta} days)"
                    person = f" {blocker.person}" if blocker.person else ""
                    blocker_lines.append(
                        f"  \u2298 {project.name}: {blocker.description}{person}{days}"
                    )

        if blocker_lines:
            panel.update(
                f"  [{len(blocker_lines)} open]\n" + "\n".join(blocker_lines)
            )
        else:
            panel.update("  No open blockers!")

    def _render_stale(self) -> None:
        panel = self.query_one("#stale-panel", Static)
        projects = self.project_store.list_projects()
        cutoff = date.today() - timedelta(days=self.stale_threshold)

        stale = []
        for p in projects:
            if p.status in ("active", "blocked"):
                # Check last log entry date
                last_touched = p.created
                if p.log:
                    last_touched = max(entry.date for entry in p.log)

                if last_touched < cutoff:
                    days = (date.today() - last_touched).days
                    stale.append(
                        f"  \u25cb {p.name} \u2014 last touched "
                        f"{last_touched.isoformat()} ({days} days ago)"
                    )

        if stale:
            panel.update("\n".join(stale))
        else:
            panel.update("  All projects recently active")

    def _render_focus_picker(self) -> None:
        picker = self.query_one("#focus-picker", OptionList)
        projects = self.project_store.list_projects()

        for p in projects:
            if p.status in ("active", "blocked"):
                focus = f" \u2014 \"{p.current_focus}\"" if p.current_focus else ""
                picker.add_option(Option(f"{p.name}{focus}", id=p.slug))

    def on_option_list_option_selected(self, event) -> None:  # noqa: ANN001
        """User picked a focus project — start working on it."""
        slug = str(event.option.id)
        project = self.project_store.get_project(slug)
        if project:
            self.active_store.set_active(slug)

            # Log to journal
            time_str = datetime.now().strftime("%H:%M")
            details = {}
            if project.current_focus:
                details["focus"] = project.current_focus
            self.journal_store.append(
                JournalEntry(
                    time=time_str,
                    entry_type="Started",
                    project=project.name,
                    details=details,
                )
            )

            self.app.pop_screen()
            self.notify(f"Started day working on: {project.name}")

    def action_start_day(self) -> None:
        """Start the day without picking a focus — just go to dashboard."""
        self.app.pop_screen()

    def action_go_back(self) -> None:
        self.app.pop_screen()
