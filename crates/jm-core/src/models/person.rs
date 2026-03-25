use chrono::NaiveDate;
use regex::Regex;
use std::sync::LazyLock;

static RE_PERSON_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s+(@[\w-]+)").unwrap());
static RE_BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-\s+(.*)").unwrap());
static RE_ROLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Role:\s*(.*)").unwrap());
static RE_PROJECTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Projects:\s*(.*)").unwrap());
static RE_PENDING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Pending:\s*(.*)").unwrap());
static RE_ASKED_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(asked\s+(\d{4}-\d{2}-\d{2})\)").unwrap());
static RE_PROJECT_BRACKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]").unwrap());

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PendingItem {
    pub description: String,
    pub since: Option<NaiveDate>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub handle: String, // "@carol"
    pub role: String,
    pub projects: Vec<String>,
    pub pending: Vec<PendingItem>,
}

impl Person {
    pub fn new(handle: &str) -> Self {
        Self {
            handle: handle.to_string(),
            role: String::new(),
            projects: Vec::new(),
            pending: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeopleFile {
    pub people: Vec<Person>,
}

impl PeopleFile {
    pub fn new() -> Self {
        Self {
            people: Vec::new(),
        }
    }

    /// Serialize to markdown (no frontmatter needed for people file).
    pub fn to_markdown(&self) -> String {
        let mut sections: Vec<String> = Vec::new();

        for person in &self.people {
            let mut lines = vec![format!("## {}", person.handle)];
            if !person.role.is_empty() {
                lines.push(format!("- Role: {}", person.role));
            }
            if !person.projects.is_empty() {
                lines.push(format!("- Projects: {}", person.projects.join(", ")));
            }
            for item in &person.pending {
                let mut text = item.description.clone();
                if let Some(since) = item.since {
                    text.push_str(&format!(" (asked {since})"));
                }
                if let Some(project) = &item.project {
                    text.push_str(&format!(" [{project}]"));
                }
                lines.push(format!("- Pending: {text}"));
            }
            sections.push(lines.join("\n"));
        }

        sections.join("\n\n")
    }

    /// Parse from markdown.
    pub fn from_markdown(text: &str) -> Self {
        let mut people: Vec<Person> = Vec::new();
        let mut current_person: Option<Person> = None;

        for line in text.split('\n') {
            // Check for person header: ## @handle
            if let Some(caps) = RE_PERSON_HEADER.captures(line) {
                if let Some(person) = current_person.take() {
                    people.push(person);
                }
                let handle = caps[1].to_string();
                current_person = Some(Person::new(&handle));
                continue;
            }

            let Some(ref mut person) = current_person else {
                continue;
            };

            let stripped = line.trim();
            if stripped.is_empty() {
                continue;
            }

            let Some(bullet_caps) = RE_BULLET.captures(stripped) else {
                continue;
            };
            let content = &bullet_caps[1];

            // Role
            if let Some(caps) = RE_ROLE.captures(content) {
                person.role = caps[1].trim().to_string();
                continue;
            }

            // Projects
            if let Some(caps) = RE_PROJECTS.captures(content) {
                person.projects = caps[1]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                continue;
            }

            // Pending
            if let Some(caps) = RE_PENDING.captures(content) {
                let mut pending_text = caps[1].trim().to_string();
                let mut since: Option<NaiveDate> = None;
                let mut project: Option<String> = None;

                // Extract date: (asked YYYY-MM-DD)
                if let Some(m) = RE_ASKED_DATE.find(&pending_text) {
                    if let Some(date_caps) = RE_ASKED_DATE.captures(&pending_text) {
                        since =
                            NaiveDate::parse_from_str(&date_caps[1], "%Y-%m-%d").ok();
                    }
                    let before = &pending_text[..m.start()];
                    let after = &pending_text[m.end()..];
                    pending_text = format!("{before}{after}").trim().to_string();
                }

                // Extract project: [ProjectName]
                if let Some(m) = RE_PROJECT_BRACKET.find(&pending_text) {
                    if let Some(proj_caps) = RE_PROJECT_BRACKET.captures(&pending_text) {
                        project = Some(proj_caps[1].to_string());
                    }
                    let before = &pending_text[..m.start()];
                    let after = &pending_text[m.end()..];
                    pending_text = format!("{before}{after}").trim().to_string();
                }

                person.pending.push(PendingItem {
                    description: pending_text,
                    since,
                    project,
                });
            }
        }

        // Don't forget the last person
        if let Some(person) = current_person {
            people.push(person);
        }

        PeopleFile { people }
    }
}

impl Default for PeopleFile {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_full_people() -> PeopleFile {
        PeopleFile {
            people: vec![
                Person {
                    handle: "@carol".to_string(),
                    role: "Display Systems Lead".to_string(),
                    projects: vec!["HMI Framework".to_string()],
                    pending: vec![PendingItem {
                        description: "spec clarification".to_string(),
                        since: Some(d(2026, 3, 14)),
                        project: None,
                    }],
                },
                Person {
                    handle: "@bob".to_string(),
                    role: "Test Infra reviewer".to_string(),
                    projects: vec!["Test Infra".to_string()],
                    pending: vec![PendingItem {
                        description: "PR re-review".to_string(),
                        since: None,
                        project: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_full_round_trip() {
        let original = make_full_people();
        let md = original.to_markdown();
        let restored = PeopleFile::from_markdown(&md);

        assert_eq!(restored.people.len(), original.people.len());
        for (orig, rest) in original.people.iter().zip(restored.people.iter()) {
            assert_eq!(rest.handle, orig.handle);
            assert_eq!(rest.role, orig.role);
            assert_eq!(rest.projects, orig.projects);
            assert_eq!(rest.pending.len(), orig.pending.len());
            for (oi, ri) in orig.pending.iter().zip(rest.pending.iter()) {
                assert_eq!(ri.description, oi.description);
                assert_eq!(ri.since, oi.since);
                assert_eq!(ri.project, oi.project);
            }
        }
    }

    #[test]
    fn test_double_round_trip_stable() {
        let original = make_full_people();
        let md1 = original.to_markdown();
        let restored = PeopleFile::from_markdown(&md1);
        let md2 = restored.to_markdown();
        assert_eq!(md1, md2);
    }

    #[test]
    fn test_empty_people_file() {
        let pf = PeopleFile::new();
        let md = pf.to_markdown();
        let restored = PeopleFile::from_markdown(&md);
        assert!(restored.people.is_empty());
    }

    #[test]
    fn test_person_no_pending() {
        let pf = PeopleFile {
            people: vec![Person {
                handle: "@alice".to_string(),
                role: "Manager".to_string(),
                projects: vec!["Project X".to_string(), "Project Y".to_string()],
                pending: Vec::new(),
            }],
        };
        let md = pf.to_markdown();
        assert!(!md.contains("Pending"));
        let restored = PeopleFile::from_markdown(&md);
        assert_eq!(restored.people[0].handle, "@alice");
        assert_eq!(
            restored.people[0].projects,
            vec!["Project X", "Project Y"]
        );
        assert!(restored.people[0].pending.is_empty());
    }

    #[test]
    fn test_person_no_role_no_projects() {
        let pf = PeopleFile {
            people: vec![Person {
                handle: "@eve".to_string(),
                role: String::new(),
                projects: Vec::new(),
                pending: vec![PendingItem {
                    description: "feedback".to_string(),
                    since: None,
                    project: None,
                }],
            }],
        };
        let md = pf.to_markdown();
        let restored = PeopleFile::from_markdown(&md);
        assert_eq!(restored.people[0].handle, "@eve");
        assert_eq!(restored.people[0].role, "");
        assert!(restored.people[0].projects.is_empty());
        assert_eq!(restored.people[0].pending[0].description, "feedback");
    }

    #[test]
    fn test_pending_with_date_and_project() {
        let pf = PeopleFile {
            people: vec![Person {
                handle: "@frank".to_string(),
                role: String::new(),
                projects: Vec::new(),
                pending: vec![PendingItem {
                    description: "design review".to_string(),
                    since: Some(d(2026, 3, 10)),
                    project: Some("HMI Framework".to_string()),
                }],
            }],
        };
        let md = pf.to_markdown();
        assert!(md.contains("(asked 2026-03-10)"));
        assert!(md.contains("[HMI Framework]"));
        let restored = PeopleFile::from_markdown(&md);
        let item = &restored.people[0].pending[0];
        assert_eq!(item.description, "design review");
        assert_eq!(item.since, Some(d(2026, 3, 10)));
        assert_eq!(item.project, Some("HMI Framework".to_string()));
    }

    #[test]
    fn test_multiple_pending_items() {
        let pf = PeopleFile {
            people: vec![Person {
                handle: "@carol".to_string(),
                role: String::new(),
                projects: Vec::new(),
                pending: vec![
                    PendingItem {
                        description: "item one".to_string(),
                        since: Some(d(2026, 3, 1)),
                        project: None,
                    },
                    PendingItem {
                        description: "item two".to_string(),
                        since: None,
                        project: None,
                    },
                ],
            }],
        };
        let md = pf.to_markdown();
        let restored = PeopleFile::from_markdown(&md);
        assert_eq!(restored.people[0].pending.len(), 2);
        assert_eq!(restored.people[0].pending[0].description, "item one");
        assert_eq!(restored.people[0].pending[0].since, Some(d(2026, 3, 1)));
        assert_eq!(restored.people[0].pending[1].description, "item two");
    }
}
