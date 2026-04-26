//! Local copy of the `core-runtime-protocol` wire types so this example
//! crate is buildable without depending on the aice repo.
//!
//! Schema must stay in sync with
//! [`D:/Dev/Projects/aice/crates/core-runtime-protocol/src/lib.rs`].

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FrontendSkillIntent {
    pub turn_id: String,
    pub intent: String,
    pub slots: serde_json::Value,
    #[serde(default)]
    pub user_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FrontendSkillResultRequest {
    pub status: String,
    pub user_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_result_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnStreamClientMessage {
    TurnStart {
        session_id: String,
        #[serde(default)]
        device_id: Option<String>,
        turn_id: String,
        #[serde(default)]
        supported_frontend_intents: Vec<String>,
        #[serde(default)]
        schema_version: Option<u32>,
    },
    TurnDone,
    TurnCancel,
    FrontendSkillResult {
        turn_id: String,
        intent_id: String,
        result: FrontendSkillResultRequest,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnStreamServerEvent {
    PartialTranscript {
        turn_id: String,
        text: String,
        stable: bool,
    },
    IntentUpdate {
        turn_id: String,
        intent: String,
    },
    Token {
        turn_id: String,
        text: String,
    },
    FrontendSkillIntent(FrontendSkillIntent),
    Done {
        turn_id: String,
    },
    Error {
        #[serde(default)]
        turn_id: Option<String>,
        message: String,
    },
}
