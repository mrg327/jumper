"""Main dashboard screen for jm — shows projects, blockers, and today's log."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Center, Vertical, VerticalScroll
from textual.screen import Screen
from textual.widgets import DataTable, Footer, Header, Input, Label, Static

from jm.storage.store import ActiveProjectStore, JournalStore, PeopleStore, ProjectStore


class DashboardScreen(Screen):
    """Main dashboard showing projects, blockers, and today's log."""

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("j", "cursor_down", "Down", show=False),
        Binding("k", "cursor_up", "Up", show=False),
        Binding("enter", "open_project", "Open"),
        Binding("w", "work", "Work"),
        Binding("s", "switch", "Switch"),
        Binding("n", "note", "Note"),
        Binding("b", "block", "Block"),
        Binding("u", "unblock", "Unblock"),
        Binding("d", "decide", "Decide"),
        Binding("slash", "search", "Search"),
        Binding("r", "review", "Review"),
        Binding("p", "people", "People"),
        Binding("a", "add_project", "Add"),
        Binding("ctrl+e", "export", "Export"),
        Binding("question_mark", "help", "Help"),
    ]

    def __init__(
        self,
        project_store: ProjectStore,
        journal_store: JournalStore,
        people_store: PeopleStore,
        active_store: ActiveProjectStore,
    ):
        super().__init__()
        self.project_store = project_store
        self.journal_store = journal_store
        self.people_store = people_store
        self.active_store = active_store

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            # Active projects section
            yield Label("ACTIVE PROJECTS", classes="section-title")
            yield DataTable(id="project-table")

            # Blockers section
            yield Label("BLOCKERS", classes="section-title")
            yield Static(id="blockers-panel")

            # Today's log section
            yield Label("TODAY'S LOG", classes="section-title")
            yield Static(id="journal-panel")
        yield Footer()

    def on_mount(self) -> None:
        """Load data and populate the dashboard."""
        self._setup_project_table()
        self._refresh_data()

    def _setup_project_table(self) -> None:
        table = self.query_one("#project-table", DataTable)
        table.cursor_type = "row"
        table.add_columns("", "Project", "Status", "Pri", "Current Focus")

    def _refresh_data(self) -> None:
        """Reload all data from storage."""
        self._refresh_projects()
        self._refresh_blockers()
        self._refresh_journal()

    def _refresh_projects(self) -> None:
        table = self.query_one("#project-table", DataTable)
        table.clear()

        active_slug = self.active_store.get_active()
        projects = self.project_store.list_projects()

        # Sort: active first, then blocked, then parked, then done
        status_order = {"active": 0, "blocked": 1, "parked": 2, "done": 3}
        projects.sort(key=lambda p: (status_order.get(p.status, 99), p.name))

        for project in projects:
            indicator = "\u25cf" if project.status in ("active", "blocked") else "\u25cb"
            if project.slug == active_slug:
                indicator = "\u25b6"  # Currently working on this

            pri_display = {"high": "high", "medium": "med", "low": "low"}.get(
                project.priority, project.priority
            )
            focus = project.current_focus[:40] if project.current_focus else ""

            table.add_row(
                indicator,
                project.name,
                project.status,
                pri_display,
                focus,
                key=project.slug,
            )

    def _refresh_blockers(self) -> None:
        panel = self.query_one("#blockers-panel", Static)
        projects = self.project_store.list_projects()

        blocker_lines: list[str] = []
        for project in projects:
            for blocker in project.blockers:
                if not blocker.resolved:
                    days = ""
                    if blocker.since:
                        from datetime import date

                        delta = date.today() - blocker.since
                        days = f" ({delta.days} days)"
                    person = f" {blocker.person}" if blocker.person else ""
                    blocker_lines.append(
                        f"  \u2298 {project.name}: {blocker.description}{person}{days}"
                    )

        if blocker_lines:
            count = len(blocker_lines)
            panel.update(f"[{count} open]\n" + "\n".join(blocker_lines))
        else:
            panel.update("  No open blockers")

    def _refresh_journal(self) -> None:
        panel = self.query_one("#journal-panel", Static)
        journal = self.journal_store.today()

        if not journal.entries:
            panel.update("  No entries yet today")
            return

        lines: list[str] = []
        for entry in journal.entries:
            if entry.entry_type == "Switched":
                lines.append(f"  {entry.time}  Switched \u2192 {entry.project}")
            elif entry.entry_type == "Done":
                lines.append(f"  {entry.time}  Done for day")
            else:
                lines.append(f"  {entry.time}  {entry.entry_type} {entry.project}")

        panel.update("\n".join(lines))

    def _get_selected_slug(self) -> str | None:
        """Get the slug of the currently selected project in the table."""
        table = self.query_one("#project-table", DataTable)
        if table.row_count == 0:
            return None
        try:
            row_key = table.coordinate_to_cell_key(table.cursor_coordinate).row_key
            return str(row_key.value)
        except Exception:
            return None

    # -- Vim-style navigation --

    def action_cursor_down(self) -> None:
        table = self.query_one("#project-table", DataTable)
        table.action_cursor_down()

    def action_cursor_up(self) -> None:
        table = self.query_one("#project-table", DataTable)
        table.action_cursor_up()

    # -- Implemented actions --

    def action_open_project(self) -> None:
        slug = self._get_selected_slug()
        if slug:
            from jm.screens.project_view import ProjectViewScreen

            self.app.push_screen(ProjectViewScreen(slug, self.project_store))

    def action_work(self) -> None:
        """Start working on the selected project. Shows resume info if available."""
        slug = self._get_selected_slug()
        if not slug:
            return

        project = self.project_store.get_project(slug)
        if not project:
            return

        # Set as active and log to journal
        self.active_store.set_active(slug)

        from datetime import datetime
        from jm.models import JournalEntry

        time_str = datetime.now().strftime("%H:%M")

        details = {}
        if project.current_focus:
            details["focus"] = project.current_focus

        self.journal_store.append(JournalEntry(
            time=time_str,
            entry_type="Started",
            project=project.name,
            details=details,
        ))

        # Check if there's resume context to show
        from jm.screens.work import find_last_switch_away, ResumeScreen

        last_switch = find_last_switch_away(self.journal_store, project.name)

        if last_switch and last_switch.details:
            # Show resume screen with previous context
            self.app.push_screen(ResumeScreen(
                project, last_switch, self.project_store, self._on_work_resumed
            ))
        else:
            self.notify(f"Working on: {project.name}")
            self._refresh_data()

    def _on_work_resumed(self) -> None:
        self._refresh_data()
        active = self.active_store.get_active()
        if active:
            project = self.project_store.get_project(active)
            name = project.name if project else active
            self.notify(f"Working on: {name}")

    def action_add_project(self) -> None:
        """Show input for new project name."""
        self.app.push_screen(
            AddProjectScreen(self.project_store, self._on_project_added)
        )

    def _on_project_added(self, result: bool) -> None:
        if result:
            self._refresh_data()

    # -- Stub actions --

    def action_note(self) -> None:
        """Quick note on active project."""
        slug = self.active_store.get_active() or self._get_selected_slug()
        if not slug:
            self.notify("No active project. Press 'w' first.")
            return
        project = self.project_store.get_project(slug)
        if not project:
            return

        from jm.widgets.quick_input import QuickNoteScreen

        self.app.push_screen(
            QuickNoteScreen(project.name, lambda text: self._handle_note(slug, text))
        )

    def _handle_note(self, slug: str, text: str | None) -> None:
        if not text:
            return
        from datetime import date, datetime
        from jm.models import LogEntry, JournalEntry

        project = self.project_store.get_project(slug)
        if not project:
            return

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
        self.project_store.save_project(project)

        # Add to journal
        time_str = datetime.now().strftime("%H:%M")
        self.journal_store.append(
            JournalEntry(
                time=time_str,
                entry_type="Note",
                project=project.name,
                details={"note": text},
            )
        )
        self.notify(f"Note saved to {project.name}")
        self._refresh_data()

    def action_switch(self) -> None:
        """Open context-switch capture screen."""
        active_slug = self.active_store.get_active()
        if not active_slug:
            self.notify(
                "No active project to switch from. Press 'w' to start working first."
            )
            return

        from jm.screens.switch import SwitchScreen

        self.app.push_screen(
            SwitchScreen(
                self.project_store,
                self.journal_store,
                self.active_store,
                self._on_switch_complete,
            )
        )

    def _on_switch_complete(self, result: bool) -> None:
        if result:
            self._refresh_data()
            active = self.active_store.get_active()
            if active:
                self.notify(f"Switched to: {active}")

    def action_block(self) -> None:
        """Log a blocker on active project."""
        slug = self.active_store.get_active() or self._get_selected_slug()
        if not slug:
            self.notify("No active project. Press 'w' first.")
            return
        project = self.project_store.get_project(slug)
        if not project:
            return

        from jm.widgets.quick_input import QuickBlockerScreen

        self.app.push_screen(
            QuickBlockerScreen(
                project.name, lambda text: self._handle_blocker(slug, text)
            )
        )

    def _handle_blocker(self, slug: str, text: str | None) -> None:
        if not text:
            return
        import re
        from datetime import date, datetime
        from jm.models import Blocker, JournalEntry, Person, PendingItem

        project = self.project_store.get_project(slug)
        if not project:
            return

        # Extract @mention
        person = None
        mention_match = re.search(r"@(\w+)", text)
        if mention_match:
            person = f"@{mention_match.group(1)}"
            # Also update people store
            self.people_store.add_or_update_person(
                Person(
                    handle=person,
                    projects=[project.name],
                    pending=[
                        PendingItem(
                            description=text,
                            since=date.today(),
                            project=project.name,
                        )
                    ],
                )
            )

        project.blockers.append(
            Blocker(description=text, person=person, since=date.today())
        )
        self.project_store.save_project(project)

        # Journal
        time_str = datetime.now().strftime("%H:%M")
        self.journal_store.append(
            JournalEntry(
                time=time_str,
                entry_type="Note",
                project=project.name,
                details={"blocker": text},
            )
        )
        self.notify(f"Blocker logged on {project.name}")
        self._refresh_data()

    def action_unblock(self) -> None:
        """Resolve a blocker on active project."""
        slug = self.active_store.get_active() or self._get_selected_slug()
        if not slug:
            self.notify("No active project.")
            return
        project = self.project_store.get_project(slug)
        if not project:
            return

        open_blockers = [b for b in project.blockers if not b.resolved]
        if not open_blockers:
            self.notify("No open blockers")
            return

        from jm.widgets.quick_input import UnblockScreen

        self.app.push_screen(
            UnblockScreen(
                project,
                self.project_store,
                lambda desc: self._handle_unblock(slug, desc),
            )
        )

    def _handle_unblock(self, slug: str, description: str | None) -> None:
        if not description:
            return
        from datetime import datetime
        from jm.models import JournalEntry

        project = self.project_store.get_project(slug)
        name = project.name if project else slug

        time_str = datetime.now().strftime("%H:%M")
        self.journal_store.append(
            JournalEntry(
                time=time_str,
                entry_type="Note",
                project=name,
                details={"resolved": description},
            )
        )
        self.notify(f"Blocker resolved on {name}")
        self._refresh_data()

    def action_decide(self) -> None:
        """Log a decision on active project."""
        slug = self.active_store.get_active() or self._get_selected_slug()
        if not slug:
            self.notify("No active project. Press 'w' first.")
            return
        project = self.project_store.get_project(slug)
        if not project:
            return

        from jm.widgets.quick_input import QuickDecisionScreen

        self.app.push_screen(
            QuickDecisionScreen(
                project.name, lambda text: self._handle_decision(slug, text)
            )
        )

    def _handle_decision(self, slug: str, text: str | None) -> None:
        if not text:
            return
        from datetime import date, datetime
        from jm.models import Decision, JournalEntry

        project = self.project_store.get_project(slug)
        if not project:
            return

        project.decisions.append(Decision(date=date.today(), choice=text))
        self.project_store.save_project(project)

        time_str = datetime.now().strftime("%H:%M")
        self.journal_store.append(
            JournalEntry(
                time=time_str,
                entry_type="Note",
                project=project.name,
                details={"decision": text},
            )
        )
        self.notify(f"Decision logged on {project.name}")
        self._refresh_data()

    def action_search(self) -> None:
        """Open search screen."""
        from jm.screens.search import SearchScreen

        self.app.push_screen(SearchScreen(project_store=self.project_store))

    def action_review(self) -> None:
        """Open morning review screen."""
        from jm.screens.review import ReviewScreen

        self.app.push_screen(
            ReviewScreen(self.project_store, self.journal_store, self.active_store)
        )

    def action_people(self) -> None:
        """Open people view."""
        from jm.screens.people import PeopleScreen

        self.app.push_screen(PeopleScreen(self.people_store, self.project_store))

    def action_export(self) -> None:
        self.notify("Export: not yet implemented")

    def action_help(self) -> None:
        self.notify("Help: not yet implemented")

    def action_quit(self) -> None:
        self.app.exit()


class AddProjectScreen(Screen):
    """Simple modal to add a new project."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, project_store: ProjectStore, callback):  # noqa: ANN001
        super().__init__()
        self.project_store = project_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="add-project-dialog"):
                yield Label("New Project Name:")
                yield Input(id="project-name-input", placeholder="e.g., HMI Framework")

    def on_input_submitted(self, event) -> None:  # noqa: ANN001
        name = event.value.strip()
        if name:
            self.project_store.create_project(name)
            self.app.pop_screen()
            self.callback(True)
        else:
            self.app.pop_screen()
            self.callback(False)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)
