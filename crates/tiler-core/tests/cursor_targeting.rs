//! Cursor-first targeting regressions (stage 2c): core-resolved workspace
//! selection and fresh-window placement over cursor/focused facts.
//!
//! Covers cursor/focus disagreement in both directions under all three
//! placements, MRU versus cursor-nearest sibling anchors, quiet
//! bootstrap/restart, settings changes without rehoming, closes and
//! reused lifetimes, missing/outside cursor fallback, every switch verb
//! plus sends under disagreement, and retained-settings observability.
//! Adapter-affinity facts deliberately disagree with the policy target
//! throughout: core resolution must win, never the carried output.

use tiler_core::connection::{HostCapabilities, RawSettingValue};
use tiler_core::cursor_targeting::{FreshPlacement, SwitchTarget};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{ScreenReport, SendSource, WindowFact, WorldCommand, WorldReport};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: tiler_core::world_report::OutputIdentity::unknown(),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

/// Initial wire settings mirroring the original per-report values: the old
/// screens carried gap 0 / outer 0, mode was per-output-local, and every
/// report carried its own switch target / fresh placement with
/// focus-follows and warp off.
fn initial_values(
    switch_target: SwitchTarget,
    fresh_placement: FreshPlacement,
) -> Vec<(String, RawSettingValue)> {
    vec![
        (
            "mode".to_owned(),
            RawSettingValue::String("per-output-local".to_owned()),
        ),
        ("gap".to_owned(), RawSettingValue::Number(0.0)),
        ("outer".to_owned(), RawSettingValue::Number(0.0)),
        (
            "switchTarget".to_owned(),
            RawSettingValue::String(switch_target.as_wire_str().to_owned()),
        ),
        (
            "freshPlacement".to_owned(),
            RawSettingValue::String(fresh_placement.as_wire_str().to_owned()),
        ),
    ]
}

fn engine_for(switch_target: SwitchTarget, fresh_placement: FreshPlacement) -> WorldEngine {
    engine_for_with_capabilities(
        switch_target,
        fresh_placement,
        HostCapabilities {
            // Fixtures carry native sticky facts; the lane under test is
            // the native route.
            native_sticky: true,
            ..HostCapabilities::default()
        },
    )
}

fn engine_for_with_capabilities(
    switch_target: SwitchTarget,
    fresh_placement: FreshPlacement,
    capabilities: HostCapabilities,
) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        capabilities,
        initial_values(switch_target, fresh_placement),
    )
    .expect("generation")
}

/// Adapter rebind through the real handshake path: a fresh handshake for
/// the new incarnation plus revision-0 settings carrying the same initial
/// values, then reports resume with the rebound generation.
fn rebind(
    engine: &mut WorldEngine,
    adapter: &str,
    switch_target: SwitchTarget,
    fresh_placement: FreshPlacement,
) {
    use tiler_core::connection::{ApplicationIdentity, HandshakeRequest, SettingsRequest};
    let handshake = engine
        .handshake(&HandshakeRequest {
            adapter: adapter.to_owned(),
            request: "reconnect-1".to_owned(),
            application_identity: ApplicationIdentity {
                app_id: None,
                executable: None,
                process_id: None,
            },
            capabilities: HostCapabilities {
                // Fixtures carry native sticky facts; only native identity
                // is unavailable on this rebind lane.
                native_sticky: true,
                ..HostCapabilities::without_native_identity()
            },
        })
        .generation
        .expect("rebound generation");
    let settings = engine
        .handle_settings(&SettingsRequest {
            adapter: adapter.to_owned(),
            generation: handshake.clone(),
            request: "settings-1".to_owned(),
            revision: 0,
            values: initial_values(switch_target, fresh_placement),
        })
        .expect("rebind settings");
    assert_eq!(
        settings.status,
        tiler_core::connection::SettingsStatus::Accepted
    );
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(tiler_core::world_report::WindowType::Normal),
        activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

/// Identity-stripped fact for reports after a no-native-identity rebind:
/// the rebound handshake declares `Executable`/`ProcessId` unavailable, so
/// carried identities must be null (a carried value holds the row as a
/// `ConflictingFact` instead of homing it). Pre-rebind reports keep the
/// known foreign pid the supported handshake requires.
fn anon(fact: WindowFact) -> WindowFact {
    WindowFact {
        executable: None,
        process_id: None,
        ..fact
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: two_screens(),
        windows,
        views: vec![],
        closed: vec![],
        native: vec![],
        cursor,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: Some(OutputId("out-a".to_owned())),
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),

        intent_records: Vec::new(),
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn homed(reply: &tiler_core::world_report::WorldReply, window: &str) -> (String, String) {
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("{window} present"));
    (row.output.0.clone(), row.workspace.clone())
}

fn allocation(reply: &tiler_core::world_report::WorldReply, window: &str) -> Rect {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("{window} present"))
        .allocation
        .expect("tiled allocation")
}

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

/// Seeds A on out-a (focused) plus B1/B2 side by side on out-b with B2
/// last-focused, then focuses A. Returns the engine ready for a fresh
/// newcomer under cursor/focus disagreement.
fn seeded_disagreement(fresh_placement: FreshPlacement) -> WorldEngine {
    let mut engine = engine_for(SwitchTarget::Cursor, fresh_placement);
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-b1", "out-b", 800)],
            "win-b1",
            None,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-b1", "out-b", 800), fact("win-b2", "out-b", 1200)],
            "win-b2",
            None,
            None,
            None,
        ),
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            None,
            None,
            None,
        ),
    );
    assert_eq!(homed(&third, "win-a").0, "out-a");
    assert_eq!(homed(&third, "win-b1").0, "out-b");
    assert_eq!(homed(&third, "win-b2").0, "out-b");
    engine
}

#[test]
fn cosmic_routes_cursor_output_with_mru_anchor() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    // Cursor over B1's half, focus on A: COSMIC homes the newcomer to
    // the cursor output but splits B2, the output's last-focused
    // window, not the cursor-nearest B1. Adapter affinity (out-a) loses.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-c").0, "out-b");
    assert_eq!(homed(&fourth, "win-a").0, "out-a");
    // B2's column split vertically: C takes B2's bottom half.
    assert_eq!(allocation(&fourth, "win-c"), rect(1200, 280, 400, 280));
}

#[test]
fn hypr_same_output_focus_anchors_despite_cursor_on_neighbor() {
    // Same-output under Hyprland: the focused window anchors even with
    // the cursor over its neighbor (Tier 1 same-output selected-focus
    // holds in all four; the pinned active-window branch covers the
    // target workspace itself).
    let mut engine = engine_for(
        SwitchTarget::Cursor,
        FreshPlacement::CursorOutputCursorClosest,
    );
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            "win-a",
            None,
            None,
            None,
        ),
    );
    // Cursor over B's right half; C homes to out-a and splits A.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 400),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((600, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&third, "win-c").0, "out-a");
    assert_eq!(allocation(&third, "win-c"), rect(0, 280, 400, 280));
}

#[test]
fn restored_intent_lanes_keep_native_affinity() {
    // A sticky / floating / automatic-fixed lifetime first seen after
    // bootstrap is restored intent, not a tiled candidate: it keeps
    // native affinity instead of routing by policy.
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    let mut sticky = fact("win-s", "out-b", 900);
    sticky.sticky = Some(true);
    let mut floated = fact("win-f", "out-b", 900);
    floated.transient = Some(true);
    let mut automatic = fact("win-g", "out-b", 900);
    automatic.hints = Some(WindowSizeHints {
        min_w: Some(40),
        min_h: Some(90),
        max_w: Some(40),
        max_h: Some(90),
    });
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                sticky,
                floated,
                automatic,
            ],
            "win-a",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-s").0, "out-b");
    assert_eq!(homed(&fourth, "win-f").0, "out-b");
    assert_eq!(homed(&fourth, "win-g").0, "out-b");
}

#[test]
fn rebind_adopts_native_affinity_for_unknown_lifetimes() {
    // A new adapter incarnation re-adopts natively: unknown lifetimes
    // keep screen affinity on the first post-rebind report while
    // retained members stay homed.
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    rebind(
        &mut engine,
        "adapter-2",
        SwitchTarget::Cursor,
        FreshPlacement::CursorOutputMru,
    );
    let mut rebound = report(
        4,
        vec![
            anon(fact("win-b1", "out-b", 800)),
            anon(fact("win-b2", "out-b", 1200)),
            anon(fact("win-a", "out-a", 0)),
            anon(fact("win-c", "out-b", 900)),
        ],
        "win-a",
        Some((100, 300)),
        None,
        None,
    );
    rebound.adapter_incarnation = "adapter-2".to_owned();
    rebound.core_generation = engine.core_generation().to_owned();
    let first = accepted(&mut engine, &rebound);
    assert_eq!(homed(&first, "win-c").0, "out-b");
    assert_eq!(homed(&first, "win-a").0, "out-a");
    assert_eq!(homed(&first, "win-b1").0, "out-b");
}

#[test]
fn floating_scope_still_supports_later_fresh_placement() {
    // A stored floating output view owns no Engine session, yet a later
    // tiled lifetime still routes by fresh-placement policy.
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    let base = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            None,
            None,
            None,
        ),
    );
    let view_b = view_of(&base, "out-b").expect("view b");
    accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            None,
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-b".to_owned()),
                workspace: view_b,
                tiled: false,
            }),
        ),
    );
    let sixth = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&sixth, "win-c").0, "out-a");
}

#[test]
fn hypr_routes_cursor_output_with_cursor_closest_anchor() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputCursorClosest);
    // Same disagreement, cursor over B1: Hyprland splits B1, the
    // cursor-closest node, while MRU would split B2.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((900, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-c").0, "out-b");
    assert_eq!(allocation(&fourth, "win-c"), rect(800, 280, 400, 280));
}

#[test]
fn i3_sway_ignores_cursor_for_output_and_anchor() {
    let mut engine = seeded_disagreement(FreshPlacement::FocusedOutputMru);
    // Cursor on out-b, focus on A: i3/sway admit into the focused
    // output, splitting its last-focused window A.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-c").0, "out-a");
    // A filled out-a alone, wide: horizontal split, C takes the right half.
    assert_eq!(allocation(&fourth, "win-c"), rect(400, 0, 400, 560));
}

#[test]
fn cursor_output_wins_against_adapter_affinity_in_both_directions() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    // Cursor on out-a, newcomer natively on out-b: cursor output wins.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-d", "out-b", 900),
            ],
            "win-b2",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-d").0, "out-a");
}

#[test]
fn missing_and_outside_cursor_fall_back_to_focused_then_primary() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    // No cursor: focused output wins over adapter affinity.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-e", "out-b", 900),
            ],
            "win-a",
            None,
            None,
            None,
        ),
    );
    assert_eq!(homed(&fourth, "win-e").0, "out-a");
    // Cursor outside every screen: same focused fallback.
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
                fact("win-e", "out-a", 100),
                fact("win-f", "out-b", 900),
            ],
            "win-a",
            Some((5000, 5000)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fifth, "win-f").0, "out-a");
}

#[test]
fn no_facts_at_all_falls_back_to_primary() {
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], "", None, None, None),
    );
    // No cursor and no focus: the primary output wins over adapter
    // affinity, never an invented focus.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-g", "out-b", 900)],
            "",
            None,
            None,
            None,
        ),
    );
    assert_eq!(homed(&second, "win-g").0, "out-a");
}

#[test]
fn bootstrap_adopts_native_affinity_despite_cursor() {
    // First report on a fresh engine: native affinity wins, the cursor
    // never routes. This is the startup/restart quiet path.
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&first, "win-a").0, "out-a");
}

#[test]
fn restart_initial_report_adopts_native_affinity() {
    // A restarted engine (no retained sessions) adopts native affinity
    // for the whole roster even under cursor/focus disagreement.
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&first, "win-a").0, "out-a");
    assert_eq!(homed(&first, "win-b1").0, "out-b");
    assert_eq!(homed(&first, "win-b2").0, "out-b");
}

#[test]
fn settings_change_moves_no_member() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    let before = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    // Flip placement and move the cursor with an identical roster:
    // membership must read exactly as before. The flip travels through
    // the revisioned settings channel, mirroring the original report's
    // carried values.
    let flipped = engine
        .apply_test_settings(
            1,
            vec![
                (
                    "switchTarget".to_owned(),
                    RawSettingValue::String("focused-output".to_owned()),
                ),
                (
                    "freshPlacement".to_owned(),
                    RawSettingValue::String("cursor-output-cursor-closest".to_owned()),
                ),
            ],
        )
        .expect("settings flip");
    assert_eq!(
        flipped.status,
        tiler_core::connection::SettingsStatus::Accepted
    );
    let after = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-b2",
            Some((100, 300)),
            None,
            None,
        ),
    );
    for window in ["win-a", "win-b1", "win-b2"] {
        assert_eq!(homed(&after, window), homed(&before, window), "{window}");
    }
}

#[test]
fn closed_lifetime_reused_as_fresh_newcomer() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    // Close B2, then re-admit the same id natively on out-a: the reused
    // lifetime routes as a fresh newcomer to the cursor output.
    let mut closed = report(
        4,
        vec![fact("win-b1", "out-b", 800), fact("win-a", "out-a", 0)],
        "win-a",
        Some((1000, 300)),
        None,
        None,
    );
    closed.closed = vec![WindowId("win-b2".to_owned())];
    accepted(&mut engine, &closed);
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-a", "out-a", 0),
                fact("win-b2", "out-a", 100),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&fifth, "win-b2").0, "out-b");
}

#[test]
fn all_switch_verbs_resolve_cursor_output_under_disagreement() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                4,
                vec![
                    fact("win-b1", "out-b", 800),
                    fact("win-b2", "out-b", 1200),
                    fact("win-a", "out-a", 0),
                ],
                "win-a",
                Some((1000, 300)),
                None,
                None,
            ),
        ),
        "out-a",
    )
    .expect("view a");
    // Trailing creation lands on the cursor output, never the focused one.
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(11),
            Some(WorldCommand::SelectTrailing { output: None }),
        ),
    );
    let trailing_b = view_of(&fifth, "out-b").expect("trailing on out-b");
    assert_ne!(
        Some(trailing_b.as_str()),
        view_of(&fifth, "out-a").as_deref()
    );
    assert_eq!(view_of(&fifth, "out-a").as_deref(), Some(view_a.as_str()));
    // Numbered selection on the cursor output's ring.
    let sixth = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: None,
                index: 1,
            }),
        ),
    );
    assert_eq!(view_of(&sixth, "out-a").as_deref(), Some(view_a.as_str()));
    // Relative step on the cursor output's ring.
    let seventh = accepted(
        &mut engine,
        &report(
            7,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(13),
            Some(WorldCommand::SelectRelative {
                output: None,
                delta: 1,
            }),
        ),
    );
    assert_eq!(view_of(&seventh, "out-a").as_deref(), Some(view_a.as_str()));
    assert_ne!(
        view_of(&seventh, "out-b").as_deref(),
        view_of(&sixth, "out-b").as_deref()
    );
    // Previous toggle walks the cursor output's history only.
    let eighth = accepted(
        &mut engine,
        &report(
            8,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(14),
            Some(WorldCommand::SelectPrevious { output: None }),
        ),
    );
    assert_eq!(view_of(&eighth, "out-a").as_deref(), Some(view_a.as_str()));
    assert_eq!(
        view_of(&eighth, "out-b").as_deref(),
        view_of(&sixth, "out-b").as_deref()
    );
}

#[test]
fn focused_switch_target_and_explicit_output_ignore_cursor() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    let base = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    let view_b = view_of(&base, "out-b").expect("view b");
    // Focused switch target acts on out-a despite the out-b cursor. The
    // target travels through the revisioned settings channel, mirroring
    // the original report's carried value.
    let focused = engine
        .apply_test_settings(
            1,
            vec![(
                "switchTarget".to_owned(),
                RawSettingValue::String("focused-output".to_owned()),
            )],
        )
        .expect("focused switch");
    assert_eq!(
        focused.status,
        tiler_core::connection::SettingsStatus::Accepted
    );
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(11),
            Some(WorldCommand::SelectRelative {
                output: None,
                delta: 1,
            }),
        ),
    );
    assert_eq!(view_of(&fifth, "out-b").as_deref(), Some(view_b.as_str()));
    // A genuinely semantic explicit output passes through untouched.
    // (The live trailing empty selected above is reused, so the out-a
    // view reads the same trailing id while out-b stays put.)
    let sixth = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-b1", "out-b", 800),
                fact("win-b2", "out-b", 1200),
                fact("win-a", "out-a", 0),
            ],
            "win-a",
            Some((1000, 300)),
            Some(12),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
        ),
    );
    assert_eq!(view_of(&sixth, "out-b").as_deref(), Some(view_b.as_str()));
    assert_eq!(
        view_of(&sixth, "out-a").as_deref(),
        view_of(&fifth, "out-a").as_deref()
    );
}

#[test]
fn sends_resolve_on_focused_member_output_despite_cursor() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    // Cursor on out-b throughout: sends still resolve on each mover's own
    // (focused-claim) output. Explicit send contracts are unchanged: the
    // adapter claim must name the mover's output.
    for (seq, window, expect, command) in [
        (
            4u64,
            "win-a",
            "out-a",
            WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 2,
                follow: false,
            },
        ),
        (
            5u64,
            "win-b1",
            "out-b",
            WorldCommand::SendRelative {
                window: WindowId("win-b1".to_owned()),
                source: SendSource::Explicit(OutputId("out-b".to_owned())),
                delta: 1,
                follow: false,
            },
        ),
    ] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                vec![
                    fact("win-b1", "out-b", 800),
                    fact("win-b2", "out-b", 1200),
                    fact("win-a", "out-a", 0),
                ],
                window,
                Some((1000, 300)),
                Some(10 + seq),
                Some(command),
            ),
        );
        assert!(
            reply.transfers.iter().any(|t| t.target_output.0 == expect),
            "homed-output send for {window}: {reply:?}",
        );
        assert!(
            reply
                .transfers
                .iter()
                .filter(|t| t.operation_id == format!("op-{}", 10 + seq))
                .all(|t| t.target_output.0 == expect),
            "send resolves on mover output for {window}: {reply:?}",
        );
    }
}

#[test]
fn retained_settings_are_observable() {
    // Cursor warp needs the warp capability; the rest rides the handshake.
    let mut engine = engine_for_with_capabilities(
        SwitchTarget::Cursor,
        FreshPlacement::CursorOutputMru,
        HostCapabilities {
            warp: true,
            // Fixtures carry native sticky facts; the lane under test is
            // the native route.
            native_sticky: true,
            ..HostCapabilities::default()
        },
    );
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(engine.switch_target(), SwitchTarget::Cursor);
    assert_eq!(engine.fresh_placement(), FreshPlacement::CursorOutputMru);
    assert!(!engine.focus_follows_cursor());
    assert!(!engine.cursor_warp());
    assert_eq!(engine.last_cursor(), Some((100, 300)));
    // The flipped settings arrive through the revisioned settings channel,
    // mirroring the original second report's carried values.
    let flipped = engine
        .apply_test_settings(
            1,
            vec![
                (
                    "switchTarget".to_owned(),
                    RawSettingValue::String("focused-output".to_owned()),
                ),
                (
                    "freshPlacement".to_owned(),
                    RawSettingValue::String("focused-output-last-focused".to_owned()),
                ),
                ("focusFollowsCursor".to_owned(), RawSettingValue::Bool(true)),
                ("cursorWarp".to_owned(), RawSettingValue::Bool(true)),
            ],
        )
        .expect("settings flip");
    assert_eq!(
        flipped.status,
        tiler_core::connection::SettingsStatus::Accepted
    );
    let warped = report(
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        None,
        None,
        None,
    );
    accepted(&mut engine, &warped);
    assert_eq!(engine.switch_target(), SwitchTarget::Focused);
    assert_eq!(engine.fresh_placement(), FreshPlacement::FocusedOutputMru);
    assert!(engine.focus_follows_cursor());
    assert!(engine.cursor_warp());
    assert_eq!(engine.last_cursor(), None);
}

#[test]
fn out_of_bounds_cursor_refuses_at_validation() {
    use tiler_core::world_report::{report_digest, validate_report, validate_report_reason};
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    let bad = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "",
        Some((20_000, 0)),
        None,
        None,
    );
    assert!(!validate_report(&bad));
    assert_eq!(validate_report_reason(&bad), Some("cursor-invalid"));
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected snapshot-invalid, got {other:?}"),
    }
    // The digest seats the cursor: a carried cursor never aliases the
    // cursor-absent digest, so a cursor move never replays as a duplicate.
    let with_cursor = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "",
        Some((100, 300)),
        None,
        None,
    );
    let plain = report(1, vec![fact("win-a", "out-a", 0)], "", None, None, None);
    assert_ne!(report_digest(&with_cursor), report_digest(&plain));
}

#[test]
fn empty_seed_routes_first_newcomer_by_policy_both_directions() {
    // Empty complete seed bootstraps the phase; the first newcomer still
    // routes by cursor policy in both disagreement directions with a
    // one-shot focus intent.
    for (native, cursor, expect) in [
        ("out-a", Some((1000, 300)), "out-b"),
        ("out-b", Some((100, 300)), "out-a"),
    ] {
        let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
        let seed = accepted(&mut engine, &report(1, vec![], "", None, None, None));
        assert!(seed.effects.is_empty(), "empty seed quiet");
        let arrival = accepted(
            &mut engine,
            &report(
                2,
                vec![fact("win-n", native, 0)],
                "win-n",
                cursor,
                None,
                None,
            ),
        );
        assert_eq!(homed(&arrival, "win-n").0, expect, "native {native}");
        assert!(
            arrival.effects.iter().any(|e| e.target.0 == "win-n"),
            "newcomer focuses: {:?}",
            arrival.effects
        );
    }
}

#[test]
fn close_all_then_new_lifetime_routes_by_policy() {
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            None,
        ),
    );
    let mut closed = report(2, vec![], "", None, None, None);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let empty = accepted(&mut engine, &closed);
    assert!(empty.effects.is_empty(), "close-all quiet");
    let arrival = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-n", "out-b", 900)],
            "win-n",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&arrival, "win-n").0, "out-a");
    assert!(
        arrival.effects.iter().any(|e| e.target.0 == "win-n"),
        "reopened lifetime focuses: {:?}",
        arrival.effects
    );
}

#[test]
fn incomplete_gap_then_complete_newcomer_routes() {
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            None,
        ),
    );
    let mut gap = report(
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        None,
        None,
        None,
    );
    gap.inventory_complete = false;
    accepted(&mut engine, &gap);
    let arrival = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-n", "out-b", 900)],
            "win-a",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&arrival, "win-n").0, "out-a");
    assert!(
        arrival.effects.iter().any(|e| e.target.0 == "win-n"),
        "post-gap newcomer focuses: {:?}",
        arrival.effects
    );
}

#[test]
fn initial_incomplete_then_first_complete_stays_native_then_later_routes() {
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    let mut partial = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some((1000, 300)),
        None,
        None,
    );
    partial.inventory_complete = false;
    accepted(&mut engine, &partial);
    // First complete is the startup baseline even though win-b was unseen
    // during the gap: both stay native and quiet.
    let baseline = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((1000, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&baseline, "win-a").0, "out-a");
    assert_eq!(homed(&baseline, "win-b").0, "out-b");
    assert!(baseline.effects.is_empty(), "baseline quiet");
    let arrival = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&arrival, "win-c").0, "out-a");
    assert!(
        arrival.effects.iter().any(|e| e.target.0 == "win-c"),
        "later newcomer focuses: {:?}",
        arrival.effects
    );
}

#[test]
fn floating_newcomer_keeps_native_then_tiled_routes() {
    let mut engine = engine_for(SwitchTarget::Cursor, FreshPlacement::CursorOutputMru);
    accepted(&mut engine, &report(1, vec![], "", None, None, None));
    let mut floated = fact("win-f", "out-b", 900);
    floated.transient = Some(true);
    let first = accepted(
        &mut engine,
        &report(2, vec![floated], "win-f", Some((100, 300)), None, None),
    );
    assert_eq!(homed(&first, "win-f").0, "out-b");
    let arrival = accepted(
        &mut engine,
        &report(
            3,
            vec![
                {
                    let mut f = fact("win-f", "out-b", 900);
                    f.transient = Some(true);
                    f
                },
                fact("win-t", "out-b", 950),
            ],
            "win-f",
            Some((100, 300)),
            None,
            None,
        ),
    );
    assert_eq!(homed(&arrival, "win-t").0, "out-a");
}

#[test]
fn rebind_baseline_quiet_then_later_newcomer_routes() {
    let mut engine = seeded_disagreement(FreshPlacement::CursorOutputMru);
    rebind(
        &mut engine,
        "adapter-2",
        SwitchTarget::Cursor,
        FreshPlacement::CursorOutputMru,
    );
    let mut rebound = report(
        4,
        vec![
            anon(fact("win-b1", "out-b", 800)),
            anon(fact("win-b2", "out-b", 1200)),
            anon(fact("win-a", "out-a", 0)),
            anon(fact("win-c", "out-b", 900)),
        ],
        "win-a",
        Some((100, 300)),
        None,
        None,
    );
    rebound.adapter_incarnation = "adapter-2".to_owned();
    rebound.core_generation = engine.core_generation().to_owned();
    let first = accepted(&mut engine, &rebound);
    assert_eq!(homed(&first, "win-c").0, "out-b");
    assert!(first.effects.is_empty(), "rebind baseline quiet");
    assert_eq!(homed(&first, "win-a").0, "out-a");
    let mut later = report(
        5,
        vec![
            anon(fact("win-b1", "out-b", 800)),
            anon(fact("win-b2", "out-b", 1200)),
            anon(fact("win-a", "out-a", 0)),
            anon(fact("win-c", "out-b", 900)),
            anon(fact("win-d", "out-b", 950)),
        ],
        "win-a",
        Some((100, 300)),
        None,
        None,
    );
    later.adapter_incarnation = "adapter-2".to_owned();
    later.core_generation = engine.core_generation().to_owned();
    let arrival = accepted(&mut engine, &later);
    assert_eq!(homed(&arrival, "win-d").0, "out-a");
    assert!(
        arrival.effects.iter().any(|e| e.target.0 == "win-d"),
        "post-rebind newcomer focuses: {:?}",
        arrival.effects
    );
}
