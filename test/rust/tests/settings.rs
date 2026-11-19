use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tern_core::error::ErrorKind;
use tern_core::settings::{AiMode, Modifier, ProviderSettings, Settings, parse_hotkey};

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!(
        "tern-settings-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn validation_kind(settings: &Settings) -> Option<ErrorKind> {
    settings.validate().err().map(|err| err.kind)
}

fn validation_kind_with_home(settings: &Settings, home: &Path) -> Option<ErrorKind> {
    settings.validate_with_home(home).err().map(|err| err.kind)
}

fn broken_files_in(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .expect("read scratch dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("config.toml.broken-"))
        })
        .collect()
}

#[test]
fn default_settings_match_architecture() {
    let settings = Settings::default();
    assert_eq!(settings.hotkey_panel, "alt+space");
    assert_eq!(settings.hotkey_capture_ask, "alt+shift+space");
    assert_eq!(settings.default_provider, "claude");
    assert_eq!(settings.ai_mode, AiMode::ReadOnly);
    assert_eq!(settings.work_dir, None);
    assert!(!settings.allow_ai_screenshots);
    assert!(settings.index_roots.is_empty());
    assert!(settings.index_exclude_globs.is_empty());
    assert_eq!(settings.max_content_file_bytes, 2_097_152);
    assert_eq!(settings.capture_retention_days, 7);
    assert_eq!(settings.turn_timeout_secs, 600);
    assert!(!settings.launch_at_login);
    assert_eq!(settings.provider("claude").map(|p| p.enabled), Some(true));
    assert_eq!(settings.provider("codex").map(|p| p.enabled), Some(true));
    assert_eq!(settings.provider("agy").map(|p| p.enabled), Some(true));
    assert_eq!(settings.provider("gemini").map(|p| p.enabled), Some(false));
    assert!(settings.provider("missing").is_none());
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn default_round_trip_is_equal() {
    let dir = scratch_dir("default-round-trip");
    let path = dir.join("config.toml");
    let settings = Settings::default();
    settings.save(&path).expect("save defaults");
    let (loaded, warning) = Settings::load(&path).expect("load defaults");
    assert_eq!(loaded, settings);
    assert_eq!(warning, None);
}

#[test]
fn customised_round_trip_is_equal() {
    let dir = scratch_dir("custom-round-trip");
    let path = dir.join("config.toml");
    let work = dir.join("work");
    fs::create_dir_all(&work).expect("work dir");
    let settings = Settings {
        hotkey_panel: "cmd+shift+k".to_string(),
        hotkey_capture_ask: "ctrl+f12".to_string(),
        default_provider: "codex".to_string(),
        providers: vec![
            ProviderSettings {
                id: "claude".to_string(),
                enabled: true,
                binary_path: Some("/opt/homebrew/bin/claude".to_string()),
                model: Some("sonnet".to_string()),
            },
            ProviderSettings {
                id: "codex".to_string(),
                enabled: true,
                binary_path: None,
                model: None,
            },
        ],
        ai_mode: AiMode::AllowEdits,
        work_dir: Some(work.display().to_string()),
        allow_ai_screenshots: true,
        index_roots: Vec::new(),
        index_exclude_globs: vec!["**/tmp/**".to_string()],
        max_content_file_bytes: 4_194_304,
        capture_retention_days: 30,
        turn_timeout_secs: 900,
        launch_at_login: true,
    };
    settings.validate().expect("customised settings are valid");
    settings.save(&path).expect("save custom");
    let (loaded, warning) = Settings::load(&path).expect("load custom");
    assert_eq!(loaded, settings);
    assert_eq!(warning, None);
}

#[test]
fn ai_mode_serializes_in_snake_case() {
    let dir = scratch_dir("ai-mode");
    let path = dir.join("config.toml");
    let settings = Settings {
        ai_mode: AiMode::AllowEdits,
        work_dir: Some(dir.display().to_string()),
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    let text = fs::read_to_string(&path).expect("read config");
    assert!(text.contains("ai_mode = \"allow_edits\""));
    let read_only = Settings::default();
    read_only.save(&path).expect("save read only");
    let text = fs::read_to_string(&path).expect("read config");
    assert!(text.contains("ai_mode = \"read_only\""));
}

#[test]
fn missing_file_loads_defaults_without_warning() {
    let dir = scratch_dir("missing");
    let path = dir.join("config.toml");
    let (loaded, warning) = Settings::load(&path).expect("missing file loads");
    assert_eq!(loaded, Settings::default());
    assert_eq!(warning, None);
}

#[test]
fn partial_file_fills_missing_keys_and_ignores_unknown_keys() {
    let dir = scratch_dir("partial");
    let path = dir.join("config.toml");
    fs::write(&path, "turn_timeout_secs = 90\nunknown_future_key = true\n").expect("write partial");
    let (loaded, warning) = Settings::load(&path).expect("partial loads");
    assert_eq!(loaded.turn_timeout_secs, 90);
    assert_eq!(loaded.default_provider, "claude");
    assert_eq!(loaded.capture_retention_days, 7);
    assert_eq!(warning, None);
}

#[test]
fn broken_file_is_moved_aside_and_defaults_load_with_warning() {
    let dir = scratch_dir("broken");
    let path = dir.join("config.toml");
    fs::write(&path, "this is = not [valid toml").expect("write broken");
    let (loaded, warning) = Settings::load(&path).expect("broken file still loads");
    assert_eq!(loaded, Settings::default());
    let warning = warning.expect("warning for broken file");
    assert!(warning.contains("config.toml.broken-"));
    assert!(!path.exists());
    assert_eq!(broken_files_in(&dir).len(), 1);
}

#[test]
fn non_utf8_file_is_treated_as_broken() {
    let dir = scratch_dir("non-utf8");
    let path = dir.join("config.toml");
    fs::write(&path, [0xff_u8, 0xfe, 0x00, 0x01]).expect("write bytes");
    let (loaded, warning) = Settings::load(&path).expect("non utf8 still loads");
    assert_eq!(loaded, Settings::default());
    assert!(warning.is_some());
    assert!(!path.exists());
}

#[test]
fn save_leaves_no_temporary_file_behind() {
    let dir = scratch_dir("atomic");
    let path = dir.join("config.toml");
    Settings::default().save(&path).expect("save");
    assert!(path.exists());
    assert!(!dir.join("config.toml.tmp").exists());
}

#[test]
fn save_replaces_existing_file() {
    let dir = scratch_dir("replace");
    let path = dir.join("config.toml");
    Settings::default().save(&path).expect("first save");
    let changed = Settings {
        turn_timeout_secs: 120,
        ..Settings::default()
    };
    changed.save(&path).expect("second save");
    let (loaded, _) = Settings::load(&path).expect("load");
    assert_eq!(loaded.turn_timeout_secs, 120);
}

#[test]
fn duplicate_provider_id_is_rejected() {
    let mut settings = Settings::default();
    settings.providers.push(ProviderSettings {
        id: "claude".to_string(),
        enabled: true,
        binary_path: None,
        model: None,
    });
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn unknown_default_provider_is_rejected() {
    let settings = Settings {
        default_provider: "nonexistent".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn disabled_default_provider_is_rejected() {
    let settings = Settings {
        default_provider: "gemini".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn enabled_default_provider_is_accepted() {
    let settings = Settings {
        default_provider: "agy".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn allow_edits_without_work_dir_is_rejected() {
    let settings = Settings {
        ai_mode: AiMode::AllowEdits,
        work_dir: None,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn allow_edits_with_missing_work_dir_is_rejected() {
    let dir = scratch_dir("missing-work");
    let settings = Settings {
        ai_mode: AiMode::AllowEdits,
        work_dir: Some(dir.join("does-not-exist").display().to_string()),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn allow_edits_with_existing_work_dir_is_accepted() {
    let dir = scratch_dir("existing-work");
    let settings = Settings {
        ai_mode: AiMode::AllowEdits,
        work_dir: Some(dir.display().to_string()),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn relative_index_root_is_rejected() {
    let settings = Settings {
        index_roots: vec!["Documents".to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn missing_index_root_is_rejected() {
    let dir = scratch_dir("missing-root");
    let settings = Settings {
        index_roots: vec![dir.join("absent").display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn file_as_index_root_is_rejected() {
    let dir = scratch_dir("file-root");
    let file = dir.join("note.txt");
    fs::write(&file, "x").expect("write file");
    let settings = Settings {
        index_roots: vec![file.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn filesystem_root_is_rejected_as_index_root() {
    let settings = Settings {
        index_roots: vec!["/".to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn home_directory_is_rejected_as_index_root() {
    let home = scratch_dir("home-root");
    let settings = Settings {
        index_roots: vec![home.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(
        validation_kind_with_home(&settings, &home),
        Some(ErrorKind::Config)
    );
}

#[test]
fn subdirectory_of_home_is_accepted_as_index_root() {
    let home = scratch_dir("home-child");
    let documents = home.join("Documents");
    fs::create_dir_all(&documents).expect("documents");
    let settings = Settings {
        index_roots: vec![documents.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind_with_home(&settings, &home), None);
}

#[test]
fn validate_with_home_rejects_missing_home() {
    let dir = scratch_dir("missing-home");
    let settings = Settings {
        index_roots: vec![dir.display().to_string()],
        ..Settings::default()
    };
    let missing = dir.join("no-such-home");
    assert_eq!(
        validation_kind_with_home(&settings, &missing),
        Some(ErrorKind::Io)
    );
}

#[test]
fn nested_index_roots_are_rejected() {
    let dir = scratch_dir("nested-roots");
    let parent = dir.join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).expect("nested dirs");
    let settings = Settings {
        index_roots: vec![parent.display().to_string(), child.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn identical_index_roots_are_rejected() {
    let dir = scratch_dir("same-roots");
    let settings = Settings {
        index_roots: vec![dir.display().to_string(), dir.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn disjoint_index_roots_are_accepted() {
    let dir = scratch_dir("disjoint-roots");
    let first = dir.join("first");
    let second = dir.join("second");
    fs::create_dir_all(&first).expect("first");
    fs::create_dir_all(&second).expect("second");
    let settings = Settings {
        index_roots: vec![first.display().to_string(), second.display().to_string()],
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn content_limit_below_minimum_is_rejected() {
    let settings = Settings {
        max_content_file_bytes: 64 * 1024 - 1,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn content_limit_at_minimum_is_accepted() {
    let settings = Settings {
        max_content_file_bytes: 64 * 1024,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn content_limit_above_maximum_is_rejected() {
    let settings = Settings {
        max_content_file_bytes: 16 * 1024 * 1024 + 1,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn content_limit_at_maximum_is_accepted() {
    let settings = Settings {
        max_content_file_bytes: 16 * 1024 * 1024,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn turn_timeout_below_minimum_is_rejected() {
    let settings = Settings {
        turn_timeout_secs: 29,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn turn_timeout_at_minimum_is_accepted() {
    let settings = Settings {
        turn_timeout_secs: 30,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn turn_timeout_above_maximum_is_rejected() {
    let settings = Settings {
        turn_timeout_secs: 3601,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn turn_timeout_at_maximum_is_accepted() {
    let settings = Settings {
        turn_timeout_secs: 3600,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn retention_below_minimum_is_rejected() {
    let settings = Settings {
        capture_retention_days: 0,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn retention_at_minimum_is_accepted() {
    let settings = Settings {
        capture_retention_days: 1,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn retention_above_maximum_is_rejected() {
    let settings = Settings {
        capture_retention_days: 91,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn retention_at_maximum_is_accepted() {
    let settings = Settings {
        capture_retention_days: 90,
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), None);
}

#[test]
fn hotkey_without_modifier_is_rejected_by_validation() {
    let settings = Settings {
        hotkey_panel: "space".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::InvalidInput));
}

#[test]
fn identical_hotkeys_are_rejected() {
    let settings = Settings {
        hotkey_panel: "alt+space".to_string(),
        hotkey_capture_ask: "alt+space".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn hotkeys_equal_after_modifier_reordering_are_rejected() {
    let settings = Settings {
        hotkey_panel: "shift+alt+space".to_string(),
        hotkey_capture_ask: "alt+shift+space".to_string(),
        ..Settings::default()
    };
    assert_eq!(validation_kind(&settings), Some(ErrorKind::Config));
}

#[test]
fn parse_hotkey_accepts_basic_combination() {
    let hotkey = parse_hotkey("alt+space").expect("alt+space parses");
    assert_eq!(hotkey.modifiers, vec![Modifier::Alt]);
    assert_eq!(hotkey.key, "space");
}

#[test]
fn parse_hotkey_treats_option_as_alt() {
    let option = parse_hotkey("option+space").expect("option+space parses");
    let alt = parse_hotkey("alt+space").expect("alt+space parses");
    assert_eq!(option, alt);
}

#[test]
fn parse_hotkey_canonicalises_modifier_order() {
    let hotkey = parse_hotkey("shift+cmd+ctrl+a").expect("multi-modifier parses");
    assert_eq!(
        hotkey.modifiers,
        vec![Modifier::Cmd, Modifier::Ctrl, Modifier::Shift]
    );
    assert_eq!(hotkey.key, "a");
}

#[test]
fn parse_hotkey_accepts_digits_return_tab_and_function_keys() {
    assert_eq!(parse_hotkey("cmd+7").expect("digit").key, "7");
    assert_eq!(parse_hotkey("cmd+return").expect("return").key, "return");
    assert_eq!(parse_hotkey("ctrl+tab").expect("tab").key, "tab");
    assert_eq!(parse_hotkey("ctrl+f1").expect("f1").key, "f1");
    assert_eq!(parse_hotkey("ctrl+f12").expect("f12").key, "f12");
}

#[test]
fn parse_hotkey_rejects_missing_modifier() {
    assert_eq!(
        parse_hotkey("space").expect_err("no modifier").kind,
        ErrorKind::InvalidInput
    );
}

#[test]
fn parse_hotkey_rejects_uppercase_tokens() {
    assert!(parse_hotkey("Alt+space").is_err());
    assert!(parse_hotkey("alt+Space").is_err());
    assert!(parse_hotkey("alt+A").is_err());
}

#[test]
fn parse_hotkey_rejects_unknown_key() {
    assert!(parse_hotkey("alt+spacebar").is_err());
    assert!(parse_hotkey("alt+!").is_err());
    assert!(parse_hotkey("alt+f13").is_err());
    assert!(parse_hotkey("alt+f0").is_err());
    assert!(parse_hotkey("alt+f01").is_err());
    assert!(parse_hotkey("alt+f+1").is_err());
}

#[test]
fn parse_hotkey_rejects_unknown_modifier() {
    assert!(parse_hotkey("win+space").is_err());
    assert!(parse_hotkey("meta+space").is_err());
}

#[test]
fn parse_hotkey_rejects_duplicate_modifier() {
    assert!(parse_hotkey("alt+option+space").is_err());
}

#[test]
fn parse_hotkey_rejects_empty_and_dangling_separators() {
    assert!(parse_hotkey("").is_err());
    assert!(parse_hotkey("alt+").is_err());
    assert!(parse_hotkey("+space").is_err());
    assert!(parse_hotkey("alt++space").is_err());
}

#[test]
fn parse_hotkey_rejects_key_before_last_position() {
    assert!(parse_hotkey("alt+space+cmd").is_err());
}
