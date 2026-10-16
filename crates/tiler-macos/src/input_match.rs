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
}
