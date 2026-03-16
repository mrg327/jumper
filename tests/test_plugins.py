"""Tests for Phase 8: Plugin sidebar, plugin infrastructure, and built-in plugins."""

from __future__ import annotations

from datetime import datetime, timedelta

import pytest


# ── Plugin base class tests ──────────────────────────────────────────


class TestJMPluginBase:
    """Test JMPlugin base class instantiation and properties."""

    def test_base_class_defaults(self):
        from jm.plugins.base import JMPlugin

        plugin = JMPlugin()
        assert plugin.PLUGIN_NAME == "Plugin"
        assert plugin.PLUGIN_DESCRIPTION == ""
        assert plugin.NEEDS_TIMER is False

    def test_subclass_overrides(self):
        from jm.plugins.base import JMPlugin

        class TestPlugin(JMPlugin):
            PLUGIN_NAME = "Test"
            PLUGIN_DESCRIPTION = "A test plugin"
            NEEDS_TIMER = True

        p = TestPlugin()
        assert p.PLUGIN_NAME == "Test"
        assert p.PLUGIN_DESCRIPTION == "A test plugin"
        assert p.NEEDS_TIMER is True


# ── Plugin discovery tests ───────────────────────────────────────────


class TestPluginDiscovery:
    """Test auto-discovery of plugins from the package."""

    def test_discover_finds_built_in_plugins(self):
        from jm.plugins import discover_plugins

        plugins = discover_plugins()
        names = [cls.PLUGIN_NAME for cls in plugins]
        assert "Clock" in names
        assert "Notifications" in names
        assert "Pomodoro" in names

    def test_discover_returns_classes_not_instances(self):
        from jm.plugins import discover_plugins

        plugins = discover_plugins()
        for cls in plugins:
            assert isinstance(cls, type)

    def test_discover_excludes_base_class(self):
        from jm.plugins import discover_plugins
        from jm.plugins.base import JMPlugin

        plugins = discover_plugins()
        assert JMPlugin not in plugins

    def test_discover_sorted_by_name(self):
        from jm.plugins import discover_plugins

        plugins = discover_plugins()
        names = [cls.PLUGIN_NAME for cls in plugins]
        assert names == sorted(names)


# ── Clock plugin tests ───────────────────────────────────────────────


class TestClockPlugin:
    """Test Clock plugin time formatting."""

    def test_format_time_has_expected_format(self):
        from jm.plugins.clock import ClockPlugin

        plugin = ClockPlugin()
        result = plugin._format_time()
        # Should contain HH:MM and day-of-week abbreviation
        now = datetime.now()
        assert now.strftime("%H:%M") in result
        assert now.strftime("%a") in result

    def test_clock_needs_timer(self):
        from jm.plugins.clock import ClockPlugin

        assert ClockPlugin.NEEDS_TIMER is True

    def test_clock_plugin_name(self):
        from jm.plugins.clock import ClockPlugin

        assert ClockPlugin.PLUGIN_NAME == "Clock"


# ── Notifications plugin tests ───────────────────────────────────────


class TestNotificationsPlugin:
    """Test Notifications plugin add/expire logic."""

    def test_add_notification(self):
        from jm.plugins.notifications import NotificationsPlugin

        plugin = NotificationsPlugin()
        plugin._notifications = []  # Ensure clean state
        plugin.add_notification("Test", "Hello world")

        assert len(plugin._notifications) == 1
        assert plugin._notifications[0].source == "Test"
        assert plugin._notifications[0].message == "Hello world"

    def test_notification_max_limit(self):
        from jm.plugins.notifications import NotificationsPlugin

        plugin = NotificationsPlugin()
        plugin._notifications = []

        for i in range(15):
            plugin.add_notification("Test", f"Message {i}")

        assert len(plugin._notifications) == plugin.MAX_NOTIFICATIONS

    def test_notification_expiry(self):
        from jm.plugins.notifications import NotificationsPlugin, _Notification

        plugin = NotificationsPlugin()

        # Add an old notification (expired)
        old = _Notification("Old", "Expired message")
        old.time = datetime.now() - timedelta(minutes=60)
        plugin._notifications = [old]

        # Add a fresh one
        plugin.add_notification("New", "Fresh message")

        # Trigger tick to expire
        from jm.plugins.base import PluginTick

        plugin.on_plugin_tick(PluginTick())

        # Old one should be expired
        assert len(plugin._notifications) == 1
        assert plugin._notifications[0].message == "Fresh message"

    def test_configure_reminders(self):
        from jm.plugins.notifications import NotificationsPlugin

        plugin = NotificationsPlugin()
        reminders = [
            {"time": "09:00", "message": "Morning review"},
            {"time": "12:00", "message": "Lunch break"},
        ]
        plugin.configure_reminders(reminders)
        assert plugin._reminders == reminders

    def test_scheduled_reminder_fires(self):
        from jm.plugins.notifications import NotificationsPlugin
        from jm.plugins.base import PluginTick

        plugin = NotificationsPlugin()
        now_hm = datetime.now().strftime("%H:%M")
        plugin.configure_reminders([{"time": now_hm, "message": "Test reminder"}])

        # Tick should fire the reminder
        plugin.on_plugin_tick(PluginTick())

        assert len(plugin._notifications) == 1
        assert plugin._notifications[0].message == "Test reminder"

    def test_scheduled_reminder_fires_once(self):
        from jm.plugins.notifications import NotificationsPlugin
        from jm.plugins.base import PluginTick

        plugin = NotificationsPlugin()
        now_hm = datetime.now().strftime("%H:%M")
        plugin.configure_reminders([{"time": now_hm, "message": "Once only"}])

        plugin.on_plugin_tick(PluginTick())
        plugin.on_plugin_tick(PluginTick())

        assert len(plugin._notifications) == 1

    def test_plugin_name(self):
        from jm.plugins.notifications import NotificationsPlugin

        assert NotificationsPlugin.PLUGIN_NAME == "Notifications"
        assert NotificationsPlugin.NEEDS_TIMER is True


# ── Pomodoro plugin tests ────────────────────────────────────────────


class TestPomodoroPlugin:
    """Test Pomodoro timer state machine."""

    def test_initial_state_is_idle(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin()
        assert plugin.state == PomodoroState.IDLE
        assert plugin.session_number == 1
        assert plugin.remaining == 25 * 60

    def test_custom_durations(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        plugin = PomodoroPlugin(
            work_minutes=10, short_break_minutes=2,
            long_break_minutes=8, sessions_before_long=3,
        )
        assert plugin.work_seconds == 600
        assert plugin.short_break_seconds == 120
        assert plugin.long_break_seconds == 480
        assert plugin.sessions_before_long == 3

    def test_toggle_starts_work(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin()
        plugin.action_toggle()
        assert plugin.state == PomodoroState.WORK
        assert plugin.remaining == 25 * 60

    def test_toggle_pauses_and_resumes(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin()
        plugin.action_toggle()  # Start
        assert plugin.state == PomodoroState.WORK

        plugin.remaining = 1200  # Simulate some time passing
        plugin.action_toggle()  # Pause
        assert plugin.state == PomodoroState.PAUSED
        assert plugin.remaining == 1200

        plugin.action_toggle()  # Resume
        assert plugin.state == PomodoroState.WORK
        assert plugin.remaining == 1200

    def test_tick_decrements_during_work(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin()
        plugin.state = PomodoroState.WORK
        plugin.remaining = 100

        plugin.on_plugin_tick(PluginTick())
        assert plugin.remaining == 99

    def test_tick_no_decrement_when_idle(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin()
        assert plugin.state == PomodoroState.IDLE
        plugin.remaining = 100

        plugin.on_plugin_tick(PluginTick())
        assert plugin.remaining == 100

    def test_tick_no_decrement_when_paused(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin()
        plugin.state = PomodoroState.PAUSED
        plugin.remaining = 100

        plugin.on_plugin_tick(PluginTick())
        assert plugin.remaining == 100

    def test_work_to_short_break_transition(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin(work_minutes=1, short_break_minutes=1, sessions_before_long=4)
        plugin.state = PomodoroState.WORK
        plugin.session_number = 1
        plugin.remaining = 1  # 1 second left

        plugin.on_plugin_tick(PluginTick())

        assert plugin.state == PomodoroState.SHORT_BREAK
        assert plugin.remaining == 60  # 1 minute short break

    def test_work_to_long_break_on_last_session(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin(
            work_minutes=1, short_break_minutes=1,
            long_break_minutes=5, sessions_before_long=4,
        )
        plugin.state = PomodoroState.WORK
        plugin.session_number = 4  # Last session
        plugin.remaining = 1

        plugin.on_plugin_tick(PluginTick())

        assert plugin.state == PomodoroState.LONG_BREAK
        assert plugin.remaining == 300  # 5 minute long break

    def test_short_break_to_work_increments_session(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin(work_minutes=1, short_break_minutes=1, sessions_before_long=4)
        plugin.state = PomodoroState.SHORT_BREAK
        plugin.session_number = 2
        plugin.remaining = 1

        plugin.on_plugin_tick(PluginTick())

        assert plugin.state == PomodoroState.WORK
        assert plugin.session_number == 3

    def test_long_break_resets_session_counter(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin(work_minutes=1, long_break_minutes=1, sessions_before_long=4)
        plugin.state = PomodoroState.LONG_BREAK
        plugin.session_number = 4
        plugin.remaining = 1

        plugin.on_plugin_tick(PluginTick())

        assert plugin.state == PomodoroState.WORK
        assert plugin.session_number == 1

    def test_reset_session(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.state = PomodoroState.WORK
        plugin.remaining = 500

        plugin.action_reset_session()

        assert plugin.state == PomodoroState.IDLE
        assert plugin.remaining == 25 * 60

    def test_reset_all(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.state = PomodoroState.WORK
        plugin.session_number = 3
        plugin.remaining = 200

        plugin.action_reset_all()

        assert plugin.state == PomodoroState.IDLE
        assert plugin.session_number == 1
        assert plugin.remaining == 25 * 60

    def test_add_time(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.remaining = 10 * 60
        plugin.action_add_time()
        assert plugin.remaining == 15 * 60

    def test_sub_time(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.remaining = 10 * 60
        plugin.action_sub_time()
        assert plugin.remaining == 5 * 60

    def test_sub_time_floors_at_zero(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.remaining = 2 * 60  # 2 minutes
        plugin.action_sub_time()   # -5 min
        assert plugin.remaining == 0

    def test_display_format_idle(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        plugin = PomodoroPlugin(work_minutes=25)
        display = plugin._format_display()
        assert "25:00" in display
        assert "idle" in display
        assert "1/4" in display

    def test_display_format_work(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin(work_minutes=25)
        plugin.state = PomodoroState.WORK
        plugin.remaining = 17 * 60 + 23
        display = plugin._format_display()
        assert "17:23" in display
        assert "remain" in display

    def test_display_format_paused(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin()
        plugin.state = PomodoroState.PAUSED
        display = plugin._format_display()
        assert "paused" in display

    def test_display_format_break(self):
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState

        plugin = PomodoroPlugin()
        plugin.state = PomodoroState.SHORT_BREAK
        plugin.remaining = 3 * 60
        display = plugin._format_display()
        assert "03:00" in display
        assert "break" in display

    def test_plugin_name(self):
        from jm.plugins.pomodoro import PomodoroPlugin

        assert PomodoroPlugin.PLUGIN_NAME == "Pomodoro"
        assert PomodoroPlugin.NEEDS_TIMER is True

    def test_full_work_break_cycle(self):
        """Test a complete work->break->work cycle."""
        from jm.plugins.pomodoro import PomodoroPlugin, PomodoroState
        from jm.plugins.base import PluginTick

        plugin = PomodoroPlugin(
            work_minutes=1, short_break_minutes=1,
            long_break_minutes=2, sessions_before_long=2,
        )

        # Start work
        plugin.action_toggle()
        assert plugin.state == PomodoroState.WORK
        assert plugin.session_number == 1

        # Simulate work session completing
        plugin.remaining = 1
        plugin.on_plugin_tick(PluginTick())
        assert plugin.state == PomodoroState.SHORT_BREAK

        # Simulate short break completing
        plugin.remaining = 1
        plugin.on_plugin_tick(PluginTick())
        assert plugin.state == PomodoroState.WORK
        assert plugin.session_number == 2

        # Simulate second work session completing (triggers long break)
        plugin.remaining = 1
        plugin.on_plugin_tick(PluginTick())
        assert plugin.state == PomodoroState.LONG_BREAK

        # Simulate long break completing
        plugin.remaining = 1
        plugin.on_plugin_tick(PluginTick())
        assert plugin.state == PomodoroState.WORK
        assert plugin.session_number == 1  # Reset after long break


# ── Config support for plugins ───────────────────────────────────────


class TestPluginConfig:
    """Test plugin configuration loading."""

    def test_default_config_has_plugins(self):
        from jm.config import DEFAULT_CONFIG

        assert "plugins" in DEFAULT_CONFIG
        plugins = DEFAULT_CONFIG["plugins"]
        assert "enabled" in plugins
        assert "pomodoro" in plugins["enabled"]
        assert "notifications" in plugins["enabled"]
        assert "clock" in plugins["enabled"]

    def test_default_pomodoro_config(self):
        from jm.config import DEFAULT_CONFIG

        pomo = DEFAULT_CONFIG["plugins"]["pomodoro"]
        assert pomo["work_minutes"] == 25
        assert pomo["short_break_minutes"] == 5
        assert pomo["long_break_minutes"] == 15
        assert pomo["sessions_before_long"] == 4

    def test_default_notifications_config(self):
        from jm.config import DEFAULT_CONFIG

        notif = DEFAULT_CONFIG["plugins"]["notifications"]
        assert notif["reminders"] == []


# ── PluginSidebar widget tests ───────────────────────────────────────


class TestPluginSidebar:
    """Test PluginSidebar container behavior."""

    def test_sidebar_init_with_no_filter(self):
        from jm.widgets.plugin_sidebar import PluginSidebar

        sidebar = PluginSidebar()
        assert sidebar._enabled_plugins is None

    def test_sidebar_init_with_filter(self):
        from jm.widgets.plugin_sidebar import PluginSidebar

        sidebar = PluginSidebar(enabled_plugins=["clock", "pomodoro"])
        assert sidebar._enabled_plugins == ["clock", "pomodoro"]

    def test_sidebar_init_with_empty_filter(self):
        from jm.widgets.plugin_sidebar import PluginSidebar

        sidebar = PluginSidebar(enabled_plugins=[])
        assert sidebar._enabled_plugins == []


# ── Plugin messages ──────────────────────────────────────────────────


class TestPluginMessages:
    """Test custom plugin messages."""

    def test_plugin_tick_message(self):
        from jm.plugins.base import PluginTick

        tick = PluginTick()
        assert tick is not None

    def test_plugin_notification_message(self):
        from jm.plugins.base import JMPlugin, PluginNotification

        plugin = JMPlugin()
        notif = PluginNotification(plugin, "Test message")
        assert notif.source_name == "Plugin"
        assert notif.notification_message == "Test message"

    def test_request_sidebar_open_message(self):
        from jm.plugins.base import RequestSidebarOpen

        msg = RequestSidebarOpen()
        assert msg is not None

    def test_request_sidebar_open_in_exports(self):
        from jm.plugins import RequestSidebarOpen

        assert RequestSidebarOpen is not None
