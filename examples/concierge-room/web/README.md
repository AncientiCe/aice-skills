# concierge-room/web

Browser SPA hosted by the concierge bridge. Two parallel sources live
here:

1. **`dist/index.html`** — a self-contained vanilla JS + CSS bundle that
   the Rust binary embeds via `rust-embed`. This is the runtime.
2. **`src/`** — a React + Vite + Tailwind project that mirrors the
   visualisation. Use this if you want to extend the UI with
   componentised state.

## Why both?

`cargo run -p concierge-room` does not invoke `npm` — Rust devs without
Node should still be able to build and run the demo. The vanilla bundle
in `dist/` makes that possible. When you upgrade the UI, run
`npm install && npm run build` (writes to `dist/`) and rebuild the
binary.

## Develop with Vite

```
cd examples/concierge-room/web
npm install
npm run dev   # starts on http://127.0.0.1:5174 with /viz proxied to the bridge
```

Run the bridge in a separate terminal so the proxy has somewhere to
forward to:

```
cargo run -p concierge-room
```

## Build for the bridge

```
npm run build
cd ../../..
cargo build -p concierge-room
```
