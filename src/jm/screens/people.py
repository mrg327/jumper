"""People/stakeholder view — shows all @mentioned people and their pending items."""

from __future__ import annotations

from datetime import date

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Label, OptionList, Static
from textual.widgets.option_list import Option

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
        # Parallel list to OptionList items: (project_slug, blocker_index)
        self._waiting_refs: list[tuple[str, int]] = []

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            yield Label("PEOPLE & STAKEHOLDERS", classes="section-title")
            yield Static(id="people-panel")

            yield Label("WAITING ON (open blockers by person)", classes="section-title")
            yield Label("  Press Enter on an item to mark it resolved", id="waiting-hint")
            yield OptionList(id="waiting-panel")
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
        waiting_option_list = self.query_one("#waiting-panel", OptionList)
        waiting_option_list.clear_options()
        self._waiting_refs = []

        for project in projects:
            for idx, blocker in enumerate(project.blockers):
                if not blocker.resolved and blocker.person:
                    days = ""
                    if blocker.since:
                        delta = (date.today() - blocker.since).days
                        days = f" ({delta} days)"
                    label = f"{blocker.person}  \u2298  {project.name}: {blocker.description}{days}"
                    opt_id = str(len(self._waiting_refs))
                    waiting_option_list.add_option(Option(label, id=opt_id))
                    self._waiting_refs.append((project.slug, idx))

        if not self._waiting_refs:
            waiting_option_list.add_option(Option("  No outstanding items", id="empty"))

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        """Resolve the selected blocker."""
        opt_id = str(event.option.id)
        if opt_id == "empty":
            return
        ref_idx = int(opt_id)
        if ref_idx >= len(self._waiting_refs):
            return

        project_slug, blocker_idx = self._waiting_refs[ref_idx]
        project = self.project_store.get_project(project_slug)
        if not project:
            return
        if blocker_idx >= len(project.blockers):
            return

        blocker = project.blockers[blocker_idx]
        blocker.resolved = True
        blocker.resolved_date = date.today()
        self.project_store.save_project(project)

        self.notify(f"Resolved: {blocker.description}")
        self._refresh()

    def action_go_back(self) -> None:
        self.app.pop_screen()

    def action_cursor_down(self) -> None:
        pass  # Scrolling handled by VerticalScroll

    def action_cursor_up(self) -> None:
        pass
