#![forbid(unsafe_code)]
//! Adapter-local adoption-time native baseline for app-owned fullscreen
//! (R14).
//!
//! When the tiler first adopts an ordinary, windowed lifetime it records the
//! window's native state exactly then: the chrome bits inside the project
//! owned mask, the maximize state, and the actual outer rectangle. This is
//! evidence only. It is deliberately separate from the project-owned
//! fullscreen marker ([`super::fullscreen`]) and from the R12 crash-safe
//! chrome preimage slot ([`super::fullscreen_preimage`]):
//!
//! * Capturing never writes to the application window: reads plus this
//!   adapter-owned file slot only, so no app-owned native state changes and
//!   no new exit authority is claimed.
//! * A window that is already fullscreen when first seen has no baseline;
//!   first-seen app fullscreen stays refused (REQ-MAX-05/05b).
//! * A project-owned fullscreen marker or an R12 chrome preimage for the
//!   same lifetime means the frame is already project-controlled, so no
//!   adoption baseline is recorded then.
//! * Rows are bound to the exact native lifetime (HWND plus pid plus process
//!   creation plus the live membership tag), never a recycled handle.
//!
//! The slot is keyed by the same exact lifetime as R12 and is pruned in the
//! same startup recovery pass, but lives in its own document so mixing the
//! two could never turn evidence into restoration authority. A future
//! per-case accepted restore must additionally prove the settled actual
//! outer geometry before retiring either evidence row
//! ([`super::fullscreen::outer_rect_settled`] exists for that proof).

use std::path::Path;

use tiler_core::geometry::Rect;

use super::ObservedWindow;

/// Serialized baseline document version. A different version is unreadable
/// (never guessed, never trusted).
const BASELINE_VERSION: u32 = 1;
/// Adapter-owned durable slot name inside the ledger directory.
const BASELINE_FILE: &str = "adoption-baselines.json";

/// One adoption-time native baseline for one exact window lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AdoptionBaseline {
    pub(super) version: u32,
    pub(super) hwnd: u64,
    pub(super) pid: u32,
    pub(super) creation: String,
    /// Stored membership tag (opaque, non-empty) proving the lifetime.
    pub(super) tag: String,
    /// Tiler session that recorded the row (provenance only; this slot is
    /// never restoration authority, so a foreign session's row is kept).
    pub(super) session: String,
    /// Owned chrome mask present at adoption (`WS_CAPTION`/`WS_THICKFRAME`
    /// subset). Zero is valid: a borderless-windowed app has no caption.
    pub(super) chrome: u32,
    /// Maximize state observed with the same adoption read.
    pub(super) was_maximized: bool,
    /// Actual outer rectangle (`GetWindowRect`) at adoption. Always present:
    /// the eventual per-case restore proof compares the settled live outer
    /// rectangle against exactly this frame.
    pub(super) outer: Rect,
}

impl AdoptionBaseline {
    /// Build one row for a live lifetime. Empty identity fields, chrome bits
    /// outside the owned mask, or a degenerate outer rectangle is never a
    /// valid baseline and nothing is persisted.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        hwnd: u64,
        pid: u32,
        creation: &str,
        tag: &str,
        session: &str,
        chrome: u32,
        was_maximized: bool,
        outer: Rect,
    ) -> Option<Self> {
        let row = Self {
            version: BASELINE_VERSION,
            hwnd,
            pid,
            creation: creation.to_owned(),
            tag: tag.to_owned(),
            session: session.to_owned(),
            chrome,
            was_maximized,
            outer,
        };
        row.valid().then_some(row)
    }

    /// Whether this row is a complete, internally consistent baseline.
    fn valid(&self) -> bool {
        self.version == BASELINE_VERSION
            && !self.creation.is_empty()
            && !self.tag.is_empty()
            && !self.session.is_empty()
            && self.chrome & !super::fullscreen::FULLSCREEN_STYLE_BITS == 0
            && self.outer.w > 0
            && self.outer.h > 0
    }

    /// Exact lifetime equality (HWND plus pid plus process creation plus the
    /// stored tag). Never a HWND-only or pid-only match.
    fn same_lifetime(
        &self,
        hwnd: u64,
        pid: u32,
        creation: &str,
        tag: &str,
    ) -> bool {
        self.hwnd == hwnd
            && self.pid == pid
            && self.creation == creation
            && !self.creation.is_empty()
            && self.tag == tag
            && !self.tag.is_empty()
    }

    /// Same window object, regardless of the membership tag. A same-process
    /// HWND reuse keeps hwnd/pid/creation but stamps a fresh tag, so the old
    /// row can never verify again and is replaced rather than accumulated.
    fn same_window(&self, other: &Self) -> bool {
        self.hwnd == other.hwnd && self.pid == other.pid && self.creation == other.creation
    }

    pub(super) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "v": self.version,
            "hwnd": self.hwnd,
            "pid": self.pid,
            "creation": self.creation,
            "tag": self.tag,
            "session": self.session,
            "chrome": self.chrome,
            "wasMaximized": self.was_maximized,
            "outer": serde_json::json!({
                "x": self.outer.x,
                "y": self.outer.y,
                "w": self.outer.w,
                "h": self.outer.h,
            }),
        })
    }

    pub(super) fn from_json(row: &serde_json::Value) -> Option<Self> {
        let version = u32::try_from(row.get("v")?.as_u64()?).ok()?;
        if version != BASELINE_VERSION {
            return None;
        }
        let hwnd = row.get("hwnd")?.as_u64()?;
        let pid = u32::try_from(row.get("pid")?.as_u64()?).ok()?;
        let creation = row.get("creation")?.as_str()?.to_owned();
        let tag = row.get("tag")?.as_str()?.to_owned();
        let session = row.get("session")?.as_str()?.to_owned();
        let chrome = u32::try_from(row.get("chrome")?.as_u64()?).ok()?;
        let was_maximized = row.get("wasMaximized")?.as_bool()?;
        let outer = row.get("outer")?;
        let x = i32::try_from(outer.get("x")?.as_i64()?).ok()?;
        let y = i32::try_from(outer.get("y")?.as_i64()?).ok()?;
        let w = i32::try_from(outer.get("w")?.as_i64()?).ok()?;
        let h = i32::try_from(outer.get("h")?.as_i64()?).ok()?;
        let row = Self {
            version,
            hwnd,
            pid,
            creation,
            tag,
            session,
            chrome,
            was_maximized,
            outer: Rect { x, y, w, h },
        };
        row.valid().then_some(row)
    }
}

/// Read the durable slot. `Ok(rows)` with an absent file is an empty store; a
/// malformed/unreadable document is `Err(())` so callers fail closed instead
/// of believing an empty store and overwriting recoverable evidence.
fn read_store(dir: &Path) -> Result<Vec<AdoptionBaseline>, ()> {
    let path = dir.join(BASELINE_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(()),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let document: serde_json::Value = serde_json::from_str(text).map_err(|_| ())?;
    let version = u32::try_from(
        document
            .get("v")
            .and_then(serde_json::Value::as_u64)
            .ok_or(())?,
    )
    .map_err(|_| ())?;
    if version != BASELINE_VERSION {
        return Err(());
    }
    let rows = document
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .ok_or(())?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // One corrupt row invalidates the whole slot: partial trust would
        // let a later capture silently drop a recoverable baseline.
        out.push(AdoptionBaseline::from_json(row).ok_or(())?);
    }
    Ok(out)
}

fn write_store(dir: &Path, rows: &[AdoptionBaseline]) -> Result<(), ()> {
    let document = serde_json::json!({
        "v": BASELINE_VERSION,
        "rows": rows.iter().map(AdoptionBaseline::to_json).collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|_| ())?;
    crate::storage::publish_replace(dir, BASELINE_FILE, &bytes, "adoption-baseline")
        .map_err(|_| ())
}

/// Persist one row exactly once per lifetime: the first adoption wins and a
/// later call never rewrites the baseline (adoption-time evidence, not a
/// rolling snapshot). A stale row for the same window object (fresh tag after
/// same-process HWND reuse) is replaced. `Ok(false)` means the exact lifetime
/// is already present; `Err(())` means the slot is unreadable or the publish
/// failed, in which case nothing is trusted and no window write follows (this
/// path never writes the window anyway).
pub(super) fn persist_first(dir: &Path, row: &AdoptionBaseline) -> Result<bool, ()> {
    if !row.valid() {
        return Err(());
    }
    let mut rows = read_store(dir)?;
    if rows
        .iter()
        .any(|existing| existing.same_lifetime(row.hwnd, row.pid, &row.creation, &row.tag))
    {
        return Ok(false);
    }
    rows.retain(|existing| !existing.same_window(row));
    rows.push(row.clone());
    write_store(dir, &rows).map(|()| true)
}

/// Load the row for one exact lifetime, if any. Unreadable slots return
/// `None`; adoption evidence is never guessed from a broken document.
#[cfg(test)]
pub(super) fn load(
    dir: &Path,
    hwnd: u64,
    pid: u32,
    creation: &str,
    tag: &str,
) -> Option<AdoptionBaseline> {
    read_store(dir)
        .ok()?
        .into_iter()
        .find(|row| row.same_lifetime(hwnd, pid, creation, tag))
}

/// D5 pruning decision for one durable baseline row: a closed window, or a
/// handle that now belongs to another process, can never verify its stored
/// lifetime again. `window_pid` needs no handle access rights, so a mismatch
/// is a gone lifetime rather than an access-denied guess.
fn row_window_gone(row: &AdoptionBaseline) -> bool {
    if !crate::ffi::is_window(row.hwnd) {
        return true;
    }
    crate::ffi::window_pid(row.hwnd) != row.pid
}

/// A row's lifetime can never be evidence again once the owning process is
/// provably gone (or its pid belongs to a different creation). Unreadable
/// identity is never treated as dead: the row waits for a later attempt.
fn lifetime_provably_dead(row: &AdoptionBaseline) -> bool {
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

/// A live window now carrying a different membership tag is a same-process
/// HWND reuse: the stored tag can never verify again. An absent/unreadable
/// live tag is not proof (never guessed), so it keeps the row.
fn live_tag_reused(row: &AdoptionBaseline) -> bool {
    crate::product_hide::sys::read_member_tag(row.hwnd)
        .is_some_and(|live| live != row.tag)
}

/// Drop baseline rows whose exact lifetime is provably gone. Runs from the
/// same startup recovery entry as R12 so the two adapter-owned slots share
/// one pruning pass; a live window's adoption baseline always survives.
/// Returns how many rows were pruned.
pub(super) fn prune_dead(dir: &Path) -> usize {
    let Ok(mut rows) = read_store(dir) else {
        return 0;
    };
    let before = rows.len();
    rows.retain(|row| {
        !row_window_gone(row) && !lifetime_provably_dead(row) && !live_tag_reused(row)
    });
    let pruned = before - rows.len();
    if pruned > 0 {
        // Best effort: a failed publish leaves rows that a later startup
        // prunes again. No native state depends on this slot.
        let _ = write_store(dir, &rows);
    }
    pruned
}

/// Typed capture outcome. Reasons are a closed, content-free vocabulary so a
/// trace line can name why adoption recorded no baseline without exposing
/// app content or raw native identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureOutcome {
    Captured,
    Present,
    Skipped(&'static str),
}

impl CaptureOutcome {
    /// Bounded trace vocabulary (presence only, never geometry or identity).
    fn label(self) -> (&'static str, Option<&'static str>) {
        match self {
            CaptureOutcome::Captured => ("captured", None),
            CaptureOutcome::Present => ("present", None),
            CaptureOutcome::Skipped(reason) => ("skipped", Some(reason)),
        }
    }
}

/// Capture the adoption-time native baseline for one observed window. Pure
/// reads plus the adapter-owned slot: no window property, style, frame,
/// activation, or z-order write ever runs here. Refuses when the window is
/// not a settled windowed shape, when the project already owns the frame
/// (fullscreen marker or pending R12 preimage), or when any lifetime leg
/// fails to verify. First capture wins; a later tick reports `Present`.
pub(super) fn capture(dir: &Path, window: &ObservedWindow) -> CaptureOutcome {
    let hwnd = window.hwnd;
    let facts = &window.facts;
    if !facts.visible {
        return CaptureOutcome::Skipped("not-visible");
    }
    if facts.minimized {
        return CaptureOutcome::Skipped("minimized");
    }
    // First-seen fullscreen has no pre-fullscreen adoption state; the
    // existing refusal stays untouched.
    if facts.captionless_fullscreen {
        return CaptureOutcome::Skipped("fullscreen");
    }
    if !crate::ffi::is_window(hwnd) {
        return CaptureOutcome::Skipped("not-window");
    }
    if crate::ffi::window_pid(hwnd) != window.identity.pid {
        return CaptureOutcome::Skipped("unverified-lifetime");
    }
    let Ok(held) = crate::native::HeldProcess::open(window.identity.pid) else {
        return CaptureOutcome::Skipped("unverified-lifetime");
    };
    let Ok(live) = held.identity() else {
        return CaptureOutcome::Skipped("unverified-lifetime");
    };
    if live.pid != window.identity.pid
        || live.process_creation != window.identity.process_creation
    {
        return CaptureOutcome::Skipped("unverified-lifetime");
    }
    let Some(tag) = crate::product_hide::sys::read_member_tag(hwnd).filter(|tag| !tag.is_empty())
    else {
        return CaptureOutcome::Skipped("unverified-lifetime");
    };
    // The observed maximize fact and the capture read must describe one
    // settled adoption state; a transition between them records nothing.
    if crate::ffi::is_zoomed(hwnd) != facts.maximized {
        return CaptureOutcome::Skipped("unsettled");
    }
    let Some(style) = crate::ffi::window_style_checked(hwnd) else {
        return CaptureOutcome::Skipped("unreadable-style");
    };
    // Separate from the project-owned fullscreen marker: a marked frame is
    // already project-controlled state, never an application baseline.
    if super::fullscreen::read_fullscreen_meta(hwnd).is_some() {
        return CaptureOutcome::Skipped("owned-marker");
    }
    // Coordinate with R12: a pending chrome preimage for this exact
    // lifetime means a project entry/restore still owns the native frame.
    if super::fullscreen_preimage::load(
        dir,
        hwnd,
        live.pid,
        &live.process_creation,
        &tag,
    )
    .is_some()
    {
        return CaptureOutcome::Skipped("r12-preimage");
    }
    let Ok(session) = crate::native::current_identity() else {
        return CaptureOutcome::Skipped("unverified-session");
    };
    let Some(row) = AdoptionBaseline::new(
        hwnd,
        live.pid,
        &live.process_creation,
        &tag,
        &session.process_creation,
        style & super::fullscreen::FULLSCREEN_STYLE_BITS,
        facts.maximized,
        window.outer,
    ) else {
        return CaptureOutcome::Skipped("invalid-frame");
    };
    match persist_first(dir, &row) {
        Ok(true) => CaptureOutcome::Captured,
        Ok(false) => CaptureOutcome::Present,
        Err(()) => CaptureOutcome::Skipped("store"),
    }
}

/// Bounded, presence-only adoption trace line: event, opaque member token,
/// outcome, and (for skips) one closed reason. No HWND, pid, creation, tag,
/// rectangle, executable, or title.
pub(super) fn trace_line(token: &str, outcome: CaptureOutcome) -> serde_json::Value {
    let (outcome, reason) = outcome.label();
    let mut line = serde_json::json!({
        "event": "adoption-baseline",
        "window": token,
        "outcome": outcome,
    });
    if let Some(reason) = reason {
        line["reason"] = serde_json::json!(reason);
    }
    line
}

/// Capture and, only when trace is enabled, emit the bounded presence line.
/// Callers use this after a fresh lifetime admission; capture never affects
/// admission success.
pub(super) fn capture_with_trace(
    dir: &Path,
    window: &ObservedWindow,
    trace: bool,
    log_path: &Path,
) {
    let outcome = capture(dir, window);
    if trace {
        super::trace::log_json_at(log_path, trace_line(&window.token, outcome));
    }
}
