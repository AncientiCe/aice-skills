import type { TurnView } from "../state";
import { fmtMs } from "../lib/format";

export default function ClassifierCard({ turn }: { turn?: TurnView }) {
  return (
    <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Classifier</h2>
      {turn?.classifier ? (
        <div>
          <div className="flex items-center justify-between gap-3">
            <span className="rounded-md bg-sky-400/15 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide text-sky-300">
              {turn.classifier.intent || "unknown"}
            </span>
            <span className="text-xs tabular-nums text-slate-400">
              {fmtMs(turn.classifier.latency_ms)} classifier
            </span>
          </div>
          {turn.classifier.user_text ? (
            <div className="mt-1 text-xs text-slate-400">
              <span className="font-semibold text-slate-300">utterance: </span>
              {turn.classifier.user_text}
            </div>
          ) : null}
          <pre className="mt-2 max-h-48 overflow-auto rounded-md border border-slate-700/60 bg-[#1a2440] p-2 text-xs">
{JSON.stringify(turn.classifier.slots, null, 2)}
          </pre>
        </div>
      ) : (
        <div className="text-xs text-slate-500">no intent classified yet</div>
      )}
    </div>
  );
}
