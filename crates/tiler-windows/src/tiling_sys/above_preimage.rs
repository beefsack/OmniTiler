#![forbid(unsafe_code)]
//! R12 durable native-property preimages for the persistent-above lane.
//!
//! Before the project raises one window's `WS_EX_TOPMOST` band it must
//! persist the original native state in a lifetime-bound adapter-owned slot
//! that outlives the tiler (contract `world-contract.md` R12). This module
//! owns that slot: one JSON document in the ledger directory, published with
//! the shared atomic replace helper, holding at most one row per exact window
//! lifetime.
//!
//! Row identity is the full native lifetime — HWND plus process id plus
//! process creation plus the stored membership tag — never the HWND alone: a
//! recycled handle or a re-created process must never inherit restoration
//! authority. Rows also record the tiler session that wrote them so a fresh
//! session can distinguish a crashed predecessor's leftovers from its own
//! live preimage. Only a different-session row whose exact lifetime is still
//! observed is restored, and a release restores only the row it owns.
//!
//! Failures are narrow and honest: an unreadable or unwritable slot reports
//! an error to the caller, which holds only the affected window/property and
//! performs no dependent write. Nothing here retries, guesses a lifetime, or
//! restores from a stale row.

use std::path::Path;

/// Serialized preimage document version. A different version is unreadable
/// (never guessed, never restored).
const PREIMAGE_VERSION: u32 = 1;
/// Adapter-owned durable slot name inside the ledger directory.
const PREIMAGE_FILE: &str = "above-preimages.json";

/// One durable topmost preimage for one exact native window lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AbovePreimage {
    pub(super) version: u32,
    pub(super) hwnd: u64,
    pub(super) pid: u32,
    pub(super) creation: String,
    /// Stored membership tag (opaque, non-empty) proving the lifetime.
    pub(super) tag: String,
    /// Original `WS_EX_TOPMOST` bit before the project raised the window.
    pub(super) prior_topmost: bool,
    /// Tiler session (`process_creation`) that wrote the row.
    pub(super) session: String,
}

impl AbovePreimage {
    /// Build one row for a live lifetime. Empty creation/tag is never a valid
    /// lifetime proof and the caller refuses to persist it.
    pub(super) fn new(
        hwnd: u64,
        pid: u32,
        creation: &str,
        tag: &str,
        prior_topmost: bool,
        session: &str,
    ) -> Option<Self> {
        if creation.is_empty() || tag.is_empty() || session.is_empty() {
            return None;
        }
        Some(Self {
            version: PREIMAGE_VERSION,
            hwnd,
            pid,
            creation: creation.to_owned(),
            tag: tag.to_owned(),
            prior_topmost,
            session: session.to_owned(),
        })
    }

    /// Exact lifetime equality (HWND plus pid plus process creation plus the
    /// stored tag). Never a HWND-only or pid-only match.
    pub(super) fn same_lifetime(&self, hwnd: u64, pid: u32, creation: &str, tag: &str) -> bool {
        self.hwnd == hwnd
            && self.pid == pid
            && self.creation == creation
            && !self.creation.is_empty()
            && self.tag == tag
            && !self.tag.is_empty()
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.hwnd == other.hwnd
            && self.pid == other.pid
            && self.creation == other.creation
            && self.tag == other.tag
    }

    /// Whether this row is a leftover of a different tiler session
    /// (restorable) rather than the restoring (current) session's own
    /// preimage. `current_session` is the restoring process's creation.
    pub(super) fn is_foreign_session(&self, current_session: &str) -> bool {
        !current_session.is_empty() && self.session != current_session
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "v": self.version,
            "hwnd": self.hwnd,
            "pid": self.pid,
            "creation": self.creation,
            "tag": self.tag,
            "priorTopmost": self.prior_topmost,
            "session": self.session,
        })
    }

    fn from_json(row: &serde_json::Value) -> Option<Self> {
        let version = u32::try_from(row.get("v")?.as_u64()?).ok()?;
        if version != PREIMAGE_VERSION {
            return None;
        }
        let hwnd = row.get("hwnd")?.as_u64()?;
        let pid = u32::try_from(row.get("pid")?.as_u64()?).ok()?;
        let creation = row.get("creation")?.as_str()?.to_owned();
        let tag = row.get("tag")?.as_str()?.to_owned();
        let prior_topmost = row.get("priorTopmost")?.as_bool()?;
        let session = row.get("session")?.as_str()?.to_owned();
        if creation.is_empty() || tag.is_empty() || session.is_empty() {
            return None;
        }
        Some(Self {
            version,
            hwnd,
            pid,
            creation,
            tag,
            prior_topmost,
            session,
        })
    }
}

/// Read the durable slot. `Ok(rows)` with an absent file is an empty store;
/// a malformed/unreadable document is `Err(())` so callers fail closed
/// instead of believing an empty store and overwriting a recoverable row.
fn read_store(dir: &Path) -> Result<Vec<AbovePreimage>, ()> {
    let path = dir.join(PREIMAGE_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(()),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let document: serde_json::Value = serde_json::from_str(text).map_err(|_| ())?;
    // The document version is part of the wire contract: a future/foreign
    // document is unreadable, never guessed, so recovery cannot "restore"
    // rows it does not understand.
    let version = u32::try_from(
        document
            .get("v")
            .and_then(serde_json::Value::as_u64)
            .ok_or(())?,
    )
    .map_err(|_| ())?;
    if version != PREIMAGE_VERSION {
        return Err(());
    }
    let rows = document
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .ok_or(())?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // One corrupt row invalidates the whole slot: partial trust would
        // let a later persist silently drop a recoverable preimage.
        out.push(AbovePreimage::from_json(row).ok_or(())?);
    }
    Ok(out)
}

fn write_store(dir: &Path, rows: &[AbovePreimage]) -> Result<(), ()> {
    let document = serde_json::json!({
        "v": PREIMAGE_VERSION,
        "rows": rows.iter().map(AbovePreimage::to_json).collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|_| ())?;
    crate::storage::publish_replace(dir, PREIMAGE_FILE, &bytes, "above-preimage").map_err(|_| ())
}

/// Persist one row before the dependent native write. Replaces the row for
/// the exact same lifetime; never touches any other lifetime. Returns
/// `Err(())` when the slot is unreadable or the publish fails, in which case
/// the caller performs no native write.
pub(super) fn persist(dir: &Path, row: &AbovePreimage) -> Result<(), ()> {
    if row.version != PREIMAGE_VERSION {
        return Err(());
    }
    let mut rows = read_store(dir)?;
    rows.retain(|existing| !existing.same_slot(row));
    rows.push(row.clone());
    write_store(dir, &rows)
}

/// Load the row for one exact lifetime, if any. Unreadable slots return
/// `None` (callers treat that as "no owned preimage", never as authority).
pub(super) fn load(
    dir: &Path,
    hwnd: u64,
    pid: u32,
    creation: &str,
    tag: &str,
) -> Option<AbovePreimage> {
    read_store(dir)
        .ok()?
        .into_iter()
        .find(|row| row.same_lifetime(hwnd, pid, creation, tag))
}

/// Clear the row for one exact lifetime after a proven restore. A removal
/// that cannot be published reports `Err(())`; the native state is already
/// restored, so the caller only logs the degraded slot (never re-writes the
/// window).
pub(super) fn clear(dir: &Path, hwnd: u64, pid: u32, creation: &str, tag: &str) -> Result<(), ()> {
    let mut rows = read_store(dir)?;
    let before = rows.len();
    rows.retain(|row| !row.same_lifetime(hwnd, pid, creation, tag));
    if rows.len() == before {
        return Ok(());
    }
    write_store(dir, &rows)
}

/// Pure leftover filter: rows written by a different tiler session. A live
/// current-session preimage is never a leftover, and a missing session is
/// never restoration authority. Nothing here inspects the window.
fn leftover_candidates<'a>(
    rows: &'a [AbovePreimage],
    current_session: &str,
) -> Vec<&'a AbovePreimage> {
    rows.iter()
        .filter(|row| row.is_foreign_session(current_session))
        .collect()
}

/// Pure current-session filter: rows written by the restoring session, the
/// graceful-stop set. A missing session is never restoration authority, so it
/// selects nothing.
fn current_candidates<'a>(
    rows: &'a [AbovePreimage],
    current_session: &str,
) -> Vec<&'a AbovePreimage> {
    if current_session.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .filter(|row| row.session == current_session)
        .collect()
}

/// D5 pruning decision for one durable row: a closed window, a handle now
/// owned by another process (recycled), or a handle that vanished between the
/// two reads can never verify its stored lifetime again, so the row is dead
/// metadata. `GetWindowThreadProcessId` needs no handle access rights: for an
/// existing HWND it always yields the owning pid, so a mismatch (including the
/// zero read of a window destroyed mid-query) is a gone lifetime, never an
/// access-denied guess. Access denied/unknown stays fail-closed in the
/// held-process identity path ([`lifetime_provably_dead`]) for a matching
/// live HWND/PID.
fn row_window_gone(row: &AbovePreimage) -> bool {
    if !crate::ffi::is_window(row.hwnd) {
        return true;
    }
    crate::ffi::window_pid(row.hwnd) != row.pid
}

/// Whether one observed band equals the stored preimage. `None` (unreadable
/// extended style) never matches: an unreadable band is not restoration
/// proof.
fn band_matches(prior_topmost: bool, observed: Option<bool>) -> bool {
    observed == Some(prior_topmost)
}

/// Restore one verified row's original topmost band and prove the result. An
/// already-correct band needs no write; an unreadable band holds immediately
/// (never a blind write), and otherwise the setter must accept and a fresh
/// read must show the preimage band. An unreadable post-write band keeps the
/// durable row: retirement is only ever based on proof.
fn restore_topmost_band(row: &AbovePreimage) -> bool {
    match super::workspace_mode::read_topmost_now(row.hwnd) {
        Some(current) if band_matches(row.prior_topmost, Some(current)) => true,
        Some(_) => {
            super::workspace_mode::set_topmost_band(row.hwnd, row.prior_topmost)
                && band_matches(
                    row.prior_topmost,
                    super::workspace_mode::read_topmost_now(row.hwnd),
                )
        }
        None => false,
    }
}

/// Restore one candidate set in place and retire each row only after its
/// native restore is proven and the exact live lifetime still verifies after
/// the native work. Returns `(restored, store_changed)`.
fn restore_candidates(
    rows: &mut Vec<AbovePreimage>,
    candidates: Vec<AbovePreimage>,
) -> (usize, bool) {
    let mut restored = 0usize;
    let mut retired = false;
    for row in &candidates {
        if !verify_live_lifetime(row) || !restore_topmost_band(row) {
            continue;
        }
        // Re-prove the lifetime after the native work: the window may have
        // been destroyed or recycled while the band was being restored, and
        // the row must never retire authority for a lifetime that drifted.
        if verify_live_lifetime(row) {
            restored += 1;
            rows.retain(|existing| !existing.same_slot(row));
            retired = true;
        }
    }
    (restored, retired)
}

/// Restore verified leftovers at startup and drop their rows. `current_session`
/// is the restoring process's creation: only rows of a different session are
/// considered, so the dead owner's rows are foreign and the current session's
/// own live preimages are never touched. A missing `current_session` is never
/// restoration authority: the store is left untouched. Rows whose exact
/// process lifetime is provably gone, whose window is closed, or whose handle
/// now belongs to another process are pruned instead of being retained
/// forever; a row with an unreadable identity is kept for a later attempt.
/// Each remaining candidate self-verifies its exact live lifetime (existing
/// window, stored pid, stored process creation, stored membership tag)
/// without an observation dependency. A row whose current band already equals
/// the preimage needs no write but is still cleared once its lifetime is
/// verified. A failed or unproven native restore keeps the row so the next
/// startup retries the same idempotent restore; a failed publish only means a
/// later startup re-checks. An unreadable store restores nothing. Returns how
/// many rows were restored.
pub(super) fn restore_verified_leftovers(dir: &Path, current_session: &str) -> usize {
    if current_session.is_empty() {
        return 0;
    }
    let Ok(mut rows) = read_store(dir) else {
        return 0;
    };
    let before = rows.len();
    rows.retain(|row| {
        row.session == current_session || (!lifetime_provably_dead(row) && !row_window_gone(row))
    });
    let mut dirty = rows.len() != before;
    let candidates: Vec<AbovePreimage> = leftover_candidates(&rows, current_session)
        .into_iter()
        .cloned()
        .collect();
    let (restored, retired) = restore_candidates(&mut rows, candidates);
    dirty |= retired;
    if dirty {
        // Best-effort slot compaction; the native restore above already
        // happened, so a failed publish only means a later startup retries
        // an idempotent no-op restore.
        let _ = write_store(dir, &rows);
    }
    restored
}

/// Restore this session's own preimages on graceful stop and report whether
/// residual work remains. `current_session` is the restoring process's
/// creation: only rows written by exactly that session are considered, so a
/// foreign crashed predecessor's leftovers are never touched here (startup
/// recovery owns them) and a missing session restores nothing and leaves the
/// store untouched. Closed/recycled window rows are pruned (their lifetime can
/// never verify again). Each candidate self-verifies its exact live lifetime,
/// is restored, then re-verifies that same lifetime before its row is retired;
/// an unreadable band holds the row. Returns `(restored, pending)`, where
/// `pending` is true when an unreadable store, a failed publish, or any
/// current-session row left behind still needs the watcher to stay up.
pub(super) fn restore_verified_current(dir: &Path, current_session: &str) -> (usize, bool) {
    if current_session.is_empty() {
        return (0, false);
    }
    let Ok(mut rows) = read_store(dir) else {
        // Unreadable store: residue is unproven, so the watcher stays up.
        return (0, true);
    };
    let before = rows.len();
    rows.retain(|row| {
        row.session != current_session || (!lifetime_provably_dead(row) && !row_window_gone(row))
    });
    let mut dirty = rows.len() != before;
    let candidates: Vec<AbovePreimage> = current_candidates(&rows, current_session)
        .into_iter()
        .cloned()
        .collect();
    let (restored, retired) = restore_candidates(&mut rows, candidates);
    dirty |= retired;
    let mut pending = false;
    if dirty {
        pending = write_store(dir, &rows).is_err();
    }
    pending |= rows.iter().any(|row| row.session == current_session);
    (restored, pending)
}

/// Self-verify one durable row's exact live lifetime from the row itself (no
/// observation list): the window still exists, its pid is the stored pid, the
/// held process identity still carries the stored creation time, and the live
/// membership tag still equals the stored tag. Any unreadable leg is not a
/// match, never a guess.
fn verify_live_lifetime(row: &AbovePreimage) -> bool {
    if row.creation.is_empty() || row.tag.is_empty() || !crate::ffi::is_window(row.hwnd) {
        return false;
    }
    if crate::ffi::window_pid(row.hwnd) != row.pid {
        return false;
    }
    let Ok(held) = crate::native::HeldProcess::open(row.pid) else {
        return false;
    };
    let Ok(live) = held.identity() else {
        return false;
    };
    if live.pid != row.pid || live.process_creation != row.creation {
        return false;
    }
    crate::product_hide::sys::read_member_tag(row.hwnd).as_deref() == Some(row.tag.as_str())
}

/// A row's lifetime can never be restored again once the process that owned
/// it is provably gone (or its pid now belongs to a different creation).
/// Such rows are permanent orphaned metadata: startup recovery can never
/// verify them, so every later startup would re-read and retain them forever.
/// Unreadable identity (access denied, unknown error) is never treated as
/// dead, and the current session's own rows are excluded by the caller.
fn lifetime_provably_dead(row: &AbovePreimage) -> bool {
    match crate::native::HeldProcess::open(row.pid) {
        Err(crate::native::IdentityError::Absent) => true,
        Err(_) => false,
        Ok(held) => match held.identity() {
            Err(crate::native::IdentityError::Absent) => true,
            Err(_) => false,
            Ok(live) => live.process_creation != row.creation,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(hwnd: u64, session: &str, prior: bool) -> AbovePreimage {
        AbovePreimage::new(hwnd, 7, "creation-a", "tag-a", prior, session).expect("row")
    }

    #[test]
    fn exact_lifetime_never_matches_recycled_or_corrupt_identity() {
        let row = row(10, "session-a", false);
        assert!(row.same_lifetime(10, 7, "creation-a", "tag-a"));
        assert!(!row.same_lifetime(11, 7, "creation-a", "tag-a"));
        assert!(!row.same_lifetime(10, 8, "creation-a", "tag-a"));
        assert!(!row.same_lifetime(10, 7, "creation-b", "tag-a"));
        assert!(!row.same_lifetime(10, 7, "creation-a", "tag-b"));
        assert!(!row.same_lifetime(10, 7, "", "tag-a"));
    }

    #[test]
    fn construction_refuses_empty_lifetime_proof() {
        assert!(AbovePreimage::new(1, 1, "", "tag", false, "session").is_none());
        assert!(AbovePreimage::new(1, 1, "creation", "", false, "session").is_none());
        assert!(AbovePreimage::new(1, 1, "creation", "tag", false, "").is_none());
    }

    #[test]
    fn unreadable_band_is_never_restoration_proof() {
        // `None` is an unreadable extended style: it can never equal either
        // stored band, so no row is retired from it.
        assert!(!band_matches(true, None));
        assert!(!band_matches(false, None));
        assert!(band_matches(true, Some(true)));
        assert!(band_matches(false, Some(false)));
        assert!(!band_matches(true, Some(false)));
        assert!(!band_matches(false, Some(true)));
    }

    #[test]
    fn row_pruning_is_exact_for_closed_and_recycled_handles() {
        let (window, _) = tagged_test_window("OmniTiler.AbovePreimagePruneTest");
        let pid = std::process::id();
        let creation = self_creation();
        // A live handle whose pid still matches: not gone.
        let live =
            AbovePreimage::new(window.hwnd(), pid, &creation, "tag-a", false, "session-a").unwrap();
        assert!(!row_window_gone(&live));
        // The same live handle under another stored pid is recycled.
        let other_pid = if pid == u32::MAX { pid - 1 } else { pid + 1 };
        let recycled = AbovePreimage::new(
            window.hwnd(),
            other_pid,
            &creation,
            "tag-a",
            false,
            "session-a",
        )
        .unwrap();
        assert!(row_window_gone(&recycled));
        // A closed handle is gone even while its process is alive.
        let closed = AbovePreimage::new(0, pid, &creation, "tag-a", false, "session-a").unwrap();
        assert!(row_window_gone(&closed));
    }

    #[test]
    fn unreadable_extended_style_never_reads_as_not_topmost() {
        // A closed handle's exstyle is unreadable (`None`), so the topmost
        // helper must not fabricate `false` from it.
        assert_eq!(super::super::workspace_mode::read_topmost_now(0), None);
    }

    #[test]
    fn unreadable_band_holds_instead_of_writing_blind() {
        let creation = self_creation();
        let row = AbovePreimage::new(
            0,
            std::process::id(),
            &creation,
            "tag-a",
            false,
            "session-a",
        )
        .expect("row");
        // The pre-read is unreadable, so the restore holds immediately and
        // performs no native write from a guessed band.
        assert!(!restore_topmost_band(&row));
    }

    #[test]
    fn leftover_candidates_skip_current_session_and_missing_session() {
        // Both rows carry a complete exact-lifetime shape (hwnd/pid/creation/
        // tag); only the session differs. Native lifetime verification runs
        // after this filter, so this is exactly the restoration-scope
        // decision: a same-session row is never restorable, a different-
        // session row is the candidate.
        let rows = vec![row(10, "session-a", false), row(11, "session-b", true)];
        let foreign = leftover_candidates(&rows, "session-b");
        assert_eq!(foreign.len(), 1);
        assert_eq!(foreign[0].hwnd, 10);
        assert!(!foreign[0].prior_topmost);
        // Same-session rows are never leftovers.
        let own = leftover_candidates(&rows, "session-a");
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].hwnd, 11);
        // A missing session is never restoration authority.
        assert!(leftover_candidates(&rows, "").is_empty());

        // The graceful-stop scope is the exact mirror image: each row is
        // selected by exactly one of the two scopes.
        let current = current_candidates(&rows, "session-b");
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].hwnd, 11);
        let current = current_candidates(&rows, "session-a");
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].hwnd, 10);
        assert!(current_candidates(&rows, "").is_empty());
    }

    fn temp_store(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omnitester-above-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn malformed_store_restores_nothing_and_is_left_untouched() {
        let dir = temp_store("malformed");
        std::fs::create_dir_all(&dir).expect("temp dir");
        // A single corrupt row invalidates the slot: the scanner must not
        // guess, overwrite, or restore anything from it.
        let path = dir.join(PREIMAGE_FILE);
        std::fs::write(
            &path,
            br#"{"v":1,"rows":[{"v":1,"hwnd":10,"pid":7,"creation":"","tag":"tag-a","priorTopmost":false,"session":"session-a"}]}"#,
        )
        .expect("write corrupt store");
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(read_store(&dir).is_err());
        assert_eq!(
            std::fs::read(&path).expect("store still present"),
            br#"{"v":1,"rows":[{"v":1,"hwnd":10,"pid":7,"creation":"","tag":"tag-a","priorTopmost":false,"session":"session-a"}]}"#
        );
        // The top-level document version is validated too: a future/foreign
        // document or a missing version is unreadable even when it has no
        // rows, so recovery fails closed instead of treating it as empty.
        std::fs::write(&path, br#"{"v":2,"rows":[]}"#).expect("write future store");
        assert!(read_store(&dir).is_err());
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        std::fs::write(&path, br#"{"rows":[]}"#).expect("write unversioned store");
        assert!(read_store(&dir).is_err());
        // An unreadable store leaves residue unproven: pending is set so a
        // graceful stop keeps the watcher up.
        assert_eq!(restore_verified_current(&dir, "session-b"), (0, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn self_creation() -> String {
        crate::native::HeldProcess::open(std::process::id())
            .expect("self process")
            .identity()
            .expect("self identity")
            .process_creation
    }

    fn tagged_test_window(class: &str) -> (crate::ffi::TestWindow, String) {
        let window =
            crate::ffi::TestWindow::create_message_only(class, "above-preimage").expect("window");
        let tag = crate::product_hide::sys::ensure_member_tag(window.hwnd(), std::process::id())
            .expect("member tag");
        (window, tag)
    }

    #[test]
    fn empty_session_never_touches_or_prunes_the_store() {
        let dir = temp_store("empty-session");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dead = AbovePreimage::new(0, u32::MAX, "creation-a", "tag-a", false, "session-a")
            .expect("row");
        persist(&dir, &dead).expect("persist");
        // A missing session is never restoration authority: neither restore
        // path may prune or retire anything, even a provably dead row.
        assert_eq!(restore_verified_leftovers(&dir, ""), 0);
        assert_eq!(restore_verified_current(&dir, ""), (0, false));
        assert_eq!(load(&dir, 0, u32::MAX, "creation-a", "tag-a"), Some(dead));
        assert_eq!(read_store(&dir).expect("store").len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn closed_window_of_live_process_is_pruned() {
        let dir = temp_store("window-gone");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let pid = std::process::id();
        let creation = self_creation();
        // Handle 0 is a closed window owned by a still-live process: the
        // process is not provably dead, so only the window check can prune
        // this row, and it can never verify its lifetime again.
        let closed =
            AbovePreimage::new(0, pid, &creation, "tag-a", false, "session-a").expect("row");
        persist(&dir, &closed).expect("persist");
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(load(&dir, 0, pid, &creation, "tag-a").is_none());
        assert!(read_store(&dir).expect("store").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn provably_dead_lifetime_row_is_pruned_not_retained() {
        let dir = temp_store("dead");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dead = AbovePreimage::new(0, u32::MAX, "creation-a", "tag-a", false, "session-a")
            .expect("row");
        persist(&dir, &dead).expect("persist");
        // The owning process creation is provably absent, so the row can
        // never be restored again: it is pruned instead of surviving every
        // future startup as orphaned metadata.
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(
            load(&dir, 0, u32::MAX, "creation-a", "tag-a").is_none(),
            "provably dead row must be pruned"
        );
        assert!(read_store(&dir).expect("store").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unverifiable_live_lifetime_row_is_kept() {
        let dir = temp_store("kept");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let (window, live_tag) = tagged_test_window("OmniTiler.AbovePreimageKeptTest");
        let pid = std::process::id();
        let creation = self_creation();
        // The window and process are live, but the stored tag is not the live
        // membership tag: this row never verifies, yet its lifetime is not
        // gone, so recovery must keep it for a later attempt and never guess
        // a match.
        let row = AbovePreimage::new(
            window.hwnd(),
            pid,
            &creation,
            "0123456789abcdef",
            false,
            "session-a",
        )
        .expect("row");
        assert_ne!(live_tag, "0123456789abcdef");
        persist(&dir, &row).expect("persist");
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert_eq!(
            load(&dir, window.hwnd(), pid, &creation, "0123456789abcdef"),
            Some(row.clone())
        );
        // The graceful-stop scope keeps it too: no proof, no retirement, and
        // the current row residue keeps the watcher up.
        assert_eq!(restore_verified_current(&dir, "session-a"), (0, true));
        assert_eq!(
            load(&dir, window.hwnd(), pid, &creation, "0123456789abcdef"),
            Some(row)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_restore_retires_own_verified_row_and_leftovers_skip_it() {
        let dir = temp_store("current");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let (window, tag) = tagged_test_window("OmniTiler.AbovePreimageCurrentTest");
        let pid = std::process::id();
        let creation = self_creation();
        let stored = AbovePreimage::new(window.hwnd(), pid, &creation, &tag, false, "session-self")
            .expect("row");
        persist(&dir, &stored).expect("persist");
        // Foreign-only recovery never touches a current-session row.
        assert_eq!(restore_verified_leftovers(&dir, "session-self"), 0);
        assert_eq!(
            load(&dir, window.hwnd(), pid, &creation, &tag),
            Some(stored.clone())
        );
        // Current-only recovery verifies the exact lifetime, proves the band
        // (the test window is already non-topmost), then retires the row.
        assert_eq!(restore_verified_current(&dir, "session-self"), (1, false));
        assert!(load(&dir, window.hwnd(), pid, &creation, &tag).is_none());
        assert!(read_store(&dir).expect("store").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_roundtrip_and_corruption_refusal() {
        let row = row(10, "session-a", true);
        let parsed = AbovePreimage::from_json(&row.to_json()).expect("roundtrip");
        assert_eq!(parsed, row);
        let mut wrong = row.to_json();
        wrong["v"] = serde_json::json!(2);
        assert!(AbovePreimage::from_json(&wrong).is_none());
        let mut empty_tag = row.to_json();
        empty_tag["tag"] = serde_json::json!("");
        assert!(AbovePreimage::from_json(&empty_tag).is_none());
    }

    #[test]
    fn store_persist_load_and_clear_are_lifetime_exact() {
        let dir = std::env::temp_dir().join(format!(
            "omnitester-above-preimage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let prior = row(10, "session-a", false);
        persist(&dir, &prior).expect("persist");
        assert_eq!(
            load(&dir, 10, 7, "creation-a", "tag-a"),
            Some(prior.clone())
        );
        // A recycled HWND (same handle, other creation) never loads it.
        assert!(load(&dir, 10, 7, "creation-b", "tag-a").is_none());
        // Persisting a second lifetime keeps both rows.
        let second =
            AbovePreimage::new(11, 7, "creation-a", "tag-a", true, "session-a").expect("second");
        persist(&dir, &second).expect("persist second");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_some());
        assert!(load(&dir, 11, 7, "creation-a", "tag-a").is_some());
        clear(&dir, 10, 7, "creation-a", "tag-a").expect("clear");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_none());
        assert!(load(&dir, 11, 7, "creation-a", "tag-a").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
