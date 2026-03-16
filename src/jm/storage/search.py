from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from typing import Optional
import re

import frontmatter

from jm.config import get_data_dir, load_config


@dataclass
class SearchResult:
    """A single search result with context."""

    file_path: Path
    file_type: str  # "project" | "journal" | "people"
    project_slug: str  # slug if project file, else ""
    line_number: int
    line_text: str  # The matching line
    context_before: list[str] = field(default_factory=list)  # 1-2 lines before
    context_after: list[str] = field(default_factory=list)  # 1-2 lines after
    match_start: int = 0  # Character offset of match in line_text
    match_end: int = 0  # End offset


@dataclass
class SearchFilter:
    """Filters to narrow search results."""

    query: str = ""  # Text to search for (substring or regex)
    project: str | None = None  # Filter to specific project slug
    person: str | None = None  # Filter by @mention (e.g., "@carol")
    tags: list[str] = field(default_factory=list)  # Filter by tags
    status: str | None = None  # Filter by project status
    date_from: date | None = None  # Journal entries after this date
    date_to: date | None = None  # Journal entries before this date
    file_type: str | None = None  # "project" | "journal" | "people"
    case_sensitive: bool = False


class SearchEngine:
    def __init__(self, data_dir: Path | None = None):
        self.data_dir = data_dir or get_data_dir()

    def search(self, filter: SearchFilter) -> list[SearchResult]:
        """Search across all markdown files with the given filter.

        Returns results sorted by relevance (file modification time, newest first).
        """
        results = []

        # Collect files to search based on file_type filter
        files = self._collect_files(filter)

        for file_path in files:
            file_results = self._search_file(file_path, filter)
            results.extend(file_results)

        return results

    def _collect_files(self, filter: SearchFilter) -> list[Path]:
        """Collect markdown files to search, applying file-level filters."""
        files: list[Path] = []

        projects_dir = self.data_dir / "projects"
        journal_dir = self.data_dir / "journal"
        people_file = self.data_dir / "people.md"

        # Project files
        if filter.file_type is None or filter.file_type == "project":
            if projects_dir.exists():
                for f in sorted(projects_dir.glob("*.md")):
                    # Filter by project slug
                    if filter.project and f.stem != filter.project:
                        continue

                    # Filter by status/tags requires parsing frontmatter
                    if filter.status or filter.tags:
                        try:
                            post = frontmatter.load(str(f))
                            meta = post.metadata
                            if filter.status and meta.get("status") != filter.status:
                                continue
                            if filter.tags:
                                file_tags = meta.get("tags", [])
                                if not any(t in file_tags for t in filter.tags):
                                    continue
                        except Exception:
                            continue

                    files.append(f)

        # Journal files
        if filter.file_type is None or filter.file_type == "journal":
            if journal_dir.exists():
                for f in sorted(journal_dir.glob("*.md")):
                    # Filter by date range
                    try:
                        file_date = date.fromisoformat(f.stem)
                        if filter.date_from and file_date < filter.date_from:
                            continue
                        if filter.date_to and file_date > filter.date_to:
                            continue
                    except ValueError:
                        pass  # Not a date-named file, include anyway
                    files.append(f)

        # People file
        if filter.file_type is None or filter.file_type == "people":
            if people_file.exists():
                files.append(people_file)

        # Sort by modification time (newest first)
        files.sort(key=lambda f: f.stat().st_mtime, reverse=True)
        return files

    def _search_file(
        self, file_path: Path, filter: SearchFilter
    ) -> list[SearchResult]:
        """Search within a single file."""
        try:
            content = file_path.read_text(encoding="utf-8")
        except Exception:
            return []

        lines = content.split("\n")
        results = []

        # Determine file type
        file_type = self._get_file_type(file_path)
        project_slug = file_path.stem if file_type == "project" else ""

        # If there's nothing to search for, return empty
        if not filter.query and not filter.person:
            return []

        # Build regex patterns for text query and person filter
        patterns: list[re.Pattern[str]] = []

        if filter.query:
            flags = 0 if filter.case_sensitive else re.IGNORECASE
            try:
                patterns.append(re.compile(filter.query, flags))
            except re.error:
                # If the query isn't valid regex, escape it and treat as literal
                patterns.append(re.compile(re.escape(filter.query), flags))

        if filter.person:
            person_pattern = re.escape(filter.person)
            patterns.append(re.compile(person_pattern, re.IGNORECASE))

        for i, line in enumerate(lines):
            for pattern in patterns:
                match = pattern.search(line)
                if match:
                    # Get context lines (up to 2 before, up to 2 after)
                    context_before = lines[max(0, i - 2) : i]
                    context_after = lines[i + 1 : min(len(lines), i + 3)]

                    results.append(
                        SearchResult(
                            file_path=file_path,
                            file_type=file_type,
                            project_slug=project_slug,
                            line_number=i + 1,
                            line_text=line,
                            context_before=context_before,
                            context_after=context_after,
                            match_start=match.start(),
                            match_end=match.end(),
                        )
                    )
                    break  # Don't double-count a line matching multiple patterns

        return results

    def _get_file_type(self, file_path: Path) -> str:
        """Determine file type from path."""
        if "projects" in file_path.parts:
            return "project"
        elif "journal" in file_path.parts:
            return "journal"
        elif file_path.name == "people.md":
            return "people"
        return "unknown"

    def quick_search(self, query: str) -> list[SearchResult]:
        """Convenience method for simple text search across everything."""
        return self.search(SearchFilter(query=query))
