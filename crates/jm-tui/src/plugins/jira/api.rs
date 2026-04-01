//! JIRA Cloud REST API v3 client + background thread.
//!
//! All HTTP calls run in a dedicated background thread. The TUI thread
//! communicates via `mpsc` channels — sending `JiraCommand` and receiving
//! `JiraResult`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use super::adf::adf_to_text;
use super::models::*;

// ── Command enum — sent from TUI thread to API thread ────────────────────────

#[derive(Debug)]
pub(crate) enum JiraCommand {
    /// Fetch all issues assigned to the user.
    /// Includes a generation counter to prevent stale overwrites.
    FetchMyIssues { generation: u64 },
    /// Fetch available transitions for an issue.
    FetchTransitions { issue_key: String },
    /// Transition an issue to a new status, with optional required fields.
    TransitionIssue {
        issue_key: String,
        transition_id: String,
        fields: Option<Value>,
    },
    /// Update a field on an issue.
    UpdateField {
        issue_key: String,
        field_id: String,
        value: Value,
    },
    /// Add a comment to an issue (body is ADF JSON).
    AddComment { issue_key: String, body: Value },
    /// Fetch comments for an issue.
    FetchComments { issue_key: String },
    /// Fetch createmeta (required fields) for a project + issue type.
    FetchCreateMeta {
        project_key: String,
        issue_type_id: String,
    },
    /// Create a new issue.
    CreateIssue {
        project_key: String,
        fields: Value,
    },
    /// Fetch editable fields metadata for an issue.
    FetchEditMeta { issue_key: String },
    /// Fetch custom field definitions for discovery.
    FetchFields,
    /// Fetch issue types for a project (createmeta).
    FetchIssueTypes { project_key: String },
    /// Cooperative shutdown signal.
    Shutdown,
}

// ── Result enum — sent from API thread to TUI thread ─────────────────────────

#[derive(Debug)]
pub(crate) enum JiraResult {
    /// Full issue list with generation counter.
    Issues {
        generation: u64,
        issues: Vec<JiraIssue>,
    },
    /// Available transitions for an issue.
    Transitions(String, Vec<JiraTransition>),
    /// Transition completed successfully.
    TransitionComplete(String),
    /// Transition failed (for optimistic UI revert).
    TransitionFailed(String, JiraError),
    /// Field updated successfully.
    FieldUpdated(String, String),
    /// Comment added successfully.
    CommentAdded(String),
    /// Comments for an issue.
    Comments(String, Vec<JiraComment>),
    /// Create meta response (fields for creation form).
    CreateMeta(CreateMetaResponse),
    /// Issue created successfully — contains the new issue key.
    IssueCreated(String),
    /// Editable field metadata for an issue.
    EditMeta(String, Vec<EditableField>),
    /// Custom field definitions from /rest/api/3/field.
    Fields(Vec<JiraFieldDef>),
    /// Issue types for a project.
    IssueTypes(String, Vec<JiraIssueType>),
    /// Generic error with context.
    Error { context: String, error: JiraError },
}

// ── Rate limit / retry constants ─────────────────────────────────────────────

const MAX_READ_RETRIES: u32 = 3;
const MAX_WRITE_RETRIES: u32 = 1;
const PAGE_SIZE: u64 = 100;

// ── Validate credentials (synchronous, called from on_enter) ─────────────────

/// Validate JIRA credentials by calling GET /rest/api/3/myself.
///
/// This is called synchronously from `on_enter()`, NOT through the background
/// thread. Returns the user's account ID and display name on success.
pub(crate) fn validate_credentials(
    base_url: &str,
    email: &str,
    api_token: &str,
) -> Result<MyselfResponse, JiraError> {
    let agent = ureq::Agent::new_with_defaults();
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", email, api_token));
    let url = format!("{}/rest/api/3/myself", base_url.trim_end_matches('/'));

    let response = agent
        .get(&url)
        .header("Authorization", &format!("Basic {}", auth))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .call()
        .map_err(|e| JiraError {
            status_code: 0,
            error_messages: vec![format!("Network error: {}", e)],
            field_errors: HashMap::new(),
        })?;

    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        let myself: MyselfResponse =
            response.into_body().read_json().map_err(|e| JiraError {
                status_code: 0,
                error_messages: vec![format!("Failed to parse /myself response: {}", e)],
                field_errors: HashMap::new(),
            })?;
        Ok(myself)
    } else {
        let err_body: JiraErrorResponse = response
            .into_body()
            .read_json()
            .unwrap_or_default();
        Err(JiraError {
            status_code: status,
            error_messages: err_body.error_messages,
            field_errors: err_body.errors,
        })
    }
}

// ── Background thread main loop ──────────────────────────────────────────────

/// Main loop for the API background thread.
///
/// Receives commands from the TUI thread, executes HTTP requests, and sends
/// results back. Uses a single `ureq::Agent` for connection pooling.
pub(crate) fn api_thread_loop(
    commands: mpsc::Receiver<JiraCommand>,
    results: mpsc::Sender<JiraResult>,
    base_url: String,
    email: String,
    api_token: String,
    account_id: String,
    story_points_field: Option<String>,
    sprint_field: Option<String>,
    shutdown: Arc<AtomicBool>,
) {
    let agent = ureq::Agent::new_with_defaults();
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", email, api_token));
    let base_url = base_url.trim_end_matches('/').to_string();

    let ctx = ApiContext {
        agent,
        auth,
        base_url,
        account_id,
        story_points_field,
        sprint_field,
    };

    while !shutdown.load(Ordering::Relaxed) {
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(JiraCommand::Shutdown) => break,
            Ok(cmd) => {
                let result = dispatch_command(&ctx, cmd);
                if results.send(result).is_err() {
                    // TUI side dropped the receiver — exit
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Shared context for API calls within the background thread.
struct ApiContext {
    agent: ureq::Agent,
    auth: String,
    base_url: String,
    account_id: String,
    story_points_field: Option<String>,
    sprint_field: Option<String>,
}

/// Dispatch a command to the appropriate handler.
fn dispatch_command(ctx: &ApiContext, cmd: JiraCommand) -> JiraResult {
    match cmd {
        JiraCommand::FetchMyIssues { generation } => fetch_my_issues(ctx, generation),
        JiraCommand::FetchTransitions { issue_key } => fetch_transitions(ctx, &issue_key),
        JiraCommand::TransitionIssue {
            issue_key,
            transition_id,
            fields,
        } => transition_issue(ctx, &issue_key, &transition_id, fields.as_ref()),
        JiraCommand::UpdateField {
            issue_key,
            field_id,
            value,
        } => update_field(ctx, &issue_key, &field_id, &value),
        JiraCommand::AddComment { issue_key, body } => add_comment(ctx, &issue_key, &body),
        JiraCommand::FetchComments { issue_key } => fetch_comments(ctx, &issue_key),
        JiraCommand::FetchCreateMeta {
            project_key,
            issue_type_id,
        } => fetch_create_meta(ctx, &project_key, &issue_type_id),
        JiraCommand::CreateIssue {
            project_key,
            fields,
        } => create_issue(ctx, &project_key, &fields),
        JiraCommand::FetchEditMeta { issue_key } => fetch_edit_meta(ctx, &issue_key),
        JiraCommand::FetchFields => fetch_fields(ctx),
        JiraCommand::FetchIssueTypes { project_key } => fetch_issue_types(ctx, &project_key),
        JiraCommand::Shutdown => unreachable!("handled in main loop"),
    }
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

/// Make a GET request with retry on 429 (rate limit).
fn get_with_retry(
    ctx: &ApiContext,
    url: &str,
    max_retries: u32,
) -> Result<Value, JiraError> {
    let mut retries = 0;
    loop {
        let response = ctx
            .agent
            .get(url)
            .header("Authorization", &format!("Basic {}", ctx.auth))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .call()
            .map_err(|e| JiraError {
                status_code: 0,
                error_messages: vec![format!("Network error: {}", e)],
                field_errors: HashMap::new(),
            })?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            let body: Value = response.into_body().read_json().map_err(|e| JiraError {
                status_code: 0,
                error_messages: vec![format!("JSON parse error: {}", e)],
                field_errors: HashMap::new(),
            })?;
            return Ok(body);
        } else if status == 429 {
            if retries >= max_retries {
                return Err(JiraError {
                    status_code: 429,
                    error_messages: vec!["Rate limited — max retries exceeded".to_string()],
                    field_errors: HashMap::new(),
                });
            }
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            thread::sleep(Duration::from_secs(retry_after.min(120)));
            retries += 1;
        } else {
            let err_body: JiraErrorResponse = response
                .into_body()
                .read_json()
                .unwrap_or_default();
            return Err(JiraError {
                status_code: status,
                error_messages: err_body.error_messages,
                field_errors: err_body.errors,
            });
        }
    }
}

/// Make a POST request with retry on 429.
fn post_with_retry(
    ctx: &ApiContext,
    url: &str,
    body: &Value,
    max_retries: u32,
) -> Result<Option<Value>, JiraError> {
    let mut retries = 0;
    loop {
        let body_str = serde_json::to_string(body).unwrap_or_default();
        let response = ctx
            .agent
            .post(url)
            .header("Authorization", &format!("Basic {}", ctx.auth))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(body_str.as_bytes())
            .map_err(|e| JiraError {
                status_code: 0,
                error_messages: vec![format!("Network error: {}", e)],
                field_errors: HashMap::new(),
            })?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            // Some endpoints return 204 No Content
            let body: Option<Value> = response.into_body().read_json().ok();
            return Ok(body);
        } else if status == 429 {
            if retries >= max_retries {
                return Err(JiraError {
                    status_code: 429,
                    error_messages: vec!["Rate limited — max retries exceeded".to_string()],
                    field_errors: HashMap::new(),
                });
            }
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            thread::sleep(Duration::from_secs(retry_after.min(120)));
            retries += 1;
        } else {
            let err_body: JiraErrorResponse = response
                .into_body()
                .read_json()
                .unwrap_or_default();
            return Err(JiraError {
                status_code: status,
                error_messages: err_body.error_messages,
                field_errors: err_body.errors,
            });
        }
    }
}

/// Make a PUT request with retry on 429.
fn put_with_retry(
    ctx: &ApiContext,
    url: &str,
    body: &Value,
    max_retries: u32,
) -> Result<(), JiraError> {
    let mut retries = 0;
    loop {
        let body_str = serde_json::to_string(body).unwrap_or_default();
        let response = ctx
            .agent
            .put(url)
            .header("Authorization", &format!("Basic {}", ctx.auth))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(body_str.as_bytes())
            .map_err(|e| JiraError {
                status_code: 0,
                error_messages: vec![format!("Network error: {}", e)],
                field_errors: HashMap::new(),
            })?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        } else if status == 429 {
            if retries >= max_retries {
                return Err(JiraError {
                    status_code: 429,
                    error_messages: vec!["Rate limited — max retries exceeded".to_string()],
                    field_errors: HashMap::new(),
                });
            }
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            thread::sleep(Duration::from_secs(retry_after.min(120)));
            retries += 1;
        } else {
            let err_body: JiraErrorResponse = response
                .into_body()
                .read_json()
                .unwrap_or_default();
            return Err(JiraError {
                status_code: status,
                error_messages: err_body.error_messages,
                field_errors: err_body.errors,
            });
        }
    }
}

// ── Command handlers ─────────────────────────────────────────────────────────

/// Fetch all issues assigned to the user, paginating through all results.
fn fetch_my_issues(ctx: &ApiContext, generation: u64) -> JiraResult {
    let mut all_issues = Vec::new();
    let mut start_at: u64 = 0;

    // Build field list including dynamic custom fields
    let mut fields =
        "summary,status,priority,issuetype,assignee,reporter,created,updated,description,labels,components,project,parent"
            .to_string();
    if let Some(ref sp) = ctx.story_points_field {
        fields.push_str(&format!(",{}", sp));
    }
    if let Some(ref sf) = ctx.sprint_field {
        fields.push_str(&format!(",{}", sf));
    }

    loop {
        let url = format!(
            "{}/rest/api/3/search?jql=assignee%3D'{}'%20ORDER%20BY%20updated%20DESC&startAt={}&maxResults={}&fields={}",
            ctx.base_url, ctx.account_id, start_at, PAGE_SIZE, fields
        );

        match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
            Ok(body) => {
                let search: SearchResponse = match serde_json::from_value(body) {
                    Ok(s) => s,
                    Err(e) => {
                        return JiraResult::Error {
                            context: "Parsing search response".to_string(),
                            error: JiraError {
                                status_code: 0,
                                error_messages: vec![format!("Parse error: {}", e)],
                                field_errors: HashMap::new(),
                            },
                        }
                    }
                };

                let page_len = search.issues.len() as u64;

                for issue_val in &search.issues {
                    all_issues.push(parse_issue(
                        issue_val,
                        ctx.story_points_field.as_deref(),
                        ctx.sprint_field.as_deref(),
                    ));
                }

                // Stop when page is empty or shorter than max_results
                if page_len == 0 || page_len < PAGE_SIZE {
                    break;
                }
                start_at += page_len;
            }
            Err(error) => {
                return JiraResult::Error {
                    context: "Fetching issues".to_string(),
                    error,
                }
            }
        }
    }

    JiraResult::Issues {
        generation,
        issues: all_issues,
    }
}

/// Parse a single issue from the search response JSON.
fn parse_issue(
    val: &Value,
    story_points_field: Option<&str>,
    sprint_field: Option<&str>,
) -> JiraIssue {
    let fields = &val["fields"];

    let status_name = fields["status"]["name"]
        .as_str()
        .unwrap_or("Unknown")
        .to_string();
    let status_category_key = fields["status"]["statusCategory"]["key"]
        .as_str()
        .unwrap_or("new");
    let category = match status_category_key {
        "indeterminate" => StatusCategory::InProgress,
        "done" => StatusCategory::Done,
        _ => StatusCategory::ToDo,
    };

    let priority = fields["priority"]["name"]
        .as_str()
        .map(|s| s.to_string());

    let assignee = fields["assignee"]["displayName"]
        .as_str()
        .map(|s| s.to_string());

    let reporter = fields["reporter"]["displayName"]
        .as_str()
        .map(|s| s.to_string());

    let description = if fields["description"].is_null() {
        None
    } else {
        let text = adf_to_text(&fields["description"]);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    };

    let labels: Vec<String> = fields["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let components: Vec<String> = fields["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let story_points = story_points_field
        .and_then(|f| fields.get(f))
        .and_then(|v| v.as_f64());

    let sprint = sprint_field
        .and_then(|f| fields.get(f))
        .and_then(|v| extract_sprint_name(v));

    // Epic: check parent field
    let epic = if let Some(parent) = fields.get("parent") {
        let is_epic = parent["fields"]["issuetype"]["hierarchyLevel"]
            .as_i64()
            .map(|l| l == 1)
            .unwrap_or(false)
            || parent["fields"]["issuetype"]["name"]
                .as_str()
                .map(|n| n == "Epic")
                .unwrap_or(false);
        if is_epic {
            Some(EpicInfo {
                key: parent["key"].as_str().unwrap_or("").to_string(),
                name: parent["fields"]["summary"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            })
        } else {
            None
        }
    } else {
        None
    };

    JiraIssue {
        key: val["key"].as_str().unwrap_or("").to_string(),
        summary: fields["summary"].as_str().unwrap_or("").to_string(),
        status: JiraStatus {
            name: status_name,
            category,
        },
        priority,
        issue_type: fields["issuetype"]["name"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string(),
        assignee,
        reporter,
        created: fields["created"].as_str().unwrap_or("").to_string(),
        updated: fields["updated"].as_str().unwrap_or("").to_string(),
        description,
        sprint,
        epic,
        story_points,
        labels,
        components,
        project_key: fields["project"]["key"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        project_name: fields["project"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    }
}

/// Extract sprint name from the sprint custom field value.
/// Handles both array (classic projects) and single object (team-managed projects).
fn extract_sprint_name(value: &Value) -> Option<String> {
    // Array case (classic projects)
    if let Some(arr) = value.as_array() {
        arr.iter()
            .find(|s| s.get("state").and_then(|s| s.as_str()) == Some("active"))
            .or_else(|| {
                arr.iter()
                    .find(|s| s.get("state").and_then(|s| s.as_str()) == Some("future"))
            })
            .or_else(|| arr.last())
            .and_then(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
    }
    // Single object case (team-managed projects)
    else if value.is_object() {
        value
            .get("name")
            .and_then(|n| n.as_str())
            .map(String::from)
    } else {
        None
    }
}

/// Fetch available transitions for an issue.
fn fetch_transitions(ctx: &ApiContext, issue_key: &str) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions?expand=transitions.fields",
        ctx.base_url, issue_key
    );

    match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
        Ok(body) => {
            let transitions = parse_transitions(&body);
            JiraResult::Transitions(issue_key.to_string(), transitions)
        }
        Err(error) => JiraResult::Error {
            context: format!("Fetching transitions for {}", issue_key),
            error,
        },
    }
}

/// Parse transitions from the API response.
fn parse_transitions(body: &Value) -> Vec<JiraTransition> {
    let transitions = body["transitions"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    transitions
        .iter()
        .map(|t| {
            let to = &t["to"];
            let to_status = JiraStatus {
                name: to["name"].as_str().unwrap_or("").to_string(),
                category: match to["statusCategory"]["key"].as_str().unwrap_or("new") {
                    "indeterminate" => StatusCategory::InProgress,
                    "done" => StatusCategory::Done,
                    _ => StatusCategory::ToDo,
                },
            };

            // Parse required fields from the fields map
            let mut required_fields = Vec::new();
            if let Some(fields_map) = t.get("fields").and_then(|f| f.as_object()) {
                for (field_id, field_val) in fields_map {
                    let required = field_val["required"].as_bool().unwrap_or(false);
                    if required {
                        let allowed_values: Vec<AllowedValue> = field_val
                            .get("allowedValues")
                            .and_then(|av| av.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| {
                                        Some(AllowedValue {
                                            id: v["id"].as_str()?.to_string(),
                                            name: v["name"].as_str()?.to_string(),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        let field_type = classify_field_type(field_val, &allowed_values);

                        required_fields.push(TransitionField {
                            field_id: field_id.clone(),
                            name: field_val["name"]
                                .as_str()
                                .unwrap_or(field_id)
                                .to_string(),
                            field_type,
                            allowed_values,
                            is_comment: field_id == "comment",
                        });
                    }
                }
            }

            JiraTransition {
                id: t["id"].as_str().unwrap_or("").to_string(),
                name: t["name"].as_str().unwrap_or("").to_string(),
                to_status,
                required_fields,
            }
        })
        .collect()
}

/// Classify a field's type based on its schema.
fn classify_field_type(field_val: &Value, allowed_values: &[AllowedValue]) -> FieldType {
    let schema_type = field_val
        .get("schema")
        .and_then(|s| s.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let schema_items = field_val
        .get("schema")
        .and_then(|s| s.get("items"))
        .and_then(|i| i.as_str())
        .unwrap_or("");

    let schema_system = field_val
        .get("schema")
        .and_then(|s| s.get("system"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    match schema_type {
        "string" => {
            if schema_system == "description" {
                FieldType::TextArea
            } else {
                FieldType::Text
            }
        }
        "number" => FieldType::Number,
        "date" | "datetime" => FieldType::Date,
        "priority" | "resolution" | "option" => {
            if allowed_values.is_empty() {
                FieldType::Text
            } else {
                FieldType::Select
            }
        }
        "array" => {
            if schema_items == "string" {
                // labels-style array of strings — treat as text (comma-separated)
                FieldType::Text
            } else if !allowed_values.is_empty() {
                FieldType::MultiSelect
            } else {
                FieldType::Unsupported
            }
        }
        "any" => {
            if schema_system == "description" {
                FieldType::TextArea
            } else {
                FieldType::Unsupported
            }
        }
        "user" | "version" | "issuetype" | "project" => FieldType::Unsupported,
        _ => {
            if !allowed_values.is_empty() {
                FieldType::Select
            } else {
                FieldType::Unsupported
            }
        }
    }
}

/// Transition an issue to a new status.
fn transition_issue(
    ctx: &ApiContext,
    issue_key: &str,
    transition_id: &str,
    fields: Option<&Value>,
) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions",
        ctx.base_url, issue_key
    );

    let mut body = json!({
        "transition": { "id": transition_id }
    });

    if let Some(fields_val) = fields {
        // Check if there's an "update" key (for comments in transitions)
        if let Some(update) = fields_val.get("update") {
            body["update"] = update.clone();
        }
        // Regular fields
        if let Some(f) = fields_val.get("fields") {
            body["fields"] = f.clone();
        } else if fields_val.get("update").is_none() {
            // If no "fields" or "update" key, treat the whole value as fields
            body["fields"] = fields_val.clone();
        }
    }

    match post_with_retry(ctx, &url, &body, MAX_WRITE_RETRIES) {
        Ok(_) => JiraResult::TransitionComplete(issue_key.to_string()),
        Err(error) => JiraResult::TransitionFailed(issue_key.to_string(), error),
    }
}

/// Update a field on an issue.
fn update_field(ctx: &ApiContext, issue_key: &str, field_id: &str, value: &Value) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}?notifyUsers=false",
        ctx.base_url, issue_key
    );

    let body = json!({
        "fields": {
            field_id: value
        }
    });

    match put_with_retry(ctx, &url, &body, MAX_WRITE_RETRIES) {
        Ok(()) => JiraResult::FieldUpdated(issue_key.to_string(), field_id.to_string()),
        Err(error) => JiraResult::Error {
            context: format!("Updating {} on {}", field_id, issue_key),
            error,
        },
    }
}

/// Add a comment to an issue.
fn add_comment(ctx: &ApiContext, issue_key: &str, body_adf: &Value) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}/comment",
        ctx.base_url, issue_key
    );

    let body = json!({ "body": body_adf });

    match post_with_retry(ctx, &url, &body, MAX_WRITE_RETRIES) {
        Ok(_) => JiraResult::CommentAdded(issue_key.to_string()),
        Err(error) => JiraResult::Error {
            context: format!("Adding comment to {}", issue_key),
            error,
        },
    }
}

/// Fetch comments for an issue.
fn fetch_comments(ctx: &ApiContext, issue_key: &str) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}/comment?startAt=0&maxResults=50&orderBy=-created",
        ctx.base_url, issue_key
    );

    match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
        Ok(body) => {
            let comments = parse_comments(&body);
            JiraResult::Comments(issue_key.to_string(), comments)
        }
        Err(error) => JiraResult::Error {
            context: format!("Fetching comments for {}", issue_key),
            error,
        },
    }
}

/// Parse comments from the API response.
fn parse_comments(body: &Value) -> Vec<JiraComment> {
    body["comments"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|c| JiraComment {
                    id: c["id"].as_str().unwrap_or("").to_string(),
                    author: c["author"]["displayName"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string(),
                    created: c["created"].as_str().unwrap_or("").to_string(),
                    body: if c["body"].is_null() {
                        String::new()
                    } else {
                        adf_to_text(&c["body"])
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fetch createmeta for a project + issue type.
fn fetch_create_meta(ctx: &ApiContext, project_key: &str, issue_type_id: &str) -> JiraResult {
    let mut all_fields = Vec::new();
    let mut start_at: u64 = 0;

    loop {
        let url = format!(
            "{}/rest/api/3/issue/createmeta/{}/issuetypes/{}?startAt={}&maxResults=50",
            ctx.base_url, project_key, issue_type_id, start_at
        );

        match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
            Ok(body) => {
                let response: CreateMetaFieldsResponse = match serde_json::from_value(body) {
                    Ok(r) => r,
                    Err(e) => {
                        return JiraResult::Error {
                            context: "Parsing createmeta response".to_string(),
                            error: JiraError {
                                status_code: 0,
                                error_messages: vec![format!("Parse error: {}", e)],
                                field_errors: HashMap::new(),
                            },
                        }
                    }
                };

                let page_len = response.values.len() as u64;

                for field_val in &response.values {
                    let field_id = field_val["fieldId"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();

                    // Filter out auto-set fields
                    if matches!(field_id.as_str(), "project" | "issuetype" | "reporter") {
                        continue;
                    }

                    let allowed_values: Vec<AllowedValue> = field_val
                        .get("allowedValues")
                        .and_then(|av| av.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| {
                                    Some(AllowedValue {
                                        id: v["id"].as_str()?.to_string(),
                                        name: v["name"].as_str()?.to_string(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    let field_type = classify_field_type(field_val, &allowed_values);

                    all_fields.push(EditableField {
                        field_id,
                        name: field_val["name"].as_str().unwrap_or("").to_string(),
                        field_type,
                        required: field_val["required"].as_bool().unwrap_or(false),
                        allowed_values: if allowed_values.is_empty() {
                            None
                        } else {
                            Some(allowed_values)
                        },
                    });
                }

                if page_len == 0 || page_len < 50 {
                    break;
                }
                start_at += page_len;
            }
            Err(error) => {
                return JiraResult::Error {
                    context: format!(
                        "Fetching createmeta for {}/{}",
                        project_key, issue_type_id
                    ),
                    error,
                }
            }
        }
    }

    JiraResult::CreateMeta(CreateMetaResponse {
        fields: all_fields,
    })
}

/// Create a new issue.
fn create_issue(ctx: &ApiContext, project_key: &str, fields: &Value) -> JiraResult {
    let url = format!("{}/rest/api/3/issue", ctx.base_url);

    // Build the create body — inject project, assignee, and reporter
    let mut body_fields = fields.clone();
    if let Some(obj) = body_fields.as_object_mut() {
        obj.insert(
            "project".to_string(),
            json!({ "key": project_key }),
        );
        obj.insert(
            "assignee".to_string(),
            json!({ "accountId": ctx.account_id }),
        );
        obj.insert(
            "reporter".to_string(),
            json!({ "accountId": ctx.account_id }),
        );
    }

    let body = json!({ "fields": body_fields });

    match post_with_retry(ctx, &url, &body, MAX_WRITE_RETRIES) {
        Ok(Some(resp)) => {
            let key = resp["key"].as_str().unwrap_or("???").to_string();
            JiraResult::IssueCreated(key)
        }
        Ok(None) => JiraResult::IssueCreated("???".to_string()),
        Err(error) => JiraResult::Error {
            context: format!("Creating issue in {}", project_key),
            error,
        },
    }
}

/// Fetch editable fields metadata for an issue.
fn fetch_edit_meta(ctx: &ApiContext, issue_key: &str) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/{}/editmeta",
        ctx.base_url, issue_key
    );

    match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
        Ok(body) => {
            let editable_fields = parse_edit_meta(&body);
            JiraResult::EditMeta(issue_key.to_string(), editable_fields)
        }
        Err(error) => JiraResult::Error {
            context: format!("Fetching editmeta for {}", issue_key),
            error,
        },
    }
}

/// Parse editmeta response into a list of editable fields.
fn parse_edit_meta(body: &Value) -> Vec<EditableField> {
    let mut fields = Vec::new();

    if let Some(fields_map) = body.get("fields").and_then(|f| f.as_object()) {
        for (field_id, field_val) in fields_map {
            // Exclude description (read-only in our plugin — ADF lossy round-trip)
            if field_id == "description" {
                continue;
            }

            // Only include fields with "set" operation
            let has_set = field_val
                .get("operations")
                .and_then(|ops| ops.as_array())
                .map(|arr| arr.iter().any(|op| op.as_str() == Some("set")))
                .unwrap_or(false);

            if !has_set {
                continue;
            }

            let allowed_values: Vec<AllowedValue> = field_val
                .get("allowedValues")
                .and_then(|av| av.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            Some(AllowedValue {
                                id: v["id"].as_str()?.to_string(),
                                name: v["name"].as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let field_type = classify_field_type(field_val, &allowed_values);

            fields.push(EditableField {
                field_id: field_id.clone(),
                name: field_val["name"].as_str().unwrap_or("").to_string(),
                field_type,
                required: field_val["required"].as_bool().unwrap_or(false),
                allowed_values: if allowed_values.is_empty() {
                    None
                } else {
                    Some(allowed_values)
                },
            });
        }
    }

    fields
}

/// Fetch custom field definitions for discovery.
fn fetch_fields(ctx: &ApiContext) -> JiraResult {
    let url = format!("{}/rest/api/3/field", ctx.base_url);

    match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
        Ok(body) => {
            let defs: Vec<JiraFieldDef> = serde_json::from_value(body).unwrap_or_default();
            JiraResult::Fields(defs)
        }
        Err(error) => JiraResult::Error {
            context: "Fetching field definitions".to_string(),
            error,
        },
    }
}

/// Fetch issue types for a project.
fn fetch_issue_types(ctx: &ApiContext, project_key: &str) -> JiraResult {
    let url = format!(
        "{}/rest/api/3/issue/createmeta/{}/issuetypes",
        ctx.base_url, project_key
    );

    match get_with_retry(ctx, &url, MAX_READ_RETRIES) {
        Ok(body) => {
            let response: IssueTypesResponse =
                serde_json::from_value(body).unwrap_or(IssueTypesResponse {
                    values: Vec::new(),
                });
            JiraResult::IssueTypes(project_key.to_string(), response.values)
        }
        Err(error) => JiraResult::Error {
            context: format!("Fetching issue types for {}", project_key),
            error,
        },
    }
}
