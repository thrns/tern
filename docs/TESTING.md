# Test and Memory Benchmark Plan

## 1. Layout
Everything lives under `/test` (ADR-012).
| Path | What |
|---|---|
| `test/rust/tests/*.rs` | Rust tests for `tern-core` and `tern-mcp`, one file per module or feature |
| `test/swift/*.swift` | XCTest for `TernKit` helpers (hotkey parsing, debouncer, Markdown, thumbnails) |
| `test/fixtures/providers/<id>/` | Real CLI output recorded by `scripts/capture-fixtures.sh` (reviewed for personal data) |
| `test/fixtures/providers/fake/` | Hand-written fixtures and `replay.sh` for the fake provider |
| `test/fixtures/corpus/` | Small checked-in tree for index tests |
| `test/security/` | Prompt-injection and fake-secret corpus (T-044) |
| `test/bench/budgets.toml` | Memory budgets enforced by `cargo xtask membench` |

Run everything: `cargo test -p tern-tests && swift test`.

## 2. Test levels
| Level | Scope | Examples | Runs on |
|---|---|---|---|
| Unit | Pure functions | calc rules, redaction patterns, hotkey grammar, FTS query quoting, file naming | macOS + Linux CI |
| Component | One module with real SQLite or processes | indexer on corpus, runner with shell-script fake CLIs, UDS round trip, settings round trip | macOS + Linux CI (UDS and capture-free) |
| Contract | Provider parsers against recorded fixtures | `provider_claude.rs`, `provider_codex.rs`, `provider_agy.rs` | macOS + Linux CI |
| Protocol | `tern-mcp` binary over stdio JSON-RPC | `mcp_protocol.rs` | macOS CI |
| End to end (manual) | Real CLIs, real UI | checklist below | TP before each release |
| Performance | Latency asserts with generous CI bounds | query p95, calc p95 | macOS CI |
| Memory | Footprint budgets | `cargo xtask membench` | macOS CI (blocking) |

## 3. Provider contract tests
1. TP records fixtures with `scripts/capture-fixtures.sh` whenever a CLI updates in a way that matters (new major or minor version, or a user-reported parse failure).
2. Each fixture has an expected event sequence written as Rust literals in its test file.
3. Parsers must ignore unknown fields and event types, so new CLI versions usually keep passing. A failing contract test after re-recording means the adapter must change.
4. `VERSION` files in each fixture dir record the CLI version used.

## 4. Manual end-to-end checklist (per release)
- [ ] Fresh `TERN_HOME`: onboarding appears, finishes, panel opens.
- [ ] Option+Space toggles the panel over a full-screen app.
- [ ] `5 km to mi`, `100 °C to °F`, `0xff to decimal`, `2^64` give correct rows; Enter copies.
- [ ] Index ~/Documents; search a filename fragment and a content word; open, reveal, copy path.
- [ ] Ask Claude, Codex and Antigravity a question each; follow-up keeps context for each.
- [ ] Capture region and ask Claude and Codex about it.
- [ ] Cancel a running ask with Cmd+.; no child processes remain (`pgrep -P <Tern pid>`).
- [ ] In an app ask, Claude uses `mcp__tern__calculate` and `mcp__tern__search_files`.
- [ ] Install into CLIs for each provider; in Terminal, `claude` lists the tern server; Remove works.
- [ ] AI screenshots off: tool returns the disabled message. On: notification appears.
- [ ] Read-only: asking to create a file is refused by every provider. Edits on: it works only in the work dir.
- [ ] Quit and relaunch: settings, history and index persist.

## 5. Memory benchmark
Budgets and metric are defined in `ARCHITECTURE.md` Section 17. Summary of the harness:
1. `cargo xtask membench` builds a release bundle, generates a deterministic 20k-file corpus (`cargo xtask gen-corpus`), and launches `Tern.app/Contents/MacOS/Tern` with `TERN_BENCH=1`, `TERN_HOME=<tmp>`, `TERN_BENCH_CORPUS=<corpus>`, `TERN_FAKE_PROVIDER=<replay.sh>`, `TERN_FAKE_FIXTURE=<hello.jsonl>`.
2. The app's `BenchMode` prints a JSON line at each phase and waits for a newline on stdin.
3. The harness samples `ri_phys_footprint` and `ri_lifetime_max_phys_footprint` via `proc_pid_rusage` every 100 ms, takes the median of 10 samples per phase and the max peak per window.
4. It also measures `tern-mcp` (spawned directly with an MCP initialize and a `search_files` call), counts Tern-owned child processes after `ask_done`, and checks bundle size.
5. Results go to `target/membench/report.json` and the GitHub job summary. Any limit breach fails CI.

Local quick check while developing: `footprint -p Tern` or Activity Monitor's Memory column (same metric).

Leak hunting: run the ask loop 200 times with `TERN_BENCH_LOOP=200` (BenchMode option added in T-043) and compare idle before and after; use Instruments "Allocations" on the app and `dhat` (dev-only feature in `test/rust`) for Rust allocations.

## 6. Baseline results
Filled in by T-043 on the first green run.
| Scenario | Target | Limit | Measured | Date | Machine |
|---|---|---|---|---|---|
| app idle empty | 22 MB | 30 MB | | | |
| app idle 100k files | 28 MB | 40 MB | | | |
| panel shown | 35 MB | 50 MB | | | |
| indexing peak 20k | 60 MB | 90 MB | | | |
| tern-mcp after init | 6 MB | 10 MB | | | |
