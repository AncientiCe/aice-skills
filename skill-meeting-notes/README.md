# skill-meeting-notes

Killer skill: meeting notes from a transcript.

1. Caller (runtime) provides a transcript (typically produced by `core-stt` Whisper from recorded audio).
2. The skill asks the **local** Ollama LLM to extract a structured summary, key points, decisions, and action items.
3. Optionally pushes each action item into Reminders via `skill-reminder`.

## Architecture

- `MeetingNotesLlm` — adapter trait the runtime implements over its concrete `LlmStream`. Returns the full JSON completion.
- `LlmMeetingNotesSkill::with_reminders(...)` — opt-in reminder fan-out.

## Run tests

```powershell
cargo test -p skill-meeting-notes
```
