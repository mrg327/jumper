"""Pomodoro timer plugin — work/break cycle timer."""

from __future__ import annotations

from enum import Enum

from textual.app import ComposeResult
from textual.binding import Binding
from textual.widgets import Static

from jm.plugins.base import JMPlugin, PluginTick


class PomodoroState(Enum):
    IDLE = "idle"
    WORK = "work"
    SHORT_BREAK = "short_break"
    LONG_BREAK = "long_break"
    PAUSED = "paused"


class PomodoroPlugin(JMPlugin):
    """Pomodoro timer: work sessions with short/long breaks.

    Keybindings (when focused):
    - Space: start/pause
    - r: reset current session
    - R: reset all sessions
    """

    PLUGIN_NAME = "Pomodoro"
    PLUGIN_DESCRIPTION = "Work/break cycle timer"
    NEEDS_TIMER = True

    can_focus = True

    BINDINGS = [
        Binding("space", "toggle", "Start/Pause"),
        Binding("r", "reset_session", "Reset"),
        Binding("R", "reset_all", "Reset All", key_display="R"),
    ]

    DEFAULT_CSS = """
    PomodoroPlugin {
        height: auto;
        width: 100%;
        padding: 0 1;
        margin-bottom: 1;
    }
    PomodoroPlugin #pomo-label {
        text-style: bold;
        color: $text-muted;
    }
    PomodoroPlugin #pomo-display {
        color: $text;
    }
    PomodoroPlugin:focus #pomo-label {
        color: $accent;
    }
    """

    def __init__(
        self,
        work_minutes: int = 25,
        short_break_minutes: int = 5,
        long_break_minutes: int = 15,
        sessions_before_long: int = 4,
    ) -> None:
        super().__init__()
        self.work_seconds = work_minutes * 60
        self.short_break_seconds = short_break_minutes * 60
        self.long_break_seconds = long_break_minutes * 60
        self.sessions_before_long = sessions_before_long

        self.state = PomodoroState.IDLE
        self._paused_state: PomodoroState | None = None
        self.session_number = 1
        self.remaining = self.work_seconds

    def compose(self) -> ComposeResult:
        yield Static("Pomodoro", id="pomo-label")
        yield Static(self._format_display(), id="pomo-display")

    def on_plugin_tick(self, event: PluginTick) -> None:
        if self.state in (PomodoroState.IDLE, PomodoroState.PAUSED):
            return

        self.remaining -= 1
        if self.remaining <= 0:
            self._on_timer_complete()
        self._update_display()

    def _on_timer_complete(self) -> None:
        """Handle timer reaching zero."""
        if self.state == PomodoroState.WORK:
            # Work session done — transition to break
            if self.session_number >= self.sessions_before_long:
                self.state = PomodoroState.LONG_BREAK
                self.remaining = self.long_break_seconds
                self.notify_user("Work session done -- long break!")
            else:
                self.state = PomodoroState.SHORT_BREAK
                self.remaining = self.short_break_seconds
                self.notify_user("Work session done -- take a break!")
            self._log_break()
        elif self.state in (PomodoroState.SHORT_BREAK, PomodoroState.LONG_BREAK):
            # Break done — transition to next work session
            if self.state == PomodoroState.LONG_BREAK:
                self.session_number = 1
            else:
                self.session_number += 1
            self.state = PomodoroState.WORK
            self.remaining = self.work_seconds
            self.notify_user("Break over -- back to work!")

    def _log_break(self) -> None:
        """Log break start to journal if app has stores."""
        try:
            from datetime import datetime

            from jm.models import JournalEntry

            app = self.app
            if hasattr(app, "journal_store"):
                time_str = datetime.now().strftime("%H:%M")
                break_type = (
                    "Long break" if self.state == PomodoroState.LONG_BREAK else "Short break"
                )
                app.journal_store.append(
                    JournalEntry(
                        time=time_str,
                        entry_type="Break",
                        project="",
                        details={"break": f"Pomodoro {break_type.lower()}"},
                    )
                )
        except Exception:
            pass  # Don't crash if journal isn't available

    def _format_display(self) -> str:
        minutes, seconds = divmod(max(0, self.remaining), 60)

        if self.state == PomodoroState.IDLE:
            return f"{minutes:02d}:{seconds:02d}  (idle)  [{self.session_number}/{self.sessions_before_long}]"
        elif self.state == PomodoroState.PAUSED:
            return f"{minutes:02d}:{seconds:02d}  (paused) [{self.session_number}/{self.sessions_before_long}]"
        elif self.state == PomodoroState.WORK:
            return f"{minutes:02d}:{seconds:02d}  remain  [{self.session_number}/{self.sessions_before_long}]"
        elif self.state == PomodoroState.SHORT_BREAK:
            return f"{minutes:02d}:{seconds:02d}  break   [{self.session_number}/{self.sessions_before_long}]"
        else:  # LONG_BREAK
            return f"{minutes:02d}:{seconds:02d}  BREAK   [{self.session_number}/{self.sessions_before_long}]"

    def _update_display(self) -> None:
        try:
            self.query_one("#pomo-display", Static).update(self._format_display())
        except Exception:
            pass  # Not yet mounted

    def action_toggle(self) -> None:
        """Start or pause the timer."""
        if self.state == PomodoroState.IDLE:
            self.state = PomodoroState.WORK
            self.remaining = self.work_seconds
        elif self.state == PomodoroState.PAUSED:
            self.state = self._paused_state or PomodoroState.WORK
            self._paused_state = None
        elif self.state in (
            PomodoroState.WORK,
            PomodoroState.SHORT_BREAK,
            PomodoroState.LONG_BREAK,
        ):
            self._paused_state = self.state
            self.state = PomodoroState.PAUSED
        self._update_display()

    def action_reset_session(self) -> None:
        """Reset the current session timer."""
        if self.state in (PomodoroState.SHORT_BREAK, PomodoroState.LONG_BREAK):
            self.remaining = (
                self.long_break_seconds
                if self.state == PomodoroState.LONG_BREAK
                else self.short_break_seconds
            )
        else:
            self.remaining = self.work_seconds
            self.state = PomodoroState.IDLE
        self._paused_state = None
        self._update_display()

    def action_reset_all(self) -> None:
        """Reset all sessions back to the beginning."""
        self.state = PomodoroState.IDLE
        self._paused_state = None
        self.session_number = 1
        self.remaining = self.work_seconds
        self._update_display()
