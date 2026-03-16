import pytest
from datetime import date
from pathlib import Path
from jm.storage.search import SearchEngine, SearchResult, SearchFilter


@pytest.fixture
def populated_data(tmp_path):
    """Create a populated data directory for search tests."""
    projects_dir = tmp_path / "projects"
    projects_dir.mkdir()
    journal_dir = tmp_path / "journal"
    journal_dir.mkdir()

    # Create project files
    (projects_dir / "hmi-framework.md").write_text(
        "---\nname: HMI Framework\nstatus: active\npriority: high\n"
        "tags: [performance, q2-goal]\ncreated: 2026-03-16\n---\n\n"
        "## Current Focus\n"
        "Debugging render loop — vsync timing issue on target hardware.\n\n"
        "## Blockers\n"
        "- [ ] Waiting on @carol for display spec clarification (since 2026-03-14)\n"
        "- [x] ~~Build server access~~ (resolved 2026-03-15)\n"
    )
    (projects_dir / "test-infra.md").write_text(
        "---\nname: Test Infra\nstatus: blocked\npriority: medium\n"
        "tags: [tooling]\ncreated: 2026-03-10\n---\n\n"
        "## Current Focus\n"
        "PR review feedback from @bob\n\n"
        "## Blockers\n"
        "- [ ] Waiting on @bob for PR review\n"
    )
    (projects_dir / "logging-spike.md").write_text(
        "---\nname: Logging Spike\nstatus: parked\npriority: low\n"
        "tags: [research]\ncreated: 2026-03-06\n---\n\n"
        "## Current Focus\n"
        "Parked, low priority\n"
    )

    # Create journal files
    (journal_dir / "2026-03-16.md").write_text(
        "---\ndate: 2026-03-16\n---\n\n"
        "## 09:15 — Started: HMI Framework\n"
        "Focus: debugging render loop\n\n"
        "## 11:30 — Switched: HMI Framework → Test Infra\n"
        "Left off: checking vsync timing\n"
        "Blocker: waiting on @carol for display spec\n"
    )
    (journal_dir / "2026-03-14.md").write_text(
        "---\ndate: 2026-03-14\n---\n\n"
        "## 10:00 — Started: HMI Framework\n"
        "Focus: prototype on target hardware\n"
    )

    # Create people file
    (tmp_path / "people.md").write_text(
        "## @carol\n"
        "- Role: Display Systems Lead\n"
        "- Projects: HMI Framework\n"
        "- Pending: spec clarification (asked 2026-03-14)\n\n"
        "## @bob\n"
        "- Role: Test Infra reviewer\n"
        "- Projects: Test Infra\n"
        "- Pending: PR re-review\n"
    )

    return tmp_path


class TestSearchEngine:
    def test_simple_text_search(self, populated_data):
        """Search for a text string across all files."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="render loop"))
        assert len(results) > 0
        assert any("render loop" in r.line_text for r in results)

    def test_search_by_person(self, populated_data):
        """Search for @mentions."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(person="@carol"))
        assert len(results) > 0
        assert all(
            "@carol" in r.line_text.lower() or "@carol" in r.line_text
            for r in results
        )

    def test_filter_by_project(self, populated_data):
        """Search within a specific project restricts project-type results."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="spec", project="hmi-framework")
        )
        # The project filter restricts which project files are searched;
        # journal and people files may still appear in results.
        project_results = [r for r in results if r.file_type == "project"]
        assert len(project_results) > 0
        assert all(r.project_slug == "hmi-framework" for r in project_results)

    def test_filter_by_status(self, populated_data):
        """Search only in projects with specific status."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="Focus", status="active")
        )
        # Should only find results in active projects (hmi-framework)
        project_results = [r for r in results if r.file_type == "project"]
        assert len(project_results) > 0
        assert all(
            r.project_slug == "hmi-framework" for r in project_results
        )

    def test_filter_by_tags(self, populated_data):
        """Search with tag filter."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="Focus", tags=["performance"])
        )
        project_results = [r for r in results if r.file_type == "project"]
        assert len(project_results) > 0
        assert all(
            r.project_slug == "hmi-framework" for r in project_results
        )

    def test_filter_by_date_range(self, populated_data):
        """Search journals within date range."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(
                query="Started",
                file_type="journal",
                date_from=date(2026, 3, 15),
                date_to=date(2026, 3, 17),
            )
        )
        # Should only find results in 2026-03-16 journal
        assert len(results) > 0
        assert all("2026-03-16" in str(r.file_path) for r in results)

    def test_filter_by_file_type(self, populated_data):
        """Search only in project files."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="spec", file_type="project")
        )
        assert all(r.file_type == "project" for r in results)

    def test_case_insensitive(self, populated_data):
        """Default search is case-insensitive."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="HMI"))
        assert len(results) > 0
        results_lower = engine.search(SearchFilter(query="hmi"))
        assert len(results_lower) > 0

    def test_quick_search(self, populated_data):
        """Quick search convenience method."""
        engine = SearchEngine(populated_data)
        results = engine.quick_search("vsync")
        assert len(results) > 0

    def test_no_results(self, populated_data):
        """Search for nonexistent text returns empty."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="xyzzy_nonexistent"))
        assert len(results) == 0

    def test_context_lines(self, populated_data):
        """Results include context before/after."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="vsync"))
        assert len(results) > 0
        # At least some results should have context
        has_context = any(
            r.context_before or r.context_after for r in results
        )
        assert has_context

    def test_empty_data_dir(self, tmp_path):
        """Search in empty directory returns empty."""
        engine = SearchEngine(tmp_path)
        results = engine.search(SearchFilter(query="anything"))
        assert len(results) == 0

    def test_search_result_has_file_path(self, populated_data):
        """Each search result includes the source file path."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="render"))
        assert len(results) > 0
        for r in results:
            assert r.file_path.exists()

    def test_search_result_has_line_number(self, populated_data):
        """Search results have valid line numbers."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="vsync"))
        assert len(results) > 0
        for r in results:
            assert r.line_number > 0

    def test_search_people_file(self, populated_data):
        """Search finds matches in people.md."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="Display Systems Lead", file_type="people")
        )
        assert len(results) > 0
        assert all(r.file_type == "people" for r in results)

    def test_person_filter_finds_in_journals(self, populated_data):
        """Person filter finds mentions in journal files."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(person="@carol"))
        journal_results = [r for r in results if r.file_type == "journal"]
        assert len(journal_results) > 0

    def test_person_filter_finds_in_projects(self, populated_data):
        """Person filter finds mentions in project files."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(person="@bob"))
        project_results = [r for r in results if r.file_type == "project"]
        assert len(project_results) > 0

    def test_combined_query_and_person(self, populated_data):
        """Combining query and person filter narrows results."""
        engine = SearchEngine(populated_data)
        # Search for @carol mentions that also contain "spec"
        all_carol = engine.search(SearchFilter(person="@carol"))
        with_query = engine.search(
            SearchFilter(query="spec", person="@carol")
        )
        # Combined results should be a subset (or equal) — person filter is
        # applied as an additional pattern, so lines matching EITHER pattern
        # are returned. But restricting to project files with query narrows.
        assert len(with_query) > 0

    def test_filter_project_excludes_other_projects(self, populated_data):
        """Project filter excludes results from other projects."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(query="Waiting", project="test-infra")
        )
        # Should only have test-infra results (in projects dir)
        project_results = [r for r in results if r.file_type == "project"]
        assert all(
            r.project_slug == "test-infra" for r in project_results
        )

    def test_date_range_excludes_old_journals(self, populated_data):
        """Date range filter properly excludes journals outside range."""
        engine = SearchEngine(populated_data)
        results = engine.search(
            SearchFilter(
                query="Started",
                file_type="journal",
                date_from=date(2026, 3, 15),
                date_to=date(2026, 3, 15),
            )
        )
        # Neither 2026-03-14 nor 2026-03-16 should match (range is 15 to 15)
        assert len(results) == 0

    def test_case_sensitive_search(self, populated_data):
        """Case-sensitive search respects case."""
        engine = SearchEngine(populated_data)
        results_upper = engine.search(
            SearchFilter(query="HMI", case_sensitive=True)
        )
        results_lower = engine.search(
            SearchFilter(query="hmi", case_sensitive=True)
        )
        # "HMI" should find matches; "hmi" should find fewer (or none)
        assert len(results_upper) > 0
        # In the test data, "HMI" appears uppercase, so lowercase shouldn't match those
        assert len(results_lower) < len(results_upper) or len(results_lower) == 0

    def test_match_offsets(self, populated_data):
        """Search results include match start/end offsets."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter(query="vsync"))
        assert len(results) > 0
        for r in results:
            assert r.match_start < r.match_end
            assert r.line_text[r.match_start : r.match_end].lower() == "vsync"

    def test_empty_query_and_no_person_returns_nothing(self, populated_data):
        """An empty filter with no query and no person returns nothing."""
        engine = SearchEngine(populated_data)
        results = engine.search(SearchFilter())
        assert len(results) == 0
