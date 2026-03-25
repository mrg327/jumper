//! Cross-link parsing: `[[project-slug]]` references between projects.

use std::sync::LazyLock;

use regex::Regex;

use crate::models::Project;

static RE_CROSSLINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([a-z0-9-]+)\]\]").unwrap());

/// Extract all `[[slug]]` references from text.
pub fn extract_links(text: &str) -> Vec<String> {
    RE_CROSSLINK
        .captures_iter(text)
        .map(|cap| cap[1].to_string())
        .collect()
}

/// Find all projects that reference `slug` via `[[slug]]`.
/// Returns `(referencing_project_slug, context_line)`.
pub fn find_references(slug: &str, projects: &[Project]) -> Vec<(String, String)> {
    let target = format!("[[{slug}]]");
    let mut refs: Vec<(String, String)> = Vec::new();

    for project in projects {
        if project.slug == slug {
            continue;
        }

        // Check current_focus
        if project.current_focus.contains(&target) {
            refs.push((project.slug.clone(), format!("Focus: {}", project.current_focus)));
        }

        // Check blockers
        for blocker in &project.blockers {
            if blocker.description.contains(&target) {
                refs.push((
                    project.slug.clone(),
                    format!("Blocker: {}", blocker.description),
                ));
            }
        }

        // Check decisions
        for decision in &project.decisions {
            if decision.choice.contains(&target) {
                refs.push((
                    project.slug.clone(),
                    format!("Decision: {}", decision.choice),
                ));
            }
        }

        // Check log entries
        for entry in &project.log {
            for line in &entry.lines {
                if line.contains(&target) {
                    refs.push((project.slug.clone(), line.clone()));
                }
            }
        }
    }

    refs
}

/// Split text into spans, highlighting `[[slug]]` references.
/// Returns (text, is_link) pairs.
pub fn split_with_links(text: &str) -> Vec<(String, bool)> {
    let mut result = Vec::new();
    let mut last_end = 0;

    for mat in RE_CROSSLINK.find_iter(text) {
        if mat.start() > last_end {
            result.push((text[last_end..mat.start()].to_string(), false));
        }
        result.push((mat.as_str().to_string(), true));
        last_end = mat.end();
    }

    if last_end < text.len() {
        result.push((text[last_end..].to_string(), false));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LogEntry, Project};
    use chrono::NaiveDate;

    fn make_project(slug: &str, focus: &str, log_lines: Vec<&str>) -> Project {
        let mut p = Project::new(slug);
        p.current_focus = focus.to_string();
        if !log_lines.is_empty() {
            p.log.push(LogEntry {
                date: NaiveDate::from_ymd_opt(2026, 3, 17).unwrap(),
                lines: log_lines.into_iter().map(String::from).collect(),
            });
        }
        p
    }

    #[test]
    fn test_extract_links() {
        let links = extract_links("See [[foo-bar]] and [[baz]]");
        assert_eq!(links, vec!["foo-bar", "baz"]);
    }

    #[test]
    fn test_extract_no_links() {
        let links = extract_links("No links here");
        assert!(links.is_empty());
    }

    #[test]
    fn test_find_references() {
        let projects = vec![
            make_project("alpha", "Working on [[beta]] integration", vec![]),
            make_project("beta", "Core lib", vec!["Need to check [[alpha]]"]),
            make_project("gamma", "Unrelated", vec![]),
        ];

        let refs = find_references("beta", &projects);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "alpha");

        let refs = find_references("alpha", &projects);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "beta");

        let refs = find_references("gamma", &projects);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_split_with_links() {
        let parts = split_with_links("See [[foo]] and [[bar]] end");
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], ("See ".to_string(), false));
        assert_eq!(parts[1], ("[[foo]]".to_string(), true));
        assert_eq!(parts[2], (" and ".to_string(), false));
        assert_eq!(parts[3], ("[[bar]]".to_string(), true));
        assert_eq!(parts[4], (" end".to_string(), false));
    }
}
