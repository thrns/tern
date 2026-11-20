use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tern_core::core::Core;

fn scratch_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "tern-core-calc-{name}-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn core_calc_evaluates_unit_conversion() {
    let root = scratch_dir("unit");
    let core = Core::open(Some(root.clone())).expect("core opens");
    let outcome = core.calc("5 km to mi").expect("5 km to mi evaluates");
    assert!(outcome.is_approximate);
    assert!(outcome.display.ends_with(" mi"));
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn core_calc_rejects_words() {
    let root = scratch_dir("words");
    let core = Core::open(Some(root.clone())).expect("core opens");
    assert_eq!(core.calc("hello"), None);
    fs::remove_dir_all(&root).expect("cleanup");
}
