use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default = "default_enabled_plugins")]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub pomodoro: PomodoroConfig,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled_plugins(),
            notifications: NotificationsConfig::default(),
            pomodoro: PomodoroConfig::default(),
        }
    }
}

fn default_enabled_plugins() -> Vec<String> {
    vec![
        "pomodoro".to_string(),
        "notifications".to_string(),
        "clock".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub reminders: Vec<ReminderConfig>,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            reminders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderConfig {
    pub time: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroConfig {
    #[serde(default = "default_work_minutes")]
    pub work_minutes: u32,
    #[serde(default = "default_short_break")]
    pub short_break_minutes: u32,
    #[serde(default = "default_long_break")]
    pub long_break_minutes: u32,
    #[serde(default = "default_sessions")]
    pub sessions_before_long: u32,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            work_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            sessions_before_long: 4,
        }
    }
}

fn default_work_minutes() -> u32 {
    25
}
fn default_short_break() -> u32 {
    5
}
fn default_long_break() -> u32 {
    15
}
fn default_sessions() -> u32 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Hour (24h) at which the morning window starts (inclusive). Default: 6.
    #[serde(default = "default_morning_start")]
    pub morning_start: u32,
    /// Hour (24h) at which the morning window ends (exclusive). Default: 11.
    #[serde(default = "default_morning_end")]
    pub morning_end: u32,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            morning_start: default_morning_start(),
            morning_end: default_morning_end(),
        }
    }
}

fn default_morning_start() -> u32 { 6 }
fn default_morning_end() -> u32 { 11 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_statuses")]
    pub statuses: Vec<String>,
    #[serde(default = "default_priorities")]
    pub priorities: Vec<String>,
    #[serde(default = "default_export_path")]
    pub export_path: String,
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub review: ReviewConfig,
    /// Map of project slug → git repo path (for `jm standup` git log)
    #[serde(default)]
    pub git_paths: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            statuses: default_statuses(),
            priorities: default_priorities(),
            export_path: default_export_path(),
            plugins: PluginConfig::default(),
            review: ReviewConfig::default(),
            git_paths: HashMap::new(),
        }
    }
}

fn default_data_dir() -> String {
    "~/.jm".to_string()
}
fn default_statuses() -> Vec<String> {
    vec![
        "active".to_string(),
        "blocked".to_string(),
        "pending".to_string(),
        "parked".to_string(),
        "done".to_string(),
    ]
}
fn default_priorities() -> Vec<String> {
    vec![
        "high".to_string(),
        "medium".to_string(),
        "low".to_string(),
    ]
}
fn default_export_path() -> String {
    "~/.jm/screen.txt".to_string()
}

impl Config {
    pub fn load() -> Self {
        let config_path = expand_tilde("~/.jm/config.yaml");
        if config_path.exists() {
            if let Ok(text) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_yml::from_str(&text) {
                    return config;
                }
            }
        }
        Config::default()
    }

    pub fn data_dir(&self) -> PathBuf {
        expand_tilde(&self.data_dir)
    }

    pub fn export_path(&self) -> PathBuf {
        expand_tilde(&self.export_path)
    }
}

/// Expand `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_home() {
            return home.join(rest);
        }
    } else if path == "~" {
        if let Some(home) = dirs_home() {
            return home;
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Ensure the data directory and subdirectories exist. Returns the data_dir path.
pub fn ensure_dirs(config: &Config) -> PathBuf {
    let data_dir = config.data_dir();
    fs::create_dir_all(data_dir.join("projects")).ok();
    fs::create_dir_all(data_dir.join("journal")).ok();
    fs::create_dir_all(data_dir.join("issues")).ok();
    data_dir
}
