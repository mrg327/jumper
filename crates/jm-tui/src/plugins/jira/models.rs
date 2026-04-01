//! JIRA data types — all structs and enums for the JIRA Cloud REST API v3 integration.

use std::collections::HashMap;
use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

// ── Core issue type ──────────────────────────────────────────────────────────

/// A JIRA issue as displayed in the TUI.
#[derive(Debug, Clone)]
pub(crate) struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub status: JiraStatus,
    pub priority: Option<String>,
    pub issue_type: String,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub created: String,
    pub updated: String,
    pub description: Option<String>,
    pub sprint: Option<String>,
    pub epic: Option<EpicInfo>,
    pub story_points: Option<f64>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub project_key: String,
    pub project_name: String,
}

// ── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct JiraStatus {
    pub name: String,
    pub category: StatusCategory,
}

/// JIRA has 4 status category keys: "new", "indeterminate", "done", "undefined".
/// We map these to 3 display categories. "undefined" and unknown values map to `ToDo`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StatusCategory {
    ToDo,
    InProgress,
    Done,
}

impl<'de> Deserialize<'de> for StatusCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StatusCategoryVisitor;

        impl<'de> Visitor<'de> for StatusCategoryVisitor {
            type Value = StatusCategory;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a status category key string")
            }

            fn visit_str<E>(self, value: &str) -> Result<StatusCategory, E>
            where
                E: de::Error,
            {
                match value {
                    "indeterminate" => Ok(StatusCategory::InProgress),
                    "done" => Ok(StatusCategory::Done),
                    // "new", "undefined", and anything else map to ToDo
                    _ => Ok(StatusCategory::ToDo),
                }
            }
        }

        deserializer.deserialize_str(StatusCategoryVisitor)
    }
}

// ── Epic info ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct EpicInfo {
    pub key: String,
    pub name: String,
}

// ── Allowed values (for select / multi-select fields) ────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AllowedValue {
    pub id: String,
    pub name: String,
}

// ── Transitions ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct JiraTransition {
    pub id: String,
    pub name: String,
    pub to_status: JiraStatus,
    pub required_fields: Vec<TransitionField>,
}

/// A required field attached to a transition (e.g., Resolution for "Done").
#[derive(Debug, Clone)]
pub(crate) struct TransitionField {
    pub field_id: String,
    pub name: String,
    pub field_type: FieldType,
    pub allowed_values: Vec<AllowedValue>,
    /// True when the transition requires a comment instead of (or in addition to) structured fields.
    pub is_comment: bool,
}

// ── Comments ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct JiraComment {
    pub id: String,
    pub author: String,
    pub created: String,
    pub body: String,
}

// ── Editable fields ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct EditableField {
    pub field_id: String,
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub allowed_values: Option<Vec<AllowedValue>>,
}

/// Field type classification for TUI rendering and editing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldType {
    Text,
    TextArea,
    Number,
    Select,
    MultiSelect,
    Date,
    /// For field types the TUI cannot render/edit. Show as read-only text.
    Unsupported,
}

// ── Field definitions (from /rest/api/3/field) ───────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JiraFieldDef {
    pub id: String,
    pub name: String,
    pub custom: bool,
    pub schema: Option<FieldSchema>,
}

/// Schema information for a JIRA field definition.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FieldSchema {
    #[serde(rename = "type")]
    pub field_type: String,
    pub custom: Option<String>,
}

// ── Issue types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JiraIssueType {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub subtask: bool,
}

// ── Error types ──────────────────────────────────────────────────────────────

/// Error returned from the JIRA REST API.
/// Covers both top-level error_messages and per-field errors.
#[derive(Debug, Clone)]
pub(crate) struct JiraError {
    pub status_code: u16,
    pub error_messages: Vec<String>,
    pub field_errors: HashMap<String, String>,
}

impl JiraError {
    pub fn display(&self) -> String {
        let mut parts = self.error_messages.clone();
        for (field, msg) in &self.field_errors {
            parts.push(format!("{}: {}", field, msg));
        }
        if parts.is_empty() {
            format!("HTTP {}", self.status_code)
        } else {
            parts.join("\n")
        }
    }
}

/// Raw error response from JIRA API (for deserialization).
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct JiraErrorResponse {
    #[serde(rename = "errorMessages", default)]
    pub error_messages: Vec<String>,
    #[serde(default)]
    pub errors: HashMap<String, String>,
}

// ── Create meta ──────────────────────────────────────────────────────────────

/// Parsed result of the createmeta endpoint.
/// `project`, `issuetype`, and `reporter` are already filtered out --
/// only the remaining required + optional fields are included.
#[derive(Debug, Clone)]
pub(crate) struct CreateMetaResponse {
    pub fields: Vec<EditableField>,
}

// ── API response helper types (for JSON deserialization) ─────────────────────

/// Response from GET /rest/api/3/myself
#[derive(Debug, Deserialize)]
pub(crate) struct MyselfResponse {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

/// Wrapper for paginated search response.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchResponse {
    #[serde(rename = "startAt", default)]
    pub start_at: u64,
    #[serde(rename = "maxResults", default)]
    pub max_results: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub issues: Vec<serde_json::Value>,
}

/// Wrapper for transitions response.
#[derive(Debug, Deserialize)]
pub(crate) struct TransitionsResponse {
    #[serde(default)]
    pub transitions: Vec<serde_json::Value>,
}

/// Wrapper for comments response.
#[derive(Debug, Deserialize)]
pub(crate) struct CommentsResponse {
    #[serde(rename = "startAt", default)]
    pub start_at: u64,
    #[serde(rename = "maxResults", default)]
    pub max_results: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub comments: Vec<serde_json::Value>,
}

/// Wrapper for issue types response from createmeta.
#[derive(Debug, Deserialize)]
pub(crate) struct IssueTypesResponse {
    #[serde(default)]
    pub values: Vec<JiraIssueType>,
}

/// Wrapper for createmeta field response (paginated).
#[derive(Debug, Deserialize)]
pub(crate) struct CreateMetaFieldsResponse {
    #[serde(rename = "startAt", default)]
    pub start_at: u64,
    #[serde(rename = "maxResults", default)]
    pub max_results: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub values: Vec<serde_json::Value>,
}

/// Wrapper for create issue response.
#[derive(Debug, Deserialize)]
pub(crate) struct CreateIssueResponse {
    pub key: String,
}
