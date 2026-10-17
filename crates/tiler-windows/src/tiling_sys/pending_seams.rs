#![forbid(unsafe_code)]
use super::*;

// Pending-drop owner-integration seams (test-only, offline, native-free).
// Each injector is `None` in ordinary tests (fail-closed native behavior);
// pending fixtures set them to script deterministic owner reads/effects
// without creating windows, hooks, or mutating the desktop. All use safe
// `std` only, mirroring the existing `INJECTED_HINTS` pattern.
#[cfg(test)]
thread_local! {
    #[allow(clippy::type_complexity)]
    pub(super) static INJECTED_OBSERVE: std::cell::RefCell<
        Option<(Vec<ObservedWindow>, Vec<RetainedRow>, bool)>,
    > = const { std::cell::RefCell::new(None) };
    #[allow(clippy::missing_const_for_thread_local)]
    static INJECTED_TAGS: std::cell::RefCell<std::collections::HashMap<u64, Option<String>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_TAGS_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static INJECTED_SUSPEND: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    pub(super) static INJECTED_ELEVATED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    pub(super) static INJECTED_FOCUS: std::cell::RefCell<Option<&'static str>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static INJECTED_STOP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static PENDING_COUNTERS: std::cell::RefCell<PendingCounters> =
        const { std::cell::RefCell::new(PendingCounters::new()) };
    /// Scripted geometry-apply verdicts for cross-operation tails (one
    /// `(readback_ok, mismatched)` per stubbed apply, consumed in order):
    /// the stubbed apply returns the next scripted verdict instead of
    /// success so full-path refusal tests can inject failed readback/writes
    /// without native effects. Empty keeps the success stub. A single
    /// entry fails only the next apply; two entries script the
    /// destination-then-source applies of the empty flow independently.
    pub(super) static INJECTED_APPLY: std::cell::RefCell<std::collections::VecDeque<(bool, usize)>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
    /// Recorded writable-token sets, one per stubbed geometry apply, in
    /// call order: lets full-path fixtures assert exactly which tokens a
    /// transfer scoped for writes (mover excluded, occupants present)
    /// without native effects. Recorded only while the injected observation
    /// fixture is active; cleared by [`clear_pending_seams`].
    pub(super) static APPLY_WRITABLES: std::cell::RefCell<Vec<std::collections::BTreeSet<String>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Scripted live overlay reads `(maximized, fullscreen)` for output-send
    /// tails (one entry per `live_overlay_flags` call, consumed in order):
    /// lets full-path fixtures script a stable overlay snapshot through the
    /// dispatch gate plus both fence reads, or drift between the pre-write
    /// and post-write reads, without native windows. Empty keeps the
    /// production read (fail-closed on foreign HWNDs).
    pub(super) static INJECTED_OVERLAY: std::cell::RefCell<std::collections::VecDeque<(bool, Option<bool>)>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
    /// Iconic HWNDs for the injected observation fixture: production
    /// `EnumWindows` yields iconic windows and `observe` resolves them to
    /// known-minimized (omitted rows, stable tokens). The injected
    /// `observe` path therefore joins these HWNDs into the inventory and
    /// marks matching member keys omitted, mirroring the real arm without
    /// native queries. Unknown HWNDs are ignored (no membership).
    pub(super) static INJECTED_ICONIC: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    static INJECTED_ICONIC_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Enumerated-but-unresolved HWNDs for the injected observation
    /// fixture: production windows whose identity or frames fail stay in
    /// the raw inventory (`Unknown`) without tokens or rows. The injected
    /// path joins them into the inventory only, so cleanup never mistakes
    /// them for closes and assembly defers instead of fabricating rows.
    pub(super) static INJECTED_UNREADABLE: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Scripted hidden-claim recovery verdicts per HWND: `Ok` snapshots
    /// flow through the real audit classification (process/nonce compared
    /// against the stored claim); `Err` strings use the production
    /// vocabulary (`absent:*` / `refuse: sid/session mismatch` retire,
    /// anything else retains). Unscripted HWNDs while active retain as
    /// `uncertain` without a native call.
    static INJECTED_RECOVERY: std::cell::RefCell<
        std::collections::HashMap<
            u64,
            std::result::Result<crate::product_hide::sys::ProductSnapshot, String>,
        >,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_RECOVERY_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Fresh native arrival readback per HWND (test-only): the post-setter
    /// physical frame distinctly from the pre-move `stable` cache. Visible
    /// members (tiled plain, float, overlay) stage their post-move visible
    /// (DWM extended-frame) rect here, never the outer `GetWindowRect`;
    /// concealed members (minimized/hidden) stage their post-move
    /// `WINDOWPLACEMENT` normal rect in screen coordinates here (the
    /// production readback converts workspace to screen before proving).
    /// Unscripted HWNDs read as `None` (arrival unverified, never the stale
    /// cache, never translation math). Cleared by [`clear_pending_seams`].
    static INJECTED_ARRIVAL: std::cell::RefCell<std::collections::HashMap<u64, Rect>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    pub(super) static INJECTED_ARRIVAL_PLACEMENT: std::cell::RefCell<std::collections::HashMap<u64, Rect>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// Live `WINDOWPLACEMENT` normal rect plus workspace-coordinate basis per
    /// HWND (test-only): the pre-setter restore frame for the maximized
    /// overlay remap and the placement-screen conversion. The rect rides the
    /// raw `rcNormalPosition` basis (workspace for non-tool top-level,
    /// screen for tool windows); the flag records which. Unscripted HWNDs
    /// read as `None` (fail-closed, never a retained fallback).
    pub(super) static INJECTED_PLACEMENT_NORMALS: std::cell::RefCell<
        std::collections::HashMap<u64, (Rect, bool)>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
    pub(super) static INJECTED_FLOAT_MOVES: std::cell::RefCell<Vec<(u64, Rect)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(super) static INJECTED_FLOAT_MOVE_OK: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    /// Recorded ALL-member native output transfers `(hwnd, moved rect, kind)`
    /// in call order (test-only): the exact production helper boundary
    /// arguments for every float/overlay/minimized/hidden remap. `kind` is
    /// `float`, `overlay`, `minimized` or `hidden`. Recorded by the real
    /// helpers (test arm) so regressions prove which members attempted a
    /// native move, not just the tiled apply. Cleared by
    /// [`clear_pending_seams`].
    pub(super) static INJECTED_NATIVE_TRANSFERS: std::cell::RefCell<Vec<(u64, Rect, &'static str)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Setter verdict for overlay/minimized/hidden placement and overlay
    /// moves (test-only): `false` simulates a refused native setter without
    /// touching native state. Float moves keep their own
    /// `INJECTED_FLOAT_MOVE_OK` cell; this cell gates the other kinds.
    pub(super) static INJECTED_NATIVE_MOVE_OK: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct PendingCounters {
    pub(super) focus: usize,
    pub(super) engine: usize,
    pub(super) apply: usize,
    pub(super) reconcile: usize,
}

#[cfg(test)]
impl PendingCounters {
    const fn new() -> Self {
        Self {
            focus: 0,
            engine: 0,
            apply: 0,
            reconcile: 0,
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static INJECTED_AREAS: std::cell::RefCell<Option<Vec<MonitorArea>>> =
        const { std::cell::RefCell::new(None) };
}

/// Inject one controlled monitor topology for cross-operation tails
/// (test-only). `Some` lists replace the native enumeration; `None` clears
/// the injection. Tests must clear before arranging (see
/// [`clear_pending_seams`]).
#[cfg(test)]
pub(super) fn inject_monitor_areas(areas: Option<Vec<MonitorArea>>) {
    INJECTED_AREAS.with(|slot| {
        *slot.borrow_mut() = areas;
    });
}

#[cfg(test)]
pub(super) fn inject_pending_observe(
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    enum_failed: bool,
) {
    INJECTED_OBSERVE.with(|slot| {
        *slot.borrow_mut() = Some((observed, retained, enum_failed));
    });
}

#[cfg(test)]
pub(super) fn inject_pending_tag(hwnd: u64, tag: Option<String>) {
    INJECTED_TAGS.with(|map| {
        map.borrow_mut().insert(hwnd, tag);
    });
    INJECTED_TAGS_ACTIVE.with(|active| active.set(true));
}

#[cfg(test)]
pub(super) fn inject_pending_suspend(blocked: Option<bool>) {
    INJECTED_SUSPEND.with(|slot| slot.set(blocked));
}

#[cfg(test)]
pub(super) fn inject_pending_elevated(elevated: Option<bool>) {
    INJECTED_ELEVATED.with(|slot| slot.set(elevated));
}

#[cfg(test)]
pub(super) fn inject_pending_focus(outcome: Option<&'static str>) {
    INJECTED_FOCUS.with(|slot| {
        *slot.borrow_mut() = outcome;
    });
}

#[cfg(test)]
pub(super) fn inject_pending_stop(stopped: bool) {
    INJECTED_STOP.with(|slot| slot.set(stopped));
}

#[cfg(test)]
pub(super) fn inject_pending_apply(outcome: Option<(bool, usize)>) {
    INJECTED_APPLY.with(|slot| {
        let mut queue = slot.borrow_mut();
        queue.clear();
        if let Some(verdict) = outcome {
            queue.push_back(verdict);
        }
    });
}

#[cfg(test)]
pub(super) fn inject_pending_apply_seq(outcomes: Vec<(bool, usize)>) {
    INJECTED_APPLY.with(|slot| {
        let mut queue = slot.borrow_mut();
        queue.clear();
        queue.extend(outcomes);
    });
}

/// Script one sequence of live overlay reads for output-send tails
/// (test-only). Each `live_overlay_flags` call consumes the next entry;
/// exhausted queues fall back to the production read. Cleared by
/// [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_overlay_seq(flags: Vec<(bool, Option<bool>)>) {
    INJECTED_OVERLAY.with(|slot| {
        let mut queue = slot.borrow_mut();
        queue.clear();
        queue.extend(flags);
    });
}

/// Declare iconic HWNDs for the next injected observation (test-only).
/// Cleared by [`clear_pending_seams`]; existing `inject_pending_observe`
/// callers without iconic windows behave exactly as before.
#[cfg(test)]
pub(super) fn inject_pending_iconic(hwnds: Vec<u64>) {
    INJECTED_ICONIC.with(|slot| {
        *slot.borrow_mut() = hwnds.into_iter().collect();
    });
    INJECTED_ICONIC_ACTIVE.with(|active| active.set(true));
}

/// Declare enumerated-but-unresolved HWNDs for the next injected
/// observation (test-only). Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_unreadable(hwnds: Vec<u64>) {
    INJECTED_UNREADABLE.with(|slot| {
        *slot.borrow_mut() = hwnds.into_iter().collect();
    });
}

/// Script one hidden-claim recovery verdict (test-only). `Ok` snapshots run
/// the real audit comparison; `Err` strings must use the production
/// vocabulary. Activates the recovery seam like [`inject_pending_tag`].
#[cfg(test)]
pub(super) fn inject_pending_recovery(
    hwnd: u64,
    verdict: std::result::Result<crate::product_hide::sys::ProductSnapshot, String>,
) {
    INJECTED_RECOVERY.with(|map| {
        map.borrow_mut().insert(hwnd, verdict);
    });
    INJECTED_RECOVERY_ACTIVE.with(|active| active.set(true));
}

#[cfg(test)]
pub(super) fn pending_counters() -> PendingCounters {
    PENDING_COUNTERS.with(|slot| {
        let counters = slot.borrow();
        PendingCounters {
            focus: counters.focus,
            engine: counters.engine,
            apply: counters.apply,
            reconcile: counters.reconcile,
        }
    })
}

/// Recorded writable-token sets from stubbed geometry applies, in call
/// order (test-only). Each entry is the exact `writable` set one apply
/// scoped, so transfer tests assert exclusion/inclusion per side without
/// native effects.
#[cfg(test)]
pub(super) fn pending_apply_writables() -> Vec<std::collections::BTreeSet<String>> {
    APPLY_WRITABLES.with(|slot| slot.borrow().clone())
}

/// Recorded ALL-member native transfers `(hwnd, moved rect, kind)` in call
/// order (test-only): the exact production helper boundary arguments.
#[cfg(test)]
pub(super) fn pending_native_transfers() -> Vec<(u64, Rect, &'static str)> {
    INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow().clone())
}

/// Script the overlay/concealed native setter verdict (test-only).
/// Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_native_move_ok(ok: bool) {
    INJECTED_NATIVE_MOVE_OK.with(|slot| slot.set(ok))
}

/// Script the float native setter verdict (test-only).
#[cfg(test)]
pub(super) fn inject_pending_float_move_ok(ok: bool) {
    INJECTED_FLOAT_MOVE_OK.with(|slot| slot.set(ok))
}

/// Stage one fresh visible arrival frame for the post-setter physical
/// readback (test-only): the actual native window read distinctly from the
/// pre-move `stable` cache. Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_arrival(hwnd: u64, rect: Rect) {
    INJECTED_ARRIVAL.with(|map| {
        map.borrow_mut().insert(hwnd, rect);
    });
}

/// Stage one fresh concealed placement readback for the post-setter
/// `WINDOWPLACEMENT` normal rect (test-only): minimized/hidden arrival
/// proves the physical placement in screen coordinates, never translation
/// math. Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_arrival_placement(hwnd: u64, rect: Rect) {
    INJECTED_ARRIVAL_PLACEMENT.with(|map| {
        map.borrow_mut().insert(hwnd, rect);
    });
}

/// Stage one live placement normal rect plus its coordinate basis
/// (test-only): the pre-setter `rcNormalPosition` for the maximized overlay
/// remap. `workspace_coords` is true for non-tool top-level windows
/// (workspace coordinates) and false for tool windows (screen coordinates).
/// Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_placement_normal(hwnd: u64, rect: Rect, workspace_coords: bool) {
    INJECTED_PLACEMENT_NORMALS.with(|map| {
        map.borrow_mut().insert(hwnd, (rect, workspace_coords));
    });
}

#[cfg(test)]
pub(super) fn clear_pending_seams() {
    INJECTED_OBSERVE.with(|slot| {
        *slot.borrow_mut() = None;
    });
    INJECTED_AREAS.with(|slot| {
        *slot.borrow_mut() = None;
    });
    INJECTED_TAGS.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_TAGS_ACTIVE.with(|active| active.set(false));
    INJECTED_SUSPEND.with(|slot| slot.set(None));
    INJECTED_ELEVATED.with(|slot| slot.set(None));
    INJECTED_FOCUS.with(|slot| {
        *slot.borrow_mut() = None;
    });
    INJECTED_STOP.with(|slot| slot.set(false));
    INJECTED_APPLY.with(|slot| {
        slot.borrow_mut().clear();
    });
    APPLY_WRITABLES.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_OVERLAY.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_ICONIC.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_ICONIC_ACTIVE.with(|active| active.set(false));
    INJECTED_UNREADABLE.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_RECOVERY.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_RECOVERY_ACTIVE.with(|active| active.set(false));
    INJECTED_FLOAT_MOVES.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_FLOAT_MOVE_OK.with(|slot| slot.set(true));
    INJECTED_NATIVE_TRANSFERS.with(|slot| {
        slot.borrow_mut().clear();
    });
    INJECTED_NATIVE_MOVE_OK.with(|slot| slot.set(true));
    INJECTED_ARRIVAL.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_ARRIVAL_PLACEMENT.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_PLACEMENT_NORMALS.with(|map| {
        map.borrow_mut().clear();
    });
    PENDING_COUNTERS.with(|slot| {
        *slot.borrow_mut() = PendingCounters::new();
    });
    clear_test_hints();
}

/// Test-aware live member-tag read: injected tags stand in for the OS
/// property read when the pending fixture activated them (unit tests only;
/// compiled out of production builds). When active but unscripted for an
/// HWND, returns `None` without a native call so fixtures stay native-free.
/// Otherwise the production read runs.
pub(super) fn live_member_tag(hwnd: u64) -> Option<String> {
    #[cfg(test)]
    {
        if INJECTED_TAGS_ACTIVE.with(|active| active.get()) {
            return INJECTED_TAGS.with(|map| map.borrow().get(&hwnd).cloned().unwrap_or(None));
        }
    }
    crate::product_hide::sys::read_member_tag(hwnd)
}

/// Test-aware iconic-state read: injected iconic HWNDs stand in for the
/// native `IsIconic` query when the fixture activated them (unit tests only;
/// compiled out of production builds). Otherwise the production read runs
/// (fail-closed `false` on invalid HWNDs, so dead evidence never refreshes).
pub(super) fn live_is_iconic(hwnd: u64) -> bool {
    #[cfg(test)]
    {
        if INJECTED_ICONIC_ACTIVE.with(|active| active.get()) {
            return INJECTED_ICONIC.with(|slot| slot.borrow().contains(&hwnd));
        }
    }
    crate::ffi::is_iconic(hwnd)
}

/// Test-aware hidden-claim recovery read: injected verdicts stand in for the
/// OS/process/nonce reads when the fixture activated them (unit tests only;
/// compiled out of production builds). `Ok` snapshots flow through the real
/// audit classification; unscripted HWNDs retain as `uncertain` without a
/// native call. Otherwise the production read runs with its error mapped to
/// the same string vocabulary the audit matches.
pub(super) fn live_query_recovery(
    hwnd: u64,
    me: &ProcessIdentity,
) -> std::result::Result<crate::product_hide::sys::ProductSnapshot, String> {
    #[cfg(test)]
    {
        if INJECTED_RECOVERY_ACTIVE.with(|active| active.get()) {
            return INJECTED_RECOVERY.with(|map| {
                map.borrow()
                    .get(&hwnd)
                    .cloned()
                    .unwrap_or_else(|| Err("uncertain: unscripted recovery".to_owned()))
            });
        }
    }
    crate::product_hide::sys::query_recovery(hwnd, me).map_err(|e| e.to_string())
}

/// Fresh post-setter visible arrival read (causal, never the pre-move
/// `stable` cache): the live visible (DWM extended-frame) rect after the
/// actual setter, the same basis as [`observe_window`] and the Engine plan
/// rows. Under test reads the injected arrival boundary distinctly from
/// pre-state (`None` when unscripted, so missing readback refuses instead of
/// proving stale math). In production reads the live DWM extended frame
/// bounds (fail-closed `None`, never the outer `GetWindowRect`, so bordered
/// windows verify on their visible frame).
pub(super) fn fresh_arrival_rect(hwnd: u64) -> Option<Rect> {
    #[cfg(test)]
    {
        INJECTED_ARRIVAL.with(|map| map.borrow().get(&hwnd).copied())
    }
    #[cfg(not(test))]
    {
        live_visible_rect(hwnd)
    }
}

/// Live visible frame for one window: the DWM extended-frame bounds read
/// with the same shape as [`observe_window`]. Fail-closed `None` on any
/// failure (never the outer rect, never a fabricated value).
#[cfg(not(test))]
fn live_visible_rect(hwnd_u64: u64) -> Option<Rect> {
    if !crate::ffi::is_window(hwnd_u64) {
        return None;
    }
    let visible_raw = crate::ffi::extended_frame_bounds(hwnd_u64)?;
    rect_from_win(visible_raw)
}
