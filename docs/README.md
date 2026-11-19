# Docs index

Start with ARCHITECTURE.md, then TASKS.md. Implementers need only those two plus the ticket they are working on.

| File | Purpose | Read it when |
|---|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Single source of truth: engineering rules, process model, modules, FFI API, data flows, provider adapters, MCP schemas, index design with complexity, concurrency, storage, permissions, threat model, memory budget, portability | Always, before any ticket |
| [TASKS.md](TASKS.md) | Ordered backlog of 52 vertical-slice tickets with model tags (HAIKU, SONNET, OPUS-ONLY, HUMAN) | Picking up work |
| [DECISIONS.md](DECISIONS.md) | 15 ADRs with rejected alternatives | Questioning a design choice |
| [RESEARCH.md](RESEARCH.md) | Verified CLI flags and events with sources and status (VERIFIED, SECONDARY, UNVERIFIED), ACP, libraries, macOS facts, prior art | Touching a provider adapter or MCP registration |
| [TESTING.md](TESTING.md) | Test layout, contract tests, manual release checklist, memory benchmark harness, baseline table | Writing tests or changing anything memory-relevant |
| [RISKS.md](RISKS.md) | Risk register with mitigations and owning tickets | Milestone reviews |
| [RELEASE.md](RELEASE.md) | Signing, notarization, Homebrew cask, versioning, release checklist | M9 |
| [ROADMAP.md](ROADMAP.md) | v1.x ideas and the Windows and Linux plan | Planning beyond v1 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev setup (including the stable signing identity), workflow, agent usage, style | First day on the project |

Root files: `README.md` (user-facing draft), `AGENTS.md` (rules for AI coding agents), `CLAUDE.md` (imports AGENTS.md).
