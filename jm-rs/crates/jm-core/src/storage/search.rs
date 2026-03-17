use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use regex::Regex;

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: PathBuf,
    pub file_type: String, // "project" | "journal" | "people"
    pub project_slug: String,
    pub line_number: usize,
    pub line_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub query: String,
    pub project: Option<String>,
    pub person: Option<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub file_type: Option<String>,
    pub case_sensitive: bool,
}

// ── SearchEngine ────────────────────────────────────────────────────

pub struct SearchEngine {
    data_dir: PathBuf,
}

impl SearchEngine {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn search(&self, filter: &SearchFilter) -> Vec<SearchResult> {
        let files = self.collect_files(filter);
        let mut results = Vec::new();

        for file_path in files {
            results.extend(self.search_file(&file_path, filter));
        }

        results
    }

    pub fn quick_search(&self, query: &str) -> Vec<SearchResult> {
        self.search(&SearchFilter {
            query: query.to_string(),
            ..Default::default()
        })
    }

    fn collect_files(&self, filter: &SearchFilter) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();

        let projects_dir = self.data_dir.join("projects");
        let journal_dir = self.data_dir.join("journal");
        let people_file = self.data_dir.join("people.md");

        // Project files
        if filter.file_type.is_none() || filter.file_type.as_deref() == Some("project") {
            if projects_dir.exists() {
                let mut project_files: Vec<PathBuf> = fs::read_dir(&projects_dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
                    .collect();
                project_files.sort();

                for f in project_files {
                    // Filter by project slug
                    if let Some(ref slug) = filter.project {
                        if f.file_stem().and_then(|s| s.to_str()) != Some(slug.as_str()) {
                            continue;
                        }
                    }

                    // Filter by status/tags requires parsing frontmatter
                    if filter.status.is_some() || !filter.tags.is_empty() {
                        if let Ok(text) = fs::read_to_string(&f) {
                            let meta = parse_yaml_frontmatter(&text);
                            if let Some(ref status) = filter.status {
                                if meta_str_val(&meta, "status").as_deref() != Some(status.as_str())
                                {
                                    continue;
                                }
                            }
                            if !filter.tags.is_empty() {
                                let file_tags = meta_list_val(&meta, "tags");
                                if !filter.tags.iter().any(|t| file_tags.contains(t)) {
                                    continue;
                                }
                            }
                        } else {
                            continue;
                        }
                    }

                    files.push(f);
                }
            }
        }

        // Journal files
        if filter.file_type.is_none() || filter.file_type.as_deref() == Some("journal") {
            if journal_dir.exists() {
                let mut journal_files: Vec<PathBuf> = fs::read_dir(&journal_dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
                    .collect();
                journal_files.sort();

                for f in journal_files {
                    if let Some(stem) = f.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(file_date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
                            if let Some(from) = filter.date_from {
                                if file_date < from {
                                    continue;
                                }
                            }
                            if let Some(to) = filter.date_to {
                                if file_date > to {
                                    continue;
                                }
                            }
                        }
                    }
                    files.push(f);
                }
            }
        }

        // People file
        if filter.file_type.is_none() || filter.file_type.as_deref() == Some("people") {
            if people_file.exists() {
                files.push(people_file);
            }
        }

        // Sort by modification time (newest first)
        files.sort_by(|a, b| {
            let ma = a
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            let mb = b
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            mb.cmp(&ma)
        });

        files
    }

    fn search_file(&self, file_path: &Path, filter: &SearchFilter) -> Vec<SearchResult> {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let lines: Vec<&str> = content.split('\n').collect();
        let file_type = self.get_file_type(file_path);
        let project_slug = if file_type == "project" {
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        if filter.query.is_empty() && filter.person.is_none() {
            return Vec::new();
        }

        let mut patterns: Vec<Regex> = Vec::new();

        if !filter.query.is_empty() {
            let pattern = if filter.case_sensitive {
                Regex::new(&filter.query)
                    .unwrap_or_else(|_| Regex::new(&regex::escape(&filter.query)).unwrap())
            } else {
                Regex::new(&format!("(?i){}", &filter.query))
                    .unwrap_or_else(|_| {
                        Regex::new(&format!("(?i){}", regex::escape(&filter.query))).unwrap()
                    })
            };
            patterns.push(pattern);
        }

        if let Some(ref person) = filter.person {
            let pattern = Regex::new(&format!("(?i){}", regex::escape(person))).unwrap();
            patterns.push(pattern);
        }

        let mut results = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            for pattern in &patterns {
                if let Some(m) = pattern.find(line) {
                    let context_before: Vec<String> = lines
                        [i.saturating_sub(2)..i]
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    let context_after: Vec<String> = lines
                        [i + 1..std::cmp::min(lines.len(), i + 3)]
                        .iter()
                        .map(|s| s.to_string())
                        .collect();

                    results.push(SearchResult {
                        file_path: file_path.to_path_buf(),
                        file_type: file_type.clone(),
                        project_slug: project_slug.clone(),
                        line_number: i + 1,
                        line_text: line.to_string(),
                        context_before,
                        context_after,
                        match_start: m.start(),
                        match_end: m.end(),
                    });
                    break; // Don't double-count
                }
            }
        }

        results
    }

    fn get_file_type(&self, file_path: &Path) -> String {
        if file_path.starts_with(self.data_dir.join("projects")) {
            "project".to_string()
        } else if file_path.starts_with(self.data_dir.join("journal")) {
            "journal".to_string()
        } else if file_path.file_name().and_then(|f| f.to_str()) == Some("people.md") {
            "people".to_string()
        } else {
            "unknown".to_string()
        }
    }
}

// ── YAML frontmatter helpers (lightweight, no full model parse) ─────

fn parse_yaml_frontmatter(text: &str) -> serde_yml::Value {
    let text = text.trim();
    if !text.starts_with("---") {
        return serde_yml::Value::Mapping(Default::default());
    }
    let after_first = &text[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);
    if let Some(end_pos) = after_first.find("\n---") {
        let yaml_str = &after_first[..end_pos];
        serde_yml::from_str(yaml_str).unwrap_or_default()
    } else {
        serde_yml::Value::Mapping(Default::default())
    }
}

fn meta_str_val(meta: &serde_yml::Value, key: &str) -> Option<String> {
    meta.get(key).and_then(|v| match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn meta_list_val(meta: &serde_yml::Value, key: &str) -> Vec<String> {
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Project;
    use crate::storage::store::ProjectStore;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().to_path_buf();
        fs::create_dir_all(data_dir.join("projects")).unwrap();
        fs::create_dir_all(data_dir.join("journal")).unwrap();
        (tmp, data_dir)
    }

    #[test]
    fn test_search_text() {
        let (_tmp, data_dir) = setup();
        let ps = ProjectStore::new(&data_dir);

        let mut p = Project::new("Test Project");
        p.current_focus = "debugging render loop".to_string();
        ps.save_project(&mut p).unwrap();

        let engine = SearchEngine::new(&data_dir);
        let results = engine.quick_search("render");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.line_text.contains("render")));
    }

    #[test]
    fn test_search_case_insensitive() {
        let (_tmp, data_dir) = setup();
        let ps = ProjectStore::new(&data_dir);

        let mut p = Project::new("Test");
        p.current_focus = "Debugging Render Loop".to_string();
        ps.save_project(&mut p).unwrap();

        let engine = SearchEngine::new(&data_dir);
        let results = engine.search(&SearchFilter {
            query: "debugging".to_string(),
            ..Default::default()
        });
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_by_person() {
        let (_tmp, data_dir) = setup();

        // Write a people file
        fs::write(
            data_dir.join("people.md"),
            "## @carol\n- Role: Lead\n- Pending: spec review\n",
        )
        .unwrap();

        let engine = SearchEngine::new(&data_dir);
        let results = engine.search(&SearchFilter {
            person: Some("@carol".to_string()),
            ..Default::default()
        });
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_empty_query() {
        let (_tmp, data_dir) = setup();
        let engine = SearchEngine::new(&data_dir);
        let results = engine.search(&SearchFilter::default());
        assert!(results.is_empty());
    }
}
