#![forbid(unsafe_code)]

//! Phase 1 core adapter: pure retained observation state plus the
//! [`tiler_core`] event constructor. No threads, locks, or sinks: owners
//! in [`crate::runtime_thread`] own these values and drive them.

use std::collections::BTreeMap;

use tiler_core::{
    boundary::{CoreCommand, CoreEvent},
    directional::{OutputId, WindowId, WorkspaceId},
    geometry::Rect,
    ids::{CorrelationId, GenerationId, OwnerId},
    seed::EngineWindow,
    session::{DomainKey, OutputDomain, Session},
    size_hints::{FixedSizePredicate, WindowSizeHints, is_fixed_size_with},
    window_policy::should_hold_born_fullscreen,
};

use crate::runtime_types::{
    NativeContext, ObservedWindow, PHASE1_BOUNDS, PHASE1_GAP, PHASE1_GENERATION, PHASE1_OUTER_GAP,
    PHASE1_OUTPUT, PHASE1_OWNER, PHASE1_WORKSPACE, ScopedWindowMeta, parse_window_id, window_id,
};

// -- Engine retained state -----------------------------------------------------

/// Core-session lifetime for one observed window, read from the existing
/// core session (no adapter lifetime maps). Unknown means first-seen: the
/// wire hold and the first-sight fixed assert both key off it. A stored
/// minimize slot counts as known lifetime (previously tiled) without being
/// a reserved tile slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct KnownState {
    pub member: bool,
    pub exception: bool,
    pub automatic: bool,
    pub slot: bool,
}

impl KnownState {
    fn of(session: Option<&Session>, id: &WindowId) -> Self {
        let Some(session) = session else {
            return Self::default();
        };
        Self {
            member: session
                .snapshot()
                .windows
                .iter()
                .any(|link| link.window == *id),
            exception: session.is_exception(id),
            automatic: session.is_automatic_fixed_float(id),
            slot: session.minimized_slot(id).is_some(),
        }
    }

    fn reserved_slot(&self) -> bool {
        self.member
    }

    fn known_lifetime(&self) -> bool {
        self.member || self.exception || self.slot
    }
}

/// Wire flags for one observed window: normalized `(floating, fixed_auto)`
/// over the truthful classifier facts plus core-session lifetime (KDE
/// `withInitialFullscreenHold` / `withFixedSizeFloats` parity, macos side
/// without adapter maps). Fullscreen and hints always ride truthfully and
/// are NOT decided here — see [`build_event`].
///
/// - Born fullscreen (shared [`should_hold_born_fullscreen`] over an
///   accurate mapping: `known_slot` is a reserved tile slot (member),
///   `seen_nonfullscreen` is any core lifetime — member, exception, or
///   stored slot. A slot-known returnee is previously tiled, never born,
///   so it never synthetic-holds; it reports truthfully on the wire.
/// - Retained fullscreen passes through truthfully (slot kept in plan
///   truth; the engine withholds geometry writes). A still-held arrival
///   keeps synthetic floating while fullscreen.
/// - First-sight fixed hints assert adapter automatic origin
///   (floating + fixed_auto, membership only, no writes). Ordinary pins
///   stay plain; retained automatics re-assert across hint loss (D2);
///   classified floats never gain provenance (intentional/transient).
#[must_use]
pub(crate) fn normalize_wire_flags(
    floating_cls: bool,
    fullscreen: bool,
    hints: WindowSizeHints,
    known: &KnownState,
    predicate: FixedSizePredicate,
) -> (bool, bool) {
    if should_hold_born_fullscreen(fullscreen, known.reserved_slot(), known.known_lifetime()) {
        return (true, false);
    }
    if known.exception && !known.automatic && fullscreen && !floating_cls {
        return (true, false);
    }
    if known.exception && known.automatic && !fullscreen {
        return (true, true);
    }
    if !known.known_lifetime()
        && !fullscreen
        && !floating_cls
        && is_fixed_size_with(hints, predicate)
    {
        return (true, true);
    }
    (floating_cls, known.exception && known.automatic)
}

pub(crate) struct RetainedApp {
    pub(crate) generation: u64,
    pub(crate) seq_last: u64,
    pub(crate) cmd_counter: u64,
    pub(crate) windows: Vec<ObservedWindow>,
    pub(crate) focused_wid: Option<u32>,
}

#[derive(Default)]
pub(crate) struct RetainedCore {
    pub(crate) apps: BTreeMap<u32, RetainedApp>,
    /// Observed focus truth: set/cleared only by typed AX observations,
    /// never by plans. A dispatched (planned) focus that is dropped or never
    /// observed must not advance this; input acts on this subject.
    pub(crate) observed_focus: Option<WindowId>,
    pub(crate) focus_seq: u64,
    pub(crate) last_revision: u64,
    pub(crate) correlation: u64,
    /// Native-mode window→domain homing, keyed by runtime window id
    /// (`"<pid>_<wid>"`). Refreshed on every scoped observation (members
    /// only) and pruned per reporting PID, so input subjects always resolve
    /// against current homing — never a stale domain.
    pub(crate) window_domains: BTreeMap<String, DomainKey>,
    /// Native-mode topology revision of the last applied scoped
    /// observation (`0` until the first). Drives retired-domain release:
    /// domain keys absent from a newer revision retire before planning.
    pub(crate) native_revision: u64,
    /// Native-mode known domains (key plus authoritative bounds) for
    /// retirement diffing and release events.
    pub(crate) known_domains: BTreeMap<DomainKey, OutputDomain>,
    /// Native-mode per-domain core revision cursor, echoed into each
    /// domain's events. Sessions advance independently per domain; a single
    /// shared cursor would misreport every domain but one.
    pub(crate) domain_revisions: BTreeMap<DomainKey, u64>,
}

/// Clear observed focus when it points into `pid` (authoritative loss:
/// terminate, relaunch, or an observation that no longer reports it).
pub(crate) fn clear_focus_into(retained: &mut RetainedCore, pid: u32) {
    let inside = retained
        .observed_focus
        .as_ref()
        .and_then(|w| parse_window_id(&w.0))
        .is_some_and(|(fpid, _)| fpid == pid);
    if inside {
        retained.observed_focus = None;
    }
}

/// Deterministic FNV-1a fingerprint over sorted `(window, rect)` pairs,
/// mirroring the Windows adapter helper shape.
pub(crate) fn fingerprint(windows: &[(String, Rect)]) -> u64 {
    let mut sorted: Vec<(&String, &Rect)> = windows.iter().map(|(t, r)| (t, r)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf29ce484222325;
    for (token, rect) in sorted {
        for byte in token.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for word in [rect.x, rect.y, rect.w, rect.h] {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

pub(crate) fn skeleton_domain() -> OutputDomain {
    OutputDomain {
        id: OutputId(PHASE1_OUTPUT.to_owned()),
        workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
        bounds: PHASE1_BOUNDS,
        gap: PHASE1_GAP,
        adjacent: BTreeMap::new(),
    }
}

pub(crate) fn skeleton_key() -> DomainKey {
    DomainKey {
        output: OutputId(PHASE1_OUTPUT.to_owned()),
        workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
    }
}

pub(crate) fn build_event(
    retained: &mut RetainedCore,
    focused_override: Option<WindowId>,
    closed: Vec<WindowId>,
    command: CoreCommand,
    session: Option<&Session>,
    predicate: FixedSizePredicate,
) -> CoreEvent {
    let mut rows: Vec<(String, Rect)> = Vec::new();
    let mut windows: Vec<EngineWindow> = Vec::new();
    for (pid, app) in &retained.apps {
        collect_app_windows(
            &mut rows,
            &mut windows,
            *pid,
            &app.windows,
            PHASE1_OUTPUT,
            PHASE1_WORKSPACE,
            session,
            predicate,
        );
    }
    assemble_event(
        retained,
        retained.last_revision,
        &rows,
        windows,
        skeleton_domain(),
        skeleton_key(),
        focused_override,
        closed,
        command,
    )
}

/// Collect one app's retained windows into core wire rows for a single
/// domain: fingerprint rows plus normalized [`EngineWindow`] values homed to
/// exactly (`output`, `workspace`). Shared by the synthetic path (all apps
/// into the skeleton domain) and the native path (member apps per domain).
#[allow(clippy::too_many_arguments)]
fn collect_app_windows(
    rows: &mut Vec<(String, Rect)>,
    windows: &mut Vec<EngineWindow>,
    pid: u32,
    observed: &[ObservedWindow],
    output: &str,
    workspace: &str,
    session: Option<&Session>,
    predicate: FixedSizePredicate,
) {
    for w in observed {
        let id = window_id(pid, w.wid);
        let known = KnownState::of(session, &WindowId(id.clone()));
        let rect = Rect {
            x: w.x,
            y: w.y,
            w: w.w,
            h: w.h,
        };
        rows.push((id.clone(), rect));
        // Wire normalization over core-session lifetime: fullscreen and
        // hints ride truthfully; floating/fixed_auto carry the hold and
        // first-sight asserts. Retained overlays keep their reserved
        // slot/allocation in plan truth (no writes via the emission
        // filter); born arrivals hold slotless. The macOS adapter performs
        // no maximized inference.
        let (floating, fixed_auto) =
            normalize_wire_flags(w.floating, w.fullscreen, w.hints, &known, predicate);
        windows.push(EngineWindow {
            window: WindowId(id),
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            rect,
            floating,
            fit_excluded: false,
            fullscreen: w.fullscreen,
            maximized: false,
            sticky: false,
            fixed_auto,
            fixed_suppress: false,
            hints: w.hints,
        });
    }
}

/// Assemble one [`CoreEvent`] over already-collected wire rows: fingerprint,
/// correlation advance, and focus resolution. Domain, revision, and command
/// ride exactly as given — one call per domain in native mode, one call
/// total in synthetic mode.
#[allow(clippy::too_many_arguments)]
fn assemble_event(
    retained: &mut RetainedCore,
    revision: u64,
    rows: &[(String, Rect)],
    windows: Vec<EngineWindow>,
    domain: OutputDomain,
    domain_key: DomainKey,
    focused_override: Option<WindowId>,
    closed: Vec<WindowId>,
    command: CoreCommand,
) -> CoreEvent {
    retained.correlation += 1;
    let fingerprint = fingerprint(rows);
    let focused_window = focused_override
        .or_else(|| retained.observed_focus.clone())
        .unwrap_or(WindowId(String::new()));
    CoreEvent {
        owner: OwnerId::parse(PHASE1_OWNER).expect("skeleton owner token"),
        generation: GenerationId::parse(PHASE1_GENERATION).expect("skeleton generation token"),
        correlation: CorrelationId::parse(&format!("phase-1-{}", retained.correlation))
            .expect("skeleton correlation token"),
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap: PHASE1_OUTER_GAP,
        focused_window,
        windows,
        closed_windows: closed,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        command,
    }
}

/// Native-mode member partition for one scoped observation: each observed
/// window homes to exactly one context domain by live Space membership —
/// never by geometry. A window is a member iff its membership is exactly
/// `[space]` for some domain's current user Space; sticky, other-Space,
/// unknown (empty/unreadable), and metadataless windows omit with a count
/// (never a close report). When several domains share one current Space
/// (unified span or mid-transition duplicates), membership alone cannot
/// home: the observed rect arbitrates by greatest integer overlap with a
/// unique-maximum rule (ties, zero overlap, and degenerate rects omit).
/// Returns per-domain member windows plus the omitted count.
pub(crate) fn partition_scoped_members(
    windows: &[ObservedWindow],
    meta: &[ScopedWindowMeta],
    ctx: &NativeContext,
) -> (BTreeMap<DomainKey, Vec<ObservedWindow>>, usize) {
    let spaces_of = |wid: u32| {
        meta.iter()
            .find(|m| m.wid == wid)
            .map(|m| m.spaces.as_slice())
            .unwrap_or(&[])
    };
    let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
    let mut omitted = 0usize;
    for w in windows {
        let spaces = spaces_of(w.wid);
        let homed = if spaces.len() == 1 {
            let current = spaces[0];
            let candidates: Vec<&crate::runtime_types::NativeDomain> =
                ctx.domains.iter().filter(|d| d.space == current).collect();
            match candidates.len() {
                0 => None,
                1 => Some(candidates[0]),
                _ => arbitrate_shared_domain(w, &candidates),
            }
        } else {
            None
        };
        match homed {
            Some(domain) => {
                members
                    .entry(DomainKey {
                        output: domain.output.clone(),
                        workspace: domain.workspace.clone(),
                    })
                    .or_default()
                    .push(*w);
            }
            None => omitted += 1,
        }
    }
    (members, omitted)
}

/// Greatest-overlap arbitration for a window whose current Space is homed
/// by several domains: the observed integer rect must overlap exactly one
/// candidate work area strictly more than every other (ties and zero
/// overlap omit — homing unprovable, never guessed).
fn arbitrate_shared_domain<'a>(
    w: &ObservedWindow,
    candidates: &[&'a crate::runtime_types::NativeDomain],
) -> Option<&'a crate::runtime_types::NativeDomain> {
    if w.w <= 0 || w.h <= 0 {
        return None;
    }
    let (ax0, ay0, ax1, ay1) = (
        i64::from(w.x),
        i64::from(w.y),
        i64::from(w.x) + i64::from(w.w),
        i64::from(w.y) + i64::from(w.h),
    );
    let area = |r: &Rect| {
        let (bx0, by0, bx1, by1) = (
            i64::from(r.x),
            i64::from(r.y),
            i64::from(r.x) + i64::from(r.w),
            i64::from(r.y) + i64::from(r.h),
        );
        ((ax1.min(bx1) - ax0.max(bx0)).max(0)) * ((ay1.min(by1) - ay0.max(by0)).max(0))
    };
    let mut best: Option<(&crate::runtime_types::NativeDomain, i64)> = None;
    let mut tied = false;
    for &domain in candidates {
        let overlap = area(&domain.visible);
        match best {
            None => best = Some((domain, overlap)),
            Some((_, top)) if overlap > top => {
                best = Some((domain, overlap));
                tied = false;
            }
            Some((_, top)) if overlap == top => {
                tied = true;
            }
            _ => {}
        }
    }
    match best {
        Some((domain, top)) if top > 0 && !tied => Some(domain),
        _ => None,
    }
}

/// Native-mode member windows for one domain across ALL retained apps:
/// every retained window currently homed to `key`. Observations arrive
/// per app, but each domain event must converge the complete domain
/// membership — otherwise convergence would read a partial observation as
/// mass removal.
pub(crate) fn domain_member_apps(
    retained: &RetainedCore,
    key: &DomainKey,
) -> Vec<(u32, Vec<ObservedWindow>)> {
    let mut out = Vec::new();
    for (pid, app) in &retained.apps {
        let members: Vec<ObservedWindow> = app
            .windows
            .iter()
            .filter(|w| {
                retained
                    .window_domains
                    .get(&window_id(*pid, w.wid))
                    .is_some_and(|k| k == key)
            })
            .copied()
            .collect();
        if !members.is_empty() {
            out.push((*pid, members));
        }
    }
    out
}

/// Drop every window→domain mapping for one PID (terminate/relaunch).
/// Domain events converge full retained membership, so a dead lifetime's
/// mappings must go before planning — otherwise the closed windows would
/// ride as members instead of closes.
pub(crate) fn forget_app_members(retained: &mut RetainedCore, pid: u32) {
    let prefix = format!("{pid}_");
    retained
        .window_domains
        .retain(|id, _| !id.starts_with(&prefix));
}

/// Record one scoped observation's homing: insert this PID's member
/// mappings and prune this PID's mappings for windows no longer homed
/// anywhere (authoritative loss for exactly this lifetime — other PIDs are
/// untouched, so concurrent apps never invalidate each other).
pub(crate) fn note_scoped_members(
    retained: &mut RetainedCore,
    pid: u32,
    members: &BTreeMap<DomainKey, Vec<ObservedWindow>>,
) {
    let prefix = format!("{pid}_");
    retained
        .window_domains
        .retain(|id, _| !id.starts_with(&prefix));
    for (key, windows) in members {
        for w in windows {
            retained
                .window_domains
                .insert(window_id(pid, w.wid), key.clone());
        }
    }
}

/// Native-mode domain view: the authoritative [`OutputDomain`] plus its key
/// for one tile domain, derived from the shared [`NativeContext`].
pub(crate) struct NativeDomainView {
    pub(crate) key: DomainKey,
    pub(crate) domain: OutputDomain,
}

/// Native-mode domain views from the shared context: every exactly-homed
/// current-user-Space display becomes one core domain planned into its
/// actual visible work area (menu/Dock-excluded integer units — never the
/// skeleton bounds). Order follows the context (display order).
pub(crate) fn native_domain_views(
    ctx: &crate::runtime_types::NativeContext,
) -> Vec<NativeDomainView> {
    ctx.domains
        .iter()
        .map(|d| {
            let key = DomainKey {
                output: d.output.clone(),
                workspace: d.workspace.clone(),
            };
            let domain = OutputDomain {
                id: d.output.clone(),
                workspace: d.workspace.clone(),
                bounds: d.visible,
                gap: PHASE1_GAP,
                adjacent: BTreeMap::new(),
            };
            NativeDomainView { key, domain }
        })
        .collect()
}

/// Placeholder domain for a release event whose authoritative bounds are
/// already gone from retention (defensive only: retirement always finds
/// the key in `known_domains`). Shape-only filler — the release path
/// never validates bounds, gaps, or membership.
pub(crate) fn retired_domain_placeholder(key: &DomainKey) -> OutputDomain {
    OutputDomain {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

/// Build one native core event for a single tile domain: only that domain's
/// member windows ride (already filtered by exact current-Space membership),
/// homed to exactly the domain key, with that domain session's lifetime for
/// wire normalization and that domain's revision cursor. One call per
/// domain per observation; the engine converges each independently, so
/// sessions stay per-display without any additive core API.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_native_event(
    retained: &mut RetainedCore,
    view: &NativeDomainView,
    member_apps: &[(u32, Vec<ObservedWindow>)],
    focused_override: Option<WindowId>,
    closed: Vec<WindowId>,
    command: CoreCommand,
    session: Option<&Session>,
    predicate: FixedSizePredicate,
    revision: u64,
) -> CoreEvent {
    let mut rows: Vec<(String, Rect)> = Vec::new();
    let mut windows: Vec<EngineWindow> = Vec::new();
    for (pid, observed) in member_apps {
        collect_app_windows(
            &mut rows,
            &mut windows,
            *pid,
            observed,
            &view.key.output.0,
            &view.key.workspace.0,
            session,
            predicate,
        );
    }
    assemble_event(
        retained,
        revision,
        &rows,
        windows,
        view.domain.clone(),
        view.key.clone(),
        focused_override,
        closed,
        command,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::size_hints::WindowSizeHints;

    fn fixed() -> WindowSizeHints {
        WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        }
    }

    fn known(member: bool, exception: bool, automatic: bool) -> KnownState {
        KnownState {
            member,
            exception,
            automatic,
            slot: false,
        }
    }

    fn slotted() -> KnownState {
        KnownState {
            slot: true,
            ..KnownState::default()
        }
    }

    #[test]
    fn wire_flags_hold_born_and_assert_first_sight() {
        let plain = WindowSizeHints::none();
        let unknown = KnownState::default();
        // Born fullscreen: synthetic floating, never automatic, overlay
        // decided outside (fullscreen rides truthfully).
        assert_eq!(
            normalize_wire_flags(false, true, plain, &unknown, FixedSizePredicate::BothAxes),
            (true, false)
        );
        // Born dialog+fullscreen keeps its float origin under the hold.
        assert_eq!(
            normalize_wire_flags(true, true, plain, &unknown, FixedSizePredicate::BothAxes),
            (true, false)
        );
        // First-sight fixed asserts automatic origin (membership only).
        assert_eq!(
            normalize_wire_flags(
                false,
                false,
                fixed(),
                &unknown,
                FixedSizePredicate::BothAxes
            ),
            (true, true)
        );
        // Ordinary first sight passes through plain.
        assert_eq!(
            normalize_wire_flags(false, false, plain, &unknown, FixedSizePredicate::BothAxes),
            (false, false)
        );
        // Classified floats never gain provenance.
        assert_eq!(
            normalize_wire_flags(true, false, fixed(), &unknown, FixedSizePredicate::BothAxes),
            (true, false)
        );
    }

    #[test]
    fn wire_flags_keep_retained_shapes_truthful() {
        let plain = WindowSizeHints::none();
        // Retained tile going fullscreen: truthful, slot kept core-side.
        assert_eq!(
            normalize_wire_flags(
                false,
                true,
                plain,
                &known(true, false, false),
                FixedSizePredicate::BothAxes
            ),
            (false, false)
        );
        // Pinned tile reporting fixed hints: stays plain (D2/D3).
        assert_eq!(
            normalize_wire_flags(
                false,
                false,
                fixed(),
                &known(true, false, false),
                FixedSizePredicate::BothAxes
            ),
            (false, false)
        );
        // Retained automatic re-asserts across hint loss (D2).
        assert_eq!(
            normalize_wire_flags(
                false,
                false,
                plain,
                &known(false, true, true),
                FixedSizePredicate::BothAxes
            ),
            (true, true)
        );
        // Still-held arrival keeps synthetic floating while fullscreen.
        assert_eq!(
            normalize_wire_flags(
                false,
                true,
                plain,
                &known(false, true, false),
                FixedSizePredicate::BothAxes
            ),
            (true, false)
        );
        // Retained dialog+fullscreen stays a truthful overlay exception.
        assert_eq!(
            normalize_wire_flags(
                true,
                true,
                plain,
                &known(false, true, false),
                FixedSizePredicate::BothAxes
            ),
            (true, false)
        );
        // Stored-slot returnees never synthetic-hold and never claim
        // first-sight origin: the gate sees prior life and the truthful
        // fullscreen shape converges core-side.
        assert_eq!(
            normalize_wire_flags(false, true, plain, &slotted(), FixedSizePredicate::BothAxes),
            (false, false)
        );
        assert_eq!(
            normalize_wire_flags(
                false,
                false,
                fixed(),
                &slotted(),
                FixedSizePredicate::BothAxes
            ),
            (false, false)
        );
    }

    #[test]
    fn fingerprint_is_order_independent() {
        let a = vec![
            (
                "b".to_owned(),
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            ),
            (
                "a".to_owned(),
                Rect {
                    x: 1,
                    y: 1,
                    w: 2,
                    h: 2,
                },
            ),
        ];
        let mut b = a.clone();
        b.reverse();
        assert_eq!(fingerprint(&a), fingerprint(&b));
        let c = vec![(
            "a".to_owned(),
            Rect {
                x: 9,
                y: 9,
                w: 2,
                h: 2,
            },
        )];
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }

    // -- Native member partition -------------------------------------------------

    use crate::runtime_types::{NativeContext, NativeDomain, ScopedWindowMeta};
    use tiler_core::directional::{OutputId, WorkspaceId};

    fn native_ctx() -> NativeContext {
        NativeContext {
            revision: 7,
            domains: vec![
                NativeDomain {
                    cg_id: 1,
                    uuid: Some("uuid-1".to_owned()),
                    output: OutputId("macos-display-1".to_owned()),
                    workspace: WorkspaceId("space-11".to_owned()),
                    space: 11,
                    bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                    visible: Rect {
                        x: 0,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                },
                NativeDomain {
                    cg_id: 2,
                    uuid: Some("uuid-2".to_owned()),
                    output: OutputId("macos-display-2".to_owned()),
                    workspace: WorkspaceId("space-21".to_owned()),
                    space: 21,
                    bounds: Rect {
                        x: 1440,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                    visible: Rect {
                        x: 1440,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                },
            ],
            skipped: Vec::new(),
        }
    }

    fn observed(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x,
            y,
            w,
            h,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn meta(wid: u32, spaces: &[u64]) -> ScopedWindowMeta {
        ScopedWindowMeta {
            wid,
            spaces: spaces.to_vec(),
            fullscreen: false,
            minimized: false,
            hidden: false,
        }
    }

    #[test]
    fn partition_homes_by_membership_never_geometry() {
        let ctx = native_ctx();
        // Geometry sits inside display 1's area for every window: only
        // membership decides (a geometry reader would home all three to
        // display 1).
        let windows = vec![
            observed(1, 10, 10, 100, 100),
            observed(2, 10, 10, 100, 100),
            observed(3, 10, 10, 100, 100),
            observed(4, 10, 10, 100, 100),
            observed(5, 10, 10, 100, 100),
        ];
        let meta = vec![
            meta(1, &[11]),
            meta(2, &[21]),
            meta(3, &[99]),
            meta(4, &[11, 21]),
            meta(5, &[]),
        ];
        let (members, omitted) = partition_scoped_members(&windows, &meta, &ctx);
        assert_eq!(omitted, 3, "other/sticky/unknown omit");
        assert_eq!(members.len(), 2);
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-21".to_owned()),
        };
        assert_eq!(
            members
                .get(&d1)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>()),
            Some(vec![1])
        );
        assert_eq!(
            members
                .get(&d2)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>()),
            Some(vec![2])
        );
    }

    #[test]
    fn partition_metadataless_windows_omit() {
        // No metadata entry at all: unknown home, omitted (never guessed
        // into a domain from geometry).
        let ctx = native_ctx();
        let windows = vec![observed(9, 10, 10, 100, 100)];
        let (members, omitted) = partition_scoped_members(&windows, &[], &ctx);
        assert!(members.is_empty());
        assert_eq!(omitted, 1);
    }

    #[test]
    fn partition_shared_space_arbitrates_by_overlap() {
        // Both domains home the same current Space: the observed rect
        // picks the unique overlap winner; ties and misses omit.
        let mut ctx = native_ctx();
        ctx.domains[1].space = 11;
        ctx.domains[1].workspace = WorkspaceId("space-11".to_owned());
        let windows = vec![
            observed(1, 100, 100, 200, 200),
            observed(2, 1500, 100, 200, 200),
            observed(3, 1340, 100, 200, 200),
            observed(4, 5000, 5000, 10, 10),
        ];
        let meta = vec![
            meta(1, &[11]),
            meta(2, &[11]),
            meta(3, &[11]),
            meta(4, &[11]),
        ];
        let (members, omitted) = partition_scoped_members(&windows, &meta, &ctx);
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let wids = |key: &DomainKey| {
            members
                .get(key)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        assert_eq!(wids(&d1), vec![1]);
        assert_eq!(wids(&d2), vec![2]);
        // Window 3 straddles the seam exactly (100px each side): tie omits.
        // Window 4 overlaps nothing: omits.
        assert_eq!(omitted, 2);
    }

    #[test]
    fn member_mapping_tracks_and_prunes_per_pid() {
        let mut retained = RetainedCore::default();
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-21".to_owned()),
        };
        let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
        members.insert(d1.clone(), vec![observed(1, 0, 0, 10, 10)]);
        members.insert(d2.clone(), vec![observed(2, 0, 0, 10, 10)]);
        // Retained app windows ride alongside the mapping (the engine
        // stores member windows per app; the mapping homes them).
        retained.apps.insert(
            1001,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
                focused_wid: None,
            },
        );
        note_scoped_members(&mut retained, 1001, &members);
        assert_eq!(
            domain_member_apps(&retained, &d1),
            vec![(1001, vec![observed(1, 0, 0, 10, 10)])]
        );
        // A new observation for the same PID replaces its mappings (window
        // 1 authoritative loss) without touching other PIDs.
        retained
            .window_domains
            .insert("1002_9".to_owned(), d2.clone());
        let mut next: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
        next.insert(d2.clone(), vec![observed(2, 0, 0, 10, 10)]);
        note_scoped_members(&mut retained, 1001, &next);
        assert!(domain_member_apps(&retained, &d1).is_empty());
        assert_eq!(
            retained.window_domains.get("1002_9"),
            Some(&d2),
            "other PIDs survive a PID-local re-home"
        );
        // Terminate prunes the whole PID.
        forget_app_members(&mut retained, 1001);
        assert!(!retained.window_domains.contains_key("1001_2"));
        assert!(retained.window_domains.contains_key("1002_9"));
    }
}
