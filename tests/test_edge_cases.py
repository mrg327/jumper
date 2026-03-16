import pytest
from datetime import date
from pathlib import Path

from jm.models import (
    Project,
    Blocker,
    DailyJournal,
    JournalEntry,
    PeopleFile,
    Person,
    PendingItem,
    LogEntry,
    Decision,
)
from jm.storage.store import ProjectStore, JournalStore, PeopleStore, ActiveProjectStore
from jm.storage.search import SearchEngine, SearchFilter
from jm.export import generate_dump
from jm.config import DEFAULT_CONFIG


class TestFirstRun:
    """Test behavior when ~/.jm/ doesn't exist yet."""

    def test_project_store_creates_dir(self, tmp_path):
        """ProjectStore creates projects/ dir on init."""
        data_dir = tmp_path / "fresh"
        store = ProjectStore(data_dir)
        assert (data_dir / "projects").exists()

    def test_journal_store_creates_dir(self, tmp_path):
        """JournalStore creates journal/ dir on init."""
        data_dir = tmp_path / "fresh"
        store = JournalStore(data_dir)
        assert (data_dir / "journal").exists()

    def test_empty_state_dump(self, tmp_path):
        """Dump with completely empty data dir."""
        ps = ProjectStore(tmp_path)
        js = JournalStore(tmp_path)
        pe = PeopleStore(tmp_path)
        ac = ActiveProjectStore(tmp_path)
        text = generate_dump(ps, js, pe, ac)
        assert "No projects yet" in text
        assert "No open blockers" in text
        assert "ACTIVE: none" in text


class TestCorruptFiles:
    """Test graceful handling of corrupt/malformed data."""

    def test_corrupt_project_file_skipped(self, tmp_path):
        """Corrupt project files are skipped without crashing."""
        store = ProjectStore(tmp_path)
        store.create_project("Good Project")
        # Write garbage to a project file
        (tmp_path / "projects" / "bad.md").write_text(
            "not valid frontmatter\n---\n---\ngarbage"
        )

        projects = store.list_projects()
        # Should get at least the good project, and not crash
        good_names = [p.name for p in projects]
        assert "Good Project" in good_names

    def test_corrupt_journal_returns_none(self, tmp_path):
        """Corrupt journal file returns None from get_day."""
        store = JournalStore(tmp_path)
        (tmp_path / "journal" / "2026-03-16.md").write_text("totally broken {{{{")

        result = store.get_day(date(2026, 3, 16))
        # Should either return None or a journal with no entries, but not crash
        assert result is None or isinstance(result, DailyJournal)

    def test_corrupt_people_returns_empty(self, tmp_path):
        """Corrupt people file returns empty PeopleFile."""
        store = PeopleStore(tmp_path)
        (tmp_path / "people.md").write_text("{{{{broken yaml\n---\ngarbage")

        result = store.load()
        assert isinstance(result, PeopleFile)

    def test_empty_project_file(self, tmp_path):
        """Empty .md file is handled gracefully."""
        store = ProjectStore(tmp_path)
        (tmp_path / "projects" / "empty.md").write_text("")
        projects = store.list_projects()
        # Should not crash
        assert isinstance(projects, list)

    def test_binary_file_in_projects(self, tmp_path):
        """Binary file in projects dir is handled."""
        store = ProjectStore(tmp_path)
        (tmp_path / "projects" / "binary.md").write_bytes(b"\x00\x01\x02\x03")
        store.create_project("Real Project")
        projects = store.list_projects()
        names = [p.name for p in projects]
        assert "Real Project" in names


class TestLongValues:
    """Test with very long strings and lists."""

    def test_long_project_name(self, tmp_path):
        """Project with very long name round-trips."""
        store = ProjectStore(tmp_path)
        long_name = "A" * 200
        project = store.create_project(long_name)
        retrieved = store.get_project(project.slug)
        assert retrieved is not None
        assert retrieved.name == long_name

    def test_long_focus_text(self, tmp_path):
        """Very long current_focus round-trips."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.current_focus = "x" * 500
        store.save_project(project)
        retrieved = store.get_project("test")
        assert retrieved.current_focus == "x" * 500

    def test_many_blockers(self, tmp_path):
        """Project with many blockers round-trips."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.blockers = [
            Blocker(description=f"blocker {i}", since=date.today()) for i in range(50)
        ]
        store.save_project(project)
        retrieved = store.get_project("test")
        assert len(retrieved.blockers) == 50

    def test_many_log_entries(self, tmp_path):
        """Project with many log entries round-trips."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.log = [
            LogEntry(date=date(2026, 1, i + 1), lines=[f"entry {i}"])
            for i in range(31)
        ]
        store.save_project(project)
        retrieved = store.get_project("test")
        assert len(retrieved.log) == 31

    def test_many_tags(self, tmp_path):
        """Project with many tags round-trips."""
        store = ProjectStore(tmp_path)
        tags = [f"tag-{i}" for i in range(20)]
        project = store.create_project("Test", tags=tags)
        retrieved = store.get_project("test")
        assert retrieved.tags == tags

    def test_long_journal_details(self, tmp_path):
        """Journal entry with very long details round-trips."""
        store = JournalStore(tmp_path)
        entry = JournalEntry(
            time="09:00",
            entry_type="Note",
            project="Test",
            details={"note": "x" * 500, "extra": "y" * 300},
        )
        store.append(entry)
        journal = store.today()
        assert len(journal.entries) == 1
        assert journal.entries[0].details.get("note") == "x" * 500


class TestSpecialCharacters:
    """Test with special characters in various fields."""

    def test_project_name_with_special_chars(self, tmp_path):
        """Project name with hyphens and numbers."""
        store = ProjectStore(tmp_path)
        project = store.create_project("v2.0-Migration")
        assert project.slug == "v2.0-migration"
        retrieved = store.get_project("v2.0-migration")
        assert retrieved is not None
        assert retrieved.name == "v2.0-Migration"

    def test_blocker_with_url(self, tmp_path):
        """Blocker containing a URL round-trips."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.blockers = [
            Blocker(description="see https://example.com/issue/123")
        ]
        store.save_project(project)
        retrieved = store.get_project("test")
        assert "https://example.com/issue/123" in retrieved.blockers[0].description

    def test_note_with_quotes(self, tmp_path):
        """Note containing quotes round-trips."""
        store = JournalStore(tmp_path)
        entry = JournalEntry(
            time="09:00",
            entry_type="Note",
            project="Test",
            details={"note": 'said "hello world" to the team'},
        )
        store.append(entry)
        journal = store.today()
        assert '"hello world"' in journal.entries[0].details.get("note", "")


class TestActiveProjectEdgeCases:
    """Edge cases for active project tracking."""

    def test_active_project_deleted(self, tmp_path):
        """Active project file deleted -- get_active still returns slug."""
        ps = ProjectStore(tmp_path)
        ac = ActiveProjectStore(tmp_path)
        ps.create_project("Test")
        ac.set_active("test")
        ps.delete_project("test")

        # Active file still has the slug
        assert ac.get_active() == "test"
        # But project is gone
        assert ps.get_project("test") is None

    def test_active_file_with_whitespace(self, tmp_path):
        """Active file with trailing whitespace/newlines."""
        ac = ActiveProjectStore(tmp_path)
        (tmp_path / ".active").write_text("  test-project  \n\n")
        assert ac.get_active() == "test-project"

    def test_active_file_empty(self, tmp_path):
        """Empty active file returns None."""
        ac = ActiveProjectStore(tmp_path)
        (tmp_path / ".active").write_text("")
        assert ac.get_active() is None


class TestSearchEdgeCases:
    """Edge cases for search functionality."""

    def test_search_with_regex_chars(self, tmp_path):
        """Search query with regex special characters doesn't crash."""
        (tmp_path / "projects").mkdir()
        (tmp_path / "projects" / "test.md").write_text(
            "---\nname: Test\nstatus: active\npriority: medium\ntags: []\ncreated: 2026-03-16\n---\n\n"
            "Some content with (parentheses) and [brackets]"
        )

        engine = SearchEngine(tmp_path)
        # These have regex special chars -- should not crash
        results = engine.search(SearchFilter(query="(parentheses)"))
        assert len(results) >= 0  # Just don't crash

        results = engine.search(SearchFilter(query="[brackets]"))
        assert len(results) >= 0

    def test_search_empty_query(self, tmp_path):
        """Empty query returns no results."""
        engine = SearchEngine(tmp_path)
        results = engine.search(SearchFilter(query=""))
        assert len(results) == 0


class TestConfigDefaults:
    """Test config loading with defaults."""

    def test_default_config_values(self):
        """Default config has expected keys."""
        assert "data_dir" in DEFAULT_CONFIG
        assert "statuses" in DEFAULT_CONFIG
        assert "priorities" in DEFAULT_CONFIG
        assert DEFAULT_CONFIG["data_dir"] == "~/.jm"

    def test_ensure_dirs_creates_structure(self, tmp_path):
        """ensure_dirs creates the expected directory structure."""
        from jm.config import ensure_dirs

        config = {"data_dir": str(tmp_path / "jm-data")}
        data_dir = ensure_dirs(config)
        assert (data_dir / "projects").exists()
        assert (data_dir / "journal").exists()
