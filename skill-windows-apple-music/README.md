# skill-windows-apple-music

Windows frontend Apple Music skill that vendors embedded Apple Music web player internals.

## Prerequisites

- Windows frontend build (`core-skills` feature `windows`)
- Apple Music session configured through the embedded login flow

## Behavior

- Supports: `play`, `pause`, `resume`, `stop`, `next`, `previous`, `shuffle_on`, `shuffle_off`, `status`
- `play` searches Apple Music and plays the best first match in the embedded player
- Returns an explicit unsupported error when not running on Windows

## Run/Test

```powershell
cargo test -p skill-windows-apple-music
```

## Gotchas

- Playback control requires a valid Apple Music authenticated session.
- `core-windows` stays unchanged; this skill is enabled by frontend `windows` feature only.
