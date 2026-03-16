"""Plugin sidebar container for the dashboard."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.timer import Timer
from textual.widget import Widget
from textual.widgets import Label, Static

from jm.plugins import PluginTick, discover_plugins
from jm.plugins.base import JMPlugin


class PluginSidebar(Widget):
    """Sidebar container that auto-discovers and mounts plugins.

    Dispatches PluginTick messages every second to plugins with NEEDS_TIMER=True.
    """

    DEFAULT_CSS = """
    PluginSidebar {
        width: 28;
        min-width: 20;
        max-width: 40;
        height: 100%;
        border-left: solid $surface-darken-2;
        padding: 0;
    }

    PluginSidebar:focus-within {
        border-left: solid $accent;
    }

    PluginSidebar #sidebar-title {
        text-style: bold;
        color: $text-muted;
        text-align: center;
        padding: 0 1;
        margin-bottom: 1;
    }
    """

    def __init__(self, enabled_plugins: list[str] | None = None) -> None:
        super().__init__()
        self._enabled_plugins = enabled_plugins
        self._timer: Timer | None = None
        self._timer_plugins: list[JMPlugin] = []

    def compose(self) -> ComposeResult:
        yield Label("PLUGINS", id="sidebar-title")

        plugin_classes = discover_plugins()

        # Filter by enabled list if configured
        if self._enabled_plugins is not None:
            name_to_cls = {cls.PLUGIN_NAME.lower(): cls for cls in plugin_classes}
            ordered: list[type[JMPlugin]] = []
            for name in self._enabled_plugins:
                cls = name_to_cls.get(name.lower())
                if cls:
                    ordered.append(cls)
            plugin_classes = ordered

        if not plugin_classes:
            yield Static("No plugins enabled", id="no-plugins")
            return

        for cls in plugin_classes:
            yield cls()

    def on_mount(self) -> None:
        """Start the shared timer for all plugins that need it."""
        self._timer_plugins = [
            w for w in self.query(JMPlugin) if w.NEEDS_TIMER
        ]
        if self._timer_plugins:
            self._timer = self.set_interval(1.0, self._dispatch_tick)

    def _dispatch_tick(self) -> None:
        """Send PluginTick to all timer-enabled plugins."""
        tick = PluginTick()
        for plugin in self._timer_plugins:
            plugin.on_plugin_tick(tick)
