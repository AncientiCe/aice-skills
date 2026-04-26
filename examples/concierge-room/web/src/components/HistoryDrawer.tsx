import type { TurnView } from "../state";
import { fmtMs } from "../lib/format";

export default function HistoryDrawer({ turns }: { turns: TurnView[] }) {
  const history = turns.slice(1);
  if (!history.length) {
    return null;
  }
  return (
    <div className="scrollbar-thin overflow-auto rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">History</h2>
      <ul className="flex flex-col gap-2">
        {history.map((turn) => (
          <li key={turn.id} className="rounded-md border border-slate-700/60 bg-[#0f1830] px-3 py-2">
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-sm font-semibold">
                {turn.classifier?.intent ?? turn.transcript.stable ?? turn.id.slice(0, 8)}
              </span>
              <span className="font-mono text-[11px] text-slate-500">{turn.id.slice(0, 8)}…</span>
            </div>
            <div className="text-xs text-slate-400">{turn.skillResult?.user_text ?? "(no result)"}</div>
            <div className="text-[11px] text-slate-500">
              total {fmtMs(Math.max(0, turn.lastTms - turn.firstTms))}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
