//! Passive time tracking derived from journal entries.
//!
//! Computes per-project session durations from Started/Switched/Done timestamps.
//! No new data entry required — everything is inferred from existing journal data.

use chrono::{Duration, NaiveTime};

use crate::models::DailyJournal;

/// A single work session on a project.
#[derive(Debug, Clone)]
pub struct Session {
    pub project: String,
    pub start: NaiveTime,
    pub end: Option<NaiveTime>,
}

impl Session {
    pub fn duration(&self, now: NaiveTime) -> Duration {
        let end = self.end.unwrap_or(now);
        end.signed_duration_since(self.start)
    }
}

/// Walk journal entries and compute sessions.
///
/// Logic:
/// - "Started" opens a new session for entry.project
/// - "Switched" closes the current open session and opens a new one
///   (the project field is "Old → New", we parse both sides)
/// - "Done"/"Break"/"Lunch" closes the current open session
pub fn compute_sessions(journal: &DailyJournal) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();
    let mut current_project: Option<String> = None;
    let mut current_start: Option<NaiveTime> = None;

    for entry in &journal.entries {
        let time = match parse_time(&entry.time) {
            Some(t) => t,
            None => continue,
        };

        match entry.entry_type.as_str() {
            "Started" => {
                // Close any open session
                if let (Some(proj), Some(start)) = (current_project.take(), current_start.take()) {
                    sessions.push(Session {
                        project: proj,
                        start,
                        end: Some(time),
                    });
                }
                current_project = Some(entry.project.clone());
                current_start = Some(time);
            }
            "Switched" => {
                // Close current session
                if let (Some(proj), Some(start)) = (current_project.take(), current_start.take()) {
                    sessions.push(Session {
                        project: proj,
                        start,
                        end: Some(time),
                    });
                }
                // Parse "Old → New" to find the new project
                let new_project = parse_switch_target(&entry.project)
                    .unwrap_or_else(|| entry.project.clone());
                current_project = Some(new_project);
                current_start = Some(time);
            }
            "Done" | "Break" | "Lunch" => {
                // Close current session
                if let (Some(proj), Some(start)) = (current_project.take(), current_start.take()) {
                    sessions.push(Session {
                        project: proj,
                        start,
                        end: Some(time),
                    });
                }
            }
            _ => {} // Note, etc. — don't affect sessions
        }
    }

    // If a session is still open, leave end=None (active session)
    if let (Some(proj), Some(start)) = (current_project, current_start) {
        sessions.push(Session {
            project: proj,
            start,
            end: None,
        });
    }

    sessions
}

/// Aggregate sessions by project, returning (project_name, total_duration).
/// Sorted by total time descending.
pub fn aggregate_sessions(sessions: &[Session], now: NaiveTime) -> Vec<(String, Duration)> {
    let mut map: std::collections::HashMap<String, Duration> = std::collections::HashMap::new();

    for session in sessions {
        let dur = session.duration(now);
        let entry = map.entry(session.project.clone()).or_insert(Duration::zero());
        *entry = *entry + dur;
    }

    let mut result: Vec<(String, Duration)> = map.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1));
    result
}

/// Get the elapsed duration of the currently active (open) session, if any.
pub fn active_session_elapsed(sessions: &[Session], now: NaiveTime) -> Option<Duration> {
    sessions
        .last()
        .filter(|s| s.end.is_none())
        .map(|s| now.signed_duration_since(s.start))
}

/// Format a duration as "Xh Ym" or "Ym".
pub fn format_duration(dur: Duration) -> String {
    let total_mins = dur.num_minutes();
    if total_mins < 0 {
        return "0m".to_string();
    }
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    if hours > 0 {
        format!("{hours}h {mins:02}m")
    } else {
        format!("{mins}m")
    }
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Parse "Old → New" from a Switched entry's project field.
/// The arrow can be → (U+2192) or thin-space variants.
fn parse_switch_target(project: &str) -> Option<String> {
    // Try unicode arrow first
    if let Some((_before, after)) = project.split_once('\u{2192}') {
        return Some(after.trim().to_string());
    }
    // Try ASCII arrow
    if let Some((_before, after)) = project.split_once("->") {
        return Some(after.trim().to_string());
    }
    None
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DailyJournal, JournalEntry};
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn test_single_session() {
        let journal = DailyJournal {
            date: d(2026, 3, 17),
            entries: vec![
                JournalEntry::new("09:00", "Started", "Project A"),
                JournalEntry::new("12:00", "Done", ""),
            ],
        };
        let sessions = compute_sessions(&journal);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project, "Project A");
        assert_eq!(sessions[0].duration(t(12, 0)).num_minutes(), 180);
    }

    #[test]
    fn test_switch_sessions() {
        let journal = DailyJournal {
            date: d(2026, 3, 17),
            entries: vec![
                JournalEntry::new("09:00", "Started", "Project A"),
                JournalEntry::new("10:30", "Switched", "Project A \u{2192} Project B"),
                JournalEntry::new("11:30", "Started", "Project B"),
                JournalEntry::new("12:00", "Done", ""),
            ],
        };
        let sessions = compute_sessions(&journal);
        // Session 1: A 09:00-10:30 (90min)
        // Session 2: B from Switched 10:30-11:30 (the Started at 11:30 closes that and opens new)
        // Session 3: B 11:30-12:00 (30min)
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].project, "Project A");
        assert_eq!(sessions[0].duration(t(12, 0)).num_minutes(), 90);

        let agg = aggregate_sessions(&sessions, t(12, 0));
        // B = 60+30 = 90min total, A = 90min
        assert_eq!(agg.len(), 2);
    }

    #[test]
    fn test_open_session() {
        let journal = DailyJournal {
            date: d(2026, 3, 17),
            entries: vec![JournalEntry::new("09:00", "Started", "Project A")],
        };
        let sessions = compute_sessions(&journal);
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].end.is_none());

        let elapsed = active_session_elapsed(&sessions, t(10, 30));
        assert_eq!(elapsed.unwrap().num_minutes(), 90);
    }

    #[test]
    fn test_empty_journal() {
        let journal = DailyJournal::new(d(2026, 3, 17));
        let sessions = compute_sessions(&journal);
        assert!(sessions.is_empty());
        assert!(active_session_elapsed(&sessions, t(12, 0)).is_none());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::minutes(0)), "0m");
        assert_eq!(format_duration(Duration::minutes(45)), "45m");
        assert_eq!(format_duration(Duration::minutes(90)), "1h 30m");
        assert_eq!(format_duration(Duration::minutes(125)), "2h 05m");
    }

    #[test]
    fn test_parse_switch_target() {
        assert_eq!(
            parse_switch_target("Project A \u{2192} Project B"),
            Some("Project B".to_string())
        );
        assert_eq!(
            parse_switch_target("Foo -> Bar"),
            Some("Bar".to_string())
        );
        assert_eq!(parse_switch_target("Just a name"), None);
    }
}
