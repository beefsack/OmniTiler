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
    ObservedWindow, PHASE1_BOUNDS, PHASE1_GAP, PHASE1_GENERATION, PHASE1_OUTER_GAP, PHASE1_OUTPUT,
    PHASE1_OWNER, PHASE1_WORKSPACE, parse_window_id, window_id,
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
        for w in &app.windows {
            let id = window_id(*pid, w.wid);
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
            // filter); born arrivals hold slotless. The macOS skeleton
            // performs no maximized inference.
            let (floating, fixed_auto) =
                normalize_wire_flags(w.floating, w.fullscreen, w.hints, &known, predicate);
            windows.push(EngineWindow {
                window: WindowId(id),
                output: OutputId(PHASE1_OUTPUT.to_owned()),
                workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
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
    retained.correlation += 1;
    let fingerprint = fingerprint(&rows);
    let focused_window = focused_override
        .or_else(|| retained.observed_focus.clone())
        .unwrap_or(WindowId(String::new()));
    CoreEvent {
        owner: OwnerId::parse(PHASE1_OWNER).expect("skeleton owner token"),
        generation: GenerationId::parse(PHASE1_GENERATION).expect("skeleton generation token"),
        correlation: CorrelationId::parse(&format!("phase-1-{}", retained.correlation))
            .expect("skeleton correlation token"),
        revision: retained.last_revision,
        fingerprint,
        domain: skeleton_domain(),
        domain_key: skeleton_key(),
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
}
