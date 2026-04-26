# concierge-room

End-to-end hotel concierge demo. The crate is a Rust bridge that:

- Captures the local microphone with `cpal` (best-effort; falls back to
  no-mic if the device is unavailable).
- Discovers an `aice-backend` instance via UDP `FIND` / `HERE:<port>`
  exactly like `aice-macos`.
- Opens a `WebSocket` to the backend's `/turns/stream`, advertising
  `skill_hotel` as the only supported frontend skill.
- Routes each `FrontendSkillIntent` it receives through `skill-hotel`'s
  `McpHotelSkill` to a per-property MCP server (the included
  `examples/hotel-mcp/`, or your own).
- Hosts a local browser SPA on `http://127.0.0.1:8740` that visualises
  every stage of the journey with millisecond-precision timing.

## Run the full demo

You'll need three terminals:

1. **Hotel MCP** (executor):

   ```
   cd examples/hotel-mcp
   cargo run
   # listening on 127.0.0.1:8790
   ```

2. **aice-backend** (classifier + LLM + memory): run from the `aice`
   repo with the `hotel` feature enabled. The bridge auto-discovers it
   via UDP.

3. **concierge-room** (this crate):

   ```
   cd examples/concierge-room
   cargo run
   # opens UI at http://127.0.0.1:8740/
   ```

Then open the printed URL on a tablet. State, transcript, classifier
output, MCP call payloads, assistant tokens and a per-turn waterfall
all stream in live.

## Configuration

| env var                          | default                   | meaning                                    |
| -------------------------------- | ------------------------- | ------------------------------------------ |
| `CONCIERGE_UI_BIND`              | `127.0.0.1:8740`          | Address the browser UI binds to.           |
| `CONCIERGE_MCP_URL`              | `http://127.0.0.1:8790/mcp` | Hotel MCP endpoint.                       |
| `CONCIERGE_BACKEND_WS`           | (auto-discover)           | Skip discovery; connect to this WS URL.    |
| `CONCIERGE_DISCOVERY_UDP_PORT`   | `9999`                    | UDP port for `FIND` broadcast.             |
| `CONCIERGE_DISCOVERY_TIMEOUT_MS` | `3000`                    | How long to wait for `HERE:<port>` reply.  |
| `CONCIERGE_DEVICE_ID`            | (random)                  | Stable device id for backend correlation.  |

## Tests

```
cargo test
```

The integration test (`tests/bridge_journey.rs`) spins up a fake
backend WebSocket and a fake MCP HTTP server, drives one turn through
the bridge, and asserts the sequence of `VizEvent`s plus the resulting
`FrontendSkillResult`.

## Why is this an MCP, not a built-in skill?

Each property runs its own MCP server (front-desk system, IoT room
controllers, billing). The hotel skill is identical across properties;
only the MCP differs. Ship the bridge, point it at the property's MCP,
done.
