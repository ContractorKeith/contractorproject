//! JSON-RPC 2.0 wire shapes for the stdio helper, and how an `ApplicationError`
//! becomes an error response.
//!
//! Hand-rolled on purpose, matching ContractorBooks: MCP over stdio is
//! newline-delimited JSON-RPC and this helper needs four methods. A small
//! synchronous loop matches the blocking SQLite seam it wraps.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::ApplicationError;
use crate::mcp::bounded;

/// The MCP revision this helper implements (same as ContractorBooks).
pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const APPLICATION_ERROR: i64 = -32000;

/// One incoming message. An absent `id` is a notification (answered with
/// silence); `"id": null` is a request whose id is null (answered).
#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default, deserialize_with = "present_id")]
    pub id: Option<Option<Value>>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

fn present_id<'de, D>(deserializer: D) -> Result<Option<Option<Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

pub fn result(id: Value, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

pub fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut body = Map::new();
    body.insert("code".into(), json!(code));
    body.insert("message".into(), json!(message));
    if let Some(data) = data {
        body.insert("data".into(), data);
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": Value::Object(body) })
}

/// Structured `data` on an error: the stable `kind` from docs/LOCAL_API.md plus
/// the safe details an agent needs to recover (field path, current version).
pub fn error_data(error: &ApplicationError) -> Value {
    let mut data = Map::new();
    let kind = match error {
        // `read_only` is its own documented kind even though it travels as a
        // validation failure inside the application error type.
        ApplicationError::ValidationFailed {
            code: "read_only", ..
        } => "read_only",
        ApplicationError::ValidationFailed {
            code: "dependency_cycle",
            ..
        } => "dependency_cycle",
        other => other.kind(),
    };
    data.insert("kind".into(), json!(kind));
    match error {
        ApplicationError::InvalidInput { field, .. } => {
            data.insert("field".into(), json!(field));
        }
        ApplicationError::ValidationFailed { code, field, .. } => {
            data.insert("code".into(), json!(code));
            data.insert("field".into(), json!(field));
        }
        ApplicationError::NotFound { resource, id } => {
            data.insert("resource".into(), json!(resource));
            data.insert("resourceId".into(), json!(bounded(id)));
        }
        ApplicationError::VersionConflict {
            resource,
            id,
            expected,
            current,
        } => {
            data.insert("resource".into(), json!(resource));
            data.insert("resourceId".into(), json!(bounded(id)));
            data.insert("expectedVersion".into(), json!(expected));
            data.insert("currentVersion".into(), json!(current));
        }
        ApplicationError::DuplicateCommand { command_id } => {
            data.insert("commandId".into(), json!(bounded(command_id)));
        }
        _ => {}
    }
    Value::Object(data)
}

/// Bad input is the caller's mistake (-32602); everything else is the
/// application refusing the command (-32000).
pub fn error_code(error: &ApplicationError) -> i64 {
    match error {
        ApplicationError::InvalidInput { .. } => INVALID_PARAMS,
        _ => APPLICATION_ERROR,
    }
}

/// A tool result: one JSON text block plus the same value as structured content.
pub fn tool_content(value: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(value).unwrap_or_default() }],
        "structuredContent": value,
        "isError": false
    })
}

/// The refusal for a write tool called in read-only mode.
pub fn read_only_error() -> ApplicationError {
    ApplicationError::ValidationFailed {
        code: "read_only",
        field: "mode",
        message:
            "this helper was started with --read-only; restart it without that flag to change jobs"
                .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_conflict_carries_both_versions() {
        let data = error_data(&ApplicationError::VersionConflict {
            resource: "job",
            id: "job-1".into(),
            expected: 1,
            current: 2,
        });
        assert_eq!(data["kind"], json!("version_conflict"));
        assert_eq!(data["expectedVersion"], json!(1));
        assert_eq!(data["currentVersion"], json!(2));
    }

    #[test]
    fn read_only_is_its_own_kind() {
        let data = error_data(&read_only_error());
        assert_eq!(data["kind"], json!("read_only"));
    }
}
