//! Workspace catalogue tests: shape, parse round-trips, trailing-0
//! semantics, exact modifier arms, preset matrix over every Meta, and the
//! honest-execution refusal. No live touch on any path: every chord is
//! plain data matched through the pure tap decision function.

use std::collections::HashSet;

use crate::config::ProbeModifier;
use crate::desktop_shortcuts::{KEY_0, KEY_1};
use crate::input::cg_flag_bits;
use crate::input_match::{TapKeyKind, decide_tap_event};
use crate::input_policy::{
    InputPreset, aux_flag_for, effective_bindings, workspace_conflict_reason,
};
use crate::native_input::{tap_specs_for, tile_catalogue};
use crate::runtime_types::{InputDirection, InputSemanticAction, TileBinding};
use crate::workspace_input::*;

const CTRL: u64 = 0x0004_0000;
const CMD: u64 = 0x0010_0000;
const SHIFT: u64 = 0x0002_0000;
const ALT: u64 = 0x0008_0000;

fn combined() -> Vec<TileBinding> {
    let mut out = tile_catalogue();
    out.extend(workspace_bindings());
    out
}

fn row_by_id(id: &str) -> WorkspaceRow {
    workspace_catalogue()
        .iter()
        .find(|row| row.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("workspace row missing: {id}"))
}

#[test]
fn catalogue_shape_is_63_rows_45_defaulted_27_implemented() {
    let catalogue = workspace_catalogue();
    assert_eq!(catalogue.len(), 63);
    assert_eq!(workspace_bindings().len(), 45);
    // Ids unique; (action, key) pairs unique — relative/output letter and
    // arrow rows intentionally share one action (canonical letter slot
    // plus arrow alias, Windows parity), so actions alone are not unique.
    let mut ids = HashSet::new();
    for row in &catalogue {
        assert!(ids.insert(row.id), "duplicate id {}", row.id);
    }
    let mut action_keys = HashSet::new();
    for row in &catalogue {
        let key = format!(
            "{:?}:{}",
            row.action,
            row.default
                .map(|chord| chord.key_code)
                .or(row.unbound_key)
                .unwrap_or(0)
        );
        assert!(action_keys.insert(key), "duplicate action+key {}", row.id);
    }
    // Select-plus-send arming: exactly the 18 select rows plus the 9
    // numbered send-follow rows report implemented (numbered
    // existing-Space 1-9, the 8 relative selects, the previous toggle,
    // and workspace-send-1..9); everything else stays unimplemented
    // (trailing select-0/send-0, all stays, relative sends, all outputs —
    // honestly refused, never faked).
    let implemented: HashSet<&str> = catalogue
        .iter()
        .filter(|row| row.implemented)
        .map(|row| row.id)
        .collect();
    let expected: HashSet<&str> = [
        "workspace-select-1",
        "workspace-select-2",
        "workspace-select-3",
        "workspace-select-4",
        "workspace-select-5",
        "workspace-select-6",
        "workspace-select-7",
        "workspace-select-8",
        "workspace-select-9",
        "workspace-send-1",
        "workspace-send-2",
        "workspace-send-3",
        "workspace-send-4",
        "workspace-send-5",
        "workspace-send-6",
        "workspace-send-7",
        "workspace-send-8",
        "workspace-send-9",
        "workspace-prev-h",
        "workspace-prev-k",
        "workspace-prev-left-arrow",
        "workspace-prev-up-arrow",
        "workspace-next-j",
        "workspace-next-l",
        "workspace-next-down-arrow",
        "workspace-next-right-arrow",
        "workspace-previous",
    ]
    .into_iter()
    .collect();
    assert_eq!(implemented, expected, "armed select-plus-send subset");
    assert!(
        !catalogue
            .iter()
            .find(|row| row.id == "workspace-send-0")
            .expect("send-0 exists")
            .implemented,
        "workspace-send-0 stays unarmed (no trailing append)"
    );
    assert!(
        !catalogue
            .iter()
            .find(|row| row.id == "workspace-select-0")
            .expect("select-0 exists")
            .implemented,
        "trailing select-0 stays unarmed"
    );
    // The product subsets expose the 18 selects plus the 9 numbered
    // send-follows as bindings.
    assert_eq!(crate::workspace_select::bindings().len(), 18);
    assert_eq!(
        crate::workspace_send::bindings().len(),
        9,
        "numbered send-follow 1..9 armed in product"
    );
    // Family counts: 10 select + 10 send + 10 stay + 8 + 8 + 8 relative +
    // 1 previous + 8 output.
    let selects = catalogue
        .iter()
        .filter(|row| matches!(row.action, InputSemanticAction::WorkspaceSelect { .. }))
        .count();
    let sends = catalogue
        .iter()
        .filter(|row| {
            matches!(
                row.action,
                InputSemanticAction::WorkspaceSend { follow: true, .. }
            )
        })
        .count();
    let stays = catalogue
        .iter()
        .filter(|row| {
            matches!(
                row.action,
                InputSemanticAction::WorkspaceSend { follow: false, .. }
            )
        })
        .count();
    assert_eq!((selects, sends, stays), (10, 10, 10));
}

#[test]
fn stay_rows_are_bindable_unbound_with_canonical_keys() {
    // 10 numbered + 8 relative stay rows: no default chord (never armed),
    // parseable ids, and the canonical key slot of the follow arm for a
    // future rebind.
    let catalogue = workspace_catalogue();
    let stays: Vec<_> = catalogue
        .iter()
        .filter(|row| {
            matches!(
                row.action,
                InputSemanticAction::WorkspaceSend { follow: false, .. }
                    | InputSemanticAction::WorkspaceSendRelative { follow: false, .. }
            )
        })
        .collect();
    assert_eq!(stays.len(), 18);
    assert!(stays.iter().all(|row| row.default.is_none()));
    assert!(stays.iter().all(|row| row.binding().is_none()));
    for row in stays {
        let follow = match row.action {
            InputSemanticAction::WorkspaceSend { index, .. } => {
                InputSemanticAction::WorkspaceSend {
                    index,
                    follow: true,
                }
            }
            InputSemanticAction::WorkspaceSendRelative { prev, .. } => {
                InputSemanticAction::WorkspaceSendRelative { prev, follow: true }
            }
            _ => unreachable!(),
        };
        let follow_row = catalogue
            .iter()
            .find(|candidate| {
                candidate.action == follow
                    && candidate.default.map(|chord| chord.key_code) == row.unbound_key
            })
            .expect("every stay shares its follow arm's key");
        let follow_key = follow_row.default.expect("follow arm is bound").key_code;
        assert_eq!(row.unbound_key, Some(follow_key), "{}", row.id);
    }
    // Defaulted rows carry no stay slot.
    assert!(
        catalogue
            .iter()
            .filter(|row| row.default.is_some())
            .all(|row| row.unbound_key.is_none())
    );
}

#[test]
fn parse_maps_every_id_and_canonical_slots_reverse() {
    // Forward: every row id parses to its action (the single source of
    // truth is the catalogue: nothing else parses).
    for row in workspace_catalogue() {
        assert_eq!(
            parse_workspace_action(row.id),
            Some(row.action),
            "{}",
            row.id
        );
    }
    // Reverse: actions shared by a letter row plus its arrow alias map to
    // the canonical letter slot (Windows canonical-slot parity); numbered
    // actions are bijective.
    let canonical: [(InputSemanticAction, &str); 19] = [
        (
            InputSemanticAction::WorkspaceRelative { prev: true },
            "workspace-prev-h",
        ),
        (
            InputSemanticAction::WorkspaceRelative { prev: false },
            "workspace-next-j",
        ),
        (
            InputSemanticAction::WorkspaceSendRelative {
                prev: true,
                follow: true,
            },
            "send-prev-h",
        ),
        (
            InputSemanticAction::WorkspaceSendRelative {
                prev: false,
                follow: true,
            },
            "send-next-j",
        ),
        (
            InputSemanticAction::WorkspaceSendRelative {
                prev: true,
                follow: false,
            },
            "send-stay-prev-h",
        ),
        (
            InputSemanticAction::WorkspaceSendRelative {
                prev: false,
                follow: false,
            },
            "send-stay-next-j",
        ),
        (
            InputSemanticAction::WorkspaceOutput {
                direction: InputDirection::Left,
                follow: true,
            },
            "send-output-left",
        ),
        (
            InputSemanticAction::WorkspaceOutput {
                direction: InputDirection::Down,
                follow: true,
            },
            "send-output-down",
        ),
        (
            InputSemanticAction::WorkspaceOutput {
                direction: InputDirection::Up,
                follow: true,
            },
            "send-output-up",
        ),
        (
            InputSemanticAction::WorkspaceOutput {
                direction: InputDirection::Right,
                follow: true,
            },
            "send-output-right",
        ),
        (
            InputSemanticAction::WorkspaceSelect { index: 1 },
            "workspace-select-1",
        ),
        (
            InputSemanticAction::WorkspaceSelect { index: 9 },
            "workspace-select-9",
        ),
        (
            InputSemanticAction::WorkspaceSelect { index: 0 },
            "workspace-select-0",
        ),
        (
            InputSemanticAction::WorkspaceSend {
                index: 1,
                follow: true,
            },
            "workspace-send-1",
        ),
        (
            InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            },
            "workspace-send-0",
        ),
        (
            InputSemanticAction::WorkspaceSend {
                index: 1,
                follow: false,
            },
            "stay-workspace-1",
        ),
        (
            InputSemanticAction::WorkspaceSend {
                index: 9,
                follow: false,
            },
            "stay-workspace-9",
        ),
        (
            InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: false,
            },
            "stay-workspace-0",
        ),
        (InputSemanticAction::WorkspacePrevious, "workspace-previous"),
    ];
    for (action, id) in canonical.iter().take(19) {
        assert_eq!(workspace_action_id(action), Some(*id), "{action:?}");
    }
    for id in [
        "",
        "focus-left",
        "workspace-select-10",
        "workspace-select-",
        "workspace-send-11",
        "stay-workspace-x",
        "workspace-prev-h-extra",
        "send-output",
        "workspace-previous ",
        "omnitiler-workspace-1",
    ] {
        assert_eq!(parse_workspace_action(id), None, "{id}");
    }
    // Basic actions have no workspace id; output send-and-stay has no
    // approved id yet.
    for action in [
        InputSemanticAction::FocusStep {
            direction: InputDirection::Left,
        },
        InputSemanticAction::ToggleFloat,
        InputSemanticAction::ToggleOrientation,
        InputSemanticAction::WorkspaceOutput {
            direction: InputDirection::Left,
            follow: false,
        },
    ] {
        assert_eq!(workspace_action_id(&action), None, "{action:?}");
    }
}

#[test]
fn trailing_zero_matches_shared_core_semantics() {
    // Index 0 is the trailing empty workspace on every platform: Windows
    // `workspace-select-0` focuses-or-creates it, `workspace-send-0`
    // appends for the mover, KDE index 0 is the append target, and the core
    // trailing policy (`tiler_core::workspace::plan_trailing`) reuses or
    // appends exactly that slot.
    assert_eq!(
        parse_workspace_action("workspace-select-0"),
        Some(InputSemanticAction::WorkspaceSelect { index: 0 })
    );
    assert_eq!(
        parse_workspace_action("workspace-send-0"),
        Some(InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true
        })
    );
    assert_eq!(
        parse_workspace_action("stay-workspace-0"),
        Some(InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false
        })
    );
    let select_0 = row_by_id("workspace-select-0");
    assert!(select_0.text.contains("trailing"));
    // The 0 leg is the `0` digit (Desktop 10 leg), 1 is the `1` digit.
    assert_eq!(select_0.default.expect("select-0 is bound").key_code, KEY_0);
    assert_eq!(
        row_by_id("workspace-select-1")
            .default
            .expect("bound")
            .key_code,
        KEY_1
    );
    // 1-9 round-trip each.
    for index in 1..=9u8 {
        let id = format!("workspace-select-{index}");
        assert_eq!(
            parse_workspace_action(&id),
            Some(InputSemanticAction::WorkspaceSelect { index })
        );
        let id = format!("workspace-send-{index}");
        assert_eq!(
            parse_workspace_action(&id),
            Some(InputSemanticAction::WorkspaceSend {
                index,
                follow: true
            })
        );
    }
}

#[test]
fn numbered_chords_keep_plain_meta_digits() {
    // Numbered rows carry no auxiliary arm: Meta+digit select,
    // Meta+Shift+digit send — under Ctrl exactly the native Desktop-N
    // default shape (the ownership seam covers the same digits; product
    // arms the 1..9 send chords).
    let select_1 = row_by_id("workspace-select-1").default.expect("bound");
    assert_eq!(
        (select_1.shift, select_1.alt, select_1.aux),
        (false, false, false)
    );
    let send_1 = row_by_id("workspace-send-1").default.expect("bound");
    assert_eq!((send_1.shift, send_1.alt, send_1.aux), (true, false, false));
    assert_eq!(
        effective_flags_for(
            ProbeModifier::Ctrl,
            &row_by_id("workspace-select-1").binding().unwrap()
        ),
        CTRL
    );
    assert_eq!(
        effective_flags_for(
            ProbeModifier::Ctrl,
            &row_by_id("workspace-send-1").binding().unwrap()
        ),
        CTRL | SHIFT
    );
}

#[test]
fn auxiliary_chords_use_ctrl_cmd_under_ctrl_and_meta_ctrl_otherwise() {
    // Approved user choice: Meta=Ctrl auxiliaries ride Ctrl+Cmd (never a
    // Ctrl duplicate of the basic chords); any other Meta rides Meta+Ctrl.
    let previous = row_by_id("workspace-previous").binding().unwrap();
    assert_eq!(
        effective_flags_for(ProbeModifier::Ctrl, &previous),
        CTRL | CMD
    );
    assert_eq!(
        effective_flags_for(ProbeModifier::Meta, &previous),
        CMD | CTRL
    );
    assert_eq!(
        effective_flags_for(ProbeModifier::Alt, &previous),
        ALT | CTRL
    );
    assert_eq!(
        effective_flags_for(ProbeModifier::Shift, &previous),
        SHIFT | CTRL
    );
    // Exact key/mod per approved form: Tab previous; H/K/Left/Up previous;
    // J/L/Down/Right next; +Shift sends; +Alt output sends.
    let prev_h = row_by_id("workspace-prev-h").binding().unwrap();
    assert_eq!(
        (prev_h.key_code, prev_h.shift, prev_h.alt, prev_h.aux),
        (crate::input_policy::KEY_H, false, false, true)
    );
    let send_next_l = row_by_id("send-next-l").binding().unwrap();
    assert_eq!(
        effective_flags_for(ProbeModifier::Ctrl, &send_next_l),
        CTRL | CMD | SHIFT
    );
    assert_eq!(send_next_l.key_code, crate::input_policy::KEY_L);
    let output_left = row_by_id("send-output-left").binding().unwrap();
    assert_eq!(
        effective_flags_for(ProbeModifier::Ctrl, &output_left),
        CTRL | CMD | ALT
    );
    assert_eq!(output_left.key_code, crate::input_policy::KEY_H);
    // The auxiliary set avoids the known-taken Ctrl+Cmd F/Q/Space/D keys:
    // Tab plus HJKL, arrows, and digits only (`kVK_ANSI_*` positions in
    // `Events.h`: F 0x03, D 0x02, Q 0x0C, Space 0x31 — none armed here).
    const KNOWN_TAKEN_AUX_KEYS: [u32; 4] = [0x03, 0x02, 0x0C, 0x31];
    for binding in workspace_bindings() {
        if binding.aux {
            assert!(
                !KNOWN_TAKEN_AUX_KEYS.contains(&binding.key_code),
                "aux row on a known-taken key: {:#x}",
                binding.key_code
            );
        }
    }
}

#[test]
fn aux_flag_matches_the_fixture_extra_bit_contract() {
    // Single home: the product auxiliary bit equals the long-standing
    // fixture extra-bit rule (Cmd under Ctrl, else Ctrl) for every Meta.
    for modifier in [
        ProbeModifier::Ctrl,
        ProbeModifier::Meta,
        ProbeModifier::Alt,
        ProbeModifier::Shift,
    ] {
        assert_eq!(
            aux_flag_for(modifier),
            crate::input_session::extra_flag_for(modifier),
            "{modifier:?}"
        );
    }
    assert_eq!(aux_flag_for(ProbeModifier::Ctrl), CMD);
    assert_eq!(aux_flag_for(ProbeModifier::Meta), CTRL);
}

#[test]
fn preset_matrix_all_metas() {
    // Authentic arms everything; Compatible drops only repo-established
    // conflicts (unshifted Ctrl arrows + unshifted Ctrl digits); Alt/Shift
    // Metas refuse validation exactly like the basic catalogue (arm
    // collapse), never silently.
    let basic = tile_catalogue();
    assert_eq!(basic.len(), 34);
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let authentic = effective_bindings(InputPreset::Authentic, modifier, &combined());
        assert_eq!(authentic.len(), 79, "{modifier:?}");
        let config = crate::runtime_types::RuntimeConfig {
            modifier,
            tile_bindings: Some(authentic),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_ok(), "{modifier:?}");
    }
    // Compatible under Ctrl: 79 minus the 4 unshifted arrows minus the 10
    // unshifted digit selects. Shifted digit sends, every aux row, and the
    // output rows stay armed (no established holder).
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &combined());
    assert_eq!(compatible.len(), 65);
    assert!(
        compatible
            .iter()
            .all(|b| !matches!(b.action, InputSemanticAction::WorkspaceSelect { .. })),
        "Compatible/Ctrl leaves digit selects to macOS"
    );
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(
                b.action,
                InputSemanticAction::WorkspaceSend { follow: true, .. }
            ))
            .count(),
        10,
        "shifted digit sends are distinct effective chords: kept"
    );
    assert!(
        compatible
            .iter()
            .any(|b| matches!(b.action, InputSemanticAction::WorkspacePrevious)),
        "previous toggle kept"
    );
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(b.action, InputSemanticAction::WorkspaceOutput { .. }))
            .count(),
        8,
        "output sends kept with the honest unknown-ownership standing"
    );
    let config = crate::runtime_types::RuntimeConfig {
        modifier: ProbeModifier::Ctrl,
        tile_bindings: Some(compatible),
        ..crate::runtime_types::RuntimeConfig::default()
    };
    assert!(config.validate().is_ok());
    // Compatible under Meta: no established holders, full set.
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Meta, &combined());
    assert_eq!(compatible.len(), 79);
    let config = crate::runtime_types::RuntimeConfig {
        modifier: ProbeModifier::Meta,
        tile_bindings: Some(compatible),
        ..crate::runtime_types::RuntimeConfig::default()
    };
    assert!(config.validate().is_ok());
    // Alt/Shift Metas collapse arms (pre-existing basic degeneracy plus the
    // workspace aux arms landing on each other): validation refuses rather
    // than arming a colliding tap.
    for modifier in [ProbeModifier::Alt, ProbeModifier::Shift] {
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            let config = crate::runtime_types::RuntimeConfig {
                modifier,
                tile_bindings: Some(effective_bindings(preset, modifier, &combined())),
                ..crate::runtime_types::RuntimeConfig::default()
            };
            assert!(config.validate().is_err(), "{preset:?}/{modifier:?}");
        }
    }
}

#[test]
fn workspace_bindings_are_duplicate_free_alone() {
    // The workspace set never collides with itself under a usable Meta,
    // and — critically — never with the basic chords (digits live only
    // here, aux rows always carry the extra bit).
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let config = crate::runtime_types::RuntimeConfig {
            modifier,
            tile_bindings: Some(workspace_bindings()),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_ok(), "{modifier:?}");
    }
}

#[test]
fn conflict_table_covers_digits_and_aux() {
    use crate::desktop_shortcuts::KEY_2;
    // Unshifted Ctrl digits conflict (native Desktop-N holders); the
    // shifted send arm, aux chords, and non-Ctrl Metas do not.
    assert!(workspace_conflict_reason(KEY_1, false, false, false, ProbeModifier::Ctrl).is_some());
    assert!(workspace_conflict_reason(KEY_2, true, false, false, ProbeModifier::Ctrl).is_none());
    assert!(workspace_conflict_reason(KEY_2, false, false, true, ProbeModifier::Ctrl).is_none());
    assert!(workspace_conflict_reason(KEY_1, false, false, false, ProbeModifier::Meta).is_none());
    assert!(workspace_conflict_reason(KEY_TAB, false, false, true, ProbeModifier::Ctrl).is_none());
    // Basic arrow rule unchanged through the workspace entry point.
    assert!(
        workspace_conflict_reason(
            crate::input_policy::KEY_H,
            false,
            false,
            false,
            ProbeModifier::Ctrl
        )
        .is_none()
    );
    assert!(
        workspace_conflict_reason(
            crate::input_policy::KEY_LEFT,
            false,
            false,
            false,
            ProbeModifier::Ctrl
        )
        .is_some()
    );
}

#[test]
fn tap_tuning_matches_exact_aux_chords() {
    // The armed spec for the previous toggle is exactly Ctrl+Cmd+Tab: the
    // exact chord matches, a missing Cmd leg or an extra Shift leg fails.
    let previous = row_by_id("workspace-previous").binding().unwrap();
    let specs = tap_specs_for(ProbeModifier::Ctrl, &[previous]);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].want_flags, CTRL | CMD);
    let (_, mask) = cg_flag_bits(ProbeModifier::Ctrl);
    assert_eq!(specs[0].flags_mask, mask);
    let matched = |flags: u64| {
        let mut pending = false;
        decide_tap_event(
            TapKeyKind::KeyDown,
            flags,
            i64::from(KEY_TAB),
            crate::input_match::TapTuning {
                want_flags: specs[0].want_flags,
                flags_mask: specs[0].flags_mask,
                want_code: i64::from(KEY_TAB),
            },
            true,
            &mut pending,
        )
        .matched
    };
    assert!(matched(CTRL | CMD));
    assert!(!matched(CTRL), "missing the auxiliary leg");
    assert!(!matched(CTRL | CMD | SHIFT), "extra Shift leg fails");
    assert!(!matched(CMD), "missing the Meta leg");
    // Spec construction agrees with the single-home effective flags for
    // every workspace binding under Ctrl and Meta.
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let bindings = workspace_bindings();
        let specs = tap_specs_for(modifier, &bindings);
        assert_eq!(specs.len(), bindings.len());
        for (spec, binding) in specs.iter().zip(bindings.iter()) {
            assert_eq!(spec.want_flags, effective_flags_for(modifier, binding));
            assert_eq!(spec.key_code, binding.key_code);
        }
    }
}

#[test]
fn unsupported_refusal_names_the_seam_per_family() {
    // Basic actions execute (no refusal), the armed select subset
    // dispatches (no refusal), and the armed numbered send-follow 1..9
    // dispatches (no refusal); every other workspace family refuses with a
    // distinct static token and a reason naming its exact missing route —
    // never a fake dispatch.
    for action in [
        InputSemanticAction::FocusStep {
            direction: InputDirection::Left,
        },
        InputSemanticAction::ToggleFloat,
        InputSemanticAction::ToggleOrientation,
    ] {
        assert_eq!(workspace_unsupported_outcome(&action), None, "{action:?}");
        assert_eq!(workspace_unsupported_reason(&action), None, "{action:?}");
        assert!(!is_armed_select(&action), "{action:?}");
        assert!(!is_armed_send(&action), "{action:?}");
    }
    for action in [
        InputSemanticAction::WorkspaceSelect { index: 1 },
        InputSemanticAction::WorkspaceSelect { index: 9 },
        InputSemanticAction::WorkspaceRelative { prev: true },
        InputSemanticAction::WorkspaceRelative { prev: false },
        InputSemanticAction::WorkspacePrevious,
    ] {
        assert_eq!(workspace_unsupported_outcome(&action), None, "{action:?}");
        assert_eq!(workspace_unsupported_reason(&action), None, "{action:?}");
        assert!(is_armed_select(&action), "{action:?}");
        assert!(!is_armed_send(&action), "{action:?}");
    }
    for index in [1u8, 9] {
        let action = InputSemanticAction::WorkspaceSend {
            index,
            follow: true,
        };
        assert_eq!(
            workspace_unsupported_reason(&action),
            None,
            "{action:?} dispatches"
        );
        assert_eq!(workspace_unsupported_outcome(&action), None, "{action:?}");
        assert!(!is_armed_select(&action), "{action:?}");
        assert!(is_armed_send(&action), "{index}");
    }
    // Trailing send-0 stays refused.
    {
        let action = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        };
        let reason = workspace_unsupported_reason(&action).expect("send-0 refuses");
        assert!(
            reason.contains("never runs from the product tile"),
            "{reason}"
        );
        assert_eq!(
            workspace_unsupported_outcome(&action),
            Some("refused-workspace-send-unsupported"),
            "{action:?}"
        );
        assert!(!is_armed_send(&action), "{action:?}");
    }
    // The trailing select-0 stays unarmed: automatic trailing creation is
    // unresolved (occupancy unproven), so it refuses with the switch token.
    let reason = workspace_unsupported_reason(&InputSemanticAction::WorkspaceSelect { index: 0 })
        .expect("trailing select refuses");
    assert!(reason.contains("trailing"), "{reason}");
    assert_eq!(
        workspace_unsupported_outcome(&InputSemanticAction::WorkspaceSelect { index: 0 }),
        Some("refused-workspace-switch-unsupported")
    );
    assert!(!is_armed_select(&InputSemanticAction::WorkspaceSelect {
        index: 0
    }));
    let reason = workspace_unsupported_reason(&InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: false,
    })
    .expect("stay refuses");
    assert!(
        reason.contains("never runs from the product tile"),
        "{reason}"
    );
    let reason = workspace_unsupported_reason(&InputSemanticAction::WorkspaceSendRelative {
        prev: false,
        follow: true,
    })
    .expect("relative send refuses");
    assert!(
        reason.contains("never runs from the product tile"),
        "{reason}"
    );
    let reason = workspace_unsupported_reason(&InputSemanticAction::WorkspaceOutput {
        direction: InputDirection::Right,
        follow: true,
    })
    .expect("output refuses");
    assert!(reason.contains("cross-display"), "{reason}");
    assert_eq!(
        workspace_unsupported_outcome(&InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true
        }),
        Some("refused-workspace-send-unsupported")
    );
    assert_eq!(
        workspace_unsupported_outcome(&InputSemanticAction::WorkspaceOutput {
            direction: InputDirection::Left,
            follow: true,
        }),
        Some("refused-workspace-output-unsupported")
    );
    // Sends, stays, and outputs are never armed selects; selects and basic
    // actions are never send-shaped. Only 1-9 follow rows are send-shaped
    // (dispatched); index 0, stays, relative sends, and outputs refuse.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 1,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: false,
            follow: false,
        },
        InputSemanticAction::WorkspaceOutput {
            direction: InputDirection::Left,
            follow: true,
        },
    ] {
        assert!(!is_armed_select(&action), "{action:?}");
    }
    for action in [
        InputSemanticAction::WorkspaceSelect { index: 1 },
        InputSemanticAction::WorkspaceRelative { prev: true },
        InputSemanticAction::WorkspacePrevious,
        InputSemanticAction::FocusStep {
            direction: InputDirection::Left,
        },
    ] {
        assert!(!is_armed_send(&action), "{action:?}");
    }
}

#[test]
fn numbered_index_validates_0_to_9() {
    let ok = |action: InputSemanticAction| {
        TileBinding {
            key_code: KEY_1,
            shift: false,
            alt: false,
            aux: false,
            action,
        }
        .validate()
        .is_ok()
    };
    assert!(ok(InputSemanticAction::WorkspaceSelect { index: 0 }));
    assert!(ok(InputSemanticAction::WorkspaceSelect { index: 9 }));
    assert!(ok(InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: false
    }));
    assert!(!ok(InputSemanticAction::WorkspaceSelect { index: 10 }));
    assert!(!ok(InputSemanticAction::WorkspaceSend {
        index: 16,
        follow: true
    }));
}

#[test]
fn product_select_matrix_is_duplicate_free_with_exact_arms() {
    // Product tap table: 34 basic chords plus the 18 armed selects plus
    // the 9 armed numbered send-follows (61). Duplicate-free under every
    // usable Meta/preset; Compatible under Ctrl leaves the 9 numbered
    // selects plus the 4 unshifted arrow basics to macOS (48 = 30 basic +
    // 9 aux selects + 9 sends), the numbers still switching natively via
    // Switch to Desktop N. Alt/Shift Metas collapse arms and refuse
    // validation rather than arming a colliding tap.
    fn product() -> Vec<TileBinding> {
        let mut out = tile_catalogue();
        out.extend(crate::workspace_select::bindings());
        out.extend(crate::workspace_send::bindings());
        out
    }
    assert_eq!(tile_catalogue().len(), 34);
    assert_eq!(crate::workspace_select::bindings().len(), 18);
    assert_eq!(crate::workspace_send::bindings().len(), 9);
    assert_eq!(product().len(), 61);
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let authentic = effective_bindings(InputPreset::Authentic, modifier, &product());
        assert_eq!(authentic.len(), 61, "{modifier:?}");
        let config = crate::runtime_types::RuntimeConfig {
            modifier,
            tile_bindings: Some(authentic),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_ok(), "{modifier:?}");
    }
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &product());
    assert_eq!(compatible.len(), 48, "30 basic + 9 aux selects + 9 sends");
    assert!(
        compatible
            .iter()
            .all(|b| !matches!(b.action, InputSemanticAction::WorkspaceSelect { .. })),
        "Compatible/Ctrl leaves numbered selects to native Switch to Desktop N"
    );
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(b.action, InputSemanticAction::WorkspaceRelative { .. }))
            .count(),
        8,
        "all 8 relative-select rows stay armed"
    );
    assert!(
        compatible
            .iter()
            .any(|b| matches!(b.action, InputSemanticAction::WorkspacePrevious)),
        "previous toggle stays armed"
    );
    // No stays, relative sends, outputs, or trailing select-0/send-0
    // enter the product table under any preset; numbered send-follow 1..9
    // does.
    for preset in [InputPreset::Authentic, InputPreset::Compatible] {
        for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
            let armed = effective_bindings(preset, modifier, &product());
            assert!(
                armed.iter().all(|b| {
                    is_armed_select(&b.action)
                        || is_armed_send(&b.action)
                        || matches!(
                            b.action,
                            InputSemanticAction::FocusStep { .. }
                                | InputSemanticAction::MoveStep { .. }
                                | InputSemanticAction::ResizeStep { .. }
                                | InputSemanticAction::ToggleFloat
                                | InputSemanticAction::ToggleOrientation
                        )
                }),
                "{preset:?}/{modifier:?}"
            );
            assert!(
                armed.iter().all(|b| !matches!(
                    b.action,
                    InputSemanticAction::WorkspaceSelect { index: 0 }
                )),
                "trailing select-0 never armed: {preset:?}/{modifier:?}"
            );
            assert!(
                armed.iter().all(|b| !matches!(
                    b.action,
                    InputSemanticAction::WorkspaceSend { index: 0, .. }
                )),
                "trailing send-0 never armed: {preset:?}/{modifier:?}"
            );
        }
    }
    // Numbered send-follow 1..9 rides the product table under both presets
    // (shifted digits are distinct effective chords with no established
    // holder).
    for preset in [InputPreset::Authentic, InputPreset::Compatible] {
        let armed = effective_bindings(preset, ProbeModifier::Ctrl, &product());
        assert_eq!(
            armed.iter().filter(|b| is_armed_send(&b.action)).count(),
            9,
            "{preset:?}"
        );
    }
    let config = crate::runtime_types::RuntimeConfig {
        modifier: ProbeModifier::Ctrl,
        tile_bindings: Some(compatible),
        ..crate::runtime_types::RuntimeConfig::default()
    };
    assert!(config.validate().is_ok());
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Meta, &product());
    assert_eq!(compatible.len(), 61);
    for modifier in [ProbeModifier::Alt, ProbeModifier::Shift] {
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            let config = crate::runtime_types::RuntimeConfig {
                modifier,
                tile_bindings: Some(effective_bindings(preset, modifier, &product())),
                ..crate::runtime_types::RuntimeConfig::default()
            };
            assert!(config.validate().is_err(), "{preset:?}/{modifier:?}");
        }
    }
}
