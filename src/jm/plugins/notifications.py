"""Notifications plugin — displays recent notifications from other plugins."""

from __future__ import annotations

from datetime import datetime, timedelta

from textual.app import ComposeResult
from textual.widgets import Static

from jm.plugins.base import JMPlugin, PluginNotification, PluginTick


class _Notification:
    """A single notification entry."""

    __slots__ = ("time", "source", "message")

    def __init__(self, source: str, message: str) -> None:
        self.time = datetime.now()
        self.source = source
        self.message = message


class NotificationsPlugin(JMPlugin):
    """Displays a scrollable list of recent notifications.

    Notifications come from other plugins via notify_user() / PluginNotification.
    Auto-expires after 30 minutes by default.
    """

    PLUGIN_NAME = "Notifications"
    PLUGIN_DESCRIPTION = "Recent notifications from plugins"
    NEEDS_TIMER = True

    MAX_NOTIFICATIONS = 10
    EXPIRE_MINUTES = 30

    DEFAULT_CSS = """
    NotificationsPlugin {
        height: auto;
        width: 100%;
        padding: 0 1;
        margin-bottom: 1;
    }
    NotificationsPlugin #notif-label {
        text-style: bold;
        color: $text-muted;
    }
    NotificationsPlugin #notif-display {
        color: $text;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self._notifications: list[_Notification] = []
        self._reminders: list[dict] = []
        self._fired_reminders: set[str] = set()

    def compose(self) -> ComposeResult:
        yield Static("Notifications", id="notif-label")
        yield Static("No notifications", id="notif-display")

    def configure_reminders(self, reminders: list[dict]) -> None:
        """Set schedule-based reminders from config.

        Each reminder: {"time": "09:00", "message": "Morning review"}
        """
        self._reminders = reminders

    def add_notification(self, source: str, message: str) -> None:
        """Add a notification directly (used by other plugins or internally)."""
        self._notifications.append(_Notification(source, message))
        # Trim to max
        if len(self._notifications) > self.MAX_NOTIFICATIONS:
            self._notifications = self._notifications[-self.MAX_NOTIFICATIONS:]
        self._update_display()

    def on_plugin_tick(self, event: PluginTick) -> None:
        """Check for expired notifications and scheduled reminders."""
        now = datetime.now()
        cutoff = now - timedelta(minutes=self.EXPIRE_MINUTES)

        # Expire old notifications
        before = len(self._notifications)
        self._notifications = [n for n in self._notifications if n.time > cutoff]

        # Check scheduled reminders
        now_hm = now.strftime("%H:%M")
        today_key = now.strftime("%Y-%m-%d")
        for reminder in self._reminders:
            rtime = reminder.get("time", "")
            rmsg = reminder.get("message", "")
            fire_key = f"{today_key}_{rtime}"
            if rtime == now_hm and fire_key not in self._fired_reminders:
                self._fired_reminders.add(fire_key)
                self.add_notification("Reminder", rmsg)

        if len(self._notifications) != before:
            self._update_display()

    def _update_display(self) -> None:
        try:
            display = self.query_one("#notif-display", Static)
        except Exception:
            return  # Not yet mounted
        if not self._notifications:
            display.update("No notifications")
            return
        lines = []
        for n in reversed(self._notifications):
            time_str = n.time.strftime("%H:%M")
            lines.append(f"{time_str} {n.message}")
        display.update("\n".join(lines))
