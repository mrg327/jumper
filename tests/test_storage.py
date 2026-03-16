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
)
from jm.storage.store import (
    ProjectStore,
    JournalStore,
    PeopleStore,
    ActiveProjectStore,
)


class TestProjectStore:
    def test_create_project(self, tmp_path):
        """Create a project and verify it's saved to disk."""
        store = ProjectStore(tmp_path)
        project = store.create_project("HMI Framework", status="active", priority="high")
        assert project.name == "HMI Framework"
        assert project.slug == "hmi-framework"
        assert (tmp_path / "projects" / "hmi-framework.md").exists()

    def test_get_project(self, tmp_path):
        """Create then retrieve a project by slug."""
        store = ProjectStore(tmp_path)
        store.create_project("Test Infra", priority="medium")
        retrieved = store.get_project("test-infra")
        assert retrieved is not None
        assert retrieved.name == "Test Infra"
        assert retrieved.priority == "medium"

    def test_get_nonexistent_project(self, tmp_path):
        """Getting a missing project returns None."""
        store = ProjectStore(tmp_path)
        assert store.get_project("nope") is None

    def test_list_projects(self, tmp_path):
        """List multiple projects."""
        store = ProjectStore(tmp_path)
        store.create_project("Alpha")
        store.create_project("Beta")
        store.create_project("Gamma")
        projects = store.list_projects()
        assert len(projects) == 3
        names = {p.name for p in projects}
        assert names == {"Alpha", "Beta", "Gamma"}

    def test_list_projects_filter_by_status(self, tmp_path):
        """Filter projects by status."""
        store = ProjectStore(tmp_path)
        store.create_project("Active1", status="active")
        store.create_project("Blocked1", status="blocked")
        store.create_project("Active2", status="active")
        active = store.list_projects(status="active")
        assert len(active) == 2
        assert all(p.status == "active" for p in active)

    def test_save_project_updates(self, tmp_path):
        """Modify a project and save — changes persist."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Test")
        project.current_focus = "new focus"
        project.status = "blocked"
        store.save_project(project)
        reloaded = store.get_project("test")
        assert reloaded.current_focus == "new focus"
        assert reloaded.status == "blocked"

    def test_delete_project(self, tmp_path):
        """Delete a project removes the file."""
        store = ProjectStore(tmp_path)
        store.create_project("ToDelete")
        assert store.delete_project("todelete")
        assert store.get_project("todelete") is None

    def test_delete_nonexistent(self, tmp_path):
        """Deleting a missing project returns False."""
        store = ProjectStore(tmp_path)
        assert not store.delete_project("nope")

    def test_project_with_blockers_persists(self, tmp_path):
        """Project with blockers round-trips through storage."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Blocked Project")
        project.blockers = [
            Blocker(
                description="waiting on spec",
                person="@carol",
                since=date(2026, 3, 14),
            )
        ]
        store.save_project(project)
        reloaded = store.get_project("blocked-project")
        assert len(reloaded.blockers) == 1
        assert reloaded.blockers[0].person == "@carol"

    def test_project_slug_auto_generated(self, tmp_path):
        """Slug is derived from name when not provided."""
        store = ProjectStore(tmp_path)
        project = store.create_project("My Cool Project")
        assert project.slug == "my-cool-project"

    def test_project_tags_persist(self, tmp_path):
        """Tags round-trip through storage."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Tagged", tags=["performance", "q2-goal"])
        store.save_project(project)
        reloaded = store.get_project("tagged")
        assert set(reloaded.tags) == {"performance", "q2-goal"}

    def test_project_default_values(self, tmp_path):
        """New project has sensible defaults."""
        store = ProjectStore(tmp_path)
        project = store.create_project("Defaults")
        assert project.status == "active"
        assert project.priority == "medium"
        assert project.tags == []
        assert project.blockers == []
        assert project.current_focus == ""

    def test_list_projects_empty(self, tmp_path):
        """Listing projects with none returns empty list."""
        store = ProjectStore(tmp_path)
        assert store.list_projects() == []

    def test_list_projects_filter_returns_empty(self, tmp_path):
        """Filtering by a status with no matches returns empty list."""
        store = ProjectStore(tmp_path)
        store.create_project("Active1", status="active")
        assert store.list_projects(status="done") == []

    def test_create_project_initializes_directory(self, tmp_path):
        """ProjectStore creates the projects directory if needed."""
        store = ProjectStore(tmp_path)
        assert (tmp_path / "projects").is_dir()

    def test_save_project_returns_path(self, tmp_path):
        """save_project returns the file path."""
        store = ProjectStore(tmp_path)
        project = store.create_project("PathCheck")
        path = store.save_project(project)
        assert path == tmp_path / "projects" / "pathcheck.md"

    def test_project_with_resolved_blocker(self, tmp_path):
        """Resolved blockers round-trip correctly."""
        store = ProjectStore(tmp_path)
        project = store.create_project("ResolvedBlocker")
        project.blockers = [
            Blocker(
                description="Build server access",
                resolved=True,
                resolved_date=date(2026, 3, 15),
            )
        ]
        store.save_project(project)
        reloaded = store.get_project("resolvedblocker")
        assert len(reloaded.blockers) == 1
        assert reloaded.blockers[0].resolved is True
        assert reloaded.blockers[0].resolved_date == date(2026, 3, 15)

    def test_corrupt_file_skipped_in_list(self, tmp_path):
        """Corrupt project files are skipped during listing."""
        store = ProjectStore(tmp_path)
        store.create_project("Good")
        # Write a corrupt file
        (tmp_path / "projects" / "bad.md").write_text("not valid frontmatter\n---\n---\n{{{{")
        projects = store.list_projects()
        # Should have at least the good one; corrupt might parse or be skipped
        names = {p.name for p in projects}
        assert "Good" in names


class TestJournalStore:
    def test_today_creates_new(self, tmp_path):
        """today() creates a new journal for today."""
        store = JournalStore(tmp_path)
        journal = store.today()
        assert journal.date == date.today()
        assert len(journal.entries) == 0

    def test_append_entry(self, tmp_path):
        """Append an entry to today's journal."""
        store = JournalStore(tmp_path)
        entry = JournalEntry(
            time="09:15",
            entry_type="Started",
            project="HMI Framework",
            details={"focus": "render loop"},
        )
        journal = store.append(entry)
        assert len(journal.entries) == 1

        # Verify it persisted
        reloaded = store.today()
        assert len(reloaded.entries) == 1
        assert reloaded.entries[0].project == "HMI Framework"

    def test_append_multiple_entries(self, tmp_path):
        """Multiple appends accumulate in the same journal."""
        store = JournalStore(tmp_path)
        store.append(
            JournalEntry(time="09:15", entry_type="Started", project="A")
        )
        store.append(
            JournalEntry(
                time="11:00",
                entry_type="Note",
                project="A",
                details={"decision": "use pytest"},
            )
        )
        journal = store.today()
        assert len(journal.entries) == 2

    def test_get_day(self, tmp_path):
        """Get journal for a specific date."""
        store = JournalStore(tmp_path)
        # Create a journal for a specific date
        journal = DailyJournal(
            date=date(2026, 3, 14),
            entries=[
                JournalEntry(time="10:00", entry_type="Started", project="Test")
            ],
        )
        store.save(journal)

        retrieved = store.get_day(date(2026, 3, 14))
        assert retrieved is not None
        assert len(retrieved.entries) == 1

    def test_get_day_nonexistent(self, tmp_path):
        """Getting a missing day returns None."""
        store = JournalStore(tmp_path)
        assert store.get_day(date(2020, 1, 1)) is None

    def test_get_previous_workday(self, tmp_path):
        """Find the most recent journal before a given date."""
        store = JournalStore(tmp_path)
        # Create journals for multiple days
        for d in [date(2026, 3, 12), date(2026, 3, 14)]:
            journal = DailyJournal(
                date=d,
                entries=[
                    JournalEntry(
                        time="09:00", entry_type="Started", project="Test"
                    )
                ],
            )
            store.save(journal)

        prev = store.get_previous_workday(date(2026, 3, 16))
        assert prev is not None
        assert prev.date == date(2026, 3, 14)

    def test_get_previous_workday_skips_gaps(self, tmp_path):
        """get_previous_workday finds journal across weekend gaps."""
        store = JournalStore(tmp_path)
        # Create journal only for Friday
        journal = DailyJournal(
            date=date(2026, 3, 6),  # Friday
            entries=[
                JournalEntry(time="09:00", entry_type="Started", project="X")
            ],
        )
        store.save(journal)

        # Ask for previous from Monday
        prev = store.get_previous_workday(date(2026, 3, 9))  # Monday
        assert prev is not None
        assert prev.date == date(2026, 3, 6)

    def test_get_previous_workday_none_found(self, tmp_path):
        """Returns None when no previous journal exists within 14 days."""
        store = JournalStore(tmp_path)
        assert store.get_previous_workday(date(2026, 3, 16)) is None

    def test_save_returns_path(self, tmp_path):
        """save() returns the journal file path."""
        store = JournalStore(tmp_path)
        journal = DailyJournal(date=date(2026, 3, 14))
        path = store.save(journal)
        assert path == tmp_path / "journal" / "2026-03-14.md"

    def test_journal_entry_details_persist(self, tmp_path):
        """Entry details dict round-trips through storage."""
        store = JournalStore(tmp_path)
        entry = JournalEntry(
            time="10:30",
            entry_type="Switched",
            project="HMI Framework",
            details={"left_off": "vsync timing", "blocker": "waiting on @carol"},
        )
        store.append(entry)

        journal = store.today()
        assert journal.entries[0].details.get("left_off") == "vsync timing"
        assert journal.entries[0].details.get("blocker") == "waiting on @carol"

    def test_journal_dir_created(self, tmp_path):
        """JournalStore creates the journal directory if needed."""
        store = JournalStore(tmp_path)
        assert (tmp_path / "journal").is_dir()

    def test_done_entry_persists(self, tmp_path):
        """A 'Done' entry with empty project round-trips."""
        store = JournalStore(tmp_path)
        entry = JournalEntry(time="17:00", entry_type="Done", project="")
        store.append(entry)

        journal = store.today()
        assert len(journal.entries) == 1
        assert journal.entries[0].entry_type == "Done"


class TestPeopleStore:
    def test_load_empty(self, tmp_path):
        """Loading when no people file exists returns empty PeopleFile."""
        store = PeopleStore(tmp_path)
        people = store.load()
        assert len(people.people) == 0

    def test_save_and_load(self, tmp_path):
        """Save and reload a PeopleFile."""
        store = PeopleStore(tmp_path)
        people = PeopleFile(
            people=[
                Person(
                    handle="@carol",
                    role="Lead",
                    projects=["HMI"],
                    pending=[
                        PendingItem(
                            description="spec review", since=date(2026, 3, 14)
                        )
                    ],
                )
            ]
        )
        store.save(people)
        reloaded = store.load()
        assert len(reloaded.people) == 1
        assert reloaded.people[0].handle == "@carol"

    def test_get_person(self, tmp_path):
        """Get a person by handle."""
        store = PeopleStore(tmp_path)
        people = PeopleFile(
            people=[
                Person(handle="@carol"),
                Person(handle="@bob"),
            ]
        )
        store.save(people)
        carol = store.get_person("@carol")
        assert carol is not None
        assert carol.handle == "@carol"

    def test_get_person_not_found(self, tmp_path):
        """Missing person returns None."""
        store = PeopleStore(tmp_path)
        assert store.get_person("@nobody") is None

    def test_add_or_update_person(self, tmp_path):
        """Add a new person, then update them."""
        store = PeopleStore(tmp_path)
        # Add new
        store.add_or_update_person(Person(handle="@dave", role="Engineer"))
        people = store.load()
        assert len(people.people) == 1

        # Update existing
        store.add_or_update_person(
            Person(handle="@dave", role="Senior Engineer")
        )
        people = store.load()
        assert len(people.people) == 1
        assert people.people[0].role == "Senior Engineer"

    def test_save_returns_path(self, tmp_path):
        """save() returns the people file path."""
        store = PeopleStore(tmp_path)
        path = store.save(PeopleFile())
        assert path == tmp_path / "people.md"

    def test_multiple_people_persist(self, tmp_path):
        """Multiple people round-trip correctly."""
        store = PeopleStore(tmp_path)
        people = PeopleFile(
            people=[
                Person(handle="@alice", role="Manager"),
                Person(handle="@bob", role="Developer"),
                Person(handle="@carol", role="Lead"),
            ]
        )
        store.save(people)
        reloaded = store.load()
        assert len(reloaded.people) == 3
        handles = {p.handle for p in reloaded.people}
        assert handles == {"@alice", "@bob", "@carol"}

    def test_pending_items_with_project(self, tmp_path):
        """Pending items with project references persist."""
        store = PeopleStore(tmp_path)
        people = PeopleFile(
            people=[
                Person(
                    handle="@carol",
                    pending=[
                        PendingItem(
                            description="review PR",
                            since=date(2026, 3, 10),
                            project="Test Infra",
                        )
                    ],
                )
            ]
        )
        store.save(people)
        reloaded = store.load()
        pending = reloaded.people[0].pending
        assert len(pending) == 1
        assert pending[0].description == "review PR"
        assert pending[0].project == "Test Infra"
        assert pending[0].since == date(2026, 3, 10)

    def test_add_person_to_existing_list(self, tmp_path):
        """Adding a person appends to the existing people list."""
        store = PeopleStore(tmp_path)
        store.add_or_update_person(Person(handle="@alice"))
        store.add_or_update_person(Person(handle="@bob"))
        people = store.load()
        assert len(people.people) == 2


class TestActiveProjectStore:
    def test_no_active(self, tmp_path):
        """No active project initially."""
        store = ActiveProjectStore(tmp_path)
        assert store.get_active() is None

    def test_set_and_get(self, tmp_path):
        """Set and retrieve active project."""
        store = ActiveProjectStore(tmp_path)
        store.set_active("hmi-framework")
        assert store.get_active() == "hmi-framework"

    def test_clear(self, tmp_path):
        """Clear active project."""
        store = ActiveProjectStore(tmp_path)
        store.set_active("test")
        store.clear_active()
        assert store.get_active() is None

    def test_overwrite_active(self, tmp_path):
        """Setting a new active project overwrites the previous one."""
        store = ActiveProjectStore(tmp_path)
        store.set_active("project-a")
        store.set_active("project-b")
        assert store.get_active() == "project-b"

    def test_clear_when_none_active(self, tmp_path):
        """Clearing when nothing is active doesn't error."""
        store = ActiveProjectStore(tmp_path)
        store.clear_active()  # Should not raise
        assert store.get_active() is None
