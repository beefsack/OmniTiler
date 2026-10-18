#![cfg(test)]
//! Offline numbered send-follow seam tests: send-shape capability,
//! world mapping, Authentic ownership (apply/resume/revert, Compatible
//! zero-touch), admission, and the full drain over a scripted backend
//! (gates, subject, target, stale binding, holder, busy, no-effect, release
//! residue, history and feedback reconciliation). No live touch on any path:
//! every holder is a fake entry, every path a temp file, every drag a
//! scripted backend. OFFLINE PREPARATION: nothing here is product-armed —
//! the catalogue marks every send row `implemented: false`, the engine
//! refuses every send, and the product tap table carries no send chord. The
//! 9 send-follow shapes are built by direct catalogue-shape injection
//! below (test-only capability), never through product arming.

use std::collections::HashMap;

use tiler_core::directional::OutputId;
use tiler_core::world_report::{AttemptKind, AttemptOutcome, WorldCommand};

use crate::desktop_shortcuts::{
    FakeStore, ShortcutEntry, desktop_digit_keycode, desktop_symbolic_id, lock_path_for,
};
use crate::input_policy::InputPreset;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction, TileBinding};
use crate::space_drag_transfer::{DragStep, PostCounts, SwitchChords};
use crate::workspace_select::{SelectionSnapshot, SelectionState};
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

fn open_gates() -> SendGates {
    SendGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

fn row() -> SelectionSnapshot {
    SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 20,
    }
}

fn claim_for(index: u8) -> ClaimedSelect {
    // Admission-bound claim: target SID resolved at admission (ordinal
    // `index` over `row()`), revision pinned, subject pinned with its
    // admission-time lifetime generation and source domain.
    let target = row().spaces[usize::from(index) - 1];
    ClaimedSelect {
        action: send(index),
        expected_sid: Some(target),
        ui_generation: 0,
        topology_revision: Some(9),
        send_subject: Some("4242_7".to_owned()),
        send_generation: Some(1),
        send_source: Some(test_source_domain()),
        operation_id: Some("op-3".to_owned()),
    }
}

/// Admission-pinned source domain for the `claim_for` fixture subject.
fn test_source_domain() -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: tiler_core::directional::WorkspaceId("space-20".to_owned()),
    }
}

/// Scripted send backend: canned row/subject/holder plus the fixture-style
/// drag effect with per-boundary fault injection. No live touch, no sleeps.
struct Fake {
    row: SelectionSnapshot,
    displays: usize,
    revision: Option<u64>,
    subject: Result<SubjectState, String>,
    holders: HashMap<u32, ShortcutEntry>,
    fail_holder_reads: Vec<u32>,
    membership: Vec<u64>,
    current: Option<u64>,
    goal: u64,
    chords: Option<SwitchChords>,
    selected: Option<(u32, u64, Option<i64>)>,
    key_posts: Vec<(u32, u64, Option<i64>, bool)>,
    counts: PostCounts,
    fail_setup: bool,
    fail_mouse_at: Option<DragStep>,
    fail_chord_down: bool,
    fail_key_up_once: bool,
    fail_key_recovery: bool,
    fail_mouse_up: bool,
    no_effect: bool,
    freeze_current: bool,
    unreadable: bool,
}

impl Fake {
    fn new() -> Self {
        let mut holders = HashMap::new();
        for desktop in 1..=9u32 {
            holders.insert(
                desktop_symbolic_id(desktop).unwrap(),
                enabled_entry(desktop),
            );
        }
        Self {
            row: row(),
            displays: 1,
            revision: Some(9),
            subject: Ok(SubjectState {
                managed_current: true,
                fullscreen: false,
            }),
            holders,
            fail_holder_reads: Vec::new(),
            membership: vec![20],
            current: Some(20),
            goal: 0,
            chords: None,
            selected: None,
            key_posts: Vec::new(),
            counts: PostCounts::default(),
            fail_setup: false,
            fail_mouse_at: None,
            fail_chord_down: false,
            fail_key_up_once: false,
            fail_key_recovery: false,
            fail_mouse_up: false,
            no_effect: false,
            freeze_current: false,
            unreadable: false,
        }
    }
}

impl SendBackend for Fake {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(self.row.clone())
    }
    fn display_count(&mut self) -> Result<usize, String> {
        Ok(self.displays)
    }
    fn revision(&mut self) -> Option<u64> {
        self.revision
    }
    fn subject_state(&mut self, window: &str) -> Result<SubjectState, String> {
        assert_eq!(window, "4242_7", "drain grabs the pinned subject only");
        self.subject.clone()
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        if self.fail_holder_reads.contains(&symbolic_id) {
            return Err(format!("injected holder read refusal for {symbolic_id}"));
        }
        self.holders.get(&symbolic_id).copied().ok_or_else(|| {
            format!("fake holder {symbolic_id} unresolvable; refusing rather than guessing")
        })
    }
    fn begin_leg(&mut self, goal: u64, chords: SwitchChords) -> Result<(), String> {
        if self.fail_setup {
            return Err("injected leg-setup refusal".to_owned());
        }
        self.goal = goal;
        // Same selection seam as the fixture backends: the single-leg
        // product send posts the target chord on both slots.
        self.selected = Some(chords.for_goal(goal, goal));
        self.chords = Some(chords);
        Ok(())
    }
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        if self.fail_mouse_at == Some(step) {
            self.fail_mouse_at = None;
            return Err(format!("injected {step:?} refusal"));
        }
        if step == DragStep::MouseUp && self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        match step {
            DragStep::MouseDown => self.counts.mouse_down += 1,
            DragStep::MouseUp => self.counts.mouse_up += 1,
            _ => {}
        }
        Ok(())
    }
    fn post_key(&mut self, down: bool) -> Result<(), String> {
        if down {
            if self.fail_chord_down {
                return Err("injected chord-down refusal".to_owned());
            }
            self.counts.key_down += 1;
            if let Some(selected) = self.selected {
                self.key_posts
                    .push((selected.0, selected.1, selected.2, true));
            }
            return Ok(());
        }
        if self.fail_key_up_once {
            self.fail_key_up_once = false;
            return Err("injected chord-up refusal".to_owned());
        }
        if self.fail_key_recovery {
            return Err("injected chord-up recovery refusal".to_owned());
        }
        self.counts.key_up += 1;
        if let Some(selected) = self.selected {
            self.key_posts
                .push((selected.0, selected.1, selected.2, false));
        }
        if !self.no_effect {
            self.membership = vec![self.goal];
            if !self.freeze_current {
                self.current = Some(self.goal);
            }
        }
        Ok(())
    }
    fn settle_grab(&mut self) {}
    fn settle_hold(&mut self) {}
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        if self.unreadable {
            return None;
        }
        Some(self.membership.clone())
    }
    fn read_current(&mut self) -> Option<u64> {
        if self.unreadable {
            return None;
        }
        self.current
    }
    fn effect_deadline_ms(&self) -> u64 {
        30
    }
    fn effect_poll_ms(&self) -> u64 {
        1
    }
    fn counts(&self) -> PostCounts {
        self.counts
    }
}

fn drain(
    history: &mut SelectionState,
    backend: &mut Fake,
    claim: ClaimedSelect,
) -> Result<SendEffect, String> {
    drain_claimed_send(history, backend, claim, &open_gates())
}

/// Test-only send-follow capability injection: the 9 send-follow shapes
/// built directly from the catalogue rows (ignoring `implemented`, which is
/// false for every send in product). The offline seam exercises exactly
/// these shapes; product arms none of them.
fn send_shape_bindings() -> Vec<TileBinding> {
    crate::workspace_input::workspace_catalogue()
        .into_iter()
        .filter(|row| {
            matches!(
                row.action,
                InputSemanticAction::WorkspaceSend { follow: true, .. }
            ) && row.id != "workspace-send-0"
        })
        .filter_map(|row| row.binding())
        .collect()
}

#[test]
fn send_follow_shape_stays_test_only_unarmed() {
    // The offline seam covers exactly the 9 send-follow shapes 1-9 with
    // Ctrl+Shift+digit chords — but the catalogue marks every one
    // `implemented: false`, the product `bindings()` arms nothing, and the
    // engine refuses every one: capability lives only in this test-only
    // injection, never in product arming.
    let injected = send_shape_bindings();
    assert_eq!(injected.len(), 9, "offline seam covers 9 send shapes");
    let mut seen = Vec::new();
    for binding in &injected {
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
    // Product reality: catalogue unimplemented, no armed bindings, engine
    // refusal for every injected shape.
    for row in crate::workspace_input::workspace_catalogue() {
        if matches!(row.action, InputSemanticAction::WorkspaceSend { .. }) {
            assert!(
                !row.implemented,
                "{} must stay implemented:false in product",
                row.id
            );
        }
    }
    assert_eq!(
        bindings().len(),
        0,
        "product arms no send chord this milestone"
    );
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
        // The seam names the shape (drain/admission/kind classifiers agree)
        // while the product engine refuses every one of them.
        assert!(crate::workspace_input::is_armed_send(&action), "{index}");
        assert!(!crate::workspace_input::is_armed_select(&action), "{index}");
        assert!(
            crate::workspace_input::workspace_unsupported_reason(&action).is_some(),
            "{index} refused by product"
        );
        assert_eq!(
            crate::workspace_input::workspace_unsupported_outcome(&action),
            Some("refused-workspace-send-unsupported"),
            "{index}"
        );
    }
}

#[test]
fn send_chords_match_settings_ctrl_shift_digits() {
    // Ctrl+Shift+digit under Ctrl (settings parity); stay rows stay
    // bindable unbound (no chord); aux rows stay Ctrl+Cmd. Uses the
    // test-only injection: product arms no send chord.
    use crate::config::ProbeModifier;
    use crate::workspace_input::effective_flags_for;
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let injected = send_shape_bindings();
    assert_eq!(injected.len(), 9);
    for binding in &injected {
        assert_eq!(
            effective_flags_for(ProbeModifier::Ctrl, binding),
            CTRL | SHIFT,
            "{:?}",
            binding.action
        );
    }
    // The tap spec category for the send shape is basic (a future send
    // would need focus): the `select` flag stays is_armed_select-only.
    let specs = crate::native_input::tap_specs_for(ProbeModifier::Ctrl, &injected);
    assert_eq!(specs.len(), 9);
    assert!(
        specs.iter().all(|s| !s.select),
        "send shapes ride the basic eligibility cache (focus-gated), never the select cache"
    );
}

#[test]
fn send_command_maps_send_shapes_to_numbered_transfers() {
    // Offline seam mapping only: no product path dispatches these (the
    // engine refuses every send first).
    use crate::world_command_map::{OP_SEND, select_command, send_command};
    assert_ne!(OP_SEND, crate::world_command_map::OP_SELECT);
    let output = OutputId("macos-display-1".to_owned());
    for index in 1..=9u8 {
        let command = send_command(&send(index), "4242_7", output.clone())
            .unwrap_or_else(|| panic!("send {index} has a world route"));
        assert_eq!(
            command,
            WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                output: output.clone(),
                index,
                follow: true,
            }
        );
        // Sends never ride the select route.
        assert_eq!(select_command(&send(index), output.clone()), None);
    }
    // Unarmed actions have no send route.
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
            send_command(&action, "4242_7", output.clone()),
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
    // Unarmed (trailing, stay, relative, select, basic) refuses.
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
fn drain_gates_refuse_before_any_post() {
    for gates in [
        SendGates {
            session_unlocked: false,
            ..open_gates()
        },
        SendGates {
            secure_active: true,
            ..open_gates()
        },
        SendGates {
            lock_screen: true,
            ..open_gates()
        },
    ] {
        let mut history = SelectionState::default();
        let mut backend = Fake::new();
        let err = drain_claimed_send(&mut history, &mut backend, claim_for(2), &gates)
            .expect_err("gated drain refuses");
        assert!(err.starts_with("send gated:"), "{err}");
        assert_eq!(
            classify_send_drain_error(&err),
            SendDrainOutcome::Refused,
            "pre-actuation refusal settles proven"
        );
        assert_eq!(backend.counts.total(), 0, "zero posts while gated");
    }
}

#[test]
fn drain_refuses_unarmed_subjectless_and_bad_display() {
    let mut history = SelectionState::default();
    // Unarmed actions refuse even through the send drain.
    for action in [
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        },
        InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: false,
        },
        InputSemanticAction::WorkspaceSelect { index: 2 },
    ] {
        let mut backend = Fake::new();
        let err = drain_claimed_send(
            &mut history,
            &mut backend,
            ClaimedSelect {
                action,
                expected_sid: None,
                ui_generation: 0,
                topology_revision: None,
                send_subject: Some("4242_7".to_owned()),
                send_generation: None,
                send_source: None,
                operation_id: None,
            },
            &open_gates(),
        )
        .expect_err("unarmed refuses");
        assert_eq!(err, "product send refuses unarmed actions", "{err}");
        assert_eq!(backend.counts.total(), 0);
    }
    // No pinned subject refuses (never grab whatever happens to be focused).
    let mut backend = Fake::new();
    let mut claim = claim_for(2);
    claim.send_subject = None;
    let err = drain(&mut history, &mut backend, claim).expect_err("subjectless refuses");
    assert!(err.contains("no subject window"), "{err}");
    assert_eq!(backend.counts.total(), 0);
    // Zero or multiple displays refuse before any backend touch.
    for displays in [0, 2] {
        let mut backend = Fake::new();
        backend.displays = displays;
        let err =
            drain(&mut history, &mut backend, claim_for(2)).expect_err("display count refuses");
        assert!(err.contains("exactly one native display"), "{err}");
        assert_eq!(backend.counts.total(), 0);
    }
}

#[test]
fn drain_refuses_unmanaged_and_fullscreen_subjects() {
    let mut history = SelectionState::default();
    // Subject left managed membership (closed, hidden, unknown): refuse.
    for state in [
        SubjectState {
            managed_current: false,
            fullscreen: false,
        },
        SubjectState {
            managed_current: false,
            fullscreen: true,
        },
    ] {
        let mut backend = Fake::new();
        backend.subject = Ok(state);
        let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("unmanaged refuses");
        assert!(err.contains("managed membership"), "{err}");
        assert_eq!(backend.counts.total(), 0);
    }
    // Fullscreen subject (game overlay owns its Space): never drag.
    let mut backend = Fake::new();
    backend.subject = Ok(SubjectState {
        managed_current: true,
        fullscreen: true,
    });
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("fullscreen refuses");
    assert!(err.contains("fullscreen"), "{err}");
    assert_eq!(backend.counts.total(), 0);
    // Unreadable subject refuses without guessing.
    let mut backend = Fake::new();
    backend.subject = Err("ax facts stale".to_owned());
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("unreadable refuses");
    assert!(err.contains("subject unreadable"), "{err}");
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn drain_refuses_stale_targets_before_any_post() {
    let mut history = SelectionState::default();
    // Ordinal past the row with no admission binding: no target.
    let mut backend = Fake::new();
    backend.row.spaces = vec![20];
    let mut unbound = ClaimedSelect::tap(send(2));
    unbound.send_subject = Some("4242_7".to_owned());
    let err = drain_claimed_send(&mut history, &mut backend, unbound, &open_gates())
        .expect_err("past-end refuses");
    assert!(err.contains("has no Space"), "{err}");
    assert_eq!(backend.counts.total(), 0);
    // Reordered row: the ordinal no longer names the admission SID.
    let mut backend = Fake::new();
    backend.row.spaces = vec![20, 7, 3];
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("reorder refuses");
    assert!(err.contains("stale bound send discarded"), "{err}");
    assert_eq!(classify_send_drain_error(&err), SendDrainOutcome::Refused);
    assert_eq!(backend.counts.total(), 0);
    // Vanished ordinal: no longer exists.
    let mut backend = Fake::new();
    backend.row.spaces = vec![20];
    let mut claim = claim_for(2);
    claim.expected_sid = Some(999);
    let err = drain(&mut history, &mut backend, claim).expect_err("vanished refuses");
    assert!(err.contains("stale bound send discarded"), "{err}");
    assert_eq!(backend.counts.total(), 0);
    // Revision drift between admission and drain.
    let mut backend = Fake::new();
    backend.revision = Some(10);
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("drift refuses");
    assert!(err.contains("revision moved"), "{err}");
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn drain_refuses_unusable_holders_before_any_post() {
    let mut history = SelectionState::default();
    // Unreadable holder.
    let mut backend = Fake::new();
    backend
        .fail_holder_reads
        .push(desktop_symbolic_id(2).unwrap());
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("unreadable refuses");
    assert!(err.contains("holder unreadable"), "{err}");
    assert_eq!(backend.counts.total(), 0);
    // Disabled holder: names the manual fix, never writes settings.
    let mut backend = Fake::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: false,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL,
        },
    );
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("disabled refuses");
    assert!(err.contains("product send refuses"), "{err}");
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::Refused,
        "holder gate is pre-actuation"
    );
    assert_eq!(backend.counts.total(), 0);
    // Customized chord (Shift+digit instead of Ctrl+digit): refused, never
    // rewritten, never posted as a chord that does nothing.
    let mut backend = Fake::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: true,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL | 0x0002_0000,
        },
    );
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("custom refuses");
    assert!(err.contains("product send refuses"), "{err}");
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn drain_sends_and_follows_with_exact_readback_and_history() {
    let mut history = SelectionState::default();
    let mut backend = Fake::new();
    let effect = drain(&mut history, &mut backend, claim_for(2)).expect("send follows");
    assert_eq!(effect.from, 20);
    assert_eq!(effect.target, 3);
    assert_eq!(effect.target_desktop, 2);
    assert_eq!(effect.membership, vec![3]);
    assert_eq!(effect.current, 3);
    assert!(!effect.pending_recovered);
    // Posted exactly the target ordinal's Control-only digit, tagged, both
    // edges — never an arrow, never untagged.
    let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
    let digit_2 = desktop_digit_keycode(2).unwrap();
    assert_eq!(
        backend.key_posts,
        vec![
            (digit_2, CG_CTRL, tag, true),
            (digit_2, CG_CTRL, tag, false)
        ]
    );
    assert!(backend.counts.balanced());
    // Verified follow notes the source in previous history: workspace-
    // previous flips back to it.
    let mut select_backend = SelectRowBackend {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3, 7],
            current: 3,
        },
    };
    let previous = history_previous(&mut history, &mut select_backend);
    assert_eq!(previous, Some(20), "follow arrival notes the source");
}

/// Minimal [`crate::workspace_select::SelectionBackend`] over a canned row
/// for proving the send-noted history through the select previous toggle.
struct SelectRowBackend {
    row: SelectionSnapshot,
}

impl crate::workspace_select::SelectionBackend for SelectRowBackend {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(self.row.clone())
    }
    fn display_count(&mut self) -> Result<usize, String> {
        Ok(1)
    }
    fn empty(&mut self, _: u64) -> Result<bool, String> {
        Ok(true)
    }
    fn create(&mut self, _: &SelectionSnapshot) -> Result<(), String> {
        Err("no create in history proof".to_owned())
    }
    fn select(&mut self, _: &SelectionSnapshot, sid: u64) -> Result<(), String> {
        self.row.current = sid;
        Ok(())
    }
}

fn history_previous(history: &mut SelectionState, backend: &mut SelectRowBackend) -> Option<u64> {
    history
        .execute(backend, InputSemanticAction::WorkspacePrevious)
        .ok()
        .map(|effect| effect.target)
}

#[test]
fn drain_no_effect_never_inverts_and_holds() {
    let mut history = SelectionState::default();
    let mut backend = Fake::new();
    backend.no_effect = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("no-effect holds");
    assert!(err.contains("no effect"), "{err}");
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::PossiblyExecuted,
        "completed chord without exact readback holds, never replays"
    );
    // Balanced release, but no history advance on ambiguous output.
    assert!(backend.counts.balanced());
    let mut select_backend = SelectRowBackend {
        row: SelectionSnapshot {
            display: "display-a".to_owned(),
            spaces: vec![20, 3, 7],
            current: 20,
        },
    };
    assert!(
        history
            .execute(&mut select_backend, InputSemanticAction::WorkspacePrevious)
            .is_err(),
        "ambiguous output advances no history"
    );
}

#[test]
fn drain_current_mismatch_holds_with_both_readbacks() {
    let mut history = SelectionState::default();
    let mut backend = Fake::new();
    backend.freeze_current = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("mismatch holds");
    assert!(err.contains("no effect"), "{err}");
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::PossiblyExecuted
    );
}

#[test]
fn drain_release_failures_park_residue_and_hold() {
    // Chord-up recovery refusal parks the key edge with the mouse released.
    let mut history = SelectionState::default();
    let mut backend = Fake::new();
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("pending parks");
    assert!(err.contains("residue"), "{err}");
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::PossiblyExecuted
    );
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    // Mouse-up refusal parks the release itself.
    let mut backend = Fake::new();
    backend.fail_mouse_up = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("release parks");
    assert!(err.contains("residue"), "{err}");
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::PossiblyExecuted
    );
    // ...while a once-only chord-up recovery still completes the follow.
    let mut backend = Fake::new();
    backend.fail_key_up_once = true;
    let effect = drain(&mut history, &mut backend, claim_for(2)).expect("recovery completes");
    assert!(effect.pending_recovered);
    assert_eq!((effect.membership, effect.current), (vec![3], 3));
}

#[test]
fn drain_setup_and_grab_refusals_are_pre_actuation() {
    // Leg-setup, grab, and chord-down refusals post no effective chord:
    // proven refusals, never holds.
    let mut history = SelectionState::default();
    let mut backend = Fake::new();
    backend.fail_setup = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("setup refuses");
    assert!(err.contains("refused before effect"), "{err}");
    assert_eq!(classify_send_drain_error(&err), SendDrainOutcome::Refused);
    assert_eq!(backend.counts.total(), 0);
    let mut backend = Fake::new();
    backend.fail_mouse_at = Some(DragStep::MouseDragged);
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("grab refuses");
    assert!(err.contains("refused before effect"), "{err}");
    assert_eq!(classify_send_drain_error(&err), SendDrainOutcome::Refused);
    let mut backend = Fake::new();
    backend.fail_chord_down = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("chord refuses");
    assert!(err.contains("refused before effect"), "{err}");
    assert_eq!(classify_send_drain_error(&err), SendDrainOutcome::Refused);
    // Unreadable post-release readback is ambiguous (the chord completed):
    // holds, never a false proven refusal.
    let mut backend = Fake::new();
    backend.unreadable = true;
    let err = drain(&mut history, &mut backend, claim_for(2)).expect_err("unreadable holds");
    assert!(
        !err.contains("refused before effect"),
        "readback refusal must not read as pre-actuation: {err}"
    );
    assert_eq!(
        classify_send_drain_error(&err),
        SendDrainOutcome::PossiblyExecuted
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
    // The live-boundary refusal classifies proven (the desktop was never
    // touched), so the core settles instead of retaining forever.
    assert_eq!(
        classify_send_drain_error(
            "product send refuses: live managed-window backend lands in the live step; no post performed"
        ),
        SendDrainOutcome::Refused
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
fn tile_bindings_cover_basic_plus_selects_only() {
    // Product tap table: 34 basic + 18 armed selects (52); the offline send
    // seam arms nothing in product (`bindings()` is empty). Duplicate-free
    // under every usable Meta/preset; Compatible under Ctrl leaves the 9
    // numbered selects plus the 4 unshifted arrow basics to macOS (39 = 30
    // basic + 9 aux selects).
    use crate::config::ProbeModifier;
    use crate::input_policy::{InputPreset, effective_bindings};
    use crate::native_input::tile_catalogue;
    fn product() -> Vec<TileBinding> {
        let mut out = tile_catalogue();
        out.extend(crate::workspace_select::bindings());
        out
    }
    assert_eq!(tile_catalogue().len(), 34);
    assert_eq!(crate::workspace_select::bindings().len(), 18);
    assert_eq!(bindings().len(), 0, "send seam arms nothing in product");
    assert_eq!(product().len(), 52);
    assert_eq!(crate::tile::product_catalogue().len(), 52);
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        let authentic = effective_bindings(InputPreset::Authentic, modifier, &product());
        assert_eq!(authentic.len(), 52, "{modifier:?}");
        let config = crate::runtime_types::RuntimeConfig {
            modifier,
            tile_bindings: Some(authentic),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_ok(), "{modifier:?}");
    }
    let compatible = effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &product());
    assert_eq!(compatible.len(), 39, "30 basic + 9 aux selects");
    assert_eq!(
        compatible
            .iter()
            .filter(|b| matches!(
                b.action,
                InputSemanticAction::WorkspaceSend { .. }
                    | InputSemanticAction::WorkspaceSendRelative { .. }
                    | InputSemanticAction::WorkspaceOutput { .. }
            ))
            .count(),
        0,
        "no send shape enters the product table under any preset"
    );
    let config = crate::runtime_types::RuntimeConfig {
        modifier: ProbeModifier::Ctrl,
        tile_bindings: Some(compatible),
        ..crate::runtime_types::RuntimeConfig::default()
    };
    assert!(config.validate().is_ok());
}
