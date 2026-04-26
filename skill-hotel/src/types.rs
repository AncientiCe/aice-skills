//! Public types for the hotel concierge skill.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Canonical concierge intents supported by `skill-hotel`.
///
/// Each variant maps 1:1 to a Hotel MCP tool name (snake_case). The
/// classifier emits one of these as the `hotel_intent_kind` slot, and the
/// frontend routes it to the configured `HotelMcpTransport`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotelIntentKind {
    SetRoomTemperature,
    SetLights,
    SetCurtains,
    SetTv,
    SetAmbientMusic,
    SetDoNotDisturb,
    OrderRoomService,
    RequestHousekeeping,
    RequestExtraTowels,
    RequestExtraPillows,
    RequestToiletries,
    RequestLaundryPickup,
    RequestIron,
    SetWakeUpCall,
    RequestLateCheckout,
    BookRestaurant,
    BookSpa,
    BookTaxi,
    ConciergeInfo,
    RequestLocalRecommendation,
    ReportComplaint,
    ReportLostItem,
    RequestBillingSummary,
    RequestCheckout,
    LanguageHelp,
}

impl HotelIntentKind {
    /// All canonical hotel intents in declaration order.
    pub const ALL: &'static [HotelIntentKind] = &[
        Self::SetRoomTemperature,
        Self::SetLights,
        Self::SetCurtains,
        Self::SetTv,
        Self::SetAmbientMusic,
        Self::SetDoNotDisturb,
        Self::OrderRoomService,
        Self::RequestHousekeeping,
        Self::RequestExtraTowels,
        Self::RequestExtraPillows,
        Self::RequestToiletries,
        Self::RequestLaundryPickup,
        Self::RequestIron,
        Self::SetWakeUpCall,
        Self::RequestLateCheckout,
        Self::BookRestaurant,
        Self::BookSpa,
        Self::BookTaxi,
        Self::ConciergeInfo,
        Self::RequestLocalRecommendation,
        Self::ReportComplaint,
        Self::ReportLostItem,
        Self::RequestBillingSummary,
        Self::RequestCheckout,
        Self::LanguageHelp,
    ];

    /// Stable snake_case identifier; same string used as MCP tool name and
    /// metrics dimension value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SetRoomTemperature => "set_room_temperature",
            Self::SetLights => "set_lights",
            Self::SetCurtains => "set_curtains",
            Self::SetTv => "set_tv",
            Self::SetAmbientMusic => "set_ambient_music",
            Self::SetDoNotDisturb => "set_do_not_disturb",
            Self::OrderRoomService => "order_room_service",
            Self::RequestHousekeeping => "request_housekeeping",
            Self::RequestExtraTowels => "request_extra_towels",
            Self::RequestExtraPillows => "request_extra_pillows",
            Self::RequestToiletries => "request_toiletries",
            Self::RequestLaundryPickup => "request_laundry_pickup",
            Self::RequestIron => "request_iron",
            Self::SetWakeUpCall => "set_wake_up_call",
            Self::RequestLateCheckout => "request_late_checkout",
            Self::BookRestaurant => "book_restaurant",
            Self::BookSpa => "book_spa",
            Self::BookTaxi => "book_taxi",
            Self::ConciergeInfo => "concierge_info",
            Self::RequestLocalRecommendation => "request_local_recommendation",
            Self::ReportComplaint => "report_complaint",
            Self::ReportLostItem => "report_lost_item",
            Self::RequestBillingSummary => "request_billing_summary",
            Self::RequestCheckout => "request_checkout",
            Self::LanguageHelp => "language_help",
        }
    }

    /// Parse a snake_case identifier (case-insensitive) into a kind.
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        Self::ALL.iter().copied().find(|k| k.as_str() == normalized)
    }
}

/// Inputs for a hotel intent execution.
#[derive(Clone, Debug, PartialEq)]
pub struct HotelQuery {
    /// Canonical intent the classifier picked.
    pub kind: HotelIntentKind,
    /// Free-form slots forwarded to the MCP tool. Should be a JSON object
    /// (other values are rejected by `validate_slots`).
    pub slots: serde_json::Value,
    /// Original user utterance, for logging and `_meta` propagation.
    pub user_text: String,
}

impl HotelQuery {
    /// Constructs a query with an empty slots object and empty user text.
    pub fn new(kind: HotelIntentKind) -> Self {
        Self {
            kind,
            slots: serde_json::Value::Object(serde_json::Map::new()),
            user_text: String::new(),
        }
    }

    /// Builder helper to attach slots.
    pub fn with_slots(mut self, slots: serde_json::Value) -> Self {
        self.slots = slots;
        self
    }

    /// Builder helper to attach user text.
    pub fn with_user_text(mut self, user_text: impl Into<String>) -> Self {
        self.user_text = user_text.into();
        self
    }
}

/// Structured result returned to the backend so the LLM can compose a
/// spoken reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotelResult {
    pub kind: HotelIntentKind,
    /// Short status label (e.g. `"ok"`, `"queued"`, `"confirmed"`).
    pub status: String,
    /// Human-readable single-line summary.
    pub summary: String,
    /// Raw structured details from the MCP tool response.
    pub details: serde_json::Value,
}

impl HotelResult {
    /// Compact, deterministic line for the LLM prompt.
    pub fn to_prompt_context(&self) -> String {
        format!(
            "Hotel concierge intent={} status={}: {}",
            self.kind.as_str(),
            self.status,
            self.summary
        )
    }
}

/// All recoverable errors produced by hotel skill execution.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HotelError {
    #[error("invalid slots for {intent}: {reason}")]
    InvalidSlots { intent: String, reason: String },
    #[error("hotel mcp unavailable: {0}")]
    McpUnavailable(String),
    #[error("hotel mcp rejected the request: {0}")]
    McpRejected(String),
    #[error("hotel mcp call timed out")]
    Timeout,
    #[error("unsupported hotel intent: {0}")]
    UnsupportedIntent(String),
    #[error("hotel error: {0}")]
    Other(String),
}

impl HotelError {
    /// Stable label for the `error_kind` metrics dimension.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidSlots { .. } => "invalid_slots",
            Self::McpUnavailable(_) => "mcp_unavailable",
            Self::McpRejected(_) => "mcp_rejected",
            Self::Timeout => "timeout",
            Self::UnsupportedIntent(_) => "unsupported_intent",
            Self::Other(_) => "other",
        }
    }
}

#[async_trait]
pub trait HotelSkill: Send + Sync {
    async fn execute(&self, query: &HotelQuery) -> Result<HotelResult, HotelError>;
}

/// Transport used by `McpHotelSkill` to call the property's MCP server.
///
/// Implementors are responsible for the wire protocol (JSON-RPC, SSE,
/// stdio, etc.). Errors are mapped to `HotelError` so the skill stays
/// transport-agnostic.
#[async_trait]
pub trait HotelMcpTransport: Send + Sync {
    async fn call_tool(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, HotelError>;
}
