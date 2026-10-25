#![forbid(unsafe_code)]
use tiler_core::geometry::Rect;
use tiler_core::gesture::{GestureIntent, classify_gesture, prefer_modal_move};
use tiler_core::window_policy::{
    MembershipVerdict, SafetyFacts, born_keep_latch, classify_membership, float_toggle_refusal,
    overlay_refusal, should_clear_maximize_at_admission,
    should_clear_maximize_at_admission_with_mode, should_hold_born_fullscreen,
};
use tiler_windows::tiling::{
    CaptureOptions, DirectionalOp, DirectionalRequest, FrameInsets, FullscreenToggle,
    OWN_SETTINGS_WINDOW_CLASS, ObservedTarget, ObservedTargetRef, ResizeOptions, ResizeRepeat,
    ResizeRequest, ScopeHostChild, SendOutputRequest, SkipReason, StatelessVerdict, TokenMap,
    WindowFacts, WorkspaceAction, WorkspaceRequest, allow_match, allowlist_digest,
    canonical_retained_rect, classify, classify_focus, fingerprint, float_topmost_restore_needed,
    fullscreen_toggle_decision, hosted_child_allows, inspect_stateless_verdict,
    is_borderless_fullscreen, is_own_settings_window, min_hints_from_outer, normalize_min_track,
    parse_allowlist, parse_capture_args, parse_children_args, parse_directional_args,
    parse_directional_request, parse_hide_proof_args, parse_inspect_args, parse_resize_args,
    parse_resize_request, parse_scope_host_child, parse_send_output_args,
    parse_send_output_request, parse_shortcut_proof_args, parse_tile_args, parse_tile_proof_args,
    parse_workspace_args, parse_workspace_proof_args, parse_workspace_request,
    render_directional_request, render_resize_request, render_send_output_request,
    render_workspace_request, resize_direction_valid, resize_mode_valid, resize_repeat_next,
    scope_allows, scope_exe_basename, send_flags_stable, tick_summary_signature,
    tiling_domain_bounds, toggle_gate_outcome, verify_directional_argv_consistency,
    verify_hide_proof_argv_consistency, verify_proof_argv_consistency,
    verify_resize_argv_consistency, verify_send_output_argv_consistency,
    verify_shortcut_proof_argv_consistency, verify_workspace_argv_consistency,
    verify_workspace_proof_argv_consistency, visible_min_from_outer, workspace_dialog_refuses,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn eligible_facts() -> WindowFacts {
    WindowFacts {
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
    }
}

/// Only the exact overlay wire token preserves maximize at admission.
fn keep_overlay(value: &str) -> bool {
    tiler_core::born_maximized::BornMaximized::parse_wire(value)
        .is_some_and(|mode| !mode.should_clear_at_admission())
}

#[test]
fn frame_insets_round_trip() {
    let outer = rect(100, 100, 800, 600);
    let visible = rect(108, 108, 784, 584);
    let insets = FrameInsets::measure(outer, visible);
    assert_eq!(
        insets,
        FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8
        }
    );
    assert_eq!(insets.visible_to_outer(visible), Some(outer));
}

#[test]
fn frame_conversion_overflow_fails_closed() {
    let insets = FrameInsets {
        left: 8,
        top: 8,
        right: 8,
        bottom: 8,
    };
    // Width overflows when insets are added.
    assert_eq!(insets.visible_to_outer(rect(0, 0, i32::MAX, 10)), None);
    // Origin underflows when insets are subtracted.
    assert_eq!(insets.visible_to_outer(rect(i32::MIN, 0, 10, 10)), None);
    // Non-positive extents after mapping are refused.
    assert_eq!(
        FrameInsets::default().visible_to_outer(rect(5, 5, 0, 0)),
        None
    );
}

#[test]
fn frame_measure_saturates_dwm_rounding() {
    // Visible slightly larger than outer (rounding): saturates, never panics.
    let insets = FrameInsets::measure(rect(100, 100, 800, 600), rect(99, 99, 802, 602));
    assert_eq!(insets.left, 0);
    assert_eq!(insets.top, 0);
}

#[test]
fn fingerprint_stable_and_order_independent() {
    let a = vec![
        ("w1".to_owned(), rect(0, 0, 100, 100)),
        ("w2".to_owned(), rect(100, 0, 100, 100)),
    ];
    let b = vec![
        ("w2".to_owned(), rect(100, 0, 100, 100)),
        ("w1".to_owned(), rect(0, 0, 100, 100)),
    ];
    assert_eq!(fingerprint(&a), fingerprint(&b));
    let c = vec![("w1".to_owned(), rect(0, 0, 100, 100))];
    assert_ne!(fingerprint(&a), fingerprint(&c));
}

#[test]
fn tokens_stable_per_identity_not_hwnd() {
    let mut map = TokenMap::default();
    let first = map.token_for(1234, "creation-a");
    assert_eq!(map.token_for(1234, "creation-a"), first);
    assert_ne!(map.token_for(5678, "creation-a"), first);
    // Recycled HWND with a new process creation never inherits the token.
    let recycled = map.token_for(1234, "creation-b");
    assert_ne!(recycled, first);
    assert!(!first.contains("1234"), "no raw HWND in token");
    map.retain(&[
        ObservedTargetRef {
            hwnd: 1234,
            creation: "creation-b",
        },
        ObservedTargetRef {
            hwnd: 5678,
            creation: "creation-a",
        },
    ]);
    // The stale (hwnd, creation-a) entry is gone: it re-mints.
    assert_ne!(map.token_for(1234, "creation-a"), first);
}

#[test]
fn eligibility_exclusions() {
    assert!(classify(&eligible_facts()).is_ok());
    let mut facts = eligible_facts();
    facts.minimized = true;
    assert_eq!(classify(&facts), Err(SkipReason::Minimized));
    let mut facts = eligible_facts();
    facts.maximized = true;
    assert_eq!(classify(&facts), Err(SkipReason::Maximized));
    let mut facts = eligible_facts();
    facts.cloaked = true;
    assert_eq!(classify(&facts), Err(SkipReason::Cloaked));
    let mut facts = eligible_facts();
    facts.elevated = true;
    assert_eq!(classify(&facts), Err(SkipReason::Elevated));
    let mut facts = eligible_facts();
    facts.owned = true;
    assert_eq!(classify(&facts), Err(SkipReason::OwnedDialog));
    let mut facts = eligible_facts();
    facts.captionless_fullscreen = true;
    assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
    let mut facts = eligible_facts();
    facts.no_activate = true;
    assert_eq!(classify(&facts), Err(SkipReason::NoActivate));
    let mut facts = eligible_facts();
    facts.dialog = true;
    assert_eq!(classify(&facts), Err(SkipReason::Dialog));
    // Owned generic dialogs keep the owned-dialog reason.
    let mut facts = eligible_facts();
    facts.owned = true;
    facts.dialog = true;
    assert_eq!(classify(&facts), Err(SkipReason::OwnedDialog));
}

/// Neutral safety projection mirroring the production `classify`
/// normalization (elevated to privileged, tool_window to tool, owned to
/// owned_dialog; overlays ride separately and never affect membership).
fn safety_of(facts: &WindowFacts) -> SafetyFacts {
    SafetyFacts {
        visible: facts.visible,
        minimized: facts.minimized,
        cloaked: facts.cloaked,
        privileged: facts.elevated,
        shell: facts.shell,
        tool: facts.tool_window,
        owned_dialog: facts.owned,
        dialog: facts.dialog,
        no_activate: facts.no_activate,
    }
}

#[test]
fn windows_dialog_outcomes_stay_exclusions_under_shared_membership() {
    // Windows invokes the shared ordered geometry classifier in process
    // and never marks its dialogs transient, so every Windows outcome is
    // unchanged: an unowned generic `#32770` dialog reports `Dialog`, an
    // owned window (including an owned generic dialog) reports
    // `OwnedDialog`, and neither floats. The shared three-way membership
    // verdict agrees with that exclusion: dialog/owned-dialog safety facts
    // leave unmanaged even when transient, pinning the intentional
    // owned-dialog divergence (exclusion remains, never a float member).
    let ordinary = eligible_facts();
    assert!(classify(&ordinary).is_ok());
    assert_eq!(
        classify_membership(&safety_of(&ordinary), false),
        MembershipVerdict::Tile
    );
    // The transient half documents verdict meaning only: Windows never
    // passes transient, so no Windows outcome changes.
    assert_eq!(
        classify_membership(&safety_of(&ordinary), true),
        MembershipVerdict::FloatMember
    );
    let mut dialog = eligible_facts();
    dialog.dialog = true;
    assert_eq!(classify(&dialog), Err(SkipReason::Dialog));
    assert_eq!(
        classify_membership(&safety_of(&dialog), false),
        MembershipVerdict::LeaveUnmanaged
    );
    assert_eq!(
        classify_membership(&safety_of(&dialog), true),
        MembershipVerdict::LeaveUnmanaged
    );
    let mut owned_dialog = eligible_facts();
    owned_dialog.owned = true;
    owned_dialog.dialog = true;
    assert_eq!(classify(&owned_dialog), Err(SkipReason::OwnedDialog));
    assert_eq!(
        classify_membership(&safety_of(&owned_dialog), false),
        MembershipVerdict::LeaveUnmanaged
    );
    assert_eq!(
        classify_membership(&safety_of(&owned_dialog), true),
        MembershipVerdict::LeaveUnmanaged
    );
    let mut owned = eligible_facts();
    owned.owned = true;
    assert_eq!(classify(&owned), Err(SkipReason::OwnedDialog));
    assert_eq!(
        classify_membership(&safety_of(&owned), true),
        MembershipVerdict::LeaveUnmanaged
    );
}

#[test]
fn own_settings_window_gate() {
    let owner_exe = "C:\\Program Files\\omnitiler\\tiler-windows.exe";
    // Own executable plus the settings class is an ordinary slotless
    // workspace member: the discriminator still matches, but the
    // membership dialog gate never refuses it.
    assert!(is_own_settings_window(
        OWN_SETTINGS_WINDOW_CLASS,
        owner_exe,
        owner_exe
    ));
    // Same-executable path spellings still match the exact owner identity.
    assert!(is_own_settings_window(
        OWN_SETTINGS_WINDOW_CLASS,
        "c:/program files/omnitiler/tiler-windows.exe",
        owner_exe
    ));
    assert!(!workspace_dialog_refuses(
        OWN_SETTINGS_WINDOW_CLASS,
        false,
        owner_exe,
        owner_exe
    ));
    // Same executable with any other class stays eligible.
    assert!(!is_own_settings_window("Notepad", owner_exe, owner_exe));
    assert!(!is_own_settings_window("#32770", owner_exe, owner_exe));
    // A foreign app reusing the class name stays eligible: class alone never
    // excludes.
    assert!(!is_own_settings_window(
        OWN_SETTINGS_WINDOW_CLASS,
        "C:\\Windows\\System32\\notepad.exe",
        owner_exe
    ));
    assert!(!workspace_dialog_refuses(
        OWN_SETTINGS_WINDOW_CLASS,
        false,
        "C:\\Windows\\System32\\notepad.exe",
        owner_exe
    ));
}

#[test]
fn workspace_dialog_gate_refuses_only_generic_unowned_dialogs() {
    let owner_exe = "C:\\Program Files\\omnitiler\\tiler-windows.exe";
    // Generic unowned `#32770` dialogs refuse membership.
    assert!(workspace_dialog_refuses(
        "#32770",
        false,
        "C:\\Windows\\System32\\notepad.exe",
        owner_exe
    ));
    // Owned dialogs are owned-dialogs, never dialog-gate refusals.
    assert!(!workspace_dialog_refuses(
        "#32770",
        true,
        "C:\\Windows\\System32\\notepad.exe",
        owner_exe
    ));
    // Ordinary windows never refuse here.
    assert!(!workspace_dialog_refuses(
        "Notepad",
        false,
        "C:\\Windows\\System32\\notepad.exe",
        owner_exe
    ));
    // Own Settings never refuses here, owned or not: it is an ordinary
    // slotless member (same-process windows still refuse via the callers'
    // same-process gates, never here).
    assert!(!workspace_dialog_refuses(
        OWN_SETTINGS_WINDOW_CLASS,
        false,
        owner_exe,
        owner_exe
    ));
    assert!(!workspace_dialog_refuses(
        OWN_SETTINGS_WINDOW_CLASS,
        true,
        owner_exe,
        owner_exe
    ));
}

#[test]
fn own_settings_observation_classifies_eligible() {
    // Offline membership chain for the Settings UI: own executable plus
    // the settings class passes the dialog gate (never a dialog), so the
    // observation facts the native gate now produces classify eligible
    // like any ordinary window. The fixed-size classifier then floats it
    // automatic (pinned from its real construction in `settings_ui`),
    // and sends plus switch-hides ride the ordinary member paths with
    // the Windows send opt-in.
    let owner_exe = "C:\\Program Files\\omnitiler\\tiler-windows.exe";
    assert!(is_own_settings_window(
        OWN_SETTINGS_WINDOW_CLASS,
        owner_exe,
        owner_exe
    ));
    let dialog = workspace_dialog_refuses(OWN_SETTINGS_WINDOW_CLASS, false, owner_exe, owner_exe);
    assert!(!dialog, "own Settings is never a dialog");
    let mut facts = eligible_facts();
    facts.dialog = dialog;
    assert!(classify(&facts).is_ok());
}

#[test]
fn no_terminal_skip_reason() {
    // Terminal windows are ordinary tile targets: eligibility carries no
    // ancestry gate, and the closed skip vocabulary has no terminal variant.
    assert!(classify(&eligible_facts()).is_ok());
    for reason in [
        SkipReason::Hidden,
        SkipReason::Minimized,
        SkipReason::Maximized,
        SkipReason::Cloaked,
        SkipReason::Elevated,
        SkipReason::Shell,
        SkipReason::Tool,
        SkipReason::OwnedDialog,
        SkipReason::Dialog,
        SkipReason::Fullscreen,
        SkipReason::NoActivate,
        SkipReason::Unreadable,
        SkipReason::IdentityChanged,
    ] {
        assert_ne!(reason.as_str(), "terminal");
    }
}

#[test]
fn maximize_overlay_refusal_orders_fullscreen_first() {
    // Directional/pointer routes refuse before any Engine mutation; focus
    // stays allowed (no refusal helper on the focus path). Fullscreen wins
    // when both overlay states hold.
    assert_eq!(overlay_refusal(false, false), None);
    assert_eq!(overlay_refusal(false, true), Some("maximize"));
    assert_eq!(overlay_refusal(true, false), Some("fullscreen"));
    assert_eq!(overlay_refusal(true, true), Some("fullscreen"));
}

#[test]
fn float_toggle_refusal_covers_both_directions() {
    // KDE float-refused-fullscreen/float-refused-maximize parity: an overlay
    // target never floats, and a float the user maximized or fullscreened
    // natively never unfloats until it reads normal again. Fullscreen wins
    // when both hold; a normal tiled window or a normal float proceeds.
    assert_eq!(float_toggle_refusal(false, false), None);
    assert_eq!(
        float_toggle_refusal(false, true),
        Some("float-refused-maximize")
    );
    assert_eq!(
        float_toggle_refusal(true, false),
        Some("float-refused-fullscreen")
    );
    assert_eq!(
        float_toggle_refusal(true, true),
        Some("float-refused-fullscreen")
    );
}

#[test]
fn float_topmost_restores_only_project_raised_bands() {
    // Graceful stop and unfloat restore only a band the project raised
    // (!prior && current) with no frame change: a pre-existing topmost stays
    // untouched even if the user later cleared it, and an untouched band
    // never writes.
    assert!(!float_topmost_restore_needed(false, false));
    assert!(!float_topmost_restore_needed(true, true));
    assert!(float_topmost_restore_needed(false, true));
    assert!(!float_topmost_restore_needed(true, false));
}

#[test]
fn maximize_retained_rect_keeps_tile_allocation() {
    // A maximized member rides its retained tile rectangle, never the native
    // maximum frame, so topology and sibling shares survive the overlay.
    // First sightings (no retained allocation) fall back to the fresh frame.
    let tile = rect(8, 8, 500, 884);
    let native_max = rect(-8, -8, 1616, 916);
    assert_eq!(canonical_retained_rect(true, native_max, Some(tile)), tile);
    assert_eq!(canonical_retained_rect(true, native_max, None), native_max);
    assert_eq!(
        canonical_retained_rect(false, native_max, Some(tile)),
        native_max
    );
}

#[test]
fn classify_orders_fullscreen_before_maximized() {
    // A window holding both overlay states reports fullscreen, matching
    // `overlay_refusal`. Safety skips keep their order: cloaked, elevated,
    // and shell still win over either overlay state.
    let mut facts = eligible_facts();
    facts.maximized = true;
    facts.captionless_fullscreen = true;
    assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
    let mut facts = eligible_facts();
    facts.maximized = true;
    facts.cloaked = true;
    assert_eq!(classify(&facts), Err(SkipReason::Cloaked));
    let mut facts = eligible_facts();
    facts.captionless_fullscreen = true;
    facts.elevated = true;
    assert_eq!(classify(&facts), Err(SkipReason::Elevated));
}

#[test]
fn maximize_admission_clears_once_never_fullscreen() {
    // First non-fullscreen admission on a tiled workspace without a retained
    // tiled slot restores once; fullscreen, already-slotted, and
    // already-attempted members never clear, so there is no automatic retry
    // loop. Floating workspaces never clear: the maximum is preserved until
    // the first tiled admission.
    assert!(should_clear_maximize_at_admission(
        false, true, false, false, true
    ));
    assert!(!should_clear_maximize_at_admission(
        false, true, false, false, false
    ));
    assert!(!should_clear_maximize_at_admission(
        true, true, false, false, true
    ));
    assert!(!should_clear_maximize_at_admission(
        false, true, true, false, true
    ));
    assert!(!should_clear_maximize_at_admission(
        false, true, false, true, true
    ));
    assert!(!should_clear_maximize_at_admission(
        false, false, false, false, true
    ));
    assert!(!should_clear_maximize_at_admission(
        true, false, false, false, true
    ));
    assert!(!should_clear_maximize_at_admission(
        false, true, true, true, false
    ));
}

#[test]
fn born_maximized_overlay_keeps_first_sighting_other_gates_unchanged() {
    // R-MAX-06: the keep-overlay value suppresses only the canonical first
    // tiled sighting; fullscreen, slotted, attempted, floating, and
    // non-maximized rows keep their existing refusal under both values, and
    // the legacy gate pins the tile-on-launch default exactly.
    assert!(should_clear_maximize_at_admission_with_mode(
        false, true, false, false, true, false
    ));
    assert!(!should_clear_maximize_at_admission_with_mode(
        false, true, false, false, true, true
    ));
    for (fullscreen, maximized, known_slot, attempted, tiled) in [
        (true, true, false, false, true),
        (false, true, true, false, true),
        (false, true, false, true, true),
        (false, true, false, false, false),
        (false, false, false, false, true),
        (true, false, false, false, true),
    ] {
        for keep in [false, true] {
            assert!(
                !should_clear_maximize_at_admission_with_mode(
                    fullscreen, maximized, known_slot, attempted, tiled, keep
                ),
                "no clear for ({fullscreen},{maximized},{known_slot},{attempted},{tiled},{keep})"
            );
        }
    }
    // Exact wire values only: the overlay token keeps, everything else tiles.
    assert!(keep_overlay("keep-maximized-overlay"));
    assert!(!keep_overlay("tile-on-launch"));
    for bad in [
        "",
        "tile",
        "overlay",
        "TILE-ON-LAUNCH",
        "group-with-neighbor",
    ] {
        assert!(!keep_overlay(bad), "{bad}");
    }
    // Production binding helper: only tiled-first under the overlay value
    // latches; floating-first (R-MAX-03) never latches under either value.
    assert!(born_keep_latch(true, true));
    assert!(!born_keep_latch(true, false));
    assert!(!born_keep_latch(false, true));
    assert!(!born_keep_latch(false, false));
}

#[test]
fn born_maximized_shared_fixture_conformance() {
    // Shared R-MAX-06 table pins raw -> normalized -> ordinary eligible
    // tiled-first admission outcome. The Windows tiling predicate and the
    // mode-aware clear gate must agree with core on every row.
    use tiler_core::born_maximized::BornMaximized;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../test-fixtures/born-maximized.json"))
            .expect("shared born-maximized fixture parses");
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .expect("fixture carries a case table");
    assert!(!cases.is_empty(), "fixture case table is non-empty");
    for case in cases {
        let name = case
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unnamed>");
        // A missing `raw` key pins the absent setting; any non-string JSON
        // shape (null, number, boolean) normalizes as missing.
        let raw = case.get("raw").and_then(serde_json::Value::as_str);
        let mode = BornMaximized::normalize_wire(raw);
        let normalized = case
            .get("normalized")
            .and_then(serde_json::Value::as_str)
            .expect("each case pins its normalized token");
        assert_eq!(mode.as_wire_str(), normalized, "{name}");
        let should_clear = case
            .get("should_clear")
            .and_then(serde_json::Value::as_bool)
            .expect("each case pins its clear-vs-keep outcome");
        assert_eq!(mode.should_clear_at_admission(), should_clear, "{name}");
        let keep = !should_clear;
        if let Some(raw_str) = raw {
            assert_eq!(keep_overlay(raw_str), keep, "{name}");
        } else {
            // Missing/non-string raws fall back to the tile default lane.
            assert!(should_clear, "{name}");
            assert!(!keep, "{name}");
            assert!(
                !keep_overlay(BornMaximized::default().as_wire_str()),
                "{name}"
            );
        }
        // Ordinary eligible tiled-first admission clears exactly under
        // tile-on-launch and keeps the overlay under keep-maximized-overlay.
        assert_eq!(
            should_clear_maximize_at_admission_with_mode(false, true, false, false, true, keep),
            should_clear,
            "{name}"
        );
    }
}

#[test]
fn maximize_send_flags_must_match_before_transfer() {
    // Workspace-send membership transfer requires live overlay flags to
    // still equal the dispatch snapshot; any flag drift defers with no
    // writes.
    assert!(send_flags_stable(false, true, false, true));
    assert!(send_flags_stable(false, false, false, false));
    assert!(send_flags_stable(true, false, true, false));
    assert!(!send_flags_stable(false, true, false, false));
    assert!(!send_flags_stable(false, false, false, true));
    assert!(!send_flags_stable(false, true, true, true));
}

#[test]
fn fullscreen_toggle_needs_owned_preimage_to_exit() {
    // Owned metadata wins over geometry: a press on any owned frame exits
    // even when the frame no longer covers a monitor (external move or
    // partial restore). Entering is always project-owned (the preimage is
    // stored first); an app-requested fullscreen frame refuses the command
    // exit with a bounded reason and stays app-owned: no input synthesis, no
    // guessed restoration. Re-entry never overwrites: owned frames exit.
    assert_eq!(
        fullscreen_toggle_decision(false, false),
        FullscreenToggle::Enter
    );
    assert_eq!(
        fullscreen_toggle_decision(false, true),
        FullscreenToggle::ExitOwned
    );
    assert_eq!(
        fullscreen_toggle_decision(true, true),
        FullscreenToggle::ExitOwned
    );
    assert_eq!(
        fullscreen_toggle_decision(true, false),
        FullscreenToggle::RefuseAppOwned
    );
    assert_eq!(FullscreenToggle::Enter.as_str(), "enter");
    assert_eq!(FullscreenToggle::ExitOwned.as_str(), "exit-owned");
    assert_eq!(
        FullscreenToggle::RefuseAppOwned.as_str(),
        "refuse-app-owned"
    );
}

#[test]
fn toggle_gate_settles_suspended_and_elevated_without_side_effects() {
    // Fresh per-intent gate for the native toggle arms: suspended sessions
    // and elevated foregrounds settle with bounded outcomes and no writes,
    // matching the workspace dispatcher. Suspension wins when both hold;
    // only a live session proceeds to revalidation.
    assert_eq!(toggle_gate_outcome(true, true), Some("suspended"));
    assert_eq!(toggle_gate_outcome(true, false), Some("suspended"));
    assert_eq!(
        toggle_gate_outcome(false, true),
        Some("elevated-foreground")
    );
    assert_eq!(toggle_gate_outcome(false, false), None);
}

#[test]
fn born_fullscreen_holds_only_first_seen_slotless() {
    // A first-seen fullscreen window without a retained tile slot is held
    // slotless until its first exit, never admitted. A slotted member is a
    // managed overlay transition, and a lifetime-known non-fullscreen window
    // never becomes born again.
    assert!(should_hold_born_fullscreen(true, false, false));
    assert!(!should_hold_born_fullscreen(true, true, false));
    assert!(!should_hold_born_fullscreen(true, false, true));
    assert!(!should_hold_born_fullscreen(true, true, true));
    assert!(!should_hold_born_fullscreen(false, false, false));
    assert!(!should_hold_born_fullscreen(false, true, false));
}

#[test]
fn focus_allows_managed_overlays_never_geometry() {
    // Focus carries no geometry write: maximized and fullscreen members stay
    // focusable (KDE `requestFocus` overlay exemption) while geometry still
    // refuses both. Every other skip refuses on both paths so the geometry
    // classifier is never weakened. A maximized window that is also
    // no-activate still refuses focus (NoActivate), even though geometry
    // reports it as Maximized first.
    let mut facts = eligible_facts();
    assert!(classify(&facts).is_ok());
    assert!(classify_focus(&facts).is_ok());
    facts.maximized = true;
    assert_eq!(classify(&facts), Err(SkipReason::Maximized));
    assert!(classify_focus(&facts).is_ok());
    facts.captionless_fullscreen = true;
    assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
    assert!(classify_focus(&facts).is_ok());
    let check = |mut facts: WindowFacts, reason: SkipReason| {
        facts.maximized = true;
        assert!(classify(&facts).is_err());
        assert_eq!(classify_focus(&facts), Err(reason));
    };
    let mut base = eligible_facts();
    base.minimized = true;
    check(base, SkipReason::Minimized);
    base = eligible_facts();
    base.cloaked = true;
    check(base, SkipReason::Cloaked);
    base = eligible_facts();
    base.elevated = true;
    check(base, SkipReason::Elevated);
    base = eligible_facts();
    base.shell = true;
    check(base, SkipReason::Shell);
    base = eligible_facts();
    base.tool_window = true;
    check(base, SkipReason::Tool);
    base = eligible_facts();
    base.owned = true;
    check(base, SkipReason::OwnedDialog);
    base = eligible_facts();
    base.dialog = true;
    check(base, SkipReason::Dialog);
    base = eligible_facts();
    base.no_activate = true;
    check(base, SkipReason::NoActivate);
}

/// Neutral token for one native skip: only `elevated` renames the core
/// `privileged` refusal; every other log token already matches the fixture.
fn neutral_skip(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Elevated => "privileged",
        other => other.as_str(),
    }
}

#[test]
fn shared_window_policy_fixture_projection_matches_core_order() {
    // Host-fact projection over the shared ordered-classification cases:
    // native-vocabulary `WindowFacts` (elevated/tool_window/owned/
    // captionless_fullscreen) enter the production `classify` /
    // `classify_focus` normalization and must report the core order, priority
    // conflicts included. This pins the normalization itself, not a
    // core-to-core reexport.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../test-fixtures/window-policy.json"))
            .expect("shared window-policy fixture parses");
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .expect("fixture carries a case table");
    let mut covered = 0;
    for case in cases {
        let name = case
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unnamed>");
        let gate = case
            .get("gate")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing gate>");
        if gate != "geometry_classification" && gate != "focus_classification" {
            continue;
        }
        covered += 1;
        let bit = |key: &str| {
            case.get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_else(|| panic!("{name} carries `{key}`"))
        };
        let facts = WindowFacts {
            visible: bit("visible"),
            minimized: bit("minimized"),
            maximized: bit("maximized"),
            cloaked: bit("cloaked"),
            elevated: bit("privileged"),
            shell: bit("shell"),
            tool_window: bit("tool"),
            owned: bit("owned_dialog"),
            captionless_fullscreen: bit("fullscreen"),
            no_activate: bit("no_activate"),
            dialog: bit("dialog"),
        };
        let expected = case.get("refusal").and_then(serde_json::Value::as_str);
        if gate == "geometry_classification" {
            assert_eq!(classify(&facts).err().map(neutral_skip), expected, "{name}");
        } else {
            assert_eq!(
                classify_focus(&facts).err().map(neutral_skip),
                expected,
                "{name}"
            );
        }
    }
    assert!(covered > 0, "fixture carries classification cases");
}

#[test]
fn maximized_member_never_takes_geometry_writes() {
    // Production seam: a maximized member classifies out of the eligible
    // observation, so the portable writable subset (the same
    // `writable_subset` production calls) never includes its retained-only
    // token: the overlaid tile keeps its allocation with no native write.
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use tiler_windows::native_guards::{WindowKey, writable_subset};
    let mut facts = eligible_facts();
    facts.maximized = true;
    assert_eq!(classify(&facts), Err(SkipReason::Maximized));
    let key = WindowKey {
        hwnd: 11,
        pid: 12,
        creation: "creation-a".to_owned(),
    };
    let members: BTreeSet<WindowKey> = BTreeSet::from([key.clone()]);
    let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([(key.clone(), "w1".to_owned())]);
    // Retained-only token: member but absent from the fresh eligible
    // observation, so not writable.
    let fresh: HashSet<String> = HashSet::new();
    assert!(writable_subset(&members, |_| false, &token_of, &fresh).is_empty());
    // Eligible-observed token stays writable; hidden members never are.
    let fresh: HashSet<String> = HashSet::from(["w1".to_owned()]);
    assert_eq!(
        writable_subset(&members, |_| false, &token_of, &fresh),
        HashSet::from(["w1".to_owned()])
    );
    assert!(writable_subset(&members, |_| true, &token_of, &fresh).is_empty());
}

#[test]
fn fullscreen_member_never_takes_geometry_writes() {
    // Item 9 production seam: a fullscreen member classifies out of the
    // eligible observation exactly like maximized, so the portable writable
    // subset (the same `writable_subset` production calls) never includes
    // its retained-only token: the overlaid tile keeps its Engine allocation
    // with no native size/position write. Only move/hide/reveal touch it;
    // focus carries no write (`classify_focus` overlay exemption, already
    // pinned). The app-owned toggle refusal (`fullscreen_toggle_decision`)
    // is untouched and covered separately.
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use tiler_windows::native_guards::{WindowKey, writable_subset};
    let mut facts = eligible_facts();
    facts.captionless_fullscreen = true;
    assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
    assert!(classify_focus(&facts).is_ok());
    let key = WindowKey {
        hwnd: 13,
        pid: 14,
        creation: "creation-b".to_owned(),
    };
    let members: BTreeSet<WindowKey> = BTreeSet::from([key.clone()]);
    let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([(key.clone(), "wB".to_owned())]);
    let fresh: HashSet<String> = HashSet::new();
    assert!(writable_subset(&members, |_| false, &token_of, &fresh).is_empty());
    let fresh: HashSet<String> = HashSet::from(["wB".to_owned()]);
    assert_eq!(
        writable_subset(&members, |_| false, &token_of, &fresh),
        HashSet::from(["wB".to_owned()])
    );
    assert!(writable_subset(&members, |_| true, &token_of, &fresh).is_empty());
}
#[test]
fn scope_filter_defaults_open_and_fences_exes() {
    let empty: Vec<String> = Vec::new();
    assert!(scope_allows(&empty, "C:\\Windows\\System32\\notepad.exe"));
    assert!(scope_allows(&empty, "WindowsTerminal.exe"));
    let scope = vec![
        "notepad.exe".to_owned(),
        "ApplicationFrameHost.exe".to_owned(),
        "mspaint.exe".to_owned(),
    ];
    assert!(scope_allows(&scope, "C:\\Windows\\System32\\NOTEPAD.EXE"));
    assert!(scope_allows(&scope, "C:/Windows/System32/mspaint.exe"));
    assert!(scope_allows(&scope, "applicationframehost.exe"));
    // Host-level scope alone cannot distinguish hosted apps (Calculator vs
    // any other Store app): the exact-target runtime fence is the
    // `--scope-host-child` pair below, enforced per tick at observation and
    // fresh at every hide admission.
    assert!(!scope_allows(
        &scope,
        "C:\\Program Files\\WindowsApps\\Microsoft.WindowsTerminal.exe"
    ));
    assert!(!scope_allows(&scope, "firefox.exe"));
    assert_eq!(scope_exe_basename("C:\\a\\NOTEPAD.EXE"), "notepad.exe");
    assert_eq!(scope_exe_basename("mspaint.exe"), "mspaint.exe");
}

#[test]
fn hosted_child_fence_needs_live_matching_child() {
    let pairs = vec![ScopeHostChild {
        host: "ApplicationFrameHost.exe".to_owned(),
        child: "CalculatorApp.exe".to_owned(),
    }];
    let calc = vec!["C:\\Program Files\\WindowsApps\\CalculatorApp.exe".to_owned()];
    let other = vec!["C:\\Program Files\\WindowsApps\\OtherApp.exe".to_owned()];
    // Empty pairs: no constraint anywhere.
    assert!(hosted_child_allows("ApplicationFrameHost.exe", &[], &[]));
    // Unlisted top-level executables are unconstrained by the pairs.
    assert!(hosted_child_allows("notepad.exe", &[], &pairs));
    assert!(hosted_child_allows("notepad.exe", &other, &pairs));
    // Listed host with a live matching hosted child passes (case-insensitive).
    assert!(hosted_child_allows(
        "applicationframehost.exe",
        &calc,
        &pairs
    ));
    // Listed host with only a non-matching hosted app fails closed, even
    // though the top-level executable itself is scope-listed.
    assert!(!hosted_child_allows(
        "ApplicationFrameHost.exe",
        &other,
        &pairs
    ));
    // Listed host with no hosted children at all fails closed: a bare host
    // frame shows no app and manages nothing.
    assert!(!hosted_child_allows(
        "ApplicationFrameHost.exe",
        &[],
        &pairs
    ));
    // Unreadable children contribute nothing: no match, no pass.
    assert!(!hosted_child_allows(
        "ApplicationFrameHost.exe",
        &["unknown".to_owned()],
        &pairs
    ));
    // Extra non-matching siblings do not veto a live match (one top-level
    // window is one app frame).
    let mixed = vec![
        "C:\\Program Files\\WindowsApps\\OtherApp.exe".to_owned(),
        "C:\\Program Files\\WindowsApps\\CalculatorApp.exe".to_owned(),
    ];
    assert!(hosted_child_allows(
        "ApplicationFrameHost.exe",
        &mixed,
        &pairs
    ));
    // Pairs constrain only their own host: a second pair neither widens nor
    // narrows the first, and unlisted hosts stay unconstrained.
    let pairs = vec![
        ScopeHostChild {
            host: "ApplicationFrameHost.exe".to_owned(),
            child: "CalculatorApp.exe".to_owned(),
        },
        ScopeHostChild {
            host: "OtherHost.exe".to_owned(),
            child: "OtherChild.exe".to_owned(),
        },
    ];
    assert!(hosted_child_allows(
        "ApplicationFrameHost.exe",
        &calc,
        &pairs
    ));
    assert!(!hosted_child_allows(
        "ApplicationFrameHost.exe",
        &other,
        &pairs
    ));
    assert!(hosted_child_allows(
        "OtherHost.exe",
        &["OtherChild.exe".to_owned()],
        &pairs
    ));
    assert!(!hosted_child_allows("OtherHost.exe", &calc, &pairs));
    assert!(hosted_child_allows("notepad.exe", &[], &pairs));
}

#[test]
fn scope_host_child_parses_pairs_and_refuses_malformed() {
    let pair = parse_scope_host_child("ApplicationFrameHost.exe=CalculatorApp.exe").expect("pair");
    assert_eq!(pair.host, "ApplicationFrameHost.exe");
    assert_eq!(pair.child, "CalculatorApp.exe");
    for bad in [
        "",
        "=",
        "=Child.exe",
        "Host.exe=",
        "Host.exe=A=B",
        "  ",
        "Host.exe = ",
    ] {
        assert!(
            parse_scope_host_child(bad).is_err(),
            "refuses {bad:?}, never a widened scope"
        );
    }
}

#[test]
fn tile_args_accept_scope_host_child_with_dedup() {
    let args = [
        "--user-start",
        "--scope-exe",
        "ApplicationFrameHost.exe",
        "--scope-host-child",
        "ApplicationFrameHost.exe=CalculatorApp.exe",
        "--scope-host-child",
        "applicationframehost.exe=calculatorapp.exe",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    let options = parse_tile_args(&args).expect("tile args");
    assert_eq!(options.scope_hosts.len(), 1);
    assert_eq!(options.scope_hosts[0].host, "ApplicationFrameHost.exe");
    assert_eq!(options.scope_hosts[0].child, "CalculatorApp.exe");
    // Malformed pair refuses the whole run, never a silent normal run.
    let bad = [
        "--user-start",
        "--scope-host-child",
        "ApplicationFrameHost.exe",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    assert!(parse_tile_args(&bad).is_err());
    // Default stays unconstrained: no pairs without the explicit flag.
    let plain = ["--user-start".to_owned()];
    assert!(
        parse_tile_args(&plain)
            .expect("plain")
            .scope_hosts
            .is_empty()
    );
}

#[test]
fn token_reissue_breaks_engine_identity_after_reuse() {
    // Same-process HWND reuse keeps (HWND, creation): the stable token would
    // inherit the previous generation's Engine identity. Reissue mints a
    // fresh token for the new generation; later ticks resolve it stably.
    let mut tokens = TokenMap::default();
    let first = tokens.token_for(0x1234, "creation-1");
    assert_eq!(tokens.token_for(0x1234, "creation-1"), first);
    let second = tokens.reissue(0x1234, "creation-1");
    assert_ne!(second, first);
    assert_eq!(tokens.token_for(0x1234, "creation-1"), second);
    // Unrelated windows keep their own stable tokens.
    let other = tokens.token_for(0x5678, "creation-1");
    assert_ne!(other, first);
    assert_ne!(other, second);
}

#[test]
fn allowlist_exact_match_and_rejections() {
    let entry = tiler_windows::tiling::AllowEntry {
        hwnd: 99,
        pid: 7,
        process_creation: "000000000000abcd".to_owned(),
        exe_path: "C:\\apps\\Helper.EXE".to_owned(),
        user_sid: "sid-1".to_owned(),
        session_id: 1,
        tag: "18da5b07aae82240".to_owned(),
    };
    let observed = ObservedTarget {
        hwnd: 99,
        pid: 7,
        process_creation: "000000000000abcd".to_owned(),
        exe_path: "c:/apps/helper.exe".to_owned(),
        user_sid: "sid-1".to_owned(),
        session_id: 1,
        tag: "18da5b07aae82240".to_owned(),
    };
    assert!(allow_match(&entry, &observed));
    let changed = ObservedTarget {
        process_creation: "ffffffffffffffff".to_owned(),
        ..observed.clone()
    };
    assert!(!allow_match(&entry, &changed), "pid reuse must not match");
    // Stored pid equality: same creation/tag but a different pid never matches.
    let repid = ObservedTarget {
        pid: 8,
        ..observed.clone()
    };
    assert!(!allow_match(&entry, &repid), "pid mismatch must not match");
    // Recycled HWND in the same process carries a fresh tag and never matches.
    let retagged = ObservedTarget {
        tag: "ffffffffffffffff".to_owned(),
        ..observed.clone()
    };
    assert!(!allow_match(&entry, &retagged), "tag reuse must not match");
    let untagged = ObservedTarget {
        tag: String::new(),
        ..observed.clone()
    };
    assert!(!allow_match(&entry, &untagged), "empty tag must not match");
}

#[test]
fn allowlist_identity_excludes_window_band() {
    // Ownership identity is hwnd/pid/creation/exe/sid/session/tag only: the
    // native topmost band (WS_EX_TOPMOST keep-above float/sticky state) is
    // approved mutable product state and never part of the frozen identity.
    // There is no band/topmost field on either side of the match, so a
    // topmost transition cannot change matching; the digest binds the same
    // seven components (pid/creation changes digest, proving full equality).
    let entry = tiler_windows::tiling::AllowEntry {
        hwnd: 99,
        pid: 7,
        process_creation: "000000000000abcd".to_owned(),
        exe_path: "C:\\apps\\Helper.EXE".to_owned(),
        user_sid: "sid-1".to_owned(),
        session_id: 1,
        tag: "18da5b07aae82240".to_owned(),
    };
    let observed = ObservedTarget {
        hwnd: 99,
        pid: 7,
        process_creation: "000000000000abcd".to_owned(),
        exe_path: "c:/apps/helper.exe".to_owned(),
        user_sid: "sid-1".to_owned(),
        session_id: 1,
        tag: "18da5b07aae82240".to_owned(),
    };
    assert!(allow_match(&entry, &observed));
    let base = allowlist_digest(std::slice::from_ref(&entry));
    let repid = tiler_windows::tiling::AllowEntry {
        pid: 8,
        ..entry.clone()
    };
    let recreation = tiler_windows::tiling::AllowEntry {
        process_creation: "ffffffffffffffff".to_owned(),
        ..entry.clone()
    };
    assert_ne!(base, allowlist_digest(std::slice::from_ref(&repid)));
    assert_ne!(base, allowlist_digest(std::slice::from_ref(&recreation)));
}

#[test]
fn allowlist_malformed_and_empty_refused() {
    assert!(parse_allowlist("not json").is_err());
    assert!(parse_allowlist("{\"windows\": []}").is_err());
    assert!(parse_allowlist("{\"windows\": [{\"hwnd\": 0}]}").is_err());
    let ok = parse_allowlist(
        "{\"windows\": [{\"hwnd\": 9, \"pid\": 3, \"process_creation\": \"a\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1, \"tag\": \"t1\"}]}",
    )
    .expect("valid");
    assert_eq!(ok.len(), 1);
    // Missing tag is malformed: never fall back to normal.
    assert!(parse_allowlist(
        "{\"windows\": [{\"hwnd\": 9, \"pid\": 3, \"process_creation\": \"a\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1}]}",
    )
    .is_err());
    assert!(parse_allowlist(
        "{\"windows\": [{\"hwnd\": 9, \"pid\": 3, \"process_creation\": \"a\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1, \"tag\": \"\"}]}",
    )
    .is_err());
}

#[test]
fn allowlist_rejects_duplicates_and_empty_fields() {
    let dup = "{\"windows\": [\
        {\"hwnd\": 9, \"pid\": 3, \"process_creation\": \"a\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1, \"tag\": \"t1\"},\
        {\"hwnd\": 9, \"pid\": 4, \"process_creation\": \"b\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1, \"tag\": \"t2\"}]}";
    assert!(parse_allowlist(dup).is_err());
    let empty = "{\"windows\": [\
        {\"hwnd\": 9, \"pid\": 3, \"process_creation\": \"\", \"exe_path\": \"e\", \"user_sid\": \"s\", \"session_id\": 1, \"tag\": \"t1\"}]}";
    assert!(parse_allowlist(empty).is_err());
}

#[test]
fn allowlist_digest_stable_and_bound() {
    let a = tiler_windows::tiling::AllowEntry {
        hwnd: 9,
        pid: 3,
        process_creation: "a".to_owned(),
        exe_path: "C:\\apps\\Helper.EXE".to_owned(),
        user_sid: "s".to_owned(),
        session_id: 1,
        tag: "t1".to_owned(),
    };
    let b = tiler_windows::tiling::AllowEntry {
        hwnd: 10,
        pid: 4,
        process_creation: "b".to_owned(),
        exe_path: "e".to_owned(),
        user_sid: "s".to_owned(),
        session_id: 1,
        tag: "t2".to_owned(),
    };
    // Order-independent: frozen set digest, not file order.
    assert_eq!(
        allowlist_digest(&[a.clone(), b.clone()]),
        allowlist_digest(&[b.clone(), a.clone()])
    );
    // Any identity or tag change changes the digest.
    let changed = tiler_windows::tiling::AllowEntry {
        tag: "t9".to_owned(),
        ..a.clone()
    };
    assert_ne!(
        allowlist_digest(&[a.clone(), b.clone()]),
        allowlist_digest(&[changed, b.clone()])
    );
    assert_eq!(allowlist_digest(&[]).len(), 16);
}

#[test]
fn gesture_true_move_same_size() {
    let before = rect(0, 0, 800, 600);
    let after = rect(100, 100, 800, 600);
    assert_eq!(
        classify_gesture(&before, &after, Some((150, 150))),
        Some(GestureIntent::MoveDrop { x: 150, y: 150 })
    );
    assert_eq!(classify_gesture(&before, &after, None), None);
    assert_eq!(classify_gesture(&before, &before, Some((1, 1))), None);
}

#[test]
fn gesture_left_resize_not_move() {
    // Origin changed with size: left-edge resize, never a move.
    let before = rect(100, 100, 800, 600);
    assert_eq!(
        classify_gesture(&before, &rect(60, 100, 840, 600), Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "left",
            boundary: 60,
            direction2: None,
            boundary2: None,
        })
    );
}

#[test]
fn gesture_up_resize_not_move() {
    let before = rect(100, 100, 800, 600);
    assert_eq!(
        classify_gesture(&before, &rect(100, 40, 800, 660), Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "up",
            boundary: 40,
            direction2: None,
            boundary2: None,
        })
    );
}

#[test]
fn gesture_corner_prefers_larger_edge() {
    // Both left and right moved: the larger delta wins (right here).
    let before = rect(100, 100, 800, 600);
    assert_eq!(
        classify_gesture(&before, &rect(90, 100, 900, 600), Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 990,
            direction2: None,
            boundary2: None,
        })
    );
}

#[test]
fn gesture_queued_settle_from_stable_pre() {
    // START+END inside one poll interval: the stable pre-gesture rectangle
    // (not a mid-gesture frame) still classifies the settle.
    let stable = rect(0, 0, 800, 600);
    let post = rect(0, 0, 950, 600);
    assert_eq!(
        classify_gesture(&stable, &post, Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 950,
            direction2: None,
            boundary2: None,
        })
    );
}

#[test]
fn modal_move_survives_mixed_dpi_rescale() {
    // Native caption drag from a 100% monitor onto a 125% one: the OS
    // rescales the frame mid-gesture (800x600 -> 1000x750), which rect
    // comparison alone misreads as a user resize. The START-sampled modal
    // kind (caption = move) keeps it a move drop at the release cursor; a
    // sizing-border start (or unknown kind) keeps the resize route, moves
    // pass through, and cursor-less resizes keep the geometry route.
    let before = rect(100, 100, 800, 600);
    let rescaled = rect(-1800, 50, 1000, 750);
    let cursor = Some((-1700, 200));
    let resize = classify_gesture(&before, &rescaled, cursor).expect("rescale classifies by rects");
    assert!(matches!(resize, GestureIntent::PointerResize { .. }));
    assert_eq!(
        prefer_modal_move(resize, cursor, true),
        GestureIntent::MoveDrop { x: -1700, y: 200 }
    );
    assert_eq!(prefer_modal_move(resize, cursor, false), resize);
    assert_eq!(prefer_modal_move(resize, None, true), resize);
    let moved = classify_gesture(&before, &rect(-1800, 50, 800, 600), cursor)
        .expect("same-size move classifies");
    assert_eq!(prefer_modal_move(moved, cursor, true), moved);
    assert_eq!(prefer_modal_move(moved, cursor, false), moved);
}

#[test]
fn gesture_resize_edges() {
    let before = rect(0, 0, 800, 600);
    // Right edge dragged out.
    assert_eq!(
        classify_gesture(&before, &rect(0, 0, 900, 600), Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 900,
            direction2: None,
            boundary2: None,
        })
    );
    // Corner drag routes both axes, horizontal first.
    assert_eq!(
        classify_gesture(&before, &rect(0, 0, 900, 700), Some((0, 0))),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 900,
            direction2: Some("down"),
            boundary2: Some(700),
        })
    );
}

#[test]
fn children_args_require_explicit_hwnd() {
    assert!(parse_children_args(&[]).is_err());
    let options = parse_children_args(&strings(&[
        "--hwnd", "123", "--hwnd", "0xAB", "--hwnd", "123",
    ]))
    .expect("parsed");
    assert_eq!(options.hwnds, vec![123, 0xAB]);
    assert!(parse_children_args(&strings(&["--bogus"])).is_err());
}

#[test]
fn inspect_args_require_allowlist() {
    assert!(parse_inspect_args(&[]).is_err());
    assert!(parse_inspect_args(&strings(&["--preimages", "p.json"])).is_err());
    let options = parse_inspect_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert_eq!(options.allowlist, std::path::PathBuf::from("a.json"));
}

#[test]
fn tile_args_require_explicit_user_start() {
    // No silent normal runs: agents never invoke this path.
    assert!(parse_tile_args(&[]).is_err());
    assert!(parse_tile_args(&strings(&["--trace"])).is_err());
    let options = parse_tile_args(&strings(&["--user-start"])).expect("user start");
    assert_eq!(options.seconds, None);
    assert!(!options.trace);
    assert!(options.user_start);
    assert!(options.scope_exes.is_empty(), "no default scope filter");
    let options =
        parse_tile_args(&strings(&["--user-start", "--seconds", "60", "--trace"])).expect("parsed");
    assert_eq!(options.seconds, Some(60));
    assert!(options.trace);
    assert!(options.scope_exes.is_empty());
    // Allowlist never rides the normal path: proof uses tile-proof so lost
    // arguments cannot fall back to tiling the whole desktop.
    assert!(parse_tile_args(&strings(&["--user-start", "--allowlist", "a.json"])).is_err());
    assert!(parse_tile_args(&["--seconds".to_owned(), "0".to_owned()]).is_err());
    assert!(parse_tile_args(&["--bogus".to_owned()]).is_err());
}

#[test]
fn tile_args_scope_exe_is_explicit_opt_in() {
    let options = parse_tile_args(&strings(&[
        "--user-start",
        "--scope-exe",
        "notepad.exe",
        "--scope-exe",
        "ApplicationFrameHost.exe",
        "--scope-exe",
        "mspaint.exe",
    ]))
    .expect("scoped");
    assert_eq!(options.scope_exes.len(), 3);
    assert!(scope_allows(
        &options.scope_exes,
        "C:\\Windows\\System32\\notepad.exe"
    ));
    assert!(!scope_allows(&options.scope_exes, "firefox.exe"));
    // Case-insensitive dedupe: same basename twice stores once.
    let options = parse_tile_args(&strings(&[
        "--user-start",
        "--scope-exe",
        "notepad.exe",
        "--scope-exe",
        "NOTEPAD.EXE",
    ]))
    .expect("scoped");
    assert_eq!(options.scope_exes.len(), 1);
    // Empty scope value refuses, never a wildcard.
    assert!(parse_tile_args(&strings(&["--user-start", "--scope-exe", " "])).is_err());
    assert!(parse_tile_args(&strings(&["--user-start", "--scope-exe"])).is_err());
}

#[test]
fn tile_proof_args_require_allowlist() {
    // Missing allowlist refuses before any lease; never normal mode.
    assert!(parse_tile_proof_args(&[]).is_err());
    assert!(parse_tile_proof_args(&strings(&["--trace"])).is_err());
    assert!(parse_tile_proof_args(&strings(&["--allowlist", ""])).is_err());
    let options = parse_tile_proof_args(&strings(&["--allowlist", "a.json"])).expect("proof");
    assert_eq!(options.allowlist, std::path::PathBuf::from("a.json"));
    assert_eq!(options.seconds, None);
    let options = parse_tile_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "60",
        "--trace",
    ]))
    .expect("parsed");
    assert_eq!(options.seconds, Some(60));
    assert!(options.trace);
    assert!(parse_tile_proof_args(&strings(&["--allowlist", "a.json", "--bogus"])).is_err());
}

#[test]
fn proof_argv_consistency_evidences_every_flag() {
    // Full delivery: allowlist path with spaces survives as one argv element,
    // seconds and trace evidenced alongside.
    let raw = strings(&[
        "--allowlist",
        "C:\\my dir\\a.json",
        "--seconds",
        "300",
        "--trace",
    ]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw, &parsed).is_ok());
    // Untimed no-write form: absent seconds on both sides is consistent.
    let raw = strings(&["--allowlist", "a.json", "--trace"]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw, &parsed).is_ok());
    // Dropped trace flag on either side is an impossible error.
    let parsed_no_trace =
        parse_tile_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw, &parsed_no_trace).is_err());
    // Dropped deadline on either side is an impossible error.
    let raw_full = strings(&["--allowlist", "a.json", "--seconds", "300", "--trace"]);
    assert!(verify_proof_argv_consistency(&raw_full, &parsed).is_err());
    // Swapped path is an impossible error, never a silent retarget.
    let raw_other = strings(&["--allowlist", "b.json", "--trace"]);
    assert!(verify_proof_argv_consistency(&raw_other, &parsed).is_err());
    // Unknown flags never verify.
    let raw_bogus = strings(&["--allowlist", "a.json", "--bogus"]);
    assert!(verify_proof_argv_consistency(&raw_bogus, &parsed).is_err());
    // Missing allowlist never verifies.
    assert!(verify_proof_argv_consistency(&strings(&["--trace"]), &parsed).is_err());
}

#[test]
fn hide_proof_args_require_allowlist() {
    assert!(parse_hide_proof_args(&[]).is_err());
    assert!(parse_hide_proof_args(&strings(&["--trace"])).is_err());
    assert!(parse_hide_proof_args(&strings(&["--allowlist", ""])).is_err());
    let options = parse_hide_proof_args(&strings(&["--allowlist", "a.json"])).expect("proof");
    assert_eq!(options.allowlist, std::path::PathBuf::from("a.json"));
    assert_eq!(options.seconds, None);
    assert!(!options.trace);
    let options = parse_hide_proof_args(&strings(&[
        "--allowlist",
        "a.json",
        "--seconds",
        "60",
        "--trace",
    ]))
    .expect("parsed");
    assert_eq!(options.seconds, Some(60));
    assert!(options.trace);
    assert!(parse_hide_proof_args(&strings(&["--allowlist", "a.json", "--bogus"])).is_err());
    // Never falls back: unknown flags and bad seconds refuse.
    assert!(parse_hide_proof_args(&strings(&["--allowlist", "a.json", "--seconds", "0"])).is_err());
}

#[test]
fn hide_proof_argv_consistency_evidences_every_flag() {
    let raw = strings(&[
        "--allowlist",
        "C:\\my dir\\a.json",
        "--seconds",
        "300",
        "--trace",
    ]);
    let parsed = parse_hide_proof_args(&raw).expect("parsed");
    assert!(verify_hide_proof_argv_consistency(&raw, &parsed).is_ok());
    let raw = strings(&["--allowlist", "a.json", "--trace"]);
    let parsed = parse_hide_proof_args(&raw).expect("parsed");
    assert!(verify_hide_proof_argv_consistency(&raw, &parsed).is_ok());
    let parsed_no_trace =
        parse_hide_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert!(verify_hide_proof_argv_consistency(&raw, &parsed_no_trace).is_err());
    let raw_full = strings(&["--allowlist", "a.json", "--seconds", "300", "--trace"]);
    assert!(verify_hide_proof_argv_consistency(&raw_full, &parsed).is_err());
    let raw_other = strings(&["--allowlist", "b.json", "--trace"]);
    assert!(verify_hide_proof_argv_consistency(&raw_other, &parsed).is_err());
    let raw_bogus = strings(&["--allowlist", "a.json", "--bogus"]);
    assert!(verify_hide_proof_argv_consistency(&raw_bogus, &parsed).is_err());
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn domain_bounds_inset_outer_gap() {
    assert_eq!(
        tiling_domain_bounds(rect(0, 0, 1600, 900)),
        Some(rect(8, 8, 1584, 884))
    );
    assert_eq!(tiling_domain_bounds(rect(0, 0, 10, 10)), None);
}

#[test]
fn capture_args_require_explicit_hwnd() {
    assert!(parse_capture_args(&[]).is_err());
    assert!(parse_capture_args(&strings(&["--out", "r.json"])).is_err());
    assert!(parse_capture_args(&strings(&["--hwnd", "9"])).is_err());
    let options = parse_capture_args(&strings(&[
        "--out", "r.json", "--hwnd", "123", "--hwnd", "0xAB", "--hwnd", "123",
    ]))
    .expect("parsed");
    assert_eq!(options.hwnds, vec![123, 0xAB]);
    let _: CaptureOptions = options;
}

#[test]
fn workspace_proof_parses_without_widening_product() {
    let args: Vec<String> = ["--allowlist", "a.json"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let ok = parse_workspace_proof_args(&args).expect("parsed");
    assert_eq!(ok.allowlist.to_str().expect("path"), "a.json");
    let raw = args.clone();
    assert!(verify_workspace_proof_argv_consistency(&raw, &ok).is_ok());
    // Product tile still refuses allowlists: no fallback into normal mode.
    let tile: Vec<String> = ["--user-start", "--allowlist", "a.json"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert!(parse_tile_args(&tile).is_err());
}

#[test]
fn borderless_fullscreen_needs_captionless_whole_monitor() {
    let full = rect(0, 0, 2560, 1440);
    let work = rect(0, 0, 2560, 1380);
    assert!(is_borderless_fullscreen(
        true,
        rect(0, 0, 2560, 1440),
        &[full]
    ));
    assert!(is_borderless_fullscreen(
        true,
        rect(-2560, 0, 2560, 1440),
        &[rect(-2560, 0, 2560, 1440)]
    ));
    assert!(!is_borderless_fullscreen(true, work, &[full]));
    assert!(!is_borderless_fullscreen(false, full, &[full]));
    assert!(!is_borderless_fullscreen(
        true,
        rect(200, 200, 640, 480),
        &[full]
    ));
    assert!(is_borderless_fullscreen(
        true,
        rect(2560, 0, 1920, 1080),
        &[full, rect(2560, 0, 1920, 1080)]
    ));
    assert!(!is_borderless_fullscreen(
        true,
        rect(2560, 0, 800, 600),
        &[full, rect(2560, 0, 1920, 1080)]
    ));
}

#[test]
fn tokens_keep_skipped_identities_stable() {
    let mut map = TokenMap::default();
    let eligible = map.token_for(1, "c1");
    let skipped = map.token_for(2, "c2");
    map.retain(&[
        ObservedTargetRef {
            hwnd: 1,
            creation: "c1",
        },
        ObservedTargetRef {
            hwnd: 2,
            creation: "c2",
        },
    ]);
    assert_eq!(map.token_for(1, "c1"), eligible);
    assert_eq!(map.token_for(2, "c2"), skipped);
}

#[test]
fn tick_summary_stable_noop_without_tick() {
    let skips = vec![
        ("w9".to_owned(), "refused-intent".to_owned()),
        ("w1".to_owned(), "cloaked".to_owned()),
    ];
    let a = tick_summary_signature(5, 0, &skips, 1, "reconcile", 7);
    let b = tick_summary_signature(5, 0, &skips, 1, "reconcile", 7);
    assert_eq!(a, b, "identical observations must dedupe across ticks");
    let changed = tick_summary_signature(5, 1, &skips, 1, "reconcile", 7);
    assert_ne!(a, changed);
}

#[test]
fn stateless_verdict_keeps_identity_without_frames() {
    use StatelessVerdict::{NeedsGeometry, Report};
    // Non-matching identity is a true unknown even when visible: never a
    // counted skip, never eligible.
    assert_eq!(
        inspect_stateless_verdict(false, true, false),
        Report {
            identity_match: false,
            eligible: false,
            skip: SkipReason::IdentityChanged,
        }
    );
    assert_eq!(SkipReason::IdentityChanged.as_str(), "identity-changed");
    // Hidden exact helper (passive, pre-admission): identity match stands.
    assert_eq!(
        inspect_stateless_verdict(true, false, false),
        Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Hidden,
        }
    );
    // Minimized exact helper: state skip with no frame read and no actuation.
    assert_eq!(
        inspect_stateless_verdict(true, true, true),
        Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Minimized,
        }
    );
    // Visible, non-minimized match still needs geometry and `classify`.
    assert_eq!(inspect_stateless_verdict(true, true, false), NeedsGeometry);
    // Mismatch beats visibility: a foreign window at a frozen HWND is
    // unknown, and hidden beats minimized.
    assert_eq!(
        inspect_stateless_verdict(false, false, false),
        Report {
            identity_match: false,
            eligible: false,
            skip: SkipReason::IdentityChanged,
        }
    );
    assert_eq!(
        inspect_stateless_verdict(true, false, true),
        Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Hidden,
        }
    );
}

#[test]
fn retain_keeps_known_identities_across_reconcile_passes() {
    // Production reconcile scenario: one eligible window, one minimized
    // helper (identity known, no frame), and one frame-unreadable window
    // (identity known) must all keep stable tokens across passes, while an
    // unidentified window and a closed window must not.
    let mut map = TokenMap::default();
    let eligible = map.token_for(11, "creation-eligible");
    let minimized = map.token_for(22, "creation-minimized");
    let unreadable_frameless = map.token_for(33, "creation-frameless");
    // Pass 1 retains every resolved identity, including the frameless ones.
    map.retain(&[
        ObservedTargetRef {
            hwnd: 11,
            creation: "creation-eligible",
        },
        ObservedTargetRef {
            hwnd: 22,
            creation: "creation-minimized",
        },
        ObservedTargetRef {
            hwnd: 33,
            creation: "creation-frameless",
        },
    ]);
    // Pass 2 with the same enumerated set: no token churn.
    map.retain(&[
        ObservedTargetRef {
            hwnd: 11,
            creation: "creation-eligible",
        },
        ObservedTargetRef {
            hwnd: 22,
            creation: "creation-minimized",
        },
        ObservedTargetRef {
            hwnd: 33,
            creation: "creation-frameless",
        },
    ]);
    assert_eq!(map.token_for(11, "creation-eligible"), eligible);
    assert_eq!(map.token_for(22, "creation-minimized"), minimized);
    assert_eq!(
        map.token_for(33, "creation-frameless"),
        unreadable_frameless
    );
    // Fresh unsorted ids still sort deterministically for summaries.
    let mut summary = vec![
        ("w9".to_owned(), "refused-intent".to_owned()),
        ("w1".to_owned(), "minimized".to_owned()),
    ];
    summary.sort();
    let signature = tick_summary_signature(3, 0, &summary, 0, "reconcile", 7);
    assert!(signature.contains("w1=minimized,w9=refused-intent"));
    // Closed windows drop: a re-minted token differs from the stale one.
    map.retain(&[ObservedTargetRef {
        hwnd: 11,
        creation: "creation-eligible",
    }]);
    assert_ne!(map.token_for(22, "creation-minimized"), minimized);
    assert_eq!(map.token_for(11, "creation-eligible"), eligible);
}

#[test]
fn workspace_cli_parses_select_and_send_exclusively() {
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    let select = parse_workspace_args(&strings(&["--select", "2"])).expect("parsed");
    assert_eq!(select.action, WorkspaceAction::Select);
    assert_eq!(select.index, 2);
    assert!(verify_workspace_argv_consistency(&strings(&["--select", "2"]), &select).is_ok());
    let send = parse_workspace_args(&strings(&["--send", "2"])).expect("parsed");
    assert_eq!(send.action, WorkspaceAction::Send);
    assert_eq!(send.index, 2);
    assert!(verify_workspace_argv_consistency(&strings(&["--send", "2"]), &send).is_ok());
    // Exclusivity: exactly one flag, one digit, no mixing.
    assert!(parse_workspace_args(&[]).is_err());
    assert!(parse_workspace_args(&strings(&["--select"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send"])).is_err());
    assert!(parse_workspace_args(&strings(&["--select", "10"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send", "10"])).is_err());
    assert!(parse_workspace_args(&strings(&["--select", "2", "--select", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send", "2", "--send", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--select", "1", "--send", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send", "1", "--select", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--other", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--select", "1", "extra"])).is_err());
    let ok0 = parse_workspace_args(&strings(&["--select", "0"])).expect("zero");
    assert_eq!(ok0.index, 0);
    let ok9 = parse_workspace_args(&strings(&["--send", "9"])).expect("nine");
    assert_eq!(ok9.index, 9);
    // Consistency binds the flag and the value: cross-flag and cross-value
    // bodies refuse.
    assert!(verify_workspace_argv_consistency(&strings(&["--select", "1"]), &select).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--send", "2"]), &select).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--select", "2"]), &send).is_err());
}

#[test]
fn workspace_request_roundtrip_and_refusals() {
    let request = WorkspaceRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        action: WorkspaceAction::Send,
        index: 2,
        direction: None,
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_workspace_request(&request);
    assert!(body.contains("abc123") && body.contains("cli-4242-ab12"));
    assert!(body.contains("send"));
    assert!(!body.contains("hwnd"));
    let parsed = parse_workspace_request(&body).expect("roundtrip");
    assert_eq!(parsed, request);
    let select = WorkspaceRequest {
        action: WorkspaceAction::Select,
        ..request.clone()
    };
    let parsed = parse_workspace_request(&render_workspace_request(&select)).expect("select");
    assert_eq!(parsed.action, WorkspaceAction::Select);
    for action in [WorkspaceAction::Float, WorkspaceAction::Sticky] {
        let routed = WorkspaceRequest {
            action,
            ..request.clone()
        };
        let body = render_workspace_request(&routed);
        assert!(body.contains(action.as_str()));
        assert!(!body.contains("hwnd"));
        let parsed = parse_workspace_request(&body).expect("float/sticky roundtrip");
        assert_eq!(parsed, routed);
        // A direction on an immediate toggle refuses: the transport carries
        // exactly the grammar, never a default.
        let mut bad_direction = routed.clone();
        bad_direction.direction = Some(tiler_windows::tiling::WorkspaceDirection::Next);
        assert!(parse_workspace_request(&render_workspace_request(&bad_direction)).is_err());
    }
    let mut bad_version = request.clone();
    bad_version.v = 2;
    assert!(parse_workspace_request(&render_workspace_request(&bad_version)).is_err());
    let mut bad_index = request.clone();
    bad_index.index = 10;
    assert!(parse_workspace_request(&render_workspace_request(&bad_index)).is_err());
    let mut bad_corr = request.clone();
    bad_corr.correlation = "bad corr".to_owned();
    assert!(parse_workspace_request(&render_workspace_request(&bad_corr)).is_err());
    let mut bad_owner = request.clone();
    bad_owner.creation = String::new();
    assert!(parse_workspace_request(&render_workspace_request(&bad_owner)).is_err());
    // Missing action (old select-only body) and unknown action refuse. A
    // missing direction on a relative action refuses, and a direction on an
    // indexed/immediate action refuses: the transport carries exactly the
    // grammar above, never a default.
    let legacy = serde_json::json!({
        "v": 1, "creation": "abc123", "pid": 4242,
        "exe_path": "C:\\bin\\tiler-windows.exe", "user_sid": "S-1-5-21-1",
        "session_id": 1, "index": 2, "correlation": "cli-4242-ab12",
    })
    .to_string();
    assert!(parse_workspace_request(&legacy).is_err());
    let unknown = serde_json::json!({
        "v": 1, "creation": "abc123", "pid": 4242,
        "exe_path": "C:\\bin\\tiler-windows.exe", "user_sid": "S-1-5-21-1",
        "session_id": 1, "action": "focus", "index": 2, "correlation": "cli-4242-ab12",
    })
    .to_string();
    assert!(parse_workspace_request(&unknown).is_err());
    assert!(parse_workspace_request("not json").is_err());
}

#[test]
fn workspace_cli_parses_stay_and_relative_forms() {
    use tiler_windows::tiling::WorkspaceDirection;
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    // Old `--send` keeps following; `--stay` stays; relative forms carry an
    // explicit direction with the same follow/stay split.
    let send = parse_workspace_args(&strings(&["--send", "2"])).expect("send");
    assert_eq!(send.action, WorkspaceAction::Send);
    assert_eq!(send.action.follow(), Some(true));
    assert!(send.action.is_send());
    let stay = parse_workspace_args(&strings(&["--stay", "2"])).expect("stay");
    assert_eq!(stay.action, WorkspaceAction::Stay);
    assert_eq!(stay.index, 2);
    assert_eq!(stay.direction, None);
    assert_eq!(stay.action.follow(), Some(false));
    assert!(stay.action.is_send());
    assert!(verify_workspace_argv_consistency(&strings(&["--stay", "2"]), &stay).is_ok());
    for (flag, action, follow) in [
        ("--send-relative", WorkspaceAction::RelativeSend, Some(true)),
        (
            "--stay-relative",
            WorkspaceAction::RelativeStay,
            Some(false),
        ),
        ("--relative", WorkspaceAction::RelativeHistory, None),
    ] {
        for (word, direction, delta) in [
            ("previous", WorkspaceDirection::Previous, -1),
            ("next", WorkspaceDirection::Next, 1),
        ] {
            let parsed = parse_workspace_args(&strings(&[flag, word])).expect("relative form");
            assert_eq!(parsed.action, action);
            assert_eq!(parsed.direction, Some(direction));
            assert_eq!(direction.delta(), delta);
            assert_eq!(parsed.action.follow(), follow);
            assert_eq!(parsed.action.is_send(), follow.is_some());
            assert!(verify_workspace_argv_consistency(&strings(&[flag, word]), &parsed).is_ok());
        }
    }
    let previous = parse_workspace_args(&strings(&["--previous"])).expect("previous");
    assert_eq!(previous.action, WorkspaceAction::Previous);
    assert_eq!(previous.direction, None);
    assert_eq!(previous.action.follow(), None);
    assert!(!previous.action.is_send());
    assert!(verify_workspace_argv_consistency(&strings(&["--previous"]), &previous).is_ok());
    // Direction vocabulary roundtrips through as_str.
    assert_eq!(WorkspaceDirection::Previous.as_str(), "previous");
    assert_eq!(WorkspaceDirection::Next.as_str(), "next");
    assert_eq!(WorkspaceAction::Stay.as_str(), "stay");
    assert_eq!(WorkspaceAction::Previous.as_str(), "previous");
    assert_eq!(WorkspaceAction::RelativeHistory.as_str(), "relative");
    assert_eq!(WorkspaceAction::RelativeSend.as_str(), "send-relative");
    assert_eq!(WorkspaceAction::RelativeStay.as_str(), "stay-relative");
    // Refusals: missing/misplaced values, mixing, unknown words.
    assert!(parse_workspace_args(&strings(&["--stay"])).is_err());
    assert!(parse_workspace_args(&strings(&["--stay", "10"])).is_err());
    assert!(parse_workspace_args(&strings(&["--previous", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--relative"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send-relative"])).is_err());
    assert!(parse_workspace_args(&strings(&["--stay-relative", "up"])).is_err());
    assert!(parse_workspace_args(&strings(&["--relative", "2"])).is_err());
    assert!(parse_workspace_args(&strings(&["--send", "1", "--stay", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--relative", "next", "--previous"])).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--stay", "1"]), &stay).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--send", "2"]), &stay).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--previous", "x"]), &previous).is_err());
}

#[test]
fn workspace_cli_parses_float_and_sticky() {
    // Test-needed exact-owner float/sticky routes (tentative pending user
    // review): no-value flags like --fullscreen, never a send, never follow.
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    let float = parse_workspace_args(&strings(&["--float"])).expect("float");
    assert_eq!(float.action, WorkspaceAction::Float);
    assert_eq!(float.action.as_str(), "float");
    assert_eq!(float.direction, None);
    assert_eq!(float.action.follow(), None);
    assert!(!float.action.is_send());
    assert!(verify_workspace_argv_consistency(&strings(&["--float"]), &float).is_ok());
    let sticky = parse_workspace_args(&strings(&["--sticky"])).expect("sticky");
    assert_eq!(sticky.action, WorkspaceAction::Sticky);
    assert_eq!(sticky.action.as_str(), "sticky");
    assert_eq!(sticky.direction, None);
    assert_eq!(sticky.action.follow(), None);
    assert!(!sticky.action.is_send());
    assert!(verify_workspace_argv_consistency(&strings(&["--sticky"]), &sticky).is_ok());
    // Refusals: values, mixing, unknown flags, cross-flag consistency.
    assert!(parse_workspace_args(&strings(&["--float", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--sticky", "next"])).is_err());
    assert!(parse_workspace_args(&strings(&["--float", "--sticky"])).is_err());
    assert!(parse_workspace_args(&strings(&["--sticky", "--float"])).is_err());
    assert!(parse_workspace_args(&strings(&["--float", "--fullscreen"])).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--sticky"]), &float).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--float"]), &sticky).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--float", "x"]), &float).is_err());
}

#[test]
fn workspace_request_roundtrip_and_refusals_relative() {
    use tiler_windows::tiling::WorkspaceDirection;
    let request = WorkspaceRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        action: WorkspaceAction::RelativeStay,
        index: 0,
        direction: Some(WorkspaceDirection::Next),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_workspace_request(&request);
    assert!(body.contains("stay-relative") && body.contains("next"));
    let parsed = parse_workspace_request(&body).expect("roundtrip");
    assert_eq!(parsed, request);
    assert_eq!(parsed.direction.map(|d| d.delta()), Some(1));
    for (action, direction) in [
        (
            WorkspaceAction::RelativeSend,
            Some(WorkspaceDirection::Next),
        ),
        (
            WorkspaceAction::RelativeHistory,
            Some(WorkspaceDirection::Previous),
        ),
        (WorkspaceAction::Previous, None),
        (WorkspaceAction::Stay, None),
    ] {
        let variant = WorkspaceRequest {
            action,
            direction,
            ..request.clone()
        };
        let parsed = parse_workspace_request(&render_workspace_request(&variant)).expect("variant");
        assert_eq!(parsed.action, action);
        assert_eq!(parsed.direction, direction);
    }
    // Missing direction on a relative action refuses; a direction on an
    // indexed/immediate action refuses.
    let mut missing = request.clone();
    missing.direction = None;
    assert!(parse_workspace_request(&render_workspace_request(&missing)).is_err());
    let mut misplaced = request.clone();
    misplaced.action = WorkspaceAction::Send;
    assert!(parse_workspace_request(&render_workspace_request(&misplaced)).is_err());
    let mut misplaced_previous = request.clone();
    misplaced_previous.action = WorkspaceAction::Previous;
    assert!(parse_workspace_request(&render_workspace_request(&misplaced_previous)).is_err());
    // Unknown action and unknown direction refuse; legacy bodies without the
    // direction field still parse for non-relative actions (serde default).
    let unknown_direction = serde_json::json!({
        "v": 1, "creation": "abc123", "pid": 4242,
        "exe_path": "C:\\bin\\tiler-windows.exe", "user_sid": "S-1-5-21-1",
        "session_id": 1, "action": "send-relative", "index": 0,
        "direction": "up", "correlation": "cli-4242-ab12",
    })
    .to_string();
    assert!(parse_workspace_request(&unknown_direction).is_err());
    let legacy_send = serde_json::json!({
        "v": 1, "creation": "abc123", "pid": 4242,
        "exe_path": "C:\\bin\\tiler-windows.exe", "user_sid": "S-1-5-21-1",
        "session_id": 1, "action": "send", "index": 2, "correlation": "cli-4242-ab12",
    })
    .to_string();
    let parsed = parse_workspace_request(&legacy_send).expect("legacy send parses");
    assert_eq!(parsed.direction, None);
}

#[cfg(windows)]
#[test]
fn live_overlay_flags_on_a_null_handle_fall_back_without_touching_windows() {
    // Item 20 live-read contract: three bounded reads, no setters. A null
    // handle reads unmaximized with an unreadable frame (no crash, no
    // fabricated fullscreen), so production falls back to the retained row
    // facts. This pins the fallback shape only; real maximized/fullscreen
    // legs stay user-owned live.
    use tiler_core::geometry::Rect;
    let fulls = [Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    }];
    assert_eq!(
        tiler_windows::tiling_sys::live_overlay_flags(0, &fulls),
        (false, None)
    );
}

#[test]
fn default_seeded_track_converts_and_zero_bounds_ride_unset_per_axis() {
    // A correctly seeded system-default track (positive, in-bound) converts
    // 1:1 with zero insets: an app that leaves the seeded default untouched
    // reports a usable hint, never a fabricated absence. A zero return (query
    // failure) stays unknown: the native path ignores the struct entirely.
    // Explicit zero bounds ride as unset per axis (G-05), never as unknown:
    // per-axis normalization keeps the usable sibling and transports the
    // zero so the shared predicate tells partial-zero from missing.
    let zero = FrameInsets {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    assert_eq!(visible_min_from_outer(160, 40, zero), (Some(160), Some(40)));
    assert_eq!(normalize_min_track(160, 40), (Some(160), Some(40)));
    assert_eq!(visible_min_from_outer(0, 0, zero), (Some(0), Some(0)));
    let hints = min_hints_from_outer(0, 0, zero);
    assert_eq!((hints.min_w, hints.min_h), (Some(0), Some(0)));
    assert_eq!((hints.max_w, hints.max_h), (None, None));
}

#[test]
fn min_track_normalization_rejects_invalid_per_axis() {
    // Valid outer track sizes pass through untouched.
    assert_eq!(normalize_min_track(880, 625), (Some(880), Some(625)));
    assert_eq!(normalize_min_track(1, 1), (Some(1), Some(1)));
    assert_eq!(
        normalize_min_track(16384, 16384),
        (Some(16384), Some(16384))
    );
    // Explicit zeros ride as unset per axis (G-05), never discarded.
    assert_eq!(normalize_min_track(0, 625), (Some(0), Some(625)));
    assert_eq!(normalize_min_track(880, 0), (Some(880), Some(0)));
    // Negative, sentinel, and absurd values mean unknown on that axis only:
    // the usable sibling still converts.
    assert_eq!(normalize_min_track(-8, 625), (None, Some(625)));
    assert_eq!(normalize_min_track(880, -8), (Some(880), None));
    assert_eq!(normalize_min_track(16385, 625), (None, Some(625)));
    assert_eq!(normalize_min_track(880, i32::MAX), (Some(880), None));
}

#[test]
fn visible_min_conversion_matches_known_apps() {
    // Paint: outer minimum 880x625 with an 8/4/8/4 frame converts to visible
    // 864x617 physical pixels.
    let paint = FrameInsets {
        left: 8,
        top: 4,
        right: 8,
        bottom: 4,
    };
    assert_eq!(
        visible_min_from_outer(880, 625, paint),
        (Some(864), Some(617))
    );
    let hints = min_hints_from_outer(880, 625, paint);
    assert_eq!(hints.min_w, Some(864));
    assert_eq!(hints.min_h, Some(617));
    assert_eq!(hints.max_w, None);
    assert_eq!(hints.max_h, None);
    // Notepad: outer minimum 415x253 with a 7/3/7/4 frame converts to visible
    // 401x246 physical pixels.
    let notepad = FrameInsets {
        left: 7,
        top: 3,
        right: 7,
        bottom: 4,
    };
    assert_eq!(
        visible_min_from_outer(415, 253, notepad),
        (Some(401), Some(246))
    );
    let hints = min_hints_from_outer(415, 253, notepad);
    assert_eq!(hints.min_w, Some(401));
    assert_eq!(hints.min_h, Some(246));
}

#[test]
fn visible_min_unknown_axes_convert_independently() {
    let insets = FrameInsets {
        left: 8,
        top: 4,
        right: 8,
        bottom: 4,
    };
    // An explicit-zero axis rides unset while its usable sibling converts;
    // only fully unknown pairs stay empty.
    assert_eq!(visible_min_from_outer(0, 625, insets), (Some(0), Some(617)));
    let hints = min_hints_from_outer(0, 625, insets);
    assert_eq!((hints.min_w, hints.min_h), (Some(0), Some(617)));
    // A minimum that vanishes inside its own frame carries no usable visible
    // constraint on that axis: unknown, never a zero or negative floor. The
    // usable sibling axis still converts.
    assert_eq!(visible_min_from_outer(16, 8, insets), (None, None));
    assert_eq!(visible_min_from_outer(10, 4, insets), (None, None));
    assert!(min_hints_from_outer(16, 8, insets).is_empty());
}

#[test]
fn underlay_defaults_match_kde_parity() {
    let options = parse_tile_args(&strings(&["--user-start"])).expect("parsed");
    assert!(options.underlay.enabled);
    assert_eq!(options.underlay.style.color, (0x2a, 0x82, 0xda));
    assert_eq!(options.underlay.style.alpha, 0x66);
    assert_eq!(options.underlay.style.extension, -1.0);
    let options = parse_tile_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert!(options.underlay.enabled);
    assert_eq!(options.underlay.style.extension, -1.0);
    let options = parse_workspace_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert!(options.underlay.enabled);
    assert_eq!(options.underlay.style.extension, -1.0);
    let options = parse_shortcut_proof_args(&strings(&["--allowlist", "a.json"])).expect("parsed");
    assert!(options.underlay.enabled);
}

#[test]
fn underlay_flags_parse_and_refuse() {
    let options = parse_tile_args(&strings(&[
        "--user-start",
        "--group-underlay-color",
        "#662a82da",
        "--group-underlay-extension",
        "5",
    ]))
    .expect("parsed");
    assert_eq!(options.underlay.style.color, (0x2a, 0x82, 0xda));
    assert_eq!(options.underlay.style.alpha, 0x66);
    assert_eq!(options.underlay.style.extension, 5.0);
    // Superseded default and dogfood values stay valid explicit overrides.
    for (hex, rgb, alpha) in [
        ("#40808080", (0x80, 0x80, 0x80), 0x40),
        ("#50909090", (0x90, 0x90, 0x90), 0x50),
    ] {
        let options = parse_tile_args(&strings(&["--user-start", "--group-underlay-color", hex]))
            .expect("explicit parses");
        assert_eq!(options.underlay.style.color, rgb, "{hex}");
        assert_eq!(options.underlay.style.alpha, alpha, "{hex}");
    }
    let options =
        parse_tile_args(&strings(&["--user-start", "--no-group-underlay"])).expect("parsed");
    assert!(!options.underlay.enabled);
    // Six-digit colour (missing alpha) refuses, never silently opaque.
    assert!(
        parse_tile_args(&strings(&[
            "--user-start",
            "--group-underlay-color",
            "#808080"
        ]))
        .is_err()
    );
    assert!(
        parse_tile_args(&strings(&[
            "--user-start",
            "--group-underlay-color",
            "#40808080",
            "--group-underlay-extension",
            "-2"
        ]))
        .is_err()
    );
    assert!(parse_tile_args(&strings(&["--user-start", "--group-underlay-color"])).is_err());
    assert!(parse_tile_args(&strings(&["--user-start", "--group-underlaybogus"])).is_err());
}

#[test]
fn proof_argv_consistency_evidences_underlay_flags() {
    // Full delivery with underlay flags verifies on every proof command.
    let raw = strings(&[
        "--allowlist",
        "a.json",
        "--trace",
        "--no-group-underlay",
        "--group-underlay-color",
        "#40808080",
        "--group-underlay-extension",
        "5",
    ]);
    let parsed = parse_tile_proof_args(&raw).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw, &parsed).is_ok());
    let parsed = parse_workspace_proof_args(&raw).expect("parsed");
    assert!(verify_workspace_proof_argv_consistency(&raw, &parsed).is_ok());
    let parsed = parse_shortcut_proof_args(&raw).expect("parsed");
    assert!(verify_shortcut_proof_argv_consistency(&raw, &parsed).is_ok());
    // Dropped underlay flags on either side are impossible errors.
    let raw_bare = strings(&["--allowlist", "a.json", "--trace"]);
    let parsed_bare = parse_tile_proof_args(&raw_bare).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw, &parsed_bare).is_err());
    let parsed_tile = parse_tile_proof_args(&raw).expect("parsed");
    assert!(verify_proof_argv_consistency(&raw_bare, &parsed_tile).is_err());
    // Swapped colour value is an impossible error, never a silent retarget.
    let raw_other = strings(&[
        "--allowlist",
        "a.json",
        "--trace",
        "--group-underlay-color",
        "#ff112233",
    ]);
    assert!(verify_proof_argv_consistency(&raw_other, &parsed_tile).is_err());
}

#[test]
fn underlay_saved_and_cli_colors_survive_new_default() {
    // Saved base without a CLI switch keeps its explicit color, including
    // the superseded default and the dogfood value; an explicit CLI switch
    // overrides the saved base and stays authoritative on re-apply.
    for (saved, rgb, alpha) in [
        ("#40808080", (0x80, 0x80, 0x80), 0x40),
        ("#50909090", (0x90, 0x90, 0x90), 0x50),
        ("#662a82da", (0x2a, 0x82, 0xda), 0x66),
    ] {
        let mut settings = tiler_windows::settings::Settings::default();
        settings.core.underlay.color = saved.to_owned();
        let base = tiler_windows::settings::tile_options_from_settings(&settings);
        assert_eq!(base.underlay.style.color, rgb, "{saved}");
        assert_eq!(base.underlay.style.alpha, alpha, "{saved}");
        let parsed =
            tiler_windows::tiling::parse_tile_args_from(&strings(&["--user-start"]), &base)
                .expect("parsed");
        assert_eq!(parsed.underlay.style.color, rgb, "{saved}");
        assert_eq!(parsed.underlay.style.alpha, alpha, "{saved}");
    }
    let mut settings = tiler_windows::settings::Settings::default();
    settings.core.underlay.color = "#40808080".to_owned();
    let base = tiler_windows::settings::tile_options_from_settings(&settings);
    let (parsed, cli) = tiler_windows::tiling::parse_tile_args_from_with_overrides(
        &strings(&["--user-start", "--group-underlay-color", "#50909090"]),
        &base,
    )
    .expect("parsed");
    assert_eq!(parsed.underlay.style.color, (0x90, 0x90, 0x90));
    assert_eq!(parsed.underlay.style.alpha, 0x50);
    let mut rebased = tiler_windows::settings::tile_options_from_settings(&settings);
    tiler_windows::tiling::apply_cli_overrides(&mut rebased, &cli);
    assert_eq!(rebased.underlay.style.color, (0x90, 0x90, 0x90));
    assert_eq!(rebased.underlay.style.alpha, 0x50);
}

#[test]
fn born_hold_never_reborn_after_first_exit() {
    // A slotted member's later fullscreen is a managed overlay transition,
    // and a lifetime-known non-fullscreen window never becomes born again:
    // only the first-seen slotless fullscreen holds.
    assert!(should_hold_born_fullscreen(true, false, false));
    assert!(!should_hold_born_fullscreen(true, true, false));
    assert!(!should_hold_born_fullscreen(true, false, true));
    // A non-fullscreen window never holds, even slotless and unseen.
    assert!(!should_hold_born_fullscreen(false, false, false));
}

#[test]
fn retained_maximized_hint_keeps_min_bound_strip_stable() {
    // Bug fixture: 2544-wide equal-share strip, minimums 401/864/627/582.
    // The shared projector funds the 864 minimum from sibling slack:
    // 456/864/629/595. Dropping the maximized member's hint (hintless
    // retained row) collapses to 636 all and siblings jump; reusing the
    // last-known hint keeps the min-bound projection stable.
    use tiler_core::directional::{Axis, Node, NodeId};
    use tiler_core::size_hints::{WindowSizeHints, project_with_hints};
    use tiler_windows::tiling::{RESTORE_WAKE_MS, retained_overlay_hint};
    let tree = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId::from("a"),
            },
            Node::Leaf {
                id: NodeId::from("b"),
            },
            Node::Leaf {
                id: NodeId::from("c"),
            },
            Node::Leaf {
                id: NodeId::from("d"),
            },
        ],
        shares: vec![1, 1, 1, 1],
    };
    let bounds = rect(0, 0, 2544, 1364);
    let hinted = |min_w: Option<i32>| WindowSizeHints {
        min_w,
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let full = project_with_hints(&tree, bounds, 0, &|leaf: &NodeId| match leaf.0.as_str() {
        "a" => hinted(Some(401)),
        "b" => hinted(Some(864)),
        "c" => hinted(Some(627)),
        "d" => hinted(Some(582)),
        _ => WindowSizeHints::none(),
    })
    .expect("hinted strip projects");
    let widths: Vec<i32> = full.leaves.iter().map(|leaf| leaf.rect.w).collect();
    assert_eq!(widths, vec![456, 864, 629, 595]);
    // Hintless retained row for the maximized 864 member: siblings collapse.
    let dropped = project_with_hints(&tree, bounds, 0, &|leaf: &NodeId| match leaf.0.as_str() {
        "a" => hinted(Some(401)),
        "b" => WindowSizeHints::none(),
        "c" => hinted(Some(627)),
        "d" => hinted(Some(582)),
        _ => WindowSizeHints::none(),
    })
    .expect("hintless strip projects");
    let flat: Vec<i32> = dropped.leaves.iter().map(|leaf| leaf.rect.w).collect();
    assert_eq!(flat, vec![636, 636, 636, 636]);
    assert_ne!(widths, flat, "dropped hint moves siblings");
    // Retained reuse restores the declared hint: same Engine input, stable.
    let reused = retained_overlay_hint(Some(hinted(Some(864))), true, true, false, false, true);
    assert!(!reused.is_empty());
    let stable = project_with_hints(&tree, bounds, 0, &|leaf: &NodeId| match leaf.0.as_str() {
        "a" => hinted(Some(401)),
        "b" => reused,
        "c" => hinted(Some(627)),
        "d" => hinted(Some(582)),
        _ => WindowSizeHints::none(),
    })
    .expect("retained strip projects");
    let kept: Vec<i32> = stable.leaves.iter().map(|leaf| leaf.rect.w).collect();
    assert_eq!(kept, widths, "retained hint keeps siblings stable");
    // Lifetime gates never reuse: gone identity, float, born hold, slotless,
    // non-overlay, and empty cache all stay hintless.
    assert!(
        retained_overlay_hint(Some(hinted(Some(864))), false, true, false, false, true).is_empty()
    );
    assert!(
        retained_overlay_hint(Some(hinted(Some(864))), true, true, true, false, true).is_empty()
    );
    assert!(
        retained_overlay_hint(Some(hinted(Some(864))), true, true, false, true, true).is_empty()
    );
    assert!(
        retained_overlay_hint(Some(hinted(Some(864))), true, true, false, false, false).is_empty()
    );
    assert!(
        retained_overlay_hint(Some(hinted(Some(864))), true, false, false, false, true).is_empty()
    );
    assert!(retained_overlay_hint(None, true, true, false, false, true).is_empty());
    assert!(
        retained_overlay_hint(
            Some(WindowSizeHints::none()),
            true,
            true,
            false,
            false,
            true
        )
        .is_empty()
    );
    assert_eq!(RESTORE_WAKE_MS, 2000);
}

#[test]
fn restore_wake_survives_pending_dispatch_until_reconcile() {
    use tiler_windows::tiling::{restore_wake_arm, restore_wake_step};
    assert!(restore_wake_arm("dispatched", true));
    assert!(!restore_wake_arm("restored", true));
    assert!(!restore_wake_arm("threw", true));
    assert!(!restore_wake_arm("dispatched", false));
    // Still pending: keep waiting, no demand.
    assert_eq!(
        restore_wake_step(true, false, false, false, false),
        (true, false)
    );
    // Observed but a key-up batch routes to dispatch instead: survive, demand.
    assert_eq!(
        restore_wake_step(true, false, false, true, false),
        (true, true)
    );
    // Next pump with no pending intents reconciles: consume.
    assert_eq!(
        restore_wake_step(true, false, false, true, true),
        (false, true)
    );
    // Gesture/suspend pauses defer the same way: survive, demand.
    assert_eq!(
        restore_wake_step(true, false, false, true, false),
        (true, true)
    );
    // Expiry, window loss, and identity loss clear without demand.
    assert_eq!(
        restore_wake_step(true, true, false, true, false),
        (false, false)
    );
    assert_eq!(
        restore_wake_step(true, false, true, true, false),
        (false, false)
    );
    assert_eq!(
        restore_wake_step(true, true, false, false, false),
        (false, false)
    );
    assert_eq!(
        restore_wake_step(false, false, false, true, true),
        (false, false)
    );
}

#[test]
fn workspace_cli_parses_fullscreen_toggle_without_move() {
    // Item 9 test-needed out-of-hook route: `workspace --fullscreen` toggles
    // the focused managed window through the existing workspace CLI request
    // transport with no workspace move, no carried HWND, and no new IPC.
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    let parsed = parse_workspace_args(&strings(&["--fullscreen"])).expect("fullscreen parses");
    assert_eq!(parsed.action, WorkspaceAction::Fullscreen);
    assert_eq!(parsed.action.as_str(), "fullscreen");
    assert_eq!(parsed.index, 0);
    assert_eq!(parsed.direction, None);
    // Normal-only transport fence: fullscreen is neither a send (no mover
    // transfer, no follow intent) nor a select/history move, so the owner
    // routes it to the fullscreen toggle branch only.
    assert!(!parsed.action.is_send());
    assert_eq!(parsed.action.follow(), None);
    assert!(verify_workspace_argv_consistency(&strings(&["--fullscreen"]), &parsed).is_ok());
    // Exclusivity and arity: exactly one bare flag, no mixing, no value.
    assert!(parse_workspace_args(&strings(&["--fullscreen", "1"])).is_err());
    assert!(parse_workspace_args(&strings(&["--fullscreen", "next"])).is_err());
    assert!(parse_workspace_args(&strings(&["--select", "1", "--fullscreen"])).is_err());
    assert!(parse_workspace_args(&strings(&["--fullscreen", "--fullscreen"])).is_err());
    assert!(parse_workspace_args(&strings(&["--previous", "--fullscreen"])).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--previous"]), &parsed).is_err());
    assert!(verify_workspace_argv_consistency(&strings(&["--fullscreen", "1"]), &parsed).is_err());
    // Transport roundtrip carries exactly the grammar (no HWND/geometry), and
    // a misplaced direction refuses: the request body never steers the target.
    let request = WorkspaceRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        action: WorkspaceAction::Fullscreen,
        index: 0,
        direction: None,
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_workspace_request(&request);
    assert!(body.contains("fullscreen"));
    assert!(!body.contains("hwnd"));
    assert_eq!(parse_workspace_request(&body).expect("roundtrip"), request);
    let mut misplaced = request.clone();
    misplaced.direction = Some(tiler_windows::tiling::WorkspaceDirection::Next);
    assert!(parse_workspace_request(&render_workspace_request(&misplaced)).is_err());
    // Route reuses the production toggle authority verbatim (no new decision):
    // normal frames enter, owned frames exit, app-owned frames refuse
    // (R-MAX-05), and suspended/elevated intents settle with no effect.
    assert_eq!(
        fullscreen_toggle_decision(false, false),
        FullscreenToggle::Enter
    );
    assert_eq!(
        fullscreen_toggle_decision(true, true),
        FullscreenToggle::ExitOwned
    );
    assert_eq!(
        fullscreen_toggle_decision(true, false),
        FullscreenToggle::RefuseAppOwned
    );
    assert!(toggle_gate_outcome(true, false).is_some());
    assert!(toggle_gate_outcome(false, true).is_some());
    assert_eq!(toggle_gate_outcome(false, false), None);
}

#[test]
fn resize_cli_parses_direction_mode_without_hwnd() {
    // Test-needed out-of-hook route (tentative): `resize --direction DIR
    // --mode MODE` queues one bounded request through the existing
    // exact-owner request transport with no carried HWND and no new IPC.
    // The owner resolves the live foreground and dispatches through the real
    // `keyboard_tick` resize arm.
    let parsed = parse_resize_args(&strings(&["--direction", "left", "--mode", "outwards"]))
        .expect("resize parses");
    assert_eq!(
        parsed,
        ResizeOptions {
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
        }
    );
    assert!(
        verify_resize_argv_consistency(
            &strings(&["--direction", "left", "--mode", "outwards"]),
            &parsed
        )
        .is_ok()
    );
    // Arity and order: exactly `--direction DIR --mode MODE`, no mixing, no
    // extras, no swapped order.
    assert!(parse_resize_args(&strings(&["--direction", "left"])).is_err());
    assert!(parse_resize_args(&strings(&["--mode", "outwards"])).is_err());
    assert!(parse_resize_args(&strings(&["--mode", "outwards", "--direction", "left"])).is_err());
    assert!(
        parse_resize_args(&strings(&[
            "--direction",
            "left",
            "--mode",
            "outwards",
            "--trace"
        ]))
        .is_err()
    );
    assert!(parse_resize_args(&strings(&["--direction", "north", "--mode", "outwards"])).is_err());
    assert!(parse_resize_args(&strings(&["--direction", "left", "--mode", "sideways"])).is_err());
    assert!(
        verify_resize_argv_consistency(
            &strings(&["--direction", "right", "--mode", "outwards"]),
            &parsed
        )
        .is_err()
    );
    // Validators pin the Engine wire vocabulary.
    assert!(resize_direction_valid("up"));
    assert!(!resize_direction_valid("north"));
    assert!(resize_mode_valid("inwards"));
    assert!(!resize_mode_valid("sideways"));
    // Transport roundtrip carries exactly the grammar (no HWND/geometry);
    // unknown direction/mode, version drift, and empty correlation refuse.
    let request = ResizeRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_resize_request(&request);
    assert!(body.contains("left"));
    assert!(!body.contains("hwnd"));
    assert_eq!(parse_resize_request(&body).expect("roundtrip"), request);
    let mut bad = request.clone();
    bad.direction = "north".to_owned();
    assert!(parse_resize_request(&render_resize_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.mode = "sideways".to_owned();
    assert!(parse_resize_request(&render_resize_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.v = 2;
    assert!(parse_resize_request(&render_resize_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.correlation.clear();
    assert!(parse_resize_request(&render_resize_request(&bad)).is_err());
    assert!(parse_resize_request("not json").is_err());
}

#[test]
fn directional_cli_parses_op_direction_without_hwnd() {
    // Test-needed out-of-hook route (tentative): `directional (--move|--focus)
    // --direction DIR` queues one bounded request through the existing
    // exact-owner request transport with no carried HWND and no new IPC.
    // The owner resolves the live foreground and dispatches through the real
    // `keyboard_tick` directional arm.
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    let moved =
        parse_directional_args(&strings(&["--move", "--direction", "left"])).expect("move parses");
    assert_eq!(moved.op, DirectionalOp::Move);
    assert_eq!(moved.op.as_str(), "move");
    assert_eq!(moved.direction, "left");
    assert!(
        verify_directional_argv_consistency(&strings(&["--move", "--direction", "left"]), &moved)
            .is_ok()
    );
    let focus = parse_directional_args(&strings(&["--focus", "--direction", "down"]))
        .expect("focus parses");
    assert_eq!(focus.op, DirectionalOp::Focus);
    assert_eq!(focus.op.as_str(), "focus");
    assert_eq!(focus.direction, "down");
    assert!(
        verify_directional_argv_consistency(&strings(&["--focus", "--direction", "down"]), &focus)
            .is_ok()
    );
    // Arity and order: exactly `(--move|--focus) --direction DIR`, no
    // mixing, no extras, no swapped order.
    assert!(parse_directional_args(&strings(&["--move"])).is_err());
    assert!(parse_directional_args(&strings(&["--direction", "left"])).is_err());
    assert!(parse_directional_args(&strings(&["--move", "--direction"])).is_err());
    assert!(parse_directional_args(&strings(&["--direction", "left", "--move"])).is_err());
    assert!(
        parse_directional_args(&strings(&["--move", "--direction", "left", "--trace"])).is_err()
    );
    assert!(parse_directional_args(&strings(&["--zoom", "--direction", "left"])).is_err());
    assert!(parse_directional_args(&strings(&["--move", "--direction", "north"])).is_err());
    assert!(parse_directional_args(&strings(&["--move", "--mode", "left"])).is_err());
    assert!(
        parse_directional_args(&strings(&["--move", "--direction", "left", "--focus"])).is_err()
    );
    assert!(
        verify_directional_argv_consistency(&strings(&["--focus", "--direction", "left"]), &moved)
            .is_err()
    );
    assert!(
        verify_directional_argv_consistency(&strings(&["--move", "--direction", "right"]), &moved)
            .is_err()
    );
    // Validators reuse the Engine direction vocabulary shared with resize.
    assert!(resize_direction_valid("up"));
    assert!(!resize_direction_valid("north"));
}

#[test]
fn directional_request_roundtrip_and_refusals() {
    let request = DirectionalRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        op: DirectionalOp::Move,
        direction: "left".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_directional_request(&request);
    assert!(body.contains("move"));
    assert!(body.contains("left"));
    assert!(!body.contains("hwnd"));
    assert_eq!(
        parse_directional_request(&body).expect("roundtrip"),
        request
    );
    let focus = DirectionalRequest {
        op: DirectionalOp::Focus,
        direction: "down".to_owned(),
        ..request.clone()
    };
    let body = render_directional_request(&focus);
    assert!(body.contains("focus") && body.contains("down"));
    assert_eq!(
        parse_directional_request(&body).expect("focus roundtrip"),
        focus
    );
    let mut bad = request.clone();
    bad.direction = "north".to_owned();
    assert!(parse_directional_request(&render_directional_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.v = 2;
    assert!(parse_directional_request(&render_directional_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.correlation.clear();
    assert!(parse_directional_request(&render_directional_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.creation.clear();
    assert!(parse_directional_request(&render_directional_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.pid = 0;
    assert!(parse_directional_request(&render_directional_request(&bad)).is_err());
    // Unknown op (a future transport action) refuses; the transport carries
    // exactly the move/focus grammar, never a default.
    let unknown = serde_json::json!({
        "v": 1, "creation": "abc123", "pid": 4242,
        "exe_path": "C:\\bin\\tiler-windows.exe", "user_sid": "S-1-5-21-1",
        "session_id": 1, "op": "zoom", "direction": "left", "correlation": "cli-4242-ab12",
    })
    .to_string();
    assert!(parse_directional_request(&unknown).is_err());
    assert!(parse_directional_request("not json").is_err());
}

#[test]
fn send_output_cli_parses_follow_stay_direction_without_hwnd() {
    // Test-needed out-of-hook route (tentative): `send-output
    // (--follow|--stay) --direction DIR` queues one bounded request through
    // the existing exact-owner request transport with no carried HWND and no
    // new IPC. The owner resolves the live foreground and dispatches through
    // the real explicit `SendToOutput` production route.
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }
    let follow = parse_send_output_args(&strings(&["--follow", "--direction", "right"]))
        .expect("follow parses");
    assert!(follow.follow);
    assert_eq!(follow.direction, "right");
    assert!(
        verify_send_output_argv_consistency(
            &strings(&["--follow", "--direction", "right"]),
            &follow
        )
        .is_ok()
    );
    let stay =
        parse_send_output_args(&strings(&["--stay", "--direction", "left"])).expect("stay parses");
    assert!(!stay.follow);
    assert_eq!(stay.direction, "left");
    assert!(
        verify_send_output_argv_consistency(&strings(&["--stay", "--direction", "left"]), &stay)
            .is_ok()
    );
    // Arity and order: exactly `(--follow|--stay) --direction DIR`, no
    // mixing, no extras, no swapped order.
    assert!(parse_send_output_args(&strings(&["--follow"])).is_err());
    assert!(parse_send_output_args(&strings(&["--direction", "left"])).is_err());
    assert!(parse_send_output_args(&strings(&["--follow", "--direction"])).is_err());
    assert!(parse_send_output_args(&strings(&["--direction", "left", "--follow"])).is_err());
    assert!(
        parse_send_output_args(&strings(&["--follow", "--direction", "left", "--trace"])).is_err()
    );
    assert!(parse_send_output_args(&strings(&["--move", "--direction", "left"])).is_err());
    assert!(parse_send_output_args(&strings(&["--follow", "--direction", "north"])).is_err());
    assert!(parse_send_output_args(&strings(&["--follow", "--mode", "left"])).is_err());
    assert!(
        verify_send_output_argv_consistency(&strings(&["--stay", "--direction", "left"]), &follow)
            .is_err()
    );
    assert!(
        verify_send_output_argv_consistency(
            &strings(&["--follow", "--direction", "left"]),
            &follow
        )
        .is_err()
    );
}

#[test]
fn send_output_request_roundtrip_and_refusals() {
    let request = SendOutputRequest {
        v: 1,
        creation: "abc123".to_owned(),
        pid: 4242,
        exe_path: "C:\\bin\\tiler-windows.exe".to_owned(),
        user_sid: "S-1-5-21-1".to_owned(),
        session_id: 1,
        follow: true,
        direction: "right".to_owned(),
        correlation: "cli-4242-ab12".to_owned(),
    };
    let body = render_send_output_request(&request);
    assert!(body.contains("right"));
    assert!(!body.contains("hwnd"));
    assert_eq!(
        parse_send_output_request(&body).expect("roundtrip"),
        request
    );
    let stay = SendOutputRequest {
        follow: false,
        direction: "left".to_owned(),
        ..request.clone()
    };
    let body = render_send_output_request(&stay);
    assert_eq!(
        parse_send_output_request(&body).expect("stay roundtrip"),
        stay
    );
    let mut bad = request.clone();
    bad.direction = "north".to_owned();
    assert!(parse_send_output_request(&render_send_output_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.v = 2;
    assert!(parse_send_output_request(&render_send_output_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.correlation.clear();
    assert!(parse_send_output_request(&render_send_output_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.creation.clear();
    assert!(parse_send_output_request(&render_send_output_request(&bad)).is_err());
    let mut bad = request.clone();
    bad.pid = 0;
    assert!(parse_send_output_request(&render_send_output_request(&bad)).is_err());
    assert!(parse_send_output_request("not json").is_err());
}

#[test]
fn resize_repeat_tracks_focus_direction_mode_without_key_up_reset() {
    // Exact KDE `requestResize` tracker rule: a fresh press starts at 0 and
    // arms 1; an identical focused/direction/mode press continues the run.
    // Key-up never resets (there is no key-up input to this rule at all).
    let (index, state) = resize_repeat_next(None, "w2", "left", "outwards");
    assert_eq!(index, 0);
    assert_eq!(
        state,
        ResizeRepeat {
            focused: "w2".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            next: 1,
        }
    );
    let (index, state) = resize_repeat_next(Some(state), "w2", "left", "outwards");
    assert_eq!((index, state.next), (1, 2));
    let (index, state) = resize_repeat_next(Some(state), "w2", "left", "outwards");
    assert_eq!((index, state.next), (2, 3));
    // Any focus, direction, or mode change restarts at the initial press.
    let (index, _) = resize_repeat_next(Some(state.clone()), "w1", "left", "outwards");
    assert_eq!(index, 0);
    let (index, _) = resize_repeat_next(Some(state.clone()), "w2", "right", "outwards");
    assert_eq!(index, 0);
    let (index, restarted) = resize_repeat_next(Some(state), "w2", "left", "inwards");
    assert_eq!(index, 0);
    assert_eq!(restarted.next, 1);
    // A restarted run continues on its own triple, and the counter saturates
    // instead of wrapping on extreme repeat holds.
    let (index, restarted) = resize_repeat_next(Some(restarted), "w2", "left", "inwards");
    assert_eq!((index, restarted.next), (1, 2));
    let saturated = ResizeRepeat {
        focused: "w2".to_owned(),
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        next: u32::MAX,
    };
    let (index, next) = resize_repeat_next(Some(saturated), "w2", "left", "outwards");
    assert_eq!((index, next.next), (u32::MAX, u32::MAX));
}

use tiler_core::size_hints::WindowSizeHints;
use tiler_core::window_policy::{
    maximized_focus_fence, sticky_toggle_refusal, unfloat_maximize_clear_eligible,
};
use tiler_core::world_directional::{
    FloatFocusCandidate, float_half_snap_rect, half_snap_constraints_ok,
};
use tiler_windows::tiling::{
    authorize_maximize_clear, maximize_clear_refusal, postclear_fullscreen_now,
};

fn float_candidate(token: &str, x: i32, y: i32, sticky: bool) -> FloatFocusCandidate {
    FloatFocusCandidate {
        token: tiler_core::directional::WindowId(token.to_owned()),
        x,
        y,
        sticky,
    }
}

// World-owned selection returns the core lifetime id; the ported KDE
// parity assertions below keep comparing display tokens.
fn select_float_focus_target(
    x: i32,
    y: i32,
    direction: &str,
    candidates: &[FloatFocusCandidate],
) -> Option<String> {
    tiler_core::world_directional::select_float_focus_target(x, y, direction, candidates)
        .map(|id| id.0)
}

// R-FLT-07..09 float-origin focus: COSMIC top-left axis rule with the sticky
// layer first (ported case-for-case from
// `kwin/tests/plan-float-focus.test.ts` selection semantics).
#[test]
fn float_focus_right_selects_nearest_positive_x_delta() {
    let candidates = vec![
        float_candidate("win-g", 1800, 500, false),
        float_candidate("win-far", 2000, 0, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "right", &candidates),
        Some("win-g".to_owned())
    );
}

#[test]
fn float_focus_ignores_perpendicular_distance() {
    // Same x delta, wildly different y: the axis alone decides, first kept.
    let candidates = vec![
        float_candidate("win-a", 600, 0, false),
        float_candidate("win-b", 600, 900, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "left", &candidates),
        None,
        "both deltas are positive for left"
    );
    let candidates = vec![
        float_candidate("win-a", 400, 0, false),
        float_candidate("win-b", 400, 900, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "left", &candidates),
        Some("win-a".to_owned())
    );
}

#[test]
fn float_focus_left_admits_equal_deltas_and_keeps_first_tie() {
    // Equal (delta 0) beats -100: zero distance is nearer.
    let candidates = vec![
        float_candidate("win-g1", 900, 0, false),
        float_candidate("win-g2", 1000, 900, false),
    ];
    assert_eq!(
        select_float_focus_target(1000, 500, "left", &candidates),
        Some("win-g2".to_owned())
    );
    // Two equidistant negatives keep the first in encounter order.
    let candidates = vec![
        float_candidate("win-g1", 900, 0, false),
        float_candidate("win-g2", 900, 900, false),
    ];
    assert_eq!(
        select_float_focus_target(1000, 500, "left", &candidates),
        Some("win-g1".to_owned())
    );
}

#[test]
fn float_focus_right_keeps_last_of_two_equidistant() {
    let candidates = vec![
        float_candidate("win-g1", 600, 0, false),
        float_candidate("win-g2", 600, 900, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "right", &candidates),
        Some("win-g2".to_owned())
    );
}

#[test]
fn float_focus_down_requires_strictly_positive_deltas() {
    let candidates = vec![float_candidate("win-g", 900, 500, false)];
    assert_eq!(
        select_float_focus_target(500, 500, "down", &candidates),
        None
    );
    let candidates = vec![float_candidate("win-g", 900, 501, false)];
    assert_eq!(
        select_float_focus_target(500, 500, "down", &candidates),
        Some("win-g".to_owned())
    );
}

#[test]
fn float_focus_up_selects_above_on_y_axis_only() {
    let candidates = vec![
        float_candidate("win-near", 0, 400, false),
        float_candidate("win-far", 500, 100, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "up", &candidates),
        Some("win-near".to_owned())
    );
}

#[test]
fn float_focus_sticky_layer_wins_left_ties_loses_right_ties() {
    // One shared race in sticky-first encounter order: left keeps the first
    // (sticky), right keeps the last (ordinary).
    let left = vec![
        float_candidate("win-ord", 900, 0, false),
        float_candidate("win-sticky", 900, 900, true),
    ];
    assert_eq!(
        select_float_focus_target(1000, 500, "left", &left),
        Some("win-sticky".to_owned())
    );
    let right = vec![
        float_candidate("win-ord", 600, 0, false),
        float_candidate("win-sticky", 600, 900, true),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "right", &right),
        Some("win-ord".to_owned())
    );
}

#[test]
fn float_focus_nearer_ordinary_beats_farther_sticky() {
    let candidates = vec![
        float_candidate("win-sticky", 900, 900, true),
        float_candidate("win-ord", 600, 0, false),
    ];
    assert_eq!(
        select_float_focus_target(500, 500, "right", &candidates),
        Some("win-ord".to_owned())
    );
}

#[test]
fn float_focus_miss_and_invalid_direction_select_nothing() {
    assert_eq!(
        select_float_focus_target(500, 500, "right", &[]),
        None,
        "lone float retains"
    );
    let candidates = vec![float_candidate("win-g", 700, 500, false)];
    assert_eq!(
        select_float_focus_target(500, 500, "diagonal", &candidates),
        None
    );
    assert_eq!(select_float_focus_target(500, 500, "", &candidates), None);
}

// R-FLT-10..11 half-snaps: COSMIC `relative_geometry` arithmetic (ported
// case-for-case from `kwin/tests/plan-float-move.test.ts`).
#[test]
fn float_half_snap_zero_gap_covers_all_four_directions() {
    let bounds = rect(0, 0, 2560, 1440);
    assert_eq!(
        float_half_snap_rect(bounds, 0, "right"),
        Some(rect(1280, 0, 1280, 1440))
    );
    assert_eq!(
        float_half_snap_rect(bounds, 0, "left"),
        Some(rect(0, 0, 1280, 1440))
    );
    assert_eq!(
        float_half_snap_rect(bounds, 0, "up"),
        Some(rect(0, 0, 2560, 720))
    );
    assert_eq!(
        float_half_snap_rect(bounds, 0, "down"),
        Some(rect(0, 720, 2560, 720))
    );
}

#[test]
fn float_half_snap_odd_sizes_use_trunc_division() {
    // W=1001: halfW=500; inner=7: halfInner=3, threeHalf=10.
    let bounds = rect(0, 0, 1001, 501);
    assert_eq!(
        float_half_snap_rect(bounds, 7, "right"),
        Some(rect(503, 7, 490, 487))
    );
    assert_eq!(
        float_half_snap_rect(bounds, 7, "left"),
        Some(rect(7, 7, 490, 487))
    );
    assert_eq!(
        float_half_snap_rect(rect(10, 20, 1001, 501), 7, "down"),
        Some(rect(17, 273, 987, 240))
    );
}

#[test]
fn float_half_snap_ignores_outer_gap_and_offsets_by_global_origin() {
    assert_eq!(
        float_half_snap_rect(rect(800, 100, 800, 600), 4, "right"),
        Some(rect(1202, 104, 394, 592))
    );
}

#[test]
fn float_half_snap_repeats_are_stateless() {
    // Every repeated arrow requests the same half again: no snap state.
    let bounds = rect(0, 0, 2560, 1440);
    let first = float_half_snap_rect(bounds, 0, "right");
    assert_eq!(float_half_snap_rect(bounds, 0, "right"), first);
    assert_eq!(
        float_half_snap_rect(bounds, 0, "left"),
        Some(rect(0, 0, 1280, 1440))
    );
}

#[test]
fn float_half_snap_rejects_invalid_input() {
    // Tiny work area with a large inner gap goes nonpositive.
    assert_eq!(float_half_snap_rect(rect(0, 0, 10, 10), 8, "right"), None);
    assert_eq!(float_half_snap_rect(rect(0, 0, 0, 600), 0, "left"), None);
    assert_eq!(
        float_half_snap_rect(rect(0, 0, 2560, 1440), 65, "right"),
        None,
        "inner gap above 64 refused"
    );
    assert_eq!(
        float_half_snap_rect(rect(0, 0, 2560, 1440), 0, "diagonal"),
        None
    );
}

// G-06 maximized directional operations (REQ-MAX-08): focus fenced while the
// focused window is maximized; move proceeds only after an observed clear
// with the same reference, focus and domain.
#[test]
fn maximized_focus_fence_covers_tiles_with_fullscreen_exempt() {
    assert!(maximized_focus_fence(true, false));
    assert!(!maximized_focus_fence(true, true));
    assert!(!maximized_focus_fence(false, false));
    assert!(!maximized_focus_fence(false, true));
}

fn size_hints(
    min_w: Option<i32>,
    min_h: Option<i32>,
    max_w: Option<i32>,
    max_h: Option<i32>,
) -> WindowSizeHints {
    WindowSizeHints {
        min_w,
        min_h,
        max_w,
        max_h,
    }
}

// G-06 pre-clear authorization: the exact subject must already be revalidated
// and home/view stability, suspension and elevation must all hold just before
// the single native clear. Any failure authorizes no clear and no move.
#[test]
fn maximize_clear_authorizer_pins_pre_write_gates() {
    assert_eq!(
        authorize_maximize_clear(false, true, true, false, false),
        None,
        "authorized: clear may issue"
    );
    // Fresh fullscreen precedence wins first: a subject that transitioned
    // after the tick snapshot is never cleared, never moved.
    assert_eq!(
        authorize_maximize_clear(true, true, true, false, false),
        Some("fullscreen-raced")
    );
    assert_eq!(
        authorize_maximize_clear(true, false, false, false, false),
        Some("fullscreen-raced"),
        "fullscreen dominates every other race"
    );
    assert_eq!(
        authorize_maximize_clear(false, false, true, false, false),
        Some("domain-raced")
    );
    assert_eq!(
        authorize_maximize_clear(false, true, false, false, false),
        Some("focus-raced")
    );
    assert_eq!(
        authorize_maximize_clear(false, true, true, true, false),
        Some("suspended")
    );
    assert_eq!(
        authorize_maximize_clear(false, true, true, false, true),
        Some("elevated-foreground")
    );
    // Priority: fullscreen, domain, focus, suspension, elevation.
    assert_eq!(
        authorize_maximize_clear(false, false, false, true, true),
        Some("domain-raced")
    );
    assert_eq!(
        authorize_maximize_clear(false, true, false, true, true),
        Some("focus-raced")
    );
    assert_eq!(
        authorize_maximize_clear(false, true, true, true, true),
        Some("suspended")
    );
}

// R-FLT-10..11 declared-size gate: an incompatible half never fights the
// client. Per-axis-zero bounds ride unset, so only genuinely constrained axes
// decide (G-05 consistency).
#[test]
fn half_snap_constraints_pin_min_max_and_per_axis_zero() {
    let half = rect(1202, 104, 394, 592);
    assert!(half_snap_constraints_ok(half, WindowSizeHints::none()));
    // Minimums refuse per axis; exact fits pass.
    assert!(!half_snap_constraints_ok(
        half,
        size_hints(Some(400), None, None, None)
    ));
    assert!(half_snap_constraints_ok(
        half,
        size_hints(Some(394), None, None, None)
    ));
    assert!(!half_snap_constraints_ok(
        half,
        size_hints(None, Some(600), None, None)
    ));
    assert!(half_snap_constraints_ok(
        half,
        size_hints(None, Some(592), None, None)
    ));
    // Maximums refuse per axis; exact fits pass.
    assert!(!half_snap_constraints_ok(
        half,
        size_hints(None, None, Some(300), None)
    ));
    assert!(half_snap_constraints_ok(
        half,
        size_hints(None, None, Some(394), None)
    ));
    assert!(!half_snap_constraints_ok(
        half,
        size_hints(None, None, None, Some(500))
    ));
    assert!(half_snap_constraints_ok(
        half,
        size_hints(None, None, None, Some(592))
    ));
    // Per-axis-zero is unset: only the genuinely fixed nonzero axis decides.
    assert!(
        !half_snap_constraints_ok(half, size_hints(Some(640), Some(0), None, None)),
        "nonzero width axis still refuses"
    );
    assert!(
        half_snap_constraints_ok(half, size_hints(Some(0), Some(0), Some(0), Some(0))),
        "all-zero hints constrain nothing"
    );
    assert!(
        half_snap_constraints_ok(half, size_hints(None, None, Some(0), Some(600))),
        "zero max width is unset; tall max height passes"
    );
    assert!(
        !half_snap_constraints_ok(half, size_hints(None, None, Some(0), Some(400))),
        "nonzero max height still refuses"
    );
}

#[test]
fn maximize_clear_refusal_pins_the_post_clear_decision_table() {
    // Undispatched clears never proceed, even with matching observation.
    assert_eq!(
        maximize_clear_refusal("threw", true, true, true, true, false),
        Some("native-write")
    );
    assert_eq!(
        maximize_clear_refusal("denied", true, true, true, true, false),
        Some("native-write")
    );
    // Async dispatched is not observed completion: still-maximized refuses.
    assert_eq!(
        maximize_clear_refusal("dispatched", false, true, true, true, false),
        Some("still-maximized")
    );
    // Observed clear with the same reference, focus and domain proceeds.
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, true, false),
        None
    );
    assert_eq!(
        maximize_clear_refusal("dispatched", true, true, true, true, false),
        None,
        "late async completion observed before routing still proceeds"
    );
    // Each race refuses narrowly with no structural move.
    assert_eq!(
        maximize_clear_refusal("restored", true, false, true, true, false),
        Some("identity-raced")
    );
    assert_eq!(
        maximize_clear_refusal("restored", false, true, true, true, false),
        Some("still-maximized")
    );
    assert_eq!(
        maximize_clear_refusal("restored", true, true, false, true, false),
        Some("domain-raced")
    );
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, true, true),
        Some("fullscreen-raced")
    );
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, false, false),
        Some("focus-raced")
    );
}

#[test]
fn postclear_fullscreen_derivation_refuses_owned_non_covering() {
    // Directional/B9 post-clear gap shape: an owned transition whose frame
    // never covered a monitor is eligible-observed (no captionless flag) with
    // no retained fullscreen flag, yet metadata is present. The pre-fix
    // directional policy derived retained-flag-only and would proceed here.
    assert!(postclear_fullscreen_now(false, false, true));
    assert!(postclear_fullscreen_now(true, false, false));
    assert!(postclear_fullscreen_now(false, true, false));
    assert!(!postclear_fullscreen_now(false, false, false));
    let old_policy = false;
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, true, old_policy),
        None,
        "retained-flag-only proceeds on the owned non-covering shape: unsafe"
    );
    let now = postclear_fullscreen_now(false, false, true);
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, true, now),
        Some("fullscreen-raced"),
        "shared derivation refuses the owned non-covering shape"
    );
    let clear = postclear_fullscreen_now(false, false, false);
    assert_eq!(
        maximize_clear_refusal("restored", true, true, true, true, clear),
        None,
        "metadata absence with a cleared frame allows routing"
    );
}

#[test]
fn unfloat_clear_lane_takes_only_ordinary_maximized_floats() {
    // B9 (R-FLT-06, items 11+15): only an ordinary maximized float takes the
    // one-shot nonactivating clear plus observed-clear route before fresh
    // Engine admission. Portable pin of the lane definition; the native
    // clear/observe/settle legs stay Windows-owned.
    assert!(unfloat_maximize_clear_eligible(false, true, true, false));
    assert!(!unfloat_maximize_clear_eligible(true, true, true, false));
    assert!(!unfloat_maximize_clear_eligible(true, false, true, false));
    assert!(!unfloat_maximize_clear_eligible(false, true, true, true));
    assert!(!unfloat_maximize_clear_eligible(false, true, false, false));
    assert!(!unfloat_maximize_clear_eligible(false, false, true, false));
    assert!(!unfloat_maximize_clear_eligible(false, false, false, false));
}

#[test]
fn unfloat_clear_lane_matches_snapshot_vocabulary() {
    // The lane it replaces still reports through the existing snapshot
    // vocabulary: fullscreen wins, sticky keeps its own refusal, and only
    // the ordinary maximized float reports the B9 entry refusal.
    assert_eq!(
        float_toggle_refusal(false, true),
        Some("float-refused-maximize")
    );
    assert!(unfloat_maximize_clear_eligible(false, true, true, false));
    assert_eq!(
        float_toggle_refusal(true, true),
        Some("float-refused-fullscreen")
    );
    assert!(!unfloat_maximize_clear_eligible(true, true, true, false));
    assert_eq!(
        sticky_toggle_refusal(false, true),
        Some("sticky-refused-maximize")
    );
    assert!(!unfloat_maximize_clear_eligible(false, true, true, true));
    assert_eq!(float_toggle_refusal(false, false), None);
    assert!(!unfloat_maximize_clear_eligible(false, false, true, false));
}

#[test]
fn retile_overlay_stays_independent_of_born_setting() {
    // R-MAX-03 (item 11): the floating-to-tiled overlay keeps no one-shot
    // clear under either `core.born_maximized` value; only born R-MAX-06
    // reads that setting. Portable pin of the pure gate both lanes share:
    // the latched (overlay) form never clears, the unlatched (tile) form
    // clears exactly once for a slotless maximized tiled admission.
    use tiler_core::window_policy::should_clear_maximize_at_admission_with_mode as gate;
    for keep in [false, true] {
        assert!(!gate(false, true, true, false, true, keep));
        assert!(!gate(true, true, false, false, true, keep));
        assert!(!gate(false, true, false, true, true, keep));
        assert!(!gate(false, true, false, false, false, keep));
        assert!(!gate(false, false, false, false, true, keep));
    }
    assert!(gate(false, true, false, false, true, false));
    assert!(!gate(false, true, false, false, true, true));
}
