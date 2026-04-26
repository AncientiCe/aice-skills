# hotel-mcp

Reference Hotel MCP server used by the `concierge-room` demo. It speaks
MCP over HTTP/SSE and exposes one tool per `HotelIntentKind` (25 tools
in total). All state is in-memory; the server is intentionally
self-contained so reviewers can run the entire concierge demo without
any external services.

## Endpoints

- `POST /mcp` — JSON-RPC 2.0 requests (`initialize`, `tools/list`,
  `tools/call`, `ping`). Responses are returned as `application/json`.
- `GET  /mcp` — SSE stream for server-initiated events (heartbeat
  only in this mock).
- `DELETE /mcp` — explicit session termination.
- `GET  /health` — plain `ok` for liveness checks.

## Run

```
cargo run -p hotel-mcp
# or with a custom bind:
HOTEL_MCP_BIND=127.0.0.1:8888 cargo run -p hotel-mcp
```

This crate is excluded from the workspace; run it from the
`examples/hotel-mcp/` directory.

## Test

```
cargo test
```

## Curl examples

Initialize:

```
curl -s http://127.0.0.1:8790/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize"}'
```

List tools:

```
curl -s http://127.0.0.1:8790/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

Call a tool:

```
curl -s http://127.0.0.1:8790/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"set_room_temperature","arguments":{"celsius":22}}}'
```

## Tool catalog

All 25 hotel concierge intents are implemented; see
[`src/tools.rs`](src/tools.rs) for handlers and
[`src/state.rs`](src/state.rs) for the persistent in-memory state.
