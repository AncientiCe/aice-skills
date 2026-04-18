# Contributing to aice-skills

Thanks for taking the time! This repo is a curated set of small Rust
crates that the `aice` voice assistant uses across macOS, Windows,
Linux, and iOS. Before opening a PR, please skim this whole document.

The non-negotiable rules live in [`AGENTS.md`](./AGENTS.md). They apply
to humans too.

## TL;DR

1. Open an issue first if you're proposing a new skill or a public-API
   change. Drive-by PRs that add network dependencies or break feature
   gating are unlikely to land.
2. **Test first.** Behavior change without a failing test is a no-go.
3. Run all three quality gates locally:

   ```bash
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   ```
4. Touch as little as possible. Small, focused PRs review faster.

## What makes a good skill

We optimize for *daily-use, local-first* utility. Before adding a skill
ask:

- **Will an average user run this every week?** If not, it probably
  belongs in a personal fork.
- **Does it work without paid cloud services?** Free / key-less APIs
  preferred. Paid integrations are accepted only when the user can
  bring their own credential.
- **Does it leverage the local LLM?** Skills that *only* call cloud
  APIs are fine but not exciting; skills that combine local data
  (files, audio, screen) with the local model are the bar we want to
  raise.
- **Cross-platform by default.** New skills go in `core-common` unless
  there's a hard OS constraint (AppleScript, Win32, …).

## Project layout

```
aice-skills/
├── Cargo.toml              # workspace + feature-gated re-exports
├── src/lib.rs              # ENABLED_SKILL_IDS + pub use
├── examples/skill-template # copy this to start a new skill
├── skill-chain/            # routine planner + executor + templates
├── skill-live-info-common/ # shared cache + retry helpers
└── skill-<name>/           # individual skill crates
```

## Adding a new skill

1. Copy `examples/skill-template/` to `skill-<your-name>/`.
2. Pick a name. Use kebab-case for the crate, snake_case for the skill
   id (`skill_<your_name>`).
3. Update the new crate's `Cargo.toml`:
   - Set the `name` and `description`.
   - Trim dependencies you don't need.
4. Edit the skill's `src/lib.rs`:
   - Define the public trait (`<Name>Skill`).
   - Provide a real implementation (`<Provider>Skill`) and a `Mock<Name>Skill`.
   - Implement `to_prompt_context()` on the result type so the LLM can
     turn the structured output into a spoken answer.
5. Wire it in to the workspace:
   - Add the crate to `members` in the top-level `Cargo.toml`.
   - Add it as an optional dependency of `core-skills`.
   - Add `dep:skill-<name>` to the right feature bucket (`core-common`,
     `macos`, `windows`, `ios`).
   - In `src/lib.rs`, append to `ENABLED_SKILL_IDS` and add a
     feature-gated `pub use`.
6. Test:
   - Unit + behavioral tests in the new crate.
   - `cargo build -p core-skills --features <bucket>` to confirm
     wiring.

## Testing principles

- Every skill ships a `Mock<Name>Skill` for orchestrator tests.
- HTTP-backed skills test against an injected provider trait
  (`<Name>Provider`), not real HTTP. See `skill-currency` or
  `skill-air-quality`.
- Bug fixes require a regression test that fails before the fix.

## Error handling

- All errors are `thiserror`-based enums.
- No `unwrap` / `expect` in production paths. Tests may use them
  liberally.
- Distinguish *transient* (worth retrying) from *permanent* errors when
  the upstream supports it (see `skill-dictionary` for the pattern).

## Metrics

Use the project's existing metric patterns:

```rust
counter!("backend_skill_execute_total", 1, "skill" => "<name>", "result" => "success");
histogram!("backend_skill_execute_duration_seconds", elapsed, "skill" => "<name>");
```

If you add a network call, also instrument:

```rust
counter!("backend_dependency_requests_total", 1, "dependency" => "<host>", ...);
```

## Coding style

- `cargo fmt` is the source of truth.
- Prefer small modules. `lib.rs` re-exports; `types.rs` defines
  data + traits; provider-specific files live alongside.
- Avoid narrating comments. Comments explain *why*, not *what*.

## Commits & PRs

- Conventional-commit-ish prefixes are appreciated (`feat:`, `fix:`,
  `chore:`, `docs:`).
- Keep PRs focused. New skill + workspace wiring + tests is one PR;
  unrelated refactors belong in their own.

## Code of conduct

Be kind. Disagree on the technical merits, not the person. Maintainers
reserve the right to close PRs or issues that violate this norm.
