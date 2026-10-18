#![cfg(test)]
//! Numbered send-follow seam tests: send-shape capability, world mapping,
//! Authentic ownership (apply/resume/revert, Compatible zero-touch),
//! admission, and feedback reconciliation. No live touch on any path:
//! every holder is a fake entry, every path a temp file. Numbered
//! send-follow 1..9 is product-armed (catalogue `implemented: true`,
//! engine admits, tap table carries the chords); trailing send-0, stays,
//! relative sends, and outputs stay refused. The production effect path
//! (engine facts, per-edge proof, tagged posts, terminal reaper) is proven
//! separately in `workspace_send_live_tests` over scripted native readers.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::session::{DomainKey, Session};
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptKind, AttemptOutcome, HoldReason, WorldCommand, validate_report,
};

use crate::ffi::{SlsDisplayRow, SlsSpaceType};
use crate::runtime_types::{NativeContext, ObservedWindow};
use crate::topology::{PointsRect, TopologySnapshot};
use crate::world_builder::{ReportInput, build_report};

use crate::desktop_shortcuts::{
    FakeStore, ShortcutEntry, desktop_digit_keycode, desktop_symbolic_id, lock_path_for,
};
use crate::input_policy::InputPreset;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction, TileBinding};
use crate::workspace_send::*;

const CG_CTRL: u64 = 0x0004_0000;

fn send(index: u8) -> InputSemanticAction {
    InputSemanticAction::WorkspaceSend {
        index,
        follow: true,
    }
}

fn enabled_entry(desktop: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: true,
        keycode: desktop_digit_keycode(desktop).unwrap(),
        modifiers: CG_CTRL,
    }
}

/// Product send-follow capability: the 9 numbered send-follow shapes
/// 1..9 from the armed product `bindings()` (catalogue
/// `implemented: true` plus `is_armed_send`).
fn send_shape_bindings() -> Vec<TileBinding> {
    bindings()
}

#[test]
fn send_follow_shape_armed_in_product() {
    // Product arming: the catalogue marks 1..9 `implemented: true`,
    // `bindings()` arms their 9 Ctrl+Shift+digit chords, and the engine
    // admits them (unsupported shapes still refuse — see below).
    let armed = send_shape_bindings();
    assert_eq!(armed.len(), 9, "product arms 9 send shapes");
    assert_eq!(armed.len(), bindings().len());
    let mut seen = Vec::new();
    for binding in &armed {
        let InputSemanticAction::WorkspaceSend { index, follow } = binding.action else {
            panic!("send shape carries a non-send: {:?}", binding.action);
        };
        assert!(follow, "seam sends always follow");
        assert!(
            (1..=9).contains(&index),
            "seam sends are 1..=9, got {index}"
        );
        assert!(binding.shift && !binding.alt && !binding.aux, "{index}");
        seen.push(index);
    }
    seen.sort_unstable();
    assert_eq!(seen, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    // Catalogue reality: 1..9 implemented, send-0 and stays unimplemented.
    for row in crate::workspace_input::workspace_catalogue() {
        match &row.action {
            InputSemanticAction::WorkspaceSend {
                index,
                follow: true,
            } if (1..=9).contains(index) => {
                assert!(row.implemented, "{} must be implemented:true", row.id);
            }
            InputSemanticAction::WorkspaceSend { .. } => {
                assert!(
                    !row.implemented,
                    "{} must stay implemented:false in product",
                    row.id
                );
            }
            _ => {}
        }
    }
    // Trailing send-0, stays, relative sends, and outputs stay out even
    // though they are defaulted/bound elsewhere.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 1,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        InputSemanticAction::WorkspaceOutput {
            direction: crate::runtime_types::InputDirection::Left,
            follow: true,
        },
    ] {
        assert!(
            !crate::workspace_input::is_armed_send(&action),
            "{action:?}"
        );
        assert!(
            crate::workspace_input::workspace_unsupported_reason(&action).is_some(),
            "{action:?} stays refused"
        );
    }
    for index in 1..=9u8 {
        let action = send(index);
        // Armed shapes dispatch: the engine admits them (no unsupported
        // reason/outcome) while the shape classifiers still agree.
        assert!(crate::workspace_input::is_armed_send(&action), "{index}");
        assert!(!crate::workspace_input::is_armed_select(&action), "{index}");
        assert_eq!(
            crate::workspace_input::workspace_unsupported_reason(&action),
            None,
            "{index} dispatches in product"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&action),
            None,
            "{index}"
        );
    }
}

#[test]
fn send_chords_match_settings_ctrl_shift_digits() {
    // Ctrl+Shift+digit under Ctrl (settings parity); stay rows stay
    // bindable unbound (no chord); aux rows stay Ctrl+Cmd. Uses the armed
    // product bindings.
    use crate::config::ProbeModifier;
    use crate::workspace_input::effective_flags_for;
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let armed = send_shape_bindings();
    assert_eq!(armed.len(), 9);
    for binding in &armed {
        assert_eq!(
            effective_flags_for(ProbeModifier::Ctrl, binding),
            CTRL | SHIFT,
            "{:?}",
            binding.action
        );
    }
    // The tap spec category for the send shape is basic (a send needs
    // focus): the `select` flag stays is_armed_select-only.
    let specs = crate::native_input::tap_specs_for(ProbeModifier::Ctrl, &armed);
    assert_eq!(specs.len(), 9);
    assert!(
        specs.iter().all(|s| !s.select),
        "send shapes ride the basic eligibility cache (focus-gated), never the select cache"
    );
}

#[test]
fn send_command_maps_send_shapes_to_explicit_sid_transfers() {
    // Product send mapping: the engine admits 1..9 and the adapter carries
    // the admission-resolved native SID as an explicit `Send` target — the
    // ordinal resolves exactly once at admission against the native row and
    // is never re-resolved through the scoped ring (whose stable-logical
    // order need not match native Desktop order).
    use crate::world_command_map::{OP_SEND, select_command, send_command};
    assert_ne!(OP_SEND, crate::world_command_map::OP_SELECT);
    let output = OutputId("macos-display-1".to_owned());
    for index in 1..=9u8 {
        let target = crate::topology::workspace_key(u64::from(index));
        let command = send_command(&send(index), "4242_7", output.clone(), &target)
            .unwrap_or_else(|| panic!("send {index} has a world route"));
        assert_eq!(
            command,
            WorldCommand::Send {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                target_output: output.clone(),
                target_workspace: target,
                follow: true,
            }
        );
        // Sends never ride the select route.
        assert_eq!(select_command(&send(index), output.clone()), None);
    }
    // Unsupported actions have no send route.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 1,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: false,
            follow: true,
        },
        InputSemanticAction::WorkspaceSelect { index: 1 },
        InputSemanticAction::ToggleFloat,
    ] {
        assert_eq!(
            send_command(&action, "4242_7", output.clone(), "space-1"),
            None,
            "{action:?}"
        );
    }
}

#[test]
fn ordinal_resolution_is_positional_never_guessed() {
    assert_eq!(send_target_sid(&[20, 3, 7], 1), Some(20));
    assert_eq!(send_target_sid(&[20, 3, 7], 3), Some(7));
    assert_eq!(send_target_sid(&[20, 3, 7], 0), None);
    assert_eq!(send_target_sid(&[20, 3, 7], 4), None);
    assert_eq!(send_target_sid(&[], 1), None);
}

#[test]
fn context_row_resolution_follows_native_order_per_output() {
    use crate::runtime_types::{NativeContext, NativeDomain};
    use tiler_core::geometry::Rect;
    let domain = |output: &str, space: u64| NativeDomain {
        cg_id: 1,
        uuid: None,
        output: OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(format!("space-{space}")),
        space,
        current: space == 20,
        readable: true,
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1440,
            h: 900,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 1440,
            h: 900,
        },
    };
    let ctx = NativeContext {
        revision: 9,
        domains: vec![
            domain("macos-display-1", 20),
            domain("macos-display-1", 3),
            domain("macos-display-2", 40),
        ],
        skipped: Vec::new(),
    };
    let out1 = OutputId("macos-display-1".to_owned());
    assert_eq!(context_target_sid(&ctx, &out1, 1), Some(20));
    assert_eq!(context_target_sid(&ctx, &out1, 2), Some(3));
    assert_eq!(context_target_sid(&ctx, &out1, 3), None);
    assert_eq!(context_target_sid(&ctx, &out1, 0), None);
    assert_eq!(
        context_target_sid(&ctx, &OutputId("macos-display-9".to_owned()), 1),
        None
    );
}

#[test]
fn compatible_performs_zero_store_touch() {
    let mut store = FakeStore::new().with_entry(
        118,
        ShortcutEntry {
            enabled: false,
            keycode: 0x12,
            modifiers: CG_CTRL,
        },
    );
    let path = std::env::temp_dir().join(format!(
        "omnitiler-send-compatible-{}-{}.json",
        std::process::id(),
        "zero-touch"
    ));
    assert!(!preset_enables_desktop_holders(InputPreset::Compatible));
    assert!(preset_enables_desktop_holders(InputPreset::Authentic));
    let outcome = ensure_holders_for_preset(InputPreset::Compatible, &mut store, &path)
        .expect("compatible never refuses");
    assert_eq!(outcome, None);
    assert_eq!((store.reads, store.writes), (0, 0), "zero store touch");
    assert!(!path.exists(), "no preimage without Authentic");
}

#[test]
fn offline_holder_seam_enable_resume_preserve_and_conditional_revert() {
    // Fake-proven ownership seam only (not wired to any runtime path):
    // Desktops 1-9 disabled would enable with owned preimages.
    let path = std::env::temp_dir().join(format!(
        "omnitiler-send-authentic-{}-{}.json",
        std::process::id(),
        "resume"
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(lock_path_for(&path));
    // Desktops 1-9 disabled: Authentic enables with owned preimages.
    let mut store = FakeStore::new();
    for desktop in 1..=9u32 {
        store = store.with_entry(
            desktop_symbolic_id(desktop).unwrap(),
            ShortcutEntry {
                enabled: false,
                keycode: desktop_digit_keycode(desktop).unwrap(),
                modifiers: CG_CTRL,
            },
        );
    }
    let outcome = ensure_holders_for_preset(InputPreset::Authentic, &mut store, &path)
        .expect("authentic ensure")
        .expect("authentic applies");
    assert_eq!(outcome.results.len(), 9);
    assert!(outcome.failures.is_empty());
    assert!(!outcome.resumed);
    assert!(path.exists(), "durable preimage published before writes");
    let writes_after_first = store.writes;
    assert!(writes_after_first > 0);
    // Resume: already-applied IDs verify without rewrite.
    let outcome = ensure_holders_for_preset(InputPreset::Authentic, &mut store, &path)
        .expect("resume")
        .expect("resume applies");
    assert!(outcome.resumed);
    assert_eq!(outcome.results.len(), 9);
    assert_eq!(
        store.writes, writes_after_first,
        "resume never rewrites owned holders"
    );
    // Later external edit is preserved, never overwritten.
    store.entries.insert(
        118,
        ShortcutEntry {
            enabled: true,
            keycode: 0x13,
            modifiers: CG_CTRL,
        },
    );
    let outcome = ensure_holders_for_preset(InputPreset::Authentic, &mut store, &path)
        .expect("external")
        .expect("external applies");
    assert!(outcome.external_modified.contains(&118));
    assert_eq!(
        store.get(118).unwrap().keycode,
        0x13,
        "external chord preserved verbatim"
    );
    // Conditional revert: the externally edited id keeps its edit and the
    // preimage file is retained; the untouched ids restore.
    let revert =
        crate::desktop_shortcuts::revert_with_store_from(&mut store, &path).expect("revert runs");
    assert!(revert.external_modified.contains(&118));
    assert!(
        revert.preimage_retained,
        "retained while an external edit stands"
    );
    assert!(path.exists());
    // Restore the owned chord, then a clean full revert removes the file.
    store.entries.insert(118, enabled_entry(1).applied());
    let revert =
        crate::desktop_shortcuts::revert_with_store_from(&mut store, &path).expect("clean revert");
    assert!(revert.external_modified.is_empty());
    assert!(revert.failures.is_empty());
    assert!(!revert.preimage_retained);
    assert!(!path.exists(), "clean full revert removes the preimage");
    let _ = std::fs::remove_file(lock_path_for(&path));
}

#[test]
fn authentic_unreadable_holder_fails_per_id_without_blocking_siblings() {
    let path = std::env::temp_dir().join(format!(
        "omnitiler-send-authentic-{}-{}.json",
        std::process::id(),
        "partial"
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(lock_path_for(&path));
    let mut store = FakeStore::new();
    for desktop in 1..=9u32 {
        store = store.with_entry(
            desktop_symbolic_id(desktop).unwrap(),
            ShortcutEntry {
                enabled: false,
                keycode: desktop_digit_keycode(desktop).unwrap(),
                modifiers: CG_CTRL,
            },
        );
    }
    store.fail_reads.insert(desktop_symbolic_id(2).unwrap());
    let outcome = ensure_holders_for_preset(InputPreset::Authentic, &mut store, &path)
        .expect("partial ensure")
        .expect("partial applies");
    assert_eq!(outcome.results.len(), 8);
    assert_eq!(outcome.failures.len(), 1);
    assert!(path.exists(), "preimage retained for retry/audit");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(lock_path_for(&path));
}

#[test]
fn admission_serializes_and_refuses_without_topology() {
    use crate::runtime_thread::admit_send_claim;
    let good = ClaimedSelect::tap(send(2));
    assert!(admit_send_claim(true, false, false, good).is_ok());
    // Unsupported (trailing, stay, relative, select, basic) refuses.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        InputSemanticAction::WorkspaceSelect { index: 2 },
        InputSemanticAction::ToggleFloat,
    ] {
        assert_eq!(
            admit_send_claim(true, false, false, ClaimedSelect::tap(action)),
            Err("refused-send-unarmed")
        );
    }
    assert_eq!(
        admit_send_claim(false, false, false, ClaimedSelect::tap(send(2))),
        Err("refused-send-no-topology")
    );
    // Busy drain or a pending claim (select OR send — one shared slot)
    // refuses instead of overlapping.
    assert_eq!(
        admit_send_claim(true, true, false, ClaimedSelect::tap(send(2))),
        Err("refused-send-busy")
    );
    assert_eq!(
        admit_send_claim(true, false, true, ClaimedSelect::tap(send(2))),
        Err("refused-send-busy")
    );
}

#[test]
fn feedback_kinds_and_outcomes_reconcile_engine_world() {
    // Sends settle transfers; selects settle views (single home for the
    // discard/feedback kind branch).
    assert_eq!(feedback_kind_for(&send(2)), AttemptKind::Transfer);
    assert_eq!(
        feedback_kind_for(&InputSemanticAction::WorkspaceSelect { index: 2 }),
        AttemptKind::View
    );
    assert_eq!(
        feedback_kind_for(&InputSemanticAction::FocusStep {
            direction: crate::runtime_types::InputDirection::Left
        }),
        AttemptKind::View
    );
    // Outcome mapping never claims false success: only verified arrival is
    // Applied; pre-actuation is proven refusal; everything else holds.
    assert_eq!(
        attempt_outcome_for(SendDrainOutcome::Applied),
        AttemptOutcome::Applied
    );
    assert_eq!(
        attempt_outcome_for(SendDrainOutcome::Refused),
        AttemptOutcome::RefusedProven
    );
    assert_eq!(
        attempt_outcome_for(SendDrainOutcome::PossiblyExecuted),
        AttemptOutcome::PossiblyExecuted
    );
    // Pre-actuation refusals classify proven (the desktop was never
    // touched), so the core settles instead of retaining forever.
    assert_eq!(
        classify_send_drain_error("send drag refused before effect at chord-down: injected"),
        SendDrainOutcome::Refused
    );
    // Effect-outcome mapping for the async controller settlement.
    use crate::workspace_send_effect::SendEffectOutcome;
    assert_eq!(
        send_drain_outcome_for(SendEffectOutcome::Applied),
        SendDrainOutcome::Applied
    );
    assert_eq!(
        send_drain_outcome_for(SendEffectOutcome::RefusedProven),
        SendDrainOutcome::Refused
    );
    assert_eq!(
        send_drain_outcome_for(SendEffectOutcome::PossiblyExecuted),
        SendDrainOutcome::PossiblyExecuted
    );
    // Unknown strings fail safe toward the hold.
    assert_eq!(
        classify_send_drain_error("something unexpected"),
        SendDrainOutcome::PossiblyExecuted
    );
    assert_eq!(
        classify_send_drain_error("send drag refused before effect"),
        SendDrainOutcome::Refused
    );
}

#[test]
fn tile_bindings_cover_basic_plus_selects_plus_sends() {
    // Product tap table: 34 basic + 18 armed selects + 9 armed numbered
    // send-follows (61). Duplicate-free under every usable Meta/preset;
    // Compatible under Ctrl leaves the 9 numbered selects plus the 4
    // unshifted arrow basics to macOS (48 = 30 basic + 9 aux selects + 9
    // sends).
    use crate::config::ProbeModifier;
    use crate::input_policy::{InputPreset, effective_bindings};
    use crate::native_input::tile_catalogue;
    fn product() -> Vec<TileBinding> {
        let mut out = tile_catalogue();
        out.extend(crate::workspace_select::bindings());
        out.extend(bindings());
        out
    }
    assert_eq!(tile_catalogue().len(), 34);
    assert_eq!(crate::workspace_select::bindings().len(), 18);
    assert_eq!(bindings().len(), 9, "product arms 9 numbered send-follows");
    assert_eq!(product().len(), 61);
    assert_eq!(crate::tile::product_catalogue().len(), 61);
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
    assert_eq!(
        compatible
            .iter()
            .filter(|b| crate::workspace_input::is_armed_send(&b.action))
            .count(),
        9,
        "numbered send-follow enters the product table (shifted digits kept)"
    );
    // Unsupported sends (trailing, stays, relative, outputs) never enter.
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(
                b.action,
                InputSemanticAction::WorkspaceSend { index: 0, .. }
                    | InputSemanticAction::WorkspaceSend { follow: false, .. }
                    | InputSemanticAction::WorkspaceSendRelative { .. }
                    | InputSemanticAction::WorkspaceOutput { .. }
            ))
            .count(),
        0,
        "unsupported send shapes never enter the product table"
    );
    let config = crate::runtime_types::RuntimeConfig {
        modifier: ProbeModifier::Ctrl,
        tile_bindings: Some(compatible),
        ..crate::runtime_types::RuntimeConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn armed_hotkey_admits_and_maps_to_transfer_feedback() {
    // Producer-shaped pathway (no live touch): the armed product chord for
    // send-2 carries Ctrl+Shift+digit, the engine admits the tap claim,
    // the world maps it to an explicit-SID `Send` transfer, and the feedback
    // classifiers settle transfers (never views). Unsupported shapes still
    // refuse admission and engine dispatch.
    use crate::config::ProbeModifier;
    use crate::runtime_thread::admit_send_claim;
    use crate::workspace_input::{effective_flags_for, workspace_unsupported_outcome};
    use crate::world_command_map::send_command;
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let binding = bindings()
        .into_iter()
        .find(|b| b.action == send(2))
        .expect("send-2 armed in product");
    assert_eq!(
        effective_flags_for(ProbeModifier::Ctrl, &binding),
        CTRL | SHIFT,
        "armed send-2 is Ctrl+Shift+digit"
    );
    assert_eq!(workspace_unsupported_outcome(&binding.action), None);
    let claim = ClaimedSelect::tap(binding.action.clone());
    assert!(admit_send_claim(true, false, false, claim).is_ok());
    let output = OutputId("macos-display-1".to_owned());
    let command = send_command(
        &binding.action,
        "4242_7",
        output.clone(),
        &crate::topology::workspace_key(2),
    )
    .expect("world route");
    assert_eq!(
        command,
        tiler_core::world_report::WorldCommand::Send {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            target_output: output,
            target_workspace: crate::topology::workspace_key(2),
            follow: true,
        }
    );
    assert_eq!(feedback_kind_for(&binding.action), AttemptKind::Transfer);
    assert_eq!(
        attempt_outcome_for(SendDrainOutcome::Applied),
        AttemptOutcome::Applied
    );
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: false,
        },
        InputSemanticAction::WorkspaceSendRelative {
            prev: true,
            follow: true,
        },
        InputSemanticAction::WorkspaceOutput {
            direction: crate::runtime_types::InputDirection::Left,
            follow: true,
        },
    ] {
        assert!(
            workspace_unsupported_outcome(&action).is_some(),
            "{action:?}"
        );
        assert_eq!(
            admit_send_claim(true, false, false, ClaimedSelect::tap(action.clone())),
            Err("refused-send-unarmed"),
            "{action:?}"
        );
        assert_eq!(
            send_command(
                &action,
                "4242_7",
                OutputId("macos-display-1".to_owned()),
                "space-2"
            ),
            None,
            "{action:?}"
        );
    }
}

#[test]
fn readiness_line_pins_scoped_runtime_config() {
    assert_eq!(
        format_send_readiness_line("tap", "authentic", 1, 57),
        "tiler-macos send readiness runtime-started input=tap preset=authentic scopes=1 chords=57",
    );
    // Off route and zero-touch preset render verbatim (never guessed).
    assert_eq!(
        format_send_readiness_line("off", "compatible", 0, 48),
        "tiler-macos send readiness runtime-started input=off preset=compatible scopes=0 chords=48",
    );
}

#[test]
fn armed_chords_are_honest_per_input_route() {
    // Tap route arms the whole effective catalogue; the off source parks
    // until stop, so the readiness line reports zero there.
    assert_eq!(send_armed_chords(true, 57), 57);
    assert_eq!(send_armed_chords(false, 48), 0);
    assert_eq!(send_armed_chords(false, 0), 0);
}

#[test]
fn admitted_line_pins_subject_target_source_and_op() {
    assert_eq!(
        format_send_admitted_line("send-2", "4242_7", 277, 1, "main", "op-9"),
        "tiler-macos send admitted action=send-2 subject=4242_7 target=277 from=1 display=main op=op-9 submitted complete=false",
    );
}

/// Synthetic single-display snapshot for the offline send-order regression:
/// SLS row order `[1, 277]` with `current` naming the startup Space. Display
/// geometry, PID/WID, and frames are synthetic shapes only; the Space IDs
/// are the stable native identities under test. The UUID is synthetic and
/// never a live machine value (separate-Spaces homing only needs the row
/// identifier to equal the display UUID exactly).
const REPRO_UUID: &str = "deadbeef-dead-4000-8000-deadbeefdead";
const REPRO_OWNER: &str = "macos-phase-1";
const REPRO_GENERATION: &str = "gen-1";
const REPRO_INCARNATION: &str = "adapter-1";

fn repro_snapshot(current: u64) -> TopologySnapshot {
    let displays = vec![crate::topology::DisplayGeometry {
        cg_id: 1,
        uuid: Some(REPRO_UUID.to_owned()),
        cg_bounds: Some(PointsRect {
            x: 0.0,
            y: 0.0,
            w: 2240.0,
            h: 1260.0,
        }),
        visible: Some(PointsRect {
            x: 0.0,
            y: 25.0,
            w: 2240.0,
            h: 1235.0,
        }),
        scale: 2.0,
    }];
    let rows = vec![SlsDisplayRow {
        identifier: Some(REPRO_UUID.to_owned()),
        spaces: vec![1, 277],
        current: Some(current),
    }];
    let revision = crate::topology::fingerprint_snapshot(&displays, &rows, Some(true));
    TopologySnapshot {
        displays,
        rows,
        separate_spaces: Some(true),
        revision,
    }
}

fn repro_ctx(current: u64) -> NativeContext {
    let snapshot = repro_snapshot(current);
    let (ctx, skipped) = NativeContext::derive(&snapshot, &|_| Some(SlsSpaceType::User));
    assert!(skipped.is_empty(), "nothing skipped: {skipped:?}");
    let row: Vec<u64> = ctx.domains.iter().map(|d| d.space).collect();
    assert_eq!(row, vec![1, 277], "derive keeps SLS row order");
    ctx
}

fn repro_key(space: u64) -> DomainKey {
    DomainKey {
        output: OutputId(crate::topology::output_key(1)),
        workspace: tiler_core::directional::WorkspaceId(crate::topology::workspace_key(space)),
    }
}

fn repro_members(
    space: u64,
    pid: u32,
    wid: u32,
) -> BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> {
    BTreeMap::from([(
        repro_key(space),
        vec![(
            pid,
            ObservedWindow {
                wid,
                x: 8,
                y: 38,
                w: 2224,
                h: 1123,
                floating: false,
                fullscreen: false,
                hints: WindowSizeHints::none(),
            },
        )],
    )])
}

fn repro_focused(space: u64, pid: u32, wid: u32) -> (BTreeMap<DomainKey, WindowId>, String) {
    let id = format!("{pid}_{wid}");
    (
        BTreeMap::from([(repro_key(space), WindowId(id.clone()))]),
        id,
    )
}

#[allow(clippy::too_many_arguments)]
fn repro_report(
    ctx: &NativeContext,
    members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    sessions: &BTreeMap<DomainKey, Session>,
    focused: &BTreeMap<DomainKey, WindowId>,
    closed: &BTreeMap<DomainKey, Vec<WindowId>>,
    unreadable: &BTreeMap<DomainKey, HoldReason>,
    focused_global: &str,
    obs: u64,
    req: u64,
    req_id: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> tiler_core::world_report::WorldReport {
    let mut input = ReportInput::new(
        REPRO_OWNER,
        REPRO_INCARNATION,
        REPRO_GENERATION,
        ctx,
        obs,
        req,
        req_id,
        members,
        sessions,
        focused,
        closed,
        unreadable,
        &[],
        &[],
        focused_global,
    );
    input.command_seq = command_seq;
    input.command = command;
    build_report(&input)
}

#[allow(clippy::type_complexity)]
fn repro_empty_maps() -> (
    BTreeMap<DomainKey, Session>,
    BTreeMap<DomainKey, Vec<WindowId>>,
    BTreeMap<DomainKey, HoldReason>,
) {
    (BTreeMap::new(), BTreeMap::new(), BTreeMap::new())
}

/// Native Desktop-N resolves exactly once at admission: a runtime started on
/// a non-first Space still sends by native ordinal. A startup on 277 with
/// the subject there sends Desktop-1 to SID 1 (the retained inverse-refusal
/// shape), and a startup on 1 keeps the proven Desktop-2 path. Same-Space
/// and unknown targets keep their honest `send-target-unknown` refusals.
#[test]
fn explicit_send_resolves_native_ordinal_once_at_admission() {
    use crate::world_command_map::send_command;
    let output = OutputId(crate::topology::output_key(1));

    // Startup on 277: admission binds the native Desktop-1 SID, and the
    // production command carries it explicitly — never the ordinal.
    let ctx = repro_ctx(277);
    assert!(
        ctx.domains.iter().any(|d| d.space == 277 && d.current),
        "277 is the actuated current"
    );
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "Desktop-1 is native SID 1");
    let command = send_command(
        &send(1),
        "4242_7",
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("send route");
    let WorldCommand::Send {
        target_output,
        target_workspace,
        follow,
        ..
    } = command
    else {
        panic!("send-1 carries an explicit SID target, got {command:?}");
    };
    assert_eq!(target_output, output);
    assert_eq!(
        target_workspace,
        crate::topology::workspace_key(sid),
        "command target == claim.expected_sid workspace"
    );
    assert!(follow, "follow semantics unchanged");
    assert_eq!(
        feedback_kind_for(&send(1)),
        AttemptKind::Transfer,
        "opcode feedback unchanged"
    );

    // The admitted transfer retains under the native target, not ring position 1.
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(277, 4242, 7);
    let (focused, global) = repro_focused(277, 4242, 7);
    let (sessions, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        1,
        1,
        "req-1",
        None,
        None,
    );
    assert!(validate_report(&first), "producer report validates");
    assert!(matches!(
        engine.handle_report(&first),
        WorldOutcome::Accepted(_)
    ));
    let command = send_command(
        &send(1),
        "4242_7",
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(command),
    );
    assert!(validate_report(&second));
    assert!(
        matches!(engine.handle_report(&second), WorldOutcome::Accepted(_)),
        "Desktop-1 from a 277-started runtime admits"
    );
    let retained = engine.retained_transfers();
    assert_eq!(retained.len(), 1, "one retained window transfer");
    assert_eq!(retained[0].target_workspace, "space-1");

    // Startup-on-1 parity: Desktop-2 from a 1-started runtime still admits.
    let ctx = repro_ctx(1);
    let sid = context_target_sid(&ctx, &output, 2).expect("Desktop-2 binds");
    assert_eq!(sid, 277);
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(1, 4242, 8);
    let (focused, global) = repro_focused(1, 4242, 8);
    let (sessions, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        1,
        1,
        "req-1",
        None,
        None,
    );
    assert!(validate_report(&first));
    assert!(matches!(
        engine.handle_report(&first),
        WorldOutcome::Accepted(_)
    ));
    let command = send_command(
        &send(2),
        "4242_8",
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(command),
    );
    assert!(validate_report(&second));
    assert!(
        matches!(engine.handle_report(&second), WorldOutcome::Accepted(_)),
        "Desktop-2 from a 1-started runtime admits"
    );
    let retained = engine.retained_transfers();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].target_workspace, "space-277");

    // Same-Space target keeps its honest refusal (no behavior change).
    let ctx = repro_ctx(277);
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(277, 4242, 7);
    let (focused, global) = repro_focused(277, 4242, 7);
    let (sessions, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        1,
        1,
        "req-1",
        None,
        None,
    );
    assert!(matches!(
        engine.handle_report(&first),
        WorldOutcome::Accepted(_)
    ));
    let command = send_command(
        &send(1),
        "4242_7",
        output.clone(),
        &crate::topology::workspace_key(277),
    )
    .expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(command),
    );
    assert!(matches!(
        engine.handle_report(&second),
        WorldOutcome::Rejected {
            kind: "send-target-unknown",
            ..
        }
    ));

    // Unknown native target keeps its honest refusal.
    let command = WorldCommand::Send {
        window: WindowId("4242_7".to_owned()),
        target_output: output,
        target_workspace: "space-999".to_owned(),
        follow: true,
    };
    let third = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        3,
        2,
        "req-2b",
        Some(2),
        Some(command),
    );
    assert!(matches!(
        engine.handle_report(&third),
        WorldOutcome::Rejected {
            kind: "send-target-unknown",
            ..
        }
    ));
}

/// Semantic contrast for the 277-startup repair: the legacy positional
/// `SendNumbered{index:1}` self-targets through the stable-logical scoped
/// ring (`[space-277, space-1]`) and refuses, while the fixed explicit
/// `Send{target_workspace: space-1}` admission-resolved against the native
/// row (`[1, 277]`) admits — under identical native context, members,
/// focus, and report shape. Both second reports validate, so the red is a
/// semantic `send-target-unknown` refusal, never an API/compile oracle.
#[test]
fn legacy_numbered_send_self_targets_under_277_startup() {
    use crate::world_command_map::send_command;
    use tiler_core::world_report::TransferKind;
    let ctx = repro_ctx(277);
    let output = OutputId(crate::topology::output_key(1));
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "native Desktop-1 is SID 1 in this context");
    let members = repro_members(277, 4242, 7);
    let (focused, global) = repro_focused(277, 4242, 7);
    let (sessions, closed, unreadable) = repro_empty_maps();

    // Legacy positional path: SendNumbered index 1 resolves through the
    // scoped send ring whose stable-logical order is [277, 1] here, so it
    // names the source itself and refuses same-space.
    let mut legacy = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let first = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        1,
        1,
        "req-1",
        None,
        None,
    );
    assert!(validate_report(&first));
    assert!(matches!(
        legacy.handle_report(&first),
        WorldOutcome::Accepted(_)
    ));
    let legacy_command = WorldCommand::SendNumbered {
        window: WindowId("4242_7".to_owned()),
        output: output.clone(),
        index: 1,
        follow: true,
    };
    let legacy_second = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(legacy_command),
    );
    assert!(
        validate_report(&legacy_second),
        "legacy report is well-formed wire input"
    );
    assert!(
        matches!(
            legacy.handle_report(&legacy_second),
            WorldOutcome::Rejected {
                kind: "send-target-unknown",
                ..
            }
        ),
        "legacy SendNumbered-1 from a 277-started runtime self-targets and refuses"
    );
    assert!(
        legacy.retained_transfers().is_empty(),
        "refused legacy send retains nothing"
    );

    // Fixed admission-resolved path: identical context/report shape, but the
    // command carries the native SID directly and admits with stable
    // follow + operation identity.
    let mut fixed = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let first = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        1,
        1,
        "req-1",
        None,
        None,
    );
    assert!(validate_report(&first));
    assert!(matches!(
        fixed.handle_report(&first),
        WorldOutcome::Accepted(_)
    ));
    let fixed_command = send_command(
        &send(1),
        "4242_7",
        output.clone(),
        &crate::topology::workspace_key(sid),
    )
    .expect("send route");
    let fixed_second = repro_report(
        &ctx,
        &members,
        &sessions,
        &focused,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(fixed_command),
    );
    assert!(
        validate_report(&fixed_second),
        "fixed report is well-formed wire input"
    );
    assert!(
        matches!(
            fixed.handle_report(&fixed_second),
            WorldOutcome::Accepted(_)
        ),
        "explicit Send to space-1 from a 277-started runtime admits"
    );
    let retained = fixed.retained_transfers();
    assert_eq!(retained.len(), 1, "one retained window transfer");
    assert_eq!(
        retained[0].kind,
        TransferKind::Window(WindowId("4242_7".to_owned()))
    );
    assert_eq!(retained[0].target_output, output);
    assert_eq!(
        retained[0].target_workspace,
        crate::topology::workspace_key(sid)
    );
    assert_eq!(retained[0].target_workspace, "space-1");
    assert!(retained[0].follow, "follow semantics unchanged");
    assert_eq!(
        retained[0].operation_id, "op-2",
        "stable operation identity unchanged"
    );
}
