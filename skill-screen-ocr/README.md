# skill-screen-ocr

Killer skill: capture a screenshot, OCR it locally, and (optionally) ask the **local** LLM a question about what's on screen.

## Architecture

`ComposedScreenOcrSkill` plugs together three traits the runtime owns:

- `ScreenshotSkill` (from `skill-screenshot`) — captures the screenshot.
- `OcrEngine` — runs OCR over the captured image. Real implementations (Tesseract, Apple Vision, ...) live outside this crate so the runtime can pick what suits the platform without forcing a heavy native dependency on every build.
- `ScreenOcrLlm` — answers questions about the OCR text using the runtime's local LLM transport.

If no question is provided, the OCR text is returned directly. If a question is provided but no LLM was wired in, the OCR text is returned and the answer is left empty.

## Run tests

```powershell
cargo test -p skill-screen-ocr
```
