#![forbid(unsafe_code)]

//! Owned disposable AppKit probe-target descriptor and session control.
//!
//! The target runs in a SEPARATE process (`tiler-macos-probe-target`): an
//! AppKit window created on the main thread only. Identity is the exact
//! `Child` handle plus its PID, a stdout handshake (`bundle`, exact child
//! PID, issued nonce) over the child's own pipe, the recorded executable
//! path, and a CURRENT live binding revalidated on every cross-command use:
//! a fresh CGWindowList snapshot must show a window owned by exactly that
//! PID whose title carries exactly the recorded per-run nonce — never a
//! caller-typed PID alone. When the window server withholds CG names, a
//! minimal owned AX title fallback applies only with Accessibility trust
//! actually observed true (bounded worker, owned windows only, exact
//! compare, no title logging). [`spawn_verified`] refuses to return until the
//! child proves itself, and [`stop_session`] revalidates the live executable
//! path plus the live window nonce before signalling. AX/input/Space code
//! must validate that identity before any mutation and must never fall back
//! to Ghostty, the controlling terminal, or an arbitrary window.
//!
//! There is no `NSRunningApplication.launchDate` token anywhere: directly
//! spawned helpers have no LaunchServices entry, and the live window-nonce
//! title is the current-process proof (a stale record plus a same-executable
//! PID reuse carries a different nonce title and is refused).
//!
//! `hung_ms` / `slow_ms` stall the owned target *after* its window is
//! serviced (ready marker printed), so AX timeout handling is measurable
//! against a live unresponsive window without touching foreign apps.

use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ownership::validate_nonce;

/// Fixed window-title prefix for the owned helper. The full title is
/// [`probe_window_title`]: prefix plus the per-run nonce, so the CURRENT
/// live helper is distinguishable from any same-executable PID reuse.
pub const PROBE_WINDOW_TITLE_PREFIX: &str = "OmniTiler owned probe ";

/// Window title the helper sets for `nonce`. Pure; unit-tested with
/// [`title_nonce`] round-trip below.
pub fn probe_window_title(nonce: &str) -> String {
    format!("{PROBE_WINDOW_TITLE_PREFIX}{nonce}")
}

/// Parse the per-run nonce out of a live window title. Returns `Some`
/// only for the exact prefix plus a valid nonce (`>=16` hex chars);
/// anything else (foreign titles, truncated/padded text) is `None`.
pub fn title_nonce(title: Option<&str>) -> Option<String> {
    let title = title?;
    let nonce = title.strip_prefix(PROBE_WINDOW_TITLE_PREFIX)?;
    validate_nonce(nonce).ok()?;
    Some(nonce.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetMode {
    Normal,
    Hung,
    Slow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeTargetSpec {
    pub nonce: String,
    pub mode: TargetMode,
    pub hung_ms: u64,
    pub seconds: u64,
    /// Owned fault control for the partial-write leg: a non-resizable window
    /// accepts AX position writes but refuses AX size writes, so a frame
    /// transaction against it exercises the exact-preimage restore path live.
    /// Default off; enabled via `--fixed-size` (probe binary and
    /// `probe-spawn`/`ax-session` holders).
    pub fixed_size: bool,
    /// Owned determinism control for the creation-notification leg: the
    /// helper pumps its runloop this many milliseconds AFTER process start
    /// (identity printed, app AX-registered) but BEFORE creating its window,
    /// so a pre-window observer deterministically covers the creation.
    /// Default 0 (window immediately); the session holder uses 2000.
    /// Clamped to 20000 in the helper, like the stall bounds.
    pub defer_window_ms: u64,
    /// Owned teardown stimulus for the destroyed-notification leg: the
    /// helper closes its OWN window this many milliseconds after printing
    /// `ready`, while the joined observer still pumps, and keeps running
    /// (process alive, no windows) until its seconds deadline or the exact
    /// holder stop. Default 0 (never self-closes); the session holder uses
    /// 9000. Clamped to 20000 in the helper.
    pub close_window_ms: u64,
}

impl ProbeTargetSpec {
    pub fn new(nonce: &str, hung_ms: u64, slow_ms: u64, seconds: u64) -> Result<Self, String> {
        validate_nonce(nonce)?;
        let seconds = crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
        if hung_ms > crate::config::MAX_AX_TIMEOUT_MS * 4
            || slow_ms > crate::config::MAX_AX_TIMEOUT_MS * 4
        {
            return Err("hung/slow delay out of bound (max 20000ms)".to_owned());
        }
        let mode = if hung_ms > 0 {
            TargetMode::Hung
        } else if slow_ms > 0 {
            TargetMode::Slow
        } else {
            TargetMode::Normal
        };
        Ok(Self {
            nonce: nonce.to_owned(),
            mode,
            hung_ms: hung_ms.max(slow_ms),
            seconds,
            fixed_size: false,
            defer_window_ms: 0,
            close_window_ms: 0,
        })
    }

    /// Enable the non-resizable fault-control window. Builder (not a `new`
    /// parameter) so existing call sites keep their shape.
    pub fn with_fixed_size(mut self, fixed_size: bool) -> Self {
        self.fixed_size = fixed_size;
        self
    }

    /// Defer window creation by `ms` (see field docs). Builder; clamped to
    /// 20000 in the helper.
    pub fn with_defer_window_ms(mut self, ms: u64) -> Self {
        self.defer_window_ms = ms;
        self
    }

    /// Self-close the owned window `ms` after `ready` (see field docs).
    /// Builder; clamped to 20000 in the helper.
    pub fn with_close_window_ms(mut self, ms: u64) -> Self {
        self.close_window_ms = ms;
        self
    }

    /// argv fragment for spawning the helper (no shell; exact args).
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec![
            "--nonce".to_owned(),
            self.nonce.clone(),
            "--hung-ms".to_owned(),
            self.hung_ms.to_string(),
            "--seconds".to_owned(),
            self.seconds.to_string(),
        ];
        if self.fixed_size {
            args.push("--fixed-size".to_owned());
        }
        if self.defer_window_ms > 0 {
            args.push("--defer-window-ms".to_owned());
            args.push(self.defer_window_ms.to_string());
        }
        if self.close_window_ms > 0 {
            args.push("--close-window-ms".to_owned());
            args.push(self.close_window_ms.to_string());
        }
        args
    }
}

/// Generate a 32-hex-char nonce from the OS CSPRNG (Unix `/dev/urandom`,
/// std only, no new dependency). Fails closed when entropy is unavailable:
/// never a predictable hash of time/PID. Non-Unix builds fail closed here;
/// callers treat `Err` as refusing the spawn.
pub fn mint_nonce() -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut bytes = [0u8; 16];
        let mut f = std::fs::File::open("/dev/urandom")
            .map_err(|e| format!("CSPRNG unavailable (/dev/urandom): {e}"))?;
        f.read_exact(&mut bytes)
            .map_err(|e| format!("CSPRNG read failed: {e}"))?;
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
    #[cfg(not(unix))]
    {
        Err("CSPRNG unavailable on this platform (no /dev/urandom); refusing".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_modes() {
        let s = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10).unwrap();
        assert_eq!(s.mode, TargetMode::Normal);
        let s = ProbeTargetSpec::new("0123456789abcdef", 500, 0, 10).unwrap();
        assert_eq!(s.mode, TargetMode::Hung);
        assert!(ProbeTargetSpec::new("short", 0, 0, 10).is_err());
        assert!(ProbeTargetSpec::new("0123456789abcdef", 0, 0, 0).is_err());
    }

    #[test]
    fn fixed_size_is_opt_in_and_reaches_helper_argv() {
        let plain = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10).unwrap();
        assert!(!plain.fixed_size);
        assert!(!plain.argv().contains(&"--fixed-size".to_owned()));
        let fixed = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10)
            .unwrap()
            .with_fixed_size(true);
        assert!(fixed.fixed_size);
        let argv = fixed.argv();
        assert!(argv.contains(&"--fixed-size".to_owned()));
        // Flag position is stable: appended after the --seconds value.
        assert_eq!(argv[argv.len() - 2], "10");
        assert_eq!(argv[argv.len() - 1], "--fixed-size");
    }

    #[test]
    fn defer_window_ms_is_opt_in_and_reaches_helper_argv() {
        let plain = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10).unwrap();
        assert_eq!(plain.defer_window_ms, 0);
        assert!(!plain.argv().contains(&"--defer-window-ms".to_owned()));
        let deferred = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10)
            .unwrap()
            .with_defer_window_ms(2000);
        assert_eq!(deferred.defer_window_ms, 2000);
        let argv = deferred.argv();
        let flag = argv
            .iter()
            .position(|a| a == "--defer-window-ms")
            .expect("flag present");
        assert_eq!(argv[flag + 1], "2000");
    }

    #[test]
    fn close_window_ms_is_opt_in_and_reaches_helper_argv() {
        let plain = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10).unwrap();
        assert_eq!(plain.close_window_ms, 0);
        assert!(!plain.argv().contains(&"--close-window-ms".to_owned()));
        let closing = ProbeTargetSpec::new("0123456789abcdef", 0, 0, 10)
            .unwrap()
            .with_close_window_ms(9000);
        assert_eq!(closing.close_window_ms, 9000);
        let argv = closing.argv();
        let flag = argv
            .iter()
            .position(|a| a == "--close-window-ms")
            .expect("flag present");
        assert_eq!(argv[flag + 1], "9000");
    }

    // Unix-only: reads `/dev/urandom`, which does not exist on Windows.
    // Windows builds exercise the fail-closed stub path instead.
    #[cfg(unix)]
    #[test]
    fn minted_nonces_validate_and_differ() {
        let a = mint_nonce().unwrap();
        let b = mint_nonce().unwrap();
        assert!(validate_nonce(&a).is_ok());
        assert!(validate_nonce(&b).is_ok());
        assert_ne!(a, b, "two CSPRNG nonces must differ");
    }

    #[cfg(not(unix))]
    #[test]
    fn nonce_fails_closed_without_unix_entropy() {
        assert!(mint_nonce().is_err());
    }

    #[test]
    fn session_record_round_trips() {
        let record = SpawnRecord {
            pid: 4242,
            nonce: "0123456789abcdef".to_owned(),
            helper_path: "/nonexistent/tiler-macos-probe-target".to_owned(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: SpawnRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
        assert_eq!(
            session_record_path(4242).file_name().unwrap(),
            "session-4242.json"
        );
    }

    #[test]
    fn record_mismatch_refused_before_live_checks() {
        // A forged record for an arbitrary PID with the wrong nonce must be
        // refused on pid/nonce mismatch without needing a live process.
        let pid = u32::MAX - 4242;
        let record = SpawnRecord {
            pid,
            nonce: "0123456789abcdef0123456789abcdef".to_owned(),
            helper_path: "x".to_owned(),
        };
        write_stale_record_for_test(&record);
        let err = load_verified_record(pid, "ffffffffffffffffffffffffffffffff").unwrap_err();
        assert!(err.contains("mismatch"), "unexpected: {err}");
        let _ = std::fs::remove_file(session_record_path(pid));
    }

    #[test]
    fn unchanged_guard_fails_closed_without_record() {
        // No record exists: the write-boundary guard refuses before any
        // comparison, so a missing baseline never authorizes a write.
        let pid = u32::MAX - 4243;
        let original = SpawnRecord {
            pid,
            nonce: "0123456789abcdef0123456789abcdef".to_owned(),
            helper_path: "x".to_owned(),
        };
        let err = verify_record_unchanged(&original, pid, "0123456789abcdef0123456789abcdef")
            .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn handshake_requires_exact_child_pid_and_nonce() {
        assert!(
            check_handshake_identity(
                crate::BUNDLE_ID,
                4242,
                "0123456789abcdef",
                4242,
                "0123456789abcdef"
            )
            .is_ok()
        );
        // Wrong bundle, PID, or nonce each fail closed.
        assert!(
            check_handshake_identity(
                "com.example.other",
                4242,
                "0123456789abcdef",
                4242,
                "0123456789abcdef"
            )
            .is_err()
        );
        assert!(
            check_handshake_identity(
                crate::BUNDLE_ID,
                4243,
                "0123456789abcdef",
                4242,
                "0123456789abcdef"
            )
            .is_err()
        );
        assert!(
            check_handshake_identity(
                crate::BUNDLE_ID,
                4242,
                "ffffffffffffffff",
                4242,
                "0123456789abcdef"
            )
            .is_err()
        );
    }

    #[test]
    fn window_title_round_trips_exact_nonce() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let title = probe_window_title(nonce);
        assert_eq!(title_nonce(Some(&title)), Some(nonce.to_owned()));
        // Foreign, truncated, or padded titles never parse.
        assert!(title_nonce(Some("OmniTiler owned probe")).is_none());
        assert!(title_nonce(Some("some other window")).is_none());
        assert!(title_nonce(Some("OmniTiler owned probe short")).is_none());
        assert!(title_nonce(Some(&format!("{title} "))).is_none());
        assert!(title_nonce(None).is_none());
    }
}

/// Durable spawn record: what `probe-stop` and every owned read/write path
/// revalidate before signalling or writing. Written at spawn (after the
/// ready handshake), removed at verified stop. No start-identity token:
/// currency is proved by the live window-nonce title (see
/// [`crate::cg::verify_live_nonce`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRecord {
    pub pid: u32,
    pub nonce: String,
    /// Canonical path of the helper binary that was actually spawned.
    pub helper_path: String,
}

fn session_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("omnitiler-macos-probe")
}

/// Canonical record path for a PID. Callers must go through
/// [`load_verified_record`]/[`stop_session`], never trust this path alone.
pub fn session_record_path(pid: u32) -> std::path::PathBuf {
    session_dir().join(format!("session-{pid}.json"))
}

/// Ensure the private owned state dir: never a symlink, mode `0700`, owned
/// by the current UID. Fails closed on any deviation.
fn ensure_private_session_dir() -> Result<std::path::PathBuf, String> {
    let dir = session_dir();
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("session dir is a symlink; refusing".to_owned());
            }
            if !meta.is_dir() {
                return Err("session dir is not a directory; refusing".to_owned());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&dir).map_err(|e| format!("session dir: {e}"))?;
        }
        Err(e) => return Err(format!("session dir metadata: {e}")),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("session dir permissions: {e}"))?;
        let meta = std::fs::metadata(&dir).map_err(|e| format!("session dir metadata: {e}"))?;
        if meta.mode() & 0o777 != 0o700 {
            return Err("session dir mode is not 0700; refusing".to_owned());
        }
        if meta.uid() != crate::ffi::current_uid() {
            return Err("session dir not owned by current UID; refusing".to_owned());
        }
        if std::fs::symlink_metadata(&dir)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            return Err("session dir became a symlink; refusing".to_owned());
        }
    }
    Ok(dir)
}

/// Reject anything that is not a plain regular file at exactly this path
/// (no symlinks, no dirs, no foreign mounts tricks at the leaf).
fn read_record_file(path: &std::path::Path) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| "no spawn record; refusing blind control".to_owned())?;
    if meta.file_type().is_symlink() {
        return Err("spawn record is a symlink; refusing".to_owned());
    }
    if !meta.is_file() {
        return Err("spawn record is not a regular file; refusing".to_owned());
    }
    std::fs::read_to_string(path).map_err(|_| "spawn record unreadable; refusing".to_owned())
}

/// Canonical path of the helper we are willing to run or trust. The recorded
/// executable is constrained to exactly this.
fn expected_helper_canonical() -> Result<String, String> {
    let current = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    let dir = current
        .parent()
        .ok_or_else(|| "current exe has no parent dir".to_owned())?;
    let sibling = dir.join("tiler-macos-probe-target");
    if !sibling.is_file()
        || std::fs::symlink_metadata(&sibling)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(
            "probe helper 'tiler-macos-probe-target' not found beside the CLI binary (or is a symlink)".to_owned(),
        );
    }
    sibling
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("helper canonicalize: {e}"))
}

fn helper_binary_next_to_current_exe() -> Result<std::path::PathBuf, String> {
    expected_helper_canonical().map(std::path::PathBuf::from)
}

/// Write a fresh spawn record with `create_new` (never truncate a live
/// record) and mode `0600`. A pre-existing record for a still-live owned
/// process refuses; a stale record (dead PID or foreign content) is
/// replaced atomically. Test-only seam below shares the stale check.
fn write_spawn_record(record: &SpawnRecord) -> Result<(), String> {
    ensure_private_session_dir()?;
    let path = session_record_path(record.pid);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err("spawn record path is a symlink; refusing".to_owned());
        }
        if meta.is_file()
            && let Ok(raw) = std::fs::read_to_string(&path)
            && let Ok(existing) = serde_json::from_str::<SpawnRecord>(&raw)
            && existing.pid == record.pid
            && crate::ownership::verify_live_process(existing.pid, Some(&existing.helper_path))
                .is_ok()
        {
            return Err(format!(
                "active spawn record already exists for pid {}; refusing",
                record.pid
            ));
        }
        // Stale record: replace atomically via a fresh temp + rename.
        let tmp = session_dir().join(format!("session-{}.tmp", record.pid));
        let _ = std::fs::remove_file(&tmp);
        write_mode_private(&tmp, record)?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("session record replace: {e}"))?;
        return Ok(());
    }
    write_mode_private(&path, record)
}

#[cfg(unix)]
fn write_mode_private(path: &std::path::Path, record: &SpawnRecord) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let json = serde_json::to_string(record).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "spawn record already exists for pid {}; refusing",
                    record.pid
                )
            } else {
                format!("session record: {e}")
            }
        })?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("session record: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("session record: {e}"))?;
    drop(file);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("session record permissions: {e}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_mode_private(path: &std::path::Path, record: &SpawnRecord) -> Result<(), String> {
    use std::io::Write;
    let json = serde_json::to_string(record).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "spawn record already exists for pid {}; refusing",
                    record.pid
                )
            } else {
                format!("session record: {e}")
            }
        })?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("session record: {e}"))?;
    Ok(())
}

#[cfg(test)]
fn write_stale_record_for_test(record: &SpawnRecord) {
    let _ = ensure_private_session_dir();
    let _ = std::fs::remove_file(session_record_path(record.pid));
    let json = serde_json::to_string(record).unwrap();
    std::fs::write(session_record_path(record.pid), json).unwrap();
}

/// Pure handshake check: the child's own stdout pipe must report the owned
/// bundle constant, the exact spawned PID, and the issued per-run nonce.
/// Unit-tested; [`spawn_verified`] fails closed on any mismatch.
pub fn check_handshake_identity(
    bundle: &str,
    claimed_pid: u64,
    claimed_nonce: &str,
    child_pid: u32,
    expected_nonce: &str,
) -> Result<(), String> {
    if bundle != crate::BUNDLE_ID
        || claimed_pid != u64::from(child_pid)
        || claimed_nonce != expected_nonce
    {
        return Err("probe helper identity mismatch (bundle/pid/nonce)".to_owned());
    }
    Ok(())
}

/// Load and fully authenticate a spawn session: record presence, pid/nonce
/// equality, recorded-exe constrained to the expected helper, live
/// bundle-or-executable revalidation, and CURRENT live nonce proof — a fresh
/// CGWindowList snapshot must show a window owned by exactly this PID whose
/// title carries exactly the recorded nonce (same-executable PID reuse
/// carries a different nonce title, or none, and is refused). Used by
/// `probe-stop` AND every owned read/write path; caller claims and the
/// historical stdout handshake alone never authorize control.
pub fn load_verified_record(pid: u32, nonce: &str) -> Result<SpawnRecord, String> {
    validate_nonce(nonce)?;
    if pid <= 1 {
        return Err(format!("refusing reserved pid {pid}"));
    }
    ensure_private_session_dir()?;
    let path = session_record_path(pid);
    let raw = read_record_file(&path)?;
    let record: SpawnRecord =
        serde_json::from_str(&raw).map_err(|_| "spawn record corrupt; refusing".to_owned())?;
    if record.pid != pid || record.nonce != nonce {
        return Err("spawn record mismatch (pid/nonce); refusing".to_owned());
    }
    // Constrain the recorded exe to the helper we expect: a record pointing
    // anywhere else (even inside the session dir) is refused.
    let expected = expected_helper_canonical()?;
    let recorded_canon = std::path::Path::new(&record.helper_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|_| "recorded helper path unresolvable; refusing".to_owned())?;
    if recorded_canon != expected {
        return Err("spawn record helper differs from expected probe helper; refusing".to_owned());
    }
    // Live revalidation before any signal or write: owned bundle when the
    // helper has a LaunchServices entry, otherwise the expected executable
    // path plus liveness (direct-spawn helpers have no bundle entry).
    crate::ownership::verify_live_process(pid, Some(&record.helper_path))?;
    // CURRENT live nonce proof: the live helper's window title must carry
    // exactly this nonce. Primary proof is the CG window title (no AX, no
    // consent). When the window server withholds names, a minimal owned AX
    // title fallback applies ONLY with AX trust actually observed true, the
    // live exe already matched above, and a fresh CG PID/WID join bounding
    // the queried windows — otherwise this fails closed. A stale record
    // plus same-executable PID reuse fails both legs (different/absent
    // title nonce); titles are never logged.
    match crate::cg::verify_live_nonce(pid, &record.nonce) {
        Ok(()) => Ok(record),
        Err(cg_err) => {
            match crate::cg::verify_ax_title_nonce(pid, &record.nonce, &record.helper_path) {
                Ok(()) => Ok(record),
                // Both legs reported: the CG cause plus the fallback cause
                // (counts/reasons only, never titles), so a refusal names
                // which title channel failed instead of hiding one.
                Err(ax_err) => Err(format!("{cg_err}; ax-title fallback: {ax_err}")),
            }
        }
    }
}

/// Revalidate that the live spawn record for `pid`/`nonce` still equals the
/// `original` snapshot taken at command entry (pid, nonce, helper
/// executable). Call this at the core write boundary —
/// immediately before a private write and again before its effect readback —
/// and compare instead of silently refreshing the baseline, so a restart or
/// PID reuse between entry and write fails closed. This narrows the
/// check-to-use window; it does not claim races impossible.
pub fn verify_record_unchanged(
    original: &SpawnRecord,
    pid: u32,
    nonce: &str,
) -> Result<SpawnRecord, String> {
    let current = load_verified_record(pid, nonce)?;
    if current != *original {
        return Err(
            "spawn record changed since command entry (pid/nonce/exe); refusing write on a stale baseline"
                .to_owned(),
        );
    }
    Ok(current)
}

/// Verified spawn session: the OS `Child` handle (exact identity, no PID
/// reuse window) plus the handshake-proven nonce and helper path.
pub struct SpawnSession {
    pub child: Child,
    pub pid: u32,
    pub nonce: String,
    pub helper_path: String,
}

/// Raw helper spawn: the exact `Child`, its PID, the canonical helper path,
/// and the stdout line channel. No handshake and no record yet — the caller
/// binds identity next. Holder sessions use this seam to attach AX
/// observation BEFORE the window exists (so window creation is observable);
/// [`spawn_verified`] below keeps the strict spawn-then-handshake order.
/// The reader thread forwards EVERY non-empty line (unbounded channel, never
/// blocks) and exits at child death; callers read exactly the lines their
/// protocol needs.
pub struct UnreadySpawn {
    pub child: Child,
    pub pid: u32,
    pub helper_path: String,
    pub lines: std::sync::mpsc::Receiver<String>,
}

pub fn start_helper_process(spec: &ProbeTargetSpec) -> Result<UnreadySpawn, String> {
    let helper = helper_binary_next_to_current_exe()?;
    let helper_canon = helper
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("helper canonicalize: {e}"))?;
    let mut child = Command::new(&helper)
        .args(spec.argv())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn probe helper: {e}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        "probe helper stdout unavailable".to_owned()
    })?;
    let (line_tx, line_rx) = std::sync::mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("probe-handshake".to_owned())
        .spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim().to_owned();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if line_tx.send(trimmed).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| {
            let _ = child.kill();
            let _ = child.wait();
            format!("handshake thread: {e}")
        })?;
    Ok(UnreadySpawn {
        child,
        pid,
        helper_path: helper_canon,
        lines: line_rx,
    })
}

/// Poll the exact child for early exit with its status for the message.
fn early_exit_text(child: &mut Child) -> Option<String> {
    match child.try_wait() {
        Ok(Some(status)) => Some(format!("probe helper exited early: {status}")),
        Ok(None) => None,
        Err(e) => Some(format!("probe wait failed: {e}")),
    }
}

/// Wait for the identity + ready handshake on a raw spawn: an identity line
/// (`bundle`, exact child PID, issued nonce) AND a `ready` marker printed
/// only after the runloop actually services the owned window. A hung/slow
/// stall is therefore scheduled *after* ready, so the child is a live
/// (unresponsive) window for real AX-latency measurement, not a missing one.
/// Identity mismatch, early exit, or either timeout kills the exact child
/// handle and fails; no PID guessing, no orphaned helper.
pub fn await_handshake(spawn: &mut UnreadySpawn, spec: &ProbeTargetSpec) -> Result<(), String> {
    let pid = spawn.pid;
    let fail = |child: &mut Child, msg: String| -> String {
        if let Some(exit) = early_exit_text(child) {
            let _ = child.kill();
            let _ = child.wait();
            return format!("{msg}; {exit}");
        }
        let _ = child.kill();
        let _ = child.wait();
        msg
    };
    let line = spawn
        .lines
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| {
            let msg = "probe helper identity handshake timed out".to_owned();
            let msg = early_exit_text(&mut spawn.child).unwrap_or(msg);
            let _ = spawn.child.kill();
            let _ = spawn.child.wait();
            msg
        })?;
    let identity: serde_json::Value = serde_json::from_str(&line).map_err(|_| {
        fail(
            &mut spawn.child,
            "probe helper identity not JSON".to_owned(),
        )
    })?;
    let bundle = identity
        .get("bundle")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let claimed_pid = identity.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
    let claimed_nonce = identity
        .get("nonce")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if check_handshake_identity(bundle, claimed_pid, claimed_nonce, pid, &spec.nonce).is_err() {
        let _ = spawn.child.kill();
        let _ = spawn.child.wait();
        return Err("probe helper identity mismatch (bundle/pid/nonce)".to_owned());
    }
    // Ready marker: proves the owned window was serviced by the runloop
    // before the parent returns. Early exit or timeout fails closed.
    let ready_line = spawn
        .lines
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| {
            let msg = "probe helper ready marker timed out (window never serviced)".to_owned();
            let msg = early_exit_text(&mut spawn.child).unwrap_or(msg);
            let _ = spawn.child.kill();
            let _ = spawn.child.wait();
            msg
        })?;
    let ready: serde_json::Value = serde_json::from_str(&ready_line).map_err(|_| {
        fail(
            &mut spawn.child,
            "probe helper ready marker not JSON".to_owned(),
        )
    })?;
    if ready.get("ready").and_then(|v| v.as_bool()) != Some(true) {
        let _ = spawn.child.kill();
        let _ = spawn.child.wait();
        return Err("probe helper ready marker missing".to_owned());
    }
    Ok(())
}

/// Write the durable spawn record after a proven handshake. Currency for
/// later cross-command uses comes from the live window-nonce title, not
/// from any start token.
pub fn record_spawn(pid: u32, spec: &ProbeTargetSpec, helper_path: &str) -> Result<(), String> {
    write_spawn_record(&SpawnRecord {
        pid,
        nonce: spec.nonce.clone(),
        helper_path: helper_path.to_owned(),
    })
}

/// Spawn the helper and return only after it proves itself on stdout:
/// an identity line (`bundle`, exact child PID, issued nonce) AND a `ready`
/// marker printed only after the runloop actually services the owned window.
/// A hung/slow stall is therefore scheduled *after* ready, so the child is
/// a live (unresponsive) window for real AX-latency measurement, not a
/// missing one. Identity mismatch, early exit, or either timeout kills the
/// exact child handle and fails; no PID guessing, no orphaned helper.
pub fn spawn_verified(spec: &ProbeTargetSpec) -> Result<SpawnSession, String> {
    let mut raw = start_helper_process(spec)?;
    let pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    await_handshake(&mut raw, spec)?;
    if let Err(e) = record_spawn(pid, spec, &helper_path) {
        let _ = raw.child.kill();
        let _ = raw.child.wait();
        return Err(e);
    }
    Ok(SpawnSession {
        child: raw.child,
        pid,
        nonce: spec.nonce.clone(),
        helper_path,
    })
}

/// Stop a session by PID+nonce: authenticate the exact spawn record
/// (pid/nonce, expected helper, live bundle-or-exe plus CURRENT live window
/// nonce), signal once, poll exit bounded, and remove the record only on
/// verified exit.
pub fn stop_session(pid: u32, nonce: &str) -> Result<(), String> {
    let _record = load_verified_record(pid, nonce)?;
    if !crate::ffi::terminate_process(pid as i32) {
        return Err(format!("SIGTERM delivery failed for pid {pid}"));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while crate::ffi::process_exists(pid as i32) {
        if Instant::now() > deadline {
            return Err(format!(
                "pid {pid} still exists after SIGTERM; record retained for inspection"
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(session_record_path(pid));
    Ok(())
}
