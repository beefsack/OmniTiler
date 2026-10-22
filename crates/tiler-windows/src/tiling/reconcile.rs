#![forbid(unsafe_code)]
use super::*;

/// Portable observed target identity for frozen-allowlist matching.
/// `tag` is the owned-helper lifetime token (`OmniTilerLifetime`
/// property): nonempty for owned helpers, required in every allowlist entry
/// so a recycled HWND in the same process never matches its predecessor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedTarget {
    pub hwnd: u64,
    pub pid: u32,
    pub process_creation: String,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub tag: String,
}

/// One frozen allowlist entry, captured at test start. `tag` is required:
/// empty tags are malformed and refused before any lease or write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    pub hwnd: u64,
    pub pid: u32,
    pub process_creation: String,
    pub exe_path: String,
    pub user_sid: String,
    pub session_id: u32,
    pub tag: String,
}

/// Parse a test-mode allowlist receipt. Malformed, empty, duplicate-HWND,
/// empty-identity, or empty-tag input is an error; the caller must never fall
/// back to normal tiling on a bad allowlist.
pub fn parse_allowlist(json: &str) -> Result<Vec<AllowEntry>, String> {
    #[derive(serde::Deserialize)]
    struct Receipt {
        windows: Vec<AllowEntryJson>,
    }
    #[derive(serde::Deserialize)]
    struct AllowEntryJson {
        hwnd: u64,
        pid: u32,
        process_creation: String,
        exe_path: String,
        user_sid: String,
        session_id: u32,
        #[serde(default)]
        tag: String,
    }
    let receipt: Receipt =
        serde_json::from_str(json).map_err(|_| "refuse: malformed allowlist".to_owned())?;
    if receipt.windows.is_empty() {
        return Err("refuse: empty allowlist".to_owned());
    }
    if receipt.windows.iter().any(|w| {
        w.hwnd == 0
            || w.pid == 0
            || w.process_creation.is_empty()
            || w.exe_path.is_empty()
            || w.user_sid.is_empty()
            || w.tag.is_empty()
    }) {
        return Err("refuse: malformed allowlist".to_owned());
    }
    let mut seen = std::collections::HashSet::new();
    for window in &receipt.windows {
        if !seen.insert(window.hwnd) {
            return Err("refuse: duplicate allowlist hwnd".to_owned());
        }
    }
    Ok(receipt
        .windows
        .into_iter()
        .map(|w| AllowEntry {
            hwnd: w.hwnd,
            pid: w.pid,
            process_creation: w.process_creation,
            exe_path: w.exe_path,
            user_sid: w.user_sid,
            session_id: w.session_id,
            tag: w.tag,
        })
        .collect())
}

/// Deterministic FNV-1a 64-bit digest over the frozen allowlist in HWND order.
/// Binds hwnd/pid/creation/exe/sid/session/tag; empty input digests as zero.
#[must_use]
pub fn allowlist_digest(entries: &[AllowEntry]) -> String {
    let mut sorted: Vec<&AllowEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.hwnd);
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for e in sorted {
        mix(&e.hwnd.to_le_bytes());
        mix(&e.pid.to_le_bytes());
        mix(e.process_creation.as_bytes());
        mix(e
            .exe_path
            .to_ascii_lowercase()
            .replace('/', "\\")
            .as_bytes());
        mix(e.user_sid.as_bytes());
        mix(&e.session_id.to_le_bytes());
        mix(e.tag.as_bytes());
    }
    format!("{hash:016x}")
}

/// Exact frozen-identity match: HWND plus full process identity plus the
/// owned-helper lifetime tag. Exe paths compare slash/case-insensitively like
/// the owner checks. A recycled HWND in the same process carries a fresh tag
/// and never matches.
#[must_use]
pub fn allow_match(entry: &AllowEntry, observed: &ObservedTarget) -> bool {
    entry.hwnd == observed.hwnd
        && entry.pid == observed.pid
        && entry.process_creation == observed.process_creation
        && crate::lifecycle::exe_paths_equal(&entry.exe_path, &observed.exe_path)
        && entry.user_sid == observed.user_sid
        && entry.session_id == observed.session_id
        && entry.tag == observed.tag
        && !entry.tag.is_empty()
}

/// Recompute one placement outer after an observed DPI transition across the
/// write: the pre-write observation (frame insets plus DPI basis) is
/// invalidated placement under a new DPI state, not an app-held clamp. The
/// intended visible rectangle maps through the freshly verified frame, and
/// the caller reapplies once under the same gates as the initial write.
///
/// Returns `None` when no correction applies: unreadable DPI on either side
/// (never guessed), no change, or an unmappable frame. A recompute that
/// equals the attempted outer still corrects: the producer evidence scales
/// beyond inset deltas, so equal geometry reasserts the intended allocation
/// under verified fresh state instead of assuming the first write landed.
/// Pure; the native path supplies live reads and bounds correction to one
/// attempt per window per tick (a second flip falls through to the tick
/// readback lanes, so flapping DPI terminates without timers or counters).
#[must_use]
pub fn dpi_transition_reouter(
    dpi_before: Option<u32>,
    dpi_after: Option<u32>,
    effective: &Rect,
    fresh_insets: &FrameInsets,
) -> Option<Rect> {
    match (dpi_before, dpi_after) {
        (Some(before), Some(after)) if before != after => fresh_insets.visible_to_outer(*effective),
        _ => None,
    }
}

/// Domain-bounds fence for a settle-time drop point: the release cursor must
/// still sit inside the given domain bounds. The cross-output router
/// (`resolve_drop_destination` in `tiling_sys`) applies this per output:
/// same-output points keep the local route, pointer-output points join the
/// destination current view through the pair route (occupied) or singleton
/// admission (empty), and outside points refuse with snap-back and no
/// transfer.
#[must_use]
pub fn drop_point_in_domain(
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: i32,
    bounds_h: i32,
    x: i32,
    y: i32,
) -> bool {
    if bounds_w <= 0 || bounds_h <= 0 {
        return false;
    }
    let dx = i64::from(x) - i64::from(bounds_x);
    let dy = i64::from(y) - i64::from(bounds_y);
    dx >= 0 && dy >= 0 && dx < i64::from(bounds_w) && dy < i64::from(bounds_h)
}

/// One settled manual gesture, derived from pre/post-gesture rectangles plus
/// the settle-time cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureIntent {
    /// Position changed: re-place at the cursor drop point (KDE drag-drop).
    MoveDrop { x: i32, y: i32 },
    /// Size changed: pointer-resize the grabbed edges (KDE pointer-resize).
    /// Horizontal axis first, matching the KDE dual-axis route.
    PointerResize {
        direction: &'static str,
        boundary: i32,
        direction2: Option<&'static str>,
        boundary2: Option<i32>,
    },
}
/// Derive the settle intent from the stable pre-gesture rectangle and the
/// post-gesture rectangle plus the settle-time cursor position.
///
/// Same size at a new origin is a true move (KDE drag-drop). Any size change
/// makes it a resize: a changed origin means the left/top edge moved, never
/// a move. Edges derive per axis from before/post; when both edges of one
/// axis moved, the larger-magnitude edge wins (ties go to the origin edge).
/// `None` when nothing changed, the cursor is missing for a move, or a
/// boundary overflows `i32`.
#[must_use]
pub fn classify_gesture(
    before: &Rect,
    after: &Rect,
    cursor: Option<(i32, i32)>,
) -> Option<GestureIntent> {
    let pos_changed = before.x != after.x || before.y != after.y;
    let size_changed = before.w != after.w || before.h != after.h;
    if !pos_changed && !size_changed {
        return None;
    }
    if !size_changed {
        let (x, y) = cursor?;
        return Some(GestureIntent::MoveDrop { x, y });
    }
    let before_right = i64::from(before.x) + i64::from(before.w);
    let after_right = i64::from(after.x) + i64::from(after.w);
    let before_bottom = i64::from(before.y) + i64::from(before.h);
    let after_bottom = i64::from(after.y) + i64::from(after.h);
    let horizontal = pick_edge(
        before.x,
        after.x,
        before_right,
        after_right,
        "left",
        "right",
    )?;
    let vertical = pick_edge(before.y, after.y, before_bottom, after_bottom, "up", "down")?;
    match (horizontal, vertical) {
        (None, None) => None,
        (Some((direction, boundary)), None) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (None, Some((direction, boundary))) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (Some((direction, boundary)), Some((direction2, boundary2))) => {
            Some(GestureIntent::PointerResize {
                direction,
                boundary,
                direction2: Some(direction2),
                boundary2: Some(boundary2),
            })
        }
    }
}

/// Pick one axis edge from origin/edge deltas. `None` when neither moved;
/// the larger-magnitude delta wins when both moved, ties to the origin edge.
fn pick_edge(
    before_origin: i32,
    after_origin: i32,
    before_far: i64,
    after_far: i64,
    origin_name: &'static str,
    far_name: &'static str,
) -> Option<Option<(&'static str, i32)>> {
    let origin_moved = before_origin != after_origin;
    let far_moved = before_far != after_far;
    if !origin_moved && !far_moved {
        return Some(None);
    }
    if origin_moved
        && (!far_moved
            || (after_origin as i64 - before_origin as i64).abs() >= (after_far - before_far).abs())
    {
        return Some(Some((origin_name, after_origin)));
    }
    if far_moved {
        return Some(Some((far_name, after_far.try_into().ok()?)));
    }
    Some(Some((origin_name, after_origin)))
}
/// Prefer START-sampled modal truth over rect classification for one native
/// gesture intent: a caption-started move loop (`HTCAPTION` at START) carries
/// user move intent even when the system rescaled the frame mid-gesture
/// (mixed-DPI `WM_DPICHANGED` across outputs), which rect comparison alone
/// would misread as a user resize. A user resize always starts on a sizing
/// border instead, so only a resize verdict is overridden, and only with a
/// captured release cursor to place the drop. Move verdicts pass through;
/// cursor-less resizes keep the geometry route (no point to place).
#[must_use]
pub fn prefer_modal_move(
    classified: GestureIntent,
    cursor: Option<(i32, i32)>,
    modal_move: bool,
) -> GestureIntent {
    if modal_move
        && matches!(classified, GestureIntent::PointerResize { .. })
        && let Some((x, y)) = cursor
    {
        return GestureIntent::MoveDrop { x, y };
    }
    classified
}
