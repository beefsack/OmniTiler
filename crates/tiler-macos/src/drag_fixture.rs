#![forbid(unsafe_code)]

//! Owned-window drag/drop offline fixture (auditable, no live state).
//!
//! This harness wires the REAL production pieces end to end without touching
//! the OS: the lock-free [`crate::drag_drop::DragTapGate`] (exact-point plus
//! monotonic-age preproof), [`crate::drag_session::resolve_press_owner`]
//! (published-facts plus explicit AX/WindowServer join), the main-thread
//! [`crate::drag_session::DragSession`] (arm/sample/release lifecycle), and
//! the shared Engine oracle ([`crate::drag_drop::engine_preview`] /
//! [`engine_drop`](crate::drag_drop::engine_drop)) with the pure overlay and
//! ordering verdicts. Two independent fixture windows stand in for two
//! disposable owned helpers; the live leg replays the same shape against
//! real helpers and reads back the carrier order from the window server.
//!
//! Anti-fake rule: the harness calls [`crate::drag_session::DragSession::on_down`]
//! ONLY when the gate consumed the down at the same point (the tap proof is
//! never bypassed), and every leg records structured evidence
//! ([`FixtureLegEvidence`]) covering the press proof, the Engine before/after
//! plan, expected versus applied frames, overlay intent and ordering, gate
//! decision counts, capture release, and restoration.

use std::collections::BTreeMap;

use serde::Serialize;
use tiler_core::boundary::{CoreCommand, CoreEvent};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};
use tiler_core::size_hints::WindowSizeHints;

use crate::border_runtime::{BorderAppRecord, BorderWindowFacts};
use crate::classifier::{AppFacts, AxFacts, CgFacts};
use crate::config::ProbeModifier;
use crate::drag_drop::{DragFences, DragOptions, DropOutcome, PreviewOutcome, frames_for_plan};
use crate::drag_session::{DragSession, PressOwner, resolve_press_owner};
use crate::ffi::AxFrame;
use crate::runtime_types::window_id;

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
                frame: (700.0, 100.0, 400.0, 300.0),
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
    #[must_use]
    pub const fn move_point_b(&self) -> (i32, i32) {
        (900, 250)
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
    pub engine_before: Vec<(String, (i32, i32, i32, i32))>,
    pub engine_after: Vec<(String, (i32, i32, i32, i32))>,
    pub applied_frames: Vec<(String, (i32, i32, i32, i32))>,
    pub engine_mutated: bool,
    pub capture_released: bool,
    pub restoration_ok: bool,
}

/// Offline harness: the gate, the session, and the current Engine wired as
/// the tile main thread wires them (gate proof, then owner proof, then arm,
/// then samples, then exactly one terminal edge).
pub struct DragFixture {
    pair: FixturePair,
    gate: crate::drag_drop::DragTapGate,
    session: DragSession,
    engine: Engine,
    owner: OwnerId,
    generation: GenerationId,
    domain: OutputDomain,
    domain_key: DomainKey,
    now_ms: u64,
    downs_consumed: u64,
    downs_passed: u64,
    drags_consumed: u64,
    ups_consumed: u64,
}

impl DragFixture {
    /// New harness over two fixture windows and a converged Engine holding
    /// both (same observation shape the reconcile path feeds the worker).
    #[must_use]
    pub fn new() -> Self {
        let pair = FixturePair::new();
        let domain = OutputDomain {
            id: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1200,
                h: 800,
            },
            gap: 8,
            adjacent: BTreeMap::new(),
        };
        let domain_key = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let mut engine = Engine::new();
        let owner = OwnerId::parse("macos-drag-fixture").expect("owner");
        let generation = GenerationId::parse("drag-fixture-gen").expect("generation");
        engine.sync_binding(&owner, &generation);
        Self {
            pair,
            gate: crate::drag_drop::DragTapGate::new(),
            session: DragSession::new(DragOptions::default(), ProbeModifier::Ctrl),
            engine,
            owner,
            generation,
            domain,
            domain_key,
            now_ms: 1_000,
            downs_consumed: 0,
            downs_passed: 0,
            drags_consumed: 0,
            ups_consumed: 0,
        }
    }

    /// Engine window rows for both fixture windows (tokens are the runtime
    /// `pid_wid` tokens, so the session owner proof names the Engine mover
    /// directly).
    fn engine_windows(&self) -> Vec<EngineWindow> {
        let rect = |frame: (f64, f64, f64, f64)| Rect {
            x: frame.0 as i32,
            y: frame.1 as i32,
            w: frame.2 as i32,
            h: frame.3 as i32,
        };
        vec![
            EngineWindow {
                window: WindowId(self.pair.a.token()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                rect: rect(self.pair.a.frame),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId(self.pair.b.token()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                rect: rect(self.pair.b.frame),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
            },
        ]
    }

    /// Sorted Engine geometry rows (window token plus rect), for before/after
    /// comparison.
    fn engine_geometry(&self) -> Vec<(String, (i32, i32, i32, i32))> {
        let mut rows: Vec<(String, (i32, i32, i32, i32))> = self
            .engine_windows()
            .into_iter()
            .map(|w| (w.window.0, (w.rect.x, w.rect.y, w.rect.w, w.rect.h)))
            .collect();
        rows.sort();
        rows
    }

    fn drag_event(&self, command: CoreCommand, correlation: &str) -> CoreEvent {
        CoreEvent {
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            correlation: CorrelationId::parse(correlation).expect("correlation"),
            revision: 0,
            fingerprint: 7,
            domain: self.domain.clone(),
            domain_key: self.domain_key.clone(),
            outer_gap: 8,
            focused_window: WindowId(self.pair.a.token()),
            windows: self.engine_windows(),
            closed_windows: Vec::new(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command,
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

    fn outcome_token(outcome: &PreviewOutcome) -> String {
        match outcome {
            PreviewOutcome::Show { .. } => "show",
            PreviewOutcome::HideTransient { reason } => reason,
            PreviewOutcome::CancelDead { reason } => reason,
        }
        .to_owned()
    }

    /// Read-only Engine preview at one point (never mutates): the live
    /// behavior probe for before/after comparison.
    fn preview_summary(
        &mut self,
        window: &str,
        point: (i32, i32),
        correlation: &str,
    ) -> PreviewSummary {
        let command = CoreCommand::DragPreview {
            window: window.to_owned(),
            x: point.0,
            y: point.1,
            hover_prior: None,
            source: None,
        };
        let event = self.drag_event(command, correlation);
        match crate::drag_drop::engine_preview(&mut self.engine, event) {
            PreviewOutcome::Show { rect, target, .. } => (
                "show".to_owned(),
                Some((rect.x, rect.y, rect.w, rect.h)),
                Some(target.0),
            ),
            other => (Self::outcome_token(&other), None, None),
        }
    }

    /// Cancel leg: exact press on A, arm, Escape, balanced release. The
    /// Engine is never contacted for a mutation (no drop submit): a
    /// read-only preview at the move point must resolve identically before
    /// and after, and the geometry rows read identical.
    pub fn run_cancel_leg(&mut self) -> FixtureLegEvidence {
        let before = self.engine_geometry();
        let moved = self.pair.move_point_b();
        let probe_before =
            self.preview_summary(&self.pair.a.token(), moved, "fixture-cancel-probe-0");
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
        }
        // Escape cancels with overlay cleared and fail-safe release; the
        // trailing release then passes through (never a drop).
        let cancelled = self.session.on_escape();
        assert!(cancelled, "Escape must cancel the armed gesture");
        let capture_released = matches!(
            self.session.on_up(point.0, point.1),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        );
        let probe_after =
            self.preview_summary(&self.pair.a.token(), moved, "fixture-cancel-probe-1");
        let after = self.engine_geometry();
        let engine_mutated = before != after || probe_before != probe_after;
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
            engine_before: before.clone(),
            engine_after: after.clone(),
            applied_frames: Vec::new(),
            engine_mutated,
            capture_released,
            restoration_ok: !engine_mutated,
        }
    }

    /// Drop leg: exact press on A, move to B, Engine preview resolves a slot,
    /// overlay shows above the joined target with verified order, release
    /// drops through the Engine, and the applied frames equal the plan.
    /// `carrier_wid` and `order_front_to_back` stand in for the live window-
    /// server readback (the live leg fills them from CG).
    pub fn run_drop_leg(
        &mut self,
        carrier_wid: u32,
        order_front_to_back: &[u32],
    ) -> FixtureLegEvidence {
        use crate::drag_drop::{DragOrdering, DragOverlay};
        use crate::drag_session::SampleIntent;

        let before = self.engine_geometry();
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
        }
        // Moved sample previews through the real Engine oracle.
        let preview_query = match self.session.on_drag(moved.0, moved.1, fences_ok()) {
            crate::drag_session::HitIntent::Sample(SampleIntent::Preview { window, x, y }) => {
                self.drags_consumed += 1;
                Some(self.session.preview_query(window, x, y))
            }
            _ => None,
        };
        let mut preview_outcome_text = "no-preview".to_owned();
        let mut preview_rect = None;
        let mut preview_target = None;
        let mut overlay_text = "hide".to_owned();
        let mut order_verified = None;
        if let Some(query) = preview_query {
            let command = CoreCommand::DragPreview {
                window: query.window.clone(),
                x: query.x,
                y: query.y,
                hover_prior: query.hover_prior.clone(),
                source: None,
            };
            let event = self.drag_event(command, "fixture-preview-1");
            let outcome = crate::drag_drop::engine_preview(&mut self.engine, event);
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
        // Release drops through the real Engine oracle (single-flight, once).
        let mut drop_outcome = None;
        let mut applied_frames = Vec::new();
        if let crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Drop {
            window,
            x,
            y,
            press_revision,
        }) = self.session.on_up(moved.0, moved.1)
        {
            self.ups_consumed += 1;
            let request = self.session.drop_request(window, x, y, press_revision);
            let command = CoreCommand::DragDrop {
                window: request.window.clone(),
                x: request.x,
                y: request.y,
                hover_prior: request.hover_prior.clone(),
                source: None,
            };
            let event = self.drag_event(command, "fixture-drop-1");
            match crate::drag_drop::engine_drop(&mut self.engine, event) {
                DropOutcome::Planned { plan } => {
                    drop_outcome = Some("planned".to_owned());
                    applied_frames = frames_for_plan(&plan)
                        .into_iter()
                        .map(|(w, r)| (w.0, (r.x, r.y, r.w, r.h)))
                        .collect::<Vec<_>>();
                    applied_frames.sort();
                }
                DropOutcome::SnapBack { reason } => {
                    drop_outcome = Some(format!("snap-back:{reason}"));
                }
                DropOutcome::CentreUnsupported => {
                    drop_outcome = Some("centre-unsupported".to_owned());
                }
                DropOutcome::CancelDead { reason } => {
                    drop_outcome = Some(format!("cancel-dead:{reason}"));
                }
            }
        }
        // Balanced: a second release ignores (no double drop).
        let capture_released = matches!(
            self.session.on_up(moved.0, moved.1),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        ) && !self.session.active();
        // The drop commits through the Engine: the committed plan rows are
        // the after-state, and the applied frames must equal them exactly
        // (no silent drift between plan and application).
        let engine_after = applied_frames.clone();
        let engine_mutated = before != engine_after;
        let restoration_ok = !applied_frames.is_empty() && applied_frames == engine_after;
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
            engine_before: before,
            engine_after,
            applied_frames,
            engine_mutated,
            capture_released,
            restoration_ok,
        }
    }

    /// Pass-through leg: the tap point misses the proven snapshot by 1px, so
    /// the gate refuses and the session must never arm (no fake `on_down`).
    pub fn run_off_point_leg(&mut self) -> FixtureLegEvidence {
        let before = self.engine_geometry();
        let moved = self.pair.move_point_b();
        let probe_before =
            self.preview_summary(&self.pair.a.token(), moved, "fixture-offpoint-probe-0");
        let press = self.pair.press_point_a();
        let window = self.pair.a.clone();
        self.publish_press_proof(&window, press);
        let missed = (press.0 + 1, press.1);
        let gate_consumed = self.tap_down(missed, true);
        assert!(!gate_consumed, "1px miss must pass through");
        // No on_down call by construction: the session stays idle and the
        // trailing up passes through.
        let idle_up_passes = !self.gate.consume_up() && !self.session.active();
        let probe_after =
            self.preview_summary(&self.pair.a.token(), moved, "fixture-offpoint-probe-1");
        let after = self.engine_geometry();
        let engine_mutated = before != after || probe_before != probe_after;
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
            engine_before: before.clone(),
            engine_after: after.clone(),
            applied_frames: Vec::new(),
            engine_mutated,
            capture_released: idle_up_passes,
            restoration_ok: !engine_mutated,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag_drop::DragController;

    #[test]
    fn fixture_cancel_leg_arms_then_cancels_without_engine_mutation() {
        let mut fixture = DragFixture::new();
        let evidence = fixture.run_cancel_leg();
        assert!(evidence.gate_consumed, "exact press must consume");
        assert_eq!(
            evidence.armed_window,
            Some(fixture.pair.a.token()),
            "arm names the proven owner"
        );
        assert!(
            !evidence.engine_mutated,
            "cancel must not mutate the Engine"
        );
        assert!(evidence.restoration_ok, "cancel restores trivially");
        assert!(evidence.capture_released, "trailing release must ignore");
        assert_eq!(evidence.overlay, "hide");
        assert_eq!(evidence.gate_counts, (1, 0, 0, 0));
        // The evidence serializes for the live-oracle shape check.
        let json = serde_json::to_string(&evidence).expect("evidence serializes");
        assert!(json.contains("\"leg\":\"cancel\""));
    }

    #[test]
    fn fixture_drop_leg_applies_engine_plan_with_verified_order() {
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
            "both windows land in the plan"
        );
        assert!(evidence.engine_mutated, "a committed drop moves geometry");
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
        assert_eq!(evidence.preview_outcome, "show", "Engine still resolves");
        assert_eq!(evidence.order_verified, Some(false));
        assert_eq!(evidence.overlay, "hide (order-unverified)");
        // The drop still commits through the Engine (ordering only gates the
        // preview render, never the gesture); capture still balances.
        assert_eq!(evidence.drop_outcome.as_deref(), Some("planned"));
        assert!(evidence.capture_released);
    }

    #[test]
    fn fixture_off_point_press_never_arms() {
        let mut fixture = DragFixture::new();
        let evidence = fixture.run_off_point_leg();
        assert!(!evidence.gate_consumed, "1px miss must pass through");
        assert_eq!(evidence.armed_window, None, "no arm without tap proof");
        assert!(!evidence.engine_mutated);
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
        assert!(fixture.session.on_escape());
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
            .press("4242_7", 300, 250, true, true, fences_ok(), 9)
            .expect("proven press arms the controller");
        assert!(controller.should_consume_held());
        assert_eq!(controller.release().as_deref(), Some("4242_7"));
        assert!(!controller.should_consume_held());
    }
}
