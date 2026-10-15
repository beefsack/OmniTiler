#![forbid(unsafe_code)]

//! Phase 1 observation helpers (offline, portable).
//!
//! Pure, dependency-free pieces of the per-app observation handoff so the
//! owner-thread machinery in [`crate::runtime_thread`] stays small:
//!
//! - [`BackendSnapshot`]: what a backend reports for one app at one instant:
//!   observed windows plus the focused window when known. `focused_wid:
//!   None` is honest unknown (unreadable or absent), never a claim of "no
//!   focus". Sequence and app generation are NOT carried here: the owning
//!   worker stamps those at publish time, so a backend can never forge a
//!   lifetime.
//! - [`snapshots_equal`]: geometry + focus equality for the no-busy-feedback
//!   rule. An unchanged snapshot is skipped, never republished: without this
//!   a native clamp/refusal (desired differs, observed identical) would
//!   replay endlessly.
//! - [`PendingObservation`]: latest-wins retry buffer for the typed backend
//!   sink. A saturated engine inbox retains the newest snapshot and retries;
//!   the final state is never silently lost. Overwrites coalesce (only the
//!   newest is kept).
//! - [`merge_readback`]: post-apply readback keeps the last known focus when
//!   it is still present, so a focus-blind frame readback never clears
//!   observed focus truth.

use crate::runtime::ObservedWindow;
use crate::runtime_types::ScopedBackendSnapshot;

/// One backend-reported observation payload for a single app. Plain data
/// only; handles never escape the owning thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSnapshot {
    pub windows: Vec<ObservedWindow>,
    pub focused_wid: Option<u32>,
}

/// True when resending `next` after `prev` would be busy feedback: same
/// geometry, focus knowledge, owner-read selected-oracle focus, membership
/// metadata, omissions, and revision. Focus evidence is deliberately
/// EXCLUDED (stamped fresh at every send; including it would republish on
/// every evidence refresh), but `sys_focus` is COMPARED: it is a real
/// owner-thread selected-oracle read per snapshot, so a changed selected
/// focus must republish — otherwise an app-local focus of a background app
/// would preserve stale global focus forever.
#[must_use]
pub fn scoped_snapshots_equal(prev: &ScopedBackendSnapshot, next: &ScopedBackendSnapshot) -> bool {
    prev.focused_wid == next.focused_wid
        && prev.focus_cg_owned == next.focus_cg_owned
        && prev.sys_focus == next.sys_focus
        && prev.windows == next.windows
        && prev.meta == next.meta
        && prev.excluded == next.excluded
        && prev.revision == next.revision
}

/// True when resending `next` after `prev` would be busy feedback: same
/// geometry and same focus knowledge.
#[must_use]
pub fn snapshots_equal(prev: &BackendSnapshot, next: &BackendSnapshot) -> bool {
    prev.focused_wid == next.focused_wid && prev.windows == next.windows
}

/// Stage one fresh snapshot against the last SENT state and the retained
/// pending state. Exactly one outcome, so callers cannot diverge:
///
/// - equal to last sent: the newest truth matches what the engine has, so
///   any retained older pending is stale transient — clear it, stage
///   nothing (no busy republish, no stale resurrection);
/// - otherwise: replace any retained pending with this latest snapshot
///   (single newest retained; an older pending never survives a newer
///   snapshot).
///
/// Staging never touches the publish sequence: `seq` advances only on send,
/// so the source stream stays monotonic across clears and replacements.
pub fn stage_latest(
    last: &Option<BackendSnapshot>,
    pending: &mut PendingObservation,
    snapshot: BackendSnapshot,
) {
    if last
        .as_ref()
        .is_some_and(|prev| snapshots_equal(prev, &snapshot))
    {
        pending.clear();
        return;
    }
    pending.set(snapshot);
}

/// Merge a focus-blind frame readback (`windows`) with the last known focus:
/// the focus hint survives only while its window is still observed
/// (authoritative loss otherwise). Never invents focus.
#[must_use]
pub fn merge_readback(windows: Vec<ObservedWindow>, focus_hint: Option<u32>) -> BackendSnapshot {
    let focused_wid = match focus_hint {
        Some(wid) if windows.iter().any(|w| w.wid == wid) => Some(wid),
        _ => None,
    };
    BackendSnapshot {
        windows,
        focused_wid,
    }
}

/// Latest-wins buffer for one unsent observation. `set` overwrites any
/// retained snapshot (coalescing); `take` drains for a send attempt. A full
/// inbox keeps the buffer (retry), it never drops the final state.
#[derive(Debug, Default)]
pub struct PendingObservation {
    latest: Option<BackendSnapshot>,
}

impl PendingObservation {
    pub fn new() -> Self {
        Self { latest: None }
    }

    /// Stage the newest snapshot, overwriting any retained one. Returns true
    /// when a retained snapshot was coalesced away.
    pub fn set(&mut self, snapshot: BackendSnapshot) -> bool {
        let coalesced = self.latest.is_some();
        self.latest = Some(snapshot);
        coalesced
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn peek(&self) -> Option<&BackendSnapshot> {
        self.latest.as_ref()
    }

    pub fn take(&mut self) -> Option<BackendSnapshot> {
        self.latest.take()
    }

    /// Drop any retained snapshot: the newest truth already matches the
    /// last sent state, so the retained older snapshot is stale transient.
    pub fn clear(&mut self) {
        self.latest = None;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.latest.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(wid: u32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            floating: false,
            fullscreen: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        }
    }

    #[test]
    fn equal_snapshots_skip_republish() {
        let a = BackendSnapshot {
            windows: vec![win(1)],
            focused_wid: Some(1),
        };
        let b = a.clone();
        assert!(snapshots_equal(&a, &b));
        let moved = BackendSnapshot {
            windows: vec![win(2)],
            focused_wid: Some(1),
        };
        assert!(!snapshots_equal(&a, &moved));
        let focus_lost = BackendSnapshot {
            windows: vec![win(1)],
            focused_wid: None,
        };
        assert!(!snapshots_equal(&a, &focus_lost));
    }

    #[test]
    fn pending_keeps_latest_only() {
        let mut pending = PendingObservation::new();
        assert!(pending.is_empty());
        assert!(!pending.set(BackendSnapshot {
            windows: vec![win(1)],
            focused_wid: None,
        }));
        assert!(pending.set(BackendSnapshot {
            windows: vec![win(2)],
            focused_wid: None,
        }));
        let taken = pending.take().expect("retained");
        assert_eq!(taken.windows, vec![win(2)]);
        assert!(pending.is_empty());
    }

    #[test]
    fn readback_preserves_focus_only_while_present() {
        let merged = merge_readback(vec![win(1), win(2)], Some(2));
        assert_eq!(merged.focused_wid, Some(2));
        let lost = merge_readback(vec![win(1)], Some(2));
        assert_eq!(lost.focused_wid, None);
    }

    /// Causal defect sequence: B sent, A staged while the inbox was full,
    /// then the newest truth returns to B. The stale pending A must be
    /// cleared (never resurrected after B), while a newer C replaces A.
    /// Staging never advances any sequence (send-side only).
    #[test]
    fn stage_latest_clears_stale_pending_on_return_to_sent() {
        let snap = |wid| BackendSnapshot {
            windows: vec![win(wid)],
            focused_wid: None,
        };
        let last = Some(snap(2));
        let mut pending = PendingObservation::new();
        // A staged while B was the last sent state.
        stage_latest(&last, &mut pending, snap(1));
        assert_eq!(pending.peek(), Some(&snap(1)));
        // Newest truth returns to sent B: stale A is gone, nothing staged.
        stage_latest(&last, &mut pending, snap(2));
        assert!(pending.peek().is_none());
        // A newer C replaces a retained A instead of keeping both.
        stage_latest(&last, &mut pending, snap(1));
        stage_latest(&last, &mut pending, snap(3));
        assert_eq!(pending.peek(), Some(&snap(3)));
        // With no sent state yet, everything stages (startup path).
        let mut fresh = PendingObservation::new();
        stage_latest(&None, &mut fresh, snap(1));
        assert_eq!(fresh.peek(), Some(&snap(1)));
    }

    fn scoped(wid: u32, spaces: &[u64], revision: u64) -> ScopedBackendSnapshot {
        ScopedBackendSnapshot {
            windows: vec![win(wid)],
            focused_wid: None,
            focus_cg_owned: false,
            focus_evidence: None,
            sys_focus: None,
            global_focus: None,
            meta: vec![crate::runtime_types::ScopedWindowMeta {
                wid,
                spaces: spaces.to_vec(),
                fullscreen: false,
                minimized: false,
                hidden: false,
            }],
            excluded: 0,
            revision,
        }
    }

    #[test]
    fn scoped_equality_covers_membership_omissions_and_revision() {
        let a = scoped(1, &[11], 9);
        assert!(scoped_snapshots_equal(&a, &a.clone()));
        // Same geometry, different home Space: must republish (the engine
        // would otherwise keep planning the stale domain).
        assert!(!scoped_snapshots_equal(&a, &scoped(1, &[12], 9)));
        // Same geometry and home, new revision: must republish (drift
        // supersedes plans; equality would swallow the fence).
        assert!(!scoped_snapshots_equal(&a, &scoped(1, &[11], 10)));
        // Moved geometry republishes too.
        let mut moved = a.clone();
        moved.windows[0].x = 5;
        assert!(!scoped_snapshots_equal(&a, &moved));
    }

    #[test]
    fn scoped_equality_ignores_fresh_evidence_stamps() {
        // Evidence stamps at every send; including it in equality would
        // republish on every evidence refresh (busy feedback storm).
        let mut a = scoped(1, &[11], 9);
        let mut b = a.clone();
        a.focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(1001),
            frontmost_bundle: None,
            ax_focus: Some((1001, Some(1))),
        });
        b.focus_evidence = None;
        assert!(scoped_snapshots_equal(&a, &b));
    }

    #[test]
    fn scoped_equality_tracks_owner_sys_focus() {
        // The selected-oracle leg is a real owner-thread read per snapshot:
        // a changed sys identity must republish (otherwise an app-local
        // focus of a background app preserves stale global focus forever),
        // while an identical sys stays quiet.
        let a = scoped(1, &[11], 9);
        let mut moved = a.clone();
        moved.sys_focus = Some((1002, Some(3)));
        assert!(!scoped_snapshots_equal(&a, &moved));
        let mut same = a.clone();
        same.sys_focus = None;
        assert!(scoped_snapshots_equal(&a, &same));
        let mut matched = a.clone();
        matched.sys_focus = Some((1001, Some(1)));
        let mut matched2 = matched.clone();
        assert!(scoped_snapshots_equal(&matched, &matched2));
        matched2.sys_focus = Some((1001, Some(2)));
        assert!(!scoped_snapshots_equal(&matched, &matched2));
    }
}
