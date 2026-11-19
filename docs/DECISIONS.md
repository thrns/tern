# Architecture Decision Records

Each record: context, decision, alternatives rejected and why, consequences. Status for all: Accepted (2026-11-19) unless noted. Evidence and sources are in `RESEARCH.md`.

---

## ADR-001 Rust core with a native AppKit shell
**Context.** Memory efficiency is the top non-functional requirement, and the core should port to Windows and Linux.
**Decision.** Product logic in Rust (`tern-core`). macOS UI in Swift with AppKit, linked in-process.
**Rejected.**
- Electron: separate renderer and GPU processes, typically well over 100 MB idle. Prior art (SuperCmd v1) moved off Electron for this reason.
- Tauri: lighter than Electron, but WKWebView spawns WebContent processes; tens of MB before any UI. The UI here is a list and a text view, which does not need HTML.
- Pure Swift: fastest to start on macOS, but the core would need a rewrite for Windows and Linux.
- Rust-only UI (egui, GPUI, iced): portable, but custom text input, accessibility and IME handling cost more than a thin native shell.
**Consequences.** One FFI boundary to maintain. Each future OS needs its own thin shell (ROADMAP.md).

## ADR-002 AppKit only, no SwiftUI
**Context.** A published comparison of the same menu bar app measured about 56 MB RSS with SwiftUI `MenuBarExtra` against about 13 MB after rewriting in AppKit.
**Decision.** AppKit for every resident view. `NSAttributedString` for Markdown.
**Rejected.** SwiftUI (memory and menu bar edge cases), hybrid SwiftUI-in-NSHostingView (pulls in the SwiftUI runtime anyway).
**Consequences.** More UI code. Acceptable because the UI surface is small.

## ADR-003 In-process FFI with UniFFI
**Decision.** `tern-ffi` builds a static library with UniFFI proc-macros; `xtask` generates Swift bindings. Rust calls back into Swift through a foreign trait (`CoreListener`).
**Rejected.**
- Separate core daemon with JSON-RPC: one more resident process (5 to 10 MB), lifecycle and versioning complexity.
- swift-bridge: smaller community, fewer type mappings.
- Hand-written C ABI: tedious and error-prone for records and callbacks; bad fit for lower-tier implementers.
**Consequences.** Build step to regenerate bindings (automated in `xtask build-core`). UniFFI is MPL-2.0 (file-level copyleft), acceptable for a permissive project because we do not modify its files.

## ADR-004 Headless CLI per turn, not ACP, for v1
**Context.** All three CLIs have a documented headless mode with JSON event streams and session resume. ACP would give richer multi-turn sessions, but Claude Code and Codex need Node-based adapters (`claude-agent-acp`, `codex-acp`), which would add a resident Node process per session, and Antigravity CLI (`agy`) dropped ACP entirely.
**Decision.** Spawn `claude -p`, `codex exec`, `agy -p` per turn; resume with each CLI's session id.
**Rejected.** ACP (memory cost of Node adapters, no agy support), the Claude Agent SDK or Codex SDK libraries (would re-implement login reuse and add runtimes).
**Consequences.** Startup cost per turn (CLI launch). No mid-turn permission prompts; we rely on read-only modes instead, which suits a launcher. Revisit when ACP is native in at least two target CLIs (ROADMAP.md).

## ADR-005 Antigravity CLI replaces Gemini CLI as the Google provider
**Context.** Google replaced Gemini CLI with Antigravity CLI (`agy`) for individual accounts on 2026-06-18. `agy` is a Go binary with `-p`, `--output-format json|stream-json`, `--conversation <id>` and workspace MCP config.
**Decision.** Ship `agy` as a P0 provider. Keep a `gemini` legacy adapter as P2, disabled by default, for enterprise or API-key users still on Gemini CLI.
**Rejected.** Gemini CLI only (sunset for most users), dropping Google entirely (the brief asks for it).
**Consequences.** Several `agy` behaviors are UNVERIFIED (image reading, print-mode loading of workspace MCP config). Their tickets include verification steps and fallbacks.

## ADR-006 MCP stdio server spawned on demand
**Decision.** Separate `tern-mcp` binary, stdio transport, built with the official Rust SDK `rmcp` 3.x. CLIs start it; it exits with them. It reads `index.db` directly (read-only, WAL) and asks the app only for screenshots over a Unix socket.
**Rejected.**
- Streamable HTTP server inside the app: always-on port, auth complexity, larger attack surface.
- MCP server that proxies every call to the app: forces the app to be running for search and calc, and adds a hop.
- Node or Python MCP SDKs: runtime dependency and memory.
**Consequences.** Zero idle cost. Screenshot tool requires the app to be running, which is reasonable because the permission belongs to the app.

## ADR-007 SQLite FTS5 index with our own filename index; Spotlight only as an explicit fallback
**Decision.** One SQLite DB: trigram FTS5 for names and paths, contentless `unicode61` FTS5 with `detail='column'` for content. Watch with FSEvents via `notify`. Spotlight (`mdfind`) is P2 "search everywhere" on demand.
**Rejected.**
- Spotlight for filenames by default: process spawn per keystroke (tens to hundreds of ms), respects user Spotlight exclusions unpredictably, not portable.
- Tantivy: excellent, but larger memory and binary footprint for this scale, and two storage engines (we need SQLite for state anyway).
- ripgrep-style search without an index: fine for code folders, too slow for whole Documents trees as-you-type.
**Consequences.** Index uses disk (roughly 0.25x to 0.5x of indexed text plus small name index). Phrase queries are not supported in v1 because of `detail='column'`.

## ADR-008 No App Sandbox
**Context.** Tern must spawn user-installed CLIs from arbitrary paths, read user-chosen folders persistently, and run `screencapture`.
**Decision.** Non-sandboxed, hardened runtime, Developer ID signed and notarized. Distributed via Homebrew cask and GitHub releases, not the Mac App Store.
**Rejected.** Sandboxed App Store build (cannot spawn arbitrary CLIs).

## ADR-009 Screenshots via `screencapture` first
**Decision.** `/usr/sbin/screencapture` for v1, ScreenCaptureKit later.
**Rejected for now.** ScreenCaptureKit (more code, async APIs, needs its own picker work). It stays on the roadmap to reduce macOS 15+ monthly re-confirmation prompts.

## ADR-010 fend-core for the calculator
**Decision.** `fend-core` (MIT, arbitrary precision, units, conversions), with the optional GPL units data feature off and no currency handler (no network).
**Rejected.** `meval`/`evalexpr` (no units), writing our own (time sink), Numbat (heavier, aimed at a REPL).

## ADR-011 Read-only AI by default with explicit, scoped elevation
**Decision.** Default `AiMode::ReadOnly` mapped to each CLI's strongest non-bypass read-only mode (ARCHITECTURE 8.4 to 8.6). `AllowEdits` needs a chosen work dir and a confirmation. No bypass flags in any mode, enforced by a CI grep.
**Rejected.** Auto-approve flags (unsafe), per-tool approval UI through `--permission-prompt-tool` (only Claude supports it; deferred to ROADMAP).

## ADR-012 Tests in a root `/test` folder
**Decision.** All Rust tests in the `test/rust` crate (`tern-tests`), Swift tests in `test/swift` via the root `Package.swift`, fixtures and budgets under `test/`.
**Consequences.** Functions that need unit tests must be `pub` in their crate. Acceptable; the crates are not published.

## ADR-013 SwiftPM instead of an Xcode project
**Decision.** `Package.swift` at the repo root builds the app executable; `cargo xtask bundle` assembles `Tern.app`.
**Rejected.** `.xcodeproj` (binary-ish `pbxproj` diffs that lower-tier models edit badly, merge conflicts).
**Consequences.** No Xcode UI previews; fine for AppKit.

## ADR-014 Apple Silicon and macOS 14+ only for v1
**Rejected.** Universal binary (doubles build time and test surface for a shrinking Intel base). Can be added in `xtask bundle` later with `lipo`.

## ADR-015 No Docker
**Decision.** No containers, devcontainers or Docker-based CI anywhere. macOS work runs on macOS runners natively; the Linux portability check is a plain `cargo check` on an Ubuntu runner.
