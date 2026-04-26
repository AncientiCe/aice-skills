import type { TurnView } from "../state";

export default function TranscriptPane({ turn }: { turn?: TurnView }) {
  const stable = turn?.transcript.stable ?? "";
  const partial = turn?.transcript.partial ?? "";
  return (
    <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Transcript</h2>
      <div className="min-h-[56px] text-2xl leading-snug">
        {stable ? <span>{stable}</span> : null}
        {partial ? (
          <span className="italic text-slate-400">{stable ? " " : ""}{partial}</span>
        ) : null}
        {!stable && !partial ? <span className="italic text-slate-500">waiting for guest…</span> : null}
      </div>
    </div>
  );
}
