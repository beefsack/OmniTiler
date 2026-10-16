#![forbid(unsafe_code)]

//! Ctrl-chord matching for the configurable modifier.
//!
//! Pure, total functions so boundedness/input-matching is unit-testable
//! without live taps. Key codes are Mac virtual keycodes (`CGKeyCode`
//! space, e.g. `0x04` is the ANSI `h` position).

use crate::config::ProbeModifier;

/// A registered probe chord: modifier filling the Ctrl role + key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub modifier: ProbeModifier,
    pub key_code: u32,
}

impl Chord {
    pub fn new(modifier: ProbeModifier, key_code: u32) -> Result<Self, String> {
        if key_code > 255 {
            return Err(format!("key_code {key_code} out of range 0..=255"));
        }
        Ok(Self { modifier, key_code })
    }

    /// True only when both modifier and key match exactly. No prefix or
    /// subset matching: prevents over-interception.
    pub fn matches(&self, modifier: ProbeModifier, key_code: u32) -> bool {
        self.modifier == modifier && self.key_code == key_code
    }
}

/// Elapsed-seconds gate for bounded tap/observe windows.
pub fn duration_within_bound(elapsed_secs: u64, max_secs: u64) -> bool {
    elapsed_secs <= max_secs
}

/// Key-event kinds the tap subscribes to. `Other` covers `FlagsChanged` and
/// any future subscribed type: never matched, never suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapKeyKind {
    KeyDown,
    KeyUp,
    Other,
}

/// Exact-chord tuning for one tap run: `(flags & flags_mask) == want_flags`
/// plus an exact key-code compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapTuning {
    pub want_flags: u64,
    pub flags_mask: u64,
    pub want_code: i64,
}

/// Provenance tag for the tiler's OWN native Space-switch chords.
///
/// Set as `kCGEventSourceUserData` (field 42) on every synthetic switch
/// chord the tiler posts for a drag transfer (arrow down/up, gesture
/// began/ended, future drag-chord posts). Tile-catalogue posts
/// ([`crate::synth::post_guarded`]) and all physical input carry no tag, so
/// the tap can tell our switch chords apart from the user's identical
/// chords: own tagged switch chords pass (`Keep`), user chords still
/// consume per the catalogue. The value spells "OMNITILE" in ASCII
/// (distinct from any PID/UID the field normally carries); nonzero so an
/// unset field never matches.
pub const OWN_SWITCH_USER_DATA: i64 = 0x4F4E_4E49_5449_4C45;

/// Mac virtual keycodes for the relative-switch arrow legs (`CGKeyCode`
/// space; pinned equal to [`crate::input_policy::KEY_LEFT`] /
/// [`crate::input_policy::KEY_RIGHT`]).
pub const SWITCH_KEY_LEFT: u32 = 0x7B;
pub const SWITCH_KEY_RIGHT: u32 = 0x7C;
/// `CGEventFlags` Control bit plus the exact-chord mask for the switch
/// legs (CG domain, never Carbon; same bits as [`crate::input::cg_flag_bits`]
/// for Ctrl).
pub const SWITCH_CTRL_BIT: u64 = 0x0004_0000;
pub const SWITCH_FLAGS_MASK: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000;

/// Pure own-switch provenance over plain data: true only for the tiler's
/// own tagged native Space-switch chord shape -- exact tag, KeyDown/KeyUp,
/// exact Ctrl with no extra masked modifier, plus a switch keycode: the
/// Left/Right arrows (relative switch) or the Desktop-N digits `1`-`9`
/// (`0` for Desktop 10, exact enabled `Ctrl+digit` chords). Everything
/// else (untagged user chords, untagged/tagged tile-catalogue chords that
/// are not exactly this shape, FlagsChanged, letters) is NOT an own switch
/// chord and follows the normal match path. No live touch.
#[must_use]
pub fn is_own_switch_chord(user_data: i64, kind: TapKeyKind, flags: u64, code: i64) -> bool {
    if user_data != OWN_SWITCH_USER_DATA {
        return false;
    }
    if !matches!(kind, TapKeyKind::KeyDown | TapKeyKind::KeyUp) {
        return false;
    }
    if (flags & SWITCH_FLAGS_MASK) != SWITCH_CTRL_BIT {
        return false;
    }
    if code == i64::from(SWITCH_KEY_LEFT) || code == i64::from(SWITCH_KEY_RIGHT) {
        return true;
    }
    u32::try_from(code).is_ok_and(crate::desktop_shortcuts::is_digit_keycode)
}

/// Pure per-event tap decision with own-switch provenance. An own tagged
/// switch chord always passes (`matched: false, suppress: false`) WITHOUT
/// touching `down_pending` -- it never arms suppression and never forwards
/// to the engine. Every other event follows [`decide_tap_event`] unchanged,
/// so user chords (untagged) and own tile-catalogue posts (untagged, or
/// tagged but not the switch shape) keep exact-chord suppression.
pub fn decide_tap_event_with_provenance(
    kind: TapKeyKind,
    flags: u64,
    code: i64,
    user_data: i64,
    tuning: TapTuning,
    suppress_armed: bool,
    down_pending: &mut bool,
) -> TapDecision {
    if is_own_switch_chord(user_data, kind, flags, code) {
        return TapDecision {
            matched: false,
            suppress: false,
        };
    }
    decide_tap_event(kind, flags, code, tuning, suppress_armed, down_pending)
}
/// Pure per-event tap decision. Suppression is armed only by the explicit
/// CLI `--suppress` flag and applies ONLY to exact-chord `KeyDown`/`KeyUp`
/// pairs: a `KeyDown` match arms `down_pending`; the matching `KeyUp`
/// suppresses only when pending. Unrelated keys, modifiers, and
/// `FlagsChanged` never suppress, and a `KeyUp` without its `KeyDown` never
/// suppresses. `matched` forwards to the engine channel (exact chord only,
/// never raw deliveries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapDecision {
    pub matched: bool,
    pub suppress: bool,
}

/// Pure per-category tap gate over already-read eligibility bits — the
/// testable core of the tap callback's scoped gate. `select_hit` is the
/// probe's match category (pure: `decide_tap_event`'s `matched` never
/// depends on pending bits, so the probe agrees with the match loop);
/// `basic_eligible` is the basic cache read; `select_eligible` is the
/// select cache read (`None` falls back to the basic bit for tables
/// without select chords). A select hit passes only on the select bit —
/// basic chords on an empty Space therefore pass through to the app and
/// can never forward stale hidden focus — while a basic hit passes only
/// on the basic bit. Unmatched events are decided downstream, never here.
#[must_use]
pub const fn tap_category_allowed(
    select_hit: bool,
    basic_eligible: bool,
    select_eligible: Option<bool>,
) -> bool {
    if select_hit {
        match select_eligible {
            Some(eligible) => eligible,
            None => basic_eligible,
        }
    } else {
        basic_eligible
    }
}

pub fn decide_tap_event(
    kind: TapKeyKind,
    flags: u64,
    code: i64,
    tuning: TapTuning,
    suppress_armed: bool,
    down_pending: &mut bool,
) -> TapDecision {
    let exact = (flags & tuning.flags_mask) == tuning.want_flags && code == tuning.want_code;
    match kind {
        TapKeyKind::Other => {
            // A modifier release dissolves a pending down (chord broken
            // before its up); nothing here ever matches or suppresses.
            if (flags & tuning.flags_mask) != tuning.want_flags {
                *down_pending = false;
            }
            TapDecision {
                matched: false,
                suppress: false,
            }
        }
        TapKeyKind::KeyDown => {
            if !exact {
                return TapDecision {
                    matched: false,
                    suppress: false,
                };
            }
            if suppress_armed {
                *down_pending = true;
                TapDecision {
                    matched: true,
                    suppress: true,
                }
            } else {
                TapDecision {
                    matched: true,
                    suppress: false,
                }
            }
        }
        TapKeyKind::KeyUp => {
            if !exact {
                // Chord broken (e.g. modifier lifted first): drop pending.
                *down_pending = false;
                return TapDecision {
                    matched: false,
                    suppress: false,
                };
            }
            if suppress_armed && *down_pending {
                *down_pending = false;
                TapDecision {
                    matched: true,
                    suppress: true,
                }
            } else {
                TapDecision {
                    matched: true,
                    suppress: false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_only() {
        let chord = Chord::new(ProbeModifier::Meta, 0x04).unwrap();
        assert!(chord.matches(ProbeModifier::Meta, 0x04));
        assert!(!chord.matches(ProbeModifier::Ctrl, 0x04));
        assert!(!chord.matches(ProbeModifier::Meta, 0x05));
    }

    #[test]
    fn rejects_bad_keycode() {
        assert!(Chord::new(ProbeModifier::Meta, 256).is_err());
    }

    #[test]
    fn duration_bound_inclusive() {
        assert!(duration_within_bound(30, 30));
        assert!(!duration_within_bound(31, 30));
    }

    fn tuning() -> TapTuning {
        TapTuning {
            want_flags: 0x0010_0000,
            flags_mask: 0x001E_0000,
            want_code: 0x04,
        }
    }

    #[test]
    fn exact_pair_suppresses_only_when_armed() {
        let t = tuning();
        // Observe-only (default): match, never suppress.
        let mut pending = false;
        let d = decide_tap_event(
            TapKeyKind::KeyDown,
            0x0010_0000,
            0x04,
            t,
            false,
            &mut pending,
        );
        assert_eq!(
            d,
            TapDecision {
                matched: true,
                suppress: false,
            }
        );
        assert!(!pending);
        // Armed: down suppresses and pends; paired up suppresses.
        let mut pending = false;
        let d = decide_tap_event(
            TapKeyKind::KeyDown,
            0x0010_0000,
            0x04,
            t,
            true,
            &mut pending,
        );
        assert!(d.matched && d.suppress && pending);
        let d = decide_tap_event(TapKeyKind::KeyUp, 0x0010_0000, 0x04, t, true, &mut pending);
        assert!(d.matched && d.suppress && !pending);
    }

    #[test]
    fn unrelated_keys_modifiers_and_flags_never_suppress() {
        let t = tuning();
        let mut pending = false;
        // Wrong key, wrong modifier, extra modifier: no match, no suppress.
        for (flags, code) in [
            (0x0010_0000, 0x05),
            (0x0004_0000, 0x04),
            (0x0010_0000 | 0x0002_0000, 0x04),
            (0x0000_0000, 0x04),
        ] {
            for kind in [TapKeyKind::KeyDown, TapKeyKind::KeyUp] {
                let d = decide_tap_event(kind, flags, code, t, true, &mut pending);
                assert_eq!(
                    d,
                    TapDecision {
                        matched: false,
                        suppress: false,
                    },
                    "kind={kind:?} flags={flags:#x} code={code:#x}"
                );
            }
            // FlagsChanged-shape events never match or suppress, even armed.
            let d = decide_tap_event(TapKeyKind::Other, flags, code, t, true, &mut pending);
            assert!(!d.matched && !d.suppress);
        }
        assert!(!pending);
    }

    #[test]
    fn lone_keyup_never_suppresses() {
        let t = tuning();
        let mut pending = false;
        // Exact chord KeyUp with no prior down: matched (observed) but kept.
        let d = decide_tap_event(TapKeyKind::KeyUp, 0x0010_0000, 0x04, t, true, &mut pending);
        assert_eq!(
            d,
            TapDecision {
                matched: true,
                suppress: false,
            }
        );
    }

    #[test]
    fn broken_chord_clears_pending_without_suppression() {
        let t = tuning();
        let mut pending = false;
        let d = decide_tap_event(
            TapKeyKind::KeyDown,
            0x0010_0000,
            0x04,
            t,
            true,
            &mut pending,
        );
        assert!(d.suppress && pending);
        // Modifier lifted first (inexact up): no suppress, pending cleared,
        // so a later exact up cannot suppress on its own.
        let d = decide_tap_event(TapKeyKind::KeyUp, 0x0000_0000, 0x04, t, true, &mut pending);
        assert!(!d.matched && !d.suppress && !pending);
        let d = decide_tap_event(TapKeyKind::KeyUp, 0x0010_0000, 0x04, t, true, &mut pending);
        assert!(d.matched && !d.suppress);
    }

    fn switch_tuning() -> TapTuning {
        // Tile-catalogue focus-right under the Ctrl modifier: exact
        // Ctrl+Right. The user's chord must consume; ours must pass.
        TapTuning {
            want_flags: SWITCH_CTRL_BIT,
            flags_mask: SWITCH_FLAGS_MASK,
            want_code: i64::from(SWITCH_KEY_RIGHT),
        }
    }

    #[test]
    fn own_tag_is_nonzero_and_distinct_from_unset() {
        assert_ne!(OWN_SWITCH_USER_DATA, 0);
        // Switch legs pin the shared domains: CG Control bit, arrow codes.
        assert_eq!(SWITCH_CTRL_BIT, 0x0004_0000);
        assert_eq!(SWITCH_KEY_LEFT, crate::input_policy::KEY_LEFT);
        assert_eq!(SWITCH_KEY_RIGHT, crate::input_policy::KEY_RIGHT);
    }

    #[test]
    fn own_switch_shape_needs_tag_chord_and_arrows() {
        use TapKeyKind::{KeyDown, KeyUp, Other};
        let down = |user_data: i64, kind: TapKeyKind, flags: u64, code: i64| {
            is_own_switch_chord(user_data, kind, flags, code)
        };
        // Own tagged Ctrl+arrows pass on both edges.
        assert!(down(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT)
        ));
        assert!(down(
            OWN_SWITCH_USER_DATA,
            KeyUp,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_LEFT)
        ));
        // Untagged (user) identical chords are not ours.
        assert!(!down(
            0,
            KeyDown,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT)
        ));
        // Wrong tag, wrong key, extra modifier, FlagsChanged, letters:
        // not ours. (Desktop-N digits have their own test below.)
        assert!(!down(
            1,
            KeyDown,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT)
        ));
        assert!(!down(OWN_SWITCH_USER_DATA, KeyDown, SWITCH_CTRL_BIT, 0x04));
        assert!(!down(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            SWITCH_CTRL_BIT | 0x0002_0000,
            i64::from(SWITCH_KEY_RIGHT)
        ));
        assert!(!down(
            OWN_SWITCH_USER_DATA,
            Other,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT)
        ));
        assert!(!down(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            0,
            i64::from(SWITCH_KEY_RIGHT)
        ));
    }

    #[test]
    fn own_switch_chords_pass_while_user_chords_consume() {
        let t = switch_tuning();
        // User Ctrl+Right (untagged), armed: down suppresses and pends,
        // paired up suppresses -- the containment proof is unchanged.
        let mut pending = false;
        let d = decide_tap_event_with_provenance(
            TapKeyKind::KeyDown,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT),
            0,
            t,
            true,
            &mut pending,
        );
        assert!(d.matched && d.suppress && pending);
        let d = decide_tap_event_with_provenance(
            TapKeyKind::KeyUp,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT),
            0,
            t,
            true,
            &mut pending,
        );
        assert!(d.matched && d.suppress && !pending);
        // Our tagged Ctrl+Right pair, armed: passes with no match, no
        // suppress, and never arms pending -- the tap never sees it.
        let mut pending = false;
        let d = decide_tap_event_with_provenance(
            TapKeyKind::KeyDown,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT),
            OWN_SWITCH_USER_DATA,
            t,
            true,
            &mut pending,
        );
        assert_eq!(
            d,
            TapDecision {
                matched: false,
                suppress: false,
            }
        );
        assert!(!pending);
        let d = decide_tap_event_with_provenance(
            TapKeyKind::KeyUp,
            SWITCH_CTRL_BIT,
            i64::from(SWITCH_KEY_RIGHT),
            OWN_SWITCH_USER_DATA,
            t,
            true,
            &mut pending,
        );
        assert_eq!(
            d,
            TapDecision {
                matched: false,
                suppress: false,
            }
        );
        assert!(!pending);
    }

    #[test]
    fn own_tag_on_non_switch_shapes_still_consumes() {
        // A tagged event that is NOT the switch shape (tile-catalogue H
        // chord shape here) follows the normal path: exact match consumes
        // when armed. Tagging is switch-scoped, never a blanket bypass --
        // this pins the input-session arrow/tile suppression regression.
        let t = TapTuning {
            want_flags: SWITCH_CTRL_BIT,
            flags_mask: SWITCH_FLAGS_MASK,
            want_code: 0x04,
        };
        let mut pending = false;
        let d = decide_tap_event_with_provenance(
            TapKeyKind::KeyDown,
            SWITCH_CTRL_BIT,
            0x04,
            OWN_SWITCH_USER_DATA,
            t,
            true,
            &mut pending,
        );
        assert!(d.matched && d.suppress && pending);
    }

    #[test]
    fn own_tagged_desktop_digits_pass_while_user_digits_consume() {
        // Exact enabled Desktop-N chords (Ctrl+1..9,0) carry the same
        // provenance contract as the arrow legs: our tagged posts pass, the
        // user's untagged identical chords consume per the catalogue.
        let digit_tuning = |code: u32| TapTuning {
            want_flags: SWITCH_CTRL_BIT,
            flags_mask: SWITCH_FLAGS_MASK,
            want_code: i64::from(code),
        };
        for desktop in 1..=10 {
            let code = crate::desktop_shortcuts::desktop_digit_keycode(desktop).unwrap();
            let t = digit_tuning(code);
            // User digit (untagged), armed: consumes like any catalogue
            // chord.
            let mut pending = false;
            let d = decide_tap_event_with_provenance(
                TapKeyKind::KeyDown,
                SWITCH_CTRL_BIT,
                i64::from(code),
                0,
                t,
                true,
                &mut pending,
            );
            assert!(d.matched && d.suppress && pending, "desktop={desktop}");
            // Own tagged digit pair: passes with no match, no suppress,
            // never arms pending.
            let mut pending = false;
            for kind in [TapKeyKind::KeyDown, TapKeyKind::KeyUp] {
                let d = decide_tap_event_with_provenance(
                    kind,
                    SWITCH_CTRL_BIT,
                    i64::from(code),
                    OWN_SWITCH_USER_DATA,
                    t,
                    true,
                    &mut pending,
                );
                assert_eq!(
                    d,
                    TapDecision {
                        matched: false,
                        suppress: false,
                    },
                    "desktop={desktop} kind={kind:?}"
                );
                assert!(!pending);
            }
        }
    }

    #[test]
    fn desktop_digit_provenance_needs_exact_shape() {
        use TapKeyKind::{KeyDown, KeyUp, Other};
        let digit = i64::from(crate::desktop_shortcuts::KEY_2);
        // Extra modifier, FlagsChanged, missing Ctrl, or negative codes
        // are never own-switch chords even when tagged.
        assert!(!is_own_switch_chord(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            SWITCH_CTRL_BIT | 0x0002_0000,
            digit
        ));
        assert!(!is_own_switch_chord(
            OWN_SWITCH_USER_DATA,
            Other,
            SWITCH_CTRL_BIT,
            digit
        ));
        assert!(!is_own_switch_chord(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            0,
            digit
        ));
        assert!(!is_own_switch_chord(
            OWN_SWITCH_USER_DATA,
            KeyUp,
            SWITCH_CTRL_BIT,
            -1
        ));
        // Letters stay outside the switch shape even when tagged.
        assert!(!is_own_switch_chord(
            OWN_SWITCH_USER_DATA,
            KeyDown,
            SWITCH_CTRL_BIT,
            0x04
        ));
    }
}

#[cfg(test)]
mod category_gate_tests {
    use super::*;

    #[test]
    fn matched_output_never_depends_on_pending_bits() {
        // The tap callback probes the match category with dummy pending
        // bits before the real match loop: sound only because `matched`
        // is pending-independent. Pin it across kinds, arms, and both
        // pending states (only `suppress` may differ).
        let tunings = [
            TapTuning {
                want_flags: 0x0004_0000,
                flags_mask: 0x001E_0000,
                want_code: 0x12,
            },
            TapTuning {
                want_flags: 0x0014_0000,
                flags_mask: 0x001E_0000,
                want_code: 0x30,
            },
        ];
        for tuning in tunings {
            for kind in [TapKeyKind::KeyDown, TapKeyKind::KeyUp, TapKeyKind::Other] {
                for flags in [0x0004_0000u64, 0x0014_0000, 0x0000_0000] {
                    for code in [0x12i64, 0x30, 0x04] {
                        for suppress_armed in [false, true] {
                            let mut a = false;
                            let mut b = true;
                            let ra =
                                decide_tap_event(kind, flags, code, tuning, suppress_armed, &mut a);
                            let rb =
                                decide_tap_event(kind, flags, code, tuning, suppress_armed, &mut b);
                            assert_eq!(
                                ra.matched, rb.matched,
                                "pending must not move matched: {kind:?} {flags:#x} {code:#x} armed={suppress_armed}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn category_gate_matrix() {
        // Basic hits pass only on the basic bit (an empty Space passes
        // them through to the app — never forwarded, never stale hidden
        // focus); select hits pass only on the select bit; a missing
        // select cache falls back to the basic bit for select tables.
        assert!(tap_category_allowed(false, true, Some(true)));
        assert!(tap_category_allowed(false, true, Some(false)));
        assert!(!tap_category_allowed(false, false, Some(true)));
        assert!(!tap_category_allowed(false, false, Some(false)));
        assert!(!tap_category_allowed(false, false, None));
        assert!(tap_category_allowed(false, true, None));
        assert!(tap_category_allowed(true, false, Some(true)));
        assert!(tap_category_allowed(true, true, Some(true)));
        assert!(!tap_category_allowed(true, true, Some(false)));
        assert!(!tap_category_allowed(true, false, Some(false)));
        assert!(tap_category_allowed(true, true, None));
        assert!(!tap_category_allowed(true, false, None));
    }
}
