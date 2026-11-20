use super::{CaptureInfo, CaptureMode, Platform, read_png_dimensions};
use crate::error::{CoreError, ErrorKind};
use std::io::{self, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SCREENCAPTURE: &str = "/usr/sbin/screencapture";
const SIPS: &str = "/usr/bin/sips";
const MDFIND: &str = "/usr/bin/mdfind";
const DEFAULT_SHELL: &str = "/bin/zsh";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const READER_GRACE: Duration = Duration::from_millis(500);
const INTERACTIVE_CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);
const FULL_SCREEN_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);
const SIPS_TIMEOUT: Duration = Duration::from_secs(30);
const OUTPUT_CAP_BYTES: u64 = 1024 * 1024;
const STDERR_SUMMARY_CHARS: usize = 500;

pub struct MacPlatform;

struct CommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
) -> Result<mpsc::Receiver<Vec<u8>>, CoreError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("platform-pipe".to_string())
        .spawn(move || {
            let mut collected = Vec::new();
            let _ = reader
                .by_ref()
                .take(OUTPUT_CAP_BYTES)
                .read_to_end(&mut collected);
            let _ = sender.send(collected);
            let _ = io::copy(&mut reader, &mut io::sink());
        })
        .map_err(|err| {
            CoreError::new(ErrorKind::Io, format!("cannot start output reader: {err}"))
        })?;
    Ok(receiver)
}

fn kill_process_group(child: &mut Child) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn run_command(
    command: &mut Command,
    timeout: Duration,
    spawn_failure: ErrorKind,
) -> Result<CommandOutput, CoreError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|err| CoreError::new(spawn_failure, format!("cannot start {program}: {err}")))?;
    let readers = match (child.stdout.take(), child.stderr.take()) {
        (Some(stdout), Some(stderr)) => {
            spawn_reader(stdout).and_then(|out| spawn_reader(stderr).map(|err| (out, err)))
        }
        _ => Err(CoreError::new(
            ErrorKind::Internal,
            format!("{program} output pipes are missing"),
        )),
    };
    let (stdout_reader, stderr_reader) = match readers {
        Ok(pair) => pair,
        Err(err) => {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(err);
        }
    };
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => {
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(CoreError::new(
                    ErrorKind::Io,
                    format!("cannot wait for {program}: {err}"),
                ));
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            timed_out = true;
            kill_process_group(&mut child);
            match child.wait() {
                Ok(status) => break status,
                Err(err) => {
                    return Err(CoreError::new(
                        ErrorKind::Io,
                        format!("cannot reap {program} after timeout: {err}"),
                    ));
                }
            }
        }
        thread::sleep(POLL_INTERVAL.min(timeout - elapsed));
    };
    Ok(CommandOutput {
        status,
        stdout: stdout_reader.recv_timeout(READER_GRACE).unwrap_or_default(),
        stderr: stderr_reader.recv_timeout(READER_GRACE).unwrap_or_default(),
        timed_out,
    })
}

fn describe_exit(status: &ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => "unknown status".to_string(),
    }
}

fn stderr_summary(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();
    let skip = trimmed.chars().count().saturating_sub(STDERR_SUMMARY_CHARS);
    trimmed.chars().skip(skip).collect()
}

fn require_success(
    output: &CommandOutput,
    program: &str,
    kind: ErrorKind,
    timeout: Duration,
) -> Result<(), CoreError> {
    if output.timed_out {
        return Err(CoreError::new(
            kind,
            format!("{program} timed out after {} s", timeout.as_secs_f32()),
        ));
    }
    if !output.status.success() {
        return Err(CoreError::new(
            kind,
            format!(
                "{program} failed with {}: {}",
                describe_exit(&output.status),
                stderr_summary(&output.stderr)
            ),
        ));
    }
    Ok(())
}

pub fn probe_login_shell(shell: &Path, timeout: Duration) -> Result<String, CoreError> {
    let mut command = Command::new(shell);
    command.args(["-l", "-c", "printf %s \"$PATH\""]);
    let output = run_command(&mut command, timeout, ErrorKind::Io)?;
    require_success(
        &output,
        &shell.display().to_string(),
        ErrorKind::Io,
        timeout,
    )?;
    let text = String::from_utf8_lossy(&output.stdout);
    let path = text
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if path.is_empty() {
        return Err(CoreError::new(
            ErrorKind::Io,
            format!("{} printed an empty PATH", shell.display()),
        ));
    }
    Ok(path.to_string())
}

impl Platform for MacPlatform {
    fn capture(&self, mode: CaptureMode, dest: &Path) -> Result<Option<CaptureInfo>, CoreError> {
        if dest.exists() {
            return Err(CoreError::new(
                ErrorKind::Capture,
                format!("capture destination {} already exists", dest.display()),
            ));
        }
        let (flags, timeout): (&[&str], Duration) = match mode {
            CaptureMode::Region => (
                &["-i", "-x", "-o", "-t", "png"],
                INTERACTIVE_CAPTURE_TIMEOUT,
            ),
            CaptureMode::Window => (
                &["-i", "-W", "-x", "-o", "-t", "png"],
                INTERACTIVE_CAPTURE_TIMEOUT,
            ),
            CaptureMode::FullScreen => (&["-x", "-m", "-t", "png"], FULL_SCREEN_CAPTURE_TIMEOUT),
        };
        let mut command = Command::new(SCREENCAPTURE);
        command.args(flags).arg(dest);
        let output = run_command(&mut command, timeout, ErrorKind::Capture)?;
        require_success(&output, "screencapture", ErrorKind::Capture, timeout)?;
        if !dest.exists() {
            return Ok(None);
        }
        let (width, height) = read_png_dimensions(dest)?;
        let bytes = std::fs::metadata(dest)
            .map_err(|err| {
                CoreError::new(
                    ErrorKind::Capture,
                    format!("cannot stat {}: {err}", dest.display()),
                )
            })?
            .len();
        Ok(Some(CaptureInfo {
            path: dest.to_path_buf(),
            width,
            height,
            bytes,
        }))
    }

    fn downscale_png(&self, src: &Path, dest: &Path, max_edge: u32) -> Result<(), CoreError> {
        if max_edge == 0 {
            return Err(CoreError::new(
                ErrorKind::InvalidInput,
                "max_edge must be greater than zero",
            ));
        }
        let (width, height) = read_png_dimensions(src)?;
        if width.max(height) <= max_edge {
            if src != dest {
                std::fs::copy(src, dest).map_err(|err| {
                    CoreError::new(
                        ErrorKind::Io,
                        format!("cannot copy {} to {}: {err}", src.display(), dest.display()),
                    )
                })?;
            }
            return Ok(());
        }
        let mut command = Command::new(SIPS);
        command
            .arg("-Z")
            .arg(max_edge.to_string())
            .arg(src)
            .arg("--out")
            .arg(dest);
        let output = run_command(&mut command, SIPS_TIMEOUT, ErrorKind::Capture)?;
        require_success(&output, "sips", ErrorKind::Capture, SIPS_TIMEOUT)?;
        if !dest.exists() {
            return Err(CoreError::new(
                ErrorKind::Capture,
                format!("sips did not produce {}", dest.display()),
            ));
        }
        Ok(())
    }

    fn set_background_priority(&self) {
        let status = unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0)
        };
        if status != 0 {
            log::warn!("could not set background QoS class: error {status}");
        }
    }

    fn login_shell_path(&self, timeout: Duration) -> Result<String, CoreError> {
        let shell = std::env::var_os("SHELL")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SHELL));
        probe_login_shell(&shell, timeout)
    }

    fn spotlight_names(
        &self,
        query: &str,
        limit: usize,
        timeout: Duration,
    ) -> Result<Vec<PathBuf>, CoreError> {
        if query.trim().is_empty() {
            return Err(CoreError::new(
                ErrorKind::InvalidInput,
                "Spotlight query is empty",
            ));
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut command = Command::new(MDFIND);
        command.arg("-name").arg(query);
        let output = run_command(&mut command, timeout, ErrorKind::Io)?;
        require_success(&output, "mdfind", ErrorKind::Io, timeout)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let truncated = output.stdout.len() as u64 >= OUTPUT_CAP_BYTES;
        let complete = if truncated {
            text.rsplit_once('\n').map_or("", |(head, _)| head)
        } else {
            &text
        };
        Ok(complete
            .lines()
            .filter(|line| !line.is_empty())
            .take(limit)
            .map(PathBuf::from)
            .collect())
    }

    fn screen_capture_allowed(&self) -> Option<bool> {
        None
    }
}
