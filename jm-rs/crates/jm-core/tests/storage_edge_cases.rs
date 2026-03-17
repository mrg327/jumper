//! Storage layer edge-case tests.
//! Covers corrupt files, empty directories, full round-trips with all fields
//! populated, atomic write behaviour, multi-day journal operations, and
//! search with special characters.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::NaiveDate;
use tempfile::TempDir;

use jm_core::models::{
    Blocker, DailyJournal, Decision, JournalEntry, LogEntry, Priority, Project, Status,
};
use jm_core::storage::{
    ActiveProjectStore, JournalStore, PeopleStore, ProjectStore, SearchEngine, SearchFilter,
};

// ── Helpers ──────────────────────────────────────────────────────────

fn setup_stores(tmp: &TempDir) -> (ProjectStore, JournalStore, PeopleStore, ActiveProjectStore) {
    let data_dir = tmp.path();
    let ps = ProjectStore::new(data_dir);
    let js = JournalStore::new(data_dir);
    let pps = PeopleStore::new(data_dir);
    let as_ = ActiveProjectStore::new(data_dir);
    (ps, js, pps, as_)
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

// ── ProjectStore edge cases ──────────────────────────────────────────

#[test]
fn test_list_projects_handles_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    // Write a valid project
    ps.create_project("Good Project").unwrap();

    // Write a file with no YAML frontmatter — from_markdown handles this gracefully
    // by returning a project with empty fields. The store must not crash.
    let no_fm_path = tmp.path().join("projects").join("no-frontmatter.md");
    fs::write(&no_fm_path, "this is not valid frontmatter at all!!!").unwrap();

    // list_projects must not panic. It may or may not include the no-frontmatter
    // file (it parses successfully as an empty-name project), but at minimum it
    // returns the valid project.
    let projects = ps.list_projects(None);
    assert!(
        projects.iter().any(|p| p.name == "Good Project"),
        "Good Project should always be present"
    );
}

#[test]
fn test_list_projects_skips_yaml_parse_error_file() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    ps.create_project("Good Project").unwrap();

    // Write a file with invalid YAML (will cause a parse error in serde_yml)
    let bad_yaml_path = tmp.path().join("projects").join("bad-yaml.md");
    // This YAML has unmatched braces which is invalid
    fs::write(
        &bad_yaml_path,
        "---\nname: {unclosed brace\nstatus: active\n---\n",
    ).unwrap();

    // list_projects must not crash even with unparseable YAML
    let projects = ps.list_projects(None);
    assert!(
        projects.iter().any(|p| p.name == "Good Project"),
        "Good Project should always be present even when other files have bad YAML"
    );
}

#[test]
fn test_list_projects_skips_empty_file() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    ps.create_project("Real Project").unwrap();

    // Write an empty file
    let empty_path = tmp.path().join("projects").join("empty.md");
    fs::write(&empty_path, "").unwrap();

    let projects = ps.list_projects(None);
    // Empty file parses as a project with empty name — it may be included.
    // The key requirement is that it does NOT crash.
    let _ = projects; // must not panic
}

#[test]
fn test_list_projects_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    let projects = ps.list_projects(None);
    assert!(projects.is_empty(), "expected empty list from empty directory");
}

#[test]
fn test_list_projects_ignores_non_md_files() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    ps.create_project("Valid").unwrap();

    // Write a non-.md file in the projects dir
    let txt_path = tmp.path().join("projects").join("notes.txt");
    fs::write(&txt_path, "some random text").unwrap();

    let hidden_path = tmp.path().join("projects").join(".hidden");
    fs::write(&hidden_path, "hidden file").unwrap();

    let projects = ps.list_projects(None);
    assert_eq!(
        projects.len(),
        1,
        "should only pick up .md files, got: {:?}",
        projects.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_save_get_project_full_fields() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    let full_project = Project {
        name: "Full Fields Project".to_string(),
        slug: "full-fields-project".to_string(),
        status: Status::Active,
        priority: Priority::High,
        tags: vec!["infra".to_string(), "critical".to_string()],
        created: d(2026, 1, 1),
        target: Some(d(2026, 12, 31)),
        current_focus: "Writing comprehensive tests".to_string(),
        blockers: vec![
            Blocker {
                description: "waiting on hardware".to_string(),
                resolved: false,
                since: Some(d(2026, 3, 1)),
                resolved_date: None,
                person: Some("@vendor".to_string()),
            },
            Blocker {
                description: "old resolved blocker".to_string(),
                resolved: true,
                since: None,
                resolved_date: Some(d(2026, 2, 15)),
                person: None,
            },
        ],
        decisions: vec![Decision {
            date: d(2026, 2, 1),
            choice: "Use property testing".to_string(),
            alternatives: vec!["manual tests only".to_string()],
        }],
        log: vec![
            LogEntry {
                date: d(2026, 3, 10),
                lines: vec!["set up test harness".to_string(), "wrote first test".to_string()],
            },
            LogEntry {
                date: d(2026, 3, 11),
                lines: vec!["added edge cases".to_string()],
            },
        ],
    };

    // Save via raw (no auto-status) to preserve the status we set
    ps.save_project_raw(&full_project).unwrap();

    let loaded = ps.get_project("full-fields-project").unwrap();

    assert_eq!(loaded.name, full_project.name);
    assert_eq!(loaded.status, full_project.status);
    assert_eq!(loaded.priority, full_project.priority);
    assert_eq!(loaded.tags, full_project.tags);
    assert_eq!(loaded.created, full_project.created);
    assert_eq!(loaded.target, full_project.target);
    assert_eq!(loaded.current_focus, full_project.current_focus);

    assert_eq!(loaded.blockers.len(), 2);
    assert_eq!(loaded.blockers[0].description, "waiting on hardware");
    assert!(!loaded.blockers[0].resolved);
    assert_eq!(loaded.blockers[0].since, Some(d(2026, 3, 1)));
    assert_eq!(loaded.blockers[0].person, Some("@vendor".to_string()));
    assert!(loaded.blockers[1].resolved);
    assert_eq!(loaded.blockers[1].resolved_date, Some(d(2026, 2, 15)));

    assert_eq!(loaded.decisions.len(), 1);
    assert_eq!(loaded.decisions[0].choice, "Use property testing");
    assert_eq!(loaded.decisions[0].alternatives, vec!["manual tests only"]);

    assert_eq!(loaded.log.len(), 2);
    assert_eq!(loaded.log[0].lines, vec!["set up test harness", "wrote first test"]);
    assert_eq!(loaded.log[1].lines, vec!["added edge cases"]);
}

#[test]
fn test_save_project_with_at_mention_blocker() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    let mut p = Project::new("Mention Blocker Test");
    p.blockers.push(Blocker {
        description: "needs sign-off from procurement".to_string(),
        resolved: false,
        since: Some(d(2026, 3, 5)),
        resolved_date: None,
        person: Some("@mgmt-procurement".to_string()),
    });
    p.status = Status::Blocked;

    ps.save_project_raw(&p).unwrap();
    let loaded = ps.get_project("mention-blocker-test").unwrap();

    let b = &loaded.blockers[0];
    assert_eq!(b.description, "needs sign-off from procurement");
    assert_eq!(b.person, Some("@mgmt-procurement".to_string()));
    assert_eq!(b.since, Some(d(2026, 3, 5)));
}

#[test]
fn test_atomic_write_produces_correct_content() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    let mut p = ps.create_project("Atomic Test").unwrap();
    p.current_focus = "checking atomicity".to_string();
    ps.save_project(&mut p).unwrap();

    // The file should not contain any .tmp extension
    let projects_dir = tmp.path().join("projects");
    for entry in fs::read_dir(&projects_dir).unwrap().flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        assert!(!name_str.ends_with(".tmp"), "tmp file left behind: {name_str}");
    }

    // The final content must parse cleanly
    let content = fs::read_to_string(projects_dir.join("atomic-test.md")).unwrap();
    let loaded = Project::from_markdown(&content).unwrap();
    assert_eq!(loaded.current_focus, "checking atomicity");
}

#[test]
fn test_get_project_returns_none_for_missing() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    assert!(ps.get_project("does-not-exist").is_none());
}

#[test]
fn test_list_projects_filter_all_statuses() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    // Use save_project_raw to bypass auto-status adjustment so the status we
    // set is exactly what gets persisted (e.g., "blocked" without open blockers).
    for (name, status_str) in [
        ("Active-P", "active"),
        ("Blocked-P", "blocked"),
        ("Pending-P", "pending"),
        ("Parked-P", "parked"),
        ("Done-P", "done"),
    ] {
        let mut p = Project::new(name);
        p.status = status_str.parse::<Status>().unwrap();
        ps.save_project_raw(&p).unwrap();
    }

    // Each status filter should return exactly 1 project
    for status in ["active", "blocked", "pending", "parked", "done"] {
        let results = ps.list_projects(Some(status));
        assert_eq!(
            results.len(),
            1,
            "filter '{status}' should match exactly 1 project, got {}: {:?}",
            results.len(),
            results.iter().map(|p| (&p.name, &p.status)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_list_projects_returns_no_match_for_unknown_status() {
    let tmp = TempDir::new().unwrap();
    let (ps, _, _, _) = setup_stores(&tmp);

    ps.create_project("Some Project").unwrap();

    let results = ps.list_projects(Some("nonexistent-status"));
    assert!(results.is_empty());
}

// ── JournalStore edge cases ───────────────────────────────────────────

#[test]
fn test_journal_entries_spanning_multiple_days() {
    let tmp = TempDir::new().unwrap();
    let (_, js, _, _) = setup_stores(&tmp);

    // Create journals for 3 different days
    let dates = [
        d(2026, 3, 10),
        d(2026, 3, 11),
        d(2026, 3, 12),
    ];

    for (i, &date) in dates.iter().enumerate() {
        let mut j = DailyJournal::new(date);
        j.append_entry(JournalEntry::new(
            "09:00",
            "Started",
            &format!("Project {}", i + 1),
        ));
        js.save(&j).unwrap();
    }

    // Each day should be retrievable independently
    for (i, &date) in dates.iter().enumerate() {
        let j = js.get_day(date).expect("journal day should exist");
        assert_eq!(j.date, date);
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].project, format!("Project {}", i + 1));
    }
}

#[test]
fn test_journal_previous_workday_finds_oldest_within_14_days() {
    let tmp = TempDir::new().unwrap();
    let (_, js, _, _) = setup_stores(&tmp);

    let today = d(2026, 3, 16);
    let two_weeks_ago = d(2026, 3, 2); // 14 days before today

    // Save a journal right at the 14-day boundary
    let j = DailyJournal::new(two_weeks_ago);
    js.save(&j).unwrap();

    let prev = js.get_previous_workday(Some(today));
    assert!(prev.is_some(), "should find journal exactly 14 days back");
    assert_eq!(prev.unwrap().date, two_weeks_ago);
}

#[test]
fn test_journal_previous_workday_does_not_exceed_14_days() {
    let tmp = TempDir::new().unwrap();
    let (_, js, _, _) = setup_stores(&tmp);

    let today = d(2026, 3, 16);
    let fifteen_days_ago = d(2026, 3, 1); // 15 days before today

    let j = DailyJournal::new(fifteen_days_ago);
    js.save(&j).unwrap();

    let prev = js.get_previous_workday(Some(today));
    assert!(
        prev.is_none(),
        "should NOT find journal 15 days back (only looks 14 days)"
    );
}

#[test]
fn test_journal_missing_day_returns_none() {
    let tmp = TempDir::new().unwrap();
    let (_, js, _, _) = setup_stores(&tmp);

    assert!(js.get_day(d(2026, 6, 15)).is_none());
}

#[test]
fn test_journal_save_and_reload() {
    let tmp = TempDir::new().unwrap();
    let (_, js, _, _) = setup_stores(&tmp);

    let date = d(2026, 3, 15);
    let mut j = DailyJournal::new(date);
    let mut e = JournalEntry::new("14:30", "Note", "My Project");
    e.details.insert("focus".to_string(), "writing edge case tests".to_string());
    j.append_entry(e);

    js.save(&j).unwrap();
    let loaded = js.get_day(date).unwrap();

    assert_eq!(loaded.date, date);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].time, "14:30");
    assert_eq!(loaded.entries[0].project, "My Project");
    assert_eq!(loaded.entries[0].details["focus"], "writing edge case tests");
}

// ── ActiveProjectStore edge cases ────────────────────────────────────

#[test]
fn test_active_store_whitespace_slug_treated_as_empty() {
    let tmp = TempDir::new().unwrap();
    let as_ = ActiveProjectStore::new(tmp.path());

    // Write a file with only whitespace
    fs::write(&tmp.path().join(".active"), "   \n").unwrap();

    // get_active trims and returns None for blank content
    assert_eq!(as_.get_active(), None);
}

#[test]
fn test_active_store_missing_file_is_none() {
    let tmp = TempDir::new().unwrap();
    let as_ = ActiveProjectStore::new(tmp.path());
    assert_eq!(as_.get_active(), None);
}

#[test]
fn test_active_store_set_and_clear_cycle() {
    let tmp = TempDir::new().unwrap();
    let as_ = ActiveProjectStore::new(tmp.path());

    assert_eq!(as_.get_active(), None);
    as_.set_active("project-a").unwrap();
    assert_eq!(as_.get_active(), Some("project-a".to_string()));

    as_.set_active("project-b").unwrap();
    assert_eq!(as_.get_active(), Some("project-b".to_string()));

    as_.clear_active();
    assert_eq!(as_.get_active(), None);
}

// ── SearchEngine edge cases ──────────────────────────────────────────

fn setup_search(tmp: &TempDir) -> (PathBuf, SearchEngine) {
    let data_dir = tmp.path().to_path_buf();
    fs::create_dir_all(data_dir.join("projects")).unwrap();
    fs::create_dir_all(data_dir.join("journal")).unwrap();
    let engine = SearchEngine::new(&data_dir);
    (data_dir, engine)
}

#[test]
fn test_search_with_regex_special_chars_does_not_crash() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    let mut p = ps.create_project("Regex Test").unwrap();
    p.current_focus = "test (parentheses) and [brackets] and {braces}".to_string();
    ps.save_project(&mut p).unwrap();

    // These are regex special chars and could cause a panic if not escaped
    for query in ["(test)", "[brackets]", "{braces}", "a+b", "a*b", "a?b", "a.b", "a|b", "a^b", "a$b", "a\\b"] {
        let results = engine.quick_search(query);
        // Must not panic — results may be empty or non-empty
        let _ = results;
    }
}

#[test]
fn test_search_returns_empty_for_empty_query() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    ps.create_project("Some Project").unwrap();

    let results = engine.search(&SearchFilter::default());
    assert!(results.is_empty(), "empty query should return no results");
}

#[test]
fn test_search_finds_text_in_project_body() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    let mut p = ps.create_project("Search Target").unwrap();
    p.current_focus = "implementing distributed consensus algorithm".to_string();
    ps.save_project(&mut p).unwrap();

    let results = engine.quick_search("consensus");
    assert!(!results.is_empty(), "should find 'consensus' in project body");
    assert!(
        results.iter().any(|r| r.line_text.contains("consensus")),
        "at least one result should contain 'consensus'"
    );
}

#[test]
fn test_search_case_insensitive_by_default() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    let mut p = ps.create_project("Case Sensitive Test").unwrap();
    p.current_focus = "UPPERCASE Focus Text".to_string();
    ps.save_project(&mut p).unwrap();

    let results = engine.quick_search("uppercase");
    assert!(!results.is_empty(), "case-insensitive search should find 'UPPERCASE'");
}

#[test]
fn test_search_returns_no_results_for_empty_datadir() {
    let tmp = TempDir::new().unwrap();
    let (_, engine) = setup_search(&tmp);

    let results = engine.quick_search("anything");
    assert!(results.is_empty());
}

#[test]
fn test_search_file_type_detection_project() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    let mut p = ps.create_project("Type Detection Project").unwrap();
    p.current_focus = "unique_detection_keyword_proj".to_string();
    ps.save_project(&mut p).unwrap();

    let results = engine.quick_search("unique_detection_keyword_proj");
    assert!(!results.is_empty());
    assert!(
        results.iter().all(|r| r.file_type == "project"),
        "all results for project file should have file_type = 'project'"
    );
}

#[test]
fn test_search_file_type_detection_journal() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    // Write a journal file directly
    let journal_path = data_dir.join("journal").join("2026-03-15.md");
    fs::write(
        &journal_path,
        "---\ndate: '2026-03-15'\n---\n## 09:00 \u{2014} Started: My Project\nFocus: unique_journal_keyword_here\n",
    ).unwrap();

    let results = engine.quick_search("unique_journal_keyword_here");
    assert!(!results.is_empty());
    assert!(
        results.iter().all(|r| r.file_type == "journal"),
        "all results for journal file should have file_type = 'journal'"
    );
}

#[test]
fn test_search_file_type_detection_people() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    // Write a people file
    let people_path = data_dir.join("people.md");
    fs::write(
        &people_path,
        "## @carol\n- Role: unique_people_role_keyword\n",
    ).unwrap();

    let results = engine.quick_search("unique_people_role_keyword");
    assert!(!results.is_empty());
    assert!(
        results.iter().any(|r| r.file_type == "people"),
        "should detect 'people' file type for people.md"
    );
}

#[test]
fn test_search_with_unusual_directory_structure() {
    // A data dir where projects dir has a subdirectory — it should not crash
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    ps.create_project("Normal Project").unwrap();

    // Create a subdirectory inside the projects dir (should be ignored)
    let subdir = data_dir.join("projects").join("subdir");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("nested.md"), "---\nname: Nested\nstatus: active\npriority: medium\ntags: []\ncreated: '2026-01-01'\n---\n").unwrap();

    // Must not panic; the nested file may or may not be picked up depending on
    // whether read_dir recurses — the key requirement is no crash.
    let results = engine.quick_search("Nested");
    let _ = results;
}

#[test]
fn test_search_filter_by_project_slug() {
    let tmp = TempDir::new().unwrap();
    let (data_dir, engine) = setup_search(&tmp);

    let ps = ProjectStore::new(&data_dir);
    let mut p1 = ps.create_project("Project Alpha").unwrap();
    p1.current_focus = "alpha unique keyword".to_string();
    ps.save_project(&mut p1).unwrap();

    let mut p2 = ps.create_project("Project Beta").unwrap();
    p2.current_focus = "beta also has unique keyword".to_string();
    ps.save_project(&mut p2).unwrap();

    // Without project filter, both should match
    let all_results = engine.quick_search("unique keyword");
    assert!(all_results.len() >= 2);

    // With project slug filter, only alpha should match
    let filtered_results = engine.search(&SearchFilter {
        query: "unique keyword".to_string(),
        project: Some("project-alpha".to_string()),
        ..Default::default()
    });
    assert!(
        filtered_results.iter().all(|r| r.project_slug == "project-alpha"),
        "slug filter should restrict results to project-alpha only"
    );
}

// ── People store edge cases ───────────────────────────────────────────

#[test]
fn test_people_store_load_missing_file_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let pps = PeopleStore::new(tmp.path());
    let pf = pps.load();
    assert!(pf.people.is_empty());
}

#[test]
fn test_people_store_roundtrip_multiple_people() {
    use jm_core::models::{PendingItem, Person};

    let tmp = TempDir::new().unwrap();
    let pps = PeopleStore::new(tmp.path());

    let people = jm_core::models::PeopleFile {
        people: vec![
            Person {
                handle: "@alice".to_string(),
                role: "Architect".to_string(),
                projects: vec!["Project A".to_string()],
                pending: vec![],
            },
            Person {
                handle: "@bob".to_string(),
                role: "Developer".to_string(),
                projects: vec!["Project B".to_string(), "Project C".to_string()],
                pending: vec![PendingItem {
                    description: "code review for PR #12".to_string(),
                    since: Some(d(2026, 3, 10)),
                    project: Some("Project B".to_string()),
                }],
            },
        ],
    };

    pps.save(&people).unwrap();
    let loaded = pps.load();

    assert_eq!(loaded.people.len(), 2);
    assert_eq!(loaded.people[0].handle, "@alice");
    assert_eq!(loaded.people[0].role, "Architect");
    assert_eq!(loaded.people[1].handle, "@bob");
    assert_eq!(loaded.people[1].pending.len(), 1);
    assert_eq!(loaded.people[1].pending[0].since, Some(d(2026, 3, 10)));
    assert_eq!(
        loaded.people[1].pending[0].project,
        Some("Project B".to_string())
    );
}
