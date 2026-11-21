# Research Notes

Snapshot taken 2026-11-19. CLI flags change often. Every row has a source and a status:
- VERIFIED: stated in official docs fetched on 2026-11-19.
- SECONDARY: stated by reputable third-party docs or guides, not seen in official docs.
- UNVERIFIED: inferred or not found; a ticket must confirm against the real CLI before relying on it.
- DISPROVEN: checked against a real CLI or recorded fixture and found false; the row says which fixture and version.

When an implementer confirms or disproves a row, update this file in the same change (ARCHITECTURE rule 12).

## 1. Claude Code (`claude`)
Sources: https://code.claude.com/docs/en/headless , https://code.claude.com/docs/en/cli-reference , https://code.claude.com/docs/en/mcp

| Fact | Status |
|---|---|
| `-p` / `--print` runs non-interactively; exit 0 on success, non-zero on failure | VERIFIED |
| `--output-format text\|json\|stream-json`; `json` has `result`, `session_id`, usage, `total_cost_usd` | VERIFIED |
| Token streaming needs `--output-format stream-json --verbose --include-partial-messages`; deltas are `stream_event` with `event.delta.type == "text_delta"`; last line is `result` | VERIFIED |
| `system/init` event carries session metadata and MCP server status | VERIFIED |
| `system/init` carries `mcp_server_errors` | DISPROVEN 2026-11-21, claude 2.1.330: no init line in any claude fixture has that key, although `mcp_servers[]` lists servers with status `failed` and `needs-auth`; use `mcp_servers[].{name,status,source}` (claude/text.jsonl line 1) |
| Shapes used by ARCHITECTURE 8.4: `system/init.session_id`; `stream_event` with `event.delta.type == "text_delta"` and `event.delta.text`; `assistant` `message.content[]` `tool_use`; `user` `tool_result`; final `result` with string `result`, `is_error`, `usage.input_tokens`, `usage.output_tokens`, `total_cost_usd` | VERIFIED 2026-11-21, claude 2.1.330 (claude/text.jsonl, image.jsonl for tool_use and tool_result, error.jsonl). A failed run has `result.subtype == "success"` with `is_error == true`, `terminal_reason == "api_error"`, exit code 1, so parsers must key on `is_error`. `--resume <id>` keeps the same `session_id` (claude/resume.jsonl). Lines of type `system/status`, `system/thinking_tokens` and `rate_limit_event` also occur; `system/api_retry` was not observed |
| `system/api_retry` events on retryable API errors | VERIFIED |
| `--resume <id>` / `-r`, `--continue`, `--session-id <uuid>`, `--no-session-persistence` | VERIFIED |
| `--allowedTools`, `--disallowedTools`, `--tools "Read,Glob,Grep"` restricts built-in tools; `--tools` does not affect MCP tools | VERIFIED |
| `--tools`, `--allowedTools` and `--disallowedTools` take multiple values and swallow trailing positional arguments; pass the prompt via stdin | VERIFIED 2026-11-21, claude 2.1.289 |
| On macOS the default `--tools` set leaves out Glob and Grep; naming them explicitly enables them | First half VERIFIED; second half VERIFIED 2026-11-21, claude 2.1.330: `--tools Read,Glob,Grep` gives built-in tools exactly `Glob`, `Grep`, `Read` in `system/init` (claude/text.jsonl line 1) |
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
| No image attachment flag; images are read through the Read tool from a path | VERIFIED (no flag listed); reading via Read VERIFIED 2026-11-21, claude 2.1.330: `Read` `tool_use` with `file_path`, then a `tool_result` holding an `image` block, then a correct description (claude/image.jsonl) |
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
| Exact placement of global flags relative to `resume` | VERIFIED 2026-11-21, codex-cli 0.166.0: `codex exec --json --sandbox read-only --skip-git-repo-check -C <dir> resume <thread_id> -` exits 0, reuses the same `thread_id` and carries context (codex/resume.variant.txt is `exec-flags-then-resume`, codex/resume.jsonl, codex/resume.exit.txt). The order `resume <id>` before the flags was not tried, because the script only tries it after a failure |
| `-i/--image <file>` attaches an image to the first message and the model sees it | VERIFIED 2026-11-21, codex-cli 0.166.0 for one file: exit 0 and a correct description of the screenshot (codex/image.jsonl) |
| `-i` takes a comma-separated list; image history survives resume since v0.117.0 | SECONDARY |
| `-C/--cd <dir>` working directory, `-m/--model` | SECONDARY |
| `-c key=value` config overrides | SECONDARY |
| `-c mcp_servers.<name>.command=...` and `-c mcp_servers.<name>.args=[]` are accepted by `codex exec` without error | VERIFIED 2026-11-21, codex-cli 0.166.0: exit 0, empty stderr, normal turn (codex/mcp_override.jsonl, mcp_override.exit.txt, mcp_override.stderr.txt) |
| The `-c` override actually starts an MCP server for that run | UNVERIFIED (codex/mcp_override.jsonl contains no MCP item and the dummy server was `/usr/bin/true`, which exits at once; T-020 needs a real stdio server and a prompt that calls its tool) |
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
| Exact field carrying text deltas inside `step_update` | VERIFIED 2026-11-21, agy 1.5.14: `step_update.text_delta` (string) on steps with `step_type == "agent_response"`, on `ACTIVE` updates and also on the final `DONE` update, where it is `"\n"` (agy/text.jsonl, resume.jsonl, mcp_workspace.jsonl) |
| Line discriminator and nesting | VERIFIED 2026-11-21, agy 1.5.14: each line is `{"event": "init"\|"step_update"\|"result", ...}` with the payload under a key of the same name; there is no top-level `type`. `init` has top-level `conversation_id` and `init.{cwd,tools,permission_mode}`; `step_update` has `conversation_id`, `step_index`, `state`, `step_type`, `text_delta`, `tool_name`, `tool_info{name,parameters,output,error}`, `duration_seconds`, `usage`; `result` has `conversation_id`, `status`, `response`, `usage`, `num_turns`, `duration_seconds`, optional `denied_actions[]`. Also observed: step type `system_message`, step state `ERROR` (tool denied; agy/denied.jsonl); `checkpoint` not observed |
| Status values `SUCCESS`, `ERROR`, `CANCELED`, `INTERRUPTED`, `INVALID`, `WAITING`, `RUNNING` | VERIFIED |
| Resume with `-c/--continue` or `--conversation <id>`; each command is a new process | VERIFIED |
| Headless: approvals that cannot be obtained are soft-denied, run exits 0 with a stderr notice | VERIFIED |
| `--dangerously-skip-permissions` approves all tool calls (Tern never uses it) | VERIFIED |
| `--mode accept-edits\|auto\|plan`, `--sandbox`, `--add-dir`, `--model`, `--print-timeout`, `--json-schema` | VERIFIED |
| No image or attachment flag; pasting media only in the TUI | VERIFIED (absent from reference) |
| `agy` can view an image referenced by path in the prompt | VERIFIED 2026-11-21, agy 1.5.14: prompt with the absolute path produced a `read_file` tool step on the PNG and the answer `Screenshot of a macOS desktop.`, which matches the image (agy/image.jsonl). The model also ran a PIL `run_command` in the same turn, so the read is shown by the correct description, not by tool output |
| MCP config: global `~/.gemini/config/mcp_config.json`, workspace `.agents/mcp_config.json`, key `mcpServers` | VERIFIED |
| Print mode loads workspace `.agents/mcp_config.json` in a folder never approved as a project | UNVERIFIED after 2026-11-21 run, agy 1.5.14: inconclusive. The model read the config file itself with `read_file` and never called `call_mcp_tool`, `list_resources` or `read_resource`; the dummy server was `/usr/bin/true`, which exits at once (agy/mcp_workspace.jsonl). T-021 needs a real stdio server |
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
