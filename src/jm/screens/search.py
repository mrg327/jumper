"""Full-text search screen for jm — search across all markdown files."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import Screen
from textual.widgets import Footer, Header, Input, Label, OptionList
from textual.widgets.option_list import Option

from jm.storage.search import SearchEngine, SearchFilter


class SearchScreen(Screen):
    """Full-text search across all jm data."""

    BINDINGS = [
        Binding("escape", "go_back", "Back"),
    ]

    def __init__(self, project_store=None):
        super().__init__()
        self.project_store = project_store
        self.search_engine = SearchEngine()
        self.results = []

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical():
            yield Input(
                id="search-input",
                placeholder="Search across all projects, journals, and people...",
            )
            yield Label("", id="result-count")
            yield OptionList(id="search-results")
        yield Footer()

    def on_mount(self) -> None:
        self.query_one("#search-input", Input).focus()

    def on_input_changed(self, event) -> None:
        """Live search as user types."""
        query = event.value.strip()
        if len(query) < 2:
            self._clear_results()
            return
        self._perform_search(query)

    def _perform_search(self, query: str) -> None:
        """Run search and display results."""
        self.results = self.search_engine.quick_search(query)

        result_list = self.query_one("#search-results", OptionList)
        result_list.clear_options()

        count_label = self.query_one("#result-count", Label)
        count_label.update(f"  {len(self.results)} results")

        for i, result in enumerate(self.results[:50]):  # Cap at 50 results
            # Format: [type] project/date - matching line
            if result.file_type == "project":
                prefix = f"[project] {result.project_slug}"
            elif result.file_type == "journal":
                # Extract date from filename
                date_str = result.file_path.stem
                prefix = f"[journal] {date_str}"
            else:
                prefix = "[people]"

            # Truncate line text
            line = result.line_text.strip()[:60]
            display = f"{prefix} -- {line}"
            result_list.add_option(Option(display, id=str(i)))

    def _clear_results(self) -> None:
        result_list = self.query_one("#search-results", OptionList)
        result_list.clear_options()
        count_label = self.query_one("#result-count", Label)
        count_label.update("")
        self.results = []

    def on_option_list_option_selected(self, event) -> None:
        """Navigate to the selected result."""
        idx = int(str(event.option.id))
        if idx < len(self.results):
            result = self.results[idx]
            if result.file_type == "project" and result.project_slug:
                # Navigate to project view
                from jm.screens.project_view import ProjectViewScreen

                self.app.pop_screen()  # Pop search
                if self.project_store:
                    self.app.push_screen(
                        ProjectViewScreen(result.project_slug, self.project_store)
                    )
            else:
                # For journals/people, just show a notification with the match
                self.notify(
                    f"Match in {result.file_path.name}:{result.line_number}"
                )

    def action_go_back(self) -> None:
        self.app.pop_screen()
