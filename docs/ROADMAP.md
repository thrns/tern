# Roadmap

## v1 (this backlog)
Milestones M0 to M9 in TASKS.md: calculator, file search, screenshots, ask via Claude Code, Codex and Antigravity CLIs, MCP tools, read-only by default, notarized Homebrew release.

## v1.x candidates (macOS)
| Item | Why | Notes |
|---|---|---|
| ScreenCaptureKit capture with `SCContentSharingPicker` | Fewer Sequoia re-confirmation prompts, window-accurate captures | Replaces `platform.capture` on macOS 14+ |
| PDF and Office text extraction | Common search request | Use macOS `PDFKit` via the shell, or a small Rust extractor; must respect memory budgets |
| Per-tool approval UI for Claude | Allow selective actions without full edits mode | `--permission-prompt-tool` pointing at a Tern MCP tool that asks the user |
| Generic provider definitions in TOML | Add simple CLIs without code | Declarative command template plus a JSONPath-like event mapping |
| ACP transport | Richer multi-turn sessions | Revisit when two of the three target CLIs speak ACP natively without Node adapters (ADR-004) |
| Universal binary | Intel users | `lipo` in `xtask bundle` |
| Clipboard history, app launching | Launcher parity | Only if memory budgets hold |

## Windows (v2)
- Core: compiles already (CI portability check). Add `platform/windows.rs`: `Windows.Graphics.Capture` for screenshots, `SetThreadPriority` background mode, no login-shell PATH probe (use registry PATH plus known dirs such as `%APPDATA%\npm`), Spotlight equivalent none (local index only).
- Watcher: `notify` uses `ReadDirectoryChangesW`.
- MCP screenshot channel: AF_UNIX sockets work on Windows 10+; fall back to a named pipe if needed.
- Shell options, in order of preference:
  1. Rust + `windows` crate (Win32 popup window, native edit control): smallest footprint, no extra runtime, links the core directly without FFI.
  2. C# WinUI 3 with `uniffi-bindgen-cs`: nicest UI, but .NET runtime memory cost must be measured against the budgets.
- Hotkey: `RegisterHotKey`. Tray: `Shell_NotifyIcon`.
- CLI names on Windows: `claude.exe`/`claude.cmd`, `codex.exe`, `agy.exe`; `procenv` must handle `.cmd` shims.

## Linux (v2)
- `platform/linux.rs`: screenshots via the xdg-desktop-portal Screenshot API (works on Wayland), fallback to `grim` or `maim` on X11.
- Shell: GTK4 via `gtk4-rs` linking the core directly (no FFI layer). Layer-shell popup on Wayland compositors that support it, regular undecorated window otherwise.
- Global hotkey: Wayland has no universal API; use the GlobalShortcuts portal where available, else document binding a desktop shortcut to `tern --toggle` (single-instance IPC over the existing Unix socket).
- Packaging: Flatpak would sandbox CLI spawning, so prefer AppImage plus distro packages.

## Shared work before porting
1. Keep all logic in `tern-core`; the FFI surface (Arch 6) is the contract.
2. Add a `tern` CLI binary (`tern --toggle`, `tern search <q>`, `tern ask <q>`) that reuses the core; useful on every OS and for scripting.
