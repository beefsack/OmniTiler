//! Core-owned frame policy tests (stage 3a): clamp suppression/rearm,
//! transient retry/backoff over deterministic reported time, DPI
//! invalidation, unreadable/minimized/unknown evidence, and close/
//! replacement pruning. Pure policy plus engine publish behavior; no
//! adapter, transport, or clock.

use super::world_frames::{
    FrameMemory, consume_attempt, dpi_basis_changed, is_stale_replay, transient_backoff_ms,
};
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    FrameAttempt, FrameSetter, HoldReason, ScopeAuthority, ScreenReport, ScreenView,
    WORLD_CONTRACT_VERSION, WindowFact, WorldReport,
};

pub(crate) fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

pub(crate) fn attempt(
    window: &str,
    target: Rect,
    setter: FrameSetter,
    readback: Option<Rect>,
    ms: u64,
) -> FrameAttempt {
    FrameAttempt {
        window: WindowId(window.to_owned()),
        target,
        setter,
        readback,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: ms,
        operation_id: None,
        prewrite: None,
    }
}

fn dpi_attempt(
    window: &str,
    target: Rect,
    readback: Option<Rect>,
    before: Option<u32>,
    after: Option<u32>,
    ms: u64,
) -> FrameAttempt {
    FrameAttempt {
        window: WindowId(window.to_owned()),
        target,
        setter: FrameSetter::Accepted,
        readback,
        dpi_before: before,
        dpi_after: after,
        attempt_ms: ms,
        operation_id: None,
        prewrite: None,
    }
}

// --- pure backoff shape ---

#[test]
fn transient_backoff_doubles_to_a_5s_cap() {
    assert_eq!(transient_backoff_ms(0), 500);
    assert_eq!(transient_backoff_ms(1), 1000);
    assert_eq!(transient_backoff_ms(2), 2000);
    assert_eq!(transient_backoff_ms(3), 4000);
    assert_eq!(transient_backoff_ms(4), 5000);
    assert_eq!(transient_backoff_ms(10), 5000);
}

#[test]
fn dpi_basis_changed_needs_both_sides_and_a_delta() {
    assert!(dpi_basis_changed(Some(96), Some(120)));
    assert!(!dpi_basis_changed(Some(120), Some(120)));
    assert!(!dpi_basis_changed(None, Some(120)));
    assert!(!dpi_basis_changed(Some(96), None));
    assert!(!dpi_basis_changed(None, None));
}

// --- pure clamp lane ---

#[test]
fn clamp_suppresses_identical_pair_and_rearms_on_change() {
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let memory = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Accepted, Some(observed), 100),
        Some(observed),
    )
    .expect("clamp records");
    assert!(memory.clamped);
    assert!(memory.suppressed(&desired, &observed, 9999));
    // Genuine desired or observation change re-arms immediately.
    assert!(!memory.suppressed(&rect(0, 0, 900, 600), &observed, 100));
    assert!(!memory.suppressed(&desired, &rect(0, 0, 710, 600), 100));
}

#[test]
fn exact_readback_clears_both_lanes() {
    let desired = rect(0, 0, 800, 600);
    let held = rect(0, 0, 700, 600);
    let clamped = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Accepted, Some(held), 100),
        Some(held),
    )
    .expect("clamp records");
    assert!(
        consume_attempt(
            Some(clamped),
            &attempt("w1", desired, FrameSetter::Accepted, Some(desired), 200),
            Some(desired),
        )
        .is_none()
    );
    // Exact arrival with no write behind it clears too.
    let transient = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Refused, Some(held), 100),
        Some(held),
    )
    .expect("transient records");
    assert!(
        consume_attempt(
            Some(transient),
            &attempt("w1", desired, FrameSetter::Refused, Some(desired), 200),
            Some(desired),
        )
        .is_none()
    );
}

// --- pure transient lane ---

#[test]
fn transient_backoff_uses_reported_time_and_rearms_on_change() {
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let first = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Refused, Some(observed), 1000),
        Some(observed),
    )
    .expect("transient records");
    assert!(!first.clamped);
    assert_eq!(first.failures, 0);
    // Inside the 500ms window: suppressed; after it: retry.
    assert!(first.suppressed(&desired, &observed, 1200));
    assert!(!first.suppressed(&desired, &observed, 1600));
    // Same-pair repeat doubles the backoff.
    let second = consume_attempt(
        Some(first),
        &attempt("w1", desired, FrameSetter::Threw, Some(observed), 1600),
        Some(observed),
    )
    .expect("transient repeats");
    assert_eq!(second.failures, 1);
    assert!(second.suppressed(&desired, &observed, 2000));
    assert!(!second.suppressed(&desired, &observed, 2700));
    // New intent re-arms immediately even inside backoff.
    assert!(!second.suppressed(&rect(0, 0, 900, 600), &observed, 1600));
    assert!(!second.suppressed(&desired, &rect(0, 0, 701, 600), 1600));
}

#[test]
fn transient_backoff_caps_and_never_pins() {
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let mut memory: Option<FrameMemory> = None;
    for round in 0..11 {
        memory = consume_attempt(
            memory,
            &attempt(
                "w1",
                desired,
                FrameSetter::Refused,
                Some(observed),
                1000 + round * 6000,
            ),
            Some(observed),
        );
    }
    let memory = memory.expect("transient records");
    assert_eq!(transient_backoff_ms(memory.failures), 5000);
    let last = memory.last_ms;
    assert!(memory.suppressed(&desired, &observed, last + 4000));
    assert!(!memory.suppressed(&desired, &observed, last + 6000));
}

#[test]
fn stale_replay_guard_ignores_only_real_redeliveries() {
    let desired = rect(0, 0, 800, 600);
    let held = rect(0, 0, 700, 600);
    let prior = attempt("w1", desired, FrameSetter::Accepted, Some(held), 1000);
    // Older stamp, same or different pair: stale either way.
    assert!(is_stale_replay(
        &prior,
        &attempt("w1", desired, FrameSetter::Accepted, Some(held), 999),
    ));
    assert!(is_stale_replay(
        &prior,
        &attempt(
            "w1",
            rect(0, 0, 900, 600),
            FrameSetter::Refused,
            Some(held),
            999,
        ),
    ));
    // Exact duplicate at the same stamp: the same write reported twice.
    assert!(is_stale_replay(
        &prior,
        &attempt("w1", desired, FrameSetter::Accepted, Some(held), 1000),
    ));
    // Changed pair at an equal-or-newer stamp always processes: time is
    // not a unique attempt identity.
    assert!(!is_stale_replay(
        &prior,
        &attempt(
            "w1",
            rect(0, 0, 900, 600),
            FrameSetter::Refused,
            Some(held),
            1000,
        ),
    ));
    assert!(!is_stale_replay(
        &prior,
        &attempt("w1", desired, FrameSetter::Accepted, Some(held), 1001),
    ));
    // Unreported clock never dedups: no ordering claim is possible.
    let clockless = attempt("w1", desired, FrameSetter::Refused, Some(held), 0);
    assert!(!is_stale_replay(&prior, &clockless));
    assert!(!is_stale_replay(&clockless, &clockless));
}

#[test]
fn stale_redelivery_never_renews_backoff() {
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let first = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Refused, Some(observed), 1000),
        Some(observed),
    )
    .expect("records");
    let second = consume_attempt(
        Some(first.clone()),
        &attempt("w1", desired, FrameSetter::Refused, Some(observed), 1600),
        Some(observed),
    )
    .expect("repeats");
    assert_eq!(second.failures, 1);
    // The lost-ack redelivery of the first attempt changes nothing.
    let replayed = consume_attempt(
        Some(second.clone()),
        &attempt("w1", desired, FrameSetter::Refused, Some(observed), 1000),
        Some(observed),
    )
    .expect("entry survives");
    assert_eq!(replayed, second);
}

#[test]
fn unknown_clock_never_suppresses_transient_but_keeps_clamp() {
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let transient = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Refused, Some(observed), 0),
        Some(observed),
    )
    .expect("records");
    assert!(!transient.suppressed(&desired, &observed, 0));
    assert!(!transient.suppressed(&desired, &observed, 9999));
    let clamped = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Accepted, Some(observed), 0),
        Some(observed),
    )
    .expect("records");
    assert!(clamped.suppressed(&desired, &observed, 0));
}

// --- evidence gates ---

#[test]
fn unreadable_evidence_records_nothing() {
    let desired = rect(0, 0, 800, 600);
    assert!(
        consume_attempt(
            None,
            &attempt("w1", desired, FrameSetter::Accepted, None, 100),
            None,
        )
        .is_none()
    );
    assert!(
        consume_attempt(
            None,
            &attempt("w1", desired, FrameSetter::Threw, None, 100),
            None,
        )
        .is_none()
    );
    // A throwing setter with a readable staged fact still keys the pair.
    let fact = Some(rect(0, 0, 700, 600));
    assert!(
        consume_attempt(
            None,
            &attempt("w1", desired, FrameSetter::Threw, None, 100),
            fact,
        )
        .is_some()
    );
}

#[test]
fn accepted_unreadable_proof_retries_with_backoff() {
    // An accepted setter with unreadable post-write proof is a transient
    // keyed on the staged fact, never a permanent clamp: 500ms, then
    // 1000ms recovery, mirroring the refused lane.
    let desired = rect(0, 0, 800, 600);
    let observed = rect(0, 0, 700, 600);
    let first = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Accepted, None, 1000),
        Some(observed),
    )
    .expect("transient records");
    assert!(!first.clamped);
    assert_eq!(first.failures, 0);
    assert_eq!((first.desired, first.observed), (desired, observed));
    assert!(first.suppressed(&desired, &observed, 1200));
    assert!(!first.suppressed(&desired, &observed, 1600));
    let second = consume_attempt(
        Some(first),
        &attempt("w1", desired, FrameSetter::Accepted, None, 1600),
        Some(observed),
    )
    .expect("transient repeats");
    assert!(!second.clamped);
    assert_eq!(second.failures, 1);
    assert!(second.suppressed(&desired, &observed, 2000));
    assert!(!second.suppressed(&desired, &observed, 2700));
}

#[test]
fn resolved_exact_observation_clears() {
    // The staged fact resolves an unreadable post-write proof exactly
    // like a readable one: observed equal to target clears both lanes,
    // including pre-existing clamp memory.
    let desired = rect(0, 0, 800, 600);
    let held = rect(0, 0, 700, 600);
    assert!(
        consume_attempt(
            None,
            &attempt("w1", desired, FrameSetter::Accepted, None, 100),
            Some(desired),
        )
        .is_none()
    );
    let clamped = consume_attempt(
        None,
        &attempt("w1", desired, FrameSetter::Accepted, Some(held), 100),
        Some(held),
    )
    .expect("clamp records");
    assert!(
        consume_attempt(
            Some(clamped),
            &attempt("w1", desired, FrameSetter::Refused, None, 200),
            Some(desired),
        )
        .is_none()
    );
}

#[test]
fn changed_dpi_never_becomes_a_stable_clamp() {
    let desired = rect(8, 8, 2544, 1364);
    let readback = rect(1, 8, 2558, 1371);
    assert!(
        consume_attempt(
            None,
            &dpi_attempt("w1", desired, Some(readback), Some(96), Some(120), 100),
            Some(readback),
        )
        .is_none()
    );
    // Unreadable DPI on either side keeps the stable lane.
    assert!(
        consume_attempt(
            None,
            &dpi_attempt("w1", desired, Some(readback), None, Some(120), 100),
            Some(readback),
        )
        .expect("stable lane")
        .clamped
    );
    // Changed DPI stays exempt even with unreadable proof: records
    // nothing, never a stable clamp and never a transient keyed on a
    // stale-basis fact; the next tick retries under fresh state.
    assert!(
        consume_attempt(
            None,
            &dpi_attempt("w1", desired, None, Some(96), Some(120), 100),
            Some(readback),
        )
        .is_none()
    );
}

// --- engine publish behavior ---

pub(crate) fn screen() -> ScreenReport {
    ScreenReport {
        output: OutputId("out-a".to_owned()),
        full_rect: Some(rect(0, 0, 800, 600)),
        work_area: Some(rect(0, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: true,
    }
}

pub(crate) fn fact(window: &str, frame: Option<Rect>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId("out-a".to_owned()),
        rect: frame,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
    }
}

pub(crate) fn minimized_fact(window: &str) -> WindowFact {
    WindowFact {
        minimized: true,
        ..fact(window, None)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn report_with_screens(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    closed: Vec<&str>,
    frames: Vec<FrameAttempt>,
    monotonic_ms: u64,
    inventory_complete: bool,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: vec![ScreenView {
            output: OutputId("out-a".to_owned()),
            view: String::new(),
        }],
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        feedback: Vec::new(),
        monotonic_ms,
        frames,
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    closed: Vec<&str>,
    frames: Vec<FrameAttempt>,
    monotonic_ms: u64,
    inventory_complete: bool,
) -> WorldReport {
    report_with_screens(
        request_seq,
        vec![screen()],
        windows,
        closed,
        frames,
        monotonic_ms,
        inventory_complete,
    )
}

pub(crate) fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

pub(crate) fn frame_row(
    reply: &crate::world_report::WorldReply,
    window: &str,
) -> crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("no row for {window}"))
        .clone()
}

#[test]
fn engine_suppresses_stable_clamp_and_clears_on_exact_match() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let observed = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    // Mechanical clamp facts: accepted setter, stable mismatched readback.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(observed),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&second, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
    // Stable identical pair stays suppressed with no new attempt.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&third, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
    // Exact arrival clears: back to controlled with no memory.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(desired))],
            vec![],
            vec![],
            3000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&fourth, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
}

#[test]
fn engine_transient_retry_follows_reported_time() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let observed = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    let refused = |ms: u64| attempt("w1", desired, FrameSetter::Refused, Some(observed), ms);
    let held = |reply: &crate::world_report::WorldReply| {
        frame_row(reply, "w1").frame_authority == ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    };
    // Failure at ms=1000: inside the 500ms backoff the desire holds.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![refused(1000)],
            1000,
            true,
        ),
    );
    assert!(held(&second));
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![],
            1200,
            true,
        ),
    );
    assert!(held(&third));
    // Past the backoff the adapter may retry: controlled again.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&fourth, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
}

#[test]
fn engine_ignores_attempts_for_minimized_unknown_and_unreadable() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let observed = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    // Minimized lifetimes record nothing.
    accepted(
        &mut engine,
        &report(
            2,
            vec![minimized_fact("w1")],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(observed),
                1000,
            )],
            1000,
            true,
        ),
    );
    // Unknown lifetimes record nothing.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![attempt(
                "ghost",
                desired,
                FrameSetter::Accepted,
                Some(observed),
                1000,
            )],
            1000,
            true,
        ),
    );
    // Unreadable evidence records nothing.
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", None)],
            vec![],
            vec![attempt("w1", desired, FrameSetter::Threw, None, 1000)],
            1000,
            false,
        ),
    );
    assert!(engine.frames.is_empty());
}

#[test]
fn engine_changed_dpi_never_suppresses_and_close_prunes() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let observed = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    // Invalidated placement: the desire stays controlled for retry.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![dpi_attempt(
                "w1",
                desired,
                Some(observed),
                Some(96),
                Some(120),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&second, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
    // A stable clamp records, then the explicit close prunes it.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(observed),
                2000,
            )],
            2000,
            true,
        ),
    );
    assert_eq!(engine.frames.len(), 1);
    accepted(
        &mut engine,
        &report(4, vec![], vec!["w1"], vec![], 3000, true),
    );
    assert!(engine.frames.is_empty());
}

#[test]
fn engine_replacement_lifetime_does_not_inherit_suppression() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let observed = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(observed))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(observed),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(engine.frames.len(), 1);
    // A replacement lifetime with the same observed frame is fresh: its own
    // pair was never recorded, so it stays controlled (never inherits w1's
    // suppression even though the observed rectangles coincide).
    let next = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(observed)), fact("w2", Some(observed))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    let w2 = frame_row(&next, "w2");
    assert!(w2.effective_frame.is_some());
    assert_eq!(w2.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn engine_withholds_float_token_while_suppressed() {
    use crate::boundary::CoreCommand;
    use crate::world_report::WorldCommand;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let target = rect(600, 400, 150, 120);
    let mut floated = report(
        2,
        vec![fact("w1", Some(rect(0, 0, 40, 90)))],
        vec![],
        vec![],
        0,
        true,
    );
    floated.command_seq = Some(9);
    floated.command = Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: "w1".to_owned(),
        float_rect: Some(target),
    }));
    floated.request_id = "req-2".to_owned();
    let placed = accepted(&mut engine, &floated);
    let row = frame_row(&placed, "w1");
    assert!(row.floating);
    assert_eq!(row.frame_operation.as_deref(), Some("op-9"));
    assert_eq!(row.effective_frame, Some(target));
    // A clamping placement attempt withholds the token while the retained
    // target stays outstanding: same core owner as tiled rows.
    let mut floating_fact = fact("w1", Some(rect(0, 0, 40, 90)));
    floating_fact.floating = true;
    let suppressed = accepted(
        &mut engine,
        &report(
            3,
            vec![floating_fact.clone()],
            vec![],
            vec![attempt(
                "w1",
                target,
                FrameSetter::Accepted,
                Some(rect(0, 0, 40, 90)),
                1000,
            )],
            1000,
            true,
        ),
    );
    let row = frame_row(&suppressed, "w1");
    assert!(row.floating);
    assert_eq!(row.frame_operation, None);
    assert_eq!(row.effective_frame, Some(target));
    assert_eq!(engine.retained_floats.len(), 1);
    // Ordinary attempts never mint a token: only the explicit one-shot
    // operation authorizes float geometry.
    let again = accepted(
        &mut engine,
        &report(4, vec![floating_fact], vec![], vec![], 2000, true),
    );
    let row = frame_row(&again, "w1");
    assert_eq!(row.frame_operation, None);
    assert_eq!(engine.retained_floats.len(), 1);
}

#[test]
fn frame_report_validation_bounds_shape() {
    use crate::world_report::{validate_report, validate_report_reason};
    let mut base = report(
        1,
        vec![fact("w1", Some(rect(0, 0, 40, 90)))],
        vec![],
        vec![],
        0,
        true,
    );
    assert!(validate_report(&base));
    base.frames.push(attempt(
        "w1",
        rect(0, 0, 800, 560),
        FrameSetter::Accepted,
        Some(rect(0, 0, 700, 560)),
        10,
    ));
    assert!(validate_report(&base));
    base.frames.push(attempt(
        "w1",
        rect(0, 0, 800, 560),
        FrameSetter::Refused,
        Some(rect(0, 0, 700, 560)),
        11,
    ));
    assert_eq!(validate_report_reason(&base), Some("duplicate-frame"));
}

#[test]
fn exact_clear_then_complete_absence_prunes_consumed_watermark() {
    // Exact observation clears suppression memory but the consumed
    // watermark survives (redelivery protection). A later complete
    // absence without explicit close must still prune the watermark —
    // `reconcile_members` drops complete-inventory absences — rather
    // than leaking it keyed on a dead lifetime.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let drifted = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(drifted))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(drifted),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(engine.frames.len(), 1);
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(desired))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert!(engine.frames.is_empty());
    assert_eq!(engine.consumed_frames.len(), 1);
    // Complete absence without explicit close. Membership still holds w1
    // through this report's settle (reconcile runs later in the
    // pipeline), so the watermark legitimately survives one more turn —
    // same lag as suppression memory itself.
    accepted(&mut engine, &report(4, vec![], vec![], vec![], 3000, true));
    assert_eq!(engine.consumed_frames.len(), 1);
    // Absence is now complete in membership too: both maps prune.
    accepted(&mut engine, &report(5, vec![], vec![], vec![], 4000, true));
    assert!(engine.frames.is_empty());
    assert!(engine.consumed_frames.is_empty());
}

#[test]
fn replayed_attempt_after_exact_clear_does_not_resurrect_clamp() {
    // Accepted stable clamp at ms=1000 for (D, R); an exact observation
    // clears the lane; re-observing R while redelivering the SAME old
    // attempt (lost-ack replay) must not re-insert the clamp: the reply
    // stays controlled so the fresh deviation may reapply.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let drifted = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    let old_attempt = attempt("w1", desired, FrameSetter::Accepted, Some(drifted), 1000);
    let clamped = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(drifted))],
            vec![],
            vec![old_attempt.clone()],
            1000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&clamped, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
    // Exact observation clears the lane.
    let cleared = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(desired))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&cleared, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
    // New drift back to R with the redelivered old attempt: no stale
    // suppression, memory stays empty until a fresh attempt arrives.
    let replayed = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(drifted))],
            vec![],
            vec![old_attempt],
            3000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&replayed, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
}

#[test]
fn stale_pair_redelivery_does_not_clobber_newer_authority() {
    // A newer different-pair attempt consumed at ms=2000 establishes the
    // live authority; redelivering the older (D, R) attempt from ms=1000
    // must leave that authority intact.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("w1", Some(rect(0, 0, 40, 90)))],
            vec![],
            vec![],
            0,
            true,
        ),
    );
    let desired = frame_row(&first, "w1")
        .effective_frame
        .expect("tiled desired");
    let drifted = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 100,
        h: desired.h,
    };
    let further = Rect {
        x: desired.x,
        y: desired.y,
        w: desired.w - 50,
        h: desired.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(drifted))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(drifted),
                1000,
            )],
            1000,
            true,
        ),
    );
    // Newer drift to a different observed rect, consumed at ms=2000.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(further))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(further),
                2000,
            )],
            2000,
            true,
        ),
    );
    let live = engine.frames.get(&WindowId("w1".to_owned())).expect("live");
    assert_eq!((live.desired, live.observed), (desired, further));
    // Redeliver the older pair: authority must not clobber.
    let held = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(further))],
            vec![],
            vec![attempt(
                "w1",
                desired,
                FrameSetter::Accepted,
                Some(drifted),
                1000,
            )],
            3000,
            true,
        ),
    );
    let live = engine.frames.get(&WindowId("w1".to_owned())).expect("live");
    assert_eq!((live.desired, live.observed), (desired, further));
    assert_eq!(
        frame_row(&held, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
}

// --- stage 3b learned minima ---

pub(crate) fn two_facts() -> Vec<WindowFact> {
    vec![
        fact("w1", Some(rect(0, 0, 40, 90))),
        fact("w2", Some(rect(0, 0, 40, 90))),
    ]
}

pub(crate) fn learned_of(engine: &WorldEngine, window: &str) -> (Option<i32>, Option<i32>) {
    engine
        .learned_mins
        .get(&WindowId(window.to_owned()))
        .map(|learned| (learned.min_w, learned.min_h))
        .unwrap_or((None, None))
}

#[test]
fn learned_growth_changes_target_layout_and_converges() {
    use super::world_frames::learnable_growth;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    assert_eq!(baseline.w, baseline_w2.w);
    // Stable accepted width growth at the same origin. Height is already
    // full-bleed (560), so only the width axis can change this layout; the
    // both-axes shape is pinned by the pure `learnable_growth` unit and the
    // overflow test below.
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 120,
        h: baseline.h,
    };
    assert_eq!(learnable_growth(&baseline, &grown), (Some(grown.w), None));
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    // Persistent constraint learned, feeding the real layout path.
    assert_eq!(learned_of(&engine, "w1"), (Some(grown.w), None));
    let next = frame_row(&second, "w1").effective_frame.expect("tiled");
    assert!(
        next.w >= grown.w && next.h == grown.h,
        "learned bound grows target {baseline:?} -> {next:?} covering {grown:?}"
    );
    assert!(
        next.w > baseline.w,
        "target layout actually changes, not only suppression"
    );
    // Converge: observe the new target exactly; suppression clears but the
    // learned bound persists for the next layout.
    let converged_w2 = frame_row(&second, "w2").effective_frame.expect("tiled");
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(next)), fact("w2", Some(converged_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&third, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
    assert_eq!(learned_of(&engine, "w1"), (Some(grown.w), None));
}

#[test]
fn learned_single_axis_only_pins_grown_axis() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 80,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    assert_eq!(learned_of(&engine, "w1").1, None);
}

#[test]
fn learned_both_axes_record_even_when_height_overflows_work_area() {
    use super::world_frames::learnable_growth;
    let target = rect(400, 0, 400, 560);
    let grown = rect(400, 0, 520, 620);
    assert_eq!(learnable_growth(&target, &grown), (Some(520), Some(620)));
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 120,
        h: baseline.h + 60,
    };
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    // Both axes prove minima even though the height overflows the work
    // area (downstream flags overconstrained and keeps the proportional
    // height; the width axis still grows the target layout). The new pair
    // is actionable, never a hold: the projector only flags overconstrained.
    assert_eq!(learned_of(&engine, "w1"), (Some(grown.w), Some(grown.h)));
    assert_eq!(
        frame_row(&second, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
}

#[test]
fn learned_rejects_origin_mismatch_shrink_and_equal() {
    use super::world_frames::learnable_growth;
    let target = rect(0, 0, 400, 300);
    assert_eq!(
        learnable_growth(&target, &rect(5, 0, 500, 300)),
        (None, None)
    );
    assert_eq!(
        learnable_growth(&target, &rect(0, 5, 500, 300)),
        (None, None)
    );
    assert_eq!(
        learnable_growth(&target, &rect(0, 0, 500, 200)),
        (None, None)
    );
    assert_eq!(learnable_growth(&target, &target), (None, None));
    // Engine never learns position drift even with a readable clamp pair.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let moved = Rect {
        x: baseline.x + 5,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(moved)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(moved),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    assert_eq!(engine.frames.len(), 1);
}

#[test]
fn learned_ignores_dpi_refused_threw() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    for (seq_no, (setter, readback, dpi)) in (2u64..).zip([
        (FrameSetter::Refused, Some(grown), (None, None)),
        (FrameSetter::Threw, Some(grown), (None, None)),
        (FrameSetter::Accepted, Some(grown), (Some(96), Some(120))),
    ]) {
        let (before, after) = dpi;
        accepted(
            &mut engine,
            &report(
                seq_no,
                vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
                vec![],
                vec![FrameAttempt {
                    window: WindowId("w1".to_owned()),
                    target: baseline,
                    setter,
                    readback,
                    dpi_before: before,
                    dpi_after: after,
                    attempt_ms: 1000,
                    operation_id: None,
                    prewrite: None,
                }],
                1000,
                true,
            ),
        );
    }
    assert!(engine.learned_mins.is_empty());
}

#[test]
fn learned_ignores_unreadable_float_overlay_and_gap() {
    // Unreadable post-write proof never learns.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt("w1", baseline, FrameSetter::Accepted, None, 1000)],
            1000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    // Float lifetimes never take tile policy.
    let mut floating = fact("w1", Some(grown));
    floating.floating = true;
    accepted(
        &mut engine,
        &report(
            3,
            vec![floating, fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                2000,
            )],
            2000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    // Fullscreen overlays never take tile policy.
    let mut overlay = fact("w1", Some(grown));
    overlay.fullscreen = true;
    accepted(
        &mut engine,
        &report(
            4,
            vec![overlay, fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                3000,
            )],
            3000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    // Incomplete inventory gaps never learn even with corroboration.
    accepted(
        &mut engine,
        &report(
            5,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                4000,
            )],
            4000,
            false,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    // Minimized lifetimes record nothing at all.
    accepted(
        &mut engine,
        &report(
            6,
            vec![minimized_fact("w1"), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                5000,
            )],
            5000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
}

#[test]
fn learned_replay_after_clear_does_not_relearn() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let old = attempt("w1", baseline, FrameSetter::Accepted, Some(grown), 1000);
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![old.clone()],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let next = frame_row(&second, "w1").effective_frame.expect("tiled");
    let next_w2 = frame_row(&second, "w2").effective_frame.expect("tiled");
    // Exact arrival clears suppression but keeps the persistent bound.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(next)), fact("w2", Some(next_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert!(engine.frames.is_empty());
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // Lost-ack redelivery of the old growth attempt cannot relearn or
    // resurrect suppression.
    let replayed = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(next)), fact("w2", Some(next_w2))],
            vec![],
            vec![old],
            3000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&replayed, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert!(engine.frames.is_empty());
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
}

#[test]
fn learned_survives_pair_change_but_prunes_on_close_departure_incarnation() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // An unrelated higher observation clears the pair hold but keeps the
    // persistent bound. (A smaller stable eligible observation would
    // disprove the floor instead; see the counter-evidence test below.)
    let drifted = Rect {
        x: grown.x,
        y: grown.y,
        w: grown.w,
        h: grown.h + 10,
    };
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(drifted)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // Complete-inventory departure without an explicit close prunes.
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            3000,
            true,
        ),
    );
    accepted(
        &mut engine,
        &report(
            5,
            vec![fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            4000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    // Explicit close prunes on a fresh engine.
    let mut engine2 = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine2,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine2,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine2, "w1").0, Some(grown.w));
    accepted(
        &mut engine2,
        &report(
            3,
            vec![fact("w2", Some(baseline_w2))],
            vec!["w1"],
            vec![],
            2000,
            true,
        ),
    );
    assert!(engine2.learned_mins.is_empty());
    // Incarnation reset prunes on a fresh engine.
    let mut engine3 = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine3,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine3,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert!(!engine3.learned_mins.is_empty());
    let mut reboot = report(3, two_facts(), vec![], vec![], 2000, true);
    reboot.adapter_incarnation = "adapter-2".to_owned();
    accepted(&mut engine3, &reboot);
    assert!(engine3.learned_mins.is_empty());
}

#[test]
fn learned_max_contradiction_keeps_declared_pair() {
    use crate::size_hints::WindowSizeHints;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    // Declared max exactly at the old desire: growth past it contradicts.
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let mut capped_w1 = fact("w1", Some(grown));
    capped_w1.hints = WindowSizeHints {
        min_w: None,
        min_h: None,
        max_w: Some(baseline.w),
        max_h: None,
    };
    let held = accepted(
        &mut engine,
        &report(
            2,
            vec![capped_w1, fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    // No learning past a contradictory declared maximum; the stable pair
    // still suppresses without moving the target.
    assert!(engine.learned_mins.is_empty());
    assert_eq!(
        frame_row(&held, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
    assert_eq!(
        frame_row(&held, "w1").effective_frame.expect("tiled"),
        baseline
    );
}

#[test]
fn learned_smaller_stable_observation_clears_floor_without_fight() {
    // A racing enlargement must not become a lifelong constraint: a later
    // stable eligible observation smaller than the floor disproves that
    // axis. The next layout drops the floor with no suppression fight.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let next = frame_row(&second, "w1").effective_frame.expect("tiled");
    let next_w2 = frame_row(&second, "w2").effective_frame.expect("tiled");
    // Converge on the learned floor first: exact arrival keeps the bound.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(next)), fact("w2", Some(next_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(
        frame_row(&third, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // Independently observed shrink below the floor, no attempt behind it:
    // the floor claim is disproven, so the next layout drops it with no
    // suppression memory and no fight.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(baseline)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            3000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    assert!(engine.frames.is_empty());
    let settled = frame_row(&fourth, "w1");
    assert_eq!(settled.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(settled.effective_frame.expect("tiled"), baseline);
}

#[test]
fn learned_stale_replay_cannot_block_fresh_smaller_counterevidence() {
    // A stale redelivery (exact duplicate or older stamp) is not fresh
    // proof: it must not veto an observation-only smaller fact that
    // disproves the floor. Undercutting targets and refused setters only
    // block when the attempt itself is fresh.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let learned = attempt("w1", baseline, FrameSetter::Accepted, Some(grown), 1000);
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![learned.clone()],
            1000,
            true,
        ),
    );
    let next = frame_row(&second, "w1").effective_frame.expect("tiled");
    let next_w2 = frame_row(&second, "w2").effective_frame.expect("tiled");
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(next)), fact("w2", Some(next_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // Fresh observation-only shrink plus the stale undercutting redelivery:
    // the floor still clears with no fight.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(baseline)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![learned],
            3000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    assert!(engine.frames.is_empty());
    let settled = frame_row(&fourth, "w1");
    assert_eq!(settled.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(settled.effective_frame.expect("tiled"), baseline);
}

#[test]
fn learned_stale_refused_and_clockless_replays_cannot_block_clearing() {
    // Same veto-proofing for refused setters and clockless attempts: a
    // redelivered refusal is a transient preimage replay, and a zero-clock
    // attempt carries no ordering claim, so neither blocks fresh smaller
    // counter-evidence. A fresh refusal still retains (pinned elsewhere).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let small = Rect {
        x: grown.x,
        y: grown.y,
        w: baseline.w,
        h: grown.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let refused = attempt("w1", grown, FrameSetter::Refused, Some(small), 2000);
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(small)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![refused.clone()],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // Redelivered refusal: stale, so the smaller fact still disproves.
    let cleared = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(small)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![refused],
            3000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    assert_eq!(
        frame_row(&cleared, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
    // Clockless refusal: no ordering claim, so it never vetoes either.
    let mut engine2 = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine2,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let small = Rect {
        x: grown.x,
        y: grown.y,
        w: baseline.w,
        h: grown.h,
    };
    accepted(
        &mut engine2,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine2, "w1").0, Some(grown.w));
    let cleared = accepted(
        &mut engine2,
        &report(
            3,
            vec![fact("w1", Some(small)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt("w1", grown, FrameSetter::Refused, Some(small), 0)],
            2000,
            true,
        ),
    );
    assert!(engine2.learned_mins.is_empty());
    assert_eq!(
        frame_row(&cleared, "w1").frame_authority,
        ScopeAuthority::Controlled
    );
}

#[test]
fn learned_ignores_veto_table_flags() {
    // Compact veto table: maximized, sticky, fit-excluded, fixed-auto and
    // fixed-suppress facts never learn, even with corroborated growth.
    // (Minimized, floating and fullscreen ride the gap test above.)
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    for (seq_no, flag) in (2u64..).zip(["maximized", "sticky", "excluded", "auto", "suppress"]) {
        let mut flagged = fact("w1", Some(grown));
        match flag {
            "maximized" => flagged.maximized = true,
            "sticky" => flagged.sticky = true,
            "excluded" => flagged.fit_excluded = true,
            "auto" => flagged.fixed_auto = true,
            "suppress" => flagged.fixed_suppress = true,
            _ => unreachable!("flag table"),
        }
        accepted(
            &mut engine,
            &report(
                seq_no,
                vec![flagged, fact("w2", Some(baseline_w2))],
                vec![],
                vec![attempt(
                    "w1",
                    baseline,
                    FrameSetter::Accepted,
                    Some(grown),
                    1000,
                )],
                1000,
                true,
            ),
        );
        assert!(engine.learned_mins.is_empty(), "{flag} vetoes learning");
    }
}

#[test]
fn learned_refused_attempt_does_not_invalidate() {
    // A refused write leaves a transient staged preimage, not external
    // counter-evidence: the floor survives while the transient lane holds.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let small = Rect {
        x: grown.x,
        y: grown.y,
        w: baseline.w,
        h: grown.h,
    };
    let held = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", Some(small)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                grown,
                FrameSetter::Refused,
                Some(small),
                2000,
            )],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    // The write itself rides the transient lane; the floor is untouched.
    // (Authority follows the reshaped pair, so only retention is pinned.)
    assert!(
        frame_row(&held, "w1").effective_frame.is_some(),
        "still publishes a tiled row"
    );
}

#[test]
fn learned_max_change_suppresses_without_clearing_and_restores_without_relearn() {
    // A changed declared maximum contradicting the floor suppresses that
    // axis instead of clearing it: the bound stays retained while the
    // target follows the declared pair, and lifting the max restores the
    // floor with no new growth proof. No clear/relearn loop either way.
    use crate::size_hints::WindowSizeHints;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    let next = frame_row(&second, "w1").effective_frame.expect("tiled");
    assert_eq!(next.w, grown.w);
    // Tighten the declared max below the floor while the app still shows
    // the floor: the floor is suppressed, not disproven, so it stays
    // retained while the target follows the declared pair.
    let mut capped_w1 = fact("w1", Some(grown));
    capped_w1.hints = WindowSizeHints {
        min_w: None,
        min_h: None,
        max_w: Some(baseline.w),
        max_h: None,
    };
    let suppressed = accepted(
        &mut engine,
        &report(
            3,
            vec![capped_w1, fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    assert_eq!(
        frame_row(&suppressed, "w1").effective_frame.expect("tiled"),
        baseline
    );
    // Lift the max with no new growth: the retained floor applies again
    // immediately, so no relearn loop is needed.
    let restored = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            3000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    assert_eq!(
        frame_row(&restored, "w1").effective_frame.expect("tiled"),
        next
    );
}

#[test]
fn learned_zero_clock_attempt_never_learns() {
    // No freshness stamp means no ordering claim: even corroborated growth
    // never learns, while the clamp lane still suppresses the stable pair.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    let held = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                0,
            )],
            1000,
            true,
        ),
    );
    assert!(engine.learned_mins.is_empty());
    assert_eq!(
        frame_row(&held, "w1").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
    );
}

#[test]
fn learned_declared_stronger_min_wins() {
    // A declared minimum stronger than the floor wins the merge: the
    // learned bound stays retained, but the target follows the declared min.
    use crate::size_hints::WindowSizeHints;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let mut declared_w1 = fact("w1", Some(grown));
    declared_w1.hints = WindowSizeHints {
        min_w: Some(grown.w + 80),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let declared = accepted(
        &mut engine,
        &report(
            3,
            vec![declared_w1, fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    assert_eq!(
        frame_row(&declared, "w1").effective_frame.expect("tiled").w,
        grown.w + 80
    );
}

#[test]
fn learned_float_excursion_preserves_and_reapplies() {
    // A float excursion never erases the floor: the bound survives the
    // exception rows and applies again once the lifetime re-tiles.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, two_facts(), vec![], vec![], 0, true),
    );
    let baseline = frame_row(&first, "w1").effective_frame.expect("tiled");
    let baseline_w2 = frame_row(&first, "w2").effective_frame.expect("tiled");
    let grown = Rect {
        x: baseline.x,
        y: baseline.y,
        w: baseline.w + 100,
        h: baseline.h,
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![attempt(
                "w1",
                baseline,
                FrameSetter::Accepted,
                Some(grown),
                1000,
            )],
            1000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let mut floating = fact("w1", Some(grown));
    floating.floating = true;
    accepted(
        &mut engine,
        &report(
            3,
            vec![floating, fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            2000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    let retiled = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", Some(grown)), fact("w2", Some(baseline_w2))],
            vec![],
            vec![],
            3000,
            true,
        ),
    );
    assert_eq!(learned_of(&engine, "w1").0, Some(grown.w));
    assert!(
        frame_row(&retiled, "w1").effective_frame.expect("tiled").w >= grown.w,
        "floor reapplies after the excursion"
    );
}
