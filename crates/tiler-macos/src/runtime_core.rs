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
    session::{DomainKey, OutputDomain},
    size_hints::WindowSizeHints,
};

use crate::runtime_types::{
    ObservedWindow, PHASE1_BOUNDS, PHASE1_GAP, PHASE1_GENERATION, PHASE1_OUTER_GAP, PHASE1_OUTPUT,
    PHASE1_OWNER, PHASE1_WORKSPACE, parse_window_id, window_id,
};

// -- Engine retained state -----------------------------------------------------

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
) -> CoreEvent {
    let mut rows: Vec<(String, Rect)> = Vec::new();
    let mut windows: Vec<EngineWindow> = Vec::new();
    for (pid, app) in &retained.apps {
        for w in &app.windows {
            let id = window_id(*pid, w.wid);
            let rect = Rect {
                x: w.x,
                y: w.y,
                w: w.w,
                h: w.h,
            };
            rows.push((id.clone(), rect));
            windows.push(EngineWindow {
                window: WindowId(id),
                output: OutputId(PHASE1_OUTPUT.to_owned()),
                workspace: WorkspaceId(PHASE1_WORKSPACE.to_owned()),
                rect,
                floating: w.floating,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
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
