//! ADF (Atlassian Document Format) conversion utilities.
//!
//! JIRA Cloud v3 uses ADF for rich text fields (descriptions, comments).
//! These functions convert between ADF JSON and plain text.

use serde_json::{json, Value};

/// Recursively convert an ADF JSON node to plain text.
///
/// Handles paragraphs, headings, bullet lists, ordered lists, code blocks,
/// links, mentions, emojis, and other common ADF node types. Unknown node
/// types are handled by recursing into their content if present.
pub(crate) fn adf_to_text(node: &Value) -> String {
    let node_type = match node.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return String::new(),
    };

    match node_type {
        "doc" => {
            let children = get_content(node);
            children
                .iter()
                .map(|c| adf_to_text(c))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        }

        "paragraph" => {
            let children = get_content(node);
            children.iter().map(|c| adf_to_text(c)).collect::<String>()
        }

        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let prefix = "#".repeat(level);
            let children = get_content(node);
            let text: String = children.iter().map(|c| adf_to_text(c)).collect();
            format!("{} {}", prefix, text)
        }

        "bulletList" => {
            let children = get_content(node);
            children
                .iter()
                .map(|item| format!("- {}", adf_to_text(item)))
                .collect::<Vec<_>>()
                .join("\n")
        }

        "orderedList" => {
            let children = get_content(node);
            children
                .iter()
                .enumerate()
                .map(|(i, item)| format!("{}. {}", i + 1, adf_to_text(item)))
                .collect::<Vec<_>>()
                .join("\n")
        }

        "listItem" => {
            let children = get_content(node);
            children.iter().map(|c| adf_to_text(c)).collect::<String>()
        }

        "codeBlock" => {
            let children = get_content(node);
            let text: String = children.iter().map(|c| adf_to_text(c)).collect();
            text.lines()
                .map(|line| format!("    {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        }

        "blockquote" => {
            let children = get_content(node);
            let text: String = children
                .iter()
                .map(|c| adf_to_text(c))
                .collect::<Vec<_>>()
                .join("\n");
            text.lines()
                .map(|line| format!("> {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        }

        "text" => {
            let text = node
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            // Check for link marks — append URL in parens
            if let Some(marks) = node.get("marks").and_then(|m| m.as_array()) {
                for mark in marks {
                    if mark.get("type").and_then(|t| t.as_str()) == Some("link") {
                        if let Some(href) =
                            mark.get("attrs").and_then(|a| a.get("href")).and_then(|h| h.as_str())
                        {
                            return format!("{} ({})", text, href);
                        }
                    }
                }
            }
            text
        }

        "hardBreak" => "\n".to_string(),

        "mention" => node
            .get("attrs")
            .and_then(|a| a.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("@unknown")
            .to_string(),

        "inlineCard" => node
            .get("attrs")
            .and_then(|a| a.get("url"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string(),

        "emoji" => node
            .get("attrs")
            .and_then(|a| a.get("shortName"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),

        "rule" => "---".to_string(),

        "table" => {
            // Flatten all table cells into space-separated text.
            let children = get_content(node);
            children
                .iter()
                .map(|row| {
                    let cells = get_content(row);
                    cells
                        .iter()
                        .map(|cell| adf_to_text(cell))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        // Unknown node types — recurse into content if present
        _ => {
            let children = get_content(node);
            if children.is_empty() {
                String::new()
            } else {
                children.iter().map(|c| adf_to_text(c)).collect::<String>()
            }
        }
    }
}

/// Convert plain text to an ADF document with multiple paragraphs.
///
/// Splits on double newlines (`\n\n`) to create separate paragraph nodes.
pub(crate) fn text_to_adf(text: &str) -> Value {
    let paragraphs: Vec<Value> = text
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            json!({
                "type": "paragraph",
                "content": [{ "type": "text", "text": p.trim() }]
            })
        })
        .collect();

    // If no paragraphs (empty text), still produce a valid doc with one empty paragraph.
    let content = if paragraphs.is_empty() {
        vec![json!({
            "type": "paragraph",
            "content": [{ "type": "text", "text": "" }]
        })]
    } else {
        paragraphs
    };

    json!({
        "version": 1,
        "type": "doc",
        "content": content
    })
}

/// Wrap a single string in one ADF paragraph node.
///
/// For form field values where the full text is a single paragraph.
pub(crate) fn text_to_adf_inline(text: &str) -> Value {
    json!({
        "version": 1,
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": text }]
        }]
    })
}

/// Extract the `content` array from an ADF node, returning an empty slice if absent.
fn get_content(node: &Value) -> Vec<&Value> {
    node.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default()
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_paragraph() {
        let adf = json!({
            "version": 1,
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Hello world" }]
            }]
        });
        assert_eq!(adf_to_text(&adf), "Hello world");
    }

    #[test]
    fn multiple_paragraphs() {
        let adf = json!({
            "version": 1,
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "First" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "Second" }] }
            ]
        });
        assert_eq!(adf_to_text(&adf), "First\n\nSecond");
    }

    #[test]
    fn heading() {
        let adf = json!({
            "type": "heading",
            "attrs": { "level": 2 },
            "content": [{ "type": "text", "text": "My Heading" }]
        });
        assert_eq!(adf_to_text(&adf), "## My Heading");
    }

    #[test]
    fn bullet_list() {
        let adf = json!({
            "type": "bulletList",
            "content": [
                { "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Item one" }] }
                ]},
                { "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Item two" }] }
                ]}
            ]
        });
        assert_eq!(adf_to_text(&adf), "- Item one\n- Item two");
    }

    #[test]
    fn ordered_list() {
        let adf = json!({
            "type": "orderedList",
            "content": [
                { "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "First" }] }
                ]},
                { "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Second" }] }
                ]}
            ]
        });
        assert_eq!(adf_to_text(&adf), "1. First\n2. Second");
    }

    #[test]
    fn code_block() {
        let adf = json!({
            "type": "codeBlock",
            "attrs": { "language": "rust" },
            "content": [{ "type": "text", "text": "fn main() {}" }]
        });
        assert_eq!(adf_to_text(&adf), "    fn main() {}");
    }

    #[test]
    fn link_text() {
        let adf = json!({
            "type": "text",
            "text": "click here",
            "marks": [{ "type": "link", "attrs": { "href": "https://example.com" } }]
        });
        assert_eq!(adf_to_text(&adf), "click here (https://example.com)");
    }

    #[test]
    fn hard_break() {
        let adf = json!({ "type": "hardBreak" });
        assert_eq!(adf_to_text(&adf), "\n");
    }

    #[test]
    fn mention() {
        let adf = json!({
            "type": "mention",
            "attrs": { "text": "@Matt Johnson", "id": "12345" }
        });
        assert_eq!(adf_to_text(&adf), "@Matt Johnson");
    }

    #[test]
    fn emoji() {
        let adf = json!({
            "type": "emoji",
            "attrs": { "shortName": ":thumbsup:", "id": "123" }
        });
        assert_eq!(adf_to_text(&adf), ":thumbsup:");
    }

    #[test]
    fn rule() {
        let adf = json!({ "type": "rule" });
        assert_eq!(adf_to_text(&adf), "---");
    }

    #[test]
    fn inline_card() {
        let adf = json!({
            "type": "inlineCard",
            "attrs": { "url": "https://jira.example.com/browse/HMI-103" }
        });
        assert_eq!(
            adf_to_text(&adf),
            "https://jira.example.com/browse/HMI-103"
        );
    }

    #[test]
    fn blockquote() {
        let adf = json!({
            "type": "blockquote",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Quoted text" }]
            }]
        });
        assert_eq!(adf_to_text(&adf), "> Quoted text");
    }

    #[test]
    fn null_node_returns_empty() {
        assert_eq!(adf_to_text(&json!(null)), "");
    }

    #[test]
    fn empty_doc() {
        let adf = json!({ "version": 1, "type": "doc", "content": [] });
        assert_eq!(adf_to_text(&adf), "");
    }

    #[test]
    fn unknown_node_type_recurses() {
        let adf = json!({
            "type": "panel",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "inside panel" }]
            }]
        });
        assert_eq!(adf_to_text(&adf), "inside panel");
    }

    // ── text_to_adf tests ────────────────────────────────────────────────────

    #[test]
    fn text_to_adf_single_paragraph() {
        let result = text_to_adf("Hello world");
        assert_eq!(result["version"], 1);
        assert_eq!(result["type"], "doc");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[0]["content"][0]["text"], "Hello world");
    }

    #[test]
    fn text_to_adf_multiple_paragraphs() {
        let result = text_to_adf("First paragraph\n\nSecond paragraph");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["content"][0]["text"], "First paragraph");
        assert_eq!(content[1]["content"][0]["text"], "Second paragraph");
    }

    #[test]
    fn text_to_adf_empty_string() {
        let result = text_to_adf("");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1); // fallback empty paragraph
    }

    #[test]
    fn text_to_adf_trims_paragraphs() {
        let result = text_to_adf("  trimmed  \n\n  also trimmed  ");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["content"][0]["text"], "trimmed");
        assert_eq!(content[1]["content"][0]["text"], "also trimmed");
    }

    // ── text_to_adf_inline tests ─────────────────────────────────────────────

    #[test]
    fn text_to_adf_inline_wraps_single_paragraph() {
        let result = text_to_adf_inline("Single line");
        assert_eq!(result["version"], 1);
        assert_eq!(result["type"], "doc");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["content"][0]["text"], "Single line");
    }

    #[test]
    fn text_to_adf_inline_preserves_newlines() {
        let result = text_to_adf_inline("Line 1\nLine 2\n\nLine 3");
        let content = result["content"].as_array().unwrap();
        // inline version always wraps in a single paragraph, preserving text as-is
        assert_eq!(content.len(), 1);
        assert_eq!(
            content[0]["content"][0]["text"],
            "Line 1\nLine 2\n\nLine 3"
        );
    }
}
