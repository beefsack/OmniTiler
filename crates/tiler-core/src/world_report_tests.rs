//! World report shape tests: digest stability, validation gates, and
//! typed fixtures through the report constructors.
//!
//! Split from [`crate::world_report`]: submodule of the report module with
//! full access to its internals.

use super::*;

fn test_report() -> WorldReport {
    use crate::directional::WorkspaceId;
    use crate::geometry::Rect;
    use std::collections::BTreeMap;
    let domain = OutputDomain {
        id: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let mut window = crate::seed::EngineWindow {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    };
    window.hints.min_w = Some(40);
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: vec![OutputTopology {
            output: OutputId("out".to_owned()),
            native_order: vec!["ws".to_owned()],
            current_native: "ws".to_owned(),
            complete: true,
            geometry: None,
            primary: false,
        }],
        bindings: vec![WorkspaceBinding {
            logical_id: "desk-1".to_owned(),
            native_id: "ws".to_owned(),
            output: OutputId("out".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }],
        domains: vec![DomainReport {
            domain,
            outer_gap: 0,
            focused: WindowId("win-a".to_owned()),
            windows: vec![window],
            closed: Vec::new(),
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: vec![AttemptFeedback {
            operation_id: "op-1".to_owned(),
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::Unattempted,
            hold: None,
        }],
    }
}

#[test]
fn digest_covers_every_section() {
    let base = test_report();
    assert!(validate_report(&base));
    let digest = report_digest(&base);
    // Any single-section change alters the digest: duplicates are
    // decided by typed equality, but the digest must never alias two
    // different inputs silently.
    let mut changed = base.clone();
    changed.topology[0].current_native = "ws2".to_owned();
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.topology[0].native_order.push("ws2".to_owned());
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.topology[0].complete = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.bindings[0].managed = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.bindings[0].pinned = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.inventory_complete = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.gesture_barrier = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.fixed_size_predicate = crate::size_hints::FixedSizePredicate::EitherAxis;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.minimized = vec![MinimizedEntry {
        window: WindowId("win-m".to_owned()),
        output: OutputId("out".to_owned()),
        workspace: crate::directional::WorkspaceId("ws".to_owned()),
    }];
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.feedback[0].outcome = AttemptOutcome::Applied;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.feedback[0].kind = AttemptKind::Effect;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.feedback[0].hold = Some(HoldReason::RefusedTarget);
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].windows[0].hints.min_w = Some(41);
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].windows[0].floating = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].windows[0].fixed_auto = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].focused = WindowId("win-b".to_owned());
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].closed = vec![WindowId("gone".to_owned())];
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].domain.adjacent.insert(
        crate::directional::Direction::Left,
        OutputId("other".to_owned()),
    );
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.domains[0].hold = Some(HoldReason::Unreadable);
    changed.domains[0].readable = false;
    changed.domains[0].windows.clear();
    changed.domains[0].focused = WindowId(String::new());
    assert_ne!(report_digest(&changed), digest);
    // Identical input digests identically.
    assert_eq!(report_digest(&base.clone()), digest);
}

#[test]
fn validation_rejects_every_malformed_section() {
    let mut bad = test_report();
    bad.topology[0].native_order.push("bad id".to_owned());
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.topology[0].current_native = String::new();
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.bindings[0].native_id = "bad id".to_owned();
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.feedback[0].operation_id = String::new();
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "sideways".to_owned(),
    });
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::SelectNumbered {
        output: OutputId("out".to_owned()),
        index: 0,
    });
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::SelectRelative {
        output: OutputId("out".to_owned()),
        delta: 2,
    });
    assert!(!validate_report(&bad));
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::MigrateWorkspace {
        source_output: OutputId("out".to_owned()),
        target_output: OutputId("out".to_owned()),
        workspace: "ws".to_owned(),
    });
    assert!(!validate_report(&bad));
    // Command without sequence (and sequence without command) refuse.
    let mut bad = test_report();
    bad.command = Some(WorldCommand::SelectTrailing {
        output: OutputId("out".to_owned()),
    });
    assert!(!validate_report(&bad));
    // Contradictory hints are advisory, never a refusal: the clamp
    // lets the minimum win and fixed-size classification treats the pair
    // as unequal (see size_hints::meaningful). Production clients report
    // such pairs, so gating them here rejected live reports.
    let mut advisory = test_report();
    advisory.domains[0].windows[0].hints.min_w = Some(100);
    advisory.domains[0].windows[0].hints.max_w = Some(50);
    assert!(validate_report(&advisory));
    assert_eq!(validate_report_reason(&advisory), None);
    // Bad adjacency target refuses.
    let mut bad = test_report();
    bad.domains[0].domain.adjacent.insert(
        crate::directional::Direction::Left,
        OutputId("bad id".to_owned()),
    );
    assert!(!validate_report(&bad));
    // Non-opaque domain identity refuses.
    let mut bad = test_report();
    bad.domains[0].domain.id = OutputId("bad id".to_owned());
    assert!(!validate_report(&bad));
    // Mismatched operation payload refuses.
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::Operation(
        crate::boundary::CoreCommand::PointerResize {
            window: "win-a".to_owned(),
            direction: "left".to_owned(),
            boundary: 0,
            direction2: Some("up".to_owned()),
            boundary2: None,
        },
    ));
    assert!(!validate_report(&bad));
}

#[test]
fn minimized_entries_reject_overlap_and_duplicates() {
    use crate::directional::WorkspaceId;
    let entry = MinimizedEntry {
        window: WindowId("win-m".to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    // Live overlap refuses: a minimized window is absent from membership.
    let mut bad = test_report();
    bad.minimized = vec![MinimizedEntry {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    }];
    assert!(!validate_report(&bad));
    // Closed overlap refuses: contradictory input fails closed.
    let mut bad = test_report();
    bad.domains[0].windows.clear();
    bad.domains[0].focused = WindowId(String::new());
    bad.domains[0].closed = vec![WindowId("win-m".to_owned())];
    bad.minimized = vec![entry.clone()];
    assert!(!validate_report(&bad));
    // Duplicate minimized ids refuse.
    let mut bad = test_report();
    bad.minimized = vec![entry.clone(), entry];
    assert!(!validate_report(&bad));
    // Non-opaque minimized homing refuses.
    let mut bad = test_report();
    bad.minimized = vec![MinimizedEntry {
        window: WindowId("win-m".to_owned()),
        output: OutputId("bad id".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    }];
    assert!(!validate_report(&bad));
    // New command shapes validate: send-numbered allows 0..=9 only,
    // set-workspace-tiling needs opaque ids.
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::SendNumbered {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out".to_owned()),
        index: 10,
        follow: false,
    });
    assert!(!validate_report(&bad));
    let mut good = test_report();
    good.command_seq = Some(1);
    good.command = Some(WorldCommand::SendNumbered {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out".to_owned()),
        index: 0,
        follow: false,
    });
    assert!(validate_report(&good));
    let mut bad = test_report();
    bad.command_seq = Some(1);
    bad.command = Some(WorldCommand::SetWorkspaceTiling {
        output: OutputId("out".to_owned()),
        workspace: "bad id".to_owned(),
        tiled: false,
    });
    assert!(!validate_report(&bad));
}

#[test]
fn tiling_modes_use_stable_tokens() {
    assert_eq!(TilingMode::Tiled.as_str(), "tiled");
    assert_eq!(TilingMode::Floating.as_str(), "floating");
}

#[test]
fn json_lengths_derive_from_escaping_rules() {
    // Opaque ids never escape; quotes, backslashes, and C0 controls do.
    assert_eq!(json_string_len("abc-1_X.Z"), 11);
    assert_eq!(json_string_len("a\"b"), 6);
    assert_eq!(json_string_len("a\\b"), 6);
    assert_eq!(json_string_len("a\x01b"), 2 + 1 + 6 + 1);
    assert_eq!(int_len(0), 1);
    assert_eq!(int_len(-1), 2);
    assert_eq!(int_len(i32::MIN), 11);
    assert_eq!(int_len(16384), 5);
    assert_eq!(u64_len(0), 1);
    assert_eq!(u64_len(u64::MAX), 20);
}

#[test]
fn estimate_upper_bounds_canonical_encoding() {
    // The estimator must bound the exact canonical bytes on small and
    // identifier-maximal replies alike; the commit gate relies on it.
    let long = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    let reply = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: long.clone(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: u64::MAX,
        state_revision: u64::MAX,
        ready: true,
        command_consumed: true,
        scopes: vec![ScopeStatus {
            scope: long.clone(),
            authority: ScopeAuthority::Hold(HoldReason::AmbiguousCreation),
        }],
        workspaces: vec![WorkspaceDesire {
            id: long.clone(),
            output: OutputId(long.clone()),
            native_binding: Some(long.clone()),
            logical_id: Some(long.clone()),
            scope: long.clone(),
            order: usize::MAX,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: Some(HoldReason::OccupiedRemoval),
        }],
        outputs: vec![OutputDesire {
            output: OutputId(long.clone()),
            logical_view: Some(long.clone()),
            tiling_mode: TilingMode::Tiled,
            hold: Some(HoldReason::IncompleteInventory),
        }],
        windows: vec![WindowDesire {
            lifetime: WindowId(long.clone()),
            output: OutputId(long.clone()),
            workspace: long.clone(),
            allocation: Some(crate::geometry::Rect {
                x: i32::MIN,
                y: -16384,
                w: 16384,
                h: 16384,
            }),
            effective_frame: None,
            frame_authority: ScopeAuthority::Hold(HoldReason::GestureBarrier),
            membership_authority: ScopeAuthority::Hold(HoldReason::RefusedTarget),
            frame_operation: None,
            // Exercised set (not empty): the estimator counts the exact
            // floating key plus the origin token width.
            floating: true,
            float_origin: Some(FloatOrigin::Capacity),
        }],
        transfers: vec![TransferTarget {
            operation_id: long.clone(),
            kind: TransferKind::Window(WindowId(long.clone())),
            target_output: OutputId(long.clone()),
            target_workspace: long.clone(),
            follow: true,
            held: true,
        }],
        effects: vec![OneShotEffect {
            id: long.clone(),
            kind: EffectKind::Focus,
            target: WindowId(long),
            issued_revision: u64::MAX,
        }],
        command_result: None,
        presentation_bytes: 0,
    };
    let exact = encode_reply_canonical(&reply).len();
    assert!(reply_estimate(&reply) >= exact);
    // Numeric extrema encode at their exact widths inside the bound.
    assert!(exact > 2048);
    assert!(reply_estimate(&reply) < WORLD_MAX_REPLY_BYTES);
}

#[test]
fn sizing_key_constants_match_wire_exactly() {
    // Exact JSON key punctuation (comma + quotes + colon) backing the
    // structural estimators: `anchorFallback` 18, `noSpaceResult` 17,
    // and the newer floating pair 12 + 16 = 28 on top of the 109 base
    // (137 per reply window row). The estimators stay conservative
    // (actual <= estimate, locked above and in the protocol codec
    // tests); these pins keep the constants exact, never padded.
    assert_eq!(",\"anchorFallback\":".len(), 18);
    assert_eq!(",\"noSpaceResult\":".len(), 17);
    assert_eq!(",\"floating\":".len(), 12);
    assert_eq!(",\"float_origin\":".len(), 16);
    assert_eq!(12 + 16, 28);
}

#[test]
fn request_freshness_never_conflates_command() {
    // Same seq + identical digest duplicates even when a command rode along.
    assert_eq!(
        classify_request(7, 99, true, 7, 99),
        RequestFreshness::Duplicate
    );
    // Same seq + changed input is invalid, never silently consumed.
    assert_eq!(
        classify_request(7, 100, true, 7, 99),
        RequestFreshness::ReusedInvalid
    );
    assert_eq!(
        classify_request(6, 99, true, 7, 99),
        RequestFreshness::Stale
    );
    assert_eq!(classify_request(8, 1, true, 7, 99), RequestFreshness::New);
    assert_eq!(classify_request(0, 0, false, 0, 0), RequestFreshness::New);
}

#[test]
fn explicit_close_allowed_with_incomplete_inventory() {
    use crate::directional::WorkspaceId;
    use crate::geometry::Rect;
    let domain = OutputDomain {
        id: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100,
        },
        gap: 0,
        adjacent: std::collections::BTreeMap::new(),
    };
    let report = WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: Vec::new(),
        bindings: Vec::new(),
        domains: vec![DomainReport {
            domain,
            outer_gap: 0,
            focused: WindowId(String::new()),
            windows: Vec::new(),
            closed: vec![WindowId("gone".to_owned())],
            readable: true,
            hold: None,
        }],
        minimized: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: false,
        gesture_barrier: false,
        feedback: Vec::new(),
    };
    assert!(validate_report(&report));
}

#[test]
fn unbound_logical_token_never_actuates() {
    let pending = WorkspaceDesire {
        id: "wl-7".to_owned(),
        output: OutputId("out".to_owned()),
        native_binding: None,
        logical_id: Some("wl-7".to_owned()),
        scope: "out".to_owned(),
        order: 2,
        tiling_mode: TilingMode::Tiled,
        existence: true,
        hold: None,
    };
    // Even if the token string appears in the native inventory by
    // coincidence, the gate stays closed: create, never adopt blindly.
    assert_eq!(actuation_native_id(&pending), None);
    let bound = WorkspaceDesire {
        native_binding: Some("native-9".to_owned()),
        ..pending
    };
    assert_eq!(actuation_native_id(&bound), Some("native-9"));
}

#[test]
fn estimator_rejects_huge_world() {
    // 9000 max-length window rows exceed 4 MiB structurally (no engine
    // needed): the transition's pre-publish gate would discard the reply
    // rather than truncate it.
    let long = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    let row = WindowDesire {
        lifetime: WindowId(long.clone()),
        output: OutputId(long.clone()),
        workspace: long.clone(),
        allocation: None,
        effective_frame: None,
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        frame_operation: None,
        floating: false,
        float_origin: None,
    };
    let reply = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: 1,
        state_revision: 2,
        ready: true,
        command_consumed: false,
        scopes: Vec::new(),
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: vec![row; 9000],
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    };
    assert!(reply_estimate(&reply) > WORLD_MAX_REPLY_BYTES);
}

#[test]
fn estimator_covers_every_section_with_long_ids() {
    let long = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    let reply = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: long.clone(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: 1,
        state_revision: 2,
        ready: true,
        command_consumed: false,
        scopes: vec![ScopeStatus {
            scope: long.clone(),
            authority: ScopeAuthority::Controlled,
        }],
        workspaces: vec![WorkspaceDesire {
            id: long.clone(),
            output: OutputId(long.clone()),
            native_binding: Some(long.clone()),
            logical_id: Some(long.clone()),
            scope: long.clone(),
            order: 0,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
        }],
        outputs: vec![OutputDesire {
            output: OutputId(long.clone()),
            logical_view: Some(long.clone()),
            tiling_mode: TilingMode::Tiled,
            hold: None,
        }],
        windows: vec![WindowDesire {
            lifetime: WindowId(long.clone()),
            output: OutputId(long.clone()),
            workspace: long.clone(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Controlled,
            membership_authority: ScopeAuthority::Controlled,
            frame_operation: None,
            floating: false,
            float_origin: None,
        }],
        transfers: vec![TransferTarget {
            operation_id: long.clone(),
            kind: TransferKind::Window(WindowId(long.clone())),
            target_output: OutputId(long.clone()),
            target_workspace: long.clone(),
            follow: true,
            held: false,
        }],
        effects: vec![OneShotEffect {
            id: long.clone(),
            kind: EffectKind::Focus,
            target: WindowId(long),
            issued_revision: 2,
        }],
        command_result: None,
        presentation_bytes: 1024,
    };
    let estimate = reply_estimate(&reply);
    // Every section contributed: well above the small-fixture size and
    // the presentation reservation is counted exactly.
    assert!(estimate > 2048 + 1024);
    assert!(estimate < WORLD_MAX_REPLY_BYTES);
}
