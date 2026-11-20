use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tern_core::core::Core;
use tern_core::error::ErrorKind;
use tern_core::settings::Settings;

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!("tern-core-{name}-{}-{nanos}", std::process::id()))
}

#[test]
fn open_creates_tree_and_returns_default_settings() {
    let root = scratch_dir("open");
    let core = Core::open(Some(root.clone())).expect("core opens");
    for name in ["captures", "workspaces", "prompts", "logs", "run"] {
        let dir = root.join(name);
        assert!(dir.is_dir(), "{} should exist", dir.display());
        let mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{} should be mode 0700", dir.display());
    }
    assert_eq!(core.settings(), Settings::default());
    assert!(!root.join("config.toml").exists());
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn version_matches_package_version() {
    assert_eq!(Core::version(), "0.1.0");
}

#[test]
fn open_fails_with_io_error_when_home_is_a_file() {
    let root = scratch_dir("blocked");
    fs::write(&root, b"not a directory").expect("create blocker file");
    let err = Core::open(Some(root.clone()))
        .err()
        .expect("open should fail");
    assert_eq!(err.kind, ErrorKind::Io);
    fs::remove_file(&root).expect("cleanup");
}

#[test]
fn update_settings_persists_and_survives_reopen() {
    let root = scratch_dir("update");
    let core = Core::open(Some(root.clone())).expect("core opens");
    let mut changed = core.settings();
    changed.capture_retention_days = 14;
    core.update_settings(changed.clone())
        .expect("valid settings are accepted");
    assert_eq!(core.settings(), changed);
    drop(core);

    let reopened = Core::open(Some(root.clone())).expect("core reopens");
    assert_eq!(reopened.settings(), changed);
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn update_settings_rejects_invalid_values_and_keeps_current() {
    let root = scratch_dir("invalid");
    let core = Core::open(Some(root.clone())).expect("core opens");
    let mut invalid = core.settings();
    invalid.turn_timeout_secs = 1;
    let err = core
        .update_settings(invalid)
        .expect_err("out of range timeout is rejected");
    assert_eq!(err.kind, ErrorKind::Config);
    assert_eq!(core.settings(), Settings::default());
    assert!(!root.join("config.toml").exists());
    fs::remove_dir_all(&root).expect("cleanup");
}
