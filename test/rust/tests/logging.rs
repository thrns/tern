use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use log::LevelFilter;
use tern_core::logging::{FileLogger, level_from_env};

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!(
        "tern-logging-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn numbered(path: &Path, index: u32) -> PathBuf {
    PathBuf::from(format!("{}.{index}", path.display()))
}

fn assert_timestamp_shape(stamp: &str) {
    let bytes = stamp.as_bytes();
    assert_eq!(bytes.len(), 24, "timestamp was {stamp}");
    assert_eq!(bytes[4], b'-');
    assert_eq!(bytes[7], b'-');
    assert_eq!(bytes[10], b'T');
    assert_eq!(bytes[13], b':');
    assert_eq!(bytes[16], b':');
    assert_eq!(bytes[19], b'.');
    assert_eq!(bytes[23], b'Z');
    assert!(stamp.chars().filter(|c| c.is_ascii_digit()).count() == 17);
}

#[test]
fn file_logger_behaviour() {
    let original_tern_log = env::var_os("TERN_LOG");
    assert_eq!(level_from_env(), LevelFilter::Info);
    unsafe {
        env::set_var("TERN_LOG", "DEBUG");
    }
    assert_eq!(level_from_env(), LevelFilter::Debug);
    unsafe {
        match original_tern_log {
            Some(value) => env::set_var("TERN_LOG", value),
            None => env::remove_var("TERN_LOG"),
        }
    }

    let dir = scratch_dir("main");
    let log_path = dir.join("tern.log");
    let other_path = dir.join("other.log");

    FileLogger::init(&log_path, LevelFilter::Info).expect("first init succeeds");
    FileLogger::init(&other_path, LevelFilter::Debug).expect("second init returns Ok");
    assert!(
        !other_path.exists(),
        "second init must not replace the sink"
    );

    log::info!(target: "tern::probe", "shown line {}", 42);
    log::debug!("hidden debug line");

    let contents = fs::read_to_string(&log_path).expect("log readable");
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 1, "debug line must be filtered: {contents}");
    let line = lines[0];
    assert_timestamp_shape(&line[..24]);
    assert_eq!(&line[24..], " INFO tern::probe: shown line 42");

    let filler = "x".repeat(900);
    for index in 0..1_200 {
        log::info!("{index} {filler}");
    }
    assert!(
        numbered(&log_path, 1).exists(),
        "first rotation should create .1"
    );
    assert!(
        !numbered(&log_path, 2).exists(),
        "no .2 before a second rotation"
    );
    let rotated = fs::metadata(numbered(&log_path, 1))
        .expect("rotated metadata")
        .len();
    assert!(rotated > 1024 * 1024, "rotated file was {rotated} bytes");
    let current = fs::metadata(&log_path).expect("current metadata").len();
    assert!(current < rotated, "current file restarted after rotation");

    for index in 0..1_200 {
        log::info!("second {index} {filler}");
    }
    assert!(
        numbered(&log_path, 2).exists(),
        "second rotation should create .2"
    );
    assert!(numbered(&log_path, 1).exists());

    fs::remove_dir_all(&dir).expect("cleanup");
}
