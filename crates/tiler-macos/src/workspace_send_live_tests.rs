#![cfg(test)]
//! Live-send production-boundary regression tests: the production evidence
//! assembly ([`assemble`](crate::workspace_send_managed::LiveManagedSendBackend)
//! over injectable [`SendNativeReaders`](crate::workspace_send_managed::SendNativeReaders)),
//! the async [`LiveSendController`](crate::workspace_send::LiveSendController),
//! nonpanic teardown, and the Authentic/Compatible settings gate. No live
//! touch: scripted reader fakes stand in for the OS on the effect owner,
//! and the test thread only submits nonblocking plus a bounded completion
//! poll. Numbered send-follow 0..9 plus stay 1..9 is product-armed;
//! trailing stay-0, relative sends, and outputs stay refused.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiler_core::directional::{OutputId, WorkspaceId};
use tiler_core::geometry::Rect;

use crate::desktop_shortcuts::{ShortcutEntry, desktop_symbolic_id};
use crate::runtime_core::EngineFactsRead;
use crate::runtime_core::EngineManagedFacts;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction, NativeContext, NativeDomain};
use crate::space_drag_transfer::DragStep;
use crate::workspace_select::SelectionSnapshot;
use crate::workspace_send::{LiveSendController, SendGates, send_drain_outcome_for};
use crate::workspace_send_effect::{
    BlindRoot, ManagedSendTarget, SendEffectHandoff, SendEffectOutcome, SendFreshEvidence,
    SendRequest,
};
use crate::workspace_send_managed::{
    ManagedSendBackend, ManagedSendThread, execute_managed_send, execute_managed_send_stay,
};
use crate::workspace_send_native::{
    AxSubjectState, GrabContext, LiveManagedSendBackend, SendNativeReaders, SendTopology,
    spawn_terminal_reaper,
};

const PID: u32 = 4242;
const WID: u32 = 7;
const WINDOW: &str = "4242_7";
// Foreign front for the post-select focus tests (the retained incident
// shape: the destination front stayed on Finder after the move+select).
const FINDER_PID: u32 = 1111;
const FINDER_WID: u32 = 99;
const GENERATION: u64 = 1;
const TARGET_SID: u64 = 3;
const REVISION: u64 = 9;
const OP: &str = "op-3";
const CG_CTRL: u64 = 0x0004_0000;
const POINT: (f64, f64) = (196.0, 108.0);

fn source_domain() -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-20".to_owned()),
    }
}

fn target_domain(space: u64, current: bool) -> NativeDomain {
    target_domain_named(space, current, "macos-display-1")
}

fn target_domain_named(space: u64, current: bool, display: &str) -> NativeDomain {
    NativeDomain {
        cg_id: 1,
        uuid: Some("uuid-1".to_owned()),
        output: OutputId(display.to_owned()),
        workspace: WorkspaceId(format!("space-{space}")),
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
            y: 0,
            w: 1440,
            h: 900,
        },
    }
}

fn topo(spaces: Vec<u64>, current: u64, revision: u64) -> SendTopology {
    SendTopology {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces,
            current,
        },
        revision,
        context: Arc::new(NativeContext {
            revision,
            domains: vec![
                target_domain(20, 20 == current),
                target_domain(3, 3 == current),
                target_domain(7, 7 == current),
            ],
            skipped: Vec::new(),
        }),
    }
}

fn base_topo() -> SendTopology {
    topo(vec![20, 3, 7], 20, REVISION)
}

fn frame() -> crate::ffi::AxFrame {
    crate::ffi::AxFrame {
        x: 100.0,
        y: 100.0,
        w: 480.0,
        h: 320.0,
    }
}

fn ax_ok() -> AxSubjectState {
    AxSubjectState {
        frame: Some(frame()),
        fullscreen: Some(false),
        minimized: Some(false),
    }
}

fn grab_ok() -> GrabContext {
    let bounds = crate::ffi::CgBounds {
        x: 100.0,
        y: 100.0,
        w: 480.0,
        h: 320.0,
    };
    GrabContext {
        owned: crate::ffi::CgWindow {
            wid: WID,
            pid: PID,
            layer: 0,
            onscreen: true,
            bounds,
            title: None,
        },
        all: vec![crate::ffi::CgWindowBasic {
            wid: WID,
            pid: PID,
            layer: 0,
            onscreen: true,
            bounds,
        }],
    }
}

fn engine_ok() -> EngineManagedFacts {
    EngineManagedFacts {
        generation: GENERATION,
        source: source_domain(),
        source_current: true,
        fullscreen: false,
        minimized: false,
        revision: REVISION,
    }
}

fn enabled_entry(desktop: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: true,
        keycode: crate::desktop_shortcuts::desktop_digit_keycode(desktop).unwrap(),
        modifiers: CG_CTRL,
    }
}

fn pinned_target() -> ManagedSendTarget {
    ManagedSendTarget::bind(
        PID,
        WID,
        GENERATION,
        source_domain(),
        TARGET_SID,
        REVISION,
        OP,
    )
    .expect("fixture target binds")
}

fn admitted_claim() -> ClaimedSelect {
    ClaimedSelect {
        action: InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        },
        expected_sid: Some(TARGET_SID),
        ui_generation: 0,
        topology_revision: Some(REVISION),
        send_subject: Some(WINDOW.to_owned()),
        send_generation: Some(GENERATION),
        send_source: Some(source_domain()),
        send_source_sid: Some(20),
        send_source_focus: None,
        send_source_focus_empty: false,
        operation_id: Some(OP.to_owned()),
        trailing_token: None,
    }
}

fn open_gates() -> SendGates {
    SendGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

/// Scripted native readers: every production evidence leg is scriptable,
/// every HID post is recorded into a shared log the test keeps. No live
/// touch.
#[derive(Debug, Default)]
struct SharedLog {
    mouse_posts: Vec<(DragStep, (f64, f64))>,
    key_posts: Vec<(u32, u64, bool)>,
}

struct FakeReaders {
    gates: SendGates,
    log: Arc<Mutex<SharedLog>>,
    topo_base: SendTopology,
    topo_script: VecDeque<SendTopology>,
    topo_err_at: Option<usize>,
    topo_calls: usize,
    displays: Result<usize, String>,
    holders: HashMap<u32, ShortcutEntry>,
    fail_holders: HashSet<u32>,
    spaces: HashMap<u32, Vec<u64>>,
    fail_spaces: HashSet<u32>,
    ax_base: AxSubjectState,
    ax_from_call: Option<(usize, AxSubjectState)>,
    ax_calls: usize,
    engine_base: Option<EngineManagedFacts>,
    engine_from_call: Option<(usize, Option<EngineManagedFacts>)>,
    engine_noreply: bool,
    engine_noreply_from: Option<usize>,
    engine_calls: usize,
    focus: Option<(u32, Option<u32>)>,
    agree: bool,
    select_calls: Vec<u64>,
    select_fail: Option<String>,
    return_source: Option<u64>,
    grab_base: Option<GrabContext>,
    ax_hit: Option<(u32, u32)>,
    ws_hit: Option<u32>,
    ax_details: HashMap<(i64, i64), crate::ffi::AxHitDetail>,
    fail_mouse: Option<DragStep>,
    fail_key_down: bool,
    fail_key_up_once: bool,
    fail_mouse_up: bool,
    block_mouse_dragged: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    block_key_down: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    block_key_up: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    block_topo_call: Option<(usize, mpsc::Sender<()>, mpsc::Receiver<()>)>,
    current: Option<u64>,
    effect_goal: u64,
    carried: bool,
    arrival_notes: usize,
    /// Fast bridged scripting: availability plus direct-move log. The
    /// membership effect lands on the destination while the view stays on
    /// the source until the destination select follows.
    bridged_enabled: bool,
    bridged_calls: Vec<(u64, u32)>,
    bridged_fail: Option<String>,
    bridged_goal: Option<u64>,
    /// Scripted membership reads (popped front per `subject_spaces` while
    /// non-empty). Models async completion landing late.
    spaces_script: VecDeque<Option<Vec<u64>>>,
    /// Sticky membership override (wins over the bridged-goal effect while
    /// set). Models a completion that never lands.
    spaces_sticky: Option<Vec<u64>>,
    /// Front-pin capability half of the scripted availability conjunction:
    /// the fake reports bridged only when BOTH the move dispatcher
    /// (`bridged_enabled`) and the front pin plus PSN mapping
    /// (`front_enabled`) resolve - mirroring the production conjunction.
    /// Clear exactly one half to preselect the drag before any effect.
    front_enabled: bool,
    /// Native command ordering log: dispatch pushes `dispatch`, front pin
    /// pushes `front`, select pushes `select`, membership reads push
    /// `membership`.
    order: Vec<String>,
    /// Destination front-pin scripting: call log plus explicit refusal.
    /// `front_gates_open` / `front_identity_ok` mirror the production
    /// pre-touch fences (fresh session gates, fresh CG owner join); when
    /// false the fake refuses before any (fake) SLS touch, like production.
    front_calls: Vec<(u64, u32, u32)>,
    front_fail: Option<String>,
    front_gates_open: bool,
    front_identity_ok: bool,
    /// Post-select focus override: while `None` the focus reads static;
    /// set to model the view landing on a foreign front after the select.
    focus_after_select: Option<(u32, Option<u32>)>,
}

impl FakeReaders {
    fn log(&self) -> Arc<Mutex<SharedLog>> {
        Arc::clone(&self.log)
    }

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
            log: Arc::new(Mutex::new(SharedLog::default())),
            topo_base: base_topo(),
            topo_script: VecDeque::new(),
            topo_err_at: None,
            topo_calls: 0,
            displays: Ok(1),
            holders,
            fail_holders: HashSet::new(),
            spaces: HashMap::from([(WID, vec![20])]),
            fail_spaces: HashSet::new(),
            ax_base: ax_ok(),
            ax_from_call: None,
            ax_calls: 0,
            engine_base: Some(engine_ok()),
            engine_from_call: None,
            engine_noreply: false,
            engine_noreply_from: None,
            engine_calls: 0,
            focus: Some((PID, Some(WID))),
            agree: true,
            select_calls: Vec::new(),
            select_fail: None,
            return_source: None,
            grab_base: Some(grab_ok()),
            ax_hit: Some((PID, WID)),
            ws_hit: Some(WID),
            ax_details: HashMap::new(),
            fail_mouse: None,
            fail_key_down: false,
            fail_key_up_once: false,
            fail_mouse_up: false,
            block_mouse_dragged: None,
            block_key_down: None,
            block_key_up: None,
            block_topo_call: None,
            current: Some(20),
            effect_goal: TARGET_SID,
            carried: false,
            arrival_notes: 0,
            bridged_enabled: false,
            bridged_calls: Vec::new(),
            bridged_fail: None,
            bridged_goal: None,
            front_calls: Vec::new(),
            front_fail: None,
            front_gates_open: true,
            front_identity_ok: true,
            focus_after_select: None,
            spaces_script: VecDeque::new(),
            spaces_sticky: None,
            front_enabled: true,
            order: Vec::new(),
        }
    }

    fn fresh_topology_inner(&mut self) -> Result<SendTopology, String> {
        let call = self.topo_calls;
        self.topo_calls += 1;
        if let Some((at, started, release)) = self.block_topo_call.take() {
            if call == at {
                let _ = started.send(());
                match release.recv_timeout(Duration::from_secs(10)) {
                    Ok(()) => {}
                    Err(_) => return Err("injected topology block timed out".to_owned()),
                }
            } else {
                self.block_topo_call = Some((at, started, release));
            }
        }
        if self.topo_err_at == Some(call) {
            return Err("injected topology unreadable".to_owned());
        }
        let mut topo = if let Some(topo) = self.topo_script.pop_front() {
            topo
        } else {
            self.topo_base.clone()
        };
        if self.carried && self.return_source.is_none() {
            // The switch fired on chord-down, so the live row current sits
            // on the goal until the return select moves it back.
            topo.row.current = self.effect_goal;
        }
        Ok(topo)
    }

    fn block_on(pair: &mut Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>, what: &str) {
        if let Some((started, release)) = pair.take() {
            let _ = started.send(());
            match release.recv_timeout(Duration::from_secs(10)) {
                Ok(()) => {}
                Err(_) => panic!("injected {what} block timed out"),
            }
        }
    }
}

impl SendNativeReaders for FakeReaders {
    fn read_gates(&mut self) -> SendGates {
        self.gates
    }
    fn fresh_topology(&mut self) -> Result<SendTopology, String> {
        self.fresh_topology_inner()
    }
    fn display_count(&mut self) -> Result<usize, String> {
        self.displays.clone()
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        if self.fail_holders.contains(&symbolic_id) {
            return Err(format!("injected holder read refusal for {symbolic_id}"));
        }
        self.holders.get(&symbolic_id).copied().ok_or_else(|| {
            format!("fake holder {symbolic_id} unresolvable; refusing rather than guessing")
        })
    }
    fn subject_spaces(&mut self, wid: u32) -> Result<Vec<u64>, String> {
        if self.fail_spaces.contains(&wid) {
            return Err(format!("injected membership unreadable for wid {wid}"));
        }
        self.order.push("membership".to_owned());
        if let Some(next) = self.spaces_script.pop_front() {
            return next.ok_or_else(|| {
                format!("fake spaces for wid {wid} unscripted; refusing rather than guessing")
            });
        }
        if let Some(sticky) = self.spaces_sticky.clone() {
            return Ok(sticky);
        }
        if let Some(goal) = self.bridged_goal {
            return Ok(vec![goal]);
        }
        if self.carried {
            return Ok(vec![self.effect_goal]);
        }
        self.spaces.get(&wid).cloned().ok_or_else(|| {
            format!("fake spaces for wid {wid} unscripted; refusing rather than guessing")
        })
    }
    fn subject_ax(&mut self, _pid: u32, _wid: u32) -> AxSubjectState {
        let call = self.ax_calls;
        self.ax_calls += 1;
        if let Some((from, state)) = self.ax_from_call
            && call >= from
        {
            return state;
        }
        self.ax_base
    }
    fn engine_facts(&mut self, window: &str) -> EngineFactsRead {
        let call = self.engine_calls;
        self.engine_calls += 1;
        if let Some(from) = self.engine_noreply_from
            && call >= from
        {
            return EngineFactsRead::NoReply;
        }
        if self.engine_noreply {
            return EngineFactsRead::NoReply;
        }
        if let Some((from, facts)) = self.engine_from_call.clone()
            && call >= from
        {
            return match facts {
                Some(facts) => EngineFactsRead::Managed(facts),
                None => EngineFactsRead::Unmanaged,
            };
        }
        if window == WINDOW {
            return match self.engine_base.clone() {
                Some(facts) => EngineFactsRead::Managed(facts),
                None => EngineFactsRead::Unmanaged,
            };
        }
        EngineFactsRead::Unmanaged
    }
    fn selected_focus(&mut self) -> Option<(u32, Option<u32>)> {
        // Post-select override models the view landing on a foreign front
        // after the select; the proof (pre-select) always reads static.
        if !self.select_calls.is_empty()
            && let Some(after) = self.focus_after_select
        {
            return Some(after);
        }
        self.focus
    }
    fn focus_agrees(&mut self, _pid: u32, _wid: u32) -> bool {
        self.agree
    }
    fn grab_context(&mut self, pid: u32, wid: u32) -> Option<GrabContext> {
        if (pid, wid) == (PID, WID) {
            return self.grab_base.clone();
        }
        None
    }
    fn ax_hit_detail(&mut self, x: f64, y: f64) -> Option<crate::ffi::AxHitDetail> {
        let key = (x.round() as i64, y.round() as i64);
        if let Some(detail) = self.ax_details.get(&key) {
            return Some(detail.clone());
        }
        self.ax_hit.map(|owner| crate::ffi::AxHitDetail {
            owner,
            role: Some("AXWindow".to_owned()),
            subrole: Some("AXStandardWindow".to_owned()),
        })
    }
    fn ws_hit_wid(&mut self, _x: f64, _y: f64) -> Option<u32> {
        self.ws_hit
    }
    fn hid_mouse(&mut self, step: DragStep, point: (f64, f64)) -> Result<(), String> {
        if step == DragStep::MouseDragged {
            Self::block_on(&mut self.block_mouse_dragged, "dragged");
        }
        if self.fail_mouse == Some(step) {
            self.fail_mouse = None;
            return Err(format!("injected {step:?} refusal"));
        }
        if step == DragStep::MouseUp && self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        self.log
            .lock()
            .expect("post log")
            .mouse_posts
            .push((step, point));
        Ok(())
    }
    fn hid_key(&mut self, keycode: u32, flags: u64, down: bool) -> Result<(), String> {
        if down {
            Self::block_on(&mut self.block_key_down, "chord-down");
            if self.fail_key_down {
                return Err("injected chord-down refusal".to_owned());
            }
            // Scripted OS effect: the switch fires on chord-down, so later
            // membership/current readback names the goal.
            self.carried = true;
        } else {
            Self::block_on(&mut self.block_key_up, "chord-up");
            if self.fail_key_up_once {
                self.fail_key_up_once = false;
                return Err("injected chord-up refusal".to_owned());
            }
        }
        self.log
            .lock()
            .expect("post log")
            .key_posts
            .push((keycode, flags, down));
        Ok(())
    }
    fn read_current(&mut self) -> Option<u64> {
        if let Some(source) = self.return_source {
            return Some(source);
        }
        if self.carried {
            return Some(self.effect_goal);
        }
        self.current
    }
    fn note_verified_arrival(&mut self) {
        self.arrival_notes += 1;
    }
    fn bridged_available(&mut self) -> bool {
        self.bridged_enabled && self.front_enabled
    }
    fn bridged_move_window(&mut self, sid: u64, wid: u32) -> Result<(), String> {
        self.bridged_calls.push((sid, wid));
        self.order.push("dispatch".to_owned());
        if let Some(reason) = self.bridged_fail.clone() {
            return Err(reason);
        }
        // Inactive-destination move: membership names the goal while the
        // view stays on the source until the destination select follows.
        self.bridged_goal = Some(sid);
        Ok(())
    }
    fn set_destination_front(&mut self, sid: u64, pid: u32, wid: u32) -> Result<(), String> {
        self.front_calls.push((sid, pid, wid));
        self.order.push("front".to_owned());
        if let Some(reason) = self.front_fail.clone() {
            return Err(reason);
        }
        // Scriptable mirrors of the production pre-touch fences: a locked
        // session or a disagreeing CG join refuses before any SLS touch.
        if !self.front_gates_open {
            return Err(
                "injected front-gate refusal: session not admissible at front touch".to_owned(),
            );
        }
        if !self.front_identity_ok {
            return Err(
                "injected front-identity refusal: CG join disagrees at front touch".to_owned(),
            );
        }
        Ok(())
    }
    fn select_space(&mut self, sid: u64) -> Result<(), String> {
        // Ordinary select (destination follow or stay return): the mover
        // stays on the goal, the view moves to the selected SID.
        self.select_calls.push(sid);
        self.order.push("select".to_owned());
        if let Some(reason) = self.select_fail.clone() {
            return Err(reason);
        }
        self.return_source = Some(sid);
        Ok(())
    }
}

fn handoff() -> Arc<SendEffectHandoff> {
    Arc::new(SendEffectHandoff::new())
}

fn mouse_count(posts: &[(DragStep, (f64, f64))], step: DragStep) -> usize {
    posts.iter().filter(|(s, _)| *s == step).count()
}

fn key_count(posts: &[(u32, u64, bool)], down: bool) -> usize {
    posts.iter().filter(|(_, _, d)| *d == down).count()
}

// -- Production evidence assembly --------------------------------------------

#[test]
fn live_applied_path_posts_exact_point_chord_and_reads_back() {
    let target = pinned_target();
    let lane = handoff();
    let readers = FakeReaders::new();
    let log = readers.log();
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("applied carries the leg readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, TARGET_SID);
    assert!(!leg.pending_recovered);
    // Exact point/chord routing: every mouse post at the selected grab
    // point, every key edge the resolved Desktop-2 chord.
    let log = log.lock().expect("post log");
    for (_, point) in &log.mouse_posts {
        assert_eq!(*point, POINT);
    }
    for (keycode, flags, _) in &log.key_posts {
        assert_eq!(
            (*keycode, *flags),
            (
                crate::desktop_shortcuts::desktop_digit_keycode(2).unwrap(),
                CG_CTRL
            )
        );
    }
    assert_eq!(mouse_count(&log.mouse_posts, DragStep::MouseDown), 1);
    assert_eq!(mouse_count(&log.mouse_posts, DragStep::MouseUp), 1);
    assert_eq!(key_count(&log.key_posts, true), 1);
    assert_eq!(key_count(&log.key_posts, false), 1);
    assert!(backend.counts().balanced());
}

#[test]
fn verified_arrival_requests_exactly_one_owner_reread() {
    // The effect owner proves arrival by exact native readback (membership
    // plus current both name the goal) and requests the tile owners'
    // reread there - once per arrival, even though the revision never
    // moved. A stable repeat poll replays nothing.
    let target = pinned_target();
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert_eq!(leg.map(|leg| leg.membership), Some(vec![TARGET_SID]));
    assert_eq!(backend.test_readers().arrival_notes, 1);
    // Stable settle polls stay quiet: the change-only readback gate never
    // re-fires the request without a new arrival.
    let _ = backend.read_membership();
    let _ = backend.read_current();
    let _ = backend.read_membership();
    let _ = backend.read_current();
    assert_eq!(backend.test_readers().arrival_notes, 1);
}

#[test]
fn no_arrival_requests_no_owner_reread() {
    // No proven subject, no arrival: the current stays off-goal and the
    // membership is unknown, so the effect owner requests nothing.
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    assert_eq!(backend.read_current(), Some(20));
    assert_eq!(backend.test_readers().arrival_notes, 0);
}

#[test]
fn live_proof_refuses_pure_focus_swap_with_zero_posts() {
    // The key managed-scope verdict over production assembly: the subject
    // is still managed, readable, same lifetime and domain — but focus
    // names another window. Never substitute the focused window.
    let target = pinned_target();
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.focus = Some((PID, Some(9)));
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    let counts = backend.counts();
    assert_eq!(counts.key_down, 0, "no chord posted on a focus swap");
    assert_eq!(counts.mouse_down, 0, "no grab posted on a focus swap");
    assert!(counts.balanced());
}

#[test]
fn live_engine_refuses_relaunched_lifetime_and_removed_subject() {
    let target = pinned_target();
    // Relaunched lifetime under the same PID: the engine generation no
    // longer matches the admission pin — never grab the new window.
    let mut relaunched = engine_ok();
    relaunched.generation = GENERATION + 1;
    for (engine, what) in [
        (Some(relaunched), "relaunched lifetime"),
        (None, "engine-removed subject"),
        (None, "engine-unknown subject"),
    ] {
        let lane = handoff();
        let mut backend = LiveManagedSendBackend::new({
            let mut readers = FakeReaders::new();
            readers.engine_base = engine.clone();
            readers
        });
        let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert!(leg.is_none(), "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
        assert_eq!(backend.counts().mouse_down, 0, "{what}: no grab posted");
    }
}

#[test]
fn live_engine_fullscreen_and_minimized_forbid_despite_healthy_os() {
    // Engine authority over a stale-healthy OS: retained fullscreen or a
    // classifier-excluded minimized lifetime refuses even when the fresh
    // AX legs still read ordinary and visible.
    let target = pinned_target();
    for (mutate, what) in [
        (
            (|engine: &mut EngineManagedFacts| engine.fullscreen = true)
                as fn(&mut EngineManagedFacts),
            "engine fullscreen",
        ),
        (
            (|engine: &mut EngineManagedFacts| engine.minimized = true)
                as fn(&mut EngineManagedFacts),
            "engine minimized exclusion",
        ),
    ] {
        let mut engine = engine_ok();
        mutate(&mut engine);
        let lane = handoff();
        let mut backend = LiveManagedSendBackend::new({
            let mut readers = FakeReaders::new();
            readers.engine_base = Some(engine);
            readers
        });
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
    }
    // Fresh OS state supplements engine truth: a fullscreen or minimized
    // the engine has not observed yet still refuses (strict-or, never a
    // permission).
    for (mutate, what) in [
        (
            (|ax: &mut AxSubjectState| ax.fullscreen = Some(true)) as fn(&mut AxSubjectState),
            "fresh fullscreen",
        ),
        (
            (|ax: &mut AxSubjectState| ax.minimized = Some(true)) as fn(&mut AxSubjectState),
            "fresh minimized",
        ),
    ] {
        let mut ax = ax_ok();
        mutate(&mut ax);
        let lane = handoff();
        let mut backend = LiveManagedSendBackend::new({
            let mut readers = FakeReaders::new();
            readers.ax_base = ax;
            readers
        });
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
    }
}

#[test]
fn live_proof_refuses_frame_mismatch() {
    // AX frame disagreeing with the CG bounds: identity unproven at the
    // grab point — refuse with no new input.
    let target = pinned_target();
    let mut ax_frame = ax_ok();
    ax_frame.frame = Some(crate::ffi::AxFrame {
        x: 900.0,
        y: 900.0,
        w: 480.0,
        h: 320.0,
    });
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.ax_base = ax_frame;
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
}

#[test]
fn live_proof_refuses_unmanaged_and_unknown_membership() {
    let target = pinned_target();
    for (spaces, what) in [
        (vec![], "unknown membership"),
        (vec![20, 3], "sticky membership"),
        (vec![99], "other-Space membership"),
    ] {
        let lane = handoff();
        let mut backend = LiveManagedSendBackend::new({
            let mut readers = FakeReaders::new();
            readers.spaces.insert(WID, spaces);
            readers
        });
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
    }
    // Unreadable membership refuses exactly like a mismatch.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.fail_spaces.insert(WID);
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
}

#[test]
fn live_proof_refuses_unknown_topology_and_revision_drift() {
    let target = pinned_target();
    // Unreadable topology at the snapshot read: snapshot-stage refusal
    // (no blind root is assembled — proof never runs).
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.topo_err_at = Some(0);
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    // Native fingerprint that no longer joins the engine revision refuses
    // as unknown — never guessed into coherence.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.topo_base = topo(vec![20, 3, 7], 20, REVISION + 1);
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts().key_down, 0, "no chord posted on drift");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted on drift");
    // Engine revision moved past admission while native stayed joined:
    // stale lifetime refuses with zero posts.
    let mut drifted = engine_ok();
    drifted.revision = REVISION + 1;
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.engine_base = Some(drifted);
        readers.topo_base = topo(vec![20, 3, 7], 20, REVISION + 1);
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts().key_down, 0, "no chord posted on drift");
}

#[test]
fn live_proof_refuses_dual_hit_disagreement() {
    // Both positional hits must join the exact target at the grab point:
    // an AX hit on the target with a foreign WindowServer hit refuses.
    let target = pinned_target();
    for (ax_hit, ws_hit, what) in [
        (Some((PID, WID)), Some(9), "foreign WindowServer hit"),
        (Some((PID, 9)), Some(WID), "foreign AX hit"),
        (None, Some(WID), "unknown AX hit"),
        (Some((PID, WID)), None, "unknown WindowServer hit"),
    ] {
        let lane = handoff();
        let mut backend = LiveManagedSendBackend::new({
            let mut readers = FakeReaders::new();
            readers.ax_hit = ax_hit;
            readers.ws_hit = ws_hit;
            readers
        });
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
    }
}

#[test]
fn live_proof_skips_control_text_hits_for_plain_window_point() {
    // Producer-shaped TextEdit titlebar: the first fractions hit filename
    // controls that still join the target owner, while the later fractions
    // hit the plain window. The grab must skip the control/text labels in
    // order and settle on the first plain `AXWindow`/`AXStandardWindow`
    // point — never the first joining owner.
    use crate::space_drag_transfer::candidate_grab_points;
    let points = candidate_grab_points(frame()).expect("fixture frame admits 5 points");
    assert_eq!(points.len(), 5);
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    let mut details = HashMap::new();
    let labels: [(Option<&str>, Option<&str>); 5] = [
        (Some("AXStaticText"), None),
        (Some("AXStaticText"), None),
        (Some("AXMenuButton"), None),
        (Some("AXWindow"), Some("AXStandardWindow")),
        (Some("AXWindow"), Some("AXStandardWindow")),
    ];
    for (point, (role, subrole)) in points.iter().zip(labels) {
        details.insert(
            (point.0.round() as i64, point.1.round() as i64),
            crate::ffi::AxHitDetail {
                owner: (PID, WID),
                role: role.map(str::to_owned),
                subrole: subrole.map(str::to_owned),
            },
        );
    }
    readers.ax_details = details;
    readers.ws_hit = Some(WID);
    let log = readers.log();
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("applied carries the leg readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, TARGET_SID);
    // First plain point is the fourth fraction (.65), not the first owner
    // join (.2). The production leg pins that point per edge, so every
    // actual mouse post sits there (not just the later evidence re-read).
    let want = points[3];
    let evidence = backend.fresh_evidence(&target);
    assert_eq!(evidence.grab_point, Some(want));
    let log = log.lock().expect("post log");
    assert!(!log.mouse_posts.is_empty(), "applied posts the grab");
    for (_, point) in &log.mouse_posts {
        assert_eq!(*point, want);
    }
    assert!(backend.counts().balanced(), "releases stay balanced");
}

#[test]
fn live_proof_refuses_when_no_plain_window_point_exists() {
    // No plain candidate: every fraction hits a control/text/unknown label
    // while still joining the target owner. The proof must refuse with zero
    // actuation — never fall back to the first joining owner.
    use crate::space_drag_transfer::candidate_grab_points;
    let points = candidate_grab_points(frame()).expect("fixture frame admits 5 points");
    for (labels, what) in [
        (
            [
                (Some("AXStaticText"), None),
                (Some("AXStaticText"), None),
                (Some("AXMenuButton"), None),
                (Some("AXStaticText"), None),
                (Some("AXMenuButton"), None),
            ],
            "all control/text",
        ),
        (
            [
                (None, None),
                (None, None),
                (None, None),
                (None, None),
                (None, None),
            ],
            "all unknown labels",
        ),
    ] {
        let target = pinned_target();
        let lane = handoff();
        let mut readers = FakeReaders::new();
        let mut details = HashMap::new();
        for (point, (role, subrole)) in points.iter().zip(labels) {
            let role_owned: Option<String> = role.map(str::to_owned);
            let subrole_owned: Option<String> = subrole.map(str::to_owned);
            details.insert(
                (point.0.round() as i64, point.1.round() as i64),
                crate::ffi::AxHitDetail {
                    owner: (PID, WID),
                    role: role_owned,
                    subrole: subrole_owned,
                },
            );
        }
        readers.ax_details = details;
        readers.ws_hit = Some(WID);
        let log = readers.log();
        let mut backend = LiveManagedSendBackend::new(readers);
        let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert!(leg.is_none(), "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
        assert_eq!(backend.counts().mouse_down, 0, "{what}: no grab posted");
        let log = log.lock().expect("post log");
        assert!(log.mouse_posts.is_empty(), "{what}: no mouse edge posted");
        assert!(log.key_posts.is_empty(), "{what}: no key edge posted");
        assert!(
            backend.counts().balanced(),
            "{what}: releases stay balanced"
        );
    }
}

#[test]
fn live_proof_still_refuses_plain_offtarget_and_ws_disagreement() {
    // Plain labels do not weaken the existing joins: a foreign AX owner or
    // a foreign WindowServer hit at the plain point still refuses.
    use crate::space_drag_transfer::candidate_grab_points;
    let points = candidate_grab_points(frame()).expect("fixture frame admits 5 points");
    let target = pinned_target();
    for (ax_owner, ws_hit, what) in [
        ((PID, 9), Some(WID), "plain label foreign AX owner"),
        ((PID, WID), Some(9), "plain label foreign WindowServer hit"),
    ] {
        let lane = handoff();
        let mut readers = FakeReaders::new();
        let mut details = HashMap::new();
        for point in &points {
            details.insert(
                (point.0.round() as i64, point.1.round() as i64),
                crate::ffi::AxHitDetail {
                    owner: ax_owner,
                    role: Some("AXWindow".to_owned()),
                    subrole: Some("AXStandardWindow".to_owned()),
                },
            );
        }
        readers.ax_details = details;
        readers.ax_hit = Some(ax_owner);
        readers.ws_hit = ws_hit;
        let log = readers.log();
        let mut backend = LiveManagedSendBackend::new(readers);
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{what}");
        assert_eq!(backend.counts().key_down, 0, "{what}: no chord posted");
        assert_eq!(backend.counts().mouse_down, 0, "{what}: no grab posted");
        let log = log.lock().expect("post log");
        assert!(log.mouse_posts.is_empty(), "{what}: no mouse edge posted");
        assert!(log.key_posts.is_empty(), "{what}: no key edge posted");
    }
}

#[test]
fn live_anchor_refuses_midleg_ordinal_reorder() {
    // Snapshot/ordinal coherence: the row reorders between the drain row
    // read and the leg setup, so the Desktop-2 chord would name the wrong
    // Space. The leg refuses before any post.
    let target = pinned_target();
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers
            .topo_script
            .push_back(topo(vec![3, 20, 7], 20, REVISION));
        readers
    });
    // First fresh_topology call (snapshot) uses the base row [20,3,7];
    // every later call uses the reordered row.
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts().key_down, 0, "no chord posted on reorder");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted on reorder");
}

#[test]
fn live_grab_drift_between_edges_refuses_before_chord() {
    // The first grab point pins the leg; a shifted fresh point on a later
    // edge refuses before the next post instead of drifting the held drag.
    let target = pinned_target();
    let lane = handoff();
    let mut moved = ax_ok();
    moved.frame = Some(crate::ffi::AxFrame {
        x: 300.0,
        y: 300.0,
        w: 480.0,
        h: 320.0,
    });
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.ax_from_call = Some((1, moved));
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        backend.counts().key_down,
        0,
        "drift before chord posts no chord"
    );
    assert!(backend.counts().balanced(), "releases stay balanced");
}

// -- Cancellation and release -----------------------------------------------

#[test]
fn live_stay_forward_plus_select_back_applies() {
    // Stay through the production backend seam: the forward drag applies,
    // the return fence sees the arrival current with unchanged display
    // identity and SID row, the ordinary select returns the view to the
    // source, and the readback proves membership exactly [target] with
    // current exactly source.
    let target = pinned_target();
    let lane = handoff();
    lane.submit(target.clone()).expect("idle submits");
    lane.take_for_owner().expect("owner takes");
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let (outcome, leg) =
        execute_managed_send_stay(&target, 20, Some("4242_7"), &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("stay applied carries exact return readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, 20);
    assert!(!leg.pending_recovered);
    assert!(backend.counts().balanced(), "stay releases balance");
}

#[test]
fn live_stay_select_back_refusal_holds() {
    // The ordinary return select refuses after the forward chord posted:
    // hold with the key post as evidence, never replayed.
    let target = pinned_target();
    let lane = handoff();
    lane.submit(target.clone()).expect("idle submits");
    lane.take_for_owner().expect("owner takes");
    let mut readers = FakeReaders::new();
    readers.select_fail = Some("injected select-back refusal".to_owned());
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, leg) =
        execute_managed_send_stay(&target, 20, Some("4242_7"), &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(leg.is_none());
    assert_eq!(backend.counts().key_down, 1, "forward chord posted");
    assert!(
        backend.counts().balanced(),
        "release-first on return failure"
    );
}

#[test]
fn live_cancel_before_effect_refuses_proven_with_zero_posts() {
    // Deterministic pre-effect stop: cancel lands after the owner take
    // but before execution starts. The entry gate refuses with zero posts:
    // proven pre-actuation, never a held input.
    let target = pinned_target();
    let lane = handoff();
    lane.submit(pinned_target())
        .expect("lane holds the request");
    let taken = lane.take_for_owner().expect("owner takes");
    assert_eq!(taken, SendRequest::Pinned(pinned_target()));
    assert!(lane.cancel(), "cancel flags the stop while inflight stands");
    assert!(lane.cancel_requested(), "owner observes the stop");
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    let counts = backend.counts();
    assert_eq!((counts.mouse_down, counts.mouse_up), (0, 0));
    assert_eq!((counts.key_down, counts.key_up), (0, 0));
}

#[test]
fn live_cancel_during_chord_post_cannot_unpost() {
    // A stop that lands inside an already-authorized chord-down post is
    // too late to prevent it: the post completes, releases still run, and
    // the verified arrival reports Applied honestly with balanced counts.
    // Cancellation refuses further down-edges — it never un-posts one.
    let target = pinned_target();
    let lane = handoff();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    lane.submit(pinned_target())
        .expect("lane holds the request");
    lane.take_for_owner().expect("owner takes");
    let mut readers = FakeReaders::new();
    readers.block_key_down = Some((started_tx, release_rx));
    let log = readers.log();
    let mut backend = LiveManagedSendBackend::new(readers);
    let lane_clone = Arc::clone(&lane);
    let handle =
        std::thread::spawn(move || execute_managed_send(&target, &mut backend, &lane_clone));
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("chord-down reached");
    assert!(lane.cancel(), "cancel flags the stop");
    let _ = release_tx.send(());
    let (outcome, _) = handle.join().expect("owner joins");
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let log = log.lock().expect("post log");
    assert_eq!(key_count(&log.key_posts, true), 1);
    assert_eq!(key_count(&log.key_posts, false), 1);
    assert_eq!(
        mouse_count(&log.mouse_posts, DragStep::MouseDown),
        mouse_count(&log.mouse_posts, DragStep::MouseUp),
        "releases stay balanced"
    );
}

#[test]
fn live_cancel_during_grab_releases_the_mouse() {
    // Stop between mouse-down and the dragged edge: the dragged post
    // refuses, the held mouse still releases exactly once, no chord posts.
    let target = pinned_target();
    let lane = handoff();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    lane.submit(pinned_target())
        .expect("lane holds the request");
    lane.take_for_owner().expect("owner takes");
    let mut readers = FakeReaders::new();
    readers.block_mouse_dragged = Some((started_tx, release_rx));
    let log = readers.log();
    let mut backend = LiveManagedSendBackend::new(readers);
    let lane_clone = Arc::clone(&lane);
    let handle =
        std::thread::spawn(move || execute_managed_send(&target, &mut backend, &lane_clone));
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("dragged edge reached");
    assert!(lane.cancel());
    let _ = release_tx.send(());
    let (outcome, _) = handle.join().expect("owner joins");
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    let log = log.lock().expect("post log");
    assert_eq!(mouse_count(&log.mouse_posts, DragStep::MouseDown), 1);
    assert_eq!(
        mouse_count(&log.mouse_posts, DragStep::MouseUp),
        1,
        "held mouse always releases"
    );
    assert_eq!(
        key_count(&log.key_posts, true),
        0,
        "no chord after a grab stop"
    );
}

#[test]
fn live_cancel_after_chord_never_skips_releases() {
    // Stop during key-up: release edges bypass the stop request and always
    // attempt, so the counts stay balanced and the leg settles honestly.
    let target = pinned_target();
    let lane = handoff();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    lane.submit(pinned_target())
        .expect("lane holds the request");
    lane.take_for_owner().expect("owner takes");
    let mut readers = FakeReaders::new();
    readers.block_key_up = Some((started_tx, release_rx));
    let log = readers.log();
    let mut backend = LiveManagedSendBackend::new(readers);
    let lane_clone = Arc::clone(&lane);
    let handle =
        std::thread::spawn(move || execute_managed_send(&target, &mut backend, &lane_clone));
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("chord-up reached");
    assert!(lane.cancel());
    let _ = release_tx.send(());
    let (outcome, _) = handle.join().expect("owner joins");
    // The chord completed before the stop landed: the verified arrival
    // still reports Applied (the stop only refuses further down-edges,
    // never release edges and never a completed effect).
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let log = log.lock().expect("post log");
    assert_eq!(
        mouse_count(&log.mouse_posts, DragStep::MouseDown),
        mouse_count(&log.mouse_posts, DragStep::MouseUp),
        "releases stay balanced"
    );
    assert_eq!(
        key_count(&log.key_posts, true),
        key_count(&log.key_posts, false),
        "releases stay balanced"
    );
}

#[test]
fn live_mouse_down_failure_still_releases_exactly_once() {
    // Grab-down refusal after a successful move: the release-first
    // executor still runs mouse-up. Nothing was held (Down never posted),
    // so the release is a truthful no-op and the counts stay balanced.
    let target = pinned_target();
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.fail_mouse = Some(DragStep::MouseDown);
        readers
    });
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    let counts = backend.counts();
    assert_eq!((counts.mouse_down, counts.mouse_up), (0, 0));
    assert_eq!(counts.key_down, 0);
}

// -- Async controller: arrival, completion, no replay ------------------------

fn poll_until(controller: &mut LiveSendController) -> Option<crate::workspace_send::SendSettle> {
    for _ in 0..100 {
        if let Some(settle) = controller.poll() {
            return Some(settle);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn live_controller_settles_applied_once_with_exact_identity() {
    let mut controller = LiveSendController::new();
    assert!(!controller.is_busy());
    controller
        .submit(&admitted_claim(), "display-a", 20, || {
            LiveManagedSendBackend::new(FakeReaders::new())
        })
        .expect("admitted claim submits");
    assert!(controller.is_busy());
    // Concurrent presses serialize/refuse instead of overlapping.
    assert!(
        controller
            .submit(&admitted_claim(), "display-a", 20, || {
                LiveManagedSendBackend::new(FakeReaders::new())
            })
            .is_err(),
        "no concurrent physical effect"
    );
    let settle = poll_until(&mut controller).expect("completion arrives");
    assert_eq!(settle.outcome, SendEffectOutcome::Applied);
    assert_eq!(settle.display, "display-a");
    assert_eq!(settle.from, 20);
    // The settled target pins the exact admitted identity: no focus
    // substitute, no relaunched lifetime, no moved source.
    assert_eq!((settle.target.pid(), settle.target.wid()), (PID, WID));
    assert_eq!(settle.target.window(), WINDOW);
    assert_eq!(settle.target.app_generation(), GENERATION);
    assert_eq!(settle.target.source(), &source_domain());
    assert_eq!(settle.target.target_sid(), TARGET_SID);
    assert_eq!(settle.target.revision(), REVISION);
    assert_eq!(settle.target.operation_id(), OP);
    // Honest arrival mapping for the tile: Applied settles the transfer,
    // and polling consumes — no replay.
    assert_eq!(
        send_drain_outcome_for(settle.outcome),
        crate::workspace_send::SendDrainOutcome::Applied
    );
    assert!(controller.poll().is_none(), "completion never replays");
    assert!(!controller.is_busy());
}

#[test]
fn live_controller_shutdown_settles_honestly_without_blocking() {
    // Idle shutdown settles nothing and spawns nothing.
    let mut controller = LiveSendController::new();
    assert!(controller.shutdown().is_none());
    // Inflight shutdown while the owner actuates: the dropped send holds
    // as possibly-executed (never replayed), and the unfinished exact
    // owner hands to the dedicated terminal reaper — never a join that
    // blocks shutdown, never a panic.
    let mut controller = LiveSendController::new();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    controller
        .submit(&admitted_claim(), "display-a", 20, move || {
            let mut readers = FakeReaders::new();
            readers.block_key_down = Some((started_tx, release_rx));
            LiveManagedSendBackend::new(readers)
        })
        .expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("owner reaches chord-down");
    let settle = controller.shutdown().expect("dropped inflight settles");
    let crate::workspace_send::ShutdownOutcome::Settle(settle) = settle else {
        panic!("pinned shutdown settles, never refuses trailing");
    };
    assert_eq!(
        settle.outcome,
        SendEffectOutcome::PossiblyExecuted,
        "an owner that may have actuated holds, never claims refusal"
    );
    // The blocked owner still finishes on its own once released, and the
    // reaper releases its lane exactly once afterward.
    let _ = release_tx.send(());
}

// -- Teardown ownership --------------------------------------------------------

#[test]
fn live_terminal_reaper_joins_off_thread_and_releases_once() {
    // Finished owner: shutdown reaps terminally with nothing to hand off
    // (a fresh owner needs one scheduling slice before it idles).
    let mut thread = ManagedSendThread::spawn(|| LiveManagedSendBackend::new(FakeReaders::new()));
    for _ in 0..100 {
        match thread.shutdown_nonblocking() {
            None => break,
            Some(t) => {
                thread = t;
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    // Live owner: shutdown hands the exact unfinished owner to the
    // dedicated terminal reaper — never a join that blocks shutdown,
    // never a panic, and the lane is never released from under it. The
    // reaper joins off-thread and releases exactly once after the terminal
    // completion.
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let target = pinned_target();
    let thread = ManagedSendThread::spawn(move || {
        let mut readers = FakeReaders::new();
        readers.block_topo_call = Some((0, started_tx, release_rx));
        LiveManagedSendBackend::new(readers)
    });
    let lane = thread.handoff();
    thread.submit(target.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("owner blocks in the effect");
    assert!(lane.is_busy());
    let retained = thread
        .shutdown_nonblocking()
        .expect("live owner handed off");
    assert!(lane.is_busy(), "lane unreleased under live owner");
    spawn_terminal_reaper(retained);
    let _ = release_tx.send(());
    let mut released = false;
    for _ in 0..200 {
        if !lane.is_busy() {
            released = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(released, "reaper joins and releases the lane");
    // The released lane settles nothing further: no completion was ever
    // staged for a non-effect, and nothing replays.
    assert!(lane.poll_completion_for(&target).is_none());
}

#[test]
fn live_thread_drop_never_panics_or_blocks() {
    // Dropping a live owner is a nonblocking backstop (stop-signal plus
    // wake; finished owners reap, live ones detach and self-complete),
    // never a panic — even without any prior shutdown call.
    let thread = ManagedSendThread::spawn(|| LiveManagedSendBackend::new(FakeReaders::new()));
    drop(thread);
    // And a dropped retainer with a live owner behaves the same.
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (_release_tx, release_rx) = mpsc::channel::<()>();
    let target = pinned_target();
    let thread = ManagedSendThread::spawn(move || {
        let mut readers = FakeReaders::new();
        readers.block_topo_call = Some((0, started_tx, release_rx));
        LiveManagedSendBackend::new(readers)
    });
    thread.submit(target).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("owner blocks in the effect");
    // Detaching through Drop stays a nonblocking backstop (stop-signal
    // plus wake; the owner was already stop-signalled here and exits on
    // its own through the release-first executor).
    drop(thread);
}

// -- Settings: Authentic enable-only, Compatible zero-touch -------------------

fn fake_store() -> crate::desktop_shortcuts::FakeStore {
    let mut store = crate::desktop_shortcuts::FakeStore::new();
    for desktop in 1..=9u32 {
        store = store.with_entry(
            desktop_symbolic_id(desktop).unwrap(),
            enabled_entry(desktop),
        );
    }
    store
}

fn preimage_path(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omnitiler-live-send-test-{}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("test preimage dir");
    dir.join("desktop-shortcuts-preimage.json")
}

#[test]
fn live_authentic_enables_desktop_holders_with_preimage() {
    let mut store = fake_store();
    let path = preimage_path("authentic");
    let _ = std::fs::remove_file(&path);
    let outcome = crate::workspace_send::ensure_holders_for_preset(
        crate::input_policy::InputPreset::Authentic,
        &mut store,
        &path,
    )
    .expect("authentic ensure runs")
    .expect("authentic ensures");
    assert_eq!(outcome.results.len(), 9);
    assert!(outcome.failures.is_empty());
    assert!(path.exists(), "durable preimage published");
    for desktop in 1..=9u32 {
        let entry = store
            .get(desktop_symbolic_id(desktop).unwrap())
            .expect("holder present");
        assert!(entry.enabled, "desktop {desktop} enabled");
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(crate::desktop_shortcuts::lock_path_for(&path));
}

#[test]
fn live_compatible_performs_zero_store_touch() {
    let mut store = fake_store();
    let path = preimage_path("compatible");
    let _ = std::fs::remove_file(&path);
    let outcome = crate::workspace_send::ensure_holders_for_preset(
        crate::input_policy::InputPreset::Compatible,
        &mut store,
        &path,
    )
    .expect("compatible ensure runs");
    assert!(outcome.is_none(), "compatible ensures nothing");
    assert_eq!(store.reads, 0, "compatible performs no holder reads");
    assert_eq!(store.writes, 0, "compatible performs no holder writes");
    assert!(!path.exists(), "compatible publishes no preimage");
    let _ = std::fs::remove_file(crate::desktop_shortcuts::lock_path_for(&path));
}

#[test]
fn live_evidence_shape_covers_every_proof_leg() {
    // The production assembly must populate every proof leg (a missing leg
    // is a compile error here, not a runtime unknown): this pins the
    // struct-shape contract between assembly and proof.
    let evidence = SendFreshEvidence {
        managed_current: Some(true),
        fullscreen: Some(false),
        minimized: Some(false),
        live_generation: Some(GENERATION),
        live_revision: Some(REVISION),
        live_source: Some(source_domain()),
        source_current: Some(true),
        focused_window: Some(WINDOW.to_owned()),
        focus_agrees: Some(true),
        geometry_ok: Some(true),
        grab_point: Some(POINT),
        ax_hit: Some(WINDOW.to_owned()),
        ws_hit: Some(WINDOW.to_owned()),
    };
    assert!(crate::workspace_send_effect::prove_managed_send(&pinned_target(), &evidence).is_ok());
}

// -- Producer pathway: hotkey -> engine admission -> effect -> feedback ----

#[test]
fn live_hotkey_to_feedback_pathway_with_injected_readers() {
    // Producer-shaped chain with no live touch: the armed product chord
    // for send-2 (Ctrl+Shift+digit under Ctrl) admits engine-side, binds
    // the managed target from the claim, actuates once through scripted
    // native readers on the dedicated owner, and maps to transfer
    // feedback for the world route. Unsupported shapes still refuse
    // admission and engine dispatch.
    use crate::config::ProbeModifier;
    use crate::workspace_input::{effective_flags_for, workspace_unsupported_outcome};
    use crate::workspace_send::{attempt_outcome_for, feedback_kind_for, send_drain_outcome_for};
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let binding = crate::workspace_send::bindings()
        .into_iter()
        .find(|b| {
            b.action
                == InputSemanticAction::WorkspaceSend {
                    index: 2,
                    follow: true,
                }
        })
        .expect("send-2 armed in product");
    assert_eq!(
        effective_flags_for(ProbeModifier::Ctrl, &binding),
        CTRL | SHIFT,
        "armed hotkey is Ctrl+Shift+digit"
    );
    assert_eq!(workspace_unsupported_outcome(&binding.action), None);
    let claim = ClaimedSelect::tap(binding.action.clone());
    assert!(
        crate::runtime_thread::admit_send_claim(true, false, false, claim).is_ok(),
        "engine admits the armed hotkey claim"
    );
    // Engine-admitted claim (pins as `handle_send_action` does) submits to
    // the dedicated owner over injected readers and settles exactly once.
    let mut controller = LiveSendController::new();
    controller
        .submit(&admitted_claim(), "display-a", 20, || {
            LiveManagedSendBackend::new(FakeReaders::new())
        })
        .expect("admitted claim submits");
    let settle = poll_until(&mut controller).expect("completion arrives");
    assert_eq!(settle.outcome, SendEffectOutcome::Applied);
    let drain = send_drain_outcome_for(settle.outcome);
    assert_eq!(
        drain,
        crate::workspace_send::SendDrainOutcome::Applied,
        "effect completion maps to the drain outcome"
    );
    assert_eq!(
        feedback_kind_for(&binding.action),
        tiler_core::world_report::AttemptKind::Transfer,
        "sends settle transfers, never views"
    );
    assert_eq!(
        attempt_outcome_for(drain),
        tiler_core::world_report::AttemptOutcome::Applied
    );
    assert!(controller.poll().is_none(), "completion never replays");
    // Numbered stay-2 dispatches too (armed, bindable unbound): no
    // unsupported outcome, admitted engine-side. Trailing send-0 follow
    // dispatches the same way (armed through the capability gate).
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: false,
        },
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
    ] {
        assert_eq!(workspace_unsupported_outcome(&action), None, "{action:?}");
        assert!(
            crate::runtime_thread::admit_send_claim(
                true,
                false,
                false,
                ClaimedSelect::tap(action.clone())
            )
            .is_ok(),
            "{action:?} admitted"
        );
    }
    // Unsupported forms still refuse admission and engine dispatch
    // (trailing stay-0, relative sends/stays, outputs).
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        InputSemanticAction::WorkspaceOutput {
            direction: crate::runtime_types::InputDirection::Left,
            follow: true,
        },
    ] {
        assert!(
            workspace_unsupported_outcome(&action).is_some(),
            "{action:?}"
        );
        assert_eq!(
            crate::runtime_thread::admit_send_claim(
                true,
                false,
                false,
                ClaimedSelect::tap(action.clone())
            ),
            Err("refused-send-unarmed"),
            "{action:?}"
        );
    }
}

#[test]
fn live_blind_root_discriminates_engine_topology_membership() {
    // One live inverse refusal (`refused-send-unknown-managed`) collapses
    // four unreadable roots; the backend must name exactly which one
    // collapsed, with zero native posts at setup in every case.
    let target = pinned_target();
    // Proven engine unmanaged while topology and membership stay healthy:
    // reports engine-facts-unmanaged — never mislabeled as topology, never
    // confused with an unproven query.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.engine_base = None;
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::EngineFactsUnmanaged));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
    // Unproven engine read (bound elapsed / inbox closed / engine down)
    // while topology and membership stay healthy: reports
    // engine-facts-noreply — never mislabeled as proven unmanaged.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.engine_noreply = true;
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::EngineFactsNoReply));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
    // Topology unreadable at the proof read (the snapshot read at call 0
    // stays healthy): reports topology.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.topo_err_at = Some(1);
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::Topology));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
    // Membership unreadable while engine and topology stay healthy:
    // reports membership.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.fail_spaces.insert(WID);
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::Membership));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
    // Fully readable evidence assembles with no blind root.
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let evidence = backend.fresh_evidence(&target);
    assert_eq!(evidence.managed_current, Some(true));
    assert_eq!(backend.blind_root(), None);
}

#[test]
fn live_inverse_roundtrip_discriminates_noreply_from_unmanaged() {
    // Producer-shaped 1->2->1 admitted/effect-query sequence at the effect
    // layer: the same admission-pinned target first applies with fresh
    // engine authority, then the inverse leg refuses — and the refusal names
    // whether the fresh engine read was proven-absent or unproven. Both
    // refuse with zero posts and never substitute the admission pin.
    let target = pinned_target();
    // Outward leg 1->2 with healthy engine authority: applies.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(leg.is_some());
    // Inverse leg with an unproven engine read (bound elapsed / engine
    // down): refuses as noreply, never mislabeled unmanaged.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.engine_noreply = true;
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::EngineFactsNoReply));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
    // Inverse leg with a proven engine-absent reply (removed / unhomed):
    // refuses as unmanaged, never mislabeled noreply.
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new({
        let mut readers = FakeReaders::new();
        readers.engine_base = None;
        readers
    });
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.blind_root(), Some(BlindRoot::EngineFactsUnmanaged));
    assert_eq!(backend.counts().key_down, 0, "no chord posted");
    assert_eq!(backend.counts().mouse_down, 0, "no grab posted");
}

fn admitted_stay_claim() -> ClaimedSelect {
    ClaimedSelect {
        action: InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: false,
        },
        expected_sid: Some(TARGET_SID),
        ui_generation: 0,
        topology_revision: Some(REVISION),
        send_subject: Some(WINDOW.to_owned()),
        send_generation: Some(GENERATION),
        send_source: Some(source_domain()),
        send_source_sid: Some(20),
        send_source_focus: None,
        send_source_focus_empty: false,
        operation_id: Some(OP.to_owned()),
        trailing_token: None,
    }
}

#[test]
fn live_controller_refuses_stay_claim_without_mru_query() {
    // Honest unarmed state: the stay claim carries an unknown source-focus
    // fallback (no core-proven pin), so `from_claim` refuses rather than
    // pinning the subject - the subject is on the target after the drag.
    // Nothing spawns, nothing posts, the lane stays idle.
    let mut controller = LiveSendController::new();
    let err = controller
        .submit(&admitted_stay_claim(), "display-a", 20, || {
            LiveManagedSendBackend::new(FakeReaders::new())
        })
        .expect_err("stay claim refuses without MRU query");
    assert!(err.contains("source focus"), "{err}");
    assert!(!err.contains(WINDOW), "refusal carries no window identity");
    assert!(!controller.is_busy());
}

#[test]
fn stay_return_display_join_runs_off_main_without_appkit() {
    // Effect-thread regression for the stay return selector: it runs on
    // the managed-send effect thread (never the main thread). The old
    // AppKit acquisition seam (`display_cgid_of` over
    // `ns_display_snapshot`) must fail on a spawned thread for the actual
    // thread-affinity reason, while the pinned plain-data join succeeds
    // over the same scripted snapshot. Zero effects allowed: no budget
    // spend, no press, no HID post, no AX action - only the read-only
    // join acquisition on both seams, then pure refusal checks.
    use crate::mc_probe::TopologySnapshot;
    let rows = vec![crate::ffi::SlsDisplayRow {
        identifier: Some("uuid-9".to_owned()),
        spaces: vec![20, 3],
        current: Some(20),
    }];
    let snapshot = TopologySnapshot::from_rows(&rows);
    let joined = std::thread::Builder::new()
        .name("stay-return-join-regression".to_owned())
        .spawn(move || {
            // Old seam: the AppKit display snapshot acquisition the
            // pre-fix selector reached through `display_cgid_of`.
            let old = crate::ffi::ns_display_snapshot();
            // New seam: the pinned plain-data join (cgid/uuid as
            // published from the main thread) over the scripted snapshot.
            let ok = crate::mc_lifecycle::live::display_cgid_of_pinned(&snapshot, 0, 7, "uuid-9");
            let wrong_uuid =
                crate::mc_lifecycle::live::display_cgid_of_pinned(&snapshot, 0, 7, "uuid-other");
            let zero_cgid =
                crate::mc_lifecycle::live::display_cgid_of_pinned(&snapshot, 0, 0, "uuid-9");
            let unknown_display =
                crate::mc_lifecycle::live::display_cgid_of_pinned(&snapshot, 4, 7, "uuid-9");
            (old, ok, wrong_uuid, zero_cgid, unknown_display)
        })
        .expect("join regression thread spawns")
        .join()
        .expect("join regression thread finishes");
    let (old, ok, wrong_uuid, zero_cgid, unknown_display) = joined;
    // Old path fails off-main for the thread-affinity reason (on macOS
    // the marker names the main thread; off-mac the stub refuses).
    let old_err = old.expect_err("AppKit acquisition fails off the main thread");
    #[cfg(target_os = "macos")]
    assert!(
        old_err.contains("main thread"),
        "old path fails for thread affinity, got: {old_err}"
    );
    #[cfg(not(target_os = "macos"))]
    assert!(
        !old_err.is_empty(),
        "old path fails closed off-mac, got: {old_err}"
    );
    // Pinned join succeeds with the exact (cgid, Space count).
    assert_eq!(ok, Ok((7, 2)));
    // A stale publication (UUID no longer naming the row), a zero CG ID,
    // and an unknown display all refuse without guessing.
    let wrong_err = wrong_uuid.expect_err("stale pinned UUID refuses");
    assert!(
        wrong_err.to_string().contains("pinned display UUID"),
        "stale pin names its leg, got: {wrong_err}"
    );
    assert!(
        zero_cgid.is_err(),
        "zero CG ID refuses without an exact mapping"
    );
    assert!(
        unknown_display.is_err(),
        "unknown display refuses without guessing"
    );
}

#[test]
fn off_main_send_backend_never_touches_main_only_display_reads() {
    // The effect-owner backend (evidence, trailing preparation, stay
    // return) runs off the main thread, where the AppKit display snapshot
    // fails closed. Pin the rule at the source: any `ns_display_snapshot`
    // or unpinned `display_cgid_of(` call in this module would block or
    // fail the owner, so it fails this test first. The pinned join
    // (`display_cgid_of_pinned`) is the only display join allowed here.
    let source = include_str!("workspace_send_native.rs");
    assert!(
        !source.contains("ns_display_snapshot"),
        "off-main backend must not read the AppKit display snapshot"
    );
    assert!(
        !source.contains("display_cgid_of(&"),
        "off-main backend must join displays through the pinned legs only"
    );
    assert!(
        source.contains("mc_add_space_pinned(") && source.contains("pinned_cgid"),
        "the pinned (cgid, UUID) legs stay the off-main display route"
    );
}

// -- Fast bridged follow over the production assembly (offline) --------------

fn fast_live_backend() -> LiveManagedSendBackend<FakeReaders> {
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    LiveManagedSendBackend::new(readers)
}

#[test]
fn live_bridged_fast_posts_no_hid_and_requests_one_reread() {
    let target = pinned_target();
    let lane = handoff();
    let mut backend = fast_live_backend();
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("fast applied carries readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, TARGET_SID);
    let readers = backend.test_readers();
    assert_eq!(readers.bridged_calls, vec![(TARGET_SID, WID)]);
    assert_eq!(readers.select_calls, vec![TARGET_SID]);
    let log = readers.log.lock().expect("post log");
    assert!(log.mouse_posts.is_empty(), "fast route posts no mouse");
    assert!(log.key_posts.is_empty(), "fast route posts no key");
    assert_eq!(
        readers.arrival_notes, 1,
        "verified arrival rereads even at same revision"
    );
}

#[test]
fn live_bridged_unavailable_stays_on_drag_with_posts() {
    let target = pinned_target();
    let lane = handoff();
    let mut backend = LiveManagedSendBackend::new(FakeReaders::new());
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(backend.test_readers().bridged_calls.is_empty());
    let log = backend.test_readers().log();
    let log = log.lock().expect("post log");
    assert!(!log.key_posts.is_empty());
}

#[test]
fn live_bridged_anchor_drift_refuses_before_dispatch() {
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    // Second topology read (the dispatch-time anchor re-fence) moves the
    // revision: the fast dispatch refuses before any OS touch.
    readers
        .topo_script
        .push_back(topo(vec![20, 3, 7], 20, REVISION + 1));
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(backend.test_readers().bridged_calls.is_empty());
    assert!(backend.test_readers().select_calls.is_empty());
}

#[test]
fn live_bridged_wrong_focus_holds_without_raise() {
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    readers.focus = Some((PID, Some(9)));
    readers.agree = false;
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(
        outcome,
        SendEffectOutcome::RefusedProven,
        "proof focus mismatch refuses pre-dispatch"
    );
    assert!(backend.test_readers().bridged_calls.is_empty());
}

#[test]
fn live_bridged_selector_starts_before_settle_membership_poll() {
    let target = pinned_target();
    let lane = handoff();
    let mut backend = fast_live_backend();
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let readers = backend.test_readers();
    let dispatch_at = readers
        .order
        .iter()
        .position(|e| e == "dispatch")
        .expect("dispatch logged");
    let select_at = readers
        .order
        .iter()
        .position(|e| e == "select")
        .expect("select logged");
    assert!(select_at > dispatch_at, "order={:?}", readers.order);
    assert!(
        !readers.order[dispatch_at + 1..select_at].contains(&"membership".to_owned()),
        "no serial membership gate between dispatch and selector, order={:?}",
        readers.order
    );
    assert_eq!(readers.select_calls.len(), 1, "exactly one select");
}

#[test]
fn live_bridged_late_completion_applies_after_select() {
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    // Async completion lands late: the first settle reads miss, then the
    // exact arrival names the goal.
    readers.spaces_script = VecDeque::from([
        Some(vec![20]),
        Some(vec![20]),
        Some(vec![TARGET_SID]),
        Some(vec![TARGET_SID]),
    ]);
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("late arrival carries readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, TARGET_SID);
    assert_eq!(backend.test_readers().select_calls.len(), 1);
}

#[test]
fn live_bridged_missing_completion_holds_without_false_applied() {
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    // The async completion never lands: membership stays on the source.
    readers.spaces_sticky = Some(vec![20]);
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!(
        backend.test_readers().select_calls.len(),
        1,
        "no select replay"
    );
}

#[test]
fn native_reader_factory_hooks_real_bridged_seam() {
    // The production readers must call the real FFI seam (never a fake):
    // availability reads the move triple PLUS the front-pin symbol PLUS
    // the PSN mapping, dispatch calls the exact bridged mover, and the
    // follow pins the destination front before the select. Pinned at the
    // source (no live touch).
    let source = include_str!("workspace_send_native.rs");
    assert!(
        source.contains("sls_bridged_availability().available"),
        "production availability hooks the real move-triple seam"
    );
    assert!(
        source.contains("sls_space_front_available()"),
        "production availability requires the front-pin symbol"
    );
    assert!(
        source.contains("slps_available()"),
        "production availability requires the PSN mapping"
    );
    assert!(
        source.contains("sls_move_windows_bridged(sid"),
        "production dispatch hooks the real bridged mover"
    );
    assert!(
        source.contains("sls_space_set_front_psn(sid, psn)"),
        "production follow pins the destination front through the real seam"
    );
    assert!(
        source.contains("set_destination_front(sid, pid, wid)"),
        "follow pins the already-proven subject before the destination select"
    );
    // The trait defaults keep every offline fake on drag.
    let trait_source = include_str!("workspace_send_native.rs");
    assert!(
        trait_source.contains("fn bridged_available(&mut self) -> bool"),
        "readers seam exposes availability"
    );
}

// -- Bridged follow destination front pin (offline, no live touch) ----------
//
// yabai `window_manager_send_window_to_space` tail
// (`src/window_manager.c:2116`): the async move dispatches, then exactly one
// `SLSSpaceSetFrontPSN` pins the destination front to the moved subject,
// then the view select follows. Offline shape of the retained incident: a
// sole-window send emptied the source (membership names the goal) while the
// destination front stayed foreign (Finder) with exact membership/current -
// the pin restores the existing follow semantics, and the final exact
// subject focus still gates Applied (no AX raise anywhere).

#[test]
fn live_bridged_follow_pins_front_once_before_single_select() {
    let target = pinned_target();
    let lane = handoff();
    let mut backend = fast_live_backend();
    let (outcome, leg) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("front-pinned follow carries readback");
    assert_eq!(leg.membership, vec![TARGET_SID]);
    assert_eq!(leg.current, TARGET_SID);
    let readers = backend.test_readers();
    assert_eq!(readers.bridged_calls, vec![(TARGET_SID, WID)]);
    assert_eq!(
        readers.front_calls,
        vec![(TARGET_SID, PID, WID)],
        "exactly one front pin to the proven subject on the destination"
    );
    assert_eq!(readers.select_calls, vec![TARGET_SID], "exactly one select");
    let order = &readers.order;
    let dispatch_at = order
        .iter()
        .position(|e| e == "dispatch")
        .expect("dispatch logged");
    let front_at = order
        .iter()
        .position(|e| e == "front")
        .expect("front logged");
    let select_at = order
        .iter()
        .position(|e| e == "select")
        .expect("select logged");
    assert!(
        dispatch_at < front_at && front_at < select_at,
        "move->front->select, order={order:?}"
    );
    assert!(
        !order[dispatch_at + 1..select_at].contains(&"membership".to_owned()),
        "no serial membership gate between dispatch and selector, order={order:?}"
    );
    let log = readers.log.lock().expect("post log");
    assert!(log.mouse_posts.is_empty(), "fast route posts no mouse");
    assert!(log.key_posts.is_empty(), "fast route posts no key");
}

#[test]
fn live_bridged_front_capability_missing_preselects_drag() {
    // The move dispatcher resolves but the front pin (or PSN mapping) does
    // not: the operation preselects the drag before any effect - never a
    // partial bridged dispatch.
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    readers.front_enabled = false;
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let readers = backend.test_readers();
    assert!(
        readers.bridged_calls.is_empty(),
        "no bridged dispatch without the pin"
    );
    assert!(readers.front_calls.is_empty());
    assert!(readers.select_calls.is_empty());
    let log = readers.log.lock().expect("post log");
    assert!(!log.key_posts.is_empty(), "drag preselected before effects");
}

#[test]
fn live_bridged_front_failure_holds_without_select_or_replay() {
    // Front-pin refusal lands after the dispatched move: PossiblyExecuted,
    // no select, no replay, no second move, no drag posts.
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    readers.front_fail = Some("injected front-pin refusal".to_owned());
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    let readers = backend.test_readers();
    assert_eq!(
        readers.bridged_calls.len(),
        1,
        "exactly one dispatch, no replay"
    );
    assert_eq!(readers.front_calls, vec![(TARGET_SID, PID, WID)]);
    assert!(
        readers.select_calls.is_empty(),
        "refused front pin selects nothing"
    );
    assert_eq!(
        backend.counts().total(),
        0,
        "no drag fallback after dispatch"
    );
}

#[test]
fn live_bridged_stale_anchor_withholds_front_pin() {
    // The revision moves between the dispatch-time and select-time fences:
    // the front pin never runs and the selector never starts.
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    // Topology reads: snapshot, proof, and the dispatch fence agree; the
    // select-time fence sees the moved revision.
    for _ in 0..3 {
        readers.topo_script.push_back(base_topo());
    }
    readers
        .topo_script
        .push_back(topo(vec![20, 3, 7], 20, REVISION + 1));
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    let readers = backend.test_readers();
    assert_eq!(readers.bridged_calls.len(), 1);
    assert!(
        readers.front_calls.is_empty(),
        "stale anchor runs no front pin"
    );
    assert!(readers.select_calls.is_empty());
}

#[test]
fn live_bridged_locked_session_and_stale_identity_withhold_front_touch() {
    // Fresh pre-touch fences fail post-dispatch: a locked session and a
    // disagreeing CG join each refuse before any SLS touch, so the selector
    // never runs. Both hold PossiblyExecuted (the dispatch executed).
    for (gates_open, identity_ok, what) in [
        (false, true, "locked session"),
        (true, false, "stale CG identity"),
    ] {
        let target = pinned_target();
        let lane = handoff();
        let mut readers = FakeReaders::new();
        readers.bridged_enabled = true;
        readers.front_gates_open = gates_open;
        readers.front_identity_ok = identity_ok;
        let mut backend = LiveManagedSendBackend::new(readers);
        let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
        assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted, "{what}");
        let readers = backend.test_readers();
        assert_eq!(readers.bridged_calls.len(), 1, "{what}");
        assert_eq!(
            readers.front_calls.len(),
            1,
            "{what}: one pin attempt, refused pre-touch"
        );
        assert!(
            readers.select_calls.is_empty(),
            "{what}: no select after refused pin"
        );
        assert_eq!(backend.counts().total(), 0, "{what}: no drag fallback");
    }
}

#[test]
fn live_bridged_foreign_focus_after_pin_holds_without_raise() {
    // The destination view lands on a foreign front after the pin+select
    // (Finder in the retained incident): the final exact-subject focus gate
    // holds PossiblyExecuted - never an AX raise, never a replay.
    let target = pinned_target();
    let lane = handoff();
    let mut readers = FakeReaders::new();
    readers.bridged_enabled = true;
    readers.focus_after_select = Some((FINDER_PID, Some(FINDER_WID)));
    let mut backend = LiveManagedSendBackend::new(readers);
    let (outcome, _) = execute_managed_send(&target, &mut backend, &lane);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    let readers = backend.test_readers();
    assert_eq!(readers.bridged_calls.len(), 1);
    assert_eq!(readers.front_calls, vec![(TARGET_SID, PID, WID)]);
    assert_eq!(
        readers.select_calls,
        vec![TARGET_SID],
        "one select, no replay"
    );
    assert_eq!(backend.counts().total(), 0, "no raise, no drag fallback");
}
