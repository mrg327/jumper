"""File-based CRUD storage for projects, journals, and people.

All write operations use atomic writes (write to .tmp, then os.replace)
to prevent data corruption. Corrupt/unparseable files are logged and skipped.
"""

from __future__ import annotations

import logging
import os
from datetime import date, timedelta
from pathlib import Path

from jm.models.journal import DailyJournal, JournalEntry
from jm.models.person import PeopleFile, Person
from jm.models.project import Project

logger = logging.getLogger(__name__)


def _atomic_write(path: Path, content: str) -> None:
    """Write content to path atomically via a temp file + os.replace."""
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    try:
        tmp_path.write_text(content, encoding="utf-8")
        os.replace(tmp_path, path)
    except BaseException:
        # Clean up the temp file on any failure
        try:
            tmp_path.unlink(missing_ok=True)
        except OSError:
            pass
        raise


class ProjectStore:
    """CRUD operations for project markdown files in data_dir/projects/."""

    def __init__(self, data_dir: Path) -> None:
        self.projects_dir = data_dir / "projects"
        self.projects_dir.mkdir(parents=True, exist_ok=True)

    def list_projects(self, status: str | None = None) -> list[Project]:
        """List all projects, optionally filtered by status.

        Returns projects sorted by file modification time (newest first).
        """
        projects: list[tuple[float, Project]] = []

        for md_file in self.projects_dir.glob("*.md"):
            try:
                text = md_file.read_text(encoding="utf-8")
                project = Project.from_markdown(text)
                mtime = md_file.stat().st_mtime
                if status is None or project.status == status:
                    projects.append((mtime, project))
            except Exception:
                logger.warning("Skipping corrupt project file: %s", md_file, exc_info=True)
                continue

        # Sort by modification time, newest first
        projects.sort(key=lambda pair: pair[0], reverse=True)
        return [p for _, p in projects]

    def get_project(self, slug: str) -> Project | None:
        """Get a single project by slug. Returns None if not found."""
        path = self.projects_dir / f"{slug}.md"
        if not path.exists():
            return None
        try:
            text = path.read_text(encoding="utf-8")
            return Project.from_markdown(text)
        except Exception:
            logger.warning("Failed to parse project file: %s", path, exc_info=True)
            return None

    def save_project(self, project: Project) -> Path:
        """Write a project to disk atomically. Returns the file path.

        Automatically adjusts status between active/blocked based on open
        blockers. Does not touch parked or done statuses.
        """
        has_open_blockers = any(not b.resolved for b in project.blockers)
        if has_open_blockers and project.status == "active":
            project.status = "blocked"
        elif not has_open_blockers and project.status == "blocked":
            project.status = "active"

        path = self.projects_dir / f"{project.slug}.md"
        _atomic_write(path, project.to_markdown())
        return path

    def create_project(self, name: str, **kwargs: object) -> Project:
        """Create a new project with defaults and save it."""
        project = Project(name=name, **kwargs)  # type: ignore[arg-type]
        self.save_project(project)
        return project

    def delete_project(self, slug: str) -> bool:
        """Delete a project file. Returns True if deleted, False if not found."""
        path = self.projects_dir / f"{slug}.md"
        if not path.exists():
            return False
        path.unlink()
        return True


class JournalStore:
    """CRUD operations for daily journal markdown files in data_dir/journal/."""

    def __init__(self, data_dir: Path) -> None:
        self.journal_dir = data_dir / "journal"
        self.journal_dir.mkdir(parents=True, exist_ok=True)

    def _path_for_date(self, target_date: date) -> Path:
        return self.journal_dir / f"{target_date.isoformat()}.md"

    def today(self) -> DailyJournal:
        """Get today's journal, creating a new one if it doesn't exist on disk."""
        return self.get_day(date.today()) or DailyJournal(date=date.today())

    def append(self, entry: JournalEntry) -> DailyJournal:
        """Append an entry to today's journal and save it."""
        journal = self.today()
        journal.append_entry(entry)
        self.save(journal)
        return journal

    def get_day(self, target_date: date) -> DailyJournal | None:
        """Get journal for a specific date. Returns None if not found."""
        path = self._path_for_date(target_date)
        if not path.exists():
            return None
        try:
            text = path.read_text(encoding="utf-8")
            return DailyJournal.from_markdown(text)
        except Exception:
            logger.warning("Failed to parse journal file: %s", path, exc_info=True)
            return None

    def get_previous_workday(self, target_date: date | None = None) -> DailyJournal | None:
        """Get the most recent journal before target_date.

        Useful for morning review. Searches up to 14 days back.
        """
        start = target_date or date.today()
        for days_back in range(1, 15):
            check_date = start - timedelta(days=days_back)
            journal = self.get_day(check_date)
            if journal is not None:
                return journal
        return None

    def save(self, journal: DailyJournal) -> Path:
        """Save a journal to disk atomically."""
        path = self._path_for_date(journal.date)
        _atomic_write(path, journal.to_markdown())
        return path


class PeopleStore:
    """CRUD operations for the people.md file in data_dir/."""

    def __init__(self, data_dir: Path) -> None:
        self.data_dir = data_dir
        self.people_file = data_dir / "people.md"

    def load(self) -> PeopleFile:
        """Load people file. Returns empty PeopleFile if file not found."""
        if not self.people_file.exists():
            return PeopleFile()
        try:
            text = self.people_file.read_text(encoding="utf-8")
            return PeopleFile.from_markdown(text)
        except Exception:
            logger.warning("Failed to parse people file: %s", self.people_file, exc_info=True)
            return PeopleFile()

    def save(self, people: PeopleFile) -> Path:
        """Save people file atomically."""
        _atomic_write(self.people_file, people.to_markdown())
        return self.people_file

    def get_person(self, handle: str) -> Person | None:
        """Get a person by handle (e.g., '@carol')."""
        people = self.load()
        for person in people.people:
            if person.handle == handle:
                return person
        return None

    def add_or_update_person(self, person: Person) -> PeopleFile:
        """Add or update a person in the people file.

        If a person with the same handle exists, merge the records:
        - Preserve existing role unless new one is non-empty
        - Union the projects lists (no duplicates)
        - Append new pending items (avoid duplicates by description)
        Otherwise the person is appended.
        """
        people = self.load()
        for i, existing in enumerate(people.people):
            if existing.handle == person.handle:
                # Preserve existing role unless new one is non-empty
                merged_role = person.role if person.role else existing.role
                # Union projects lists (no duplicates)
                merged_projects = list(existing.projects)
                for proj in person.projects:
                    if proj not in merged_projects:
                        merged_projects.append(proj)
                # Append new pending items (avoid duplicates by description)
                existing_descriptions = {p.description for p in existing.pending}
                merged_pending = list(existing.pending)
                for item in person.pending:
                    if item.description not in existing_descriptions:
                        merged_pending.append(item)
                        existing_descriptions.add(item.description)
                people.people[i] = Person(
                    handle=existing.handle,
                    role=merged_role,
                    projects=merged_projects,
                    pending=merged_pending,
                )
                self.save(people)
                return people
        people.people.append(person)
        self.save(people)
        return people


class ActiveProjectStore:
    """Tracks which project is currently being worked on via data_dir/.active file."""

    def __init__(self, data_dir: Path) -> None:
        self.active_file = data_dir / ".active"

    def get_active(self) -> str | None:
        """Get the active project slug. Returns None if none active."""
        if not self.active_file.exists():
            return None
        try:
            slug = self.active_file.read_text(encoding="utf-8").strip()
            return slug if slug else None
        except Exception:
            logger.warning("Failed to read active file: %s", self.active_file, exc_info=True)
            return None

    def set_active(self, slug: str) -> None:
        """Set the active project slug."""
        _atomic_write(self.active_file, slug)

    def clear_active(self) -> None:
        """Clear the active project."""
        try:
            self.active_file.unlink(missing_ok=True)
        except OSError:
            logger.warning("Failed to clear active file: %s", self.active_file, exc_info=True)


def create_stores(
    config: dict | None = None,
) -> tuple[ProjectStore, JournalStore, PeopleStore, ActiveProjectStore]:
    """Create all stores using config. Calls ensure_dirs first."""
    from jm.config import ensure_dirs, load_config

    cfg = config or load_config()
    data_dir = ensure_dirs(cfg)
    return (
        ProjectStore(data_dir),
        JournalStore(data_dir),
        PeopleStore(data_dir),
        ActiveProjectStore(data_dir),
    )
