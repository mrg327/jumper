"""People/stakeholder view — shows all @mentioned people and their pending items."""

from __future__ import annotations

from datetime import date

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Label, Static

from jm.storage.store import PeopleStore, ProjectStore


class PeopleScreen(Screen):
    """People/stakeholder view — shows all @mentioned people and their pending items."""

    BINDINGS = [
        Binding("escape", "go_back", "Back"),
        Binding("j", "cursor_down", "Down", show=False),
        Binding("k", "cursor_up", "Up", show=False),
    ]

    def __init__(self, people_store: PeopleStore, project_store: ProjectStore):
        super().__init__()
        self.people_store = people_store
        self.project_store = project_store

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            yield Label("PEOPLE & STAKEHOLDERS", classes="section-title")
            yield Static(id="people-panel")

            yield Label("WAITING ON (open blockers by person)", classes="section-title")
            yield Static(id="waiting-panel")
        yield Footer()

    def on_mount(self) -> None:
        self._refresh()

    def _refresh(self) -> None:
        # Build people info from people file
        people_file = self.people_store.load()

        # Build people panel
        if people_file.people:
            lines = []
            for person in people_file.people:
                line = f"  {person.handle}"
                if person.role:
                    line += f" — {person.role}"
                if person.projects:
                    line += f" (projects: {', '.join(person.projects)})"
                lines.append(line)

                for item in person.pending:
                    since_str = f" (since {item.since.isoformat()})" if item.since else ""
                    proj_str = f" [{item.project}]" if item.project else ""
                    lines.append(f"    \u2298 {item.description}{proj_str}{since_str}")

            self.query_one("#people-panel", Static).update("\n".join(lines))
        else:
            self.query_one("#people-panel", Static).update(
                "  No people tracked yet. @mentions in blockers will appear here."
            )

        # Build waiting-on view from project blockers
        projects = self.project_store.list_projects()
        waiting: dict[str, list[str]] = {}  # person -> list of blocker descriptions

        for project in projects:
            for blocker in project.blockers:
                if not blocker.resolved and blocker.person:
                    person = blocker.person
                    if person not in waiting:
                        waiting[person] = []
                    days = ""
                    if blocker.since:
                        delta = (date.today() - blocker.since).days
                        days = f" ({delta} days)"
                    waiting[person].append(f"{project.name}: {blocker.description}{days}")

        if waiting:
            lines = []
            for person, items in sorted(waiting.items()):
                lines.append(f"  {person}:")
                for item in items:
                    lines.append(f"    \u2298 {item}")
            self.query_one("#waiting-panel", Static).update("\n".join(lines))
        else:
            self.query_one("#waiting-panel", Static).update("  No outstanding items")

    def action_go_back(self) -> None:
        self.app.pop_screen()

    def action_cursor_down(self) -> None:
        pass  # Scrolling handled by VerticalScroll

    def action_cursor_up(self) -> None:
        pass
