use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Days, Local, NaiveDate};

use crate::models::{DailyJournal, Inbox, Issue, IssueFile, IssueStatus, JournalEntry, PeopleFile, Person, Project};

// ── Atomic write ────────────────────────────────────────────────────

fn atomic_write(path: &Path, content: &str) -> anyhow::Result<()> {
    let tmp_path = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    match fs::write(&tmp_path, content) {
        Ok(()) => {
            if let Err(e) = fs::rename(&tmp_path, path) {
                let _ = fs::remove_file(&tmp_path);
                return Err(e.into());
            }
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(e.into())
        }
    }
}

// ── ProjectStore ────────────────────────────────────────────────────

pub struct ProjectStore {
    pub projects_dir: PathBuf,
}

impl ProjectStore {
    pub fn new(data_dir: &Path) -> Self {
        let projects_dir = data_dir.join("projects");
        fs::create_dir_all(&projects_dir).ok();
        Self { projects_dir }
    }

    /// List all projects, optionally filtered by status.
    /// Returns projects sorted by file modification time (newest first).
    pub fn list_projects(&self, status: Option<&str>) -> Vec<Project> {
        let mut projects: Vec<(std::time::SystemTime, Project)> = Vec::new();

        let entries = match fs::read_dir(&self.projects_dir) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let slug = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let project = match Project::from_markdown_with_slug(&text, Some(&slug)) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Warning: skipping {slug}.md — {e}");
                    continue;
                }
            };
            if let Some(s) = status {
                if project.status.to_string() != s {
                    continue;
                }
            }
            let mtime = path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            projects.push((mtime, project));
        }

        projects.sort_by(|a, b| b.0.cmp(&a.0));
        projects.into_iter().map(|(_, p)| p).collect()
    }

    /// Get a single project by slug.
    pub fn get_project(&self, slug: &str) -> Option<Project> {
        let path = self.projects_dir.join(format!("{slug}.md"));
        if !path.exists() {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        match Project::from_markdown_with_slug(&text, Some(slug)) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("Warning: could not parse {slug}.md — {e}");
                None
            }
        }
    }

    /// Save project to disk atomically.
    /// Auto-adjusts status between active/blocked based on open blockers.
    pub fn save_project(&self, project: &mut Project) -> anyhow::Result<PathBuf> {
        use crate::models::Status;
        let has_open_blockers = project.blockers.iter().any(|b| !b.resolved);
        if has_open_blockers && project.status == Status::Active {
            project.status = Status::Blocked;
        } else if !has_open_blockers && project.status == Status::Blocked {
            project.status = Status::Active;
        }

        let path = self.projects_dir.join(format!("{}.md", project.slug));
        atomic_write(&path, &project.to_markdown())?;
        Ok(path)
    }

    /// Save project without auto-status adjustment.
    /// Used when the user explicitly sets a status (e.g. cycling with S key).
    pub fn save_project_raw(&self, project: &Project) -> anyhow::Result<PathBuf> {
        let path = self.projects_dir.join(format!("{}.md", project.slug));
        atomic_write(&path, &project.to_markdown())?;
        Ok(path)
    }

    /// Create a new project with defaults and save it.
    pub fn create_project(&self, name: &str) -> anyhow::Result<Project> {
        let mut project = Project::new(name);
        self.save_project(&mut project)?;
        Ok(project)
    }

    /// Create a new project with custom fields and save it.
    pub fn create_project_with(
        &self,
        name: &str,
        status: &str,
        priority: &str,
        tags: Vec<String>,
    ) -> anyhow::Result<Project> {
        use crate::models::{Priority, Status};
        let mut project = Project::new(name);
        project.status = status.parse::<Status>().unwrap_or_else(|_| {
            eprintln!("Warning: unknown status '{status}', defaulting to 'active'");
            Status::Active
        });
        project.priority = priority.parse::<Priority>().unwrap_or_else(|_| {
            eprintln!("Warning: unknown priority '{priority}', defaulting to 'medium'");
            Priority::Medium
        });
        project.tags = tags;
        self.save_project_raw(&project)?;
        Ok(project)
    }

    /// Delete a project file.
    pub fn delete_project(&self, slug: &str) -> bool {
        let path = self.projects_dir.join(format!("{slug}.md"));
        if !path.exists() {
            return false;
        }
        fs::remove_file(path).is_ok()
    }
}

// ── JournalStore ────────────────────────────────────────────────────

pub struct JournalStore {
    pub journal_dir: PathBuf,
}

impl JournalStore {
    pub fn new(data_dir: &Path) -> Self {
        let journal_dir = data_dir.join("journal");
        fs::create_dir_all(&journal_dir).ok();
        Self { journal_dir }
    }

    fn path_for_date(&self, date: NaiveDate) -> PathBuf {
        self.journal_dir.join(format!("{date}.md"))
    }

    /// Get today's journal, creating a new one if it doesn't exist.
    pub fn today(&self) -> DailyJournal {
        let today = Local::now().date_naive();
        self.get_day(today)
            .unwrap_or_else(|| DailyJournal::new(today))
    }

    /// Append an entry to today's journal and save it.
    pub fn append(&self, entry: JournalEntry) -> anyhow::Result<DailyJournal> {
        let mut journal = self.today();
        journal.append_entry(entry);
        self.save(&journal)?;
        Ok(journal)
    }

    /// Get journal for a specific date.
    pub fn get_day(&self, date: NaiveDate) -> Option<DailyJournal> {
        let path = self.path_for_date(date);
        if !path.exists() {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        DailyJournal::from_markdown(&text).ok()
    }

    /// Get the most recent journal before target_date (up to 14 days back).
    pub fn get_previous_workday(&self, target_date: Option<NaiveDate>) -> Option<DailyJournal> {
        let start = target_date.unwrap_or_else(|| Local::now().date_naive());
        for days_back in 1..=14 {
            let check_date = start.checked_sub_days(Days::new(days_back))?;
            if let Some(journal) = self.get_day(check_date) {
                return Some(journal);
            }
        }
        None
    }

    /// Save a journal to disk atomically.
    pub fn save(&self, journal: &DailyJournal) -> anyhow::Result<PathBuf> {
        let path = self.path_for_date(journal.date);
        atomic_write(&path, &journal.to_markdown())?;
        Ok(path)
    }
}

// ── PeopleStore ─────────────────────────────────────────────────────

pub struct PeopleStore {
    pub people_file: PathBuf,
}

impl PeopleStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            people_file: data_dir.join("people.md"),
        }
    }

    /// Load people file. Returns empty PeopleFile if not found.
    pub fn load(&self) -> PeopleFile {
        if !self.people_file.exists() {
            return PeopleFile::new();
        }
        match fs::read_to_string(&self.people_file) {
            Ok(text) => PeopleFile::from_markdown(&text),
            Err(_) => PeopleFile::new(),
        }
    }

    /// Save people file atomically.
    pub fn save(&self, people: &PeopleFile) -> anyhow::Result<PathBuf> {
        atomic_write(&self.people_file, &people.to_markdown())?;
        Ok(self.people_file.clone())
    }

    /// Get a person by handle (e.g., "@carol").
    pub fn get_person(&self, handle: &str) -> Option<Person> {
        let people = self.load();
        people
            .people
            .into_iter()
            .find(|p| p.handle == handle)
    }

    /// Add or update a person. Merges records if handle exists.
    pub fn add_or_update_person(&self, person: Person) -> anyhow::Result<PeopleFile> {
        let mut people = self.load();

        if let Some(existing) = people.people.iter_mut().find(|p| p.handle == person.handle) {
            // Preserve existing role unless new one is non-empty
            if !person.role.is_empty() {
                existing.role = person.role;
            }
            // Union projects lists (no duplicates)
            for proj in person.projects {
                if !existing.projects.contains(&proj) {
                    existing.projects.push(proj);
                }
            }
            // Append new pending items (avoid duplicates by description)
            let existing_descriptions: std::collections::HashSet<String> =
                existing.pending.iter().map(|p| p.description.clone()).collect();
            for item in person.pending {
                if !existing_descriptions.contains(&item.description) {
                    existing.pending.push(item);
                }
            }
        } else {
            people.people.push(person);
        }

        self.save(&people)?;
        Ok(people)
    }
}

// ── ActiveProjectStore ──────────────────────────────────────────────

pub struct ActiveProjectStore {
    pub active_file: PathBuf,
}

impl ActiveProjectStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            active_file: data_dir.join(".active"),
        }
    }

    /// Get the active project slug.
    pub fn get_active(&self) -> Option<String> {
        if !self.active_file.exists() {
            return None;
        }
        let slug = fs::read_to_string(&self.active_file).ok()?.trim().to_string();
        if slug.is_empty() {
            None
        } else {
            Some(slug)
        }
    }

    /// Set the active project slug.
    pub fn set_active(&self, slug: &str) -> anyhow::Result<()> {
        atomic_write(&self.active_file, slug)
    }

    /// Clear the active project.
    pub fn clear_active(&self) {
        let _ = fs::remove_file(&self.active_file);
    }
}

// ── InboxStore ──────────────────────────────────────────────────────

pub struct InboxStore {
    pub inbox_file: PathBuf,
}

impl InboxStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            inbox_file: data_dir.join("inbox.md"),
        }
    }

    /// Load the inbox. Returns empty Inbox if file doesn't exist.
    pub fn load(&self) -> Inbox {
        if !self.inbox_file.exists() {
            return Inbox::new();
        }
        match fs::read_to_string(&self.inbox_file) {
            Ok(text) => Inbox::from_markdown(&text),
            Err(_) => Inbox::new(),
        }
    }

    /// Save inbox atomically.
    pub fn save(&self, inbox: &Inbox) -> anyhow::Result<PathBuf> {
        atomic_write(&self.inbox_file, &inbox.to_markdown())?;
        Ok(self.inbox_file.clone())
    }

    /// Append a new item and save.
    pub fn append(&self, text: &str) -> anyhow::Result<()> {
        let mut inbox = self.load();
        inbox.items.push(Inbox::capture(text));
        self.save(&inbox)?;
        Ok(())
    }

    /// Mark item at index as refiled to a project slug.
    pub fn refile(&self, index: usize, slug: &str) -> anyhow::Result<()> {
        let mut inbox = self.load();
        if let Some(item) = inbox.items.get_mut(index) {
            item.refiled_to = Some(slug.to_string());
        }
        self.save(&inbox)?;
        Ok(())
    }

    /// Delete item at index.
    pub fn delete(&self, index: usize) -> anyhow::Result<()> {
        let mut inbox = self.load();
        if index < inbox.items.len() {
            inbox.items.remove(index);
        }
        self.save(&inbox)?;
        Ok(())
    }
}

// ── IssueStore ──────────────────────────────────────────────────────

pub struct IssueStore {
    pub issues_dir: PathBuf,
}

impl IssueStore {
    pub fn new(data_dir: &Path) -> Self {
        let issues_dir = data_dir.join("issues");
        fs::create_dir_all(&issues_dir).ok();
        Self { issues_dir }
    }

    fn path(&self, slug: &str) -> PathBuf {
        self.issues_dir.join(format!("{slug}.md"))
    }

    /// Load issues for a project. Returns empty IssueFile if file not found.
    pub fn load(&self, slug: &str) -> IssueFile {
        let path = self.path(slug);
        if !path.exists() {
            return IssueFile::new(slug);
        }
        match fs::read_to_string(&path) {
            Ok(text) => IssueFile::from_markdown(&text).unwrap_or_else(|_| IssueFile::new(slug)),
            Err(_) => IssueFile::new(slug),
        }
    }

    /// Save an issue file atomically. Returns the file path.
    pub fn save(&self, issue_file: &IssueFile) -> anyhow::Result<PathBuf> {
        let path = self.path(&issue_file.project_slug);
        atomic_write(&path, &issue_file.to_markdown())?;
        Ok(path)
    }

    /// Load all issue files across all projects. Returns (slug, IssueFile) pairs.
    pub fn load_all(&self) -> Vec<IssueFile> {
        let Ok(entries) = fs::read_dir(&self.issues_dir) else {
            return Vec::new();
        };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(issue_file) = IssueFile::from_markdown(&text) {
                        files.push(issue_file);
                    }
                }
            }
        }
        files
    }

    /// Get a single issue by project slug and issue ID.
    pub fn get_issue(&self, slug: &str, issue_id: u32) -> Option<Issue> {
        let issue_file = self.load(slug);
        issue_file.issues.into_iter().find(|i| i.id == issue_id)
    }

    /// Create a new issue, assign next_id, save, and return it.
    pub fn create_issue(
        &self,
        slug: &str,
        title: &str,
        parent_id: Option<u32>,
    ) -> anyhow::Result<Issue> {
        let mut issue_file = self.load(slug);
        let issue = Issue {
            id: issue_file.next_id,
            title: title.to_string(),
            status: IssueStatus::Todo,
            parent_id,
            created: chrono::Local::now().date_naive(),
            closed: None,
            notes: String::new(),
            r#ref: String::new(),
        };
        issue_file.next_id += 1;
        issue_file.issues.push(issue.clone());
        self.save(&issue_file)?;
        Ok(issue)
    }

    /// Delete an issue by ID. Also removes sub-issues. Returns true if found.
    pub fn delete_issue(&self, slug: &str, issue_id: u32) -> anyhow::Result<bool> {
        let mut issue_file = self.load(slug);
        let original_len = issue_file.issues.len();
        issue_file
            .issues
            .retain(|i| i.id != issue_id && i.parent_id != Some(issue_id));
        if issue_file.issues.len() < original_len {
            self.save(&issue_file)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Update an issue's status. Sets/clears closed date as appropriate.
    pub fn set_status(
        &self,
        slug: &str,
        issue_id: u32,
        new_status: IssueStatus,
    ) -> anyhow::Result<bool> {
        let mut issue_file = self.load(slug);
        let found = issue_file.issues.iter_mut().find(|i| i.id == issue_id);
        if let Some(issue) = found {
            issue.status = new_status;
            if new_status == IssueStatus::Done {
                issue.closed = Some(chrono::Local::now().date_naive());
            } else {
                issue.closed = None;
            }
            self.save(&issue_file)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// ── LastReviewStore ──────────────────────────────────────────────────

/// Tracks the last date a morning review was completed.
/// Persisted to `~/.jm/.last_review` as a bare YYYY-MM-DD string.
pub struct LastReviewStore {
    pub file: PathBuf,
}

impl LastReviewStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            file: data_dir.join(".last_review"),
        }
    }

    /// Read the stored date. Returns `None` if the file is missing or unparseable.
    pub fn last_review_date(&self) -> Option<NaiveDate> {
        let text = fs::read_to_string(&self.file).ok()?;
        NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
    }

    /// Write today's date as the last-review date.
    pub fn mark_reviewed_today(&self) -> anyhow::Result<()> {
        let today = Local::now().date_naive().to_string();
        atomic_write(&self.file, &today)
    }
}

// ── Factory ─────────────────────────────────────────────────────────

/// Create all stores from a data directory.
pub fn create_stores(
    data_dir: &Path,
) -> (ProjectStore, JournalStore, PeopleStore, ActiveProjectStore) {
    (
        ProjectStore::new(data_dir),
        JournalStore::new(data_dir),
        PeopleStore::new(data_dir),
        ActiveProjectStore::new(data_dir),
    )
}

/// Create all stores including InboxStore.
pub fn create_all_stores(
    data_dir: &Path,
) -> (
    ProjectStore,
    JournalStore,
    PeopleStore,
    ActiveProjectStore,
    InboxStore,
) {
    (
        ProjectStore::new(data_dir),
        JournalStore::new(data_dir),
        PeopleStore::new(data_dir),
        ActiveProjectStore::new(data_dir),
        InboxStore::new(data_dir),
    )
}

// ── Stores struct ───────────────────────────────────────────────────

/// A named collection of all persistent stores for the jm data directory.
///
/// Prefer this over the tuple-returning factory functions for new callers.
/// Existing callers using `create_stores` / `create_all_stores` can migrate
/// incrementally.
pub struct Stores {
    pub projects: ProjectStore,
    pub journal: JournalStore,
    pub people: PeopleStore,
    pub active: ActiveProjectStore,
    pub inbox: InboxStore,
    pub issues: IssueStore,
}

impl Stores {
    /// Open (and create if necessary) all stores rooted at `data_dir`.
    pub fn open(data_dir: &Path) -> Self {
        Self {
            projects: ProjectStore::new(data_dir),
            journal: JournalStore::new(data_dir),
            people: PeopleStore::new(data_dir),
            active: ActiveProjectStore::new(data_dir),
            inbox: InboxStore::new(data_dir),
            issues: IssueStore::new(data_dir),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Blocker, PendingItem};
    use tempfile::TempDir;

    fn setup() -> (TempDir, ProjectStore, JournalStore, PeopleStore, ActiveProjectStore) {
        let tmp = TempDir::new().unwrap();
        let (ps, js, pps, as_) = create_stores(tmp.path());
        (tmp, ps, js, pps, as_)
    }

    // ── ProjectStore tests ──────────────────────────────────────────

    #[test]
    fn test_create_and_get_project() {
        let (_tmp, ps, _, _, _) = setup();
        let project = ps.create_project("Test Project").unwrap();
        assert_eq!(project.slug, "test-project");

        let loaded = ps.get_project("test-project").unwrap();
        assert_eq!(loaded.name, "Test Project");
        assert_eq!(loaded.status, crate::models::Status::Active);
    }

    #[test]
    fn test_list_projects() {
        let (_tmp, ps, _, _, _) = setup();
        ps.create_project("Project A").unwrap();
        ps.create_project("Project B").unwrap();

        let all = ps.list_projects(None);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_list_projects_filter_status() {
        let (_tmp, ps, _, _, _) = setup();
        ps.create_project_with("Active One", "active", "medium", Vec::new())
            .unwrap();
        ps.create_project_with("Parked One", "parked", "low", Vec::new())
            .unwrap();

        let active = ps.list_projects(Some("active"));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Active One");

        let parked = ps.list_projects(Some("parked"));
        assert_eq!(parked.len(), 1);
        assert_eq!(parked[0].name, "Parked One");
    }

    #[test]
    fn test_delete_project() {
        let (_tmp, ps, _, _, _) = setup();
        ps.create_project("To Delete").unwrap();
        assert!(ps.get_project("to-delete").is_some());

        assert!(ps.delete_project("to-delete"));
        assert!(ps.get_project("to-delete").is_none());
        assert!(!ps.delete_project("to-delete")); // already gone
    }

    #[test]
    fn test_save_project_auto_status() {
        let (_tmp, ps, _, _, _) = setup();
        let mut project = ps.create_project("Auto Status").unwrap();
        assert_eq!(project.status, crate::models::Status::Active);

        // Add a blocker → should auto-set to blocked
        project.blockers.push(Blocker {
            description: "test blocker".to_string(),
            ..Default::default()
        });
        ps.save_project(&mut project).unwrap();
        assert_eq!(project.status, crate::models::Status::Blocked);

        // Resolve it → should revert to active
        project.blockers[0].resolved = true;
        ps.save_project(&mut project).unwrap();
        assert_eq!(project.status, crate::models::Status::Active);
    }

    // ── JournalStore tests ──────────────────────────────────────────

    #[test]
    fn test_journal_today() {
        let (_tmp, _, js, _, _) = setup();
        let journal = js.today();
        assert_eq!(journal.date, Local::now().date_naive());
        assert!(journal.entries.is_empty());
    }

    #[test]
    fn test_journal_append() {
        let (_tmp, _, js, _, _) = setup();
        let entry = JournalEntry::new("10:00", "Started", "Test");
        js.append(entry).unwrap();

        let journal = js.today();
        assert_eq!(journal.entries.len(), 1);
        assert_eq!(journal.entries[0].project, "Test");
    }

    #[test]
    fn test_journal_get_day() {
        let (_tmp, _, js, _, _) = setup();
        let date = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        assert!(js.get_day(date).is_none());

        let journal = DailyJournal::new(date);
        js.save(&journal).unwrap();
        assert!(js.get_day(date).is_some());
    }

    #[test]
    fn test_journal_previous_workday() {
        let (_tmp, _, js, _, _) = setup();
        let today = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap();
        // No previous days
        assert!(js.get_previous_workday(Some(today)).is_none());

        // Save journal for 2 days ago
        let two_days_ago = NaiveDate::from_ymd_opt(2026, 3, 14).unwrap();
        let journal = DailyJournal::new(two_days_ago);
        js.save(&journal).unwrap();

        let prev = js.get_previous_workday(Some(today)).unwrap();
        assert_eq!(prev.date, two_days_ago);
    }

    // ── PeopleStore tests ───────────────────────────────────────────

    #[test]
    fn test_people_load_empty() {
        let (_tmp, _, _, pps, _) = setup();
        let people = pps.load();
        assert!(people.people.is_empty());
    }

    #[test]
    fn test_people_add_and_get() {
        let (_tmp, _, _, pps, _) = setup();
        let person = Person {
            handle: "@carol".to_string(),
            role: "Lead".to_string(),
            projects: vec!["Proj A".to_string()],
            pending: Vec::new(),
        };
        pps.add_or_update_person(person).unwrap();

        let loaded = pps.get_person("@carol").unwrap();
        assert_eq!(loaded.role, "Lead");
        assert_eq!(loaded.projects, vec!["Proj A"]);
    }

    #[test]
    fn test_people_merge() {
        let (_tmp, _, _, pps, _) = setup();

        let p1 = Person {
            handle: "@carol".to_string(),
            role: "Lead".to_string(),
            projects: vec!["Proj A".to_string()],
            pending: vec![PendingItem {
                description: "item 1".to_string(),
                since: None,
                project: None,
            }],
        };
        pps.add_or_update_person(p1).unwrap();

        // Update with new data
        let p2 = Person {
            handle: "@carol".to_string(),
            role: String::new(), // empty role should preserve existing
            projects: vec!["Proj B".to_string()],
            pending: vec![
                PendingItem {
                    description: "item 1".to_string(), // duplicate
                    since: None,
                    project: None,
                },
                PendingItem {
                    description: "item 2".to_string(), // new
                    since: None,
                    project: None,
                },
            ],
        };
        pps.add_or_update_person(p2).unwrap();

        let loaded = pps.get_person("@carol").unwrap();
        assert_eq!(loaded.role, "Lead"); // preserved
        assert_eq!(loaded.projects, vec!["Proj A", "Proj B"]); // union
        assert_eq!(loaded.pending.len(), 2); // deduped
    }

    // ── IssueStore tests ─────────────────────────────────────────────

    #[test]
    fn test_issue_create_sequential_ids() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        let i1 = store.create_issue("proj", "First", None).unwrap();
        let i2 = store.create_issue("proj", "Second", None).unwrap();
        assert_eq!(i1.id, 1);
        assert_eq!(i2.id, 2);
    }

    #[test]
    fn test_issue_never_reuses_ids() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "One", None).unwrap();
        store.create_issue("proj", "Two", None).unwrap();
        store.delete_issue("proj", 2).unwrap();
        let i3 = store.create_issue("proj", "Three", None).unwrap();
        assert_eq!(i3.id, 3);
    }

    #[test]
    fn test_issue_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        let f = store.load("nonexistent");
        assert_eq!(f.project_slug, "nonexistent");
        assert_eq!(f.next_id, 1);
        assert!(f.issues.is_empty());
    }

    #[test]
    fn test_issue_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "Task A", None).unwrap();
        store.create_issue("proj", "Task B", None).unwrap();
        let loaded = store.load("proj");
        assert_eq!(loaded.issues.len(), 2);
        assert_eq!(loaded.issues[0].title, "Task A");
        assert_eq!(loaded.issues[1].title, "Task B");
        assert_eq!(loaded.next_id, 3);
    }

    #[test]
    fn test_issue_delete() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "Keep", None).unwrap();
        store.create_issue("proj", "Delete", None).unwrap();
        assert!(store.delete_issue("proj", 2).unwrap());
        assert!(!store.delete_issue("proj", 2).unwrap());
        let loaded = store.load("proj");
        assert_eq!(loaded.issues.len(), 1);
        assert_eq!(loaded.issues[0].title, "Keep");
    }

    #[test]
    fn test_issue_delete_cascades() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "Parent", None).unwrap();
        store.create_issue("proj", "Child", Some(1)).unwrap();
        assert!(store.delete_issue("proj", 1).unwrap());
        let loaded = store.load("proj");
        assert!(loaded.issues.is_empty());
    }

    #[test]
    fn test_issue_create_sub_issue() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        let parent = store.create_issue("proj", "Parent", None).unwrap();
        let child = store.create_issue("proj", "Child", Some(parent.id)).unwrap();
        assert_eq!(child.parent_id, Some(1));
        assert_eq!(child.id, 2);
    }

    #[test]
    fn test_issue_get() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "First", None).unwrap();
        store.create_issue("proj", "Second", None).unwrap();
        let issue = store.get_issue("proj", 2).unwrap();
        assert_eq!(issue.title, "Second");
        assert!(store.get_issue("proj", 99).is_none());
    }

    #[test]
    fn test_issue_set_status() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj", "Task", None).unwrap();
        store.set_status("proj", 1, crate::models::IssueStatus::Done).unwrap();
        let issue = store.get_issue("proj", 1).unwrap();
        assert_eq!(issue.status, crate::models::IssueStatus::Done);
        assert!(issue.closed.is_some());

        // Cycle back to todo clears closed
        store.set_status("proj", 1, crate::models::IssueStatus::Todo).unwrap();
        let issue = store.get_issue("proj", 1).unwrap();
        assert_eq!(issue.status, crate::models::IssueStatus::Todo);
        assert!(issue.closed.is_none());
    }

    #[test]
    fn test_issue_separate_projects() {
        let tmp = TempDir::new().unwrap();
        let store = IssueStore::new(tmp.path());
        store.create_issue("proj-a", "Issue A", None).unwrap();
        store.create_issue("proj-b", "Issue B", None).unwrap();
        assert_eq!(store.load("proj-a").issues.len(), 1);
        assert_eq!(store.load("proj-b").issues.len(), 1);
        assert_eq!(store.load("proj-a").issues[0].title, "Issue A");
    }

    // ── ActiveProjectStore tests ────────────────────────────────────

    #[test]
    fn test_active_store() {
        let (_tmp, _, _, _, as_) = setup();
        assert!(as_.get_active().is_none());

        as_.set_active("test-project").unwrap();
        assert_eq!(as_.get_active(), Some("test-project".to_string()));

        as_.clear_active();
        assert!(as_.get_active().is_none());
    }
}
