//! JIRA Cloud integration — full-screen plugin for viewing, transitioning,
//! editing, commenting on, and creating JIRA issues from within the TUI.
//!
//! Data always comes from the API; nothing is persisted locally.

pub(crate) mod adf;
pub(crate) mod api;
pub(crate) mod board;
pub(crate) mod config;
pub(crate) mod create;
pub(crate) mod detail;
pub(crate) mod models;

pub(crate) use config::JiraConfig;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::plugins::{PluginAction, ScreenPlugin};

use self::api::{JiraCommand, JiraResult};
use self::models::*;

// ── BoardState ───────────────────────────────────────────────────────────────

/// State for the horizontally-scrolling kanban board.
pub(crate) struct BoardState {
    pub columns: Vec<StatusColumn>,
    pub selected_col: usize,
    pub scroll_offset: usize,
    pub selected_row: usize,
    pub col_scroll_offsets: Vec<usize>,
}

impl Default for BoardState {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            selected_col: 0,
            scroll_offset: 0,
            selected_row: 0,
            col_scroll_offsets: Vec::new(),
        }
    }
}

/// A single column in the kanban board, representing one workflow status.
pub(crate) struct StatusColumn {
    pub name: String,
    pub category: StatusCategory,
    /// Indices into `JiraPlugin.issues` (filtered).
    pub issue_indices: Vec<usize>,
}

// ── FormState ────────────────────────────────────────────────────────────────

/// UI state for form modals (create issue, transition fields).
///
/// Tracks cursor position, edit state, and dropdown state. Actual field values
/// are stored in a parallel `Vec<(EditableField, Option<FieldValue>)>` that
/// lives in the `JiraModal` variant, NOT inside `FormState`.
pub(crate) enum FormState {
    /// Navigating between fields. j/k moves cursor, Enter enters edit mode.
    Navigating { cursor: usize },

    /// Editing a text or number field inline.
    EditingText {
        cursor: usize,
        buffer: String,
        cursor_pos: usize,
    },

    /// A select dropdown is open for the focused field.
    SelectOpen {
        field_cursor: usize,
        dropdown_cursor: usize,
    },

    /// A multi-select dropdown is open. Space toggles items, Enter confirms.
    MultiSelectOpen {
        field_cursor: usize,
        dropdown_cursor: usize,
        checked: HashSet<usize>,
    },

    /// Form submitted, waiting for API response.
    Submitting,

    /// API returned validation errors. Fields marked with errors.
    ValidationError {
        cursor: usize,
        errors: HashMap<String, String>,
    },
}

// ── FieldValue ───────────────────────────────────────────────────────────────

/// Represents a field value in the form. Used alongside `EditableField`
/// in a parallel Vec: `Vec<(EditableField, Option<FieldValue>)>`.
#[derive(Debug, Clone)]
pub(crate) enum FieldValue {
    /// Text or TextArea field value.
    Text(String),
    /// Number field value (story points, etc.).
    Number(f64),
    /// Single-select field value (stores the AllowedValue id).
    Select(String),
    /// Multi-select field values (stores AllowedValue ids).
    MultiSelect(Vec<String>),
    /// Date field value ("YYYY-MM-DD" format).
    Date(String),
}

// ── DetailFocus ──────────────────────────────────────────────────────────────

/// Which section of the detail modal has focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DetailFocus {
    Fields,
    Comments,
}

// ── DetailEditState ──────────────────────────────────────────────────────────

/// Edit state within the issue detail modal.
pub(crate) enum DetailEditState {
    EditingText {
        field_id: String,
        buffer: String,
        cursor_pos: usize,
    },
    SelectOpen {
        field_id: String,
        options: Vec<AllowedValue>,
        cursor: usize,
    },
}

// ── JiraModal ────────────────────────────────────────────────────────────────

/// Plugin-owned modal state. All modals are managed internally by the plugin --
/// they are NOT created via the App's modal system.
pub(crate) enum JiraModal {
    /// Full issue detail view with fields, transitions, and comments.
    IssueDetail {
        issue_key: String,
        fields: Option<Vec<EditableField>>,
        transitions: Option<Vec<JiraTransition>>,
        comments: Option<Vec<JiraComment>>,
        scroll_offset: usize,
        field_cursor: usize,
        focus: DetailFocus,
        edit_state: Option<DetailEditState>,
    },

    /// Transition picker — select from available transitions.
    TransitionPicker {
        issue_key: String,
        transitions: Vec<JiraTransition>,
        cursor: usize,
    },

    /// Transition with required fields — fill fields before executing.
    TransitionFields {
        issue_key: String,
        transition: JiraTransition,
        fields: Vec<(EditableField, Option<FieldValue>)>,
        form: FormState,
    },

    /// Step 1 of issue creation: select a project.
    SelectProject {
        projects: Vec<(String, String)>, // (key, name)
        cursor: usize,
    },

    /// Step 2 of issue creation: select an issue type.
    SelectIssueType {
        project_key: String,
        issue_types: Vec<JiraIssueType>,
        cursor: usize,
    },

    /// Step 3 of issue creation: fill in fields.
    CreateForm {
        project_key: String,
        issue_type_id: String,
        fields: Vec<(EditableField, Option<FieldValue>)>,
        form: FormState,
    },

    /// Error modal — blocking, must be dismissed before continuing.
    ErrorModal {
        title: String,
        message: String,
    },
}

// ── JiraPlugin ───────────────────────────────────────────────────────────────

/// The JIRA Cloud integration plugin implementing `ScreenPlugin`.
///
/// All fields are `pub(crate)` so sibling modules (`board.rs`, `detail.rs`,
/// `create.rs`) can access them.
pub struct JiraPlugin {
    // ── Config ────────────────────────────────────────────────────────────
    pub(crate) config: JiraConfig,
    pub(crate) account_id: Option<String>,

    // ── Background thread communication ──────────────────────────────────
    pub(crate) command_tx: Option<mpsc::Sender<JiraCommand>>,
    pub(crate) result_rx: Option<mpsc::Receiver<JiraResult>>,
    pub(crate) shutdown_flag: Option<Arc<AtomicBool>>,
    pub(crate) thread_handle: Option<std::thread::JoinHandle<()>>,

    // ── Data (from API, never persisted) ─────────────────────────────────
    pub(crate) issues: Vec<JiraIssue>,
    pub(crate) field_defs: Vec<JiraFieldDef>,
    pub(crate) story_points_field: Option<String>,
    pub(crate) sprint_field: Option<String>,

    // ── Board state ──────────────────────────────────────────────────────
    pub(crate) board: BoardState,
    pub(crate) project_filter: Option<String>,
    pub(crate) show_done: bool,

    // ── Modal state (plugin-owned modals) ────────────────────────────────
    pub(crate) modal: Option<JiraModal>,
    pub(crate) previous_modal: Option<JiraModal>,

    // ── Loading/refresh state ────────────────────────────────────────────
    pub(crate) loading: bool,
    pub(crate) refreshing: bool,
    pub(crate) generation: u64,
    pub(crate) last_sync: Option<Instant>,

    // ── Error state ──────────────────────────────────────────────────────
    pub(crate) last_error: Option<String>,

    // ── Toast queue ──────────────────────────────────────────────────────
    pub(crate) pending_toasts: Vec<String>,
}

impl JiraPlugin {
    /// Create a new `JiraPlugin` with the given configuration.
    ///
    /// All fields are initialized to defaults/None/empty. The background thread
    /// is NOT started here — it is spawned in `on_enter()`.
    pub fn new(config: JiraConfig) -> Self {
        let story_points_field = config.story_points_field.clone();
        let sprint_field = config.sprint_field.clone();

        Self {
            config,
            account_id: None,

            command_tx: None,
            result_rx: None,
            shutdown_flag: None,
            thread_handle: None,

            issues: Vec::new(),
            field_defs: Vec::new(),
            story_points_field,
            sprint_field,

            board: BoardState::default(),
            project_filter: None,
            show_done: false,

            modal: None,
            previous_modal: None,

            loading: false,
            refreshing: false,
            generation: 0,
            last_sync: None,

            last_error: None,

            pending_toasts: Vec::new(),
        }
    }

    /// Return issues filtered by `project_filter` and `show_done`.
    pub(crate) fn filtered_issues(&self) -> Vec<&JiraIssue> {
        self.issues
            .iter()
            .filter(|issue| {
                // Filter by project
                if let Some(ref filter) = self.project_filter {
                    if issue.project_key != *filter {
                        return false;
                    }
                }
                // Filter out Done if not showing
                if !self.show_done && issue.status.category == StatusCategory::Done {
                    return false;
                }
                true
            })
            .collect()
    }

    /// Rebuild the kanban board columns from the current filtered issues.
    ///
    /// Groups issues by their workflow status name, preserving the order
    /// encountered in the issue list. Updates `self.board.columns` and
    /// ensures `col_scroll_offsets` is the right length.
    pub(crate) fn rebuild_columns(&mut self) {
        let filtered = self.filtered_issues();

        // Collect unique statuses in the order they appear
        let mut status_order: Vec<(String, StatusCategory)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for issue in &filtered {
            if seen.insert(issue.status.name.clone()) {
                status_order.push((issue.status.name.clone(), issue.status.category.clone()));
            }
        }

        // Sort columns by category: ToDo, InProgress, Done
        status_order.sort_by_key(|(_, cat)| match cat {
            StatusCategory::ToDo => 0,
            StatusCategory::InProgress => 1,
            StatusCategory::Done => 2,
        });

        // Build columns with issue indices
        let columns: Vec<StatusColumn> = status_order
            .into_iter()
            .map(|(name, category)| {
                let issue_indices: Vec<usize> = filtered
                    .iter()
                    .enumerate()
                    .filter(|(_, issue)| issue.status.name == name)
                    .map(|(i, _)| i)
                    .collect();
                StatusColumn {
                    name,
                    category,
                    issue_indices,
                }
            })
            .collect();

        // Resize col_scroll_offsets to match new column count
        let col_count = columns.len();
        self.board.col_scroll_offsets.resize(col_count, 0);

        self.board.columns = columns;

        // Clamp selected_col and selected_row
        if self.board.columns.is_empty() {
            self.board.selected_col = 0;
            self.board.selected_row = 0;
        } else {
            if self.board.selected_col >= self.board.columns.len() {
                self.board.selected_col = self.board.columns.len() - 1;
            }
            let issue_count = self.board.columns[self.board.selected_col]
                .issue_indices
                .len();
            if issue_count == 0 {
                self.board.selected_row = 0;
            } else if self.board.selected_row >= issue_count {
                self.board.selected_row = issue_count - 1;
            }
        }
    }
}

// ── ScreenPlugin implementation ──────────────────────────────────────────────

impl ScreenPlugin for JiraPlugin {
    fn name(&self) -> &str {
        "jira"
    }

    fn needs_timer(&self) -> bool {
        true
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Render the kanban board as the base layer
        board::render(frame, area, self);

        // Render modal overlay if one is active
        match &self.modal {
            Some(JiraModal::IssueDetail { .. }) => {
                detail::render_detail(frame, area, self);
            }
            Some(JiraModal::TransitionPicker { .. }) => {
                detail::render_transition_picker(frame, area, self);
            }
            Some(JiraModal::TransitionFields { .. }) => {
                detail::render_transition_fields(frame, area, self);
            }
            Some(JiraModal::SelectProject { .. }) => {
                create::render_select_project(frame, area, self);
            }
            Some(JiraModal::SelectIssueType { .. }) => {
                create::render_select_issue_type(frame, area, self);
            }
            Some(JiraModal::CreateForm { .. }) => {
                create::render_create_form(frame, area, self);
            }
            Some(JiraModal::ErrorModal { .. }) => {
                // Error modal rendering will be done by Agent 2
                let _ = (frame, area);
            }
            None => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        // Modal gets first crack at keys
        match &self.modal {
            Some(JiraModal::IssueDetail { .. }) => {
                return detail::handle_detail_key(key, self);
            }
            Some(JiraModal::TransitionPicker { .. }) => {
                return detail::handle_transition_picker_key(key, self);
            }
            Some(JiraModal::TransitionFields { .. }) => {
                return detail::handle_transition_fields_key(key, self);
            }
            Some(JiraModal::SelectProject { .. }) => {
                return create::handle_select_project_key(key, self);
            }
            Some(JiraModal::SelectIssueType { .. }) => {
                return create::handle_select_issue_type_key(key, self);
            }
            Some(JiraModal::CreateForm { .. }) => {
                return create::handle_create_form_key(key, self);
            }
            Some(JiraModal::ErrorModal { .. }) => {
                // Dismiss error modal on any key
                self.modal = None;
                return PluginAction::None;
            }
            None => {}
        }

        // No modal — handle board-level keys
        board::handle_key(key, self)
    }

    fn on_enter(&mut self) {
        self.loading = true;
        // Stub — full implementation will validate credentials, spawn background
        // thread, and send initial FetchMyIssues command.
    }

    fn on_leave(&mut self) {
        // Signal background thread to shut down
        if let Some(flag) = &self.shutdown_flag {
            flag.store(true, Ordering::Relaxed);
        }
        // Send explicit Shutdown command
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(JiraCommand::Shutdown);
        }
        // Wait for the thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        // Clear channel endpoints
        self.command_tx = None;
        self.result_rx = None;
        self.shutdown_flag = None;
    }

    fn on_tick(&mut self) -> Vec<String> {
        let mut notifications = Vec::new();
        let mut needs_rebuild = false;

        // Drain all pending results from the background thread.
        // Collect results first to avoid borrow conflicts.
        let results: Vec<JiraResult> = self
            .result_rx
            .as_ref()
            .map(|rx| {
                let mut batch = Vec::new();
                while let Ok(result) = rx.try_recv() {
                    batch.push(result);
                }
                batch
            })
            .unwrap_or_default();

        for result in results {
            match result {
                JiraResult::Issues {
                    generation,
                    issues,
                } => {
                    // Only apply if generation matches (prevent stale overwrites)
                    if generation >= self.generation {
                        self.issues = issues;
                        self.loading = false;
                        self.refreshing = false;
                        self.last_sync = Some(Instant::now());
                        self.last_error = None;
                        needs_rebuild = true;
                    } else {
                        // Stale result — still clear refreshing flag
                        self.refreshing = false;
                    }
                }
                JiraResult::Error { context, error } => {
                    let msg = format!("JIRA: {} — {}", context, error.display());
                    notifications.push(msg.clone());
                    self.last_error = Some(msg);
                    self.loading = false;
                    self.refreshing = false;
                }
                // Other result types will be handled by Agent 2
                _ => {}
            }
        }

        if needs_rebuild {
            self.rebuild_columns();
        }

        // Drain pending toasts
        notifications.extend(self.pending_toasts.drain(..));

        notifications
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        match &self.modal {
            Some(JiraModal::IssueDetail { .. }) => {
                vec![
                    ("j/k", "navigate"),
                    ("e", "edit"),
                    ("s", "transition"),
                    ("c", "comment"),
                    ("Esc", "close"),
                ]
            }
            Some(JiraModal::TransitionPicker { .. }) => {
                vec![("j/k", "navigate"), ("Enter", "apply"), ("Esc", "cancel")]
            }
            Some(JiraModal::TransitionFields { .. }) | Some(JiraModal::CreateForm { .. }) => {
                vec![
                    ("j/k", "navigate"),
                    ("Enter", "edit"),
                    ("S", "submit"),
                    ("Esc", "cancel"),
                ]
            }
            Some(JiraModal::SelectProject { .. }) | Some(JiraModal::SelectIssueType { .. }) => {
                vec![("j/k", "navigate"), ("Enter", "select"), ("Esc", "cancel")]
            }
            Some(JiraModal::ErrorModal { .. }) => {
                vec![("Enter", "dismiss")]
            }
            None => {
                vec![
                    ("hjkl", "navigate"),
                    ("s", "transition"),
                    ("c", "comment"),
                    ("Enter", "detail"),
                    ("p", "project"),
                    ("R", "refresh"),
                    ("n", "new"),
                    ("D", "toggle-done"),
                    ("Esc", "back"),
                ]
            }
        }
    }

    fn on_editor_complete(&mut self, _content: String, _context: &str) {
        // Stub — will be implemented to handle comment and description editing.
    }
}
