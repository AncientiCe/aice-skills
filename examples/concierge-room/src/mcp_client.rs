//! Minimal Hotel MCP client over JSON-RPC HTTP.
//!
//! The reference server in `examples/hotel-mcp/` returns
//! `application/json` for `tools/call`, so this client only implements
//! the request side; SSE notifications are not consumed because the
//! server does not initiate any during a tool call. A persistent SSE
//! channel can be added later for asynchronous progress events.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use skill_hotel::{HotelError, HotelMcpTransport};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// HTTP transport implementing `HotelMcpTransport` for any MCP server
/// that accepts JSON-RPC over `POST <url>`.
pub struct HttpMcpTransport {
    url: String,
    client: Client,
    next_id: AtomicI64,
}

impl HttpMcpTransport {
    pub fn new(url: impl Into<String>) -> Result<Self, HotelError> {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|error| HotelError::Other(format!("http client build failed: {error}")))?;
        Ok(Self {
            url: url.into(),
            client,
            next_id: AtomicI64::new(1),
        })
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i32,
    message: String,
}

#[async_trait]
impl HotelMcpTransport for HttpMcpTransport {
    async fn call_tool(&self, tool: &str, args: Value) -> Result<Value, HotelError> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        });
        let response = self
            .client
            .post(&self.url)
            .json(&payload)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    HotelError::Timeout
                } else {
                    HotelError::McpUnavailable(error.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(HotelError::McpRejected(format!("http {status}: {body}")));
        }
        let envelope: JsonRpcResponse = response.json().await.map_err(|error| {
            HotelError::McpUnavailable(format!("invalid mcp response: {error}"))
        })?;
        if let Some(rpc_err) = envelope.error {
            return Err(HotelError::McpRejected(rpc_err.message));
        }
        envelope
            .result
            .ok_or_else(|| HotelError::McpUnavailable("mcp response had no result".to_string()))
    }
}
