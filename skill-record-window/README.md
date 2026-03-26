# skill-record-window

Records a specific macOS app window to an `.mp4` file with both:

- system output audio
- microphone input audio

The implementation uses AppleScript (`osascript`) for window bounds and `ffmpeg` (`avfoundation`) for capture.

## Requirements

- macOS (Intel or Apple Silicon)
- `ffmpeg` in `PATH`
- `blackhole-2ch` installed
- Accessibility + Screen Recording permissions granted to the host app/process running this skill

Install dependencies:

```bash
brew install ffmpeg blackhole-2ch
```

## One-time audio setup

Use **Audio MIDI Setup** to create:

1. **Multi-Output Device**:
   - include `BlackHole 2ch`
   - include your normal output (for monitoring)
2. **Aggregate Device**:
   - include `BlackHole 2ch`
   - include your microphone

Then set macOS output to the Multi-Output Device while recording.

Default aggregate input device name expected by this crate: `Aggregate Device`.

## How capture works

1. Reads front window bounds from the target app via AppleScript.
2. Reads display scale factor (Retina-aware) and converts logical points -> physical pixels.
3. Finds `Aggregate Device` index from `ffmpeg -f avfoundation -list_devices true -i ""`.
4. Runs `ffmpeg` with a crop filter matching the target window.

## Output

- Default path: `~/Movies/aice/`
- Default filename format: `record-window-YYYY-MM-DD-HHMMSS.mp4`
- Custom filename is accepted; `.mp4` is appended when missing.

## Common errors

- `AppNotFound`: target app name is wrong or app is not running.
- `WindowBoundsUnavailable`: app has no visible front window.
- `AudioDeviceNotFound`: aggregate device is not created or named differently.
- `FfmpegNotFound`: `ffmpeg` is not installed or not on `PATH`.

## Test and verify

```bash
cargo fmt
cargo clippy --all-targets --all-features -D warnings
cargo test -p skill-record-window
```
