#![forbid(unsafe_code)]
//! Q2 fixed-size float admission rows (D1-D8, authorized batch).
//!
//! Real `Session`/`Engine` fixtures (no flag mocks): the shared
//! `is_fixed_size` predicate floats newly admitted fixed windows as
//! membership-only automatic exceptions, honors explicit user tile wins
//! (retained set, command origin, or observed suppress signal) for the
//! same live client, bypasses born fullscreen, floats born maximized
//! without a reserved tile, retiles automatic (never intentional) floats,
//! and emits bounded diagnostics without identifiers. Failed proposals
//! and divergences stage nothing; the opt-out path keeps original
//! refusals byte-identical.

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::contract::{AckOutcome, AdapterAck, LifecycleCapabilities, Observation};
use crate::directional::{Node, OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{
    DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError, RefusalKind, Session,
    SessionCommand, SessionObservation,
};
use crate::size_hints::WindowSizeHints;
use std::collections::BTreeMap;

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn correlation(value: &str) -> CorrelationId {
    CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
}
fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}
fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    }
}
#[allow(clippy::too_many_arguments)]
fn obs(
    window: &str,
    floating: bool,
    fullscreen: bool,
    maximized: bool,
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating,
        fullscreen,
        maximized,
        sticky,
        fixed_auto,
        fixed_suppress,
        hints,
    }
}
fn plain(window: &str) -> ObservedWindow {
    obs(
        window,
        false,
        false,
        false,
        false,
        false,
        false,
        WindowSizeHints::none(),
    )
}
fn fixed(window: &str) -> ObservedWindow {
    obs(
        window,
        false,
        false,
        false,
        false,
        false,
        false,
        fixed_hints(),
    )
}
fn observation(base: u64, windows: Vec<ObservedWindow>) -> SessionObservation {
    SessionObservation {
        observation: Observation::new(owner(), generation(), base, 100 + base),
        windows,
    }
}
fn fixed_session() -> Session {
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    session.set_fixed_size_admission(true);
    session
}
fn admit_plain(session: &mut Session, window: &str, corr: &str) {
    admit_with(session, window, corr, false, WindowSizeHints::none());
}
fn admit_with(
    session: &mut Session,
    window: &str,
    corr: &str,
    suppress: bool,
    hints: WindowSizeHints,
) {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    let mut admitted = plain(window);
    admitted.hints = hints;
    windows.push(admitted);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let base = session.accepted_revision();
    let plan = session
        .propose(
            &SessionCommand::Admit {
                window: WindowId(window.to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: suppress,
            },
            &observation(base, windows),
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("seed ack");
    session
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("seed verify");
}
fn tiled_ids(session: &Session) -> Vec<String> {
    let mut ids: Vec<String> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    ids.sort();
    ids
}
fn tree_leaf_count(session: &Session) -> usize {
    fn collect(node: &Node, out: &mut usize) {
        match node {
            Node::Leaf { .. } => *out += 1,
            Node::Group { children, .. } => {
                for child in children {
                    collect(child, out);
                }
            }
        }
    }
    let view = session
        .snapshot()
        .domains
        .into_iter()
        .find(|d| d.output.0 == "out-1")
        .expect("domain");
    match view.tree {
        None => 0,
        Some(tree) => {
            let mut count = 0;
            collect(&tree, &mut count);
            count
        }
    }
}

#[test]
fn fixed_brand_new_floats_without_tile_focus_or_geometry() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let focus_before = session.focus();
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("converge");
    assert_eq!(result.flags_adopted, 1, "fixed admission adopts flags");
    assert_eq!(result.admitted, 0, "fixed admission takes no tiled slot");
    assert_eq!(result.removed, 0);
    assert!(
        session.is_exception(&WindowId("win-f".to_owned())),
        "fixed is exception"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-f".to_owned())),
        "automatic marker"
    );
    assert_eq!(
        tiled_ids(&session),
        vec!["win-a".to_owned()],
        "tree excludes fixed"
    );
    assert_eq!(tree_leaf_count(&session), 1, "no leaf reserved for fixed");
    assert_eq!(
        session.focus(),
        focus_before,
        "admission never refocuses to fixed"
    );
}

#[test]
fn nonfixed_hint_variants_tile() {
    let variants: Vec<(&str, WindowSizeHints)> = vec![
        (
            "missing-max",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: None,
                max_h: None,
            },
        ),
        (
            "missing-min",
            WindowSizeHints {
                min_w: None,
                min_h: None,
                max_w: Some(640),
                max_h: Some(480),
            },
        ),
        (
            "one-axis",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(100),
                max_w: Some(640),
                max_h: Some(480),
            },
        ),
        (
            "unequal",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(800),
                max_h: Some(600),
            },
        ),
        (
            "full-zero",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(0),
                max_w: Some(0),
                max_h: Some(0),
            },
        ),
        ("unset", WindowSizeHints::none()),
        (
            "negative",
            WindowSizeHints {
                min_w: Some(-1),
                min_h: Some(480),
                max_w: Some(-1),
                max_h: Some(480),
            },
        ),
        (
            "sentinel",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(i32::MAX),
                max_h: Some(i32::MAX),
            },
        ),
        (
            "out-of-range",
            WindowSizeHints {
                min_w: Some(16385),
                min_h: Some(16385),
                max_w: Some(16385),
                max_h: Some(16385),
            },
        ),
    ];
    for (name, hints) in variants {
        let mut session = fixed_session();
        let base = session.accepted_revision();
        let result = session
            .converge_observation(
                &observation(
                    base,
                    vec![obs(
                        "win-v", false, false, false, false, false, false, hints,
                    )],
                ),
                None,
            )
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(result.admitted, 1, "{name} tiles");
        assert!(
            !session.is_exception(&WindowId("win-v".to_owned())),
            "{name} not exception"
        );
        assert!(
            !session.is_automatic_fixed_float(&WindowId("win-v".to_owned())),
            "{name} not automatic"
        );
        assert_eq!(
            tiled_ids(&session),
            vec!["win-v".to_owned()],
            "{name} in tree"
        );
    }
}

#[test]
fn partial_zero_vectors_tile_under_both_axes_and_float_under_either_axis() {
    use crate::size_hints::FixedSizePredicate;
    // G-05: zero is unset per axis. Both transposed equal partial-zero
    // vectors pin nothing under `both-axes-fixed` (tile) but pin via their
    // nonzero axis under `either-axis-fixed` (float). Full-zero and the
    // unbounded sentinel stay tiled under both predicates.
    let partials = [
        (
            "w-fixed",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(0),
                max_w: Some(640),
                max_h: Some(0),
            },
        ),
        (
            "h-fixed",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(480),
                max_w: Some(0),
                max_h: Some(480),
            },
        ),
    ];
    for (name, hints) in partials {
        let mut both = fixed_session();
        let base = both.accepted_revision();
        let result = both
            .converge_observation(
                &observation(
                    base,
                    vec![obs(
                        "win-p", false, false, false, false, false, false, hints,
                    )],
                ),
                None,
            )
            .unwrap_or_else(|e| panic!("{name} both-axes: {e:?}"));
        assert_eq!(result.admitted, 1, "{name} tiles under both-axes");
        assert!(
            !both.is_automatic_fixed_float(&WindowId("win-p".to_owned())),
            "{name} not automatic under both-axes"
        );
        assert_eq!(
            tiled_ids(&both),
            vec!["win-p".to_owned()],
            "{name} keeps a slot under both-axes"
        );

        let mut either = fixed_session();
        either.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        let base = either.accepted_revision();
        either
            .converge_observation(
                &observation(
                    base,
                    vec![obs(
                        "win-p", false, false, false, false, false, false, hints,
                    )],
                ),
                None,
            )
            .unwrap_or_else(|e| panic!("{name} either-axis: {e:?}"));
        assert!(
            either.is_automatic_fixed_float(&WindowId("win-p".to_owned())),
            "{name} floats under either-axis"
        );
        assert_eq!(
            tiled_ids(&either),
            Vec::<String>::new(),
            "{name} takes no slot under either-axis"
        );
    }
    // Guards unchanged: full-zero and sentinel tile under either-axis too.
    for (name, hints) in [
        (
            "full-zero",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(0),
                max_w: Some(0),
                max_h: Some(0),
            },
        ),
        (
            "sentinel",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(i32::MAX),
                max_h: Some(i32::MAX),
            },
        ),
    ] {
        let mut either = fixed_session();
        either.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        let base = either.accepted_revision();
        either
            .converge_observation(
                &observation(
                    base,
                    vec![obs(
                        "win-p", false, false, false, false, false, false, hints,
                    )],
                ),
                None,
            )
            .unwrap_or_else(|e| panic!("{name} either-axis: {e:?}"));
        assert!(
            !either.is_automatic_fixed_float(&WindowId("win-p".to_owned())),
            "{name} still tiles under either-axis"
        );
    }
}

#[test]
fn partial_zero_predicate_switch_never_reclassifies_retained_tile() {
    use crate::size_hints::FixedSizePredicate;
    // Admissions-only (D1): a partial-zero client admitted tiled under
    // `both-axes-fixed` stays tiled after the switch to `either-axis-fixed`.
    let hints = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(0),
        max_w: Some(640),
        max_h: Some(0),
    };
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-p", false, false, false, false, false, false, hints,
                )],
            ),
            None,
        )
        .expect("partial-zero tiles by default");
    assert_eq!(tiled_ids(&session), vec!["win-p".to_owned()]);
    session.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-p", false, false, false, false, false, false, hints,
                )],
            ),
            None,
        )
        .expect("switch converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "switch changes nothing retained"
    );
    assert_eq!(tiled_ids(&session), vec!["win-p".to_owned()]);
    assert!(!session.is_automatic_fixed_float(&WindowId("win-p".to_owned())));
}

#[test]
fn hint_transitions_never_reclassify_either_direction() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    // Tiled window becomes fixed: stays tiled (D2).
    let base = session.accepted_revision();
    let result = session
        .converge_observation(&observation(base, vec![fixed("win-a")]), None)
        .expect("hint change converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "no change"
    );
    assert_eq!(tiled_ids(&session), vec!["win-a".to_owned()]);
    assert!(!session.is_automatic_fixed_float(&WindowId("win-a".to_owned())));
    // Automatic float loses fixed hints: stays floating (D2).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    assert!(session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        WindowSizeHints::none(),
                    ),
                ],
            ),
            None,
        )
        .expect("hint loss converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "identity kept"
    );
    assert!(
        session.is_exception(&WindowId("win-f".to_owned())),
        "still floating"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-f".to_owned())),
        "still automatic"
    );
}

#[test]
fn automatic_hint_loss_with_native_unfloat_stays_floating_until_explicit_win() {
    // D2: a known automatic float observed native floating:false with
    // hintless hints keeps floating with its marker (no fresh
    // admission, so no new geometry or focus); only a genuine
    // explicit win (suppress signal, retained override) tiles.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(
        session.is_automatic_fixed_float(&id),
        "automatic marker set"
    );
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(base, vec![plain("win-a"), plain("win-f")]),
            None,
        )
        .expect("hint loss converges");
    assert_eq!(
        (result.removed, result.admitted),
        (0, 0),
        "no fresh admission"
    );
    assert!(session.is_exception(&id), "still floating");
    assert!(
        session.is_automatic_fixed_float(&id),
        "marker kept across hint loss"
    );
    assert!(!session.has_fixed_tile_override(&id), "no inferred win");
    // An observed suppress signal with the same hintless hints is a
    // genuine explicit win: it tiles and records the override.
    let base = session.accepted_revision();
    let suppressed = obs(
        "win-f",
        false,
        false,
        false,
        false,
        false,
        true,
        WindowSizeHints::none(),
    );
    session
        .converge_observation(&observation(base, vec![plain("win-a"), suppressed]), None)
        .expect("suppress converges");
    assert!(!session.is_exception(&id), "tiled");
    assert!(session.has_fixed_tile_override(&id), "win recorded");
    // The retained override keeps it tiled across restored or lost hints.
    for hints in [fixed_hints(), WindowSizeHints::none()] {
        let base = session.accepted_revision();
        let shown = obs("win-f", false, false, false, false, false, false, hints);
        let result = session
            .converge_observation(&observation(base, vec![plain("win-a"), shown]), None)
            .expect("resync converges");
        assert_eq!(
            (result.removed, result.admitted),
            (0, 0),
            "override keeps tile"
        );
        assert_eq!(tiled_ids(&session).len(), 2, "stays tiled");
    }
}

#[test]
fn floating_fixed_without_origin_signal_stays_intentional() {
    // Known floating intent is never inferred automatic from hints
    // alone: without the adapter's explicit fixed_auto origin the
    // window floats as an intentional exception.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-f",
                    true,
                    false,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("converge");
    let id = WindowId("win-f".to_owned());
    assert!(session.is_exception(&id), "floating membership");
    assert!(!session.is_automatic_fixed_float(&id), "no inferred origin");
    // Re-observed tiled without a suppress signal becomes automatic
    // (D5/D6 fixed becomes float): no override is inferred, the
    // automatic marker is. An explicit suppress still tiles (D3, see
    // suppress_signal_records_override_on_retile).
    let base = session.accepted_revision();
    session
        .converge_observation(&observation(base, vec![fixed("win-f")]), None)
        .expect("retile");
    assert!(session.is_exception(&id), "fixed stays floating");
    assert!(
        session.is_automatic_fixed_float(&id),
        "fresh automatic marker"
    );
    assert!(
        !session.has_fixed_tile_override(&id),
        "no inferred override"
    );
}

#[test]
fn fixed_auto_signal_marks_automatic() {
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-f",
                    true,
                    false,
                    false,
                    false,
                    true,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("converge");
    let id = WindowId("win-f".to_owned());
    assert!(session.is_exception(&id));
    assert!(
        session.is_automatic_fixed_float(&id),
        "explicit origin honored"
    );
}

#[test]
fn suppress_signal_tiles_and_records_override() {
    // The adapter-retained tile win crosses admission: a suppressed
    // fixed window tiles and the win stages into the retained set, so
    // later unsuppressed observations keep it tiled.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-f",
                    false,
                    false,
                    false,
                    false,
                    false,
                    true,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("converge");
    assert_eq!(result.admitted, 1, "suppressed tiles");
    let id = WindowId("win-f".to_owned());
    assert!(!session.is_exception(&id));
    assert!(
        session.has_fixed_tile_override(&id),
        "win mirrored into retained set"
    );
    let base = session.accepted_revision();
    let result = session
        .converge_observation(&observation(base, vec![fixed("win-f")]), None)
        .expect("resync");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(tiled_ids(&session), vec!["win-f".to_owned()], "stays tiled");
}

fn unfloat_committed(session: &mut Session, window: &str, corr: &str) {
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    // The unfloat target rides floating until the toggle commits.
    for entry in windows.iter_mut() {
        if entry.window.0 == window {
            entry.floating = true;
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let plan = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId(window.to_owned()),
                float_geometry: None,
            },
            &observation(base, windows),
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("unfloat {window}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("unfloat ack");
    session
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("unfloat verify");
}

#[test]
fn explicit_unfloat_wins_across_hide_show() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    unfloat_committed(&mut session, "win-f", "c-u");
    let id = WindowId("win-f".to_owned());
    assert!(session.has_fixed_tile_override(&id), "override recorded");
    assert!(!session.is_automatic_fixed_float(&id), "automatic cleared");
    assert_eq!(tiled_ids(&session).len(), 2, "win-f tiled");
    // Hide/show and hint churn keep the explicit tile (D3, D2).
    for round in 0..2 {
        let base = session.accepted_revision();
        let hints = if round == 0 {
            fixed_hints()
        } else {
            WindowSizeHints::none()
        };
        let shown = obs("win-f", false, false, false, false, false, false, hints);
        let result = session
            .converge_observation(&observation(base, vec![plain("win-a"), shown]), None)
            .expect("resync");
        assert_eq!(
            (result.removed, result.admitted, result.flags_adopted),
            (0, 0, 0),
            "no re-float"
        );
        assert!(!session.is_exception(&id), "stays tiled");
    }
    assert!(
        session.has_fixed_tile_override(&id),
        "override survives re-observation"
    );
}

#[test]
fn removal_clears_override_and_replacement_reclassifies() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    unfloat_committed(&mut session, "win-f", "c-u");
    let id = WindowId("win-f".to_owned());
    assert!(session.has_fixed_tile_override(&id));
    // Removal drops bookkeeping (D3 fence).
    let base = session.accepted_revision();
    let result = session
        .converge_observation(&observation(base, vec![plain("win-a")]), None)
        .expect("remove");
    assert_eq!(result.removed, 1);
    assert!(
        !session.has_fixed_tile_override(&id),
        "override cleared on removal"
    );
    assert!(!session.is_automatic_fixed_float(&id));
    // A replaced native reference with the same id classifies again.
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("re-admit");
    assert!(
        session.is_automatic_fixed_float(&id),
        "new client floats again"
    );
}

#[test]
fn fixed_maximized_floats_without_tile_nonfixed_maxima_unchanged() {
    let mut session = fixed_session();
    let base = session.accepted_revision();
    // Fixed born maximized: floating base under the native overlay (D4).
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-m",
                    false,
                    false,
                    true,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("max fixed converges");
    assert!(
        session.is_exception(&WindowId("win-m".to_owned())),
        "max fixed floats"
    );
    assert!(session.is_automatic_fixed_float(&WindowId("win-m".to_owned())));
    assert_eq!(
        tree_leaf_count(&session),
        0,
        "no tile reserved under maximize"
    );
    // Nonfixed born maxima tile exactly as before (Q3 unchanged). The
    // maximize overlay itself stays native-only: core membership rides
    // the floating flag, so the retained exception observes floating.
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    obs(
                        "win-m",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                    obs(
                        "win-n",
                        false,
                        false,
                        true,
                        false,
                        false,
                        false,
                        WindowSizeHints::none(),
                    ),
                ],
            ),
            None,
        )
        .expect("nonfixed max converges");
    assert_eq!(
        tiled_ids(&session),
        vec!["win-n".to_owned()],
        "nonfixed max tiles"
    );
}

#[test]
fn born_fullscreen_holds_then_exits_fresh_fixed_floats() {
    let mut session = fixed_session();
    let base = session.accepted_revision();
    // Born fullscreen holds slotless with no writes (D5): plain
    // exception, no tiled slot, no automatic marker yet.
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("born FS converges");
    assert_eq!(result.admitted, 0, "born FS takes no tiled slot while held");
    assert!(
        session.is_exception(&WindowId("win-s".to_owned())),
        "held slotless"
    );
    assert!(
        !session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "no marker while held"
    );
    // First exit is fresh admission with the CURRENT predicate (D5):
    // fixed floats untouched automatic, no tiled slot.
    let base = session.accepted_revision();
    let result = session
        .converge_observation(&observation(base, vec![fixed("win-s")]), None)
        .expect("FS exit converges");
    assert!(
        session.is_exception(&WindowId("win-s".to_owned())),
        "fixed exit floats"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "automatic marker on exit"
    );
    assert_eq!(tiled_ids(&session), Vec::<String>::new(), "no tile taken");
    let _ = result;
}

#[test]
fn born_fullscreen_nonfixed_exit_tiles() {
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-n",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    WindowSizeHints::none(),
                )],
            ),
            None,
        )
        .expect("born FS converges");
    assert!(session.is_exception(&WindowId("win-n".to_owned())), "held");
    let base = session.accepted_revision();
    session
        .converge_observation(&observation(base, vec![plain("win-n")]), None)
        .expect("exit converges");
    assert_eq!(
        tiled_ids(&session),
        vec!["win-n".to_owned()],
        "nonfixed exit tiles"
    );
    assert!(
        !session.is_automatic_fixed_float(&WindowId("win-n".to_owned())),
        "no marker"
    );
}

#[test]
fn born_fullscreen_exit_uses_current_predicate() {
    // One-axis-fixed born fullscreen: exits tiled under both-axes,
    // floats under either-axis (CURRENT predicate, D5).
    let mut both = fixed_session();
    let base = both.accepted_revision();
    both.converge_observation(
        &observation(
            base,
            vec![obs(
                "win-s",
                false,
                true,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            )],
        ),
        None,
    )
    .expect("born holds");
    let base = both.accepted_revision();
    both.converge_observation(
        &observation(
            base,
            vec![obs(
                "win-s",
                false,
                false,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            )],
        ),
        None,
    )
    .expect("exit");
    assert_eq!(
        tiled_ids(&both),
        vec!["win-s".to_owned()],
        "both-axes exit tiles one-axis"
    );

    let mut either = fixed_session();
    either.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    let base = either.accepted_revision();
    either
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    one_axis_hints(),
                )],
            ),
            None,
        )
        .expect("born holds");
    let base = either.accepted_revision();
    either
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    false,
                    false,
                    false,
                    false,
                    false,
                    one_axis_hints(),
                )],
            ),
            None,
        )
        .expect("exit");
    assert!(
        either.is_exception(&WindowId("win-s".to_owned())),
        "either-axis exit floats one-axis"
    );
    assert!(
        either.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "automatic under either-axis"
    );
}

#[test]
fn born_fullscreen_hint_change_while_held_uses_exit_hints() {
    // Hints changing while still fullscreen must not classify: the
    // first exit decides with its own hints.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    WindowSizeHints::none(),
                )],
            ),
            None,
        )
        .expect("born holds hintless");
    // Still fullscreen but now reporting fixed: still held, no marker.
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("fullscreen hint churn holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "no classification while fullscreen"
    );
    assert!(session.is_exception(&WindowId("win-s".to_owned())), "held");
    assert!(
        !session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "no marker while held"
    );
    // Exit with fixed hints floats.
    let base = session.accepted_revision();
    session
        .converge_observation(&observation(base, vec![fixed("win-s")]), None)
        .expect("exit");
    assert!(
        session.is_exception(&WindowId("win-s".to_owned())),
        "exit floats"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "automatic on exit"
    );
}

#[test]
fn born_fullscreen_predicate_switch_while_held_uses_current() {
    // Predicate switches after birth but before exit: the exit uses
    // the CURRENT predicate, not birth-time state.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    one_axis_hints(),
                )],
            ),
            None,
        )
        .expect("born holds under both-axes");
    session.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    false,
                    false,
                    false,
                    false,
                    false,
                    one_axis_hints(),
                )],
            ),
            None,
        )
        .expect("exit");
    assert!(
        session.is_exception(&WindowId("win-s".to_owned())),
        "switched predicate floats one-axis on exit"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "automatic under current predicate"
    );
}

#[test]
fn repeated_fullscreen_exits_follow_retained_identity() {
    // First exit classifies fresh; later fullscreen cycles keep
    // retained identity instead of re-admitting.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("born holds");
    let base = session.accepted_revision();
    session
        .converge_observation(&observation(base, vec![fixed("win-s")]), None)
        .expect("first exit floats");
    assert!(session.is_automatic_fixed_float(&WindowId("win-s".to_owned())));
    // Second fullscreen cycle: rides as floating with origin, exits
    // retained even though hints are now plain (D2: no re-admission).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    true,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("second fullscreen rides");
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    true,
                    false,
                    false,
                    false,
                    false,
                    false,
                    WindowSizeHints::none(),
                )],
            ),
            None,
        )
        .expect("second exit retains");
    assert_eq!(
        (result.removed, result.admitted),
        (0, 0),
        "no fresh admission on later exit"
    );
    assert!(
        session.is_exception(&WindowId("win-s".to_owned())),
        "retained"
    );
    assert!(
        session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "marker retained"
    );
    // Tiled windows keep their slot across later fullscreen too,
    // even when fixed hints appear (D2: tiled hint changes never move).
    let mut tiled = fixed_session();
    admit_plain(&mut tiled, "win-t", "c-t");
    let base = tiled.accepted_revision();
    tiled
        .converge_observation(
            &observation(base, vec![plain("win-t"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    let id = WindowId("win-t".to_owned());
    let base = tiled.accepted_revision();
    tiled
        .converge_observation(
            &observation(
                base,
                vec![
                    obs(
                        "win-t",
                        false,
                        true,
                        false,
                        false,
                        false,
                        false,
                        WindowSizeHints::none(),
                    ),
                    obs(
                        "win-f",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("tiled fullscreen keeps slot");
    assert_eq!(tiled_ids(&tiled), vec!["win-t".to_owned()]);
    let base = tiled.accepted_revision();
    tiled
        .converge_observation(
            &observation(
                base,
                vec![
                    obs(
                        "win-t",
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                    obs(
                        "win-f",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("later exit keeps tile despite fixed hints");
    assert_eq!(tiled_ids(&tiled), vec!["win-t".to_owned()], "no re-float");
    assert!(!tiled.is_automatic_fixed_float(&id), "no marker inferred");
}

#[test]
fn repeated_fullscreen_hold_takes_no_slot() {
    // Unchanged fullscreen observations never classify: direct
    // fullscreen holds across repeats with no marker, even fixed.
    let mut session = fixed_session();
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("born holds");
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-s",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    fixed_hints(),
                )],
            ),
            None,
        )
        .expect("repeat holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "repeat fullscreen is converged hold"
    );
    assert!(session.is_exception(&WindowId("win-s".to_owned())), "held");
    assert!(
        !session.is_automatic_fixed_float(&WindowId("win-s".to_owned())),
        "still no marker before exit"
    );
    assert!(tiled_ids(&session).is_empty(), "no slot while held");
}

#[test]
fn optout_exception_retile_with_fullscreen_tiles_legacy() {
    // Without the opt-in there is no fullscreen hold: a floating
    // exception re-observed tiled while fullscreen re-tiles exactly
    // like before.
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    assert!(!session.fixed_size_admission(), "opt-in stays off");
    admit_plain(&mut session, "win-a", "c-a");
    float_intentional(&mut session, "win-a", "c-f");
    let id = WindowId("win-a".to_owned());
    assert!(session.is_exception(&id), "intentional floats");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![obs(
                    "win-a",
                    false,
                    true,
                    false,
                    false,
                    false,
                    false,
                    WindowSizeHints::none(),
                )],
            ),
            None,
        )
        .expect("retile converges");
    assert!(!session.is_exception(&id), "legacy retile tiles");
    assert_eq!(tiled_ids(&session), vec!["win-a".to_owned()]);
}

#[test]
fn optin_exception_retile_with_fullscreen_holds() {
    // Same observation shape with the opt-in on holds instead: the
    // automatic keeps floating with its marker and no tiled slot.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(session.is_automatic_fixed_float(&id));
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        false,
                        true,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("fullscreen holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "hold converges clean"
    );
    assert!(session.is_exception(&id), "still floating");
    assert!(session.is_automatic_fixed_float(&id), "marker kept");
    assert_eq!(tiled_ids(&session), vec!["win-a".to_owned()]);
}

#[test]
fn floating_fullscreen_cycle_restores_floating() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    // A formerly fixed-floating client entering fullscreen keeps floating.
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        true,
                        true,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("enter FS");
    assert!(session.is_exception(&id), "floating through FS");
    // Exiting restores floating (D5).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("exit FS");
    assert!(session.is_exception(&id), "restores floating");
    assert!(session.is_automatic_fixed_float(&id), "automatic kept");
}

fn float_intentional(session: &mut Session, window: &str, corr: &str) {
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let plan = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId(window.to_owned()),
                float_geometry: Some(Rect {
                    x: 10,
                    y: 10,
                    w: 100,
                    h: 80,
                }),
            },
            &observation(base, windows),
            &correlation(corr),
            &LifecycleCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("float {window}: {e:?}"));
    session
        .acknowledge(&AdapterAck::new(
            correlation(corr),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("float ack");
    session
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 200 + base),
            correlation(corr),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("float verify");
}

#[test]
fn workspace_enable_keeps_automatic_floats_others_tile() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    admit_plain(&mut session, "win-g", "c-g");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), plain("win-g"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    float_intentional(&mut session, "win-g", "c-f");
    let auto = WindowId("win-f".to_owned());
    let manual = WindowId("win-g".to_owned());
    assert!(session.is_automatic_fixed_float(&auto));
    assert!(
        !session.is_automatic_fixed_float(&manual),
        "intentional never automatic"
    );
    // Enabling workspace tiling checks every window (D6): the automatic
    // float stays floating untouched (no override recorded) while the
    // intentional float keeps its existing rule (re-tiles when observed
    // tiled without a fixed win).
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-g",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        WindowSizeHints::none(),
                    ),
                    fixed("win-f"),
                ],
            ),
            None,
        )
        .expect("retile");
    let _ = result;
    assert!(session.is_exception(&auto), "automatic stays floating");
    assert!(
        session.is_automatic_fixed_float(&auto),
        "automatic marker kept"
    );
    assert!(
        !session.has_fixed_tile_override(&auto),
        "no tile win recorded for automatic"
    );
    assert!(session.is_exception(&manual), "intentional keeps floating");
    assert!(
        !session.has_fixed_tile_override(&manual),
        "intentional untouched"
    );
    // An explicit suppress signal still tiles the automatic (D3 user
    // override wins for the same live client).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-g",
                        true,
                        false,
                        false,
                        false,
                        false,
                        false,
                        WindowSizeHints::none(),
                    ),
                    obs(
                        "win-f",
                        false,
                        false,
                        false,
                        false,
                        false,
                        true,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("explicit retile");
    assert!(!session.is_exception(&auto), "suppressed automatic tiles");
    assert!(
        session.has_fixed_tile_override(&auto),
        "explicit win recorded"
    );
    assert_eq!(
        tiled_ids(&session),
        vec!["win-a".to_owned(), "win-f".to_owned()],
        "retiled joins tree"
    );
}

#[test]
fn suppress_signal_records_override_on_retile() {
    // An intentional float re-tiled with the adapter's suppress signal
    // (sticky-off-to-tile) records the win; without the signal the
    // re-admit stays plain.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    admit_plain(&mut session, "win-g", "c-g");
    float_intentional(&mut session, "win-g", "c-f");
    let manual = WindowId("win-g".to_owned());
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-g",
                        false,
                        false,
                        false,
                        false,
                        false,
                        true,
                        WindowSizeHints::none(),
                    ),
                ],
            ),
            None,
        )
        .expect("retile with signal");
    assert!(!session.is_exception(&manual));
    assert!(session.has_fixed_tile_override(&manual), "signal recorded");
}

#[test]
fn sticky_adopts_automatic_as_intentional() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(session.is_automatic_fixed_float(&id));
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        true,
                        false,
                        false,
                        true,
                        false,
                        false,
                        fixed_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("sticky on");
    assert!(session.is_exception(&id), "stays floating");
    assert!(
        !session.is_automatic_fixed_float(&id),
        "sticky is intentional now"
    );
}

#[test]
fn explicit_admit_floats_fixed_as_deferred_without_geometry() {
    // The explicit SessionCommand::Admit route classifies too: no tile,
    // no geometry, focus kept. The suppress origin instead tiles with
    // its win staged transactionally.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let focus_before = session.focus();
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.push(fixed("win-f"));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let plan = session
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-f".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: false,
            },
            &observation(base, windows),
            &correlation("c-admit-fixed"),
            &LifecycleCapabilities::full(),
        )
        .expect("fixed admit proposes");
    assert!(
        plan.desired_geometry.is_empty(),
        "membership only, no geometry (D8)"
    );
    assert_eq!(plan.desired_focus_domain, focus_before.0, "focus untouched");
    assert_eq!(plan.desired_focus_leaf, focus_before.1, "focus untouched");
    session
        .acknowledge(&AdapterAck::new(
            correlation("c-admit-fixed"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    session
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 201),
            correlation("c-admit-fixed"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    assert!(session.is_exception(&WindowId("win-f".to_owned())));
    assert!(session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
}

#[test]
fn failed_propose_stages_nothing() {
    // A refused fixed admission stages no plan and no markers: the
    // committed sets are exactly untouched.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.push(fixed("win-f"));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = session
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-f".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                // Degenerate bounds refuse before anything stages.
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                },
                suppress_fixed_float: false,
            },
            &observation(base, windows),
            &correlation("c-fail"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("degenerate bounds refuse");
    assert_eq!(error, ProposeError::Refused(RefusalKind::MalformedInput));
    assert!(!session.has_pending(), "no pending staged");
    assert!(!session.has_pending_desired(), "no desired staged");
    assert!(!session.is_exception(&WindowId("win-f".to_owned())));
    assert!(!session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    assert!(!session.has_fixed_tile_override(&WindowId("win-f".to_owned())));
}

#[test]
fn divergence_after_propose_discards_staged_markers() {
    // A divergence between propose and verify clears the staged plan
    // including its fixed-size markers: the committed sets never see
    // the failed admission.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.push(fixed("win-f"));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    session
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-f".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: false,
            },
            &observation(base, windows),
            &correlation("c-div"),
            &LifecycleCapabilities::full(),
        )
        .expect("fixed admit proposes");
    assert!(session.has_pending(), "plan pending");
    session.note_adapter_loss();
    assert!(!session.has_pending(), "pending discarded");
    assert!(!session.has_pending_desired(), "desired discarded");
    assert!(
        !session.is_automatic_fixed_float(&WindowId("win-f".to_owned())),
        "no leaked marker"
    );
    assert!(!session.has_fixed_tile_override(&WindowId("win-f".to_owned())));
    assert_eq!(
        tiled_ids(&session),
        vec!["win-a".to_owned()],
        "topology untouched"
    );
}

#[test]
fn optout_keeps_original_refusals_and_tiles_fixed() {
    // With the opt-in off, legacy behavior is byte-identical: fixed
    // hints tile (no classifier), overlay admissions refuse as
    // partial-observation, and unfloat bindings compare exact flags.
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    assert!(!session.fixed_size_admission(), "opt-in defaults off");
    admit_plain(&mut session, "win-a", "c-a");
    // Fixed hints tile without the opt-in.
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("converge");
    assert_eq!(result.admitted, 1, "opt-out tiles fixed");
    assert!(!session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    assert!(!session.has_fixed_tile_override(&WindowId("win-f".to_owned())));
    // Overlay admission refuses exactly as before.
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.push(obs(
        "win-o",
        false,
        true,
        false,
        false,
        false,
        false,
        fixed_hints(),
    ));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = session
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-o".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: false,
            },
            &observation(base, windows),
            &correlation("c-opt"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("overlay admit refuses");
    assert_eq!(
        error,
        ProposeError::Refused(RefusalKind::PartialObservation)
    );
}

#[test]
fn optout_unfloat_keeps_exact_flag_binding() {
    // Without the opt-in, an unfloat whose observed flags drift from the
    // record (sticky overlay) refuses exactly as before.
    let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    admit_plain(&mut session, "win-a", "c-a");
    float_intentional(&mut session, "win-a", "c-f");
    let base = session.accepted_revision();
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| plain(&l.window.0))
        .collect();
    windows.extend(session.exception_observed());
    for entry in windows.iter_mut() {
        if entry.window.0 == "win-a" {
            entry.sticky = true;
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = session
        .propose(
            &SessionCommand::ToggleFloat {
                window: WindowId("win-a".to_owned()),
                float_geometry: None,
            },
            &observation(base, windows),
            &correlation("c-opt-u"),
            &LifecycleCapabilities::full(),
        )
        .expect_err("drifted flags refuse");
    assert_eq!(
        error,
        ProposeError::Refused(RefusalKind::PartialObservation)
    );
    assert!(!session.has_pending(), "no pending staged");
}

#[test]
fn fixed_size_reason_vocabulary() {
    use crate::size_hints::{fixed_size_reason, is_fixed_size};
    assert!(is_fixed_size(fixed_hints()));
    assert_eq!(fixed_size_reason(fixed_hints()), "fixed-equal");
    assert_eq!(
        fixed_size_reason(WindowSizeHints::none()),
        "not-fixed-missing"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(-1),
            min_h: None,
            max_w: Some(-1),
            max_h: None
        }),
        "not-fixed-negative"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(1),
            min_h: Some(1),
            max_w: Some(i32::MAX),
            max_h: Some(i32::MAX)
        }),
        "not-fixed-sentinel"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(16385),
            min_h: Some(16385),
            max_w: Some(16385),
            max_h: Some(16385)
        }),
        "not-fixed-out-of-range"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(0),
            min_h: Some(0),
            max_w: Some(0),
            max_h: Some(0)
        }),
        "not-fixed-zero"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(1),
            min_h: Some(1),
            max_w: Some(2),
            max_h: Some(2)
        }),
        "not-fixed-unequal"
    );
}

fn carried(
    window: &str,
    floating: bool,
    fullscreen: bool,
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        floating,
        fit_excluded: floating || fullscreen,
        fullscreen,
        maximized: false,
        sticky,
        fixed_auto,
        fixed_suppress,
        hints,
    }
}
fn plain_carried(window: &str) -> EngineWindow {
    carried(
        window,
        false,
        false,
        false,
        false,
        false,
        WindowSizeHints::none(),
    )
}
fn fixed_carried(window: &str) -> EngineWindow {
    carried(window, false, false, false, false, false, fixed_hints())
}
fn reconcile_event(base: u64, focused: &str, windows: Vec<EngineWindow>, corr: &str) -> CoreEvent {
    CoreEvent {
        owner: owner(),
        generation: generation(),
        correlation: correlation(corr),
        revision: base,
        fingerprint: 999,
        domain: domain(),
        domain_key: key(),
        outer_gap: 0,
        focused_window: WindowId(focused.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: vec![],
        closed_windows: vec![],
        command: CoreCommand::Reconcile,
    }
}

#[test]
fn engine_fresh_fixed_reports_bounded_diagnostic() {
    // Opt-in engine: fresh foreground+hidden-style adoption classifies (D7).
    let mut engine = Engine::new();
    engine.sync_binding(&owner(), &generation());
    engine.set_fixed_size_admission(true);
    let event = reconcile_event(
        0,
        "win-a",
        vec![plain_carried("win-a"), fixed_carried("win-f")],
        "fixed-eng-1",
    );
    match engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            let ids: Vec<&str> = plan.geometry.iter().map(|g| g.window.0.as_str()).collect();
            assert!(ids.contains(&"win-a"), "sibling tiles {ids:?}");
            assert!(
                !ids.contains(&"win-f"),
                "fixed takes no geometry (D8) {ids:?}"
            );
        }
        CoreReply::Projection(_) => {}
        other => panic!("fresh fixed must plan, got {other:?}"),
    }
    let session = engine.session(&key()).expect("session retained");
    assert!(session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    let report = engine
        .last_fixed_admission()
        .expect("fixed report recorded");
    assert_eq!(report.correlation.as_str(), "fixed-eng-1", "correlated");
    assert_eq!(report.reason, "fixed-equal");
    assert_eq!(report.admitted, 1);
    assert_eq!(report.evaluated, 1);
    // A suppressed re-observation after domain release tiles without
    // reclassifying: the adapter-retained win crosses sessions.
    let mut engine2 = Engine::new();
    engine2.sync_binding(&owner(), &generation());
    engine2.set_fixed_size_admission(true);
    let event = reconcile_event(
        0,
        "win-f",
        vec![carried(
            "win-f",
            false,
            false,
            false,
            false,
            true,
            fixed_hints(),
        )],
        "fixed-eng-3",
    );
    match engine2.handle(&event) {
        CoreReply::Tiled(plan) => {
            let ids: Vec<&str> = plan.geometry.iter().map(|g| g.window.0.as_str()).collect();
            assert!(ids.contains(&"win-f"), "suppressed tiles {ids:?}");
        }
        other => panic!("suppressed must tile, got {other:?}"),
    }
    assert!(
        engine2.last_fixed_admission().is_none(),
        "suppression logs no admission"
    );
    // Windows default (opt-in off) keeps exact current behavior: fixed tiles.
    let mut plain_engine = Engine::new();
    plain_engine.sync_binding(&owner(), &generation());
    let event = reconcile_event(
        0,
        "win-a",
        vec![plain_carried("win-a"), fixed_carried("win-f")],
        "fixed-eng-2",
    );
    match plain_engine.handle(&event) {
        CoreReply::Tiled(plan) => {
            let ids: Vec<&str> = plan.geometry.iter().map(|g| g.window.0.as_str()).collect();
            assert!(ids.contains(&"win-f"), "opt-out tiles fixed {ids:?}");
        }
        other => panic!("opt-out must tile, got {other:?}"),
    }
    assert!(
        plain_engine.last_fixed_admission().is_none(),
        "no report when off"
    );
}

fn dom_pair(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn obs_homed(
    window: &str,
    floating: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
    output: &str,
    workspace: &str,
) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress,
        hints,
    }
}

fn observation_fp(base: u64, fingerprint: u64, windows: Vec<ObservedWindow>) -> SessionObservation {
    SessionObservation {
        observation: Observation::new(owner(), generation(), base, fingerprint),
        windows,
    }
}

fn fixed_session_at(output: &str, workspace: &str) -> Session {
    let mut session = Session::new(
        owner(),
        generation(),
        0,
        7,
        vec![dom_pair(output, workspace)],
    )
    .expect("session");
    session.set_fixed_size_admission(true);
    session
}

/// Full pair/send/split metadata roundtrip for one suppressed tile win
/// plus one automatic float (D3, actual send fixture API).
///
/// Source holds win-keep (plain tile), win-auto (automatic fixed float),
/// and win-sup (fixed, explicitly untilled with a retained override);
/// target holds win-t. Pairing unions the marks, the workspace send
/// carries the suppressed mover, and the split partitions every mark
/// with membership. A refused automatic send stages nothing.
#[test]
fn pair_send_split_preserves_automatic_and_override() {
    let dom_a = dom_pair("out-1", "ws-a");
    let dom_b = dom_pair("out-1", "ws-b");
    let mut source = fixed_session_at("out-1", "ws-a");
    // Plain tile first so later fixed admissions never steal focus.
    let base = source.accepted_revision();
    let plan = source
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-keep".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: false,
            },
            &observation_fp(
                base,
                300,
                vec![obs_homed(
                    "win-keep",
                    false,
                    false,
                    WindowSizeHints::none(),
                    "out-1",
                    "ws-a",
                )],
            ),
            &correlation("pair-keep"),
            &LifecycleCapabilities::full(),
        )
        .expect("admit win-keep");
    source
        .acknowledge(&AdapterAck::new(
            correlation("pair-keep"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    source
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 301),
            correlation("pair-keep"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    // Both fixed windows admit as automatic floats in one convergence.
    let base = source.accepted_revision();
    let result = source
        .converge_observation(
            &observation_fp(
                base,
                302,
                vec![
                    obs_homed(
                        "win-keep",
                        false,
                        false,
                        WindowSizeHints::none(),
                        "out-1",
                        "ws-a",
                    ),
                    obs_homed("win-auto", false, false, fixed_hints(), "out-1", "ws-a"),
                    obs_homed("win-sup", false, false, fixed_hints(), "out-1", "ws-a"),
                ],
            ),
            None,
        )
        .expect("converge fixed pair");
    assert_eq!(result.flags_adopted, 2, "both fixed float");
    let auto = WindowId("win-auto".to_owned());
    let sup = WindowId("win-sup".to_owned());
    assert!(source.is_automatic_fixed_float(&auto));
    assert!(source.is_automatic_fixed_float(&sup));
    // Explicit unfloat records the suppress override for win-sup only.
    let base = source.accepted_revision();
    let plan = source
        .propose(
            &SessionCommand::ToggleFloat {
                window: sup.clone(),
                float_geometry: None,
            },
            &observation_fp(
                base,
                303,
                vec![
                    obs_homed(
                        "win-keep",
                        false,
                        false,
                        WindowSizeHints::none(),
                        "out-1",
                        "ws-a",
                    ),
                    obs_homed(
                        "win-auto",
                        true,
                        false,
                        WindowSizeHints::none(),
                        "out-1",
                        "ws-a",
                    ),
                    obs_homed(
                        "win-sup",
                        true,
                        false,
                        WindowSizeHints::none(),
                        "out-1",
                        "ws-a",
                    ),
                ],
            ),
            &correlation("pair-unfloat"),
            &LifecycleCapabilities::full(),
        )
        .expect("unfloat win-sup");
    source
        .acknowledge(&AdapterAck::new(
            correlation("pair-unfloat"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    source
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 304),
            correlation("pair-unfloat"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    assert!(source.has_fixed_tile_override(&sup), "override recorded");
    assert!(!source.is_automatic_fixed_float(&sup), "automatic cleared");
    assert!(
        source.is_automatic_fixed_float(&auto),
        "sibling stays automatic"
    );
    assert!(
        source.sync_focus_from_window(
            &DomainKey {
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
            },
            &sup,
        ),
        "mover focused for the send"
    );
    let mut target = fixed_session_at("out-1", "ws-b");
    let base = target.accepted_revision();
    let plan = target
        .propose(
            &SessionCommand::Admit {
                window: WindowId("win-t".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-b".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                suppress_fixed_float: false,
            },
            &observation_fp(
                base,
                310,
                vec![obs_homed(
                    "win-t",
                    false,
                    false,
                    WindowSizeHints::none(),
                    "out-1",
                    "ws-b",
                )],
            ),
            &correlation("pair-t"),
            &LifecycleCapabilities::full(),
        )
        .expect("admit win-t");
    target
        .acknowledge(&AdapterAck::new(
            correlation("pair-t"),
            owner(),
            generation(),
            base,
            AckOutcome::Accepted,
        ))
        .expect("ack");
    target
        .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
            Observation::new(owner(), generation(), base, 311),
            correlation("pair-t"),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("verify");
    // Roundtrip with no mover: pairing then splitting preserves every
    // mark on its homed side.
    let pair = Session::paired_from_canonical_for_send(
        &source,
        Some(&target),
        vec![dom_a.clone(), dom_b.clone()],
    )
    .expect("pair");
    assert!(
        pair.is_automatic_fixed_float(&auto),
        "pair unions automatic"
    );
    assert!(pair.has_fixed_tile_override(&sup), "pair unions override");
    let (source_rt, target_rt) = pair.split_canonical_pair().expect("split");
    let target_rt = target_rt.expect("target retained");
    assert!(
        source_rt.is_automatic_fixed_float(&auto),
        "automatic stays sourced"
    );
    assert!(
        source_rt.has_fixed_tile_override(&sup),
        "override stays sourced"
    );
    assert!(
        !target_rt.is_automatic_fixed_float(&auto) && !target_rt.has_fixed_tile_override(&sup),
        "target gains no foreign marks"
    );
    // An automatic float is no send mover: the refusal stages nothing.
    let mut pair = Session::paired_from_canonical_for_send(
        &source,
        Some(&target),
        vec![dom_a.clone(), dom_b.clone()],
    )
    .expect("pair for refusal");
    let base = pair.accepted_revision();
    let mut observed: Vec<ObservedWindow> = pair
        .snapshot()
        .windows
        .iter()
        .map(|l| {
            obs_homed(
                &l.window.0,
                false,
                l.window == sup,
                if l.window == sup {
                    fixed_hints()
                } else {
                    WindowSizeHints::none()
                },
                &l.output.0,
                &l.workspace.0,
            )
        })
        .collect();
    observed.extend(pair.exception_observed());
    match pair.propose(
        &SessionCommand::MoveToWorkspace {
            window: auto.clone(),
            target_output: OutputId("out-1".to_owned()),
            target_workspace: WorkspaceId("ws-b".to_owned()),
            follow: true,
        },
        &observation_fp(base, 320, observed),
        &correlation("pair-refuse-auto"),
        &LifecycleCapabilities::full(),
    ) {
        Err(ProposeError::Refused(RefusalKind::NotTiled)) => {}
        other => panic!("automatic send must refuse NotTiled, got {other:?}"),
    }
    assert!(
        pair.is_automatic_fixed_float(&auto),
        "refusal keeps automatic"
    );
    // The suppressed tile win sends: the mover lands tiled on the target
    // with its override, while the automatic sibling never moves.
    let base = pair.accepted_revision();
    let mut observed: Vec<ObservedWindow> = pair
        .snapshot()
        .windows
        .iter()
        .map(|l| {
            obs_homed(
                &l.window.0,
                false,
                l.window == sup,
                if l.window == sup {
                    fixed_hints()
                } else {
                    WindowSizeHints::none()
                },
                &l.output.0,
                &l.workspace.0,
            )
        })
        .collect();
    observed.extend(pair.exception_observed());
    let plan = pair
        .propose(
            &SessionCommand::MoveToWorkspace {
                window: sup.clone(),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: WorkspaceId("ws-b".to_owned()),
                follow: true,
            },
            &observation_fp(base, 321, observed),
            &correlation("pair-send"),
            &LifecycleCapabilities::full(),
        )
        .expect("suppressed send proposes");
    assert!(
        plan.desired_geometry
            .iter()
            .any(|g| g.window == sup && g.workspace.0 == "ws-b"),
        "mover desired on target"
    );
    pair.acknowledge(&AdapterAck::new(
        correlation("pair-send"),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    pair.verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 322),
        correlation("pair-send"),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify");
    let (source_after, target_after) = pair.split_canonical_pair().expect("split");
    let target_after = target_after.expect("target retained");
    assert!(
        source_after.is_automatic_fixed_float(&auto),
        "automatic never moves"
    );
    assert!(
        !source_after.has_fixed_tile_override(&sup),
        "override travels with the mover, never duplicates"
    );
    assert!(
        target_after.has_fixed_tile_override(&sup),
        "target keeps the tile win"
    );
    assert!(
        !target_after.is_automatic_fixed_float(&sup),
        "moved tile is not automatic"
    );
    let sup_link = target_after
        .snapshot()
        .windows
        .into_iter()
        .find(|l| l.window == sup)
        .expect("mover tiled on target");
    assert_eq!(sup_link.workspace.0, "ws-b", "mover homed on target");
    assert!(
        target_after
            .snapshot()
            .windows
            .iter()
            .any(|l| l.window.0 == "win-t"),
        "target survivor undisturbed"
    );
}

fn one_axis_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(100),
        max_w: Some(640),
        max_h: Some(480),
    }
}

#[test]
fn predicate_defaults_to_both_axes_matching_delivered_behavior() {
    use crate::size_hints::{FixedSizePredicate, fixed_size_reason_with, is_fixed_size_with};
    assert_eq!(FixedSizePredicate::default(), FixedSizePredicate::BothAxes);
    assert_eq!(
        FixedSizePredicate::BothAxes.as_wire_str(),
        "both-axes-fixed"
    );
    assert_eq!(
        FixedSizePredicate::EitherAxis.as_wire_str(),
        "either-axis-fixed"
    );
    assert_eq!(
        FixedSizePredicate::parse_wire("both-axes-fixed"),
        Some(FixedSizePredicate::BothAxes)
    );
    assert_eq!(
        FixedSizePredicate::parse_wire("either-axis-fixed"),
        Some(FixedSizePredicate::EitherAxis)
    );
    for invalid in ["", "both", "either", "fixed", "both_axes_fixed", "COSMIC"] {
        assert_eq!(FixedSizePredicate::parse_wire(invalid), None, "{invalid:?}");
    }
    // One-axis fixture: tiles under the default, floats under either-axis.
    assert!(!is_fixed_size_with(
        one_axis_hints(),
        FixedSizePredicate::BothAxes
    ));
    assert!(is_fixed_size_with(
        one_axis_hints(),
        FixedSizePredicate::EitherAxis
    ));
    assert_eq!(
        fixed_size_reason_with(one_axis_hints(), FixedSizePredicate::BothAxes),
        "not-fixed-unequal"
    );
    assert_eq!(
        fixed_size_reason_with(one_axis_hints(), FixedSizePredicate::EitherAxis),
        "fixed-equal"
    );
    // Guards preserved under either-axis: missing/sentinel still tile.
    let missing = WindowSizeHints {
        min_w: Some(640),
        min_h: None,
        max_w: Some(640),
        max_h: Some(480),
    };
    assert!(!is_fixed_size_with(missing, FixedSizePredicate::EitherAxis));
    let sentinel = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(i32::MAX),
    };
    assert!(!is_fixed_size_with(
        sentinel,
        FixedSizePredicate::EitherAxis
    ));
    // Engine/session defaults match.
    let engine = Engine::new();
    assert!(!engine.fixed_size_admission(), "opt-in defaults off");
    assert_eq!(engine.fixed_size_predicate(), FixedSizePredicate::BothAxes);
    let session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("session");
    assert_eq!(session.fixed_size_predicate(), FixedSizePredicate::BothAxes);
}

#[test]
fn either_axis_admits_one_axis_fixed_while_both_axes_tiles() {
    let mut both = fixed_session();
    admit_plain(&mut both, "win-a", "c-a");
    let base = both.accepted_revision();
    let result = both
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        one_axis_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("converge");
    assert_eq!(result.flags_adopted, 0, "one-axis tiles under both-axes");
    assert!(!both.is_exception(&WindowId("win-f".to_owned())));
    assert_eq!(
        tiled_ids(&both),
        vec!["win-a".to_owned(), "win-f".to_owned()]
    );

    let mut either = fixed_session();
    either.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    admit_plain(&mut either, "win-a", "c-a");
    let base = either.accepted_revision();
    let result = either
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-f",
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        one_axis_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("converge");
    assert_eq!(result.flags_adopted, 1, "one-axis floats under either-axis");
    assert!(either.is_exception(&WindowId("win-f".to_owned())));
    assert!(either.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    assert_eq!(tiled_ids(&either), vec!["win-a".to_owned()]);
}

#[test]
fn predicate_switch_applies_to_subsequent_admissions_without_reclassifying() {
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-a");
    // win-b admitted tiled under the both-axes default (one-axis hints tile).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    obs(
                        "win-b",
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        one_axis_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("converge");
    assert!(!session.is_exception(&WindowId("win-b".to_owned())));
    // Switch to either-axis: retained win-b stays tiled (no reclassification).
    session.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), plain("win-b")]),
            None,
        )
        .expect("converge");
    assert!(
        !session.is_exception(&WindowId("win-b".to_owned())),
        "switch must not reclassify retained windows"
    );
    assert_eq!(
        tiled_ids(&session),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    // Subsequent one-axis admission floats under the new predicate.
    let base = session.accepted_revision();
    let result = session
        .converge_observation(
            &observation(
                base,
                vec![
                    plain("win-a"),
                    plain("win-b"),
                    obs(
                        "win-c",
                        false,
                        false,
                        false,
                        false,
                        false,
                        false,
                        one_axis_hints(),
                    ),
                ],
            ),
            None,
        )
        .expect("converge");
    assert_eq!(result.flags_adopted, 1);
    assert!(session.is_automatic_fixed_float(&WindowId("win-c".to_owned())));
}

#[test]
fn live_tile_override_wins_under_either_axis_predicate() {
    let mut session = fixed_session();
    session.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    admit_plain(&mut session, "win-a", "c-a");
    // Explicit user tile (suppress origin) wins for the live client.
    admit_with(&mut session, "win-f", "c-f", true, one_axis_hints());
    assert!(
        !session.is_exception(&WindowId("win-f".to_owned())),
        "override wins over either-axis admission"
    );
    assert!(session.has_fixed_tile_override(&WindowId("win-f".to_owned())));
    assert_eq!(
        tiled_ids(&session),
        vec!["win-a".to_owned(), "win-f".to_owned()]
    );
    // Later re-observation keeps it tiled (no reclassification).
    let base = session.accepted_revision();
    session
        .converge_observation(
            &observation(base, vec![plain("win-a"), plain("win-f")]),
            None,
        )
        .expect("converge");
    assert!(!session.is_exception(&WindowId("win-f".to_owned())));
}

// B9 maximized intentional unfloat (M09=A): shared-core regression evidence.
// No production change: a floating window still observed maximized refuses
// the unfloat (pre-clear state), while the same float observed unmaximized
// (post-clear state) fresh-admits as a new tiled window with the D3 suppress
// pin, so fixed clients never re-auto-float. Both the ordinary and the
// fixed-size legs are covered.
#[test]
fn b9_maximized_float_unfloat_refuses_until_clear_then_fresh_admits() {
    // Ordinary non-fixed leg.
    let mut session = fixed_session();
    admit_plain(&mut session, "win-a", "c-b9-a1");
    admit_plain(&mut session, "win-b", "c-b9-a2");
    float_intentional(&mut session, "win-b", "c-b9-f1");
    // Pre-clear: floating + maximized refuses.
    {
        let base = session.accepted_revision();
        let windows = vec![
            plain("win-a"),
            obs(
                "win-b",
                true,
                false,
                true,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ];
        let err = session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-b".to_owned()),
                    float_geometry: Some(Rect {
                        x: 0,
                        y: 0,
                        w: 1200,
                        h: 800,
                    }),
                },
                &observation(base, windows),
                &correlation("c-b9-pre1"),
                &LifecycleCapabilities::full(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            ProposeError::Refused(RefusalKind::PartialObservation),
            "maximized float must refuse before the native clear"
        );
        assert!(!session.has_pending_desired(), "refusal stages nothing");
    }
    // Post-clear: floating + unmaximized fresh-admits with the D3 pin.
    {
        let base = session.accepted_revision();
        let windows = vec![
            plain("win-a"),
            obs(
                "win-b",
                true,
                false,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ];
        let plan = session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-b".to_owned()),
                    float_geometry: Some(Rect {
                        x: 100,
                        y: 100,
                        w: 400,
                        h: 300,
                    }),
                },
                &observation(base, windows),
                &correlation("c-b9-post1"),
                &LifecycleCapabilities::full(),
            )
            .unwrap_or_else(|e| panic!("cleared unfloat admits: {e:?}"));
        session
            .acknowledge(&AdapterAck::new(
                correlation("c-b9-post1"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        session
            .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
                Observation::new(owner(), generation(), base, 200 + base),
                correlation("c-b9-post1"),
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("verify");
        assert_eq!(
            tiled_ids(&session),
            vec!["win-a".to_owned(), "win-b".to_owned()],
            "cleared float rejoins the tiled topology"
        );
        assert!(
            session.has_fixed_tile_override(&WindowId("win-b".to_owned())),
            "explicit unfloat records the user tile win"
        );
    }

    // Fixed-size leg: automatic float, maximized, then cleared.
    let mut fixed_session_state = fixed_session();
    admit_plain(&mut fixed_session_state, "win-a", "c-b9-fa");
    let base = fixed_session_state.accepted_revision();
    fixed_session_state
        .converge_observation(
            &observation(base, vec![plain("win-a"), fixed("win-f")]),
            None,
        )
        .expect("admit fixed automatic");
    assert!(fixed_session_state.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
    // Pre-clear refuses without staging.
    {
        let base = fixed_session_state.accepted_revision();
        let mut windows: Vec<ObservedWindow> = fixed_session_state
            .snapshot()
            .windows
            .iter()
            .map(|l| plain(&l.window.0))
            .collect();
        windows.extend(fixed_session_state.exception_observed());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
                entry.maximized = true;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        let err = fixed_session_state
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-f".to_owned()),
                    float_geometry: Some(Rect {
                        x: 0,
                        y: 0,
                        w: 1200,
                        h: 800,
                    }),
                },
                &observation(base, windows),
                &correlation("c-b9-pre2"),
                &LifecycleCapabilities::full(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            ProposeError::Refused(RefusalKind::PartialObservation),
            "maximized automatic float must refuse before the native clear"
        );
    }
    // Post-clear admits with the suppress pin and never re-auto-floats.
    {
        let base = fixed_session_state.accepted_revision();
        let mut windows: Vec<ObservedWindow> = fixed_session_state
            .snapshot()
            .windows
            .iter()
            .map(|l| plain(&l.window.0))
            .collect();
        windows.extend(fixed_session_state.exception_observed());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
                entry.maximized = false;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        let plan = fixed_session_state
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-f".to_owned()),
                    float_geometry: Some(Rect {
                        x: 100,
                        y: 100,
                        w: 400,
                        h: 300,
                    }),
                },
                &observation(base, windows),
                &correlation("c-b9-post2"),
                &LifecycleCapabilities::full(),
            )
            .unwrap_or_else(|e| panic!("cleared fixed unfloat admits: {e:?}"));
        fixed_session_state
            .acknowledge(&AdapterAck::new(
                correlation("c-b9-post2"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        fixed_session_state
            .verify_lifecycle(&crate::contract::LifecyclePostObservation::new(
                Observation::new(owner(), generation(), base, 200 + base),
                correlation("c-b9-post2"),
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("verify");
        let id = WindowId("win-f".to_owned());
        assert!(
            fixed_session_state.has_fixed_tile_override(&id),
            "explicit unfloat wins over fixed re-float"
        );
        assert!(
            !fixed_session_state.is_automatic_fixed_float(&id),
            "automatic identity withdrawn"
        );
        assert_eq!(
            tiled_ids(&fixed_session_state),
            vec!["win-a".to_owned(), "win-f".to_owned()],
            "fixed client tiles after the clear"
        );
    }
}

// Retained-float unfloat exact-refusal shapes: every reachable
// PartialObservation emit on the explicit-tile route after convergence,
// each row mutating exactly one fact so the earliest guard is proved.
// Unreachable on this route (not tested): the tiled-to-float missing
// target lookup (that leg never runs for a retained float), the admit
// subject lookup shadowed by completeness set equality, the normal
// admission overlay gate shadowed by the subject binding check, and the
// remove-leg completeness/known-match guards (unfloat fresh-admits,
// never removes).
#[test]
fn retained_float_unfloat_exact_refusals() {
    fn setup() -> Session {
        let mut session = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![domain(), dom_pair("out-1", "ws-2")],
        )
        .expect("session");
        session.set_fixed_size_admission(true);
        admit_plain(&mut session, "win-a", "c-rf-a");
        let base = session.accepted_revision();
        session
            .converge_observation(
                &observation(base, vec![plain("win-a"), fixed("win-f")]),
                None,
            )
            .expect("admit fixed automatic");
        assert!(session.is_automatic_fixed_float(&WindowId("win-f".to_owned())));
        session
    }
    fn complete(session: &Session) -> Vec<ObservedWindow> {
        let mut windows: Vec<ObservedWindow> = session
            .snapshot()
            .windows
            .iter()
            .map(|l| plain(&l.window.0))
            .collect();
        windows.extend(session.exception_observed());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        windows
    }
    #[derive(Clone, Copy)]
    enum RefusalCase {
        MissingTarget,
        StaleFloating,
        StaleHoming,
        MaximizedTarget,
        MissingPeer,
        ExtraPeer,
        StalePeerFlags,
        StalePeerHoming,
        StaleSubjectSticky,
    }
    impl RefusalCase {
        fn site(self) -> &'static str {
            match self {
                Self::MissingTarget => "propose_toggle_float missing-target",
                Self::StaleFloating => "propose_toggle_float stale-floating-binding",
                Self::StaleHoming => "propose_toggle_float stale-homing-binding",
                Self::MaximizedTarget => "propose_toggle_float maximized-target",
                Self::MissingPeer => "propose_admit completeness missing-peer",
                Self::ExtraPeer => "propose_admit completeness extra-peer",
                Self::StalePeerFlags => "propose_admit known-match stale-peer-flags",
                Self::StalePeerHoming => "propose_admit known-match stale-peer-homing",
                Self::StaleSubjectSticky => "propose_admit subject-binding stale-subject-sticky",
            }
        }
        fn mutate(self, windows: &mut Vec<ObservedWindow>) {
            match self {
                Self::MissingTarget => windows.retain(|e| e.window.0 != "win-f"),
                Self::MissingPeer => windows.retain(|e| e.window.0 != "win-a"),
                Self::ExtraPeer => windows.push(plain("win-x")),
                Self::StaleFloating => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.floating = false;
                        }
                    }
                }
                Self::StaleHoming => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.workspace = WorkspaceId("ws-2".to_owned());
                        }
                    }
                }
                Self::MaximizedTarget => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.maximized = true;
                        }
                    }
                }
                Self::StalePeerFlags => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-a" {
                            e.floating = true;
                        }
                    }
                }
                Self::StalePeerHoming => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-a" {
                            e.workspace = WorkspaceId("ws-2".to_owned());
                        }
                    }
                }
                Self::StaleSubjectSticky => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.sticky = true;
                        }
                    }
                }
            }
        }
    }
    // Each case names its earliest guard; the mutation proves it by
    // changing exactly one fact of the complete observation.
    let rows = [
        RefusalCase::MissingTarget,
        RefusalCase::StaleFloating,
        RefusalCase::StaleHoming,
        RefusalCase::MaximizedTarget,
        RefusalCase::MissingPeer,
        RefusalCase::ExtraPeer,
        RefusalCase::StalePeerFlags,
        RefusalCase::StalePeerHoming,
        RefusalCase::StaleSubjectSticky,
    ];
    for (i, case) in rows.iter().enumerate() {
        let site = case.site();
        let mut session = setup();
        let base = session.accepted_revision();
        let mut windows = complete(&session);
        case.mutate(&mut windows);
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        let err = session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-f".to_owned()),
                    float_geometry: None,
                },
                &observation(base, windows),
                &correlation(&format!("c-rf-{i}")),
                &LifecycleCapabilities::full(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            ProposeError::Refused(RefusalKind::PartialObservation),
            "{site} refuses exact"
        );
        assert!(!session.has_pending(), "{site} stages no plan");
        assert!(!session.has_pending_desired(), "{site} stages no desired");
        let id = WindowId("win-f".to_owned());
        assert!(session.is_exception(&id), "{site} keeps float");
        assert!(
            session.is_automatic_fixed_float(&id),
            "{site} keeps automatic marker"
        );
        assert!(
            !session.has_fixed_tile_override(&id),
            "{site} records no override"
        );
        assert_eq!(
            tiled_ids(&session),
            vec!["win-a".to_owned()],
            "{site} mutates no topology"
        );
        // Restored complete facts accept in the same session: the refusal
        // staged nothing, so the genuine toggle still proposes.
        let base = session.accepted_revision();
        session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-f".to_owned()),
                    float_geometry: None,
                },
                &observation(base, complete(&session)),
                &correlation(&format!("c-rf-restore-{i}")),
                &LifecycleCapabilities::full(),
            )
            .unwrap_or_else(|e| panic!("{site} restored facts accept: {e:?}"));
    }
    // Corrected full facts accept, directly and after two converging
    // re-observations.
    for (tag, resyncs) in [("direct", 0), ("resync", 2)] {
        let mut session = setup();
        for _ in 0..resyncs {
            let base = session.accepted_revision();
            session
                .converge_observation(&observation(base, complete(&session)), None)
                .expect("resync converges");
        }
        let base = session.accepted_revision();
        session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: WindowId("win-f".to_owned()),
                    float_geometry: None,
                },
                &observation(base, complete(&session)),
                &correlation(&format!("c-rf-ok-{tag}")),
                &LifecycleCapabilities::full(),
            )
            .unwrap_or_else(|e| panic!("{tag} complete facts accept: {e:?}"));
    }
}
