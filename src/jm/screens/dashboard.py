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
            self.notify(f"Open project: {slug} (not yet implemented)")

    def action_work(self) -> None:
        slug = self._get_selected_slug()
        if slug:
            self.active_store.set_active(slug)
            self.notify(f"Now working on: {slug}")
            self._refresh_data()

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
        self.notify("Note: not yet implemented")

    def action_switch(self) -> None:
        self.notify("Switch: not yet implemented")

    def action_block(self) -> None:
        self.notify("Block: not yet implemented")

    def action_unblock(self) -> None:
        self.notify("Unblock: not yet implemented")

    def action_decide(self) -> None:
        self.notify("Decide: not yet implemented")

    def action_search(self) -> None:
        self.notify("Search: not yet implemented")

    def action_review(self) -> None:
        self.notify("Review: not yet implemented")

    def action_people(self) -> None:
        self.notify("People: not yet implemented")

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
