#![forbid(unsafe_code)]
//! R12 durable native-property preimages for the project-owned fullscreen
//! chrome toggle.
//!
//! Before the project strips a window's caption/sizing frame it must persist
//! the original native state in a lifetime-bound adapter-owned slot that
//! outlives the tiler (contract `world-contract.md` R12). This module owns
//! that slot: one JSON document in the ledger directory, published with the
//! shared atomic replace helper, holding at most one row per exact window
//! lifetime. The live per-window properties in [`super::fullscreen`] remain
//! the fast path; this slot is the crash/restart authority.
//!
//! Row identity is the full native lifetime — HWND plus process id plus
//! process creation plus the stored membership tag — never the HWND alone: a
//! recycled handle or a re-created process must never inherit restoration
//! authority. Rows also record the tiler session that wrote them so a fresh
//! session can distinguish a crashed predecessor's leftovers from its own
//! live preimage. Only a different-session row whose exact lifetime still
//! self-verifies is restored, and a verified exit clears only the row it owns.
//!
//! Failures are narrow and honest: an unreadable or unwritable slot reports
//! an error to the caller, which performs no dependent native write. Nothing
//! here retries, guesses a lifetime, or restores from a stale row.

use std::path::Path;

use tiler_core::geometry::Rect;

use super::fullscreen::{FULLSCREEN_STYLE_BITS, FullscreenMeta};

/// Serialized preimage document version. A different version is unreadable
/// (never guessed, never restored).
const PREIMAGE_VERSION: u32 = 1;
/// Adapter-owned durable slot name inside the ledger directory.
const PREIMAGE_FILE: &str = "fullscreen-preimages.json";

/// One durable fullscreen-chrome preimage for one exact native window
/// lifetime: the exact style bits cleared on entry, the pre-maximize state,
/// and the pre-fullscreen outer frame for non-maximized entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FullscreenPreimage {
    pub(super) version: u32,
    pub(super) hwnd: u64,
    pub(super) pid: u32,
    pub(super) creation: String,
    /// Stored membership tag (opaque, non-empty) proving the lifetime.
    pub(super) tag: String,
    /// Tiler session (`process_creation`) that wrote the row.
    pub(super) session: String,
    /// Style bits cleared on entry; always a subset of `FULLSCREEN_STYLE_BITS`.
    pub(super) changed: u32,
    pub(super) was_maximized: bool,
    /// Pre-fullscreen outer frame for non-maximized entries; never carried by
    /// a maximized entry.
    pub(super) frame: Option<Rect>,
}

impl FullscreenPreimage {
    /// Build one row for a live lifetime. Empty identity fields, arbitrary
    /// style bits, or a maximized/non-maximized frame shape mismatch is never
    /// a valid preimage and the caller refuses to persist it.
    ///
    /// The fields stay explicit on purpose: this is the durable row's
    /// validated wire shape, and folding them into a struct would hide the
    /// frame/maximize consistency rule the constructor enforces.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        hwnd: u64,
        pid: u32,
        creation: &str,
        tag: &str,
        session: &str,
        changed: u32,
        was_maximized: bool,
        frame: Option<Rect>,
    ) -> Option<Self> {
        let row = Self {
            version: PREIMAGE_VERSION,
            hwnd,
            pid,
            creation: creation.to_owned(),
            tag: tag.to_owned(),
            session: session.to_owned(),
            changed,
            was_maximized,
            frame,
        };
        row.valid().then_some(row)
    }

    /// Whether this row is a complete, internally consistent preimage.
    fn valid(&self) -> bool {
        let frame_shape_ok = if self.was_maximized {
            self.frame.is_none()
        } else {
            self.frame.is_some_and(|frame| frame.w > 0 && frame.h > 0)
        };
        self.version == PREIMAGE_VERSION
            && !self.creation.is_empty()
            && !self.tag.is_empty()
            && !self.session.is_empty()
            && self.changed & !FULLSCREEN_STYLE_BITS == 0
            && frame_shape_ok
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

    fn meta(&self) -> FullscreenMeta {
        FullscreenMeta {
            changed: self.changed,
            was_maximized: self.was_maximized,
            frame: self.frame,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "v": self.version,
            "hwnd": self.hwnd,
            "pid": self.pid,
            "creation": self.creation,
            "tag": self.tag,
            "session": self.session,
            "changed": self.changed,
            "wasMaximized": self.was_maximized,
            "frame": self.frame.map(|frame| serde_json::json!({
                "x": frame.x,
                "y": frame.y,
                "w": frame.w,
                "h": frame.h,
            })),
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
        let session = row.get("session")?.as_str()?.to_owned();
        let changed = u32::try_from(row.get("changed")?.as_u64()?).ok()?;
        let was_maximized = row.get("wasMaximized")?.as_bool()?;
        let frame = match row.get("frame")? {
            serde_json::Value::Null => None,
            value => {
                let x = i32::try_from(value.get("x")?.as_i64()?).ok()?;
                let y = i32::try_from(value.get("y")?.as_i64()?).ok()?;
                let w = i32::try_from(value.get("w")?.as_i64()?).ok()?;
                let h = i32::try_from(value.get("h")?.as_i64()?).ok()?;
                Some(Rect { x, y, w, h })
            }
        };
        let row = Self {
            version,
            hwnd,
            pid,
            creation,
            tag,
            session,
            changed,
            was_maximized,
            frame,
        };
        row.valid().then_some(row)
    }
}

/// Read the durable slot. `Ok(rows)` with an absent file is an empty store;
/// a malformed/unreadable document is `Err(())` so callers fail closed
/// instead of believing an empty store and overwriting a recoverable row.
fn read_store(dir: &Path) -> Result<Vec<FullscreenPreimage>, ()> {
    let path = dir.join(PREIMAGE_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(()),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let document: serde_json::Value = serde_json::from_str(text).map_err(|_| ())?;
    let version = u32::try_from(document.get("v").and_then(serde_json::Value::as_u64).ok_or(())?)
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
        out.push(FullscreenPreimage::from_json(row).ok_or(())?);
    }
    Ok(out)
}

fn write_store(dir: &Path, rows: &[FullscreenPreimage]) -> Result<(), ()> {
    let document = serde_json::json!({
        "v": PREIMAGE_VERSION,
        "rows": rows.iter().map(FullscreenPreimage::to_json).collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|_| ())?;
    crate::storage::publish_replace(dir, PREIMAGE_FILE, &bytes, "fullscreen-preimage")
        .map_err(|_| ())
}

/// Persist one row before the dependent native write. Replaces the row for
/// the exact same lifetime; never touches any other lifetime. Returns
/// `Err(())` when the row is invalid, the slot is unreadable, or the publish
/// fails, in which case the caller performs no native write.
pub(super) fn persist(dir: &Path, row: &FullscreenPreimage) -> Result<(), ()> {
    if !row.valid() {
        return Err(());
    }
    let mut rows = read_store(dir)?;
    rows.retain(|existing| !existing.same_slot(row));
    rows.push(row.clone());
    write_store(dir, &rows)
}

/// Load the row for one exact lifetime, if any. Unreadable slots return
/// `None` (callers treat that as "no owned preimage", never as authority).
/// Used by the R14 adoption baseline to keep a project-owned chrome preimage
/// separate from an application baseline.
pub(super) fn load(
    dir: &Path,
    hwnd: u64,
    pid: u32,
    creation: &str,
    tag: &str,
) -> Option<FullscreenPreimage> {
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
    rows: &'a [FullscreenPreimage],
    current_session: &str,
) -> Vec<&'a FullscreenPreimage> {
    rows.iter()
        .filter(|row| row.is_foreign_session(current_session))
        .collect()
}

/// Pure current-session filter: rows written by the restoring session, the
/// graceful-stop set. A missing session is never restoration authority, so it
/// selects nothing.
fn current_candidates<'a>(
    rows: &'a [FullscreenPreimage],
    current_session: &str,
) -> Vec<&'a FullscreenPreimage> {
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
fn row_window_gone(row: &FullscreenPreimage) -> bool {
    if !crate::ffi::is_window(row.hwnd) {
        return true;
    }
    crate::ffi::window_pid(row.hwnd) != row.pid
}

/// Self-verify one durable row's exact live lifetime from the row itself (no
/// observation list): the window still exists, its pid is the stored pid, the
/// held process identity still carries the stored creation time, and the live
/// membership tag still equals the stored tag. Any unreadable leg is not a
/// match, never a guess.
fn verify_live_lifetime(row: &FullscreenPreimage) -> bool {
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
fn lifetime_provably_dead(row: &FullscreenPreimage) -> bool {
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

/// Restore one candidate set in place and retire each row only after the
/// production restore sequence reports `restored` and the exact live lifetime
/// still verifies after that native work. Returns `(restored, store_changed)`.
fn restore_candidates(
    rows: &mut Vec<FullscreenPreimage>,
    candidates: Vec<FullscreenPreimage>,
) -> (usize, bool) {
    let mut restored = 0usize;
    let mut retired = false;
    for row in &candidates {
        if !verify_live_lifetime(row) {
            continue;
        }
        if super::fullscreen::restore_owned_chrome(row.hwnd, &row.meta()) != "restored" {
            continue;
        }
        // Re-prove the lifetime after the native work: the window may have
        // been destroyed or recycled while the chrome was being restored.
        if verify_live_lifetime(row) {
            restored += 1;
            rows.retain(|existing| !existing.same_slot(row));
            retired = true;
        }
    }
    (restored, retired)
}

/// Restore verified fullscreen-chrome leftovers at startup and drop their
/// rows. `current_session` is the restoring process's creation: only rows of a
/// different session are considered, so the dead owner's rows are foreign and
/// the current session's own live preimages are never touched. A missing
/// `current_session` is never restoration authority: the store is left
/// untouched. Rows whose exact process lifetime is provably gone, whose window
/// is closed, or whose handle now belongs to another process are pruned
/// instead of being retained forever; a row with an unreadable identity is
/// kept for a later attempt. Each remaining candidate self-verifies its exact
/// live lifetime (existing window, stored pid, stored process creation, stored
/// membership tag) without an observation dependency. The row leaves only
/// after the production restore sequence reports `restored` and that lifetime
/// re-verifies; a failed or unproven native restore keeps the row so the next
/// startup retries. A failed publish only means a later startup re-checks, and
/// an unreadable store restores nothing. Returns how many rows were restored.
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
    let candidates: Vec<FullscreenPreimage> = leftover_candidates(&rows, current_session)
        .into_iter()
        .cloned()
        .collect();
    let (restored, retired) = restore_candidates(&mut rows, candidates);
    dirty |= retired;
    if dirty {
        let _ = write_store(dir, &rows);
    }
    restored
}

/// Restore this session's own fullscreen-chrome preimages on graceful stop and
/// report whether residual work remains. `current_session` is the restoring
/// process's creation: only rows written by exactly that session are
/// considered, so a foreign crashed predecessor's leftovers are never touched
/// here (startup recovery owns them) and a missing session restores nothing
/// and leaves the store untouched. Closed/recycled window rows are pruned (their
/// lifetime can never verify again). Each candidate self-verifies its exact
/// live lifetime, is restored, then re-verifies that same lifetime before its
/// row is retired. Returns `(restored, pending)`, where `pending` is true when
/// an unreadable store, a failed publish, or any current-session row left
/// behind still needs the watcher to stay up.
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
    let candidates: Vec<FullscreenPreimage> = current_candidates(&rows, current_session)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        hwnd: u64,
        session: &str,
        changed: u32,
        was_maximized: bool,
        frame: Option<Rect>,
    ) -> FullscreenPreimage {
        FullscreenPreimage::new(
            hwnd,
            7,
            "creation-a",
            "tag-a",
            session,
            changed,
            was_maximized,
            frame,
        )
        .expect("row")
    }

    fn frame() -> Rect {
        Rect {
            x: -20,
            y: 30,
            w: 800,
            h: 600,
        }
    }

    #[test]
    fn preimage_row_validates_and_round_trips_exactly() {
        let normal = row(10, "session-a", FULLSCREEN_STYLE_BITS, false, Some(frame()));
        let parsed = FullscreenPreimage::from_json(&normal.to_json()).expect("roundtrip");
        assert_eq!(parsed, normal);
        assert_eq!(
            parsed.meta(),
            FullscreenMeta {
                changed: FULLSCREEN_STYLE_BITS,
                was_maximized: false,
                frame: Some(frame()),
            }
        );
        let maximized = row(11, "session-a", 0, true, None);
        assert_eq!(
            FullscreenPreimage::from_json(&maximized.to_json()),
            Some(maximized.clone())
        );

        // Construction refuses empty identity, foreign style bits, and
        // inconsistent frame shapes.
        assert!(
            FullscreenPreimage::new(1, 1, "", "tag", "session", 0, true, None).is_none()
        );
        assert!(
            FullscreenPreimage::new(1, 1, "creation", "", "session", 0, true, None).is_none()
        );
        assert!(
            FullscreenPreimage::new(1, 1, "creation", "tag", "", 0, true, None).is_none()
        );
        assert!(
            FullscreenPreimage::new(1, 1, "creation", "tag", "session", 0x1, true, None)
                .is_none()
        );
        assert!(
            FullscreenPreimage::new(1, 1, "creation", "tag", "session", 0, true, Some(frame()))
                .is_none()
        );
        assert!(
            FullscreenPreimage::new(1, 1, "creation", "tag", "session", 0, false, None).is_none()
        );
        assert!(
            FullscreenPreimage::new(
                1,
                1,
                "creation",
                "tag",
                "session",
                0,
                false,
                Some(Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 100,
                }),
            )
            .is_none()
        );

        // Decode refuses the same corrupt shapes rather than defaulting.
        let mut wrong_version = normal.to_json();
        wrong_version["v"] = serde_json::json!(2);
        assert!(FullscreenPreimage::from_json(&wrong_version).is_none());
        let mut foreign_bits = normal.to_json();
        foreign_bits["changed"] = serde_json::json!(0x8000_0000_u32);
        assert!(FullscreenPreimage::from_json(&foreign_bits).is_none());
        let mut missing_frame = normal.to_json();
        missing_frame["frame"] = serde_json::Value::Null;
        assert!(FullscreenPreimage::from_json(&missing_frame).is_none());
        let mut maximized_frame = maximized.to_json();
        maximized_frame["frame"] = normal.to_json()["frame"].clone();
        assert!(FullscreenPreimage::from_json(&maximized_frame).is_none());
    }

    #[test]
    fn leftover_candidates_skip_current_session_and_missing_session() {
        // Both rows carry a complete exact-lifetime shape (hwnd/pid/creation/
        // tag); only the session differs. Native lifetime verification runs
        // after this filter, so this is exactly the restoration-scope
        // decision: a same-session row is never restorable, a different-
        // session row is the candidate.
        let rows = vec![
            row(10, "session-a", 0, true, None),
            row(11, "session-b", 0, true, None),
        ];
        let foreign = leftover_candidates(&rows, "session-b");
        assert_eq!(foreign.len(), 1);
        assert_eq!(foreign[0].hwnd, 10);
        let own = leftover_candidates(&rows, "session-a");
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].hwnd, 11);
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
            "omnitester-fullscreen-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    fn self_creation() -> String {
        crate::native::HeldProcess::open(std::process::id())
            .expect("self process")
            .identity()
            .expect("self identity")
            .process_creation
    }

    #[test]
    fn empty_session_never_touches_or_prunes_the_store() {
        let dir = temp_store("empty-session");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dead = FullscreenPreimage::new(
            0,
            u32::MAX,
            "creation-a",
            "tag-a",
            "session-a",
            0,
            true,
            None,
        )
        .expect("row");
        persist(&dir, &dead).expect("persist");
        // A missing session is never restoration authority: neither restore
        // path may prune or retire anything, even a provably dead row.
        assert_eq!(restore_verified_leftovers(&dir, ""), 0);
        assert_eq!(restore_verified_current(&dir, ""), (0, false));
        assert_eq!(
            load(&dir, 0, u32::MAX, "creation-a", "tag-a"),
            Some(dead)
        );
        assert_eq!(read_store(&dir).expect("store").len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_restore_prunes_only_its_own_closed_rows() {
        let dir = temp_store("current-scope");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let pid = std::process::id();
        let creation = self_creation();
        let own = FullscreenPreimage::new(
            0,
            pid,
            &creation,
            "tag-a",
            "session-self",
            0,
            true,
            None,
        )
        .expect("row");
        let foreign = FullscreenPreimage::new(
            0,
            pid,
            &creation,
            "tag-b",
            "session-other",
            0,
            true,
            None,
        )
        .expect("row");
        persist(&dir, &own).expect("persist own");
        persist(&dir, &foreign).expect("persist foreign");
        // Handle 0 is closed, so the current-session row can never restore
        // and is pruned without leaving residue; the other session's row is
        // not this pass's business.
        assert_eq!(restore_verified_current(&dir, "session-self"), (0, false));
        assert!(load(&dir, 0, pid, &creation, "tag-a").is_none());
        assert_eq!(
            load(&dir, 0, pid, &creation, "tag-b"),
            Some(foreign)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unverifiable_current_row_keeps_watcher_pending() {
        let dir = temp_store("pending");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let window = crate::ffi::TestWindow::create_message_only(
            "OmniTiler.FullscreenPreimagePendingTest",
            "fullscreen-preimage",
        )
        .expect("window");
        let pid = std::process::id();
        let creation = self_creation();
        // A live window with the exact pid but no matching membership tag:
        // the row cannot verify, so it must stay and keep the watcher up.
        let row = FullscreenPreimage::new(
            window.hwnd(),
            pid,
            &creation,
            "0123456789abcdef",
            "session-self",
            0,
            true,
            None,
        )
        .expect("row");
        persist(&dir, &row).expect("persist");
        assert_eq!(restore_verified_current(&dir, "session-self"), (0, true));
        assert_eq!(
            load(&dir, window.hwnd(), pid, &creation, "0123456789abcdef"),
            Some(row)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_persist_load_and_clear_are_lifetime_exact() {
        let dir = std::env::temp_dir().join(format!(
            "omnitester-fullscreen-preimage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let prior = row(10, "session-a", FULLSCREEN_STYLE_BITS, false, Some(frame()));
        persist(&dir, &prior).expect("persist");
        assert_eq!(
            load(&dir, 10, 7, "creation-a", "tag-a"),
            Some(prior.clone())
        );
        // A recycled HWND (same handle, other creation) never loads it.
        assert!(load(&dir, 10, 7, "creation-b", "tag-a").is_none());
        // Clearing requires the exact lifetime; a near miss leaves the row.
        clear(&dir, 10, 7, "creation-b", "tag-a").expect("near-miss clear");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_some());
        clear(&dir, 10, 7, "creation-a", "tag-a").expect("clear");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_store_restores_nothing_and_is_left_untouched() {
        let dir = std::env::temp_dir().join(format!(
            "omnitester-fullscreen-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(PREIMAGE_FILE);
        std::fs::write(&path, br#"{"v":1,"rows":[{"v":1}]}"#).expect("write corrupt store");
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(read_store(&dir).is_err());
        assert_eq!(
            std::fs::read(&path).expect("store still present"),
            br#"{"v":1,"rows":[{"v":1}]}"#
        );
        // A different document version is unreadable, even with no rows.
        std::fs::write(&path, br#"{"v":2,"rows":[]}"#).expect("write future store");
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(read_store(&dir).is_err());
        // An unreadable store leaves residue unproven: pending is set so a
        // graceful stop keeps the watcher up.
        assert_eq!(restore_verified_current(&dir, "session-b"), (0, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn provably_dead_lifetime_row_is_pruned_not_retained() {
        let dir = std::env::temp_dir().join(format!(
            "omnitester-fullscreen-dead-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dead = FullscreenPreimage::new(
            0,
            u32::MAX,
            "creation-a",
            "tag-a",
            "session-a",
            FULLSCREEN_STYLE_BITS,
            false,
            Some(frame()),
        )
        .expect("row");
        persist(&dir, &dead).expect("persist");
        // The owning process creation is provably absent: the row can never
        // be restored and is pruned instead of surviving every startup.
        assert_eq!(restore_verified_leftovers(&dir, "session-b"), 0);
        assert!(load(&dir, 0, u32::MAX, "creation-a", "tag-a").is_none());
        assert!(read_store(&dir).expect("store").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
