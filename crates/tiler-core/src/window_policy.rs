#![forbid(unsafe_code)]
//! Shared pure window policy over adapter-normalized boolean facts.
//!
//! Adapters map host facts into [`SafetyFacts`] (plus separate fullscreen /
//! maximized flags); the core never reads native state and reports neutral
//! [`EligibilityRefusal`] tokens. [`classify_geometry`] pins the full refusal
//! order: safety gates, then overlays (fullscreen before maximized), then
//! `no_activate`. [`classify_focus`] is the same gate with both overlays
//! clear, since focus carries no geometry write. The overlay snapshot
//! vocabulary ([`overlay_refusal`] reports `"maximize"`) predates this gate
//! and is unchanged.
//!
//! Membership vs reserved slot vs fresh writes:
//! - Overlays keep membership (hide/reveal, focus). A retained
//!   fullscreen/maximized member keeps its RESERVED slot/allocation while
//!   fresh geometry writes refuse ([`geometry_eligible`],
//!   [`geometry_write_allowed`]); born-fullscreen rides slotless until its
//!   first exit, and slotless maximized joins without a tile slot yet.
//! - Slot admission is separate: the one-shot maximize clear
//!   ([`should_clear_maximize_at_admission_with_mode`]) and the slot rules
//!   ([`should_admit_slotless_maximized`], [`should_seed_member_slot`],
//!   [`born_keep_latch`], [`should_hold_born_fullscreen`]) decide reservation
//!   without disturbing retained allocations.
//! - Directional focus commands stay fenced while maximized
//!   ([`maximized_focus_fence`], fullscreen exempt); the unfloat lane admits
//!   only ordinary maximized floats ([`unfloat_maximize_clear_eligible`]).
//!
//! Fixed-size and born-maximized reuse (no new policy): fixed admission is
//! [`is_fixed_size_with`]; born mode is [`BornMaximized`].
//!
//! These stateless predicates do not retain the world or lifetime history.
//! Native fact collection, setters and identity proofs remain outside core;
//! world reconciliation supplies the retained policy context.

use crate::born_maximized::BornMaximized;
use crate::size_hints::{FixedSizePredicate, WindowSizeHints, is_fixed_size_with};

/// Normalized safety facts for one observed window.
///
/// Adapters map host facts into these booleans; the core never reads native
/// state. Windows `WindowFacts` mapping: `visible`, `minimized`, `cloaked`,
/// `privileged` (= elevated), `shell`, `tool` (= tool_window),
/// `owned_dialog` (= owned), `dialog`, `no_activate`. Fullscreen/maximized
/// ride separately so the clear gate can order them after safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SafetyFacts {
    pub visible: bool,
    pub minimized: bool,
    pub cloaked: bool,
    pub privileged: bool,
    pub shell: bool,
    pub tool: bool,
    pub owned_dialog: bool,
    pub dialog: bool,
    pub no_activate: bool,
}

/// Safety eligibility: every host safety gate passes. Overlays are NOT
/// checked here; callers order them after this (fullscreen first). Boolean
/// output only: the ordered refusal form is [`classify_geometry`] (geometry)
/// and [`classify_focus`] (focus).
#[must_use]
pub const fn safety_eligible(facts: &SafetyFacts) -> bool {
    facts.visible
        && !facts.minimized
        && !facts.cloaked
        && !facts.privileged
        && !facts.shell
        && !facts.tool
        && !facts.owned_dialog
        && !facts.dialog
        && !facts.no_activate
}

/// Admission-clear safety: same safety gates as [`safety_eligible`].
/// Fullscreen/maximized are handled by the clear gate itself, never here.
#[must_use]
pub const fn admission_clear_eligible(facts: &SafetyFacts) -> bool {
    safety_eligible(facts)
}

/// Ordered eligibility refusal over normalized facts, neutral vocabulary.
///
/// Adapters map these to native diagnostics; Windows reports `privileged` as
/// `elevated` and keeps every other token verbatim. The overlay snapshot
/// vocabulary differs on purpose: [`overlay_refusal`] reports `"maximize"`
/// while this gate reports [`EligibilityRefusal::Maximized`] as `"maximized"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityRefusal {
    Hidden,
    Minimized,
    Cloaked,
    Privileged,
    Shell,
    Tool,
    OwnedDialog,
    Dialog,
    Fullscreen,
    Maximized,
    NoActivate,
}

impl EligibilityRefusal {
    /// Neutral refusal token for fixtures and logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Minimized => "minimized",
            Self::Cloaked => "cloaked",
            Self::Privileged => "privileged",
            Self::Shell => "shell",
            Self::Tool => "tool",
            Self::OwnedDialog => "owned-dialog",
            Self::Dialog => "dialog",
            Self::Fullscreen => "fullscreen",
            Self::Maximized => "maximized",
            Self::NoActivate => "no-activate",
        }
    }
}

/// Ordered geometry classification over normalized facts: the first held
/// refusal wins, safety gates first, then overlays (fullscreen before
/// maximized), then `no_activate`. A no-activate maximized window therefore
/// reports [`EligibilityRefusal::Maximized`], never `NoActivate`.
pub const fn classify_geometry(
    facts: &SafetyFacts,
    fullscreen: bool,
    maximized: bool,
) -> Result<(), EligibilityRefusal> {
    if !facts.visible {
        return Err(EligibilityRefusal::Hidden);
    }
    if facts.minimized {
        return Err(EligibilityRefusal::Minimized);
    }
    if facts.cloaked {
        return Err(EligibilityRefusal::Cloaked);
    }
    if facts.privileged {
        return Err(EligibilityRefusal::Privileged);
    }
    if facts.shell {
        return Err(EligibilityRefusal::Shell);
    }
    if facts.tool {
        return Err(EligibilityRefusal::Tool);
    }
    if facts.owned_dialog {
        return Err(EligibilityRefusal::OwnedDialog);
    }
    if facts.dialog {
        return Err(EligibilityRefusal::Dialog);
    }
    if fullscreen {
        return Err(EligibilityRefusal::Fullscreen);
    }
    if maximized {
        return Err(EligibilityRefusal::Maximized);
    }
    if facts.no_activate {
        return Err(EligibilityRefusal::NoActivate);
    }
    Ok(())
}

/// Ordered focus classification over normalized facts: focus carries no
/// geometry write, so both overlays are ignored while every other gate keeps
/// its geometry priority, including `no_activate` last. Single ordered
/// implementation: this is [`classify_geometry`] with both overlays clear.
pub const fn classify_focus(facts: &SafetyFacts) -> Result<(), EligibilityRefusal> {
    classify_geometry(facts, false, false)
}

/// Neutral three-way membership verdict for one observed window.
///
/// Adapters invoke this in process and project `FloatMember` into the
/// existing `WindowFact.floating` flag; no wire contract change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipVerdict {
    Tile,
    FloatMember,
    LeaveUnmanaged,
}

/// Three-way membership classification over normalized facts plus ONE
/// neutral host fact: `transient` (dialog/sheet/modal, not a standalone
/// document). Any held safety gate (including `owned_dialog`/`dialog`)
/// leaves unmanaged even when transient; then transient floats as a
/// member; otherwise tile. Fullscreen/maximized ride separately and never
/// affect this verdict.
#[must_use]
pub const fn classify_membership(facts: &SafetyFacts, transient: bool) -> MembershipVerdict {
    if !safety_eligible(facts) {
        MembershipVerdict::LeaveUnmanaged
    } else if transient {
        MembershipVerdict::FloatMember
    } else {
        MembershipVerdict::Tile
    }
}

/// Retained membership outcome for hosts retaining hidden/minimized windows: the
/// membership verdict ignores `!visible`/`minimized` only (a hidden or
/// minimized window stays a retained member), while every other safety
/// exclusion still leaves unmanaged even when transient. `eligible`
/// is the full [`safety_eligible`] gate: hidden/minimized members are
/// retained but ineligible for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetainedMembership {
    pub membership: MembershipVerdict,
    pub eligible: bool,
}

/// Retained membership classification over normalized facts plus the
/// shared neutral `transient` fact. [`classify_membership`] is unchanged:
/// membership ignores `!visible`/`minimized` only via relaxed facts, and
/// eligibility is the full safety gate.
#[must_use]
pub const fn classify_retained_membership(
    facts: &SafetyFacts,
    transient: bool,
) -> RetainedMembership {
    let membership = classify_membership(
        &SafetyFacts {
            visible: true,
            minimized: false,
            ..*facts
        },
        transient,
    );
    RetainedMembership {
        membership,
        eligible: safety_eligible(facts),
    }
}

/// Geometry eligibility for fresh writes: safe AND
/// not overlaid. This matches Windows `classify`'s boolean geometry
/// eligibility, NOT generic tile membership: a retained overlaid member keeps
/// its reserved slot/allocation while fresh writes refuse. This does not
/// prevent the keep-maximized admission lane from reserving a new slot.
#[must_use]
pub const fn geometry_eligible(facts: &SafetyFacts, fullscreen: bool, maximized: bool) -> bool {
    safety_eligible(facts) && !fullscreen && !maximized
}

/// Focus-actuation eligibility: focus carries no geometry write, so overlays
/// stay focusable while every other gate still refuses. Boolean-equivalent to
/// Windows `classify_focus`; geometry classification is untouched.
#[must_use]
pub const fn focus_actuation_eligible(facts: &SafetyFacts) -> bool {
    safety_eligible(facts)
}

/// Whether a member may receive a fresh tile geometry write right now: false
/// under either overlay. The reserved allocation of a retained overlaid
/// member is unaffected. Adapters still own the actual setter and its
/// verification.
#[must_use]
pub const fn geometry_write_allowed(fullscreen: bool, maximized: bool) -> bool {
    !fullscreen && !maximized
}

/// Overlay precedence for command refusal: fullscreen wins when both hold.
/// Same token vocabulary as Windows `overlay_refusal` so the conformance
/// fixture compares literally.
#[must_use]
pub const fn overlay_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("fullscreen")
    } else if maximized {
        Some("maximize")
    } else {
        None
    }
}

/// Legacy native-float route refusal (`float-refused-fullscreen` /
/// `float-refused-maximize` vocabulary). Fullscreen wins until callers can
/// change membership without native fullscreen-subject writes. Session's
/// model-only float toggle independently permits retained fullscreen.
/// The B9 lane ([`unfloat_maximize_clear_eligible`]) is defined separately;
/// this snapshot vocabulary still reports that lane.
#[must_use]
pub const fn float_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("float-refused-fullscreen")
    } else if maximized {
        Some("float-refused-maximize")
    } else {
        None
    }
}

/// Sticky-toggle refusal (`sticky-refused-fullscreen` /
/// `sticky-refused-maximize` vocabulary). Fullscreen wins.
#[must_use]
pub const fn sticky_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("sticky-refused-fullscreen")
    } else if maximized {
        Some("sticky-refused-maximize")
    } else {
        None
    }
}

/// Orientation-toggle refusal for an overlay target (R-LAY-01; KDE gates the
/// same way under `toggle-orient-refused-*` tokens). Fullscreen wins.
#[must_use]
pub const fn orientation_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("orientation-refused-fullscreen")
    } else if maximized {
        Some("orientation-refused-maximize")
    } else {
        None
    }
}

/// B9 unfloat lane (R-FLT-06, COSMIC unmaximize-then-fresh-admit): only an
/// ordinary currently-floating target being explicitly tiled takes the
/// one-shot clear plus observed-clear route before fresh admission.
/// Fullscreen floats, sticky origins, tiled targets, and non-maximized floats
/// keep their existing paths.
#[must_use]
pub const fn unfloat_maximize_clear_eligible(
    fullscreen: bool,
    maximized: bool,
    is_float: bool,
    is_sticky_origin: bool,
) -> bool {
    is_float && !is_sticky_origin && maximized && !fullscreen
}

/// Directional refusal for a sticky subject (sticky isolation): a sticky
/// focused window never starts directional focus/move. `None` when the route
/// may proceed.
#[must_use]
pub const fn sticky_directional_refusal(is_sticky: bool, is_move: bool) -> Option<&'static str> {
    if !is_sticky {
        return None;
    }
    if is_move {
        Some("move-refused-sticky")
    } else {
        Some("focus-refused-sticky")
    }
}

/// G-06 maximized directional focus fence (REQ-MAX-08): directional focus is
/// fenced while the focused window is maximized (no-op; leave via unmaximize
/// or Alt+Tab). Fullscreen stays exempt: it carries no geometry write.
#[must_use]
pub const fn maximized_focus_fence(maximized: bool, fullscreen: bool) -> bool {
    maximized && !fullscreen
}

/// Born-fullscreen hold gate: a first-seen fullscreen window without a
/// retained tile slot is tracked as a slotless exception until its first
/// exit, never admitted. A window with a retained slot is a managed overlay
/// transition, and a window already seen non-fullscreen in this lifetime
/// never becomes born again. Adapters own the slot/seen maps; this pins the
/// gate.
#[must_use]
pub const fn should_hold_born_fullscreen(
    fullscreen: bool,
    known_slot: bool,
    seen_nonfullscreen: bool,
) -> bool {
    fullscreen && !known_slot && !seen_nonfullscreen
}

/// One-shot maximize-clear gate for first admission: the first non-fullscreen
/// admission of a maximized window without a retained tiled slot clears once
/// with no automatic retry. Fullscreen never clears, already-slotted members
/// never re-clear, attempted identities never retry, and floating domains
/// defer (`tiled == false`).
#[must_use]
pub const fn should_clear_maximize_at_admission(
    fullscreen: bool,
    maximized: bool,
    known_slot: bool,
    attempted: bool,
    tiled: bool,
) -> bool {
    should_clear_maximize_at_admission_with_mode(
        fullscreen, maximized, known_slot, attempted, tiled, false,
    )
}

/// Mode-aware clear gate: `keep_overlay` (latched at first-seen tiled
/// admission) keeps the native maximize over the reserved slot instead of
/// clearing. Every other gate is unchanged.
#[must_use]
pub const fn should_clear_maximize_at_admission_with_mode(
    fullscreen: bool,
    maximized: bool,
    known_slot: bool,
    attempted: bool,
    tiled: bool,
    keep_overlay: bool,
) -> bool {
    !keep_overlay && tiled && !fullscreen && maximized && !known_slot && !attempted
}

/// Mode-typed clear gate reusing the shared [`BornMaximized`] setting: the
/// decision binds through [`BornMaximized::should_clear_at_admission`], so
/// adapters pass their normalized mode instead of re-matching tokens.
#[must_use]
pub const fn should_clear_maximize_at_admission_with_born_mode(
    fullscreen: bool,
    maximized: bool,
    known_slot: bool,
    attempted: bool,
    tiled: bool,
    mode: BornMaximized,
) -> bool {
    should_clear_maximize_at_admission_with_mode(
        fullscreen,
        maximized,
        known_slot,
        attempted,
        tiled,
        !mode.should_clear_at_admission(),
    )
}

/// R-MAX-06 latch decision at slotless admission: bind keep-overlay to the
/// exact fresh lifetime only for tiled-first admissions under the live
/// overlay value. Floating-first (R-MAX-03), pre-existing, and
/// born-fullscreen lifetimes never latch; a later live flip never moves an
/// already-bound lifetime.
#[must_use]
pub const fn born_keep_latch(tiled_first: bool, keep_setting: bool) -> bool {
    tiled_first && keep_setting
}

/// Slotless maximized admission rule, R-MAX-03: a first-seen maximized,
/// non-fullscreen retained window without membership and without a retained
/// slot joins slotless (membership for hide/reveal and focus, no tile slot
/// yet), so a floating workspace preserves its native frame. Its later tiled
/// admission keeps the maximize over the reserved slot (retile overlay lane,
/// under both born modes); no second clear fires.
#[must_use]
pub const fn should_admit_slotless_maximized(
    maximized: bool,
    fullscreen: bool,
    is_member: bool,
    has_slot: bool,
) -> bool {
    maximized && !fullscreen && !is_member && !has_slot
}

/// Retained-slot seed rule: a slotless maximized row on a floating domain
/// keeps no tile slot, so a later tiled admission still sees it slotless and
/// takes the retile overlay lane. Every other combination seeds/refreshes the
/// last-known rectangle as before.
#[must_use]
pub const fn should_seed_member_slot(domain_tiled: bool, maximized: bool, has_slot: bool) -> bool {
    domain_tiled || !maximized || has_slot
}

/// D5 born-fullscreen first-exit disposition: the exit classifies as newly
/// admitted through the shared fixed-size predicate — fixed floats with no
/// writes, otherwise tiles. Previously fixed-floating restores float via the
/// adapter's own mark; this pins only the fresh-hint half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstExitDisposition {
    FloatAutomatic,
    Tile,
}

/// Classify a born-fullscreen first exit from its current hints under the
/// live predicate.
#[must_use]
pub fn classify_born_fullscreen_exit(
    hints: WindowSizeHints,
    predicate: FixedSizePredicate,
) -> FirstExitDisposition {
    if is_fixed_size_with(hints, predicate) {
        FirstExitDisposition::FloatAutomatic
    } else {
        FirstExitDisposition::Tile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safety_all_clear() -> SafetyFacts {
        SafetyFacts {
            visible: true,
            ..SafetyFacts::default()
        }
    }

    #[test]
    fn overlay_precedence_is_fullscreen_first() {
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                let expected = if fullscreen {
                    Some("fullscreen")
                } else if maximized {
                    Some("maximize")
                } else {
                    None
                };
                assert_eq!(overlay_refusal(fullscreen, maximized), expected);
                assert_eq!(
                    geometry_write_allowed(fullscreen, maximized),
                    expected.is_none()
                );
            }
        }
        // Both held reports fullscreen everywhere (boolean precedence only;
        // Windows orders no_activate after overlays, which is out of scope).
        assert_eq!(overlay_refusal(true, true), Some("fullscreen"));
    }

    #[test]
    fn toggle_refusals_share_fullscreen_precedence() {
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                assert_eq!(
                    float_toggle_refusal(fullscreen, maximized),
                    if fullscreen {
                        Some("float-refused-fullscreen")
                    } else if maximized {
                        Some("float-refused-maximize")
                    } else {
                        None
                    }
                );
                assert_eq!(
                    sticky_toggle_refusal(fullscreen, maximized),
                    if fullscreen {
                        Some("sticky-refused-fullscreen")
                    } else if maximized {
                        Some("sticky-refused-maximize")
                    } else {
                        None
                    }
                );
                assert_eq!(
                    orientation_toggle_refusal(fullscreen, maximized),
                    if fullscreen {
                        Some("orientation-refused-fullscreen")
                    } else if maximized {
                        Some("orientation-refused-maximize")
                    } else {
                        None
                    }
                );
            }
        }
    }

    #[test]
    fn b9_lane_is_ordinary_maximized_floats_only() {
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                for is_float in [false, true] {
                    for is_sticky_origin in [false, true] {
                        assert_eq!(
                            unfloat_maximize_clear_eligible(
                                fullscreen,
                                maximized,
                                is_float,
                                is_sticky_origin
                            ),
                            is_float && !is_sticky_origin && maximized && !fullscreen,
                            "fs={fullscreen} max={maximized} float={is_float} sticky={is_sticky_origin}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sticky_directional_and_focus_fence_truth_tables() {
        for is_sticky in [false, true] {
            for is_move in [false, true] {
                assert_eq!(
                    sticky_directional_refusal(is_sticky, is_move),
                    if !is_sticky {
                        None
                    } else if is_move {
                        Some("move-refused-sticky")
                    } else {
                        Some("focus-refused-sticky")
                    }
                );
            }
        }
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                assert_eq!(
                    maximized_focus_fence(maximized, fullscreen),
                    maximized && !fullscreen
                );
            }
        }
    }

    #[test]
    fn born_hold_needs_first_seen_slotless_fullscreen() {
        for fullscreen in [false, true] {
            for known_slot in [false, true] {
                for seen_nonfullscreen in [false, true] {
                    assert_eq!(
                        should_hold_born_fullscreen(fullscreen, known_slot, seen_nonfullscreen),
                        fullscreen && !known_slot && !seen_nonfullscreen,
                        "fs={fullscreen} slot={known_slot} seen={seen_nonfullscreen}"
                    );
                }
            }
        }
    }

    #[test]
    fn admission_clear_base_gate_is_exhaustive() {
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                for known_slot in [false, true] {
                    for attempted in [false, true] {
                        for tiled in [false, true] {
                            let expected =
                                tiled && !fullscreen && maximized && !known_slot && !attempted;
                            assert_eq!(
                                should_clear_maximize_at_admission(
                                    fullscreen, maximized, known_slot, attempted, tiled
                                ),
                                expected
                            );
                            // Keep-overlay latches to no-clear on every combo.
                            assert!(!should_clear_maximize_at_admission_with_mode(
                                fullscreen, maximized, known_slot, attempted, tiled, true
                            ));
                            assert_eq!(
                                should_clear_maximize_at_admission_with_mode(
                                    fullscreen, maximized, known_slot, attempted, tiled, false
                                ),
                                expected
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn born_mode_gate_reuses_shared_setting() {
        // Tile-on-launch behaves as keep_overlay=false; keep-overlay as true.
        for fullscreen in [false, true] {
            for maximized in [false, true] {
                for known_slot in [false, true] {
                    for attempted in [false, true] {
                        for tiled in [false, true] {
                            assert_eq!(
                                should_clear_maximize_at_admission_with_born_mode(
                                    fullscreen,
                                    maximized,
                                    known_slot,
                                    attempted,
                                    tiled,
                                    BornMaximized::TileOnLaunch
                                ),
                                should_clear_maximize_at_admission(
                                    fullscreen, maximized, known_slot, attempted, tiled
                                )
                            );
                            assert!(!should_clear_maximize_at_admission_with_born_mode(
                                fullscreen,
                                maximized,
                                known_slot,
                                attempted,
                                tiled,
                                BornMaximized::KeepMaximizedOverlay
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn slot_and_latch_gates_are_exhaustive() {
        for maximized in [false, true] {
            for fullscreen in [false, true] {
                for is_member in [false, true] {
                    for has_slot in [false, true] {
                        assert_eq!(
                            should_admit_slotless_maximized(
                                maximized, fullscreen, is_member, has_slot
                            ),
                            maximized && !fullscreen && !is_member && !has_slot
                        );
                    }
                }
            }
        }
        for domain_tiled in [false, true] {
            for maximized in [false, true] {
                for has_slot in [false, true] {
                    assert_eq!(
                        should_seed_member_slot(domain_tiled, maximized, has_slot),
                        domain_tiled || !maximized || has_slot
                    );
                }
            }
        }
        for tiled_first in [false, true] {
            for keep_setting in [false, true] {
                assert_eq!(
                    born_keep_latch(tiled_first, keep_setting),
                    tiled_first && keep_setting
                );
            }
        }
    }

    #[test]
    fn safety_membership_geometry_focus_split() {
        // All 512 safety combinations: any single refusal drops eligibility,
        // and overlays never affect safety itself.
        for bits in 0..512u32 {
            let facts = SafetyFacts {
                visible: bits & 1 != 0,
                minimized: bits & 2 != 0,
                cloaked: bits & 4 != 0,
                privileged: bits & 8 != 0,
                shell: bits & 16 != 0,
                tool: bits & 32 != 0,
                owned_dialog: bits & 64 != 0,
                dialog: bits & 128 != 0,
                no_activate: bits & 256 != 0,
            };
            let expected = facts.visible
                && !facts.minimized
                && !facts.cloaked
                && !facts.privileged
                && !facts.shell
                && !facts.tool
                && !facts.owned_dialog
                && !facts.dialog
                && !facts.no_activate;
            assert_eq!(safety_eligible(&facts), expected, "bits={bits:09b}");
            assert_eq!(admission_clear_eligible(&facts), expected);
            assert_eq!(focus_actuation_eligible(&facts), expected);
            // Overlays keep membership/focus eligibility but refuse fresh
            // geometry writes; reserved slots stay adapter-owned.
            assert_eq!(geometry_eligible(&facts, false, false), expected);
            assert!(!geometry_eligible(&facts, true, false));
            assert!(!geometry_eligible(&facts, false, true));
        }
        // The all-clear window is the only eligible shape.
        assert!(safety_eligible(&safety_all_clear()));
        assert!(geometry_eligible(&safety_all_clear(), false, false));
        assert!(focus_actuation_eligible(&safety_all_clear()));
    }

    #[test]
    fn ordered_classification_priority_is_exhaustive() {
        // Every safety combination crossed with both overlays: geometry
        // reports the first held refusal in priority order, focus reports the
        // same order minus the overlays.
        for bits in 0..512u32 {
            let facts = SafetyFacts {
                visible: bits & 1 != 0,
                minimized: bits & 2 != 0,
                cloaked: bits & 4 != 0,
                privileged: bits & 8 != 0,
                shell: bits & 16 != 0,
                tool: bits & 32 != 0,
                owned_dialog: bits & 64 != 0,
                dialog: bits & 128 != 0,
                no_activate: bits & 256 != 0,
            };
            let safety = if !facts.visible {
                Some(EligibilityRefusal::Hidden)
            } else if facts.minimized {
                Some(EligibilityRefusal::Minimized)
            } else if facts.cloaked {
                Some(EligibilityRefusal::Cloaked)
            } else if facts.privileged {
                Some(EligibilityRefusal::Privileged)
            } else if facts.shell {
                Some(EligibilityRefusal::Shell)
            } else if facts.tool {
                Some(EligibilityRefusal::Tool)
            } else if facts.owned_dialog {
                Some(EligibilityRefusal::OwnedDialog)
            } else if facts.dialog {
                Some(EligibilityRefusal::Dialog)
            } else {
                None
            };
            let focus_expected = safety.or(if facts.no_activate {
                Some(EligibilityRefusal::NoActivate)
            } else {
                None
            });
            assert_eq!(
                classify_focus(&facts).err(),
                focus_expected,
                "bits={bits:09b}"
            );
            for fullscreen in [false, true] {
                for maximized in [false, true] {
                    let geometry_expected = safety
                        .or(if fullscreen {
                            Some(EligibilityRefusal::Fullscreen)
                        } else if maximized {
                            Some(EligibilityRefusal::Maximized)
                        } else {
                            None
                        })
                        .or(if facts.no_activate {
                            Some(EligibilityRefusal::NoActivate)
                        } else {
                            None
                        });
                    assert_eq!(
                        classify_geometry(&facts, fullscreen, maximized).err(),
                        geometry_expected,
                        "bits={bits:09b} fs={fullscreen} max={maximized}"
                    );
                    // Boolean gates agree with the ordered gate on every combo.
                    assert_eq!(
                        geometry_eligible(&facts, fullscreen, maximized),
                        geometry_expected.is_none()
                    );
                }
            }
            assert_eq!(focus_actuation_eligible(&facts), focus_expected.is_none());
            assert_eq!(
                safety_eligible(&facts),
                safety.is_none() && !facts.no_activate
            );
        }
        // Priority conflicts pin the order, not just the single causes.
        let conflict = |facts: SafetyFacts, fullscreen: bool, maximized: bool| {
            classify_geometry(&facts, fullscreen, maximized).err()
        };
        let all_set = SafetyFacts {
            visible: false,
            minimized: true,
            cloaked: true,
            privileged: true,
            shell: true,
            tool: true,
            owned_dialog: true,
            dialog: true,
            no_activate: true,
        };
        assert_eq!(
            conflict(all_set, true, true),
            Some(EligibilityRefusal::Hidden)
        );
        assert_eq!(
            conflict(safety_all_clear(), true, true),
            Some(EligibilityRefusal::Fullscreen)
        );
        // no_activate orders after the overlays: an overlaid no-activate
        // window reports the overlay, never NoActivate.
        assert_eq!(
            conflict(safety_all_clear(), false, true),
            Some(EligibilityRefusal::Maximized)
        );
        let no_activate_only = SafetyFacts {
            no_activate: true,
            ..safety_all_clear()
        };
        assert_eq!(
            conflict(no_activate_only, false, true),
            Some(EligibilityRefusal::Maximized)
        );
        assert_eq!(
            conflict(no_activate_only, true, false),
            Some(EligibilityRefusal::Fullscreen)
        );
        assert_eq!(
            conflict(no_activate_only, false, false),
            Some(EligibilityRefusal::NoActivate)
        );
        // Focus ignores both overlays but keeps no_activate.
        assert!(classify_focus(&safety_all_clear()).is_ok());
        assert_eq!(
            classify_focus(&no_activate_only).err(),
            Some(EligibilityRefusal::NoActivate)
        );
    }

    #[test]
    fn membership_verdict_transient_ordinary_safety_precedence() {
        // Exhaustive: every safety combination crossed with the transient
        // fact. Any held safety gate leaves unmanaged (even when transient);
        // all-clear transient floats as a member; all-clear ordinary tiles.
        for bits in 0..512u32 {
            let facts = SafetyFacts {
                visible: bits & 1 != 0,
                minimized: bits & 2 != 0,
                cloaked: bits & 4 != 0,
                privileged: bits & 8 != 0,
                shell: bits & 16 != 0,
                tool: bits & 32 != 0,
                owned_dialog: bits & 64 != 0,
                dialog: bits & 128 != 0,
                no_activate: bits & 256 != 0,
            };
            let safe = safety_eligible(&facts);
            for transient in [false, true] {
                let expected = if !safe {
                    MembershipVerdict::LeaveUnmanaged
                } else if transient {
                    MembershipVerdict::FloatMember
                } else {
                    MembershipVerdict::Tile
                };
                assert_eq!(
                    classify_membership(&facts, transient),
                    expected,
                    "bits={bits:09b} transient={transient}"
                );
            }
        }
        // Ordinary all-clear tiles; transient all-clear floats as a member.
        assert_eq!(
            classify_membership(&safety_all_clear(), false),
            MembershipVerdict::Tile
        );
        assert_eq!(
            classify_membership(&safety_all_clear(), true),
            MembershipVerdict::FloatMember
        );
        // Windows divergence pin: an owned dialog stays unmanaged even when
        // marked transient, and an unowned dialog likewise never floats.
        let mut owned = safety_all_clear();
        owned.owned_dialog = true;
        assert_eq!(
            classify_membership(&owned, true),
            MembershipVerdict::LeaveUnmanaged
        );
        let mut dialog = safety_all_clear();
        dialog.dialog = true;
        assert_eq!(
            classify_membership(&dialog, true),
            MembershipVerdict::LeaveUnmanaged
        );
        assert_eq!(
            classify_membership(&dialog, false),
            MembershipVerdict::LeaveUnmanaged
        );
        // Safety precedence conflicts: transient loses to every gate.
        let invisible = SafetyFacts::default();
        assert_eq!(
            classify_membership(&invisible, true),
            MembershipVerdict::LeaveUnmanaged
        );
        let no_activate_only = SafetyFacts {
            no_activate: true,
            ..safety_all_clear()
        };
        assert_eq!(
            classify_membership(&no_activate_only, true),
            MembershipVerdict::LeaveUnmanaged
        );
    }

    #[test]
    fn retained_membership_ignores_hidden_minimized_only() {
        // Exhaustive: every safety combination crossed with the transient
        // fact. Only !visible/minimized are ignored for membership; every
        // other held gate still leaves unmanaged even when transient.
        // Eligibility is the full safety gate.
        for bits in 0..512u32 {
            let facts = SafetyFacts {
                visible: bits & 1 != 0,
                minimized: bits & 2 != 0,
                cloaked: bits & 4 != 0,
                privileged: bits & 8 != 0,
                shell: bits & 16 != 0,
                tool: bits & 32 != 0,
                owned_dialog: bits & 64 != 0,
                dialog: bits & 128 != 0,
                no_activate: bits & 256 != 0,
            };
            let relaxed = !facts.cloaked
                && !facts.privileged
                && !facts.shell
                && !facts.tool
                && !facts.owned_dialog
                && !facts.dialog
                && !facts.no_activate;
            for transient in [false, true] {
                let expected_membership = if !relaxed {
                    MembershipVerdict::LeaveUnmanaged
                } else if transient {
                    MembershipVerdict::FloatMember
                } else {
                    MembershipVerdict::Tile
                };
                let expected_eligible = expected_membership != MembershipVerdict::LeaveUnmanaged
                    && safety_eligible(&facts);
                let got = classify_retained_membership(&facts, transient);
                assert_eq!(
                    got.membership, expected_membership,
                    "bits={bits:09b} transient={transient}"
                );
                assert_eq!(
                    got.eligible, expected_eligible,
                    "bits={bits:09b} transient={transient}"
                );
            }
        }
        // Hidden/minimized ordinary windows stay members but ineligible;
        // hidden/minimized transients stay floating members but ineligible.
        let hidden = SafetyFacts {
            visible: false,
            ..safety_all_clear()
        };
        assert_eq!(
            classify_retained_membership(&hidden, false),
            RetainedMembership {
                membership: MembershipVerdict::Tile,
                eligible: false,
            }
        );
        assert_eq!(
            classify_retained_membership(&hidden, true),
            RetainedMembership {
                membership: MembershipVerdict::FloatMember,
                eligible: false,
            }
        );
        let minimized = SafetyFacts {
            minimized: true,
            ..safety_all_clear()
        };
        assert_eq!(
            classify_retained_membership(&minimized, false),
            RetainedMembership {
                membership: MembershipVerdict::Tile,
                eligible: false,
            }
        );
        // Other gates still leave unmanaged even when transient.
        let mut tool = safety_all_clear();
        tool.tool = true;
        assert_eq!(
            classify_retained_membership(&tool, true),
            RetainedMembership {
                membership: MembershipVerdict::LeaveUnmanaged,
                eligible: false,
            }
        );
    }

    #[test]
    fn first_exit_reuses_fixed_size_predicate() {
        let fixed = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        assert_eq!(
            classify_born_fullscreen_exit(fixed, FixedSizePredicate::BothAxes),
            FirstExitDisposition::FloatAutomatic
        );
        assert_eq!(
            classify_born_fullscreen_exit(WindowSizeHints::none(), FixedSizePredicate::BothAxes),
            FirstExitDisposition::Tile
        );
        // G-05 partial-zero: nonzero axis pins under either-axis only.
        let partial = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(0),
            max_w: Some(640),
            max_h: Some(0),
        };
        assert_eq!(
            classify_born_fullscreen_exit(partial, FixedSizePredicate::BothAxes),
            FirstExitDisposition::Tile
        );
        assert_eq!(
            classify_born_fullscreen_exit(partial, FixedSizePredicate::EitherAxis),
            FirstExitDisposition::FloatAutomatic
        );
    }

    #[test]
    fn shared_fixture_table_conformance() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../test-fixtures/window-policy.json"))
                .expect("shared window-policy fixture parses");
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
            let gate = case
                .get("gate")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing gate>");
            let bit = |key: &str| {
                case.get(key)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| panic!("{name} carries `{key}`"))
            };
            let refusal = case.get("refusal").and_then(serde_json::Value::as_str);
            match gate {
                "overlay_refusal" => {
                    assert_eq!(
                        overlay_refusal(bit("fullscreen"), bit("maximized")),
                        refusal,
                        "{name}"
                    );
                }
                "float_toggle_refusal" => {
                    assert_eq!(
                        float_toggle_refusal(bit("fullscreen"), bit("maximized")),
                        refusal,
                        "{name}"
                    );
                }
                "sticky_toggle_refusal" => {
                    assert_eq!(
                        sticky_toggle_refusal(bit("fullscreen"), bit("maximized")),
                        refusal,
                        "{name}"
                    );
                }
                "orientation_toggle_refusal" => {
                    assert_eq!(
                        orientation_toggle_refusal(bit("fullscreen"), bit("maximized")),
                        refusal,
                        "{name}"
                    );
                }
                "unfloat_lane" => {
                    let expected = case
                        .get("eligible")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `eligible`"));
                    assert_eq!(
                        unfloat_maximize_clear_eligible(
                            bit("fullscreen"),
                            bit("maximized"),
                            bit("is_float"),
                            bit("is_sticky_origin")
                        ),
                        expected,
                        "{name}"
                    );
                }
                "focus_fence" => {
                    let expected = case
                        .get("fenced")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `fenced`"));
                    assert_eq!(
                        maximized_focus_fence(bit("maximized"), bit("fullscreen")),
                        expected,
                        "{name}"
                    );
                }
                "born_hold" => {
                    let expected = case
                        .get("hold")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `hold`"));
                    assert_eq!(
                        should_hold_born_fullscreen(
                            bit("fullscreen"),
                            bit("known_slot"),
                            bit("seen_nonfullscreen")
                        ),
                        expected,
                        "{name}"
                    );
                }
                "admission_clear" => {
                    let expected = case
                        .get("clear")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `clear`"));
                    assert_eq!(
                        should_clear_maximize_at_admission_with_mode(
                            bit("fullscreen"),
                            bit("maximized"),
                            bit("known_slot"),
                            bit("attempted"),
                            bit("tiled"),
                            bit("keep_overlay")
                        ),
                        expected,
                        "{name}"
                    );
                }
                "slotless" => {
                    let expected = case
                        .get("slotless")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `slotless`"));
                    assert_eq!(
                        should_admit_slotless_maximized(
                            bit("maximized"),
                            bit("fullscreen"),
                            bit("is_member"),
                            bit("has_slot")
                        ),
                        expected,
                        "{name}"
                    );
                }
                "born_latch" => {
                    let expected = case
                        .get("latch")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `latch`"));
                    assert_eq!(
                        born_keep_latch(bit("tiled_first"), bit("keep_setting")),
                        expected,
                        "{name}"
                    );
                }
                "safety" => {
                    let facts = SafetyFacts {
                        visible: bit("visible"),
                        minimized: bit("minimized"),
                        cloaked: bit("cloaked"),
                        privileged: bit("privileged"),
                        shell: bit("shell"),
                        tool: bit("tool"),
                        owned_dialog: bit("owned_dialog"),
                        dialog: bit("dialog"),
                        no_activate: bit("no_activate"),
                    };
                    let expected = case
                        .get("eligible")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| panic!("{name} carries `eligible`"));
                    assert_eq!(safety_eligible(&facts), expected, "{name}");
                    assert_eq!(admission_clear_eligible(&facts), expected, "{name}");
                    assert_eq!(focus_actuation_eligible(&facts), expected, "{name}");
                    if let Some(geometry) = case
                        .get("geometry_when_plain")
                        .and_then(serde_json::Value::as_bool)
                    {
                        assert_eq!(geometry_eligible(&facts, false, false), geometry, "{name}");
                    }
                    if let Some(focus) = case
                        .get("focus_when_plain")
                        .and_then(serde_json::Value::as_bool)
                    {
                        assert_eq!(focus_actuation_eligible(&facts), focus, "{name}");
                    }
                    if let Some(geometry) = case
                        .get("geometry_when_maximized")
                        .and_then(serde_json::Value::as_bool)
                    {
                        assert_eq!(geometry_eligible(&facts, false, true), geometry, "{name}");
                    }
                    if let Some(focus) = case
                        .get("focus_when_maximized")
                        .and_then(serde_json::Value::as_bool)
                    {
                        assert_eq!(focus_actuation_eligible(&facts), focus, "{name}");
                    }
                    if let Some(write) = case
                        .get("geometry_when_maximized_write")
                        .and_then(serde_json::Value::as_bool)
                    {
                        assert_eq!(geometry_write_allowed(false, true), write, "{name}");
                    }
                }
                "geometry_classification" => {
                    let facts = SafetyFacts {
                        visible: bit("visible"),
                        minimized: bit("minimized"),
                        cloaked: bit("cloaked"),
                        privileged: bit("privileged"),
                        shell: bit("shell"),
                        tool: bit("tool"),
                        owned_dialog: bit("owned_dialog"),
                        dialog: bit("dialog"),
                        no_activate: bit("no_activate"),
                    };
                    let expected = case.get("refusal").and_then(serde_json::Value::as_str);
                    assert_eq!(
                        classify_geometry(&facts, bit("fullscreen"), bit("maximized"))
                            .err()
                            .map(EligibilityRefusal::as_str),
                        expected,
                        "{name}"
                    );
                }
                "focus_classification" => {
                    let facts = SafetyFacts {
                        visible: bit("visible"),
                        minimized: bit("minimized"),
                        cloaked: bit("cloaked"),
                        privileged: bit("privileged"),
                        shell: bit("shell"),
                        tool: bit("tool"),
                        owned_dialog: bit("owned_dialog"),
                        dialog: bit("dialog"),
                        no_activate: bit("no_activate"),
                    };
                    // Focus takes no overlay input: cases may carry held
                    // overlays and still expect the non-overlay verdict.
                    let expected = case.get("refusal").and_then(serde_json::Value::as_str);
                    assert_eq!(
                        classify_focus(&facts).err().map(EligibilityRefusal::as_str),
                        expected,
                        "{name}"
                    );
                }
                "first_exit" => {
                    let bound = |key: &str| match case.get(key) {
                        None | Some(serde_json::Value::Null) => None,
                        Some(serde_json::Value::Number(n)) => {
                            n.as_i64().and_then(|v| i32::try_from(v).ok())
                        }
                        _ => panic!("{name} carries numeric-or-null `{key}`"),
                    };
                    let hints = WindowSizeHints {
                        min_w: bound("min_w"),
                        min_h: bound("min_h"),
                        max_w: bound("max_w"),
                        max_h: bound("max_h"),
                    };
                    let predicate = match case.get("predicate").and_then(serde_json::Value::as_str)
                    {
                        Some("either-axis-fixed") => FixedSizePredicate::EitherAxis,
                        _ => FixedSizePredicate::BothAxes,
                    };
                    let expected = case
                        .get("disposition")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_else(|| panic!("{name} carries `disposition`"));
                    let disposition = classify_born_fullscreen_exit(hints, predicate);
                    assert_eq!(
                        match disposition {
                            FirstExitDisposition::FloatAutomatic => "float-automatic",
                            FirstExitDisposition::Tile => "tile",
                        },
                        expected,
                        "{name}"
                    );
                }
                other => panic!("{name} names an unknown gate `{other}`"),
            }
        }
    }
}
