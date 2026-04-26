//! Reference Hotel MCP server.
//!
//! Implements a minimal MCP-over-HTTP transport that the
//! `concierge-room` demo (or any MCP client) can call. We support:
//!
//! - `POST /mcp` — JSON-RPC requests; replies use `application/json`.
//! - `GET /mcp` — server-initiated event stream (SSE; heartbeat only).
//! - `DELETE /mcp` — explicit session termination.
//!
//! The set of tools exposed is exactly `HotelIntentKind::ALL`. Each tool
//! mutates an in-memory `RoomState` so that subsequent calls see prior
//! actions (e.g. setting the room temperature is reflected in
//! `concierge_info`).

pub mod state;
pub mod tools;
pub mod transport;

pub use state::{RoomState, SharedState};
pub use transport::{router, McpServerInfo};

/// Default bind address used by the binary and integration tests when
/// `HOTEL_MCP_BIND` is not set.
pub const DEFAULT_BIND: &str = "127.0.0.1:8790";
