# aice-skills

A library of **voice-assistant skills designed for local LLMs**. Skills are
small, focused Rust crates with clear traits, mock implementations, and
strict quality gates. They power the `aice` voice-assistant project across
macOS, Windows, Linux, and iOS — and they run happily on your own machine
against a locally hosted Ollama model.

> **Why local-first?** Cloud assistants cost money, leak data, and stop
> working when your network does. A 7B model on your own GPU plus the right
> set of skills can already do the boring useful stuff — and the *interesting*
> stuff (your own files, your own habits, your own audio) that the big
> vendors won't touch.

---

## Highlights

- **Zero-cloud baseline.** Every CORE skill works offline or against
  free, key-less APIs (Open-Meteo, Frankfurter, dictionaryapi.dev, …).
- **Cross-platform CORE.** Calculator, units, currency, weather, air
  quality, dictionary, translation, calendar, email, journal — all the
  daily-use stuff runs on Mac, Windows, and Linux.
- **macOS power user.** Native AppleScript bridges for Reminders,
  Calendar, Mail, Messages, Music, smart home, and screen capture.
- **Killer local-LLM skills:**
  - `skill-meeting-notes` — record + transcribe + extract action items into
    Reminders.
  - `skill-screen-ocr` — screenshot + OCR + ask questions about it.
  - `skill-journal` — voice journaling with sentiment + temporal recall.
  - `skill-briefing` — one-command morning briefing composed from
    weather + calendar + email + news.
  - `skill-translate` — fully local translation against an Ollama model.
- **Routine templates.** Pre-built `morning_briefing`, `wind_down`,
  `focus`, and `commute` chains via `skill-chain::routine_catalog()`.

---

## Skill catalog

### CORE (cross-platform)

| Skill | Provider | Network | Purpose |
| --- | --- | --- | --- |
| `skill-time` | Open-Meteo | yes (lazy) | Local time, sunrise/sunset |
| `skill-weather` | Open-Meteo | yes | Current conditions, multi-day forecast, severe-weather alerts |
| `skill-air-quality` | Open-Meteo | yes | AQI + pollutant breakdown |
| `skill-distance` | Open-Meteo | yes | Distance + travel time |
| `skill-currency` | Frankfurter | yes | FX conversion |
| `skill-dictionary` | dictionaryapi.dev | yes | Word definitions |
| `skill-translate` | local Ollama | no | Local-LLM translation |
| `skill-calculator` | `meval` | no | Offline expression eval |
| `skill-unit-conversion` | local tables | no | Length, mass, volume, temperature, … |
| `skill-news-headlines` | NewsData.io | yes | Topic headlines + opt-in streaming summaries |
| `skill-sports-live` | TheSportsDB | yes | Live scores |
| `skill-holiday-lookup` | Nager.Date | yes | Public holidays |
| `skill-fuel-price-lookup` | open data | yes | Pump prices by region |
| `skill-horoscope-daily` | aztro | yes | Daily horoscope (because why not) |
| `skill-spotify` | Spotify Web API | yes | Cross-platform music control |
| `skill-calendar` | Google Calendar / Apple Calendar | yes / no | List + create events |
| `skill-email` | IMAP / Apple Mail | yes / no | Inbox + LLM triage |
| `skill-meeting-notes` | local Whisper + Ollama | no | Transcript → summary + reminders |
| `skill-briefing` | composed | mixed | One-shot daily briefing |
| `skill-journal` | local SQLite | no | Voice journaling + sentiment + recall |
| `skill-screen-ocr` | local OCR + Ollama | no | "What's on my screen?" |

### macOS

| Skill | Purpose |
| --- | --- |
| `skill-reminder` | Reminders.app via AppleScript |
| `skill-shopping-list` | Notes.app shopping list |
| `skill-message` | iMessage |
| `skill-screenshot` | Native screen capture |
| `skill-record-window` | Per-window recording |
| `skill-volume` | System volume |
| `skill-timer` | Native countdown timers + in-process registry |
| `skill-app-switcher` | App focus / launch |
| `skill-computer` | Power / display control |
| `skill-smart-home` | Hue bridge integration |
| `skill-media` | Apple Music control |

### Windows

| Skill | Purpose |
| --- | --- |
| `skill-windows-apple-music` | Apple Music for Windows |

### Hospitality / hotel concierge

| Skill | Purpose |
| --- | --- |
| `skill-hotel` | In-room concierge (climate, lights, room service, wake-up calls, transport, complaints, late checkout, …). Dispatches to a per-property Hotel MCP transport so each chain plugs in their own backend. |

See `examples/concierge-room/` for a tablet-style demo frontend that
discovers `aice-backend` over UDP, performs intent classification, and
forwards every classified hotel intent to the `examples/hotel-mcp/` JSON-RPC
HTTP+SSE server. Both example crates are standalone; activate the skill in
the backend and the desktop with the `hotel` feature flag.

---

## Feature buckets

Skill inclusion is compile-time only via Cargo features.

- `core-common` — every cross-platform skill above.
- `core-macos` / `core-windows` — platform-specific core slots (currently empty).
- `macos` — macOS-only frontend skills.
- `windows` — Windows-only frontend skills.
- `ios` — iOS-only frontend skills.
- `hotel` — `skill-hotel` (mutually exclusive with `macos` / `windows` /
  `ios`; intended for hospitality frontends like
  `examples/concierge-room/`).

Default features: `[core-common]`.

### Wiring

Backend (macOS):

```toml
core-skills = { version = "*", default-features = false, features = ["core-common", "core-macos"] }
```

Frontend (macOS desktop):

```toml
core-skills = { version = "*", default-features = false, features = ["core-common", "core-macos", "macos"] }
```

Frontend (Windows desktop):

```toml
core-skills = { version = "*", default-features = false, features = ["core-common", "core-windows", "windows"] }
```

Frontend (iOS):

```toml
core-skills = { version = "*", default-features = false, features = ["core-common", "ios"] }
```

Hospitality frontend (concierge-room demo, hotel chains):

```toml
core-skills = { version = "*", default-features = false, features = ["core-common", "hotel"] }
```

`ENABLED_SKILL_IDS` reflects whatever the active feature set produced.

---

## Routine templates

Ready-to-run `ChainPlan`s ship in `skill-chain::routines`:

- `morning_briefing()` — time + weather + calendar + headlines.
- `wind_down()` — lower volume, dim lights, journal prompt.
- `focus()` — silence notifications, start a 25-minute timer.
- `commute()` — weather, distance to work, headlines.

```rust
use skill_chain::{routine_for, RoutineId};

let plan = routine_for(RoutineId::MorningBriefing).plan;
executor.run(plan).await?;
```

---

## Repository layout

```
aice-skills/
├── Cargo.toml              # workspace + feature-gated re-exports
├── src/lib.rs              # ENABLED_SKILL_IDS + pub use
├── examples/
│   ├── skill-template/     # copy this when starting a new skill
│   ├── hotel-mcp/          # standalone JSON-RPC HTTP+SSE MCP server (demo executor for skill-hotel)
│   └── concierge-room/     # standalone tablet-style hotel-room demo frontend (Rust bridge + browser UI)
├── skill-chain/            # routine planner + executor + templates
├── skill-live-info-common/ # shared cache + retry helpers
├── skill-<name>/           # individual skill crates
└── ...
```

---

## Building blocks

- **Common HTTP retry/cache:** `skill-live-info-common` exposes
  `TtlCache` + `with_retries` reused by every network-backed skill.
- **Metrics:** every skill records `*_execute_total` /
  `*_execute_duration_seconds` via the `metrics` crate.
- **Mocks everywhere:** every skill ships a `Mock<Name>Skill` for tests
  and orchestrator wiring.

---

## Status

Pre-1.0. APIs may change. Cross-repository wiring (intent classifier,
runtime dispatch, frontend UI) lives in sibling repos and lags by a
release.

## License

See `LICENSE`.

## Contributing

Read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and copy
[`examples/skill-template/`](./examples/skill-template/) when adding a
new skill.
