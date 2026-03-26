# Agent Rules

Follow these rules on every task in this repository.

---

## 1. Test-Driven Development (Required)

- Write behavioral tests first.
- Run tests and confirm they fail (red).
- Implement the smallest change to make them pass.
- Run the full relevant test suite and confirm pass (green).

Do not ship behavior changes without a failing test that defines the behavior.

---

## 2. Quality Gates on Every Task

Before a task is complete, all of these must pass:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -D warnings`
- `cargo test`

If any gate fails, fix it before marking the task done.

---

## 3. Guardrails for Safe Changes

- Make focused changes; avoid broad refactors unless requested.
- Keep public behavior and interfaces stable unless the task explicitly changes them.
- Think through impact on callers, data structures, and error paths before editing.
- Prefer explicit error handling and clear failure modes over hidden behavior.

---

## 4. No Plan Markdown Files

- Do not create plan files like `PLAN.md` or `TODO.md`.
- Keep planning in conversation, issues, or code comments.

---

## 4.1 Crate/Skill README When Needed

- If a crate or skill requires setup, configuration, usage notes, or operational context, add or update a README for it.
- Prefer `crate-name/README.md` for crate-specific instructions and a colocated README in the skill directory for skill-specific instructions.
- Keep these READMEs concise and task-focused: prerequisites, setup steps, run/test commands, and known gotchas.

---

## 5. Observability and Metrics

- New user-facing behavior and skill logic changes should emit metrics using the existing project metrics patterns.
- At minimum, instrument invocation count, errors by kind, and latency for critical paths.
- If you modify a path that already has metrics, preserve and update metric coverage as needed.
- Verify metric emission through tests when practical, or by code-path inspection when tests are not feasible.

---

## 6. No Unused Variables or Dead Code

- No unused variables (use `_` only when intentionally unused).
- Remove dead code, stale imports, and unreachable branches.
- Do not bypass with `#[allow(dead_code)]` unless explicitly requested.

---

## 7. No Placeholders

- No `todo!()`, `unimplemented!()`, stub returns, or "coming soon" paths.
- Commit only real implementations or explicit `Result` errors where required.

---

## 8. No Unsafe `unwrap`/`expect` in Production Paths

- Do not use `unwrap()` or `expect()` in production code.
- Propagate errors with `Result` and `?`, or handle them explicitly.
- If something is guaranteed, model that guarantee with types/validation.

---

## 9. Keep Tests Honest

- Prefer integration-style coverage for behavior that crosses module boundaries.
- Avoid brittle assertions tied to incidental implementation details.
- Every bug fix should include a regression test.

---

## 10. Close Running Processes

- If you start long-running commands for verification, stop them before finishing.
- Do not leave watchers/servers running after task completion.

---

## Quick Reference

| Rule | Action |
|------|--------|
| TDD | Test first -> fail -> implement -> pass |
| Quality | `cargo fmt` + `cargo clippy -D warnings` + `cargo test` |
| Guardrails | Small focused changes, preserve contracts unless requested |
| No plan files | No plan `.md` files in repo |
| Crate/skill docs | Add/update crate or skill README when setup/docs are needed |
| Observability | Add/maintain metrics for skill and user-facing behavior changes |
| No dead code | Remove unused vars/imports/branches |
| No placeholders | No `todo!()`/`unimplemented!()`/stubs |
| Safe errors | No `unwrap()`/`expect()` in production paths |
| Honest tests | Add regression tests for bug fixes |
| Process hygiene | Stop long-running verification processes |
