# Contributing

## Setup (macOS 14+, Apple Silicon)
1. Install Xcode Command Line Tools (`xcode-select --install`) and Rust via rustup. The repo's `rust-toolchain.toml` selects the toolchain.
2. `cargo xtask build-core && swift build` to compile; `cargo xtask bundle && open dist/Tern.app` to run.
3. Create a stable local signing identity once so macOS keeps Screen Recording and folder grants across rebuilds:
   Keychain Access > Certificate Assistant > Create a Certificate, name "Tern Dev", identity type Self Signed Root, certificate type Code Signing. `xtask bundle` uses it automatically.
4. Use a throwaway data dir while developing: `TERN_HOME=/tmp/tern-dev open dist/Tern.app` (or run the binary directly to see logs).
5. No Docker or containers are used anywhere in this project.

## Workflow
- Pick the next unblocked ticket in `docs/TASKS.md`. One ticket per PR. PR title: `T-0xx <ticket title>`.
- Read `docs/ARCHITECTURE.md` Section 0 before writing code. The rules there are enforced in review.
- If you need a decision the docs do not cover, open an issue labeled `needs-architect` and stop. Do not invent architecture inside a ticket.
- Update `docs/RESEARCH.md` when you verify or disprove a fact about an external CLI.
- Run before pushing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p tern-tests`, `swift test`, and `cargo xtask membench` if you touched the shell, index, runner or MCP server.

## Working with AI coding agents
`AGENTS.md` (read by Codex and others) and `CLAUDE.md` (imports AGENTS.md for Claude Code) give agents the rules. Model guidance per ticket tag:
| Tag | Who | Notes |
|---|---|---|
| HAIKU | Claude Haiku or similar | Give it the ticket text and ARCHITECTURE.md only |
| SONNET | Claude Sonnet or similar | Same inputs; expect it to verify external CLI behavior where the ticket says so |
| OPUS-ONLY | Claude Opus or a human architect | Design-sensitive work |
| HUMAN | TP | Accounts, logins, hardware, release secrets |

Never run AI agents on this repo with permission-bypass flags; that would contradict the product's own safety stance.

## Code style
- No comments in code (Arch rule 1). Prefer small functions with descriptive names.
- No `unwrap`/`expect`/`panic!` outside tests; no force unwraps in Swift.
- Tests go under `/test` only.
- Errors: `CoreError { kind, message }` with actionable messages that include the relevant path or provider id.

## Commit messages
Imperative mood, reference the ticket: `T-032 Add RRF ranking to query engine`.
