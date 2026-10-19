#![forbid(unsafe_code)]

//! Producer-shaped trailing-binding integration regressions (blocker 1).
//!
//! These exercise the real shared-core contract end to end with only the
//! native leg injected: a real [`WorldEngine`] mints the trailing token,
//! the real [`build_report`](crate::world_builder::build_report) carries
//! the binding, and a scripted [`SelectionBackend`] supplies the exact
//! native append proof. Hand-stitched `TrailingSendPrep` values are never
//! substituted: the select path runs the real
//! [`SelectionState::execute`](crate::workspace_select::SelectionState::execute)
//! and the send path runs the real
//! [`prepare_trailing_send_target`](crate::workspace_send::prepare_trailing_send_target).
//! Capability flags stay `false` throughout: trailing admission arms only
//! through the explicit `true` locals passed to the capability-explicit
//! seams, exactly like the existing trailing unit tests.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::{DomainKey, Session};
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, HoldReason, WorldCommand, WorldReply, WorldReport,
};

use crate::runtime_types::{NativeContext, NativeDomain, ObservedWindow, PHASE1_OUTER_GAP};
use crate::trailing_binding::{TrailingBindingStore, unbound_trailing_token};
use crate::workspace_select::{SelectionBackend, SelectionSnapshot};
use crate::world_builder::{ReportInput, build_report};

const OWNER: &str = "macos-phase-1";
const GENERATION: &str = "gen-1";
const INCARNATION: &str = "adapter-1";
const UUID_1: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";
const PID_A: u32 = 4242;
const PID_B: u32 = 4243;

/// Scripted native leg: the ONLY fake in these regressions. Models one
/// display's user-Space row with an exact-append create and a verified
/// select; emptiness reads from explicit proof sets.
struct FakeBackend {
    display: String,
    spaces: Vec<u64>,
    current: u64,
    revision: u64,
    occupied: BTreeSet<u64>,
    unknown: BTreeSet<u64>,
    next_sid: u64,
}

impl FakeBackend {
    fn new(spaces: &[u64], current: u64, occupied: &[u64]) -> Self {
        Self {
            display: "display-1".to_owned(),
            spaces: spaces.to_vec(),
            current,
            revision: 9,
            occupied: occupied.iter().copied().collect(),
            unknown: BTreeSet::new(),
            next_sid: spaces.iter().copied().max().unwrap_or(0) + 1,
        }
    }
}

impl SelectionBackend for FakeBackend {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(SelectionSnapshot {
            display: self.display.clone(),
            spaces: self.spaces.clone(),
            current: self.current,
        })
    }

    fn display_count(&mut self) -> Result<usize, String> {
        Ok(1)
    }

    fn revision(&mut self) -> Option<u64> {
        Some(self.revision)
    }

    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        if self.unknown.contains(&sid) {
            return Err("scripted unknown".to_owned());
        }
        Ok(!self.occupied.contains(&sid))
    }

    fn create(&mut self, _before: &SelectionSnapshot) -> Result<(), String> {
        let sid = self.next_sid;
        self.next_sid += 1;
        self.spaces.push(sid);
        self.revision += 1;
        Ok(())
    }

    fn select(&mut self, _before: &SelectionSnapshot, sid: u64) -> Result<(), String> {
        if !self.spaces.contains(&sid) {
            return Err("scripted select of absent Space".to_owned());
        }
        self.current = sid;
        self.revision += 1;
        Ok(())
    }
}

fn output() -> OutputId {
    OutputId(crate::topology::output_key(1))
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn native_domain(space: u64, current: bool) -> NativeDomain {
    NativeDomain {
        cg_id: 1,
        uuid: Some(UUID_1.to_owned()),
        output: output(),
        workspace: WorkspaceId(crate::topology::workspace_key(space)),
        space,
        current,
        readable: true,
        bounds: rect(0, 0, 1440, 900),
        visible: rect(0, 25, 1440, 875),
    }
}

fn ctx_for(spaces: &[u64], current: u64) -> NativeContext {
    NativeContext {
        revision: 9,
        domains: spaces
            .iter()
            .map(|space| native_domain(*space, *space == current))
            .collect(),
        skipped: Vec::new(),
    }
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

fn key(workspace: &str) -> DomainKey {
    DomainKey {
        output: output(),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn window_id(pid: u32, wid: u32) -> String {
    crate::runtime_types::window_id(pid, wid)
}

fn no_focus() -> BTreeMap<DomainKey, WindowId> {
    BTreeMap::new()
}

/// Build one full world report through the real builder, carrying `bindings`.
#[allow(clippy::too_many_arguments)]
fn assemble(
    ctx: &NativeContext,
    members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    focused: &BTreeMap<DomainKey, WindowId>,
    bindings: &[tiler_core::world_report::WorkspaceBinding],
    command: Option<WorldCommand>,
    command_seq: Option<u64>,
    feedback: &[AttemptFeedback],
    obs: u64,
    req: u64,
) -> WorldReport {
    let sessions: BTreeMap<DomainKey, Session> = BTreeMap::new();
    let closed: BTreeMap<DomainKey, Vec<WindowId>> = BTreeMap::new();
    let unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
    let request_id = format!("req-{req}");
    let mut input = ReportInput::new(
        OWNER,
        INCARNATION,
        GENERATION,
        ctx,
        obs,
        req,
        &request_id,
        members,
        &sessions,
        focused,
        &closed,
        &unreadable,
        &[],
        feedback,
        "",
    );
    input.command = command;
    input.command_seq = command_seq;
    input.bindings = bindings;
    input.complete = true;
    input.domain_outer_gap = PHASE1_OUTER_GAP;
    build_report(&input)
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn unbound_ids(reply: &WorldReply) -> Vec<String> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.native_binding.is_none())
        .filter_map(|row| row.logical_id.clone())
        .collect()
}

fn has_bound_row(reply: &WorldReply, logical: &str, native: &str) -> bool {
    reply.workspaces.iter().any(|row| {
        row.logical_id.as_deref() == Some(logical) && row.native_binding.as_deref() == Some(native)
    })
}

/// Select convergence: real `SelectTrailing` command -> real unbound token
/// -> real native append+select through the injected backend -> binding
/// report -> core settles the logical View without any core change, the
/// mapping adopts the new current, a second report is stable, and a later
/// occupied trailing mints a fresh token.
#[test]
fn trailing_select_binding_converges_view_and_adopts_current() {
    let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
    let mut store = TrailingBindingStore::default();
    // Space-277 holds window B so the trailing slot needs an append.
    let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    members.insert(key("space-277"), vec![(PID_B, observed(9))]);

    // Observation-only baseline: lifecycle mints the pending token.
    let ctx = ctx_for(&[1, 277], 1);
    let empty: Vec<tiler_core::world_report::WorkspaceBinding> = Vec::new();
    let reply = accepted(
        &mut engine,
        &assemble(&ctx, &members, &no_focus(), &empty, None, None, &[], 1, 1),
    );
    let logical =
        unbound_trailing_token(&reply, &output()).expect("engine desires a trailing token");
    assert_eq!(unbound_ids(&reply), vec![logical.clone()]);

    // Real trailing command against the same topology.
    let reply = accepted(
        &mut engine,
        &assemble(
            &ctx,
            &members,
            &no_focus(),
            &empty,
            Some(WorldCommand::SelectTrailing { output: output() }),
            Some(2),
            &[],
            2,
            2,
        ),
    );
    assert_eq!(
        unbound_trailing_token(&reply, &output()),
        Some(logical.clone()),
        "stable token across the command"
    );
    assert_eq!(
        engine.retained_transfers().len(),
        1,
        "logical View retained"
    );

    // Real native leg through the injected backend only.
    let before = vec![1, 277];
    let mut backend = FakeBackend::new(&before, 1, &[277]);
    let mut select_state = crate::workspace_select::SelectionState::default();
    let effect = select_state
        .execute(
            &mut backend,
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 },
        )
        .expect("native append+select proves");
    assert_eq!(effect.created, Some(278));
    assert_eq!(effect.target, 278);
    let after = backend.spaces.clone();
    store
        .bind_exact_append(&logical, &output(), 278, &before, &after)
        .expect("exact append binds the engine token");

    // Binding report: new native current, same windows, binding carried.
    let staged = store.report_bindings();
    let ctx = ctx_for(&[1, 277, 278], 278);
    let report = assemble(&ctx, &members, &no_focus(), &staged, None, None, &[], 3, 3);
    assert_eq!(report.bindings, staged, "full report carries the binding");
    let reply = accepted(&mut engine, &report);
    assert!(
        engine.retained_transfers().is_empty(),
        "logical View settles on new current"
    );
    assert!(unbound_ids(&reply).is_empty(), "pending token resolved");
    assert!(
        has_bound_row(&reply, &logical, "space-278"),
        "core publishes the bound row"
    );
    assert!(
        reply.workspaces.iter().any(|row| row.id == "space-278"),
        "mapping adopts the new current"
    );
    assert_eq!(
        store.note_ack(&reply),
        vec![logical.clone()],
        "real core ack releases"
    );
    assert!(store.is_empty());

    // Second report with identical topology: stable, nothing pending.
    let report = assemble(&ctx, &members, &no_focus(), &[], None, None, &[], 4, 4);
    let reply = accepted(&mut engine, &report);
    assert!(engine.retained_transfers().is_empty());
    assert!(unbound_ids(&reply).is_empty());
    assert!(
        reply.workspaces.iter().any(|row| row.id == "space-278"),
        "adopted native persists without re-binding"
    );

    // Later the new trailing fills: a fresh command mints a fresh token.
    members.insert(key("space-278"), vec![(PID_A, observed(7))]);
    let reply = accepted(
        &mut engine,
        &assemble(
            &ctx,
            &members,
            &no_focus(),
            &[],
            Some(WorldCommand::SelectTrailing { output: output() }),
            Some(5),
            &[],
            5,
            5,
        ),
    );
    let fresh = unbound_trailing_token(&reply, &output()).expect("occupied trailing mints again");
    assert_ne!(
        fresh, logical,
        "later occupied trailing yields a fresh token"
    );
}

/// Send convergence: real `SendNumbered` trailing command pends under the
/// core -> real native preparation through the injected backend -> binding
/// report starts the engine send against the native target (never the
/// token) -> Applied feedback with observed arrival settles it.
#[test]
fn trailing_send_binding_starts_native_send_and_settles() {
    let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
    let mut store = TrailingBindingStore::default();
    let win_a = window_id(PID_A, 7);
    let win_b = window_id(PID_B, 9);
    let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    members.insert(key("space-1"), vec![(PID_A, observed(7))]);
    members.insert(key("space-277"), vec![(PID_B, observed(9))]);
    // Observed focus truth per domain: the engine send refuses unfocused
    // sources, exactly like the core's own send fixtures.
    let mut focused: BTreeMap<DomainKey, WindowId> = BTreeMap::new();
    focused.insert(key("space-1"), WindowId(win_a.clone()));
    focused.insert(key("space-277"), WindowId(win_b.clone()));
    let ctx = ctx_for(&[1, 277], 1);
    let empty: Vec<tiler_core::world_report::WorkspaceBinding> = Vec::new();
    let _ = accepted(
        &mut engine,
        &assemble(&ctx, &members, &focused, &empty, None, None, &[], 1, 1),
    );

    // Real trailing send command: pends under the core, no view target.
    let reply = accepted(
        &mut engine,
        &assemble(
            &ctx,
            &members,
            &focused,
            &empty,
            Some(WorldCommand::SendNumbered {
                window: WindowId(win_a.clone()),
                output: output(),
                index: 0,
                follow: true,
            }),
            Some(2),
            &[],
            2,
            2,
        ),
    );
    let logical = unbound_trailing_token(&reply, &output()).expect("send pends a trailing token");
    assert!(
        engine.retained_transfers().is_empty(),
        "pending send retains nothing yet"
    );

    // Real native preparation through the injected backend only.
    let before = vec![1, 277];
    let mut backend = FakeBackend::new(&before, 1, &[277]);
    let prep = crate::workspace_send::prepare_trailing_send_target(&mut backend, true)
        .expect("native append proves");
    assert_eq!(prep.target_sid, 278);
    assert_eq!(prep.created, Some(278));
    let after = backend.spaces.clone();
    store
        .bind_exact_append(&logical, &output(), prep.target_sid, &before, &after)
        .expect("exact append binds the send token");

    // Binding report: the pending send starts against the native target.
    let staged = store.report_bindings();
    let ctx = ctx_for(&[1, 277, 278], 1);
    let report = assemble(&ctx, &members, &focused, &staged, None, None, &[], 3, 3);
    let reply = accepted(&mut engine, &report);
    let retained = engine.retained_transfers();
    assert_eq!(retained.len(), 1, "binding drains the pending send");
    assert_eq!(
        retained[0].operation_id, "op-2",
        "stable operation identity"
    );
    assert_eq!(
        retained[0].target_workspace, "space-278",
        "native target, never the token"
    );
    assert!(unbound_ids(&reply).is_empty(), "pending token resolved");
    assert!(has_bound_row(&reply, &logical, "space-278"));
    assert_eq!(store.note_ack(&reply), vec![logical.clone()]);
    assert!(store.is_empty());

    // Observed arrival plus Applied feedback settles the transfer.
    let mut moved = members.clone();
    moved.remove(&key("space-1"));
    moved.insert(key("space-278"), vec![(PID_A, observed(7))]);
    let mut followed = BTreeMap::new();
    followed.insert(key("space-277"), WindowId(win_b.clone()));
    followed.insert(key("space-278"), WindowId(win_a.clone()));
    let ctx = ctx_for(&[1, 277, 278], 278);
    let feedback = vec![AttemptFeedback {
        operation_id: "op-2".to_owned(),
        kind: AttemptKind::Transfer,
        outcome: AttemptOutcome::Applied,
        hold: None,
    }];
    let _ = accepted(
        &mut engine,
        &assemble(&ctx, &moved, &followed, &[], None, None, &feedback, 4, 4),
    );
    assert!(
        engine.retained_transfers().is_empty(),
        "arrival plus Applied settles"
    );
}

/// Drift and stale refusal: a vanished native prunes (pending honestly
/// persists, no false ack) and an invented token resolves nothing real.
#[test]
fn trailing_binding_drift_and_stale_refuse() {
    let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
    let mut store = TrailingBindingStore::default();
    let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    members.insert(key("space-277"), vec![(PID_B, observed(9))]);
    let ctx = ctx_for(&[1, 277], 1);
    let empty: Vec<tiler_core::world_report::WorkspaceBinding> = Vec::new();
    let reply = accepted(
        &mut engine,
        &assemble(&ctx, &members, &no_focus(), &empty, None, None, &[], 1, 1),
    );
    let logical = unbound_trailing_token(&reply, &output()).expect("pending token minted");
    let _ = accepted(
        &mut engine,
        &assemble(
            &ctx,
            &members,
            &no_focus(),
            &empty,
            Some(WorldCommand::SelectTrailing { output: output() }),
            Some(2),
            &[],
            2,
            2,
        ),
    );
    store
        .bind_exact_append(&logical, &output(), 278, &[1, 277], &[1, 277, 278])
        .expect("honest append binds");

    // The native vanishes before the binding ever reports: prune drops it.
    let mut live: BTreeMap<OutputId, Vec<String>> = BTreeMap::new();
    live.insert(output(), vec!["space-1".to_owned(), "space-277".to_owned()]);
    assert_eq!(store.prune_vanished(&live), vec![logical.clone()]);
    let reply = accepted(
        &mut engine,
        &assemble(&ctx, &members, &no_focus(), &[], None, None, &[], 3, 3),
    );
    assert_eq!(
        unbound_trailing_token(&reply, &output()),
        Some(logical.clone()),
        "pending persists without the binding: no false resolution"
    );
    assert_eq!(
        engine.retained_transfers().len(),
        1,
        "logical View still retained"
    );

    // An invented token tied to a real append resolves nothing real: the
    // trailing stays occupied, so the genuine pending token persists.
    members.insert(key("space-278"), vec![(PID_A, observed(7))]);
    let invented = tiler_core::world_report::WorkspaceBinding {
        logical_id: "wl-999".to_owned(),
        native_id: "space-278".to_owned(),
        output: output(),
        pinned: false,
        displaced: false,
        managed: true,
    };
    let ctx = ctx_for(&[1, 277, 278], 1);
    let reply = accepted(
        &mut engine,
        &assemble(
            &ctx,
            &members,
            &no_focus(),
            &[invented],
            None,
            None,
            &[],
            4,
            4,
        ),
    );
    assert_eq!(
        unbound_trailing_token(&reply, &output()),
        Some(logical.clone()),
        "invented token never resolves the real pending creation"
    );
    assert!(
        store.note_ack(&reply).is_empty(),
        "no false ack off invented rows"
    );
}

/// The builder carries explicit bindings verbatim and the core accepts
/// them: the trailing contract rides the full-report path, not a side
/// channel.
#[test]
fn builder_carries_explicit_bindings_for_core() {
    let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
    let ctx = ctx_for(&[1, 277, 278], 1);
    let bindings = vec![tiler_core::world_report::WorkspaceBinding {
        logical_id: "wl-1".to_owned(),
        native_id: "space-278".to_owned(),
        output: output(),
        pinned: false,
        displaced: false,
        managed: true,
    }];
    let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
    let report = assemble(
        &ctx,
        &members,
        &no_focus(),
        &bindings,
        None,
        None,
        &[],
        1,
        1,
    );
    assert_eq!(report.bindings, bindings);
    assert!(tiler_core::world_report::validate_report(&report));
    let _ = accepted(&mut engine, &report);
}

/// Same-tick proof plus Applied feedback: production reports the exact
/// native append proof and the Applied attempt outcome on the same
/// main-tick handoff (select drain: proof plus View Applied; send
/// settle: proof plus Transfer Applied), so the next full report carries
/// BOTH rows at once. This pins that one combined report converges each
/// lane with no retained transfer: the select lane leaves no unbound row
/// (the new trailing stays reusable), while the settled send occupies
/// the new trailing so the core mints a fresh onward token (the settled
/// pending resolved, never retained). The following observation-only
/// report stays stable with no replay in both lanes.
///
/// Real engine plus real builder throughout; only the native leg is the
/// injected backend. The store seam below is the exact function the
/// `EngineJob::TrailingProof` lane invokes through
/// `session.adopt_trailing_binding` (see `runtime_thread::process_job`),
/// and each feedback row is the exact
/// `AttemptFeedback` shape the main stages beside it (operation identity
/// read off retained truth, never guessed). Capability flags stay
/// `false`: no arming, explicit seams only.
#[test]
fn trailing_combined_proof_and_applied_feedback_converge_in_single_report() {
    // -- Select lane: View Applied plus binding in one report. --
    {
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut store = TrailingBindingStore::default();
        let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
        members.insert(key("space-277"), vec![(PID_B, observed(9))]);
        let ctx = ctx_for(&[1, 277], 1);
        let empty: Vec<tiler_core::world_report::WorkspaceBinding> = Vec::new();
        let reply = accepted(
            &mut engine,
            &assemble(&ctx, &members, &no_focus(), &empty, None, None, &[], 1, 1),
        );
        let logical =
            unbound_trailing_token(&reply, &output()).expect("engine desires a trailing token");
        let reply = accepted(
            &mut engine,
            &assemble(
                &ctx,
                &members,
                &no_focus(),
                &empty,
                Some(WorldCommand::SelectTrailing { output: output() }),
                Some(2),
                &[],
                2,
                2,
            ),
        );
        assert_eq!(
            unbound_trailing_token(&reply, &output()),
            Some(logical.clone())
        );
        assert_eq!(engine.retained_transfers().len(), 1);
        let view_op = engine.retained_transfers()[0].operation_id.clone();

        // Real native leg through the injected backend only.
        let before = vec![1, 277];
        let mut backend = FakeBackend::new(&before, 1, &[277]);
        let mut select_state = crate::workspace_select::SelectionState::default();
        let effect = select_state
            .execute(
                &mut backend,
                crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 },
            )
            .expect("native append+select proves");
        assert_eq!((effect.created, effect.target), (Some(278), 278));
        let after = backend.spaces.clone();
        store
            .bind_exact_append(&logical, &output(), 278, &before, &after)
            .expect("exact append binds the engine token");

        // Same-tick handoff in one report: binding plus View Applied.
        let staged = store.report_bindings();
        let feedback = vec![AttemptFeedback {
            operation_id: view_op,
            kind: AttemptKind::View,
            outcome: AttemptOutcome::Applied,
            hold: None,
        }];
        let ctx = ctx_for(&[1, 277, 278], 278);
        let report = assemble(
            &ctx,
            &members,
            &no_focus(),
            &staged,
            None,
            None,
            &feedback,
            3,
            3,
        );
        assert_eq!(report.bindings, staged, "report carries the binding");
        assert_eq!(report.feedback, feedback, "report carries the outcome");
        let reply = accepted(&mut engine, &report);
        assert!(
            engine.retained_transfers().is_empty(),
            "logical View settles on the combined report"
        );
        assert!(unbound_ids(&reply).is_empty(), "pending token resolved");
        assert!(has_bound_row(&reply, &logical, "space-278"));
        assert!(
            reply.workspaces.iter().any(|row| row.id == "space-278"),
            "mapping adopts the new current"
        );
        assert_eq!(store.note_ack(&reply), vec![logical.clone()]);
        assert!(store.is_empty());

        // Next observation: stable, nothing pending, nothing replayed.
        let reply = accepted(
            &mut engine,
            &assemble(&ctx, &members, &no_focus(), &[], None, None, &[], 4, 4),
        );
        assert!(engine.retained_transfers().is_empty());
        assert!(unbound_ids(&reply).is_empty());
        assert!(
            reply.workspaces.iter().any(|row| row.id == "space-278"),
            "adopted native persists without re-binding"
        );
    }

    // -- Send lane: Transfer Applied plus binding in one report. --
    {
        let mut engine = WorldEngine::new(GENERATION).expect("valid generation");
        let mut store = TrailingBindingStore::default();
        let win_a = window_id(PID_A, 7);
        let win_b = window_id(PID_B, 9);
        let mut members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::new();
        members.insert(key("space-1"), vec![(PID_A, observed(7))]);
        members.insert(key("space-277"), vec![(PID_B, observed(9))]);
        let mut focused: BTreeMap<DomainKey, WindowId> = BTreeMap::new();
        focused.insert(key("space-1"), WindowId(win_a.clone()));
        focused.insert(key("space-277"), WindowId(win_b.clone()));
        let ctx = ctx_for(&[1, 277], 1);
        let empty: Vec<tiler_core::world_report::WorkspaceBinding> = Vec::new();
        let _ = accepted(
            &mut engine,
            &assemble(&ctx, &members, &focused, &empty, None, None, &[], 1, 1),
        );
        let reply = accepted(
            &mut engine,
            &assemble(
                &ctx,
                &members,
                &focused,
                &empty,
                Some(WorldCommand::SendNumbered {
                    window: WindowId(win_a.clone()),
                    output: output(),
                    index: 0,
                    follow: true,
                }),
                Some(2),
                &[],
                2,
                2,
            ),
        );
        let logical =
            unbound_trailing_token(&reply, &output()).expect("send pends a trailing token");

        // Real native preparation through the injected backend only.
        let before = vec![1, 277];
        let mut backend = FakeBackend::new(&before, 1, &[277]);
        let prep = crate::workspace_send::prepare_trailing_send_target(&mut backend, true)
            .expect("native append proves");
        assert_eq!((prep.target_sid, prep.created), (278, Some(278)));
        let after = backend.spaces.clone();
        store
            .bind_exact_append(&logical, &output(), prep.target_sid, &before, &after)
            .expect("exact append binds the send token");

        // Same-tick handoff in one report: binding plus Transfer Applied
        // with observed arrival (the owner drag applied and followed).
        let staged = store.report_bindings();
        let mut moved = members.clone();
        moved.remove(&key("space-1"));
        moved.insert(key("space-278"), vec![(PID_A, observed(7))]);
        let mut followed = BTreeMap::new();
        followed.insert(key("space-277"), WindowId(win_b.clone()));
        followed.insert(key("space-278"), WindowId(win_a.clone()));
        let ctx = ctx_for(&[1, 277, 278], 278);
        let feedback = vec![AttemptFeedback {
            operation_id: "op-2".to_owned(),
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::Applied,
            hold: None,
        }];
        let report = assemble(
            &ctx, &moved, &followed, &staged, None, None, &feedback, 3, 3,
        );
        assert_eq!(report.bindings, staged, "report carries the binding");
        assert_eq!(report.feedback, feedback, "report carries the outcome");
        let reply = accepted(&mut engine, &report);
        assert!(
            engine.retained_transfers().is_empty(),
            "arrival plus Applied settles on the combined report"
        );
        // The settled send occupied the new trailing, so the core desires
        // onward trailing under a FRESH token: the settled `logical`
        // pending resolved (never retained), and only the next desire
        // rides unbound.
        let fresh =
            unbound_trailing_token(&reply, &output()).expect("occupied trailing mints onward");
        assert_ne!(
            fresh, logical,
            "settled pending resolved; only the next desire rides unbound"
        );
        assert!(has_bound_row(&reply, &logical, "space-278"));
        assert_eq!(store.note_ack(&reply), vec![logical.clone()]);
        assert!(store.is_empty());

        // Next observation: stable, nothing retained, same onward desire,
        // nothing replayed.
        let reply = accepted(
            &mut engine,
            &assemble(&ctx, &moved, &followed, &[], None, None, &[], 4, 4),
        );
        assert!(engine.retained_transfers().is_empty());
        assert_eq!(
            unbound_trailing_token(&reply, &output()),
            Some(fresh),
            "onward desire stable without re-binding"
        );
        assert!(
            reply.workspaces.iter().any(|row| row.id == "space-278"),
            "settled native persists without re-binding"
        );
    }
}
