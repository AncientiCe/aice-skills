# skill-email

Killer skill: read your email by voice, optionally with **local** LLM triage.

Two providers behind one `EmailSkill` trait:

- `AppleMailEmailSkill` — macOS-only via AppleScript (Mail.app).
- `ImapEmailSkill` — cross-platform; the runtime owns the IMAP client and supplies an `ImapFetcher` so this crate stays IMAP-client-agnostic.

## Actions

- `list_unread` — list unread messages
- `list_inbox` — list recent inbox messages
- `search` (IMAP only for now) — text search
- `triage` — list unread messages and bucket them with a local LLM (`needs reply today`, `meeting / calendar`, `fyi`, `newsletter`, `spam-likely`, ...)

## Run tests

```powershell
cargo test -p skill-email
```
