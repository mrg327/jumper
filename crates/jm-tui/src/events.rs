//! Event types and action enum for the TUI.
//! All state mutations flow through the Action enum.

use crossterm::event::KeyEvent;

/// Focus state — which panel has keyboard focus.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Focus {
    Main,
    Sidebar(usize), // index into plugin list
    Modal,
}

/// Which screen is currently displayed.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenId {
    Dashboard,
    ProjectView(String), // slug
    Switch(Option<String>), // optional target slug
    Review,
    Search,
    People,
    IssueBoard,
}

/// Modal types that can be pushed onto the popup stack.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ModalId {
    Help,
    AddProject,
    QuickNote,
    QuickBlocker,
    QuickDecision,
    Unblock(usize),            // blocker index
    EditFocus,
    EditTags,
    EditTarget,
    MoveBlocker(usize),        // blocker index
    BlockerAction(usize),      // blocker index
    DeleteConfirm(String),     // project slug
    BreakOptions,
    CommandMode,               // :command
    AddIssue,                  // new issue input
    AddSubIssue,               // sub-issue: pick parent, then input
    SelectIssue(String),       // action label: "cycle", "close"
}

/// Actions that flow through the central update() function.
/// Every key press ultimately maps to one of these.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Action {
    None,
    Quit,

    // Navigation
    Down,
    Up,
    Top,       // g
    Bottom,    // G
    HalfPageDown, // Ctrl+D
    HalfPageUp,   // Ctrl+U

    // Selection
    Select,    // Enter
    Back,      // Escape

    // Dashboard actions
    StartWork,
    SwitchContext,
    QuickNote,
    QuickBlocker,
    QuickDecision,
    Unblock,
    LogDecision,
    SearchOpen,
    MorningReview,
    PeopleView,
    AddProject,
    Help,
    StopWork,
    Export,

    // Sidebar
    ToggleSidebar,
    FocusSidebar,

    // Issue board
    OpenIssueBoard,

    // Project view actions
    EditFocus,
    CycleStatus,
    CyclePriority,
    EditTags,
    EditTarget,
    DeleteProject,
    MoveBlocker,

    // Issue actions
    AddIssue,
    AddSubIssue,
    CycleIssueStatus,
    CycleIssueStatusReverse,
    CloseIssue,

    // Open project file in $EDITOR
    OpenEditor,          // from ProjectView: open current project (o)
    OpenEditorSelected,  // from Dashboard: open cursor-selected project (O)

    // Command palette
    OpenCommandMode,     // ':' — open command input at the bottom

    // Modal results
    SubmitInput(String),
    Cancel,
    /// Save switch context to the current project log without switching.
    /// Used when Escape is pressed at SelectProject step with captured context.
    SaveContextOnly,
    /// Emitted when the context-switch wizard completes all steps.
    /// Replaces the `SubmitInput("switch_complete")` string sentinel.
    SwitchComplete,

    // Screen/modal stack
    PushScreen(ScreenId),
    PopScreen,
    PushModal(ModalId),
    PopModal,

    // Toast
    Toast(String),

    // Tick (1-second timer)
    Tick,

    // Raw key passthrough (for text input, plugin keys)
    RawKey(KeyEvent),
}
