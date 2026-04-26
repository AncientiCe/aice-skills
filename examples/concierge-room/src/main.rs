use concierge_room::audio::spawn_capture;
use concierge_room::discovery::{discover_one, DiscoveryConfig};
use concierge_room::executor::HotelExecutor;
use concierge_room::mcp_client::HttpMcpTransport;
use concierge_room::stages::{MonoClock, VizEvent};
use concierge_room::viz::{router, VizHub};
use concierge_room::ws_loop::{connect, run_ws_loop, WsLoopConfig};
use skill_hotel::McpHotelSkill;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const DEFAULT_UI_BIND: &str = "127.0.0.1:8740";
const DEFAULT_MCP_URL: &str = "http://127.0.0.1:8790/mcp";
const DEFAULT_AUDIO_CHUNK_SAMPLES: usize = 1600;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,concierge_room=info")),
        )
        .init();

    let ui_bind: SocketAddr = env::var("CONCIERGE_UI_BIND")
        .unwrap_or_else(|_| DEFAULT_UI_BIND.to_string())
        .parse()
        .map_err(|error| format!("invalid CONCIERGE_UI_BIND: {error}"))?;
    let mcp_url = env::var("CONCIERGE_MCP_URL").unwrap_or_else(|_| DEFAULT_MCP_URL.to_string());
    let backend_ws_override = env::var("CONCIERGE_BACKEND_WS").ok();

    let hub = VizHub::new();
    let clock = MonoClock::new();

    spawn_ui_server(ui_bind, hub.clone());
    info!(%ui_bind, "browser UI available; navigate to http://{ui_bind}/");

    let mcp_transport = Arc::new(HttpMcpTransport::new(&mcp_url)?);
    let skill = Arc::new(McpHotelSkill::new(mcp_transport));
    let executor = Arc::new(HotelExecutor::new(skill, hub.clone(), clock));

    let ws_url = match backend_ws_override {
        Some(url) => url,
        None => {
            hub.emit(VizEvent::State {
                turn_id: None,
                t_ms: clock.elapsed_ms(),
                label: "discovering_backend".to_string(),
                detail: None,
            });
            match discover_one(DiscoveryConfig::default()).await {
                Ok(found) => {
                    info!(ws = %found.ws_base_url, "backend discovered");
                    found.ws_base_url
                }
                Err(error) => {
                    error!(%error, "backend discovery failed");
                    hub.emit(VizEvent::BackendError {
                        turn_id: None,
                        t_ms: clock.elapsed_ms(),
                        message: format!("backend discovery failed: {error}"),
                    });
                    return Err(format!("backend discovery failed: {error}").into());
                }
            }
        }
    };

    let session_id = Uuid::new_v4().to_string();
    let device_id = env::var("CONCIERGE_DEVICE_ID")
        .unwrap_or_else(|_| format!("concierge-room-{}", &session_id[..8.min(session_id.len())]));

    hub.emit(VizEvent::State {
        turn_id: None,
        t_ms: clock.elapsed_ms(),
        label: "connecting_backend".to_string(),
        detail: Some(ws_url.clone()),
    });
    let ws = connect(&ws_url).await?;

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let _audio_stream = match spawn_capture(DEFAULT_AUDIO_CHUNK_SAMPLES) {
        Ok((mut rx, stream)) => {
            tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    if audio_tx.send(chunk).is_err() {
                        break;
                    }
                }
            });
            Some(stream)
        }
        Err(error) => {
            warn!(%error, "microphone unavailable, continuing without mic capture");
            hub.emit(VizEvent::State {
                turn_id: None,
                t_ms: clock.elapsed_ms(),
                label: "microphone_unavailable".to_string(),
                detail: Some(error.to_string()),
            });
            None
        }
    };

    hub.emit(VizEvent::State {
        turn_id: None,
        t_ms: clock.elapsed_ms(),
        label: "connected".to_string(),
        detail: Some(ws_url.clone()),
    });

    let config = WsLoopConfig {
        ws_url,
        session_id,
        device_id,
    };

    let result = run_ws_loop(ws, config, audio_rx, executor, hub.clone(), clock).await;
    if let Err(error) = result {
        error!(%error, "ws loop ended");
        hub.emit(VizEvent::BackendError {
            turn_id: None,
            t_ms: clock.elapsed_ms(),
            message: error.to_string(),
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Ok(())
}

fn spawn_ui_server(addr: SocketAddr, hub: VizHub) {
    let app = router(hub);
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if let Err(error) = axum::serve(listener, app).await {
                    warn!(%error, "ui server stopped");
                }
            }
            Err(error) => warn!(%error, "ui server bind failed"),
        }
    });
}
