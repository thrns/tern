# Research Notes

Snapshot taken 2026-11-19. CLI flags change often. Every row has a source and a status:
- VERIFIED: stated in official docs fetched on 2026-11-19.
- SECONDARY: stated by reputable third-party docs or guides, not seen in official docs.
- UNVERIFIED: inferred or not found; a ticket must confirm against the real CLI before relying on it.

When an implementer confirms or disproves a row, update this file in the same change (ARCHITECTURE rule 12).

## 1. Claude Code (`claude`)
Sources: https://code.claude.com/docs/en/headless , https://code.claude.com/docs/en/cli-reference , https://code.claude.com/docs/en/mcp

| Fact | Status |
|---|---|
| `-p` / `--print` runs non-interactively; exit 0 on success, non-zero on failure | VERIFIED |
| `--output-format text\|json\|stream-json`; `json` has `result`, `session_id`, usage, `total_cost_usd` | VERIFIED |
| Token streaming needs `--output-format stream-json --verbose --include-partial-messages`; deltas are `stream_event` with `event.delta.type == "text_delta"`; last line is `result` | VERIFIED |
| `system/init` event carries session metadata, MCP server status and `mcp_server_errors` | VERIFIED |
| `system/api_retry` events on retryable API errors | VERIFIED |
| `--resume <id>` / `-r`, `--continue`, `--session-id <uuid>`, `--no-session-persistence` | VERIFIED |
| `--allowedTools`, `--disallowedTools`, `--tools "Read,Glob,Grep"` restricts built-in tools; `--tools` does not affect MCP tools | VERIFIED |
| On macOS the default `--tools` set leaves out Glob and Grep; naming them explicitly enables them | VERIFIED 2026-11-21 against claude 2.1.289 (T-019): `system/init` `tools` was `[Glob, Grep, Read]` with `--tools Read,Glob,Grep`, `[Read]` with `--tools Read`, and a default list with no Glob or Grep without `--tools` |
| An unauthenticated turn still emits `system/init`, then an `assistant` line with top-level `error: "authentication_failed"`, then `result` with `is_error: true`, a string `result` and `total_cost_usd: 0`; `claude auth status` printed `loggedIn: false` and exited 1 (the turn's own exit code was not captured) | VERIFIED 2026-11-21 against claude 2.1.289 (OAuth session expired on the capture machine, so no successful `text` turn was recorded; fixtures stay with T-016) |
| `--permission-mode` values: `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions`, `manual` | VERIFIED |
| In `dontAsk`, reads in working dirs and the read-only command set still run; everything else needing approval is denied | VERIFIED |
| `--permission-prompts none` exists from v2.1.259 | VERIFIED |
| Since v2.1.302, `-p` runs start in Manual mode when nothing sets a mode | VERIFIED |
| `--mcp-config <file-or-json>` and `--strict-mcp-config` | VERIFIED |
| Without `--bare`, `-p` loads hooks and `.mcp.json` from the cwd with no trust dialog | VERIFIED |
| `--bare` skips OAuth/keychain and needs `ANTHROPIC_API_KEY` (so not usable for login reuse) | VERIFIED |
| `--append-system-prompt-file`, `--max-turns`, `--max-budget-usd`, `--add-dir`, `--model` | VERIFIED |
| Piped stdin capped at 10 MB | VERIFIED |
| SIGTERM exits 143 and leaves the turn unfinished; SIGINT ends the turn | VERIFIED |
| `claude auth status` exits 0 if logged in, 1 if not | VERIFIED |
| `claude mcp add --scope user --transport stdio <name> -- <cmd> [args]`; user scope stored in `~/.claude.json` | VERIFIED (docs + secondary) |
| No image attachment flag; images are read through the Read tool from a path | VERIFIED (no flag listed) / reading images via Read is long-standing behavior, confirm in T-014 |
| `--input-format stream-json` accepts image content blocks | UNVERIFIED (optimization only) |

## 2. Codex CLI (`codex`)
Sources: https://learn.chatgpt.com/docs/non-interactive-mode (redirect target of developers.openai.com/codex/noninteractive), https://developers.openai.com/codex/mcp , secondary: mintlify mirror of openai/codex docs, codex.danielvaughan.com

| Fact | Status |
|---|---|
| `codex exec` runs non-interactively; `-` reads the prompt from stdin | VERIFIED / SECONDARY for `-` |
| `--json` makes stdout JSONL; events `thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `error`, `item.started`, `item.completed` | VERIFIED |
| Default sandbox is read-only; `--sandbox workspace-write\|danger-full-access` | VERIFIED |
| `--full-auto` deprecated or no longer supported | VERIFIED (doc says no longer supported) |
| `--ephemeral`, `-o/--output-last-message`, `--skip-git-repo-check`, `--output-schema`, `--ignore-user-config` | VERIFIED |
| `codex exec resume --last` and `codex exec resume <SESSION_ID>` | VERIFIED |
| Exact placement of global flags relative to `resume` | UNVERIFIED |
| `-i/--image <file>[,file]` attaches images to the first message; image history survives resume since v0.117.0 | SECONDARY |
| `-C/--cd <dir>` working directory, `-m/--model` | SECONDARY |
| `-c key=value` config overrides | SECONDARY |
| `-c mcp_servers.<name>.command=...` creating a new MCP server for one run | UNVERIFIED |
| `-c approval_policy=...` in exec | SECONDARY (exec cannot approve mid-run anyway) |
| Saved CLI auth reused by default; `CODEX_API_KEY` for single runs | VERIFIED |
| `codex mcp add <name> [--env K=V] -- <cmd> [args]`; config in `~/.codex/config.toml` as `[mcp_servers.<name>]` | VERIFIED |
| `codex login status` | UNVERIFIED |
| No token-level streaming deltas in exec JSONL | SECONDARY (agent_message arrives in `item.completed`) |

## 3. Antigravity CLI (`agy`) and Gemini CLI (`gemini`)
Sources: https://antigravity.google/docs/cli/headless/ , https://antigravity.google/docs/cli/reference , https://antigravity.google/docs/mcp , https://geminicli.com/docs/cli/headless/ , secondary: Flutter docs, Google codelab, Medium (Google Cloud), agentpedia.codes

| Fact | Status |
|---|---|
| Gemini CLI replaced by Antigravity CLI for unpaid tier and Google One users on 2026-06-18 | VERIFIED (geminicli.com banner) |
| `agy` is a Go binary, no Node required | SECONDARY |
| `agy` is closed source and does not support ACP | SECONDARY |
| `-p/--print/--prompt`, `--output-format text\|json\|stream-json`, `--input-format text\|stream-json` | VERIFIED |
| JSON envelope: `conversation_id`, `status`, `response`, `error`, `duration_seconds`, `num_turns`, `usage{input_tokens,output_tokens,thinking_tokens,cache_read_tokens,total_tokens}` | VERIFIED |
| stream-json events: `init`, `step_update` (step types `user_input`, `agent_response`, `tool`, `checkpoint`; states `ACTIVE`, `DONE`), `result` | VERIFIED |
| Exact field carrying text deltas inside `step_update` | UNVERIFIED (take from fixtures) |
| Status values `SUCCESS`, `ERROR`, `CANCELED`, `INTERRUPTED`, `INVALID`, `WAITING`, `RUNNING` | VERIFIED |
| Resume with `-c/--continue` or `--conversation <id>`; each command is a new process | VERIFIED |
| Headless: approvals that cannot be obtained are soft-denied, run exits 0 with a stderr notice | VERIFIED |
| `--dangerously-skip-permissions` approves all tool calls (Tern never uses it) | VERIFIED |
| `--mode accept-edits\|auto\|plan`, `--sandbox`, `--add-dir`, `--model`, `--print-timeout`, `--json-schema` | VERIFIED |
| No image or attachment flag; pasting media only in the TUI | VERIFIED (absent from reference) |
| `agy` can view an image referenced by path in the prompt | UNVERIFIED |
| MCP config: global `~/.gemini/config/mcp_config.json`, workspace `.agents/mcp_config.json`, key `mcpServers` | VERIFIED |
| Print mode loads workspace `.agents/mcp_config.json` in a folder never approved as a project | UNVERIFIED |
| Tool permissions via `permissions.allow/ask/deny` in `~/.gemini/antigravity-cli/settings.json` | VERIFIED |
| Gemini CLI headless: `-p`, `--output-format json\|stream-json`; JSON `response`, `stats`, `error`; events `init`, `message`, `tool_use`, `tool_result`, `error`, `result`; exit codes 0, 1, 42, 53 | VERIFIED |
| Gemini CLI `--resume`, `--approval-mode`, `gemini mcp add` syntax | SECONDARY / UNVERIFIED |

## 4. Agent Client Protocol (ACP)
- ACP is JSON-RPC over stdio between editors and agents; Zed, JetBrains, Neovim and Emacs clients exist; a Rust crate `agent-client-protocol` exists. (SECONDARY)
- Claude Code and Codex use adapters (`@agentclientprotocol/claude-agent-acp`, `@agentclientprotocol/codex-acp`), which are Node packages. (SECONDARY: acpx changelog, Vercel AI SDK docs)
- Vercel's own guidance prefers direct Claude Code and Codex harnesses over the ACP route. (SECONDARY)
- `agy` dropped ACP support. (SECONDARY)
- Conclusion: ADR-004, do not adopt in v1.

## 5. Libraries
| Library | Finding | Source |
|---|---|---|
| rmcp | Official Rust MCP SDK, 3.x series published (3.0.1, 3.1.0); features `server`, `transport-io`, `macros`, `schemars` | docs.rs/crate/rmcp |
| fend-core | MIT, edition 2024 (1.5.x); arbitrary precision, units, conversions; optional GPL unit data behind a feature | docs.rs/crate/fend-core, fend README |
| UniFFI | Mozilla, production Swift bindings, proc-macro support, `uniffi-bindgen-swift` | mozilla.github.io/uniffi-rs |
| SQLite FTS5 | trigram tokenizer since 3.34; contentless and contentless-delete tables; `detail` option shrinks index | sqlite.org/fts5.html |
| notify | Cross-platform watcher, FSEvents on macOS | docs.rs/notify |

## 6. macOS specifics
| Topic | Finding | Source |
|---|---|---|
| Screen Recording re-prompts | macOS 15 Sequoia asks users to re-confirm screen recording apps monthly, including ScreenCaptureKit apps | 9to5Mac, TidBITS (2024) |
| SwiftUI vs AppKit memory | Same menu bar app: about 56 MB (SwiftUI MenuBarExtra) vs about 13 MB (AppKit) | dev.to CursorMeter write-up |
| Native utility footprints | Native AppKit utilities commonly idle at 15 to 40 MB | teenyapps roundups, System Guardian |
| Global hotkeys | Carbon `RegisterEventHotKey` works without Accessibility permission | Long-standing API behavior; confirm in T-007 |
| Intel deprecation | Homebrew downgrades Intel macOS to Tier 3 from September 2026 | Fatbobman's Swift Weekly #111 |

## 7. Prior art
| Product | Stack | Lesson |
|---|---|---|
| Raycast | Native app with a Node extension runtime | Great UX benchmark; memory heavy because of the extension runtime. Copy: single input, instant results, actions on Enter variants. Avoid: account and cloud AI. |
| Alfred | Native, closed source | Copy: speed, keyboard-first. |
| Spotlight | System | Copy: calculator and unit conversions inline. Avoid as our only file search (latency, exclusions). |
| SuperCmd | Electron v1, rewritten in Swift for v2 claiming about 3x less memory than Raycast | Confirms the native-shell decision. |
| Tinycast | Native Swift, about 3 MB, AGPL | Shows the size floor; we stay permissive (MIT/Apache). |
| Sol | Swift + React Native, MIT | Calculator and launcher patterns. |
| look | Swift + Rust, MIT | Same split as ours; useful reference for FFI layout. |
| Wox | Cross-platform native, advertises about 150 MB | Example of what "light" usually means; our target is well below. |

What to copy: one input box, Tab into AI, preview before sending a screenshot, Esc always dismisses, no account. What to avoid: background extension runtimes, webviews, sending anything to a vendor without the user's CLI login.
