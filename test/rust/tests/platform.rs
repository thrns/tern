use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tern_core::error::ErrorKind;
use tern_core::platform::{self, CaptureMode, png_dimensions, read_png_dimensions};

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tern-platform-{label}-{}-{}",
        std::process::id(),
        SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

fn gray_png(width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in 0..height {
        raw.push(0);
        for column in 0..width {
            raw.push(((row + column) % 256) as u8);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let mut remaining = raw.as_slice();
    while !remaining.is_empty() {
        let take = remaining.len().min(65535);
        let (block, rest) = remaining.split_at(take);
        zlib.push(u8::from(rest.is_empty()));
        zlib.extend_from_slice(&(take as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(take as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
        remaining = rest;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

#[cfg(target_os = "macos")]
fn write_shell_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod script");
    path
}

#[test]
fn png_dimensions_reads_big_endian_ihdr() {
    let png = gray_png(300, 120);
    assert_eq!(png_dimensions(&png).expect("dimensions"), (300, 120));
}

#[test]
fn png_dimensions_rejects_short_data() {
    let png = gray_png(10, 10);
    let error = png_dimensions(&png[..23]).expect_err("short header");
    assert_eq!(error.kind, ErrorKind::Capture);
}

#[test]
fn png_dimensions_rejects_missing_signature() {
    let mut png = gray_png(10, 10);
    png[1] = b'X';
    let error = png_dimensions(&png).expect_err("bad signature");
    assert_eq!(error.kind, ErrorKind::Capture);
}

#[test]
fn read_png_dimensions_reports_missing_file() {
    let dir = scratch_dir("missing");
    let error = read_png_dimensions(&dir.join("absent.png")).expect_err("missing file");
    assert_eq!(error.kind, ErrorKind::Capture);
}

#[test]
fn read_png_dimensions_reads_file_header() {
    let dir = scratch_dir("header");
    let path = dir.join("a.png");
    std::fs::write(&path, gray_png(64, 32)).expect("write png");
    assert_eq!(read_png_dimensions(&path).expect("dimensions"), (64, 32));
}

#[test]
fn screen_capture_allowed_is_unknown_from_rust() {
    assert_eq!(platform::current().screen_capture_allowed(), None);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_platform_reports_unsupported() {
    let platform = platform::current();
    let dest = Path::new("/tmp/tern-unsupported.png");
    let timeout = Duration::from_secs(1);
    assert_eq!(
        platform
            .capture(CaptureMode::Region, dest)
            .expect_err("capture")
            .kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        platform
            .downscale_png(dest, dest, 100)
            .expect_err("downscale")
            .kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        platform.login_shell_path(timeout).expect_err("shell").kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        platform
            .spotlight_names("a", 1, timeout)
            .expect_err("spotlight")
            .kind,
        ErrorKind::Unsupported
    );
    platform.set_background_priority();
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn login_shell_path_contains_usr_bin() {
    let path = platform::current()
        .login_shell_path(Duration::from_secs(10))
        .expect("login shell path");
    assert!(path.split(':').any(|entry| entry == "/usr/bin"), "{path}");
}

#[cfg(target_os = "macos")]
#[test]
fn probe_login_shell_takes_last_line_after_banner() {
    let dir = scratch_dir("banner");
    let shell = write_shell_script(
        &dir,
        "banner-shell",
        "echo welcome back\necho another banner\nprintf %s /opt/one:/usr/bin",
    );
    let path = tern_core::platform::macos::probe_login_shell(&shell, Duration::from_secs(5))
        .expect("path");
    assert_eq!(path, "/opt/one:/usr/bin");
}

#[cfg(target_os = "macos")]
#[test]
fn probe_login_shell_errors_on_empty_output() {
    let dir = scratch_dir("empty");
    let shell = write_shell_script(&dir, "empty-shell", "printf ''");
    let error = tern_core::platform::macos::probe_login_shell(&shell, Duration::from_secs(5))
        .expect_err("empty path");
    assert_eq!(error.kind, ErrorKind::Io);
}

#[cfg(target_os = "macos")]
#[test]
fn probe_login_shell_errors_on_failure_exit() {
    let dir = scratch_dir("failing");
    let shell = write_shell_script(&dir, "failing-shell", "echo /usr/bin\nexit 3");
    let error = tern_core::platform::macos::probe_login_shell(&shell, Duration::from_secs(5))
        .expect_err("failure exit");
    assert_eq!(error.kind, ErrorKind::Io);
}

#[cfg(target_os = "macos")]
#[test]
fn probe_login_shell_kills_hung_shell_process_group() {
    let dir = scratch_dir("hang");
    let marker = dir.join("grandchild.alive");
    let shell = write_shell_script(
        &dir,
        "hung-shell",
        &format!(
            "(sleep 2; touch '{}') &\nsleep 30\nprintf %s /usr/bin",
            marker.display()
        ),
    );
    let started = Instant::now();
    let error = tern_core::platform::macos::probe_login_shell(&shell, Duration::from_millis(300))
        .expect_err("timeout");
    assert_eq!(error.kind, ErrorKind::Io);
    assert!(started.elapsed() < Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(2500));
    assert!(!marker.exists(), "background job survived the timeout");
}

#[cfg(target_os = "macos")]
#[test]
fn probe_login_shell_reports_missing_shell() {
    let error = tern_core::platform::macos::probe_login_shell(
        Path::new("/nonexistent/tern-shell"),
        Duration::from_secs(1),
    )
    .expect_err("missing shell");
    assert_eq!(error.kind, ErrorKind::Io);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn downscale_png_limits_longest_edge() {
    let dir = scratch_dir("downscale");
    let src = dir.join("big.png");
    let dest = dir.join("small.png");
    std::fs::write(&src, gray_png(400, 200)).expect("write png");
    platform::current()
        .downscale_png(&src, &dest, 100)
        .expect("downscale");
    let (width, height) = read_png_dimensions(&dest).expect("dimensions");
    assert!(width.max(height) <= 100, "{width}x{height}");
    assert_eq!(width.max(height), 100);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn downscale_png_copies_images_already_within_limit() {
    let dir = scratch_dir("copy");
    let src = dir.join("small.png");
    let dest = dir.join("copy.png");
    let bytes = gray_png(80, 40);
    std::fs::write(&src, &bytes).expect("write png");
    platform::current()
        .downscale_png(&src, &dest, 100)
        .expect("downscale");
    assert_eq!(std::fs::read(&dest).expect("read copy"), bytes);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn downscale_png_rejects_zero_edge() {
    let dir = scratch_dir("zero");
    let src = dir.join("a.png");
    std::fs::write(&src, gray_png(8, 8)).expect("write png");
    let error = platform::current()
        .downscale_png(&src, &dir.join("b.png"), 0)
        .expect_err("zero edge");
    assert_eq!(error.kind, ErrorKind::InvalidInput);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn downscale_png_rejects_truncated_source() {
    let dir = scratch_dir("truncated");
    let src = dir.join("short.png");
    std::fs::write(&src, [0x89, b'P', b'N', b'G']).expect("write png");
    let error = platform::current()
        .downscale_png(&src, &dir.join("out.png"), 50)
        .expect_err("short png");
    assert_eq!(error.kind, ErrorKind::Capture);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn capture_refuses_existing_destination() {
    let dir = scratch_dir("capture");
    let dest = dir.join("cap.png");
    std::fs::write(&dest, b"occupied").expect("write file");
    let error = platform::current()
        .capture(CaptureMode::FullScreen, &dest)
        .expect_err("existing destination");
    assert_eq!(error.kind, ErrorKind::Capture);
    assert_eq!(std::fs::read(&dest).expect("read"), b"occupied");
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn spotlight_names_rejects_blank_query() {
    let error = platform::current()
        .spotlight_names("   ", 5, Duration::from_secs(5))
        .expect_err("blank query");
    assert_eq!(error.kind, ErrorKind::InvalidInput);
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn spotlight_names_honours_limit() {
    let platform = platform::current();
    assert!(
        platform
            .spotlight_names("Safari", 0, Duration::from_secs(5))
            .expect("zero limit")
            .is_empty()
    );
    let hits = platform
        .spotlight_names("Safari", 3, Duration::from_secs(20))
        .expect("spotlight");
    assert!(hits.len() <= 3);
    let unmatched = platform
        .spotlight_names("zzqqxx-no-such-file-name", 3, Duration::from_secs(20))
        .expect("spotlight");
    assert!(unmatched.is_empty());
}

#[cfg_attr(not(target_os = "macos"), ignore)]
#[test]
fn set_background_priority_runs_on_worker_thread() {
    let handle = std::thread::spawn(|| platform::current().set_background_priority());
    handle.join().expect("worker thread");
}
