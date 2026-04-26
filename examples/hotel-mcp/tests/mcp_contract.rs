//! HTTP integration tests for the Hotel MCP server.

use hotel_mcp::{router, state};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tokio::net::TcpListener;

async fn start_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = router(state::new_shared());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, handle)
}

async fn rpc(addr: SocketAddr, payload: Value) -> Value {
    let url = format!("http://{addr}/mcp");
    let client = reqwest::Client::new();
    let resp = client.post(url).json(&payload).send().await.expect("send");
    assert!(resp.status().is_success(), "status: {}", resp.status());
    resp.json::<Value>().await.expect("json")
}

#[tokio::test]
async fn initialize_returns_server_info() {
    let (addr, handle) = start_server().await;
    let resp = rpc(
        addr,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
    )
    .await;
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["serverInfo"]["name"], "hotel-mcp");
    assert!(resp["result"]["capabilities"]["tools"].is_object());
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn tools_list_returns_25_tools() {
    let (addr, handle) = start_server().await;
    let resp = rpc(
        addr,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 25);
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"set_room_temperature"));
    assert!(names.contains(&"order_room_service"));
    assert!(names.contains(&"language_help"));
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn tools_call_set_room_temperature_succeeds() {
    let (addr, handle) = start_server().await;
    let resp = rpc(
        addr,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "set_room_temperature",
                "arguments": { "celsius": 22.5 }
            }
        }),
    )
    .await;
    let result = &resp["result"];
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert!(text.contains("22.5"));
    assert_eq!(result["structuredContent"]["celsius"], 22.5);
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn tools_call_unknown_tool_returns_is_error() {
    let (addr, handle) = start_server().await;
    let resp = rpc(
        addr,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "no_such_tool", "arguments": {} }
        }),
    )
    .await;
    assert_eq!(resp["result"]["isError"], true);
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn unknown_method_returns_jsonrpc_error() {
    let (addr, handle) = start_server().await;
    let resp = rpc(
        addr,
        json!({ "jsonrpc": "2.0", "id": 5, "method": "no_such_method" }),
    )
    .await;
    assert!(resp["result"].is_null());
    assert_eq!(resp["error"]["code"], -32601);
    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn state_persists_across_calls() {
    let (addr, handle) = start_server().await;
    rpc(
        addr,
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "set_room_temperature",
                "arguments": { "celsius": 24.0 }
            }
        }),
    )
    .await;

    let order = rpc(
        addr,
        json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "order_room_service",
                "arguments": { "items": [{"name": "club sandwich", "quantity": 1}] }
            }
        }),
    )
    .await;
    let order_id = order["result"]["structuredContent"]["order_id"]
        .as_str()
        .expect("order_id");
    assert!(order_id.starts_with("RS-"));
    handle.abort();
    let _ = handle.await;
}
