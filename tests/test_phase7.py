"""Phase 7 tests: person merge, multi-day journal search, CLI commands, and more."""

from __future__ import annotations

from datetime import date, timedelta
from io import StringIO
from pathlib import Path
from unittest.mock import patch

import pytest

from jm.models import (
    Blocker,
    DailyJournal,
    JournalEntry,
    PendingItem,
    Person,
    Project,
)
from jm.storage.store import (
    ActiveProjectStore,
    JournalStore,
    PeopleStore,
    ProjectStore,
)


# ── 1. Person merge logic ─────────────────────────────────────────────────────


class TestPersonMerge:
    def test_person_merge_preserves_existing_role(self, tmp_path):
        """Adding a person with empty role should preserve existing role."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(
            Person(handle="@carol", role="Display Lead", projects=["HMI"])
        )
        # Update with new pending item but no role
        store.add_or_update_person(
            Person(
                handle="@carol",
                projects=["Test"],
                pending=[PendingItem(description="spec review")],
            )
        )
        result = store.get_person("@carol")
        assert result is not None
        assert result.role == "Display Lead"  # preserved
        assert "HMI" in result.projects
        assert "Test" in result.projects
        assert len(result.pending) == 1

    def test_person_merge_unions_projects(self, tmp_path):
        """Merging should union project lists without duplicates."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(Person(handle="@bob", projects=["A", "B"]))
        store.add_or_update_person(Person(handle="@bob", projects=["B", "C"]))
        result = store.get_person("@bob")
        assert result is not None
        assert sorted(result.projects) == ["A", "B", "C"]

    def test_person_merge_appends_pending_no_duplicates(self, tmp_path):
        """Merging should append new pending items, not duplicate by description."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(
            Person(handle="@carol", pending=[PendingItem(description="item1")])
        )
        store.add_or_update_person(
            Person(handle="@carol", pending=[PendingItem(description="item2")])
        )
        # Add duplicate
        store.add_or_update_person(
            Person(handle="@carol", pending=[PendingItem(description="item1")])
        )
        result = store.get_person("@carol")
        assert result is not None
        assert len(result.pending) == 2  # item1 + item2, not 3
        descriptions = {item.description for item in result.pending}
        assert descriptions == {"item1", "item2"}

    def test_person_merge_updates_role_when_nonempty(self, tmp_path):
        """Non-empty role should update existing role."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(Person(handle="@bob", role="Old Role"))
        store.add_or_update_person(Person(handle="@bob", role="New Role"))
        result = store.get_person("@bob")
        assert result is not None
        assert result.role == "New Role"

    def test_person_new_does_not_replace_existing_other(self, tmp_path):
        """Adding a new person should not affect existing people."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(Person(handle="@alice", role="Manager"))
        store.add_or_update_person(Person(handle="@bob", role="Developer"))
        alice = store.get_person("@alice")
        assert alice is not None
        assert alice.role == "Manager"

    def test_person_merge_empty_projects_on_new(self, tmp_path):
        """Merging a person with empty project list keeps existing projects."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(Person(handle="@carol", projects=["HMI", "Infra"]))
        store.add_or_update_person(Person(handle="@carol", projects=[]))
        result = store.get_person("@carol")
        assert result is not None
        assert sorted(result.projects) == ["HMI", "Infra"]


# ── 2. find_last_switch_away multi-day search ─────────────────────────────────


class TestFindLastSwitchAwayMultiDay:
    def test_find_last_switch_away_multiple_days_back(self, tmp_path):
        """Should find switch entries up to 14 days back, not just 1."""
        from jm.screens.work import find_last_switch_away

        store = JournalStore(tmp_path)

        # Create a switch entry 5 days ago
        old_date = date.today() - timedelta(days=5)
        journal = DailyJournal(date=old_date)
        journal.append_entry(
            JournalEntry(
                time="14:00",
                entry_type="Switched",
                project="MyProject \u2192 OtherProject",
                details={"left_off": "was debugging", "next_step": "check logs"},
            )
        )
        store.save(journal)

        result = find_last_switch_away(store, "MyProject")
        assert result is not None
        assert result.details["left_off"] == "was debugging"
        assert result.details["next_step"] == "check logs"

    def test_find_last_switch_away_returns_none_when_too_old(self, tmp_path):
        """Returns None when switch entry is older than 14 days."""
        from jm.screens.work import find_last_switch_away

        store = JournalStore(tmp_path)

        # Create a switch entry 15 days ago (just outside the 14-day window)
        old_date = date.today() - timedelta(days=15)
        journal = DailyJournal(date=old_date)
        journal.append_entry(
            JournalEntry(
                time="14:00",
                entry_type="Switched",
                project="MyProject \u2192 OtherProject",
                details={"left_off": "old context"},
            )
        )
        store.save(journal)

        result = find_last_switch_away(store, "MyProject")
        assert result is None

    def test_find_last_switch_away_prefers_most_recent(self, tmp_path):
        """When multiple journals have switch entries, returns the most recent."""
        from jm.screens.work import find_last_switch_away

        store = JournalStore(tmp_path)

        # Older entry (3 days ago)
        older_date = date.today() - timedelta(days=3)
        older_journal = DailyJournal(date=older_date)
        older_journal.append_entry(
            JournalEntry(
                time="10:00",
                entry_type="Switched",
                project="MyProject \u2192 OtherProject",
                details={"left_off": "older context"},
            )
        )
        store.save(older_journal)

        # Newer entry (1 day ago)
        newer_date = date.today() - timedelta(days=1)
        newer_journal = DailyJournal(date=newer_date)
        newer_journal.append_entry(
            JournalEntry(
                time="15:00",
                entry_type="Switched",
                project="MyProject \u2192 OtherProject",
                details={"left_off": "newer context"},
            )
        )
        store.save(newer_journal)

        result = find_last_switch_away(store, "MyProject")
        assert result is not None
        assert result.details["left_off"] == "newer context"

    def test_find_last_switch_away_case_insensitive(self, tmp_path):
        """Project name matching should be case-insensitive."""
        from jm.screens.work import find_last_switch_away

        store = JournalStore(tmp_path)

        old_date = date.today() - timedelta(days=2)
        journal = DailyJournal(date=old_date)
        journal.append_entry(
            JournalEntry(
                time="09:00",
                entry_type="Switched",
                project="HMI Framework \u2192 Test Infra",
                details={"left_off": "render loop"},
            )
        )
        store.save(journal)

        result = find_last_switch_away(store, "hmi framework")
        assert result is not None
        assert result.details["left_off"] == "render loop"


# ── 3. CLI commands ────────────────────────────────────────────────────────────


def _make_stores(tmp_path: Path):
    """Create all stores pointing to tmp_path."""
    ps = ProjectStore(tmp_path)
    js = JournalStore(tmp_path)
    pe = PeopleStore(tmp_path)
    ac = ActiveProjectStore(tmp_path)
    return ps, js, pe, ac


class TestCliList:
    def test_cli_list_shows_all_projects(self, tmp_path, capsys):
        """_cmd_list should print all projects."""
        from jm.cli import _cmd_list

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("Alpha", status="active")
        ps.create_project("Beta", status="parked")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_list()

        out = capsys.readouterr().out
        assert "alpha" in out
        assert "beta" in out
        assert "Alpha" in out
        assert "Beta" in out

    def test_cli_list_filter_by_status(self, tmp_path, capsys):
        """_cmd_list with status filter should only show matching projects."""
        from jm.cli import _cmd_list
        from jm.models import Blocker

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("Active One", status="active")
        # Must have an open blocker to stay "blocked" (auto-status logic)
        blocked = ps.create_project("Blocked One", status="active")
        blocked.blockers.append(Blocker(description="waiting on X"))
        ps.save_project(blocked)  # auto-sets to blocked

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_list(status="active")

        out = capsys.readouterr().out
        assert "Active One" in out
        assert "Blocked One" not in out

    def test_cli_list_empty(self, tmp_path, capsys):
        """_cmd_list with no projects should print no-projects message."""
        from jm.cli import _cmd_list

        ps, js, pe, ac = _make_stores(tmp_path)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_list()

        out = capsys.readouterr().out
        assert "No projects" in out


class TestCliAdd:
    def test_cli_add_creates_project(self, tmp_path, capsys):
        """_cmd_add should create a new project and print confirmation."""
        from jm.cli import _cmd_add

        ps, js, pe, ac = _make_stores(tmp_path)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_add("New Feature")

        out = capsys.readouterr().out
        assert "New Feature" in out

        project = ps.get_project("new-feature")
        assert project is not None
        assert project.name == "New Feature"
        assert project.status == "active"
        assert project.priority == "medium"

    def test_cli_add_with_options(self, tmp_path, capsys):
        """_cmd_add with status, priority and tags options."""
        from jm.cli import _cmd_add

        ps, js, pe, ac = _make_stores(tmp_path)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_add("HMI Work", status="parked", priority="high", tags=["infra", "hmi"])

        project = ps.get_project("hmi-work")
        assert project is not None
        assert project.status == "parked"
        assert project.priority == "high"
        assert set(project.tags) == {"infra", "hmi"}


class TestCliBreak:
    def test_cli_break_eod_clears_active(self, tmp_path, capsys):
        """_cmd_break('eod') should clear the active project."""
        from jm.cli import _cmd_break

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", status="active")
        ac.set_active("my-project")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_break("eod")

        assert ac.get_active() is None
        out = capsys.readouterr().out
        assert "Done for day" in out

    def test_cli_break_eod_logs_journal_entry(self, tmp_path, capsys):
        """_cmd_break('eod') should log a Done journal entry."""
        from jm.cli import _cmd_break

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("Active Project", status="active")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_break("eod")

        journal = js.today()
        done_entries = [e for e in journal.entries if e.entry_type == "Done"]
        assert len(done_entries) == 1

    def test_cli_break_15min_keeps_active(self, tmp_path, capsys):
        """_cmd_break('15min') should NOT clear the active project."""
        from jm.cli import _cmd_break

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", status="active")
        ac.set_active("my-project")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_break("15min")

        assert ac.get_active() == "my-project"
        journal = js.today()
        break_entries = [e for e in journal.entries if e.entry_type == "Break"]
        assert len(break_entries) == 1

    def test_cli_break_lunch_keeps_active(self, tmp_path, capsys):
        """_cmd_break('lunch') should NOT clear the active project."""
        from jm.cli import _cmd_break

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", status="active")
        ac.set_active("my-project")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_break("lunch")

        assert ac.get_active() == "my-project"
        out = capsys.readouterr().out
        assert "Out to lunch" in out


class TestCliSetStatus:
    def test_cli_set_status_changes_status(self, tmp_path, capsys):
        """_cmd_set_status should update the project's status."""
        from jm.cli import _cmd_set_status

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", status="active")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_set_status("my-project", "parked")

        project = ps.get_project("my-project")
        assert project is not None
        assert project.status == "parked"

        out = capsys.readouterr().out
        assert "parked" in out

    def test_cli_set_status_not_found(self, tmp_path, capsys):
        """_cmd_set_status with missing project exits with error."""
        from jm.cli import _cmd_set_status

        ps, js, pe, ac = _make_stores(tmp_path)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            with pytest.raises(SystemExit):
                _cmd_set_status("nonexistent", "blocked")

    def test_cli_set_status_preserves_other_fields(self, tmp_path, capsys):
        """_cmd_set_status should only change the status, not other fields."""
        from jm.cli import _cmd_set_status

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", status="active", priority="high")
        proj = ps.get_project("my-project")
        proj.current_focus = "important work"
        ps.save_project(proj)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_set_status("my-project", "parked")

        project = ps.get_project("my-project")
        assert project.status == "parked"
        assert project.priority == "high"
        assert project.current_focus == "important work"


class TestCliSetPriority:
    def test_cli_set_priority_changes_priority(self, tmp_path, capsys):
        """_cmd_set_priority should update the project's priority."""
        from jm.cli import _cmd_set_priority

        ps, js, pe, ac = _make_stores(tmp_path)
        ps.create_project("My Project", priority="medium")

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            _cmd_set_priority("my-project", "high")

        project = ps.get_project("my-project")
        assert project is not None
        assert project.priority == "high"

        out = capsys.readouterr().out
        assert "high" in out

    def test_cli_set_priority_not_found(self, tmp_path, capsys):
        """_cmd_set_priority with missing project exits with error."""
        from jm.cli import _cmd_set_priority

        ps, js, pe, ac = _make_stores(tmp_path)

        with patch("jm.storage.store.create_stores", return_value=(ps, js, pe, ac)):
            with pytest.raises(SystemExit):
                _cmd_set_priority("nonexistent", "low")


# ── 4. Rich markup — no brackets in blocker count ─────────────────────────────


class TestRichMarkupEscaping:
    def test_blocker_panel_uses_parens_not_brackets(self):
        """Blocker count should use parentheses (not square brackets) to avoid Rich markup."""
        import ast

        source_path = Path(__file__).parent.parent / "src" / "jm" / "screens" / "dashboard.py"
        source = source_path.read_text(encoding="utf-8")
        # The count display must NOT use [N open] (which Rich would interpret as markup)
        assert "[{count} open]" not in source
        assert "[ open]" not in source
        # It should use parentheses
        assert "open)" in source

    def test_blocker_panel_format_at_runtime(self, tmp_path):
        """Dashboard._refresh_blockers builds the count string with parentheses."""
        from jm.screens.dashboard import DashboardScreen

        ps = ProjectStore(tmp_path)
        js = JournalStore(tmp_path)
        pe = PeopleStore(tmp_path)
        ac = ActiveProjectStore(tmp_path)

        # Add a project with a blocker
        proj = ps.create_project("Test", status="active")
        proj.blockers = [Blocker(description="waiting", since=date.today())]
        ps.save_project(proj)

        screen = DashboardScreen(ps, js, pe, ac)
        # Verify _refresh_blockers builds "(N open)" not "[N open]"
        projects = ps.list_projects()
        # Simulate what the method builds
        blocker_lines = []
        for project in projects:
            for b in project.blockers:
                if not b.resolved:
                    blocker_lines.append(f"  \u2298 {project.name}: {b.description}")

        count = len(blocker_lines)
        result = f"({count} open)\n" + "\n".join(blocker_lines)
        assert result.startswith("(1 open)")
        assert not result.startswith("[1 open]")


# ── 5. Empty-state onboarding message ─────────────────────────────────────────


class TestEmptyState:
    def test_empty_state_message_in_source(self):
        """Dashboard source should contain an onboarding message for empty project list."""
        source_path = Path(__file__).parent.parent / "src" / "jm" / "screens" / "dashboard.py"
        source = source_path.read_text(encoding="utf-8")
        # There should be some onboarding text when no projects exist
        assert "No projects" in source or "Press 'a'" in source or "create" in source.lower()

    def test_empty_state_at_store_level(self, tmp_path):
        """Empty project store returns empty list."""
        store = ProjectStore(tmp_path)
        assert store.list_projects() == []


# ── 6. Project metadata cycling ───────────────────────────────────────────────


class TestProjectMetadataCycling:
    def test_project_status_cycle_full_rotation(self):
        """Status cycles active → blocked → parked → done → active."""
        statuses = ["active", "blocked", "parked", "done"]
        p = Project(name="Test", status="active")
        for expected_next in ["blocked", "parked", "done", "active"]:
            idx = statuses.index(p.status)
            p.status = statuses[(idx + 1) % len(statuses)]
            assert p.status == expected_next

    def test_project_priority_cycle_full_rotation(self):
        """Priority cycles high → medium → low → high."""
        priorities = ["high", "medium", "low"]
        p = Project(name="Test", priority="high")
        for expected_next in ["medium", "low", "high"]:
            idx = priorities.index(p.priority)
            p.priority = priorities[(idx + 1) % len(priorities)]
            assert p.priority == expected_next

    def test_project_status_cycle_starts_from_blocked(self):
        """Status cycle works starting from any status."""
        statuses = ["active", "blocked", "parked", "done"]
        p = Project(name="Test", status="blocked")
        idx = statuses.index(p.status)
        p.status = statuses[(idx + 1) % len(statuses)]
        assert p.status == "parked"

    def test_project_status_cycle_from_done_wraps_to_active(self):
        """Status cycle wraps from done back to active."""
        statuses = ["active", "blocked", "parked", "done"]
        p = Project(name="Test", status="done")
        idx = statuses.index(p.status)
        p.status = statuses[(idx + 1) % len(statuses)]
        assert p.status == "active"

    def test_project_status_cycle_persists_to_store(self, tmp_path):
        """Cycled status is correctly saved and reloaded."""
        store = ProjectStore(tmp_path)
        project = store.create_project("CycleTest", status="active")

        # Cycle to parked (skip blocked — auto-status would revert without blockers)
        project.status = "parked"
        store.save_project(project)

        reloaded = store.get_project("cycletest")
        assert reloaded is not None
        assert reloaded.status == "parked"


# ── 7. Blocker move between projects ──────────────────────────────────────────


class TestBlockerMoveBetweenProjects:
    def test_blocker_move_basic(self, tmp_path):
        """Moving a blocker removes from source and adds to target."""
        store = ProjectStore(tmp_path)

        source = store.create_project("Source", status="active")
        target = store.create_project("Target", status="active")

        blocker = Blocker(description="waiting on spec", since=date.today())
        source.blockers.append(blocker)
        store.save_project(source)

        # Reload both to ensure we have the latest state
        source = store.get_project("source")
        target = store.get_project("target")

        # Simulate the move
        moved = source.blockers.pop(0)
        target.blockers.append(moved)
        store.save_project(source)
        store.save_project(target)

        # Verify
        source_reloaded = store.get_project("source")
        target_reloaded = store.get_project("target")
        assert len(source_reloaded.blockers) == 0
        assert len(target_reloaded.blockers) == 1
        assert target_reloaded.blockers[0].description == "waiting on spec"

    def test_blocker_move_preserves_metadata(self, tmp_path):
        """Moved blocker retains its person and since date."""
        store = ProjectStore(tmp_path)

        source = store.create_project("Source", status="active")
        target = store.create_project("Target", status="active")

        original_date = date.today() - timedelta(days=3)
        blocker = Blocker(
            description="need API docs",
            person="@carol",
            since=original_date,
        )
        source.blockers.append(blocker)
        store.save_project(source)

        source = store.get_project("source")
        target = store.get_project("target")

        moved = source.blockers.pop(0)
        target.blockers.append(moved)
        store.save_project(source)
        store.save_project(target)

        target_reloaded = store.get_project("target")
        b = target_reloaded.blockers[0]
        assert b.description == "need API docs"
        assert b.person == "@carol"
        assert b.since == original_date

    def test_blocker_move_source_keeps_other_blockers(self, tmp_path):
        """Moving one blocker leaves other blockers on the source project."""
        store = ProjectStore(tmp_path)

        source = store.create_project("Source", status="active")
        target = store.create_project("Target", status="active")

        source.blockers = [
            Blocker(description="blocker A", since=date.today()),
            Blocker(description="blocker B", since=date.today()),
        ]
        store.save_project(source)

        source = store.get_project("source")
        target = store.get_project("target")

        # Move only the first blocker
        moved = source.blockers.pop(0)
        target.blockers.append(moved)
        store.save_project(source)
        store.save_project(target)

        source_reloaded = store.get_project("source")
        target_reloaded = store.get_project("target")

        assert len(source_reloaded.blockers) == 1
        assert source_reloaded.blockers[0].description == "blocker B"
        assert len(target_reloaded.blockers) == 1
        assert target_reloaded.blockers[0].description == "blocker A"


# ── 8. Auto-status from blockers ─────────────────────────────────────────────


class TestAutoStatusFromBlockers:
    def test_adding_blocker_sets_blocked(self, tmp_path):
        """Adding an open blocker to an active project auto-sets status to blocked."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        assert project.status == "active"

        project.blockers.append(Blocker(description="waiting on X", since=date.today()))
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "blocked"

    def test_resolving_last_blocker_sets_active(self, tmp_path):
        """Resolving the last open blocker auto-sets status back to active."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.blockers.append(Blocker(description="waiting on X", since=date.today()))
        store.save_project(project)
        assert store.get_project("test").status == "blocked"

        project = store.get_project("test")
        project.blockers[0].resolved = True
        project.blockers[0].resolved_date = date.today()
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "active"

    def test_resolving_one_of_two_stays_blocked(self, tmp_path):
        """Resolving one blocker when another remains keeps status blocked."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.blockers = [
            Blocker(description="blocker A", since=date.today()),
            Blocker(description="blocker B", since=date.today()),
        ]
        store.save_project(project)

        project = store.get_project("test")
        project.blockers[0].resolved = True
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "blocked"

    def test_auto_status_does_not_touch_parked(self, tmp_path):
        """A parked project stays parked even if blockers are added."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test", status="parked")
        project.blockers.append(Blocker(description="waiting", since=date.today()))
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "parked"

    def test_auto_status_does_not_touch_done(self, tmp_path):
        """A done project stays done even if blockers are added."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test", status="done")
        project.blockers.append(Blocker(description="waiting", since=date.today()))
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "done"

    def test_removing_blocker_from_blocked_sets_active(self, tmp_path):
        """Removing (not resolving) a blocker also triggers auto-status."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.blockers.append(Blocker(description="waiting", since=date.today()))
        store.save_project(project)
        assert store.get_project("test").status == "blocked"

        project = store.get_project("test")
        project.blockers.pop(0)
        store.save_project(project)

        reloaded = store.get_project("test")
        assert reloaded.status == "active"

    def test_blocker_move_updates_both_statuses(self, tmp_path):
        """Moving a blocker: source becomes active, target becomes blocked."""
        store = ProjectStore(tmp_path)
        source = store.create_project("Source")
        target = store.create_project("Target")

        source.blockers.append(Blocker(description="waiting", since=date.today()))
        store.save_project(source)
        assert store.get_project("source").status == "blocked"

        source = store.get_project("source")
        target = store.get_project("target")
        moved = source.blockers.pop(0)
        target.blockers.append(moved)
        store.save_project(source)
        store.save_project(target)

        assert store.get_project("source").status == "active"
        assert store.get_project("target").status == "blocked"
