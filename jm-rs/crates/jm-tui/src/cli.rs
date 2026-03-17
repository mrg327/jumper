use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jm", about = "Job Manager TUI", version = "0.1.0")]
pub struct Cli {
    /// Export current state to stdout
    #[arg(long)]
    pub dump: bool,

    /// Export to file instead of stdout (use with --dump)
    #[arg(short, long)]
    pub output: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a quick note to active project
    Note {
        /// Note text
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },

    /// Log a blocker on active project
    Block {
        /// Blocker description
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },

    /// Switch active project, prompting to capture context before switching
    Switch {
        /// Project slug to switch to
        project_name: String,
        /// Skip context-capture prompts (silent switch, for scripting)
        #[arg(long)]
        no_capture: bool,
    },

    /// Show current status (one-line)
    Status,

    /// Start working on a project
    Work {
        /// Project slug
        project_name: Option<String>,
    },

    /// Take a break (15min, lunch, or end of day)
    Break {
        /// Break type
        #[arg(default_value = "eod", value_parser = ["15min", "lunch", "eod"])]
        r#type: String,
    },

    /// End of day — log done, optionally record a reflection
    Done {
        /// What you shipped / completed today
        #[arg(long)]
        reflect: Option<String>,

        /// Most important thing for tomorrow
        #[arg(long)]
        tomorrow: Option<String>,
    },

    /// Create a new project
    Add {
        /// Project name
        name: String,

        /// Initial status
        #[arg(long, default_value = "active", value_parser = ["active", "blocked", "pending", "parked", "done"])]
        status: String,

        /// Initial priority
        #[arg(long, default_value = "medium", value_parser = ["high", "medium", "low"])]
        priority: String,

        /// Comma-separated tags
        #[arg(long, default_value = "")]
        tags: String,
    },

    /// List all projects
    List {
        /// Filter by status
        #[arg(long, value_parser = ["active", "blocked", "pending", "parked", "done"])]
        status: Option<String>,
    },

    /// Change a project's status
    #[command(name = "set-status")]
    SetStatus {
        /// Project slug
        project_slug: String,
        /// New status
        #[arg(value_parser = ["active", "blocked", "pending", "parked", "done"])]
        status: String,
    },

    /// Show time tracked today (or for a given date)
    Time {
        /// Date (YYYY-MM-DD), defaults to today
        date: Option<String>,
    },

    /// Generate standup report from journal + git
    Standup {
        /// Date (YYYY-MM-DD), defaults to today
        date: Option<String>,
    },

    /// Capture a quick thought to the inbox
    Inbox {
        /// Text to capture
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },

    /// Show cross-references for a project
    Refs {
        /// Project slug
        slug: String,
    },

    /// Change a project's priority
    #[command(name = "set-priority")]
    SetPriority {
        /// Project slug
        project_slug: String,
        /// New priority
        #[arg(value_parser = ["high", "medium", "low"])]
        priority: String,
    },
}
