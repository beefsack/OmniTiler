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

// NOTE: normalized gesture lifecycle and pure gesture decisions (stage 4a)
// are owned by the shared core (`tiler_core::gesture`); the adapter only
// observes, captures, renders, and applies. Native call sites import the
// owner directly.
