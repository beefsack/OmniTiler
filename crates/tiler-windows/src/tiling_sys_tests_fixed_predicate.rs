use super::{fixed_predicate_for, rmax03_adapter_tests::test_state};
use crate::settings::{FIXED_SIZE_EITHER_AXIS, LiveSettings, Settings};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

#[test]
fn proof_owners_without_live_settings_keep_both_axes() {
    assert_eq!(fixed_predicate_for(None), FixedSizePredicate::BothAxes);
}

#[test]
fn live_setting_selects_subsequent_admission_predicate() {
    let live = LiveSettings::fresh(Settings::default(), None);
    assert_eq!(
        fixed_predicate_for(Some(&live)),
        FixedSizePredicate::BothAxes
    );
    let mut either = Settings::default();
    either.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
    let live = LiveSettings::fresh(either, None);
    assert_eq!(
        fixed_predicate_for(Some(&live)),
        FixedSizePredicate::EitherAxis
    );
}

#[test]
fn fixed_birth_probe_fails_closed_without_effects() {
    // Null handles never query: fail-closed false with no native effect.
    // (A live maximized row always carries a real HWND; this pins the
    // guard, not the query path.)
    let mut cx = super::HintCx::new();
    assert!(!super::fixed_birth_for_slotless(
        0,
        FixedSizePredicate::BothAxes,
        &mut cx
    ));
    assert!(!super::fixed_birth_for_slotless(
        0,
        FixedSizePredicate::EitherAxis,
        &mut cx
    ));
}

#[test]
fn stamp_applies_live_fullscreen_origin_and_override() {
    use crate::workspace::WindowKey;
    use std::collections::HashMap;
    let mut state = test_state();
    let key = WindowKey {
        hwnd: 4242,
        pid: 4243,
        creation: "c000000000000001".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "wfix".to_owned());
    state.overlay_now.insert("wfix".to_owned(), (true, true));
    state.fixed_auto_seen.insert(key.clone());
    let other = WindowKey {
        hwnd: 4342,
        pid: 4343,
        creation: "c000000000000002".to_owned(),
    };
    state
        .member_tokens
        .insert(other.clone(), "wover".to_owned());
    state.fixed_tile_overrides.insert(other.clone());
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("stamp-1").expect("correlation"),
        0,
        3,
        &crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8).0,
        &crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8).1,
        8,
        &[
            (
                WindowId("wfix".to_owned()),
                bounds,
                WindowSizeHints::none(),
                true,
            ),
            (
                WindowId("wover".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("wplain".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("wfix".to_owned())),
    );
    super::stamp_engine_event(&state, &mut event);
    let flagged = |id: &str| {
        event
            .windows
            .iter()
            .find(|w| w.window.0 == id)
            .expect("row")
            .clone()
    };
    let wfix = flagged("wfix");
    assert!(wfix.fullscreen, "live fullscreen snapshot stamps");
    assert!(wfix.fixed_auto, "D4 latch stamps the origin");
    assert!(!wfix.fixed_suppress, "no override without a win");
    let wover = flagged("wover");
    assert!(!wover.fullscreen, "no snapshot without observation");
    assert!(!wover.fixed_auto);
    assert!(wover.fixed_suppress, "override set stamps the win");
    let wplain = flagged("wplain");
    assert!(!wplain.fullscreen && !wplain.fixed_auto && !wplain.fixed_suppress);
    assert!(
        !wplain.sticky && !wplain.maximized,
        "adapter lanes stay false"
    );
    let _ = HashMap::<String, String>::new();
}

#[test]
fn apply_predicate_only_returns_false_adopts_and_reclassifies_nothing() {
    use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
    use tiler_core::boundary::{CoreCommand, CoreReply};

    // Scratch home for the ledger lock and the owner log line: never the
    // repo or the live product directory. No window, hook, suspend, or
    // registry effect exists on this path; the temp dir is removed.
    let tmp = std::env::temp_dir().join(format!(
        "tiler-fixed-predicate-live-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = tmp.join("owner.log");
    state.engine.set_fixed_size_admission(true);
    let base = tile_options_from_settings(&Settings::default());
    state.border = base.border;
    state.underlay = base.underlay;
    state.keyboard = crate::snapkey::KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    state.last_remap = build_remap(&Settings::default());
    state.last_disabled = build_disabled(&Settings::default());
    state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
    let mut snap_want = true;
    // One-axis-fixed window tiles under the default predicate.
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1600,
        h: 900,
    };
    let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let one_axis = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(960),
    };
    let seed_fp =
        crate::tiling::fingerprint(&[("w1".to_owned(), bounds), ("wone".to_owned(), bounds)]);
    let seed = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("live-seed").expect("correlation"),
        0,
        seed_fp,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
            (WindowId("wone".to_owned()), bounds, one_axis, false),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    let reply = state.engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {reply:?}"
    );
    assert!(
        !state
            .engine
            .session(&domain.1)
            .expect("session")
            .is_automatic_fixed_float(&WindowId("wone".to_owned())),
        "one-axis-fixed tiles under the default"
    );
    let before = state.engine.session(&domain.1).expect("session").snapshot();
    let me = crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    };
    let mut incoming = Settings::default();
    incoming.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
    incoming.revision = 7;
    let changed = super::apply_live_settings(
        &mut state,
        &tmp,
        &me,
        &store,
        &mut snap_want,
        incoming,
        None,
    );
    assert!(!changed, "predicate-only adoption reports no owner change");
    assert_eq!(
        state.engine.session(&domain.1).expect("session").snapshot(),
        before,
        "retained session untouched: no rebuild, no resync"
    );
    assert!(
        !state
            .engine
            .session(&domain.1)
            .expect("session")
            .is_automatic_fixed_float(&WindowId("wone".to_owned())),
        "retained tile never reclassifies on reload"
    );
    assert_eq!(
        fixed_predicate_for(state.settings_live.as_ref()),
        FixedSizePredicate::EitherAxis,
        "subsequent admissions use the adopted predicate"
    );
    assert_eq!(
        state.engine.fixed_size_predicate(),
        FixedSizePredicate::EitherAxis
    );
    // A subsequent identical admission floats under either-axis.
    let rows_fp = crate::tiling::fingerprint(&[
        ("w1".to_owned(), bounds),
        ("wone".to_owned(), bounds),
        ("wtwo".to_owned(), bounds),
    ]);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("live-next").expect("correlation"),
        state
            .engine
            .session(&domain.1)
            .map(|session| session.accepted_revision())
            .unwrap_or(0),
        rows_fp,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
            (WindowId("wone".to_owned()), bounds, one_axis, false),
            (WindowId("wtwo".to_owned()), bounds, one_axis, false),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    event.command = CoreCommand::Reconcile;
    let reply = super::handle_engine_event(&mut state, &mut event);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "next admission converges, got {reply:?}"
    );
    assert!(
        !state
            .engine
            .session(&domain.1)
            .expect("session")
            .is_automatic_fixed_float(&WindowId("wone".to_owned())),
        "old tile stays tiled"
    );
    assert!(
        state
            .engine
            .session(&domain.1)
            .expect("session")
            .is_automatic_fixed_float(&WindowId("wtwo".to_owned())),
        "later admission floats under either-axis"
    );
    assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
    drop(store);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn release_resets_latch_and_hints_but_preserves_override_intent_sticky() {
    // P1 D6: a confirmed domain release resets the D4 automatic latch
    // and drops last-known hints for that domain's candidates (fresh
    // classification at enable), while explicit overrides, intentional
    // floats, sticky members, and other domains are preserved.
    use crate::workspace::WindowKey;
    let mut state = super::rmax03_adapter_tests::test_state();
    let key = |hwnd: u64| WindowKey {
        hwnd,
        pid: 5000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    };
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let active = state.workspaces.active_id(output).expect("active");
    let (latched, plain) = (key(11), key(12));
    let durable = key(13);
    for k in [&latched, &plain, &durable] {
        assert!(state.workspaces.assign(k.clone(), output, &active, false));
        state
            .member_tokens
            .insert(k.clone(), format!("w{}", k.hwnd));
    }
    state.fixed_auto_seen.insert(latched.clone());
    state.fixed_tile_overrides.insert(plain.clone());
    state.fixed_tile_overrides.insert(durable.clone());
    state.fixed_origin_overrides.insert(durable.clone());
    state.floated.insert(plain.clone());
    state.sticky.insert(plain.clone(), false);
    state.hint_logged.insert(
        "w11".to_owned(),
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        },
    );
    // Another domain's latch must survive a foreign release.
    let other = key(99);
    state.workspaces.ensure_output("mon-b");
    let active_b = state.workspaces.active_id("mon-b").expect("active");
    assert!(
        state
            .workspaces
            .assign(other.clone(), "mon-b", &active_b, false)
    );
    state.member_tokens.insert(other.clone(), "w99".to_owned());
    state.fixed_auto_seen.insert(other.clone());
    super::reset_fixed_admission_for_domain(&mut state, output, &active);
    assert!(
        !state.fixed_auto_seen.contains(&latched),
        "released domain latch resets"
    );
    assert!(
        !state.hint_logged.contains_key("w11"),
        "released domain hints drop for fresh classification"
    );
    assert!(
        state.fixed_tile_overrides.contains(&plain),
        "explicit override preserved"
    );
    assert!(
        state.fixed_origin_overrides.contains(&durable),
        "durable fixed origin preserved for enable re-adoption"
    );
    assert!(state.floated.contains(&plain), "intent preserved");
    assert!(state.sticky.contains_key(&plain), "sticky preserved");
    assert!(
        state.fixed_auto_seen.contains(&other),
        "foreign domain latch untouched"
    );
}

#[test]
fn stamp_never_asserts_automatic_origin_for_sticky() {
    // P5 sticky-dominance: even with both the D4 latch and the override
    // set present, a sticky member stamps no automatic origin (core
    // never marks automatic for a sticky observation). Suppress still
    // rides when the override set holds it.
    use crate::workspace::WindowKey;
    let mut state = super::rmax03_adapter_tests::test_state();
    let key = WindowKey {
        hwnd: 4242,
        pid: 4243,
        creation: "c000000000000001".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "wfix".to_owned());
    state.fixed_auto_seen.insert(key.clone());
    state.fixed_tile_overrides.insert(key.clone());
    state.sticky.insert(key.clone(), false);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("stamp-sticky").expect("correlation"),
        0,
        3,
        &domain.0,
        &domain.1,
        8,
        &[(
            WindowId("wfix".to_owned()),
            bounds,
            WindowSizeHints::none(),
            true,
        )],
        Some(&WindowId("wfix".to_owned())),
    );
    super::stamp_engine_event(&state, &mut event);
    let window = event
        .windows
        .iter()
        .find(|w| w.window.0 == "wfix")
        .expect("row");
    assert!(
        !window.fixed_auto,
        "sticky dominates: no automatic origin signal"
    );
    assert!(
        !window.sticky && !window.maximized,
        "adapter lanes stay false"
    );
    assert!(window.fixed_suppress, "override win still rides");
    // Without the sticky entry the same latch stamps the origin.
    state.sticky.remove(&key);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("stamp-sticky-2").expect("correlation"),
        0,
        3,
        &domain.0,
        &domain.1,
        8,
        &[(
            WindowId("wfix".to_owned()),
            bounds,
            WindowSizeHints::none(),
            true,
        )],
        Some(&WindowId("wfix".to_owned())),
    );
    super::stamp_engine_event(&state, &mut event);
    let window = event
        .windows
        .iter()
        .find(|w| w.window.0 == "wfix")
        .expect("row");
    assert!(window.fixed_auto, "unstuck latch stamps the D4 origin");
}

#[test]
fn stamp_refuses_automatic_origin_for_durable_override() {
    // D3/D7 dominance: even with a stale D4 latch beside it, a durable
    // fixed-origin override never stamps automatic origin; the explicit
    // win rides suppress only.
    use crate::workspace::WindowKey;
    let mut state = super::rmax03_adapter_tests::test_state();
    let key = WindowKey {
        hwnd: 4342,
        pid: 4343,
        creation: "c000000000000003".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "wdur".to_owned());
    state.fixed_auto_seen.insert(key.clone());
    state.fixed_tile_overrides.insert(key.clone());
    state.fixed_origin_overrides.insert(key.clone());
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("stamp-durable").expect("correlation"),
        0,
        3,
        &domain.0,
        &domain.1,
        8,
        &[(
            WindowId("wdur".to_owned()),
            bounds,
            WindowSizeHints::none(),
            false,
        )],
        Some(&WindowId("wdur".to_owned())),
    );
    super::stamp_engine_event(&state, &mut event);
    let window = event
        .windows
        .iter()
        .find(|w| w.window.0 == "wdur")
        .expect("row");
    assert!(
        !window.fixed_auto,
        "durable override never acquires automatic origin"
    );
    assert!(window.fixed_suppress, "explicit win still rides");
}

#[test]
fn production_handle_plans_no_writes_for_latched_max_birth() {
    // D4/D8 through the production stamped handle (not a manual core
    // event): a latched fixed max-birth admitted slotless rides floating
    // with the adapter origin, the Engine marks it automatic, and the
    // committed plan carries no geometry, focus, or stacking effect for
    // it while the ordinary tile is placed.
    use crate::workspace::WindowKey;
    use tiler_core::boundary::{CoreCommand, CoreReply};
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let key = WindowKey {
        hwnd: 4242,
        pid: 4243,
        creation: "c000000000000001".to_owned(),
    };
    state.member_tokens.insert(key.clone(), "wfix".to_owned());
    state.fixed_auto_seen.insert(key.clone());
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let fixed = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("latch-1").expect("correlation"),
        0,
        9,
        &domain.0,
        &domain.1,
        8,
        &[
            (WindowId("wfix".to_owned()), bounds, fixed, true),
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    event.command = CoreCommand::Reconcile;
    let reply = super::handle_engine_event(&mut state, &mut event);
    let plan = match &reply {
        CoreReply::Tiled(plan) => plan.clone(),
        CoreReply::Projection(_) => {
            // Single-domain seed shape: converge through one more
            // observation carrying the same stamped rows.
            let mut retry = event.clone();
            retry.revision = state
                .engine
                .session(&domain.1)
                .map(|session| session.accepted_revision())
                .unwrap_or(0);
            match super::handle_engine_event(&mut state, &mut retry) {
                CoreReply::Tiled(plan) => plan,
                other => panic!("latched birth converges, got {other:?}"),
            }
        }
        other => panic!("latched birth converges, got {other:?}"),
    };
    assert!(
        plan.geometry.iter().all(|g| g.window.0 != "wfix"),
        "no geometry write planned for the automatic float"
    );
    assert!(
        plan.geometry.iter().any(|g| g.window.0 == "w1"),
        "ordinary tile still placed"
    );
    assert!(
        state
            .engine
            .session(&domain.1)
            .expect("session")
            .is_automatic_fixed_float(&WindowId("wfix".to_owned())),
        "latched origin marks automatic through the production stamp"
    );
    let report = state.engine.last_fixed_admission().expect("bounded report");
    assert_eq!(report.reason, "fixed-equal");
}

/// D5 production-ordering member tables with distinct fake HWNDs per
/// token (deterministic derivation, never real windows): the cfg(test)
/// query seam keys injections by HWND, so members must not share one.
/// No test takes a native write (identity gates fail closed on invalid
/// handles; every real query fails closed with no effect).
fn d5_member(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    token: &str,
) -> crate::workspace::WindowKey {
    let key = crate::workspace::WindowKey {
        hwnd: 1100 + token.bytes().map(u64::from).sum::<u64>(),
        pid: 7000 + state.member_tokens.len() as u32,
        creation: format!("cd5{:016x}", state.member_tokens.len()),
    };
    assert!(state.workspaces.assign(key.clone(), output, ws, false));
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-d5".to_owned(),
            session_id: 1,
            exe_path: "d5.exe".to_owned(),
        },
    );
    state.member_tags.insert(key.clone(), "d5-tag".to_owned());
    state.last_hwnds.insert(key.hwnd);
    key
}

fn d5_domain(
    output: &str,
    ws: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    crate::workspace_owner::workspace_domain(output, ws, bounds, 8)
}

fn d5_retained(
    key: &crate::workspace::WindowKey,
    token: &str,
    maximized: bool,
    fullscreen: bool,
) -> super::RetainedRow {
    super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }),
        maximized,
        fullscreen,
        facts: None,
    }
}

fn d5_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

/// Visible eligible observation for one member with null HWND: every
/// native query fails closed with no effect, so the row exercises the
/// visible lane with unknown fresh hints.
fn d5_observed(token: &str, key: &crate::workspace::WindowKey) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "d5.exe".to_owned(),
            user_sid: "S-1-5-d5".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

/// Inject converted production hints for one fake member HWND (clearing
/// any previous injection first, so legs never leak into each other).
/// The hints must come from [`crate::tiling::hints_from_outer`], never
/// hand-built, so the conversion path stays under test.
fn d5_inject(
    hwnd: u64,
    min_outer: Option<(i32, i32)>,
    max_outer: Option<(i32, i32)>,
) -> WindowSizeHints {
    super::clear_test_hints();
    let hints = crate::tiling::hints_from_outer(
        min_outer,
        max_outer,
        crate::tiling::FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8,
        },
    );
    super::inject_test_hints(hwnd, hints);
    hints
}

/// Drive one born hold through the production stamped handle: the
/// stamped fullscreen row floats with no automatic mark.
fn d5_hold(
    state: &mut super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    token: &str,
    focus: &str,
) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    state.overlay_now.insert(token.to_owned(), (false, true));
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("d5-hold").expect("correlation"),
        0,
        41,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId(token.to_owned()),
                bounds,
                WindowSizeHints::none(),
                true,
            ),
            (
                WindowId(focus.to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId(focus.to_owned())),
    );
    event.command = CoreCommand::Reconcile;
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "born hold converges, got {reply:?}"
    );
}

fn d5_is_exception(
    state: &super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    token: &str,
) -> bool {
    state
        .engine
        .session(&domain.1)
        .is_some_and(|session| session.is_exception(&WindowId(token.to_owned())))
}

fn d5_is_automatic(
    state: &super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    token: &str,
) -> bool {
    state
        .engine
        .session(&domain.1)
        .is_some_and(|session| session.is_automatic_fixed_float(&WindowId(token.to_owned())))
}

fn d5_is_tiled(
    state: &super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    token: &str,
) -> bool {
    state.engine.session(&domain.1).is_some_and(|session| {
        session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == token)
            && !session.is_exception(&WindowId(token.to_owned()))
    })
}

/// Assemble one domain through production assembly and commit it
/// through the production stamped handle. Returns the committed rows
/// plus the Engine reply (for no-write planning assertions).
#[allow(clippy::too_many_arguments)]
fn d5_assemble_and_commit(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    observed: &[super::ObservedWindow],
    retained: &[super::RetainedRow],
    focus: &str,
    correlation: &str,
) -> Option<(
    Vec<crate::workspace_owner::OwnerRow>,
    tiler_core::boundary::CoreReply,
)> {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    // Mirror the per-tick observe refresh: overlay snapshots are always
    // current at stamp time in production, so tests refresh them from
    // the same inputs here instead of carrying stale hold-time flags
    // (a stale fullscreen stamp would pin the hold and hide the exit).
    for window in observed {
        state.overlay_now.insert(
            window.token.clone(),
            (window.facts.maximized, window.facts.captionless_fullscreen),
        );
    }
    for row in retained {
        state
            .overlay_now
            .insert(row.token.clone(), (row.maximized, row.fullscreen));
    }
    let mut hint_cx = super::HintCx::new();
    let rows = super::assemble_domain_rows(
        state,
        output,
        ws,
        observed,
        retained,
        "d5-test",
        correlation,
        &mut hint_cx,
    )?;
    let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|row| {
            (
                WindowId(row.token.clone()),
                row.rect,
                row.hints,
                row.floating,
            )
        })
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|row| (row.token.clone(), row.rect))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        super::revision_for(state, output, ws),
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focus.to_owned())),
    );
    event.command = CoreCommand::Reconcile;
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "d5 commit converges, got {reply:?}"
    );
    Some((rows, reply))
}

/// One real `ToggleFloat` commit through the production stamped handle:
/// the same converge path the float/unfloat dispatchers commit, with the
/// adapter's live origin/override stamps. Returns the Engine reply.
fn d5_toggle_float(
    state: &mut super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    rows: &[(WindowId, Rect, WindowSizeHints, bool)],
    focus: &str,
    window: &str,
    float_rect: Option<Rect>,
    correlation: &str,
) -> tiler_core::boundary::CoreReply {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(id, rect, _, _)| (id.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse(correlation).expect("correlation"),
        state
            .engine
            .session(&domain.1)
            .map(|session| session.accepted_revision())
            .unwrap_or(0),
        fp,
        &domain.0,
        &domain.1,
        8,
        rows,
        Some(&WindowId(focus.to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect,
    };
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        !matches!(
            reply,
            CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
        ),
        "d5 toggle converges, got {reply:?}"
    );
    reply
}

fn d5_has_core_override(
    state: &super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    token: &str,
) -> bool {
    state
        .engine
        .session(&domain.1)
        .is_some_and(|session| session.has_fixed_tile_override(&WindowId(token.to_owned())))
}

#[test]
fn maximized_fixed_first_exit_defers_clear_then_floats_automatic() {
    // D5 maximized leg through production assembly: the exit tick
    // defers the clear (mark peeked, never consumed) and classifies
    // from transported hints; the committed automatic mark consumes the
    // mark and skips every later clear. No hintless-exception stall.
    use tiler_core::boundary::CoreReply;
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    assert!(d5_is_exception(&state, &domain, "wfix"));
    assert!(!d5_is_automatic(&state, &domain, "wfix"));
    // The hold observed fullscreen at admission: mirror the born lane.
    state.born_fullscreen.insert(key.clone());
    // Track releases the born hold into a first exit without consuming
    // it (no commit evidence yet).
    let retained = vec![d5_retained(&key, "wfix", true, false)];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    assert!(
        !state.born_fullscreen.contains(&key),
        "exit releases the hold"
    );
    assert!(
        state.born_first_exit.contains(&key),
        "exit inserts the mark"
    );
    // Clear defers by peek: the mark survives for assembly.
    super::clear_maximize_at_admission(&mut state, &d5_me(), &retained);
    assert!(
        state.born_first_exit.contains(&key),
        "clear peeks, never consumes pre-classification"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "no attempt recorded for unclassified rows"
    );
    // Converted fixed hints through the test query seam: the live
    // probe call site transports them, proving the conversion path
    // (outer 656x496 minus 8px frames = visible 640x480 both sides).
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((656, 496)));
    assert_eq!(
        (
            converted.min_w,
            converted.min_h,
            converted.max_w,
            converted.max_h
        ),
        (Some(640), Some(480), Some(640), Some(480))
    );
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, reply) = d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "first exit rides the tiled lane");
    assert_eq!(
        row.hints, converted,
        "classification rides transported hints"
    );
    if let CoreReply::Tiled(plan) = &reply {
        assert!(
            plan.geometry.iter().all(|g| g.window.0 != "wfix"),
            "no native target writes while classifying"
        );
    }
    assert!(
        d5_is_automatic(&state, &domain, "wfix"),
        "fixed exit floats"
    );
    // Commit evidence consumes the mark; the next clear skips via the
    // Engine automatic mark instead of firing.
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(
        !state.born_first_exit.contains(&key),
        "committed classification consumes the mark"
    );
    super::clear_maximize_at_admission(&mut state, &d5_me(), &retained);
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "automatic floats never clear"
    );
}

#[test]
fn stale_logged_fixed_with_failed_fresh_tiles() {
    // Anti-fallback regression (D5 normative): last-known fixed bounds
    // never stand in for a failed fresh probe. With stale fixed history
    // present but the fresh query failing, the exit rides tiled
    // hintless and tiles admission-only; the commit still consumes the
    // mark. Later passes use the normal one-shot gate instead of the
    // first-exit deferral.
    use tiler_core::size_hints::WindowSizeHints as Hints;
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![d5_retained(&key, "wfix", true, false)];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    super::clear_maximize_at_admission(&mut state, &d5_me(), &retained);
    assert!(state.born_first_exit.contains(&key));
    state.hint_logged.insert(
        "wfix".to_owned(),
        Hints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        },
    );
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "stale evidence never floats: tiled lane");
    assert!(row.hints.is_empty(), "failed fresh means no hints ride");
    assert!(
        d5_is_tiled(&state, &domain, "wfix"),
        "stale fixed still tiles"
    );
    assert!(!d5_is_automatic(&state, &domain, "wfix"));
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(
        !state.born_first_exit.contains(&key),
        "tiling commit consumes the mark"
    );
}

#[test]
fn normal_visible_exit_unknown_tiles_and_consumes() {
    // D5 normal leg through production assembly: a visible exit whose
    // fresh query fails rides tiled hintless (failure-as-unknown) and
    // tiles admission-only; the commit consumes the mark (one shot,
    // no linger). A later intentional float settles through a real
    // ToggleFloat plus the runtime lane and is never undone: no mark
    // remains to force the tiled lane.
    use tiler_core::boundary::{CoreCommand, CoreReply};
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    // Visible non-fullscreen exit observation (null HWND: queries fail
    // closed with no native effect, so no evidence transports).
    let observed = vec![d5_observed("wfix", &key), d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(
        state.born_first_exit.contains(&key),
        "exit inserts the mark"
    );
    let (rows, reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-exit",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "first exit rides the tiled lane");
    assert!(row.hints.is_empty(), "unknown query flows as no hints");
    if let CoreReply::Tiled(plan) = &reply {
        assert!(
            plan.geometry.iter().any(|g| g.window.0 == "wfix"),
            "tiled exit takes a tile slot"
        );
    }
    assert!(d5_is_tiled(&state, &domain, "wfix"), "unknown exit tiles");
    assert!(!d5_is_automatic(&state, &domain, "wfix"));
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(
        !state.born_first_exit.contains(&key),
        "tiling commit consumes the mark: one shot only"
    );
    // A later intentional float settles through a real ToggleFloat plus
    // the runtime lane (mirroring the float commit path) and is never
    // undone: no mark remains to force the tiled lane.
    let revision = super::revision_for(&state, output, &ws);
    let mut float_event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("d5-float-commit").expect("correlation"),
        revision,
        77,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("wfix".to_owned()),
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w1".to_owned()),
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("wfix".to_owned())),
    );
    float_event.command = CoreCommand::ToggleFloat {
        window: "wfix".to_owned(),
        float_rect: None,
    };
    let reply = state.engine.handle(&float_event);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "explicit float commits, got {reply:?}"
    );
    assert!(d5_is_exception(&state, &domain, "wfix"));
    state.floated.insert(key.clone());
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "wfix",
        "d5-float",
    )
    .expect("float rides");
    assert!(
        rows.iter()
            .find(|row| row.token == "wfix")
            .expect("row")
            .floating,
        "later intent keeps the float lane: no stale-mark undo"
    );
}

#[test]
fn first_exit_unknown_tiles_and_predicate_switch_applies() {
    // D5 current-hints/current-predicate legs through production
    // assembly: a failed fresh probe flows as no hints with
    // floating=false and tiles admission-only (failure-as-unknown);
    // a predicate switch mid-hold selects the exit layer from the
    // fresh output at exit.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![d5_retained(&key, "wfix", true, false)];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    // Failed fresh probe with no injection: unknown flows as no hints
    // with floating=false and tiles admission-only.
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "unknown exits ride the tiled lane");
    assert!(row.hints.is_empty(), "unknown flows as no hints");
    assert!(d5_is_tiled(&state, &domain, "wfix"), "unknown tiles");
    assert!(!d5_is_automatic(&state, &domain, "wfix"));
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(
        !state.born_first_exit.contains(&key),
        "tiling commit consumes the mark"
    );
    // Predicate switch mid-hold with converted partial-zero output:
    // the exit floats under either-axis through its nonzero axis.
    // The injected converted hints stand in for the live probe at the
    // real call site (outer 656x0 minus 8px frames = visible 640x0:
    // explicit-zero outers ride unset per axis; a remainder swallowed
    // by its frame would read unknown instead).
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    state
        .engine
        .set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![d5_retained(&key, "wfix", true, false)];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    let converted = d5_inject(key.hwnd, Some((656, 0)), Some((656, 0)));
    assert_eq!(
        (
            converted.min_w,
            converted.min_h,
            converted.max_w,
            converted.max_h
        ),
        (Some(640), Some(0), Some(640), Some(0))
    );
    let observed = vec![d5_observed("w1", &w1key)];
    d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit",
    )
    .expect("exit classifies");
    assert!(
        d5_is_automatic(&state, &domain, "wfix"),
        "partial-zero exit floats under the current either-axis predicate"
    );
}

#[test]
fn repeated_fullscreen_toggles_keep_ordinary_identity() {
    // D2 through production assembly: after a tiled exit, a later
    // fullscreen transition is a managed overlay (never a born hold
    // again) and the next exit retires tiled with no marks left.
    // Exits carry unknown fresh hints and tile admission-only.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let mut retained = vec![d5_retained(&key, "wfix", true, false)];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    let observed = vec![d5_observed("w1", &w1key)];
    d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit",
    )
    .expect("exit classifies");
    assert!(d5_is_tiled(&state, &domain, "wfix"));
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(!state.born_first_exit.contains(&key));
    // Later fullscreen transition on the lifetime-known window: a
    // managed overlay, never a born hold (seen_nonfullscreen pins it).
    retained[0].fullscreen = true;
    retained[0].maximized = false;
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(
        !state.born_fullscreen.contains(&key),
        "lifetime-known fullscreen never re-holds"
    );
    assert!(
        !state.born_first_exit.contains(&key),
        "no exit mark without a hold"
    );
    let (rows, _) = d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-full",
    )
    .expect("overlay rides");
    assert!(
        rows.iter().any(|row| row.token == "wfix"),
        "overlay member rides"
    );
    assert!(
        d5_is_tiled(&state, &domain, "wfix"),
        "overlay keeps the slot"
    );
    // Exit again: ordinary, mark-free, still tiled.
    retained[0].fullscreen = false;
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(!state.born_first_exit.contains(&key));
    d5_assemble_and_commit(
        &mut state, output, &ws, &domain, &observed, &retained, "w1", "d5-exit2",
    )
    .expect("second exit classifies");
    assert!(d5_is_tiled(&state, &domain, "wfix"));
    assert!(!state.born_first_exit.contains(&key));
    assert!(!state.born_fullscreen.contains(&key));
}

#[test]
fn hidden_fixed_first_exit_rides_tiled_and_floats_automatic() {
    // D5 hidden leg through production assembly: a hidden first exit
    // with converted fixed hints rides tiled (floating=false) instead
    // of the hidden float lane, and floats automatic; the commit
    // consumes the mark. Sticky and intentional-float lanes keep their
    // float rows. A hidden exit with unknown hints rides tiled
    // hintless and tiles.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.member_rects.insert(
        "wfix".to_owned(),
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
    );
    state.workspaces.set_hidden(&key, true);
    // Lingering mark from an exit whose commit never landed (e.g. a
    // refused tick before the hide): the hidden lane still classifies.
    // Production prunes hidden tokens from the overlay snapshot every
    // tick, so hidden rows stamp no fullscreen flag: mirror that here
    // instead of carrying the hold-time snapshot.
    state.born_first_exit.insert(key.clone());
    state.overlay_now.remove("wfix");
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((656, 496)));
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-hide-exit",
    )
    .expect("hidden exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "hidden first exit rides tiled");
    assert_eq!(row.hints, converted);
    assert!(d5_is_automatic(&state, &domain, "wfix"));
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(
        !state.born_first_exit.contains(&key),
        "hidden commit consumes the mark"
    );
    // Sticky hidden exits keep the float lane.
    state.born_first_exit.insert(key.clone());
    state.sticky.insert(key.clone(), false);
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-hide-sticky",
    )
    .expect("sticky hidden rides");
    assert!(
        rows.iter()
            .find(|row| row.token == "wfix")
            .expect("row")
            .floating,
        "explicit sticky intent preserved while hidden"
    );
}

#[test]
fn first_exit_without_rect_or_evidence_defers_with_mark_intact() {
    // No-rect edge: a frameless first-exit row with no evidence and no
    // snapshots pushes no view, so assembly defers the whole domain
    // with retained state instead of classifying blind. The mark
    // survives for the next pass; nothing is consumed or reset.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![super::RetainedRow {
        key: key.clone(),
        token: "wfix".to_owned(),
        rect: None,
        maximized: true,
        fullscreen: false,
        facts: None,
    }];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    assert!(state.born_first_exit.contains(&key));
    let observed = vec![d5_observed("w1", &w1key)];
    let mut hint_cx = super::HintCx::new();
    let rows = super::assemble_domain_rows(
        &mut state,
        output,
        &ws,
        &observed,
        &retained,
        "d5-test",
        "d5-norect",
        &mut hint_cx,
    );
    assert!(rows.is_none(), "no view means defer, never a blind row");
    assert!(
        state.born_first_exit.contains(&key),
        "deferral consumes and resets nothing"
    );
    assert!(d5_is_exception(&state, &domain, "wfix"), "hold kept");
}

#[test]
fn fixed_birth_probes_share_one_preamble_budget() {
    // Item-13 aggregate budget: sequential fixed-birth probes share the
    // caller's HintCx (the preamble creates exactly one; the admit loop
    // never resets per row), so many fresh maximized windows cannot
    // restore the timer. Null probes fail closed without consuming a
    // meaningful slice, and the shared deadline only ever advances.
    let mut cx = super::HintCx::new();
    let first = cx.timeout_ms();
    for _ in 0..8 {
        assert!(!super::fixed_birth_for_slotless(
            0,
            FixedSizePredicate::BothAxes,
            &mut cx
        ));
        assert!(!super::fixed_birth_for_slotless(
            0,
            FixedSizePredicate::EitherAxis,
            &mut cx
        ));
    }
    assert!(cx.timeout_ms() <= first, "shared deadline never resets");
}

fn clear_retained(key: &crate::workspace::WindowKey, token: &str) -> super::RetainedRow {
    super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }),
        maximized: true,
        fullscreen: false,
        facts: Some(crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: true,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }),
    }
}

fn clear_assemble(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    observed: &[super::ObservedWindow],
    retained: &[super::RetainedRow],
) -> Vec<crate::workspace_owner::OwnerRow> {
    let mut hint_cx = super::HintCx::new();
    super::assemble_domain_rows(
        state,
        output,
        ws,
        observed,
        retained,
        "clear-test",
        "clear",
        &mut hint_cx,
    )
    .expect("rows assemble")
}

#[test]
fn slotless_nonfixed_max_plans_clear_and_defers_slot() {
    // Core regression (tile-on-launch): a non-fixed born-max plans CLEAR
    // dispositionally, and assembly must not plant a slot that disables
    // the required clear on the next pass.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    // Converted non-fixed hints through the test query seam (outer
    // 656x496 minus 8px frames = 640x480 min; 1936x1096 = 1920x1080 max).
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((1936, 1096)));
    assert!(!converted.is_empty());
    let retained = vec![clear_retained(&key, "wmax")];
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "non-fixed slotless max plans CLEAR"
    );
    let observed = vec![d5_observed("w1", &w1key)];
    let rows = clear_assemble(&mut state, output, &ws, &observed, &retained);
    let row = rows.iter().find(|row| row.token == "wmax").expect("row");
    assert!(!row.floating, "pending clear rides the tiled lane");
    assert!(
        !state.member_rects.contains_key("wmax"),
        "assembly defers the slot so the next clear still sees slotless"
    );
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "still CLEAR after assembly: no slot was planted"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "peeking records no attempt and invents no hold"
    );
    super::clear_test_hints();
}

#[test]
fn committed_ordinary_pin_survives_later_fixed_hints() {
    // D2 admission-only: a nonfixed commit tiles ordinary; when current
    // hints turn fixed before the required clear lands, the Engine
    // admission pin (not the fresh predicate) still plans CLEAR.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_inject(key.hwnd, Some((656, 496)), Some((1936, 1096)));
    let retained = vec![clear_retained(&key, "wmax")];
    let observed = vec![d5_observed("w1", &w1key)];
    let (_, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &retained,
        "w1",
        "clear-pin-commit",
    )
    .expect("nonfixed classifies");
    assert!(
        d5_is_tiled(&state, &domain, "wmax"),
        "ordinary commit tiles"
    );
    assert!(
        !state.member_rects.contains_key("wmax"),
        "commit plants no slot ahead of the required clear"
    );
    // Current hints turn fixed with no re-commit: retained admission is
    // pinned ordinary regardless.
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((656, 496)));
    assert_eq!(
        (
            converted.min_w,
            converted.min_h,
            converted.max_w,
            converted.max_h
        ),
        (Some(640), Some(480), Some(640), Some(480))
    );
    assert!(d5_is_tiled(&state, &domain, "wmax"), "no reclassification");
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "pinned ordinary admission still plans CLEAR despite fixed hints"
    );
    let rows = clear_assemble(&mut state, output, &ws, &observed, &retained);
    assert!(rows.iter().any(|row| row.token == "wmax"));
    assert!(
        !state.member_rects.contains_key("wmax"),
        "pin mirror still defers the slot"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "peeking records no attempt"
    );
    super::clear_test_hints();
}

#[test]
fn slotless_fixed_max_skips_clear_under_both_born_modes() {
    // D4 wins over core.born_maximized under both values: fixed floats,
    // never clears. Disposition asserted directly, plus the committed
    // Engine automatic mark on the default lane.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((656, 496)));
    assert_eq!(
        (
            converted.min_w,
            converted.min_h,
            converted.max_w,
            converted.max_h
        ),
        (Some(640), Some(480), Some(640), Some(480))
    );
    let retained = vec![clear_retained(&key, "wmax")];
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "fixed skips by default"
    );
    state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "fixed skips under explicit tile-on-launch"
    );
    let mut overlay = Settings::default();
    overlay.core.born_maximized = crate::settings::BORN_MAXIMIZED_OVERLAY.to_owned();
    state.settings_live = Some(LiveSettings::fresh(overlay, None));
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "fixed skips under keep-overlay too"
    );
    state.settings_live = None;
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &retained,
        "w1",
        "clear-fixed",
    )
    .expect("fixed classifies");
    assert!(rows.iter().any(|row| row.token == "wmax"));
    assert!(
        d5_is_automatic(&state, &domain, "wmax"),
        "fixed floats automatic"
    );
    super::clear_test_hints();
}

#[test]
fn first_exit_max_defers_clear_then_fixed_floats_unknown_pends() {
    // D5 maximized legs: the exit tick defers the clear (mark peeked);
    // fixed then floats automatic while unknown tiles and pends the
    // normal one-shot clear with no slot planted ahead of it.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![clear_retained(&key, "wfix")];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    assert!(state.born_first_exit.contains(&key));
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "first exit defers the clear pre-classification"
    );
    let converted = d5_inject(key.hwnd, Some((656, 496)), Some((656, 496)));
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &retained,
        "w1",
        "d5-max-fixed",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "first exit rides the tiled lane");
    assert_eq!(row.hints, converted, "classification rides current hints");
    assert!(
        d5_is_automatic(&state, &domain, "wfix"),
        "fixed exit floats"
    );
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(!state.born_first_exit.contains(&key));
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "committed automatic never clears"
    );
    super::clear_test_hints();

    // Unknown leg: failed fresh probe tiles hintless, then pends clear.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wfix");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wfix", "w1");
    state.born_fullscreen.insert(key.clone());
    let retained = vec![clear_retained(&key, "wfix")];
    super::track_fullscreen_holds(&mut state, &[], &retained);
    super::clear_test_hints();
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "unknown exit still defers while marked"
    );
    let observed = vec![d5_observed("w1", &w1key)];
    let (rows, _) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &retained,
        "w1",
        "d5-max-unknown",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wfix").expect("row");
    assert!(!row.floating, "unknown exits ride the tiled lane");
    assert!(row.hints.is_empty(), "unknown flows as no hints");
    assert!(d5_is_tiled(&state, &domain, "wfix"), "unknown tiles");
    super::track_fullscreen_holds(&mut state, &observed, &retained);
    assert!(!state.born_first_exit.contains(&key));
    assert!(
        !state.member_rects.contains_key("wfix"),
        "no slot planted ahead of the pending clear"
    );
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "tiled unknown exit pends the normal one-shot clear"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "no attempt recorded before the native gate"
    );
}

#[test]
fn startup_fullscreen_exit_tiles_like_new() {
    // Leg (a): already fullscreen at owner startup then exit admits like
    // newly admitted window through production assembly. Fresh state
    // carries no lifetime-known non-fullscreen identity, so the hold is
    // allowed; the visible exit rides tiled hintless and tiles.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wnew");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wnew", "w1");
    assert!(
        !state.seen_nonfullscreen.contains(&key),
        "startup carries no lifetime-known non-fullscreen identity"
    );
    assert!(
        crate::tiling::should_hold_born_fullscreen(true, false, false),
        "slotless first-seen fullscreen holds"
    );
    state.born_fullscreen.insert(key.clone());
    let observed = vec![d5_observed("wnew", &key), d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(
        state.born_first_exit.contains(&key),
        "exit inserts the mark"
    );
    let (rows, reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-startup-exit",
    )
    .expect("exit classifies");
    let row = rows.iter().find(|row| row.token == "wnew").expect("row");
    assert!(!row.floating, "startup exit rides the tiled lane");
    assert!(row.hints.is_empty(), "unknown flows as no hints");
    let geometry = match &reply {
        tiler_core::boundary::CoreReply::Tiled(plan) => &plan.geometry,
        tiler_core::boundary::CoreReply::Projection(plan) => &plan.geometry,
        other => panic!("startup exit commits tiled/projection, got {other:?}"),
    };
    assert!(
        geometry.iter().any(|g| g.window.0 == "wnew"),
        "tiled exit takes a tile slot"
    );
    assert!(d5_is_tiled(&state, &domain, "wnew"), "startup exit tiles");
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(
        !state.born_first_exit.contains(&key),
        "tiling commit consumes the mark"
    );
}

#[test]
fn born_after_startup_exit_tiles_like_new() {
    // Leg (b): born fullscreen after owner startup then exit admits like
    // newly admitted window. Existing members are lifetime-known
    // non-fullscreen, but the new fullscreen key is not, so the hold is
    // allowed; the exit tiles through production assembly.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wborn");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wborn", "w1");
    // Existing member observed non-fullscreen first: lifetime-known.
    let live = vec![d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &live, &[]);
    assert!(
        state.seen_nonfullscreen.contains(&w1key),
        "existing member is lifetime-known"
    );
    assert!(
        !state.seen_nonfullscreen.contains(&key),
        "new fullscreen key is not lifetime-known"
    );
    assert!(
        crate::tiling::should_hold_born_fullscreen(true, false, false),
        "new slotless fullscreen holds"
    );
    state.born_fullscreen.insert(key.clone());
    let observed = vec![d5_observed("wborn", &key), d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(state.born_first_exit.contains(&key));
    let (rows, reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-born-exit",
    )
    .expect("exit classifies");
    let geometry = match &reply {
        tiler_core::boundary::CoreReply::Tiled(plan) => &plan.geometry,
        tiler_core::boundary::CoreReply::Projection(plan) => &plan.geometry,
        other => panic!("born exit commits tiled/projection, got {other:?}"),
    };
    assert!(
        geometry.iter().any(|g| g.window.0 == "wborn"),
        "tiled exit takes a tile slot"
    );
    let row = rows.iter().find(|row| row.token == "wborn").expect("row");
    assert!(!row.floating, "born exit rides the tiled lane");
    assert!(d5_is_tiled(&state, &domain, "wborn"), "born exit tiles");
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(!state.born_first_exit.contains(&key));
}

#[test]
fn managed_fullscreen_returns_tile_without_writes() {
    // Leg (c): managed tiled fullscreen/unfullscreen returns its tile.
    // A lifetime-known tiled member transitioning to fullscreen is a
    // managed overlay (never a born hold): while fullscreen the window
    // is absent from the eligible observation and rides retained-only,
    // the reserved slot topology is unchanged, the plan carries no
    // native geometry write for the member, and the next exit returns
    // the same slot mark-free.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wm");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    // Tile both members first so `wm` is lifetime-known ordinary.
    let observed = vec![d5_observed("wm", &key), d5_observed("w1", &w1key)];
    let (tiled_rows, tiled_reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-managed-tile",
    )
    .expect("initial tiles");
    match &tiled_reply {
        tiler_core::boundary::CoreReply::Tiled(_)
        | tiler_core::boundary::CoreReply::Projection(_) => {}
        other => panic!("initial commit converges, got {other:?}"),
    }
    let slot0 = state.member_rects.get("wm").copied().expect("slot");
    let tile_rect0 = tiled_rows
        .iter()
        .find(|row| row.token == "wm")
        .expect("row")
        .rect;
    assert!(d5_is_tiled(&state, &domain, "wm"));
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(state.seen_nonfullscreen.contains(&key));
    // Fullscreen phase: the member leaves the eligible observation and
    // rides retained-only, the possible production shape.
    let observed_full = vec![d5_observed("w1", &w1key)];
    let retained = vec![d5_retained(&key, "wm", false, true)];
    super::track_fullscreen_holds(&mut state, &observed_full, &retained);
    assert!(
        !state.born_fullscreen.contains(&key),
        "lifetime-known fullscreen never re-holds"
    );
    assert!(
        !state.born_first_exit.contains(&key),
        "no exit mark without a hold"
    );
    let (rows, reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed_full,
        &retained,
        "w1",
        "d5-managed-full",
    )
    .expect("overlay rides");
    let geometry = match &reply {
        tiler_core::boundary::CoreReply::Tiled(plan) => &plan.geometry,
        tiler_core::boundary::CoreReply::Projection(plan) => &plan.geometry,
        other => panic!("overlay commits tiled/projection, got {other:?}"),
    };
    let overlay_row = rows.iter().find(|row| row.token == "wm").expect("row");
    assert_eq!(
        overlay_row.rect, slot0,
        "overlay rides the last-known slot rect"
    );
    assert_eq!(
        state.member_rects.get("wm").copied(),
        Some(slot0),
        "fullscreen keeps the reserved slot topology"
    );
    assert!(d5_is_tiled(&state, &domain, "wm"), "overlay keeps the slot");
    assert!(
        geometry.iter().any(|g| g.window.0 == "wm"),
        "Engine plan retains the slot allocation while native writes skip it"
    );
    // The Engine plan still carries the slot allocation; the production
    // write path excludes the retained-only member because its token is
    // absent from the fresh eligible observation (same
    // `writable_subset` the tick calls).
    let members: std::collections::BTreeSet<crate::workspace::WindowKey> =
        [key.clone(), w1key.clone()].into_iter().collect();
    let token_of: std::collections::BTreeMap<crate::workspace::WindowKey, String> = [
        (key.clone(), "wm".to_owned()),
        (w1key.clone(), "w1".to_owned()),
    ]
    .into_iter()
    .collect();
    let fresh: std::collections::HashSet<String> = ["w1".to_owned()].into_iter().collect();
    let writable = crate::workspace_owner::writable_subset(
        &members,
        |_: &crate::workspace::WindowKey| false,
        &token_of,
        &fresh,
    );
    assert!(
        !writable.contains("wm"),
        "retained-only fullscreen member takes no geometry write"
    );
    // Exit again: ordinary, mark-free, same slot.
    let observed_exit = vec![d5_observed("wm", &key), d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &observed_exit, &[]);
    assert!(!state.born_first_exit.contains(&key));
    let (exit_rows, exit_reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed_exit,
        &[],
        "w1",
        "d5-managed-exit",
    )
    .expect("exit classifies");
    match &exit_reply {
        tiler_core::boundary::CoreReply::Tiled(_)
        | tiler_core::boundary::CoreReply::Projection(_) => {}
        other => panic!("exit commit converges, got {other:?}"),
    }
    assert_eq!(
        exit_rows
            .iter()
            .find(|row| row.token == "wm")
            .expect("row")
            .rect,
        tile_rect0,
        "exit returns the same tile rect"
    );
    assert_eq!(
        state.member_rects.get("wm").copied(),
        Some(slot0),
        "exit preserves the slot topology"
    );
    assert!(d5_is_tiled(&state, &domain, "wm"), "exit returns its tile");
    assert!(!state.born_first_exit.contains(&key));
}

#[test]
fn borderless_exit_tiles_like_new() {
    // Leg (e): borderless windowed fullscreen exit admits like newly
    // admitted window. The production borderless predicate derives the
    // hold flag from a captionless full-monitor frame; the exit then
    // tiles through production assembly.
    let full = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    assert!(
        crate::tiling::is_borderless_fullscreen(true, full, &[full]),
        "captionless full-monitor frame is fullscreen"
    );
    assert!(
        !crate::tiling::is_borderless_fullscreen(
            true,
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            &[full]
        ),
        "small borderless window stays eligible"
    );
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wbl");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    d5_hold(&mut state, &domain, "wbl", "w1");
    // Predicate-derived hold flag feeds the production hold gate.
    let hold = crate::tiling::should_hold_born_fullscreen(
        crate::tiling::is_borderless_fullscreen(true, full, &[full]),
        false,
        state.seen_nonfullscreen.contains(&key),
    );
    assert!(hold, "borderless cover holds slotless");
    state.born_fullscreen.insert(key.clone());
    let observed = vec![d5_observed("wbl", &key), d5_observed("w1", &w1key)];
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(state.born_first_exit.contains(&key));
    let (rows, reply) = d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "d5-borderless-exit",
    )
    .expect("exit classifies");
    let geometry = match &reply {
        tiler_core::boundary::CoreReply::Tiled(plan) => &plan.geometry,
        tiler_core::boundary::CoreReply::Projection(plan) => &plan.geometry,
        other => panic!("borderless exit commits tiled/projection, got {other:?}"),
    };
    let row = rows.iter().find(|row| row.token == "wbl").expect("row");
    assert!(!row.floating, "borderless exit rides the tiled lane");
    assert!(
        geometry.iter().any(|g| g.window.0 == "wbl"),
        "tiled exit takes a tile slot"
    );
    assert!(d5_is_tiled(&state, &domain, "wbl"), "borderless exit tiles");
    super::track_fullscreen_holds(&mut state, &observed, &[]);
    assert!(!state.born_first_exit.contains(&key));
}

#[test]
fn released_formerly_fixed_latch_now_plans_clear() {
    // D6: a confirmed release resets the latch and last-known hints, so a
    // formerly fixed maximum with current ordinary hints plans CLEAR.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    state.fixed_auto_seen.insert(key.clone());
    state.hint_logged.insert(
        "wmax".to_owned(),
        WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        },
    );
    let retained = vec![clear_retained(&key, "wmax")];
    let cx = super::HintCx::new();
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &cx),
        "latched fixed never clears"
    );
    super::reset_fixed_admission_for_domain(&mut state, output, &ws);
    assert!(
        !state.fixed_auto_seen.contains(&key),
        "release resets latch"
    );
    assert!(
        !state.hint_logged.contains_key("wmax"),
        "release drops hints for fresh classification"
    );
    d5_inject(key.hwnd, Some((656, 496)), Some((1936, 1096)));
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "formerly latched ordinary maximum now plans CLEAR"
    );
    let observed = vec![d5_observed("w1", &w1key)];
    clear_assemble(&mut state, output, &ws, &observed, &retained);
    assert!(
        !state.member_rects.contains_key("wmax"),
        "assembly defers the slot ahead of the pending clear"
    );
    super::clear_test_hints();
}

#[test]
fn override_max_keeps_clear_precedence() {
    // Explicit tile overrides keep the preexisting one-shot maximize
    // policy: an ordinary maximized override still plans CLEAR.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    state.fixed_tile_overrides.insert(key.clone());
    d5_inject(key.hwnd, Some((656, 496)), Some((1936, 1096)));
    let retained = vec![clear_retained(&key, "wmax")];
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "ordinary override plans CLEAR"
    );
    let observed = vec![d5_observed("w1", &w1key)];
    clear_assemble(&mut state, output, &ws, &observed, &retained);
    assert!(
        !state.member_rects.contains_key("wmax"),
        "no slot planted ahead of the override clear"
    );
    super::clear_test_hints();
}

#[test]
fn unknown_query_failure_plans_clear_without_hold_or_retry() {
    // Persistent query failure is failure-as-unknown: it plans CLEAR
    // (never an indefinite hold) and records nothing pre-gate.
    super::clear_test_hints();
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let key = d5_member(&mut state, output, &ws, "wmax");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    let retained = vec![clear_retained(&key, "wmax")];
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "failed query plans CLEAR failure-as-unknown"
    );
    let observed = vec![d5_observed("w1", &w1key)];
    let rows = clear_assemble(&mut state, output, &ws, &observed, &retained);
    let row = rows.iter().find(|row| row.token == "wmax").expect("row");
    assert!(!row.floating, "unknown rides the tiled lane");
    assert!(row.hints.is_empty(), "no hints invented");
    assert!(
        !state.member_rects.contains_key("wmax"),
        "no slot planted for unknown"
    );
    let cx = super::HintCx::new();
    assert!(
        super::admission_clear_planned(&state, &retained[0], &cx),
        "still pending: no hold, no retry state"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "no attempt recorded pre-gate"
    );
}

#[test]
fn fixed_origin_provenance_needs_known_automatic_origin() {
    // D7 provenance (R-SPC-04): only a known automatic-fixed prior
    // proves fixed origin for a settled tile win. The D4 latch, a
    // committed Engine automatic mark, and a hydrated durable override
    // each prove it; a generic float with none of those reads false and
    // persists no marker. Fake HWNDs keep every native peek fail-closed
    // (absent), so the negative leg exercises the real reader.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let latched = d5_member(&mut state, output, &ws, "wlatched");
    let engine = d5_member(&mut state, output, &ws, "wengine");
    let durable = d5_member(&mut state, output, &ws, "wdurable");
    let generic = d5_member(&mut state, output, &ws, "wgeneric");
    // D4 latch proves origin with no Engine mark yet.
    state.fixed_auto_seen.insert(latched.clone());
    assert!(
        super::fixed_origin_provenance(&state, &latched),
        "D4 latch proves fixed origin"
    );
    // Committed Engine automatic proves origin after the latch
    // withdraws. The latch stamps the origin for the commit, then the
    // win-time capture below models the pre-commit read.
    let converted = d5_inject(engine.hwnd, Some((656, 496)), Some((656, 496)));
    assert!(!converted.is_empty());
    state.fixed_auto_seen.insert(engine.clone());
    // Every domain member must ride the observation or assembly
    // defers with retained state; the uninvolved members ride plain
    // tiled rows here.
    let w1key = d5_member(&mut state, output, &ws, "w1");
    let observed = vec![
        d5_observed("wlatched", &latched),
        d5_observed("wdurable", &durable),
        d5_observed("wgeneric", &generic),
        d5_observed("w1", &w1key),
    ];
    let retained = vec![clear_retained(&engine, "wengine")];
    d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &retained,
        "w1",
        "provenance-commit",
    )
    .expect("automatic commits");
    assert!(
        d5_is_automatic(&state, &domain, "wengine"),
        "automatic committed"
    );
    state.fixed_auto_seen.remove(&engine);
    // Hint loss after the commit: injections cleared and no last-known
    // hints apply to this member, yet the committed mark still proves
    // origin. Provenance consults no hints and no native reads.
    super::clear_test_hints();
    assert!(
        super::fixed_origin_provenance(&state, &engine),
        "committed automatic proves fixed origin without the latch"
    );
    // Hydrated durable proves origin with no other signal.
    state.fixed_origin_overrides.insert(durable.clone());
    assert!(
        super::fixed_origin_provenance(&state, &durable),
        "hydrated override proves fixed origin"
    );
    // Generic float: admitted member with no latch, no mark, no
    // override, and no Engine automatic.
    assert!(
        !super::fixed_origin_provenance(&state, &generic),
        "generic unfloat persists no marker"
    );
    super::clear_test_hints();
}

#[test]
fn generic_unfloat_cycles_record_core_override_without_fixed_provenance() {
    // D7 negative control (R-SPC-04): core records EVERY settled
    // unfloat win including generic non-fixed ones, so the committed
    // core override must never prove fixed origin. Repeated generic
    // float/unfloat/suppress cycles keep the core mark TRUE while
    // adapter provenance stays FALSE with no durable record and no
    // marker path. Real `ToggleFloat` converge commits through the
    // production stamped handle with converted production hints.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = d5_domain(output, &ws);
    let key = d5_member(&mut state, output, &ws, "wgen");
    let w1key = d5_member(&mut state, output, &ws, "w1");
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    // Converted generic hints: 640x480 min with a wider max tiles
    // under both predicates.
    let generic = d5_inject(key.hwnd, Some((656, 496)), Some((1936, 1096)));
    assert!(!generic.is_empty());
    let observed = vec![d5_observed("wgen", &key), d5_observed("w1", &w1key)];
    d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "generic-seed",
    )
    .expect("generic tiles");
    assert!(d5_is_tiled(&state, &domain, "wgen"));
    let toggle_rows = |floating: bool| {
        vec![
            (WindowId("wgen".to_owned()), bounds, generic, floating),
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ]
    };
    for cycle in 1..=2 {
        d5_toggle_float(
            &mut state,
            &domain,
            &toggle_rows(false),
            "wgen",
            "wgen",
            None,
            &format!("generic-float-{cycle}"),
        );
        assert!(
            d5_is_exception(&state, &domain, "wgen"),
            "cycle {cycle}: generic floats"
        );
        d5_toggle_float(
            &mut state,
            &domain,
            &toggle_rows(true),
            "wgen",
            "wgen",
            None,
            &format!("generic-unfloat-{cycle}"),
        );
        assert!(
            d5_is_tiled(&state, &domain, "wgen"),
            "cycle {cycle}: generic re-tiles"
        );
        assert!(
            d5_has_core_override(&state, &domain, "wgen"),
            "cycle {cycle}: core records every unfloat"
        );
        assert!(
            !super::fixed_origin_provenance(&state, &key),
            "cycle {cycle}: core mark never proves fixed origin"
        );
        assert!(
            !state.fixed_origin_overrides.contains(&key),
            "cycle {cycle}: no durable record"
        );
    }
    // The production generic win-site records runtime suppression only:
    // a suppress-stamped re-observation still tiles without provenance
    // and without durable persistence.
    state.fixed_tile_overrides.insert(key.clone());
    d5_assemble_and_commit(
        &mut state,
        output,
        &ws,
        &domain,
        &observed,
        &[],
        "w1",
        "generic-suppress",
    )
    .expect("suppressed generic re-tiles");
    assert!(d5_is_tiled(&state, &domain, "wgen"));
    assert!(
        !super::fixed_origin_provenance(&state, &key),
        "suppressed generic still proves nothing"
    );
    assert!(
        !state.fixed_origin_overrides.contains(&key),
        "suppress alone persists nothing durable"
    );
    super::clear_test_hints();
}

#[test]
fn durable_override_skips_clear_and_reserves_slot_without_auto() {
    // D3/D4/D6/Q3 (R-SPC-04): a durable fixed-origin override keeps its
    // tiled base through the reserved-maximized slot lane under both
    // `core.born_maximized` values, across hint loss, and never
    // acquires automatic origin. Generic runtime-only wins keep the
    // existing clear rules (see override_max_keeps_clear_precedence).
    use crate::settings::{BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, LiveSettings, Settings};
    for born in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
        let mut state = super::rmax03_adapter_tests::test_state();
        state.engine.set_fixed_size_admission(true);
        let mut settings = Settings::default();
        settings.core.born_maximized = born.to_owned();
        state.settings_live = Some(LiveSettings::fresh(settings, None));
        let output = "mon-a";
        state.workspaces.ensure_output(output);
        let ws = state.workspaces.active_id(output).expect("active");
        let domain = d5_domain(output, &ws);
        let key = d5_member(&mut state, output, &ws, "wmax");
        let w1key = d5_member(&mut state, output, &ws, "w1");
        state.fixed_tile_overrides.insert(key.clone());
        state.fixed_origin_overrides.insert(key.clone());
        // Hint loss: no injection, so the fresh probe reads unknown.
        super::clear_test_hints();
        let retained = vec![clear_retained(&key, "wmax")];
        let cx = super::HintCx::new();
        assert!(
            !super::admission_clear_planned(&state, &retained[0], &cx),
            "durable override never plans CLEAR under {born}"
        );
        let observed = vec![d5_observed("w1", &w1key)];
        let rows = clear_assemble(&mut state, output, &ws, &observed, &retained);
        assert!(
            state.member_rects.contains_key("wmax"),
            "durable override reserves the maximized slot under {born}"
        );
        let row = rows.iter().find(|row| row.token == "wmax").expect("row");
        assert!(!row.floating, "override rides the tiled lane under {born}");
        // Commit through the stamped handle: suppress tiles, never auto.
        d5_assemble_and_commit(
            &mut state,
            output,
            &ws,
            &domain,
            &observed,
            &retained,
            "w1",
            "durable-commit",
        )
        .expect("override commits");
        assert!(
            d5_is_tiled(&state, &domain, "wmax"),
            "override tiles under {born}"
        );
        assert!(
            !d5_is_automatic(&state, &domain, "wmax"),
            "override never automatic under {born}"
        );
        // No core-mirror assert here: on a fresh domain the seed route
        // tiles suppressed rows without mirroring the win into the
        // session override set (the toggle-float win path mirrors it).
        // The adapter durable set plus the per-tick suppress stamp are
        // the source of fixed provenance every tick; the generic core
        // override cannot prove automatic fixed origin.
    }
}

#[test]
fn retained_adopt_without_marker_hydrates_nothing() {
    // D7 retained pass fails closed: a retained member row with no
    // marker installs no override, stages no Engine commit, and takes
    // no write. Fake HWNDs keep every native peek fail-closed.
    let mut state = super::rmax03_adapter_tests::test_state();
    state.engine.set_fixed_size_admission(true);
    let output = "mon-a";
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let key = d5_member(&mut state, output, &ws, "wmax");
    let retained = vec![clear_retained(&key, "wmax")];
    let mut hint_cx = super::HintCx::new();
    super::adopt_restart_markers(&mut state, &d5_me(), &[], &retained, &[], &mut hint_cx);
    assert!(
        !state.fixed_tile_overrides.contains(&key),
        "no marker means no runtime override"
    );
    assert!(
        !state.fixed_origin_overrides.contains(&key),
        "no marker means no durable override"
    );
    assert!(
        state.floated.is_empty() && state.sticky.is_empty(),
        "no marker stages no intent lane"
    );
}
