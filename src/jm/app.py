from textual.app import App, ComposeResult
from textual.widgets import Header, Footer, Static


class JMApp(App):
    """jm — Job Manager TUI"""

    TITLE = "jm — Job Manager"
    CSS_PATH = "styles/app.tcss"
    BINDINGS = [("q", "quit", "Quit")]

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static("Welcome to jm — Job Manager. No projects yet.", id="placeholder")
        yield Footer()
