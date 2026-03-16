"""Base class for all jm sidebar plugins."""

from __future__ import annotations

from textual.message import Message
from textual.widget import Widget


class PluginTick(Message):
    """Dispatched every second to plugins with NEEDS_TIMER = True."""


class PluginNotification(Message):
    """Sent by plugins to push a notification to the notification center."""

    def __init__(self, source_plugin: JMPlugin, message: str) -> None:
        super().__init__()
        self.source_name = source_plugin.PLUGIN_NAME
        self.notification_message = message


class RequestSidebarOpen(Message):
    """Posted by plugins to request the sidebar becomes visible.

    Bubbles up to the dashboard which will show the sidebar if hidden.
    """


class JMPlugin(Widget):
    """Base class for all jm sidebar plugins.

    Subclass this, set PLUGIN_NAME and PLUGIN_DESCRIPTION, and implement
    compose() to render your plugin. Plugins are self-contained widgets
    that live in the sidebar.

    Lifecycle:
    - on_mount(): called when plugin is added to the sidebar
    - on_plugin_tick(): called every second (for timers, clocks)
    - notify_user(msg): push a message to the notification plugin
    - request_attention(): request the sidebar to open if hidden
    """

    PLUGIN_NAME: str = "Plugin"
    PLUGIN_DESCRIPTION: str = ""
    NEEDS_TIMER: bool = False

    DEFAULT_CSS = """
    JMPlugin {
        height: auto;
        width: 100%;
        padding: 0 1;
        margin-bottom: 1;
    }
    """

    def notify_user(self, message: str) -> None:
        """Send a notification and request sidebar to open if hidden."""
        try:
            self.post_message(PluginNotification(self, message))
            self.post_message(RequestSidebarOpen())
        except Exception:
            pass  # Not mounted / no app context

    def request_attention(self) -> None:
        """Request the sidebar to open if hidden (without sending a notification)."""
        try:
            self.post_message(RequestSidebarOpen())
        except Exception:
            pass

    def on_plugin_tick(self, event: PluginTick) -> None:
        """Override to handle per-second ticks. Only called if NEEDS_TIMER = True."""
