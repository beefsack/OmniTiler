//! Commandless born maximize-clear lifecycle regressions (N6):
//! WorldEngine-owned born-maximized / born-fullscreen / fixed-size gates
//! integrated from the shared stateless [`crate::window_policy`]
//! predicates, settled commandlessly through the existing `clear_operation`
//! desire and raw [`ClearAttempt`](crate::world_report::ClearAttempt) lane.
//!
//! Every test drives the retained born lifecycle through
//! [`crate::world_engine::WorldEngine`]. Shared report builders live in
//! [`crate::world_directional::tests`]; facts below mirror captured-shape
//! host data (maximized rows ride `fitExcluded`, fixed rows ride fixed
//! hints plus `fixedAuto`).

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_directional::tests::{accepted, fact, float_fact, report, report_full, screen};
use crate::world_engine::{RetainedFloat, RetainedTransfer, WorldEngine, WorldOutcome};
use crate::world_report::{
    ClearAttempt, FrameSetter, HoldReason, ScopeAuthority, ScreenReport, TransferKind,
    TransferTarget, WindowFact, WorldCommand, WorldReport,
};

fn clear_attempt(
    operation_id: &str,
    window: &str,
    setter: FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> ClearAttempt {
    ClearAttempt {
        operation_id: operation_id.to_owned(),
        window: WindowId(window.to_owned()),
        setter,
        pre_maximized: pre,
        post_maximized: post,
        attempt_ms: 7,
    }
}

fn report_clears(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    clears: Vec<ClearAttempt>,
) -> WorldReport {
    let mut report = report_full(
        request_seq,
        observation_seq,
        screens,
        windows,
        Vec::new(),
        focused,
        None,
        None,
    );
    report.clear_attempts = clears;
    report
}

fn born_maximized(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        maximized: true,
        fit_excluded: true,
        ..fact(window, output, x)
    }
}

fn born_fullscreen(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: true,
        fit_excluded: true,
        ..fact(window, output, x)
    }
}

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: Some(400),
        max_h: Some(300),
    }
}

fn fixed_maximized(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        maximized: true,
        fit_excluded: true,
        floating: true,
        fixed_auto: true,
        hints: fixed_hints(),
        ..fact(window, output, x)
    }
}

fn desire_op(reply: &crate::world_report::WorldReply, window: &str) -> Option<String> {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .and_then(|row| row.clear_operation.clone())
}

#[test]
fn commandless_birth_mints_and_publishes_clear() {
    // A tiled-first born maximized window binds commandlessly and the
    // reply carries the one-shot `clear_operation` desire. No command
    // ran: the binding id comes from the core born serial.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let op = engine
        .born_operation_for(&WindowId("win-born".to_owned()))
        .expect("born binds");
    assert!(op.starts_with("op-born-"), "commandless id, got {op}");
    assert_eq!(desire_op(&reply, "win-born").as_deref(), Some(op.as_str()));
}

#[test]
fn disabled_birth_never_mints_or_publishes() {
    // Default-disabled pin: a tiled-first maximized birth mints nothing,
    // tracks nothing, and publishes no desire when the capability is off.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut first = report(
        1,
        1,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    first.birth_clear = false;
    let reply = accepted(&mut engine, &first);
    assert_eq!(engine.born_retained_len(), 0, "disabled mints nothing");
    assert_eq!(
        engine.born_tracked_count(),
        0,
        "disabled tracks no lifetime marks"
    );
    assert_eq!(desire_op(&reply, "win-born"), None);
}

#[test]
fn disabled_fullscreen_birth_and_exit_never_bind() {
    // Default-disabled pin: a fullscreen birth holds nothing and its exit
    // to maximized binds no held-exit lane while the capability is off.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut first = report(
        1,
        1,
        vec![screen("out-a", 0, true)],
        vec![born_fullscreen("win-fs", "out-a", 100)],
        None,
        None,
    );
    first.birth_clear = false;
    let reply = accepted(&mut engine, &first);
    assert_eq!(engine.born_retained_len(), 0, "disabled holds nothing");
    assert_eq!(desire_op(&reply, "win-fs"), None);
    let mut exit = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-fs", "out-a", 100)],
        None,
        None,
    );
    exit.birth_clear = false;
    let reply = accepted(&mut engine, &exit);
    assert_eq!(engine.born_retained_len(), 0, "disabled exit binds nothing");
    assert_eq!(desire_op(&reply, "win-fs"), None);
}

#[test]
fn disabled_deferred_conditions_mint_nothing() {
    // Default-disabled pin: deferred conditions (incomplete-inventory gap,
    // gesture barrier, game-paused presentation) mint nothing while the
    // capability is off, and publish no desire either way.
    for name in ["gap", "gesture", "paused"] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let mut deferred = report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        );
        deferred.birth_clear = false;
        match name {
            "gap" => deferred.inventory_complete = false,
            "gesture" => deferred.gesture_barrier = true,
            "paused" => deferred.presentation.paused = true,
            _ => unreachable!(),
        }
        let reply = accepted(&mut engine, &deferred);
        assert_eq!(
            engine.born_retained_len(),
            0,
            "{name}: disabled mints nothing"
        );
        assert_eq!(desire_op(&reply, "win-born"), None, "{name}: no desire");
    }
}

#[test]
fn disabled_flag_off_after_mint_produces_no_desire() {
    // Opt-out drops the lane: a minted token stops publishing once the
    // capability turns off, and no binding survives the opt-out report.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert!(desire_op(&first, "win-born").is_some());
    let mut off = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    off.birth_clear = false;
    let reply = accepted(&mut engine, &off);
    assert_eq!(engine.born_retained_len(), 0, "opt-out drops the binding");
    assert_eq!(desire_op(&reply, "win-born"), None);
}

#[test]
fn disabled_explicit_directional_clear_unaffected() {
    // The capability gates only the automatic birth lane: an explicit
    // directional clear (move-maximize-clear) still binds and publishes
    // with the capability off.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let mut first = report(
        1,
        1,
        vec![screen("out-a", 0, true)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        None,
        None,
    );
    first.birth_clear = false;
    accepted(&mut engine, &first);
    let mut commanded = report_full(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        Vec::new(),
        "win-max",
        Some(13),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("win-max".to_owned()),
            direction: "right".to_owned(),
            same_axis: crate::directional::SameAxisMove::default(),
        }),
    );
    commanded.birth_clear = false;
    let reply = accepted(&mut engine, &commanded);
    assert_eq!(engine.born_retained_len(), 0, "no automatic birth beside");
    assert_eq!(engine.retained_clears.len(), 1);
    assert_eq!(engine.retained_clears[0].operation_id, "op-13");
    assert_eq!(
        desire_op(&reply, "win-max").as_deref(),
        Some("op-13"),
        "directional clear still publishes while disabled"
    );
}

#[test]
fn accepted_attempt_plus_unmaximized_proves_and_retires() {
    // The spent operation never re-actuates: a correlated accepted
    // attempt plus complete readable unmaximized facts retires the
    // binding, and the next report carries no desire and mints nothing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-born", "out-a", 100)
    };
    let second = accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![cleared.clone()],
            "",
            vec![clear_attempt(
                &op,
                "win-born",
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    assert_eq!(desire_op(&second, "win-born"), None);
    // A further report never re-mints for the same lifetime: the attempt
    // set holds at most once.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![cleared],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    assert_eq!(desire_op(&third, "win-born"), None);
}

#[test]
fn external_clear_without_attempt_abandons() {
    // Unmaximized facts with no correlated attempt name newer external
    // input, not our clear: the binding abandons with no continuation
    // and never re-mints.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-born", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![cleared],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
}

#[test]
fn proven_refusal_retires_and_never_retries() {
    // An explicit refusal with an unchanged readable maximized preimage
    // proves refusal: the binding retires and later maximized reports
    // never re-mint.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let second = accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            "",
            vec![clear_attempt(
                &op,
                "win-born",
                FrameSetter::Refused,
                Some(true),
                Some(true),
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    assert_eq!(desire_op(&second, "win-born"), None);
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "refused never retries");
    assert_eq!(desire_op(&third, "win-born"), None);
}

#[test]
fn throw_latches_ambiguity_through_unchanged_reads() {
    // A throw may have executed: the binding holds with no retry, and
    // the staged pending attempt survives a later unmaximized read
    // without proving an external clear.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let second = accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            "",
            vec![clear_attempt(
                &op,
                "win-born",
                FrameSetter::Threw,
                Some(true),
                None,
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "throw holds");
    assert_eq!(desire_op(&second, "win-born"), None, "spent withholds");
    // Unmaximized facts arrive with no new attempt: the staged throw is
    // ambiguous evidence, never external-clear proof.
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-born", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![cleared],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "ambiguity persists");
}

#[test]
fn unreadable_proof_holds_without_spend() {
    // An accepted attempt with an unreadable post latches ambiguity:
    // never success, never retry.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            "",
            vec![clear_attempt(
                &op,
                "win-born",
                FrameSetter::Accepted,
                Some(true),
                None,
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
}

#[test]
fn async_success_completes_on_later_unmaximized_facts() {
    // Setter acceptance is never synchronous completion: the binding
    // holds while the window still reads maximized, then retires when
    // later readable facts prove the async clear landed.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            "",
            vec![clear_attempt(
                &op,
                "win-born",
                FrameSetter::Accepted,
                Some(true),
                Some(true),
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "still maximized holds");
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-born", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![cleared],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "later proof retires");
}

#[test]
fn fullscreen_birth_holds_then_exit_clears() {
    // A first-seen fullscreen birth holds slotless with no clear; its
    // first exit to maximized binds the held-exit lane commandlessly.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_fullscreen("win-fs", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "held birth never clears");
    assert_eq!(desire_op(&first, "win-fs"), None);
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-fs", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "held exit binds");
    let op = desire_op(&second, "win-fs").expect("held exit desires");
    assert!(op.starts_with("op-born-"));
    // Proof retires the exit with no re-entry into the held set.
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-fs", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![cleared],
            "",
            vec![clear_attempt(
                &op,
                "win-fs",
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
}

#[test]
fn fullscreen_exit_to_plain_consumes_hold_without_clear() {
    // A held birth that exits to plain (non-maximized) ends the hold:
    // a later user maximize is never mistaken for the held exit.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_fullscreen("win-fs", "out-a", 100)],
            None,
            None,
        ),
    );
    let plain = WindowFact {
        fit_excluded: false,
        ..fact("win-fs", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![plain],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-fs", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "consumed hold never re-clears"
    );
    assert_eq!(desire_op(&third, "win-fs"), None);
}

#[test]
fn later_fullscreen_on_admitted_tile_never_enters_held() {
    // A fullscreen that arrives on an already-admitted tile is a managed
    // overlay transition, never a birth: no hold, no clear, and no exit
    // clear when it restores to maximized.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let plain = WindowFact {
        fit_excluded: false,
        ..fact("win-a", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![plain],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_fullscreen("win-a", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "admitted fullscreen never holds"
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-a", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "admitted restore never clears"
    );
    assert_eq!(desire_op(&third, "win-a"), None);
}

#[test]
fn fixed_size_birth_floats_without_clear() {
    // A fixed-size maximized birth floats beneath its overlay with no
    // clear and no tile slot fight.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![fixed_maximized("win-fixed", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "fixed never clears");
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-fixed")
        .expect("fixed keeps a row");
    assert!(row.floating, "fixed floats beneath the overlay");
    assert_eq!(row.clear_operation, None);
}

#[test]
fn fixed_fullscreen_exit_skips_clear() {
    // A held fullscreen birth whose exit still carries fixed hints
    // floats automatic with no clear, consuming the hold.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_fullscreen("win-fs", "out-a", 100)],
            None,
            None,
        ),
    );
    let exit = WindowFact {
        maximized: true,
        fit_excluded: true,
        floating: true,
        fixed_auto: true,
        hints: fixed_hints(),
        ..fact("win-fs", "out-a", 100)
    };
    let second = accepted(
        &mut engine,
        &report(2, 2, vec![screen("out-a", 0, true)], vec![exit], None, None),
    );
    assert_eq!(engine.born_retained_len(), 0, "fixed exit never clears");
    assert_eq!(desire_op(&second, "win-fs"), None);
}

#[test]
fn keep_overlay_skips_born_but_clears_held_exit() {
    // Under `keep-maximized-overlay` a tiled-first born maximized window
    // keeps its overlay with no clear; the held-fullscreen exit lane
    // still clears once. The kept lifetime stays present throughout, so a
    // later flip to tile-on-launch never reclears it.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut first_report = report(
        1,
        1,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    first_report.born_maximized = crate::born_maximized::BornMaximized::KeepMaximizedOverlay;
    let first = accepted(&mut engine, &first_report);
    assert_eq!(
        engine.born_retained_len(),
        0,
        "keep mode never clears births"
    );
    assert_eq!(desire_op(&first, "win-born"), None);
    // Held-fullscreen exit still clears under both modes.
    let mut fs_report = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![
            born_maximized("win-born", "out-a", 100),
            born_fullscreen("win-fs", "out-a", 200),
        ],
        None,
        None,
    );
    fs_report.born_maximized = crate::born_maximized::BornMaximized::KeepMaximizedOverlay;
    accepted(&mut engine, &fs_report);
    let mut exit_report = report(
        3,
        3,
        vec![screen("out-a", 0, true)],
        vec![
            born_maximized("win-born", "out-a", 100),
            born_maximized("win-fs", "out-a", 200),
        ],
        None,
        None,
    );
    exit_report.born_maximized = crate::born_maximized::BornMaximized::KeepMaximizedOverlay;
    let exit_reply = accepted(&mut engine, &exit_report);
    assert_eq!(
        engine.born_retained_len(),
        1,
        "held exit clears under both modes"
    );
    assert!(desire_op(&exit_reply, "win-fs").is_some());
    assert_eq!(desire_op(&exit_reply, "win-born"), None);
    // A later flip to tile-on-launch never reclears the previously kept
    // lifetime: the keep decision latched at tiled-first evaluation and
    // the lifetime stayed known throughout.
    let mut flipped = report(
        4,
        4,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    flipped.born_maximized = crate::born_maximized::BornMaximized::TileOnLaunch;
    let flipped_reply = accepted(&mut engine, &flipped);
    assert_eq!(engine.born_retained_len(), 0, "kept lifetime never rebinds");
    assert_eq!(desire_op(&flipped_reply, "win-born"), None);
}

#[test]
fn floating_scope_never_clears_and_latches_floating_first() {
    // A maximized birth on a floating workspace keeps its overlay, and a
    // later tiled admission of the same lifetime never clears either.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    // Flip the workspace to floating via the retained neutral command.
    // The binding abandons on the next observation (settlement runs
    // before command execution in the toggle report itself).
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            Vec::new(),
            "",
            Some(7),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: false,
            }),
        ),
    );
    let after_flip = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "tiling flip abandons");
    assert_eq!(desire_op(&after_flip, "win-born"), None);
    // Re-tile the same lifetime: floating-first latches, no second clear.
    let retiled = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            Vec::new(),
            "",
            Some(8),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-1".to_owned(),
                tiled: true,
            }),
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "floating-first never re-clears"
    );
    assert_eq!(desire_op(&retiled, "win-born"), None);
}

#[test]
fn close_prunes_and_replacement_rearms() {
    // An explicit close prunes every born lifetime set, so a replacement
    // lifetime under the same id binds fresh with a new operation id.
    use crate::directional::WindowId as W;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let mut closed_report = report(2, 2, vec![screen("out-a", 0, true)], vec![], None, None);
    closed_report.closed = vec![W("win-born".to_owned())];
    accepted(&mut engine, &closed_report);
    assert_eq!(engine.born_retained_len(), 0);
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op2 = desire_op(&third, "win-born").expect("replacement re-binds");
    assert_ne!(op, op2, "fresh operation id per lifetime");
}

#[test]
fn minimized_and_unreadable_hold_without_spend() {
    // Minimized and unreadable facts hold the binding with no attempt
    // and no spent mark: a later readable reply retries the desire.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let minimized = WindowFact {
        minimized: true,
        ..born_maximized("win-born", "out-a", 100)
    };
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![minimized],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "minimized holds");
    assert_eq!(desire_op(&second, "win-born"), None, "minimized withholds");
    let unreadable = WindowFact {
        rect: None,
        ..born_maximized("win-born", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![unreadable],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "unreadable holds");
    // The readable retry carries the same operation id: dedup, no second
    // binding.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert_eq!(desire_op(&fourth, "win-born").as_deref(), Some(op.as_str()));
}

#[test]
fn gap_arrival_decides_immediately() {
    // Incomplete inventory never gates the birth decision itself (matching
    // the retired adapter path, which decided on every accepted reply):
    // a maximized birth on a provable tile scope binds at arrival even
    // when the inventory is incomplete. Settlement and publish still hold
    // while gapped, so no write fires before readable proof.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut gapped = report(
        1,
        1,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert_eq!(engine.born_retained_len(), 1, "gap arrival still binds");
    assert!(desire_op(&reply, "win-born").is_some());
    // The binding holds across the gap with no attempt and no spend: the
    // next readable report retries the same desire.
    let mut still_gapped = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    still_gapped.inventory_complete = false;
    accepted(&mut engine, &still_gapped);
    assert_eq!(engine.born_retained_len(), 1, "gap holds without spend");
}

#[test]
fn sticky_birth_never_clears() {
    // Sticky identities keep their overlay: no mint, no retry loop.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let sticky = WindowFact {
        sticky: true,
        ..born_maximized("win-sticky", "out-a", 100)
    };
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![sticky],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    assert_eq!(desire_op(&reply, "win-sticky"), None);
}

#[test]
fn fullscreen_flip_abandons_into_held_lane() {
    // A fullscreen flip while a born clear is outstanding abandons the
    // born binding and re-arms the held lane for the eventual exit.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_fullscreen("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "fullscreen abandons born");
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        1,
        "exit re-binds through held lane"
    );
    assert!(desire_op(&third, "win-born").is_some());
}

#[test]
fn distinct_births_dedup_per_window() {
    // Two births on separate outputs bind independently with distinct
    // operation ids; a repeat report mints nothing further. (Two
    // maximized windows sharing one domain hold at engine converge, so
    // the dedup pair rides separate outputs like a real two-display
    // capture.)
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                born_maximized("win-a", "out-a", 100),
                born_maximized("win-b", "out-b", 900),
            ],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 2);
    let opa = desire_op(&reply, "win-a").expect("a desires");
    let opb = desire_op(&reply, "win-b").expect("b desires");
    assert_ne!(opa, opb);
    accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![
                born_maximized("win-a", "out-a", 100),
                born_maximized("win-b", "out-b", 900),
            ],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 2, "no duplicate bindings");
}

#[test]
fn later_maximize_of_admitted_tile_never_mints() {
    // A plain admission followed by a later maximize keeps its overlay:
    // only first-sightings bind, never later transitions of admitted tiles.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let plain = WindowFact {
        fit_excluded: false,
        ..fact("win-a", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![plain],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-a", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "later maximize never mints");
    assert_eq!(desire_op(&reply, "win-a"), None);
}

#[test]
fn delayed_fixed_transition_abandons_without_retry() {
    // A birth that reads fixed only after minting abandons with no retry:
    // fixed births float beneath their overlay. The first report mints on
    // non-fixed hints; the fixed report retires the binding.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    assert!(op.starts_with("op-born-"));
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![fixed_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "delayed fixed abandons");
    assert_eq!(desire_op(&second, "win-born"), None);
    // No retry even if hints unfix later: the resolution latched.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "abandoned never retries");
    assert_eq!(desire_op(&third, "win-born"), None);
}

#[test]
fn delayed_unreadable_late_admission_binds() {
    // An unreadable birth (no rect) marks nothing and never mints; the
    // first readable maximized report binds it. Late admission is never
    // defeated by the earlier non-decision.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let unreadable = WindowFact {
        rect: None,
        ..born_maximized("win-born", "out-a", 100)
    };
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![unreadable],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "unreadable mints nothing");
    assert_eq!(desire_op(&first, "win-born"), None);
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "readable arrival binds late");
    assert!(desire_op(&second, "win-born").is_some());
}

#[test]
fn implicit_departure_restore_is_not_birth() {
    // A sole-window implicit departure retains a minimized slot (D23
    // restore), so a maximized restore is a restore, not a birth: no
    // binding mints. The departure still prunes every born lifetime set
    // (boundedness), and the slot — not stale marks — governs the restore.
    // (Explicit close forgets the slot; replacement re-mint is covered by
    // close_prunes_and_replacement_rearms.)
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let plain = WindowFact {
        fit_excluded: false,
        ..fact("win-a", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![plain],
            None,
            None,
        ),
    );
    // Implicit departure: complete inventory, window absent, no close.
    accepted(
        &mut engine,
        &report(2, 2, vec![screen("out-a", 0, true)], vec![], None, None),
    );
    assert_eq!(engine.born_tracked_count(), 0, "departure prunes all marks");
    // Restore maximized: the retained slot proves it is no birth.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-a", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "restore is not a birth");
    assert_eq!(desire_op(&reply, "win-a"), None);
}

#[test]
fn complete_departures_bound_all_born_sets() {
    // Resolved, held and seen lifetimes all prune on complete-inventory
    // departure: born memory stays bounded across churn. Arrivals stagger
    // across reports on the established domain (a fullscreen+plain
    // first-seed mix holds at engine converge, unrelated to born policy).
    // (A returning id re-evaluates against retained slots, covered by
    // implicit_departure_restore_is_not_birth; replacement after explicit
    // close is covered by close_prunes_and_replacement_rearms.)
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-old", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-old").expect("old binds");
    // win-old resolves (attempted) while win-hold holds fullscreen on the
    // established domain.
    let cleared = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-old", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![cleared, born_fullscreen("win-hold", "out-a", 200)],
            "",
            vec![clear_attempt(
                &op,
                "win-old",
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    // win-seen admits plain on the established domain.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![
                WindowFact {
                    maximized: false,
                    fit_excluded: false,
                    ..fact("win-old", "out-a", 100)
                },
                born_fullscreen("win-hold", "out-a", 200),
                WindowFact {
                    fit_excluded: false,
                    ..fact("win-seen", "out-a", 300)
                },
            ],
            None,
            None,
        ),
    );
    assert!(
        engine.born_tracked_count() > 0,
        "departure test needs live marks"
    );
    // All three lifetimes depart implicitly on a complete inventory.
    accepted(
        &mut engine,
        &report(4, 4, vec![screen("out-a", 0, true)], vec![], None, None),
    );
    assert_eq!(engine.born_tracked_count(), 0, "departure prunes all marks");
}

#[test]
fn same_domain_cohort_clear_admission_parked_b1() {
    // N6 tentative property authority (2026-10-26): two maximized births
    // arriving in the SAME report on one domain bind despite the sweep
    // geometry hold (`Unreadable` overlap). Maximize-clear is a
    // birth-authorized mode transition proven by the windows' own birth
    // facts and the shared `window_policy` gates, not a geometry write;
    // requiring converged geometry deadlocks because maximized frames
    // stay host-controlled until the clear lands. Geometry writes stay
    // held; only the clear token rides, settled solely by fresh native
    // evidence plus the correlated raw attempt. Sequential arrival binds
    // both (see distinct_births_dedup_per_window).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![
                born_maximized("win-a", "out-a", 100),
                born_maximized("win-b", "out-a", 300),
            ],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 2, "B1 parked: cohort binds");
    assert!(desire_op(&reply, "win-a").is_some());
    assert!(desire_op(&reply, "win-b").is_some());
}

fn cohort_pair() -> Vec<WindowFact> {
    vec![
        born_maximized("win-a", "out-a", 100),
        born_maximized("win-b", "out-a", 300),
    ]
}

fn cohort_report(request_seq: u64, observation_seq: u64) -> WorldReport {
    report(
        request_seq,
        observation_seq,
        vec![screen("out-a", 0, true)],
        cohort_pair(),
        None,
        None,
    )
}

#[test]
fn cohort_clear_publishes_held_property_tokens() {
    // The held cohort publishes the clear token on held rows: the KDE
    // actuator consumes `clear_operation` without any frame/member
    // authority, so geometry writes stay held while the property clear
    // proceeds. Both rows carry no allocation and hold both authorities.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(&mut engine, &cohort_report(1, 1));
    assert_eq!(engine.born_retained_len(), 2);
    for win in ["win-a", "win-b"] {
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("held cohort row");
        assert!(
            row.clear_operation.is_some(),
            "{win}: held row still carries the clear token"
        );
        assert_eq!(row.allocation, None, "{win}: no geometry write rides");
        assert!(
            matches!(row.frame_authority, ScopeAuthority::Hold(_)),
            "{win}: frame authority stays held"
        );
        assert!(
            matches!(row.membership_authority, ScopeAuthority::Hold(_)),
            "{win}: membership authority stays held"
        );
    }
    let scope = reply
        .scopes
        .iter()
        .find(|scope| scope.scope == "out-a/ws-1")
        .expect("cohort scope row");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Hold(HoldReason::Unreadable),
        "geometry hold is unchanged"
    );
}

#[test]
fn cohort_clear_settles_and_converges_frames() {
    // Full journey: cohort binds under the geometry hold, both clears
    // dispatch, and fresh native evidence (correlated accepted attempts
    // plus readable unmaximized facts) retires both bindings with no
    // re-mint. The converged scope then tiles the proven frames.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(&mut engine, &cohort_report(1, 1));
    let opa = desire_op(&first, "win-a").expect("a desires");
    let opb = desire_op(&first, "win-b").expect("b desires");
    assert_ne!(opa, opb, "distinct one-shot tokens");
    let cleared_a = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-a", "out-a", 100)
    };
    let cleared_b = WindowFact {
        maximized: false,
        fit_excluded: false,
        ..fact("win-b", "out-a", 300)
    };
    let second = accepted(
        &mut engine,
        &report_clears(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![cleared_a.clone(), cleared_b.clone()],
            "",
            vec![
                clear_attempt(
                    &opa,
                    "win-a",
                    FrameSetter::Accepted,
                    Some(true),
                    Some(false),
                ),
                clear_attempt(
                    &opb,
                    "win-b",
                    FrameSetter::Accepted,
                    Some(true),
                    Some(false),
                ),
            ],
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "proof retires both");
    assert_eq!(desire_op(&second, "win-a"), None);
    assert_eq!(desire_op(&second, "win-b"), None);
    let scope = second
        .scopes
        .iter()
        .find(|scope| scope.scope == "out-a/ws-1")
        .expect("converged scope row");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Controlled,
        "proven frames converge after the clear lands"
    );
    // A further report never re-mints either lifetime: at-most-once holds
    // across the cohort.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![cleared_a, cleared_b],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0);
    assert_eq!(desire_op(&third, "win-a"), None);
    assert_eq!(desire_op(&third, "win-b"), None);
}

#[test]
fn cohort_clear_held_for_game() {
    // The focused-fullscreen-game lane vetoes birth-clear authority: no
    // binding mints and no token rides while paused.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut paused = cohort_report(1, 1);
    paused.presentation.paused = true;
    let reply = accepted(&mut engine, &paused);
    assert_eq!(engine.born_retained_len(), 0, "game safety wins");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_clear_held_for_gesture() {
    // An active gesture barrier defers birth admission: nothing mints.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut gated = cohort_report(1, 1);
    gated.gesture_barrier = true;
    let reply = accepted(&mut engine, &gated);
    assert_eq!(engine.born_retained_len(), 0, "gesture defers birth");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_clear_held_for_incomplete_screen() {
    // An incomplete screen capture proves nothing: the staged domain is
    // unreadable, so no binding mints.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut incomplete_screen = screen("out-a", 0, true);
    incomplete_screen.complete = false;
    let reply = accepted(
        &mut engine,
        &report(1, 1, vec![incomplete_screen], cohort_pair(), None, None),
    );
    assert_eq!(engine.born_retained_len(), 0, "incomplete proves nothing");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_clear_held_for_unknown_host_owned_homing() {
    // Host-owned unknown membership never proves a home: with the
    // capability set and no native receipt, the cohort holds.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut unknown = cohort_report(1, 1);
    unknown.host_owned_workspace_domains = true;
    let reply = accepted(&mut engine, &unknown);
    assert_eq!(engine.born_retained_len(), 0, "unknown native homing holds");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_fullscreen_pair_holds_slotless() {
    // Fullscreen safety wins for the whole cohort: both births hold
    // slotless with no clear, like the single fullscreen birth.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![
                born_fullscreen("win-a", "out-a", 100),
                born_fullscreen("win-b", "out-a", 300),
            ],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "fullscreen never clears");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_keep_mode_skips_born() {
    // Keep-maximized-overlay vetoes the whole cohort birth lane.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut kept = cohort_report(1, 1);
    kept.born_maximized = crate::born_maximized::BornMaximized::KeepMaximizedOverlay;
    let reply = accepted(&mut engine, &kept);
    assert_eq!(engine.born_retained_len(), 0, "keep mode never clears");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_fixed_pair_floats_without_clear() {
    // Fixed-size cohort births float beneath their overlay with no clear.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![
                fixed_maximized("win-a", "out-a", 100),
                fixed_maximized("win-b", "out-a", 300),
            ],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "fixed never clears");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
    for win in ["win-a", "win-b"] {
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("fixed row");
        assert!(row.floating, "{win}: fixed floats beneath the overlay");
    }
}

#[test]
fn cohort_minimized_pair_holds_without_spend() {
    // Minimized cohort facts prove nothing: no binding mints.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let minimized_a = WindowFact {
        minimized: true,
        ..born_maximized("win-a", "out-a", 100)
    };
    let minimized_b = WindowFact {
        minimized: true,
        ..born_maximized("win-b", "out-a", 300)
    };
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![minimized_a, minimized_b],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "minimized proves nothing");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn cohort_floating_pair_never_clears() {
    // Floating cohort identities keep their overlay with no clear.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let floating_a = WindowFact {
        floating: true,
        ..born_maximized("win-a", "out-a", 100)
    };
    let floating_b = WindowFact {
        floating: true,
        ..born_maximized("win-b", "out-a", 300)
    };
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![floating_a, floating_b],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "floating never clears");
    assert_eq!(desire_op(&reply, "win-a"), None);
    assert_eq!(desire_op(&reply, "win-b"), None);
}

#[test]
fn migration_gap_defers_born_mint_until_complete_inventory() {
    // Migration-gap safety journey: an outstanding workspace migration
    // touching the born window's home holds the birth while the inventory
    // is incomplete (matching the sweep gap fence); the next complete
    // report mints normally. The retained transfer outlives the gap.
    use crate::directional::OutputId;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let plain = WindowFact {
        fit_excluded: false,
        ..fact("win-keep", "out-a", 100)
    };
    let first = accepted(
        &mut engine,
        &report(1, 1, screens.clone(), vec![plain.clone()], None, None),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("out-a views a workspace");
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            screens.clone(),
            vec![plain.clone()],
            Vec::new(),
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
        ),
    );
    // Gapped report: born maximized window arrives on the migration target
    // while the transfer is still outstanding. No binding mints.
    let mut gapped = report(
        3,
        3,
        screens.clone(),
        vec![plain.clone(), born_maximized("win-born", "out-b", 900)],
        None,
        None,
    );
    gapped.inventory_complete = false;
    let gap_reply = accepted(&mut engine, &gapped);
    assert_eq!(engine.born_retained_len(), 0, "migration gap defers mint");
    assert_eq!(desire_op(&gap_reply, "win-born"), None);
    // Complete inventory with the same facts mints the outstanding birth:
    // the retained transfer alone never gates a provable report.
    let minted = accepted(
        &mut engine,
        &report(
            4,
            4,
            screens,
            vec![plain, born_maximized("win-born", "out-b", 900)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1, "complete inventory mints");
    assert!(desire_op(&minted, "win-born").is_some());
}

#[test]
fn sticky_flip_abandons_with_no_retry() {
    // A sticky flip ends the automatic clear with no retry: the retained
    // binding abandons on the flip report, publishes no desire there or
    // later, and never re-mints even if the window unstickies while
    // still maximized.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let sticky = WindowFact {
        sticky: true,
        ..born_maximized("win-born", "out-a", 100)
    };
    let flipped = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![sticky],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "sticky flip abandons");
    assert_eq!(desire_op(&flipped, "win-born"), None);
    let unsticky = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "abandoned sticky never re-mints"
    );
    assert_eq!(desire_op(&unsticky, "win-born"), None);
}

#[test]
fn floating_fact_flip_abandons_and_latches() {
    // A floating-fact flip ends the automatic clear with no retry and
    // latches floating-first: the flip report abandons, and a later
    // return to maximized-tiled facts never re-mints.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let floating = WindowFact {
        floating: true,
        ..born_maximized("win-born", "out-a", 100)
    };
    let flipped = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![floating],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "floating flip abandons");
    assert_eq!(desire_op(&flipped, "win-born"), None);
    let retiled = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "floating-first latch holds, no re-mint"
    );
    assert_eq!(desire_op(&retiled, "win-born"), None);
}

#[test]
fn floating_flip_gapped_withholds_desire_then_abandons() {
    // Gap plus floating flip: settlement holds the binding (never settles
    // from absence), but publish withholds the desire through the
    // per-window reproof. The next complete report abandons, and the
    // spent token never rides.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    let floating = WindowFact {
        floating: true,
        ..born_maximized("win-born", "out-a", 100)
    };
    let mut gapped = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![floating.clone()],
        None,
        None,
    );
    gapped.inventory_complete = false;
    let held = accepted(&mut engine, &gapped);
    assert_eq!(engine.born_retained_len(), 1, "gap holds the binding");
    assert_eq!(
        desire_op(&held, "win-born"),
        None,
        "publish reproof withholds across the flip"
    );
    let abandoned = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![floating],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "complete flip abandons");
    assert_eq!(desire_op(&abandoned, "win-born"), None);
    let _ = op;
}

#[test]
fn sticky_flip_gapped_withholds_desire() {
    // Gap plus sticky flip: the binding holds across the gap with no
    // desire, then abandons on the complete report with no retry.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let sticky = WindowFact {
        sticky: true,
        ..born_maximized("win-born", "out-a", 100)
    };
    let mut gapped = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![sticky.clone()],
        None,
        None,
    );
    gapped.inventory_complete = false;
    let held = accepted(&mut engine, &gapped);
    assert_eq!(engine.born_retained_len(), 1, "gap holds the binding");
    assert_eq!(desire_op(&held, "win-born"), None);
    let abandoned = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![sticky],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "complete sticky abandons");
    assert_eq!(desire_op(&abandoned, "win-born"), None);
    let retiled = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "no re-mint after sticky");
    assert_eq!(desire_op(&retiled, "win-born"), None);
}

#[test]
fn explicit_float_intent_supersedes_birth() {
    // Outstanding session float placement (the state a same-report
    // ToggleFloat commits before publish) supersedes the automatic birth:
    // settlement abandons with no retry, and no desire rides. A
    // ToggleFloat command on a maximized born-pending window refuses at
    // the session (only unflagged members float), so the intent is staged
    // directly here; the command-refusal path is pinned separately below.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    engine.retained_floats.push(RetainedFloat {
        operation_id: "op-7".to_owned(),
        window: WindowId("win-born".to_owned()),
        target: Rect {
            x: 600,
            y: 400,
            w: 150,
            h: 120,
        },
        source: None,
        output: OutputId("out-a".to_owned()),
        workspace: "ws-1".to_owned(),
        ambiguous: false,
        pending: None,
    });
    let superseded = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        engine.born_retained_len(),
        0,
        "session float intent abandons the birth"
    );
    assert_eq!(desire_op(&superseded, "win-born"), None);
    let later = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 0, "no re-actuation");
    assert_eq!(desire_op(&later, "win-born"), None);
}

#[test]
fn toggle_float_on_maximized_birth_refuses_without_clear() {
    // A ToggleFloat command on a maximized born-pending window refuses
    // (the session floats only unflagged members): no float intent is
    // created, ToggleFloat geometry/command semantics are unchanged, and
    // the automatic birth is correctly NOT superseded — the clear must
    // land before any float (clear-then-float coherence). The binding
    // survives the refusal and the desire rides the next report.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    let target = Rect {
        x: 600,
        y: 400,
        w: 150,
        h: 120,
    };
    match engine.handle_report(&report_full(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        Vec::new(),
        "",
        Some(7),
        Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-born".to_owned(),
            float_rect: Some(target),
        })),
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "not-tiled"),
        other => panic!("expected ToggleFloat refusal, got {other:?}"),
    }
    assert_eq!(engine.born_retained_len(), 1, "refusal keeps the binding");
    assert!(
        engine.retained_floats.is_empty(),
        "refused command creates no float intent"
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert!(desire_op(&third, "win-born").is_some());
}

#[test]
fn mode_flip_does_not_reclear_retained_lifetime() {
    // A global born-maximized setting flip never reclears or reclassifies
    // a retained lifetime: the outstanding binding keeps its operation id
    // across TileOnLaunch -> KeepMaximizedOverlay -> TileOnLaunch with no
    // second mint.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    let op = desire_op(&first, "win-born").expect("desire rides");
    assert_eq!(engine.born_retained_len(), 1);
    let mut kept = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    kept.born_maximized = crate::born_maximized::BornMaximized::KeepMaximizedOverlay;
    accepted(&mut engine, &kept);
    assert_eq!(engine.born_retained_len(), 1, "flip keeps the binding");
    assert_eq!(
        engine
            .born_operation_for(&WindowId("win-born".to_owned()))
            .as_deref(),
        Some(op.as_str()),
        "flip never re-mints"
    );
    let restored = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert_eq!(
        desire_op(&restored, "win-born").as_deref(),
        Some(op.as_str()),
        "same token resumes, no second clear"
    );
}

#[test]
fn migration_gap_withholds_born_clear_authority() {
    // Migration-gap safety: incomplete inventory with an outstanding
    // migration touching the home scope withholds mint and desire,
    // mirroring the sweep gap fence. A memberless retained migration is
    // staged directly (it holds inertly through settlement); only the
    // born-authority mirror is pinned here.
    use std::collections::BTreeMap;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert!(engine.retained.is_empty());
    engine.retained.push(RetainedTransfer {
        target: TransferTarget {
            operation_id: "op-11".to_owned(),
            kind: TransferKind::Migration,
            target_output: OutputId("out-b".to_owned()),
            target_workspace: "ws-9".to_owned(),
            follow: false,
            held: false,
        },
        policy: false,
        source_output: OutputId("out-a".to_owned()),
        source_workspace: "ws-1".to_owned(),
        members: Vec::new(),
        migration_refill: None,
        migration_initial_empty: true,
        migration_source_index: None,
        migration_source_view: None,
        migration_target_view: None,
        migration_terminal: None,
        pending_member: None,
        pending_members: BTreeMap::new(),
        pending_view: None,
    });
    let mut gapped = report(
        2,
        2,
        vec![screen("out-a", 0, true)],
        vec![born_maximized("win-born", "out-a", 100)],
        None,
        None,
    );
    gapped.inventory_complete = false;
    let held = accepted(&mut engine, &gapped);
    assert_eq!(engine.born_retained_len(), 1, "gap holds the binding");
    assert_eq!(
        desire_op(&held, "win-born"),
        None,
        "migration gap withholds the token"
    );
    engine.retained.clear();
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true)],
            vec![born_maximized("win-born", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(engine.born_retained_len(), 1);
    assert!(desire_op(&recovered, "win-born").is_some());
}
