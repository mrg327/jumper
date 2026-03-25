use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::storage::{ActiveProjectStore, IssueStore, JournalStore, PeopleStore, ProjectStore};

/// Generate a clean text dump of current jm state.
/// Returns ANSI-free plain text suitable for AI agent consumption.
pub fn generate_dump(
    project_store: &ProjectStore,
    journal_store: &JournalStore,
    _people_store: &PeopleStore,
    active_store: &ActiveProjectStore,
) -> String {
    generate_dump_with_issues(project_store, journal_store, _people_store, active_store, None)
}

/// Generate dump, optionally including issues section.
pub fn generate_dump_with_issues(
    project_store: &ProjectStore,
    journal_store: &JournalStore,
    _people_store: &PeopleStore,
    active_store: &ActiveProjectStore,
    issue_store: Option<&IssueStore>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let today = Local::now().date_naive();
    let today_str = today.format("%a %b %d, %Y").to_string();

    // Header
    lines.push(format!("jm -- Job Manager{:>28}", today_str));
    lines.push(String::new());

    // Projects
    let mut projects = project_store.list_projects(None);
    let status_order = |s: &crate::models::Status| -> u8 {
        use crate::models::Status;
        match s {
            Status::Active => 0,
            Status::Blocked => 1,
            Status::Pending => 2,
            Status::Parked => 3,
            Status::Done => 4,
        }
    };
    projects.sort_by(|a, b| {
        status_order(&a.status)
            .cmp(&status_order(&b.status))
            .then_with(|| a.name.cmp(&b.name))
    });

    lines.push(format!("ACTIVE PROJECTS ({})", projects.len()));
    if !projects.is_empty() {
        lines.push(format!(
            "  {:<20} {:<10} {:<6} Current Focus",
            "Project", "Status", "Pri"
        ));
        for p in &projects {
            let pri = match p.priority {
                crate::models::Priority::High => "high",
                crate::models::Priority::Medium => "med",
                crate::models::Priority::Low => "low",
            };
            let focus = if p.current_focus.len() > 40 {
                &p.current_focus[..40]
            } else {
                &p.current_focus
            };
            lines.push(format!(
                "  {:<20} {:<10} {:<6} {}",
                p.name, p.status, pri, focus
            ));
        }
    } else {
        lines.push("  No projects yet".to_string());
    }
    lines.push(String::new());

    // Blockers
    let mut blocker_items: Vec<String> = Vec::new();
    for p in &projects {
        for b in &p.blockers {
            if !b.resolved {
                let days = b.since.map(|s| {
                    let delta = (today - s).num_days();
                    format!(" ({delta} days)")
                }).unwrap_or_default();
                let person = b.person.as_deref().map(|p| format!(" {p}")).unwrap_or_default();
                blocker_items.push(format!(
                    "  {}: {}{}{}",
                    p.name, b.description, person, days
                ));
            }
        }
    }

    lines.push(format!("BLOCKERS ({})", blocker_items.len()));
    if !blocker_items.is_empty() {
        lines.extend(blocker_items);
    } else {
        lines.push("  No open blockers".to_string());
    }
    lines.push(String::new());

    // Issues
    if let Some(is) = issue_store {
        use crate::models::IssueStatus;

        let mut all_open = 0usize;
        let mut issue_lines: Vec<String> = Vec::new();
        let mut proj_with_issues = 0usize;

        for p in &projects {
            let issue_file = is.load(&p.slug);
            let open_issues: Vec<_> = issue_file
                .issues
                .iter()
                .filter(|i| i.status != IssueStatus::Done)
                .collect();
            if open_issues.is_empty() {
                continue;
            }
            proj_with_issues += 1;
            let active_count = open_issues.iter().filter(|i| i.status == IssueStatus::Active).count();
            let mut summary = format!("{} open", open_issues.len());
            if active_count > 0 {
                summary.push_str(&format!(", {active_count} active"));
            }
            issue_lines.push(format!("  {}: {summary}", p.name));
            all_open += open_issues.len();

            let cm = issue_file.children_map();
            let mut shown = 0usize;
            for issue in cm.get(&None).unwrap_or(&vec![]) {
                if issue.status == IssueStatus::Done || shown >= 10 {
                    continue;
                }
                issue_lines.push(format!(
                    "    #{:<3} [{:<7}] {}",
                    issue.id, issue.status, issue.title
                ));
                shown += 1;
                if let Some(children) = cm.get(&Some(issue.id)) {
                    for child in children {
                        if child.status == IssueStatus::Done || shown >= 10 {
                            continue;
                        }
                        issue_lines.push(format!(
                            "      #{:<3} [{:<7}] {}",
                            child.id, child.status, child.title
                        ));
                        shown += 1;
                    }
                }
            }
        }

        if !issue_lines.is_empty() {
            let pl = if proj_with_issues == 1 { "" } else { "s" };
            lines.push(format!("ISSUES ({all_open} open across {proj_with_issues} project{pl})"));
            lines.extend(issue_lines);
        } else {
            lines.push("ISSUES (0)".to_string());
            lines.push("  No open issues".to_string());
        }
        lines.push(String::new());
    }

    // Today's log
    let journal = journal_store.today();
    lines.push("TODAY'S LOG".to_string());
    if !journal.entries.is_empty() {
        for entry in &journal.entries {
            if entry.entry_type == "Switched" {
                lines.push(format!("  {}  Switched -> {}", entry.time, entry.project));
            } else if entry.entry_type == "Done" {
                lines.push(format!("  {}  Done for day", entry.time));
            } else {
                lines.push(format!(
                    "  {}  {} {}",
                    entry.time, entry.entry_type, entry.project
                ));
            }
        }
    } else {
        lines.push("  No entries yet today".to_string());
    }
    lines.push(String::new());

    // Active project
    let active_slug = active_store.get_active();
    if let Some(slug) = active_slug {
        let active_project = project_store.get_project(&slug);
        let name = active_project
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or(&slug);

        // Find when the active project was last started
        let mut last_start = String::new();
        for entry in journal.entries.iter().rev() {
            if (entry.entry_type == "Started" || entry.entry_type == "Switched")
                && entry.project.contains(name)
            {
                last_start = format!(" (since {})", entry.time);
                break;
            }
        }
        lines.push(format!("ACTIVE: {name}{last_start}"));
    } else {
        lines.push("ACTIVE: none".to_string());
    }

    lines.join("\n")
}

/// Export dump to file. Returns the path written to.
pub fn export_to_file(
    project_store: &ProjectStore,
    journal_store: &JournalStore,
    people_store: &PeopleStore,
    active_store: &ActiveProjectStore,
    output_path: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let text = generate_dump(project_store, journal_store, people_store, active_store);

    let path = match output_path {
        Some(p) => p.to_path_buf(),
        None => {
            let config = crate::config::Config::load();
            config.export_path()
        }
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &text)?;
    Ok(path)
}

/// Print dump to stdout.
pub fn dump_to_stdout(
    project_store: &ProjectStore,
    journal_store: &JournalStore,
    people_store: &PeopleStore,
    active_store: &ActiveProjectStore,
) {
    println!(
        "{}",
        generate_dump(project_store, journal_store, people_store, active_store)
    );
}

/// Print dump to stdout, including issues.
pub fn dump_to_stdout_with_issues(
    project_store: &ProjectStore,
    journal_store: &JournalStore,
    people_store: &PeopleStore,
    active_store: &ActiveProjectStore,
    issue_store: &IssueStore,
) {
    println!(
        "{}",
        generate_dump_with_issues(project_store, journal_store, people_store, active_store, Some(issue_store))
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::store::create_stores;
    use tempfile::TempDir;

    #[test]
    fn test_generate_dump_empty() {
        let tmp = TempDir::new().unwrap();
        let (ps, js, pps, as_) = create_stores(tmp.path());
        let dump = generate_dump(&ps, &js, &pps, &as_);
        assert!(dump.contains("ACTIVE PROJECTS (0)"));
        assert!(dump.contains("No projects yet"));
        assert!(dump.contains("BLOCKERS (0)"));
        assert!(dump.contains("No open blockers"));
        assert!(dump.contains("ACTIVE: none"));
    }

    #[test]
    fn test_generate_dump_with_project() {
        let tmp = TempDir::new().unwrap();
        let (ps, js, pps, as_) = create_stores(tmp.path());

        ps.create_project("Test Project").unwrap();

        let dump = generate_dump(&ps, &js, &pps, &as_);
        assert!(dump.contains("ACTIVE PROJECTS (1)"));
        assert!(dump.contains("Test Project"));
    }

    #[test]
    fn test_dump_no_ansi() {
        let tmp = TempDir::new().unwrap();
        let (ps, js, pps, as_) = create_stores(tmp.path());
        let dump = generate_dump(&ps, &js, &pps, &as_);
        // Should contain no ANSI escape codes
        assert!(!dump.contains("\x1b["));
    }
}
