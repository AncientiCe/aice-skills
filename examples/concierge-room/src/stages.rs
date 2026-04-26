//! Visualization events emitted to the browser UI.

use serde::Serialize;
use std::time::Instant;

/// Monotonic clock anchor; converts [`Instant`] deltas to milliseconds so
/// the browser can render a waterfall.
#[derive(Clone, Copy, Debug)]
pub struct MonoClock {
    started: Instant,
}

impl MonoClock {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

impl Default for MonoClock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum VizEvent {
    /// Bridge state announcement (e.g. "Listening" / "Connected").
    State {
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        t_ms: u64,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    WakeWordDetected {
        turn_id: String,
        t_ms: u64,
    },
    TurnStarted {
        turn_id: String,
        t_ms: u64,
        session_id: String,
    },
    TranscriptPartial {
        turn_id: String,
        t_ms: u64,
        text: String,
        stable: bool,
    },
    ClassifierStarted {
        turn_id: String,
        t_ms: u64,
    },
    ClassifierIntent {
        turn_id: String,
        t_ms: u64,
        intent: String,
    },
    ClassifierDone {
        turn_id: String,
        t_ms: u64,
        intent: String,
        slots: serde_json::Value,
        user_text: String,
        latency_ms: u64,
    },
    McpCallStarted {
        turn_id: String,
        t_ms: u64,
        tool: String,
        args: serde_json::Value,
    },
    McpCallDone {
        turn_id: String,
        t_ms: u64,
        tool: String,
        latency_ms: u64,
        response: serde_json::Value,
    },
    McpCallError {
        turn_id: String,
        t_ms: u64,
        tool: String,
        error: String,
        latency_ms: u64,
    },
    SkillResultSent {
        turn_id: String,
        t_ms: u64,
        status: String,
        user_text: String,
    },
    AssistantToken {
        turn_id: String,
        t_ms: u64,
        token: String,
    },
    TurnDone {
        turn_id: String,
        t_ms: u64,
        total_ms: u64,
    },
    BackendError {
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        t_ms: u64,
        message: String,
    },
}

impl VizEvent {
    pub fn turn_id(&self) -> Option<&str> {
        match self {
            Self::State { turn_id, .. } | Self::BackendError { turn_id, .. } => turn_id.as_deref(),
            Self::WakeWordDetected { turn_id, .. }
            | Self::TurnStarted { turn_id, .. }
            | Self::TranscriptPartial { turn_id, .. }
            | Self::ClassifierStarted { turn_id, .. }
            | Self::ClassifierIntent { turn_id, .. }
            | Self::ClassifierDone { turn_id, .. }
            | Self::McpCallStarted { turn_id, .. }
            | Self::McpCallDone { turn_id, .. }
            | Self::McpCallError { turn_id, .. }
            | Self::SkillResultSent { turn_id, .. }
            | Self::AssistantToken { turn_id, .. }
            | Self::TurnDone { turn_id, .. } => Some(turn_id),
        }
    }

    pub fn stage(&self) -> &'static str {
        match self {
            Self::State { .. } => "state",
            Self::WakeWordDetected { .. } => "wake_word_detected",
            Self::TurnStarted { .. } => "turn_started",
            Self::TranscriptPartial { .. } => "transcript_partial",
            Self::ClassifierStarted { .. } => "classifier_started",
            Self::ClassifierIntent { .. } => "classifier_intent",
            Self::ClassifierDone { .. } => "classifier_done",
            Self::McpCallStarted { .. } => "mcp_call_started",
            Self::McpCallDone { .. } => "mcp_call_done",
            Self::McpCallError { .. } => "mcp_call_error",
            Self::SkillResultSent { .. } => "skill_result_sent",
            Self::AssistantToken { .. } => "assistant_token",
            Self::TurnDone { .. } => "turn_done",
            Self::BackendError { .. } => "backend_error",
        }
    }
}
