//! Local axum server that hosts the SPA and broadcasts viz events.

use crate::stages::VizEvent;
use crate::ui_assets;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Capacity of the broadcast channel feeding browser clients. New events
/// over capacity drop the oldest first; UI just reconnects to catch up.
pub const VIZ_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct VizHub {
    sender: broadcast::Sender<VizEvent>,
}

impl VizHub {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(VIZ_CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VizEvent> {
        self.sender.subscribe()
    }

    pub fn emit(&self, event: VizEvent) {
        let _ = self.sender.send(event);
    }

    pub fn sender(&self) -> broadcast::Sender<VizEvent> {
        self.sender.clone()
    }
}

impl Default for VizHub {
    fn default() -> Self {
        Self::new()
    }
}

pub struct UiState {
    pub hub: VizHub,
}

pub fn router(hub: VizHub) -> Router {
    let state = Arc::new(UiState { hub });
    Router::new()
        .route("/viz", get(viz_ws))
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_asset))
        .with_state(state)
}

async fn viz_ws(ws: WebSocketUpgrade, State(state): State<Arc<UiState>>) -> impl IntoResponse {
    let mut rx = state.hub.subscribe();
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = run_viz_ws(socket, &mut rx).await {
            tracing::debug!(%error, "viz ws ended");
        }
    })
}

async fn run_viz_ws(
    socket: WebSocket,
    rx: &mut broadcast::Receiver<VizEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            biased;
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(_)) => continue,
            },
            recv = rx.recv() => match recv {
                Ok(event) => {
                    let json = serde_json::to_string(&event)?;
                    sink.send(Message::Text(json.into())).await?;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let warn = serde_json::json!({
                        "stage": "state",
                        "t_ms": 0,
                        "label": format!("ui_lagged_{n}"),
                    });
                    sink.send(Message::Text(warn.to_string().into())).await?;
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
        }
    }
}

async fn serve_index(State(_state): State<Arc<UiState>>) -> Response {
    serve_path("index.html")
}

async fn serve_asset(Path(path): Path<String>) -> Response {
    serve_path(&path)
}

fn serve_path(path: &str) -> Response {
    if let Some(content) = ui_assets::Asset::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let mut response = (StatusCode::OK, content.data.into_owned()).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            mime.as_ref().parse().unwrap_or_else(|_| {
                "application/octet-stream"
                    .parse()
                    .expect("static value parses as header")
            }),
        );
        response
    } else if let Some(content) = ui_assets::Asset::get("index.html") {
        let mut response = (StatusCode::OK, content.data.into_owned()).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            "text/html".parse().expect("static value parses as header"),
        );
        response
    } else {
        (StatusCode::NOT_FOUND, "ui assets not bundled").into_response()
    }
}
