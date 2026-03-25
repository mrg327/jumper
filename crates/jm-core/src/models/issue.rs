//! Issue model: hierarchical issues per project (2 levels max: issue + sub-issue).
//!
//! Stored as one markdown file per project in `~/.jm/issues/<slug>.md`.
//!
//! Format:
//! ```markdown
//! ---
//! project: hmi-framework
//! next_id: 4
//! ---
//!
//! ## #1 Implement focus ring on NavBar
//! status: todo
//! created: 2026-03-15
//!
//! ## #2 Write CSS for :focus states
//! status: active
//! parent: 1
//! created: 2026-03-16
//! ```

use std::collections::HashMap;
use std::fmt;

use chrono::NaiveDate;
use regex::Regex;
use std::sync::LazyLock;

static RE_ISSUE_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#(\d+)\s+(.+)").unwrap());

static RE_KEY_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-z_]+):\s*(.*)").unwrap());

// ── Issue ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub id: u32,
    pub title: String,
    pub status: IssueStatus,
    pub parent_id: Option<u32>,
    pub created: NaiveDate,
    pub closed: Option<NaiveDate>,
    pub notes: String,
    pub r#ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueStatus {
    Todo,
    Active,
    Blocked,
    Done,
}

impl IssueStatus {
    /// Cycle forward: todo -> active -> blocked -> done -> todo
    pub fn cycle(self) -> Self {
        match self {
            Self::Todo => Self::Active,
            Self::Active => Self::Blocked,
            Self::Blocked => Self::Done,
            Self::Done => Self::Todo,
        }
    }

    /// Cycle backward: todo -> done -> blocked -> active -> todo
    pub fn cycle_reverse(self) -> Self {
        match self {
            Self::Todo => Self::Done,
            Self::Active => Self::Todo,
            Self::Blocked => Self::Active,
            Self::Done => Self::Blocked,
        }
    }

    /// All variants in display order (for kanban columns).
    pub fn all_variants() -> &'static [IssueStatus] {
        &[Self::Todo, Self::Active, Self::Blocked, Self::Done]
    }
}

impl fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Todo => write!(f, "todo"),
            Self::Active => write!(f, "active"),
            Self::Blocked => write!(f, "blocked"),
            Self::Done => write!(f, "done"),
        }
    }
}

impl std::str::FromStr for IssueStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "todo" => Ok(Self::Todo),
            "active" => Ok(Self::Active),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Done),
            other => Err(format!("unknown issue status: '{other}'")),
        }
    }
}

// ── IssueFile ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IssueFile {
    pub project_slug: String,
    pub next_id: u32,
    pub issues: Vec<Issue>,
}

impl IssueFile {
    pub fn new(project_slug: &str) -> Self {
        Self {
            project_slug: project_slug.to_string(),
            next_id: 1,
            issues: Vec::new(),
        }
    }

    /// O(n) single pass. Returns parent_id -> children list.
    pub fn children_map(&self) -> HashMap<Option<u32>, Vec<&Issue>> {
        let mut map: HashMap<Option<u32>, Vec<&Issue>> = HashMap::new();
        for issue in &self.issues {
            map.entry(issue.parent_id).or_default().push(issue);
        }
        map
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();

        // YAML frontmatter
        out.push_str("---\n");
        out.push_str(&format!("project: {}\n", self.project_slug));
        out.push_str(&format!("next_id: {}\n", self.next_id));
        out.push_str("---\n");

        for (i, issue) in self.issues.iter().enumerate() {
            if i == 0 {
                out.push('\n');
            } else {
                out.push('\n');
            }
            out.push_str(&format!("## #{} {}\n", issue.id, issue.title));
            out.push_str(&format!("status: {}\n", issue.status));
            if let Some(pid) = issue.parent_id {
                out.push_str(&format!("parent: {pid}\n"));
            }
            if let Some(closed) = issue.closed {
                out.push_str(&format!("closed: {closed}\n"));
            }
            out.push_str(&format!("created: {}\n", issue.created));
            if !issue.notes.is_empty() {
                out.push_str(&format!("notes: {}\n", issue.notes));
            }
            if !issue.r#ref.is_empty() {
                out.push_str(&format!("ref: {}\n", issue.r#ref));
            }
        }

        out
    }

    pub fn from_markdown(text: &str) -> Result<Self, String> {
        // Parse frontmatter manually (simple YAML between --- markers)
        let (meta, body) = parse_frontmatter(text)?;

        let project_slug = meta
            .get("project")
            .cloned()
            .unwrap_or_default();
        let next_id: u32 = meta
            .get("next_id")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let mut issues = Vec::new();
        let mut current_header: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();

        for line in body.lines() {
            if line.starts_with("## #") {
                if let Some(ref header) = current_header {
                    if let Some(issue) = parse_issue(header, &current_lines) {
                        issues.push(issue);
                    }
                }
                current_header = Some(line[3..].trim().to_string());
                current_lines = Vec::new();
            } else {
                current_lines.push(line.to_string());
            }
        }

        // Process last issue
        if let Some(ref header) = current_header {
            if let Some(issue) = parse_issue(header, &current_lines) {
                issues.push(issue);
            }
        }

        Ok(Self {
            project_slug,
            next_id,
            issues,
        })
    }
}

// ── Frontmatter parser ───────────────────────────────────────────────

fn parse_frontmatter(text: &str) -> Result<(HashMap<String, String>, String), String> {
    let text = text.trim_start();
    if !text.starts_with("---") {
        return Ok((HashMap::new(), text.to_string()));
    }

    let after_first = &text[3..];
    let end = after_first
        .find("\n---")
        .ok_or("no closing --- in frontmatter")?;

    let yaml_block = &after_first[..end];
    let body_start = end + 4; // skip past \n---
    let body = if body_start < after_first.len() {
        // Skip the newline after closing ---
        after_first[body_start..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    let mut meta = HashMap::new();
    for line in yaml_block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            meta.insert(
                key.trim().to_string(),
                value.trim().to_string(),
            );
        }
    }

    Ok((meta, body))
}

// ── Issue line parser ────────────────────────────────────────────────

fn parse_issue(header: &str, detail_lines: &[String]) -> Option<Issue> {
    let caps = RE_ISSUE_HEADER.captures(header)?;
    let id: u32 = caps[1].parse().ok()?;
    let title = caps[2].trim().to_string();

    let mut fields: HashMap<String, String> = HashMap::new();
    for line in detail_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = RE_KEY_VALUE.captures(trimmed) {
            fields.insert(caps[1].to_string(), caps[2].trim().to_string());
        }
    }

    let status: IssueStatus = fields
        .get("status")
        .and_then(|s| s.parse().ok())
        .unwrap_or(IssueStatus::Todo);

    let parent_id: Option<u32> = fields.get("parent").and_then(|s| s.parse().ok());

    let created = fields
        .get("created")
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Local::now().date_naive());

    let closed = fields
        .get("closed")
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let notes = fields.get("notes").cloned().unwrap_or_default();
    let r#ref = fields.get("ref").cloned().unwrap_or_default();

    Some(Issue {
        id,
        title,
        status,
        parent_id,
        created,
        closed,
        notes,
        r#ref,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_full_issue_file() -> IssueFile {
        IssueFile {
            project_slug: "hmi-framework".to_string(),
            next_id: 4,
            issues: vec![
                Issue {
                    id: 1,
                    title: "Implement focus ring on NavBar".to_string(),
                    status: IssueStatus::Todo,
                    parent_id: None,
                    created: d(2026, 3, 15),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
                Issue {
                    id: 2,
                    title: "Write CSS for :focus states".to_string(),
                    status: IssueStatus::Active,
                    parent_id: Some(1),
                    created: d(2026, 3, 16),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
                Issue {
                    id: 3,
                    title: "Fix crash on startup".to_string(),
                    status: IssueStatus::Done,
                    parent_id: None,
                    created: d(2026, 3, 10),
                    closed: Some(d(2026, 3, 17)),
                    notes: "was a null pointer in init sequence".to_string(),
                    r#ref: "JIRA-1234".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_full_round_trip() {
        let original = make_full_issue_file();
        let md = original.to_markdown();
        let restored = IssueFile::from_markdown(&md).unwrap();

        assert_eq!(restored.project_slug, original.project_slug);
        assert_eq!(restored.next_id, original.next_id);
        assert_eq!(restored.issues.len(), original.issues.len());

        for (orig, rest) in original.issues.iter().zip(restored.issues.iter()) {
            assert_eq!(rest.id, orig.id);
            assert_eq!(rest.title, orig.title);
            assert_eq!(rest.status, orig.status);
            assert_eq!(rest.parent_id, orig.parent_id);
            assert_eq!(rest.created, orig.created);
            assert_eq!(rest.closed, orig.closed);
            assert_eq!(rest.notes, orig.notes);
            assert_eq!(rest.r#ref, orig.r#ref);
        }
    }

    #[test]
    fn test_double_round_trip_stable() {
        let original = make_full_issue_file();
        let md1 = original.to_markdown();
        let restored = IssueFile::from_markdown(&md1).unwrap();
        let md2 = restored.to_markdown();
        assert_eq!(md1, md2);
    }

    #[test]
    fn test_minimal_issue() {
        let issue_file = IssueFile {
            project_slug: "test".to_string(),
            next_id: 2,
            issues: vec![Issue {
                id: 1,
                title: "Simple task".to_string(),
                status: IssueStatus::Todo,
                parent_id: None,
                created: d(2026, 3, 18),
                closed: None,
                notes: String::new(),
                r#ref: String::new(),
            }],
        };
        let md = issue_file.to_markdown();
        let restored = IssueFile::from_markdown(&md).unwrap();
        assert_eq!(restored.issues.len(), 1);
        let i = &restored.issues[0];
        assert_eq!(i.id, 1);
        assert_eq!(i.title, "Simple task");
        assert_eq!(i.status, IssueStatus::Todo);
        assert!(i.parent_id.is_none());
        assert!(i.closed.is_none());
        assert!(i.notes.is_empty());
        assert!(i.r#ref.is_empty());
    }

    #[test]
    fn test_empty_issue_file() {
        let issue_file = IssueFile::new("empty-project");
        let md = issue_file.to_markdown();
        let restored = IssueFile::from_markdown(&md).unwrap();
        assert_eq!(restored.project_slug, "empty-project");
        assert_eq!(restored.next_id, 1);
        assert!(restored.issues.is_empty());
    }

    #[test]
    fn test_parent_child_hierarchy() {
        let issue_file = IssueFile {
            project_slug: "test".to_string(),
            next_id: 5,
            issues: vec![
                Issue {
                    id: 1,
                    title: "Parent A".to_string(),
                    status: IssueStatus::Todo,
                    parent_id: None,
                    created: d(2026, 3, 18),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
                Issue {
                    id: 2,
                    title: "Child A.1".to_string(),
                    status: IssueStatus::Todo,
                    parent_id: Some(1),
                    created: d(2026, 3, 18),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
                Issue {
                    id: 3,
                    title: "Child A.2".to_string(),
                    status: IssueStatus::Todo,
                    parent_id: Some(1),
                    created: d(2026, 3, 18),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
                Issue {
                    id: 4,
                    title: "Parent B".to_string(),
                    status: IssueStatus::Todo,
                    parent_id: None,
                    created: d(2026, 3, 18),
                    closed: None,
                    notes: String::new(),
                    r#ref: String::new(),
                },
            ],
        };
        let cm = issue_file.children_map();
        let top = cm.get(&None).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].id, 1);
        assert_eq!(top[1].id, 4);
        let children = cm.get(&Some(1)).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].id, 2);
        assert_eq!(children[1].id, 3);
        assert!(!cm.contains_key(&Some(4)));
    }

    #[test]
    fn test_omits_default_fields() {
        let issue_file = IssueFile {
            project_slug: "test".to_string(),
            next_id: 2,
            issues: vec![Issue {
                id: 1,
                title: "Top level".to_string(),
                status: IssueStatus::Todo,
                parent_id: None,
                created: d(2026, 3, 18),
                closed: None,
                notes: String::new(),
                r#ref: String::new(),
            }],
        };
        let md = issue_file.to_markdown();
        assert!(!md.contains("parent:"));
        assert!(!md.contains("notes:"));
        assert!(!md.contains("ref:"));
        assert!(!md.contains("closed:"));
    }

    #[test]
    fn test_issue_status_cycle() {
        assert_eq!(IssueStatus::Todo.cycle(), IssueStatus::Active);
        assert_eq!(IssueStatus::Active.cycle(), IssueStatus::Blocked);
        assert_eq!(IssueStatus::Blocked.cycle(), IssueStatus::Done);
        assert_eq!(IssueStatus::Done.cycle(), IssueStatus::Todo);
    }

    #[test]
    fn test_issue_status_cycle_reverse() {
        assert_eq!(IssueStatus::Todo.cycle_reverse(), IssueStatus::Done);
        assert_eq!(IssueStatus::Active.cycle_reverse(), IssueStatus::Todo);
        assert_eq!(IssueStatus::Blocked.cycle_reverse(), IssueStatus::Active);
        assert_eq!(IssueStatus::Done.cycle_reverse(), IssueStatus::Blocked);
    }

    #[test]
    fn test_issue_status_cycle_roundtrip() {
        // Forward then reverse returns to original
        for &status in IssueStatus::all_variants() {
            assert_eq!(status.cycle().cycle_reverse(), status);
            assert_eq!(status.cycle_reverse().cycle(), status);
        }
    }

    #[test]
    fn test_issue_status_all_variants() {
        let all = IssueStatus::all_variants();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0], IssueStatus::Todo);
        assert_eq!(all[1], IssueStatus::Active);
        assert_eq!(all[2], IssueStatus::Blocked);
        assert_eq!(all[3], IssueStatus::Done);
    }

    #[test]
    fn test_issue_status_parse() {
        assert_eq!("todo".parse::<IssueStatus>().unwrap(), IssueStatus::Todo);
        assert_eq!("active".parse::<IssueStatus>().unwrap(), IssueStatus::Active);
        assert_eq!("blocked".parse::<IssueStatus>().unwrap(), IssueStatus::Blocked);
        assert_eq!("done".parse::<IssueStatus>().unwrap(), IssueStatus::Done);
        assert!("invalid".parse::<IssueStatus>().is_err());
    }
}
