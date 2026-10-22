#![forbid(unsafe_code)]
pub const DEFAULT_RUN_SECONDS: u64 = 120;
pub const MAX_RUN_SECONDS: u64 = 600;
pub const STOP_REQUEST_FILE: &str = "stop.request";
/// Exact-owner out-of-hook workspace request (normal `tile` only): one JSON
/// object naming the owner creation plus an action (`select`/`send`/`stay`/
/// `previous`/`fullscreen`/`relative`/`send-relative`/`stay-relative), a digit
/// index 0..=9 for the indexed actions, and a `previous`/`next` direction for
/// relative actions. The owner loop validates the full owner binding and
/// consumes the file once through the existing `workspace_do_select` /
/// `workspace_do_send` resolvers (plus the pure history resolvers and, for
/// `fullscreen`, the existing project-owned fullscreen toggle); the CLI
/// never actuates windows.
pub const WORKSPACE_REQUEST_FILE: &str = "workspace.request";
/// Pointer to the current per-run log file name (never geometry).
pub const RUN_CURRENT_FILE: &str = "run-current.txt";

/// Per-run unique log file name keyed by owner process creation. Historical
/// logs are never overwritten: each owner run writes its own file and updates
/// the current pointer. `creation` must be nonempty hex from process times.
#[must_use]
pub fn run_log_file_name(creation: &str) -> String {
    format!("run-{creation}.log")
}

pub const MEDIUM_RID_MIN: u32 = 8192;
pub const MEDIUM_RID_MAX: u32 = 12288;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub seconds: u64,
    pub trace: bool,
    pub hide_hwnd: Option<u64>,
}

#[must_use]
pub fn is_medium_rid(rid: u32) -> bool {
    (MEDIUM_RID_MIN..MEDIUM_RID_MAX).contains(&rid)
}

#[must_use]
pub fn exe_paths_equal(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_ascii_lowercase();
    norm(a) == norm(b)
}

pub fn parse_run_args(args: &[String]) -> Result<RunOptions, String> {
    let mut seconds = DEFAULT_RUN_SECONDS;
    let mut trace = false;
    let mut hide_hwnd: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or("usage: run --seconds N [--trace] [--hide HWND]")?;
                seconds = value
                    .parse::<u64>()
                    .map_err(|_| "usage: run --seconds N [--trace] [--hide HWND]")?;
                i += 1;
            }
            "--hide" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or("usage: run --seconds N [--trace] [--hide HWND]")?;
                hide_hwnd = Some(
                    crate::test_window::parse_hwnd(value)
                        .ok_or("usage: run --seconds N [--trace] [--hide HWND]")?,
                );
                i += 1;
            }
            _ => return Err("usage: run --seconds N [--trace] [--hide HWND]".to_owned()),
        }
    }
    if seconds == 0 || seconds > MAX_RUN_SECONDS {
        return Err(format!("refuse: seconds must be 1..={MAX_RUN_SECONDS}"));
    }
    Ok(RunOptions {
        seconds,
        trace,
        hide_hwnd,
    })
}

#[must_use]
pub fn stop_request_matches(contents: &str, owner_creation: &str) -> bool {
    !owner_creation.is_empty() && contents.trim() == owner_creation
}

#[cfg(windows)]
pub mod sys {
    use super::{
        RUN_CURRENT_FILE, STOP_REQUEST_FILE, is_medium_rid, run_log_file_name, stop_request_matches,
    };
    use crate::model::{
        LEDGER_SCHEMA_VERSION, MouseSnapPreimage, MouseSnapRestoreDecision, MouseSnapSetupDecision,
        RecoveryLedger, mouse_snap_owned, mouse_snap_restore_decision, mouse_snap_setup_decision,
        parse_ledger, restore_eligibility, watcher_may_restore,
    };
    use crate::native::{
        HeldProcess, IdentityError, current_exe_path, current_identity, current_integrity_level,
        ledger_directory,
    };
    use crate::storage::{LEDGER_FILE_NAME, LedgerStore};
    use crate::test_window::{SnapshotError, WindowSnapshot};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    const POLL_MS: u64 = 150;
    const STOP_WAIT_MS: u32 = 5000;

    type DynError = Box<dyn std::error::Error>;
    type Result<T> = std::result::Result<T, DynError>;

    fn err(msg: impl Into<String>) -> DynError {
        Box::new(std::io::Error::other(msg.into()))
    }

    fn write_log_at(path: &Path, line: &str, first: bool) -> Result<()> {
        use std::io::Write;
        let mut file = if first {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
        } else {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        }
        .map_err(|e| err(format!("error: log write: {e}")))?;
        writeln!(file, "{line}").map_err(|e| err(format!("error: log write: {e}")))?;
        file.flush()
            .map_err(|e| err(format!("error: log write: {e}")))
    }

    /// Per-run unique log path persisted via the owner creation. Historical
    /// logs are never overwritten: each run owns `run-<creation>.log`.
    pub fn log_path_for(dir: &Path, creation: &str) -> PathBuf {
        dir.join(run_log_file_name(creation))
    }

    fn write_log_for(dir: &Path, creation: &str, line: &str, first: bool) -> Result<()> {
        write_log_at(&log_path_for(dir, creation), line, first)
    }

    fn point_current(dir: &Path, creation: &str) -> Result<()> {
        std::fs::write(dir.join(RUN_CURRENT_FILE), run_log_file_name(creation))
            .map_err(|e| err(format!("error: log pointer write: {e}")))
    }

    fn medium_caller() -> Result<crate::model::ProcessIdentity> {
        let me = current_identity().map_err(|e| err(format!("error: identity {e}")))?;
        let rid = current_integrity_level().map_err(|e| err(format!("error: integrity {e}")))?;
        if !is_medium_rid(rid) {
            return Err(err(format!("refuse: integrity {rid} is not medium")));
        }
        Ok(me)
    }

    fn open_held(pid: u32, terminate: bool, ctx: &str) -> Result<Option<HeldProcess>> {
        match if terminate {
            HeldProcess::open_terminate(pid)
        } else {
            HeldProcess::open(pid)
        } {
            Ok(held) => Ok(Some(held)),
            Err(IdentityError::Absent) => Ok(None),
            Err(e) => Err(err(format!("error: {ctx} {e}"))),
        }
    }

    fn held_live(
        pid: u32,
        terminate: bool,
    ) -> Result<Option<(HeldProcess, crate::model::ProcessIdentity)>> {
        let Some(held) = open_held(pid, terminate, "owner")? else {
            return Ok(None);
        };
        match held.identity() {
            Ok(live) => Ok(Some((held, live))),
            Err(IdentityError::Absent) => Ok(None),
            Err(e) => Err(err(format!("error: owner {e}"))),
        }
    }

    fn owned(hwnd: u64, exe: &str, sid: &str, session: u32, ctx: &str) -> Result<WindowSnapshot> {
        match crate::test_window::sys::query_owned(hwnd, exe, sid, session) {
            Ok(s) => Ok(s),
            Err(SnapshotError::Absent(msg)) => Err(err(format!("refuse: {ctx}{msg}"))),
            Err(SnapshotError::Failed(msg)) => Err(err(format!("error: {ctx}{msg}"))),
        }
    }

    fn owned_opt(hwnd: u64, exe: &str, sid: &str, session: u32) -> Result<Option<WindowSnapshot>> {
        match crate::test_window::sys::query_owned(hwnd, exe, sid, session) {
            Ok(s) => Ok(Some(s)),
            Err(SnapshotError::Absent(_)) => Ok(None),
            Err(SnapshotError::Failed(msg)) => Err(err(format!("error: {msg}"))),
        }
    }

    /// Snapshot must equal the ledger entry; `visible` is the expected state.
    fn snap_matches(
        snap: &WindowSnapshot,
        want: &crate::model::WindowIdentity,
        hwnd: u64,
        visible: bool,
    ) -> bool {
        snap.hwnd == hwnd
            && snap.process == want.process
            && snap.tag == want.tag
            && snap.visible == visible
    }

    fn open_store(dir: &Path) -> Result<LedgerStore> {
        LedgerStore::open(dir).map_err(|e| match e {
            crate::storage::StorageError::LockContended => {
                err("refuse: another owner holds the store")
            }
            other => err(format!("error: store open: {other}")),
        })
    }

    fn committed_or_none(store: &LedgerStore) -> Result<Option<RecoveryLedger>> {
        match store.committed() {
            Ok(v) => Ok(v),
            Err(crate::storage::StorageError::Ledger(_)) => Err(err("refuse: corrupt ledger")),
            Err(crate::storage::StorageError::Io(e)) => {
                Err(err(format!("error: ledger read: {e}")))
            }
            Err(e) => Err(err(format!("error: store read: {e}"))),
        }
    }

    fn read_stop(dir: &Path) -> Result<Option<String>> {
        match std::fs::read_to_string(dir.join(STOP_REQUEST_FILE)) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(err(format!("error: stop read: {e}"))),
        }
    }

    fn cleanup_own_stop(dir: &Path, own_creation: &str) -> Result<()> {
        match read_stop(dir)? {
            Some(text) if stop_request_matches(&text, own_creation) => {
                match std::fs::remove_file(dir.join(STOP_REQUEST_FILE)) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(err(format!("error: stop cleanup: {e}"))),
                }
            }
            _ => Ok(()),
        }
    }

    /// Remove a stale out-of-hook workspace request left by a dead owner. Only
    /// called when no ledger is committed (no live owner), so any residue is
    /// dead and safe to drop; the live single-pending queue is never touched
    /// under a lease.
    fn cleanup_stale_workspace_request(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(super::WORKSPACE_REQUEST_FILE));
    }

    /// Remove the owner's own workspace request marker (exact creation match
    /// only). Malformed or foreign bodies are left for the owner loop's
    /// consume-once refusal, never deleted blindly here.
    fn cleanup_own_workspace_request(dir: &Path, own_creation: &str) {
        let path = dir.join(super::WORKSPACE_REQUEST_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if let Ok(request) = crate::tiling::parse_workspace_request(text.trim())
            && request.creation == own_creation
        {
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Remove a stale out-of-hook directional request left by a dead owner.
    /// Only called when no ledger is committed (no live owner), so any
    /// residue is dead and safe to drop; the live single-pending queue is
    /// never touched under a lease.
    fn cleanup_stale_directional_request(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE));
    }

    /// Remove the owner's own directional request marker (exact creation
    /// match only). Malformed or foreign bodies are left for the owner
    /// loop's consume-once refusal, never deleted blindly here.
    fn cleanup_own_directional_request(dir: &Path, own_creation: &str) {
        let path = dir.join(crate::tiling::DIRECTIONAL_REQUEST_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if let Ok(request) = crate::tiling::parse_directional_request(text.trim())
            && request.creation == own_creation
        {
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Remove the owner's own workspace-migration request marker (exact
    /// creation match only). Malformed or foreign bodies are left for the
    /// owner loop's consume-once refusal, never deleted blindly here.
    fn cleanup_own_migrate_workspace_request(dir: &Path, own_creation: &str) {
        let path = dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if let Ok(request) = crate::tiling::parse_migrate_workspace_request(text.trim())
            && request.creation == own_creation
        {
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Remove a stale out-of-hook workspace-migration request left by a dead
    /// owner. Only called when no ledger is committed (no live owner), so any
    /// residue is dead and safe to drop; the live single-pending queue is
    /// never touched under a lease.
    fn cleanup_stale_migrate_workspace_request(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(crate::tiling::MIGRATE_WORKSPACE_REQUEST_FILE));
    }

    /// Remove a stale out-of-hook output-send request left by a dead owner.
    /// Only called when no ledger is committed (no live owner), so any
    /// residue is dead and safe to drop; the live single-pending queue is
    /// never touched under a lease.
    fn cleanup_stale_send_output_request(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE));
    }

    /// Remove the owner's own output-send request marker (exact creation
    /// match only). Malformed or foreign bodies are left for the owner
    /// loop's consume-once refusal, never deleted blindly here.
    fn cleanup_own_send_output_request(dir: &Path, own_creation: &str) {
        let path = dir.join(crate::tiling::SEND_OUTPUT_REQUEST_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if let Ok(request) = crate::tiling::parse_send_output_request(text.trim())
            && request.creation == own_creation
        {
            let _ = std::fs::remove_file(&path);
        }
    }

    fn hold_helper_verified(
        pid: u32,
        expected: &crate::model::ProcessIdentity,
    ) -> Result<HeldProcess> {
        let Some(held) = open_held(pid, false, "helper")? else {
            return Err(err(format!("refuse: helper pid={pid} absent")));
        };
        let live = match held.identity() {
            Ok(l) => l,
            Err(IdentityError::Absent) => {
                return Err(err(format!("refuse: helper pid={pid} absent")));
            }
            Err(e) => return Err(err(format!("error: helper {e}"))),
        };
        if live != *expected || !held.is_alive() {
            return Err(err(format!("refuse: helper pid={pid} identity changed")));
        }
        Ok(held)
    }

    pub fn cmd_run(seconds: u64, trace: bool, hide_hwnd: Option<u64>) -> Result<String> {
        run_with_callback(seconds, trace, hide_hwnd, |dir, me, _store| {
            let deadline = Some(Instant::now() + Duration::from_secs(seconds));
            poll_stop(dir, me, deadline)
        })
    }

    fn poll_stop(
        dir: &Path,
        me: &crate::model::ProcessIdentity,
        deadline: Option<Instant>,
    ) -> Result<bool> {
        loop {
            if stop_requested(dir, me)? {
                return Ok(true);
            }
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    /// Non-blocking exact-owner stop check for product loops that poll on
    /// their own schedule.
    pub fn stop_requested(dir: &Path, me: &crate::model::ProcessIdentity) -> Result<bool> {
        match read_stop(dir)? {
            Some(text) if stop_request_matches(&text, &me.process_creation) => Ok(true),
            Some(_) | None => Ok(false),
        }
    }

    /// Product tiling loop seam: same owner lease/commit/stop checks as
    /// `run_with_callback`, but untimed (body returns on exact-owner stop).
    /// The body must poll [`stop_requested`] itself and return when it
    /// observes a request. The owner lease (`store`) is held throughout the
    /// body so the loop can persist fresh Snap preimages across
    /// suspend/resume. `snap_prevention` enables the default-on session-only
    /// `SPI_SETWINARRANGING FALSE` effect with ledger-backed conditional
    /// restoration.
    ///
    /// Dead-owner residue is reclaimed before the new owner commits. The
    /// crash watcher is managed centrally by the guarded run below: spawned
    /// before the body, stopped after the reveal, and left running when the
    /// reveal keeps residue so owner exit still triggers recovery.
    pub fn run_product(
        trace: bool,
        snap_prevention: bool,
        body: impl FnOnce(&Path, &crate::model::ProcessIdentity, &LedgerStore) -> Result<()>,
    ) -> Result<String> {
        let dir = ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        let me = medium_caller()?;
        crate::product_hide::sys::reclaim_dead_residue()?;
        match run_guarded(
            &dir,
            &me,
            GuardConfig {
                seconds: 0,
                trace,
                hide_hwnd: None,
                product: true,
                snap_prevention,
            },
            |dir, me, store| {
                body(dir, me, store)?;
                Ok(true)
            },
        ) {
            Ok(out) => Ok(out),
            Err(e) => {
                // Post-lease failures append to the per-run log; pre-lease
                // refusals (no file yet) surface without writing.
                let path = log_path_for(&dir, &me.process_creation);
                if path.exists() {
                    let msg = e.to_string();
                    let line = serde_json::json!({"event": "run-error", "error": msg}).to_string();
                    write_log_at(&path, &line, false)?;
                }
                Err(e)
            }
        }
    }

    /// Owner setup/commit/log seam; body runs on the calling thread. The spike
    /// path never takes Snap prevention: the store is passed only so the
    /// guarded lease stays shared with the product path.
    pub fn run_with_callback(
        seconds: u64,
        trace: bool,
        hide_hwnd: Option<u64>,
        body: impl FnOnce(&Path, &crate::model::ProcessIdentity, &LedgerStore) -> Result<bool>,
    ) -> Result<String> {
        let dir = ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        let me = medium_caller()?;
        match run_guarded(
            &dir,
            &me,
            GuardConfig {
                seconds,
                trace,
                hide_hwnd,
                product: false,
                snap_prevention: false,
            },
            body,
        ) {
            Ok(out) => Ok(out),
            Err(e) => {
                let path = log_path_for(&dir, &me.process_creation);
                if path.exists() {
                    let msg = e.to_string();
                    write_log_at(&path, &format!("run failed: {msg}"), false)?;
                }
                Err(e)
            }
        }
    }

    fn hide_once(
        dir: &Path,
        hwnd: u64,
        me: &crate::model::ProcessIdentity,
        expect: &crate::model::WindowIdentity,
    ) -> Result<()> {
        let helper_exe =
            crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
        let fresh = owned(hwnd, &helper_exe, &me.user_sid, me.session_id, "")?;
        if !snap_matches(&fresh, expect, hwnd, true) {
            return Err(err("refuse: hide pre-readback mismatch"));
        }
        let _held = hold_helper_verified(fresh.process.pid, &fresh.process)?;
        // Write-before-hide: ledger commit precedes this ShowWindow.
        if !crate::ffi::show_window_prev_visible(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
        ) {
            return Err(err("refuse: hide not previously visible"));
        }
        let back = owned(hwnd, &helper_exe, &me.user_sid, me.session_id, "")?;
        if !snap_matches(&back, expect, hwnd, false) {
            return Err(err("refuse: hide readback mismatch"));
        }
        write_log_for(
            dir,
            &me.process_creation,
            &format!("run hide tag={}", expect.tag),
            false,
        )
    }

    /// Session-only Snap prevention helpers. The ledger is the coordination
    /// state between setup, the active loop (suspend/resume), teardown, and
    /// crash recovery: every helper re-reads the committed preimage plus a
    /// fresh live value, so in-process and independent restore always agree.
    /// Structured JSON only, no native ids: `{"event":"mouse-snap",
    /// "phase":..., "outcome":...}`.
    fn snap_log(dir: &Path, me: &crate::model::ProcessIdentity, phase: &str, outcome: &str) {
        let line = serde_json::json!({
            "event": "mouse-snap",
            "phase": phase,
            "outcome": outcome,
        })
        .to_string();
        let _ = write_log_for(dir, &me.process_creation, &line, false);
    }

    /// Same-owner Snap ledger note preserving the committed windows. Refuses
    /// when the committed owner changed under the lease so a stale ledger can
    /// never overwrite another owner's preimage. `Some` records a preimage
    /// claim; `None` relinquishes the claim after a completed activation
    /// (verified restore or observed drift), permitting a later fresh
    /// recapture of the exact live preimage.
    fn snap_note(
        store: &LedgerStore,
        owner: &crate::model::ProcessIdentity,
        preimage: Option<MouseSnapPreimage>,
    ) -> Result<()> {
        let committed = committed_or_none(store)?;
        let Some(existing) = committed else {
            return Err(err("refuse: ledger missing under owner lease"));
        };
        if existing.owner != *owner {
            return Err(err("refuse: ledger owner changed under lease"));
        }
        let updated = RecoveryLedger {
            v: LEDGER_SCHEMA_VERSION,
            owner: owner.clone(),
            windows: existing.windows,
            mouse_snap: preimage,
        };
        store.commit(&updated).map_err(|e| match e {
            crate::storage::StorageError::DifferentOwner => {
                err("refuse: ledger owner changed under lease")
            }
            crate::storage::StorageError::Ledger(_) => err("refuse: corrupt ledger"),
            crate::storage::StorageError::LockContended => {
                err("refuse: another owner holds the store")
            }
            crate::storage::StorageError::Io(io) => err(format!("error: ledger commit: {io}")),
        })
    }

    /// Drive toward the `FALSE` effect: exact preimage capture, durable
    /// intent note before the first setter (write-before-effect), setter with
    /// exact readback. Failures degrade narrowly (tiling continues) with an
    /// honest outcome log; an uncertain readback retains the intent claim for
    /// later independent restore instead of downgrading it.
    fn snap_drive_disabled(
        dir: &Path,
        me: &crate::model::ProcessIdentity,
        store: &LedgerStore,
        phase: &str,
    ) {
        let live = match crate::mouse_snap::sys::get() {
            Ok(value) => value,
            Err(_) => {
                snap_log(dir, me, phase, "unavailable");
                return;
            }
        };
        let committed = match committed_or_none(store) {
            Ok(value) => value,
            Err(_) => {
                snap_log(dir, me, phase, "unavailable");
                return;
            }
        };
        let claimed = committed.as_ref().and_then(|record| record.mouse_snap);
        if mouse_snap_owned(claimed) && !live {
            // Already our verified effect; quiet.
            return;
        }
        match mouse_snap_setup_decision(Some(live)) {
            MouseSnapSetupDecision::AlreadyOff => {
                // Preserve the exact recorded preimage: an earlier activation
                // that captured original TRUE (mismatch downgrade or
                // unverified attempt) must never be overwritten by a later
                // AlreadyOff note. Only a successfully completed lifecycle
                // followed by an explicit recapture may replace it; this drive
                // is a retry within the same lease, not a new activation.
                if claimed.is_some_and(|record| record.original) {
                    snap_log(dir, me, phase, "preserve-preimage");
                    return;
                }
                if snap_note(
                    store,
                    me,
                    Some(MouseSnapPreimage {
                        original: false,
                        owned: false,
                    }),
                )
                .is_err()
                {
                    snap_log(dir, me, phase, "note-failed");
                    return;
                }
                snap_log(dir, me, phase, "already-off");
            }
            MouseSnapSetupDecision::Disable => {
                // Intent claim precedes the setter: a crash past this commit
                // still recovers (live `TRUE` then reads as drift-preserve,
                // live `FALSE` as ours to restore).
                if snap_note(
                    store,
                    me,
                    Some(MouseSnapPreimage {
                        original: true,
                        owned: true,
                    }),
                )
                .is_err()
                {
                    snap_log(dir, me, phase, "note-failed");
                    return;
                }
                match crate::mouse_snap::sys::set_verified(false) {
                    Ok(false) => snap_log(dir, me, phase, "disabled"),
                    Ok(true) => {
                        let _ = snap_note(
                            store,
                            me,
                            Some(MouseSnapPreimage {
                                original: true,
                                owned: false,
                            }),
                        );
                        snap_log(dir, me, phase, "mismatch");
                    }
                    Err(_) => snap_log(dir, me, phase, "unverified"),
                }
            }
            MouseSnapSetupDecision::UnknownRetain => {
                snap_log(dir, me, phase, "unavailable");
            }
        }
    }

    /// Conditional restoration: writes the exact original `TRUE` only while
    /// the live value is still our verified `FALSE`, then relinquishes the
    /// claim (`None`) under the lease so a later resume recaptures the exact
    /// live preimage instead of mistaking foreign `FALSE` for our effect.
    /// Foreign `TRUE` drift is preserved without a write and likewise
    /// relinquished. A failed relinquish retains the old claim with
    /// uncertainty logged, never implying safe recapture. Read failures
    /// retain the ledger for a later independent restore. Never fails hard:
    /// the caller's result is preserved and recovery evidence is kept while
    /// uncertain.
    fn snap_drive_restore(
        dir: &Path,
        me: &crate::model::ProcessIdentity,
        store: &LedgerStore,
        phase: &str,
    ) {
        let committed = match committed_or_none(store) {
            Ok(value) => value,
            Err(_) => {
                snap_log(dir, me, phase, "unavailable");
                return;
            }
        };
        let preimage = committed.as_ref().and_then(|record| record.mouse_snap);
        if !mouse_snap_owned(preimage) {
            return;
        }
        let live = crate::mouse_snap::sys::get().ok();
        match mouse_snap_restore_decision(preimage, live) {
            MouseSnapRestoreDecision::Restore => match crate::mouse_snap::sys::set_verified(true) {
                Ok(true) => {
                    snap_log(dir, me, phase, "restored");
                    // Verified TRUE write completes this activation:
                    // relinquish atomically under the lease. A failed clear
                    // retains the claim with uncertainty (never imply safe
                    // recapture while history is uncertain); the next drive
                    // retries from the retained claim.
                    if snap_note(store, me, None).is_err() {
                        snap_log(dir, me, phase, "note-failed");
                    }
                }
                Ok(false) => snap_log(dir, me, phase, "mismatch"),
                Err(_) => snap_log(dir, me, phase, "unavailable"),
            },
            MouseSnapRestoreDecision::PreserveDrift => {
                // Foreign TRUE drift ends this activation without a write;
                // relinquish so the completed original-TRUE claim never
                // blocks a later AlreadyOff recapture of FALSE.
                if snap_note(store, me, None).is_err() {
                    snap_log(dir, me, phase, "note-failed");
                } else {
                    snap_log(dir, me, phase, "preserved-drift");
                }
            }
            MouseSnapRestoreDecision::NoOwnedMutation => {}
            MouseSnapRestoreDecision::UncertainRetain => {
                snap_log(dir, me, phase, "unavailable");
            }
        }
    }

    /// Suspend entry for an inactive loop (fullscreen foreground): restore
    /// conditionally while tiling holds no geometry. Resume re-drives through
    /// [`snap_drive_disabled`] with a fresh preimage when the setting drifted.
    /// No-ops unless prevention was armed for this run.
    pub fn snap_suspend(dir: &Path, me: &crate::model::ProcessIdentity, store: &LedgerStore) {
        snap_drive_restore(dir, me, store, "suspend");
    }

    /// Resume entry: fresh preimage capture plus re-disable when the setting
    /// drifted during suspension (or setup degraded earlier). Retries only at
    /// these meaningful transitions, never per tick.
    pub fn snap_resume(dir: &Path, me: &crate::model::ProcessIdentity, store: &LedgerStore) {
        snap_drive_disabled(dir, me, store, "resume");
    }

    /// Bundled owner-run setup so the guarded entry keeps a narrow signature.
    struct GuardConfig {
        seconds: u64,
        trace: bool,
        hide_hwnd: Option<u64>,
        product: bool,
        snap_prevention: bool,
    }

    /// Graceful product teardown: reveal product claims on success AND error
    /// paths. Helper claims are never touched here (Phase 1 proof semantics
    /// leave them for independent restore). Structured JSON only, no native
    /// ids. Never fails hard: the caller's result is preserved. Returns true
    /// when residue was kept (the watcher must stay running in that case so
    /// owner exit still triggers recovery).
    fn teardown_product_claims(
        dir: &Path,
        me: &crate::model::ProcessIdentity,
        store: &LedgerStore,
    ) -> bool {
        let (outcomes, _) = crate::product_hide::sys::reveal_all_product(store, me);
        if outcomes.is_empty() {
            return false;
        }
        let summary: Vec<&str> = outcomes
            .iter()
            .map(|o| match o {
                crate::product_hide::ProductTeardown::Revealed => "revealed",
                crate::product_hide::ProductTeardown::AlreadyVisible => "already-visible",
                crate::product_hide::ProductTeardown::Retired => "retired",
                crate::product_hide::ProductTeardown::Uncertain => "uncertain",
            })
            .collect();
        let uncertain = crate::product_hide::teardown_keep_ledger(&outcomes);
        let line = serde_json::json!({
            "event": "product-hide-teardown",
            "outcomes": summary,
            "residue": uncertain,
        })
        .to_string();
        let _ = write_log_for(dir, &me.process_creation, &line, false);
        uncertain
    }

    fn run_guarded(
        dir: &Path,
        me: &crate::model::ProcessIdentity,
        config: GuardConfig,
        body: impl FnOnce(&Path, &crate::model::ProcessIdentity, &LedgerStore) -> Result<bool>,
    ) -> Result<String> {
        let GuardConfig {
            seconds,
            trace,
            hide_hwnd,
            product,
            snap_prevention,
        } = config;
        let store = open_store(dir)?;
        if committed_or_none(&store)?.is_some() {
            return Err(err("refuse: ledger committed until stopped cleanup"));
        }
        cleanup_own_stop(dir, &me.process_creation)?;
        // No live owner holds the lease here: any workspace request residue is
        // dead and must not block the new owner.
        cleanup_stale_workspace_request(dir);
        cleanup_stale_directional_request(dir);
        cleanup_stale_send_output_request(dir);
        cleanup_stale_migrate_workspace_request(dir);
        // Same lease, same guarantee: no live project icon exists, so prune
        // a crash ghost through the stable GUID before this owner adds its
        // own (tolerant, unclaimed).
        let _ = crate::tray_sys::remove_stale_icon();
        let windows = if let Some(hwnd) = hide_hwnd {
            let helper_exe =
                crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
            let snap = owned(hwnd, &helper_exe, &me.user_sid, me.session_id, "")?;
            if !snap.visible {
                return Err(err("refuse: target not visible"));
            }
            vec![crate::model::WindowIdentity {
                hwnd: snap.hwnd,
                process: snap.process.clone(),
                tag: snap.tag.clone(),
                kind: crate::model::WindowClaimKind::Helper,
                show: crate::model::WindowShowState::default(),
            }]
        } else {
            Vec::new()
        };
        let span = if seconds == 0 {
            "untimed".to_owned()
        } else {
            format!("seconds={seconds}")
        };
        // Per-run unique log: refuse when the file already exists so one
        // owner run can never overwrite another. The ledger (committed below)
        // persists the owner creation, and `cmd_ready` derives the same path.
        let log_file = log_path_for(dir, &me.process_creation);
        if log_file.exists() {
            return Err(err("refuse: log preexists, will not overwrite"));
        }
        if product {
            // Product log vocabulary: structured JSON, no native ids (no pid,
            // creation, SID, or session). Legacy spike/dev lines stay below.
            write_log_at(
                &log_file,
                "{\"event\":\"run-start\",\"mode\":\"product\"}",
                true,
            )?;
        } else {
            write_log_at(&log_file, &format!("run start pid={} {span}", me.pid), true)?;
        }
        if trace && !product {
            write_log_at(
                &log_file,
                &format!(
                    "run owner pid={} creation={} sid={} session={}",
                    me.pid, me.process_creation, me.user_sid, me.session_id
                ),
                false,
            )?;
        }
        // Sanitized panic hook: installed after the owner log exists (the
        // hook captures the path internally, never logs it) and before
        // worker threads/native startup below. Restored on normal scope
        // end when the guard drops; unwind/abort/exit policy unchanged.
        let _panic_diag = crate::panic_diag::PanicDiagGuard::install(log_file.clone());
        point_current(dir, &me.process_creation)?;
        let record = RecoveryLedger {
            v: LEDGER_SCHEMA_VERSION,
            owner: (*me).clone(),
            windows,
            mouse_snap: None,
        };
        store
            .commit(&record)
            .map_err(|e| err(format!("error: ledger commit: {e}")))?;
        // Product crash watcher starts after the commit and before the body
        // (no hide can precede it) so a crash past any later commit still
        // auto-reveals. Spike/proof runs never spawn one.
        let mut watcher: Option<std::process::Child> = None;
        if product {
            match crate::product_hide::sys::spawn_watcher(me, dir) {
                Ok(child) => watcher = Some(child),
                Err(e) => {
                    let _ = std::fs::remove_file(dir.join(LEDGER_FILE_NAME));
                    cleanup_stale_workspace_request(dir);
                    cleanup_stale_directional_request(dir);
                    cleanup_stale_send_output_request(dir);
                    cleanup_stale_migrate_workspace_request(dir);
                    return Err(e);
                }
            }
        }
        // Mouse prevention is deferred until the loop observes an actual
        // active tick or resume: setup here must not disable while a
        // fullscreen foreground is already holding the session, only to have
        // the loop immediately restore. The loop drives the first effect via
        // resume/setup phases; teardown below plus crash restore keep the
        // lease-held guarantee. Proof and spike paths pass false.
        let snap_want = product && snap_prevention;
        if let Some(hwnd) = hide_hwnd {
            let expect = record
                .windows
                .first()
                .ok_or_else(|| err("error: ledger empty"))?;
            hide_once(dir, hwnd, me, expect)?;
        }
        // Scope-guard/finalizer shape: teardown runs on ordinary errors too.
        // It only restores while the live value is still ours and never
        // overwrites the body's result; uncertain reads retain the ledger.
        let body_result = body(dir, me, &store);
        // Ordinary callback-return marker: distinguishes a returned `Err`
        // from no return at all (abrupt termination leaves no marker, and
        // the existing `run-end` below is skipped on `Err`). Outcome only,
        // never the error content.
        let callback_outcome = if body_result.is_ok() { "ok" } else { "err" };
        let _ = write_log_at(
            &log_file,
            &format!(
                "{{\"event\":\"owner-exit\",\"phase\":\"callback-returned\",\"outcome\":\"{callback_outcome}\"}}"
            ),
            false,
        );
        // The request queue never outlives its owner: drop our own pending
        // marker (exact creation match) on every teardown path so stop and
        // restore observe no residue.
        cleanup_own_workspace_request(dir, &me.process_creation);
        cleanup_own_directional_request(dir, &me.process_creation);
        cleanup_own_send_output_request(dir, &me.process_creation);
        cleanup_own_migrate_workspace_request(dir, &me.process_creation);
        if snap_want {
            snap_drive_restore(dir, me, &store, "teardown");
        }
        // The watcher stops only after the reveal: killing it first would
        // leave a crash window with committed hides and no recovery. When
        // the reveal keeps residue the watcher stays running so owner exit
        // still triggers recovery.
        let mut keep_recovery = false;
        if product {
            keep_recovery = teardown_product_claims(dir, me, &store);
        }
        if let Some(mut child) = watcher
            && !keep_recovery
        {
            crate::product_hide::sys::stop_watcher(&mut child, dir, &me.process_creation);
        }
        let stopped = body_result?;
        let log_path = log_file.to_string_lossy().into_owned();
        if product {
            let status = if stopped { "stopped" } else { "expired" };
            write_log_at(
                &log_file,
                &format!("{{\"event\":\"run-end\",\"status\":\"{status}\"}}"),
                false,
            )?;
            Ok(serde_json::json!({"status": status, "log_path": log_path}).to_string())
        } else {
            let (status, message) = if stopped {
                ("stopped", "run stop-request observed")
            } else {
                ("expired", "run deadline expired")
            };
            write_log_at(&log_file, message, false)?;
            Ok(
                serde_json::json!({"status": status, "owner": me, "log_path": log_path})
                    .to_string(),
            )
        }
    }

    fn caller_owns(me: &crate::model::ProcessIdentity, record: &RecoveryLedger) -> Result<bool> {
        if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
            return Ok(false);
        }
        let exe = current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
        Ok(super::exe_paths_equal(&exe, &record.owner.exe_path))
    }

    fn target_verified(held: &HeldProcess, _pid: u32) -> Result<()> {
        let rid = held
            .integrity()
            .map_err(|e| err(format!("error: target integrity {e}")))?;
        if !is_medium_rid(rid) {
            return Err(err(format!("refuse: target integrity {rid} is not medium")));
        }
        Ok(())
    }

    fn not_ready() -> Result<String> {
        Ok(serde_json::json!({"ready": false}).to_string())
    }

    pub fn cmd_ready() -> Result<String> {
        let dir: PathBuf =
            ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        let Some(record) = unlocked(&dir)? else {
            return not_ready();
        };
        if !caller_owns(&medium_caller()?, &record)? {
            return not_ready();
        }
        let Some((held, live)) = held_live(record.owner.pid, false)? else {
            return not_ready();
        };
        if live != record.owner || !held.is_alive() {
            return not_ready();
        }
        match target_verified(&held, record.owner.pid) {
            Ok(()) => {}
            Err(e) if e.to_string().starts_with("refuse:") => {
                return not_ready();
            }
            Err(e) => return Err(e),
        }
        if !record.windows.is_empty() {
            let helper_exe =
                crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
            for w in &record.windows {
                if w.kind == crate::model::WindowClaimKind::Product {
                    // Hidden claims verify identity-only: a safely hidden
                    // retained member may read maximized or captionless, which
                    // the strict admission gates would refuse.
                    let snap = match crate::product_hide::sys::query_recovery(w.hwnd, &record.owner)
                    {
                        Ok(s) => s,
                        Err(_) => return not_ready(),
                    };
                    if snap.process != w.process
                        || snap.nonce.as_deref() != Some(w.tag.as_str())
                        || snap.visible
                    {
                        return not_ready();
                    }
                    continue;
                }
                let Some(snap) = owned_opt(
                    w.hwnd,
                    &helper_exe,
                    &record.owner.user_sid,
                    record.owner.session_id,
                )?
                else {
                    return not_ready();
                };
                if !snap_matches(&snap, w, w.hwnd, false) {
                    return not_ready();
                }
            }
        }
        let log_path = log_path_for(&dir, &record.owner.process_creation)
            .to_string_lossy()
            .into_owned();
        Ok(
            serde_json::json!({"ready": true, "owner": record.owner, "log_path": log_path})
                .to_string(),
        )
    }

    fn unlocked(dir: &Path) -> Result<Option<RecoveryLedger>> {
        match std::fs::read(dir.join(LEDGER_FILE_NAME)) {
            Ok(bytes) => {
                let text =
                    std::str::from_utf8(&bytes).map_err(|_| err("refuse: corrupt ledger"))?;
                parse_ledger(text)
                    .map(Some)
                    .map_err(|_| err("refuse: corrupt ledger"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(err(format!("error: ledger read: {e}"))),
        }
    }

    fn reveal_once(
        w: &crate::model::WindowIdentity,
        helper_exe: &str,
        owner: &crate::model::ProcessIdentity,
    ) -> Result<()> {
        let pre = format!("restore pre-read {} ", w.hwnd);
        let fresh = owned(w.hwnd, helper_exe, &owner.user_sid, owner.session_id, &pre)?;
        if !snap_matches(&fresh, w, w.hwnd, false) {
            return Err(err(format!(
                "refuse: restore pre-read mismatch hwnd={}",
                w.hwnd
            )));
        }
        let _held = hold_helper_verified(fresh.process.pid, &fresh.process)?;
        crate::ffi::show_window(
            w.hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNA,
        );
        let ctx = format!("restore readback {} ", w.hwnd);
        let back = owned(w.hwnd, helper_exe, &owner.user_sid, owner.session_id, &ctx)?;
        if !snap_matches(&back, w, w.hwnd, true) {
            return Err(err(format!(
                "refuse: restore readback mismatch hwnd={}",
                w.hwnd
            )));
        }
        Ok(())
    }

    /// Strict owner-death check: absent pid or identity is dead; access and
    /// other failures are errors (never treated as absent or alive).
    fn owner_dead(expected: &crate::model::ProcessIdentity) -> Result<bool> {
        let held = match HeldProcess::open(expected.pid) {
            Ok(h) => h,
            Err(IdentityError::Absent) => return Ok(true),
            Err(e) => return Err(err(format!("error: owner {e}"))),
        };
        let live = match held.identity() {
            Ok(l) => l,
            Err(IdentityError::Absent) => return Ok(true),
            Err(e) => return Err(err(format!("error: owner {e}"))),
        };
        Ok(live != *expected || !held.is_alive())
    }

    /// Dead-owner independent restore under an already-held lease: verified
    /// hidden windows (helper and product claims) plus the optional Snap
    /// preimage. `expected` binds the exact owner while the lock is held; a
    /// replacement ledger refuses with `owner-replaced` instead of touching
    /// it. Product nonces leave only after a verified reveal or release;
    /// missing/destroyed/recycled windows retire with no writes; uncertain
    /// identities refuse and retain the ledger. Returns the window count and
    /// whether a Snap write ran.
    pub(crate) fn restore_under_lease(
        store: &LedgerStore,
        expected: Option<&crate::model::ProcessIdentity>,
    ) -> Result<(usize, bool)> {
        let dir = ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        let me = medium_caller()?;
        let Some(record) = committed_or_none(store)? else {
            match read_stop(&dir)? {
                Some(text) if !stop_request_matches(&text, &me.process_creation) => {
                    return Err(err("refuse: orphan stop.request"));
                }
                Some(_) => cleanup_own_stop(&dir, &me.process_creation)?,
                None => {}
            }
            cleanup_own_workspace_request(&dir, &me.process_creation);
            cleanup_own_directional_request(&dir, &me.process_creation);
            cleanup_own_send_output_request(&dir, &me.process_creation);
            cleanup_own_migrate_workspace_request(&dir, &me.process_creation);
            // No committed owner means no live icon: prune a crash ghost, if
            // any, through the stable project GUID (tolerant, unclaimed).
            let _ = crate::tray_sys::remove_stale_icon();
            return Ok((0, false));
        };
        if !caller_owns(&me, &record)? {
            return Err(err("refuse: owner mismatch"));
        }
        if let Some(want) = expected
            && !watcher_may_restore(want, &record.owner)
        {
            return Err(err("refuse: owner-replaced"));
        }
        // Owner must be dead; PID reuse with different full identity is dead.
        if !owner_dead(&record.owner)? {
            return Err(err("refuse: owner still active"));
        }
        // The owner is dead, so no live project icon exists: prune the crash
        // ghost through the stable GUID before revealing windows (tolerant,
        // unclaimed; same-user trust like every recovery step here).
        let _ = crate::tray_sys::remove_stale_icon();
        let helper_exe =
            crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
        for w in &record.windows {
            if w.kind == crate::model::WindowClaimKind::Product {
                match crate::product_hide::sys::reveal_product_claim(store, &record.owner, w) {
                    Ok(_) => {}
                    Err(e) => {
                        let msg = e.to_string();
                        // Retired claims already cleaned their ledger note;
                        // uncertain identities keep the residue.
                        if msg.starts_with("uncertain:") || msg.starts_with("error:") {
                            return Err(e);
                        }
                        return Err(err(format!("refuse: restore hwnd={} {msg}", w.hwnd)));
                    }
                }
                continue;
            }
            let ctx = format!("restore hwnd={} ", w.hwnd);
            let snap = owned(
                w.hwnd,
                &helper_exe,
                &record.owner.user_sid,
                record.owner.session_id,
                &ctx,
            )?;
            let observed = crate::model::ObservedWindow {
                identity: crate::model::WindowIdentity {
                    hwnd: snap.hwnd,
                    process: snap.process.clone(),
                    tag: snap.tag.clone(),
                    kind: crate::model::WindowClaimKind::Helper,
                    show: crate::model::WindowShowState::default(),
                },
                visible: snap.visible,
            };
            match restore_eligibility(w, std::slice::from_ref(&observed)) {
                crate::model::RestoreDecision::SkipAlreadyVisible { .. } => {}
                crate::model::RestoreDecision::Reveal { .. } => {
                    reveal_once(w, &helper_exe, &record.owner)?;
                }
                crate::model::RestoreDecision::Refuse { reason } => {
                    return Err(err(format!("refuse: restore hwnd={} {reason:?}", w.hwnd)));
                }
            }
        }
        let mut snap_restored = false;
        if mouse_snap_owned(record.mouse_snap) {
            let live = crate::mouse_snap::sys::get().ok();
            match mouse_snap_restore_decision(record.mouse_snap, live) {
                MouseSnapRestoreDecision::Restore => {
                    match crate::mouse_snap::sys::set_verified(true) {
                        Ok(true) => snap_restored = true,
                        Ok(false) => {
                            return Err(err("error: mouse-snap restore mismatch"));
                        }
                        Err(e) => return Err(e),
                    }
                }
                // Foreign `TRUE` drift: preserve it, clean the resolved claim
                // with the windows below. No write under any drift.
                MouseSnapRestoreDecision::PreserveDrift => {}
                MouseSnapRestoreDecision::NoOwnedMutation => {}
                MouseSnapRestoreDecision::UncertainRetain => {
                    return Err(err("error: mouse-snap preimage read failed"));
                }
            }
        }
        let count = record.windows.len();
        cleanup_own_stop(&dir, &record.owner.process_creation)?;
        cleanup_own_workspace_request(&dir, &record.owner.process_creation);
        cleanup_own_directional_request(&dir, &record.owner.process_creation);
        cleanup_own_send_output_request(&dir, &record.owner.process_creation);
        cleanup_own_migrate_workspace_request(&dir, &record.owner.process_creation);
        std::fs::remove_file(dir.join(LEDGER_FILE_NAME))
            .map_err(|e| err(format!("error: ledger cleanup: {e}")))?;
        Ok((count, snap_restored))
    }

    fn restore_locked() -> Result<(usize, bool)> {
        let dir = ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        if !dir.exists() {
            return Ok((0, false));
        }
        let store = open_store(&dir)?;
        restore_under_lease(&store, None)
    }

    pub fn cmd_restore() -> Result<String> {
        let (windows, snap_restored) = restore_locked()?;
        Ok(serde_json::json!({
            "restored": true,
            "windows": windows,
            "ledger_cleaned": true,
            "mouse_snap_restored": snap_restored,
        })
        .to_string())
    }

    pub fn cmd_stop(terminate: bool) -> Result<String> {
        let dir: PathBuf =
            ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
        let Some(record) = unlocked(&dir)? else {
            return Ok(stop_json(terminate, false, false));
        };
        if !caller_owns(&medium_caller()?, &record)? {
            return Err(err("refuse: owner mismatch"));
        }
        let held = match held_live(record.owner.pid, terminate)? {
            Some((held, live)) if live == record.owner => held,
            _ => return Ok(stop_json(terminate, true, true)),
        };
        target_verified(&held, record.owner.pid)?;
        if terminate {
            held.terminate()
                .map_err(|e| err(format!("error: terminate {e}")))?;
        } else {
            // Atomic stop publish (same-directory temp plus rename-replace,
            // overwriting any pending stop): the ledger watch carries
            // file-name events only, so the signaled rename must expose
            // complete content — a direct write could wake the owner on the
            // create before the body lands and lose the stop. Overwrite
            // semantics preserved: repeated stops replace, never refuse.
            match crate::storage::publish_replace(
                &dir,
                STOP_REQUEST_FILE,
                record.owner.process_creation.as_bytes(),
                "stop",
            ) {
                Ok(()) => {}
                Err(e) => {
                    return Err(err(format!("error: stop request write: {e}")));
                }
            }
        }
        let exited = held.wait(STOP_WAIT_MS);
        if !exited {
            return Err(err("error: owner exit timeout"));
        }
        // The owner loop consumes its request once, but a stop racing a
        // pending dispatch must leave no marker behind: drop the exact
        // owner's residue (creation match only) after a verified exit.
        cleanup_own_workspace_request(&dir, &record.owner.process_creation);
        cleanup_own_directional_request(&dir, &record.owner.process_creation);
        cleanup_own_send_output_request(&dir, &record.owner.process_creation);
        cleanup_own_migrate_workspace_request(&dir, &record.owner.process_creation);
        Ok(stop_json(terminate, true, true))
    }

    fn stop_json(terminate: bool, exited: bool, recovery_required: bool) -> String {
        let key = if terminate {
            "emergency_stopped"
        } else {
            "stopped"
        };
        serde_json::json!({key: exited, "owner_exited": exited, "recovery_required": recovery_required})
            .to_string()
    }
}
