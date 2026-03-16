import pytest
from pathlib import Path

from jm.storage.store import ProjectStore, JournalStore, PeopleStore, ActiveProjectStore


@pytest.fixture
def stores(tmp_path):
    ps = ProjectStore(tmp_path)
    js = JournalStore(tmp_path)
    pe = PeopleStore(tmp_path)
    ac = ActiveProjectStore(tmp_path)
    return ps, js, pe, ac


class TestScreenImports:
    """Verify all screens can be imported and instantiated."""

    def test_dashboard_screen(self, stores):
        from jm.screens.dashboard import DashboardScreen

        screen = DashboardScreen(*stores)
        assert screen is not None
        # Check key bindings exist
        binding_keys = [b.key for b in screen.BINDINGS]
        assert "q" in binding_keys
        assert "j" in binding_keys
        assert "k" in binding_keys
        assert "s" in binding_keys
        assert "w" in binding_keys
        assert "n" in binding_keys

    def test_switch_screen(self, stores):
        from jm.screens.switch import SwitchScreen

        ps, js, pe, ac = stores
        screen = SwitchScreen(ps, js, ac, lambda r: None)
        assert screen is not None

    def test_search_screen(self, stores):
        from jm.screens.search import SearchScreen

        ps, js, pe, ac = stores
        screen = SearchScreen(project_store=ps)
        assert screen is not None

    def test_people_screen(self, stores):
        from jm.screens.people import PeopleScreen

        ps, js, pe, ac = stores
        screen = PeopleScreen(pe, ps)
        assert screen is not None

    def test_review_screen(self, stores):
        from jm.screens.review import ReviewScreen

        ps, js, pe, ac = stores
        screen = ReviewScreen(ps, js, ac)
        assert screen is not None

    def test_project_view_screen(self, stores):
        from jm.screens.project_view import ProjectViewScreen

        ps, js, pe, ac = stores
        ps.create_project("Test")
        screen = ProjectViewScreen("test", ps)
        assert screen is not None
        assert screen.project is not None

    def test_project_view_missing_project(self, stores):
        from jm.screens.project_view import ProjectViewScreen

        ps, js, pe, ac = stores
        screen = ProjectViewScreen("nonexistent", ps)
        assert screen.project is None

    def test_quick_input_screens(self):
        from jm.widgets.quick_input import (
            QuickNoteScreen,
            QuickBlockerScreen,
            QuickDecisionScreen,
        )

        note = QuickNoteScreen("Test", lambda t: None)
        block = QuickBlockerScreen("Test", lambda t: None)
        decide = QuickDecisionScreen("Test", lambda t: None)
        assert note is not None
        assert block is not None
        assert decide is not None


class TestWorkModule:
    def test_find_last_switch_away_no_entries(self, stores):
        """No switch entries returns None."""
        from jm.screens.work import find_last_switch_away

        ps, js, pe, ac = stores
        result = find_last_switch_away(js, "Test")
        assert result is None

    def test_find_last_switch_away_with_entry(self, stores):
        """Finds the most recent switch-away entry."""
        from jm.screens.work import find_last_switch_away
        from jm.models import JournalEntry

        ps, js, pe, ac = stores

        js.append(
            JournalEntry(
                time="10:00",
                entry_type="Switched",
                project="Test \u2192 Other",
                details={"left_off": "debugging"},
            )
        )

        result = find_last_switch_away(js, "Test")
        assert result is not None
        assert result.details.get("left_off") == "debugging"
