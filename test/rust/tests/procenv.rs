use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tern_core::error::{CoreError, ErrorKind};
use tern_core::platform::{CaptureInfo, CaptureMode, Platform};
use tern_core::procenv::ProcEnv;

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);
static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct FakePlatform {
    shell_path: Result<String, ()>,
}

impl Platform for FakePlatform {
    fn capture(&self, _mode: CaptureMode, _dest: &Path) -> Result<Option<CaptureInfo>, CoreError> {
        Ok(None)
    }

    fn downscale_png(&self, _src: &Path, _dest: &Path, _max_edge: u32) -> Result<(), CoreError> {
        Ok(())
    }

    fn set_background_priority(&self) {}

    fn login_shell_path(&self, _timeout: Duration) -> Result<String, CoreError> {
        self.shell_path
            .clone()
            .map_err(|()| CoreError::new(ErrorKind::Io, "shell failed"))
    }

    fn spotlight_names(
        &self,
        _query: &str,
        _limit: usize,
        _timeout: Duration,
    ) -> Result<Vec<PathBuf>, CoreError> {
        Ok(Vec::new())
    }

    fn screen_capture_allowed(&self) -> Option<bool> {
        None
    }
}

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tern-procenv-{label}-{}-{}",
        std::process::id(),
        SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write_file(dir: &Path, name: &str, body: &str, mode: u32) -> PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("create file");
    file.write_all(body.as_bytes()).expect("write file");
    drop(file);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    path
}

fn env_pair(key: &str, value: &str) -> (OsString, OsString) {
    (OsString::from(key), OsString::from(value))
}

fn value_of<'a>(env: &'a ProcEnv, key: &str) -> Option<&'a OsString> {
    env.base_env
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

#[test]
fn find_binary_finds_executable_in_first_path_dir() {
    let first = scratch_dir("first");
    let second = scratch_dir("second");
    let expected = write_file(&first, "claude", "#!/bin/sh\n", 0o755);
    write_file(&second, "claude", "#!/bin/sh\n", 0o755);
    let path = format!("{}:{}", first.display(), second.display());
    let env = ProcEnv::from_parts(path, &[], PathBuf::from("/nonexistent-home"));
    assert_eq!(env.find_binary("claude", None), Some(expected));
}

#[test]
fn find_binary_skips_non_executable_files() {
    let first = scratch_dir("noexec");
    let second = scratch_dir("exec");
    write_file(&first, "codex", "#!/bin/sh\n", 0o644);
    let expected = write_file(&second, "codex", "#!/bin/sh\n", 0o755);
    let path = format!("{}:{}", first.display(), second.display());
    let env = ProcEnv::from_parts(path, &[], PathBuf::from("/nonexistent-home"));
    assert_eq!(env.find_binary("codex", None), Some(expected));
}

#[test]
fn find_binary_skips_directories() {
    let dir = scratch_dir("dirs");
    std::fs::create_dir(dir.join("agy")).expect("mkdir");
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    assert_eq!(env.find_binary("agy", None), None);
}

#[test]
fn find_binary_returns_none_for_unknown_name() {
    let dir = scratch_dir("empty");
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    assert_eq!(env.find_binary("tern-no-such-binary-xyz", None), None);
}

#[test]
fn find_binary_rejects_names_with_separators() {
    let dir = scratch_dir("sep");
    write_file(&dir, "claude", "#!/bin/sh\n", 0o755);
    let env = ProcEnv::from_parts(
        dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    assert_eq!(env.find_binary("../claude", None), None);
    assert_eq!(env.find_binary("", None), None);
}

#[test]
fn find_binary_prefers_executable_override() {
    let path_dir = scratch_dir("path");
    let override_dir = scratch_dir("override");
    write_file(&path_dir, "claude", "#!/bin/sh\n", 0o755);
    let custom = write_file(&override_dir, "my-claude", "#!/bin/sh\n", 0o755);
    let env = ProcEnv::from_parts(
        path_dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    assert_eq!(env.find_binary("claude", Some(&custom)), Some(custom));
}

#[test]
fn find_binary_ignores_non_executable_override() {
    let path_dir = scratch_dir("path-fallback");
    let override_dir = scratch_dir("override-bad");
    let expected = write_file(&path_dir, "claude", "#!/bin/sh\n", 0o755);
    let bad = write_file(&override_dir, "my-claude", "#!/bin/sh\n", 0o644);
    let env = ProcEnv::from_parts(
        path_dir.display().to_string(),
        &[],
        PathBuf::from("/nonexistent-home"),
    );
    assert_eq!(env.find_binary("claude", Some(&bad)), Some(expected));
}

#[test]
fn find_binary_expands_tilde_in_override() {
    let home = scratch_dir("home-override");
    let custom = write_file(&home, "my-claude", "#!/bin/sh\n", 0o755);
    let env = ProcEnv::from_parts(String::new(), &[], home.clone());
    assert_eq!(
        env.find_binary("claude", Some(Path::new("~/my-claude"))),
        Some(custom)
    );
}

#[test]
fn find_binary_searches_home_relative_extra_dirs() {
    let home = scratch_dir("home-extra");
    let bin = home.join(".cargo/bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    let expected = write_file(&bin, "agy", "#!/bin/sh\n", 0o755);
    let env = ProcEnv::from_parts(String::new(), &[], home);
    assert_eq!(env.find_binary("agy", None), Some(expected));
}

#[test]
fn find_binary_prefers_path_over_extra_dirs() {
    let home = scratch_dir("home-order");
    let extra = home.join(".local/bin");
    std::fs::create_dir_all(&extra).expect("mkdir");
    write_file(&extra, "agy", "#!/bin/sh\n", 0o755);
    let path_dir = scratch_dir("path-order");
    let expected = write_file(&path_dir, "agy", "#!/bin/sh\n", 0o755);
    let env = ProcEnv::from_parts(path_dir.display().to_string(), &[], home);
    assert_eq!(env.find_binary("agy", None), Some(expected));
}

#[test]
fn from_parts_removes_duplicate_and_empty_path_entries() {
    let env = ProcEnv::from_parts("/a::/b:/a:/c:/b".to_string(), &[], PathBuf::from("/home/x"));
    assert_eq!(env.path, "/a:/b:/c");
    assert_eq!(value_of(&env, "PATH"), Some(&OsString::from("/a:/b:/c")));
}

#[test]
fn base_env_contains_only_allowlisted_keys() {
    let app_env = vec![
        env_pair("HOME", "/Users/app"),
        env_pair("USER", "tern"),
        env_pair("LOGNAME", "tern"),
        env_pair("SHELL", "/bin/zsh"),
        env_pair("LANG", "de_DE.UTF-8"),
        env_pair("TMPDIR", "/var/tmp/x/"),
        env_pair("PATH", "/ignored/app/path"),
        env_pair("TERM", "xterm-256color"),
        env_pair("CODEX_HOME", "/codex"),
        env_pair("CLAUDE_CONFIG_DIR", "/claude"),
        env_pair("XDG_CONFIG_HOME", "/xdg"),
        env_pair("HTTPS_PROXY", "http://proxy:1"),
        env_pair("HTTP_PROXY", "http://proxy:2"),
        env_pair("NO_PROXY", "localhost"),
        env_pair("OPENAI_API_KEY", "secret"),
        env_pair("ANTHROPIC_API_KEY", "secret"),
        env_pair("AWS_SECRET_ACCESS_KEY", "secret"),
    ];
    let env = ProcEnv::from_parts("/usr/bin".to_string(), &app_env, PathBuf::from("/h"));
    let mut keys: Vec<String> = env
        .base_env
        .iter()
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
            "HOME",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "LANG",
            "LOGNAME",
            "NO_COLOR",
            "NO_PROXY",
            "PATH",
            "SHELL",
            "TERM",
            "TMPDIR",
            "USER",
            "XDG_CONFIG_HOME",
        ]
    );
    assert_eq!(value_of(&env, "HOME"), Some(&OsString::from("/h")));
    assert_eq!(value_of(&env, "PATH"), Some(&OsString::from("/usr/bin")));
    assert_eq!(value_of(&env, "TERM"), Some(&OsString::from("dumb")));
    assert_eq!(value_of(&env, "NO_COLOR"), Some(&OsString::from("1")));
    assert_eq!(value_of(&env, "LANG"), Some(&OsString::from("de_DE.UTF-8")));
}

#[test]
fn base_env_defaults_lang_and_omits_missing_optionals() {
    let env = ProcEnv::from_parts("/usr/bin".to_string(), &[], PathBuf::from("/h"));
    assert_eq!(value_of(&env, "LANG"), Some(&OsString::from("en_US.UTF-8")));
    assert_eq!(value_of(&env, "USER"), None);
    assert_eq!(value_of(&env, "CODEX_HOME"), None);
}

#[test]
fn base_env_preserves_non_utf8_values() {
    let raw = OsString::from_vec(vec![b'/', b'x', 0xFF, 0xFE]);
    let app_env = vec![(OsString::from("CODEX_HOME"), raw.clone())];
    let env = ProcEnv::from_parts("/usr/bin".to_string(), &app_env, PathBuf::from("/h"));
    assert_eq!(value_of(&env, "CODEX_HOME"), Some(&raw));
}

#[test]
fn build_uses_login_shell_path_and_app_home() {
    let platform = FakePlatform {
        shell_path: Ok("/opt/tools:/usr/bin:/opt/tools".to_string()),
    };
    let app_env = vec![env_pair("HOME", "/Users/app")];
    let env = ProcEnv::build(&platform, &app_env);
    assert_eq!(env.path, "/opt/tools:/usr/bin");
    assert_eq!(env.home, PathBuf::from("/Users/app"));
}

#[test]
fn build_falls_back_when_login_shell_fails() {
    let platform = FakePlatform {
        shell_path: Err(()),
    };
    let env = ProcEnv::build(&platform, &[env_pair("HOME", "/Users/app")]);
    assert_eq!(
        env.path,
        "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"
    );
}

#[test]
fn build_falls_back_when_login_shell_path_is_blank() {
    let platform = FakePlatform {
        shell_path: Ok("  ".to_string()),
    };
    let env = ProcEnv::build(&platform, &[]);
    assert!(env.path.starts_with("/usr/bin:/bin"));
}

#[test]
fn run_version_returns_first_trimmed_stdout_line() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("version");
    let script = write_file(
        &dir,
        "tool",
        "#!/bin/sh\nprintf '  tool 1.2.3  \\nsecond line\\n'\n",
        0o755,
    );
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let version = env
        .run_version(&script, &["--version"], Duration::from_secs(5))
        .expect("version");
    assert_eq!(version, "tool 1.2.3");
}

#[test]
fn run_version_passes_args_and_scrubbed_env() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("env");
    let script = write_file(
        &dir,
        "tool",
        "#!/bin/sh\nprintf '%s|%s|%s|%s' \"$1\" \"$TERM\" \"${TERN_SECRET:-unset}\" \"$NO_COLOR\"\n",
        0o755,
    );
    let app_env = vec![env_pair("TERN_SECRET", "leak")];
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &app_env, PathBuf::from("/h"));
    let version = env
        .run_version(&script, &["-v"], Duration::from_secs(5))
        .expect("version");
    assert_eq!(version, "-v|dumb|unset|1");
}

#[test]
fn run_version_gives_the_child_null_stdin() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("stdin");
    let script = write_file(&dir, "tool", "#!/bin/sh\ncat\necho done\n", 0o755);
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let version = env
        .run_version(&script, &[], Duration::from_secs(5))
        .expect("version");
    assert_eq!(version, "done");
}

#[test]
fn run_version_reports_missing_binary() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("missing");
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let err = env
        .run_version(&dir.join("absent"), &["--version"], Duration::from_secs(5))
        .expect_err("missing binary");
    assert_eq!(err.kind, ErrorKind::ProviderNotInstalled);
}

#[test]
fn run_version_reports_non_zero_exit() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("fail");
    let script = write_file(&dir, "tool", "#!/bin/sh\necho oops\nexit 3\n", 0o755);
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let err = env
        .run_version(&script, &["--version"], Duration::from_secs(5))
        .expect_err("failure");
    assert_eq!(err.kind, ErrorKind::Provider);
    assert!(err.message.contains("exit status: 3"), "{}", err.message);
}

#[test]
fn run_version_returns_empty_string_for_silent_binary() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("silent");
    let script = write_file(&dir, "tool", "#!/bin/sh\nexit 0\n", 0o755);
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let version = env
        .run_version(&script, &[], Duration::from_secs(5))
        .expect("version");
    assert_eq!(version, "");
}

#[test]
fn run_version_times_out_and_kills_process_group() {
    let _spawn_guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch_dir("hang");
    let script = write_file(&dir, "tool", "#!/bin/sh\nsleep 30 &\nsleep 30\n", 0o755);
    let env = ProcEnv::from_parts("/usr/bin:/bin".to_string(), &[], PathBuf::from("/h"));
    let started = Instant::now();
    let err = env
        .run_version(&script, &[], Duration::from_millis(300))
        .expect_err("timeout");
    assert_eq!(err.kind, ErrorKind::Provider);
    assert!(err.message.contains("timed out"), "{}", err.message);
    assert!(started.elapsed() < Duration::from_secs(5));
}
