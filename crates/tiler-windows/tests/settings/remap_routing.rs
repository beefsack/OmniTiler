#![forbid(unsafe_code)]
use super::*;

#[test]
fn rebind_routes_single_arm_and_old_default_passes_through() {
    // focus-left rebound Win+H -> Win+U: unshifted Win+U focuses left, the
    // shifted chord on the same key passes through (no unintended move),
    // and the rebound-away Win+H passes through too.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let remap = tiler_windows::settings::build_remap(&settings);
    assert_eq!(
        remap,
        vec![ChordRemap {
            from_vk: VK_U,
            from_shift: false,
            from_ctrl: false,
            from_alt: false,
            action: tiler_windows::snapkey::ChordAction::Directional,
            to_vk: VK_H
        },]
    );
    let disabled = build_disabled(&settings);
    assert_eq!(
        disabled,
        vec![ChordDisable {
            vk: VK_H,
            shift: false,
            ctrl: false,
            alt: false
        }]
    );
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(remap);
    machine.set_disabled(disabled);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("rebound U classifies as the directional arm");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.direction, tiler_core::directional::Direction::Left);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    // Shifted Win+U is not the rebound: passes through untracked.
    let mut shifted = SnapClassify::new(takeover());
    shifted.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    shifted.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut shifted, VK_LWIN, false, true, false);
    SnapClassify::push(&mut shifted, VK_SHIFT, false, true, false);
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_U, false, true, false),
        None,
        "shifted rebound chord passes through: single-arm routing"
    );
    SnapClassify::push(&mut shifted, VK_SHIFT, true, true, false);
    // The rebound-away Win+H passes through on the same machine.
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_H, false, true, false),
        None,
        "rebound-away default passes through"
    );
    assert_eq!(
        SnapClassify::push(&mut shifted, VK_H, true, true, false),
        None,
        "rebound-away pair leaves no hold"
    );
}

#[test]
fn focus_right_safe_rebind_needs_no_win_l_opt_in() {
    // focus-right rebound onto Win+U: the physical chord is not the OS lock
    // chord, so it routes without `allow_win_l`, while physical Win+L still
    // passes through.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-right".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(tiler_windows::settings::build_remap(&settings));
    machine.set_disabled(build_disabled(&settings));
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("safe rebound routes to focus-right");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.direction, tiler_core::directional::Direction::Right);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed());
    // Physical Win+L stays gated without opt-in on the same machine.
    assert_eq!(
        SnapClassify::push(&mut machine, 0x4C, false, true, false),
        None,
        "physical Win+L still passes through without opt-in"
    );
}

#[test]
fn takeover_off_preserves_consumed_release_and_mask() {
    let mut machine = SnapClassify::new(takeover());
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    assert!(down.consumed());
    // Live disable: the owner publishes the policy (no hold clearing) and
    // drains the queue (here: the queue drain is owner-side; the classifier
    // half must still swallow the consumed pair and keep the Start mask, so
    // no orphan key-up reaches the OS and no naked Win tap opens Start).
    machine.set_enabled(false);
    let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("up");
    assert!(
        up.consumed() && !up.announce(),
        "paired key-up rides the stored consumed verdict"
    );
    // A fresh chord while off passes through with its pair, never consuming.
    let mut off = SnapClassify::new(KeyboardConfig::disabled());
    SnapClassify::push(&mut off, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut off, VK_H, false, true, false).expect("down");
    assert!(!down.consumed() && !down.announce());
    let up = SnapClassify::push(&mut off, VK_H, true, true, false).expect("up");
    assert!(!up.consumed());
}

#[test]
fn mid_hold_remap_removal_pins_pairing_and_swallows() {
    // Win+U rebound to H, removed mid-hold: the repeat stays swallowed
    // without dispatching, the pair still closes consumed, and a fresh
    // press afterwards passes through (no orphan slot, no leaked up).
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("down");
    assert!(down.consumed() && down.announce());
    assert!(machine.key_is_down(VK_U));
    machine.set_remap(Vec::new());
    // Pinned repeat: swallowed, never re-dispatched under the new tables.
    let repeat = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("repeat");
    assert!(repeat.consumed() && !repeat.announce());
    // Pinned pair closes consumed even though the mapping is gone.
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    assert!(!machine.key_is_down(VK_U));
    // Fresh press under the new tables passes through untracked.
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, false, true, false),
        None
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, true, true, false),
        None
    );
}

#[test]
fn mid_hold_disable_gates_repeat_dispatch_but_keeps_pair() {
    // Consumed Win+H, default disabled mid-hold: the repeat stays swallowed
    // without dispatching (no stale action), the pair still closes consumed.
    let mut machine = SnapClassify::new(takeover());
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    assert!(down.consumed() && down.announce());
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    let repeat = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("repeat");
    assert!(repeat.consumed() && !repeat.announce());
    let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    // Fresh press under the new tables passes through untracked.
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, false, true, false),
        None
    );
}

#[test]
fn keep_equivalent_rebind_routes_natively() {
    // focus-left rebound onto its own Win+H: valid, no tables, native focus.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "focus-left".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+H".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    assert!(tiler_windows::settings::build_remap(&settings).is_empty());
    assert!(build_disabled(&settings).is_empty());
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(tiler_windows::settings::build_remap(&settings));
    machine.set_disabled(build_disabled(&settings));
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("down");
    let Classified::Snap(intent) = down else {
        panic!("native focus-left still routes");
    };
    assert!(intent.consumed && intent.announce);
    assert_eq!(intent.op, tiler_windows::snapkey::SnapOp::Focus);
}

#[test]
fn remap_up_survives_mid_hold_shift_flip() {
    // Sticky rebound onto Win+Shift+Q: a Shift release before key-up must
    // still close the pair instead of orphaning a stuck hold. Single-polarity:
    // the custom chord matches the sticky (shifted) arm.
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_Q,
        from_shift: true,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Sticky,
        to_vk: VK_G,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_Q, false, true, false).expect("down");
    let Classified::Sticky(intent) = down else {
        panic!("rebound Q classifies sticky");
    };
    assert!(intent.consumed && intent.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
    let up = SnapClassify::push(&mut machine, VK_Q, true, true, false).expect("up");
    assert!(up.consumed() && !up.announce());
    assert_eq!(
        SnapClassify::push(&mut machine, VK_Q, true, true, false),
        None,
        "hold closed: no orphan"
    );
}

#[test]
fn rebound_collision_shifted_native_never_repeats_owner() {
    // focus-left rebound Win+U (canonical H) with native shifted
    // Win+Shift+H (move-left) kept: Win, U down (consumed focus), Shift,
    // H down is swallowed as a safe consumed refusal (inheriting the live
    // owner's shape, never dispatched), never U's repeat. Both paired ups
    // stay consumed with no stuck slot.
    use tiler_windows::snapkey::{SnapEdge, SnapOp};
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    let Classified::Snap(focus) = down else {
        panic!("rebound U classifies directional");
    };
    assert_eq!((focus.op, focus.edge), (SnapOp::Focus, SnapEdge::Down));
    assert!(focus.consumed && focus.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let hit = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("H down");
    let Classified::Snap(refused) = hit else {
        panic!("colliding H classifies directional");
    };
    // Never U's repeat: a fresh safe refusal down, consumed and never
    // dispatched. Old shared-slot code returned U's Focus repeat.
    assert_eq!(
        refused.edge,
        SnapEdge::Down,
        "colliding H is a fresh down, not U's repeat"
    );
    assert!(refused.consumed && !refused.announce);
    let h_up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("H up");
    assert!(h_up.consumed() && !h_up.announce());
    let u_up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(u_up.consumed() && !u_up.announce());
    assert!(!machine.key_is_down(VK_U));
    assert!(!machine.key_is_down(VK_H));
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, true, true, false),
        None,
        "no stuck slot: extra up passes"
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_U, true, true, false),
        None,
        "no stuck slot: extra up passes"
    );
}

#[test]
fn suppressed_rebound_away_up_never_steals_owner_pair() {
    // First U consumed down, suppressed rebound-away H down passes, H up
    // must PASS untracked without closing U's slot; U up still consumed.
    // Old unpinned-up fallback mapped H up to canonical H and stole U's hold.
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    assert!(down.consumed());
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, false, true, false),
        None,
        "rebound-away H down passes suppressed"
    );
    assert_eq!(
        SnapClassify::push(&mut machine, VK_H, true, true, false),
        None,
        "unrelated H up passes without stealing U's slot"
    );
    assert!(
        machine.key_is_down(VK_U),
        "U hold survives the foreign H tap"
    );
    let u_up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(u_up.consumed() && !u_up.announce());
    assert!(!machine.key_is_down(VK_U));
}

#[test]
fn same_physical_rebound_pins_across_shift_flip() {
    // Same physical U rebound to H: Shift flip mid-hold keeps the pinned
    // Focus op, repeats stay swallowed without dispatching, pair closes
    // consumed. Per-binding single-polarity holds: shifted U never routes.
    use tiler_windows::snapkey::{SnapEdge, SnapOp};
    let mut machine = SnapClassify::new(takeover());
    machine.set_remap(vec![ChordRemap {
        from_vk: VK_U,
        from_shift: false,
        from_ctrl: false,
        from_alt: false,
        action: tiler_windows::snapkey::ChordAction::Directional,
        to_vk: VK_H,
    }]);
    machine.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: false,
        alt: false,
    }]);
    SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
    let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
    let Classified::Snap(focus) = down else {
        panic!("rebound U classifies directional");
    };
    assert_eq!(focus.op, SnapOp::Focus);
    SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
    let repeat = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U repeat");
    let Classified::Snap(rep) = repeat else {
        panic!("pinned U repeat classifies directional");
    };
    assert_eq!((rep.op, rep.edge), (SnapOp::Focus, SnapEdge::Repeat));
    assert!(rep.consumed && !rep.announce);
    SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
    let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
    assert!(up.consumed() && !up.announce());
    assert!(!machine.key_is_down(VK_U));
}

#[test]
fn three_colliders_pair_in_any_release_order_across_remap_change() {
    // Arbitrary bare-API mapping: two rebound physicals plus the native key
    // share one canonical. Owner plus both colliders each close consumed in
    // any release order with no stuck slot and no OS leak, even when the
    // mapping vanishes mid-hold (colliders ride their cached verdict, the
    // owner rides its pin).
    const VK_I: u32 = 0x49;
    let tables = || {
        (
            vec![
                ChordRemap {
                    from_vk: VK_U,
                    from_shift: false,
                    from_ctrl: false,
                    from_alt: false,
                    action: tiler_windows::snapkey::ChordAction::Directional,
                    to_vk: VK_H,
                },
                ChordRemap {
                    from_vk: VK_I,
                    from_shift: false,
                    from_ctrl: false,
                    from_alt: false,
                    action: tiler_windows::snapkey::ChordAction::Directional,
                    to_vk: VK_H,
                },
            ],
            vec![ChordDisable {
                vk: VK_H,
                shift: false,
                ctrl: false,
                alt: false,
            }],
        )
    };
    for owner_first in [false, true] {
        let mut machine = SnapClassify::new(takeover());
        let (remap, disabled) = tables();
        machine.set_remap(remap);
        machine.set_disabled(disabled);
        SnapClassify::push(&mut machine, VK_LWIN, false, true, false);
        let down = SnapClassify::push(&mut machine, VK_U, false, true, false).expect("U down");
        assert!(down.consumed() && down.announce());
        // Native H routes only shifted here (unshifted H is suppressed);
        // rebound I routes only unshifted (single-polarity entries).
        SnapClassify::push(&mut machine, VK_SHIFT, false, true, false);
        let h = SnapClassify::push(&mut machine, VK_H, false, true, false).expect("H collider");
        assert!(h.consumed() && !h.announce());
        SnapClassify::push(&mut machine, VK_SHIFT, true, true, false);
        let i = SnapClassify::push(&mut machine, VK_I, false, true, false).expect("I collider");
        assert!(i.consumed() && !i.announce());
        assert!(machine.key_is_down(VK_U));
        assert!(machine.key_is_down(VK_H));
        assert!(machine.key_is_down(VK_I));
        // Mapping vanishes mid-hold: pairs must still close consumed.
        machine.set_remap(Vec::new());
        if owner_first {
            let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
            assert!(up.consumed() && !up.announce());
        }
        let up = SnapClassify::push(&mut machine, VK_I, true, true, false).expect("I up");
        assert!(up.consumed() && !up.announce());
        let up = SnapClassify::push(&mut machine, VK_H, true, true, false).expect("H up");
        assert!(up.consumed() && !up.announce());
        if !owner_first {
            let up = SnapClassify::push(&mut machine, VK_U, true, true, false).expect("U up");
            assert!(up.consumed() && !up.announce());
        }
        assert!(!machine.key_is_down(VK_U));
        assert!(!machine.key_is_down(VK_H));
        assert!(!machine.key_is_down(VK_I));
        assert_eq!(
            SnapClassify::push(&mut machine, VK_U, true, true, false),
            None,
            "no leak: extra U up passes"
        );
    }
}
