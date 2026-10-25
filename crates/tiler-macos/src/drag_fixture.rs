#![forbid(unsafe_code)]

//! Owned-window drag/drop offline fixture (auditable, no live state).
//!
//! This harness wires the REAL production pieces end to end without touching
//! the OS: the lock-free [`crate::drag_drop::DragTapGate`] (exact-point plus
//! monotonic-age preproof), [`crate::drag_session::resolve_press_owner`]
//! (published-facts plus explicit AX/WindowServer join), the main-thread
//! [`crate::drag_session::DragSession`] (arm/sample/release lifecycle), and
//! the world gesture route (gesture ingress per report plus correlated
//! [`CommandResult::DragPreview`](tiler_core::world_report::CommandResult)
//! and [`GestureSettled`](tiler_core::gesture::GestureSettled) results from
//! a retained [`WorldEngine`](tiler_core::world_engine::WorldEngine)) with
//! the pure overlay and ordering verdicts. Two independent fixture windows
//! stand in for two disposable owned helpers; the live leg replays the same
//! shape against real helpers and reads back the carrier order from the
//! window server.
//!
//! Anti-fake rule: the harness calls [`crate::drag_session::DragSession::on_down`]
//! ONLY when the gate consumed the down at the same point (the tap proof is
//! never bypassed), and every leg records structured evidence
//! ([`FixtureLegEvidence`]) covering the press proof, the world before/after
//! snapshot, expected versus applied frames, overlay intent and ordering,
//! gate decision counts, capture release, and restoration.

use std::collections::BTreeMap;

use serde::Serialize;
use tiler_core::directional::{OutputId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureIngress;
use tiler_core::session::DomainKey;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{CommandResult, WorldReport};

use crate::border_runtime::{BorderAppRecord, BorderWindowFacts};
use crate::classifier::{AppFacts, AxFacts, CgFacts};
use crate::config::ProbeModifier;
use crate::drag_drop::{DragFences, DragOptions, PreviewOutcome};
use crate::drag_session::{DragSession, PressOwner, resolve_press_owner};
use crate::ffi::AxFrame;
use crate::runtime_core::LifetimeMemory;
use crate::runtime_types::{NativeContext, NativeDomain, ObservedWindow, window_id};
use crate::world_builder::{ReportInput, build_report};

/// One fixture window: the published facts plus the explicit off-tap join
/// legs the live leg reads from AX and the window server.
#[derive(Debug, Clone)]
pub struct FixtureWindow {
    pub pid: u32,
    pub wid: u32,
    /// CG top-left frame (tap space; matches the AX frame here).
    pub frame: (f64, f64, f64, f64),
    pub revision: u64,
}

impl FixtureWindow {
    fn token(&self) -> String {
        window_id(self.pid, self.wid)
    }

    fn record(&self, now_ms: u64) -> BorderAppRecord {
        let (x, y, w, h) = self.frame;
        BorderAppRecord {
            pid: self.pid,
            bundle: Some("com.apple.TextEdit".to_owned()),
            focused_wid: Some(self.wid),
            selected: Some((self.pid, Some(self.wid))),
            window: Some(BorderWindowFacts {
                wid: self.wid,
                frame: AxFrame { x, y, w, h },
                ax: AxFacts::standard(),
                cg: CgFacts::normal(),
                app: AppFacts::regular(),
                spaces: vec![11],
            }),
            revision: self.revision,
            updated_ms: now_ms,
        }
    }

    fn observed(&self) -> ObservedWindow {
        let (x, y, w, h) = self.frame;
        ObservedWindow {
            wid: self.wid,
            x: x as i32,
            y: y as i32,
            w: w as i32,
            h: h as i32,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }
}

/// Two independent fixture windows (helper A left, helper B right).
#[derive(Debug, Clone)]
pub struct FixturePair {
    pub a: FixtureWindow,
    pub b: FixtureWindow,
}

impl FixturePair {
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: FixtureWindow {
                pid: 4242,
                wid: 7,
                frame: (100.0, 100.0, 400.0, 300.0),
                revision: 9,
            },
            b: FixtureWindow {
                pid: 4243,
                wid: 9,
                frame: (0.0, 400.0, 400.0, 300.0),
                revision: 9,
            },
        }
    }

    /// Interior point of window A (the press point for every leg).
    #[must_use]
    pub const fn press_point_a(&self) -> (i32, i32) {
        (300, 250)
    }

    /// Interior point of window B (the move/drop point for drop legs).
    /// Also two pixels inside the tiled sibling's left edge for the
    /// 1200x800 world tiling, so the world preview resolves an insertion
    /// slot instead of refusing self.
    #[must_use]
    pub const fn move_point_b(&self) -> (i32, i32) {
        (10, 450)
    }
}

impl Default for FixturePair {
    fn default() -> Self {
        Self::new()
    }
}

fn fences_ok() -> DragFences {
    DragFences {
        enabled: true,
        session_unlocked: true,
        secure_input_active: false,
        scope_allowed: true,
        fullscreen_active: false,
        revision_fresh: true,
        focus_matches: true,
    }
}

/// Read-only preview summary: outcome token, optional slot rect
/// `(x, y, w, h)`, optional target window token.
type PreviewSummary = (String, Option<(i32, i32, i32, i32)>, Option<String>);

/// Structured evidence for one fixture leg (plain data, JSON-serializable so
/// the live leg can emit the same shape with live readbacks).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixtureLegEvidence {
    pub leg: String,
    /// Gate decisions over the leg: (downs_consumed, downs_passed,
    /// drags_consumed, ups_consumed).
    pub gate_counts: (u64, u64, u64, u64),
    pub press_point: (i32, i32),
    pub snapshot_point: Option<(i32, i32)>,
    pub gate_consumed: bool,
    pub armed_window: Option<String>,
    pub owner_proof: Option<String>,
    pub preview_outcome: String,
    pub preview_rect: Option<(i32, i32, i32, i32)>,
    pub preview_target: Option<String>,
    pub overlay: String,
    pub order_verified: Option<bool>,
    pub drop_outcome: Option<String>,
    pub world_before: Vec<(String, (i32, i32, i32, i32))>,
    pub world_after: Vec<(String, (i32, i32, i32, i32))>,
    pub applied_frames: Vec<(String, (i32, i32, i32, i32))>,
    pub world_mutated: bool,
    pub capture_released: bool,
    pub restoration_ok: bool,
}

/// Offline harness: the gate, the session, and the retained world engine
/// wired as the tile main thread wires them (gate proof, then owner proof,
/// then arm, then samples, then exactly one terminal edge). Every
/// observation converges through one complete world report; every preview
/// and terminal correlates from the reply (never a direct engine oracle).
pub struct DragFixture {
    pair: FixturePair,
    gate: crate::drag_drop::DragTapGate,
    session: DragSession,
    engine: WorldEngine,
    now_ms: u64,
    obs: u64,
    req: u64,
    downs_consumed: u64,
    downs_passed: u64,
    drags_consumed: u64,
    ups_consumed: u64,
}

impl DragFixture {
    /// New harness over two fixture windows and a world engine converging
    /// both (same observation shape the reconcile path feeds the worker).
    #[must_use]
    pub fn new() -> Self {
        let mut fixture = Self {
            pair: FixturePair::new(),
            gate: crate::drag_drop::DragTapGate::new(),
            session: DragSession::new(DragOptions::default(), ProbeModifier::Ctrl),
            engine: WorldEngine::new("drag-fixture-gen").expect("generation"),
            now_ms: 1_000,
            obs: 0,
            req: 0,
            downs_consumed: 0,
            downs_passed: 0,
            drags_consumed: 0,
            ups_consumed: 0,
        };
        // Bootstrap observation (no gesture): the engine adopts both
        // fixture windows before any gesture binds.
        let _ = fixture.dispatch(None, fixture.frames());
        fixture
    }

    /// Live fixture frames per window token, sorted (before/after
    /// comparison over the world snapshot, never a direct plan).
    fn frames(&self) -> Vec<(String, (i32, i32, i32, i32))> {
        let mut rows = vec![
            (self.pair.a.token(), rect_tuple(self.pair.a.frame)),
            (self.pair.b.token(), rect_tuple(self.pair.b.frame)),
        ];
        rows.sort();
        rows
    }

    /// Native context for one report: both fixture windows homed by exact
    /// Space membership to the single current user Space (stage-1 native
    /// order/current vocabulary, no invented logical ids).
    fn context(&self) -> NativeContext {
        NativeContext {
            revision: 9,
            domains: vec![NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                space: 11,
                current: true,
                readable: true,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                },
                visible: Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                },
            }],
            skipped: Vec::new(),
        }
    }

    /// Dispatch one complete world report with an optional gesture ingress
    /// and return the accepted reply. Observation and request sequences
    /// advance per dispatch (never a duplicate, never a replay).
    fn dispatch(
        &mut self,
        gesture: Option<GestureIngress>,
        frames: Vec<(String, (i32, i32, i32, i32))>,
    ) -> tiler_core::world_report::WorldReply {
        self.obs += 1;
        self.req += 1;
        let request_id = format!("req-{}", self.req);
        let ctx = self.context();
        let key = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
        let mut observed_a = self.pair.a.observed();
        let mut observed_b = self.pair.b.observed();
        for (token, (x, y, w, h)) in &frames {
            let observed = if *token == self.pair.a.token() {
                &mut observed_a
            } else {
                &mut observed_b
            };
            observed.x = *x;
            observed.y = *y;
            observed.w = *w;
            observed.h = *h;
        }
        members.insert(
            key,
            vec![(self.pair.a.pid, observed_a), (self.pair.b.pid, observed_b)],
        );
        let empty_unreadable: BTreeMap<DomainKey, tiler_core::world_report::HoldReason> =
            BTreeMap::new();
        let memory = LifetimeMemory::empty();
        let a_token = self.pair.a.token();
        let mut input = ReportInput::new(
            "macos-drag-fixture",
            "drag-fixture-1",
            "drag-fixture-gen",
            &ctx,
            self.obs,
            self.req,
            &request_id,
            &members,
            &memory,
            &[],
            &empty_unreadable,
            &[],
            &a_token,
            false,
        );
        input.gesture = gesture;
        let report: WorldReport = build_report(&mut input);
        assert!(
            tiler_core::world_report::validate_report(&report),
            "fixture report validates"
        );
        match self.engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
            other => panic!("fixture report must apply, got {other:?}"),
        }
    }

    fn outcome_token(outcome: &PreviewOutcome) -> String {
        match outcome {
            PreviewOutcome::Show { .. } => "show".to_owned(),
            PreviewOutcome::HideTransient { reason } => (*reason).to_owned(),
            PreviewOutcome::CancelDead { reason } => (*reason).to_owned(),
        }
    }

    fn to_preview(reply: &tiler_core::world_report::WorldReply) -> PreviewOutcome {
        match &reply.command_result {
            Some(CommandResult::DragPreview {
                rect,
                hover,
                target,
                ..
            }) => PreviewOutcome::Show {
                rect: *rect,
                hover_prior: hover.clone().map(Box::new),
                target: target.clone(),
            },
            _ => PreviewOutcome::HideTransient { reason: "held" },
        }
    }

    /// Main-thread idle publish for the press point: the exact cursor point
    /// joined with the explicit AX/WindowServer legs (offline these are the
    /// true owners; the live leg reads them from the OS and refuses on any
    /// mismatch the same way). Returns the proven owner.
    fn publish_press_proof(&self, window: &FixtureWindow, point: (i32, i32)) -> PressOwner {
        let record = window.record(self.now_ms);
        let owner = resolve_press_owner(
            Some(&record),
            window.pid,
            point.0,
            point.1,
            self.now_ms,
            window.revision,
            Some((window.pid, window.wid)),
            Some(window.wid),
        )
        .expect("fixture press point must prove");
        self.gate
            .publish_at(owner.pid, owner.wid, point.0, point.1, true, self.now_ms);
        owner
    }

    /// Tap down through the REAL gate (exact point plus fresh age). Only a
    /// consumed down reaches [`DragSession::on_down`]; a pass keeps the
    /// session idle (never a fake arm).
    fn tap_down(&mut self, point: (i32, i32), meta_held: bool) -> bool {
        const CONTROL_FLAGS: u64 = 0x0004_0000;
        if self
            .gate
            .try_consume_down_at(point.0, point.1, meta_held, self.now_ms)
        {
            self.downs_consumed += 1;
            true
        } else {
            self.downs_passed += 1;
            let _ = CONTROL_FLAGS;
            false
        }
    }

    /// Arm the session on a consumed down (caller proves consumption first).
    fn arm_on_consumed(
        &self,
        owner: &PressOwner,
        point: (i32, i32),
        meta_held: bool,
    ) -> Result<(), &'static str> {
        const CONTROL_FLAGS: u64 = 0x0004_0000;
        let hit = crate::ffi::DragMouseHit {
            kind: crate::ffi::DragMouseKind::Down,
            x: point.0,
            y: point.1,
            flags: if meta_held { CONTROL_FLAGS } else { 0 },
        };
        match self.session.on_down(hit, &Ok(owner.clone()), fences_ok()) {
            crate::drag_session::HitIntent::Down(crate::drag_session::Downcome::Armed {
                ..
            }) => Ok(()),
            crate::drag_session::HitIntent::Down(crate::drag_session::Downcome::Refused {
                reason,
            }) => Err(reason),
            _ => Err("unexpected-intent"),
        }
    }

    /// Read-only world preview at one point (never mutates): the live
    /// behavior probe for before/after comparison.
    fn preview_summary(&mut self, _window: &str, _point: (i32, i32)) -> PreviewSummary {
        let reply = self.dispatch(None, self.frames());
        match Self::to_preview(&reply) {
            PreviewOutcome::Show { rect, target, .. } => (
                "show".to_owned(),
                Some((rect.x, rect.y, rect.w, rect.h)),
                Some(target.0),
            ),
            other => (Self::outcome_token(&other), None, None),
        }
    }

    /// Cancel leg: exact press on A, arm, world START, Escape, world
    /// CANCELLED, balanced release. The world never commits (no terminal
    /// result): a read-only preview at the move point must resolve
    /// identically before and after, and the snapshot geometry reads
    /// identical.
    pub fn run_cancel_leg(&mut self) -> FixtureLegEvidence {
        let before = self.frames();
        let moved = self.pair.move_point_b();
        let window_token = self.pair.a.token();
        let probe_before = self.preview_summary(&window_token, moved);
        let point = self.pair.press_point_a();
        let window = self.pair.a.clone();
        let owner = self.publish_press_proof(&window, point);
        let snapshot_point = self.gate.published_snapshot().map(|(_, _, x, y)| (x, y));
        let gate_consumed = self.tap_down(point, true);
        let mut armed_window = None;
        if gate_consumed {
            self.arm_on_consumed(&owner, point, true)
                .expect("proven press must arm");
            armed_window = Some(owner.window.clone());
            // START binds the world session through the ACTUAL session
            // handoff (same ingress the product drive submits); no overlay
            // publishes at the down point itself.
            let started_ingress = self
                .session
                .start_ingress()
                .expect("armed gesture must offer a bind ingress");
            let started = self.dispatch(Some(started_ingress), self.frames());
            assert!(
                started.command_result.is_none(),
                "start binds only, never previews"
            );
        }
        // Escape cancels with overlay cleared and fail-safe release; the
        // world clears the retained session with no commit, and the
        // trailing release then passes through (never a drop).
        let cancelled = self.session.on_escape();
        assert!(cancelled.is_some(), "Escape must cancel the armed gesture");
        if gate_consumed {
            let terminal = self.dispatch(cancelled, self.frames());
            assert!(
                terminal
                    .gesture_settled
                    .is_none_or(|settled| !settled.committed),
                "cancel clears with no commit"
            );
        }
        let capture_released = matches!(
            self.session.on_up(point.0, point.1),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        );
        let probe_after = self.preview_summary(&window_token, moved);
        let after = self.frames();
        let world_mutated = before != after || probe_before != probe_after;
        FixtureLegEvidence {
            leg: "cancel".to_owned(),
            gate_counts: self.counts(),
            press_point: point,
            snapshot_point,
            gate_consumed,
            armed_window,
            owner_proof: Some(format!("{}_{}", owner.pid, owner.wid)),
            preview_outcome: "none (zero movement hides)".to_owned(),
            preview_rect: None,
            preview_target: None,
            overlay: "hide".to_owned(),
            order_verified: None,
            drop_outcome: None,
            world_before: before.clone(),
            world_after: after.clone(),
            applied_frames: Vec::new(),
            world_mutated,
            capture_released,
            restoration_ok: !world_mutated,
        }
    }

    /// Drop leg: exact press on A, world START, move to B, world UPDATED
    /// preview resolves a slot, overlay shows above the joined target with
    /// verified order, release drops through a world RELEASED terminal, and
    /// the applied snapshot frames equal the committed reply geometry.
    /// `carrier_wid` and `order_front_to_back` stand in for the live window-
    /// server readback (the live leg fills them from CG).
    pub fn run_drop_leg(
        &mut self,
        carrier_wid: u32,
        order_front_to_back: &[u32],
    ) -> FixtureLegEvidence {
        use crate::drag_drop::{DragOrdering, DragOverlay};
        use crate::drag_session::SampleIntent;

        let before = self.frames();
        let press = self.pair.press_point_a();
        let moved = self.pair.move_point_b();
        let window = self.pair.a.clone();
        let owner = self.publish_press_proof(&window, press);
        let snapshot_point = self.gate.published_snapshot().map(|(_, _, x, y)| (x, y));
        let gate_consumed = self.tap_down(press, true);
        let mut armed_window = None;
        if gate_consumed {
            self.arm_on_consumed(&owner, press, true)
                .expect("proven press must arm");
            armed_window = Some(owner.window.clone());
            let started_ingress = self
                .session
                .start_ingress()
                .expect("armed gesture must offer a bind ingress");
            let started = self.dispatch(Some(started_ingress), self.frames());
            assert!(
                started.command_result.is_none(),
                "start binds only, never previews"
            );
        }
        // Moved sample previews through the world gesture route using the
        // ACTUAL session ingress (same handoff the product drive submits).
        let preview_ingress = match self.session.on_drag(moved.0, moved.1, fences_ok()) {
            crate::drag_session::HitIntent::Sample(SampleIntent::Preview { ingress }) => {
                self.drags_consumed += 1;
                Some(ingress)
            }
            _ => None,
        };
        let mut preview_outcome_text = "no-preview".to_owned();
        let mut preview_rect = None;
        let mut preview_target = None;
        let mut overlay_text = "hide".to_owned();
        let mut order_verified = None;
        if let Some(ingress) = preview_ingress {
            let updated = self.dispatch(Some(ingress), self.frames());
            let outcome = Self::to_preview(&updated);
            preview_outcome_text = Self::outcome_token(&outcome);
            self.session.note_preview(&outcome);
            if let PreviewOutcome::Show { rect, target, .. } = &outcome {
                preview_rect = Some((rect.x, rect.y, rect.w, rect.h));
                preview_target = Some(target.0.clone());
                if let DragOverlay::ShowFillAbove { ordering, .. } = self.session.overlay_for(*rect)
                {
                    assert_eq!(ordering, DragOrdering::AboveTarget);
                    overlay_text = "show-fill-above".to_owned();
                    let target_wid = crate::drag_session::preview_target_wid(target).unwrap_or(0);
                    let verified = crate::drag_session::carrier_above_verified(
                        order_front_to_back,
                        carrier_wid,
                        target_wid,
                    );
                    order_verified = Some(verified);
                    if !verified {
                        overlay_text = "hide (order-unverified)".to_owned();
                    }
                }
            }
        }
        // Release drops through the world gesture terminal (single-flight,
        // once). The native move happened under the gesture, so the release
        // report carries the post-gesture observation the intent derives
        // from — exactly like the live AX readback.
        let mut drop_outcome = None;
        let mut applied_frames = Vec::new();
        if let crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Drop {
            ingress,
            ..
        }) = self.session.on_up(moved.0, moved.1)
        {
            let window = ingress.window.0.clone();
            self.ups_consumed += 1;
            assert_eq!(
                window,
                self.pair.a.token(),
                "release drops the proven owner"
            );
            let start_frame = rect_tuple(self.pair.a.frame);
            let mut moved_frames = self.frames();
            for (token, frame) in &mut moved_frames {
                if *token == self.pair.a.token() {
                    // Native post-gesture observation: the window server
                    // moved the window with the gesture (same size, new
                    // origin — the release intent derives from this delta).
                    *frame = (100, 400, start_frame.2, start_frame.3);
                }
            }
            assert_ne!(
                moved_frames, before,
                "post-gesture observation must differ from START"
            );
            let released = self.dispatch(Some(ingress), moved_frames);
            match released.gesture_settled {
                Some(terminal) if terminal.committed => {
                    drop_outcome = Some("planned".to_owned());
                    applied_frames = released
                        .windows
                        .iter()
                        .filter_map(|row| {
                            row.effective_frame.or(row.allocation).map(|frame| {
                                (row.lifetime.0.clone(), (frame.x, frame.y, frame.w, frame.h))
                            })
                        })
                        .collect::<Vec<_>>();
                    applied_frames.sort();
                }
                Some(_) => {
                    drop_outcome = Some("snap-back:uncommitted".to_owned());
                }
                None => {
                    drop_outcome = Some("cancel-dead:no-terminal".to_owned());
                }
            }
        }
        // Balanced: a second release ignores (no double drop).
        let capture_released = matches!(
            self.session.on_up(moved.0, moved.1),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        ) && !self.session.active();
        // The release commits through the world: the committed snapshot rows
        // are the after-state, and the applied frames must equal them
        // exactly (no silent drift between commit and application).
        let world_after = applied_frames.clone();
        let world_mutated = before != world_after;
        let restoration_ok = !applied_frames.is_empty() && applied_frames == world_after;
        FixtureLegEvidence {
            leg: "drop".to_owned(),
            gate_counts: self.counts(),
            press_point: press,
            snapshot_point,
            gate_consumed,
            armed_window,
            owner_proof: Some(format!("{}_{}", owner.pid, owner.wid)),
            preview_outcome: preview_outcome_text,
            preview_rect,
            preview_target,
            overlay: overlay_text,
            order_verified,
            drop_outcome,
            world_before: before,
            world_after,
            applied_frames,
            world_mutated,
            capture_released,
            restoration_ok,
        }
    }

    /// Pass-through leg: the tap point misses the proven snapshot by 1px, so
    /// the gate refuses and the session must never arm (no fake `on_down`).
    pub fn run_off_point_leg(&mut self) -> FixtureLegEvidence {
        let before = self.frames();
        let moved = self.pair.move_point_b();
        let window_token = self.pair.a.token();
        let probe_before = self.preview_summary(&window_token, moved);
        let press = self.pair.press_point_a();
        let window = self.pair.a.clone();
        self.publish_press_proof(&window, press);
        let missed = (press.0 + 1, press.1);
        let gate_consumed = self.tap_down(missed, true);
        assert!(!gate_consumed, "1px miss must pass through");
        // No on_down call by construction: the session stays idle and the
        // trailing up passes through.
        let idle_up_passes = !self.gate.consume_up() && !self.session.active();
        let probe_after = self.preview_summary(&window_token, moved);
        let after = self.frames();
        let world_mutated = before != after || probe_before != probe_after;
        FixtureLegEvidence {
            leg: "off-point-pass-through".to_owned(),
            gate_counts: self.counts(),
            press_point: missed,
            snapshot_point: self.gate.published_snapshot().map(|(_, _, x, y)| (x, y)),
            gate_consumed,
            armed_window: None,
            owner_proof: None,
            preview_outcome: "none (never armed)".to_owned(),
            preview_rect: None,
            preview_target: None,
            overlay: "hide".to_owned(),
            order_verified: None,
            drop_outcome: None,
            world_before: before.clone(),
            world_after: after.clone(),
            applied_frames: Vec::new(),
            world_mutated,
            capture_released: idle_up_passes,
            restoration_ok: !world_mutated,
        }
    }

    fn counts(&self) -> (u64, u64, u64, u64) {
        (
            self.downs_consumed,
            self.downs_passed,
            self.drags_consumed,
            self.ups_consumed,
        )
    }
}

impl Default for DragFixture {
    fn default() -> Self {
        Self::new()
    }
}

fn rect_tuple(frame: (f64, f64, f64, f64)) -> (i32, i32, i32, i32) {
    (
        frame.0 as i32,
        frame.1 as i32,
        frame.2 as i32,
        frame.3 as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag_drop::DragController;

    #[test]
    fn fixture_cancel_leg_arms_then_cancels_without_world_mutation() {
        let mut fixture = DragFixture::new();
        let evidence = fixture.run_cancel_leg();
        assert!(evidence.gate_consumed, "exact press must consume");
        assert_eq!(
            evidence.armed_window,
            Some(fixture.pair.a.token()),
            "arm names the proven owner"
        );
        assert!(!evidence.world_mutated, "cancel must not mutate the world");
        assert!(evidence.restoration_ok, "cancel restores trivially");
        assert!(evidence.capture_released, "trailing release must ignore");
        assert_eq!(evidence.overlay, "hide");
        assert_eq!(evidence.gate_counts, (1, 0, 0, 0));
        // The evidence serializes for the live-world shape check.
        let json = serde_json::to_string(&evidence).expect("evidence serializes");
        assert!(json.contains("\"leg\":\"cancel\""));
    }

    #[test]
    fn fixture_drop_leg_applies_world_snapshot_with_verified_order() {
        let mut fixture = DragFixture::new();
        // Carrier above the B target in the global order (live: CG join).
        let carrier_wid = 55;
        let order = vec![55, fixture.pair.b.wid, fixture.pair.a.wid];
        let evidence = fixture.run_drop_leg(carrier_wid, &order);
        assert!(evidence.gate_consumed, "exact press must consume");
        assert_eq!(
            evidence.preview_outcome, "show",
            "moved pointer resolves a slot"
        );
        let rect = evidence.preview_rect.expect("slot rect");
        assert!(rect.2 > 0 && rect.3 > 0);
        assert_eq!(evidence.overlay, "show-fill-above");
        assert_eq!(evidence.order_verified, Some(true));
        assert_eq!(evidence.drop_outcome.as_deref(), Some("planned"));
        assert_eq!(
            evidence.applied_frames.len(),
            2,
            "both windows land in the snapshot"
        );
        assert!(evidence.world_mutated, "a committed drop moves geometry");
        assert!(evidence.capture_released, "release balances exactly once");
        // Applied frames name both fixture windows (no silent drift).
        let windows: Vec<&str> = evidence
            .applied_frames
            .iter()
            .map(|(w, _)| w.as_str())
            .collect();
        assert!(windows.contains(&fixture.pair.a.token().as_str()));
        assert!(windows.contains(&fixture.pair.b.token().as_str()));
    }

    #[test]
    fn fixture_drop_leg_hides_on_unverified_order() {
        let mut fixture = DragFixture::new();
        // Carrier BELOW the target: the show must not render (hide instead
        // of assuming the order call landed).
        let carrier_wid = 55;
        let order = vec![fixture.pair.b.wid, 55, fixture.pair.a.wid];
        let evidence = fixture.run_drop_leg(carrier_wid, &order);
        assert_eq!(evidence.preview_outcome, "show", "world still resolves");
        assert_eq!(evidence.order_verified, Some(false));
        assert_eq!(evidence.overlay, "hide (order-unverified)");
        // The release still commits through the world (ordering only gates
        // the preview render, never the gesture); capture still balances.
        assert_eq!(evidence.drop_outcome.as_deref(), Some("planned"));
        assert!(evidence.capture_released);
    }

    #[test]
    fn fixture_off_point_press_never_arms() {
        let mut fixture = DragFixture::new();
        let evidence = fixture.run_off_point_leg();
        assert!(!evidence.gate_consumed, "1px miss must pass through");
        assert_eq!(evidence.armed_window, None, "no arm without tap proof");
        assert!(!evidence.world_mutated);
        assert!(evidence.restoration_ok);
        assert!(evidence.capture_released);
        assert!(!fixture.session.active(), "session stays idle");
        assert_eq!(evidence.gate_counts, (0, 1, 0, 0));
    }

    #[test]
    fn fixture_two_windows_stay_independent() {
        // A press proven on B arms B, never A: per-window owner proofs do
        // not leak across the pair.
        let mut fixture = DragFixture::new();
        let point = fixture.pair.move_point_b();
        let window = fixture.pair.b.clone();
        let owner = fixture.publish_press_proof(&window, point);
        assert_eq!(owner.window, fixture.pair.b.token());
        assert!(fixture.tap_down(point, true));
        fixture
            .arm_on_consumed(&owner, point, true)
            .expect("B press arms");
        assert!(fixture.session.active());
        assert!(fixture.session.on_escape().is_some());
        assert!(!fixture.session.active());
    }

    #[test]
    fn fixture_controller_lifecycle_needs_no_click_loss() {
        // Stationary exact press consumes (ordinary drag start works under
        // the exact proof); only a moved cursor passes through.
        let controller_gate = crate::drag_drop::DragTapGate::new();
        controller_gate.publish_at(4242, 7, 300, 250, true, 1_000);
        assert!(controller_gate.try_consume_down_at(300, 250, true, 1_000));
        let mut controller = DragController::new(DragOptions::default());
        controller
            .press(
                "4242_7",
                300,
                250,
                true,
                true,
                fences_ok(),
                9,
                tiler_core::geometry::Rect {
                    x: 300,
                    y: 250,
                    w: 400,
                    h: 300,
                },
            )
            .expect("proven press arms the controller");
        assert!(controller.should_consume_held());
        assert_eq!(controller.release().as_deref(), Some("4242_7"));
        assert!(!controller.should_consume_held());
    }
}
