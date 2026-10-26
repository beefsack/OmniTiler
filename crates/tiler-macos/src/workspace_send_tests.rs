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
use tiler_core::session::DomainKey;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{FrameSetter, HoldReason, WorldCommand, validate_report};

use crate::ffi::{SlsDisplayRow, SlsSpaceType};
use crate::runtime_core::LifetimeMemory;
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

/// Product send capability: the 9 numbered send-follow shapes
/// 1..9 from the armed product `bindings()` (catalogue
/// `implemented: true` plus `is_armed_send`; stays armed engine-side but
/// bindable unbound, so no chord here).
fn send_shape_bindings() -> Vec<TileBinding> {
    bindings()
}

#[test]
fn send_follow_shape_armed_in_product() {
    // Product arming: the catalogue marks follows 0..9 plus stays 1..9
    // `implemented: true`, `bindings()` arms the 10 Ctrl+Shift+digit
    // follow chords (stays stay bindable unbound), and the engine admits
    // both shapes (unsupported shapes still refuse - see below).
    let armed = send_shape_bindings();
    assert_eq!(armed.len(), 10, "product arms 10 send shapes");
    assert_eq!(armed.len(), bindings().len());
    let mut seen = Vec::new();
    for binding in &armed {
        let InputSemanticAction::WorkspaceSend { index, follow } = binding.action else {
            panic!("send shape carries a non-send: {:?}", binding.action);
        };
        assert!(follow, "seam sends always follow");
        assert!(
            (0..=9).contains(&index),
            "seam sends are 0..=9, got {index}"
        );
        assert!(binding.shift && !binding.alt && !binding.aux, "{index}");
        seen.push(index);
    }
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    // Catalogue reality: follows 0..9 plus stays 1..9 implemented;
    // trailing stay-0 stays unimplemented.
    for row in crate::workspace_input::workspace_catalogue() {
        match &row.action {
            InputSemanticAction::WorkspaceSend { index, follow } if *follow => {
                assert!(row.implemented, "{} must be implemented:true", row.id);
                let _ = index;
            }
            InputSemanticAction::WorkspaceSend { index, follow }
                if !follow && (1..=9).contains(index) =>
            {
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
    // Trailing stay-0, relative sends/stays, and outputs stay refused
    // engine-side (stays 1..9 are armed engine-side but bindable unbound,
    // so they never enter the default tap chords either).
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
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
    // Trailing send-0 follow is armed in product through the capability
    // gate (dispatches, no refusal).
    {
        let action = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        };
        assert!(crate::workspace_input::is_armed_send(&action), "{action:?}");
        assert_eq!(
            crate::workspace_input::workspace_unsupported_reason(&action),
            None,
            "{action:?} dispatches in product"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&action),
            None,
            "{action:?}"
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
        let stay = InputSemanticAction::WorkspaceSend {
            index,
            follow: false,
        };
        assert!(crate::workspace_input::is_armed_send(&stay), "stay {index}");
        assert_eq!(
            crate::workspace_input::workspace_unsupported_reason(&stay),
            None,
            "stay {index} dispatches in product"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&stay),
            None,
            "stay {index}"
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
    assert_eq!(armed.len(), 10);
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
    assert_eq!(specs.len(), 10);
    assert!(
        specs.iter().all(|s| !s.select),
        "send shapes ride the basic eligibility cache (focus-gated), never the select cache"
    );
}

#[test]
fn send_command_maps_send_shapes_to_positional_numbered_sends() {
    // Product send mapping: numbered sends ride positional
    // `SendNumbered` for the admission-resolved explicit source — the
    // ordinal resolves core-side against the adopted ring (native Desktop
    // order), and the admission pins the native SID drain-side for
    // revalidation only.
    use crate::world_command_map::{OP_SEND, select_command, send_command};
    use tiler_core::world_report::SendSource;
    assert_ne!(OP_SEND, crate::world_command_map::OP_SELECT);
    let output = OutputId("macos-display-1".to_owned());
    for index in 1..=9u8 {
        let command = send_command(&send(index), "4242_7", output.clone())
            .unwrap_or_else(|| panic!("send {index} has a world route"));
        assert_eq!(
            command,
            WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::Explicit(output.clone()),
                index,
                follow: true,
            }
        );
        // Sends never ride the select route.
        assert_eq!(select_command(&send(index), Some(output.clone())), None);
    }
    // Unsupported actions have no send route. Numbered stays 1..9 map
    // with follow:false preserved (armed engine-side, bindable unbound);
    // trailing stay-0, relative
    // sends, selects, and basics have no route. Trailing send-0 follow
    // maps through the capability gate (armed in product).
    for index in 1..=9u8 {
        let action = InputSemanticAction::WorkspaceSend {
            index,
            follow: false,
        };
        assert_eq!(
            send_command(&action, "4242_7", output.clone()),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::Explicit(output.clone()),
                index,
                follow: false,
            }),
            "{action:?} maps with follow:false"
        );
    }
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
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
            send_command(&action, "4242_7", output.clone()),
            None,
            "{action:?}"
        );
    }
    // Trailing send-0 follow maps in product (capability armed) and
    // refuses capability-disabled; stay-0 never maps.
    assert_eq!(
        send_command(
            &InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            },
            "4242_7",
            output.clone(),
        ),
        Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: SendSource::Explicit(output.clone()),
            index: 0,
            follow: true,
        })
    );
    assert_eq!(
        crate::world_command_map::send_command_with_capability(
            &InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            },
            "4242_7",
            output.clone(),
            false
        ),
        None,
        "capability-disabled send-0 has no route"
    );
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
fn trailing_send_target_reuses_empty_appends_when_occupied_refuses_unknown() {
    use crate::workspace_send::{TrailingSendTarget, trailing_send_target};
    // Producer-shaped `[1, 277]`: proven empty reuses last (even current).
    assert_eq!(
        trailing_send_target(&[1, 277], Ok(true)),
        Ok(TrailingSendTarget::Reuse(277))
    );
    // Proven occupied: append required (caller creates exactly once).
    assert_eq!(
        trailing_send_target(&[1, 277], Ok(false)),
        Ok(TrailingSendTarget::NeedAppend)
    );
    // Unknown (zero-area/foreign-chrome ambiguity): refuses, never guesses.
    assert!(trailing_send_target(&[1, 277], Err("unreadable".to_owned())).is_err());
    // Unprovable rows refuse; the shared bound gates appends only.
    assert!(trailing_send_target(&[], Ok(true)).is_err());
    assert!(trailing_send_target(&[1, 0], Ok(true)).is_err());
    let full: Vec<u64> = (1..=32).collect();
    assert!(trailing_send_target(&full, Ok(false)).is_err());
    // Regression: reuse at MAX appends nothing, so the bound cannot block
    // it (the ordinal guard lives in preparation, not here).
    assert_eq!(
        trailing_send_target(&full, Ok(true)),
        Ok(TrailingSendTarget::Reuse(32))
    );
}

#[test]
fn trailing_send_prepare_reuse_append_and_ordinal_guards() {
    use crate::workspace_select::{SelectionBackend, SelectionSnapshot};
    use crate::workspace_send::{TrailingSendPrep, prepare_trailing_send_target};
    struct PrepFake {
        row: SelectionSnapshot,
        revision: Option<u64>,
        empty: Result<bool, String>,
        creates: usize,
        drift: bool,
        no_effect: bool,
    }
    impl SelectionBackend for PrepFake {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            Ok(self.row.clone())
        }
        fn revision(&mut self) -> Option<u64> {
            self.revision
        }
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            if output == "macos-display-1" {
                Ok(())
            } else {
                Err(format!(
                    "product select refuses: reply target output {output} is not the live display macos-display-1; no actuation"
                ))
            }
        }
        fn empty(&mut self, _: u64) -> Result<bool, String> {
            self.empty.clone()
        }
        fn create(
            &mut self,
            _: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.creates += 1;
            if !self.no_effect {
                let next = self.row.spaces.iter().max().copied().unwrap_or(0) + 1;
                self.row.spaces.push(next);
            }
            if self.drift {
                self.row.spaces.swap(0, 1);
            }
            Ok(())
        }
        fn select(
            &mut self,
            _: &SelectionSnapshot,
            _: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("send preparation never selects");
        }
    }
    fn fake(spaces: Vec<u64>, empty: Result<bool, String>) -> PrepFake {
        PrepFake {
            row: SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces,
                current: 1,
            },
            revision: Some(9),
            empty,
            creates: 0,
            drift: false,
            no_effect: false,
        }
    }
    // Unarmed refuses before any native effect.
    let mut unarmed = fake(vec![1, 277], Ok(false));
    assert!(prepare_trailing_send_target(&mut unarmed, false).is_err());
    assert_eq!(unarmed.creates, 0);
    // Proven-empty last reuses with no append under the post-prep revision.
    let mut reuse = fake(vec![1, 277], Ok(true));
    assert_eq!(
        prepare_trailing_send_target(&mut reuse, true),
        Ok(TrailingSendPrep {
            target_sid: 277,
            created: None,
            revision: 9,
        })
    );
    assert_eq!(reuse.creates, 0);
    // Proven-occupied appends exactly once and adopts the exact append.
    let mut append = fake(vec![1, 277], Ok(false));
    assert_eq!(
        prepare_trailing_send_target(&mut append, true),
        Ok(TrailingSendPrep {
            target_sid: 278,
            created: Some(278),
            revision: 9,
        })
    );
    assert_eq!(append.creates, 1);
    // Unknown emptiness (zero-area/foreign-chrome ambiguity) refuses
    // without an append.
    let mut unknown = fake(vec![1, 277], Err("unreadable".to_owned()));
    assert!(prepare_trailing_send_target(&mut unknown, true).is_err());
    assert_eq!(unknown.creates, 0);
    // Reuse past Desktop-9 refuses (existing owner ordinal refusal, raised
    // before any post, never a relative route).
    let mut past_nine = fake((1..=10).collect(), Ok(true));
    assert!(prepare_trailing_send_target(&mut past_nine, true).is_err());
    assert_eq!(past_nine.creates, 0);
    // An append that would land past Desktop-9 refuses BEFORE creating an
    // unmappable Space.
    let mut append_past_nine = fake((1..=9).collect(), Ok(false));
    assert!(prepare_trailing_send_target(&mut append_past_nine, true).is_err());
    assert_eq!(append_past_nine.creates, 0);
    // Append at the shared bound refuses without an append.
    let mut at_max = fake((1..=32).collect(), Ok(false));
    assert!(prepare_trailing_send_target(&mut at_max, true).is_err());
    assert_eq!(at_max.creates, 0);
    // Post-append drift refuses adoption (the single append already ran
    // exactly once — never replayed).
    let mut drifted = fake(vec![1, 277], Ok(false));
    drifted.drift = true;
    assert!(prepare_trailing_send_target(&mut drifted, true).is_err());
    assert_eq!(drifted.creates, 1);
    // Unprovable post-preparation revision refuses (the owner fences a
    // revision, never a missing one).
    let mut no_revision = fake(vec![1, 277], Ok(true));
    no_revision.revision = None;
    assert!(prepare_trailing_send_target(&mut no_revision, true).is_err());
}

#[test]
fn trailing_send_bind_pins_original_window_and_exact_created() {
    use crate::workspace_send::{TrailingSendPrep, bind_trailing_send_target};
    use tiler_core::session::DomainKey;
    fn trailing_claim() -> ClaimedSelect {
        let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        });
        claim.send_subject = Some("4242_7".to_owned());
        claim.send_generation = Some(9);
        claim.send_source = Some(DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-1".to_owned()),
        });
        claim.operation_id = Some("op-2".to_owned());
        claim.topology_revision = Some(4);
        claim
    }
    // Reuse binds the prepared SID under the post-preparation revision.
    let reuse = bind_trailing_send_target(
        &trailing_claim(),
        TrailingSendPrep {
            target_sid: 277,
            created: None,
            revision: 4,
        },
    )
    .expect("trailing reuse binds");
    assert_eq!(reuse.target_sid(), 277);
    assert_eq!(reuse.revision(), 4);
    assert!(reuse.follow());
    assert_eq!(reuse.window(), "4242_7");
    // Append binds exactly the created SID.
    let append = bind_trailing_send_target(
        &trailing_claim(),
        TrailingSendPrep {
            target_sid: 278,
            created: Some(278),
            revision: 5,
        },
    )
    .expect("trailing append binds");
    assert_eq!(append.target_sid(), 278);
    // A prepared target naming anything but the exact-created SID refuses
    // (created-target drift never substitutes).
    assert!(
        bind_trailing_send_target(
            &trailing_claim(),
            TrailingSendPrep {
                target_sid: 279,
                created: Some(278),
                revision: 5,
            },
        )
        .is_err()
    );
    assert!(
        bind_trailing_send_target(
            &trailing_claim(),
            TrailingSendPrep {
                target_sid: 0,
                created: None,
                revision: 5,
            },
        )
        .is_err()
    );
    // Stay-0, numbered sends, and unpinned claims never bind here (the
    // numbered lane keeps its own `from_claim` with admission SIDs).
    let mut stay = trailing_claim();
    stay.action = InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: false,
    };
    assert!(
        bind_trailing_send_target(
            &stay,
            TrailingSendPrep {
                target_sid: 277,
                created: None,
                revision: 4,
            },
        )
        .is_err()
    );
    let mut numbered = trailing_claim();
    numbered.action = InputSemanticAction::WorkspaceSend {
        index: 1,
        follow: true,
    };
    assert!(
        bind_trailing_send_target(
            &numbered,
            TrailingSendPrep {
                target_sid: 277,
                created: None,
                revision: 4,
            },
        )
        .is_err()
    );
    let mut unpinned = trailing_claim();
    unpinned.send_generation = None;
    assert!(
        bind_trailing_send_target(
            &unpinned,
            TrailingSendPrep {
                target_sid: 277,
                created: None,
                revision: 4,
            },
        )
        .is_err(),
        "relaunched lifetime without a generation pin never binds"
    );
    // The numbered `from_claim` lane still refuses trailing claims (no
    // admission SID was ever pinned for index 0).
    assert!(
        crate::workspace_send_effect::ManagedSendTarget::from_claim(&trailing_claim()).is_err()
    );
}

#[test]
fn trailing_send_command_and_admission_armed_in_production() {
    use crate::runtime_thread::{admit_send_claim, admit_send_claim_with};
    use crate::runtime_types::ClaimedSelect;
    use crate::world_command_map::{send_command, send_command_with_capability};
    use tiler_core::directional::OutputId;
    let output = OutputId("macos-display-1".to_owned());
    let trailing = InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: true,
    };
    // Production (gate `true`): trailing follow maps to the core trailing
    // path with the pinned subject, and the engine admits the shape.
    assert_eq!(
        send_command(&trailing, "4242_7", output.clone()),
        Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(output.clone()),
            index: 0,
            follow: true,
        })
    );
    assert!(crate::workspace_input::is_armed_send(&trailing));
    assert!(admit_send_claim(true, false, false, ClaimedSelect::tap(trailing.clone())).is_ok());
    // Capability-disabled pin through the same production mapping: no
    // send route, engine refuses.
    assert_eq!(
        send_command_with_capability(&trailing, "4242_7", output.clone(), false),
        None
    );
    assert!(
        admit_send_claim_with(
            true,
            false,
            false,
            ClaimedSelect::tap(trailing.clone()),
            false
        )
        .is_err()
    );
    // Explicit local capability through the same production mapping:
    // trailing follow maps to the core trailing path with the pinned
    // subject; stay-0 never maps.
    let command = send_command_with_capability(&trailing, "4242_7", output.clone(), true)
        .expect("trailing follow has a core route");
    assert_eq!(
        command,
        WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(output.clone()),
            index: 0,
            follow: true,
        }
    );
    let stay0 = InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: false,
    };
    assert_eq!(
        send_command_with_capability(&stay0, "4242_7", output.clone(), true),
        None
    );
    // Explicit local capability admits the trailing shape at the engine gate
    // (stay-0 still refuses: out of scope).
    let admitted = admit_send_claim_with(
        true,
        false,
        false,
        ClaimedSelect::tap(trailing.clone()),
        true,
    );
    assert!(admitted.is_ok(), "capability-armed trailing admits");
    assert!(admit_send_claim_with(true, false, false, ClaimedSelect::tap(stay0), true).is_err());
    assert!(
        admit_send_claim(true, false, false, ClaimedSelect::tap(trailing)).is_ok(),
        "production wrapper admits when armed"
    );
}

#[test]
fn trailing_send_target_pin_release_history_semantics() {
    // Target pin: the managed-send target binds the trailing SID plus the
    // admission pins (subject/generation/source/revision/operation); a
    // claim without admission pins refuses instead of actuating.
    use crate::runtime_types::ClaimedSelect;
    use crate::workspace_send::{
        SendDrainOutcome, is_send_action, setter_for, should_note_history,
    };
    use crate::workspace_send_effect::{ManagedSendTarget, SendEffectOutcome};
    use tiler_core::session::DomainKey;
    use tiler_core::world_report::FrameSetter;
    let source = DomainKey {
        output: tiler_core::directional::OutputId("out-1".to_owned()),
        workspace: tiler_core::directional::WorkspaceId("space-1".to_owned()),
    };
    let target =
        ManagedSendTarget::bind(4242, 7, 9, source, 277, 4, "op-2").expect("trailing reuse binds");
    assert_eq!(target.target_sid(), 277);
    assert!(target.follow());
    // Release-first/no-replay + history semantics ride the shared outcome
    // mapping: Applied settles transfer and notes history on follows only;
    // proven refusal settles observed; ambiguous holds without replay.
    assert_eq!(setter_for(SendDrainOutcome::Applied), FrameSetter::Accepted);
    assert_eq!(setter_for(SendDrainOutcome::Refused), FrameSetter::Refused);
    assert_eq!(
        setter_for(SendDrainOutcome::PossiblyExecuted),
        FrameSetter::Threw
    );
    let follow = InputSemanticAction::WorkspaceSend {
        index: 1,
        follow: true,
    };
    assert!(is_send_action(&follow));
    assert!(should_note_history(SendDrainOutcome::Applied, true));
    assert!(!should_note_history(SendDrainOutcome::Applied, false));
    assert!(!should_note_history(SendDrainOutcome::Refused, true));
    assert!(!should_note_history(
        SendDrainOutcome::PossiblyExecuted,
        true
    ));
    // Unpinned claim refuses instead of grabbing a different window.
    let bare = ClaimedSelect::tap(follow);
    assert!(ManagedSendTarget::from_claim(&bare).is_err());
    let _ = (target, SendEffectOutcome::Applied);
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
    let good_stay = ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
        index: 2,
        follow: false,
    });
    assert!(admit_send_claim(true, false, false, good_stay).is_ok());
    // Trailing send-0 follow is armed in production (admits with no
    // admission SID: the drain ensures the Space natively).
    assert!(
        admit_send_claim(
            true,
            false,
            false,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            })
        )
        .is_ok()
    );
    // Unsupported (trailing stay-0, relative, select, basic)
    // refuses.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
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
    // discard/feedback lane branch).
    use crate::workspace_send::is_send_action;
    assert!(is_send_action(&send(2)));
    assert!(!is_send_action(&InputSemanticAction::WorkspaceSelect {
        index: 2
    }));
    assert!(!is_send_action(&InputSemanticAction::FocusStep {
        direction: crate::runtime_types::InputDirection::Left
    }));
    // Outcome mapping never claims false success: only verified arrival is
    // Applied; pre-actuation is proven refusal; everything else holds.
    assert_eq!(setter_for(SendDrainOutcome::Applied), FrameSetter::Accepted);
    assert_eq!(setter_for(SendDrainOutcome::Refused), FrameSetter::Refused);
    assert_eq!(
        setter_for(SendDrainOutcome::PossiblyExecuted),
        FrameSetter::Threw
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
    // Product tap table: 34 basic + 19 armed selects + 10 armed numbered
    // send-follows (63). Duplicate-free under every usable Meta/preset;
    // Compatible under Ctrl leaves the 10 numbered selects plus the 4
    // unshifted arrow basics to macOS (49 = 30 basic + 9 aux selects + 10
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
    assert_eq!(crate::workspace_select::bindings().len(), 19);
    assert_eq!(
        bindings().len(),
        10,
        "product arms 10 numbered send-follows"
    );
    assert_eq!(product().len(), 63);
    assert_eq!(crate::tile::product_catalogue().len(), 63);
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let authentic = effective_bindings(InputPreset::Authentic, modifier, &product());
        assert_eq!(authentic.len(), 63, "{modifier:?}");
        let config = crate::runtime_types::RuntimeConfig {
            modifier,
            tile_bindings: Some(authentic),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_ok(), "{modifier:?}");
    }
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &product());
    assert_eq!(compatible.len(), 49, "30 basic + 9 aux selects + 10 sends");
    assert_eq!(
        compatible
            .iter()
            .filter(|b| crate::workspace_input::is_armed_send(&b.action))
            .count(),
        10,
        "numbered send-follow enters the product table (shifted digits kept)"
    );
    // Unsupported sends (trailing stay, stays, relative, outputs) never enter.
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(
                b.action,
                InputSemanticAction::WorkspaceSend {
                    index: 0,
                    follow: false,
                } | InputSemanticAction::WorkspaceSend { follow: false, .. }
                    | InputSemanticAction::WorkspaceSendRelative { .. }
                    | InputSemanticAction::WorkspaceOutput { .. }
            ))
            .count(),
        0,
        "unsupported send shapes never enter the product table"
    );
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(
                b.action,
                InputSemanticAction::WorkspaceSend {
                    index: 0,
                    follow: true
                }
            ))
            .count(),
        1,
        "armed trailing send-0 follow rides the Compatible table (shifted digit kept)"
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
    // the world maps it to a positional `SendNumbered` transfer for the
    // explicit source, and the feedback lane settles transfers (never
    // views). Unsupported shapes still refuse admission and engine
    // dispatch.
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
    let command = send_command(&binding.action, "4242_7", output.clone()).expect("world route");
    assert_eq!(
        command,
        tiler_core::world_report::WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(output),
            index: 2,
            follow: true,
        }
    );
    assert!(is_send_action(&binding.action));
    assert_eq!(setter_for(SendDrainOutcome::Applied), FrameSetter::Accepted);
    // Numbered stays map with follow:false preserved and now dispatch:
    // armed engine-side (bindable unbound, no default chord).
    let stay = InputSemanticAction::WorkspaceSend {
        index: 2,
        follow: false,
    };
    let stay_output = OutputId("macos-display-1".to_owned());
    assert_eq!(
        send_command(&stay, "4242_7", stay_output.clone()),
        Some(tiler_core::world_report::WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(stay_output),
            index: 2,
            follow: false,
        }),
        "stay maps with follow:false"
    );
    assert!(is_send_action(&stay));
    assert_eq!(
        workspace_unsupported_outcome(&stay),
        None,
        "stay-2 dispatches (armed, bindable unbound)"
    );
    assert!(
        admit_send_claim(true, false, false, ClaimedSelect::tap(stay.clone())).is_ok(),
        "stay-2 admitted"
    );
    // Trailing send-0 follow dispatches in product (armed through the
    // capability gate): no refusal outcome, admitted, world route to the
    // core trailing path.
    let trailing = InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: true,
    };
    assert_eq!(workspace_unsupported_outcome(&trailing), None);
    assert!(
        admit_send_claim(true, false, false, ClaimedSelect::tap(trailing.clone())).is_ok(),
        "send-0 admitted"
    );
    assert_eq!(
        send_command(&trailing, "4242_7", OutputId("macos-display-1".to_owned()),),
        Some(tiler_core::world_report::WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId("4242_7".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(OutputId(
                "macos-display-1".to_owned()
            )),
            index: 0,
            follow: true,
        }),
        "send-0 maps to the trailing core route"
    );
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
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
            send_command(&action, "4242_7", OutputId("macos-display-1".to_owned()),),
            None,
            "{action:?}"
        );
    }
}

#[test]
fn stay_shape_stays_unbound_but_armed_in_product() {
    // Numbered send-and-stay 1..9: recognized and dispatched - bindable
    // rows with no default chord (never a default tap chord), armed
    // engine-side and admitted. Trailing stay-0 stays refused (no world
    // route). Relative send-and-stay rows stay bindable-unbound and
    // unarmed.
    use crate::runtime_thread::admit_send_claim;
    use crate::world_command_map::send_command;
    let output = OutputId("macos-display-1".to_owned());
    for index in [1, 2, 3, 4, 5, 6, 7, 8, 9] {
        let action = InputSemanticAction::WorkspaceSend {
            index,
            follow: false,
        };
        assert!(
            crate::workspace_input::is_armed_send(&action),
            "stay {index} armed"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_reason(&action),
            None,
            "stay {index} dispatches"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&action),
            None,
            "stay {index}"
        );
        assert!(
            send_command(&action, "4242_7", output.clone()).is_some(),
            "stay {index} maps"
        );
        assert!(
            admit_send_claim(true, false, false, ClaimedSelect::tap(action.clone())).is_ok(),
            "stay {index} admitted"
        );
        // Catalogue: bindable, unbound, armed - with the digit slot kept
        // for a rebind inject.
        let row = crate::workspace_input::workspace_catalogue()
            .into_iter()
            .find(|row| row.action == action)
            .unwrap_or_else(|| panic!("stay {index} has a catalogue row"));
        assert!(row.implemented, "{} implemented:true", row.id);
        assert_eq!(row.default, None, "{} stays bindable unbound", row.id);
        let want_key = desktop_digit_keycode(u32::from(index)).unwrap();
        assert_eq!(
            row.unbound_key,
            Some(want_key),
            "{} keeps its digit slot",
            row.id
        );
    }
    {
        let action = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        };
        assert!(!crate::workspace_input::is_armed_send(&action));
        assert!(crate::workspace_input::workspace_unsupported_reason(&action).is_some());
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&action),
            Some("refused-workspace-send-unsupported")
        );
        assert_eq!(
            send_command(&action, "4242_7", output.clone()),
            None,
            "stay-0 has no world route"
        );
        assert_eq!(
            admit_send_claim(true, false, false, ClaimedSelect::tap(action.clone())),
            Err("refused-send-unarmed"),
            "stay-0 refused at admission"
        );
        let row = crate::workspace_input::workspace_catalogue()
            .into_iter()
            .find(|row| row.action == action)
            .expect("stay-0 has a catalogue row");
        assert!(!row.implemented, "{} stays implemented:false", row.id);
        assert_eq!(row.default, None, "{} stays bindable unbound", row.id);
        assert_eq!(
            row.unbound_key,
            Some(desktop_digit_keycode(10).unwrap()),
            "{} keeps its digit slot",
            row.id
        );
    }
    // Relative send-and-stay rows stay bindable-unbound too.
    for row in crate::workspace_input::workspace_catalogue() {
        if matches!(
            row.action,
            InputSemanticAction::WorkspaceSendRelative { follow: false, .. }
        ) {
            assert!(!row.implemented, "{} stays implemented:false", row.id);
            assert_eq!(row.default, None, "{} stays bindable unbound", row.id);
        }
    }
    // Recognized (parses) but undispatched: no stay chord enters the armed
    // product bindings.
    assert_eq!(
        crate::workspace_input::parse_workspace_action("stay-workspace-3"),
        Some(InputSemanticAction::WorkspaceSend {
            index: 3,
            follow: false
        })
    );
    assert!(
        bindings().iter().all(|b| !matches!(
            b.action,
            InputSemanticAction::WorkspaceSend { follow: false, .. }
        )),
        "no stay chord armed in product"
    );
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
        format_send_admitted_line("send-2", "4242_7", 277, 1, "main", "op-9", Some(77), 9100),
        "tiler-macos send admitted action=send-2 subject=4242_7 target=277 from=1 display=main op=op-9 route=unselected phase=admission revision=Some(77) now_ms=9100 submitted complete=false",
    );
    // Unknown revision renders explicit (never guessed).
    assert_eq!(
        format_send_admitted_line("send-2", "4242_7", 277, 1, "main", "op-9", None, 9100),
        "tiler-macos send admitted action=send-2 subject=4242_7 target=277 from=1 display=main op=op-9 route=unselected phase=admission revision=None now_ms=9100 submitted complete=false",
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

fn repro_global(_space: u64, pid: u32, wid: u32) -> String {
    format!("{pid}_{wid}")
}

#[allow(clippy::too_many_arguments)]
fn repro_report(
    ctx: &NativeContext,
    members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
    memory: &LifetimeMemory,
    closed: &[WindowId],
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
        memory,
        closed,
        unreadable,
        &[],
        focused_global,
        false,
    );
    input.command_seq = command_seq;
    input.command = command;
    build_report(&mut input)
}

fn repro_empty_maps() -> (
    LifetimeMemory,
    Vec<WindowId>,
    BTreeMap<DomainKey, HoldReason>,
) {
    (LifetimeMemory::empty(), Vec::new(), BTreeMap::new())
}

/// Native Desktop-N resolves positionally core-side under adoption: a
/// runtime started on a non-first Space still sends by native ordinal. A
/// startup on 277 with the subject there sends Desktop-1 (positional index
/// 1 over the adopted native-order ring), and a startup on 1 keeps the
/// proven Desktop-2 path. Same-Space targets refuse as honest `unchanged`
/// no-ops (shared-core `RefusalKind::Unchanged`); unknown logical targets
/// keep their honest `send-target-unknown` refusals.
#[test]
fn explicit_send_resolves_native_ordinal_once_at_admission() {
    use crate::world_command_map::send_command;
    let output = OutputId(crate::topology::output_key(1));

    // Startup on 277: admission pins the native Desktop-1 SID for drain
    // revalidation, and the production command carries the positional
    // index — the core resolves it against the adopted ring in the same
    // report the adoption converges.
    let ctx = repro_ctx(277);
    assert!(
        ctx.domains.iter().any(|d| d.space == 277 && d.current),
        "277 is the actuated current"
    );
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "Desktop-1 is native SID 1");
    let command = send_command(&send(1), "4242_7", output.clone()).expect("send route");
    let WorldCommand::SendNumbered {
        window,
        source,
        index,
        follow,
    } = command
    else {
        panic!("send-1 carries a positional index, got {command:?}");
    };
    assert_eq!(window, WindowId("4242_7".to_owned()));
    assert!(
        matches!(source, tiler_core::world_report::SendSource::Explicit(_)),
        "admission-resolved source passes through unchanged"
    );
    assert_eq!(index, 1);
    assert!(follow, "follow semantics unchanged");
    assert!(is_send_action(&send(1)), "opcode feedback unchanged");

    // The admitted transfer retains under the adopted logical for native
    // Desktop-1 (ring position 1 in native order), never a guessed id.
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(277, 4242, 7);
    let global = repro_global(277, 4242, 7);
    let (memory, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    let command = send_command(&send(1), "4242_7", output.clone()).expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &memory,
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
    let windowed: Vec<_> = retained
        .iter()
        .filter(|target| {
            matches!(
                target.kind,
                tiler_core::world_report::TransferKind::Window(_)
            )
        })
        .collect();
    assert_eq!(windowed.len(), 1, "one retained window transfer");
    assert_eq!(windowed[0].target_workspace, "ws-1");

    // Startup-on-1 parity: Desktop-2 from a 1-started runtime still admits.
    let ctx = repro_ctx(1);
    let sid = context_target_sid(&ctx, &output, 2).expect("Desktop-2 binds");
    assert_eq!(sid, 277);
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(1, 4242, 8);
    let global = repro_global(1, 4242, 8);
    let (memory, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    let command = send_command(&send(2), "4242_8", output.clone()).expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &memory,
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
    let windowed: Vec<_> = retained
        .iter()
        .filter(|target| {
            matches!(
                target.kind,
                tiler_core::world_report::TransferKind::Window(_)
            )
        })
        .collect();
    assert_eq!(windowed.len(), 1);
    assert_eq!(windowed[0].target_workspace, "ws-2");

    // Same-Space target is an honest `unchanged` no-op (shared-core
    // `RefusalKind::Unchanged`): the current ordinal resolves to the
    // source home itself. No transfer, no native effect.
    let ctx = repro_ctx(277);
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let members = repro_members(277, 4242, 7);
    let global = repro_global(277, 4242, 7);
    let (memory, closed, unreadable) = repro_empty_maps();
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    let command = send_command(&send(2), "4242_7", output.clone()).expect("send route");
    let second = repro_report(
        &ctx,
        &members,
        &memory,
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
            kind: "unchanged",
            ..
        }
    ));

    // Unknown logical target keeps its honest refusal.
    let command = WorldCommand::Send {
        window: WindowId("4242_7".to_owned()),
        target_output: output,
        target_workspace: "ws-999".to_owned(),
        follow: true,
    };
    let third = repro_report(
        &ctx,
        &members,
        &memory,
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

/// Same-current numbered send is an honest unchanged no-op aligned with the
/// shared core: the producer ordinal that resolves to the current home
/// (Desktop-2 while started on 277) dispatches the positional
/// `SendNumbered` against the source home itself, and the world refuses it
/// as `unchanged` - the
/// [`tiler_core::session::RefusalKind::Unchanged`] verdict the core returns
/// for same-domain sends - retaining no transfer, binding no claim (so the
/// drain never submits to the effect owner and posts nothing native), and
/// repeating identically without replay. Stale/unmanaged fences stay
/// intact: an unknown subject still refuses `send-window-unknown`.
#[test]
fn same_current_numbered_send_is_honest_unchanged_noop() {
    use tiler_core::session::RefusalKind;
    let ctx = repro_ctx(277);
    let output = OutputId(crate::topology::output_key(1));
    // The honest producer path: Desktop-2 resolves to SID 277, the current
    // Space, so the positional target is the source home itself.
    let sid = context_target_sid(&ctx, &output, 2).expect("Desktop-2 binds");
    assert_eq!(sid, 277);
    let command = crate::world_command_map::send_command(&send(2), "4242_7", output.clone())
        .expect("send route");
    let members = repro_members(277, 4242, 7);
    let global = repro_global(277, 4242, 7);
    let (memory, closed, unreadable) = repro_empty_maps();
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    // Same-current refuses as `unchanged` with the shared-core wording -
    // never a native transfer, never a mistargeted failure.
    let second = repro_report(
        &ctx,
        &members,
        &memory,
        &closed,
        &unreadable,
        &global,
        2,
        2,
        "req-2",
        Some(2),
        Some(command.clone()),
    );
    assert!(validate_report(&second));
    match engine.handle_report(&second) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, RefusalKind::Unchanged.as_str());
            assert_eq!(message, RefusalKind::Unchanged.message());
        }
        other => panic!("same-current send refuses unchanged, got {other:?}"),
    }
    assert!(
        engine.retained_transfers().is_empty(),
        "refused same-current send retains no transfer (no post, no focus effect)"
    );
    // A repeat refuses identically: no replay, no accumulation, no plan.
    let third = repro_report(
        &ctx,
        &members,
        &memory,
        &closed,
        &unreadable,
        &global,
        3,
        3,
        "req-3",
        Some(3),
        Some(command),
    );
    assert!(validate_report(&third));
    assert!(
        matches!(
            engine.handle_report(&third),
            WorldOutcome::Rejected {
                kind: "unchanged",
                ..
            }
        ),
        "repeat same-current send still refuses without replay"
    );
    assert!(engine.retained_transfers().is_empty());
    // Unmanaged-subject fence is untouched by the unchanged path.
    let unknown = WorldCommand::Send {
        window: WindowId("4242_9".to_owned()),
        target_output: output,
        target_workspace: "ws-2".to_owned(),
        follow: true,
    };
    let fourth = repro_report(
        &ctx,
        &members,
        &memory,
        &closed,
        &unreadable,
        &global,
        4,
        4,
        "req-4",
        Some(4),
        Some(unknown),
    );
    assert!(matches!(
        engine.handle_report(&fourth),
        WorldOutcome::Rejected {
            kind: "send-window-unknown",
            ..
        }
    ));
}

/// Positional sends resolve native order under adoption: on a 277-started
/// runtime the positional `SendNumbered{index: 1}` names native Desktop-1
/// (SID 1) — the adopted send ring tracks native order, so the mover
/// targets the adopted logical with stable follow + operation identity and
/// never self-targets the startup current. Both reports validate, so any
/// refusal would be semantic, never an API/compile oracle.
#[test]
fn positional_send_resolves_native_order_under_adoption() {
    use crate::world_command_map::send_command;
    use tiler_core::world_report::TransferKind;
    let ctx = repro_ctx(277);
    let output = OutputId(crate::topology::output_key(1));
    let sid = context_target_sid(&ctx, &output, 1).expect("Desktop-1 binds");
    assert_eq!(sid, 1, "native Desktop-1 is SID 1 in this context");
    let members = repro_members(277, 4242, 7);
    let global = repro_global(277, 4242, 7);
    let (memory, closed, unreadable) = repro_empty_maps();

    // Positional path: SendNumbered index 1 resolves through the adopted
    // send ring in native order, so it names Desktop-1 and admits with a
    // retained window transfer.
    let mut engine = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    let legacy_command = WorldCommand::SendNumbered {
        window: WindowId("4242_7".to_owned()),
        source: tiler_core::world_report::SendSource::Explicit(output.clone()),
        index: 1,
        follow: true,
    };
    let legacy_second = repro_report(
        &ctx,
        &members,
        &memory,
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
        "positional report is well-formed wire input"
    );
    let reply = match engine.handle_report(&legacy_second) {
        WorldOutcome::Accepted(reply) => reply,
        other => {
            panic!("positional SendNumbered-1 from a 277-started runtime admits, got {other:?}")
        }
    };
    assert_eq!(
        reply
            .transfers
            .iter()
            .filter(|target| target.operation_id == "op-2")
            .count(),
        1,
        "positional send retains exactly its window transfer"
    );
    assert!(
        !engine.retained_transfers().is_empty(),
        "positional send retains the transfer for the drain"
    );

    // Production admission path: identical context/report shape, but the
    // command comes from the admission mapping and admits with stable
    // follow + operation identity.
    let mut fixed = WorldEngine::new(REPRO_GENERATION).expect("generation parses");
    let first = repro_report(
        &ctx,
        &members,
        &memory,
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
    let fixed_command = send_command(&send(1), "4242_7", output.clone()).expect("send route");
    let fixed_second = repro_report(
        &ctx,
        &members,
        &memory,
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
        "positional report is well-formed wire input"
    );
    assert!(
        matches!(
            fixed.handle_report(&fixed_second),
            WorldOutcome::Accepted(_)
        ),
        "positional send to Desktop-1 from a 277-started runtime admits"
    );
    let retained = fixed.retained_transfers();
    let windowed: Vec<_> = retained
        .iter()
        .filter(
            |target| matches!(&target.kind, TransferKind::Window(window) if window.0 == "4242_7"),
        )
        .collect();
    assert_eq!(windowed.len(), 1, "one retained window transfer");
    assert_eq!(windowed[0].target_output, output);
    assert_eq!(windowed[0].target_workspace, "ws-1");
    assert!(windowed[0].follow, "follow semantics unchanged");
    assert_eq!(
        windowed[0].operation_id, "op-2",
        "stable operation identity unchanged"
    );
}

/// Stay claim handling: the follow intent rides the target, and stay claims
/// pin the return-leg focus fallback only from a core-proven admission pin.
/// Regression for a faked subject-as-fallback: with mover B (`4242_7`) and
/// resident A (`4242_9` standing in for the retained resident the engine -
/// not the claim - knows), the claim must refuse an unknown fallback rather
/// than pin B, since B is on the target after the drag. A resident pin
/// converts, a proven-empty source binds `None`, and unknown/mover/
/// unparsable/missing pins refuse. Follows carry no stay pins.
#[test]
fn stay_claim_pins_follow_intent_and_refuses_unpinnable_focus() {
    use crate::runtime_types::ClaimedSelect;
    use crate::workspace_send_effect::ManagedSendTarget;
    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::session::DomainKey;
    let domain = || DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-20".to_owned()),
    };
    fn claim(
        follow: bool,
        source_sid: Option<u64>,
        source_focus: Option<&str>,
        empty: bool,
    ) -> ClaimedSelect {
        ClaimedSelect {
            action: InputSemanticAction::WorkspaceSend { index: 2, follow },
            expected_sid: Some(3),
            ui_generation: 0,
            topology_revision: Some(9),
            send_subject: Some("4242_7".to_owned()),
            send_generation: Some(1),
            send_source: Some(DomainKey {
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-20".to_owned()),
            }),
            send_source_sid: source_sid,
            send_source_focus: source_focus.map(str::to_owned),
            send_source_focus_empty: empty,
            operation_id: Some("op-3".to_owned()),
            select_target_output: None,
            select_target_workspace: None,
            select_native_binding: None,
            select_creation_operation: None,
            select_creation_logical: None,
            select_creation_output: None,
            select_baseline_sid: None,
            send_follow_view_operation: None,
        }
    }
    let follow =
        ManagedSendTarget::from_claim(&claim(true, None, None, false)).expect("follow pins");
    assert!(follow.follow());
    assert_eq!(follow.source_sid(), None);
    assert_eq!(follow.source_focus(), None);
    // Producer-shaped: mover B with resident A pins A.
    let resident = ManagedSendTarget::from_claim(&claim(false, Some(20), Some("4242_9"), false))
        .expect("resident stay pins");
    assert!(!resident.follow());
    assert_eq!(resident.source_sid(), Some(20));
    assert_eq!(resident.source_focus(), Some("4242_9"));
    // Last-mover empty source binds None on proven-empty only.
    let empty = ManagedSendTarget::from_claim(&claim(false, Some(20), None, true))
        .expect("proven-empty stay binds");
    assert!(!empty.follow());
    assert_eq!(empty.source_sid(), Some(20));
    assert_eq!(empty.source_focus(), None);
    // Unknown, mover-as-fallback, unparsable, missing-SID, and
    // contradictory (resident plus proven-empty) pins refuse.
    for stay in [
        claim(false, Some(20), None, false),
        claim(false, None, Some("4242_9"), false),
        claim(false, None, None, false),
        claim(false, Some(20), Some("4242_7"), false),
        claim(false, Some(20), Some("not-a-window"), false),
        claim(false, Some(0), Some("4242_9"), false),
        claim(false, Some(20), Some("4242_9"), true),
    ] {
        let err = ManagedSendTarget::from_claim(&stay).expect_err("untrustworthy stay refuses");
        assert!(
            err.contains("source focus"),
            "refusal names the missing focus pin, got: {err}"
        );
        assert!(
            !err.contains("4242_7"),
            "refusal carries no window identity, got: {err}"
        );
    }
    // No claim path yields a subject-derived fallback: the mover can never
    // become the pin.
    // Explicit pins keep working with caller-supplied values only.
    let explicit =
        ManagedSendTarget::bind_stay(4242, 7, 1, domain(), 3, 9, "op-3", 20, Some("4242_9"))
            .expect("explicit resident-A stay binds");
    assert!(!explicit.follow());
    assert_eq!(explicit.source_sid(), Some(20));
    assert_eq!(explicit.source_focus(), Some("4242_9"));
    let empty_explicit = ManagedSendTarget::bind_stay(4242, 7, 1, domain(), 3, 9, "op-3", 20, None)
        .expect("explicit empty-source stay binds");
    assert_eq!(empty_explicit.source_focus(), None);
    assert!(
        ManagedSendTarget::bind_stay(4242, 7, 1, domain(), 3, 9, "op-3", 0, Some("4242_9"),)
            .is_err(),
        "zero source SID refuses"
    );
}

/// Previous-history gating: only applied follows note an arrival. Stay
/// settlements keep the source current, so even applied stays note
/// nothing; non-applied outcomes never note.
#[test]
fn history_notes_applied_follows_only() {
    use crate::workspace_send::{SendDrainOutcome, should_note_history};
    assert!(should_note_history(SendDrainOutcome::Applied, true));
    assert!(!should_note_history(SendDrainOutcome::Applied, false));
    assert!(!should_note_history(SendDrainOutcome::Refused, true));
    assert!(!should_note_history(
        SendDrainOutcome::PossiblyExecuted,
        true
    ));
    assert!(!should_note_history(
        SendDrainOutcome::PossiblyExecuted,
        false
    ));
}

#[test]
fn trailing_request_carries_admitted_pins_and_pinned_display() {
    use crate::workspace_send_effect::{SendEffectOutcome, TrailingSendRequest};
    fn claim() -> ClaimedSelect {
        let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        });
        claim.send_subject = Some("4242_7".to_owned());
        claim.send_generation = Some(9);
        claim.send_source = Some(DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("space-1".to_owned()),
        });
        claim.operation_id = Some("op-2".to_owned());
        claim.topology_revision = Some(4);
        claim
    }
    // Fully pinned claims convert; the claim's own (pre-preparation)
    // SID/revision legs are not trusted.
    let request =
        TrailingSendRequest::from_claim(&claim(), 0, 7, "uuid-7", true).expect("pins convert");
    assert_eq!((request.pid, request.wid), (4242, 7));
    assert_eq!(request.window, "4242_7");
    assert_eq!(request.app_generation, 9);
    assert_eq!(request.operation_id, "op-2");
    assert_eq!(request.pins.display_index, 0);
    assert_eq!(request.pins.pinned_cgid, 7);
    assert_eq!(request.pins.pinned_uuid, "uuid-7");
    // Non-trailing actions, unpinned subjects/lifetimes/sources/
    // operations, and unpinned displays refuse before any submit.
    let mut numbered = claim();
    numbered.action = InputSemanticAction::WorkspaceSend {
        index: 2,
        follow: true,
    };
    assert!(TrailingSendRequest::from_claim(&numbered, 0, 7, "uuid-7", true).is_err());
    let mut stay = claim();
    stay.action = InputSemanticAction::WorkspaceSend {
        index: 0,
        follow: false,
    };
    assert!(TrailingSendRequest::from_claim(&stay, 0, 7, "uuid-7", true).is_err());
    let mut unpinned = claim();
    unpinned.send_generation = None;
    assert!(TrailingSendRequest::from_claim(&unpinned, 0, 7, "uuid-7", true).is_err());
    let mut no_op = claim();
    no_op.operation_id = None;
    assert!(TrailingSendRequest::from_claim(&no_op, 0, 7, "uuid-7", true).is_err());
    assert!(TrailingSendRequest::from_claim(&claim(), 0, 0, "uuid-7", true).is_err());
    assert!(TrailingSendRequest::from_claim(&claim(), 0, 7, "", true).is_err());
    // The classifier behind the owner: pre-actuation reasons refuse
    // proven, anything past a possibly-sent press holds, unknowns hold.
    use crate::workspace_send::classify_trailing_prep_error;
    for reason in [
        "trailing send-0 unarmed: no trailing append runs",
        "trailing send refuses: caffeinate unspawnable (gone)",
        "trailing send refuses: fresh row unreadable (gone)",
        "trailing send refuses: fresh row unprovable (gone)",
        "trailing send refuses: emptiness unknown (dark)",
        "trailing send refuses: reuse ordinal past Desktop-9; unmappable",
        "trailing send refuses: append ordinal past Desktop-9; unmappable",
        "trailing send refuses: append failed (pre-create topology drift)",
        "trailing preparation unimplemented on this backend; refusing",
        "trailing send refuses: expected exactly one native display, found 2",
        "trailing send refuses: display count unreadable (gone)",
    ] {
        assert_eq!(
            classify_trailing_prep_error(reason),
            SendEffectOutcome::RefusedProven,
            "{reason}"
        );
    }
    for reason in [
        "trailing send refuses: append failed (press refused)",
        "trailing send refuses: post-append row unreadable (gone)",
        "trailing send refuses: create effect is not an exact same-display append; adoption refused",
        "trailing send refuses: post-append revision unprovable",
        "created but MC close unproven: dismiss failed",
        "owner trailing preparation never selects; refusing",
        "",
    ] {
        assert_eq!(
            classify_trailing_prep_error(reason),
            SendEffectOutcome::PossiblyExecuted,
            "{reason}"
        );
    }
}
