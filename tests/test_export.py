import pytest
from datetime import date
from pathlib import Path

from jm.models import Project, Blocker, DailyJournal, JournalEntry, LogEntry
from jm.storage.store import ProjectStore, JournalStore, PeopleStore, ActiveProjectStore
from jm.export import generate_dump, export_to_file


@pytest.fixture
def stores(tmp_path):
    """Create all four stores with tmp_path."""
    ps = ProjectStore(tmp_path)
    js = JournalStore(tmp_path)
    pe = PeopleStore(tmp_path)
    ac = ActiveProjectStore(tmp_path)
    return ps, js, pe, ac


class TestGenerateDump:
    def test_empty_state(self, stores):
        """Dump with no data shows placeholder text."""
        text = generate_dump(*stores)
        assert "jm" in text
        assert "No projects yet" in text
        assert "No open blockers" in text
        assert "ACTIVE: none" in text

    def test_with_projects(self, stores):
        """Dump shows project table."""
        ps, js, pe, ac = stores
        ps.create_project("HMI Framework", status="active", priority="high")
        ps.create_project("Test Infra", status="blocked", priority="medium")

        text = generate_dump(*stores)
        assert "HMI Framework" in text
        assert "Test Infra" in text
        assert "ACTIVE PROJECTS (2)" in text

    def test_with_blockers(self, stores):
        """Dump shows blocker summary."""
        ps, js, pe, ac = stores
        project = ps.create_project("Test", status="active")
        project.blockers = [
            Blocker(description="waiting on review", person="@bob", since=date(2026, 3, 14))
        ]
        ps.save_project(project)

        text = generate_dump(*stores)
        assert "BLOCKERS (1)" in text
        assert "waiting on review" in text
        assert "@bob" in text

    def test_with_journal(self, stores):
        """Dump shows today's journal entries."""
        ps, js, pe, ac = stores
        js.append(
            JournalEntry(
                time="09:15", entry_type="Started", project="Test", details={}
            )
        )

        text = generate_dump(*stores)
        assert "09:15" in text
        assert "Started Test" in text

    def test_with_active_project(self, stores):
        """Dump shows active project."""
        ps, js, pe, ac = stores
        ps.create_project("HMI")
        ac.set_active("hmi")

        text = generate_dump(*stores)
        assert "ACTIVE: HMI" in text

    def test_no_ansi_codes(self, stores):
        """Dump output must be completely ANSI-free."""
        ps, js, pe, ac = stores
        ps.create_project("Test Project", status="active", priority="high")
        project = ps.get_project("test-project")
        project.current_focus = "debugging something"
        project.blockers = [
            Blocker(description="blocked", person="@alice", since=date.today())
        ]
        ps.save_project(project)
        ac.set_active("test-project")
        js.append(
            JournalEntry(
                time="09:00",
                entry_type="Started",
                project="Test Project",
                details={},
            )
        )

        text = generate_dump(*stores)
        # No ANSI escape sequences
        assert "\x1b" not in text
        assert "\033" not in text
        # No Rich markup
        assert "[bold]" not in text
        assert "[red]" not in text

    def test_dump_is_parseable(self, stores):
        """Dump sections are clearly delimited and parseable."""
        ps, js, pe, ac = stores
        ps.create_project("Alpha", status="active")
        ps.create_project("Beta", status="blocked")

        text = generate_dump(*stores)
        assert "ACTIVE PROJECTS" in text
        assert "BLOCKERS" in text
        assert "TODAY'S LOG" in text
        assert "ACTIVE:" in text


class TestExportToFile:
    def test_writes_file(self, stores, tmp_path):
        """Export writes to specified path."""
        output = tmp_path / "export.txt"
        result = export_to_file(*stores, output_path=output)
        assert result == output
        assert output.exists()
        content = output.read_text()
        assert "jm" in content

    def test_creates_parent_dirs(self, stores, tmp_path):
        """Export creates parent directories if needed."""
        output = tmp_path / "subdir" / "deep" / "export.txt"
        export_to_file(*stores, output_path=output)
        assert output.exists()


class TestFullWorkflow:
    """End-to-end workflow tests using stores directly."""

    def test_create_work_note_switch_resume(self, stores):
        """Full lifecycle: create project, work, note, switch, check state."""
        ps, js, pe, ac = stores

        # Create two projects
        ps.create_project("Alpha", status="active", priority="high")
        ps.create_project("Beta", status="active", priority="medium")

        # Start working on Alpha
        ac.set_active("alpha")
        js.append(
            JournalEntry(
                time="09:00",
                entry_type="Started",
                project="Alpha",
                details={"focus": "initial work"},
            )
        )

        # Add a note
        project = ps.get_project("alpha")
        today_log = LogEntry(date=date.today(), lines=["fixed the build"])
        project.log.insert(0, today_log)
        ps.save_project(project)
        js.append(
            JournalEntry(
                time="09:30",
                entry_type="Note",
                project="Alpha",
                details={"note": "fixed the build"},
            )
        )

        # Switch to Beta (simulate what SwitchScreen does)
        js.append(
            JournalEntry(
                time="10:00",
                entry_type="Switched",
                project="Alpha \u2192 Beta",
                details={"left_off": "build is green", "next_step": "write tests"},
            )
        )
        js.append(
            JournalEntry(
                time="10:00", entry_type="Started", project="Beta", details={}
            )
        )
        ac.set_active("beta")

        # Verify state
        assert ac.get_active() == "beta"
        journal = js.today()
        assert len(journal.entries) == 4

        # Verify dump reflects all this
        text = generate_dump(*stores)
        assert "Alpha" in text
        assert "Beta" in text
        assert "ACTIVE: Beta" in text
        assert "09:00" in text
        assert "10:00" in text

    def test_blocker_lifecycle(self, stores):
        """Create blocker, verify it shows in dump, resolve it."""
        ps, js, pe, ac = stores

        project = ps.create_project("Test", status="active")
        project.blockers = [
            Blocker(description="need API docs", person="@carol", since=date.today())
        ]
        ps.save_project(project)

        # Blocker shows in dump
        text = generate_dump(*stores)
        assert "BLOCKERS (1)" in text
        assert "need API docs" in text

        # Resolve blocker
        project = ps.get_project("test")
        # After round-tripping through markdown, need to find the right blocker
        unresolved = [b for b in project.blockers if not b.resolved]
        assert len(unresolved) == 1
        unresolved[0].resolved = True
        unresolved[0].resolved_date = date.today()
        ps.save_project(project)

        # No blockers in dump
        text = generate_dump(*stores)
        assert "BLOCKERS (0)" in text

    def test_multiple_journal_entries_in_dump(self, stores):
        """Multiple journal entries appear in chronological order."""
        ps, js, pe, ac = stores

        js.append(
            JournalEntry(time="08:00", entry_type="Started", project="A", details={})
        )
        js.append(
            JournalEntry(time="10:00", entry_type="Note", project="A", details={})
        )
        js.append(
            JournalEntry(
                time="12:00",
                entry_type="Switched",
                project="A \u2192 B",
                details={},
            )
        )

        text = generate_dump(*stores)
        # All three times should appear
        assert "08:00" in text
        assert "10:00" in text
        assert "12:00" in text
        # And in order
        pos_08 = text.index("08:00")
        pos_10 = text.index("10:00")
        pos_12 = text.index("12:00")
        assert pos_08 < pos_10 < pos_12
