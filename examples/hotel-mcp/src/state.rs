//! Shared in-memory state for the mock Hotel MCP server.

use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-room state. The mock keeps everything in memory; production
/// deployments would persist to a property management system.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RoomState {
    pub room_temperature_celsius: Option<f64>,
    pub lights_scene: Option<String>,
    pub lights_brightness: Option<u8>,
    pub curtains_position: Option<String>,
    pub tv_state: Option<String>,
    pub ambient_music: Option<String>,
    pub do_not_disturb: bool,
    pub do_not_disturb_until: Option<String>,
    pub wake_up_call_time: Option<String>,
    pub late_checkout_until: Option<String>,
    pub orders: Vec<RoomOrder>,
    pub housekeeping: Vec<HousekeepingRequest>,
    pub bookings: Vec<Booking>,
    pub complaints: Vec<Ticket>,
    pub lost_items: Vec<Ticket>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomOrder {
    pub id: String,
    pub kind: String,
    pub eta_minutes: u32,
    pub items: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct HousekeepingRequest {
    pub id: String,
    pub kind: String,
    pub when: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Booking {
    pub id: String,
    pub kind: String,
    pub time: Option<String>,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct Ticket {
    pub id: String,
    pub category: String,
    pub details: Option<String>,
}

/// Cheap clone-able handle to the room state.
pub type SharedState = Arc<Mutex<RoomState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(RoomState::default()))
}
