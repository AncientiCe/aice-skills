//! HTTP/SSE transport for the Hotel MCP server.

use crate::state::SharedState;
use crate::tools;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

/// Server identity returned in `initialize`.
#[derive(Clone, Debug)]
pub struct McpServerInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub protocol_version: &'static str,
}

impl Default for McpServerInfo {
    fn default() -> Self {
        Self {
            name: "hotel-mcp",
            version: env!("CARGO_PKG_VERSION"),
            protocol_version: "2024-11-05",
        }
    }
}

#[derive(Clone)]
struct AppState {
    state: SharedState,
    info: McpServerInfo,
}

/// Build the axum router with the JSON-RPC POST endpoint, the SSE GET
/// endpoint, and the DELETE termination endpoint.
pub fn router(state: SharedState) -> Router {
    let app = AppState {
        state,
        info: McpServerInfo::default(),
    };
    Router::new()
        .route(
            "/mcp",
            post(handle_post).get(handle_sse).delete(handle_delete),
        )
        .route("/health", get(handle_health))
        .with_state(app)
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

async fn handle_health() -> &'static str {
    "ok"
}

async fn handle_post(State(app): State<AppState>, Json(payload): Json<Value>) -> impl IntoResponse {
    if let Some(array) = payload.as_array() {
        let mut responses = Vec::with_capacity(array.len());
        for entry in array {
            if let Some(resp) = process_message(&app, entry.clone()).await {
                responses.push(resp);
            }
        }
        if responses.is_empty() {
            return (StatusCode::ACCEPTED, mcp_headers(), Json(Value::Null)).into_response();
        }
        return (StatusCode::OK, mcp_headers(), Json(Value::Array(responses))).into_response();
    }
    match process_message(&app, payload).await {
        Some(resp) => (StatusCode::OK, mcp_headers(), Json(resp)).into_response(),
        None => (StatusCode::ACCEPTED, mcp_headers(), Json(Value::Null)).into_response(),
    }
}

fn mcp_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Mcp-Session-Id",
        HeaderValue::from_static("hotel-mcp-default"),
    );
    headers
}

async fn process_message(app: &AppState, payload: Value) -> Option<Value> {
    let Ok(req) = serde_json::from_value::<JsonRpcRequest>(payload.clone()) else {
        return Some(error_response(
            payload.get("id").cloned().unwrap_or(Value::Null),
            -32700,
            "Parse error",
        ));
    };

    if req.id.is_none() {
        match req.method.as_str() {
            "notifications/initialized" | "notifications/cancelled" => return None,
            _ => return None,
        }
    }

    let id = req.id.clone().unwrap_or(Value::Null);
    let result = dispatch(app, &req).await;
    let response = match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(value),
            error: None,
        },
        Err((code, message)) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        },
    };
    Some(value_from(response))
}

fn value_from(resp: JsonRpcResponse) -> Value {
    serde_json::to_value(resp).unwrap_or(Value::Null)
}

async fn dispatch(app: &AppState, req: &JsonRpcRequest) -> Result<Value, (i32, String)> {
    match req.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": app.info.protocol_version,
            "capabilities": {
                "tools": { "listChanged": false },
                "logging": {},
            },
            "serverInfo": {
                "name": app.info.name,
                "version": app.info.version,
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": tools::catalog(),
        })),
        "tools/call" => {
            let params = req.params.clone().unwrap_or(Value::Null);
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((-32602, "tools/call missing 'name'".to_string()))?
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(tools::call(&app.state, &name, &args).await)
        }
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    value_from(JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    })
}

async fn handle_sse(
    State(_app): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let interval = tokio::time::interval(Duration::from_secs(15));
    let stream =
        IntervalStream::new(interval).map(|_| Ok(Event::default().event("ping").data("{}")));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn handle_delete(State(_app): State<AppState>) -> StatusCode {
    StatusCode::NO_CONTENT
}
