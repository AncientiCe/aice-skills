import type { TurnView } from "../state";
import { fmtMs } from "../lib/format";

const STAGE_LABELS: Record<string, string> = {
  turn_started: "Turn started",
  wake_word_detected: "Wake word",
  transcript_partial: "Transcribing",
  classifier_started: "Classifying",
  classifier_done: "Classified",
  mcp_call_started: "Calling MCP",
  mcp_call_done: "MCP done",
  mcp_call_error: "MCP error",
  skill_result_sent: "Skill result",
  assistant_token: "Assistant",
  turn_done: "Turn done",
};
const STAGE_ORDER = Object.keys(STAGE_LABELS);

export default function JourneyTimeline({ turns }: { turns: TurnView[] }) {
  const latest = turns[0];
  if (!latest) {
    return (
      <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
        <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Journey timeline</h2>
        <div className="text-center italic text-slate-500">No turns yet. Speak to the room.</div>
      </div>
    );
  }
  return (
    <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Journey timeline</h2>
      <TurnRow turn={latest} />
    </div>
  );
}

function TurnRow({ turn }: { turn: TurnView }) {
  const totalMs = Math.max(1, turn.lastTms - turn.firstTms);
  const stages = STAGE_ORDER.filter((stage) => turn.stages[stage]);
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between gap-2">
        <span className="text-sm font-semibold">
          {turn.classifier?.intent ? `${turn.classifier.intent} · ${turn.classifier.user_text}` : turn.transcript.stable || turn.transcript.partial || "incoming turn"}
        </span>
        <span className="font-mono text-[11px] text-slate-500">{turn.id.slice(0, 8)}…</span>
      </div>
      <div className="flex flex-col gap-1">
        {stages.map((stage) => {
          const ev = turn.stages[stage]!;
          const offset = ev.t_ms - turn.firstTms;
          const pct = Math.max(0.05, Math.min(1, offset / totalMs));
          return (
            <div key={stage} className="grid grid-cols-[130px_1fr_70px] items-center gap-2">
              <span className="text-[11px] uppercase tracking-wide text-slate-400">
                {STAGE_LABELS[stage]}
              </span>
              <div className="relative h-2 overflow-hidden rounded-full bg-sky-400/10">
                <div
                  className="absolute inset-y-0 left-0 rounded-full bg-gradient-to-r from-sky-400 to-sky-300 shadow-[0_0_14px_rgba(56,189,248,0.5)]"
                  style={{ width: `${(pct * 100).toFixed(1)}%` }}
                />
              </div>
              <span className="text-right text-[11px] tabular-nums text-slate-400">
                {fmtMs(offset)}
              </span>
            </div>
          );
        })}
      </div>
      {turn.finished ? (
        <div className="mt-2 text-[11px] text-slate-500">
          <span className="font-semibold text-slate-400">total: </span>
          {fmtMs(totalMs)}
        </div>
      ) : null}
    </div>
  );
}
