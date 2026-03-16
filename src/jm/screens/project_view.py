from datetime import date, datetime

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Center, Vertical, VerticalScroll
from textual.screen import ModalScreen, Screen
from textual.widgets import Footer, Header, Input, Label, Static

from jm.storage.store import JournalStore, ProjectStore


class ProjectViewScreen(Screen):
    """Detailed view of a single project."""

    BINDINGS = [
        Binding("escape", "go_back", "Back"),
        Binding("e", "edit_focus", "Edit Focus"),
        Binding("S", "cycle_status", "Status"),
        Binding("P", "cycle_priority", "Priority"),
        Binding("t", "edit_tags", "Tags"),
        Binding("T", "edit_target", "Target"),
        Binding("x", "delete_project", "Delete"),
        Binding("m", "move_blocker", "Move Blocker"),
    ]

    def __init__(self, slug: str, project_store: ProjectStore, journal_store: JournalStore | None = None):
        super().__init__()
        self.slug = slug
        self.project_store = project_store
        self.journal_store = journal_store
        self.project = project_store.get_project(slug)

    def compose(self) -> ComposeResult:
        yield Header()
        with VerticalScroll():
            if self.project is None:
                yield Label(f"Project '{self.slug}' not found.")
            else:
                p = self.project

                yield Label(f"[bold]{p.name}[/bold]", id="project-title")
                yield Static(self._build_meta(), id="project-meta")

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

    def _build_meta(self) -> str:
        """Build the metadata string for the project-meta panel."""
        if not self.project:
            return ""
        p = self.project
        meta_parts = [f"Status: {p.status}", f"Priority: {p.priority}"]
        if p.tags:
            meta_parts.append(f"Tags: {', '.join(p.tags)}")
        if p.target:
            meta_parts.append(f"Target: {p.target.isoformat()}")
        meta_parts.append(f"Created: {p.created.isoformat()}")
        return "  ".join(meta_parts)

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

    def _refresh_meta(self) -> None:
        """Update the #project-meta Static with fresh metadata."""
        self.query_one("#project-meta", Static).update(self._build_meta())

    def _refresh_view(self) -> None:
        """Reload project from disk and refresh all panels."""
        self.project = self.project_store.get_project(self.slug)
        if not self.project:
            return
        self._refresh_meta()
        focus_text = self.project.current_focus if self.project.current_focus else "(no focus set)"
        self.query_one("#focus-panel", Static).update(f"  {focus_text}")
        self.query_one("#blockers-panel", Static).update(self._render_blockers())
        self.query_one("#decisions-panel", Static).update(self._render_decisions())
        self.query_one("#log-panel", Static).update(self._render_log())

    # ── Existing actions ──────────────────────────────────────────────────────

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
            self.project = self.project_store.get_project(self.slug)
            focus_panel = self.query_one("#focus-panel", Static)
            focus_text = self.project.current_focus if self.project and self.project.current_focus else "(no focus set)"
            focus_panel.update(f"  {focus_text}")

    # ── New actions ───────────────────────────────────────────────────────────

    def action_cycle_status(self) -> None:
        """Cycle project status: active → blocked → parked → done → active."""
        if not self.project:
            return
        statuses = ["active", "blocked", "parked", "done"]
        idx = statuses.index(self.project.status) if self.project.status in statuses else 0
        self.project.status = statuses[(idx + 1) % len(statuses)]
        self.project_store.save_project(self.project)
        self._refresh_meta()
        self.notify(f"Status: {self.project.status}")

    def action_cycle_priority(self) -> None:
        """Cycle project priority: high → medium → low → high."""
        if not self.project:
            return
        priorities = ["high", "medium", "low"]
        idx = priorities.index(self.project.priority) if self.project.priority in priorities else 1
        self.project.priority = priorities[(idx + 1) % len(priorities)]
        self.project_store.save_project(self.project)
        self._refresh_meta()
        self.notify(f"Priority: {self.project.priority}")

    def action_edit_tags(self) -> None:
        """Open modal to edit project tags."""
        if not self.project:
            return
        current_tags = ", ".join(self.project.tags) if self.project.tags else ""
        self.app.push_screen(
            EditTagsScreen(self.project.name, current_tags, self._on_tags_updated)
        )

    def _on_tags_updated(self, new_tags_str: str | None) -> None:
        if new_tags_str is None or not self.project:
            return
        if new_tags_str.strip():
            self.project.tags = [t.strip() for t in new_tags_str.split(",") if t.strip()]
        else:
            self.project.tags = []
        self.project_store.save_project(self.project)
        self._refresh_meta()
        self.notify("Tags updated")

    def action_edit_target(self) -> None:
        """Open modal to edit project target date."""
        if not self.project:
            return
        current_target = self.project.target.isoformat() if self.project.target else ""
        self.app.push_screen(
            EditTargetScreen(self.project.name, current_target, self._on_target_updated)
        )

    def _on_target_updated(self, new_target_str: str | None) -> None:
        if new_target_str is None or not self.project:
            return
        if new_target_str.strip():
            try:
                self.project.target = date.fromisoformat(new_target_str.strip())
            except ValueError:
                self.notify("Invalid date format. Use YYYY-MM-DD.", severity="error")
                return
        else:
            self.project.target = None
        self.project_store.save_project(self.project)
        self._refresh_meta()
        target_display = self.project.target.isoformat() if self.project.target else "cleared"
        self.notify(f"Target: {target_display}")

    def action_delete_project(self) -> None:
        """Open confirmation dialog to delete this project."""
        if not self.project:
            return
        self.app.push_screen(
            DeleteProjectScreen(self.project.name, self._on_delete_confirmed)
        )

    def _on_delete_confirmed(self, confirmed: bool) -> None:
        if not confirmed or not self.project:
            return
        self.project_store.delete_project(self.project.slug)
        self.notify(f"Deleted: {self.project.name}")
        self.app.pop_screen()

    def action_move_blocker(self) -> None:
        """Open screen to manage/move/delete an open blocker."""
        if not self.project:
            return
        open_blockers = [b for b in self.project.blockers if not b.resolved]
        if not open_blockers:
            self.notify("No open blockers to manage.")
            return
        self.app.push_screen(
            MoveBlockerScreen(
                self.project,
                self.project_store,
                self.journal_store,
                self._on_blocker_managed,
            )
        )

    def _on_blocker_managed(self, changed: bool) -> None:
        if changed:
            self._refresh_view()


# ── Modal sub-screens ─────────────────────────────────────────────────────────


class EditFocusScreen(ModalScreen):
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


class EditTagsScreen(ModalScreen):
    """Modal screen to edit project tags (comma-separated)."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, current_tags: str, callback):
        super().__init__()
        self.project_name = project_name
        self.current_tags = current_tags
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Tags for {self.project_name}:")
                yield Input(
                    id="tags-input",
                    value=self.current_tags,
                    placeholder="e.g., infra, hmi, urgent",
                )

    def on_input_submitted(self, event) -> None:
        self.app.pop_screen()
        self.callback(event.value)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)


class EditTargetScreen(ModalScreen):
    """Modal screen to edit project target date."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, current_target: str, callback):
        super().__init__()
        self.project_name = project_name
        self.current_target = current_target
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Target date for {self.project_name}:")
                yield Input(
                    id="target-input",
                    value=self.current_target,
                    placeholder="YYYY-MM-DD  (empty to clear)",
                )

    def on_input_submitted(self, event) -> None:
        self.app.pop_screen()
        self.callback(event.value)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(None)


class DeleteProjectScreen(ModalScreen):
    """Modal confirmation screen to delete a project."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project_name: str, callback):
        super().__init__()
        self.project_name = project_name
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Delete {self.project_name}? Type 'yes' to confirm:")
                yield Input(id="confirm-input", placeholder="yes")

    def on_input_submitted(self, event) -> None:
        confirmed = event.value.strip().lower() == "yes"
        self.app.pop_screen()
        self.callback(confirmed)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)


class MoveBlockerScreen(ModalScreen):
    """Select an open blocker to edit, move, or delete."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, project, project_store: ProjectStore, journal_store: JournalStore | None, callback):
        super().__init__()
        self.project = project
        self.project_store = project_store
        self.journal_store = journal_store
        self.callback = callback
        self.open_blockers = [b for b in project.blockers if not b.resolved]

    def compose(self) -> ComposeResult:
        from textual.widgets import OptionList

        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Manage blocker on {self.project.name}:")
                if self.open_blockers:
                    yield OptionList(id="blocker-select")
                else:
                    yield Static("  No open blockers.")

    def on_mount(self) -> None:
        if not self.open_blockers:
            return
        from textual.widgets import OptionList
        from textual.widgets.option_list import Option

        option_list = self.query_one("#blocker-select", OptionList)
        for i, b in enumerate(self.open_blockers):
            person = f" {b.person}" if b.person else ""
            option_list.add_option(Option(f"{b.description}{person}", id=str(i)))

    def on_option_list_option_selected(self, event) -> None:
        idx = int(str(event.option.id))
        blocker = self.open_blockers[idx]
        self.app.pop_screen()
        self.app.push_screen(
            BlockerActionScreen(
                blocker,
                self.project,
                self.project_store,
                self.journal_store,
                self.callback,
            )
        )

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)


class BlockerActionScreen(ModalScreen):
    """Choose action for a selected blocker: edit description, move, or delete."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, blocker, project, project_store: ProjectStore, journal_store: JournalStore | None, callback):
        super().__init__()
        self.blocker = blocker
        self.project = project
        self.project_store = project_store
        self.journal_store = journal_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        from textual.widgets import OptionList
        from textual.widgets.option_list import Option

        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label(f"Blocker: {self.blocker.description}")
                yield OptionList(
                    Option("Edit description", id="edit"),
                    Option("Move to another project", id="move"),
                    Option("Delete blocker", id="delete"),
                    id="action-list",
                )

    def on_option_list_option_selected(self, event) -> None:
        action = str(event.option.id)
        if action == "edit":
            self.app.pop_screen()
            self.app.push_screen(
                EditBlockerDescScreen(
                    self.blocker,
                    self.project,
                    self.project_store,
                    self.callback,
                )
            )
        elif action == "move":
            other_projects = [
                p for p in self.project_store.list_projects()
                if p.slug != self.project.slug
            ]
            self.app.pop_screen()
            self.app.push_screen(
                MoveBlockerToProjectScreen(
                    self.blocker,
                    self.project,
                    other_projects,
                    self.project_store,
                    self.journal_store,
                    self.callback,
                )
            )
        elif action == "delete":
            self.project.blockers.remove(self.blocker)
            self.project_store.save_project(self.project)
            self.app.pop_screen()
            self.callback(True)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)


class EditBlockerDescScreen(ModalScreen):
    """Edit the description of a blocker inline."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, blocker, project, project_store: ProjectStore, callback):
        super().__init__()
        self.blocker = blocker
        self.project = project
        self.project_store = project_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label("Edit blocker description:")
                yield Input(
                    id="blocker-desc-input",
                    value=self.blocker.description,
                    placeholder="Blocker description",
                )

    def on_input_submitted(self, event) -> None:
        new_desc = event.value.strip()
        if new_desc:
            self.blocker.description = new_desc
            self.project_store.save_project(self.project)
        self.app.pop_screen()
        self.callback(bool(new_desc))

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)


class MoveBlockerToProjectScreen(ModalScreen):
    """Select target project to move a blocker to."""

    BINDINGS = [Binding("escape", "cancel", "Cancel")]

    def __init__(self, blocker, source_project, other_projects: list, project_store: ProjectStore, journal_store: JournalStore | None, callback):
        super().__init__()
        self.blocker = blocker
        self.source_project = source_project
        self.other_projects = other_projects
        self.project_store = project_store
        self.journal_store = journal_store
        self.callback = callback

    def compose(self) -> ComposeResult:
        from textual.widgets import OptionList

        with Center():
            with Vertical(id="quick-input-dialog"):
                yield Label("Move blocker to project:")
                if self.other_projects:
                    yield OptionList(id="project-list")
                else:
                    yield Static("  No other projects available.")

    def on_mount(self) -> None:
        if not self.other_projects:
            return
        from textual.widgets import OptionList
        from textual.widgets.option_list import Option

        option_list = self.query_one("#project-list", OptionList)
        for i, p in enumerate(self.other_projects):
            option_list.add_option(Option(p.name, id=str(i)))

    def on_option_list_option_selected(self, event) -> None:
        idx = int(str(event.option.id))
        target_project = self.other_projects[idx]

        # Remove from source project
        self.source_project.blockers.remove(self.blocker)
        self.project_store.save_project(self.source_project)

        # Add to target project
        target_project.blockers.append(self.blocker)
        self.project_store.save_project(target_project)

        # Log the move in the journal if available
        if self.journal_store:
            from jm.models.journal import JournalEntry
            time_str = datetime.now().strftime("%H:%M")
            self.journal_store.append(JournalEntry(
                time=time_str,
                entry_type="Note",
                project=self.source_project.name,
                details={
                    "note": (
                        f"Blocker moved to {target_project.name}: "
                        f"{self.blocker.description}"
                    )
                },
            ))

        self.app.pop_screen()
        self.callback(True)

    def action_cancel(self) -> None:
        self.app.pop_screen()
        self.callback(False)
