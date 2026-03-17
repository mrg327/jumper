//! Property-based round-trip tests for jm-core models.
//! Uses `proptest` to generate arbitrary inputs and verify serialization
//! stability across to_markdown → from_markdown cycles.

use jm_core::models::{
    Blocker, DailyJournal, Decision, JournalEntry, LogEntry, PendingItem, PeopleFile, Person,
    Priority, Project, Status,
};
use proptest::prelude::*;

// ── Strategy helpers ─────────────────────────────────────────────────

/// Generate a valid name: printable ASCII only, non-empty, no leading/trailing
/// whitespace, avoids characters that are YAML special at the start of a string.
/// The name must end with an alphanumeric character to avoid YAML trailing-space
/// stripping on unquoted scalars.
fn arb_name() -> impl Strategy<Value = String> {
    // Generates names that start with a letter and are followed by 0-48
    // alphanumeric/hyphen/underscore chars (no trailing spaces).
    // This ensures the YAML unquoted scalar round-trip is stable.
    "[a-zA-Z][a-zA-Z0-9_-]{0,48}"
        .prop_filter("must be non-empty", |s| !s.is_empty())
}

/// Generate a text fragment that may contain YAML special characters.
fn arb_text_with_specials() -> impl Strategy<Value = String> {
    prop_oneof![
        // plain ASCII phrases
        "[a-zA-Z0-9 .,!?;]{1,60}",
        // phrases with YAML special chars in the middle (not at the start)
        "[a-zA-Z][a-zA-Z0-9 ]*[:#{}\\[\\]\"]{1,3}[a-zA-Z0-9 ]*",
        // phrases with single quotes
        "[a-zA-Z][a-zA-Z0-9 ]*'[a-zA-Z0-9 ]*",
        // handle that looks like a mention in text
        "[a-zA-Z]+ @[a-zA-Z0-9_-]+",
    ]
}

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![
        Just(Status::Active),
        Just(Status::Blocked),
        Just(Status::Pending),
        Just(Status::Parked),
        Just(Status::Done),
    ]
}

fn arb_priority() -> impl Strategy<Value = Priority> {
    prop_oneof![
        Just(Priority::High),
        Just(Priority::Medium),
        Just(Priority::Low),
    ]
}

fn arb_naive_date() -> impl Strategy<Value = chrono::NaiveDate> {
    // Dates between 2000-01-01 and 2099-12-31
    (2000_i32..=2099_i32, 1_u32..=12_u32, 1_u32..=28_u32)
        .prop_map(|(y, m, d)| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap())
}

fn arb_tag() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,15}".prop_filter("non-empty", |s| !s.is_empty())
}

fn arb_tags() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(arb_tag(), 0..=5)
        .prop_map(|mut v| {
            v.dedup();
            v
        })
}

// ── Project property tests ────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_project_roundtrip_basic(
        name in arb_name(),
        status in arb_status(),
        priority in arb_priority(),
    ) {
        let mut p = Project::new(&name);
        p.status = status;
        p.priority = priority;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(&p2.name, &p.name);
        prop_assert_eq!(p2.status, p.status);
        prop_assert_eq!(p2.priority, p.priority);
    }

    #[test]
    fn prop_project_roundtrip_with_tags(
        name in arb_name(),
        tags in arb_tags(),
    ) {
        let mut p = Project::new(&name);
        p.tags = tags.clone();
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(&p2.name, &p.name);
        prop_assert_eq!(p2.tags, p.tags);
    }

    #[test]
    fn prop_project_roundtrip_with_focus(
        name in arb_name(),
        focus in "[a-zA-Z0-9 .,!?]{1,80}",
    ) {
        let mut p = Project::new(&name);
        p.current_focus = focus.clone();
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.current_focus.trim(), focus.trim());
    }

    #[test]
    fn prop_project_double_roundtrip_stable(
        name in arb_name(),
        status in arb_status(),
        priority in arb_priority(),
        tags in arb_tags(),
    ) {
        let mut p = Project::new(&name);
        p.status = status;
        p.priority = priority;
        p.tags = tags;
        let md1 = p.to_markdown();
        let p2 = Project::from_markdown(&md1).expect("first from_markdown failed");
        let md2 = p2.to_markdown();
        prop_assert_eq!(md1, md2, "double round-trip not stable");
    }

    #[test]
    fn prop_project_with_log_entries(
        name in arb_name(),
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
        // Lines must start and end with a non-space char to avoid trim-induced
        // differences on round-trip (the parser strips the leading "- " prefix
        // and trims the line).
        lines in prop::collection::vec(
            "[a-zA-Z][a-zA-Z0-9 .,!?]*[a-zA-Z0-9]|[a-zA-Z]",
            0..=5
        ),
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let mut p = Project::new(&name);
        p.log.push(LogEntry { date, lines: lines.clone() });
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.log.len(), 1);
        prop_assert_eq!(p2.log[0].date, date);
        prop_assert_eq!(&p2.log[0].lines, &lines);
    }

    #[test]
    fn prop_project_roundtrip_with_decisions(
        name in arb_name(),
        choice in "[a-zA-Z][a-zA-Z0-9 .,!?]{1,40}",
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let mut p = Project::new(&name);
        p.decisions.push(Decision {
            date,
            choice: choice.trim().to_string(),
            alternatives: Vec::new(),
        });
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.decisions.len(), 1);
        prop_assert_eq!(&p2.decisions[0].choice, choice.trim());
        prop_assert_eq!(p2.decisions[0].date, date);
    }

    #[test]
    fn prop_project_roundtrip_with_simple_blocker(
        name in arb_name(),
        desc in "[a-zA-Z][a-zA-Z0-9 .,!?]{1,40}",
        resolved in any::<bool>(),
    ) {
        let mut p = Project::new(&name);
        let blocker = Blocker {
            description: desc.trim().to_string(),
            resolved,
            since: None,
            resolved_date: None,
            person: None,
        };
        p.blockers.push(blocker);
        // Force consistent status to avoid auto-adjustment confusion
        if !resolved {
            p.status = Status::Blocked;
        }
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.blockers.len(), 1);
        prop_assert_eq!(&p2.blockers[0].description, &p.blockers[0].description);
        prop_assert_eq!(p2.blockers[0].resolved, resolved);
    }

    #[test]
    fn prop_project_status_survives_roundtrip(status in arb_status()) {
        let mut p = Project::new("Status Test");
        p.status = status;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.status, status);
    }

    #[test]
    fn prop_project_priority_survives_roundtrip(priority in arb_priority()) {
        let mut p = Project::new("Priority Test");
        p.priority = priority;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.priority, priority);
    }

    #[test]
    fn prop_project_target_date_survives_roundtrip(
        year in 2020_i32..=2099_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let mut p = Project::new("Target Test");
        p.target = Some(date);
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(p2.target, Some(date));
    }

    #[test]
    fn prop_project_name_with_yaml_special_chars(
        // Names starting with alnum, containing special YAML chars
        name in "[a-zA-Z][a-zA-Z0-9 ]*[':!?#{}\\[\\]@]{1,2}[a-zA-Z0-9 ]*",
    ) {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut p = Project::new(&trimmed);
        p.status = Status::Active;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).expect("from_markdown with special chars failed");
        prop_assert_eq!(&p2.name, &trimmed);
    }
}

// ── Journal property tests ────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    #[test]
    fn prop_journal_roundtrip_basic(
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let j = DailyJournal::new(date);
        let md = j.to_markdown();
        let j2 = DailyJournal::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(j2.date, date);
        prop_assert!(j2.entries.is_empty());
    }

    #[test]
    fn prop_journal_roundtrip_with_entry(
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
        hour in 0_u32..=23_u32,
        minute in 0_u32..=59_u32,
        project_name in "[a-zA-Z][a-zA-Z0-9 ]{1,20}",
        entry_type in prop_oneof![
            Just("Started"),
            Just("Note"),
            Just("Switched"),
        ],
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let time = format!("{:02}:{:02}", hour, minute);
        let mut j = DailyJournal::new(date);
        j.append_entry(JournalEntry::new(&time, entry_type, &project_name.trim().to_string()));
        let md = j.to_markdown();
        let j2 = DailyJournal::from_markdown(&md).expect("from_markdown failed");
        prop_assert_eq!(j2.date, date);
        prop_assert_eq!(j2.entries.len(), 1);
        prop_assert_eq!(&j2.entries[0].time, &time);
        prop_assert_eq!(&j2.entries[0].entry_type, entry_type);
    }

    #[test]
    fn prop_journal_double_roundtrip_stable(
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
        hour in 0_u32..=23_u32,
        minute in 0_u32..=59_u32,
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let time = format!("{:02}:{:02}", hour, minute);
        let mut j = DailyJournal::new(date);
        j.append_entry(JournalEntry::new(&time, "Started", "Project Alpha"));
        let md1 = j.to_markdown();
        let j2 = DailyJournal::from_markdown(&md1).expect("first from_markdown failed");
        let md2 = j2.to_markdown();
        prop_assert_eq!(md1, md2, "double round-trip not stable");
    }
}

// ── Person / PeopleFile property tests ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    #[test]
    fn prop_person_roundtrip_basic(
        handle_suffix in "[a-z][a-z0-9_-]{1,20}",
        role in "[a-zA-Z][a-zA-Z0-9 .,]{0,40}",
    ) {
        let handle = format!("@{handle_suffix}");
        let mut p = Person::new(&handle);
        p.role = role.trim().to_string();
        let pf = PeopleFile { people: vec![p.clone()] };
        let md = pf.to_markdown();
        let pf2 = PeopleFile::from_markdown(&md);
        prop_assert_eq!(pf2.people.len(), 1);
        prop_assert_eq!(&pf2.people[0].handle, &handle);
        prop_assert_eq!(&pf2.people[0].role, p.role.trim());
    }

    #[test]
    fn prop_people_double_roundtrip_stable(
        handle_suffix in "[a-z][a-z0-9_-]{1,15}",
        role in "[a-zA-Z][a-zA-Z0-9 .,]{0,30}",
    ) {
        let handle = format!("@{handle_suffix}");
        let mut person = Person::new(&handle);
        person.role = role.trim().to_string();
        let pf = PeopleFile { people: vec![person] };
        let md1 = pf.to_markdown();
        let pf2 = PeopleFile::from_markdown(&md1);
        let md2 = pf2.to_markdown();
        prop_assert_eq!(md1, md2, "double round-trip not stable");
    }

    #[test]
    fn prop_person_with_pending_item(
        handle_suffix in "[a-z][a-z0-9]{1,15}",
        desc in "[a-zA-Z][a-zA-Z0-9 .,!?]{1,40}",
    ) {
        let handle = format!("@{handle_suffix}");
        let mut person = Person::new(&handle);
        person.pending.push(PendingItem {
            description: desc.trim().to_string(),
            since: None,
            project: None,
        });
        let pf = PeopleFile { people: vec![person] };
        let md = pf.to_markdown();
        let pf2 = PeopleFile::from_markdown(&md);
        prop_assert_eq!(pf2.people.len(), 1);
        prop_assert_eq!(pf2.people[0].pending.len(), 1);
        prop_assert_eq!(&pf2.people[0].pending[0].description, desc.trim());
    }

    #[test]
    fn prop_person_with_date_survives_roundtrip(
        handle_suffix in "[a-z][a-z0-9]{1,10}",
        desc in "[a-zA-Z][a-zA-Z0-9 .,!?]{1,30}",
        year in 2020_i32..=2026_i32,
        month in 1_u32..=12_u32,
        day in 1_u32..=28_u32,
    ) {
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let handle = format!("@{handle_suffix}");
        let mut person = Person::new(&handle);
        person.pending.push(PendingItem {
            description: desc.trim().to_string(),
            since: Some(date),
            project: None,
        });
        let pf = PeopleFile { people: vec![person] };
        let md = pf.to_markdown();
        let pf2 = PeopleFile::from_markdown(&md);
        prop_assert_eq!(pf2.people[0].pending[0].since, Some(date));
    }
}

// ── Year-boundary edge cases ──────────────────────────────────────────

#[test]
fn test_project_roundtrip_year_boundary_2000() {
    let date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let mut p = Project::new("Year 2000 Project");
    p.created = date;
    p.target = Some(chrono::NaiveDate::from_ymd_opt(2000, 12, 31).unwrap());
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.created, date);
    assert_eq!(p2.target, p.target);
}

#[test]
fn test_project_roundtrip_year_boundary_2099() {
    let date = chrono::NaiveDate::from_ymd_opt(2099, 12, 31).unwrap();
    let mut p = Project::new("Far Future Project");
    p.created = date;
    p.target = Some(date);
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.created, date);
}

#[test]
fn test_project_roundtrip_all_status_values() {
    for status in [
        Status::Active,
        Status::Blocked,
        Status::Pending,
        Status::Parked,
        Status::Done,
    ] {
        let mut p = Project::new("Status Test");
        p.status = status;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).unwrap();
        assert_eq!(p2.status, status, "status {status} did not survive round-trip");
    }
}

#[test]
fn test_project_roundtrip_all_priority_values() {
    for priority in [Priority::High, Priority::Medium, Priority::Low] {
        let mut p = Project::new("Priority Test");
        p.priority = priority;
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).unwrap();
        assert_eq!(p2.priority, priority, "priority {priority} did not survive round-trip");
    }
}

#[test]
fn test_project_roundtrip_unicode_in_focus() {
    let mut p = Project::new("Unicode Project");
    p.current_focus = "Debugging 日本語 chars and café résumé".to_string();
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.current_focus.trim(), p.current_focus.trim());
}

#[test]
fn test_project_roundtrip_unicode_in_name() {
    // Names with unicode characters that need YAML quoting
    for name in ["Café Project", "Proiect România", "プロジェクト名"] {
        let p = Project::new(name);
        let md = p.to_markdown();
        let p2 = Project::from_markdown(&md).unwrap();
        assert_eq!(&p2.name, name, "unicode name '{name}' did not survive round-trip");
    }
}

#[test]
fn test_project_roundtrip_name_with_colon() {
    let p = Project::new("Project: Phase 2");
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.name, "Project: Phase 2");
}

#[test]
fn test_project_roundtrip_name_with_hash() {
    let p = Project::new("Project #1 Fix");
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.name, "Project #1 Fix");
}

#[test]
fn test_project_roundtrip_name_with_brackets() {
    let p = Project::new("Project [MVP] release");
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.name, "Project [MVP] release");
}

#[test]
fn test_project_roundtrip_long_name() {
    let name = "A".repeat(200);
    let p = Project::new(&name);
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.name, name);
}

#[test]
fn test_project_blockers_with_at_mention_survives_roundtrip() {
    let mut p = Project::new("Mention Test");
    p.blockers.push(Blocker {
        description: "waiting on API from vendor".to_string(),
        resolved: false,
        since: Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()),
        resolved_date: None,
        person: Some("@alice-vendor".to_string()),
    });
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    let b = &p2.blockers[0];
    assert_eq!(b.description, "waiting on API from vendor");
    assert_eq!(b.person, Some("@alice-vendor".to_string()));
}

#[test]
fn test_project_log_entry_with_markdown_formatting() {
    let mut p = Project::new("Markdown Log Test");
    p.log.push(LogEntry {
        date: chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        lines: vec![
            "Fixed issue with **bold** text in output".to_string(),
            "Reviewed PR #42 and approved".to_string(),
        ],
    });
    let md = p.to_markdown();
    let p2 = Project::from_markdown(&md).unwrap();
    assert_eq!(p2.log[0].lines[0], "Fixed issue with **bold** text in output");
    assert_eq!(p2.log[0].lines[1], "Reviewed PR #42 and approved");
}
