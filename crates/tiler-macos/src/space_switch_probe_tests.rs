#![cfg(test)]
//! Offline tests for the same-display Space switch comparison: argument
//! gates, leg verdicts, route eligibility, terminal evidence keys,
//! membership exactness, unknown-frame semantics, and residue outcomes.
//! Pure data only — no live dispatch, no windows touched.

use crate::space_switch_probe::*;

#[test]
fn switch_args_require_opt_in_distinct_sids_and_budget() {
    assert!(validate_switch_args(1, 66, 120, false).is_err());
    assert!(validate_switch_args(0, 66, 120, true).is_err());
    assert!(validate_switch_args(1, 1, 120, true).is_err());
    assert!(validate_switch_args(1, 66, 45, true).is_err());
    assert!(validate_switch_args(1, 66, 121, true).is_err());
    assert_eq!(validate_switch_args(1, 66, 120, true).unwrap(), 120);
    assert_eq!(validate_switch_args(1, 66, 60, true).unwrap(), 60);
}

#[test]
fn leg_verdict_never_retries_noeffect() {
    // Exact goal: the only success. Stable preimage: observed
    // no-effect (stop, never repeat, never invert). Anything else:
    // ambiguous (MC AX recovery, never a synthetic inverse).
    assert_eq!(classify_leg(Some(66), 66, Some(1)), LegVerdict::Switched);
    assert_eq!(classify_leg(Some(1), 66, Some(1)), LegVerdict::NoEffect);
    assert_eq!(classify_leg(Some(7), 66, Some(1)), LegVerdict::Ambiguous);
    assert_eq!(classify_leg(None, 66, Some(1)), LegVerdict::Ambiguous);
}

#[test]
fn mapping_is_unsupported_without_effective_corroboration() {
    // Fakes protect: a disabled/unresolved chord never becomes an
    // implemented leg. The harness reports `unsupported`, never a
    // dispatched switch.
    let no_arrows = crate::symbolic_hotkeys::corroborate_entries(&[]);
    assert!(!no_arrows.both_enabled());
}

#[test]
fn early_report_carries_every_terminal_evidence_key() {
    // Blocker 1: no silent orphan, no thin error — every setup failure
    // reports exact stops, absence, currents, Ghostty, MC, caffeinate.
    let body = early_report(
        "spawn-t",
        "spawn-t: helper spawn failed: x",
        1,
        0,
        vec![
            serde_json::json!({"pid": 4242, "exited": true, "reaped": true, "record_removed": true}),
        ],
        true,
        true,
        true,
        Some(false),
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    for key in [
        "stage",
        "action",
        "error",
        "original",
        "display",
        "helper_stops",
        "helpers_absent",
        "currents_restored",
        "ghostty_stable",
        "mc_open_after",
        "caffeinate_reaped",
        "synthetic_events_posted",
        "mutates",
    ] {
        assert!(v.get(key).is_some(), "early report missing key {key}");
    }
    assert_eq!(v["synthetic_events_posted"], false);
    assert_eq!(v["helpers_absent"], true);
    assert_eq!(v["caffeinate_reaped"], true);
}

#[test]
fn wrong_single_space_is_drift_not_unchanged() {
    // Blocker 2: exact expected SIDs — a helper sitting on the wrong
    // single Space (even still single) never reads as unchanged.
    use crate::space_recovery::membership_is_exact;
    assert!(membership_is_exact(&[1], 1));
    assert!(membership_is_exact(&[66], 66));
    assert!(!membership_is_exact(&[66], 1));
    assert!(!membership_is_exact(&[1], 66));
    assert!(!membership_is_exact(&[1, 66], 1));
    assert!(!membership_is_exact(&[], 1));
}

#[test]
fn unreadable_frame_is_unknown_never_false_changed() {
    let f = crate::ffi::AxFrame {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
    };
    let g = crate::ffi::AxFrame {
        x: 1.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
    };
    assert_eq!(
        classify_frames(Ok(f), Some(f), Ok(f), Some(f)),
        (Some(true), None)
    );
    assert_eq!(
        classify_frames(Ok(g), Some(f), Ok(f), Some(f)).0,
        Some(false)
    );
    let (frames, err) = classify_frames(Err("AX read: -25201".to_owned()), Some(f), Ok(f), Some(f));
    assert_eq!(frames, None);
    assert!(err.unwrap().contains('O'));
    let (frames, _) = classify_frames(Err("a".to_owned()), Some(f), Err("b".to_owned()), Some(f));
    assert_eq!(frames, None);
}

#[test]
fn unresolved_synthetic_is_always_residue() {
    // Blocker 5: an unreleased edge can never report clean, even when
    // currents, helpers, and Ghostty otherwise verify.
    assert_eq!(
        probe_outcome(true, true, true, false),
        "switch-probe-complete"
    );
    assert_eq!(
        probe_outcome(true, true, true, true),
        "switch-probe-residue"
    );
    assert_eq!(
        probe_outcome(false, true, true, false),
        "switch-probe-residue"
    );
    assert_eq!(
        probe_outcome(true, false, true, false),
        "switch-probe-residue"
    );
    assert_eq!(
        probe_outcome(true, true, false, false),
        "switch-probe-residue"
    );
}
