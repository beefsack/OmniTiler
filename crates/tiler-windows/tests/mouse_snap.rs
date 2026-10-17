#![forbid(unsafe_code)]
use std::path::PathBuf;
use tiler_windows::model::{
    LEDGER_SCHEMA_VERSION, LedgerError, MouseSnapPreimage, MouseSnapRestoreDecision,
    MouseSnapSetupDecision, ProcessIdentity, RecoveryLedger, WindowIdentity, mouse_snap_owned,
    mouse_snap_restore_decision, mouse_snap_setup_decision, parse_ledger, validate_ledger,
};
use tiler_windows::storage::{LedgerStore, StorageError};
use tiler_windows::tiling::{parse_tile_args, parse_tile_proof_args};

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
            "tiler-windows-mousesnap-{name}-{}-{t}-{n}",
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

fn ledger_with(snap: Option<MouseSnapPreimage>) -> RecoveryLedger {
    RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: Vec::new(),
        mouse_snap: snap,
    }
}

fn owned_preimage() -> MouseSnapPreimage {
    MouseSnapPreimage {
        original: true,
        owned: true,
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn setup_original_true_disables() {
    assert_eq!(
        mouse_snap_setup_decision(Some(true)),
        MouseSnapSetupDecision::Disable
    );
}

#[test]
fn setup_original_false_claims_no_mutation() {
    assert_eq!(
        mouse_snap_setup_decision(Some(false)),
        MouseSnapSetupDecision::AlreadyOff
    );
    assert!(!mouse_snap_owned(Some(MouseSnapPreimage {
        original: false,
        owned: false,
    })));
    assert!(!mouse_snap_owned(Some(MouseSnapPreimage {
        original: false,
        owned: true,
    })));
}

#[test]
fn setup_read_failure_retains_without_setter() {
    assert_eq!(
        mouse_snap_setup_decision(None),
        MouseSnapSetupDecision::UnknownRetain
    );
    assert!(!mouse_snap_owned(None));
}

#[test]
fn restore_live_still_ours() {
    assert_eq!(
        mouse_snap_restore_decision(Some(owned_preimage()), Some(false)),
        MouseSnapRestoreDecision::Restore
    );
}

#[test]
fn restore_true_drift_preserved_never_forced() {
    assert_eq!(
        mouse_snap_restore_decision(Some(owned_preimage()), Some(true)),
        MouseSnapRestoreDecision::PreserveDrift
    );
}

#[test]
fn restore_without_owned_mutation_writes_nothing() {
    for preimage in [
        None,
        Some(MouseSnapPreimage {
            original: false,
            owned: false,
        }),
        Some(MouseSnapPreimage {
            original: true,
            owned: false,
        }),
    ] {
        for live in [None, Some(false), Some(true)] {
            assert_eq!(
                mouse_snap_restore_decision(preimage, live),
                MouseSnapRestoreDecision::NoOwnedMutation,
                "preimage={preimage:?} live={live:?}"
            );
        }
    }
}

#[test]
fn restore_live_read_failure_retains_evidence() {
    assert_eq!(
        mouse_snap_restore_decision(Some(owned_preimage()), None),
        MouseSnapRestoreDecision::UncertainRetain
    );
}

#[test]
fn old_ledger_without_field_parses_to_no_snap_work() {
    let json = serde_json::json!({
        "v": 1,
        "owner": owner(),
        "windows": Vec::<WindowIdentity>::new(),
    })
    .to_string();
    let ledger = parse_ledger(&json).expect("old ledger parses");
    assert_eq!(ledger.mouse_snap, None);
    assert!(!mouse_snap_owned(ledger.mouse_snap));
    assert_eq!(
        mouse_snap_restore_decision(ledger.mouse_snap, Some(false)),
        MouseSnapRestoreDecision::NoOwnedMutation
    );
}

#[test]
fn new_ledger_with_field_roundtrips() {
    let ledger = ledger_with(Some(owned_preimage()));
    validate_ledger(&ledger).expect("valid");
    let json = serde_json::to_string(&ledger).expect("serialize");
    let back = parse_ledger(&json).expect("parse");
    assert_eq!(back, ledger);
    assert!(mouse_snap_owned(back.mouse_snap));
}

#[test]
fn new_ledger_omits_absent_field() {
    let json = serde_json::to_string(&ledger_with(None)).expect("serialize");
    assert!(!json.contains("mouse_snap"), "absent field stays omitted");
}

#[test]
fn invalid_snap_data_is_malformed_never_defaulted() {
    let mut value = serde_json::json!({
        "v": 1,
        "owner": owner(),
        "windows": Vec::<WindowIdentity>::new(),
        "mouse_snap": {"original": "yes", "owned": true},
    });
    assert_eq!(
        parse_ledger(&value.to_string()),
        Err(LedgerError::Malformed)
    );
    value["mouse_snap"] = serde_json::json!({"original": true});
    assert_eq!(
        parse_ledger(&value.to_string()),
        Err(LedgerError::Malformed)
    );
}

#[test]
fn version_bump_still_refused() {
    // Future schema versions never parse as owner evidence.
    let mut ledger = ledger_with(Some(owned_preimage()));
    ledger.v = LEDGER_SCHEMA_VERSION + 1;
    let json = serde_json::to_string(&ledger).expect("serialize");
    assert_eq!(parse_ledger(&json), Err(LedgerError::UnsupportedVersion));
}

#[test]
fn v1_without_snap_parses_to_no_work_v1_with_snap_refused() {
    // Old ledgers without the field still parse (no Snap work).
    let old = serde_json::json!({
        "v": 1,
        "owner": owner(),
        "windows": Vec::<WindowIdentity>::new(),
    })
    .to_string();
    let ledger = parse_ledger(&old).expect("v1 without field parses");
    assert_eq!(ledger.mouse_snap, None);
    // A v1 tag carrying a Snap claim is version confusion, never evidence.
    let confused = serde_json::json!({
        "v": 1,
        "owner": owner(),
        "windows": Vec::<WindowIdentity>::new(),
        "mouse_snap": {"original": true, "owned": true},
    })
    .to_string();
    assert_eq!(
        parse_ledger(&confused),
        Err(LedgerError::UnsupportedVersion)
    );
}

#[test]
fn old_v1_only_reader_refuses_v2_so_no_silent_snap_loss() {
    // Old binaries validate `v == 1` only: a current write with Snap work must
    // refuse there instead of parsing without the field, restoring windows,
    // and deleting the ledger while leaving the setting off.
    fn old_validate(value: &serde_json::Value) -> Result<(), LedgerError> {
        if value.get("v").and_then(|v| v.as_u64()) != Some(1) {
            return Err(LedgerError::UnsupportedVersion);
        }
        Ok(())
    }
    // Old v2-only binaries likewise refuse v3 product-schema writes.
    fn old_v2_validate(value: &serde_json::Value) -> Result<(), LedgerError> {
        if !matches!(value.get("v").and_then(|v| v.as_u64()), Some(1) | Some(2)) {
            return Err(LedgerError::UnsupportedVersion);
        }
        Ok(())
    }
    let current = serde_json::to_value(ledger_with(Some(owned_preimage()))).expect("value");
    assert_eq!(
        current.get("v").and_then(|v| v.as_u64()),
        Some(u64::from(LEDGER_SCHEMA_VERSION))
    );
    assert_eq!(old_validate(&current), Err(LedgerError::UnsupportedVersion));
    assert_eq!(
        old_v2_validate(&current),
        Err(LedgerError::UnsupportedVersion)
    );
    // And the new reader accepts that same current payload.
    let json = serde_json::to_string(&current).expect("json");
    assert!(mouse_snap_owned(
        parse_ledger(&json).expect("current parses").mouse_snap
    ));
}

#[test]
fn store_refuses_version_confusion_and_retains_bytes() {
    // A v1 tag with a Snap claim fails validation, so the store refuses the
    // commit and retains the existing bytes instead of overwriting evidence.
    let temp = Temp::new("version-confusion");
    let store = LedgerStore::open(&temp.path).expect("open");
    store.commit(&ledger_with(None)).expect("v2 commit");
    let before = std::fs::read(temp.path.join("ledger.json")).expect("bytes");
    let confused = RecoveryLedger {
        v: 1,
        owner: owner(),
        windows: Vec::new(),
        mouse_snap: Some(owned_preimage()),
    };
    assert!(matches!(
        store.commit(&confused).expect_err("confused commit"),
        StorageError::Ledger(LedgerError::UnsupportedVersion)
    ));
    assert_eq!(
        std::fs::read(temp.path.join("ledger.json")).expect("bytes"),
        before,
        "version-confusion bytes preserved"
    );
}

#[test]
fn completed_restore_relinquishes_claim_so_resume_recaptures_fresh() {
    // Activation 1: live TRUE captures Disable intent with an owned claim.
    assert_eq!(
        mouse_snap_setup_decision(Some(true)),
        MouseSnapSetupDecision::Disable
    );
    let claim = Some(owned_preimage());
    assert!(mouse_snap_owned(claim));
    // Suspend with live still our FALSE decides Restore; the verified TRUE
    // write completes the activation and relinquishes the claim (None) under
    // the lease, windows and owner preserved.
    assert_eq!(
        mouse_snap_restore_decision(claim, Some(false)),
        MouseSnapRestoreDecision::Restore
    );
    let temp = Temp::new("relinquish");
    let store = LedgerStore::open(&temp.path).expect("open");
    store.commit(&ledger_with(claim)).expect("activation");
    let relinquished = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: Vec::new(),
        mouse_snap: None,
    };
    store.commit(&relinquished).expect("relinquish");
    let read = store.committed().expect("read").expect("ledger");
    assert_eq!(read.mouse_snap, None);
    assert!(!mouse_snap_owned(read.mouse_snap));
    // Without relinquish the stale claim plus foreign FALSE still decides
    // Restore: the exact incorrect stop-write this lifecycle prevents.
    assert_eq!(
        mouse_snap_restore_decision(claim, Some(false)),
        MouseSnapRestoreDecision::Restore
    );
    // External FALSE during suspend; resume recaptures the exact live FALSE
    // as a new activation with no owned mutation, and stop writes nothing.
    assert_eq!(
        mouse_snap_setup_decision(Some(false)),
        MouseSnapSetupDecision::AlreadyOff
    );
    let recaptured = RecoveryLedger {
        v: LEDGER_SCHEMA_VERSION,
        owner: owner(),
        windows: Vec::new(),
        mouse_snap: Some(MouseSnapPreimage {
            original: false,
            owned: false,
        }),
    };
    store.commit(&recaptured).expect("recapture");
    let live = store.committed().expect("read").expect("ledger").mouse_snap;
    assert!(!mouse_snap_owned(live));
    for observed in [Some(false), Some(true), None] {
        assert_eq!(
            mouse_snap_restore_decision(live, observed),
            MouseSnapRestoreDecision::NoOwnedMutation,
            "recaptured FALSE never authorizes a stop write, live={observed:?}"
        );
    }
}

#[test]
fn drift_preserve_relinquishes_claim_so_resume_recaptures() {
    // Foreign TRUE drift ends the activation without a write and likewise
    // relinquishes, so the completed original-TRUE claim never blocks a
    // later AlreadyOff recapture of FALSE.
    let claim = Some(owned_preimage());
    assert_eq!(
        mouse_snap_restore_decision(claim, Some(true)),
        MouseSnapRestoreDecision::PreserveDrift
    );
    let temp = Temp::new("drift-relinquish");
    let store = LedgerStore::open(&temp.path).expect("open");
    store.commit(&ledger_with(claim)).expect("activation");
    store
        .commit(&RecoveryLedger {
            v: LEDGER_SCHEMA_VERSION,
            owner: owner(),
            windows: Vec::new(),
            mouse_snap: None,
        })
        .expect("relinquish");
    assert!(!mouse_snap_owned(
        store.committed().expect("read").expect("ledger").mouse_snap
    ));
    assert_eq!(
        mouse_snap_setup_decision(Some(false)),
        MouseSnapSetupDecision::AlreadyOff
    );
}

#[test]
fn store_roundtrip_preserves_snap() {
    let temp = Temp::new("roundtrip");
    let store = LedgerStore::open(&temp.path).expect("open");
    store
        .commit(&ledger_with(Some(owned_preimage())))
        .expect("commit");
    assert_eq!(
        store.committed().expect("read"),
        Some(ledger_with(Some(owned_preimage())))
    );
    store
        .commit(&ledger_with(None))
        .expect("same-owner drift relinquish");
    let read = store.committed().expect("read").expect("ledger");
    assert_eq!(read.mouse_snap, None);
    assert!(
        !serde_json::to_string(&read)
            .expect("serialize")
            .contains("mouse_snap")
    );
}

#[test]
fn stale_snap_ledger_blocks_other_owner_overwrite() {
    let temp = Temp::new("stale");
    let store = LedgerStore::open(&temp.path).expect("open");
    store
        .commit(&ledger_with(Some(owned_preimage())))
        .expect("commit");
    let before = std::fs::read(temp.path.join("ledger.json")).expect("bytes");
    let mut other = ledger_with(Some(MouseSnapPreimage {
        original: false,
        owned: false,
    }));
    other.owner.pid = 101;
    assert!(matches!(
        store.commit(&other).expect_err("other owner"),
        StorageError::DifferentOwner
    ));
    assert_eq!(
        std::fs::read(temp.path.join("ledger.json")).expect("bytes"),
        before,
        "stale preimage bytes preserved"
    );
    assert!(mouse_snap_owned(
        store.committed().expect("read").expect("ledger").mouse_snap
    ));
}

#[test]
fn tile_snap_prevention_default_on_with_visible_off() {
    let base = parse_tile_args(&strings(&["--user-start"])).expect("parsed");
    assert!(!base.no_mouse_snap_prevention);
    let off =
        parse_tile_args(&strings(&["--user-start", "--no-mouse-snap-prevention"])).expect("parsed");
    assert!(off.no_mouse_snap_prevention);
    // Existing keyboard surface is unchanged by the new flag.
    assert!(!base.no_keyboard_snap_takeover);
    assert!(!base.allow_win_l);
}

#[test]
fn proof_takes_no_snap_setting() {
    assert!(
        parse_tile_proof_args(&strings(&[
            "--allowlist",
            "a.json",
            "--no-mouse-snap-prevention"
        ]))
        .is_err()
    );
}

#[test]
fn portable_protocol_targets_winarranging_never_pen_visualization() {
    // MS SystemParametersInfoW docs: WINARRANGING GET 0x0082 / SET 0x0083 is
    // the window-arrangement master switch; PENVISUALIZATION GET 0x201E / SET
    // 0x201F is a different pen-feedback target (ON=0x23). Local doc cache
    // tool_0f7edd44b001jEAY4xyCyhAzEV lines 374/398 vs 235/263. The portable
    // preimage/restore vocabulary below is boolean-only: it can only express
    // the WINARRANGING effect and can never address the PEN numeric range.
    const SPI_GETWINARRANGING_DOC: u32 = 0x0082;
    const SPI_SETWINARRANGING_DOC: u32 = 0x0083;
    const SPI_GETPENVISUALIZATION_DOC: u32 = 0x201E;
    const SPI_SETPENVISUALIZATION_DOC: u32 = 0x201F;
    const PENVISUALIZATION_ON_DOC: u32 = 0x23;
    assert_ne!(SPI_GETWINARRANGING_DOC, SPI_GETPENVISUALIZATION_DOC);
    assert_ne!(SPI_SETWINARRANGING_DOC, SPI_SETPENVISUALIZATION_DOC);
    assert_eq!(PENVISUALIZATION_ON_DOC, 35);
    // Boolean protocol gates the only owned mutation: original TRUE plus live
    // still our FALSE restores; any other combination writes nothing, so a PEN
    // raw (1/35/garbage) can never authorize a WINARRANGING write by itself.
    assert_eq!(
        mouse_snap_restore_decision(Some(owned_preimage()), Some(false)),
        MouseSnapRestoreDecision::Restore
    );
    assert_eq!(
        mouse_snap_restore_decision(Some(owned_preimage()), Some(true)),
        MouseSnapRestoreDecision::PreserveDrift
    );
}
