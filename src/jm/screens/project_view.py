from datetime import date

from textual.app import ComposeResult
from textual.screen import Screen
from textual.widgets import Header, Footer, Static, Label, Input
from textual.containers import Vertical, VerticalScroll, Center
from textual.binding import Binding

from jm.storage.store import ProjectStore


class ProjectViewScreen(Screen):
    """Detailed view of a single project."""

    BINDINGS = [
        Binding("escape", "go_back", "Back"),
        Binding("e", "edit_focus", "Edit Focus"),
    ]

    def __init__(self, slug: str, project_store: ProjectStore):
        super().__init__()
        self.slug = slug
        self.project_store = project_store
        self.project = project_store.get_project(slug)

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            if self.project is None:
                yield Label(f"Project '{self.slug}' not found.")
            else:
                p = self.project

                # Project header with metadata
                meta_parts = [f"Status: {p.status}", f"Priority: {p.priority}"]
                if p.tags:
                    meta_parts.append(f"Tags: {', '.join(p.tags)}")
                if p.target:
                    meta_parts.append(f"Target: {p.target.isoformat()}")
                meta_parts.append(f"Created: {p.created.isoformat()}")

                yield Label(f"[bold]{p.name}[/bold]", id="project-title")
                yield Static("  ".join(meta_parts), id="project-meta")

                # Current Focus section
                yield Label("CURRENT FOCUS", classes="section-title")
                focus_text = p.current_focus if p.current_focus else "(no focus set)"
                yield Static(f"  {focus_text}", id="focus-panel")

                # Blockers section
                yield Label("BLOCKERS", classes="section-title")
                yield Static(self._render_blockers(), id="blockers-panel")

                # Decisions section
                yield Label("DECISIONS", classes="section-title")
                yield Static(self._render_decisions(), id="decisions-panel")

                # Log section
                yield Label("LOG", classes="section-title")
                yield Static(self._render_log(), id="log-panel")
        yield Footer()

    def _render_blockers(self) -> str:
        if not self.project or not self.project.blockers:
            return "  No blockers"

        lines = []
        for b in self.project.blockers:
            if b.resolved:
                text = f"  \u2713 ~~{b.description}~~"
                if b.resolved_date:
                    text += f" (resolved {b.resolved_date.isoformat()})"
            else:
                text = f"  \u2298 {b.description}"
                if b.person:
                    text += f" {b.person}"
                if b.since:
                    delta = (date.today() - b.since).days
                    text += f" ({delta} days)"
            lines.append(text)
        return "\n".join(lines)

    def _render_decisions(self) -> str:
        if not self.project or not self.project.decisions:
            return "  No decisions logged"

        lines = []
        for d in self.project.decisions:
            lines.append(f"  [{d.date.isoformat()}] {d.choice}")
            if d.alternatives:
                lines.append(f"    Alternatives: {', '.join(d.alternatives)}")
        return "\n".join(lines)

    def _render_log(self) -> str:
        if not self.project or not self.project.log:
            return "  No log entries"

        lines = []
        for entry in self.project.log:
            lines.append(f"  --- {entry.date.isoformat()} ---")
            for line in entry.lines:
                lines.append(f"    \u2022 {line}")
        return "\n".join(lines)

    def action_go_back(self) -> None:
        self.app.pop_screen()

    def action_edit_focus(self) -> None:
        """Open inline editor for current focus."""
        if self.project:
            self.app.push_screen(
                EditFocusScreen(self.project, self.project_store, self._on_focus_updated)
            )

    def _on_focus_updated(self, updated: bool) -> None:
        if updated:
            # Reload and refresh
            self.project = self.project_store.get_project(self.slug)
            focus_panel = self.query_one("#focus-panel", Static)
            focus_text = self.project.current_focus if self.project and self.project.current_focus else "(no focus set)"
            focus_panel.update(f"  {focus_text}")


class EditFocusScreen(Screen):
    """Modal screen to edit a project's current focus."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, project, project_store: ProjectStore, callback):
        super().__init__()
        self.project = project
        self.project_store = project_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="edit-focus-dialog"):
                yield Label(f"Edit Focus: {self.project.name}")
                yield Input(
                    id="focus-input",
                    value=self.project.current_focus,
                    placeholder="What are you working on?",
                )

    def on_input_submitted(self, event) -> None:
        new_focus = event.value.strip()
        self.project.current_focus = new_focus
        self.project_store.save_project(self.project)
        self.app.pop_screen()
        self.callback(True)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)
