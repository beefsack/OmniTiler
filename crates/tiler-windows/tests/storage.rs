#![forbid(unsafe_code)]
use std::path::PathBuf;
use tiler_windows::model::{
    LEDGER_SCHEMA_VERSION, ProcessIdentity, RecoveryLedger, WindowClaimKind, WindowIdentity,
};
use tiler_windows::storage::{LedgerStore, PublishError, StorageError, publish_no_overwrite};

static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(name: &str) -> Self {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiler-windows-{name}-{}-{t}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temp dir");
        Self { path }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn owner() -> ProcessIdentity {
    ProcessIdentity {
        pid: 100,
        process_creation: "c-owner".to_owned(),
        user_sid: "S-owner".to_owned(),
        session_id: 1,
        exe_path: "C:\\tools\\t.exe".to_owned(),
    }
}

fn window(hwnd: u64, tag: &str) -> WindowIdentity {
    WindowIdentity {
        hwnd,
        process: ProcessIdentity {
            pid: 200,
            process_creation: "c-200".to_owned(),
            user_sid: "S-owner".to_owned(),
            session_id: 1,
            exe_path: "C:\\apps\\a.exe".to_owned(),
        },
        tag: tag.to_owned(),
        kind: WindowClaimKind::Helper,
        show: tiler_windows::model::WindowShowState::default(),
    }
}

fn ledger() -> RecoveryLedger {
    RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: vec![window(0xABCD, "tag-1")],
        mouse_snap: None,
    }
}

#[test]
fn roundtrip() {
    let temp = Temp::new("roundtrip");
    let store = LedgerStore::open(&temp.path).expect("open");
    assert_eq!(store.committed().expect("read"), None);
    store.commit(&ledger()).expect("commit");
    assert_eq!(store.committed().expect("read"), Some(ledger()));
    let mut updated = ledger();
    updated.windows.push(window(0xABCE, "tag-2"));
    store.commit(&updated).expect("same-owner update");
    assert_eq!(store.committed().expect("read"), Some(updated));
}

#[test]
fn different_owner_refusal_preserves_bytes() {
    let temp = Temp::new("owner");
    let store = LedgerStore::open(&temp.path).expect("open");
    store.commit(&ledger()).expect("commit");
    let before = std::fs::read(temp.path.join("ledger.json")).expect("bytes");
    let mut other = ledger();
    other.owner.pid = 101;
    assert!(matches!(
        store.commit(&other).expect_err("owner"),
        StorageError::DifferentOwner
    ));
    assert_eq!(
        std::fs::read(temp.path.join("ledger.json")).expect("bytes"),
        before
    );
    assert_eq!(store.committed().expect("read"), Some(ledger()));
}

#[test]
fn corrupt_ledger_refused() {
    let temp = Temp::new("corrupt");
    {
        let store = LedgerStore::open(&temp.path).expect("open");
        store.commit(&ledger()).expect("commit");
    }
    std::fs::write(temp.path.join("ledger.json"), b"{not json").expect("corrupt");
    let store = LedgerStore::open(&temp.path).expect("open");
    assert!(matches!(
        store.committed().expect_err("read"),
        StorageError::Ledger(_)
    ));
    assert!(matches!(
        store.commit(&ledger()).expect_err("write"),
        StorageError::Ledger(_)
    ));
    assert_eq!(
        std::fs::read(temp.path.join("ledger.json")).expect("bytes"),
        b"{not json"
    );
}

#[test]
fn lock_contention() {
    let temp = Temp::new("lock");
    let first = LedgerStore::open(&temp.path).expect("first");
    let err = match LedgerStore::open(&temp.path) {
        Ok(_) => panic!("second open must fail"),
        Err(e) => e,
    };
    assert!(matches!(err, StorageError::LockContended));
    drop(first);
    LedgerStore::open(&temp.path).expect("reopen");
}

fn dir_entries(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn publish_second_writer_pending_keeps_first_bytes() {
    let temp = Temp::new("publish-pending");
    let first = b"{\"v\":1,\"index\":2}".to_vec();
    publish_no_overwrite(&temp.path, "workspace.request", &first, "a-1").expect("first publish");
    assert_eq!(
        std::fs::read(temp.path.join("workspace.request")).expect("bytes"),
        first
    );
    let second = b"{\"v\":1,\"index\":1}".to_vec();
    assert!(matches!(
        publish_no_overwrite(&temp.path, "workspace.request", &second, "b-2").expect_err("pending"),
        PublishError::Pending
    ));
    // Existing final untouched, caller temp cleaned: only the final remains.
    assert_eq!(
        std::fs::read(temp.path.join("workspace.request")).expect("bytes"),
        first
    );
    assert_eq!(
        dir_entries(&temp.path),
        vec!["workspace.request".to_owned()]
    );
}

#[test]
fn publish_concurrent_single_winner_full_body() {
    let temp = Temp::new("publish-race");
    let bodies: Vec<Vec<u8>> = (0..8u8)
        .map(|i| {
            let mut body = format!("{{\"v\":1,\"index\":{},\"pad\":\"", i % 3).into_bytes();
            body.extend(std::iter::repeat_n(i + 0x41, 65536));
            body.extend(b"\"}".iter());
            body
        })
        .collect();
    let dir = &temp.path;
    let results: Vec<_> = std::thread::scope(|scope| {
        bodies
            .iter()
            .enumerate()
            .map(|(i, body)| {
                scope.spawn(move || {
                    publish_no_overwrite(dir, "workspace.request", body, &format!("t-{i}"))
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });
    let wins = results.iter().filter(|r| r.is_ok()).count();
    let pending = results
        .iter()
        .filter(|r| matches!(r, Err(PublishError::Pending)))
        .count();
    assert_eq!(wins, 1, "exactly one publisher wins");
    assert_eq!(pending, results.len() - 1, "losers refuse as pending");
    // Final is byte-for-byte one complete body: never partial, never mixed.
    let final_bytes = std::fs::read(temp.path.join("workspace.request")).expect("bytes");
    assert!(
        bodies.iter().any(|b| b == &final_bytes),
        "final must equal one full published body"
    );
    assert_eq!(
        dir_entries(&temp.path),
        vec!["workspace.request".to_owned()]
    );
}
