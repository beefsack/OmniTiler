#![forbid(unsafe_code)]
#![cfg(windows)]

use tiler_windows::native::{current_identity, current_integrity_level, ledger_directory};

#[test]
fn current_identity_is_self_consistent() {
    let first = current_identity().expect("current identity");
    let second = current_identity().expect("current identity again");
    assert_eq!(first, second);
    assert_eq!(first.pid, std::process::id());
    assert!(!first.user_sid.is_empty());
    assert!(!first.exe_path.is_empty());
    assert!(!first.process_creation.is_empty());
    let queried = tiler_windows::native::query_identity(first.pid).expect("query self identity");
    assert_eq!(queried, first);
    let level = current_integrity_level().expect("integrity level");
    assert!(level > 0);
    let queried_level =
        tiler_windows::native::query_integrity_level(first.pid).expect("query self level");
    assert_eq!(queried_level, level);
}

#[test]
fn ledger_path_is_private_session_dir() {
    let dir = ledger_directory().expect("ledger directory");
    assert!(dir.is_absolute());
    let session = current_identity().expect("identity").session_id;
    assert_eq!(
        dir.file_name().and_then(|s| s.to_str()),
        Some(format!("session-{session}").as_str())
    );
    assert_eq!(
        dir.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str()),
        Some("omnitiler")
    );
}
