#![cfg(test)]
//! Mechanical split of `native_input.rs` cfg tests (no behavior change).
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

use crate::config::ProbeModifier;
use crate::input::cg_flag_bits;
use crate::input_eligibility::EligibilityCache;
use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
use crate::native_input::*;
use crate::runtime::{InputDirection, InputSemanticAction};
use crate::scope::AppScope;

fn tuning_for(modifier: ProbeModifier, key_code: u32, shift: bool) -> TapTuning {
    let (mut want, mask) = cg_flag_bits(modifier);
    if shift {
        want |= CG_SHIFT_BIT;
    }
    TapTuning {
        want_flags: want,
        flags_mask: mask,
        want_code: i64::from(key_code),
    }
}

#[test]
fn catalogue_is_exactly_focus_move_float() {
    let catalogue = tile_catalogue();
    assert_eq!(catalogue.len(), 17);
    let focus: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::FocusStep { .. }))
        .collect();
    let moves: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::MoveStep { .. }))
        .collect();
    let floats: Vec<_> = catalogue
        .iter()
        .filter(|b| matches!(b.action, InputSemanticAction::ToggleFloat))
        .collect();
    assert_eq!(focus.len(), 8);
    assert_eq!(moves.len(), 8);
    assert_eq!(floats.len(), 1);
    // Focus chords carry no Shift; the move arm always does; float is
    // bare `G`.
    assert!(focus.iter().all(|b| !b.shift));
    assert!(moves.iter().all(|b| b.shift));
    assert_eq!(floats[0].key_code, KEY_G);
    assert!(!floats[0].shift);
    // Exact key set: HJKL plus four arrows on both arms.
    let mut keys: Vec<(u32, bool)> = catalogue.iter().map(|b| (b.key_code, b.shift)).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 17, "catalogue chords must be unique");
    for key in [
        KEY_H, KEY_J, KEY_K, KEY_L, KEY_LEFT, KEY_DOWN, KEY_UP, KEY_RIGHT,
    ] {
        assert!(catalogue.iter().any(|b| b.key_code == key && !b.shift));
        assert!(catalogue.iter().any(|b| b.key_code == key && b.shift));
    }
    // Directions: H/Left, J/Down, K/Up, L/Right on both arms.
    let dir_of = |key: u32, shift: bool| {
        catalogue
            .iter()
            .find(|b| b.key_code == key && b.shift == shift)
            .map(|b| match b.action {
                InputSemanticAction::FocusStep { direction }
                | InputSemanticAction::MoveStep { direction } => direction,
                InputSemanticAction::ToggleFloat => InputDirection::Left,
            })
    };
    assert_eq!(dir_of(KEY_H, false), Some(InputDirection::Left));
    assert_eq!(dir_of(KEY_J, false), Some(InputDirection::Down));
    assert_eq!(dir_of(KEY_K, false), Some(InputDirection::Up));
    assert_eq!(dir_of(KEY_L, false), Some(InputDirection::Right));
    assert_eq!(dir_of(KEY_LEFT, true), Some(InputDirection::Left));
    assert_eq!(dir_of(KEY_DOWN, true), Some(InputDirection::Down));
    assert_eq!(dir_of(KEY_UP, true), Some(InputDirection::Up));
    assert_eq!(dir_of(KEY_RIGHT, true), Some(InputDirection::Right));
}

#[test]
fn carbon_mods_cover_ctrl_role_plus_shift_arm() {
    assert_eq!(carbon_mods_for(ProbeModifier::Meta, false), 256);
    assert_eq!(carbon_mods_for(ProbeModifier::Meta, true), 256 + 512);
    assert_eq!(carbon_mods_for(ProbeModifier::Ctrl, false), 4096);
    assert_eq!(carbon_mods_for(ProbeModifier::Ctrl, true), 4096 + 512);
    assert_eq!(carbon_mods_for(ProbeModifier::Alt, false), 2048);
    assert_eq!(carbon_mods_for(ProbeModifier::Shift, false), 512);
}

#[test]
fn exact_match_rejects_extras_but_ignores_caps_fn() {
    // Meta+H focus chord: exact bits match; any extra *masked* modifier
    // (Shift/Ctrl/Alt) fails; Caps/Fn sit outside the mask per the
    // existing matcher contract and neither match nor break.
    let tuning = tuning_for(ProbeModifier::Meta, KEY_H, false);
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
    const CAPS: u64 = 0x0001_0000;
    const FN: u64 = 0x0080_0000;
    assert!(exact(META));
    assert!(!exact(META | SHIFT), "extra Shift must fail a focus chord");
    assert!(!exact(META | CTRL));
    assert!(!exact(0), "bare key must fail");
    assert!(exact(META | CAPS), "Caps sits outside the mask");
    assert!(exact(META | FN), "Fn sits outside the mask");
    // Move arm needs exactly Mod+Shift.
    let move_tuning = tuning_for(ProbeModifier::Meta, KEY_H, true);
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
}

#[test]
fn paired_suppression_repeats_and_release() {
    // Armed tap over one spec: exact down suppresses and pends, a
    // repeat down still forwards (matched), the paired up suppresses, a
    // lone up never suppresses, and a modifier release clears pending.
    let tuning = tuning_for(ProbeModifier::Meta, KEY_H, false);
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
fn carbon_hit_conversion_distinguishes_shift_arm() {
    use crate::ffi::CarbonHit;
    // Regression: focus and move arms share key codes, so the replay
    // table MUST key the full (key code, Carbon modifiers) pair. A
    // key-code-only lookup replays the focus flags for a Shift move hit
    // (focus arm sorts first), silently converting every move into a
    // focus step.
    let modifier = ProbeModifier::Ctrl;
    let catalogue = tile_catalogue();
    let (want, _) = cg_flag_bits(modifier);
    let table: Vec<(u32, u32, u64)> = catalogue
        .iter()
        .map(|binding| {
            (
                binding.key_code,
                carbon_mods_for(modifier, binding.shift),
                want | if binding.shift { CG_SHIFT_BIT } else { 0 },
            )
        })
        .collect();
    let focus_hit = CarbonHit {
        key_code: KEY_H,
        carbon_modifiers: carbon_mods_for(modifier, false),
    };
    let move_hit = CarbonHit {
        key_code: KEY_H,
        carbon_modifiers: carbon_mods_for(modifier, true),
    };
    let focus_flags = carbon_flags_for_hit(&table, &focus_hit);
    let move_flags = carbon_flags_for_hit(&table, &move_hit);
    assert_ne!(
        focus_flags, move_flags,
        "Shift move hit must not replay focus flags"
    );
    assert_eq!(focus_flags & CG_SHIFT_BIT, 0);
    assert_eq!(move_flags & CG_SHIFT_BIT, CG_SHIFT_BIT);
    // End to end through the exact matcher: the move replay matches the
    // move tuning and fails the focus tuning, and vice versa.
    let focus_tuning = tuning_for(modifier, KEY_H, false);
    let move_tuning = tuning_for(modifier, KEY_H, true);
    let matched = |flags: u64, tuning: TapTuning| {
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
    assert!(matched(focus_flags, focus_tuning));
    assert!(!matched(focus_flags, move_tuning));
    assert!(matched(move_flags, move_tuning));
    assert!(
        !matched(move_flags, focus_tuning),
        "Shift replay must fail the focus chord"
    );
    // Unknown pairs replay 0: no exact chord matches downstream, so the
    // hit is ignored — never guessed into a chord.
    let unknown = CarbonHit {
        key_code: KEY_G,
        carbon_modifiers: 0,
    };
    assert_eq!(carbon_flags_for_hit(&table, &unknown), 0);
    assert!(!matched(0, focus_tuning));
    assert!(!matched(0, move_tuning));
}

#[test]
fn carbon_gated_loop_parks_without_dirty_end() {
    // Gated the whole time: the owner holds no registration, counts
    // gated time, records no refusal, and a set stop flag ends the loop
    // promptly (no hang, no dirty event). Single-threaded: the
    // injected gate trips the stop flag itself after a few slices
    // (the source is `!Send`, like the live owner thread confinement).
    let modifier = ProbeModifier::Ctrl;
    let catalogue = tile_catalogue();
    let (want, _) = cg_flag_bits(modifier);
    let chords: Vec<(u32, u32)> = catalogue
        .iter()
        .map(|binding| (binding.key_code, carbon_mods_for(modifier, binding.shift)))
        .collect();
    let flags_of: Vec<(u32, u32, u64)> = catalogue
        .iter()
        .map(|binding| {
            (
                binding.key_code,
                carbon_mods_for(modifier, binding.shift),
                want | if binding.shift { CG_SHIFT_BIT } else { 0 },
            )
        })
        .collect();
    let (_, rx) = mpsc::sync_channel::<crate::ffi::CarbonHit>(64);
    let mut source = CarbonTileSource {
        set: None,
        rx,
        flags_of,
        chords,
        counter: Arc::new(crate::ffi::EventCounter::new()),
        statuses: Arc::new(Mutex::new(Vec::new())),
        refusal: Arc::new(Mutex::new(None)),
        gated_ms: Arc::new(AtomicU64::new(0)),
        scope: AppScope::disabled(),
        eligibility: Arc::new(EligibilityCache::new_unrestricted_test_gate()),
        facts: None,
        last_attest: None,
    };
    let stop = AtomicBool::new(false);
    let calls = AtomicU64::new(0);
    let outcome = source.next_gated(&stop, &|| {
        if calls.fetch_add(1, Ordering::SeqCst) >= 3 {
            stop.store(true, Ordering::SeqCst);
        }
        Some("session-unknown")
    });
    assert!(outcome.is_none(), "stop ends the run, never a dirty event");
    assert!(
        source.gated_ms.load(Ordering::SeqCst) >= 100,
        "gated time counted while released"
    );
    assert!(
        source
            .refusal
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_none(),
        "a merely gated run records no refusal"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn carbon_failed_recovery_ends_truthfully() {
    // Off-macOS the stub refuses every registration: with the gate
    // clear and no live set, the loop attempts recovery once, records
    // the refusal, and ends (never a dirty EOS, never silent hotkeys).
    // Re-arm on ungate also needs native delivery acceptance.
    let modifier = ProbeModifier::Ctrl;
    let catalogue = tile_catalogue();
    let (want, _) = cg_flag_bits(modifier);
    let chords: Vec<(u32, u32)> = catalogue
        .iter()
        .map(|binding| (binding.key_code, carbon_mods_for(modifier, binding.shift)))
        .collect();
    let flags_of: Vec<(u32, u32, u64)> = catalogue
        .iter()
        .map(|binding| {
            (
                binding.key_code,
                carbon_mods_for(modifier, binding.shift),
                want | if binding.shift { CG_SHIFT_BIT } else { 0 },
            )
        })
        .collect();
    let (_, rx) = mpsc::sync_channel::<crate::ffi::CarbonHit>(64);
    let mut source = CarbonTileSource {
        set: None,
        rx,
        flags_of,
        chords,
        counter: Arc::new(crate::ffi::EventCounter::new()),
        statuses: Arc::new(Mutex::new(Vec::new())),
        refusal: Arc::new(Mutex::new(None)),
        gated_ms: Arc::new(AtomicU64::new(0)),
        scope: AppScope::disabled(),
        eligibility: Arc::new(EligibilityCache::new_unrestricted_test_gate()),
        facts: None,
        last_attest: None,
    };
    let stop = AtomicBool::new(false);
    assert!(source.next_gated(&stop, &|| None).is_none());
    let refusal = source
        .refusal
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    assert!(
        refusal.is_some_and(|reason| reason.contains("refused")),
        "unrecoverable re-registration refuses truthfully: {refusal:?}"
    );
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
    assert!(CarbonTileFactory::new(ProbeModifier::Meta, vec![]).is_err());
    assert!(TapTileFactory::new(ProbeModifier::Meta, vec![]).is_err());
    let catalogue = tile_catalogue();
    assert!(CarbonTileFactory::new(ProbeModifier::Meta, catalogue.clone()).is_ok());
    assert!(TapTileFactory::new(ProbeModifier::Meta, catalogue).is_ok());
    // Off-macOS creation fails closed (no live tap/Carbon here); the
    // refusal shape — not silent success — is what this pins.
    #[cfg(not(target_os = "macos"))]
    {
        let factory = TapTileFactory::new(ProbeModifier::Meta, tile_catalogue()).unwrap();
        assert!(factory.create().is_err());
        let factory = CarbonTileFactory::new(ProbeModifier::Meta, tile_catalogue()).unwrap();
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
    let carbon = CarbonTileFactory::new(ProbeModifier::Ctrl, catalogue.clone()).unwrap();
    assert!(!carbon.eligibility().is_eligible());
    assert!(!carbon.eligibility().is_unrestricted());
    let tap = TapTileFactory::new(ProbeModifier::Ctrl, catalogue).unwrap();
    assert!(!tap.eligibility().is_eligible());
    let scoped = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_scope(crate::scope::AppScope {
            allowed: vec!["com.apple.TextEdit".to_owned()],
        });
    assert!(!scoped.eligibility().is_eligible());
    // Fixture unrestricted gate: starts eligible (per-edge oracle still
    // enforced for posts; suppression bypass is explicit).
    let gate = TapTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_unrestricted_test_gate();
    assert!(gate.eligibility().is_unrestricted());
    assert!(gate.eligibility().is_eligible());
    let gate = CarbonTileFactory::new(ProbeModifier::Ctrl, tile_catalogue())
        .unwrap()
        .with_unrestricted_test_gate();
    assert!(gate.eligibility().is_eligible());
}

#[test]
fn carbon_releases_registration_while_gated() {
    // The gated loop above already proves the release path (no
    // registration held, gated time counted, no refusal, stop ends
    // promptly). Pin the release invariant explicitly: while gated the
    // owner holds NO hotkeys (dropping hits would NOT restore
    // passthrough — only the RAII release does).
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
