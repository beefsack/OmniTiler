#![cfg(test)]
//! Offline managed-send effect tests: injectable-backend proof of the
//! producer-shaped runtime path. No live touch, no taps/helpers/probes, no
//! AX polling on the test (main) thread beyond the bounded completion poll.
//!
//! The dedicated effect thread owns its fake backend (built on that thread
//! via the spawn factory); the test thread only submits nonblocking and
//! polls nonblocking. Numbered send-follow 0..9 is product-armed;
//! trailing send-0 arms through the production capability gate (explicit
//! locals still pin the disabled lane in these tests), numbered stay 1..9,
//! relative sends, and outputs stay refused.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use tiler_core::directional::{OutputId, WorkspaceId};
use tiler_core::world_report::FrameSetter;

use crate::desktop_shortcuts::{
    ShortcutEntry, desktop_digit_keycode, desktop_symbolic_id, resolve_numbered_chord,
};
use crate::space_drag_transfer::{DragStep, PostCounts};
use crate::workspace_select::SelectionSnapshot;
use crate::workspace_send::{SendGates, bindings};
use crate::workspace_send_effect::{
    BlindRoot, ManagedSendTarget, SendEffectHandoff, SendEffectOutcome, SendFreshEvidence,
    SendProofRefusal, TrailingPrepPins, TrailingPrepProof, TrailingSendRequest, prove_managed_send,
};
use crate::workspace_send_managed::{
    EFFECT_REFUSAL_DETAIL_CHARS, ManagedPostPermit, ManagedSendBackend, ManagedSendThread,
    authorize_managed_edge, classify_holder_chord_error, completion_for, execute_managed_send,
    execute_managed_send_stay, execute_trailing_send, format_effect_refusal_line,
    format_proof_refusal_reason, sanitize_refusal_detail,
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
        minimized: Some(false),
        live_generation: Some(1),
        live_revision: Some(9),
        live_source: Some(source_domain()),
        source_current: Some(true),
        focused_window: Some("4242_7".to_owned()),
        focus_agrees: Some(true),
        geometry_ok: Some(true),
        grab_point: Some((100.0, 200.0)),
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

/// Scripted managed backend: canned gates/row/holders plus
/// per-edge fresh-evidence scripting and fixture-style drag effect with
/// fault injection. No live touch, no sleeps on the fast path.
struct FakeManaged {
    gates: SendGates,
    displays: Result<usize, String>,
    snapshot: Result<SelectionSnapshot, String>,
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
    last_grab: Option<(f64, f64)>,
    membership: Vec<u64>,
    current: Option<u64>,
    goal: u64,
    selected_chord: Option<(u32, u64)>,
    mouse_points: Vec<(f64, f64)>,
    key_chords: Vec<(u32, u64)>,
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
    /// Stay-return scripting: post-forward fresh-row overrides (popped only
    /// once the forward chord posted - the pre-forward pin and the forward
    /// leg's own snapshot both read the canned row - else the canned row
    /// returns); the `select_back` return actuation log plus its failure
    /// and wrong-current scripting; the stay return-leg focus read; the
    /// post-forward gates override; post-return readback failures; and a
    /// one-shot stop request fired on the first membership read (a cancel
    /// landing mid-settle, after the forward edges proved).
    snapshot_script: VecDeque<Result<SelectionSnapshot, String>>,
    select_back_calls: Vec<u64>,
    select_back_fail: Option<String>,
    select_back_hold_current: bool,
    focused: Option<String>,
    gates_after_forward: Option<SendGates>,
    displays_after_forward: Option<Result<usize, String>>,
    fail_return_reads: bool,
    cancel_on_membership_read: Option<Arc<SendEffectHandoff>>,
    /// When set, `post_mouse(Down)` requests stop on this lane mid-effect.
    cancel_on_mouse_down: Option<Arc<SendEffectHandoff>>,
    /// When set, `prepare_trailing_target` requests stop on this lane
    /// mid-preparation (a cancel landing between the native ensure and the
    /// drag — the drag must never run, but a ran append still holds).
    cancel_on_prep: Option<Arc<SendEffectHandoff>>,
    /// When set, `prepare_trailing_target` signals start then blocks for
    /// release (busy + shutdown determinism for the threaded tests).
    block_prep: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    /// Trailing preparation scripting: the scripted
    /// [`ManagedSendBackend::prepare_trailing_target`] verdict plus its
    /// call count. `None` keeps the default honest refusal (numbered-only
    /// backend, never serves trailing).
    trailing_prep: Option<Result<TrailingPrepProof, crate::workspace_send::TrailingPrepFailure>>,
    prep_calls: usize,
    /// Fast bridged-route scripting: availability chosen once per op,
    /// consumed-permit dispatch log, destination-select log, and anchor
    /// drift injection. Defaults keep every existing regression on drag
    /// (`bridged_enabled=false`).
    bridged_enabled: bool,
    bridged_calls: Vec<u64>,
    bridged_fail: Option<String>,
    /// Set when a bridged dispatch reaches the scripted OS touch (after the
    /// pre-touch drift fence, including on an injected dispatch failure):
    /// proves the write was attempted, so the owner reports `Threw` instead
    /// of a pre-actuation refusal.
    bridged_touched: bool,
    bridged_drift_row: Option<SelectionSnapshot>,
    destination_calls: Vec<u64>,
    destination_fail: Option<String>,
    destination_hold_current: bool,
    use_route_calls: usize,
    /// Follow-leg focus read (target subject) vs stay return-leg source
    /// focus. The fast follow checks the target; the stay return checks
    /// the source. Separate fields keep both honest in one fake.
    focused_follow: Option<String>,
    /// Native command ordering log for the fast path: `bridged_move`
    /// pushes `bridged`, `select_destination` pushes `select`,
    /// `read_membership` pushes `membership`. Tests assert the selector
    /// starts before any post-dispatch membership poll.
    order: Vec<String>,
    /// Scripted settle membership reads (popped front per
    /// `read_membership` call while non-empty, before the canned
    /// `membership`). Models async completion landing late: script wrong
    /// values first, then the exact arrival.
    settle_script: VecDeque<Option<Vec<u64>>>,
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
            holders,
            fail_holder_reads: Vec::new(),
            evidence_base: fresh_ok(),
            evidence_script: VecDeque::new(),
            swap_from_call: None,
            swap_focus_to: "4242_9".to_owned(),
            evidence_calls: 0,
            last_grab: None,
            membership: vec![20],
            current: Some(20),
            goal: 0,
            selected_chord: None,
            mouse_points: Vec::new(),
            key_chords: Vec::new(),
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
            snapshot_script: VecDeque::new(),
            select_back_calls: Vec::new(),
            select_back_fail: None,
            select_back_hold_current: false,
            focused: None,
            gates_after_forward: None,
            displays_after_forward: None,
            fail_return_reads: false,
            cancel_on_membership_read: None,
            block_begin: None,
            cancel_on_mouse_down: None,
            cancel_on_prep: None,
            block_prep: None,
            trailing_prep: None,
            prep_calls: 0,
            bridged_enabled: false,
            bridged_calls: Vec::new(),
            bridged_fail: None,
            bridged_touched: false,
            bridged_drift_row: None,
            destination_calls: Vec::new(),
            destination_fail: None,
            destination_hold_current: false,
            use_route_calls: 0,
            focused_follow: Some("4242_7".to_owned()),
            order: Vec::new(),
            settle_script: VecDeque::new(),
        }
    }

    fn check_permit(&self, permit: &ManagedPostPermit) {
        assert_eq!((permit.pid(), permit.wid()), (4242, 7));
        assert_eq!(
            Some(permit.point()),
            self.last_grab,
            "post uses exactly the backend-selected fresh grab point"
        );
    }

    fn forwarded(&self) -> bool {
        // Forward actuation posted: the drag chord or exactly one fast
        // bridged dispatch plus destination select. Drives the stay
        // return-leg overrides (the live view then sits on the goal).
        self.counts.key_down > 0 || !self.bridged_calls.is_empty()
    }
}

impl ManagedSendBackend for FakeManaged {
    fn read_gates(&mut self) -> SendGates {
        // Post-forward override (the forward chord posted): the stay return
        // re-fences gates here, while the forward leg's own admission read
        // the canned gates.
        if self.forwarded()
            && let Some(gates) = self.gates_after_forward
        {
            return gates;
        }
        self.gates
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        // Post-forward override (same forwarded discriminator): the
        // pre-forward pin and the forward leg's own snapshot both read the
        // canned row; only the return-leg fresh row scripts drift.
        if self.forwarded()
            && let Some(next) = self.snapshot_script.pop_front()
        {
            return next;
        }
        let mut row = self.snapshot.clone()?;
        if self.forwarded() {
            // The forward leg posted, so the live view sits on the goal:
            // the return-leg row carries the arrival current.
            row.current = self.goal;
        }
        Ok(row)
    }
    fn display_count(&mut self) -> Result<usize, String> {
        // Post-forward override (same forwarded discriminator as gates):
        // the return leg re-fences the single-display rule here.
        if self.forwarded()
            && let Some(next) = self.displays_after_forward.clone()
        {
            return next;
        }
        self.displays.clone()
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
        let evidence = if let Some(evidence) = self.evidence_script.pop_front() {
            evidence
        } else if self
            .swap_from_call
            .is_some_and(|from| self.evidence_calls >= from)
        {
            let mut swapped = self.evidence_base.clone();
            swapped.focused_window = Some(self.swap_focus_to.clone());
            swapped
        } else {
            self.evidence_base.clone()
        };
        self.last_grab = evidence.grab_point;
        evidence
    }
    fn begin_leg(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String> {
        self.check_permit(&permit);
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
        self.selected_chord = Some((permit.keycode(), permit.flags()));
        self.mouse_points.push(permit.point());
        Ok(())
    }
    fn post_mouse(&mut self, permit: ManagedPostPermit, step: DragStep) -> Result<(), String> {
        match step {
            DragStep::MouseMove | DragStep::MouseDown | DragStep::MouseDragged => {}
            _ => {
                return Err(format!(
                    "refusing non-grab step {step:?} on the managed mouse post"
                ));
            }
        }
        self.check_permit(&permit);
        if self.fail_mouse_at == Some(step) {
            self.fail_mouse_at = None;
            return Err(format!("injected {step:?} refusal"));
        }
        assert_eq!(
            Some((permit.keycode(), permit.flags())),
            self.selected_chord,
            "every grab edge carries the exact resolved holder chord"
        );
        self.mouse_points.push(permit.point());
        if step == DragStep::MouseDown {
            self.counts.mouse_down += 1;
            if let Some(lane) = self.cancel_on_mouse_down.clone() {
                lane.cancel();
            }
        }
        Ok(())
    }
    fn post_key_down(&mut self, permit: ManagedPostPermit) -> Result<(), String> {
        self.check_permit(&permit);
        if self.fail_chord_down {
            return Err("injected chord-down refusal".to_owned());
        }
        assert_eq!(
            Some((permit.keycode(), permit.flags())),
            self.selected_chord,
            "chord-down carries the exact resolved holder chord"
        );
        self.counts.key_down += 1;
        let chord = (permit.keycode(), permit.flags());
        self.key_chords.push(chord);
        let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
        self.key_posts.push((chord.0, chord.1, tag, true));
        Ok(())
    }
    fn release_mouse(&mut self) -> Result<(), String> {
        if self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        self.counts.mouse_up += 1;
        Ok(())
    }
    fn release_key(&mut self) -> Result<(), String> {
        if self.fail_key_up_once {
            self.fail_key_up_once = false;
            return Err("injected chord-up refusal".to_owned());
        }
        if self.fail_key_recovery {
            return Err("injected chord-up recovery refusal".to_owned());
        }
        self.counts.key_up += 1;
        if let Some(selected) = self.selected_chord {
            let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
            self.key_posts.push((selected.0, selected.1, tag, false));
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
        // One-shot stop fired on the first membership read: models a cancel
        // landing mid-settle (after the forward edges proved, before the
        // return leg runs).
        if let Some(lane) = self.cancel_on_membership_read.take() {
            lane.cancel();
        }
        if self.unreadable {
            return None;
        }
        if self.fail_return_reads && !self.select_back_calls.is_empty() {
            return None;
        }
        // Fast-path ordering log plus late-completion scripting: scripted
        // settle reads win while non-empty, otherwise the canned value.
        if !self.bridged_calls.is_empty() || !self.destination_calls.is_empty() {
            self.order.push("membership".to_owned());
        }
        if let Some(next) = self.settle_script.pop_front() {
            return next;
        }
        Some(self.membership.clone())
    }
    fn read_current(&mut self) -> Option<u64> {
        if self.unreadable {
            return None;
        }
        if self.fail_return_reads && !self.select_back_calls.is_empty() {
            return None;
        }
        self.current
    }
    fn select_back(&mut self, source_sid: u64) -> Result<(), String> {
        self.select_back_calls.push(source_sid);
        if let Some(reason) = self.select_back_fail.clone() {
            return Err(reason);
        }
        // Ordinary return select: the mover stays on the target, the view
        // returns to the source (unless the test holds the wrong current).
        if !self.select_back_hold_current {
            self.current = Some(source_sid);
        }
        Ok(())
    }
    fn read_focused(&mut self) -> Option<String> {
        // Follow leg (no return select yet) reads the target subject;
        // the stay return leg reads the source fallback.
        if self.select_back_calls.is_empty() {
            self.focused_follow.clone()
        } else {
            self.focused.clone()
        }
    }
    fn prepare_trailing_target(
        &mut self,
        _pins: &TrailingPrepPins,
    ) -> Result<TrailingPrepProof, crate::workspace_send::TrailingPrepFailure> {
        use crate::workspace_send::TrailingPrepFailure;
        use tiler_core::world_report::FrameSetter;
        self.prep_calls += 1;
        if let Some(handoff) = self.cancel_on_prep.take() {
            handoff.cancel();
        }
        if let Some((started, release)) = self.block_prep.take() {
            let _ = started.send(());
            // Bounded block: the test releases; a timeout fails closed.
            if release.recv_timeout(Duration::from_secs(5)).is_err() {
                return Err(TrailingPrepFailure {
                    reason: "injected prep block timed out".to_owned(),
                    setter: FrameSetter::Threw,
                    before: None,
                    after: None,
                    dispatched: false,
                });
            }
        }
        self.trailing_prep.clone().unwrap_or_else(|| {
            Err(TrailingPrepFailure {
                reason: "trailing preparation unimplemented on this backend; refusing".to_owned(),
                setter: FrameSetter::Refused,
                before: None,
                after: None,
                dispatched: false,
            })
        })
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
    fn bridged_available(&mut self) -> bool {
        self.bridged_enabled
    }
    fn use_bridged_route(&mut self) {
        self.use_route_calls += 1;
    }
    fn bridged_dispatched(&self) -> bool {
        self.bridged_touched || !self.bridged_calls.is_empty()
    }
    fn bridged_move(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String> {
        // Consumed single-use permit plus fresh-anchor gate: the bound
        // point must equal the backend-selected grab, and the goal must
        // still sit in the fresh row (drift refuses pre-dispatch, before
        // any touch). An injected failure models the OS touch erroring:
        // the touch flag is set first, so the owner reports native
        // uncertainty instead of a pre-actuation refusal.
        self.check_permit(&permit);
        let row = self
            .bridged_drift_row
            .clone()
            .unwrap_or_else(|| self.snapshot.clone().expect("fake row readable"));
        if !row.spaces.contains(&goal) {
            return Err("fake anchor drift: target SID left the row mid-leg; refusing".to_owned());
        }
        self.bridged_touched = true;
        if let Some(reason) = self.bridged_fail.clone() {
            return Err(reason);
        }
        self.bridged_calls.push(goal);
        self.goal = goal;
        // Inactive-destination move: membership names the goal, the view
        // stays on the source until the destination select follows.
        self.membership = vec![goal];
        self.order.push("bridged".to_owned());
        Ok(())
    }
    fn select_destination(&mut self, sid: u64) -> Result<(), String> {
        self.destination_calls.push(sid);
        self.order.push("select".to_owned());
        if let Some(reason) = self.destination_fail.clone() {
            return Err(reason);
        }
        if !self.destination_hold_current {
            self.current = Some(sid);
        }
        Ok(())
    }
}

fn execute(
    target: &ManagedSendTarget,
    backend: &mut FakeManaged,
    handoff: &SendEffectHandoff,
) -> (
    SendEffectOutcome,
    Option<crate::space_drag_transfer::LegOk>,
    FrameSetter,
) {
    let effect = execute_managed_send(target, backend, handoff);
    (effect.outcome, effect.leg, effect.setter)
}

fn finish_and_poll(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    setter: FrameSetter,
    outcome: SendEffectOutcome,
) -> crate::workspace_send_effect::SendCompletion {
    let completion = completion_for(target, setter, outcome, None);
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
) -> (
    SendEffectOutcome,
    Option<crate::space_drag_transfer::LegOk>,
    FrameSetter,
) {
    handoff.submit(target.clone()).expect("idle submits");
    handoff.take_for_owner().expect("owner takes");
    let effect = execute_managed_send(target, backend, handoff);
    (effect.outcome, effect.leg, effect.setter)
}

#[test]
fn managed_permit_gates_down_edges_but_never_up() {
    let pinned = target();
    let chord = (50u32, CG_CTRL);
    assert!(authorize_managed_edge(&pinned, &fresh_ok(), chord).is_ok());
    // Pure focus swap while the subject stays managed/readable refuses.
    let mut swapped = fresh_ok();
    swapped.focused_window = Some("4242_9".to_owned());
    let err = authorize_managed_edge(&pinned, &swapped, chord).expect_err("focus swap refuses");
    assert_eq!(
        err,
        crate::workspace_send_effect::SendProofRefusal::FocusMismatch
    );
    // Unknown legs refuse too (never guessed).
    let mut unknown = fresh_ok();
    unknown.focused_window = None;
    assert!(authorize_managed_edge(&pinned, &unknown, chord).is_err());
    // Missing grab point refuses: no permit without the exact point proof.
    let mut no_point = fresh_ok();
    no_point.grab_point = None;
    assert!(authorize_managed_edge(&pinned, &no_point, chord).is_err());
    // Permit pins the exact subject, point, and chord for one edge.
    let permit = authorize_managed_edge(&pinned, &fresh_ok(), chord).expect("permit");
    assert_eq!((permit.pid(), permit.wid()), (4242, 7));
    assert_eq!(permit.point(), (100.0, 200.0));
    assert_eq!((permit.keycode(), permit.flags()), chord);
}

#[test]
fn managed_down_edges_route_exact_point_and_chord() {
    // Producer-shaped happy path: every down-edge posts at the
    // backend-selected fresh grab point with the exact resolved holder chord.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let (outcome, leg, setter) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert_eq!(setter, FrameSetter::Accepted);
    assert!(leg.is_some());
    let digit_2 = desktop_digit_keycode(2).unwrap();
    assert_eq!(backend.selected_chord, Some((digit_2, CG_CTRL)));
    assert_eq!(backend.key_chords, vec![(digit_2, CG_CTRL)]);
    assert!(
        !backend.mouse_points.is_empty()
            && backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "every grab edge uses the fresh backend-selected point: {:?}",
        backend.mouse_points
    );
    let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
    assert_eq!(
        backend.key_posts,
        vec![
            (digit_2, CG_CTRL, tag, true),
            (digit_2, CG_CTRL, tag, false)
        ]
    );
}

#[test]
fn managed_hit_disagreement_releases_without_chord() {
    // WS hit names a foreign window at the chord-down proof: uncertain only
    // if the chord already fired, proven here with zero key-downs.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut bad = fresh_ok();
    bad.ws_hit = Some("4242_9".to_owned());
    for _ in 0..4 {
        backend.evidence_script.push_back(fresh_ok());
    }
    backend.evidence_script.push_back(bad);
    let (outcome, _, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.key_down, 0, "disagreement posts no chord");
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "disagreement still releases the held button"
    );
}

#[test]
fn managed_grab_point_shift_before_hold_refuses_without_posts() {
    // Fresh point drifts at the grab-move proof (call 2), after leg-setup
    // pinned (100, 200): the move refuses before the button is held, so no
    // wrong-point post and no chord can follow.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut shifted = fresh_ok();
    shifted.grab_point = Some((300.0, 400.0));
    backend.evidence_script.push_back(fresh_ok());
    backend.evidence_script.push_back(shifted);
    let (outcome, _, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.mouse_down, 0, "drift pre-hold holds nothing");
    assert_eq!(backend.counts.key_down, 0, "drift pre-hold posts no chord");
    assert!(
        backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "shifted point never posts: {:?}",
        backend.mouse_points
    );
    assert!(backend.key_chords.is_empty());
}

#[test]
fn managed_grab_point_shift_after_hold_releases_without_chord() {
    // Fresh point drifts at the dragged proof (call 4): move and down already
    // posted at the pinned point, the dragged edge refuses, the held button
    // still releases, and no chord posts at either point.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut shifted = fresh_ok();
    shifted.grab_point = Some((300.0, 400.0));
    for _ in 0..3 {
        backend.evidence_script.push_back(fresh_ok());
    }
    backend.evidence_script.push_back(shifted);
    let (outcome, _, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "drift mid-hold still releases the held button"
    );
    assert_eq!(backend.counts.key_down, 0, "drift mid-hold posts no chord");
    assert!(
        backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "shifted point never posts: {:?}",
        backend.mouse_points
    );
    assert!(backend.key_chords.is_empty());
}

#[test]
fn managed_releases_are_unconditional_despite_stale_proof() {
    // Release edges never consult proof: even with a poisoned evidence base
    // the backend releases cleanly with no down-edge posts outstanding.
    let mut backend = FakeManaged::new();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    backend.evidence_base.grab_point = None;
    assert!(backend.release_mouse().is_ok());
    assert!(backend.release_key().is_ok());
    assert_eq!((backend.counts.mouse_up, backend.counts.key_up), (1, 1));
    assert_eq!((backend.counts.mouse_down, backend.counts.key_down), (0, 0));
}

#[test]
fn managed_permit_is_single_use_bound_authority() {
    // No discarded-permit ceremony may return: the permit is single-use
    // (no Clone/Copy), carries point+chord, posts consume it by value, and
    // no duplicate revision fence survives beside the fresh proof.
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        !source.contains("let _permit"),
        "down-edges must consume the permit, never discard it"
    );
    assert!(
        !source.contains("fn revision("),
        "no duplicate revision accessor beside the fresh proof fence"
    );
    assert!(
        !source.contains("SwitchChords"),
        "no free chord struct escapes the bound permit"
    );
    for forbidden in [
        "let _permit",
        "fn revision(",
        "SwitchChords",
        "post_guarded_with_tag",
    ] {
        assert!(
            !source.contains(forbidden),
            "retired free-arg/discard ceremony must stay gone ({forbidden})"
        );
    }
    assert!(
        source.contains("pub struct ManagedPostPermit")
            && source.contains("point: (f64, f64)")
            && source.contains("keycode: u32"),
        "permit binds the exact point and chord"
    );
    assert!(
        source.contains("post_key_down(&mut self, permit: ManagedPostPermit)")
            && source.contains("release_mouse(&mut self)")
            && source.contains("release_key(&mut self)"),
        "down-edges consume permits; releases stay permit-free"
    );
}

#[test]
fn managed_never_touches_fixture_post_target() {
    // Separate product-post authority: the fixture's unforgeable target is
    // never constructed, forged, or bypassed here. This grep-guard fails the
    // build if a future edit reaches for its constructors. Doc mentions of
    // the type name are allowed; construction/bypass call-sites are not.
    // Both send modules are scanned: the live backend lives in
    // `workspace_send_native`.
    let source = concat!(
        include_str!("workspace_send_managed.rs"),
        include_str!("workspace_send_native.rs")
    );
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
    let mut worker = ManagedSendThread::spawn(FakeManaged::new);
    let handoff = worker.handoff();
    let pinned = target();
    worker.submit(pinned.clone()).expect("idle submits");
    // Second submit while pending/inflight refuses: no overlapping effects.
    assert_eq!(
        worker.submit(pinned.clone()),
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
    // Core owns select history: no send-driven previous-history notes on
    // the select owner. The verified send arrival settles the window
    // transfer core-side only.
    // Idle shutdown is event-driven and nonblocking: request, then reap once
    // finished (the owner already idles on its wake channel).
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "nothing outstanding after poll");
}

#[test]
fn managed_applied_posts_exact_control_digit_tagged() {
    // Deterministic single-effect execution (no thread scheduling): exact
    // target digit, Control-only, tagged, both edges, balanced.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let (outcome, leg, setter) = execute_inflight(&handoff, &pinned, &mut backend);
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
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
}

#[test]
fn managed_focus_swap_refuses_without_posts() {
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    let (outcome, leg, setter) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.total(), 0, "focus swap posts nothing");
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(done.setter, FrameSetter::Refused);
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
        (
            (|e: &mut SendFreshEvidence| e.grab_point = None) as fn(&mut SendFreshEvidence),
            "unknown-grab-point",
        ),
    ] {
        let handoff = SendEffectHandoff::new();
        let pinned = target();
        let mut backend = FakeManaged::new();
        mutate(&mut backend.evidence_base);
        let (outcome, _, _) = execute(&pinned, &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{token}");
        assert_eq!(backend.counts.total(), 0, "{token} posts nothing");
    }
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
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable holder refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend
        .fail_holder_reads
        .push(desktop_symbolic_id(2).unwrap());
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
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
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Zero/multiple displays refuse (single-display capability preserved).
    for displays in [Ok(0), Ok(2), Err("row unreadable".to_owned())] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.displays = displays;
        let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
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
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable snapshot refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Err("row unreadable".to_owned());
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn managed_mid_leg_focus_swap_stops_before_chord() {
    // Per-edge re-proof: leg-setup, move, down and dragged pass, then the
    // chord-down proof names a foreign window.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.swap_from_call = Some(5);
    let (outcome, leg, _) = execute(&pinned, &mut backend, &handoff);
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
    backend.swap_from_call = Some(4);
    let (outcome, _, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(backend.counts.key_down, 0);
    // Key-up recovery still completes the follow when the first up is lost.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    let (outcome, leg, _) = execute(&pinned, &mut backend, &handoff);
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
    let (outcome, _, setter) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        backend.counts.total(),
        0,
        "stop before posts touches nothing"
    );
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(done.setter, FrameSetter::Refused);
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
    let (outcome, _, setter) = execute(&pinned, &mut backend, &handoff);
    assert!(handoff.cancel_requested(), "stop was requested mid-effect");
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(
        backend.counts.key_down, 0,
        "stop before chord posts no chord"
    );
    assert!(handoff.is_busy(), "terminal completion (not cancel) frees");
    let done = {
        let completion = completion_for(&pinned, setter, outcome, None);
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
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 0));
    // Chord-up recovery refusal parks the key edge with the mouse released.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
}

#[test]
fn managed_backend_errors_map_honestly() {
    // Leg-setup refusals post nothing: proven refusals.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_begin = true;
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(setter, FrameSetter::Refused);
    assert_eq!(backend.counts.total(), 0);
    // Native post-call failures throw instead of claiming a proven
    // pre-actuation refusal: the write was attempted and the OS refused it.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_mouse_at = Some(DragStep::MouseDragged);
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Threw);
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_chord_down = true;
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Threw);
    assert_eq!(backend.counts.key_down, 0);
    // Completed chord without exact readback holds with the write's
    // Accepted setter (never inverts, never a derived Threw).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.no_effect = true;
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Accepted);
    assert!(backend.counts.balanced());
    // Wrong current holds with the write's Accepted setter.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.freeze_current = true;
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Accepted);
    // Unreadable post-release readback holds with the chord's Accepted
    // setter (arrival unknown — never a false proven refusal, never a
    // derived Threw).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.unreadable = true;
    let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Accepted);
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
        let (outcome, _, setter) = execute(&target(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(setter, FrameSetter::Refused);
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
    worker.submit(pinned.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("effect thread takes and blocks in begin_leg");
    assert!(handoff.is_busy());
    assert_eq!(
        worker.submit(pinned.clone()),
        Err("send-effect-busy"),
        "no overlapping physical effect while blocked"
    );
    // try_reap while the owner still lives is nonblocking None: the caller
    // must never join-block nor release the lane from under a live owner.
    let mut worker = worker;
    assert!(
        worker.try_reap().is_none(),
        "reap while inflight is nonblocking None"
    );
    assert!(handoff.is_busy(), "lane stays busy while the owner lives");
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
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "polled lane releases idle");
}

#[test]
fn managed_thread_is_event_driven_no_idle_sleep() {
    // Events over polling: the owner blocks on a wake channel while idle.
    // No millisecond sleep may remain on the idle path.
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        source.contains("wake_rx.recv()"),
        "idle owner blocks on the event wake channel"
    );
    assert!(
        source.contains("wake_tx.send(())") || source.contains("wake_tx.send()"),
        "submit/cancel/shutdown wake the owner"
    );
    assert!(
        !source.contains("millis(1)"),
        "no 1ms idle poll sleep remains on the effect thread"
    );
    assert!(
        source.contains("request_shutdown") && source.contains("try_reap"),
        "nonblocking shutdown request plus finished-only reap"
    );
    assert!(
        source.contains("is_finished()"),
        "reap joins only once the owner already finished"
    );
}

#[test]
fn managed_idle_shutdown_request_then_reap_without_blocking() {
    // An idle owner shuts down promptly: request is nonblocking, reap
    // arrives once finished, and the released lane was idle.
    let mut worker = ManagedSendThread::spawn(FakeManaged::new);
    assert!(!worker.is_finished(), "owner starts live");
    worker.request_shutdown();
    // request_shutdown never blocks and never releases by itself.
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        released,
        Some(false),
        "idle lane releases with nothing outstanding"
    );
}

#[test]
fn managed_inflight_request_shutdown_never_releases_while_live() {
    // While the owner is blocked mid-effect, request_shutdown returns
    // immediately, try_reap stays None (no blocking join), and the lane
    // stays busy: the caller can never release from under a live owner.
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut worker = ManagedSendThread::spawn(move || {
        let mut backend = FakeManaged::new();
        backend.block_begin = Some((started_tx, release_rx));
        backend
    });
    let handoff = worker.handoff();
    let pinned = target();
    worker.submit(pinned.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("owner blocks mid-effect");
    worker.request_shutdown();
    assert!(
        worker.try_reap().is_none(),
        "reap while the owner lives is nonblocking None"
    );
    assert!(handoff.is_busy(), "no release while the owner is live");
    release_tx.send(()).expect("unblock the owner");
    // The owner finishes its terminal completion (cancelled before the
    // chord: proven refusal), then exits on the stop flag.
    let mut terminal = None;
    for _ in 0..500 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            terminal = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let terminal = terminal.expect("terminal completion after unblock");
    assert_eq!(terminal.outcome, SendEffectOutcome::RefusedProven);
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "staged completion was polled first");
}

#[test]
fn managed_second_operation_on_reused_backend_classifies_fresh() {
    // Regression: post counts are per-operation deltas. A first success must
    // not corrupt a second early proof refusal into PossiblyExecuted via
    // cumulative totals.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    // First operation: exact arrival.
    handoff.submit(pinned.clone()).expect("first submits");
    handoff.take_for_owner().expect("first takes");
    let crate::workspace_send_managed::SendEffect {
        outcome: first,
        leg,
        setter: first_setter,
    } = execute_managed_send(&pinned, &mut backend, &handoff);
    assert_eq!(first, SendEffectOutcome::Applied);
    assert!(leg.is_some());
    let first_posts = backend.counts.total();
    assert!(first_posts > 0, "first operation posts");
    assert!(handoff.finish(completion_for(&pinned, first_setter, first, None)));
    assert!(handoff.poll_completion_for(&pinned).is_some());
    // Second operation on the SAME backend: focus swaps before any new post.
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    handoff.submit(pinned.clone()).expect("second submits");
    handoff.take_for_owner().expect("second takes");
    let crate::workspace_send_managed::SendEffect {
        outcome: second,
        leg,
        setter: second_setter,
    } = execute_managed_send(&pinned, &mut backend, &handoff);
    assert_eq!(
        second,
        SendEffectOutcome::RefusedProven,
        "early proof refusal stays proven despite prior cumulative posts"
    );
    assert!(leg.is_none());
    assert_eq!(
        backend.counts.total(),
        first_posts,
        "second operation posts nothing new"
    );
    assert!(handoff.finish(completion_for(&pinned, second_setter, second, None)));
    let done = handoff
        .poll_completion_for(&pinned)
        .expect("second terminal polls");
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_terminal_feedback_settles_transfer_and_stale_drops() {
    // Fenced completion roundtrip: matching polls, stale drops counted.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("takes");
    let mut stale = completion_for(
        &pinned,
        FrameSetter::Accepted,
        SendEffectOutcome::Applied,
        None,
    );
    stale.operation_id = "op-9".to_owned();
    assert!(!handoff.finish(stale), "wrong operation is stale");
    assert_eq!(handoff.stale_dropped(), 1);
    let mut stale = completion_for(
        &pinned,
        FrameSetter::Accepted,
        SendEffectOutcome::Applied,
        None,
    );
    stale.app_generation = 2;
    assert!(!handoff.finish(stale), "relaunched lifetime is stale");
    let done = finish_and_poll(
        &handoff,
        &pinned,
        FrameSetter::Accepted,
        SendEffectOutcome::Applied,
    );
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    // A poll for another target's identity drops instead of misapplying.
    handoff
        .submit(pinned.clone())
        .expect("resubmits after poll");
    handoff.take_for_owner().expect("takes");
    assert!(handoff.finish(completion_for(
        &pinned,
        FrameSetter::Accepted,
        SendEffectOutcome::Applied,
        None
    )));
    let other = ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-9")
        .expect("other operation binds");
    assert!(handoff.poll_completion_for(&other).is_none());
    assert_eq!(handoff.stale_dropped(), 3);
    assert!(!handoff.is_busy());
    // Main-thread history is core-owned: no send-driven previous notes.
    // The applied path proves arrival; the window transfer settles
    // core-side only.
    assert!(
        prove_managed_send(&pinned, &fresh_ok()).is_ok(),
        "applied path proved first"
    );
}

#[test]
fn managed_arming_and_holder_wiring_armed_for_numbered_follow() {
    // Numbered sends 0..9 follow plus 1..9 stay are armed; trailing
    // stay-0, relative sends/stays, and outputs stay unarmed. The
    // async live controller still
    // refuses a claim without admission pins before any thread spawn or
    // post; no holder ensure is wired into the managed backend path
    // (startup owns it).
    for row in crate::workspace_input::workspace_catalogue() {
        match &row.action {
            crate::runtime_types::InputSemanticAction::WorkspaceSend { index, follow }
                if *follow || (1..=9).contains(index) =>
            {
                assert!(row.implemented, "{} armed", row.id);
            }
            crate::runtime_types::InputSemanticAction::WorkspaceSend { .. } => {
                assert!(!row.implemented, "{} stays implemented:false", row.id);
            }
            _ => {}
        }
    }
    assert_eq!(
        bindings().len(),
        10,
        "10 numbered send-follow chords armed (stays bindable unbound, no chord)"
    );
    let mut controller = crate::workspace_send::LiveSendController::new();
    let claim = crate::runtime_types::ClaimedSelect::tap(
        crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        },
    );
    let err = controller
        .submit(&claim, "display-a", 20, FakeManaged::new)
        .expect_err("unpinned claim refuses before any spawn");
    assert!(err.contains("unpinned"), "{err}");
    assert!(!controller.is_busy());
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        !source.contains("ensure_authentic_holders")
            && !source.contains("ensure_holders_for_preset"),
        "no startup holder writes from the managed path this unit"
    );
}

#[test]
fn trailing_send_drain_bound_target_submits_and_settles_through_the_same_lane() {
    use crate::runtime_types::{ClaimedSelect, InputSemanticAction};
    use crate::workspace_select::SelectionBackend;
    use crate::workspace_send::{
        SendDrainOutcome, bind_trailing_send_target, prepare_trailing_send_target,
        send_drain_outcome_for, should_note_history,
    };
    // Admission pins (engine side, explicit local capability): the ORIGINAL
    // window, lifetime, and source — NO target SID (the Space may not exist
    // yet); the admission topology revision rides for pre-preparation drift
    // detection.
    let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: true,
    });
    claim.send_subject = Some("4242_7".to_owned());
    claim.send_generation = Some(1);
    claim.send_source = Some(source_domain());
    claim.operation_id = Some("op-2".to_owned());
    claim.topology_revision = Some(4);
    assert!(
        ManagedSendTarget::from_claim(&claim).is_err(),
        "no admission SID was ever pinned for index 0"
    );
    // Drain preparation over a scripted selection backend (the same trait
    // the live drain proves through): producer-shaped `[1, 277]` with the
    // last proven empty reuses under the post-preparation revision.
    struct PrepFake {
        row: SelectionSnapshot,
    }
    impl SelectionBackend for PrepFake {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            Ok(self.row.clone())
        }
        fn revision(&mut self) -> Option<u64> {
            Some(9)
        }
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            if output == "macos-display-1" {
                Ok(())
            } else {
                Err(format!(
                    "product select refuses: reply target output {output} is not the live display macos-display-1; no actuation"
                ))
            }
        }
        fn empty(&mut self, _: u64) -> Result<bool, String> {
            Ok(true)
        }
        fn create(
            &mut self,
            _: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("proven-empty last reuses without an append");
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("preparation never selects");
        }
    }
    let mut prep_backend = PrepFake {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![1, 277],
            current: 1,
        },
    };
    let prep =
        prepare_trailing_send_target(&mut prep_backend, true, false).expect("reuse prepares");
    assert_eq!(prep.target_sid, 277);
    assert_eq!(prep.created, None);
    assert_eq!(prep.revision, 9);
    // The tile binds the prepared target to the admitted original pins and
    // re-pins the claim exactly like the drain (the own-append revision move
    // rides explicitly, never bypassed).
    let bound = bind_trailing_send_target(&claim, prep).expect("trailing binds");
    assert_eq!(bound.target_sid(), 277);
    assert!(bound.follow());
    claim.expected_sid = Some(bound.target_sid());
    claim.topology_revision = Some(bound.revision());
    // The same single-flight lane accepts the drain-bound claim.
    let mut controller = crate::workspace_send::LiveSendController::new();
    controller
        .submit(&claim, "display-a", 1, FakeManaged::new)
        .expect("drain-bound claim submits");
    assert!(controller.is_busy());
    // The same owner executes the bound target to Applied with exact
    // membership-plus-current readback (row names the prepared SID at the
    // Desktop-2 ordinal; the fake lands the drag there on release).
    let mut fake = FakeManaged::new();
    fake.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![1, 277],
        current: 1,
    });
    let handoff = SendEffectHandoff::new();
    let crate::workspace_send_managed::SendEffect { outcome, leg, .. } =
        execute_managed_send(&bound, &mut fake, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("applied carries readback");
    assert_eq!(leg.membership, vec![277]);
    assert_eq!(leg.current, 277);
    // The same settlement: an Applied follow notes history and settles the
    // transfer under the claim's operation identity.
    assert_eq!(send_drain_outcome_for(outcome), SendDrainOutcome::Applied);
    assert!(should_note_history(
        SendDrainOutcome::Applied,
        bound.follow()
    ));
    assert_eq!(bound.operation_id(), "op-2");
    // Reap the submitted owner (its scripted execution refuses on the
    // default row — target absent there — while the direct execution above
    // proves the prepared row applies; either terminal settle keeps the
    // single-flight lane honest).
    let _ = controller.shutdown();
}

#[test]
fn trailing_send_bound_target_refuses_focus_swap_without_actuation() {
    use crate::runtime_types::{ClaimedSelect, InputSemanticAction};
    use crate::workspace_send::TrailingSendPrep;
    use crate::workspace_send::bind_trailing_send_target;
    // A pure focus swap — the subject stays managed and readable while
    // focus names another window — refuses at the first edge with zero
    // posts, even for a drain-bound trailing target.
    let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: true,
    });
    claim.send_subject = Some("4242_7".to_owned());
    claim.send_generation = Some(1);
    claim.send_source = Some(source_domain());
    claim.operation_id = Some("op-2".to_owned());
    let bound = bind_trailing_send_target(
        &claim,
        TrailingSendPrep {
            target_sid: 277,
            created: None,
            revision: 9,
        },
    )
    .expect("trailing binds");
    let mut fake = FakeManaged::new();
    fake.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![1, 277],
        current: 1,
    });
    fake.swap_from_call = Some(1);
    let handoff = SendEffectHandoff::new();
    let crate::workspace_send_managed::SendEffect { outcome, leg, .. } =
        execute_managed_send(&bound, &mut fake, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(fake.counts.key_down, 0, "no chord posted before proof");
    assert_eq!(fake.counts.mouse_down, 0, "no grab posted before proof");
}

#[test]
fn effect_refusal_detail_is_bounded_single_line() {
    assert_eq!(sanitize_refusal_detail("", 200), "unspecified");
    assert_eq!(sanitize_refusal_detail("  a\nb\tc  ", 200), "a b c");
    assert_eq!(
        sanitize_refusal_detail("refused-send-focus-mismatch", 200),
        "refused-send-focus-mismatch"
    );
    let long = "x".repeat(500);
    let cut = sanitize_refusal_detail(&long, EFFECT_REFUSAL_DETAIL_CHARS);
    assert_eq!(cut.chars().count(), EFFECT_REFUSAL_DETAIL_CHARS);
    assert!(!cut.contains('\n'));
    // Vertical whitespace and control chars never survive, even mid-token.
    assert_eq!(sanitize_refusal_detail("a\r\nb\x07c", 200), "a b c");
}

#[test]
fn effect_refusal_holder_chord_tokens_carry_no_key_material() {
    let digit_2 = desktop_digit_keycode(2).unwrap();
    // Disabled holder: raw names the cause; token fixes it without values.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: false,
            keycode: digit_2,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("disabled holder refuses");
    assert!(raw.contains("disabled or unbound"));
    assert_eq!(classify_holder_chord_error(&raw), "holder-disabled");
    // Customized holder: raw echoes the entry debug (keycode/modifiers
    // numbers); the token carries none.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: true,
            keycode: digit_2,
            modifiers: CG_CTRL | 0x0002_0000,
        },
    )
    .expect_err("customized holder refuses");
    assert!(raw.contains("not the default"));
    assert!(raw.contains("keycode"), "raw leaks entry debug: {raw}");
    assert_eq!(classify_holder_chord_error(&raw), "holder-customized");
    // Unrepresentable keycode: raw echoes the number; token does not.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: true,
            keycode: 999,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("unrepresentable keycode refuses");
    assert!(raw.contains("out of range"));
    assert!(raw.contains("999"), "raw echoes the number: {raw}");
    assert_eq!(classify_holder_chord_error(&raw), "holder-unrepresentable");
    // Unmappable desktop: no digit chord exists past the catalogue.
    let raw = resolve_numbered_chord(
        11,
        ShortcutEntry {
            enabled: true,
            keycode: digit_2,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("desktop 11 has no digit chord");
    assert!(raw.contains("no default digit chord"));
    assert_eq!(classify_holder_chord_error(&raw), "holder-unmappable");
    assert_eq!(
        classify_holder_chord_error("some future resolver text"),
        "holder-chord-refused"
    );
    // No token carries digits, entry debug, or key names.
    for token in [
        "holder-disabled",
        "holder-unrepresentable",
        "holder-customized",
        "holder-unmappable",
        "holder-chord-refused",
    ] {
        assert!(
            !token.chars().any(|c| c.is_ascii_digit()),
            "{token} carries no numbers"
        );
    }
}

#[test]
fn effect_refusal_line_shape_names_stage_and_deltas() {
    let start = PostCounts {
        mouse_down: 2,
        mouse_up: 2,
        key_down: 1,
        key_up: 1,
    };
    let end = PostCounts {
        mouse_down: 3,
        mouse_up: 3,
        key_down: 1,
        key_up: 1,
    };
    let line = format_effect_refusal_line(
        277,
        "op-3",
        SendEffectOutcome::RefusedProven,
        "leg-setup",
        "send leg-setup managed proof refused: refused-send-focus-mismatch",
        start,
        end,
    );
    assert_eq!(
        line,
        "tiler-macos send effect refused target=277 op=op-3 stage=leg-setup \
         reason=send leg-setup managed proof refused: refused-send-focus-mismatch \
         dposts=md:1 mu:1 kd:0 ku:0"
    );
    // Long reasons truncate; the whole line stays bounded and single-line.
    let big = format_effect_refusal_line(
        277,
        "op-3",
        SendEffectOutcome::PossiblyExecuted,
        "readback",
        &"y".repeat(500),
        start,
        start,
    );
    assert!(!big.contains('\n'));
    assert!(
        big.chars().count() <= 420,
        "bounded line, got {} chars",
        big.chars().count()
    );
    assert!(big.starts_with("tiler-macos send effect possibly-executed target=277 "));
}

#[test]
fn effect_refusal_row_stage_refuses_without_posts() {
    // Current Space missing from the row: identity unprovable, zero posts.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 999,
    });
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Duplicate SIDs likewise refuse.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 3],
        current: 20,
    });
    let (outcome, _, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn effect_owner_emits_exactly_one_bounded_refusal_line_site() {
    let source = include_str!("workspace_send_managed.rs");
    assert_eq!(
        source.matches("eprintln!").count(),
        1,
        "single owner refusal emitter"
    );
    assert!(
        source.contains("eprintln!(\"{line}\");"),
        "the emitter prints only the formatted line"
    );
    // Every pre-actuation gate names its stage at the refusal site.
    for stage in [
        "\"cancel\"",
        "\"gates\"",
        "\"display\"",
        "\"snapshot\"",
        "\"row\"",
        "\"target\"",
        "\"holder\"",
        "\"holder-chord\"",
    ] {
        assert!(
            source.contains(stage),
            "early refusal names its stage {stage}"
        );
    }
    // The formatter renders counts only: no posted values, locations,
    // bindings, or window content may reach the line.
    let region = source
        .split("fn format_effect_refusal_line")
        .nth(1)
        .expect("formatter present")
        .split("fn log_effect_refusal")
        .next()
        .expect("formatter ends at the emitter");
    for forbidden in [
        "keycode",
        "flags",
        "permit",
        "entry",
        "point",
        "title",
        "chord",
        "ShortcutEntry",
        "modifiers",
    ] {
        assert!(
            !region.contains(forbidden),
            "refusal line formats counts only, never {forbidden}"
        );
    }
}

#[test]
fn proof_refusal_reason_names_blind_root_only_for_unknown_managed() {
    // Each blind root gets a distinct token-only suffix so one live
    // refusal line discriminates engine-facts-unmanaged vs
    // engine-facts-noreply vs topology vs membership.
    for (root, token) in [
        (BlindRoot::EngineFactsUnmanaged, "engine-facts-unmanaged"),
        (BlindRoot::EngineFactsNoReply, "engine-facts-noreply"),
        (BlindRoot::Topology, "topology"),
        (BlindRoot::Membership, "membership"),
    ] {
        let reason =
            format_proof_refusal_reason("leg-setup", SendProofRefusal::UnknownManaged, Some(root));
        assert!(
            reason.contains("refused-send-unknown-managed"),
            "keeps the proof token"
        );
        assert!(
            reason.contains(&format!("(blind-root={token})")),
            "names the {token} root"
        );
    }
    // A backend that never reports a root is labeled unreported — never
    // silently mislabeled as topology or membership.
    let reason = format_proof_refusal_reason("leg-setup", SendProofRefusal::UnknownManaged, None);
    assert!(reason.contains("(blind-root=unreported)"), "{reason}");
    // Any other refusal leg carries no blind suffix even when a stale
    // root stands.
    for refusal in [
        SendProofRefusal::NotManaged,
        SendProofRefusal::FocusMismatch,
        SendProofRefusal::RevisionDrift,
    ] {
        let reason = format_proof_refusal_reason("leg-setup", refusal, Some(BlindRoot::Topology));
        assert!(
            !reason.contains("blind-root"),
            "{refusal:?} carries no blind suffix: {reason}"
        );
        assert!(reason.contains(refusal.token()), "{reason}");
    }
}

/// Stay fixtures: the pinned follow target (SID 3, op-3) with the source
/// SID 20 from the canned row `[20, 3, 7]` (current 20).
const STAY_SOURCE: u64 = 20;

fn stay_ready_backend() -> FakeManaged {
    let mut backend = FakeManaged::new();
    backend.focused = Some("4242_7".to_owned());
    backend
}

/// Submit + take to place one request inflight, then run the stay effect
/// against that lane. Mirrors the threaded owner (take, execute, finish)
/// without spawning a thread.
fn execute_stay_inflight(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    source_sid: u64,
    source_focus: Option<&str>,
    backend: &mut FakeManaged,
) -> (
    SendEffectOutcome,
    Option<crate::space_drag_transfer::LegOk>,
    FrameSetter,
) {
    handoff.submit(target.clone()).expect("idle submits");
    handoff.take_for_owner().expect("owner takes");
    let effect = execute_managed_send_stay(target, source_sid, source_focus, backend, handoff);
    (effect.outcome, effect.leg, effect.setter)
}

#[test]
fn stay_source_sid_zero_refuses_before_any_post() {
    // Unpinned source refuses pre-actuation: zero posts, no return leg.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    let (outcome, leg, _) =
        execute_stay_inflight(&handoff, &target(), 0, Some("4242_7"), &mut backend);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.total(), 0, "zero source posts nothing");
    assert!(backend.select_back_calls.is_empty());
}

#[test]
fn stay_forward_plus_return_applies_and_settles_without_replay() {
    // Happy path: forward drag proves membership exactly [3] on current 3,
    // the released select-back returns the view to 20, and the readback
    // proves membership still exactly [3] with current exactly 20.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = stay_ready_backend();
    let (outcome, leg, setter) =
        execute_stay_inflight(&handoff, &pinned, STAY_SOURCE, Some("4242_7"), &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("stay applied carries exact return readback");
    assert_eq!((leg.membership, leg.current), (vec![3], STAY_SOURCE));
    assert!(!leg.pending_recovered);
    assert!(backend.counts.balanced(), "stay releases balance");
    assert_eq!(
        backend.select_back_calls,
        vec![STAY_SOURCE],
        "exactly one return select to the admission source"
    );
    // Same fenced completion roundtrip as the follow path: matching polls,
    // no replay, lane idles.
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
    assert!(!handoff.is_busy());
}

#[test]
fn stay_empty_source_makes_no_focus_claim() {
    // Empty source (`None` focus): the stay applies on exact
    // membership-plus-current alone, with no focus read required.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.focused = None;
    let (outcome, leg, _) =
        execute_stay_inflight(&handoff, &target(), STAY_SOURCE, None, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("empty-source stay applies");
    assert_eq!((leg.membership, leg.current), (vec![3], STAY_SOURCE));
    // Contrast: a claimed source focus that reads unknown holds instead of
    // guessing.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.focused = None;
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_source_focus_mismatch_holds_without_replay() {
    // Membership exactly [3] with current exactly 20, but focus names a
    // foreign window: uncertain return holds (never replayed, never a
    // proven refusal after the forward chord).
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = stay_ready_backend();
    backend.focused = Some("4242_9".to_owned());
    let (outcome, leg, setter) =
        execute_stay_inflight(&handoff, &pinned, STAY_SOURCE, Some("4242_7"), &mut backend);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.counts.balanced());
    assert_eq!(backend.select_back_calls, vec![STAY_SOURCE]);
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
}

#[test]
fn stay_failed_select_back_holds_after_key_post() {
    // The return select refuses after the forward chord posted: hold with
    // the key post as evidence (never a proven refusal, never replayed).
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = stay_ready_backend();
    backend.select_back_fail = Some("injected select-back refusal".to_owned());
    let (outcome, leg, setter) =
        execute_stay_inflight(&handoff, &pinned, STAY_SOURCE, Some("4242_7"), &mut backend);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert_eq!(backend.counts.key_down, 1, "forward chord posted");
    assert!(
        backend.counts.balanced(),
        "release-first even on return failure"
    );
    assert_eq!(backend.select_back_calls, vec![STAY_SOURCE]);
    let done = finish_and_poll(&handoff, &pinned, setter, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
}

#[test]
fn stay_unreadable_return_readback_holds() {
    // Post-return membership/current unreadable (the forward readback was
    // exact): hold, never a false proven refusal.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.fail_return_reads = true;
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.counts.balanced());
    assert_eq!(backend.select_back_calls, vec![STAY_SOURCE]);
}

#[test]
fn stay_changed_topology_return_holds_without_select() {
    // Foreign drift between the pinned pre-forward row and the return-leg
    // fresh row (SID 7 replaced by 9): hold before any return select. The
    // expected current change (20 -> 3) is not drift - only the SID
    // order/identity is fenced here.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.snapshot_script.push_back(Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 9],
        current: 3,
    }));
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(
        backend.select_back_calls.is_empty(),
        "drift holds before any return select"
    );
    assert!(backend.counts.balanced());
}

#[test]
fn stay_display_growth_holds_without_select() {
    // A second display appears after the forward chord: hold before any
    // return select rather than selecting under widened topology.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.displays_after_forward = Some(Ok(2));
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.select_back_calls.is_empty());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_display_identity_change_holds_without_select() {
    // Same SIDs, different display identity (swap under the row): hold
    // before any return select.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.snapshot_script.push_back(Ok(SelectionSnapshot {
        display: "display-b".to_owned(),
        spaces: vec![20, 3, 7],
        current: 3,
    }));
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.select_back_calls.is_empty());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_arrival_moved_before_return_holds_without_select() {
    // A foreign switch moves the view off the forward arrival before the
    // return leg (current 7, row otherwise unchanged): hold before select.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.snapshot_script.push_back(Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 7,
    }));
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.select_back_calls.is_empty());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_unreadable_fresh_row_holds_without_select() {
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend
        .snapshot_script
        .push_back(Err("injected row loss".to_owned()));
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(backend.select_back_calls.is_empty());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_wrong_current_return_holds() {
    // The return select runs but the view stays on the target: membership
    // exact, current wrong - hold, never claim arrival.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.select_back_hold_current = true;
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert_eq!(backend.select_back_calls, vec![STAY_SOURCE]);
    assert!(backend.counts.balanced());
}

#[test]
fn stay_cancel_before_return_holds_without_select() {
    // A stop landing mid-settle (after the forward edges proved, before
    // the return leg): hold with the forward releases balanced and no
    // return select attempted.
    let handoff = Arc::new(SendEffectHandoff::new());
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("takes");
    let mut backend = stay_ready_backend();
    backend.cancel_on_membership_read = Some(Arc::clone(&handoff));
    let crate::workspace_send_managed::SendEffect {
        outcome,
        leg,
        setter,
    } = execute_managed_send_stay(&pinned, STAY_SOURCE, Some("4242_7"), &mut backend, &handoff);
    assert!(handoff.cancel_requested(), "stop landed mid-settle");
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert!(
        backend.select_back_calls.is_empty(),
        "stop before select-back selects nothing"
    );
    assert!(backend.counts.balanced());
    let done = {
        let completion = completion_for(&pinned, setter, outcome, None);
        assert!(handoff.finish(completion));
        handoff
            .poll_completion_for(&pinned)
            .expect("terminal completion polls")
    };
    assert_eq!(done.outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(!handoff.is_busy());
}

#[test]
fn stay_forward_refusal_attempts_no_return() {
    // Pre-actuation forward refusal (focus swap): proven refusal, zero key
    // posts, no return leg.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.key_down, 0, "refused forward posts no chord");
    assert!(backend.select_back_calls.is_empty());
}

#[test]
fn stay_gate_recheck_before_return_holds() {
    // Gates open at forward admission, locked before the return: hold with
    // the forward chord posted, no return select.
    let handoff = SendEffectHandoff::new();
    let mut backend = stay_ready_backend();
    backend.gates_after_forward = Some(SendGates {
        session_unlocked: false,
        ..open_gates()
    });
    let (outcome, leg, _) = execute_stay_inflight(
        &handoff,
        &target(),
        STAY_SOURCE,
        Some("4242_7"),
        &mut backend,
    );
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert_eq!(backend.counts.key_down, 1, "forward chord posted");
    assert!(backend.select_back_calls.is_empty());
    assert!(backend.counts.balanced());
}

#[test]
fn stay_path_never_advances_previous_history() {
    // Stays keep the source current: the stay orchestrator must never note
    // a previous-history arrival (the follow drain's arrival note stays
    // follow-only) and must never touch select state.
    let source = include_str!("workspace_send_managed.rs");
    let start = source
        .find("pub(crate) fn execute_managed_send_stay")
        .expect("stay path present");
    let rest = &source[start..];
    let end = rest
        .find("\nfn ")
        .map(|i| start + i)
        .unwrap_or(source.len());
    let body = &source[start..end];
    assert!(
        !body.contains("note_arrival"),
        "stay advances no previous history"
    );
    assert!(
        !body.contains("SelectionState"),
        "stay touches no select state"
    );
}

/// Owner stay routing with explicitly supplied pins: mover B (`4242_7`)
/// leaves source 20 while resident A (`4242_9`) remains, and the caller
/// pins A as the fallback. The dedicated owner thread runs the forward
/// drag plus the return select and verifies the A pin. Nothing here
/// derives the fallback from the subject; the claim path cannot build this
/// target yet, so the pins are caller-supplied.
#[test]
fn managed_thread_routes_explicit_resident_stay_without_replay() {
    let pinned = ManagedSendTarget::bind_stay(
        4242,
        7,
        1,
        source_domain(),
        3,
        9,
        "op-3",
        STAY_SOURCE,
        Some("4242_9"),
    )
    .expect("explicit resident-A stay binds");
    assert!(!pinned.follow());
    let mut worker = ManagedSendThread::spawn(|| {
        let mut backend = FakeManaged::new();
        backend.focused = Some("4242_9".to_owned());
        backend
    });
    let handoff = worker.handoff();
    worker.submit(pinned.clone()).expect("idle submits");
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
    assert!(!done.follow, "completion carries the pinned stay intent");
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
    assert!(!handoff.is_busy());
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "nothing outstanding after poll");
}

/// Stay-branch discrimination through the owner thread: the same explicit
/// A pin with native focus still on mover B holds, while a follow through
/// the same backend would apply (it never checks the return focus). This
/// proves the owner ran the stay branch, not the follow path.
#[test]
fn managed_thread_stay_branch_holds_on_focus_mismatch_where_follow_applies() {
    let pinned = ManagedSendTarget::bind_stay(
        4242,
        7,
        1,
        source_domain(),
        3,
        9,
        "op-3",
        STAY_SOURCE,
        Some("4242_9"),
    )
    .expect("explicit resident-A stay binds");
    let mut worker = ManagedSendThread::spawn(|| {
        let mut backend = FakeManaged::new();
        backend.focused = Some("4242_7".to_owned());
        backend
    });
    let handoff = worker.handoff();
    worker.submit(pinned.clone()).expect("idle submits");
    let mut done = None;
    for _ in 0..500 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            done = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = done.expect("effect thread finishes promptly");
    assert_eq!(
        done.outcome,
        SendEffectOutcome::PossiblyExecuted,
        "mover-as-focus against an A pin holds"
    );
    assert!(!done.follow);
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
    assert!(!handoff.is_busy());
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "nothing outstanding after poll");
}

// -- Trailing owner execution (explicit local capability) ---------------------

fn trailing_request() -> TrailingSendRequest {
    TrailingSendRequest {
        pid: 4242,
        wid: 7,
        window: "4242_7".to_owned(),
        app_generation: 1,
        source: source_domain(),
        operation_id: "op-3".to_owned(),
        pins: TrailingPrepPins {
            display_index: 0,
            pinned_cgid: 1,
            pinned_uuid: "uuid-a".to_owned(),
            trailing_enabled: true,
            require_append: false,
        },
        creation_operation: None,
        creation_logical: None,
        creation_output: None,
    }
}

/// Scripted typed preparation failure: the setter mirrors
/// [`crate::workspace_send::classify_trailing_prep_error`] (proven
/// pre-actuation reasons refuse, everything else holds ambiguous) with no
/// genuine rows — the owner stages nothing and retains nothing.
fn prep_fail(reason: &str) -> crate::workspace_send::TrailingPrepFailure {
    use tiler_core::world_report::FrameSetter;
    let refused = crate::workspace_send::classify_trailing_prep_error(reason)
        == crate::workspace_send_effect::SendEffectOutcome::RefusedProven;
    crate::workspace_send::TrailingPrepFailure {
        reason: reason.to_owned(),
        setter: if refused {
            FrameSetter::Refused
        } else {
            FrameSetter::Threw
        },
        before: None,
        after: None,
        dispatched: false,
    }
}

fn trailing_prep(target: u64, created: Option<u64>) -> TrailingPrepProof {
    let (before, after) = match created {
        Some(sid) => (vec![20, 3, 7], vec![20, 3, 7, sid]),
        None => (vec![20, 3, 7], vec![20, 3, 7]),
    };
    TrailingPrepProof {
        target_sid: target,
        created,
        revision: 9,
        display_id: "display-a".to_owned(),
        before,
        after,
    }
}

fn appended_row() -> SelectionSnapshot {
    SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7, 21],
        current: 20,
    }
}

#[test]
fn trailing_append_applies_with_proof_for_adoption() {
    // Owner-verified append: the forward drag runs against the exact
    // prepared SID and the completion carries the created SID for
    // lifecycle adoption (the world adopts the native from its order row;
    // no binding rows travel).
    let handoff = SendEffectHandoff::new();
    let request = trailing_request();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(appended_row());
    backend.trailing_prep = Some(Ok(trailing_prep(21, Some(21))));
    let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
        execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert_eq!(backend.prep_calls, 1, "preparation runs exactly once");
    let target = target.expect("append binds a target");
    assert_eq!((target.target_sid(), target.revision()), (21, 9));
    assert_eq!(target.operation_id(), "op-3");
    let proof = proof.expect("append proves");
    assert_eq!(proof.target_sid, 21);
    assert_eq!(proof.revision, 9);
    assert_eq!(proof.created, Some(21), "creation rides for adoption");
    assert!(proof.creation.is_none(), "no desire stages no creation row");
    // The bound target settles through the normal completion fence.
    let done = completion_for(&target, FrameSetter::Accepted, outcome, Some(proof));
    handoff.submit(target).expect("idle submits");
    let _ = handoff.take_for_owner();
    assert!(handoff.finish(done), "proof completion stages");
}

#[test]
fn trailing_reuse_applies_without_creation() {
    // Reuse proves no append: the drag still runs and settles, but no
    // creation rides (nothing to adopt).
    let handoff = SendEffectHandoff::new();
    let request = trailing_request();
    let mut backend = FakeManaged::new();
    backend.trailing_prep = Some(Ok(trailing_prep(7, None)));
    let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
        execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let target = target.expect("reuse binds a target");
    assert_eq!(target.target_sid(), 7);
    let proof = proof.expect("target still reports for settlement");
    assert_eq!((proof.target_sid, proof.revision), (7, 9));
    assert_eq!(proof.created, None, "reuse creates nothing");
}

#[test]
fn trailing_append_reports_created_for_adoption() {
    // A request still sends to its prepared target and still reports the
    // runtime-created SID so the main thread adopts it into lifecycle
    // ownership (the world adopts the native from its order row).
    let handoff = SendEffectHandoff::new();
    let request = trailing_request();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(appended_row());
    backend.trailing_prep = Some(Ok(trailing_prep(21, Some(21))));
    let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
        execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(target.is_some());
    let proof = proof.expect("target still reports for settlement");
    assert_eq!(proof.created, Some(21), "creation rides for adoption");
    // The owned creation reaches the real main-thread adoption helper
    // (the same `adopt_created` the tile settle and shutdown arms call),
    // so the Space is lifecycle-owned even though nothing binds.
    let mut lifecycle = crate::workspace_select::TrailingLifecycle::new(&[20, 3, 7]);
    lifecycle
        .adopt_created(proof.created.expect("created above"))
        .expect("exact append adopts");
    assert_eq!(lifecycle.owned(), &[21]);
}

#[test]
fn trailing_prep_pre_actuation_refuses_proven_without_proof() {
    // Preparation refusals before any press settle proven with no proof
    // and no posts; the default backend refuses the same way.
    for reason in [
        "trailing send-0 unarmed: no trailing append runs",
        "trailing send refuses: fresh row unreadable (gone)",
        "trailing send refuses: emptiness unknown (dark)",
        "trailing send refuses: append ordinal past Desktop-9; unmappable",
        "trailing send refuses: append failed (pre-create topology drift)",
        "trailing preparation unimplemented on this backend; refusing",
        "trailing send refuses: caffeinate unspawnable (spawn failed)",
    ] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.trailing_prep = Some(Err(prep_fail(reason)));
        let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
            execute_trailing_send(&trailing_request(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{reason}");
        assert!(target.is_none() && proof.is_none(), "{reason}");
        assert_eq!(backend.counts.total(), 0, "{reason} posts nothing");
    }
    // The default (numbered-only) backend refuses without preparation.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
        execute_trailing_send(&trailing_request(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(target.is_none() && proof.is_none());
    assert_eq!(backend.prep_calls, 1);
}

#[test]
fn trailing_prep_ambiguous_holds_without_proof() {
    // Anything past a possibly-sent press holds without re-actuation and
    // proves nothing (an exact append that validated would have
    // succeeded, so no rows can bind). Unknown strings hold too.
    for reason in [
        "trailing send refuses: append failed (press refused)",
        "trailing send refuses: post-append row unreadable (gone)",
        "trailing send refuses: create effect is not an exact same-display append; adoption refused",
        "trailing send refuses: post-append revision unprovable",
        "created but MC close unproven: dismiss failed",
        "something entirely new",
    ] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.trailing_prep = Some(Err(prep_fail(reason)));
        let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
            execute_trailing_send(&trailing_request(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted, "{reason}");
        assert!(target.is_none() && proof.is_none(), "{reason}");
    }
}

/// Desire-carrying trailing request: the reply published a creation desire
/// for the unbound trailing target, so the owner stages the append receipt
/// under it.
fn trailing_request_with_desire() -> TrailingSendRequest {
    let mut request = trailing_request();
    request.creation_operation = Some("op-native-7".to_owned());
    request.creation_logical = Some("ws-trailing".to_owned());
    request.creation_output = Some("macos-display-1".to_owned());
    request
}

fn prep_row() -> SelectionSnapshot {
    SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 20,
    }
}

#[test]
fn trailing_append_stages_creation_receipt_under_desire() {
    // Owner-verified append under an engine creation desire: the proof
    // carries the raw creation receipt (exact desire identity, genuine
    // pre/post rows, verified SID, original dispatch stamp) so the main
    // thread stages it under the desire instead of fabricating one.
    let handoff = SendEffectHandoff::new();
    let request = trailing_request_with_desire();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(appended_row());
    backend.trailing_prep = Some(Ok(trailing_prep(21, Some(21))));
    let (effect, target, proof) = execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(effect.outcome, SendEffectOutcome::Applied);
    assert!(target.is_some());
    let proof = proof.expect("append proves");
    assert_eq!(proof.created, Some(21));
    let creation = proof
        .creation
        .expect("desire-gated append reports its receipt");
    assert_eq!(creation.operation_id, "op-native-7");
    assert_eq!(creation.logical, "ws-trailing");
    assert_eq!(creation.output, OutputId("macos-display-1".to_owned()));
    assert_eq!(creation.setter, FrameSetter::Accepted);
    assert_eq!(creation.order_before, vec![20, 3, 7]);
    assert_eq!(creation.order_after, vec![20, 3, 7, 21]);
    assert_eq!(creation.created, Some(21));
    assert_eq!(
        handoff.pending_creation_count(),
        0,
        "verified receipts never pend"
    );
}

#[test]
fn trailing_reuse_stages_no_creation_row() {
    // Reuse proves no append: the proof carries no creation receipt even
    // under a desire, and nothing pends.
    let handoff = SendEffectHandoff::new();
    let request = trailing_request_with_desire();
    let mut backend = FakeManaged::new();
    backend.trailing_prep = Some(Ok(trailing_prep(7, None)));
    let (effect, target, proof) = execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(effect.outcome, SendEffectOutcome::Applied);
    assert!(target.is_some());
    let proof = proof.expect("reuse still reports for settlement");
    assert_eq!(proof.created, None);
    assert!(proof.creation.is_none(), "reuse ran no create leg");
    assert_eq!(handoff.pending_creation_count(), 0);
}

#[test]
fn trailing_ambiguous_prep_retains_deferred_receipt() {
    // Ambiguous preparation (partial dispatch, unreadable post): nothing is
    // submitted, but the genuine pre-order is retained with the original
    // stamp. The first readable post-order completes the desire exactly
    // once with no claimed identity — never a fabricated post-order.
    use crate::workspace_send::TrailingPrepFailure;
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.trailing_prep = Some(Err(TrailingPrepFailure {
        reason: "trailing send refuses: append failed (press refused)".to_owned(),
        setter: FrameSetter::Threw,
        before: Some(Box::new(prep_row())),
        after: None,
        dispatched: false,
    }));
    let (effect, target, proof) =
        execute_trailing_send(&trailing_request_with_desire(), &mut backend, &handoff);
    assert_eq!(effect.outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(target.is_none() && proof.is_none());
    assert_eq!(handoff.pending_creation_count(), 1);
    // A later readable row completes the retained receipt exactly once.
    let row = SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7, 21],
        current: 20,
    };
    let pending = handoff.take_pending_creations();
    assert_eq!(pending.len(), 1);
    let report = pending[0]
        .observe(&row, u64::MAX)
        .expect("readable row completes");
    assert_eq!(report.operation_id, "op-native-7");
    assert_eq!(report.logical, "ws-trailing");
    assert_eq!(
        report.setter,
        FrameSetter::Threw,
        "dispatch truth survives deferral"
    );
    assert_eq!(report.order_before, vec![20, 3, 7]);
    assert_eq!(report.order_after, vec![20, 3, 7, 21]);
    assert_eq!(report.created, None, "a later-turn delta is uncorrelated");
}

#[test]
fn trailing_refused_prep_stages_no_receipt() {
    // Proven pre-actuation refusal with no readable post: nothing ran, so
    // nothing is retained and nothing is submitted — not even under a
    // desire.
    use crate::workspace_send::TrailingPrepFailure;
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.trailing_prep = Some(Err(TrailingPrepFailure {
        reason: "trailing send refuses: append failed (pre-create topology drift)".to_owned(),
        setter: FrameSetter::Refused,
        before: Some(Box::new(prep_row())),
        after: None,
        dispatched: false,
    }));
    let (effect, target, proof) =
        execute_trailing_send(&trailing_request_with_desire(), &mut backend, &handoff);
    assert_eq!(effect.outcome, SendEffectOutcome::RefusedProven);
    assert!(target.is_none() && proof.is_none());
    assert_eq!(
        handoff.pending_creation_count(),
        0,
        "proven refusals stage nothing"
    );
}

#[test]
fn trailing_cancel_before_prep_refuses_without_preparation() {
    // A stop that lands while the request stands inflight refuses before
    // preparation: proven pre-actuation, preparation never runs.
    let handoff = SendEffectHandoff::new();
    let request = trailing_request();
    handoff
        .submit_trailing(request.clone())
        .expect("idle submits");
    handoff.take_for_owner().expect("owner takes");
    assert!(handoff.cancel(), "inflight cancel flags the stop");
    let mut backend = FakeManaged::new();
    backend.trailing_prep = Some(Ok(trailing_prep(21, Some(21))));
    let (crate::workspace_send_managed::SendEffect { outcome, .. }, target, proof) =
        execute_trailing_send(&request, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(target.is_none() && proof.is_none());
    assert_eq!(backend.prep_calls, 0, "cancelled preparation never runs");
}

#[test]
fn trailing_cancel_after_prep_skips_drag_but_keeps_proof() {
    // A stop landing between the native ensure and the drag never runs
    // the drag; a ran append still holds as possibly-executed with its
    // proof (creation is a press), while a reuse refuses proven.
    for (created, outcome) in [
        (Some(21), SendEffectOutcome::PossiblyExecuted),
        (None, SendEffectOutcome::RefusedProven),
    ] {
        let handoff = Arc::new(SendEffectHandoff::new());
        let request = trailing_request();
        handoff
            .submit_trailing(request.clone())
            .expect("idle submits");
        handoff.take_for_owner().expect("owner takes");
        let mut backend = FakeManaged::new();
        backend.snapshot = Ok(appended_row());
        backend.trailing_prep = Some(Ok(trailing_prep(21, created)));
        backend.cancel_on_prep = Some(Arc::clone(&handoff));
        let (crate::workspace_send_managed::SendEffect { outcome: got, .. }, target, proof) =
            execute_trailing_send(&request, &mut backend, &handoff);
        assert_eq!(got, outcome, "created={created:?}");
        assert_eq!(backend.counts.total(), 0, "the drag never posts");
        if created.is_some() {
            let proof = proof.expect("ran append keeps its proof");
            assert_eq!(proof.created, created, "stopped append keeps creation");
            assert!(target.is_some(), "bound before the stop");
        } else {
            assert!(target.is_some(), "reuse binds before the stop");
            assert!(proof.expect("target reports").created.is_none());
        }
    }
}

// -- Trailing controller: off-caller preparation through the owner --------

fn trailing_claim() -> crate::runtime_types::ClaimedSelect {
    crate::runtime_types::ClaimedSelect {
        action: crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        expected_sid: None,
        ui_generation: 0,
        topology_revision: Some(9),
        send_subject: Some("4242_7".to_owned()),
        send_generation: Some(1),
        send_source: Some(source_domain()),
        send_source_sid: Some(20),
        send_source_focus: None,
        send_source_focus_empty: false,
        operation_id: Some("op-3".to_owned()),
        select_target_output: None,
        select_target_workspace: None,
        select_native_binding: None,
        select_creation_operation: None,
        select_creation_logical: None,
        select_creation_output: None,
        select_baseline_sid: None,
        send_follow_view_operation: None,
    }
}

fn trailing_factory(
    prep: Result<TrailingPrepProof, crate::workspace_send::TrailingPrepFailure>,
    snapshot: SelectionSnapshot,
    owner_thread: Arc<std::sync::Mutex<Vec<std::thread::ThreadId>>>,
) -> impl FnOnce() -> FakeManaged + Send + 'static {
    move || {
        owner_thread
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(std::thread::current().id());
        let mut backend = FakeManaged::new();
        backend.snapshot = Ok(snapshot);
        backend.trailing_prep = Some(prep);
        backend
    }
}

fn poll_until_settle(
    controller: &mut crate::workspace_send::LiveSendController,
) -> Option<crate::workspace_send::SendSettle> {
    for _ in 0..500 {
        if let Some(settle) = controller.poll() {
            return Some(settle);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn trailing_controller_prepares_off_caller_and_settles_with_proof() {
    // The submitting thread never prepares: the owner thread runs the
    // ensure, binds the token, and actuates the single drag. Submission
    // and polling are both nonblocking; settlement carries the proof.
    let owner_thread = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut controller = crate::workspace_send::LiveSendController::new();
    controller
        .submit_trailing(
            &trailing_claim(),
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            trailing_factory(
                Ok(trailing_prep(21, Some(21))),
                appended_row(),
                Arc::clone(&owner_thread),
            ),
        )
        .expect("trailing submits");
    assert!(
        controller.is_busy(),
        "submitted request serializes the lane"
    );
    let settle = poll_until_settle(&mut controller).expect("owner settles");
    assert_eq!(
        settle.outcome,
        crate::workspace_send_effect::SendEffectOutcome::Applied
    );
    assert_eq!(settle.target.target_sid(), 21);
    assert!(settle.follow);
    let proof = settle.trailing.as_ref().expect("proof settles");
    assert_eq!(proof.created, Some(21), "created SID rides for adoption");
    assert!(!controller.is_busy(), "polled lane idles");
    assert!(controller.poll().is_none(), "settlement never replays");
    // Preparation ran on the owner thread, never the submitter.
    let threads = owner_thread
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(threads.len(), 1, "one owner backend built on its thread");
    assert_ne!(
        threads[0],
        std::thread::current().id(),
        "preparation never runs on the submitting thread"
    );
    // Clean shutdown with nothing outstanding stops the owner without
    // blocking and retains nothing.
    assert!(
        controller.shutdown().is_none(),
        "settled lane shuts down idle"
    );
}

#[test]
fn trailing_controller_refusal_settles_feedback_only() {
    // Proof-less completions (preparation refused before any press) settle
    // feedback-only: no target, no history, no proof - never replayed.
    let owner_thread = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut controller = crate::workspace_send::LiveSendController::new();
    controller
        .submit_trailing(
            &trailing_claim(),
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            trailing_factory(
                Err(prep_fail(
                    "trailing send refuses: fresh row unreadable (gone)",
                )),
                row(),
                Arc::clone(&owner_thread),
            ),
        )
        .expect("trailing submits");
    for _ in 0..500 {
        if let Some(refusal) = controller.poll_trailing_refusal() {
            assert_eq!(refusal.operation_id, "op-3");
            assert_eq!(
                refusal.setter,
                tiler_core::world_report::FrameSetter::Refused
            );
            assert!(!controller.is_busy());
            assert!(controller.poll().is_none());
            assert!(controller.poll_trailing_refusal().is_none());
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("proof-less completion settles");
}

#[test]
fn trailing_controller_observation_path_completes_deferred_receipt_once() {
    // Owner-path integration through the production observation seam
    // (`LiveSendController::observe_trailing_creation_receipts`): an
    // ambiguous preparation (partial dispatch, unreadable post) retains
    // the genuine pre-order under the desire; a mismatched-display row
    // retains with nothing submitted; the first readable same-display
    // delta emits exactly one genuine receipt (original dispatch stamp,
    // no claimed identity); a repeat observation emits none.
    use crate::workspace_select::SelectionSnapshot;
    use crate::workspace_send::TrailingPrepFailure;
    let owner_thread = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut controller = crate::workspace_send::LiveSendController::new();
    let mut claim = trailing_claim();
    claim.select_creation_operation = Some("op-native-7".to_owned());
    claim.select_creation_logical = Some("ws-trailing".to_owned());
    claim.select_creation_output = Some("macos-display-1".to_owned());
    let submit_ms = crate::input_eligibility::now_ms();
    controller
        .submit_trailing(
            &claim,
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            trailing_factory(
                Err(TrailingPrepFailure {
                    reason: "trailing send refuses: append failed (press refused)".to_owned(),
                    setter: FrameSetter::Threw,
                    before: Some(Box::new(prep_row())),
                    after: None,
                    dispatched: false,
                }),
                prep_row(),
                Arc::clone(&owner_thread),
            ),
        )
        .expect("trailing submits");
    // Proof-less refusal settles feedback-only; the deferred creation
    // receipt stays retained on the owner handoff.
    let mut refused = None;
    for _ in 0..500 {
        if let Some(refusal) = controller.poll_trailing_refusal() {
            refused = Some(refusal);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let refusal = refused.expect("proof-less refusal settles");
    assert_eq!(refusal.operation_id, "op-3");
    assert_eq!(refusal.setter, FrameSetter::Threw);
    let observe_ms = crate::input_eligibility::now_ms();
    // Mismatched display: retained, nothing submitted.
    let foreign = SelectionSnapshot {
        display: "display-b".to_owned(),
        spaces: vec![20, 3, 7, 21],
        current: 20,
    };
    assert!(
        controller
            .observe_trailing_creation_receipts(&foreign, u64::MAX)
            .is_empty(),
        "mismatched display retains without submission"
    );
    // Fresh readable same-display delta: exactly one genuine receipt.
    let arrived = SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7, 21],
        current: 20,
    };
    let receipts = controller.observe_trailing_creation_receipts(&arrived, u64::MAX);
    assert_eq!(receipts.len(), 1, "first readable delta completes once");
    let receipt = &receipts[0];
    assert_eq!(receipt.operation_id, "op-native-7");
    assert_eq!(receipt.logical, "ws-trailing");
    assert_eq!(receipt.output, OutputId("macos-display-1".to_owned()));
    assert_eq!(
        receipt.setter,
        FrameSetter::Threw,
        "dispatch truth survives deferral"
    );
    assert_eq!(receipt.order_before, vec![20, 3, 7]);
    assert_eq!(receipt.order_after, vec![20, 3, 7, 21]);
    assert_eq!(receipt.created, None, "a later-turn delta is uncorrelated");
    assert!(
        (submit_ms..=observe_ms).contains(&receipt.attempt_ms),
        "original dispatch stamp preserved, not the observe time: {} not in [{submit_ms},{observe_ms}]",
        receipt.attempt_ms,
    );
    // Repeat observation emits none (at-most-once, no replay).
    assert!(
        controller
            .observe_trailing_creation_receipts(&arrived, u64::MAX)
            .is_empty(),
        "receipt completes exactly once"
    );
    assert!(
        controller.shutdown().is_none(),
        "settled lane shuts down idle"
    );
}

#[test]
fn trailing_controller_busy_and_unpinned_refuse_before_submit() {
    // A second submit while inflight refuses (concurrent presses
    // serialize, never overlap); unpinned claims and displays refuse
    // before any owner spawn or actuation.
    let mut controller = crate::workspace_send::LiveSendController::new();
    let owner_thread = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    controller
        .submit_trailing(
            &trailing_claim(),
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            move || {
                owner_thread
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .push(std::thread::current().id());
                let mut backend = FakeManaged::new();
                backend.block_prep = Some((started_tx, release_rx));
                backend
            },
        )
        .expect("first submits");
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("owner reaches preparation");
    assert_eq!(
        controller.submit_trailing(
            &trailing_claim(),
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            FakeManaged::new,
        ),
        Err("send-effect-busy: one managed send already inflight; refusing overlap".to_owned()),
        "concurrent presses serialize"
    );
    // Unpinned display legs refuse without touching the lane state.
    let mut idle = crate::workspace_send::LiveSendController::new();
    assert!(
        idle.submit_trailing(
            &trailing_claim(),
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 0,
                pinned_uuid: "",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            FakeManaged::new,
        )
        .is_err()
    );
    assert!(!idle.is_busy());
    // A non-trailing claim refuses too.
    let mut numbered = trailing_claim();
    numbered.action = crate::runtime_types::InputSemanticAction::WorkspaceSend {
        index: 2,
        follow: true,
    };
    assert!(
        idle.submit_trailing(
            &numbered,
            crate::workspace_send::TrailingSubmit {
                display_index: 0,
                pinned_cgid: 1,
                pinned_uuid: "uuid-a",
                display: "display-a",
                from: 20,
                trailing_enabled: true,
            },
            FakeManaged::new
        )
        .is_err()
    );
    assert!(!idle.is_busy());
    // Shutdown never blocks on the held owner and settles honestly: the
    // blocked preparation may still run, so the dropped inflight holds.
    let outcome = controller.shutdown().expect("dropped inflight settles");
    match outcome {
        crate::workspace_send::ShutdownOutcome::TrailingRefusal(refusal) => {
            assert_eq!(refusal.operation_id, "op-3");
            assert_eq!(
                refusal.setter,
                tiler_core::world_report::FrameSetter::Threw,
                "a preparation that may still run holds, never claims refusal"
            );
        }
        other => panic!("blocked trailing shuts down as refusal, got {other:?}"),
    }
    let _ = release_tx.send(());
}

// -- Fast bridged follow regressions (offline, no live touch) --------------

fn fast_backend() -> FakeManaged {
    let mut backend = FakeManaged::new();
    backend.bridged_enabled = true;
    // Follow arrival needs the subject focus on the target; stay tests
    // override `focused` (source) while this stays the target.
    backend.focused_follow = Some("4242_7".to_owned());
    backend.focused = Some("4242_7".to_owned());
    backend
}

#[test]
fn bridged_fast_applies_without_mouse_or_key_posts() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    let (outcome, leg, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("fast applied carries readback");
    assert_eq!(leg.membership, vec![3]);
    assert_eq!(leg.current, 3);
    assert!(!leg.pending_recovered);
    // Exactly one direct native move, one destination select, zero HID.
    assert_eq!(backend.bridged_calls, vec![3]);
    assert_eq!(backend.destination_calls, vec![3]);
    assert_eq!(backend.use_route_calls, 1);
    assert_eq!(backend.counts.total(), 0, "fast route posts no mouse/key");
    assert!(backend.mouse_points.is_empty());
    assert!(backend.key_posts.is_empty());
    assert!(backend.key_chords.is_empty());
}

#[test]
fn bridged_unavailable_falls_back_to_drag_before_dispatch() {
    // Default fake reports unavailable: the existing drag posts.
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.focused = Some("4242_7".to_owned());
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(backend.bridged_calls.is_empty());
    assert!(backend.destination_calls.is_empty());
    assert!(backend.counts.key_down > 0);
}

#[test]
fn bridged_proof_refusal_posts_nothing_and_never_selects() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    let (outcome, _, setter) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(setter, FrameSetter::Refused);
    assert!(backend.bridged_calls.is_empty());
    assert!(backend.destination_calls.is_empty());
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn bridged_anchor_drift_refuses_pre_dispatch_without_select() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    backend.bridged_drift_row = Some(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 7],
        current: 20,
    });
    let (outcome, _, setter) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(setter, FrameSetter::Refused);
    assert!(
        backend.bridged_calls.is_empty(),
        "drift refuses before dispatch"
    );
    assert!(backend.destination_calls.is_empty());
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn bridged_dispatch_failure_throws_without_fallback_to_drag() {
    // An errored OS dispatch is native uncertainty, never a proven
    // pre-actuation refusal: the touch happened, so the setter throws and
    // the outcome holds without fallback or replay.
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    backend.bridged_fail = Some("injected bridged dispatch refusal".to_owned());
    let (outcome, _, setter) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(setter, FrameSetter::Threw);
    assert!(backend.destination_calls.is_empty());
    assert_eq!(
        backend.counts.key_down, 0,
        "no drag fallback after dispatch failure"
    );
    assert_eq!(backend.counts.mouse_down, 0);
}

#[test]
fn bridged_uncertain_arrival_holds_without_replay_or_second_dispatch() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    // Async completion never lands: the settle deadline holds after the
    // selector already ran (select-first, settle-after).
    backend.membership = vec![20];
    // Keep the dispatched membership write from fixing it.
    backend.unreadable = true;
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(
        backend.bridged_calls.len(),
        1,
        "exactly one dispatch, no replay"
    );
    assert_eq!(
        backend.destination_calls,
        vec![3],
        "selector runs before the settle await, even when arrival never lands"
    );
    assert_eq!(backend.counts.total(), 0, "no drag fallback after dispatch");
}

#[test]
fn bridged_selector_starts_before_any_post_dispatch_membership_poll() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let bridged_at = backend
        .order
        .iter()
        .position(|e| e == "bridged")
        .expect("dispatch logged");
    let select_at = backend
        .order
        .iter()
        .position(|e| e == "select")
        .expect("select logged");
    assert!(
        select_at > bridged_at,
        "selector starts after dispatch, order={:?}",
        backend.order
    );
    assert!(
        !backend.order[bridged_at + 1..select_at].contains(&"membership".to_owned()),
        "no serial membership gate between dispatch and selector, order={:?}",
        backend.order
    );
    assert_eq!(backend.destination_calls.len(), 1, "exactly one select");
}

#[test]
fn bridged_late_membership_still_applies_after_select() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    // Async completion lands late: the first settle reads miss, then the
    // exact arrival names the goal.
    backend.settle_script =
        VecDeque::from([Some(vec![20]), Some(vec![20]), Some(vec![3]), Some(vec![3])]);
    let (outcome, leg, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("late arrival still carries readback");
    assert_eq!(leg.membership, vec![3]);
    assert_eq!(leg.current, 3);
    assert_eq!(backend.bridged_calls.len(), 1);
    assert_eq!(backend.destination_calls, vec![3], "no select replay");
}

#[test]
fn bridged_missing_membership_after_select_holds() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    // Selector ran fine but the async completion never lands.
    backend.settle_script = VecDeque::from([Some(vec![20]), Some(vec![20]), None]);
    backend.unreadable = true;
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(backend.bridged_calls.len(), 1);
    assert_eq!(backend.destination_calls, vec![3], "one select, no replay");
}

#[test]
fn bridged_wrong_current_after_select_holds() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    // Membership lands but the view never follows: the final exact
    // membership/current gate holds.
    backend.destination_hold_current = true;
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(backend.bridged_calls.len(), 1);
    assert_eq!(backend.destination_calls, vec![3], "one select, no replay");
}

#[test]
fn bridged_cancel_during_settle_stops_promptly_without_replay() {
    let target = target();
    let lane = Arc::new(SendEffectHandoff::new());
    lane.submit(target.clone()).expect("idle submits");
    lane.take_for_owner().expect("owner takes");
    let mut backend = fast_backend();
    // The first settle membership read requests stop: the read-only await
    // honors cancel for a prompt stop (still possibly-executed - dispatch
    // and select both already executed).
    backend.cancel_on_membership_read = Some(Arc::clone(&lane));
    backend.settle_script = VecDeque::from([Some(vec![20]), Some(vec![3])]);
    let crate::workspace_send_managed::SendEffect { outcome, .. } =
        execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(backend.bridged_calls.len(), 1);
    assert_eq!(backend.destination_calls, vec![3], "one select, no replay");
}

#[test]
fn bridged_uncertain_select_holds_without_replay() {
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    backend.destination_fail = Some("injected destination select refusal".to_owned());
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(backend.bridged_calls.len(), 1);
    assert_eq!(backend.destination_calls, vec![3]);
    // No drag posts even though the select failed.
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn bridged_wrong_membership_current_or_focus_holds() {
    // Foreign focus after an otherwise exact arrival holds (no AX raise).
    let target = target();
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    backend.focused_follow = Some("4242_9".to_owned());
    let (outcome, _, _) = execute(&target, &mut backend, &lane);
    assert_eq!(
        outcome,
        SendEffectOutcome::PossiblyExecuted,
        "foreign focus holds"
    );
    assert_eq!(backend.bridged_calls.len(), 1);
}

#[test]
fn bridged_cancel_before_dispatch_refuses_proven() {
    let target = target();
    let lane = SendEffectHandoff::new();
    lane.submit(target.clone()).expect("idle submits");
    lane.take_for_owner().expect("owner takes");
    lane.cancel();
    let mut backend = fast_backend();
    let crate::workspace_send_managed::SendEffect { outcome, .. } =
        execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(backend.bridged_calls.is_empty());
}

#[test]
fn bridged_stay_reuses_fast_follow_then_released_return_with_source_focus() {
    let stay = ManagedSendTarget::bind_stay(
        4242,
        7,
        1,
        source_domain(),
        3,
        9,
        "op-3",
        20,
        Some("4242_9"),
    )
    .expect("stay binds");
    let lane = SendEffectHandoff::new();
    let mut backend = fast_backend();
    // Fast follow moves membership to the target; the destination select
    // follows the view; the stay return selects back to the source.
    backend.focused = Some("4242_9".to_owned());
    // The stay return-leg focus read names the source resident.
    let crate::workspace_send_managed::SendEffect { outcome, leg, .. } =
        execute_managed_send_stay(&stay, 20, Some("4242_9"), &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("stay applied carries return readback");
    assert_eq!(leg.membership, vec![3]);
    assert_eq!(leg.current, 20);
    assert_eq!(backend.bridged_calls, vec![3]);
    assert_eq!(backend.destination_calls, vec![3]);
    assert_eq!(backend.select_back_calls, vec![20]);
    assert_eq!(backend.counts.total(), 0);
}

// -- Reply-driven send report/attempt join tests ---------------------------
//
// Real engine dispatch plus the scripted native backend: the accepted
// reply's retained window transfer plus its live native binding binds the
// claim plan (never an input-ordinal pin), the owner actuates and captures
// genuine reads around the dispatch, and the staged raw attempt rows settle
// through the next production dispatch. No live touch.
mod send_join_tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::session::DomainKey;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::FrameSetter;

    use crate::focus_route::FocusRoute;
    use crate::runtime::{FocusRequest, Shared, lock_shared, window_id};
    use crate::runtime_core::{RetainedApp, RetainedCore};
    use crate::runtime_thread::{Inbox, bind_send_plan};
    use crate::runtime_types::{
        ClaimedSelect, InputSemanticAction, NativeContext, NativeDomain, ObservedWindow,
    };
    use crate::topology::{output_key, workspace_key};
    use crate::workspace_send_effect::{ManagedSendTarget, SendEffectHandoff, SendEffectOutcome};
    use crate::workspace_send_managed::ManagedSendBackend;
    use crate::world_command_map::OP_SEND;
    use crate::world_dispatch::{DispatchVerdict, dispatch_report};
    use crate::world_runtime::{DispatchContext, WorldSession};
    use crate::world_sequencer::ObservationClock;

    use super::{FakeManaged, execute_managed_send};

    const UUID_1: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";

    fn output() -> OutputId {
        OutputId(output_key(1))
    }

    fn source_key() -> DomainKey {
        DomainKey {
            output: output(),
            workspace: WorkspaceId(workspace_key(20)),
        }
    }

    fn domain(space: u64, workspace: &str, current: bool) -> NativeDomain {
        NativeDomain {
            cg_id: 1,
            uuid: Some(UUID_1.to_owned()),
            output: output(),
            workspace: WorkspaceId(workspace.to_owned()),
            space,
            current,
            readable: true,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            visible: Rect {
                x: 0,
                y: 25,
                w: 1440,
                h: 875,
            },
        }
    }

    fn ctx() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 9,
            domains: vec![
                domain(20, &workspace_key(20), true),
                domain(3, &workspace_key(3), false),
            ],
            skipped: Vec::new(),
        })
    }

    fn observed(wid: u32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x: 100,
            y: 100,
            w: 800,
            h: 600,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    /// Retained truth for the join lane: subject window on the current
    /// Space (observed focus), homed exactly, generation 1.
    fn retained_send_subject() -> RetainedCore {
        let mut retained = RetainedCore::default();
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(7)],
                focused_wid: Some(7),
                roster: BTreeMap::new(),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        let window = window_id(4242, 7);
        retained.window_domains.insert(window.clone(), source_key());
        retained.observed_focus = Some(WindowId(window));
        retained
    }

    fn test_shared(ctx: Arc<NativeContext>) -> Arc<Mutex<Shared>> {
        Arc::new(Mutex::new(Shared {
            config: crate::runtime_types::RuntimeConfig::default(),
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            clock: Arc::new(ObservationClock::new()),
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            topology_revision: ctx.revision,
            native_context: Some(Arc::clone(&ctx)),
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: None,
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
            selection_focus_floor_seq: 0,
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: None,
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_input: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_logger: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            log_tx: None,
            diagnostics: crate::runtime_types::RuntimeDiagnostics::default(),
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: tiler_core::world_presentation::WorldPresentation::default(),
            last_pager_bindings: Vec::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }))
    }

    #[allow(clippy::type_complexity)]
    fn harness(
        ctx: Arc<NativeContext>,
        retained: RetainedCore,
    ) -> (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Inbox>,
        Arc<Mutex<Vec<FocusRequest>>>,
    ) {
        (
            WorldSession::new(
                tiler_core::size_hints::FixedSizePredicate::BothAxes,
                true,
                FocusRoute::Slps,
                Arc::new(ObservationClock::new()),
            ),
            retained,
            test_shared(ctx),
            Arc::new(Inbox::new(16)),
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    /// Bootstrap one observation-only report through the real dispatch path
    /// so adoption converges before input dispatches.
    fn bootstrap_observation(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        ctx: &Arc<NativeContext>,
    ) {
        match dispatch_report(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
            DispatchContext::Native(Arc::clone(ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } | DispatchVerdict::Duplicate => {}
            DispatchVerdict::Refused { token, .. } => {
                panic!("bootstrap observation must apply, got {token}")
            }
            DispatchVerdict::NoReport => panic!("bootstrap observation must report"),
        }
    }

    /// Scripted native backend for the join lane: the reply-bound target
    /// SID sits at live ordinal 2 of the fresh row, homing starts on the
    /// source, and the drag flips both legs to the goal on release.
    fn fake_send_backend() -> FakeManaged {
        let mut backend = FakeManaged::new();
        backend.snapshot = Ok(crate::workspace_select::SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3],
            current: 20,
        });
        backend.evidence_base = crate::workspace_send_effect::SendFreshEvidence {
            managed_current: Some(true),
            fullscreen: Some(false),
            minimized: Some(false),
            live_generation: Some(1),
            live_revision: Some(9),
            live_source: Some(source_key()),
            source_current: Some(true),
            focused_window: Some("4242_7".to_owned()),
            focus_agrees: Some(true),
            geometry_ok: Some(true),
            grab_point: Some((100.0, 200.0)),
            ax_hit: Some("4242_7".to_owned()),
            ws_hit: Some("4242_7".to_owned()),
        };
        backend.membership = vec![20];
        backend.current = Some(20);
        backend
    }

    fn single_sid(membership: Option<Vec<u64>>) -> Option<u64> {
        match membership {
            Some(list) if list.len() == 1 && list[0] != 0 => Some(list[0]),
            _ => None,
        }
    }

    /// Dispatch one follow send through the real engine lane and bind the
    /// reply plan into the claim. Returns the bound claim, its operation
    /// identity, and the pinned owner target.
    fn dispatch_bound_send(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        ctx: &Arc<NativeContext>,
    ) -> (ClaimedSelect, String, ManagedSendTarget) {
        let action = InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        };
        // Same implicit core-owned source mapping as production: no
        // adapter output decision, no native-domain default.
        let command = crate::world_command_map::send_command_implicit_with_capability(
            &action, "4242_7", true,
        )
        .expect("armed send maps");
        let mut claim = ClaimedSelect::tap(action);
        claim.send_subject = Some("4242_7".to_owned());
        claim.send_generation = Some(1);
        claim.send_source = Some(source_key());
        claim.send_source_sid = Some(20);
        claim.topology_revision = Some(ctx.revision);
        let operation = match dispatch_report(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
            DispatchContext::Native(Arc::clone(ctx)),
            Vec::new(),
            Some(command),
            None,
            true,
            OP_SEND,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied {
                select_operation: Some(op),
            } => op,
            other => panic!("send dispatch must apply with an operation, got {other:?}"),
        };
        claim.operation_id = Some(operation.clone());
        bind_send_plan(session, ctx, &mut claim, &operation);
        // Reply-driven binding: the retained transfer target plus its live
        // native binding authorizes the send - never the input index.
        assert_eq!(
            claim.select_native_binding.as_deref(),
            Some(workspace_key(3).as_str()),
            "reply binds the live target desktop"
        );
        assert_eq!(
            claim.expected_sid,
            Some(3),
            "owner target rides the reply binding"
        );
        // The follow view identity is the reply's own published View flight
        // for the same target - read, never formatted.
        let reply = session.sequencer().last_reply().expect("applied reply");
        let view_flight = reply
            .transfers
            .iter()
            .find(|flight| {
                matches!(flight.kind, tiler_core::world_report::TransferKind::View)
                    && flight.target_output.0 == output().0
                    && flight.target_workspace
                        == claim
                            .select_target_workspace
                            .clone()
                            .expect("plan binds target")
            })
            .expect("reply publishes the follow view flight");
        assert_eq!(
            claim.send_follow_view_operation.as_deref(),
            Some(view_flight.operation_id.as_str()),
            "follow view identity rides the reply, never a formatted id"
        );
        let target =
            ManagedSendTarget::from_claim(&claim).expect("reply-bound claim pins the target");
        assert_eq!(target.target_sid(), 3);
        (claim, operation, target)
    }

    /// Run one forward effect through the real owner path and return the
    /// real owner completion: genuine reads captured around the dispatch
    /// (pre-write pair before the effect, leg readback or confirming
    /// re-read after), combined with the owner's raw write-boundary setter
    /// through the real [`completion_for_with_reads`] constructor. `arm`
    /// runs after the pre-capture and before the effect, so a test can
    /// script mid-effect native behavior (delayed arrival, post-write
    /// unreadability) without disturbing the pre-write baseline.
    fn execute_captured(
        target: &ManagedSendTarget,
        backend: &mut FakeManaged,
        arm: impl FnOnce(&mut FakeManaged),
    ) -> crate::workspace_send_effect::SendCompletion {
        use crate::workspace_send_managed::completion_for_with_reads;
        let pre_member = single_sid(backend.read_membership());
        let pre_view = backend.read_current();
        arm(backend);
        let lane = SendEffectHandoff::new();
        let effect = execute_managed_send(target, backend, &lane);
        let (post_member, post_view) = match effect.leg.as_ref() {
            Some(leg) => (single_sid(Some(leg.membership.clone())), Some(leg.current)),
            None => (
                single_sid(backend.read_membership()),
                backend.read_current(),
            ),
        };
        completion_for_with_reads(
            target,
            effect.setter,
            effect.outcome,
            None,
            pre_member,
            post_member,
            pre_view,
            post_view,
        )
    }

    /// Moved topology after arrival: the same native-keyed domains with
    /// the bound target Space current (never re-keyed to core logicals:
    /// the adapter speaks native keys and the core translates).
    fn moved_ctx() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 10,
            domains: vec![
                domain(20, &workspace_key(20), false),
                domain(3, &workspace_key(3), true),
            ],
            skipped: Vec::new(),
        })
    }

    /// Topology with a third native Space for the external-move case: the
    /// window homes to a location that is neither the source nor the
    /// bound target.
    fn third_space_ctx() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 10,
            domains: vec![
                domain(20, &workspace_key(20), true),
                domain(3, &workspace_key(3), false),
                domain(99, &workspace_key(99), false),
            ],
            skipped: Vec::new(),
        })
    }

    #[test]
    fn reply_bound_send_applies_and_settles_on_exact_arrival() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "bootstrap retains nothing"
        );
        let (claim, operation, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        let completion = execute_captured(&target, &mut backend, |_| {});
        assert_eq!(completion.outcome, SendEffectOutcome::Applied);
        // The raw setter comes from the actual write boundary (the chord
        // post reported success), never derived from the arrival outcome.
        assert_eq!(completion.setter, FrameSetter::Accepted);
        // Genuine reads around the dispatch: homing starts on the source,
        // the verified post names the bound arrival on both legs.
        assert_eq!(
            (completion.pre_member, completion.post_member),
            (Some(20), Some(3))
        );
        assert_eq!(
            (completion.pre_view, completion.post_view),
            (Some(20), Some(3))
        );
        assert!(
            backend.counts.key_down > 0,
            "forward chord posted exactly once"
        );
        // The follow view flight settles on the same genuine reads through
        // the ordinary view-attempt lane, under the reply's own published
        // view identity carried by the claim.
        let follow_op = claim
            .send_follow_view_operation
            .clone()
            .expect("reply binds the follow view flight");
        session.stage_member_attempt(
            &operation,
            completion.setter,
            completion.pre_member,
            completion.post_member,
        );
        session.stage_view_attempt(
            &follow_op,
            completion.setter,
            completion.pre_view,
            completion.post_view,
            claim.select_baseline_sid,
        );
        assert_eq!(claim.select_baseline_sid, Some(20));
        // Arrival observation: homing names the bound target's native key
        // under the moved current, so the next production dispatch settles
        // the transfer.
        retained.window_domains.insert(
            window_id(4242, 7),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(workspace_key(3)),
            },
        );
        let moved = moved_ctx();
        lock_shared(&shared).native_context = Some(Arc::clone(&moved));
        lock_shared(&shared).topology_revision = moved.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&moved)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("arrival observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "exact arrival plus genuine post settles the transfer"
        );
    }

    #[test]
    fn explicit_refusal_before_dispatch_stages_refused_with_unchanged_preimage() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let (claim, operation, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        // Pure focus swap while the subject stays managed and readable:
        // the per-edge proof refuses before any post.
        backend.evidence_base.focused_window = Some("4242_9".to_owned());
        let completion = execute_captured(&target, &mut backend, |_| {});
        assert_eq!(completion.outcome, SendEffectOutcome::RefusedProven);
        // Refusal before any post carries `Refused` from the write boundary.
        assert_eq!(completion.setter, FrameSetter::Refused);
        assert_eq!(
            backend.counts.total(),
            0,
            "pre-actuation refusal posts nothing"
        );
        // The confirming re-read names the untouched source on both legs.
        assert_eq!(
            (completion.pre_member, completion.post_member),
            (Some(20), Some(20))
        );
        assert_eq!(
            (completion.pre_view, completion.post_view),
            (Some(20), Some(20))
        );
        let follow_op = claim
            .send_follow_view_operation
            .clone()
            .expect("reply binds the follow view flight");
        session.stage_member_attempt(
            &operation,
            completion.setter,
            completion.pre_member,
            completion.post_member,
        );
        session.stage_view_attempt(
            &follow_op,
            completion.setter,
            completion.pre_view,
            completion.post_view,
            claim.select_baseline_sid,
        );
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("refusal observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "proven refusal with the unchanged-readable preimage settles to observed"
        );
    }

    #[test]
    fn accepted_delayed_arrival_reports_accepted_with_genuine_post() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let (claim, operation, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        // Intermediate settle read still names the source before the async
        // completion lands on the bound target.
        let completion = execute_captured(&target, &mut backend, |backend| {
            backend.settle_script.push_back(Some(vec![20]));
        });
        assert_eq!(completion.outcome, SendEffectOutcome::Applied);
        // Successful queue/post stages Accepted even though arrival was
        // delayed past an intermediate read: the verified post still names
        // the bound target genuinely. The setter is the write-boundary
        // truth, not a derivation of the delayed arrival.
        assert_eq!(completion.setter, FrameSetter::Accepted);
        assert_eq!(
            (completion.pre_member, completion.post_member),
            (Some(20), Some(3))
        );
        let follow_op = claim
            .send_follow_view_operation
            .clone()
            .expect("reply binds the follow view flight");
        session.stage_member_attempt(
            &operation,
            completion.setter,
            completion.pre_member,
            completion.post_member,
        );
        session.stage_view_attempt(
            &follow_op,
            completion.setter,
            completion.pre_view,
            completion.post_view,
            claim.select_baseline_sid,
        );
        retained.window_domains.insert(
            window_id(4242, 7),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(workspace_key(3)),
            },
        );
        let moved = moved_ctx();
        lock_shared(&shared).native_context = Some(Arc::clone(&moved));
        lock_shared(&shared).topology_revision = moved.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&moved)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("delayed arrival must still apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "delayed-but-verified arrival settles the transfer"
        );
    }

    #[test]
    fn accepted_write_then_unreadable_reports_accepted_and_holds() {
        // The core correction: a native write that reported success stages
        // `Accepted` even when the post-write readback is unreadable. The
        // setter is the write-boundary truth (the chord demonstrably
        // posted); the missing post holds the transfer instead of settling
        // or refusing it.
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let (claim, operation, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        let completion = execute_captured(&target, &mut backend, |backend| {
            backend.unreadable = true;
        });
        assert!(
            backend.counts.key_down > 0,
            "the chord post fired before the readback failed"
        );
        assert_eq!(completion.outcome, SendEffectOutcome::PossiblyExecuted);
        assert_eq!(
            completion.setter,
            FrameSetter::Accepted,
            "a reported-success write stages Accepted, never a derived Threw"
        );
        assert_eq!(
            (completion.pre_member, completion.post_member),
            (Some(20), None)
        );
        assert_eq!(
            (completion.pre_view, completion.post_view),
            (Some(20), None)
        );
        let follow_op = claim
            .send_follow_view_operation
            .clone()
            .expect("reply binds the follow view flight");
        session.stage_member_attempt(
            &operation,
            completion.setter,
            completion.pre_member,
            completion.post_member,
        );
        session.stage_view_attempt(
            &follow_op,
            completion.setter,
            completion.pre_view,
            completion.post_view,
            claim.select_baseline_sid,
        );
        // The staged row carries the write truth verbatim: Accepted with a
        // genuine preimage and an unknown post.
        let staged = session
            .pending_members
            .get(&(operation.clone(), "4242_7".to_owned()))
            .expect("member attempt staged");
        assert_eq!(staged.setter, FrameSetter::Accepted);
        assert_eq!(staged.native_pre.as_deref(), Some("space-20"));
        assert_eq!(staged.native_post, None);
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("unreadable hold must still apply, got {other:?}"),
        }
        assert!(
            !session.engine_mut().retained_transfers().is_empty(),
            "unreadable post holds the transfer without false settlement"
        );
    }

    #[test]
    fn newer_external_homing_holds_without_reactuation() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let (claim, operation, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        let completion = execute_captured(&target, &mut backend, |_| {});
        assert_eq!(completion.outcome, SendEffectOutcome::Applied);
        assert_eq!(completion.setter, FrameSetter::Accepted);
        let follow_op = claim
            .send_follow_view_operation
            .clone()
            .expect("reply binds the follow view flight");
        session.stage_member_attempt(
            &operation,
            completion.setter,
            completion.pre_member,
            completion.post_member,
        );
        session.stage_view_attempt(
            &follow_op,
            completion.setter,
            completion.pre_view,
            completion.post_view,
            claim.select_baseline_sid,
        );
        // Newer external input moves the window to a third Space before
        // the next report: the verified post still names the bound target,
        // but current homing contradicts it on a known third location.
        // The flight must hold (never re-actuate, never fight the newer
        // input).
        retained.window_domains.insert(
            window_id(4242, 7),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(workspace_key(99)),
            },
        );
        let third = third_space_ctx();
        lock_shared(&shared).native_context = Some(Arc::clone(&third));
        lock_shared(&shared).topology_revision = third.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&third)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("external observation must apply, got {other:?}"),
        }
        let transfers = session.engine_mut().retained_transfers();
        assert!(
            transfers.iter().any(|t| t.operation_id == operation),
            "contradicted arrival holds the flight instead of settling it"
        );
        // No replay: an identical redispatch against unchanged truth is a
        // duplicate with no new retained operation.
        let before = session.engine_mut().retained_transfers().len();
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&third)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } | DispatchVerdict::Duplicate => {}
            other => panic!("quiet redispatch must not refuse, got {other:?}"),
        }
        assert_eq!(
            session.engine_mut().retained_transfers().len(),
            before,
            "held flight never re-actuates or duplicates"
        );
    }

    #[test]
    fn completion_replay_never_reapplies_and_staging_is_latest_wins() {
        // The completion under test comes from the real executor path, not
        // a hand-built row: pre-write reads, effect, post reads, and the
        // owner's raw setter flow through the real completion constructor.
        // Only the stale-lifetime row below is forged (a mutated copy),
        // which is exactly what the fence must reject.
        let target = ManagedSendTarget::bind(4242, 7, 1, source_key(), 3, 9, "op-9")
            .expect("fixture target binds");
        let lane = SendEffectHandoff::new();
        lane.submit(target.clone()).expect("idle submits");
        lane.take_for_owner().expect("owner takes");
        let mut backend = fake_send_backend();
        backend.snapshot = Ok(crate::workspace_select::SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3, 7],
            current: 20,
        });
        backend.evidence_base.live_source = Some(source_key());
        let completion = execute_captured(&target, &mut backend, |_| {});
        assert_eq!(completion.outcome, SendEffectOutcome::Applied);
        assert_eq!(completion.setter, FrameSetter::Accepted);
        assert!(
            lane.finish(completion.clone()),
            "matching finish stages once"
        );
        assert!(
            !lane.finish(completion.clone()),
            "the same completion never stages twice"
        );
        let done = lane
            .poll_completion_for(&target)
            .expect("main polls the completion");
        assert_eq!((done.pre_member, done.post_member), (Some(20), Some(3)));
        assert_eq!((done.pre_view, done.post_view), (Some(20), Some(3)));
        assert_eq!(done.setter, FrameSetter::Accepted);
        assert!(
            lane.poll_completion_for(&target).is_none(),
            "polling consumes: never replayed"
        );
        // A relaunched lifetime under the same PID never settles under the
        // old completion: forge the generation on a copy of the real row.
        lane.submit(target.clone()).expect("idle submits again");
        lane.take_for_owner().expect("owner takes again");
        let mut stale = completion;
        stale.app_generation = 2;
        assert!(!lane.finish(stale), "wrong lifetime is stale");
    }

    #[test]
    fn generation_mismatch_refuses_without_posts() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let (_, _, target) = dispatch_bound_send(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        let mut backend = fake_send_backend();
        // Relaunched lifetime under the same PID: the pinned generation no
        // longer matches - never grab the new window.
        backend.evidence_base.live_generation = Some(2);
        let completion = execute_captured(&target, &mut backend, |_| {});
        assert_eq!(completion.outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(completion.setter, FrameSetter::Refused);
        assert_eq!(backend.counts.total(), 0, "stale lifetime posts nothing");
    }

    #[test]
    fn trailing_unbound_without_desire_refuses_without_creation() {
        // Missing bindings never create: an unbound trailing target with no
        // explicit creation desire refuses at the owner boundary instead of
        // guessing. (An unbound target WITH a desire is a separate blocked
        // case: the desire-gated native creation seam is unavailable pending
        // typed truthful receipts - see the drain.)
        let claim = ClaimedSelect {
            action: InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            },
            expected_sid: None,
            ui_generation: 0,
            topology_revision: Some(9),
            send_subject: Some("4242_7".to_owned()),
            send_generation: Some(1),
            send_source: Some(source_key()),
            send_source_sid: Some(20),
            send_source_focus: None,
            send_source_focus_empty: false,
            operation_id: Some("op-3".to_owned()),
            select_target_output: None,
            select_target_workspace: None,
            select_native_binding: None,
            select_creation_operation: None,
            select_creation_logical: None,
            select_creation_output: None,
            select_baseline_sid: None,
            send_follow_view_operation: None,
        };
        assert!(
            ManagedSendTarget::from_claim(&claim).is_err(),
            "unbound trailing binds no owner target"
        );
        let mut controller = crate::workspace_send::LiveSendController::new();
        assert!(
            controller
                .submit(&claim, "display-a", 20, FakeManaged::new,)
                .is_err(),
            "unbound submit refuses before any post"
        );
        assert!(!controller.is_busy());
    }

    #[test]
    fn trailing_unbound_with_desire_is_blocked_on_creation_seam() {
        // Prerequisite blocker (not a settled refusal): the reply bound a
        // retained transfer plus an explicit creation desire but no native
        // binding, and the desire-gated native creation seam is unavailable
        // pending typed truthful receipts. The drain must refuse rather than
        // guess or invent a target — this test pins that refusal while the
        // seam is unavailable.
        let claim = ClaimedSelect {
            action: InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            },
            expected_sid: None,
            ui_generation: 0,
            topology_revision: Some(9),
            send_subject: Some("4242_7".to_owned()),
            send_generation: Some(1),
            send_source: Some(source_key()),
            send_source_sid: Some(20),
            send_source_focus: None,
            send_source_focus_empty: false,
            operation_id: Some("op-native-7".to_owned()),
            select_target_output: Some("macos-display-1".to_owned()),
            select_target_workspace: Some("ws-trailing".to_owned()),
            select_native_binding: None,
            select_creation_operation: Some("op-native-7".to_owned()),
            select_creation_logical: Some("ws-trailing".to_owned()),
            select_creation_output: Some("macos-display-1".to_owned()),
            select_baseline_sid: Some(20),
            send_follow_view_operation: Some("op-native-7-follow".to_owned()),
        };
        assert!(
            ManagedSendTarget::from_claim(&claim).is_err(),
            "unbound desire target binds no owner target until the creation seam lands"
        );
        let mut controller = crate::workspace_send::LiveSendController::new();
        assert!(
            controller
                .submit(&claim, "display-a", 20, FakeManaged::new,)
                .is_err(),
            "desire-only submit refuses before any post"
        );
        assert!(!controller.is_busy());
    }
}
