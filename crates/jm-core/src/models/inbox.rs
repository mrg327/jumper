//! Inbox model: quick capture without project association.
//!
//! Stored as simple timestamped lines in `~/.jm/inbox.md`.
//! Format: `- 2026-03-17 09:15 | some thought`
//! Refiled: `- ~~2026-03-17 09:15 | thought~~ -> project-slug`

use chrono::Local;
use regex::Regex;
use std::sync::LazyLock;

static RE_INBOX_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-\s*(?:~~)?(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2})\s*\|\s*(.+?)(?:~~\s*->\s*(.+))?$")
        .unwrap()
});

#[derive(Debug, Clone, PartialEq)]
pub struct InboxItem {
    pub timestamp: String,
    pub text: String,
    pub refiled_to: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Inbox {
    pub items: Vec<InboxItem>,
}

impl Inbox {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn to_markdown(&self) -> String {
        let mut lines = vec!["# Inbox".to_string(), String::new()];
        for item in &self.items {
            if let Some(ref slug) = item.refiled_to {
                lines.push(format!(
                    "- ~~{} | {}~~ -> {}",
                    item.timestamp, item.text, slug
                ));
            } else {
                lines.push(format!("- {} | {}", item.timestamp, item.text));
            }
        }
        lines.join("\n")
    }

    pub fn from_markdown(text: &str) -> Self {
        let mut items = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(caps) = RE_INBOX_LINE.captures(line) {
                let timestamp = caps[1].to_string();
                let text = caps[2].trim().to_string();
                let refiled_to = caps.get(3).map(|m| m.as_str().trim().to_string());
                items.push(InboxItem {
                    timestamp,
                    text,
                    refiled_to,
                });
            }
        }
        Inbox { items }
    }

    /// Create a new inbox item with current timestamp.
    pub fn capture(text: &str) -> InboxItem {
        let ts = Local::now().format("%Y-%m-%d %H:%M").to_string();
        InboxItem {
            timestamp: ts,
            text: text.to_string(),
            refiled_to: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let inbox = Inbox {
            items: vec![
                InboxItem {
                    timestamp: "2026-03-17 09:15".to_string(),
                    text: "Check deployment logs".to_string(),
                    refiled_to: None,
                },
                InboxItem {
                    timestamp: "2026-03-17 10:00".to_string(),
                    text: "Review PR #42".to_string(),
                    refiled_to: Some("my-project".to_string()),
                },
            ],
        };

        let md = inbox.to_markdown();
        let restored = Inbox::from_markdown(&md);
        assert_eq!(restored.items.len(), 2);
        assert_eq!(restored.items[0].text, "Check deployment logs");
        assert!(restored.items[0].refiled_to.is_none());
        assert_eq!(restored.items[1].text, "Review PR #42");
        assert_eq!(
            restored.items[1].refiled_to,
            Some("my-project".to_string())
        );
    }

    #[test]
    fn test_empty_inbox() {
        let inbox = Inbox::new();
        let md = inbox.to_markdown();
        let restored = Inbox::from_markdown(&md);
        assert!(restored.items.is_empty());
    }
}
