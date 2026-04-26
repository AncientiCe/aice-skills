import type { VizEvent } from "./lib/types";

export interface TurnView {
  id: string;
  firstTms: number;
  lastTms: number;
  events: VizEvent[];
  stages: Partial<Record<string, VizEvent>>;
  transcript: { partial: string; stable: string };
  classifier?: { intent: string; slots: unknown; user_text: string; latency_ms: number };
  mcpCall?: { tool: string; args: unknown };
  mcpResponse?: { tool: string; response: unknown; latency_ms: number };
  mcpError?: { tool: string; error: string; latency_ms: number };
  skillResult?: { status: string; user_text: string };
  assistantText: string;
  finished: boolean;
}

export interface AppState {
  bridgeState: string;
  bridgeDetail: string;
  turns: Record<string, TurnView>;
  turnOrder: string[];
  activeTurnId: string | null;
  wsConnected: boolean;
}

export const initialState: AppState = {
  bridgeState: "connecting",
  bridgeDetail: "",
  turns: {},
  turnOrder: [],
  activeTurnId: null,
  wsConnected: false,
};

export type Action =
  | { type: "viz"; event: VizEvent }
  | { type: "status"; connected: boolean };

export function reducer(state: AppState, action: Action): AppState {
  if (action.type === "status") {
    return { ...state, wsConnected: action.connected };
  }
  const ev = action.event;
  if (ev.stage === "state") {
    return { ...state, bridgeState: ev.label, bridgeDetail: ev.detail ?? "" };
  }
  if (ev.stage === "backend_error" && !ev.turn_id) {
    return { ...state, bridgeState: "backend_error", bridgeDetail: ev.message };
  }
  const turnId = ev.turn_id;
  if (!turnId) return state;

  const previous = state.turns[turnId];
  const turn: TurnView = previous
    ? { ...previous, events: previous.events.concat(ev), stages: { ...previous.stages, [ev.stage]: ev } }
    : {
        id: turnId,
        firstTms: ev.t_ms,
        lastTms: ev.t_ms,
        events: [ev],
        stages: { [ev.stage]: ev },
        transcript: { partial: "", stable: "" },
        assistantText: "",
        finished: false,
      };
  turn.firstTms = previous ? previous.firstTms : ev.t_ms;
  turn.lastTms = ev.t_ms;

  switch (ev.stage) {
    case "transcript_partial":
      turn.transcript = ev.stable
        ? { ...turn.transcript, stable: ev.text }
        : { ...turn.transcript, partial: ev.text };
      break;
    case "classifier_done":
      turn.classifier = {
        intent: ev.intent,
        slots: ev.slots,
        user_text: ev.user_text,
        latency_ms: ev.latency_ms,
      };
      break;
    case "mcp_call_started":
      turn.mcpCall = { tool: ev.tool, args: ev.args };
      break;
    case "mcp_call_done":
      turn.mcpResponse = { tool: ev.tool, response: ev.response, latency_ms: ev.latency_ms };
      break;
    case "mcp_call_error":
      turn.mcpError = { tool: ev.tool, error: ev.error, latency_ms: ev.latency_ms };
      break;
    case "assistant_token":
      turn.assistantText = (turn.assistantText || "") + ev.token;
      break;
    case "skill_result_sent":
      turn.skillResult = { status: ev.status, user_text: ev.user_text };
      break;
    case "turn_done":
      turn.finished = true;
      break;
  }

  const turnOrder = previous
    ? state.turnOrder
    : [turnId, ...state.turnOrder].slice(0, 30);
  return {
    ...state,
    turns: { ...state.turns, [turnId]: turn },
    turnOrder,
    activeTurnId: turnId,
  };
}
