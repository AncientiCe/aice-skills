import type { TurnView } from "../state";

export default function AssistantReply({ turn }: { turn?: TurnView }) {
  const text = turn?.assistantText ?? "";
  const finished = turn?.finished ?? false;
  return (
    <div className="rounded-2xl border border-slate-700/60 bg-elev p-4 shadow-md">
      <h2 className="mb-2 text-xs uppercase tracking-widest text-slate-400">Assistant reply</h2>
      <div className="min-h-[56px] text-lg leading-relaxed">
        {text ? <span>{text}</span> : <span className="italic text-slate-500">awaiting reply…</span>}
        {text && !finished ? <span className="ml-1 animate-pulse text-slate-400">▍</span> : null}
      </div>
    </div>
  );
}
