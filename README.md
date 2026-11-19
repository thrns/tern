# Tern (working name)

A tiny macOS menu bar launcher: calculator, file search, screenshots, and quick questions to the AI CLIs you already use. Native, local, no accounts, no telemetry.

> Status: pre-alpha. This README is a draft that gets finalized in ticket T-048.

## What it does
- **Option+Space** opens a small panel.
- Type math or conversions: `5 km to mi`, `100 °C to °F`, `0xff to decimal`. Enter copies the result.
- Type part of a filename or a word inside a file to search the folders you chose. Enter opens, Cmd+Enter reveals in Finder.
- Press **Tab** to ask AI. Tern runs the CLI you already have installed and logged in:
  - Claude Code (`claude`)
  - Codex CLI (`codex`)
  - Antigravity CLI (`agy`, Google's successor to Gemini CLI)
- **Option+Shift+Space** captures part of the screen and opens an AI question with it attached.
- Your CLIs can use Tern too: through its MCP server they can search your indexed files, read them, calculate, and (only if you allow it) take a screenshot.

## Principles
- **Light.** Native AppKit plus a Rust core. Target under 30 MB of memory when idle. No Electron, no web views, no background helpers.
- **Local.** Tern never talks to the network. Only your AI CLIs do, using their own logins. Tern stores no API keys.
- **Safe by default.** AI runs read-only. Edits need an explicit setting and a chosen folder. Tern never uses auto-approve or skip-permission flags.

## Install
Coming with the first release:
```
brew install --cask <tap>/tern
```
From source (Apple Silicon, macOS 14+):
```
cargo xtask bundle --release
open dist/Tern.app
```

## Permissions
- **Screen Recording**: asked the first time you capture. macOS 15 and later may ask you to re-confirm about once a month.
- **Desktop, Documents, Downloads**: macOS asks once when Tern first indexes one of these folders.
- Tern does not need Accessibility or Full Disk Access.

## Privacy
Nothing leaves your Mac through Tern. When you ask AI, your question and any attachment you chose go to the provider behind that CLI, exactly as if you ran the CLI yourself. Files matching secret patterns (`.env`, keys, credentials) are never indexed, and secrets inside search snippets are redacted.

## Use Tern tools from your terminal
Settings > AI > "Install into my CLIs" registers the `tern-mcp` server with Claude Code, Codex and Antigravity. You see exactly what will change before it happens, and you can remove it the same way.

## License
MIT OR Apache-2.0.
