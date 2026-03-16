"""Comprehensive tests for jm model layer: round-trip serialization fidelity."""

from datetime import date

import pytest

from jm.models import (
    Blocker,
    DailyJournal,
    Decision,
    JournalEntry,
    LogEntry,
    PendingItem,
    PeopleFile,
    Person,
    Project,
)


# ── Project round-trip tests ──────────────────────────────────────────


class TestProjectRoundTrip:
    """Test Project to_markdown → from_markdown round-trip fidelity."""

    def _make_full_project(self) -> Project:
        """Create a fully-populated Project for testing."""
        return Project(
            name="HMI Framework",
            slug="hmi-framework",
            status="active",
            priority="high",
            tags=["infotainment", "rendering"],
            created=date(2026, 1, 15),
            target=date(2026, 6, 30),
            current_focus="debugging render loop",
            blockers=[
                Blocker(
                    description="waiting on display spec",
                    resolved=False,
                    since=date(2026, 3, 10),
                    person="@carol",
                ),
                Blocker(
                    description="GPU driver issue",
                    resolved=True,
                    resolved_date=date(2026, 3, 14),
                    person=None,
                ),
            ],
            decisions=[
                Decision(
                    date=date(2026, 2, 1),
                    choice="Use Vulkan over OpenGL",
                    alternatives=["OpenGL ES", "DirectFB"],
                ),
                Decision(
                    date=date(2026, 3, 1),
                    choice="Keep custom compositor",
                    alternatives=[],
                ),
            ],
            log=[
                LogEntry(
                    date=date(2026, 3, 14),
                    lines=["Fixed GPU driver init sequence", "Ran benchmarks"],
                ),
                LogEntry(
                    date=date(2026, 3, 15),
                    lines=["Started render loop debugging"],
                ),
            ],
        )

    def test_full_round_trip(self):
        """All fields survive a full round-trip."""
        original = self._make_full_project()
        md = original.to_markdown()
        restored = Project.from_markdown(md)

        assert restored.name == original.name
        assert restored.slug == original.slug
        assert restored.status == original.status
        assert restored.priority == original.priority
        assert restored.tags == original.tags
        assert restored.created == original.created
        assert restored.target == original.target
        assert restored.current_focus == original.current_focus

        # Blockers
        assert len(restored.blockers) == len(original.blockers)
        for orig_b, rest_b in zip(original.blockers, restored.blockers):
            assert rest_b.description == orig_b.description
            assert rest_b.resolved == orig_b.resolved
            assert rest_b.since == orig_b.since
            assert rest_b.resolved_date == orig_b.resolved_date
            assert rest_b.person == orig_b.person

        # Decisions
        assert len(restored.decisions) == len(original.decisions)
        for orig_d, rest_d in zip(original.decisions, restored.decisions):
            assert rest_d.date == orig_d.date
            assert rest_d.choice == orig_d.choice
            assert rest_d.alternatives == orig_d.alternatives

        # Log
        assert len(restored.log) == len(original.log)
        for orig_l, rest_l in zip(original.log, restored.log):
            assert rest_l.date == orig_l.date
            assert rest_l.lines == orig_l.lines

    def test_double_round_trip_stable(self):
        """Two round trips produce identical markdown."""
        original = self._make_full_project()
        md1 = original.to_markdown()
        restored1 = Project.from_markdown(md1)
        md2 = restored1.to_markdown()
        assert md1 == md2

    def test_minimal_project(self):
        """A Project with only name (all defaults) round-trips cleanly."""
        original = Project(name="Quick Task", created=date(2026, 3, 16))
        md = original.to_markdown()
        restored = Project.from_markdown(md)

        assert restored.name == "Quick Task"
        assert restored.slug == "quick-task"
        assert restored.status == "active"
        assert restored.priority == "medium"
        assert restored.tags == []
        assert restored.target is None
        assert restored.current_focus == ""
        assert restored.blockers == []
        assert restored.decisions == []
        assert restored.log == []

    def test_project_no_target(self):
        """Project without target date does not include target in output."""
        p = Project(name="No Target", created=date(2026, 3, 16))
        md = p.to_markdown()
        assert "target" not in md
        restored = Project.from_markdown(md)
        assert restored.target is None

    def test_project_no_blockers(self):
        """Project with no blockers section still round-trips."""
        p = Project(
            name="Clean Project",
            created=date(2026, 3, 16),
            decisions=[Decision(date=date(2026, 3, 16), choice="Go with plan A")],
        )
        md = p.to_markdown()
        assert "## Blockers" not in md
        restored = Project.from_markdown(md)
        assert restored.blockers == []
        assert len(restored.decisions) == 1


class TestBlockerParsing:
    """Test detailed blocker parsing edge cases."""

    def test_unresolved_blocker_with_since_and_mention(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            blockers=[
                Blocker(
                    description="need API docs",
                    resolved=False,
                    since=date(2026, 3, 10),
                    person="@dave",
                ),
            ],
        )
        md = p.to_markdown()
        assert "- [ ]" in md
        assert "@dave" in md
        assert "(since 2026-03-10)" in md

        restored = Project.from_markdown(md)
        b = restored.blockers[0]
        assert b.description == "need API docs"
        assert b.resolved is False
        assert b.since == date(2026, 3, 10)
        assert b.person == "@dave"

    def test_resolved_blocker_with_date(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            blockers=[
                Blocker(
                    description="hardware not available",
                    resolved=True,
                    resolved_date=date(2026, 3, 14),
                ),
            ],
        )
        md = p.to_markdown()
        assert "- [x]" in md
        assert "~~" in md
        assert "(resolved 2026-03-14)" in md

        restored = Project.from_markdown(md)
        b = restored.blockers[0]
        assert b.description == "hardware not available"
        assert b.resolved is True
        assert b.resolved_date == date(2026, 3, 14)

    def test_resolved_blocker_with_person(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            blockers=[
                Blocker(
                    description="waiting on spec",
                    resolved=True,
                    resolved_date=date(2026, 3, 15),
                    person="@carol",
                ),
            ],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        b = restored.blockers[0]
        assert b.description == "waiting on spec"
        assert b.resolved is True
        assert b.resolved_date == date(2026, 3, 15)
        assert b.person == "@carol"

    def test_blocker_no_date_no_person(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            blockers=[
                Blocker(description="simple blocker"),
            ],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        b = restored.blockers[0]
        assert b.description == "simple blocker"
        assert b.resolved is False
        assert b.since is None
        assert b.resolved_date is None
        assert b.person is None


class TestDecisionParsing:
    """Test decision parsing details."""

    def test_decision_with_alternatives(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            decisions=[
                Decision(
                    date=date(2026, 2, 1),
                    choice="Use React",
                    alternatives=["Vue", "Angular"],
                ),
            ],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        d = restored.decisions[0]
        assert d.date == date(2026, 2, 1)
        assert d.choice == "Use React"
        assert d.alternatives == ["Vue", "Angular"]

    def test_decision_without_alternatives(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            decisions=[
                Decision(date=date(2026, 3, 1), choice="Keep current approach"),
            ],
        )
        md = p.to_markdown()
        assert "Alternatives" not in md
        restored = Project.from_markdown(md)
        d = restored.decisions[0]
        assert d.choice == "Keep current approach"
        assert d.alternatives == []

    def test_multiple_decisions(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            decisions=[
                Decision(date=date(2026, 1, 1), choice="First choice"),
                Decision(
                    date=date(2026, 2, 1),
                    choice="Second choice",
                    alternatives=["Alt A"],
                ),
                Decision(date=date(2026, 3, 1), choice="Third choice"),
            ],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        assert len(restored.decisions) == 3
        assert restored.decisions[0].choice == "First choice"
        assert restored.decisions[1].alternatives == ["Alt A"]
        assert restored.decisions[2].choice == "Third choice"


class TestLogParsing:
    """Test log entry parsing."""

    def test_multiple_log_entries(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            log=[
                LogEntry(
                    date=date(2026, 3, 14),
                    lines=["Did thing one", "Did thing two"],
                ),
                LogEntry(
                    date=date(2026, 3, 15),
                    lines=["Did thing three"],
                ),
            ],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        assert len(restored.log) == 2
        assert restored.log[0].date == date(2026, 3, 14)
        assert restored.log[0].lines == ["Did thing one", "Did thing two"]
        assert restored.log[1].date == date(2026, 3, 15)
        assert restored.log[1].lines == ["Did thing three"]

    def test_empty_log_entry(self):
        p = Project(
            name="Test",
            created=date(2026, 3, 16),
            log=[LogEntry(date=date(2026, 3, 16), lines=[])],
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        assert len(restored.log) == 1
        assert restored.log[0].lines == []


# ── Journal round-trip tests ─────────────────────────────────────────


class TestJournalRoundTrip:
    """Test DailyJournal serialization round-trips."""

    def _make_full_journal(self) -> DailyJournal:
        return DailyJournal(
            date=date(2026, 3, 16),
            entries=[
                JournalEntry(
                    time="09:15",
                    entry_type="Started",
                    project="HMI Framework",
                    details={"focus": "debugging render loop"},
                ),
                JournalEntry(
                    time="11:30",
                    entry_type="Switched",
                    project="HMI Framework \u2192 Test Infra",
                    details={
                        "left_off": "checking vsync timing",
                        "blocker": "waiting on @carol for display spec",
                        "next_step": "read compositor docs",
                    },
                ),
                JournalEntry(
                    time="14:00",
                    entry_type="Note",
                    project="Test Infra",
                    details={"decision": "keeping pytest over unittest"},
                ),
                JournalEntry(
                    time="16:30",
                    entry_type="Done",
                    project="",
                    details={
                        "active": "Test Infra, HMI Framework (parked on blocker)"
                    },
                ),
            ],
        )

    def test_full_round_trip(self):
        original = self._make_full_journal()
        md = original.to_markdown()
        restored = DailyJournal.from_markdown(md)

        assert restored.date == original.date
        assert len(restored.entries) == len(original.entries)

        for orig_e, rest_e in zip(original.entries, restored.entries):
            assert rest_e.time == orig_e.time
            assert rest_e.entry_type == orig_e.entry_type
            assert rest_e.project == orig_e.project
            assert rest_e.details == orig_e.details

    def test_double_round_trip_stable(self):
        original = self._make_full_journal()
        md1 = original.to_markdown()
        restored = DailyJournal.from_markdown(md1)
        md2 = restored.to_markdown()
        assert md1 == md2

    def test_empty_journal(self):
        j = DailyJournal(date=date(2026, 3, 16), entries=[])
        md = j.to_markdown()
        restored = DailyJournal.from_markdown(md)
        assert restored.date == date(2026, 3, 16)
        assert restored.entries == []

    def test_append_entry(self):
        j = DailyJournal(date=date(2026, 3, 16))
        assert len(j.entries) == 0

        e1 = JournalEntry(time="10:00", entry_type="Started", project="Foo")
        j.append_entry(e1)
        assert len(j.entries) == 1
        assert j.entries[0] is e1

        e2 = JournalEntry(time="12:00", entry_type="Note", project="Foo", details={"decision": "yes"})
        j.append_entry(e2)
        assert len(j.entries) == 2

        # Verify round-trip after appending
        md = j.to_markdown()
        restored = DailyJournal.from_markdown(md)
        assert len(restored.entries) == 2
        assert restored.entries[0].project == "Foo"
        assert restored.entries[1].details == {"decision": "yes"}

    def test_done_entry(self):
        j = DailyJournal(
            date=date(2026, 3, 16),
            entries=[
                JournalEntry(
                    time="17:00",
                    entry_type="Done",
                    project="",
                    details={"active": "Project A, Project B"},
                ),
            ],
        )
        md = j.to_markdown()
        assert "Done for day" in md
        restored = DailyJournal.from_markdown(md)
        assert restored.entries[0].entry_type == "Done"
        assert restored.entries[0].project == ""
        assert restored.entries[0].details["active"] == "Project A, Project B"

    def test_started_entry_with_focus(self):
        j = DailyJournal(
            date=date(2026, 3, 16),
            entries=[
                JournalEntry(
                    time="09:00",
                    entry_type="Started",
                    project="My Project",
                    details={"focus": "writing tests"},
                ),
            ],
        )
        md = j.to_markdown()
        restored = DailyJournal.from_markdown(md)
        e = restored.entries[0]
        assert e.time == "09:00"
        assert e.entry_type == "Started"
        assert e.project == "My Project"
        assert e.details == {"focus": "writing tests"}


# ── People round-trip tests ──────────────────────────────────────────


class TestPeopleRoundTrip:
    """Test PeopleFile serialization round-trips."""

    def _make_full_people(self) -> PeopleFile:
        return PeopleFile(
            people=[
                Person(
                    handle="@carol",
                    role="Display Systems Lead",
                    projects=["HMI Framework"],
                    pending=[
                        PendingItem(
                            description="spec clarification",
                            since=date(2026, 3, 14),
                        ),
                    ],
                ),
                Person(
                    handle="@bob",
                    role="Test Infra reviewer",
                    projects=["Test Infra"],
                    pending=[
                        PendingItem(description="PR re-review"),
                    ],
                ),
            ]
        )

    def test_full_round_trip(self):
        original = self._make_full_people()
        md = original.to_markdown()
        restored = PeopleFile.from_markdown(md)

        assert len(restored.people) == len(original.people)
        for orig_p, rest_p in zip(original.people, restored.people):
            assert rest_p.handle == orig_p.handle
            assert rest_p.role == orig_p.role
            assert rest_p.projects == orig_p.projects
            assert len(rest_p.pending) == len(orig_p.pending)
            for orig_i, rest_i in zip(orig_p.pending, rest_p.pending):
                assert rest_i.description == orig_i.description
                assert rest_i.since == orig_i.since
                assert rest_i.project == orig_i.project

    def test_double_round_trip_stable(self):
        original = self._make_full_people()
        md1 = original.to_markdown()
        restored = PeopleFile.from_markdown(md1)
        md2 = restored.to_markdown()
        assert md1 == md2

    def test_empty_people_file(self):
        pf = PeopleFile(people=[])
        md = pf.to_markdown()
        restored = PeopleFile.from_markdown(md)
        assert restored.people == []

    def test_person_no_pending(self):
        pf = PeopleFile(
            people=[
                Person(
                    handle="@alice",
                    role="Manager",
                    projects=["Project X", "Project Y"],
                ),
            ]
        )
        md = pf.to_markdown()
        assert "Pending" not in md
        restored = PeopleFile.from_markdown(md)
        assert restored.people[0].handle == "@alice"
        assert restored.people[0].projects == ["Project X", "Project Y"]
        assert restored.people[0].pending == []

    def test_person_no_role_no_projects(self):
        pf = PeopleFile(
            people=[
                Person(
                    handle="@eve",
                    pending=[PendingItem(description="feedback")],
                ),
            ]
        )
        md = pf.to_markdown()
        restored = PeopleFile.from_markdown(md)
        assert restored.people[0].handle == "@eve"
        assert restored.people[0].role == ""
        assert restored.people[0].projects == []
        assert restored.people[0].pending[0].description == "feedback"

    def test_pending_with_date_and_project(self):
        pf = PeopleFile(
            people=[
                Person(
                    handle="@frank",
                    pending=[
                        PendingItem(
                            description="design review",
                            since=date(2026, 3, 10),
                            project="HMI Framework",
                        ),
                    ],
                ),
            ]
        )
        md = pf.to_markdown()
        assert "(asked 2026-03-10)" in md
        assert "[HMI Framework]" in md
        restored = PeopleFile.from_markdown(md)
        item = restored.people[0].pending[0]
        assert item.description == "design review"
        assert item.since == date(2026, 3, 10)
        assert item.project == "HMI Framework"

    def test_multiple_pending_items(self):
        pf = PeopleFile(
            people=[
                Person(
                    handle="@carol",
                    pending=[
                        PendingItem(description="item one", since=date(2026, 3, 1)),
                        PendingItem(description="item two"),
                    ],
                ),
            ]
        )
        md = pf.to_markdown()
        restored = PeopleFile.from_markdown(md)
        assert len(restored.people[0].pending) == 2
        assert restored.people[0].pending[0].description == "item one"
        assert restored.people[0].pending[0].since == date(2026, 3, 1)
        assert restored.people[0].pending[1].description == "item two"


# ── Edge case tests ──────────────────────────────────────────────────


class TestEdgeCases:
    """Test edge cases: whitespace, empty bodies, etc."""

    def test_project_empty_body(self):
        md = "---\nname: Empty\nstatus: active\npriority: low\ntags: []\ncreated: '2026-03-16'\n---\n"
        p = Project.from_markdown(md)
        assert p.name == "Empty"
        assert p.blockers == []
        assert p.decisions == []
        assert p.log == []
        assert p.current_focus == ""

    def test_project_extra_whitespace_in_body(self):
        p = Project(
            name="Whitespace Test",
            created=date(2026, 3, 16),
            current_focus="  some focus  ",
        )
        md = p.to_markdown()
        restored = Project.from_markdown(md)
        # Focus text gets stripped during round-trip
        assert restored.current_focus == "some focus"

    def test_journal_entry_no_details(self):
        j = DailyJournal(
            date=date(2026, 3, 16),
            entries=[
                JournalEntry(time="10:00", entry_type="Note", project="Proj"),
            ],
        )
        md = j.to_markdown()
        restored = DailyJournal.from_markdown(md)
        assert restored.entries[0].details == {}

    def test_project_slug_auto_generated(self):
        p = Project(name="My Cool Project")
        assert p.slug == "my-cool-project"

    def test_project_slug_preserved_if_set(self):
        p = Project(name="My Cool Project", slug="custom-slug")
        assert p.slug == "custom-slug"

    def test_blocker_re_export(self):
        """The blocker module re-exports the same class."""
        from jm.models.blocker import Blocker as BlockerReexport
        assert BlockerReexport is Blocker

    def test_imports_from_init(self):
        """All expected classes are importable from jm.models."""
        from jm.models import (
            Blocker,
            DailyJournal,
            Decision,
            JournalEntry,
            LogEntry,
            PendingItem,
            PeopleFile,
            Person,
            Project,
        )
        # Just verify they exist
        assert all([
            Blocker, DailyJournal, Decision, JournalEntry,
            LogEntry, PendingItem, PeopleFile, Person, Project,
        ])
