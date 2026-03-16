from textual.app import App

from jm.config import ensure_dirs, load_config
from jm.storage.store import ActiveProjectStore, JournalStore, PeopleStore, ProjectStore


class JMApp(App):
    """jm — Job Manager TUI"""

    TITLE = "jm — Job Manager"
    CSS_PATH = "styles/app.tcss"

    def __init__(self):
        super().__init__()
        config = load_config()
        data_dir = ensure_dirs(config)
        self.project_store = ProjectStore(data_dir)
        self.journal_store = JournalStore(data_dir)
        self.people_store = PeopleStore(data_dir)
        self.active_store = ActiveProjectStore(data_dir)

    def on_mount(self) -> None:
        from jm.screens.dashboard import DashboardScreen

        self.push_screen(
            DashboardScreen(
                self.project_store,
                self.journal_store,
                self.people_store,
                self.active_store,
            )
        )
