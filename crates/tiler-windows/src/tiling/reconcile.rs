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

/// Per-window write memory with two narrow lanes and no learned policy:
///
/// - `clamp`: a successful write whose readback still mismatches (the app
///   holds its own size). The identical intent is skipped until the desired
///   rectangle or the native observation genuinely changes.
/// - `transient`: a failed setter API call. Retried with bounded backoff
///   (500ms doubling, capped at 5s); any intent/observation change re-arms
///   immediately, so transient failures always recover.
#[derive(Debug, Default, Clone)]
pub struct RefusedTracker {
    clamp: HashMap<String, (Rect, Rect)>,
    transient: HashMap<String, TransientEntry>,
}

/// Backoff base for transient setter failures.
pub const TRANSIENT_BASE_MS: u64 = 500;
/// Backoff cap for transient setter failures.
pub const TRANSIENT_MAX_MS: u64 = 5000;

#[derive(Debug, Clone)]
struct TransientEntry {
    desired: Rect,
    observed: Rect,
    failures: u32,
    last: std::time::Instant,
}

impl RefusedTracker {
    /// `true` when this exact `(desired, observed)` pair is currently
    /// suppressed: an identical app-clamp refusal, or a transient failure
    /// still inside its backoff window.
    #[must_use]
    pub fn should_skip(
        &self,
        token: &str,
        desired: &Rect,
        observed: &Rect,
        now: std::time::Instant,
    ) -> bool {
        if self
            .clamp
            .get(token)
            .is_some_and(|(d, o)| d == desired && o == observed)
        {
            return true;
        }
        if let Some(entry) = self.transient.get(token)
            && entry.desired == *desired
            && entry.observed == *observed
        {
            let shift = entry.failures.min(4);
            let backoff = (TRANSIENT_BASE_MS << shift).min(TRANSIENT_MAX_MS);
            if now.duration_since(entry.last).as_millis() < u128::from(backoff) {
                return true;
            }
        }
        false
    }

    /// Exact application clears both lanes.
    pub fn note_match(&mut self, token: &str) {
        self.clamp.remove(token);
        self.transient.remove(token);
    }

    /// Successful write, readback still mismatches: app-held size.
    pub fn note_clamp(&mut self, token: &str, desired: &Rect, observed: &Rect) {
        if desired == observed {
            self.note_match(token);
        } else {
            self.clamp.insert(token.to_owned(), (*desired, *observed));
        }
    }

    /// Failed setter API call: bounded-backoff lane.
    pub fn note_transient(
        &mut self,
        token: &str,
        desired: &Rect,
        observed: &Rect,
        now: std::time::Instant,
    ) {
        let failures = self
            .transient
            .get(token)
            .filter(|entry| entry.desired == *desired && entry.observed == *observed)
            .map_or(0, |entry| entry.failures.saturating_add(1));
        self.transient.insert(
            token.to_owned(),
            TransientEntry {
                desired: *desired,
                observed: *observed,
                failures,
                last: now,
            },
        );
    }

    /// Forget tokens for windows no longer managed.
    pub fn retain(&mut self, live_tokens: &[&str]) {
        self.clamp
            .retain(|token, _| live_tokens.contains(&token.as_str()));
        self.transient
            .retain(|token, _| live_tokens.contains(&token.as_str()));
    }

    /// App-clamp lane pair for one token, if any. Pure diagnostic read: lets
    /// the per-window summary explain suppression (`identical-clamp`) and
    /// re-arm (`desired-changed` / `observed-changed` / `exact-match`)
    /// without touching tracker behavior.
    #[must_use]
    pub fn clamp_pair(&self, token: &str) -> Option<(Rect, Rect)> {
        self.clamp.get(token).copied()
    }

    /// Transient-lane intent pair for one token, if any (timing excluded).
    /// Same diagnostic read contract as [`Self::clamp_pair`].
    #[must_use]
    pub fn transient_pair(&self, token: &str) -> Option<(Rect, Rect)> {
        self.transient
            .get(token)
            .map(|entry| (entry.desired, entry.observed))
    }
}

/// Post-write readback decision for one desired entry. Only a window that
/// was actually written this tick may enter the app-clamp lane; a mismatch
/// with no write behind it stays pending so transient backoff and re-reads
/// keep working instead of being pinned as a permanent clamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackOutcome {
    Match,
    Clamp,
    Pending,
}

/// Decide the tracker update for one `(desired, readback)` pair.
/// `wrote_this_tick` must be true only when this tick's setter succeeded
/// for the token; a failed or skipped setter is never a clamp.
#[must_use]
pub const fn readback_outcome(
    wrote_this_tick: bool,
    desired: &Rect,
    readback: &Rect,
) -> ReadbackOutcome {
    if desired.x == readback.x
        && desired.y == readback.y
        && desired.w == readback.w
        && desired.h == readback.h
    {
        ReadbackOutcome::Match
    } else if wrote_this_tick {
        ReadbackOutcome::Clamp
    } else {
        ReadbackOutcome::Pending
    }
}

/// Decide the tracker update for one `(desired, readback)` pair after the
/// final DPI basis check. `matched` wins over everything: an exact arrival
/// needs no rewrite even across a DPI flip. A written mismatch whose
/// per-attempt DPI basis differs from the live DPI is an invalidated
/// placement, never an app-held clamp: it stays pending so a later tick
/// rebases under fresh state instead of pinning a permanent clamp. Only a
/// stable-DPI written mismatch is an app clamp; anything unwritten is
/// pending (transient/backoff lanes keep working). Unreadable DPI on
/// either side never guesses a transition: the stable lane applies.
#[must_use]
pub const fn dpi_readback_decision(
    wrote_this_tick: bool,
    matched: bool,
    dpi_attempt: Option<u32>,
    dpi_live: Option<u32>,
) -> ReadbackOutcome {
    if matched {
        return ReadbackOutcome::Match;
    }
    if matches!(
        (dpi_attempt, dpi_live),
        (Some(before), Some(after)) if before != after
    ) {
        return ReadbackOutcome::Pending;
    }
    if wrote_this_tick {
        ReadbackOutcome::Clamp
    } else {
        ReadbackOutcome::Pending
    }
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

/// Trace-only per-window transfer readback entry: the requested effective
/// visible rectangle with the immediate post-write readback plus DPI,
/// outer-frame, and inset evidence for the mismatch analysis. A DPI change
/// across the write is evidence, not a discriminator: it does not by
/// itself distinguish virtualization from app self-resize. A later tick
/// re-emits the same shape under its own tick/op. Opaque token plus
/// integers only (no HWNDs, pids, paths, titles); `None` means unreadable,
/// never guessed. Pure formatter; the native path calls it under trace.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn transfer_readback_entry(
    window: &str,
    effective: &Rect,
    readback: Option<&Rect>,
    dpi_write: Option<u32>,
    dpi_readback: Option<u32>,
    outer: Option<&Rect>,
    outer_readback: Option<&Rect>,
    insets_readback: Option<[i32; 4]>,
) -> serde_json::Value {
    let matched = readback.is_some_and(|r| r == effective);
    let dpi_changed = match (dpi_write, dpi_readback) {
        (Some(before), Some(after)) => before != after,
        _ => false,
    };
    let rect_json = |rect: &Rect| serde_json::json!([rect.x, rect.y, rect.w, rect.h]);
    serde_json::json!({
        "window": window,
        "desired": [effective.x, effective.y, effective.w, effective.h],
        "readback": readback.map_or(serde_json::Value::Null, rect_json),
        "matched": matched,
        "dpi_write": dpi_write.map_or(serde_json::Value::Null, |dpi| serde_json::json!(dpi)),
        "dpi_readback": dpi_readback.map_or(serde_json::Value::Null, |dpi| serde_json::json!(dpi)),
        "dpi_changed": dpi_changed,
        "outer": outer.map_or(serde_json::Value::Null, rect_json),
        "outer_readback": outer_readback.map_or(serde_json::Value::Null, rect_json),
        "insets_readback": insets_readback.map_or(serde_json::Value::Null, |insets| serde_json::json!(insets)),
    })
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
/// Complete observation inputs for one single-domain `Reconcile` tick.
pub struct ReconcileInput<'a> {
    pub owner: &'a OwnerId,
    pub generation: &'a GenerationId,
    pub correlation: &'a CorrelationId,
    pub revision: u64,
    pub fingerprint: u64,
    pub domain_bounds: Rect,
    pub windows: &'a [(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)],
    pub focused: Option<&'a WindowId>,
}

/// Build a `Reconcile` event for one explicit `(output, workspace)` domain.
/// Rectangles are the current visible observations plus hidden snapshots for
/// that domain; `revision` is the last Engine-accepted revision for it.
/// Hidden Engine membership is preserved by inclusion: a retained member that
/// is invisible, minimized, maximized, or fullscreen rides its last-known
/// rectangle instead of vanishing from the observation.
/// Hints ride per window: fresh application-declared minimums for eligible
/// visible and identity-verified hidden observations, no hint on failure.
/// Hint-only changes still reach
/// projection because the caller never skips the Engine on an unchanged
/// rectangle fingerprint.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_reconcile_event_for(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)],
    focused: Option<&WindowId>,
) -> CoreEvent {
    let floating: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = windows
        .iter()
        .map(|(window, rect, hints)| (window.clone(), *rect, *hints, false))
        .collect();
    build_reconcile_event_for_floating(
        owner,
        generation,
        correlation,
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        &floating,
        focused,
    )
}

/// Floating-aware per-domain `Reconcile` event: intentional floats and
/// born-held fullscreen members ride as slotless floating Engine exceptions
/// (no tile slot, siblings keep the tile area) while every other member stays
/// tiled. Floating is an Engine observation only, never a native write.
/// Hidden floating rows stay floating until reveal; hidden born rows stay
/// floating until a verified non-fullscreen exit.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_reconcile_event_for_floating(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )],
    focused: Option<&WindowId>,
) -> CoreEvent {
    CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation: correlation.clone(),
        revision,
        fingerprint,
        domain: domain.clone(),
        domain_key: domain_key.clone(),
        outer_gap,
        focused_window: focused.cloned().unwrap_or(WindowId(String::new())),
        windows: windows
            .iter()
            .map(|(window, rect, hints, floating)| EngineWindow {
                window: window.clone(),
                output: domain_key.output.clone(),
                workspace: domain_key.workspace.clone(),
                rect: *rect,
                floating: *floating,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: *hints,
            })
            .collect(),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::Reconcile,
    }
}

/// Build the single-domain `Reconcile` event for one tick. Rectangles are the
/// current visible observations; `revision` is the last Engine-accepted
/// revision for the domain.
#[must_use]
pub fn build_reconcile_event(input: &ReconcileInput<'_>) -> CoreEvent {
    let domain = OutputDomain {
        id: OutputId(OUTPUT_ID.to_owned()),
        workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
        bounds: input.domain_bounds,
        gap: INNER_GAP,
        adjacent: std::collections::BTreeMap::new(),
    };
    let domain_key = DomainKey {
        output: OutputId(OUTPUT_ID.to_owned()),
        workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
    };
    CoreEvent {
        owner: input.owner.clone(),
        generation: input.generation.clone(),
        correlation: input.correlation.clone(),
        revision: input.revision,
        fingerprint: input.fingerprint,
        domain: domain.clone(),
        domain_key,
        outer_gap: OUTER_GAP,
        focused_window: input.focused.cloned().unwrap_or(WindowId(String::new())),
        windows: input
            .windows
            .iter()
            .map(|(window, rect, hints)| EngineWindow {
                window: window.clone(),
                output: OutputId(OUTPUT_ID.to_owned()),
                workspace: WorkspaceId(WORKSPACE_ID.to_owned()),
                rect: *rect,
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: *hints,
            })
            .collect(),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::Reconcile,
    }
}
