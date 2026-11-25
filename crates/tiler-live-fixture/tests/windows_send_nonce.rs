#![cfg(windows)]

use std::process::Command;

const NONCE_ENV: &str = "TILER_LIVE_FIXTURE_NONCE";

fn send() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tiler-live-fixture"));
    command
        .args(["send", "--socket", "unused-by-parser-test"])
        .env_remove(NONCE_ENV);
    command
}

#[test]
fn windows_send_reads_the_nonce_from_the_environment_not_argv() {
    let nonce = "a052".repeat(16);
    let legacy = send().args(["--nonce", &nonce]).output().unwrap();
    assert_eq!(legacy.status.code(), Some(2));
    let error = String::from_utf8(legacy.stderr).unwrap();
    assert!(error.contains("unknown argument '--nonce'"));
    assert!(!error.contains(&nonce));
    assert!(legacy.stdout.is_empty());

    let absent = send().args(["--wait-ms", "1"]).output().unwrap();
    assert_eq!(absent.status.code(), Some(2));
    assert!(String::from_utf8(absent.stderr).unwrap().contains(NONCE_ENV));

    // This pair check runs after nonce validation and before any transport IO.
    let present = send()
        .env(NONCE_ENV, &nonce)
        .args(["--wait-ms", "1"])
        .output()
        .unwrap();
    assert_eq!(present.status.code(), Some(2));
    let error = String::from_utf8(present.stderr).unwrap();
    assert!(error.contains("--wait-ms and --expect must be given together"));
    assert!(!error.contains(&nonce));
    assert!(present.stdout.is_empty());
}
