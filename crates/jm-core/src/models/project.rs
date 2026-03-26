use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// ── Regex patterns ──────────────────────────────────────────────────

static RE_CHECKBOX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\s*\[([ xX])\]\s*(.*)").unwrap());
static RE_RESOLVED_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(resolved\s+(\d{4}-\d{2}-\d{2})\)").unwrap());
static RE_STRIKETHROUGH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^~~(.+?)~~$").unwrap());
static RE_SINCE_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(since\s+(\d{4}-\d{2}-\d{2})\)").unwrap());
static RE_MENTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@([\w-]+)").unwrap());
static RE_DECISION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\s*\*\*(\d{4}-\d{2}-\d{2}):\*\*\s*(.*)").unwrap());
static RE_ALT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\s*Alternatives:\s*(.*)").unwrap());
static RE_LOG_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^###\s+(\d{4}-\d{2}-\d{2})").unwrap());

// ── Status enum ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Blocked,
    Pending,
    Parked,
    Done,
}

impl Default for Status {
    fn default() -> Self {
        Status::Active
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Active => write!(f, "active"),
            Status::Blocked => write!(f, "blocked"),
            Status::Pending => write!(f, "pending"),
            Status::Parked => write!(f, "parked"),
            Status::Done => write!(f, "done"),
        }
    }
}

impl FromStr for Status {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Status::Active),
            "blocked" => Ok(Status::Blocked),
            "pending" => Ok(Status::Pending),
            "parked" => Ok(Status::Parked),
            "done" => Ok(Status::Done),
            _ => Err(()),
        }
    }
}

// ── Priority enum ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Medium
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::High => write!(f, "high"),
            Priority::Medium => write!(f, "medium"),
            Priority::Low => write!(f, "low"),
        }
    }
}

impl FromStr for Priority {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "high" => Ok(Priority::High),
            "medium" => Ok(Priority::Medium),
            "low" => Ok(Priority::Low),
            _ => Err(()),
        }
    }
}

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Blocker {
    pub description: String,
    pub resolved: bool,
    pub since: Option<NaiveDate>,
    pub resolved_date: Option<NaiveDate>,
    pub person: Option<String>, // "@carol"
}

impl Default for Blocker {
    fn default() -> Self {
        Self {
            description: String::new(),
            resolved: false,
            since: None,
            resolved_date: None,
            person: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub date: NaiveDate,
    pub choice: String,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub date: NaiveDate,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    pub slug: String,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub created: NaiveDate,
    pub target: Option<NaiveDate>,
    pub active_issue: Option<u32>,
    pub current_focus: String,
    pub blockers: Vec<Blocker>,
    pub decisions: Vec<Decision>,
    pub log: Vec<LogEntry>,
}

impl Project {
    pub fn new(name: &str) -> Self {
        let slug = name.to_lowercase().replace(' ', "-");
        Self {
            name: name.to_string(),
            slug,
            status: Status::Active,
            priority: Priority::Medium,
            tags: Vec::new(),
            created: chrono::Local::now().date_naive(),
            target: None,
            active_issue: None,
            current_focus: String::new(),
            blockers: Vec::new(),
            decisions: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Serialize to markdown with YAML frontmatter.
    /// Output is byte-compatible with the Python `python-frontmatter` library.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();

        // ── Frontmatter ─────────────────────────────────────────────
        // python-frontmatter/PyYAML outputs keys in alphabetical order
        out.push_str("---\n");
        out.push_str(&format!("created: '{}'\n", self.created));
        out.push_str(&format!("name: {}\n", yaml_string(&self.name)));
        out.push_str(&format!("priority: {}\n", self.priority));
        out.push_str(&format!("status: {}\n", self.status));
        // Tags
        if self.tags.is_empty() {
            out.push_str("tags: []\n");
        } else {
            out.push_str("tags:\n");
            for tag in &self.tags {
                out.push_str(&format!("- {}\n", yaml_string(tag)));
            }
        }
        if let Some(target) = self.target {
            out.push_str(&format!("target: '{}'\n", target));
        }
        if let Some(active_issue) = self.active_issue {
            out.push_str(&format!("active_issue: {}\n", active_issue));
        }
        out.push_str("---");

        // ── Body sections ───────────────────────────────────────────
        let mut sections: Vec<String> = Vec::new();

        if !self.current_focus.is_empty() {
            sections.push(format!("## Current Focus\n{}", self.current_focus));
        }

        if !self.blockers.is_empty() {
            let mut lines = vec!["## Blockers".to_string()];
            for b in &self.blockers {
                let check = if b.resolved { "x" } else { " " };
                let mut text = b.description.clone();
                if let Some(person) = &b.person {
                    let p = person.trim_start_matches('@');
                    text.push_str(&format!(" @{p}"));
                }
                if b.resolved {
                    text = format!("~~{text}~~");
                    if let Some(rd) = b.resolved_date {
                        text.push_str(&format!(" (resolved {rd})"));
                    }
                } else if let Some(since) = b.since {
                    text.push_str(&format!(" (since {since})"));
                }
                lines.push(format!("- [{check}] {text}"));
            }
            sections.push(lines.join("\n"));
        }

        if !self.decisions.is_empty() {
            let mut lines = vec!["## Decisions".to_string()];
            for d in &self.decisions {
                lines.push(format!("- **{}:** {}", d.date, d.choice));
                if !d.alternatives.is_empty() {
                    lines.push(format!("  - Alternatives: {}", d.alternatives.join(", ")));
                }
            }
            sections.push(lines.join("\n"));
        }

        if !self.log.is_empty() {
            let mut lines = vec!["## Log".to_string()];
            for entry in &self.log {
                lines.push(format!("### {}", entry.date));
                for line in &entry.lines {
                    lines.push(format!("- {line}"));
                }
            }
            sections.push(lines.join("\n"));
        }

        if !sections.is_empty() {
            out.push_str("\n\n");
            out.push_str(&sections.join("\n\n"));
        }

        out
    }

    /// Parse from markdown with YAML frontmatter.
    /// The `slug` parameter is the authoritative slug (derived from the filename).
    /// If `None`, falls back to deriving from the name field (useful for tests).
    pub fn from_markdown_with_slug(text: &str, slug: Option<&str>) -> anyhow::Result<Self> {
        let (meta, body) = parse_frontmatter(text)?;

        // Parse frontmatter fields
        let name = meta_str(&meta, "name").unwrap_or_default();

        // Parse status — warn and default if unknown
        let status = match meta_str(&meta, "status") {
            Some(s) => match s.parse::<Status>() {
                Ok(st) => st,
                Err(_) => {
                    eprintln!("Warning: unknown status '{s}', defaulting to 'active'");
                    Status::Active
                }
            },
            None => Status::Active,
        };

        // Parse priority — warn and default if unknown
        let priority = match meta_str(&meta, "priority") {
            Some(p) => match p.parse::<Priority>() {
                Ok(pr) => pr,
                Err(_) => {
                    eprintln!("Warning: unknown priority '{p}', defaulting to 'medium'");
                    Priority::Medium
                }
            },
            None => Priority::Medium,
        };

        let tags = meta_string_list(&meta, "tags");
        let created = meta_date(&meta, "created")
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        let target = meta_date(&meta, "target");
        let active_issue = meta_u32(&meta, "active_issue");

        // Use provided slug (filename) as authoritative; fall back to name-derived slug
        let derived_slug = name.to_lowercase().replace(' ', "-");
        let final_slug = slug.unwrap_or(&derived_slug).to_string();

        // Parse body sections
        let mut current_focus = String::new();
        let mut blockers: Vec<Blocker> = Vec::new();
        let mut decisions: Vec<Decision> = Vec::new();
        let mut log: Vec<LogEntry> = Vec::new();

        let mut current_section: Option<String> = None;
        let mut section_lines: Vec<String> = Vec::new();

        for line in body.split('\n') {
            if let Some(section_name) = line.strip_prefix("## ") {
                if let Some(sec) = current_section.take() {
                    process_section(
                        &sec,
                        &section_lines,
                        &mut current_focus,
                        &mut blockers,
                        &mut decisions,
                        &mut log,
                    );
                }
                current_section = Some(section_name.trim().to_string());
                section_lines.clear();
            } else {
                section_lines.push(line.to_string());
            }
        }

        // Process last section
        if let Some(sec) = current_section.take() {
            process_section(
                &sec,
                &section_lines,
                &mut current_focus,
                &mut blockers,
                &mut decisions,
                &mut log,
            );
        }

        Ok(Project {
            name,
            slug: final_slug,
            status,
            priority,
            tags,
            created,
            target,
            active_issue,
            current_focus,
            blockers,
            decisions,
            log,
        })
    }

    /// Parse from markdown with YAML frontmatter.
    /// The slug is derived from the `name` field (for use in tests / roundtrips).
    /// In production code, prefer `from_markdown_with_slug` with the filename stem.
    pub fn from_markdown(text: &str) -> anyhow::Result<Self> {
        Self::from_markdown_with_slug(text, None)
    }
}

// ── Section parsers ─────────────────────────────────────────────────

fn process_section(
    section_name: &str,
    lines: &[String],
    current_focus: &mut String,
    blockers: &mut Vec<Blocker>,
    decisions: &mut Vec<Decision>,
    log: &mut Vec<LogEntry>,
) {
    match section_name {
        "Current Focus" => {
            let text: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
            *current_focus = text.join("\n").trim().to_string();
        }
        "Blockers" => {
            for line in lines {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Some(caps) = RE_CHECKBOX.captures(line) else {
                    continue;
                };

                let resolved = caps[1].to_lowercase() == "x";
                let mut text = caps[2].to_string();

                let mut resolved_date: Option<NaiveDate> = None;
                let mut since: Option<NaiveDate> = None;
                let mut person: Option<String> = None;

                if resolved {
                    // Extract resolved date from after strikethrough
                    if let Some(m) = RE_RESOLVED_DATE.find(&text) {
                        if let Some(caps) = RE_RESOLVED_DATE.captures(&text) {
                            resolved_date =
                                NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok();
                        }
                        text = text[..m.start()].trim().to_string();
                    }

                    // Strip strikethrough markers
                    if let Some(caps) = RE_STRIKETHROUGH.captures(&text) {
                        text = caps[1].to_string();
                    }
                } else {
                    // Extract since date
                    if let Some(m) = RE_SINCE_DATE.find(&text) {
                        if let Some(caps) = RE_SINCE_DATE.captures(&text) {
                            since = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok();
                        }
                        text = text[..m.start()].trim().to_string();
                    }
                }

                // Extract @mention from text
                if let Some(m) = RE_MENTION.find(&text) {
                    if let Some(caps) = RE_MENTION.captures(&text) {
                        person = Some(format!("@{}", &caps[1]));
                    }
                    // Remove the @mention from description
                    let before = text[..m.start()].trim();
                    let after = text[m.end()..].trim();
                    text = format!("{before} {after}").trim().to_string();
                }

                blockers.push(Blocker {
                    description: text,
                    resolved,
                    since,
                    resolved_date,
                    person,
                });
            }
        }
        "Decisions" => {
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i].trim();
                if line.is_empty() {
                    i += 1;
                    continue;
                }

                let Some(caps) = RE_DECISION.captures(line) else {
                    i += 1;
                    continue;
                };

                let dec_date =
                    NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").unwrap();
                let choice = caps[2].trim().to_string();
                let mut alternatives: Vec<String> = Vec::new();

                // Check next line for alternatives
                if i + 1 < lines.len() {
                    let alt_line = lines[i + 1].trim();
                    if let Some(caps) = RE_ALT.captures(alt_line) {
                        let alt_text = &caps[1];
                        alternatives = alt_text
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        i += 1;
                    }
                }

                decisions.push(Decision {
                    date: dec_date,
                    choice,
                    alternatives,
                });
                i += 1;
            }
        }
        "Log" => {
            let mut current_entry: Option<LogEntry> = None;
            for line in lines {
                if let Some(caps) = RE_LOG_HEADER.captures(line) {
                    if let Some(entry) = current_entry.take() {
                        log.push(entry);
                    }
                    let entry_date =
                        NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").unwrap();
                    current_entry = Some(LogEntry {
                        date: entry_date,
                        lines: Vec::new(),
                    });
                    continue;
                }
                if let Some(entry) = current_entry.as_mut() {
                    let stripped = line.trim();
                    if let Some(rest) = stripped.strip_prefix("- ") {
                        entry.lines.push(rest.to_string());
                    }
                }
            }
            if let Some(entry) = current_entry.take() {
                log.push(entry);
            }
        }
        _ => {}
    }
}

// ── Frontmatter helpers ─────────────────────────────────────────────

/// Parse `---\nyaml\n---\nbody` format.
pub(crate) fn parse_frontmatter(text: &str) -> anyhow::Result<(serde_yml::Value, String)> {
    let text = text.trim_start_matches('\u{feff}'); // strip BOM
    let text = text.trim();

    if !text.starts_with("---") {
        return Err(anyhow::anyhow!("missing YAML frontmatter (file must start with '---')"));
    }

    // Find the second --- delimiter
    let after_first = &text[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end_pos = after_first.find("\n---");
    let (yaml_str, body) = match end_pos {
        Some(pos) => {
            let yaml = &after_first[..pos];
            let rest = &after_first[pos + 4..]; // skip "\n---"
            let body = rest.strip_prefix('\n').unwrap_or(rest);
            (yaml, body.to_string())
        }
        None => {
            // No closing --- found, treat everything as YAML
            (after_first, String::new())
        }
    };

    let meta: serde_yml::Value = serde_yml::from_str(yaml_str)
        .map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))?;
    Ok((meta, body))
}

/// Get a string value from YAML metadata.
fn meta_str(meta: &serde_yml::Value, key: &str) -> Option<String> {
    meta.get(key).and_then(|v| match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        serde_yml::Value::Number(n) => Some(n.to_string()),
        serde_yml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

/// Get a date value from YAML metadata.
fn meta_date(meta: &serde_yml::Value, key: &str) -> Option<NaiveDate> {
    let s = meta_str(meta, key)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()
}

/// Get an optional u32 from YAML metadata.
fn meta_u32(meta: &serde_yml::Value, key: &str) -> Option<u32> {
    meta.get(key).and_then(|v| match v {
        serde_yml::Value::Number(n) => n.as_u64().map(|n| n as u32),
        serde_yml::Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

/// Get a list of strings from YAML metadata.
fn meta_string_list(meta: &serde_yml::Value, key: &str) -> Vec<String> {
    match meta.get(key) {
        Some(serde_yml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| match v {
                serde_yml::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Format a string for YAML output. Strings that need quoting get single-quoted.
fn yaml_string(s: &str) -> String {
    // Simple scalars that don't need quoting
    if s.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
        && !s.is_empty()
        && !["true", "false", "null", "yes", "no", "on", "off"]
            .contains(&s.to_lowercase().as_str())
    {
        // Check if it looks like a date or number
        if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() || s.parse::<f64>().is_ok() {
            return format!("'{s}'");
        }
        s.to_string()
    } else if s.is_empty() {
        "''".to_string()
    } else {
        // Use single quotes, escaping internal single quotes by doubling
        format!("'{}'", s.replace('\'', "''"))
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_full_project() -> Project {
        Project {
            name: "HMI Framework".to_string(),
            slug: "hmi-framework".to_string(),
            status: Status::Active,
            priority: Priority::High,
            tags: vec!["infotainment".to_string(), "rendering".to_string()],
            created: d(2026, 1, 15),
            target: Some(d(2026, 6, 30)),
            active_issue: None,
            current_focus: "debugging render loop".to_string(),
            blockers: vec![
                Blocker {
                    description: "waiting on display spec".to_string(),
                    resolved: false,
                    since: Some(d(2026, 3, 10)),
                    resolved_date: None,
                    person: Some("@carol".to_string()),
                },
                Blocker {
                    description: "GPU driver issue".to_string(),
                    resolved: true,
                    since: None,
                    resolved_date: Some(d(2026, 3, 14)),
                    person: None,
                },
            ],
            decisions: vec![
                Decision {
                    date: d(2026, 2, 1),
                    choice: "Use Vulkan over OpenGL".to_string(),
                    alternatives: vec!["OpenGL ES".to_string(), "DirectFB".to_string()],
                },
                Decision {
                    date: d(2026, 3, 1),
                    choice: "Keep custom compositor".to_string(),
                    alternatives: Vec::new(),
                },
            ],
            log: vec![
                LogEntry {
                    date: d(2026, 3, 14),
                    lines: vec![
                        "Fixed GPU driver init sequence".to_string(),
                        "Ran benchmarks".to_string(),
                    ],
                },
                LogEntry {
                    date: d(2026, 3, 15),
                    lines: vec!["Started render loop debugging".to_string()],
                },
            ],
        }
    }

    #[test]
    fn test_full_round_trip() {
        let original = make_full_project();
        let md = original.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();

        assert_eq!(restored.name, original.name);
        assert_eq!(restored.slug, original.slug);
        assert_eq!(restored.status, original.status);
        assert_eq!(restored.priority, original.priority);
        assert_eq!(restored.tags, original.tags);
        assert_eq!(restored.created, original.created);
        assert_eq!(restored.target, original.target);
        assert_eq!(restored.current_focus, original.current_focus);

        assert_eq!(restored.blockers.len(), original.blockers.len());
        for (orig, rest) in original.blockers.iter().zip(restored.blockers.iter()) {
            assert_eq!(rest.description, orig.description);
            assert_eq!(rest.resolved, orig.resolved);
            assert_eq!(rest.since, orig.since);
            assert_eq!(rest.resolved_date, orig.resolved_date);
            assert_eq!(rest.person, orig.person);
        }

        assert_eq!(restored.decisions.len(), original.decisions.len());
        for (orig, rest) in original.decisions.iter().zip(restored.decisions.iter()) {
            assert_eq!(rest.date, orig.date);
            assert_eq!(rest.choice, orig.choice);
            assert_eq!(rest.alternatives, orig.alternatives);
        }

        assert_eq!(restored.log.len(), original.log.len());
        for (orig, rest) in original.log.iter().zip(restored.log.iter()) {
            assert_eq!(rest.date, orig.date);
            assert_eq!(rest.lines, orig.lines);
        }
    }

    #[test]
    fn test_double_round_trip_stable() {
        let original = make_full_project();
        let md1 = original.to_markdown();
        let restored = Project::from_markdown(&md1).unwrap();
        let md2 = restored.to_markdown();
        assert_eq!(md1, md2);
    }

    #[test]
    fn test_minimal_project() {
        let original = Project {
            name: "Quick Task".to_string(),
            slug: "quick-task".to_string(),
            created: d(2026, 3, 16),
            ..Project::new("Quick Task")
        };
        let md = original.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();

        assert_eq!(restored.name, "Quick Task");
        assert_eq!(restored.slug, "quick-task");
        assert_eq!(restored.status, Status::Active);
        assert_eq!(restored.priority, Priority::Medium);
        assert_eq!(restored.tags, Vec::<String>::new());
        assert_eq!(restored.target, None);
        assert_eq!(restored.current_focus, "");
        assert!(restored.blockers.is_empty());
        assert!(restored.decisions.is_empty());
        assert!(restored.log.is_empty());
    }

    #[test]
    fn test_project_no_target() {
        let p = Project {
            created: d(2026, 3, 16),
            ..Project::new("No Target")
        };
        let md = p.to_markdown();
        assert!(!md.contains("target"));
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.target, None);
    }

    #[test]
    fn test_project_no_blockers() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Clean Project")
        };
        p.decisions.push(Decision {
            date: d(2026, 3, 16),
            choice: "Go with plan A".to_string(),
            alternatives: Vec::new(),
        });
        let md = p.to_markdown();
        assert!(!md.contains("## Blockers"));
        let restored = Project::from_markdown(&md).unwrap();
        assert!(restored.blockers.is_empty());
        assert_eq!(restored.decisions.len(), 1);
    }

    #[test]
    fn test_unresolved_blocker_with_since_and_mention() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.blockers.push(Blocker {
            description: "need API docs".to_string(),
            resolved: false,
            since: Some(d(2026, 3, 10)),
            person: Some("@dave".to_string()),
            ..Default::default()
        });
        let md = p.to_markdown();
        assert!(md.contains("- [ ]"));
        assert!(md.contains("@dave"));
        assert!(md.contains("(since 2026-03-10)"));

        let restored = Project::from_markdown(&md).unwrap();
        let b = &restored.blockers[0];
        assert_eq!(b.description, "need API docs");
        assert!(!b.resolved);
        assert_eq!(b.since, Some(d(2026, 3, 10)));
        assert_eq!(b.person, Some("@dave".to_string()));
    }

    #[test]
    fn test_resolved_blocker_with_date() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.blockers.push(Blocker {
            description: "hardware not available".to_string(),
            resolved: true,
            resolved_date: Some(d(2026, 3, 14)),
            ..Default::default()
        });
        let md = p.to_markdown();
        assert!(md.contains("- [x]"));
        assert!(md.contains("~~"));
        assert!(md.contains("(resolved 2026-03-14)"));

        let restored = Project::from_markdown(&md).unwrap();
        let b = &restored.blockers[0];
        assert_eq!(b.description, "hardware not available");
        assert!(b.resolved);
        assert_eq!(b.resolved_date, Some(d(2026, 3, 14)));
    }

    #[test]
    fn test_resolved_blocker_with_person() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.blockers.push(Blocker {
            description: "waiting on spec".to_string(),
            resolved: true,
            resolved_date: Some(d(2026, 3, 15)),
            person: Some("@carol".to_string()),
            ..Default::default()
        });
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        let b = &restored.blockers[0];
        assert_eq!(b.description, "waiting on spec");
        assert!(b.resolved);
        assert_eq!(b.resolved_date, Some(d(2026, 3, 15)));
        assert_eq!(b.person, Some("@carol".to_string()));
    }

    #[test]
    fn test_blocker_no_date_no_person() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.blockers.push(Blocker {
            description: "simple blocker".to_string(),
            ..Default::default()
        });
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        let b = &restored.blockers[0];
        assert_eq!(b.description, "simple blocker");
        assert!(!b.resolved);
        assert_eq!(b.since, None);
        assert_eq!(b.resolved_date, None);
        assert_eq!(b.person, None);
    }

    #[test]
    fn test_decision_with_alternatives() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.decisions.push(Decision {
            date: d(2026, 2, 1),
            choice: "Use React".to_string(),
            alternatives: vec!["Vue".to_string(), "Angular".to_string()],
        });
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        let dec = &restored.decisions[0];
        assert_eq!(dec.date, d(2026, 2, 1));
        assert_eq!(dec.choice, "Use React");
        assert_eq!(dec.alternatives, vec!["Vue", "Angular"]);
    }

    #[test]
    fn test_decision_without_alternatives() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.decisions.push(Decision {
            date: d(2026, 3, 1),
            choice: "Keep current approach".to_string(),
            alternatives: Vec::new(),
        });
        let md = p.to_markdown();
        assert!(!md.contains("Alternatives"));
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.decisions[0].choice, "Keep current approach");
        assert!(restored.decisions[0].alternatives.is_empty());
    }

    #[test]
    fn test_multiple_decisions() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.decisions = vec![
            Decision {
                date: d(2026, 1, 1),
                choice: "First choice".to_string(),
                alternatives: Vec::new(),
            },
            Decision {
                date: d(2026, 2, 1),
                choice: "Second choice".to_string(),
                alternatives: vec!["Alt A".to_string()],
            },
            Decision {
                date: d(2026, 3, 1),
                choice: "Third choice".to_string(),
                alternatives: Vec::new(),
            },
        ];
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.decisions.len(), 3);
        assert_eq!(restored.decisions[0].choice, "First choice");
        assert_eq!(restored.decisions[1].alternatives, vec!["Alt A"]);
        assert_eq!(restored.decisions[2].choice, "Third choice");
    }

    #[test]
    fn test_multiple_log_entries() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.log = vec![
            LogEntry {
                date: d(2026, 3, 14),
                lines: vec!["Did thing one".to_string(), "Did thing two".to_string()],
            },
            LogEntry {
                date: d(2026, 3, 15),
                lines: vec!["Did thing three".to_string()],
            },
        ];
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.log.len(), 2);
        assert_eq!(restored.log[0].date, d(2026, 3, 14));
        assert_eq!(
            restored.log[0].lines,
            vec!["Did thing one", "Did thing two"]
        );
        assert_eq!(restored.log[1].date, d(2026, 3, 15));
        assert_eq!(restored.log[1].lines, vec!["Did thing three"]);
    }

    #[test]
    fn test_empty_log_entry() {
        let mut p = Project {
            created: d(2026, 3, 16),
            ..Project::new("Test")
        };
        p.log.push(LogEntry {
            date: d(2026, 3, 16),
            lines: Vec::new(),
        });
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.log.len(), 1);
        assert!(restored.log[0].lines.is_empty());
    }

    #[test]
    fn test_project_empty_body() {
        let md = "---\nname: Empty\nstatus: active\npriority: low\ntags: []\ncreated: '2026-03-16'\n---\n";
        let p = Project::from_markdown(md).unwrap();
        assert_eq!(p.name, "Empty");
        assert!(p.blockers.is_empty());
        assert!(p.decisions.is_empty());
        assert!(p.log.is_empty());
        assert_eq!(p.current_focus, "");
    }

    #[test]
    fn test_project_extra_whitespace_in_body() {
        let p = Project {
            created: d(2026, 3, 16),
            current_focus: "  some focus  ".to_string(),
            ..Project::new("Whitespace Test")
        };
        let md = p.to_markdown();
        let restored = Project::from_markdown(&md).unwrap();
        assert_eq!(restored.current_focus, "some focus");
    }

    #[test]
    fn test_slug_auto_generated() {
        let p = Project::new("My Cool Project");
        assert_eq!(p.slug, "my-cool-project");
    }

    #[test]
    fn test_slug_preserved_if_set() {
        let mut p = Project::new("My Cool Project");
        p.slug = "custom-slug".to_string();
        assert_eq!(p.slug, "custom-slug");
    }

    #[test]
    fn test_slug_from_filename_overrides_name() {
        let md = "---\nname: My Project\nstatus: active\npriority: medium\ntags: []\ncreated: '2026-03-16'\n---\n";
        let p = Project::from_markdown_with_slug(md, Some("custom-filename-slug")).unwrap();
        assert_eq!(p.slug, "custom-filename-slug");
        assert_eq!(p.name, "My Project");
    }

    #[test]
    fn test_status_display() {
        assert_eq!(Status::Active.to_string(), "active");
        assert_eq!(Status::Blocked.to_string(), "blocked");
        assert_eq!(Status::Pending.to_string(), "pending");
        assert_eq!(Status::Parked.to_string(), "parked");
        assert_eq!(Status::Done.to_string(), "done");
    }

    #[test]
    fn test_status_parse() {
        assert_eq!("active".parse::<Status>(), Ok(Status::Active));
        assert_eq!("ACTIVE".parse::<Status>(), Ok(Status::Active));
        assert_eq!("blocked".parse::<Status>(), Ok(Status::Blocked));
        assert!("unknown".parse::<Status>().is_err());
    }

    #[test]
    fn test_priority_display() {
        assert_eq!(Priority::High.to_string(), "high");
        assert_eq!(Priority::Medium.to_string(), "medium");
        assert_eq!(Priority::Low.to_string(), "low");
    }

    #[test]
    fn test_priority_parse() {
        assert_eq!("high".parse::<Priority>(), Ok(Priority::High));
        assert_eq!("HIGH".parse::<Priority>(), Ok(Priority::High));
        assert_eq!("medium".parse::<Priority>(), Ok(Priority::Medium));
        assert!("unknown".parse::<Priority>().is_err());
    }

    #[test]
    fn test_yaml_parse_error_returns_err() {
        // Invalid YAML (unclosed sequence)
        let md = "---\nname: Test\nstatus: [unclosed\n---\n";
        // Should error rather than silently default
        assert!(Project::from_markdown(md).is_err());
    }
}
