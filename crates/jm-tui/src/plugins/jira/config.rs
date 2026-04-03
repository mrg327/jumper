//! JIRA plugin configuration — deserialized from `~/.jm/config.yaml` under `plugins.jira`.

use serde::{Deserialize, Serialize};

/// Configuration for the JIRA Cloud integration plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JiraConfig {
    /// JIRA Cloud instance URL (e.g., "https://myorg.atlassian.net").
    pub url: String,
    /// Target user email — whose issues to fetch and display.
    pub email: String,
    /// Optional: service/technical account email used for API authentication.
    /// When set, this email + `JIRA_API_TOKEN` are used for Basic auth,
    /// while `email` identifies the target user whose issues are fetched.
    /// When omitted, `email` is used for both auth and queries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_email: Option<String>,
    /// Auto-refresh interval in seconds (default: 60).
    #[serde(default = "default_refresh")]
    pub refresh_interval_secs: u64,
    /// Custom field ID for story points (e.g., "customfield_10016").
    /// If not set, auto-discovered via GET /rest/api/3/field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_points_field: Option<String>,
    /// Custom field ID for sprint (e.g., "customfield_10020").
    /// If not set, auto-discovered via GET /rest/api/3/field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_field: Option<String>,
}

fn default_refresh() -> u64 {
    60
}

impl JiraConfig {
    /// Extract a `JiraConfig` from the plugin config's extra map.
    ///
    /// The extra map captures arbitrary YAML keys under `plugins:` via `serde(flatten)`.
    /// This method looks for the `"jira"` key and attempts to deserialize it.
    /// We convert through JSON to avoid requiring `serde_yml` as a direct dependency
    /// of `jm-tui` — the `extra` values implement `Serialize`, so this round-trip is safe.
    pub fn from_plugin_config(config: &jm_core::config::Config) -> Option<JiraConfig> {
        let raw = config.plugins.extra.get("jira")?;
        // Convert serde_yml::Value -> serde_json::Value -> JiraConfig
        let json_val = serde_json::to_value(raw).ok()?;
        serde_json::from_value(json_val).ok()
    }

    /// Returns the email to use for API authentication (Basic auth header).
    /// If `auth_email` is set, returns that; otherwise returns `email`.
    pub fn auth_email(&self) -> &str {
        self.auth_email.as_deref().unwrap_or(&self.email)
    }

    /// Returns true if a separate service account is configured for auth.
    pub fn uses_service_account(&self) -> bool {
        self.auth_email.is_some()
    }

    /// Validate that the configuration has all required values.
    ///
    /// Returns `Ok(())` if valid, or `Err(message)` describing what is missing.
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("JIRA url is empty in config (plugins.jira.url)".to_string());
        }
        if self.email.trim().is_empty() {
            return Err("JIRA email is empty in config (plugins.jira.email)".to_string());
        }
        if let Some(ref auth_email) = self.auth_email {
            if auth_email.trim().is_empty() {
                return Err(
                    "JIRA auth_email is set but empty in config (plugins.jira.auth_email)"
                        .to_string(),
                );
            }
        }
        if std::env::var("JIRA_API_TOKEN").unwrap_or_default().is_empty() {
            return Err(
                "JIRA_API_TOKEN environment variable is not set. \
                 Generate one at https://id.atlassian.com/manage-profile/security/api-tokens"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_from_json() {
        let json = serde_json::json!({
            "url": "https://myorg.atlassian.net",
            "email": "matt@company.com",
            "refresh_interval_secs": 120,
            "story_points_field": "customfield_10016"
        });
        let config: JiraConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.url, "https://myorg.atlassian.net");
        assert_eq!(config.email, "matt@company.com");
        assert_eq!(config.auth_email, None);
        assert_eq!(config.refresh_interval_secs, 120);
        assert_eq!(
            config.story_points_field,
            Some("customfield_10016".to_string())
        );
        assert_eq!(config.sprint_field, None);
    }

    #[test]
    fn deserialize_with_auth_email() {
        let json = serde_json::json!({
            "url": "https://myorg.atlassian.net",
            "email": "matt@company.com",
            "auth_email": "jira-bot@company.com"
        });
        let config: JiraConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.email, "matt@company.com");
        assert_eq!(config.auth_email, Some("jira-bot@company.com".to_string()));
    }

    #[test]
    fn defaults_applied() {
        let json = serde_json::json!({
            "url": "https://test.atlassian.net",
            "email": "test@test.com"
        });
        let config: JiraConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.refresh_interval_secs, 60);
        assert_eq!(config.auth_email, None);
        assert_eq!(config.story_points_field, None);
        assert_eq!(config.sprint_field, None);
    }

    #[test]
    fn from_plugin_config_none() {
        // Default config has no jira key in extra.
        let config = jm_core::config::Config::default();
        assert!(JiraConfig::from_plugin_config(&config).is_none());
    }

    #[test]
    fn validate_empty_url() {
        let config = JiraConfig {
            url: "".to_string(),
            email: "test@test.com".to_string(),
            auth_email: None,
            refresh_interval_secs: 60,
            story_points_field: None,
            sprint_field: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_empty_email() {
        let config = JiraConfig {
            url: "https://test.atlassian.net".to_string(),
            email: "".to_string(),
            auth_email: None,
            refresh_interval_secs: 60,
            story_points_field: None,
            sprint_field: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_empty_auth_email() {
        let config = JiraConfig {
            url: "https://test.atlassian.net".to_string(),
            email: "test@test.com".to_string(),
            auth_email: Some("".to_string()),
            refresh_interval_secs: 60,
            story_points_field: None,
            sprint_field: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn auth_email_helper_returns_auth_when_set() {
        let config = JiraConfig {
            url: "https://test.atlassian.net".to_string(),
            email: "matt@company.com".to_string(),
            auth_email: Some("bot@company.com".to_string()),
            refresh_interval_secs: 60,
            story_points_field: None,
            sprint_field: None,
        };
        assert_eq!(config.auth_email(), "bot@company.com");
        assert!(config.uses_service_account());
    }

    #[test]
    fn auth_email_helper_returns_email_when_not_set() {
        let config = JiraConfig {
            url: "https://test.atlassian.net".to_string(),
            email: "matt@company.com".to_string(),
            auth_email: None,
            refresh_interval_secs: 60,
            story_points_field: None,
            sprint_field: None,
        };
        assert_eq!(config.auth_email(), "matt@company.com");
        assert!(!config.uses_service_account());
    }
}
