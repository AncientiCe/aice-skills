//! Single backend WebSocket session.
//!
//! Connects to `<ws_url>/turns/stream`, sends a `TurnStart` advertising
//! `skill_hotel` as the only supported frontend intent, then streams
//! audio frames while reading server events. When `Done` arrives we
//! send `TurnDone` and immediately open a new turn so subsequent
//! utterances reuse the same WebSocket.

use crate::executor::FrontendSkillExecutor;
use crate::protocol::{TurnStreamClientMessage, TurnStreamServerEvent};
use crate::stages::{MonoClock, VizEvent};
use crate::viz::VizHub;
use crate::SUPPORTED_FRONTEND_INTENTS;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Maximum time we wait for a single server frame before assuming the
/// session is hung. Audio still flows during this window.
const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum WsLoopError {
    #[error("ws connect failed: {0}")]
    Connect(String),
    #[error("ws io error: {0}")]
    Io(String),
    #[error("ws closed by remote")]
    Closed,
    #[error("ws read timed out")]
    Timeout,
    #[error("audio channel closed")]
    AudioClosed,
}

/// Inputs to the WS loop. Lets the integration test substitute a
/// hand-crafted backend.
pub struct WsLoopConfig {
    pub ws_url: String,
    pub session_id: String,
    pub device_id: String,
}

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Open a WebSocket to the backend's `/turns/stream` endpoint.
pub async fn connect(ws_url: &str) -> Result<WsStream, WsLoopError> {
    let url = if ws_url.ends_with("/turns/stream") {
        ws_url.to_string()
    } else {
        format!("{}/turns/stream", ws_url.trim_end_matches('/'))
    };
    let (stream, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|error| WsLoopError::Connect(error.to_string()))?;
    Ok(stream)
}

/// Run the WS loop until the audio channel closes or a fatal error
/// occurs. The loop processes multiple turns sequentially.
pub async fn run_ws_loop<E>(
    mut ws: WsStream,
    config: WsLoopConfig,
    mut audio: mpsc::UnboundedReceiver<Vec<i16>>,
    executor: Arc<E>,
    hub: VizHub,
    clock: MonoClock,
) -> Result<(), WsLoopError>
where
    E: FrontendSkillExecutor + 'static,
{
    let mut turn_id = new_turn_id();
    let mut turn_started_at = Instant::now();
    let mut classifier_started_at: Option<Instant> = None;
    let mut active_intent: Option<String> = None;

    send_turn_start(&mut ws, &config.session_id, &config.device_id, &turn_id).await?;
    hub.emit(VizEvent::TurnStarted {
        turn_id: turn_id.clone(),
        t_ms: clock.elapsed_ms(),
        session_id: config.session_id.clone(),
    });

    loop {
        tokio::select! {
            biased;
            frame = tokio::time::timeout(FRAME_READ_TIMEOUT, ws.next()) => {
                let msg = match frame {
                    Ok(Some(Ok(message))) => message,
                    Ok(Some(Err(error))) => return Err(WsLoopError::Io(error.to_string())),
                    Ok(None) => return Err(WsLoopError::Closed),
                    Err(_) => return Err(WsLoopError::Timeout),
                };
                let Message::Text(text) = msg else { continue };
                let Ok(event) = serde_json::from_str::<TurnStreamServerEvent>(&text) else {
                    debug!(payload = %text, "ignoring non-server-event text frame");
                    continue;
                };
                match event {
                    TurnStreamServerEvent::PartialTranscript { turn_id: t, text, stable } => {
                        hub.emit(VizEvent::TranscriptPartial {
                            turn_id: t,
                            t_ms: clock.elapsed_ms(),
                            text,
                            stable,
                        });
                        if classifier_started_at.is_none() {
                            classifier_started_at = Some(Instant::now());
                            hub.emit(VizEvent::ClassifierStarted {
                                turn_id: turn_id.clone(),
                                t_ms: clock.elapsed_ms(),
                            });
                        }
                    }
                    TurnStreamServerEvent::IntentUpdate { turn_id: t, intent } => {
                        active_intent = Some(intent.clone());
                        hub.emit(VizEvent::ClassifierIntent {
                            turn_id: t,
                            t_ms: clock.elapsed_ms(),
                            intent,
                        });
                    }
                    TurnStreamServerEvent::FrontendSkillIntent(intent) => {
                        let latency_ms = classifier_started_at
                            .map(|s| s.elapsed().as_millis() as u64)
                            .unwrap_or(0);
                        hub.emit(VizEvent::ClassifierDone {
                            turn_id: intent.turn_id.clone(),
                            t_ms: clock.elapsed_ms(),
                            intent: active_intent
                                .clone()
                                .unwrap_or_else(|| intent.intent.clone()),
                            slots: intent.slots.clone(),
                            user_text: intent.user_text.clone(),
                            latency_ms,
                        });
                        let result = executor.execute(&intent).await;
                        let msg = TurnStreamClientMessage::FrontendSkillResult {
                            turn_id: intent.turn_id.clone(),
                            intent_id: intent.intent.clone(),
                            result: result.clone(),
                        };
                        send_text(&mut ws, &msg).await?;
                        hub.emit(VizEvent::SkillResultSent {
                            turn_id: intent.turn_id.clone(),
                            t_ms: clock.elapsed_ms(),
                            status: result.status.clone(),
                            user_text: result.user_text.clone(),
                        });
                    }
                    TurnStreamServerEvent::Token { turn_id: t, text } => {
                        hub.emit(VizEvent::AssistantToken {
                            turn_id: t,
                            t_ms: clock.elapsed_ms(),
                            token: text,
                        });
                    }
                    TurnStreamServerEvent::Done { turn_id: t } => {
                        let total_ms = turn_started_at.elapsed().as_millis() as u64;
                        hub.emit(VizEvent::TurnDone {
                            turn_id: t.clone(),
                            t_ms: clock.elapsed_ms(),
                            total_ms,
                        });
                        let _ = send_text(&mut ws, &TurnStreamClientMessage::TurnDone).await;
                        turn_id = new_turn_id();
                        turn_started_at = Instant::now();
                        classifier_started_at = None;
                        active_intent = None;
                        send_turn_start(
                            &mut ws,
                            &config.session_id,
                            &config.device_id,
                            &turn_id,
                        ).await?;
                        hub.emit(VizEvent::TurnStarted {
                            turn_id: turn_id.clone(),
                            t_ms: clock.elapsed_ms(),
                            session_id: config.session_id.clone(),
                        });
                    }
                    TurnStreamServerEvent::Error { turn_id: t, message } => {
                        warn!(?t, %message, "backend error");
                        hub.emit(VizEvent::BackendError {
                            turn_id: t,
                            t_ms: clock.elapsed_ms(),
                            message,
                        });
                    }
                }
            }
            chunk = audio.recv() => match chunk {
                Some(samples) => {
                    let bytes: Vec<u8> = samples
                        .iter()
                        .flat_map(|s| s.to_le_bytes())
                        .collect();
                    if let Err(error) = ws.send(Message::Binary(bytes)).await {
                        return Err(WsLoopError::Io(error.to_string()));
                    }
                }
                None => return Err(WsLoopError::AudioClosed),
            },
        }
    }
}

fn new_turn_id() -> String {
    Uuid::new_v4().to_string()
}

async fn send_turn_start(
    ws: &mut WsStream,
    session_id: &str,
    device_id: &str,
    turn_id: &str,
) -> Result<(), WsLoopError> {
    let msg = TurnStreamClientMessage::TurnStart {
        session_id: session_id.to_string(),
        device_id: Some(device_id.to_string()),
        turn_id: turn_id.to_string(),
        supported_frontend_intents: SUPPORTED_FRONTEND_INTENTS
            .iter()
            .map(|s| s.to_string())
            .collect(),
        schema_version: None,
    };
    info!(turn_id, "TurnStart sent (skill_hotel only)");
    send_text(ws, &msg).await
}

async fn send_text(ws: &mut WsStream, msg: &TurnStreamClientMessage) -> Result<(), WsLoopError> {
    let json = serde_json::to_string(msg).map_err(|error| WsLoopError::Io(error.to_string()))?;
    ws.send(Message::Text(json))
        .await
        .map_err(|error| WsLoopError::Io(error.to_string()))
}
