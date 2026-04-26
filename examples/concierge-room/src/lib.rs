//! Concierge-room bridge: mirrors the `aice-macos` discovery + WS flow
//! while restricting the frontend skill set to `skill_hotel` and serving
//! a browser-based "room control center" that visualises every stage of
//! the user's journey.
//!
//! Layout:
//!
//! - [`audio`] — cpal microphone capture with a naive resampler down to
//!   the backend's 16 kHz mono PCM contract.
//! - [`discovery`] — UDP `FIND` / `HERE:<port>` resolution (drop-in
//!   replacement for the version in `aice-macos`).
//! - [`protocol`] — local copy of the `core-runtime-protocol` wire
//!   types so this example crate is self-contained.
//! - [`ws_loop`] — drives a single backend WebSocket session.
//! - [`executor`] — turns `FrontendSkillIntent` into a hotel
//!   `McpHotelSkill` call and back into a `FrontendSkillResultRequest`.
//! - [`mcp_client`] — `HotelMcpTransport` impl over JSON-RPC HTTP.
//! - [`viz`] — broadcast channel + axum `/viz` WS for the browser UI.
//! - [`stages`] — `VizEvent` enum with monotonic timestamps.
//! - [`ui_assets`] — `rust-embed` wrapper around the SPA bundle.

pub mod audio;
pub mod discovery;
pub mod executor;
pub mod mcp_client;
pub mod protocol;
pub mod stages;
pub mod ui_assets;
pub mod viz;
pub mod ws_loop;

pub const SUPPORTED_FRONTEND_INTENTS: &[&str] = &["skill_hotel"];

/// PCM sample rate the backend expects (16 kHz mono int16 little-endian).
pub const BACKEND_SAMPLE_RATE: u32 = 16_000;
