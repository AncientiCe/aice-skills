//! End-to-end test of the concierge bridge against a fake backend WS
//! and a fake MCP HTTP server.

use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{any, post};
use axum::{Json, Router};
use concierge_room::executor::HotelExecutor;
use concierge_room::mcp_client::HttpMcpTransport;
use concierge_room::stages::{MonoClock, VizEvent};
use concierge_room::viz::VizHub;
use concierge_room::ws_loop::{connect, run_ws_loop, WsLoopConfig};
use futures_util::StreamExt;
use serde_json::{json, Value};
use skill_hotel::McpHotelSkill;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

async fn start_fake_backend() -> (
    String,
    tokio::task::JoinHandle<()>,
    mpsc::UnboundedReceiver<Value>,
) {
    let (client_msg_tx, client_msg_rx) = mpsc::unbounded_channel();
    let app = Router::new()
        .route("/turns/stream", any(ws_handler))
        .with_state(BackendState {
            client_msg_tx: Arc::new(client_msg_tx),
        });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("ws://{addr}"), handle, client_msg_rx)
}

#[derive(Clone)]
struct BackendState {
    client_msg_tx: Arc<mpsc::UnboundedSender<Value>>,
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<BackendState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run_fake_session(socket, state))
}

async fn run_fake_session(mut socket: WebSocket, state: BackendState) {
    while let Some(msg) = socket.next().await {
        let Ok(msg) = msg else { return };
        match msg {
            AxumMessage::Text(text) => {
                let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let _ = state.client_msg_tx.send(parsed.clone());
                if parsed.get("type") == Some(&Value::String("turn_start".to_string())) {
                    let turn_id = parsed
                        .get("turn_id")
                        .and_then(Value::as_str)
                        .unwrap_or("T")
                        .to_string();

                    let partial = json!({
                        "type": "partial_transcript",
                        "turn_id": turn_id,
                        "text": "set the room temperature to 22 degrees",
                        "stable": true,
                    });
                    let _ = socket
                        .send(AxumMessage::Text(partial.to_string().into()))
                        .await;

                    let intent_update = json!({
                        "type": "intent_update",
                        "turn_id": turn_id,
                        "intent": "skill_hotel",
                    });
                    let _ = socket
                        .send(AxumMessage::Text(intent_update.to_string().into()))
                        .await;

                    let frontend_intent = json!({
                        "type": "frontend_skill_intent",
                        "turn_id": turn_id,
                        "intent": "skill_hotel",
                        "slots": {
                            "hotel_intent_kind": "set_room_temperature",
                            "hotel_slots": { "celsius": 22 }
                        },
                        "user_text": "set the room temperature to 22 degrees"
                    });
                    let _ = socket
                        .send(AxumMessage::Text(frontend_intent.to_string().into()))
                        .await;

                    if let Some(skill_msg) = wait_for_skill_result(&mut socket, &state).await {
                        let _ = state.client_msg_tx.send(skill_msg);
                    }

                    let token = json!({
                        "type": "token",
                        "turn_id": turn_id,
                        "text": "Done.",
                    });
                    let _ = socket
                        .send(AxumMessage::Text(token.to_string().into()))
                        .await;

                    let done = json!({
                        "type": "done",
                        "turn_id": turn_id,
                    });
                    let _ = socket
                        .send(AxumMessage::Text(done.to_string().into()))
                        .await;
                    return;
                }
            }
            AxumMessage::Binary(_) => continue,
            AxumMessage::Close(_) => return,
            _ => continue,
        }
    }
}

async fn wait_for_skill_result(socket: &mut WebSocket, _state: &BackendState) -> Option<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let frame = tokio::time::timeout(Duration::from_millis(500), socket.next()).await;
        let Ok(Some(Ok(AxumMessage::Text(text)))) = frame else {
            continue;
        };
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if parsed.get("type") == Some(&Value::String("frontend_skill_result".to_string())) {
            return Some(parsed);
        }
    }
    None
}

async fn start_fake_mcp() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new().route("/mcp", post(mcp_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr: SocketAddr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}/mcp"), handle)
}

async fn mcp_handler(Json(payload): Json<Value>) -> Json<Value> {
    let id = payload.get("id").cloned().unwrap_or(Value::Null);
    let method = payload.get("method").and_then(Value::as_str).unwrap_or("");
    if method == "tools/call" {
        let name = payload
            .pointer("/params/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("{} confirmed", name) }],
                "isError": false,
                "status": "confirmed",
                "structuredContent": { "echo": payload.get("params") }
            }
        });
        return Json(response);
    }
    Json(
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "method not found" } }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_journey_emits_expected_viz_events_and_skill_result() {
    let (backend_url, backend_handle, mut client_msgs) = start_fake_backend().await;
    let (mcp_url, mcp_handle) = start_fake_mcp().await;

    let hub = VizHub::new();
    let clock = MonoClock::new();
    let mut viz_rx = hub.subscribe();
    let transport = Arc::new(HttpMcpTransport::new(&mcp_url).expect("transport"));
    let skill = Arc::new(McpHotelSkill::new(transport));
    let executor = Arc::new(HotelExecutor::new(skill, hub.clone(), clock));

    let ws = connect(&backend_url).await.expect("ws connect");
    let (_audio_tx, audio_rx) = mpsc::unbounded_channel();

    let config = WsLoopConfig {
        ws_url: backend_url.clone(),
        session_id: "session-test".to_string(),
        device_id: "concierge-room-test".to_string(),
    };

    let loop_handle = tokio::spawn(run_ws_loop(
        ws,
        config,
        audio_rx,
        executor,
        hub.clone(),
        clock,
    ));

    let mut events: Vec<VizEvent> = Vec::new();
    let collect_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < collect_deadline {
        match tokio::time::timeout(Duration::from_millis(200), viz_rx.recv()).await {
            Ok(Ok(event)) => {
                let stage = event.stage();
                events.push(event);
                if stage == "skill_result_sent" {
                    break;
                }
            }
            _ => continue,
        }
    }

    loop_handle.abort();
    backend_handle.abort();
    mcp_handle.abort();
    let _ = loop_handle.await;
    let _ = backend_handle.await;
    let _ = mcp_handle.await;

    let stages: Vec<&str> = events.iter().map(VizEvent::stage).collect();
    assert!(
        stages.contains(&"turn_started"),
        "missing turn_started in {:?}",
        stages
    );
    assert!(
        stages.contains(&"transcript_partial"),
        "missing transcript_partial in {:?}",
        stages
    );
    assert!(
        stages.contains(&"classifier_started"),
        "missing classifier_started in {:?}",
        stages
    );
    assert!(
        stages.contains(&"classifier_done"),
        "missing classifier_done in {:?}",
        stages
    );
    assert!(
        stages.contains(&"mcp_call_started"),
        "missing mcp_call_started in {:?}",
        stages
    );
    assert!(
        stages.contains(&"mcp_call_done"),
        "missing mcp_call_done in {:?}",
        stages
    );
    assert!(
        stages.contains(&"skill_result_sent"),
        "missing skill_result_sent in {:?}",
        stages
    );

    let mut saw_turn_start = false;
    let mut skill_result: Option<Value> = None;
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < drain_deadline {
        match tokio::time::timeout(Duration::from_millis(100), client_msgs.recv()).await {
            Ok(Some(msg)) => {
                let kind = msg
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if kind == "turn_start" {
                    saw_turn_start = true;
                    let intents = msg
                        .get("supported_frontend_intents")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    assert!(intents.iter().any(|v| v == "skill_hotel"));
                } else if kind == "frontend_skill_result" {
                    skill_result = Some(msg);
                }
            }
            _ => continue,
        }
    }

    assert!(saw_turn_start, "backend never received turn_start");
    let result = skill_result.expect("backend never received frontend_skill_result");
    let user_text = result
        .pointer("/result/user_text")
        .and_then(Value::as_str)
        .expect("user_text");
    assert!(
        user_text.contains("set_room_temperature"),
        "user_text: {user_text}"
    );
    let status = result
        .pointer("/result/status")
        .and_then(Value::as_str)
        .expect("status");
    assert_eq!(status, "confirmed");
}
