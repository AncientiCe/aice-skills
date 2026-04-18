# skill-journal

Killer skill: voice journaling with timestamps, sentiment, and temporal recall — all stored locally in a single SQLite file owned by the user.

## Actions

- `Add { text, sentiment?, tags }` — append a timestamped entry. If `sentiment` is omitted, a tiny offline lexicon classifier is used. The runtime can also pre-compute sentiment with a local LLM and pass it in.
- `Recall { from?, to?, contains?, limit }` — fetch matching entries (most recent first).
- `Stats { from?, to? }` — sentiment counts over a time window.

## Local-LLM angle

Because everything is stored locally, the runtime can periodically run a local LLM over the journal to surface trends ("you mentioned 'tired' on 6 of the last 10 entries"). Cloud assistants can't do this without uploading your inner life.

## Run tests

```powershell
cargo test -p skill-journal
```
