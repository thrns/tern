use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tern_core::error::ErrorKind;
use tern_core::paths::{TernPaths, validate_id};

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!("tern-paths-{name}-{}-{nanos}", std::process::id()))
}

#[test]
fn override_home_wins_and_derives_every_path() {
    let root = scratch_dir("derive");
    let paths = TernPaths::resolve(Some(&root)).expect("override resolves");
    assert_eq!(paths.home, root);
    assert_eq!(paths.config_file(), root.join("config.toml"));
    assert_eq!(paths.index_db(), root.join("index.db"));
    assert_eq!(paths.state_db(), root.join("state.db"));
    assert_eq!(paths.captures_dir(), root.join("captures"));
    assert_eq!(paths.workspaces_dir(), root.join("workspaces"));
    assert_eq!(paths.workspace_dir("abc-1"), root.join("workspaces/abc-1"));
    assert_eq!(paths.prompts_dir(), root.join("prompts"));
    assert_eq!(paths.system_prompt_file(), root.join("prompts/system.md"));
    assert_eq!(paths.run_dir(), root.join("run"));
    assert_eq!(paths.logs_dir(), root.join("logs"));
    assert_eq!(paths.app_log_file(), root.join("logs/tern.log"));
    assert_eq!(paths.mcp_log_file(), root.join("logs/tern-mcp.log"));
}

#[test]
fn environment_and_home_resolution_order() {
    let original_tern_home = env::var_os("TERN_HOME");
    let original_home = env::var_os("HOME");
    let env_home = scratch_dir("env");
    let user_home = scratch_dir("userhome");

    unsafe {
        env::set_var("TERN_HOME", &env_home);
        env::set_var("HOME", &user_home);
    }
    let from_env = TernPaths::resolve(None).expect("TERN_HOME resolves");
    assert_eq!(from_env.home, env_home);

    let overridden = TernPaths::resolve(Some(Path::new("/explicit/override"))).expect("override");
    assert_eq!(overridden.home, PathBuf::from("/explicit/override"));

    unsafe {
        env::remove_var("TERN_HOME");
    }
    let from_home = TernPaths::resolve(None).expect("HOME resolves");
    assert_eq!(
        from_home.home,
        user_home.join("Library/Application Support/Tern")
    );

    unsafe {
        env::remove_var("HOME");
    }
    let missing = TernPaths::resolve(None).expect_err("missing HOME fails");
    assert_eq!(missing.kind, ErrorKind::Config);

    unsafe {
        match original_tern_home {
            Some(value) => env::set_var("TERN_HOME", value),
            None => env::remove_var("TERN_HOME"),
        }
        match original_home {
            Some(value) => env::set_var("HOME", value),
            None => env::remove_var("HOME"),
        }
    }
}

#[test]
fn ensure_dirs_creates_private_tree() {
    let root = scratch_dir("tree");
    let paths = TernPaths::resolve(Some(&root)).expect("override resolves");
    paths.ensure_dirs().expect("tree is created");
    for dir in [
        paths.home.clone(),
        paths.captures_dir(),
        paths.workspaces_dir(),
        paths.prompts_dir(),
        paths.logs_dir(),
        paths.run_dir(),
    ] {
        assert!(dir.is_dir(), "{} should exist", dir.display());
        let mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{} should be mode 0700", dir.display());
    }
    paths.ensure_dirs().expect("second call is a no-op");
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn unwritable_home_reports_io_error_with_path() {
    let root = scratch_dir("blocked");
    fs::write(&root, b"not a directory").expect("create blocker file");
    let paths = TernPaths::resolve(Some(&root)).expect("override resolves");
    let err = paths.ensure_dirs().expect_err("home is a file");
    assert_eq!(err.kind, ErrorKind::Io);
    assert!(
        err.message.contains(&root.display().to_string()),
        "message should name the path: {}",
        err.message
    );
    fs::remove_file(&root).expect("cleanup");
}

#[test]
fn short_home_uses_run_socket() {
    let paths = TernPaths::resolve(Some(Path::new("/tmp/tern-short"))).expect("override resolves");
    assert_eq!(paths.socket_path(), PathBuf::from("/tmp/tern-short/run/s"));
}

#[test]
fn long_home_falls_back_to_temp_socket() {
    let long_name = "h".repeat(120);
    let root = scratch_dir(&long_name);
    let paths = TernPaths::resolve(Some(&root)).expect("override resolves");
    let socket = paths.socket_path();
    assert!(
        socket.starts_with(env::temp_dir()),
        "socket should live under the temp dir: {}",
        socket.display()
    );
    assert_eq!(socket.file_name().and_then(|name| name.to_str()), Some("s"));
    let parent_name = socket
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    assert!(parent_name.starts_with("tern-"), "parent was {parent_name}");
    assert!(socket.as_os_str().len() <= 104);

    paths
        .ensure_dirs()
        .expect("tree and fallback dir are created");
    let parent = socket.parent().expect("socket has parent");
    assert!(parent.is_dir());
    let mode = fs::metadata(parent).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    fs::remove_dir_all(parent).expect("cleanup fallback dir");
    fs::remove_dir_all(&root).expect("cleanup home");
}

#[test]
fn validate_id_accepts_documented_shapes() {
    assert!(validate_id("a").is_ok());
    assert!(validate_id("conv-2024-ABC").is_ok());
    assert!(validate_id(&"x".repeat(64)).is_ok());
}

#[test]
fn validate_id_rejects_unsafe_shapes() {
    let rejected = [
        String::new(),
        "x".repeat(65),
        "a/b".to_owned(),
        "..".to_owned(),
        "../etc".to_owned(),
        "a.b".to_owned(),
        "a b".to_owned(),
        "naïve".to_owned(),
    ];
    for id in &rejected {
        let err = validate_id(id).expect_err("id should be rejected");
        assert_eq!(err.kind, ErrorKind::InvalidInput, "id {id:?}");
    }
}
