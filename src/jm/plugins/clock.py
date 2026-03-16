"""Clock plugin — shows current time and date."""

from __future__ import annotations

from datetime import datetime

from textual.app import ComposeResult
from textual.widgets import Static

from jm.plugins.base import JMPlugin, PluginTick


class ClockPlugin(JMPlugin):
    """Displays the current time and date. Updates every second."""

    PLUGIN_NAME = "Clock"
    PLUGIN_DESCRIPTION = "Current time and date"
    NEEDS_TIMER = True

    DEFAULT_CSS = """
    ClockPlugin {
        height: auto;
        width: 100%;
        padding: 0 1;
        margin-bottom: 1;
    }
    ClockPlugin #clock-label {
        text-style: bold;
        color: $text-muted;
    }
    ClockPlugin #clock-display {
        color: $text;
    }
    """

    def compose(self) -> ComposeResult:
        yield Static("Clock", id="clock-label")
        yield Static(self._format_time(), id="clock-display")

    def on_mount(self) -> None:
        self._update_display()

    def on_plugin_tick(self, event: PluginTick) -> None:
        self._update_display()

    def _update_display(self) -> None:
        try:
            self.query_one("#clock-display", Static).update(self._format_time())
        except Exception:
            pass  # Not yet mounted

    @staticmethod
    def _format_time() -> str:
        now = datetime.now()
        return now.strftime("%H:%M  %a %b %d")
