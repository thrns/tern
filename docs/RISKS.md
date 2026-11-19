# Risk Register

Scores: likelihood (L) and impact (I) from 1 (low) to 3 (high). Review at each milestone.

| # | Risk | L | I | Mitigation | Owner / ticket |
|---|---|---|---|---|---|
| R1 | CLI flags or JSON events change and break an adapter | 3 | 2 | Tolerant parsers, fixture contract tests, `VERSION` files, RESEARCH.md status column, adapters isolated behind `Provider` | T-019, T-024, T-025 |
| R2 | Codex `-c` MCP injection does not work | 2 | 1 | Fallback to user-level "Install into my CLIs" | T-024, T-036 |
| R3 | `agy` does not load workspace MCP config or cannot read images in print mode | 2 | 2 | Global install fallback; `ImageSupport::Unverified` warning in UI | T-025, T-036 |
| R4 | Google changes `agy` further (closed source, fast-moving) | 2 | 2 | Same as R1; legacy Gemini adapter as backup for enterprise users | T-049 |
| R5 | Memory budgets missed because of SQLite or AppKit overhead | 2 | 3 | Budgets measured from M0; cache caps; drop data on hide; strict CI gate | T-008, T-043 |
| R6 | macOS monthly Screen Recording re-prompt annoys users | 3 | 1 | Documented; ScreenCaptureKit picker on the roadmap | ROADMAP |
| R7 | TCC grants reset on every dev build | 3 | 1 | Stable "Tern Dev" self-signed identity | CONTRIBUTING, T-006 |
| R8 | Prompt injection through indexed files or screenshots | 2 | 3 | Read-only defaults, untrusted wrappers, redaction, no network tools for Claude, security review | T-044 |
| R9 | `agy` network tools in read-only mode allow exfiltration | 2 | 2 | Approvals soft-denied without bypass; verify in security review; document residual risk | T-044 |
| R10 | GUI app cannot find CLIs installed via nvm or other managers | 3 | 2 | Login-shell PATH probe, known dirs, per-provider path override | T-012 |
| R11 | UniFFI and SwiftPM linking friction slows M0 | 2 | 2 | SONNET ticket with explicit fallback; done once | T-005 |
| R12 | No paid Apple Developer account, so no notarization | 2 | 2 | Build-from-source path with local signing; cask only after T-045 | T-045 |
| R13 | Users index huge trees (whole home) | 2 | 2 | Validation rejects home and `/`; exclusions; streaming walk; size caps | T-004, T-030 |
| R14 | rmcp 3.x API churn | 2 | 1 | Pin exact version; tools logic in plain functions independent of rmcp | T-035 |
| R15 | Claude `-p` defaults change (permission mode, bare default) | 2 | 2 | Tern always passes explicit `--permission-mode` and tools; never relies on defaults. When `--bare` becomes the default for `-p`, re-check login reuse | T-019 |
| R16 | Scope creep (clipboard history, launcher apps) delays v1 | 2 | 2 | Non-goals list in Arch 1.2; P2 bucket | TP |
