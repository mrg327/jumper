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
    /// `scroll_offset` tracks the first visible field row when the field count
    /// exceeds the modal's visible height (cursor-follows algorithm).
    Navigating { cursor: usize, scroll_offset: usize },

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
    /// `scroll_offset` preserved from the Navigating state to keep view consistent.
    ValidationError {
        cursor: usize,
        scroll_offset: usize,
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

    // ── Post-write refresh timer ─────────────────────────────────────
    pub(crate) pending_refresh_at: Option<Instant>,

    // ── Optimistic UI revert data ────────────────────────────────────
    /// Stores original status before an optimistic transition so we can
    /// revert if the API call fails. Key: issue key, Value: original status.
    pub(crate) pending_transitions: HashMap<String, JiraStatus>,
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

            pending_refresh_at: None,
            pending_transitions: HashMap::new(),
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

    /// Send a command to the background API thread.
    pub(crate) fn send_command(&self, cmd: JiraCommand) {
        if let Some(tx) = &self.command_tx {
            tx.send(cmd).ok();
        }
    }

    /// Trigger a full issue list refresh via the background thread.
    ///
    /// Increments the generation counter and sends `FetchMyIssues`.
    /// Skipped if a refresh is already in-flight (`self.refreshing`).
    pub(crate) fn trigger_refresh(&mut self) {
        if !self.refreshing {
            self.generation += 1;
            self.refreshing = true;
            self.send_command(JiraCommand::FetchMyIssues {
                generation: self.generation,
            });
        }
    }

    /// Schedule a post-write refresh with a 500ms delay.
    ///
    /// JIRA has eventual consistency — immediate reads after writes may return
    /// stale data. The 500ms delay is checked in `on_tick()`.
    pub(crate) fn schedule_post_write_refresh(&mut self) {
        self.pending_refresh_at =
            Some(Instant::now() + std::time::Duration::from_millis(500));
        self.refreshing = true;
    }

    /// Find an issue by key in the loaded issue list.
    pub(crate) fn find_issue(&self, key: &str) -> Option<&JiraIssue> {
        self.issues.iter().find(|i| i.key == key)
    }

    /// Return a deduplicated list of (project_key, project_name) from loaded issues.
    pub(crate) fn distinct_projects(&self) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        self.issues
            .iter()
            .filter(|i| seen.insert(i.project_key.clone()))
            .map(|i| (i.project_key.clone(), i.project_name.clone()))
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
            Some(JiraModal::ErrorModal { title, message }) => {
                detail::render_error_modal(frame, area, title, message);
            }
            None => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        // Deliver pending toasts one at a time before dispatching keys.
        // This ensures toasts from background operations reach the app.
        if !self.pending_toasts.is_empty() {
            let msg = self.pending_toasts.remove(0);
            return PluginAction::Toast(msg);
        }

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
                // Dismiss error modal only on Enter or Esc
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        self.modal = self.previous_modal.take();
                    }
                    _ => {}
                }
                return PluginAction::None;
            }
            None => {}
        }

        // No modal — handle board-level keys
        board::handle_key(key, self)
    }

    fn on_enter(&mut self) {
        self.loading = true;

        // 1. Validate config (url, email, JIRA_API_TOKEN env var).
        if let Err(msg) = self.config.validate() {
            self.modal = Some(JiraModal::ErrorModal {
                title: "Configuration Error".to_string(),
                message: msg,
            });
            self.loading = false;
            return;
        }

        // 2. Thread lifecycle guard — clean up any previous thread.
        if let Some(handle) = self.thread_handle.take() {
            // If shutdown was previously requested, join the old thread first
            // to avoid a race where the old thread exits after the new one spawns.
            if self
                .shutdown_flag
                .as_ref()
                .map_or(false, |f| f.load(Ordering::Relaxed))
            {
                let _ = handle.join();
            } else if handle.is_finished() {
                // Thread already exited on its own — just clean up
                let _ = handle.join();
            } else {
                // Thread is still running but not shut down — signal and join
                if let Some(flag) = &self.shutdown_flag {
                    flag.store(true, Ordering::Relaxed);
                }
                if let Some(tx) = &self.command_tx {
                    let _ = tx.send(JiraCommand::Shutdown);
                }
                let _ = handle.join();
            }
            self.command_tx = None;
            self.result_rx = None;
            self.shutdown_flag = None;
        }

        // 3. Validate credentials synchronously via GET /rest/api/3/myself.
        let api_token = std::env::var("JIRA_API_TOKEN").unwrap_or_default();
        match api::validate_credentials(&self.config.url, &self.config.email, &api_token) {
            Ok(myself) => {
                self.account_id = Some(myself.account_id);
            }
            Err(err) => {
                self.modal = Some(JiraModal::ErrorModal {
                    title: "Authentication Failed".to_string(),
                    message: format!(
                        "Could not connect to JIRA.\n\n{}\n\nCheck JIRA_API_TOKEN and plugins.jira.email in config.",
                        err.display()
                    ),
                });
                self.loading = false;
                return;
            }
        }

        // 4. Create channels.
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();

        // 5. Create shutdown flag.
        let shutdown = Arc::new(AtomicBool::new(false));

        // 6. Spawn background thread.
        let thread_base_url = self.config.url.clone();
        let thread_email = self.config.email.clone();
        let thread_api_token = api_token;
        let thread_account_id = self.account_id.clone().unwrap_or_default();
        let thread_sp_field = self.story_points_field.clone();
        let thread_sprint_field = self.sprint_field.clone();
        let thread_shutdown = shutdown.clone();

        let handle = std::thread::Builder::new()
            .name("jira-api".to_string())
            .spawn(move || {
                api::api_thread_loop(
                    cmd_rx,
                    res_tx,
                    thread_base_url,
                    thread_email,
                    thread_api_token,
                    thread_account_id,
                    thread_sp_field,
                    thread_sprint_field,
                    thread_shutdown,
                );
            })
            .ok();

        // 7. Store handles.
        self.command_tx = Some(cmd_tx);
        self.result_rx = Some(res_rx);
        self.shutdown_flag = Some(shutdown);
        self.thread_handle = handle;

        // 8. Send initial commands — fetch issues and field definitions.
        self.generation += 1;
        self.refreshing = true;
        self.send_command(JiraCommand::FetchMyIssues {
            generation: self.generation,
        });
        self.send_command(JiraCommand::FetchFields);
    }

    fn on_leave(&mut self) {
        // 1. Signal shutdown via AtomicBool + Shutdown command
        if let Some(flag) = &self.shutdown_flag {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(JiraCommand::Shutdown);
        }

        // 2. Join the thread
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        // 3. Clear all handles
        self.command_tx = None;
        self.result_rx = None;
        self.shutdown_flag = None;

        // 4. Clear data, modal state, pending toasts
        self.issues.clear();
        self.field_defs.clear();
        self.modal = None;
        self.previous_modal = None;
        self.pending_toasts.clear();
        self.pending_transitions.clear();
        self.board = BoardState::default();
        self.project_filter = None;
        self.show_done = false;

        // 5. Reset loading/refreshing/last_sync/last_error
        self.loading = false;
        self.refreshing = false;
        self.last_sync = None;
        self.last_error = None;
        self.pending_refresh_at = None;
    }

    fn on_tick(&mut self) -> Vec<String> {
        let mut notifications = Vec::new();
        let mut needs_rebuild = false;
        let mut channel_disconnected = false;

        // Drain all pending results from the background thread.
        // Collect results first to avoid borrow conflicts.
        let results: Vec<JiraResult> = self
            .result_rx
            .as_ref()
            .map(|rx| {
                let mut batch = Vec::new();
                loop {
                    match rx.try_recv() {
                        Ok(result) => batch.push(result),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            channel_disconnected = true;
                            break;
                        }
                    }
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
                    // Always clear refreshing, even for stale results
                    self.refreshing = false;

                    // Only apply if generation matches (prevent stale overwrites)
                    if generation >= self.generation {
                        self.issues = issues;
                        self.loading = false;
                        self.last_sync = Some(Instant::now());
                        self.last_error = None;
                        needs_rebuild = true;

                        // Stale detail modal check: if detail modal is open,
                        // verify the viewed issue still exists.
                        if let Some(JiraModal::IssueDetail { ref issue_key, .. }) = self.modal {
                            let key = issue_key.clone();
                            if self.find_issue(&key).is_none() {
                                self.modal = None;
                                self.pending_toasts.push(
                                    "Issue no longer assigned to you".to_string(),
                                );
                            }
                        }
                    }
                }

                JiraResult::Transitions(key, transitions) => {
                    // Update IssueDetail modal if it's open for this issue
                    if let Some(JiraModal::IssueDetail {
                        ref issue_key,
                        transitions: ref mut modal_transitions,
                        ..
                    }) = self.modal
                    {
                        if *issue_key == key {
                            *modal_transitions = Some(transitions.clone());
                        }
                    }
                    // Update TransitionPicker modal if it's open for this issue
                    if let Some(JiraModal::TransitionPicker {
                        ref issue_key,
                        transitions: ref mut picker_transitions,
                        ..
                    }) = self.modal
                    {
                        if *issue_key == key {
                            *picker_transitions = transitions;
                        }
                    }
                }

                JiraResult::TransitionComplete(key) => {
                    // Clear optimistic revert data — transition succeeded
                    self.pending_transitions.remove(&key);
                    self.pending_toasts
                        .push(format!("Transitioned {}", key));
                    self.schedule_post_write_refresh();
                }

                JiraResult::TransitionFailed(key, error) => {
                    // Revert optimistic UI — restore original status
                    if let Some(original_status) = self.pending_transitions.remove(&key) {
                        if let Some(issue) =
                            self.issues.iter_mut().find(|i| i.key == key)
                        {
                            issue.status = original_status;
                            needs_rebuild = true;
                        }
                    }
                    self.refreshing = false;
                    // User-initiated action — blocking error modal
                    // Save the current modal so it can be restored after dismissal
                    self.previous_modal = self.modal.take();
                    self.modal = Some(JiraModal::ErrorModal {
                        title: format!("Failed to transition {}", key),
                        message: error.display(),
                    });
                    self.pending_toasts
                        .push(format!("Transition failed: {}", key));
                }

                JiraResult::FieldUpdated(key, field_id) => {
                    self.pending_toasts
                        .push(format!("Updated {} on {}", field_id, key));
                    self.schedule_post_write_refresh();
                }

                JiraResult::CommentAdded(key) => {
                    self.pending_toasts
                        .push(format!("Comment added to {}", key));
                    self.schedule_post_write_refresh();
                }

                JiraResult::Comments(key, comments) => {
                    // Update IssueDetail modal's comments if open for this issue
                    if let Some(JiraModal::IssueDetail {
                        ref issue_key,
                        comments: ref mut modal_comments,
                        ..
                    }) = self.modal
                    {
                        if *issue_key == key {
                            *modal_comments = Some(comments);
                        }
                    }
                }

                JiraResult::CreateMeta(response) => {
                    // Transition from loading CreateForm to populated CreateForm.
                    // The modal should already be in a loading state or we need to
                    // check if we're expecting this response.
                    if let Some(JiraModal::SelectIssueType {
                        ref project_key,
                        ref issue_types,
                        cursor,
                        ..
                    }) = self.modal
                    {
                        // We got createmeta while still on SelectIssueType — this
                        // shouldn't normally happen. Just store it.
                        let _ = (project_key, issue_types, cursor);
                    }
                    // More commonly, we're in a transitional state where the form
                    // was requested. Look for a CreateForm with Submitting state
                    // or transition directly.
                    if let Some(JiraModal::CreateForm {
                        ref project_key,
                        ref issue_type_id,
                        ..
                    }) = self.modal
                    {
                        // CreateForm already exists — update its fields
                        let pk = project_key.clone();
                        let it = issue_type_id.clone();
                        let fields_with_values: Vec<(EditableField, Option<FieldValue>)> =
                            response.fields.into_iter().map(|f| (f, None)).collect();
                        self.modal = Some(JiraModal::CreateForm {
                            project_key: pk,
                            issue_type_id: it,
                            fields: fields_with_values,
                            form: FormState::Navigating { cursor: 0, scroll_offset: 0 },
                        });
                    } else {
                        // Store the response — the key handlers will pick it up
                        // when transitioning from SelectIssueType to CreateForm.
                        // We need to create the CreateForm modal from context.
                        // The previous_modal may hold context.
                        if let Some(JiraModal::SelectIssueType {
                            ref project_key,
                            ref issue_types,
                            cursor,
                        }) = self.previous_modal
                        {
                            let pk = project_key.clone();
                            let it_id = issue_types
                                .get(cursor)
                                .map(|t| t.id.clone())
                                .unwrap_or_default();
                            let fields_with_values: Vec<(EditableField, Option<FieldValue>)> =
                                response.fields.into_iter().map(|f| (f, None)).collect();
                            self.modal = Some(JiraModal::CreateForm {
                                project_key: pk,
                                issue_type_id: it_id,
                                fields: fields_with_values,
                                form: FormState::Navigating { cursor: 0, scroll_offset: 0 },
                            });
                            self.previous_modal = None;
                        }
                    }
                }

                JiraResult::IssueCreated(key) => {
                    self.modal = None;
                    self.pending_toasts.push(format!("Created {}", key));
                    self.schedule_post_write_refresh();
                }

                JiraResult::EditMeta(key, fields) => {
                    // Update IssueDetail modal's editable fields if open for this issue
                    if let Some(JiraModal::IssueDetail {
                        ref issue_key,
                        fields: ref mut modal_fields,
                        ..
                    }) = self.modal
                    {
                        if *issue_key == key {
                            *modal_fields = Some(fields);
                        }
                    }
                }

                JiraResult::Fields(field_defs) => {
                    // Discover story_points and sprint field IDs from field definitions
                    if self.story_points_field.is_none() {
                        let sp_matches: Vec<&JiraFieldDef> = field_defs
                            .iter()
                            .filter(|f| {
                                f.custom
                                    && f.name.to_lowercase().contains("story point")
                            })
                            .collect();
                        if sp_matches.len() == 1 {
                            self.story_points_field = Some(sp_matches[0].id.clone());
                        }
                    }
                    if self.sprint_field.is_none() {
                        let sprint_matches: Vec<&JiraFieldDef> = field_defs
                            .iter()
                            .filter(|f| {
                                f.custom
                                    && f.name.to_lowercase().contains("sprint")
                            })
                            .collect();
                        if sprint_matches.len() == 1 {
                            self.sprint_field = Some(sprint_matches[0].id.clone());
                        }
                    }
                    self.field_defs = field_defs;
                }

                JiraResult::IssueTypes(project_key, types) => {
                    // Transition from SelectProject to SelectIssueType modal
                    self.modal = Some(JiraModal::SelectIssueType {
                        project_key,
                        issue_types: types,
                        cursor: 0,
                    });
                }

                JiraResult::Error { context, error } => {
                    let msg = format!("JIRA: {} — {}", context, error.display());

                    // Determine if this was a user-initiated action or auto-refresh.
                    // User-initiated contexts contain action verbs (Transition, Update,
                    // Add, Create). Auto-refresh contexts are fetch operations.
                    let is_user_action = context.starts_with("Transition")
                        || context.starts_with("Updat")
                        || context.starts_with("Add")
                        || context.starts_with("Creat");

                    if is_user_action {
                        // Blocking error modal for user actions
                        self.previous_modal = self.modal.take();
                        self.modal = Some(JiraModal::ErrorModal {
                            title: format!("JIRA Error: {}", context),
                            message: error.display(),
                        });
                    } else {
                        // Non-blocking toast for auto-refresh errors
                        notifications.push(msg.clone());
                    }
                    self.last_error = Some(msg);
                    self.loading = false;
                    self.refreshing = false;
                }
            }
        }

        if needs_rebuild {
            self.rebuild_columns();
        }

        // Handle disconnected channel — background thread panicked
        if channel_disconnected && self.thread_handle.is_some() {
            self.loading = false;
            self.refreshing = false;
            self.thread_handle = None;
            self.command_tx = None;
            self.result_rx = None;
            self.shutdown_flag = None;
            self.modal = Some(JiraModal::ErrorModal {
                title: "JIRA Connection Lost".to_string(),
                message: "Background thread disconnected. Press Esc to return to dashboard, then reopen JIRA to reconnect.".to_string(),
            });
        }

        // Auto-refresh timer: if time since last_sync exceeds refresh_interval_secs
        // and not refreshing, trigger refresh.
        if let Some(last) = self.last_sync {
            let interval = std::time::Duration::from_secs(self.config.refresh_interval_secs);
            if last.elapsed() >= interval && !self.refreshing && !self.loading {
                self.trigger_refresh();
            }
        }

        // Post-write refresh: check if pending_refresh_at has elapsed.
        if let Some(refresh_at) = self.pending_refresh_at {
            if Instant::now() >= refresh_at {
                self.pending_refresh_at = None;
                // Force refreshing to false so trigger_refresh doesn't skip
                self.refreshing = false;
                self.trigger_refresh();
            }
        }

        // Drain pending toasts into notifications
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

    fn on_editor_complete(&mut self, content: String, context: &str) {
        // Empty content means the user quit the editor without writing — cancel.
        if content.trim().is_empty() {
            return;
        }

        if let Some(issue_key) = context.strip_prefix("comment:") {
            // Convert plain text to ADF and send as a JIRA comment
            let adf_body = adf::text_to_adf(&content);
            self.refreshing = true;
            self.send_command(JiraCommand::AddComment {
                issue_key: issue_key.to_string(),
                body: adf_body,
            });
        } else if let Some(rest) = context.strip_prefix("transition_comment:") {
            // Format: "transition_comment:{issue_key}:{transition_id}"
            if let Some((issue_key, transition_id)) = rest.split_once(':') {
                let adf_body = adf::text_to_adf(&content);
                let fields = serde_json::json!({
                    "update": {
                        "comment": [{
                            "add": {
                                "body": adf_body
                            }
                        }]
                    }
                });
                self.refreshing = true;
                self.send_command(JiraCommand::TransitionIssue {
                    issue_key: issue_key.to_string(),
                    transition_id: transition_id.to_string(),
                    fields: Some(fields),
                });
            }
        } else if let Some(rest) = context.strip_prefix("textarea:") {
            // Format: "textarea:{issue_key}:{field_id}"
            if let Some((issue_key, field_id)) = rest.split_once(':') {
                let adf_body = adf::text_to_adf(&content);
                self.refreshing = true;
                self.send_command(JiraCommand::UpdateField {
                    issue_key: issue_key.to_string(),
                    field_id: field_id.to_string(),
                    value: adf_body,
                });
            }
        }
    }
}
