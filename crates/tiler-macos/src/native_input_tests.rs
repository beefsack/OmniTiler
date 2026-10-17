#![cfg(test)]
//! Mechanical split of `native_input.rs` cfg tests (no behavior change).
use std::sync::Arc;

use crate::config::ProbeModifier;
use crate::dev_selection::TileScope;
use crate::input::cg_flag_bits;
use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
use crate::input_policy::{DisableAction, unnotified_disable_action};
use crate::native_input::*;
use crate::runtime::{InputDirection, InputSemanticAction};
#[cfg(not(target_os = "macos"))]
use crate::runtime_backend::InputSourceFactory;

fn tuning_for(modifier: ProbeModifier, key_code: u32, shift: bool, alt: bool) -> TapTuning {
    let (mut want, mask) = cg_flag_bits(modifier);
    if shift {
        want |= CG_SHIFT_BIT;
    }
    if alt {
        want |= CG_ALT_BIT;
    }
    TapTuning {
        want_flags: want,
        flags_mask: mask,
        want_code: i64::from(key_code),
    }
}

#[test]
fn catalogue_is_exactly_focus_move_resize_float_orientation() {
    let catalogue = tile_catalogue();
    assert_eq!(catalogue.len(), 34);
    let focus: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::FocusStep { .. }))
        .collect();
    let moves: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::MoveStep { .. }))
        .collect();
    let grows: Vec<_> = catalogue
        .iter()
        .filter(|b| {
            matches!(
                b.action,
                InputSemanticAction::ResizeStep { mode, .. }
                    if mode == tiler_core::contract::ResizeMode::Outwards
            )
        })
        .collect();
    let shrinks: Vec<_> = catalogue
        .iter()
        .filter(|b| {
            matches!(
                b.action,
                InputSemanticAction::ResizeStep { mode, .. }
                    if mode == tiler_core::contract::ResizeMode::Inwards
            )
        })
        .collect();
    let floats: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::ToggleFloat))
        .collect();
    let orientations: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::ToggleOrientation))
        .collect();
    assert_eq!(focus.len(), 8);
    assert_eq!(moves.len(), 8);
    assert_eq!(grows.len(), 8);
    assert_eq!(shrinks.len(), 8);
    assert_eq!(floats.len(), 1);
    assert_eq!(orientations.len(), 1);
    // Focus chords carry neither arm; move carries Shift; grow carries Alt;
    // shrink carries both; float and orientation are bare.
    assert!(focus.iter().all(|b| !b.shift && !b.alt));
    assert!(moves.iter().all(|b| b.shift && !b.alt));
    assert!(grows.iter().all(|b| !b.shift && b.alt));
    assert!(shrinks.iter().all(|b| b.shift && b.alt));
    assert_eq!(floats[0].key_code, KEY_G);
    assert!(!floats[0].shift && !floats[0].alt);
    assert_eq!(orientations[0].key_code, KEY_O);
    assert!(!orientations[0].shift && !orientations[0].alt);
    // Exact chord set: HJKL plus four arrows on all four arms, plus G/O.
    let mut keys: Vec<(u32, bool, bool)> = catalogue
        .iter()
        .map(|b| (b.key_code, b.shift, b.alt))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 34, "catalogue chords must be unique");
    for key in [
        KEY_H, KEY_J, KEY_K, KEY_L, KEY_LEFT, KEY_DOWN, KEY_UP, KEY_RIGHT,
    ] {
        assert!(
            catalogue
                .iter()
                .any(|b| b.key_code == key && !b.shift && !b.alt)
        );
        assert!(
            catalogue
                .iter()
                .any(|b| b.key_code == key && b.shift && !b.alt)
        );
        assert!(
            catalogue
                .iter()
                .any(|b| b.key_code == key && !b.shift && b.alt)
        );
        assert!(
            catalogue
                .iter()
                .any(|b| b.key_code == key && b.shift && b.alt)
        );
    }
    // Directions: H/Left, J/Down, K/Up, L/Right on all four arms.
    let dir_of = |key: u32, shift: bool, alt: bool| {
        catalogue
            .iter()
            .find(|b| b.key_code == key && b.shift == shift && b.alt == alt)
            .map(|b| match b.action.clone() {
                InputSemanticAction::FocusStep { direction }
                | InputSemanticAction::MoveStep { direction }
                | InputSemanticAction::ResizeStep { direction, .. } => direction,
                InputSemanticAction::ToggleFloat | InputSemanticAction::ToggleOrientation => {
                    InputDirection::Left
                }
                // The basic catalogue holds no workspace or drag actions.
                InputSemanticAction::WorkspaceSelect { .. }
                | InputSemanticAction::WorkspaceSend { .. }
                | InputSemanticAction::WorkspaceRelative { .. }
                | InputSemanticAction::WorkspaceSendRelative { .. }
                | InputSemanticAction::WorkspacePrevious
                | InputSemanticAction::WorkspaceOutput { .. }
                | InputSemanticAction::DragDrop { .. } => InputDirection::Left,
            })
    };
    assert_eq!(dir_of(KEY_H, false, false), Some(InputDirection::Left));
    assert_eq!(dir_of(KEY_J, false, false), Some(InputDirection::Down));
    assert_eq!(dir_of(KEY_K, false, false), Some(InputDirection::Up));
    assert_eq!(dir_of(KEY_L, false, false), Some(InputDirection::Right));
    assert_eq!(dir_of(KEY_LEFT, true, false), Some(InputDirection::Left));
    assert_eq!(dir_of(KEY_DOWN, true, false), Some(InputDirection::Down));
    assert_eq!(dir_of(KEY_UP, true, false), Some(InputDirection::Up));
    assert_eq!(dir_of(KEY_RIGHT, true, false), Some(InputDirection::Right));
    assert_eq!(dir_of(KEY_H, false, true), Some(InputDirection::Left));
    assert_eq!(dir_of(KEY_L, true, true), Some(InputDirection::Right));
    assert_eq!(dir_of(KEY_UP, false, true), Some(InputDirection::Up));
}

#[test]
fn exact_match_rejects_extras_but_ignores_caps_fn() {
    // Meta+H focus chord: exact bits match; any extra *masked* modifier
    // (Shift/Ctrl/Alt) fails; Caps/Fn sit outside the mask per the
    // existing matcher contract and neither match nor break.
    let tuning = tuning_for(ProbeModifier::Meta, KEY_H, false, false);
    let exact = |flags: u64| {
        let mut pending = false;
        decide_tap_event(
            TapKeyKind::KeyDown,
            flags,
            i64::from(KEY_H),
            tuning,
            true,
            &mut pending,
        )
        .matched
    };
    const META: u64 = 0x0010_0000;
    const SHIFT: u64 = 0x0002_0000;
    const CTRL: u64 = 0x0004_0000;
    const ALT: u64 = 0x0008_0000;
    const CAPS: u64 = 0x0001_0000;
    const FN: u64 = 0x0080_0000;
    assert!(exact(META));
    assert!(!exact(META | SHIFT), "extra Shift must fail a focus chord");
    assert!(!exact(META | CTRL));
    assert!(!exact(META | ALT), "Alt selects the grow arm, never focus");
    assert!(!exact(0), "bare key must fail");
    assert!(exact(META | CAPS), "Caps sits outside the mask");
    assert!(exact(META | FN), "Fn sits outside the mask");
    // Move arm needs exactly Mod+Shift.
    let move_tuning = tuning_for(ProbeModifier::Meta, KEY_H, true, false);
    let moved = |flags: u64| {
        let mut pending = false;
        decide_tap_event(
            TapKeyKind::KeyDown,
            flags,
            i64::from(KEY_H),
            move_tuning,
            true,
            &mut pending,
        )
        .matched
    };
    assert!(moved(META | SHIFT));
    assert!(!moved(META), "move needs its Shift arm");
    assert!(!moved(META | SHIFT | CTRL), "extra Ctrl must fail");
    assert!(!moved(META | SHIFT | ALT), "Alt selects shrink, never move");
    // Grow arm needs exactly Mod+Alt; shrink needs Mod+Alt+Shift.
    let grow_tuning = tuning_for(ProbeModifier::Meta, KEY_H, false, true);
    let grown = |flags: u64| {
        let mut pending = false;
        decide_tap_event(
            TapKeyKind::KeyDown,
            flags,
            i64::from(KEY_H),
            grow_tuning,
            true,
            &mut pending,
        )
        .matched
    };
    assert!(grown(META | ALT));
    assert!(!grown(META), "grow needs its Alt arm");
    assert!(
        !grown(META | ALT | SHIFT),
        "Shift selects shrink, never grow"
    );
    assert!(!grown(META | SHIFT), "Shift alone never grows");
    let shrink_tuning = tuning_for(ProbeModifier::Meta, KEY_H, true, true);
    let shrunk = |flags: u64| {
        let mut pending = false;
        decide_tap_event(
            TapKeyKind::KeyDown,
            flags,
            i64::from(KEY_H),
            shrink_tuning,
            true,
            &mut pending,
        )
        .matched
    };
    assert!(shrunk(META | ALT | SHIFT));
    assert!(!shrunk(META | ALT), "shrink needs Shift too");
    assert!(!shrunk(META | SHIFT), "shrink needs Alt too");
}

#[test]
fn paired_suppression_repeats_and_release() {
    // Armed tap over one spec: exact down suppresses and pends, a
    // repeat down still forwards (matched), the paired up suppresses, a
    // lone up never suppresses, and a modifier release clears pending.
    let tuning = tuning_for(ProbeModifier::Meta, KEY_H, false, false);
    const META: u64 = 0x0010_0000;
    let mut pending = false;
    let down = decide_tap_event(
        TapKeyKind::KeyDown,
        META,
        i64::from(KEY_H),
        tuning,
        true,
        &mut pending,
    );
    assert!(down.matched && down.suppress && pending);
    // Repeat while held: still an exact match (forwards).
    let repeat = decide_tap_event(
        TapKeyKind::KeyDown,
        META,
        i64::from(KEY_H),
        tuning,
        true,
        &mut pending,
    );
    assert!(repeat.matched);
    // Paired up suppresses and clears.
    let up = decide_tap_event(
        TapKeyKind::KeyUp,
        META,
        i64::from(KEY_H),
        tuning,
        true,
        &mut pending,
    );
    assert!(up.matched && up.suppress && !pending);
    // Lone up (no prior down): observed but kept.
    let mut lone = false;
    let up = decide_tap_event(
        TapKeyKind::KeyUp,
        META,
        i64::from(KEY_H),
        tuning,
        true,
        &mut lone,
    );
    assert!(up.matched && !up.suppress);
    // Modifier released first: pending clears, the later exact up is
    // observed but kept.
    let mut held = false;
    let _ = decide_tap_event(
        TapKeyKind::KeyDown,
        META,
        i64::from(KEY_H),
        tuning,
        true,
        &mut held,
    );
    assert!(held);
    let broken = decide_tap_event(
        TapKeyKind::KeyUp,
        0,
        i64::from(KEY_H),
        tuning,
        true,
        &mut held,
    );
    assert!(!broken.matched && !broken.suppress && !held);
}

#[test]
fn gate_matrix() {
    use crate::ffi::SessionLock;
    let unlocked = |secure: Option<bool>, frontmost: Option<(i32, String)>| {
        interception_gate(secure, frontmost, SessionLock::Unlocked)
    };
    // Lock screen gates; unknown session gates; Secure Input gates;
    // the ordinary unlocked state proceeds (Unknown secure proceeds).
    assert_eq!(
        unlocked(Some(false), Some((42, "com.apple.TextEdit".to_owned()))),
        None
    );
    assert_eq!(
        unlocked(None, Some((42, "com.apple.TextEdit".to_owned()))),
        None,
        "unknown Secure Input proceeds"
    );
    assert_eq!(
        unlocked(Some(true), Some((42, "com.apple.TextEdit".to_owned()))),
        Some("secure-input-active")
    );
    assert_eq!(
        unlocked(Some(false), Some((7, "com.apple.loginwindow".to_owned()))),
        Some("lock-screen")
    );
    assert_eq!(unlocked(Some(false), None), Some("session-unknown"));
    assert_eq!(
        unlocked(Some(false), Some((0, String::new()))),
        Some("session-unknown")
    );
    // The actual session lock verdict gates first: locked refuses even
    // with a healthy frontmost, and unknown refuses (never guessed
    // unlocked); the lock screen names its own token under an unlocked
    // verdict above.
    assert_eq!(
        interception_gate(
            Some(false),
            Some((42, "com.apple.TextEdit".to_owned())),
            SessionLock::Locked
        ),
        Some("session-locked")
    );
    assert_eq!(
        interception_gate(
            Some(false),
            Some((42, "com.apple.TextEdit".to_owned())),
            SessionLock::Unknown
        ),
        Some("session-unknown")
    );
    assert_eq!(
        interception_gate(None, None, SessionLock::Locked),
        Some("session-locked"),
        "lock verdict precedes the secure/frontmost legs"
    );
}

#[test]
fn stall_seam_is_probe_only_never_product() {
    // The deliberate callback-stall seam exists ONLY for the
    // `input-session --stall-once-ms` fault-injection leg. The exact
    // production tile construction (scope + facts store, no unrestricted
    // gate, no stall builder) always reads disarmed; only the explicit
    // probe builder arms it. Tile/dev code paths never call
    // `with_stall_once` (only `input_session` does).
    use crate::input_eligibility::EligibilityFactsStore;
    let production = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_scope(TileScope::Scoped(crate::scope::AppScope {
            allowed: vec!["com.apple.TextEdit".to_owned()],
        }))
        .with_facts_store(Arc::new(EligibilityFactsStore::new()));
    assert!(!production.stall_armed());
    let probe = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_stall_once(1200);
    assert!(probe.stall_armed());
    // A zero stall never arms either (default factory state).
    let plain = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue()).unwrap();
    assert!(!plain.stall_armed());
}

#[test]
fn direct_hold_defaults_off_with_unknown_state() {
    // Fixture-only DIRECT control: production default is hold-off with
    // no state reading yet; set/clear round-trips on the flag while the
    // published state stays unknown until the owner thread reads the OS.
    // The EXPLICIT re-arm epoch starts at 0 (no fresh source yet).
    let factory = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue()).unwrap();
    assert_eq!(factory.latest_direct_enabled(), None);
    assert_eq!(factory.rearm_epoch(), 0);
    factory.set_direct_hold(true);
    factory.set_direct_hold(false);
    assert_eq!(factory.latest_direct_enabled(), None);
    assert_eq!(factory.rearm_epoch(), 0);
}

#[test]
fn factories_reject_empty_tables() {
    assert!(TapTileFactory::new(ProbeModifier::Meta, vec![]).is_err());
    let catalogue = tile_catalogue();
    assert!(TapTileFactory::new(ProbeModifier::Meta, catalogue).is_ok());
    // Off-macOS creation fails closed (no live tap here); the
    // refusal shape — not silent success — is what this pins.
    #[cfg(not(target_os = "macos"))]
    {
        let factory = TapTileFactory::new(ProbeModifier::Meta, tile_catalogue()).unwrap();
        assert!(factory.create().is_err());
    }
}

#[test]
fn scoped_sources_start_ineligible_until_attested() {
    // Normal tile path: scoped by default, ineligible until fresh
    // scope/foreground/Space/showstate facts attest. The fixture uses an
    // explicit unrestricted test gate instead (known post targets with
    // strict per-edge oracle).
    let catalogue = tile_catalogue();
    let tap = TapTileFactory::new(ProbeModifier::Ctrl, catalogue).unwrap();
    assert!(!tap.eligibility().is_eligible());
    let scoped = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_scope(TileScope::Scoped(crate::scope::AppScope {
            allowed: vec!["com.apple.TextEdit".to_owned()],
        }));
    assert!(!scoped.eligibility().is_eligible());
    // Fixture unrestricted gate: starts eligible (per-edge oracle still
    // enforced for posts; suppression bypass is explicit).
    let gate = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_unrestricted_test_gate();
    assert!(gate.eligibility().is_unrestricted());
    assert!(gate.eligibility().is_eligible());
}

#[test]
fn eligibility_cache_gates_and_rearms_on_flip() {
    // Consume-first: an ineligible publish denies owner-side triage
    // (consumed hits refuse without effects — dropping them never restores
    // passthrough, which is exactly the point). Pin the cache invariant
    // explicitly: ineligible publishes deny, and re-enabling bumps the
    // epoch so the owner re-arms rather than trusting a stale guard.
    use crate::input_eligibility::EligibilityCache;
    let cache = EligibilityCache::new_unrestricted_test_gate();
    assert!(cache.is_eligible());
    cache.publish(false, 4242);
    assert!(!cache.is_eligible());
    // Re-enabled: normal cached exact chords resume (epoch bumped on
    // flip, so the owner re-arms rather than trusting a stale guard).
    let epoch = cache.epoch();
    cache.publish(true, 4242);
    assert!(cache.is_eligible());
    assert!(cache.epoch() > epoch);
}

#[test]
fn held_turn_health_recovers_unnotified_disable_outside_the_fixture_hold() {
    // Simulates the owner's decision on a bounded pump turn that reported
    // Held (no OS notification): enabled reads healthy, a directly-disabled
    // read without the fixture hold recovers via same-tap re-enable (never
    // the timeout count), and the explicit fixture hold bypasses recovery
    // while its deliberate disabled observation or release transition runs.
    let decide = |direct_enabled: bool, direct_hold: bool, holding_direct: bool| {
        if fixture_hold_active(direct_hold, holding_direct) {
            return None;
        }
        unnotified_disable_action(direct_enabled, false)
    };
    assert_eq!(decide(true, false, false), None);
    assert_eq!(
        decide(false, false, false),
        Some(DisableAction::ReenableSameTap)
    );
    // Explicit direct-disable-hold observation: bypassed.
    assert_eq!(decide(false, true, false), None);
    // Hold-to-release transition (single fixture re-enable owns it):
    // bypassed even after the flag clears mid-turn.
    assert_eq!(decide(false, false, true), None);
    assert_eq!(decide(true, true, false), None);
}

#[test]
fn fixture_hold_bypass_is_structural_never_product() {
    // The bypass predicate is a pure OR over the two fixture flags, and the
    // production tile construction leaves both clear (the only setters are
    // the fixture-only `set_direct_hold` / re-arm legs, which production
    // never calls).
    assert!(fixture_hold_active(true, false));
    assert!(fixture_hold_active(false, true));
    assert!(fixture_hold_active(true, true));
    assert!(!fixture_hold_active(false, false));
    let production = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue()).unwrap();
    assert_eq!(production.latest_direct_enabled(), None);
    assert_eq!(production.rearm_epoch(), 0);
}

#[test]
fn tap_reports_accumulate_across_fresh_guard_recreations() {
    // Fresh guards restart their own counters at zero; the owner folds each
    // retired guard's snapshot into the cumulative total, so terminal
    // evidence survives recreations with the timeout / user-input /
    // readback-only kinds kept distinct (a readback-only recovery never
    // lands in the timeout count).
    use crate::ffi::TapSourceReport;
    let zero = TapSourceReport {
        matched: 0,
        suppressed: 0,
        deliveries: 0,
        reenables_after_timeout: 0,
        user_disables: 0,
        unnotified_disables: 0,
    };
    let first = TapSourceReport {
        matched: 2,
        suppressed: 2,
        deliveries: 9,
        reenables_after_timeout: 1,
        user_disables: 1,
        unnotified_disables: 0,
    };
    let total = accumulate_tap_report(zero, first);
    assert_eq!(total, first);
    let second = TapSourceReport {
        matched: 2,
        suppressed: 2,
        deliveries: 4,
        reenables_after_timeout: 0,
        user_disables: 0,
        unnotified_disables: 1,
    };
    let total = accumulate_tap_report(total, second);
    assert_eq!(
        total,
        TapSourceReport {
            matched: 4,
            suppressed: 4,
            deliveries: 13,
            reenables_after_timeout: 1,
            user_disables: 1,
            unnotified_disables: 1,
        }
    );
    // Saturation, never wrap: a hot guard cannot roll the terminal counts.
    let hot = TapSourceReport {
        matched: u32::MAX,
        suppressed: u32::MAX,
        deliveries: u32::MAX,
        reenables_after_timeout: u32::MAX,
        user_disables: u32::MAX,
        unnotified_disables: u32::MAX,
    };
    let total = accumulate_tap_report(total, hot);
    assert_eq!(total, hot);
}

#[test]
fn fresh_guard_reinstall_reuses_the_exact_same_chords() {
    // The owner recreates over the same chord specs it installed with
    // (same modifier, same bindings, same placement): the spec table is a
    // pure function of that pair, so a reinstall replays it exactly.
    let catalogue = tile_catalogue();
    let first = tap_specs_for(ProbeModifier::Ctrl, &catalogue);
    let replay = tap_specs_for(ProbeModifier::Ctrl, &catalogue);
    assert_eq!(first, replay);
    assert_eq!(first.len(), 34);
    // Compatible keeps the shifted move arrows armed (30 chords): a product
    // recreation under Compatible replays that same reduced set.
    use crate::input_policy::{InputPreset, effective_bindings};
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &catalogue);
    assert_eq!(compatible.len(), 30);
    let replay = tap_specs_for(ProbeModifier::Ctrl, &compatible);
    assert_eq!(replay.len(), 30);
    assert_eq!(tap_specs_for(ProbeModifier::Ctrl, &compatible), replay);
}

#[test]
fn tap_specs_carry_select_category_per_action() {
    // Wiring boundary: every spec names its gate category from the action
    // (single home with `is_armed_select`) so the tap callback intercepts
    // exactly the armed selects on the select cache and everything else on
    // the basic cache. Basic tables carry no select chords at all.
    use crate::workspace_input::is_armed_select;
    for binding in tile_catalogue() {
        assert!(!is_armed_select(&binding.action));
    }
    let specs = tap_specs_for(ProbeModifier::Ctrl, &tile_catalogue());
    assert!(!specs.iter().any(|s| s.select));
    let mut product = tile_catalogue();
    product.extend(crate::workspace_select::bindings());
    let specs = tap_specs_for(ProbeModifier::Ctrl, &product);
    assert_eq!(specs.len(), 52);
    assert_eq!(specs.iter().filter(|s| s.select).count(), 18);
    for (spec, binding) in specs.iter().zip(product.iter()) {
        assert_eq!(spec.select, is_armed_select(&binding.action));
        assert_eq!(spec.key_code, binding.key_code);
    }
    // Full catalogue: only the 18 armed rows flag select (sends, stays,
    // outputs, and trailing select-0 stay basic-gated, refused downstream).
    let specs = tap_specs_for(
        ProbeModifier::Ctrl,
        &crate::workspace_input::workspace_bindings(),
    );
    assert_eq!(specs.len(), 45);
    assert_eq!(specs.iter().filter(|s| s.select).count(), 18);
}

#[test]
fn consume_first_triage_refuses_ineligible_without_effects() {
    // Consume-first parity: owned chords always consume in the callback;
    // the owner triages each consumed hit here. Ineligible basics AND
    // selects refuse (never become actions); eligible dispatch unchanged;
    // unowned events forward (never swallowed here); ups always forward
    // (hold release). Lock / Secure Input release is unchanged (see
    // `gate_matrix`); the callback still suppresses exact pairs (see
    // `paired_suppression_repeats_and_release`).
    use crate::input_policy::aux_flag_for;
    let basic_specs = tap_specs_for(ProbeModifier::Ctrl, &tile_catalogue());
    let mut product = tile_catalogue();
    product.extend(crate::workspace_select::bindings());
    let product_specs = tap_specs_for(ProbeModifier::Ctrl, &product);
    let (ctrl_flags, _) = cg_flag_bits(ProbeModifier::Ctrl);
    // Ineligible basic down refuses as basic (fullscreen/game,
    // out-of-scope, stale/unknown facts, zero user currents, and
    // focus/Space mismatch all deny the basic cache).
    assert_eq!(
        triage_consumed_hit(&basic_specs, KEY_H, ctrl_flags, true, false, Some(false)),
        "consumed-refused-basic"
    );
    // Eligible basic down dispatches unchanged (select-cache state is
    // irrelevant for basics).
    assert_eq!(
        triage_consumed_hit(&basic_specs, KEY_H, ctrl_flags, true, true, Some(false)),
        "dispatch"
    );
    // Select digit Ctrl+2: exact flags land in the select category.
    let digit2 = crate::desktop_shortcuts::desktop_digit_keycode(2).unwrap();
    assert_eq!(
        consumed_hit_category(&product_specs, digit2, ctrl_flags),
        Some(true)
    );
    assert_eq!(
        triage_consumed_hit(&product_specs, digit2, ctrl_flags, true, false, Some(false)),
        "consumed-refused-select"
    );
    // Select eligible while basic is not (empty Space): dispatches.
    assert_eq!(
        triage_consumed_hit(&product_specs, digit2, ctrl_flags, true, false, Some(true)),
        "dispatch"
    );
    // Basic while only selects attest (empty Space): consumes yet refuses —
    // never dispatches a stale hidden focus.
    assert_eq!(
        triage_consumed_hit(&product_specs, KEY_H, ctrl_flags, true, false, Some(true)),
        "consumed-refused-basic"
    );
    // Relative-select (aux arm) triages as select too.
    let aux = aux_flag_for(ProbeModifier::Ctrl);
    assert_eq!(
        consumed_hit_category(&product_specs, KEY_H, ctrl_flags | aux),
        Some(true)
    );
    assert_eq!(
        triage_consumed_hit(
            &product_specs,
            KEY_H,
            ctrl_flags | aux,
            true,
            true,
            Some(false)
        ),
        "consumed-refused-select"
    );
    // Unowned key passes through (never swallowed here).
    assert_eq!(
        triage_consumed_hit(&product_specs, 0x00, ctrl_flags, true, false, Some(false)),
        "forward-unowned"
    );
    // Unarmed send shape (Ctrl+Shift+digit): no exact spec, forwards.
    assert_eq!(
        triage_consumed_hit(
            &product_specs,
            digit2,
            ctrl_flags | CG_SHIFT_BIT,
            true,
            false,
            Some(false)
        ),
        "forward-unowned"
    );
    // Ups always forward — both categories, any eligibility (hold release;
    // dropping them would wedge the orientation hold).
    assert_eq!(
        triage_consumed_hit(&basic_specs, KEY_H, ctrl_flags, false, false, Some(false)),
        "forward-up"
    );
    assert_eq!(
        triage_consumed_hit(&product_specs, digit2, ctrl_flags, false, true, Some(true)),
        "forward-up"
    );
}

#[test]
fn drain_wiring_never_escapes_refused_hits() {
    // Production drain/channel wiring: a received channel hit flows through
    // the real triage helper into the real drain mapping — the exact
    // continue-vs-return `TapTileSource::next` applies after the pump.
    // Refused hits map to `None` (the caller counts and keeps pumping, so
    // they can never escape as actions); eligible downs and all ups map to
    // the exact `RawKeyEvent` the input owner re-matches downstream.
    use crate::ffi::TapHit;
    use crate::runtime_backend::RawKeyEvent;
    let basic_specs = tap_specs_for(ProbeModifier::Ctrl, &tile_catalogue());
    let mut product = tile_catalogue();
    product.extend(crate::workspace_select::bindings());
    let product_specs = tap_specs_for(ProbeModifier::Ctrl, &product);
    let (ctrl_flags, _) = cg_flag_bits(ProbeModifier::Ctrl);
    let digit2 = crate::desktop_shortcuts::desktop_digit_keycode(2).unwrap();
    let drain =
        |specs: &[_], key_code: u32, flags: u64, down: bool, basic: bool, select: Option<bool>| {
            let hit = TapHit {
                key_code,
                down,
                flags,
            };
            let decision = triage_consumed_hit(specs, key_code, flags, down, basic, select);
            (decision, drain_received_hit(&hit, decision))
        };
    // Refused basic down: no escape (fullscreen/out-of-scope/stale facts
    // and every other basic denial).
    let (decision, event) = drain(&basic_specs, KEY_H, ctrl_flags, true, false, Some(false));
    assert_eq!(decision, "consumed-refused-basic");
    assert_eq!(event, None);
    // Refused select down: no escape.
    let (decision, event) = drain(&product_specs, digit2, ctrl_flags, true, true, Some(false));
    assert_eq!(decision, "consumed-refused-select");
    assert_eq!(event, None);
    // Eligible basic down escapes with the exact edge/flags/code.
    let (decision, event) = drain(&basic_specs, KEY_H, ctrl_flags, true, true, Some(false));
    assert_eq!(decision, "dispatch");
    assert_eq!(
        event,
        Some(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags: ctrl_flags,
            key_code: KEY_H,
        })
    );
    // Eligible select down escapes too.
    let (decision, event) = drain(&product_specs, digit2, ctrl_flags, true, false, Some(true));
    assert_eq!(decision, "dispatch");
    assert_eq!(
        event,
        Some(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags: ctrl_flags,
            key_code: digit2,
        })
    );
    // Ineligible up escapes as KeyUp (hold release downstream).
    let (decision, event) = drain(&basic_specs, KEY_H, ctrl_flags, false, false, Some(false));
    assert_eq!(decision, "forward-up");
    assert_eq!(
        event,
        Some(RawKeyEvent {
            kind: TapKeyKind::KeyUp,
            flags: ctrl_flags,
            key_code: KEY_H,
        })
    );
    // Unowned down escapes (never swallowed here; the input owner
    // re-matches exactly and ignores it).
    let (decision, event) = drain(&product_specs, 0x00, ctrl_flags, true, false, Some(false));
    assert_eq!(decision, "forward-unowned");
    assert!(event.is_some());
}

#[test]
fn factory_wires_two_distinct_caches() {
    // Both caches start ineligible on the normal path (distinct handles —
    // publishing one never moves the other); the fixture gate arms both.
    let tap = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue()).unwrap();
    assert!(!tap.eligibility().is_eligible());
    assert!(!tap.eligibility_select().is_eligible());
    assert!(!Arc::ptr_eq(&tap.eligibility(), &tap.eligibility_select()));
    tap.eligibility().publish(true, 4242);
    assert!(tap.eligibility().is_eligible());
    assert!(!tap.eligibility_select().is_eligible());
    let gate = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_unrestricted_test_gate();
    assert!(gate.eligibility().is_unrestricted());
    assert!(gate.eligibility_select().is_unrestricted());
    assert!(gate.eligibility().is_eligible());
    assert!(gate.eligibility_select().is_eligible());
}
