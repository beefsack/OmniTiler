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
    /// (restorable) rather than this live session's own preimage.
    pub(super) fn is_foreign_session(&self, session: &str) -> bool {
        !session.is_empty() && self.session != session
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
pub(super) fn clear(
    dir: &Path,
    hwnd: u64,
    pid: u32,
    creation: &str,
    tag: &str,
) -> Result<(), ()> {
    let mut rows = read_store(dir)?;
    let before = rows.len();
    rows.retain(|row| !row.same_lifetime(hwnd, pid, creation, tag));
    if rows.len() == before {
        return Ok(());
    }
    write_store(dir, &rows)
}

/// One observed live lifetime eligible for leftover restoration: the exact
/// native identity plus the stored membership tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LiveLifetime<'a> {
    pub(super) hwnd: u64,
    pub(super) pid: u32,
    pub(super) creation: &'a str,
    pub(super) tag: &'a str,
}

/// Pure restoration plan: foreign-session rows whose exact lifetime is still
/// observed. Never returns a row for the current session (that is a live
/// preimage, not a leftover) and never matches a recycled HWND.
pub(super) fn restore_plan(
    rows: &[AbovePreimage],
    session: &str,
    live: &[LiveLifetime<'_>],
) -> Vec<AbovePreimage> {
    rows.iter()
        .filter(|row| row.is_foreign_session(session))
        .filter(|row| {
            live.iter().any(|candidate| {
                row.same_lifetime(
                    candidate.hwnd,
                    candidate.pid,
                    candidate.creation,
                    candidate.tag,
                )
            })
        })
        .cloned()
        .collect()
}

/// Restore verified leftovers at startup and drop their rows. Restores only
/// rows of a different tiler session with an exact still-observed lifetime;
/// a row whose current band already equals the preimage needs no write but is
/// still cleared once its lifetime is verified. Persistence failures after a
/// successful native restore keep the row for the next startup and never
/// re-write the window here. Returns how many rows were restored.
pub(super) fn restore_leftovers(
    dir: &Path,
    session: &str,
    live: &[LiveLifetime<'_>],
) -> usize {
    let Ok(mut rows) = read_store(dir) else {
        return 0;
    };
    let plan = restore_plan(&rows, session, live);
    if plan.is_empty() {
        return 0;
    }
    let mut restored = 0usize;
    let mut dirty = false;
    for row in &plan {
        let current = super::workspace_mode::read_topmost_now(row.hwnd);
        if current == row.prior_topmost
            || super::workspace_mode::set_topmost_band(row.hwnd, row.prior_topmost)
        {
            restored += 1;
            rows.retain(|existing| !existing.same_slot(row));
            dirty = true;
        }
    }
    if dirty {
        // Best-effort slot compaction; the native restore above already
        // happened, so a failed publish only means a later startup retries
        // an idempotent no-op restore.
        let _ = write_store(dir, &rows);
    }
    restored
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
    fn restore_plan_skips_current_session_and_unobserved_lifetimes() {
        let rows = vec![row(10, "session-a", false), row(11, "session-b", true)];
        let live = [
            LiveLifetime {
                hwnd: 10,
                pid: 7,
                creation: "creation-a",
                tag: "tag-a",
            },
            LiveLifetime {
                hwnd: 11,
                pid: 7,
                creation: "creation-a",
                tag: "tag-a",
            },
        ];
        let plan = restore_plan(&rows, "session-b", &live);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].hwnd, 10);
        assert!(!plan[0].prior_topmost);
        // Same-session rows are never leftovers.
        assert!(restore_plan(&rows, "session-a", &live).len() == 1);
        assert_eq!(restore_plan(&rows, "session-a", &live)[0].hwnd, 11);
    }

    #[test]
    fn restore_plan_requires_exact_tag_and_creation() {
        let rows = vec![row(10, "session-a", false)];
        let live = [LiveLifetime {
            hwnd: 10,
            pid: 7,
            creation: "creation-a",
            tag: "tag-b",
        }];
        assert!(restore_plan(&rows, "session-b", &live).is_empty());
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
        assert_eq!(load(&dir, 10, 7, "creation-a", "tag-a"), Some(prior.clone()));
        // A recycled HWND (same handle, other creation) never loads it.
        assert!(load(&dir, 10, 7, "creation-b", "tag-a").is_none());
        // Persisting a second lifetime keeps both rows.
        let second = AbovePreimage::new(11, 7, "creation-a", "tag-a", true, "session-a")
            .expect("second");
        persist(&dir, &second).expect("persist second");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_some());
        assert!(load(&dir, 11, 7, "creation-a", "tag-a").is_some());
        clear(&dir, 10, 7, "creation-a", "tag-a").expect("clear");
        assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_none());
        assert!(load(&dir, 11, 7, "creation-a", "tag-a").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
