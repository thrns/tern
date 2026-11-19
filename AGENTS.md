# Rules for AI coding agents

You are implementing tickets from `docs/TASKS.md`. Your only design reference is `docs/ARCHITECTURE.md`.

1. Do exactly one ticket. Touch only the files the ticket lists unless a compile error forces a related change; mention any extra file in the PR description.
2. Follow `docs/ARCHITECTURE.md` Section 0. Key points: no code comments of any kind, no `unwrap`/`expect`/`panic!` outside tests, no Swift force unwraps, typed errors, all tests under `/test`.
3. Never add or run `--dangerously-skip-permissions`, `--yolo`, `bypassPermissions`, `danger-full-access` or `--full-auto`. CI rejects them.
4. No network calls from Tern code. No telemetry. No Docker or containers.
5. When the ticket says a fact is UNVERIFIED, verify it (fixture or real CLI) and update `docs/RESEARCH.md` in the same change.
6. If the ticket needs a decision that ARCHITECTURE.md does not make, stop and say so. Do not invent architecture.
7. Before finishing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p tern-tests`, `swift test` (if Swift changed), `cargo xtask membench` (if shell, index, runner or MCP changed).
8. Keep memory in mind: bounded channels, streaming instead of collecting, no caches without a cap.
