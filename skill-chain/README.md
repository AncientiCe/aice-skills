# skill-chain

Orchestrates phrase-driven skill chains with planner abstraction, sequential execution, confirmation gating for side effects, TTS updates after each completed skill, and explicit done termination.

## Run tests

`powershell
cargo test -p skill-chain
`

## Quality gates

`powershell
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
`
