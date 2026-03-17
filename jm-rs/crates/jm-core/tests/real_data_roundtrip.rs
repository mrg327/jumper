//! Round-trip compatibility tests against real ~/.jm/ data.
//! These tests verify the Rust implementation can read/write files
//! created by the Python implementation without data loss.

use std::fs;
use std::path::PathBuf;

fn jm_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".jm")
}

#[test]
fn test_real_project_roundtrip() {
    let projects_dir = jm_dir().join("projects");
    if !projects_dir.exists() {
        eprintln!("Skipping: no ~/.jm/projects directory");
        return;
    }

    let mut count = 0;
    for entry in fs::read_dir(&projects_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let text = fs::read_to_string(&path).unwrap();
        let project = jm_core::models::Project::from_markdown(&text)
            .unwrap_or_else(|e| panic!("Failed to parse {:?}: {e}", path));

        // Re-serialize and re-parse
        let md2 = project.to_markdown();
        let project2 = jm_core::models::Project::from_markdown(&md2)
            .unwrap_or_else(|e| panic!("Failed re-parse {:?}: {e}", path));

        assert_eq!(project.name, project2.name, "name mismatch in {path:?}");
        assert_eq!(project.status, project2.status, "status mismatch in {path:?}");
        assert_eq!(project.priority, project2.priority, "priority mismatch in {path:?}");
        assert_eq!(project.tags, project2.tags, "tags mismatch in {path:?}");
        assert_eq!(project.created, project2.created, "created mismatch in {path:?}");
        assert_eq!(project.target, project2.target, "target mismatch in {path:?}");
        assert_eq!(
            project.current_focus, project2.current_focus,
            "focus mismatch in {path:?}"
        );
        assert_eq!(project.blockers, project2.blockers, "blockers mismatch in {path:?}");
        assert_eq!(
            project.decisions, project2.decisions,
            "decisions mismatch in {path:?}"
        );
        assert_eq!(project.log, project2.log, "log mismatch in {path:?}");

        count += 1;
        eprintln!("  ✓ {:?} round-trips OK", path.file_name().unwrap());
    }
    eprintln!("  {count} project files verified");
}

#[test]
fn test_real_journal_roundtrip() {
    let journal_dir = jm_dir().join("journal");
    if !journal_dir.exists() {
        eprintln!("Skipping: no ~/.jm/journal directory");
        return;
    }

    let mut count = 0;
    for entry in fs::read_dir(&journal_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let text = fs::read_to_string(&path).unwrap();
        let journal = jm_core::models::DailyJournal::from_markdown(&text)
            .unwrap_or_else(|e| panic!("Failed to parse {:?}: {e}", path));

        let md2 = journal.to_markdown();
        let journal2 = jm_core::models::DailyJournal::from_markdown(&md2)
            .unwrap_or_else(|e| panic!("Failed re-parse {:?}: {e}", path));

        assert_eq!(journal.date, journal2.date, "date mismatch in {path:?}");
        assert_eq!(
            journal.entries.len(),
            journal2.entries.len(),
            "entry count mismatch in {path:?}"
        );

        for (i, (e1, e2)) in journal
            .entries
            .iter()
            .zip(journal2.entries.iter())
            .enumerate()
        {
            assert_eq!(e1.time, e2.time, "time mismatch entry {i} in {path:?}");
            assert_eq!(
                e1.entry_type, e2.entry_type,
                "type mismatch entry {i} in {path:?}"
            );
            assert_eq!(
                e1.project, e2.project,
                "project mismatch entry {i} in {path:?}"
            );
            assert_eq!(
                e1.details, e2.details,
                "details mismatch entry {i} in {path:?}"
            );
        }

        count += 1;
        eprintln!("  ✓ {:?} round-trips OK", path.file_name().unwrap());
    }
    eprintln!("  {count} journal files verified");
}

#[test]
fn test_real_people_roundtrip() {
    let people_file = jm_dir().join("people.md");
    if !people_file.exists() {
        eprintln!("Skipping: no ~/.jm/people.md");
        return;
    }

    let text = fs::read_to_string(&people_file).unwrap();
    let people = jm_core::models::PeopleFile::from_markdown(&text);

    let md2 = people.to_markdown();
    let people2 = jm_core::models::PeopleFile::from_markdown(&md2);

    assert_eq!(people.people.len(), people2.people.len(), "people count mismatch");

    for (p1, p2) in people.people.iter().zip(people2.people.iter()) {
        assert_eq!(p1.handle, p2.handle);
        assert_eq!(p1.role, p2.role);
        assert_eq!(p1.projects, p2.projects);
        assert_eq!(p1.pending, p2.pending);
    }

    eprintln!("  ✓ people.md round-trips OK ({} people)", people.people.len());
}
