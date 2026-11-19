# Release and Packaging Plan

## Channels
1. GitHub Releases: notarized `Tern-<version>.zip` and `Tern-<version>.dmg` with SHA-256 checksums.
2. Homebrew cask in a project tap first (`brew install --cask <owner>/tern/tern`). Submit to `homebrew/cask` later, once the project meets its notability rules.
3. Build from source: `cargo xtask bundle --release` (signs with "Tern Dev" or ad-hoc). Documented in the README for users without a notarized build.

Not planned: Mac App Store (needs App Sandbox, ADR-008).

## Prerequisites (TP)
- Apple Developer Program membership (paid, yearly). Without it, releases cannot be notarized and Gatekeeper blocks first launch. Decide before M9 (T-045).
- Final product name and bundle identifier (T-050) before the first public release, because TCC grants and the support folder path are keyed to them.

## Versioning
SemVer. `Cargo.toml` workspace version is the single source; `xtask bundle` writes it into `CFBundleShortVersionString` and the git commit count into `CFBundleVersion`. Tags are `v<version>`.

## Pipeline (T-046)
1. Tag push triggers `release.yml` on `macos-15`.
2. Verify tag equals workspace version.
3. Full CI (tests, membench) must pass.
4. Import Developer ID certificate into a temporary keychain.
5. `cargo xtask bundle --release --sign "$MACOS_SIGN_IDENTITY"` with hardened runtime and secure timestamp; `tern-mcp` signed before the app.
6. Zip, `xcrun notarytool submit --wait`, `xcrun stapler staple`, re-zip.
7. DMG, notarize, staple.
8. Upload release assets and checksums.
9. Open a PR on the tap bumping version and sha256 (T-047).
10. Delete the temporary keychain.

## Release checklist
- [ ] TESTING.md manual checklist done on a clean user account.
- [ ] RESEARCH.md rows touched by this release re-verified with current CLI versions.
- [ ] Security review (T-044) has no open High items.
- [ ] CHANGELOG.md updated.
- [ ] `spctl -a -vv` accepts the app as Notarized Developer ID.
