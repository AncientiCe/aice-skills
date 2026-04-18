# skill-template

Reference implementation showing the conventions used by every skill in
this workspace. Copy this directory and rename when starting a new
skill.

## Files

| Path | Purpose |
| --- | --- |
| `Cargo.toml` | Crate manifest. Trim deps you don't need. |
| `src/lib.rs` | Public surface: real impl + `Mock<Name>Skill` + tests + metrics. |
| `src/types.rs` | `Query`, `Result`, `Error` enum, public trait. |

## After copying

1. Rename the directory and crate.
2. Replace every occurrence of `template` / `Template`.
3. Add the crate to the workspace `members`, the `core-skills`
   dependency list, the right feature bucket, and `ENABLED_SKILL_IDS`
   in the top-level `src/lib.rs`. See [`CONTRIBUTING.md`](../../CONTRIBUTING.md).
4. Write a behavioral test, watch it fail, then make it pass.
5. Run `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.

## What this template does *not* show

- HTTP retry / cache: see `skill-currency` for the
  `skill-live-info-common` pattern.
- LLM-backed skills: see `skill-translate` or `skill-meeting-notes` for
  the trait + sanitization shape.
- macOS AppleScript bridges: see `skill-reminder` or `skill-calendar`.

This crate is `publish = false` and intentionally not part of the
workspace `members` so it can't accidentally ship.
