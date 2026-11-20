use crate::error::{CoreError, ErrorKind};
use crate::platform::Platform;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);
const FALLBACK_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin";
const DEFAULT_LANG: &str = "en_US.UTF-8";
const EXTRA_DIRS: [&str; 7] = [
    "~/.local/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.npm-global/bin",
    "~/.bun/bin",
    "~/.volta/bin",
    "~/.cargo/bin",
];
const FROM_APP: [&str; 4] = ["USER", "LOGNAME", "SHELL", "TMPDIR"];
const PASSTHROUGH: [&str; 6] = [
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "XDG_CONFIG_HOME",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
];
const VERSION_OUTPUT_CAP: u64 = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const READER_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct ProcEnv {
    pub path: String,
    pub base_env: Vec<(OsString, OsString)>,
    pub home: PathBuf,
}

fn lookup<'a>(env: &'a [(OsString, OsString)], key: &str) -> Option<&'a OsString> {
    env.iter()
        .rev()
        .find(|(name, value)| name == OsStr::new(key) && !value.is_empty())
        .map(|(_, value)| value)
}

fn dedupe_path(path: &str) -> String {
    let mut seen = HashSet::new();
    path.split(':')
        .filter(|entry| !entry.is_empty() && seen.insert(*entry))
        .collect::<Vec<_>>()
        .join(":")
}

fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    let bytes = path.as_os_str().as_bytes();
    if bytes == b"~" {
        return home.to_path_buf();
    }
    match bytes.strip_prefix(b"~/") {
        Some(rest) => home.join(OsStr::from_bytes(rest)),
        None => path.to_path_buf(),
    }
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn kill_process_group(child: &mut Child) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn spawn_stdout_reader<R: Read + Send + 'static>(
    mut reader: R,
) -> Result<mpsc::Receiver<Vec<u8>>, CoreError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("procenv-pipe".to_string())
        .spawn(move || {
            let mut collected = Vec::new();
            let _ = reader
                .by_ref()
                .take(VERSION_OUTPUT_CAP)
                .read_to_end(&mut collected);
            let _ = sender.send(collected);
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        })
        .map_err(|err| {
            CoreError::new(ErrorKind::Io, format!("cannot start output reader: {err}"))
        })?;
    Ok(receiver)
}

impl ProcEnv {
    pub fn build(platform: &dyn Platform, app_env: &[(OsString, OsString)]) -> ProcEnv {
        let path = match platform.login_shell_path(LOGIN_SHELL_TIMEOUT) {
            Ok(path) if !path.trim().is_empty() => path,
            Ok(_) => {
                log::warn!("login shell reported an empty PATH, using fallback PATH");
                FALLBACK_PATH.to_string()
            }
            Err(err) => {
                log::warn!("login shell PATH lookup failed ({err}), using fallback PATH");
                FALLBACK_PATH.to_string()
            }
        };
        let home = lookup(app_env, "HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                log::warn!("HOME is not set in the app environment");
                PathBuf::from("/")
            });
        ProcEnv::from_parts(path, app_env, home)
    }

    pub fn from_parts(path: String, app_env: &[(OsString, OsString)], home: PathBuf) -> ProcEnv {
        let path = dedupe_path(&path);
        let mut base_env: Vec<(OsString, OsString)> = Vec::new();
        base_env.push((OsString::from("HOME"), home.clone().into_os_string()));
        for key in FROM_APP {
            if let Some(value) = lookup(app_env, key) {
                base_env.push((OsString::from(key), value.clone()));
            }
        }
        let lang = lookup(app_env, "LANG")
            .cloned()
            .unwrap_or_else(|| OsString::from(DEFAULT_LANG));
        base_env.push((OsString::from("LANG"), lang));
        base_env.push((OsString::from("PATH"), OsString::from(&path)));
        base_env.push((OsString::from("TERM"), OsString::from("dumb")));
        base_env.push((OsString::from("NO_COLOR"), OsString::from("1")));
        for key in PASSTHROUGH {
            if let Some(value) = lookup(app_env, key) {
                base_env.push((OsString::from(key), value.clone()));
            }
        }
        ProcEnv {
            path,
            base_env,
            home,
        }
    }

    pub fn find_binary(&self, name: &str, override_path: Option<&Path>) -> Option<PathBuf> {
        if let Some(candidate) = override_path {
            let expanded = expand_tilde(candidate, &self.home);
            if is_executable_file(&expanded) {
                return Some(expanded);
            }
            log::warn!(
                "configured binary path {} is not an executable file, ignoring it",
                expanded.display()
            );
        }
        if name.is_empty() || name.contains('/') || name.contains('\0') {
            return None;
        }
        let path_dirs = self
            .path
            .split(':')
            .filter(|entry| !entry.is_empty())
            .map(|entry| expand_tilde(Path::new(entry), &self.home));
        let extra_dirs = EXTRA_DIRS
            .iter()
            .map(|entry| expand_tilde(Path::new(entry), &self.home));
        let mut seen = HashSet::new();
        path_dirs
            .chain(extra_dirs)
            .filter(|dir| seen.insert(dir.clone()))
            .map(|dir| dir.join(name))
            .find(|candidate| is_executable_file(candidate))
    }

    pub fn run_version(
        &self,
        binary: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, CoreError> {
        let mut command = Command::new(binary);
        command
            .args(args)
            .env_clear()
            .envs(self.base_env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let display = binary.display().to_string();
        let mut child = command.spawn().map_err(|err| {
            let kind = if err.kind() == std::io::ErrorKind::NotFound {
                ErrorKind::ProviderNotInstalled
            } else {
                ErrorKind::Provider
            };
            CoreError::new(kind, format!("cannot start {display}: {err}"))
        })?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(CoreError::new(
                    ErrorKind::Internal,
                    format!("{display} stdout pipe is missing"),
                ));
            }
        };
        let reader = match spawn_stdout_reader(stdout) {
            Ok(reader) => reader,
            Err(err) => {
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(err);
            }
        };
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(err) => {
                    kill_process_group(&mut child);
                    let _ = child.wait();
                    return Err(CoreError::new(
                        ErrorKind::Provider,
                        format!("cannot wait for {display}: {err}"),
                    ));
                }
            }
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(CoreError::new(
                    ErrorKind::Provider,
                    format!("{display} timed out after {} s", timeout.as_secs_f32()),
                ));
            }
            thread::sleep(POLL_INTERVAL.min(timeout - elapsed));
        };
        if !status.success() {
            return Err(CoreError::new(
                ErrorKind::Provider,
                format!("{display} exited with {status}"),
            ));
        }
        let bytes = reader.recv_timeout(READER_GRACE).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.lines().next().unwrap_or_default().trim().to_string())
    }
}
