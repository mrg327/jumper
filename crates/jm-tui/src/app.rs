//! Central App struct and TEA (The Elm Architecture) event loop.
//!
//! State is a single struct, view is a pure function of state,
//! and all mutations flow through a central `update()` function.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::prelude::*;

use jm_core::config::Config;
use jm_core::models::{Blocker, JournalEntry, LogEntry, Priority, Status};
use jm_core::storage::{ActiveProjectStore, IssueStore, JournalStore, LastReviewStore, PeopleStore, ProjectStore, SearchEngine};
use jm_core::time as jm_time;

use crate::events::{Action, Focus, ScreenId};
use crate::modals::{self, InputAction, InputModal, Modal, SelectAction, SelectModal};
use crate::plugins::PluginSidebar;
use crate::screens::{dashboard, issue_board, people, project_view, review, search, switch, weekly};
use crate::widgets::toast::Toast;
use crate::keyhints;

// ── App struct ───────────────────────────────────────────────────────

pub struct App {
    // Data stores
    pub project_store: ProjectStore,
    pub journal_store: JournalStore,
    pub people_store: PeopleStore,
    pub active_store: ActiveProjectStore,
    pub issue_store: IssueStore,
    pub search_engine: SearchEngine,
    #[allow(dead_code)]
    pub config: Config,

    // UI state
    pub screen: ScreenId,
    pub modal_stack: Vec<Modal>,
    pub plugins: PluginSidebar,
    pub sidebar_visible: bool,
    pub focus: Focus,

    // Screen states
    pub dashboard: dashboard::DashboardState,
    pub project_view: project_view::ProjectViewState,
    pub switch_state: switch::SwitchState,
    pub review_state: review::ReviewState,
    pub search_state: search::SearchState,
    pub people_state: people::PeopleState,
    pub issue_board_state: issue_board::IssueBoardState,
    pub weekly_state: weekly::WeeklyState,

    // Transient
    pub toast: Option<Toast>,
    pub last_tick: Instant,
    pub should_quit: bool,

    // Idle reminder
    pub idle_dismissed_at: Option<Instant>,
    pub app_started_at: Instant,

    // Unblock multi-blocker selection state
    // When there are 2+ open blockers, these hold the mapping from
    // SelectModal index → real blocker index in the project's blockers vec.
    pub unblock_open_indices: Vec<usize>,
    pub unblock_slug: String,

    // MoveBlocker flow state
    // After the user picks a source blocker, we stash its index here so
    // the second SelectModal (project selection) can reference it.
    pub move_blocker_source_idx: usize,
    pub move_blocker_slug: String,

    // Pending editor open: set to Some(slug) by OpenEditor/OpenEditorSelected,
    // consumed and cleared at the top of the run loop before drawing.
    pub pending_editor_slug: Option<String>,

    // Morning review auto-trigger
    pub last_review_store: LastReviewStore,
}

// ── Constructor ──────────────────────────────────────────────────────

impl App {
    pub fn new(config: Config, data_dir: PathBuf) -> Self {
        let project_store = ProjectStore::new(&data_dir);
        let journal_store = JournalStore::new(&data_dir);
        let people_store = PeopleStore::new(&data_dir);
        let active_store = ActiveProjectStore::new(&data_dir);
        let issue_store = IssueStore::new(&data_dir);
        let search_engine = SearchEngine::new(&data_dir);
        let last_review_store = LastReviewStore::new(&data_dir);

        let dashboard_state = dashboard::init(&project_store);
        let people_state = people::init(&people_store);
        let issue_board_state = issue_board::init(&issue_store);
        let weekly_state = weekly::init(&journal_store, &issue_store);
        let plugins = PluginSidebar::new(&config);

        // Check if we should auto-trigger morning review.
        let morning_start = config.review.morning_start;
        let morning_end = config.review.morning_end;
        let now = Local::now();
        let hour = now.hour();
        let today = now.date_naive();
        let should_auto_review = (hour >= morning_start && hour < morning_end)
            && last_review_store
                .last_review_date()
                .map(|d| d < today)
                .unwrap_or(true);

        let initial_screen = if should_auto_review {
            ScreenId::Review
        } else {
            ScreenId::Dashboard
        };

        App {
            project_store,
            journal_store,
            people_store,
            active_store,
            issue_store,
            search_engine,
            config,

            screen: initial_screen,
            modal_stack: Vec::new(),
            plugins,
            sidebar_visible: true,
            focus: Focus::Main,

            dashboard: dashboard_state,
            project_view: project_view::init(""),
            switch_state: switch::init(None),
            review_state: review::init(),
            search_state: search::init(),
            people_state,
            issue_board_state,
            weekly_state,

            toast: None,
            last_tick: Instant::now(),
            should_quit: false,

            idle_dismissed_at: None,
            app_started_at: Instant::now(),

            unblock_open_indices: Vec::new(),
            unblock_slug: String::new(),
            move_blocker_source_idx: 0,
            move_blocker_slug: String::new(),
            pending_editor_slug: None,

            last_review_store,
        }
    }

    // ── Main event loop ──────────────────────────────────────────────

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        loop {
            // Handle a pending editor open request before drawing.
            // We do this here so we have access to `terminal` for teardown/restore.
            if let Some(slug) = self.pending_editor_slug.take() {
                let path = self
                    .project_store
                    .projects_dir
                    .join(format!("{slug}.md"));
                if path.exists() {
                    let editor =
                        std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                    // Restore terminal before handing off to editor.
                    let _ = crossterm::terminal::disable_raw_mode();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::LeaveAlternateScreen,
                        crossterm::cursor::Show,
                    );
                    let _ = std::process::Command::new(&editor).arg(&path).status();
                    // Reclaim terminal after editor exits.
                    let _ = crossterm::terminal::enable_raw_mode();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::EnterAlternateScreen,
                        crossterm::cursor::Hide,
                    );
                    let _ = terminal.clear();
                    // Reload project data to reflect any edits.
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                } else {
                    self.toast = Some(Toast::new("Project file not found."));
                }
            }

            terminal.draw(|f| self.render(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    let action = self.handle_key(key);
                    self.update(action);
                }
            }

            // 1-second tick for plugins
            if self.last_tick.elapsed() >= Duration::from_secs(1) {
                self.last_tick = Instant::now();
                self.update(Action::Tick);
            }

            // Expire toast
            if let Some(ref toast) = self.toast {
                if toast.is_expired() {
                    self.toast = None;
                }
            }

            // Idle reminder: nudge user if no active project during work hours
            if self.should_show_idle_reminder() {
                self.modal_stack.push(Modal::Confirm(modals::ConfirmModal::new(
                    "No Active Project",
                    "You have no active project. Press w to start working, or dismiss.",
                )));
                self.idle_dismissed_at = Some(Instant::now());
            }

            // Debounced search
            if matches!(self.screen, ScreenId::Search) {
                search::maybe_search(&mut self.search_state, &self.search_engine);
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    // ── Render pipeline ──────────────────────────────────────────────

    fn render(&self, frame: &mut Frame) {
        let [main_area, footer] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        // Split main area for sidebar if visible
        let (content_area, sidebar_area) = if self.sidebar_visible {
            let [content, sidebar] = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(22),
            ])
            .areas(main_area);
            (content, Some(sidebar))
        } else {
            (main_area, None)
        };

        // 1. Current screen
        let focus_main = matches!(self.focus, Focus::Main);
        match &self.screen {
            ScreenId::Dashboard => {
                let active_slug = self.active_store.get_active();
                dashboard::render(
                    &self.dashboard,
                    &self.dashboard.projects,
                    active_slug.as_deref(),
                    focus_main,
                    frame,
                    content_area,
                    &self.issue_store,
                );
            }
            ScreenId::ProjectView(slug) => {
                if let Some(project) = self.project_store.get_project(slug) {
                    let all_projects = self.project_store.list_projects(None);
                    let references = jm_core::crosslinks::find_references(slug, &all_projects);
                    let issue_file = self.issue_store.load(slug);
                    let issues = if issue_file.issues.is_empty() { None } else { Some(&issue_file) };
                    project_view::render(
                        &self.project_view,
                        &project,
                        &references,
                        issues,
                        frame,
                        content_area,
                    );
                }
            }
            ScreenId::Switch(_) => {
                let projects = self.project_store.list_projects(None);
                let active = self.active_store.get_active();
                switch::render(
                    &self.switch_state,
                    &projects,
                    active.as_deref(),
                    frame,
                    content_area,
                );
            }
            ScreenId::Review => {
                let yesterday = self.journal_store.get_previous_workday(None);
                let all_projects = self.project_store.list_projects(None);
                let mut all_blockers: Vec<(String, Blocker)> = Vec::new();
                for p in &all_projects {
                    for b in &p.blockers {
                        if !b.resolved {
                            all_blockers.push((p.name.clone(), b.clone()));
                        }
                    }
                }
                let today = Local::now().date_naive();
                let stale: Vec<jm_core::models::Project> = all_projects
                    .into_iter()
                    .filter(|p| p.status != jm_core::models::Status::Done && p.status != jm_core::models::Status::Parked)
                    .filter(|p| {
                        p.log
                            .first()
                            .map(|e| (today - e.date).num_days() > 7)
                            .unwrap_or(true)
                    })
                    .collect();
                let had_closeout = yesterday.as_ref().map(|j| {
                    j.entries.iter().any(|e| e.entry_type == "Done" || e.entry_type == "Break")
                }).unwrap_or(true); // true if no yesterday = nothing to warn about
                review::render(
                    &self.review_state,
                    yesterday.as_ref(),
                    &all_blockers,
                    &stale,
                    had_closeout,
                    frame,
                    content_area,
                );
            }
            ScreenId::Search => {
                search::render(&self.search_state, frame, content_area);
            }
            ScreenId::People => {
                people::render(&self.people_state, frame, content_area);
            }
            ScreenId::IssueBoard => {
                issue_board::render(
                    &self.issue_board_state,
                    &self.project_store,
                    frame,
                    content_area,
                );
            }
            ScreenId::Weekly => {
                weekly::render(&self.weekly_state, frame, content_area);
            }
        }

        // 2. Plugin sidebar
        if let Some(sidebar_area) = sidebar_area {
            let sidebar_focused = matches!(self.focus, Focus::Sidebar(_));
            let focused_idx = if let Focus::Sidebar(idx) = self.focus {
                Some(idx)
            } else {
                None
            };
            self.plugins
                .render(sidebar_area, frame.buffer_mut(), sidebar_focused, focused_idx);
        }

        // 3. Modal stack (render all, topmost gets focus)
        for (i, modal) in self.modal_stack.iter().enumerate() {
            if i < self.modal_stack.len() - 1 {
                modals::render_dim_overlay(frame, main_area);
            }
            modal.render(frame, main_area);
        }

        // 4. Toast overlay
        if let Some(ref toast) = self.toast {
            toast.render(frame, main_area);
        }

        // 5. Keybinding footer with status info
        let is_kanban = matches!(self.screen, ScreenId::Dashboard)
            && matches!(self.dashboard.view_mode, dashboard::ViewMode::Kanban);

        let mut status_spans: Vec<Span> = Vec::new();

        // Switch counter
        let switch_count = self
            .journal_store
            .today()
            .entries
            .iter()
            .filter(|e| e.entry_type == "Switched")
            .count();
        if switch_count > 0 {
            let style = if switch_count > 5 {
                Style::default().fg(crate::theme::TEXT_WARNING)
            } else {
                crate::theme::dim()
            };
            status_spans.push(Span::styled(format!("Sw:{switch_count}"), style));
        }

        // Active session timer
        let sessions = jm_time::compute_sessions(&self.journal_store.today());
        let now = Local::now().time();
        if let Some(elapsed) = jm_time::active_session_elapsed(&sessions, now) {
            if !status_spans.is_empty() {
                status_spans.push(Span::styled(" | ", crate::theme::dim()));
            }
            status_spans.push(Span::styled(
                jm_time::format_duration(elapsed),
                Style::default().fg(crate::theme::TEXT_ACCENT),
            ));
        }

        keyhints::render(
            &self.screen,
            &self.focus,
            !self.modal_stack.is_empty(),
            is_kanban,
            &status_spans,
            frame,
            footer,
        );
    }

    // ── Key handling ─────────────────────────────────────────────────

    fn handle_key(&mut self, key: KeyEvent) -> Action {
        // 1. Modal takes priority
        if let Some(modal) = self.modal_stack.last_mut() {
            return modal.handle_key(key);
        }

        // 2. Sidebar if focused
        if let Focus::Sidebar(idx) = self.focus {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => {
                    return Action::Back; // unfocus sidebar
                }
                _ => {
                    self.plugins.handle_key(idx, key);
                    return Action::None;
                }
            }
        }

        // 3. Screen
        match &self.screen {
            ScreenId::Dashboard => dashboard::handle_key(&mut self.dashboard, key),
            ScreenId::ProjectView(_) => {
                if let Some(project) = self.current_project() {
                    project_view::handle_key(&mut self.project_view, key, &project)
                } else {
                    Action::None
                }
            }
            ScreenId::Switch(_) => {
                let projects = self.project_store.list_projects(None);
                switch::handle_key(&mut self.switch_state, key, &projects)
            }
            ScreenId::Review => review::handle_key(&mut self.review_state, key),
            ScreenId::Search => search::handle_key(&mut self.search_state, key),
            ScreenId::People => people::handle_key(&mut self.people_state, key),
            ScreenId::IssueBoard => issue_board::handle_key(&mut self.issue_board_state, key),
            ScreenId::Weekly => weekly::handle_key(&mut self.weekly_state, key),
        }
    }

    fn current_project(&self) -> Option<jm_core::models::Project> {
        if let ScreenId::ProjectView(ref slug) = self.screen {
            self.project_store.get_project(slug)
        } else {
            None
        }
    }

    /// Return the slug of the project that n/b/d/u actions should target:
    /// - In ProjectView: the currently viewed project.
    /// - On Dashboard:   the cursor-highlighted (selected) project.
    fn targeted_project_slug(&self) -> Option<String> {
        match &self.screen {
            ScreenId::ProjectView(slug) => Some(slug.clone()),
            ScreenId::Dashboard => self
                .dashboard
                .projects
                .get(self.dashboard.selected)
                .map(|p| p.slug.clone()),
            _ => None,
        }
    }

    // ── Central update ───────────────────────────────────────────────

    fn update(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Quit => self.should_quit = true,

            // Navigation — handled inline by screen handle_key already
            Action::Down
            | Action::Up
            | Action::Top
            | Action::Bottom
            | Action::HalfPageDown
            | Action::HalfPageUp => {}

            Action::Select => self.handle_select(),
            Action::Back => self.handle_back(),

            Action::StartWork => self.handle_start_work(),
            Action::SwitchContext => {
                self.screen = ScreenId::Switch(None);
                self.switch_state = switch::init(None);
            }
            Action::MeetingMode => {
                self.handle_meeting_mode();
            }
            Action::QuickNote => {
                self.push_input_modal("Quick Note", "Note:", InputAction::QuickNote);
            }
            Action::QuickBlocker => {
                self.push_input_modal("Log Blocker", "Blocker:", InputAction::QuickBlocker);
            }
            Action::QuickDecision => {
                self.push_input_modal("Log Decision", "Decision:", InputAction::QuickDecision);
            }
            Action::LogDecision => {
                self.push_input_modal("Log Decision", "Decision:", InputAction::QuickDecision);
            }
            Action::EditFocus => {
                let initial = self
                    .current_project()
                    .map(|p| p.current_focus.clone())
                    .unwrap_or_default();
                self.push_input_modal_with(
                    "Edit Focus",
                    "Focus:",
                    &initial,
                    InputAction::EditFocus,
                );
            }
            Action::SearchOpen => {
                self.screen = ScreenId::Search;
                self.search_state = search::init();
            }
            Action::MorningReview => {
                self.screen = ScreenId::Review;
                self.review_state = review::init();
                // Mark reviewed today immediately when manually triggered via `r`.
                let _ = self.last_review_store.mark_reviewed_today();
            }
            Action::PeopleView => {
                self.screen = ScreenId::People;
                self.people_state = people::init(&self.people_store);
            }
            Action::AddProject => {
                self.push_input_modal("Add Project", "Project name:", InputAction::AddProject);
            }
            Action::Help => {
                let screen_name = match self.screen {
                    ScreenId::Dashboard => "dashboard",
                    ScreenId::ProjectView(_) => "project_view",
                    _ => "dashboard",
                };
                self.modal_stack
                    .push(Modal::Help(modals::HelpModal::new(screen_name)));
            }
            Action::StopWork => {
                self.modal_stack.push(Modal::Select(modals::SelectModal::new(
                    "Stop Working",
                    vec![
                        "15 min break".to_string(),
                        "Lunch".to_string(),
                        "Done for day".to_string(),
                    ],
                    modals::SelectAction::StopWorkChoice,
                )));
            }
            Action::Export => self.handle_export(),

            Action::OpenIssueBoard => {
                self.issue_board_state = issue_board::init(&self.issue_store);
                self.screen = ScreenId::IssueBoard;
            }

            Action::OpenWeekly => {
                self.weekly_state = weekly::init(&self.journal_store, &self.issue_store);
                self.screen = ScreenId::Weekly;
            }

            Action::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            Action::FocusSidebar => {
                if self.sidebar_visible && self.plugins.plugin_count() > 0 {
                    self.focus = Focus::Sidebar(0);
                }
            }

            Action::CycleStatus => self.handle_cycle_status(),
            Action::CyclePriority => self.handle_cycle_priority(),
            Action::EditTags => {
                let initial = self
                    .current_project()
                    .map(|p| p.tags.join(", "))
                    .unwrap_or_default();
                self.push_input_modal_with(
                    "Edit Tags",
                    "Tags (comma-separated):",
                    &initial,
                    InputAction::EditTags,
                );
            }
            Action::EditTarget => {
                let initial = self
                    .current_project()
                    .and_then(|p| p.target.map(|d| d.to_string()))
                    .unwrap_or_default();
                self.push_input_modal_with(
                    "Edit Target",
                    "Target date (YYYY-MM-DD):",
                    &initial,
                    InputAction::EditTarget,
                );
            }
            Action::Unblock => {
                self.handle_unblock();
            }
            Action::MoveBlocker => {
                self.push_input_modal("Move Blocker", "Blocker text:", InputAction::MoveBlocker);
            }
            Action::OpenEditor => {
                // In ProjectView: open the currently-viewed project file.
                if let ScreenId::ProjectView(ref slug) = self.screen.clone() {
                    self.pending_editor_slug = Some(slug.clone());
                }
            }
            Action::OpenEditorSelected => {
                // On Dashboard: open the cursor-selected project file.
                if let Some(slug) = self.targeted_project_slug() {
                    self.pending_editor_slug = Some(slug);
                }
            }
            Action::OpenCommandMode => {
                self.push_input_modal_with(
                    "Command",
                    ":command  (work <slug> / note <text> / block <text> / park <slug> / done / q)",
                    ":",
                    InputAction::CommandMode,
                );
            }
            Action::DeleteProject => {
                if let ScreenId::ProjectView(ref slug) = self.screen {
                    let slug = slug.clone();
                    self.modal_stack.push(Modal::Confirm(modals::ConfirmModal::new(
                        "Delete Project",
                        &format!("Delete project '{slug}'?"),
                    )));
                }
            }

            // ── Issue actions ─────────────────────────────────────────
            Action::AddIssue => {
                self.push_input_modal("Add Issue", "Issue title:", InputAction::AddIssue);
            }
            Action::AddSubIssue => {
                // Pick parent issue, then prompt for title
                if let Some(slug) = self.targeted_project_slug() {
                    let issue_file = self.issue_store.load(&slug);
                    let top_issues: Vec<(usize, String)> = issue_file
                        .issues
                        .iter()
                        .filter(|i| i.parent_id.is_none())
                        .enumerate()
                        .map(|(idx, i)| (idx, format!("#{} {}", i.id, i.title)))
                        .collect();
                    if top_issues.is_empty() {
                        self.toast = Some(Toast::new("No issues to add sub-issue to."));
                    } else if top_issues.len() == 1 {
                        // Only one top-level issue, skip selection
                        let parent = issue_file.issues.iter().find(|i| i.parent_id.is_none()).unwrap();
                        self.push_input_modal(
                            &format!("Sub-issue of #{} {}", parent.id, parent.title),
                            "Sub-issue title:",
                            InputAction::AddSubIssue(parent.id),
                        );
                    } else {
                        let labels: Vec<String> = top_issues.iter().map(|(_, l)| l.clone()).collect();
                        self.modal_stack.push(Modal::Select(modals::SelectModal::new(
                            "Pick parent issue",
                            labels,
                            modals::SelectAction::PickParentIssue,
                        )));
                    }
                }
            }
            Action::CycleIssueStatus => {
                self.handle_cycle_issue_status(false);
            }
            Action::CycleIssueStatusReverse => {
                self.handle_cycle_issue_status(true);
            }
            Action::CloseIssue => {
                self.handle_close_issue();
            }
            Action::PinIssue => {
                self.handle_pin_issue();
            }
            Action::NoteToIssue => {
                self.handle_note_to_issue();
            }

            Action::PopModal => {
                self.modal_stack.pop();
                self.idle_dismissed_at = Some(Instant::now());
            }
            Action::PushModal(_) => {} // handled by specific actions above
            Action::PushScreen(ref screen) => {
                if let ScreenId::ProjectView(slug) = screen {
                    self.project_view = project_view::init(slug);
                }
                self.screen = screen.clone();
            }
            Action::PopScreen => self.handle_back(),

            Action::SubmitInput(text) => self.handle_submit_input(text),
            Action::Cancel => {
                if self.modal_stack.pop().is_none() {
                    // No modal was open — cancel the current screen (e.g. Switch)
                    self.handle_back();
                }
                self.idle_dismissed_at = Some(Instant::now());
            }

            Action::SaveContextOnly => {
                self.handle_save_context_only();
                self.idle_dismissed_at = Some(Instant::now());
            }

            Action::SwitchComplete => {
                self.handle_switch_complete();
                self.idle_dismissed_at = Some(Instant::now());
            }

            Action::Toast(ref msg) if msg.starts_with("issue_board_set_status:") => {
                // Format: issue_board_set_status:slug:id:status
                let parts: Vec<&str> = msg.splitn(4, ':').collect();
                if parts.len() == 4 {
                    let slug = parts[1];
                    let id: u32 = parts[2].parse().unwrap_or(0);
                    if let Ok(new_status) = parts[3].parse::<jm_core::models::IssueStatus>() {
                        let _ = self.issue_store.set_status(slug, id, new_status);
                        self.toast = Some(Toast::new(&format!("#{id} → {new_status}")));
                        issue_board::refresh(&mut self.issue_board_state, &self.issue_store);
                    }
                }
            }
            Action::Toast(msg) => {
                self.toast = Some(Toast::new(&msg));
            }

            Action::Tick => {
                let notifications = self.plugins.on_tick();
                for msg in notifications {
                    self.toast = Some(Toast::new(&msg));
                }
            }

            _ => {}
        }
    }

    // ── Helper methods ───────────────────────────────────────────────

    /// Open the selected item on the current screen.
    fn handle_select(&mut self) {
        match &self.screen {
            ScreenId::Dashboard => {
                if let Some(project) = self.dashboard.projects.get(self.dashboard.selected) {
                    let slug = project.slug.clone();
                    self.screen = ScreenId::ProjectView(slug.clone());
                    self.project_view = project_view::init(&slug);
                }
            }
            ScreenId::Search => {
                // Enter on search results handled by search::handle_key returning PushScreen
            }
            _ => {}
        }
    }

    /// Return to dashboard or unfocus sidebar.
    fn handle_back(&mut self) {
        if let Focus::Sidebar(_) = self.focus {
            self.focus = Focus::Main;
            return;
        }
        // When leaving the review screen, mark today as reviewed.
        if matches!(self.screen, ScreenId::Review) {
            let _ = self.last_review_store.mark_reviewed_today();
        }
        self.screen = ScreenId::Dashboard;
        self.focus = Focus::Main;
        // Refresh dashboard when returning from other screens
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Set the highlighted project as active and log a journal entry.
    fn handle_start_work(&mut self) {
        let slug = match &self.screen {
            ScreenId::Dashboard => self
                .dashboard
                .projects
                .get(self.dashboard.selected)
                .map(|p| p.slug.clone()),
            ScreenId::ProjectView(slug) => Some(slug.clone()),
            _ => None,
        };

        let Some(slug) = slug else { return };
        let Some(project) = self.project_store.get_project(&slug) else {
            return;
        };

        // If there is an active project AND it's different from the target,
        // route through the context-switch wizard so the user captures where
        // they left off before changing projects.
        if let Some(active_slug) = self.active_store.get_active() {
            if active_slug != slug {
                // When switching to the meetings project, skip the 3-step capture.
                if slug == self.config.meetings_project {
                    self.switch_state = switch::init_skip(&slug);
                    self.handle_switch_complete();
                } else {
                    self.switch_state = switch::init(Some(&slug));
                    self.screen = ScreenId::Switch(Some(slug));
                }
                return;
            }
        }

        // No active project, or already working on the selected project —
        // start directly and show resume info if available.
        if let Err(e) = self.active_store.set_active(&slug) {
            self.toast = Some(Toast::new(&format!("Error: {e}")));
            return;
        }

        let time = Local::now().format("%H:%M").to_string();
        let mut entry = JournalEntry::new(&time, "Started", &project.name);
        if !project.current_focus.is_empty() {
            entry
                .details
                .insert("focus".to_string(), project.current_focus.clone());
            self.track_mentions(&project.current_focus, &project.name);
        }
        let _ = self.journal_store.append(entry);

        self.toast = Some(Toast::new(&format!("Working on: {}", project.name)));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Quick-meeting mode: switch to the meetings project (slim capture) and prompt for meeting topic.
    fn handle_meeting_mode(&mut self) {
        let slug = self.config.meetings_project.clone();

        // Verify the meetings project exists.
        if self.project_store.get_project(&slug).is_none() {
            self.toast = Some(Toast::new("Meetings project not found"));
            return;
        }

        // If a different project is currently active, write a slim "Switched" journal entry.
        if let Some(active_slug) = self.active_store.get_active() {
            if active_slug != slug {
                if let Some(old_project) = self.project_store.get_project(&active_slug) {
                    let meetings_name = self.project_store
                        .get_project(&slug)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| slug.clone());
                    let time = Local::now().format("%H:%M").to_string();
                    let switch_label = format!("{} \u{2192} {meetings_name}", old_project.name);
                    let mut entry = JournalEntry::new(&time, "Switched", &switch_label);
                    entry.details.insert("left_off".to_string(), "Went to meeting".to_string());
                    let _ = self.journal_store.append(entry);
                    // Preserve old project's current_focus — no overwrite.
                }
            }
        }

        // Set meetings project as active.
        let _ = self.active_store.set_active(&slug);

        // Push input modal for meeting topic.
        self.push_input_modal("Meeting", "What's the meeting?", InputAction::MeetingNote);

        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Log a break or lunch without clearing active project.
    fn handle_pause(&mut self, entry_type: &str, toast_msg: &str) {
        let time = Local::now().format("%H:%M").to_string();
        let project_name = self
            .active_store
            .get_active()
            .and_then(|slug| {
                self.project_store
                    .get_project(&slug)
                    .map(|p| p.name.clone())
                    .or(Some(slug))
            })
            .unwrap_or_default();

        let entry = JournalEntry::new(&time, entry_type, &project_name);
        let _ = self.journal_store.append(entry);

        self.toast = Some(Toast::new(toast_msg));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Log end-of-day Done entry and clear active project.
    fn handle_eod_stop_work(&mut self) {
        let time = Local::now().format("%H:%M").to_string();
        let project_name = self
            .active_store
            .get_active()
            .and_then(|slug| {
                self.project_store
                    .get_project(&slug)
                    .map(|p| p.name.clone())
                    .or(Some(slug))
            })
            .unwrap_or_default();

        let entry = JournalEntry::new(&time, "Done", &project_name);
        let _ = self.journal_store.append(entry);

        self.active_store.clear_active();
        self.toast = Some(Toast::new("Done for the day."));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Export current state to file and show a toast.
    fn handle_export(&mut self) {
        let result = jm_core::export::export_to_file(
            &self.project_store,
            &self.journal_store,
            &self.people_store,
            &self.active_store,
            None,
        );
        match result {
            Ok(path) => {
                self.toast = Some(Toast::new(&format!("Exported to {}", path.display())));
            }
            Err(e) => {
                self.toast = Some(Toast::new(&format!("Export failed: {e}")));
            }
        }
    }

    /// Cycle the current project's status: active → blocked → pending → parked → done → active.
    /// Uses save_project_raw to bypass auto-status logic.
    fn handle_cycle_status(&mut self) {
        let Some(mut project) = self.current_project() else {
            return;
        };

        project.status = match project.status {
            Status::Active  => Status::Blocked,
            Status::Blocked => Status::Pending,
            Status::Pending => Status::Parked,
            Status::Parked  => Status::Done,
            Status::Done    => Status::Active,
        };

        let new_status = project.status.to_string();
        if let Err(e) = self.project_store.save_project_raw(&project) {
            self.toast = Some(Toast::new(&format!("Error: {e}")));
            return;
        }

        self.toast = Some(Toast::new(&format!("Status → {new_status}")));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Cycle the current project's priority: high → medium → low → high.
    fn handle_cycle_priority(&mut self) {
        let Some(mut project) = self.current_project() else {
            return;
        };

        project.priority = match project.priority {
            Priority::High   => Priority::Medium,
            Priority::Medium => Priority::Low,
            Priority::Low    => Priority::High,
        };

        let new_priority = project.priority.to_string();
        if let Err(e) = self.project_store.save_project(&mut project) {
            self.toast = Some(Toast::new(&format!("Error: {e}")));
            return;
        }

        self.toast = Some(Toast::new(&format!("Priority → {new_priority}")));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Cycle an issue's status (for the current project).
    /// In kanban mode: directly cycles the highlighted issue.
    /// Otherwise: 0 non-done → toast. 1 → cycle directly. 2+ → show picker.
    /// If `reverse` is true, cycles backward instead of forward.
    fn handle_cycle_issue_status(&mut self, reverse: bool) {
        let Some(slug) = self.targeted_project_slug() else { return };
        let issue_file = self.issue_store.load(&slug);

        // In project-view kanban mode, cycle the highlighted issue directly.
        if let Some(issue_id) =
            project_view::kanban_selected_issue_id(&self.project_view, &issue_file.issues)
        {
            if let Some(issue) = issue_file.issues.iter().find(|i| i.id == issue_id) {
                let new_status = if reverse {
                    issue.status.cycle_reverse()
                } else {
                    issue.status.cycle()
                };
                let _ = self.issue_store.set_status(&slug, issue_id, new_status);
                self.toast = Some(Toast::new(&format!("#{issue_id} → {new_status}")));
                return;
            }
        }

        let non_done: Vec<&jm_core::models::Issue> = issue_file
            .issues
            .iter()
            .filter(|i| i.status != jm_core::models::IssueStatus::Done)
            .collect();

        if non_done.is_empty() {
            self.toast = Some(Toast::new("No open issues to cycle."));
            return;
        }

        if non_done.len() == 1 {
            let issue = non_done[0];
            let new_status = if reverse { issue.status.cycle_reverse() } else { issue.status.cycle() };
            let _ = self.issue_store.set_status(&slug, issue.id, new_status);
            self.toast = Some(Toast::new(&format!("#{} → {new_status}", issue.id)));
            return;
        }

        // 2+ issues: show picker
        let labels: Vec<String> = non_done
            .iter()
            .map(|i| format!("#{} [{}] {}", i.id, i.status, i.title))
            .collect();
        let action = if reverse {
            modals::SelectAction::PickIssueToCycleReverse
        } else {
            modals::SelectAction::PickIssueToCycle
        };
        self.modal_stack.push(Modal::Select(modals::SelectModal::new(
            if reverse { "Cycle issue status (reverse)" } else { "Cycle issue status" },
            labels,
            action,
        )));
    }

    /// Close an issue (set to done).
    /// In kanban mode: directly closes the highlighted issue.
    /// Otherwise: 0 non-done → toast. 1 → close directly. 2+ → show picker.
    fn handle_close_issue(&mut self) {
        let Some(slug) = self.targeted_project_slug() else { return };
        let issue_file = self.issue_store.load(&slug);

        // In project-view kanban mode, close the highlighted issue directly.
        if let Some(issue_id) =
            project_view::kanban_selected_issue_id(&self.project_view, &issue_file.issues)
        {
            let _ = self.issue_store.set_status(&slug, issue_id, jm_core::models::IssueStatus::Done);
            self.toast = Some(Toast::new(&format!("#{issue_id} closed.")));
            return;
        }
        let non_done: Vec<&jm_core::models::Issue> = issue_file
            .issues
            .iter()
            .filter(|i| i.status != jm_core::models::IssueStatus::Done)
            .collect();

        if non_done.is_empty() {
            self.toast = Some(Toast::new("No open issues to close."));
            return;
        }

        if non_done.len() == 1 {
            let issue = non_done[0];
            let _ = self.issue_store.set_status(&slug, issue.id, jm_core::models::IssueStatus::Done);
            self.toast = Some(Toast::new(&format!("#{} closed.", issue.id)));
            return;
        }

        // 2+ issues: show picker
        let labels: Vec<String> = non_done
            .iter()
            .map(|i| format!("#{} [{}] {}", i.id, i.status, i.title))
            .collect();
        self.modal_stack.push(Modal::Select(modals::SelectModal::new(
            "Close issue",
            labels,
            modals::SelectAction::PickIssueToClose,
        )));
    }

    /// Collect the last 10 note lines from a project's log (across all entries).
    fn collect_recent_notes(project: &jm_core::models::Project) -> Vec<String> {
        let mut notes: Vec<String> = Vec::new();
        for entry in &project.log {
            for line in &entry.lines {
                notes.push(line.clone());
            }
        }
        // Take the most recent (log is stored newest-first)
        notes.truncate(10);
        notes
    }

    /// Promote a note line to an issue.
    /// - 0 notes  → toast
    /// - 1 note   → create issue directly
    /// - 2+ notes → show picker
    fn handle_note_to_issue(&mut self) {
        let Some(slug) = self.targeted_project_slug() else { return };
        let Some(project) = self.project_store.get_project(&slug) else { return };
        let notes = Self::collect_recent_notes(&project);

        if notes.is_empty() {
            self.toast = Some(Toast::new("No notes to convert."));
            return;
        }

        if notes.len() == 1 {
            let text = notes[0].clone();
            match self.issue_store.create_issue(&slug, &text, None) {
                Ok(issue) => {
                    self.toast = Some(Toast::new(&format!("Created issue #{}: {}", issue.id, issue.title)));
                }
                Err(e) => {
                    self.toast = Some(Toast::new(&format!("Error: {e}")));
                }
            }
            return;
        }

        // 2+ notes: show picker
        let labels = notes.clone();
        self.modal_stack.push(Modal::Select(modals::SelectModal::new(
            "Convert note to issue",
            labels,
            modals::SelectAction::NoteToIssue,
        )));
    }

    /// Start the unblock flow:
    /// - 0 open blockers  → toast saying nothing to resolve
    /// - 1 open blocker   → resolve it immediately
    /// - 2+ open blockers → show a SelectModal so the user picks which one
    fn handle_unblock(&mut self) {
        let slug = self.targeted_project_slug();
        let Some(slug) = slug else { return };
        let Some(project) = self.project_store.get_project(&slug) else {
            return;
        };

        let open_indices: Vec<usize> = project
            .blockers
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.resolved)
            .map(|(i, _)| i)
            .collect();

        match open_indices.len() {
            0 => {
                self.toast = Some(Toast::new("No open blockers to resolve."));
            }
            1 => {
                // Resolve immediately
                self.resolve_blocker_at_index(&slug, open_indices[0]);
            }
            _ => {
                // Build display strings for the select modal
                let today = Local::now().date_naive();
                let items: Vec<String> = open_indices
                    .iter()
                    .map(|&i| {
                        let b = &project.blockers[i];
                        let days = b.since.map(|d| (today - d).num_days()).unwrap_or(0);
                        let person_part = b
                            .person
                            .as_deref()
                            .map(|p| format!(" {p}"))
                            .unwrap_or_default();
                        format!("{}{person_part}  ({days}d)", b.description)
                    })
                    .collect();

                // The SelectModal selected index maps to open_indices[selected].
                // We store the open_indices in the modal items as a parallel list;
                // on submit we decode via open_indices[selected].
                // To avoid storing extra state we embed the real blocker index in
                // the item string's prefix: we'll keep a separate mapping by
                // pushing `ChooseBlocker` and decoding in handle_submit_select.
                self.modal_stack.push(Modal::Select(SelectModal::new(
                    "Resolve Blocker",
                    items,
                    SelectAction::ChooseBlocker,
                )));
                // Stash the open indices so we can decode them after selection.
                // We do this by storing them as a json-like string in a "shadow"
                // select modal approach — instead, use the simplest route:
                // store a parallel vec on App.
                self.unblock_open_indices = open_indices;
                self.unblock_slug = slug;
            }
        }
    }

    /// Resolve the blocker at `index` in the project identified by `slug`.
    fn resolve_blocker_at_index(&mut self, slug: &str, index: usize) {
        let Some(mut project) = self.project_store.get_project(slug) else {
            return;
        };
        let today = Local::now().date_naive();
        if let Some(blocker) = project.blockers.get_mut(index) {
            blocker.resolved = true;
            blocker.resolved_date = Some(today);
        } else {
            self.toast = Some(Toast::new("Blocker not found."));
            return;
        }
        if let Err(e) = self.project_store.save_project(&mut project) {
            self.toast = Some(Toast::new(&format!("Error: {e}")));
            return;
        }
        self.toast = Some(Toast::new("Blocker resolved."));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Route a submitted input string based on the topmost modal's `InputAction`.
    fn handle_submit_input(&mut self, text: String) {
        // Peek at the top modal to find the InputAction before popping.
        let input_action = if let Some(Modal::Input(modal)) = self.modal_stack.last() {
            Some(modal.on_submit.clone())
        } else if let Some(Modal::Select(modal)) = self.modal_stack.last() {
            // A SelectModal submitted — route by its action.
            let select_action = modal.on_submit.clone();
            self.modal_stack.pop();
            self.handle_submit_select(select_action, &text);
            return;
        } else if let Some(Modal::Confirm(_)) = self.modal_stack.last() {
            // Confirm modal submit — check for delete project
            if text == "confirm" {
                self.modal_stack.pop();
                self.handle_confirm_delete();
                return;
            } else {
                self.modal_stack.pop();
                return;
            }
        } else {
            None
        };

        // Pop the modal
        self.modal_stack.pop();

        let text = text.trim().to_string();

        let Some(action) = input_action else { return };

        match action {
            InputAction::AddProject => {
                if text.is_empty() {
                    return;
                }
                match self.project_store.create_project(&text) {
                    Ok(project) => {
                        self.toast = Some(Toast::new(&format!("Created: {}", project.name)));
                        dashboard::refresh(&mut self.dashboard, &self.project_store);
                    }
                    Err(e) => {
                        self.toast = Some(Toast::new(&format!("Error: {e}")));
                    }
                }
            }

            InputAction::QuickNote => {
                let target_slug = self.targeted_project_slug();

                let Some(slug) = target_slug else {
                    self.toast = Some(Toast::new("No project selected."));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    return;
                };

                if !text.is_empty() {
                    // Add to project log
                    let today = Local::now().date_naive();
                    if let Some(entry) = project.log.iter_mut().find(|e| e.date == today) {
                        entry.lines.push(text.clone());
                    } else {
                        project.log.insert(
                            0,
                            LogEntry {
                                date: today,
                                lines: vec![text.clone()],
                            },
                        );
                    }
                    let _ = self.project_store.save_project(&mut project);

                    // Track @mentions
                    self.track_mentions(&text, &project.name);

                    // Journal entry
                    let time = Local::now().format("%H:%M").to_string();
                    let mut entry = JournalEntry::new(&time, "Note", &project.name);
                    entry.details.insert("note".to_string(), text.clone());
                    let _ = self.journal_store.append(entry);

                    self.toast = Some(Toast::new("Note added."));
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                }
            }

            InputAction::QuickBlocker => {
                let target_slug = self.targeted_project_slug();

                let Some(slug) = target_slug else {
                    self.toast = Some(Toast::new("No project selected."));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    return;
                };

                if !text.is_empty() {
                    // Extract first @mention for blocker's person field
                    let person = extract_mentions(&text).into_iter().next();
                    let today = Local::now().date_naive();

                    project.blockers.push(Blocker {
                        description: text.clone(),
                        person,
                        since: Some(today),
                        ..Default::default()
                    });
                    let _ = self.project_store.save_project(&mut project);

                    // Track all @mentions
                    self.track_mentions(&text, &project.name);

                    // Journal entry
                    let time = Local::now().format("%H:%M").to_string();
                    let mut entry = JournalEntry::new(&time, "Note", &project.name);
                    entry.details.insert("blocker".to_string(), text.clone());
                    let _ = self.journal_store.append(entry);

                    self.toast = Some(Toast::new("Blocker logged."));
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                }
            }

            InputAction::QuickDecision => {
                let target_slug = self.targeted_project_slug();

                let Some(slug) = target_slug else {
                    self.toast = Some(Toast::new("No project selected."));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    return;
                };

                if !text.is_empty() {
                    let today = Local::now().date_naive();
                    project.decisions.push(jm_core::models::Decision {
                        date: today,
                        choice: text.clone(),
                        alternatives: Vec::new(),
                    });
                    let _ = self.project_store.save_project(&mut project);

                    // Track @mentions
                    self.track_mentions(&text, &project.name);

                    // Journal entry
                    let time = Local::now().format("%H:%M").to_string();
                    let mut entry = JournalEntry::new(&time, "Note", &project.name);
                    entry.details.insert("decision".to_string(), text.clone());
                    let _ = self.journal_store.append(entry);

                    self.toast = Some(Toast::new("Decision logged."));
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                }
            }

            InputAction::EditFocus => {
                let Some(mut project) = self.current_project() else {
                    return;
                };
                project.current_focus = text.clone();
                let _ = self.project_store.save_project(&mut project);
                self.track_mentions(&text, &project.name);
                self.toast = Some(Toast::new("Focus updated."));
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            InputAction::EditTags => {
                let Some(mut project) = self.current_project() else {
                    return;
                };
                let tags: Vec<String> = text
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                project.tags = tags;
                let _ = self.project_store.save_project(&mut project);
                self.toast = Some(Toast::new("Tags updated."));
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            InputAction::EditTarget => {
                let Some(mut project) = self.current_project() else {
                    return;
                };
                if text.is_empty() {
                    project.target = None;
                    let _ = self.project_store.save_project(&mut project);
                    self.toast = Some(Toast::new("Target date cleared."));
                } else {
                    match chrono::NaiveDate::parse_from_str(&text, "%Y-%m-%d") {
                        Ok(date) => {
                            project.target = Some(date);
                            let _ = self.project_store.save_project(&mut project);
                            self.toast = Some(Toast::new(&format!("Target set to {date}.")));
                        }
                        Err(_) => {
                            self.toast =
                                Some(Toast::new("Invalid date. Use YYYY-MM-DD format."));
                        }
                    }
                }
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            InputAction::Unblock => {
                self.handle_unblock();
            }

            InputAction::MoveBlocker => {
                // Start the move-blocker flow: show source blocker selection list.
                let source_slug = self.targeted_project_slug();
                if let Some(slug) = source_slug {
                    if let Some(project) = self.project_store.get_project(&slug) {
                        let open_blockers: Vec<(usize, String)> = project
                            .blockers
                            .iter()
                            .enumerate()
                            .filter(|(_, b)| !b.resolved)
                            .map(|(i, b)| {
                                let person_part = b
                                    .person
                                    .as_deref()
                                    .map(|p| format!(" {p}"))
                                    .unwrap_or_default();
                                (i, format!("{}{person_part}", b.description))
                            })
                            .collect();

                        if open_blockers.is_empty() {
                            self.toast = Some(Toast::new("No open blockers to move."));
                        } else if open_blockers.len() == 1 {
                            // Only one blocker — skip source selection, go straight to dest
                            let blocker_idx = open_blockers[0].0;
                            self.move_blocker_slug = slug.clone();
                            self.move_blocker_source_idx = blocker_idx;
                            self.show_move_blocker_dest_modal(&slug, blocker_idx);
                        } else {
                            let items: Vec<String> =
                                open_blockers.iter().map(|(_, s)| s.clone()).collect();
                            // Store the real indices mapping
                            self.move_blocker_slug = slug.clone();
                            // Store open_indices in unblock_open_indices (reuse the field)
                            self.unblock_open_indices =
                                open_blockers.iter().map(|(i, _)| *i).collect();
                            self.modal_stack.push(Modal::Select(SelectModal::new(
                                "Move Blocker — Select Source",
                                items,
                                SelectAction::MoveBlockerSource,
                            )));
                        }
                    }
                }
            }

            InputAction::CommandMode => {
                self.handle_command_mode_input(&text);
            }

            InputAction::EodReflectShipped => {
                // Step 1 done: push step 2 with the shipped text captured.
                let shipped = text.clone();
                self.push_input_modal_with(
                    "End of Day — Most important thing for tomorrow?",
                    "(Enter to skip)",
                    "",
                    InputAction::EodReflectTomorrow(shipped),
                );
            }

            InputAction::EodReflectTomorrow(shipped) => {
                // Step 2 done: write reflection entry then do stop-work cleanup.
                let tomorrow_text = text.clone();
                if !shipped.is_empty() || !tomorrow_text.is_empty() {
                    let time = Local::now().format("%H:%M").to_string();
                    let project_name = self
                        .active_store
                        .get_active()
                        .and_then(|slug| {
                            self.project_store
                                .get_project(&slug)
                                .map(|p| p.name.clone())
                                .or(Some(slug))
                        })
                        .unwrap_or_default();

                    let mut entry = JournalEntry::new(&time, "Reflection", &project_name);
                    if !shipped.is_empty() {
                        entry.details.insert("shipped".to_string(), shipped);
                    }
                    if !tomorrow_text.is_empty() {
                        entry.details.insert("tomorrow".to_string(), tomorrow_text);
                    }
                    let _ = self.journal_store.append(entry);
                }
                // Now do the actual stop-work / EOD cleanup.
                self.handle_eod_stop_work();
            }

            InputAction::AddIssue => {
                if let Some(slug) = self.targeted_project_slug() {
                    match self.issue_store.create_issue(&slug, &text, None) {
                        Ok(issue) => {
                            self.toast = Some(Toast::new(&format!("Issue #{} created.", issue.id)));
                        }
                        Err(e) => {
                            self.toast = Some(Toast::new(&format!("Error: {e}")));
                        }
                    }
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                }
            }

            InputAction::AddSubIssue(parent_id) => {
                if let Some(slug) = self.targeted_project_slug() {
                    match self.issue_store.create_issue(&slug, &text, Some(parent_id)) {
                        Ok(issue) => {
                            self.toast = Some(Toast::new(&format!(
                                "Sub-issue #{} created under #{}.",
                                issue.id, parent_id
                            )));
                        }
                        Err(e) => {
                            self.toast = Some(Toast::new(&format!("Error: {e}")));
                        }
                    }
                }
            }

            InputAction::MeetingNote => {
                let slug = self.config.meetings_project.clone();
                if let Some(mut project) = self.project_store.get_project(&slug) {
                    if !text.is_empty() {
                        // Add to project log.
                        let today = Local::now().date_naive();
                        if let Some(entry) = project.log.iter_mut().find(|e| e.date == today) {
                            entry.lines.push(text.clone());
                        } else {
                            project.log.insert(
                                0,
                                LogEntry {
                                    date: today,
                                    lines: vec![text.clone()],
                                },
                            );
                        }
                        let _ = self.project_store.save_project(&mut project);

                        // Journal entry.
                        let time = Local::now().format("%H:%M").to_string();
                        let mut entry = JournalEntry::new(&time, "Note", &project.name);
                        entry.details.insert("note".to_string(), text.clone());
                        let _ = self.journal_store.append(entry);
                    }
                    self.toast = Some(Toast::new(&format!("Meeting: {text}")));
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                }
            }
        }
    }

    /// Handle selection from a SelectModal.
    fn handle_submit_select(&mut self, action: SelectAction, text: &str) {
        let selected_idx: usize = match text.trim().parse() {
            Ok(i) => i,
            Err(_) => return,
        };

        match action {
            SelectAction::ChooseBlocker => {
                // selected_idx is an index into self.unblock_open_indices
                if let Some(&real_idx) = self.unblock_open_indices.get(selected_idx) {
                    let slug = self.unblock_slug.clone();
                    self.resolve_blocker_at_index(&slug, real_idx);
                }
            }

            SelectAction::MoveBlockerSource => {
                // selected_idx is an index into self.unblock_open_indices (reused)
                if let Some(&real_idx) = self.unblock_open_indices.get(selected_idx) {
                    let slug = self.move_blocker_slug.clone();
                    self.move_blocker_source_idx = real_idx;
                    self.show_move_blocker_dest_modal(&slug, real_idx);
                }
            }

            SelectAction::MoveBlockerDest(source_blocker_idx) => {
                // selected_idx is an index into the project list (all projects
                // except the source).  We stored those in unblock_open_indices
                // as project indices in dashboard.projects.
                if let Some(&proj_idx) = self.unblock_open_indices.get(selected_idx) {
                    let source_slug = self.move_blocker_slug.clone();
                    let dest_slug = self
                        .dashboard
                        .projects
                        .get(proj_idx)
                        .map(|p| p.slug.clone());

                    if let Some(dest_slug) = dest_slug {
                        self.do_move_blocker(&source_slug, source_blocker_idx, &dest_slug);
                    }
                }
            }

            SelectAction::PickParentIssue => {
                // selected_idx maps to top-level issues
                if let Some(slug) = self.targeted_project_slug() {
                    let issue_file = self.issue_store.load(&slug);
                    let top_issues: Vec<&jm_core::models::Issue> = issue_file
                        .issues
                        .iter()
                        .filter(|i| i.parent_id.is_none())
                        .collect();
                    if let Some(parent) = top_issues.get(selected_idx) {
                        self.push_input_modal(
                            &format!("Sub-issue of #{} {}", parent.id, parent.title),
                            "Sub-issue title:",
                            InputAction::AddSubIssue(parent.id),
                        );
                    }
                }
            }

            SelectAction::PickIssueToCycle | SelectAction::PickIssueToCycleReverse => {
                let reverse = matches!(action, SelectAction::PickIssueToCycleReverse);
                if let Some(slug) = self.targeted_project_slug() {
                    let mut issue_file = self.issue_store.load(&slug);
                    let non_done: Vec<u32> = issue_file
                        .issues
                        .iter()
                        .filter(|i| i.status != jm_core::models::IssueStatus::Done)
                        .map(|i| i.id)
                        .collect();
                    if let Some(&issue_id) = non_done.get(selected_idx) {
                        if let Some(issue) = issue_file.issues.iter_mut().find(|i| i.id == issue_id) {
                            let new_status = if reverse { issue.status.cycle_reverse() } else { issue.status.cycle() };
                            issue.status = new_status;
                            if new_status == jm_core::models::IssueStatus::Done {
                                issue.closed = Some(chrono::Local::now().date_naive());
                            } else {
                                issue.closed = None;
                            }
                            let _ = self.issue_store.save(&issue_file);
                            self.toast = Some(Toast::new(&format!(
                                "#{issue_id} -> {new_status}"
                            )));
                        }
                    }
                }
            }

            SelectAction::PickIssueToClose => {
                if let Some(slug) = self.targeted_project_slug() {
                    let non_done: Vec<u32> = self.issue_store.load(&slug)
                        .issues
                        .iter()
                        .filter(|i| i.status != jm_core::models::IssueStatus::Done)
                        .map(|i| i.id)
                        .collect();
                    if let Some(&issue_id) = non_done.get(selected_idx) {
                        let _ = self.issue_store.set_status(
                            &slug,
                            issue_id,
                            jm_core::models::IssueStatus::Done,
                        );
                        self.toast = Some(Toast::new(&format!("#{issue_id} closed.")));
                    }
                }
            }

            SelectAction::PinIssue => {
                // Index 0 = clear pin; index 1+ = pin issue at (selected_idx - 1)
                if let Some(slug) = self.targeted_project_slug() {
                    if let Some(mut project) = self.project_store.get_project(&slug) {
                        if selected_idx == 0 {
                            project.active_issue = None;
                            let _ = self.project_store.save_project(&mut project);
                            dashboard::refresh(&mut self.dashboard, &self.project_store);
                            self.toast = Some(Toast::new("Active issue pin cleared."));
                        } else {
                            let issue_file = self.issue_store.load(&slug);
                            let open_issues: Vec<&jm_core::models::Issue> = issue_file
                                .issues
                                .iter()
                                .filter(|i| i.status != jm_core::models::IssueStatus::Done)
                                .collect();
                            if let Some(issue) = open_issues.get(selected_idx - 1) {
                                let issue_id = issue.id;
                                let issue_title = issue.title.clone();
                                project.active_issue = Some(issue_id);
                                let _ = self.project_store.save_project(&mut project);
                                dashboard::refresh(&mut self.dashboard, &self.project_store);
                                self.toast = Some(Toast::new(&format!("Pinned #{issue_id} {issue_title}")));
                            }
                        }
                    }
                }
            }
            SelectAction::NoteToIssue => {
                if let Some(slug) = self.targeted_project_slug() {
                    if let Some(project) = self.project_store.get_project(&slug) {
                        let notes = Self::collect_recent_notes(&project);
                        if let Some(text) = notes.get(selected_idx) {
                            let text = text.clone();
                            match self.issue_store.create_issue(&slug, &text, None) {
                                Ok(issue) => {
                                    self.toast = Some(Toast::new(&format!(
                                        "Created issue #{}: {}",
                                        issue.id, issue.title
                                    )));
                                }
                                Err(e) => {
                                    self.toast = Some(Toast::new(&format!("Error: {e}")));
                                }
                            }
                        }
                    }
                }
            }
            SelectAction::StopWorkChoice => {
                match selected_idx {
                    0 => self.handle_pause("Break", "Break logged."),
                    1 => self.handle_pause("Lunch", "Lunch logged."),
                    2 => {
                        self.push_input_modal(
                            "End of Day — What did you ship today?",
                            "(Enter to skip)",
                            InputAction::EodReflectShipped,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// Show the destination-project selection modal for MoveBlocker step 2.
    fn show_move_blocker_dest_modal(&mut self, source_slug: &str, blocker_idx: usize) {
        // Build a list of all projects except the source, storing their
        // indices into dashboard.projects in self.unblock_open_indices.
        let items: Vec<(usize, String)> = self
            .dashboard
            .projects
            .iter()
            .enumerate()
            .filter(|(_, p)| p.slug != source_slug)
            .map(|(i, p)| (i, p.name.clone()))
            .collect();

        if items.is_empty() {
            self.toast = Some(Toast::new("No other projects to move blocker to."));
            return;
        }

        self.unblock_open_indices = items.iter().map(|(i, _)| *i).collect();
        let names: Vec<String> = items.into_iter().map(|(_, n)| n).collect();
        self.modal_stack.push(Modal::Select(SelectModal::new(
            "Move Blocker — Select Destination",
            names,
            SelectAction::MoveBlockerDest(blocker_idx),
        )));
    }

    /// Move a blocker from one project to another.
    fn do_move_blocker(&mut self, source_slug: &str, blocker_idx: usize, dest_slug: &str) {
        let Some(mut source_project) = self.project_store.get_project(source_slug) else {
            return;
        };
        let Some(mut dest_project) = self.project_store.get_project(dest_slug) else {
            return;
        };

        if blocker_idx >= source_project.blockers.len() {
            self.toast = Some(Toast::new("Blocker index out of range."));
            return;
        }

        let blocker = source_project.blockers.remove(blocker_idx);
        let blocker_desc = blocker.description.clone();
        dest_project.blockers.push(blocker);

        if let Err(e) = self.project_store.save_project(&mut source_project) {
            self.toast = Some(Toast::new(&format!("Error saving source: {e}")));
            return;
        }
        if let Err(e) = self.project_store.save_project(&mut dest_project) {
            self.toast = Some(Toast::new(&format!("Error saving dest: {e}")));
            return;
        }

        self.toast = Some(Toast::new(&format!(
            "Moved blocker \"{}\" to {}.",
            blocker_desc, dest_project.name
        )));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Handle a confirmed delete project action.
    fn handle_confirm_delete(&mut self) {
        if let ScreenId::ProjectView(ref slug) = self.screen.clone() {
            if self.project_store.delete_project(slug) {
                // Also clean up associated issues file
                let issues_path = self.issue_store.issues_dir.join(format!("{slug}.md"));
                if issues_path.exists() {
                    let _ = std::fs::remove_file(&issues_path);
                }
                self.toast = Some(Toast::new("Project deleted."));
                // If this was the active project, clear it
                if self.active_store.get_active().as_deref() == Some(slug) {
                    self.active_store.clear_active();
                }
                self.screen = ScreenId::Dashboard;
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            } else {
                self.toast = Some(Toast::new("Failed to delete project."));
            }
        }
    }

    /// Handle the PinIssue action — pin (or clear) the active issue for the current project.
    fn handle_pin_issue(&mut self) {
        let Some(slug) = self.targeted_project_slug() else {
            return;
        };
        let issue_file = self.issue_store.load(&slug);
        let open_issues: Vec<&jm_core::models::Issue> = issue_file
            .issues
            .iter()
            .filter(|i| i.status != jm_core::models::IssueStatus::Done)
            .collect();

        match open_issues.len() {
            0 => {
                self.toast = Some(Toast::new("No open issues to pin."));
            }
            1 => {
                let issue_id = open_issues[0].id;
                let issue_title = open_issues[0].title.clone();
                if let Some(mut project) = self.project_store.get_project(&slug) {
                    project.active_issue = Some(issue_id);
                    let _ = self.project_store.save_project(&mut project);
                    dashboard::refresh(&mut self.dashboard, &self.project_store);
                    self.toast = Some(Toast::new(&format!("Pinned #{issue_id} {issue_title}")));
                }
            }
            _ => {
                // Build items list: first entry is "Clear pin", then open issues
                let mut items = vec!["Clear pin".to_string()];
                for issue in &open_issues {
                    items.push(format!("#{} {}", issue.id, issue.title));
                }
                self.modal_stack.push(Modal::Select(SelectModal::new(
                    "Pin Active Issue",
                    items,
                    SelectAction::PinIssue,
                )));
            }
        }
    }

    /// Complete the context switch flow after all steps are filled in.
    fn handle_switch_complete(&mut self) {
        let state = self.switch_state.clone();

        // Log context switch to journal for the old project
        if let Some(old_slug) = self.active_store.get_active() {
            if let Some(old_project) = self.project_store.get_project(&old_slug) {
                let time = Local::now().format("%H:%M").to_string();
                let target_name = state
                    .target_slug
                    .as_ref()
                    .and_then(|s| self.project_store.get_project(s))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "—".to_string());

                let switch_label = format!("{} \u{2192} {target_name}", old_project.name);
                let mut entry = JournalEntry::new(&time, "Switched", &switch_label);
                if !state.left_off.is_empty() {
                    entry
                        .details
                        .insert("left_off".to_string(), state.left_off.clone());
                }
                if !state.blocker.is_empty() {
                    entry
                        .details
                        .insert("blocker".to_string(), state.blocker.clone());
                }
                if !state.next_step.is_empty() {
                    entry
                        .details
                        .insert("next_step".to_string(), state.next_step.clone());
                }
                let _ = self.journal_store.append(entry);

                // Update old project's focus to next_step
                let mut old = old_project.clone();
                if !state.next_step.is_empty() {
                    old.current_focus = state.next_step.clone();
                    let _ = self.project_store.save_project(&mut old);
                }

                // Add blocker if provided
                if !state.blocker.is_empty() {
                    let person = extract_mentions(&state.blocker).into_iter().next();
                    old.blockers.push(Blocker {
                        description: state.blocker.clone(),
                        person,
                        since: Some(Local::now().date_naive()),
                        ..Default::default()
                    });
                    let _ = self.project_store.save_project(&mut old);
                }

                // Track @mentions from all switch fields
                let project_name = &old_project.name;
                self.track_mentions(&state.left_off, project_name);
                self.track_mentions(&state.blocker, project_name);
                self.track_mentions(&state.next_step, project_name);
            }
        }

        // Switch to the new project
        if let Some(new_slug) = state.target_slug {
            if let Some(new_project) = self.project_store.get_project(&new_slug) {
                let _ = self.active_store.set_active(&new_slug);

                let time = Local::now().format("%H:%M").to_string();
                let mut entry = JournalEntry::new(&time, "Started", &new_project.name);
                if !new_project.current_focus.is_empty() {
                    entry.details.insert(
                        "focus".to_string(),
                        new_project.current_focus.clone(),
                    );
                    self.track_mentions(&new_project.current_focus, &new_project.name);
                }
                let _ = self.journal_store.append(entry);

                let msg = format!("Switched to: {}", new_project.name);
                self.toast = Some(Toast::new(&msg));
            }
        }

        self.screen = ScreenId::Dashboard;
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    /// Save captured switch context to the current project log without
    /// switching to another project, then clear the active project.
    /// This is triggered by pressing Escape at the SelectProject step when
    /// context (left_off / blocker / next_step) has been entered.
    fn handle_save_context_only(&mut self) {
        let state = self.switch_state.clone();

        if let Some(old_slug) = self.active_store.get_active() {
            if let Some(mut old_project) = self.project_store.get_project(&old_slug) {
                let time = Local::now().format("%H:%M").to_string();
                let mut entry = JournalEntry::new(&time, "Context Saved", &old_project.name);
                if !state.left_off.is_empty() {
                    entry
                        .details
                        .insert("left_off".to_string(), state.left_off.clone());
                }
                if !state.blocker.is_empty() {
                    entry
                        .details
                        .insert("blocker".to_string(), state.blocker.clone());
                }
                if !state.next_step.is_empty() {
                    entry
                        .details
                        .insert("next_step".to_string(), state.next_step.clone());
                }
                let _ = self.journal_store.append(entry);

                // Persist next_step as the current focus so it surfaces on resume.
                if !state.next_step.is_empty() {
                    old_project.current_focus = state.next_step.clone();
                    let _ = self.project_store.save_project(&mut old_project);
                }

                // Add blocker if provided.
                if !state.blocker.is_empty() {
                    let person = extract_mentions(&state.blocker).into_iter().next();
                    old_project.blockers.push(Blocker {
                        description: state.blocker.clone(),
                        person,
                        since: Some(Local::now().date_naive()),
                        ..Default::default()
                    });
                    let _ = self.project_store.save_project(&mut old_project);
                }

                // Track @mentions.
                let project_name = old_project.name.clone();
                self.track_mentions(&state.left_off, &project_name);
                self.track_mentions(&state.blocker, &project_name);
                self.track_mentions(&state.next_step, &project_name);

                self.toast = Some(Toast::new(&format!("Context saved for {project_name}")));
            }
        }

        // Clear active project — the user is done for now.
        self.active_store.clear_active();
        self.screen = ScreenId::Dashboard;
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }

    // ── @mention tracking ─────────────────────────────────────────

    /// Track all @mentions in a text string, associating them with a project.
    fn track_mentions(&self, text: &str, project_name: &str) {
        for handle in extract_mentions(text) {
            let person = jm_core::models::Person {
                handle,
                role: String::new(),
                projects: vec![project_name.to_string()],
                pending: Vec::new(),
            };
            let _ = self.people_store.add_or_update_person(person);
        }
    }

    // ── Idle reminder ───────────────────────────────────────────────

    fn should_show_idle_reminder(&self) -> bool {
        // Don't show if there's already a modal open
        if !self.modal_stack.is_empty() {
            return false;
        }
        // Don't show if not on the dashboard
        if !matches!(self.screen, ScreenId::Dashboard) {
            return false;
        }
        // Don't show within 60 seconds of app launch
        if self.app_started_at.elapsed() < Duration::from_secs(60) {
            return false;
        }
        // Don't show if there IS an active project
        if self.active_store.get_active().is_some() {
            return false;
        }
        // Don't show outside working hours (8am–6pm)
        let hour = Local::now().hour();
        if hour < 8 || hour >= 18 {
            return false;
        }
        // Don't show if dismissed less than 5 minutes ago
        if let Some(dismissed) = self.idle_dismissed_at {
            if dismissed.elapsed() < Duration::from_secs(300) {
                return false;
            }
        }
        true
    }

    // ── Modal convenience helpers ────────────────────────────────────

    fn push_input_modal(&mut self, title: &str, prompt: &str, on_submit: InputAction) {
        self.modal_stack
            .push(Modal::Input(InputModal::new(title, prompt, on_submit)));
    }

    fn push_input_modal_with(
        &mut self,
        title: &str,
        prompt: &str,
        initial: &str,
        on_submit: InputAction,
    ) {
        self.modal_stack.push(Modal::Input(InputModal::with_initial(
            title, prompt, initial, on_submit,
        )));
    }

    // ── Command palette ───────────────────────────────────────────────

    /// Parse and execute a command-palette input string.
    ///
    /// Supported commands (leading colon is stripped):
    ///   work <slug>   — start working on project
    ///   note <text>   — add note to active/selected project
    ///   block <text>  — log a blocker on active/selected project
    ///   park <slug>   — set project status to parked
    ///   done          — end of day (stop work)
    ///   q             — quit
    fn handle_command_mode_input(&mut self, raw: &str) {
        let input = raw.trim_start_matches(':').trim();
        if input.is_empty() {
            return;
        }

        let (cmd, rest) = match input.find(char::is_whitespace) {
            Some(pos) => (&input[..pos], input[pos + 1..].trim()),
            None => (input, ""),
        };

        match cmd {
            "q" | "quit" => {
                self.should_quit = true;
            }

            "done" => {
                self.handle_eod_stop_work();
            }

            "work" => {
                if rest.is_empty() {
                    self.toast = Some(Toast::new("Usage: :work <slug>"));
                    return;
                }
                let slug = if self.project_store.get_project(rest).is_some() {
                    rest.to_string()
                } else {
                    let projects = self.project_store.list_projects(None);
                    let lower = rest.to_lowercase();
                    match projects
                        .iter()
                        .find(|p| p.slug.contains(&lower) || p.name.to_lowercase().contains(&lower))
                    {
                        Some(p) => p.slug.clone(),
                        None => {
                            self.toast = Some(Toast::new(&format!("No project matching '{rest}'")));
                            return;
                        }
                    }
                };
                if let Some(idx) = self.dashboard.projects.iter().position(|p| p.slug == slug) {
                    self.dashboard.selected = idx;
                }
                self.handle_start_work_on_slug(&slug);
            }

            "note" => {
                if rest.is_empty() {
                    self.toast = Some(Toast::new("Usage: :note <text>"));
                    return;
                }
                let target_slug = match &self.screen {
                    ScreenId::ProjectView(slug) => Some(slug.clone()),
                    _ => self.active_store.get_active(),
                };
                let Some(slug) = target_slug else {
                    self.toast = Some(Toast::new("No active project. Use :work <slug> first."));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    return;
                };
                let text = rest.to_string();
                let today = Local::now().date_naive();
                if let Some(entry) = project.log.iter_mut().find(|e| e.date == today) {
                    entry.lines.push(text.clone());
                } else {
                    project.log.insert(
                        0,
                        LogEntry {
                            date: today,
                            lines: vec![text.clone()],
                        },
                    );
                }
                let _ = self.project_store.save_project(&mut project);
                self.track_mentions(&text, &project.name);
                let time = Local::now().format("%H:%M").to_string();
                let mut entry = JournalEntry::new(&time, "Note", &project.name);
                entry.details.insert("note".to_string(), text.clone());
                let _ = self.journal_store.append(entry);
                self.toast = Some(Toast::new("Note added."));
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            "block" => {
                if rest.is_empty() {
                    self.toast = Some(Toast::new("Usage: :block <text>"));
                    return;
                }
                let target_slug = match &self.screen {
                    ScreenId::ProjectView(slug) => Some(slug.clone()),
                    _ => self.active_store.get_active(),
                };
                let Some(slug) = target_slug else {
                    self.toast = Some(Toast::new("No active project. Use :work <slug> first."));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    return;
                };
                let text = rest.to_string();
                let person = extract_mentions(&text).into_iter().next();
                project.blockers.push(Blocker {
                    description: text.clone(),
                    person,
                    since: Some(Local::now().date_naive()),
                    ..Default::default()
                });
                let _ = self.project_store.save_project(&mut project);
                self.track_mentions(&text, &project.name);
                self.toast = Some(Toast::new("Blocker logged."));
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            "park" => {
                let target = if rest.is_empty() {
                    match &self.screen {
                        ScreenId::ProjectView(slug) => Some(slug.clone()),
                        ScreenId::Dashboard => self
                            .dashboard
                            .projects
                            .get(self.dashboard.selected)
                            .map(|p| p.slug.clone()),
                        _ => None,
                    }
                } else {
                    Some(rest.to_string())
                };
                let Some(slug) = target else {
                    self.toast = Some(Toast::new(
                        "Usage: :park <slug>  (or navigate to a project first)",
                    ));
                    return;
                };
                let Some(mut project) = self.project_store.get_project(&slug) else {
                    self.toast = Some(Toast::new(&format!("Project '{slug}' not found.")));
                    return;
                };
                project.status = Status::Parked;
                let _ = self.project_store.save_project_raw(&project);
                self.toast = Some(Toast::new(&format!("{} parked.", project.name)));
                dashboard::refresh(&mut self.dashboard, &self.project_store);
            }

            _ => {
                self.toast = Some(Toast::new(&format!("Unknown command: :{cmd}")));
            }
        }
    }

    /// Start work on a project by slug (used by command palette).
    fn handle_start_work_on_slug(&mut self, slug: &str) {
        let Some(project) = self.project_store.get_project(slug) else {
            self.toast = Some(Toast::new(&format!("Project '{slug}' not found.")));
            return;
        };
        if let Some(active) = self.active_store.get_active() {
            if active == slug {
                self.toast = Some(Toast::new(&format!("Already working on: {}", project.name)));
                return;
            }
            // Different project active — route through context-switch wizard.
            // When switching to the meetings project, skip the 3-step capture.
            if slug == self.config.meetings_project {
                self.switch_state = switch::init_skip(slug);
                self.handle_switch_complete();
            } else {
                self.switch_state = switch::init(Some(slug));
                self.screen = ScreenId::Switch(Some(slug.to_string()));
            }
            return;
        }
        let _ = self.active_store.set_active(slug);
        let time = Local::now().format("%H:%M").to_string();
        let mut entry = JournalEntry::new(&time, "Started", &project.name);
        if !project.current_focus.is_empty() {
            entry
                .details
                .insert("focus".to_string(), project.current_focus.clone());
            self.track_mentions(&project.current_focus, &project.name);
        }
        let _ = self.journal_store.append(entry);
        self.toast = Some(Toast::new(&format!("Working on: {}", project.name)));
        dashboard::refresh(&mut self.dashboard, &self.project_store);
    }
}

// ── Utilities ────────────────────────────────────────────────────────

/// Extract all @mentions from a string.
fn extract_mentions(text: &str) -> Vec<String> {
    let Some(re) = regex::Regex::new(r"@([\w-]+)").ok() else {
        return Vec::new();
    };
    re.captures_iter(text)
        .map(|caps| format!("@{}", &caps[1]))
        .collect()
}
