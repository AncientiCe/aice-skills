# skill-hotel

Pluggable concierge skill for hotel deployments. The classifier decides
*which* concierge intent the guest is asking for; this skill dispatches
that intent to a per-property MCP server (the property's "executor")
through a transport-agnostic [`HotelMcpTransport`] trait, then returns a
structured `HotelResult` so the backend's LLM can compose the spoken
reply.

## Wiring shape

```
classifier --> hotel_intent_kind --> HotelQuery --> McpHotelSkill --> HotelMcpTransport --> Hotel MCP
                                                            |
                                                            +--> HotelResult --> backend LLM
```

`HotelMcpTransport` is the only piece each property has to implement; it
hides the wire protocol (HTTP+SSE, stdio JSON-RPC, etc.) from the skill.

## Canonical intents (25)

`set_room_temperature`, `set_lights`, `set_curtains`, `set_tv`,
`set_ambient_music`, `set_do_not_disturb`, `order_room_service`,
`request_housekeeping`, `request_extra_towels`,
`request_extra_pillows`, `request_toiletries`,
`request_laundry_pickup`, `request_iron`, `set_wake_up_call`,
`request_late_checkout`, `book_restaurant`, `book_spa`, `book_taxi`,
`concierge_info`, `request_local_recommendation`, `report_complaint`,
`report_lost_item`, `request_billing_summary`, `request_checkout`,
`language_help`.

Each name is both the `HotelIntentKind` discriminator (snake_case via
serde) and the MCP tool name. Required slots are enforced in
`validate_slots`; everything else is forwarded verbatim to the MCP.

## Implementations

- `McpHotelSkill` — production. Wraps any `Arc<dyn HotelMcpTransport>`.
- `LocalHotelSkill` — offline canned responses; useful for
  development without a running MCP.
- `MockHotelSkill` — fixed result/err for orchestrator unit tests.

## Tests

```
cargo test -p skill-hotel
```

Tests cover:
- Tool-name dispatch for every canonical intent.
- Slot validation (`HotelError::InvalidSlots` for missing required
  fields, non-object payloads, empty `items` arrays).
- Transport error propagation (`McpUnavailable` / `Timeout` /
  `McpRejected`).
- MCP `isError: true` responses mapping to `McpRejected`.
- `LocalHotelSkill` returning canned results for all 25 intents.

## Metrics

- `backend_skill_execute_total{skill="hotel", intent, result, error_kind}`
- `backend_skill_execute_duration_seconds{skill="hotel", intent}`
