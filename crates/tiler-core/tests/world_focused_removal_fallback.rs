//! Focused-removal MRU fallback at the full-world boundary.
//!
//! Live 3-app partial close (one window closed, its app alive, no native
//! focus move afterwards) wedged the domain: the removed window held
//! session focus, the focus-stack history contained only that leaf
//! (first-report seeding never appended later admissions), convergence
//! fell back to focus none, and every later report refused as
//! focus-mismatch — scope held Unreadable with zero frame authority and
//! no recovery. Closing the last-admitted window kept working, so the
//! failure depended on admission order.
//!
//! Drives the real `WorldEngine::handle_report` path with sequential
//! admits (first-admitted focused, like the live closer), then absence
//! (never an explicit close — the adapter reports omission, which stays
//! a minimize slot per D23), and asserts the scope stays Controlled with
//! reflowed survivors from every close position.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, OutputTopology, ScopeAuthority, WORLD_CONTRACT_VERSION, WorldReply, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-1".to_owned()),
        bounds: rect(0, 0, 2240, 1160),
        gap: 4,
        adjacent: BTreeMap::new(),
    }
}

fn win(id: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-1".to_owned()),
        rect: rect(0, 0, 100, 100),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}

/// One-domain report over `ids` (observation order), observed focus
/// `focused` (empty = none).
fn report(req: u64, ids: &[&str], focused: &str) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "macos-phase-1".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "phase-1".to_owned(),
        observation_seq: req,
        request_seq: req,
        request_id: format!("req-{req}"),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("macos-display-1".to_owned()),
            native_order: vec!["space-1".to_owned()],
            current_native: "space-1".to_owned(),
            complete: true,
            geometry: None,
            primary: true,
        }],
        bindings: Vec::new(),
        domains: vec![DomainReport {
            domain: domain(),
            outer_gap: 8,
            focused: WindowId(focused.to_owned()),
            windows: ids.iter().map(|id| win(id)).collect(),
            closed: Vec::new(),
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn scope_controlled(reply: &WorldReply) {
    assert_eq!(reply.scopes.len(), 1);
    assert_eq!(
        reply.scopes[0].authority,
        ScopeAuthority::Controlled,
        "scope must stay controlled, got {:?}",
        reply.scopes[0].authority
    );
}

/// Controlled rows with effective frames, keyed by lifetime.
fn controlled_frames(reply: &WorldReply) -> BTreeMap<String, Rect> {
    reply
        .windows
        .iter()
        .filter(|row| row.frame_authority == ScopeAuthority::Controlled)
        .filter_map(|row| {
            row.effective_frame
                .map(|frame| (row.lifetime.0.clone(), frame))
        })
        .collect()
}

fn assert_minimized_slot(reply: &WorldReply, window: &str) {
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("expected a row for removed {window}"));
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Minimized),
        "absence stays a minimize slot, never a silent close"
    );
    assert!(row.effective_frame.is_none() && row.allocation.is_none());
}

/// Admit c, then a, then b across three sequential reports (c focused,
/// the live closer shape); returns the engine plus the 3-window reply.
fn admit_three_focused_first() -> (WorldEngine, WorldReply) {
    let mut engine = WorldEngine::new("phase-1").expect("generation");
    accepted(&mut engine, &report(1, &["c"], "c"));
    accepted(&mut engine, &report(2, &["c", "a"], "c"));
    let reply = accepted(&mut engine, &report(3, &["c", "a", "b"], "c"));
    scope_controlled(&reply);
    assert_eq!(controlled_frames(&reply).len(), 3);
    (engine, reply)
}

#[test]
fn close_focused_first_admitted_keeps_scope_controlled() {
    // The live wedge, deterministically: removing the focused
    // first-admitted window by absence must reflow the survivors under a
    // Controlled scope — never hold Unreadable with zero frames.
    let (mut engine, before) = admit_three_focused_first();
    let before_frames = controlled_frames(&before);
    assert_eq!(before_frames.len(), 3);

    let close = accepted(&mut engine, &report(4, &["a", "b"], ""));
    scope_controlled(&close);
    let after = controlled_frames(&close);
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        vec!["a", "b"],
        "exactly the survivors keep frame authority"
    );
    assert_ne!(
        after, before_frames,
        "survivors must reflow, not retain pre-close frames"
    );
    assert_minimized_slot(&close, "c");

    // Staggered survivor re-observations (still unfocused) must keep
    // converging — the pre-fix wedge held every later report too.
    let reobs = accepted(&mut engine, &report(5, &["a", "b"], ""));
    scope_controlled(&reobs);
    assert_eq!(controlled_frames(&reobs), after);
}

#[test]
fn close_each_position_reflows_survivors() {
    // Every close position (first/middle/last admitted) reflows under a
    // Controlled scope with the removed lifetime kept as a minimize slot.
    for (removed, survivors) in [
        ("c", vec!["a", "b"]),
        ("a", vec!["c", "b"]),
        ("b", vec!["c", "a"]),
    ] {
        let mut engine = WorldEngine::new("phase-1").expect("generation");
        accepted(&mut engine, &report(1, &["c"], "c"));
        accepted(&mut engine, &report(2, &["c", "a"], "c"));
        let before = accepted(&mut engine, &report(3, &["c", "a", "b"], "c"));
        scope_controlled(&before);

        let remaining: Vec<&str> = ["c", "a", "b"]
            .into_iter()
            .filter(|id| *id != removed)
            .collect();
        let close = accepted(&mut engine, &report(4, &remaining, ""));
        scope_controlled(&close);
        let frames = controlled_frames(&close);
        assert_eq!(
            frames.len(),
            2,
            "removing {removed}: survivors keep frames: {frames:?}"
        );
        for survivor in &survivors {
            assert!(
                frames.contains_key(*survivor),
                "removing {removed}: {survivor} keeps frame authority"
            );
        }
        // Survivors tile without overlap inside the work area.
        let rects: Vec<Rect> = survivors.iter().map(|id| frames[*id]).collect();
        let (r0, r1) = (rects[0], rects[1]);
        let disjoint_x = r0.x + r0.w <= r1.x || r1.x + r1.w <= r0.x;
        let disjoint_y = r0.y + r0.h <= r1.y || r1.y + r1.h <= r0.y;
        assert!(
            disjoint_x || disjoint_y,
            "removing {removed}: survivors must not overlap: {rects:?}"
        );
        assert_minimized_slot(&close, removed);
    }
}
