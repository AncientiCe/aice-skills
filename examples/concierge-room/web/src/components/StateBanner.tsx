interface Props {
  label: string;
  detail: string;
  connected: boolean;
}

export default function StateBanner({ label, detail, connected }: Props) {
  return (
    <div className="flex items-center justify-between rounded-2xl border border-slate-700/60 bg-gradient-to-br from-slate-900 to-[#0c1530] px-5 py-3 shadow-lg">
      <div className="flex items-baseline gap-3">
        <h1 className="text-xl font-semibold tracking-wide">Concierge Room</h1>
        <span className="text-sm text-slate-400">aice hotel skill demo</span>
      </div>
      <div
        className={`flex items-center gap-2 rounded-full border px-4 py-1 text-sm font-semibold ${
          connected
            ? "border-sky-400/40 bg-sky-400/10 text-sky-300"
            : "border-red-400/40 bg-red-400/10 text-red-300"
        }`}
      >
        <span className={`h-2 w-2 rounded-full ${connected ? "bg-sky-400" : "bg-red-400"}`} />
        {label.replaceAll("_", " ")}
        {detail ? <span className="text-xs text-slate-400"> · {detail}</span> : null}
      </div>
    </div>
  );
}
