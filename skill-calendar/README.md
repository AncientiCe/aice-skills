# skill-calendar

Calendar skill with two providers behind one `CalendarSkill` trait:

- `AppleCalendarSkill` — macOS-only via AppleScript (`Calendar.app`).
- `GoogleCalendarSkill` — cross-platform via Google Calendar Web API + OAuth.

## Actions

- `list_today`, `list_tomorrow` — events for today / tomorrow
- `list_upcoming` (with `days`) — events in the next N days
- `create` — schedule a new event (`title`, `start`, optional `end`, optional `location`, optional `calendar_name`)

## Architecture

- `CalendarQuery` / `CalendarResult` / `CalendarEvent` / `CalendarError` are the shared types.
- `GoogleCalendarAuth` is implemented by the runtime (it owns the OAuth flow) and yields fresh access tokens. `StaticGoogleCalendarAuth` exists for tests.
- `GoogleCalendarTransport` abstracts the HTTP layer for testability. `HttpGoogleCalendarTransport` is the live `reqwest`-backed implementation.

## Run tests

```powershell
cargo test -p skill-calendar
```
