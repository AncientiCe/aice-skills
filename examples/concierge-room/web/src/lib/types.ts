export type VizEvent =
  | StateEvent
  | TurnStartedEvent
  | TranscriptPartialEvent
  | ClassifierStartedEvent
  | ClassifierIntentEvent
  | ClassifierDoneEvent
  | McpCallStartedEvent
  | McpCallDoneEvent
  | McpCallErrorEvent
  | SkillResultSentEvent
  | AssistantTokenEvent
  | TurnDoneEvent
  | BackendErrorEvent;

export interface BaseEvent { stage: string; t_ms: number; turn_id?: string }

export interface StateEvent extends BaseEvent { stage: "state"; label: string; detail?: string }
export interface TurnStartedEvent extends BaseEvent { stage: "turn_started"; turn_id: string; session_id: string }
export interface TranscriptPartialEvent extends BaseEvent { stage: "transcript_partial"; turn_id: string; text: string; stable: boolean }
export interface ClassifierStartedEvent extends BaseEvent { stage: "classifier_started"; turn_id: string }
export interface ClassifierIntentEvent extends BaseEvent { stage: "classifier_intent"; turn_id: string; intent: string }
export interface ClassifierDoneEvent extends BaseEvent { stage: "classifier_done"; turn_id: string; intent: string; slots: unknown; user_text: string; latency_ms: number }
export interface McpCallStartedEvent extends BaseEvent { stage: "mcp_call_started"; turn_id: string; tool: string; args: unknown }
export interface McpCallDoneEvent extends BaseEvent { stage: "mcp_call_done"; turn_id: string; tool: string; latency_ms: number; response: unknown }
export interface McpCallErrorEvent extends BaseEvent { stage: "mcp_call_error"; turn_id: string; tool: string; error: string; latency_ms: number }
export interface SkillResultSentEvent extends BaseEvent { stage: "skill_result_sent"; turn_id: string; status: string; user_text: string }
export interface AssistantTokenEvent extends BaseEvent { stage: "assistant_token"; turn_id: string; token: string }
export interface TurnDoneEvent extends BaseEvent { stage: "turn_done"; turn_id: string; total_ms: number }
export interface BackendErrorEvent extends BaseEvent { stage: "backend_error"; message: string }
