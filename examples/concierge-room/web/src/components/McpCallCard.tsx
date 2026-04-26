import type { TurnView } from "../state";
import { fmtMs } from "../lib/format";

export default function McpCallCard({ turn }: { turn?: TurnView }) {
  return (
    <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Hotel MCP call</h2>
      {turn?.mcpError ? (
        <div>
          <div className="flex items-center justify-between gap-3">
            <span className="rounded-md bg-red-400/15 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide text-red-300">
              {turn.mcpError.tool}
            </span>
            <span className="text-xs tabular-nums text-slate-400">{fmtMs(turn.mcpError.latency_ms)}</span>
          </div>
          <div className="mt-1 text-xs text-red-300">error: {turn.mcpError.error}</div>
        </div>
      ) : turn?.mcpResponse ? (
        <div>
          <div className="flex items-center justify-between gap-3">
            <span className="rounded-md bg-sky-400/15 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide text-sky-300">
              {turn.mcpResponse.tool}
            </span>
            <span className="text-xs tabular-nums text-slate-400">{fmtMs(turn.mcpResponse.latency_ms)} tool</span>
          </div>
          {turn.mcpCall ? (
            <pre className="mt-2 max-h-48 overflow-auto rounded-md border border-slate-700/60 bg-[#1a2440] p-2 text-xs">
{JSON.stringify(turn.mcpCall.args, null, 2)}
            </pre>
          ) : null}
          <div className="mt-2 text-xs text-slate-400">
            <span className="font-semibold text-slate-300">response</span>
          </div>
          <pre className="mt-1 max-h-48 overflow-auto rounded-md border border-slate-700/60 bg-[#1a2440] p-2 text-xs">
{JSON.stringify(turn.mcpResponse.response, null, 2)}
          </pre>
        </div>
      ) : turn?.mcpCall ? (
        <div>
          <div className="flex items-center justify-between gap-3">
            <span className="rounded-md bg-sky-400/15 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide text-sky-300">
              {turn.mcpCall.tool}
            </span>
            <span className="text-xs tabular-nums text-slate-400">in flight…</span>
          </div>
          <pre className="mt-2 max-h-48 overflow-auto rounded-md border border-slate-700/60 bg-[#1a2440] p-2 text-xs">
{JSON.stringify(turn.mcpCall.args, null, 2)}
          </pre>
        </div>
      ) : (
        <div className="text-xs text-slate-500">no MCP call yet for this turn</div>
      )}
    </div>
  );
}
