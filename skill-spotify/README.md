# skill-spotify

Cross-platform Spotify media skill (Web API + OAuth) implementing the shared `MediaSkill` trait from `skill-media`. Works on macOS, Windows, and Linux because it talks to the Spotify Web API directly rather than to a desktop app.

## Architecture

- `SpotifyAuth` trait — runtime owns the OAuth dance and refresh-token rotation. It returns a fresh access token on demand. `StaticSpotifyAuth` is provided for tests/dev.
- `SpotifyTransport` trait — abstracts the HTTP layer so the skill can be unit-tested without network. `HttpSpotifyTransport` is the live implementation backed by `reqwest`.
- `SpotifySkill` — implements `skill_media::MediaSkill`. Supports `play`, `pause`, `resume`, `stop`, `next`, `previous`, `shuffle_on`, `shuffle_off`, `status`.

## Routing

`SpotifySkill` shares the `MediaSkill` trait with `MacOsMusicSkill`. The runtime decides per-utterance which provider to invoke:
- If the user names a service ("play X on spotify" / "play X on apple music"), route to that provider.
- Otherwise, fall back to a configured `media.preferred_provider`.

## Run tests

```powershell
cargo test -p skill-spotify
```
