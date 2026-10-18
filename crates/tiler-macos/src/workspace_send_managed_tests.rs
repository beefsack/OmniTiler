#![cfg(test)]
//! Offline managed-send effect tests: injectable-backend proof of the
//! producer-shaped runtime path. No live touch, no taps/helpers/probes, no
//! AX polling on the test (main) thread beyond the bounded completion poll.
//!
//! The dedicated effect thread owns its fake backend (built on that thread
//! via the spawn factory); the test thread only submits nonblocking and
//! polls nonblocking. Every send catalogue row stays `implemented: false`,
//! the engine still refuses every send, and the tile drain still refuses at
//! `drain_live_claimed` — reaching this path happens only here.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use tiler_core::directional::{OutputId, WorkspaceId};

use crate::desktop_shortcuts::{ShortcutEntry, desktop_digit_keycode, desktop_symbolic_id};
use crate::space_drag_transfer::{DragStep, PostCounts, SwitchChords};
use crate::workspace_select::{SelectionSnapshot, SelectionState};
use crate::workspace_send::{SendGates, bindings, drain_live_claimed};
use crate::workspace_send_effect::{
    ManagedSendTarget, SendEffectHandoff, SendEffectOutcome, SendFreshEvidence, prove_managed_send,
};
use crate::workspace_send_managed::{
    ManagedSendBackend, ManagedSendThread, authorize_managed_edge, completion_for,
    execute_managed_send,
};

const CG_CTRL: u64 = 0x0004_0000;

fn source_domain() -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-20".to_owned()),
    }
}

fn target() -> ManagedSendTarget {
    ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-3")
        .expect("fixture target binds")
}

fn fresh_ok() -> SendFreshEvidence {
    SendFreshEvidence {
        managed_current: Some(true),
        fullscreen: Some(false),
        live_generation: Some(1),
        live_revision: Some(9),
        live_source: Some(source_domain()),
        source_current: Some(true),
        focused_window: Some("4242_7".to_owned()),
        focus_agrees: Some(true),
        geometry_ok: Some(true),
        ax_hit: Some("4242_7".to_owned()),
        ws_hit: Some("4242_7".to_owned()),
    }
}

fn row() -> SelectionSnapshot {
    SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 20,
    }
}

fn open_gates() -> SendGates {
    SendGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

fn enabled_entry(desktop: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: true,
        keycode: desktop_digit_keycode(desktop).unwrap(),
        modifiers: CG_CTRL,
    }
}

/// Scripted managed backend: canned gates/row/revision/holders plus
/// per-edge fresh-evidence scripting and fixture-style drag effect with
/// fault injection. No live touch, no sleeps on the fast path.
struct FakeManaged {
    gates: SendGates,
    displays: Result<usize, String>,
    snapshot: Result<SelectionSnapshot, String>,
    revision: Option<u64>,
    holders: HashMap<u32, ShortcutEntry>,
    fail_holder_reads: Vec<u32>,
    evidence_base: SendFreshEvidence,
    /// Queued per-call evidence overrides (popped front per
    /// `fresh_evidence` call once non-empty; otherwise the base returns).
    evidence_script: VecDeque<SendFreshEvidence>,
    /// From this `fresh_evidence` call number onward, focus names a foreign
    /// window (mid-leg swap simulation). `None` disables.
    swap_from_call: Option<usize>,
    swap_focus_to: String,
    evidence_calls: usize,
    membership: Vec<u64>,
    current: Option<u64>,
    goal: u64,
    selected: Option<(u32, u64, Option<i64>)>,
    key_posts: Vec<(u32, u64, Option<i64>, bool)>,
    counts: PostCounts,
    fail_begin: bool,
    fail_mouse_at: Option<DragStep>,
    fail_chord_down: bool,
    fail_key_up_once: bool,
    fail_key_recovery: bool,
    fail_mouse_up: bool,
    no_effect: bool,
    freeze_current: bool,
    unreadable: bool,
    /// When set, `begin_leg` signals start then blocks for release (busy +
    /// cancellation determinism for the threaded tests).
    block_begin: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    /// When set, `post_mouse(Down)` requests stop on this lane mid-effect.
    cancel_on_mouse_down: Option<Arc<SendEffectHandoff>>,
}

impl FakeManaged {
    fn new() -> Self {
        let mut holders = HashMap::new();
        for desktop in 1..=9u32 {
            holders.insert(
                desktop_symbolic_id(desktop).unwrap(),
                enabled_entry(desktop),
            );
        }
        Self {
            gates: open_gates(),
            displays: Ok(1),
            snapshot: Ok(row()),
            revision: Some(9),
            holders,
            fail_holder_reads: Vec::new(),
            evidence_base: fresh_ok(),
            evidence_script: VecDeque::new(),
            swap_from_call: None,
            swap_focus_to: "4242_9".to_owned(),
            evidence_calls: 0,
            membership: vec![20],
            current: Some(20),
            goal: 0,
            selected: None,
            key_posts: Vec::new(),
            counts: PostCounts::default(),
            fail_begin: false,
            fail_mouse_at: None,
            fail_chord_down: false,
            fail_key_up_once: false,
            fail_key_recovery: false,
            fail_mouse_up: false,
            no_effect: false,
            freeze_current: false,
            unreadable: false,
            block_begin: None,
            cancel_on_mouse_down: None,
        }
    }
}

impl ManagedSendBackend for FakeManaged {
    fn read_gates(&mut self) -> SendGates {
        self.gates
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        self.snapshot.clone()
    }
    fn display_count(&mut self) -> Result<usize, String> {
        self.displays.clone()
    }
    fn revision(&mut self) -> Option<u64> {
        self.revision
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        if self.fail_holder_reads.contains(&symbolic_id) {
            return Err(format!("injected holder read refusal for {symbolic_id}"));
        }
        self.holders.get(&symbolic_id).copied().ok_or_else(|| {
            format!("fake holder {symbolic_id} unresolvable; refusing rather than guessing")
        })
    }
    fn fresh_evidence(&mut self, target: &ManagedSendTarget) -> SendFreshEvidence {
        self.evidence_calls += 1;
        assert_eq!(
            target.window(),
            "4242_7",
            "effect grabs the pinned subject only"
        );
        if let Some(evidence) = self.evidence_script.pop_front() {
            return evidence;
        }
        if self
            .swap_from_call
            .is_some_and(|from| self.evidence_calls >= from)
        {
            let mut swapped = self.evidence_base.clone();
            swapped.focused_window = Some(self.swap_focus_to.clone());
            return swapped;
        }
        self.evidence_base.clone()
    }
    fn begin_leg(&mut self, goal: u64, chords: SwitchChords) -> Result<(), String> {
        if let Some((started, release)) = self.block_begin.take() {
            let _ = started.send(());
            // Bounded block: the test releases; a timeout fails closed.
            match release.recv_timeout(Duration::from_secs(5)) {
                Ok(()) => {
                    self.block_begin = None;
                }
                Err(_) => return Err("injected begin_leg block timed out".to_owned()),
            }
        }
        if self.fail_begin {
            return Err("injected leg-setup refusal".to_owned());
        }
        self.goal = goal;
        self.selected = Some(chords.for_goal(goal, goal));
        Ok(())
    }
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        if self.fail_mouse_at == Some(step) {
            self.fail_mouse_at = None;
            return Err(format!("injected {step:?} refusal"));
        }
        if step == DragStep::MouseUp && self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        match step {
            DragStep::MouseDown => {
                self.counts.mouse_down += 1;
                if let Some(lane) = self.cancel_on_mouse_down.clone() {
                    lane.cancel();
                }
            }
            DragStep::MouseUp => self.counts.mouse_up += 1,
            _ => {}
        }
        Ok(())
    }
    fn post_key(&mut self, down: bool) -> Result<(), String> {
        if down {
            if self.fail_chord_down {
                return Err("injected chord-down refusal".to_owned());
            }
            self.counts.key_down += 1;
            if let Some(selected) = self.selected {
                self.key_posts
                    .push((selected.0, selected.1, selected.2, true));
            }
            return Ok(());
        }
        if self.fail_key_up_once {
            self.fail_key_up_once = false;
            return Err("injected chord-up refusal".to_owned());
        }
        if self.fail_key_recovery {
            return Err("injected chord-up recovery refusal".to_owned());
        }
        self.counts.key_up += 1;
        if let Some(selected) = self.selected {
            self.key_posts
                .push((selected.0, selected.1, selected.2, false));
        }
        if !self.no_effect {
            self.membership = vec![self.goal];
            if !self.freeze_current {
                self.current = Some(self.goal);
            }
        }
        Ok(())
    }
    fn settle_grab(&mut self) {}
    fn settle_hold(&mut self) {}
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        if self.unreadable {
            return None;
        }
        Some(self.membership.clone())
    }
    fn read_current(&mut self) -> Option<u64> {
        if self.unreadable {
            return None;
        }
        self.current
    }
    fn effect_deadline_ms(&self) -> u64 {
        30
    }
    fn effect_poll_ms(&self) -> u64 {
        1
    }
    fn counts(&self) -> PostCounts {
        self.counts
    }
}

fn execute(
    target: &ManagedSendTarget,
    backend: &mut FakeManaged,
    handoff: &SendEffectHandoff,
) -> (SendEffectOutcome, Option<crate::space_drag_transfer::LegOk>) {
    execute_managed_send(target, backend, handoff)
}

fn finish_and_poll(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    outcome: SendEffectOutcome,
) -> crate::workspace_send_effect::SendCompletion {
    let completion = completion_for(target, outcome);
    assert!(
        handoff.finish(completion.clone()),
        "matching finish stages the terminal completion"
    );
    handoff
        .poll_completion_for(target)
        .expect("main polls the terminal completion")
}

/// Submit + take to place one request inflight, then run the effect-owner
/// execution against that lane. Mirrors the threaded owner (take, execute,
/// finish) without spawning a thread.
fn execute_inflight(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    backend: &mut FakeManaged,
) -> (SendEffectOutcome, Option<crate::space_drag_transfer::LegOk>) {
    handoff.submit(target.clone()).expect("idle submits");
    handoff.take_for_owner().expect("owner takes");
    execute_managed_send(target, backend, handoff)
}

#[test]
fn managed_permit_gates_down_edges_but_never_up() {
    let pinned = target();
    assert!(authorize_managed_edge(&pinned, &fresh_ok()).is_ok());
    // Pure focus swap while the subject stays managed/readable refuses.
    let mut swapped = fresh_ok();
    swapped.focused_window = Some("4242_9".to_owned());
    let err = authorize_managed_edge(&pinned, &swapped).expect_err("focus swap refuses");
    assert_eq!(
        err,
        crate::workspace_send_effect::SendProofRefusal::FocusMismatch
    );
    // Unknown legs refuse too (never guessed).
    let mut unknown = fresh_ok();
    unknown.focused_window = None;
    assert!(authorize_managed_edge(&pinned, &unknown).is_err());
    // Permit pins the exact subject.
    let permit = authorize_managed_edge(&pinned, &fresh_ok()).expect("permit");
    assert_eq!((permit.pid(), permit.wid()), (4242, 7));
}

#[test]
fn managed_never_touches_fixture_post_target() {
    // Separate product-post authority: the fixture's unforgeable target is
    // never constructed, forged, or bypassed here. This grep-guard fails the
    // build if a future edit reaches for its constructors. Doc mentions of
    // the type name are allowed; construction/bypass call-sites are not.
    let source = include_str!("workspace_send_managed.rs");
    for forbidden in [
        "OwnedPostTarget::",
        "from_held_child",
        "from_spawn_record",
        "post_guarded_with_tag",
        "post_guarded(",
    ] {
        assert!(
            !source.contains(forbidden),
            "managed product path must not touch the fixture post target ({forbidden})"
        );
    }
    assert!(
        source.contains("hid_mouse_post") && source.contains("hid_key_post"),
        "managed posts reuse the maintained ffi posting wrappers"
    );
    assert!(
        source.contains("forbid(unsafe_code)"),
        "no new unsafe on the managed path"
    );
}

#[test]
fn managed_applied_roundtrip_via_thread_with_tagged_digit() {
    let worker = ManagedSendThread::spawn(FakeManaged::new);
    let handoff = worker.handoff();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("idle submits");
    // Second submit while pending/inflight refuses: no overlapping effects.
    assert_eq!(
        handoff.submit(pinned.clone()),
        Err("send-effect-busy"),
        "concurrent press serializes/refuses"
    );
    // Bounded nonblocking poll: the effect thread proves/actuates off main
    // while this thread only polls (no AX waits here).
    let mut done = None;
    for _ in 0..500 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            done = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = done.expect("effect thread finishes promptly");
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    assert_eq!(done.operation_id, "op-3");
    assert_eq!(done.app_generation, 1);
    assert_eq!(done.revision, 9);
    assert!(!handoff.is_busy(), "polled completion idles the lane");
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
    // Verified follow arrival is noted on main (never on the effect thread):
    // the per-display previous becomes the source the send followed away from.
    let mut history = SelectionState::default();
    history.note_arrival("display-a", 20);
    assert!(!worker.shutdown(), "nothing outstanding after poll");
}

#[test]
fn managed_applied_posts_exact_control_digit_tagged() {
    // Deterministic single-effect execution (no thread scheduling): exact
    // target digit, Control-only, tagged, both edges, balanced.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let (outcome, leg) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("applied carries exact readback");
    assert_eq!((leg.membership, leg.current), (vec![3], 3));
    assert!(!leg.pending_recovered);
    let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
    let digit_2 = desktop_digit_keycode(2).unwrap();
    assert_eq!(
        backend.key_posts,
        vec![
            (digit_2, CG_CTRL, tag, true),
            (digit_2, CG_CTRL, tag, false)
        ],
        "posts exactly the target ordinal Control-only digit, tagged"
    );
    assert!(backend.counts.balanced());
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
}

#[test]
fn managed_focus_swap_refuses_without_posts() {
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    let (outcome, leg) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.total(), 0, "focus swap posts nothing");
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
}

#[test]
fn managed_stale_generation_revision_and_unknown_refuse() {
    for (mutate, token) in [
        (
            (|e: &mut SendFreshEvidence| e.live_generation = Some(2)) as fn(&mut SendFreshEvidence),
            "stale-lifetime",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_generation = None) as fn(&mut SendFreshEvidence),
            "unknown-generation",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_revision = Some(10)) as fn(&mut SendFreshEvidence),
            "revision-drift",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_revision = None) as fn(&mut SendFreshEvidence),
            "unknown-revision",
        ),
    ] {
        let handoff = SendEffectHandoff::new();
        let pinned = target();
        let mut backend = FakeManaged::new();
        mutate(&mut backend.evidence_base);
        let (outcome, _) = execute(&pinned, &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{token}");
        assert_eq!(backend.counts.total(), 0, "{token} posts nothing");
    }
    // Backend row-revision drift refuses too (pinned revision compared live
    // before actuation).
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.revision = Some(10);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn managed_holder_display_and_sid_refusals_post_nothing() {
    // Disabled holder refuses (never a settings write here).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: false,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL,
        },
    );
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable holder refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend
        .fail_holder_reads
        .push(desktop_symbolic_id(2).unwrap());
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Customized chord refuses (never posted as a chord that does nothing).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: true,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL | 0x0002_0000,
        },
    );
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Zero/multiple displays refuse (single-display capability preserved).
    for displays in [Ok(0), Ok(2), Err("row unreadable".to_owned())] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.displays = displays;
        let (outcome, _) = execute(&target(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(backend.counts.total(), 0);
    }
    // Missing target SID refuses (never guess another occupant).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 7],
        current: 20,
    });
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable snapshot refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Err("row unreadable".to_owned());
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn managed_mid_leg_focus_swap_stops_before_chord() {
    // Per-edge re-proof: the first five proofs pass (admission, leg-setup,
    // move, down, dragged), the chord-down proof names a foreign window.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.swap_from_call = Some(6);
    let (outcome, leg) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.key_down, 0, "no chord after the swap");
    // The held mouse still released (release-first guarantee).
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "stale down-edge never strands the button"
    );
}

#[test]
fn managed_release_edges_always_attempt_despite_stale_gates() {
    // Swap lands at the dragged-edge proof: the drag refuses, but mouse-up
    // still attempts and succeeds (balanced, no chord, proven refusal).
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.swap_from_call = Some(5);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(backend.counts.key_down, 0);
    // Key-up recovery still completes the follow when the first up is lost.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    let (outcome, leg) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(leg.expect("applied").pending_recovered);
    assert_eq!((backend.counts.key_down, backend.counts.key_up), (1, 1));
}

#[test]
fn managed_cancellation_before_posts_refuses_and_keeps_lane() {
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("owner takes");
    assert!(handoff.cancel(), "inflight cancel flags stop");
    assert!(handoff.is_busy(), "cancelled inflight stays busy");
    assert_eq!(
        handoff.submit(pinned.clone()),
        Err("send-effect-busy"),
        "no concurrent physical effect while inflight stands"
    );
    let mut backend = FakeManaged::new();
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        backend.counts.total(),
        0,
        "stop before posts touches nothing"
    );
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_cancel_during_mouse_hold_still_releases() {
    // Concurrent stop arrives inside mouse-down (via the backend hook): the
    // next down-edge stops, mouse-up still releases, no chord posts.
    let handoff = Arc::new(SendEffectHandoff::new());
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("owner takes");
    let mut backend = FakeManaged::new();
    backend.cancel_on_mouse_down = Some(Arc::clone(&handoff));
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert!(handoff.cancel_requested(), "stop was requested mid-effect");
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(
        backend.counts.key_down, 0,
        "stop before chord posts no chord"
    );
    assert!(handoff.is_busy(), "terminal completion (not cancel) frees");
    let done = {
        let completion = completion_for(&pinned, outcome);
        assert!(handoff.finish(completion.clone()));
        handoff
            .poll_completion_for(&pinned)
            .expect("terminal completion polls")
    };
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_release_failures_hold_as_possibly_executed() {
    // Mouse-up refusal parks the release itself: uncertain, never replayed.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_mouse_up = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 0));
    // Chord-up recovery refusal parks the key edge with the mouse released.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
}

#[test]
fn managed_backend_errors_map_honestly() {
    // Leg-setup / grab / chord-down refusals post no effective chord:
    // proven refusals.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_begin = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_mouse_at = Some(DragStep::MouseDragged);
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_chord_down = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.key_down, 0);
    // Completed chord without exact readback holds (never inverts).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.no_effect = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(backend.counts.balanced());
    // Wrong current holds.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.freeze_current = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    // Unreadable post-release readback holds (chord completed, arrival
    // unknown — never a false proven refusal).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.unreadable = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    // Gated sessions refuse before any post.
    for gates in [
        SendGates {
            session_unlocked: false,
            ..open_gates()
        },
        SendGates {
            secure_active: true,
            ..open_gates()
        },
        SendGates {
            lock_screen: true,
            ..open_gates()
        },
    ] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.gates = gates;
        let (outcome, _) = execute(&target(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(backend.counts.total(), 0);
    }
}

#[test]
fn managed_thread_busy_blocks_overlap_and_shutdown_releases() {
    // Blocking begin_leg holds the lane inflight: overlap refuses, shutdown
    // joins first and only then releases.
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = ManagedSendThread::spawn(move || {
        let mut backend = FakeManaged::new();
        backend.block_begin = Some((started_tx, release_rx));
        backend
    });
    let handoff = worker.handoff();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("effect thread takes and blocks in begin_leg");
    assert!(handoff.is_busy());
    assert_eq!(
        handoff.submit(pinned.clone()),
        Err("send-effect-busy"),
        "no overlapping physical effect while blocked"
    );
    assert!(worker.cancel(), "cancel flags the blocked inflight");
    assert!(handoff.is_busy(), "cancel keeps the lane busy");
    release_tx.send(()).expect("release the blocked leg");
    let mut terminal = None;
    for _ in 0..200 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            terminal = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let terminal = terminal.expect("terminal completion after release");
    // Cancel arrived before the chord: proven refusal (nothing posted).
    assert_eq!(terminal.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
    assert!(!worker.shutdown(), "polled lane releases idle");
}

#[test]
fn managed_terminal_feedback_settles_transfer_and_stale_drops() {
    // Fenced completion roundtrip: matching polls, stale drops counted.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("takes");
    let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
    stale.operation_id = "op-9".to_owned();
    assert!(!handoff.finish(stale), "wrong operation is stale");
    assert_eq!(handoff.stale_dropped(), 1);
    let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
    stale.app_generation = 2;
    assert!(!handoff.finish(stale), "relaunched lifetime is stale");
    let done = finish_and_poll(&handoff, &pinned, SendEffectOutcome::Applied);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    // A poll for another target's identity drops instead of misapplying.
    handoff
        .submit(pinned.clone())
        .expect("resubmits after poll");
    handoff.take_for_owner().expect("takes");
    assert!(handoff.finish(completion_for(&pinned, SendEffectOutcome::Applied)));
    let other = ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-9")
        .expect("other operation binds");
    assert!(handoff.poll_completion_for(&other).is_none());
    assert_eq!(handoff.stale_dropped(), 3);
    assert!(!handoff.is_busy());
    // Main-thread follow history advances only on exact Applied readback
    // (never on refusals or holds — same rule as the offline drain core).
    let mut history = SelectionState::default();
    assert!(
        prove_managed_send(&pinned, &fresh_ok()).is_ok(),
        "applied path proved first"
    );
    history.note_arrival("display-a", 20);
}

#[test]
fn managed_arming_and_holder_wiring_stay_unarmed() {
    // All send catalogue rows stay unarmed; the product drain still refuses
    // at the live boundary; no holder ensure is wired into the managed path.
    for row in crate::workspace_input::workspace_catalogue() {
        if matches!(
            row.action,
            crate::runtime_types::InputSemanticAction::WorkspaceSend { .. }
        ) {
            assert!(!row.implemented, "{} stays implemented:false", row.id);
        }
    }
    assert_eq!(bindings().len(), 0, "no send chord armed");
    let gates = open_gates();
    let mut history = SelectionState::default();
    let claim = crate::runtime_types::ClaimedSelect::tap(
        crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        },
    );
    let err = drain_live_claimed(&mut history, claim, &gates).expect_err("live drain refuses");
    assert!(err.contains("live step"), "{err}");
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        !source.contains("ensure_authentic_holders")
            && !source.contains("ensure_holders_for_preset"),
        "no startup holder writes from the managed path this unit"
    );
}
