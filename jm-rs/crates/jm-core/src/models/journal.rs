use chrono::NaiveDate;
use indexmap::IndexMap;
use regex::Regex;
use std::sync::LazyLock;

use super::project::parse_frontmatter;

static RE_JOURNAL_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{2}:\d{2})\s*[\u{2014}\u{2013}\-]+\s*(.*)").unwrap());
static RE_TYPE_PROJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\w+):\s*(.*)").unwrap());
static RE_KV_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z][A-Za-z _]*?):\s*(.*)").unwrap());

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    pub time: String,       // "09:15"
    pub entry_type: String, // "Started" | "Switched" | "Note" | "Done"
    pub project: String,    // project name (or "" for Done)
    pub details: IndexMap<String, String>,
}

impl JournalEntry {
    pub fn new(time: &str, entry_type: &str, project: &str) -> Self {
        Self {
            time: time.to_string(),
            entry_type: entry_type.to_string(),
            project: project.to_string(),
            details: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyJournal {
    pub date: NaiveDate,
    pub entries: Vec<JournalEntry>,
}

impl DailyJournal {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            entries: Vec::new(),
        }
    }

    pub fn append_entry(&mut self, entry: JournalEntry) {
        self.entries.push(entry);
    }

    /// Serialize to markdown with YAML frontmatter.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();

        // Frontmatter
        out.push_str("---\n");
        out.push_str(&format!("date: '{}'\n", self.date));
        out.push_str("---");

        // Entries
        let mut sections: Vec<String> = Vec::new();
        for entry in &self.entries {
            let header = if entry.entry_type == "Done" {
                format!("## {} \u{2014} Done for day", entry.time)
            } else if entry.entry_type == "Switched" && entry.project.contains("\u{2009}\u{2192}\u{2009}") {
                format!("## {} \u{2014} Switched: {}", entry.time, entry.project)
            } else if entry.entry_type == "Done" {
                format!("## {} \u{2014} Done for day", entry.time)
            } else {
                format!(
                    "## {} \u{2014} {}: {}",
                    entry.time, entry.entry_type, entry.project
                )
            };

            let mut lines = vec![header];

            // Detail lines in stable key order
            let preferred_order = ["focus", "left_off", "blocker", "next_step", "decision", "active", "shipped", "tomorrow"];
            let mut ordered_keys: Vec<&str> = Vec::new();
            for &k in &preferred_order {
                if entry.details.contains_key(k) {
                    ordered_keys.push(k);
                }
            }
            for k in entry.details.keys() {
                if !preferred_order.contains(&k.as_str()) {
                    ordered_keys.push(k);
                }
            }

            for key in ordered_keys {
                let value = &entry.details[key];
                let display_key = display_key(key);
                lines.push(format!("{display_key}: {value}"));
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
    pub fn from_markdown(text: &str) -> anyhow::Result<Self> {
        let (meta, body) = parse_frontmatter(text)?;

        // Parse date
        let journal_date = meta_date_or_today(&meta, "date");

        let mut entries: Vec<JournalEntry> = Vec::new();
        let mut current_header: Option<String> = None;
        let mut current_lines: Vec<String> = Vec::new();

        for line in body.split('\n') {
            if let Some(header_text) = line.strip_prefix("## ") {
                if let Some(header) = current_header.take() {
                    if let Some(entry) = parse_journal_entry(&header, &current_lines) {
                        entries.push(entry);
                    }
                }
                current_header = Some(header_text.trim().to_string());
                current_lines.clear();
            } else {
                current_lines.push(line.to_string());
            }
        }

        // Process last entry
        if let Some(header) = current_header.take() {
            if let Some(entry) = parse_journal_entry(&header, &current_lines) {
                entries.push(entry);
            }
        }

        Ok(DailyJournal {
            date: journal_date,
            entries,
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn display_key(key: &str) -> String {
    match key {
        "left_off" => "Left off".to_string(),
        "next_step" => "Next step".to_string(),
        "shipped" => "Shipped".to_string(),
        "tomorrow" => "Tomorrow".to_string(),
        _ => {
            let s = key.replace('_', " ");
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let first = c.to_uppercase().to_string();
                    first + chars.as_str()
                }
            }
        }
    }
}

fn normalize_key(raw: &str) -> String {
    raw.trim().to_lowercase().replace(' ', "_")
}

fn parse_journal_entry(header: &str, detail_lines: &[String]) -> Option<JournalEntry> {
    let caps = RE_JOURNAL_HEADER.captures(header)?;
    let time = caps[1].to_string();
    let rest = caps[2].trim().to_string();

    let (entry_type, project) = if rest.to_lowercase().starts_with("done") {
        ("Done".to_string(), String::new())
    } else {
        let type_caps = RE_TYPE_PROJECT.captures(&rest)?;
        (type_caps[1].to_string(), type_caps[2].trim().to_string())
    };

    let mut details = IndexMap::new();
    for line in detail_lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if let Some(caps) = RE_KV_LINE.captures(stripped) {
            let raw_key = caps[1].trim().to_string();
            let value = caps[2].trim().to_string();
            let key = normalize_key(&raw_key);
            details.insert(key, value);
        }
    }

    Some(JournalEntry {
        time,
        entry_type,
        project,
        details,
    })
}

fn meta_date_or_today(meta: &serde_yml::Value, key: &str) -> NaiveDate {
    let s = meta.get(key).and_then(|v| match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        _ => None,
    });
    match s {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::Local::now().date_naive()),
        None => chrono::Local::now().date_naive(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_full_journal() -> DailyJournal {
        let mut e1 = JournalEntry::new("09:15", "Started", "HMI Framework");
        e1.details.insert("focus".to_string(), "debugging render loop".to_string());

        let mut e2 = JournalEntry::new("11:30", "Switched", "HMI Framework \u{2192} Test Infra");
        e2.details.insert("left_off".to_string(), "checking vsync timing".to_string());
        e2.details.insert("blocker".to_string(), "waiting on @carol for display spec".to_string());
        e2.details.insert("next_step".to_string(), "read compositor docs".to_string());

        let mut e3 = JournalEntry::new("14:00", "Note", "Test Infra");
        e3.details.insert("decision".to_string(), "keeping pytest over unittest".to_string());

        let mut e4 = JournalEntry::new("16:30", "Done", "");
        e4.details.insert("active".to_string(), "Test Infra, HMI Framework (parked on blocker)".to_string());

        DailyJournal {
            date: d(2026, 3, 16),
            entries: vec![e1, e2, e3, e4],
        }
    }

    #[test]
    fn test_full_round_trip() {
        let original = make_full_journal();
        let md = original.to_markdown();
        let restored = DailyJournal::from_markdown(&md).unwrap();

        assert_eq!(restored.date, original.date);
        assert_eq!(restored.entries.len(), original.entries.len());

        for (orig, rest) in original.entries.iter().zip(restored.entries.iter()) {
            assert_eq!(rest.time, orig.time);
            assert_eq!(rest.entry_type, orig.entry_type);
            assert_eq!(rest.project, orig.project);
            assert_eq!(rest.details, orig.details);
        }
    }

    #[test]
    fn test_double_round_trip_stable() {
        let original = make_full_journal();
        let md1 = original.to_markdown();
        let restored = DailyJournal::from_markdown(&md1).unwrap();
        let md2 = restored.to_markdown();
        assert_eq!(md1, md2);
    }

    #[test]
    fn test_empty_journal() {
        let j = DailyJournal::new(d(2026, 3, 16));
        let md = j.to_markdown();
        let restored = DailyJournal::from_markdown(&md).unwrap();
        assert_eq!(restored.date, d(2026, 3, 16));
        assert!(restored.entries.is_empty());
    }

    #[test]
    fn test_append_entry() {
        let mut j = DailyJournal::new(d(2026, 3, 16));
        assert!(j.entries.is_empty());

        j.append_entry(JournalEntry::new("10:00", "Started", "Foo"));
        assert_eq!(j.entries.len(), 1);

        let mut e2 = JournalEntry::new("12:00", "Note", "Foo");
        e2.details.insert("decision".to_string(), "yes".to_string());
        j.append_entry(e2);
        assert_eq!(j.entries.len(), 2);

        let md = j.to_markdown();
        let restored = DailyJournal::from_markdown(&md).unwrap();
        assert_eq!(restored.entries.len(), 2);
        assert_eq!(restored.entries[0].project, "Foo");
        assert_eq!(restored.entries[1].details["decision"], "yes");
    }

    #[test]
    fn test_done_entry() {
        let mut e = JournalEntry::new("17:00", "Done", "");
        e.details.insert("active".to_string(), "Project A, Project B".to_string());

        let j = DailyJournal {
            date: d(2026, 3, 16),
            entries: vec![e],
        };
        let md = j.to_markdown();
        assert!(md.contains("Done for day"));
        let restored = DailyJournal::from_markdown(&md).unwrap();
        assert_eq!(restored.entries[0].entry_type, "Done");
        assert_eq!(restored.entries[0].project, "");
        assert_eq!(restored.entries[0].details["active"], "Project A, Project B");
    }

    #[test]
    fn test_started_entry_with_focus() {
        let mut e = JournalEntry::new("09:00", "Started", "My Project");
        e.details.insert("focus".to_string(), "writing tests".to_string());

        let j = DailyJournal {
            date: d(2026, 3, 16),
            entries: vec![e],
        };
        let md = j.to_markdown();
        let restored = DailyJournal::from_markdown(&md).unwrap();
        let e = &restored.entries[0];
        assert_eq!(e.time, "09:00");
        assert_eq!(e.entry_type, "Started");
        assert_eq!(e.project, "My Project");
        assert_eq!(e.details["focus"], "writing tests");
    }

    #[test]
    fn test_entry_no_details() {
        let j = DailyJournal {
            date: d(2026, 3, 16),
            entries: vec![JournalEntry::new("10:00", "Note", "Proj")],
        };
        let md = j.to_markdown();
        let restored = DailyJournal::from_markdown(&md).unwrap();
        assert!(restored.entries[0].details.is_empty());
    }
}
