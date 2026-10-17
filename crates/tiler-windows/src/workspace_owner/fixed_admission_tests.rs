#![forbid(unsafe_code)]
use super::*;
use std::collections::HashMap;
use tiler_core::boundary::{CoreCommand, CoreReply};
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    }
}

fn partial_w_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(0),
        max_w: Some(640),
        max_h: Some(0),
    }
}

fn partial_h_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(0),
        min_h: Some(480),
        max_w: Some(0),
        max_h: Some(480),
    }
}

fn rig() -> (
    tiler_core::engine::Engine,
    OwnerId,
    GenerationId,
    (OutputDomain, DomainKey),
    Rect,
) {
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    engine.set_fixed_size_admission(true);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    let key = DomainKey {
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds,
        gap: 8,
        adjacent: BTreeMap::new(),
    };
    (engine, owner, generation, (domain, key), bounds)
}

fn revision_of(engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)) -> u64 {
    engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn reconcile(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &(OutputDomain, DomainKey),
    rows: &[(WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags)],
    focused: &str,
    correlation: &str,
) -> CoreReply {
    let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|(id, rect, hints, floating, _)| (id.clone(), *rect, *hints, *floating))
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(id, rect, _, _, _)| (id.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse(correlation).expect("correlation");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        owner,
        generation,
        &correlation,
        revision_of(engine, domain),
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    let flags: HashMap<WindowId, FixedAdmissionFlags> = rows
        .iter()
        .map(|(id, _, _, _, flags)| (id.clone(), *flags))
        .collect();
    stamp_fixed_admission_flags(&mut event.windows, &flags);
    engine.handle(&event)
}

fn plain_row(
    id: &str,
    bounds: Rect,
) -> (WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags) {
    (
        WindowId(id.to_owned()),
        bounds,
        WindowSizeHints::none(),
        false,
        FixedAdmissionFlags::default(),
    )
}

fn fixed_row(
    id: &str,
    bounds: Rect,
) -> (WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags) {
    (
        WindowId(id.to_owned()),
        bounds,
        fixed_hints(),
        false,
        FixedAdmissionFlags::default(),
    )
}

fn is_tiled(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
    id: &str,
) -> bool {
    engine.session(&domain.1).is_some_and(|s| {
        s.snapshot().windows.iter().any(|w| w.window.0 == id)
            && !s.is_exception(&WindowId(id.to_owned()))
    })
}

fn is_float(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
    id: &str,
) -> bool {
    engine
        .session(&domain.1)
        .is_some_and(|s| s.is_exception(&WindowId(id.to_owned())))
}

fn is_automatic(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
    id: &str,
) -> bool {
    engine
        .session(&domain.1)
        .is_some_and(|s| s.is_automatic_fixed_float(&WindowId(id.to_owned())))
}

fn has_override(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
    id: &str,
) -> bool {
    engine
        .session(&domain.1)
        .is_some_and(|s| s.has_fixed_tile_override(&WindowId(id.to_owned())))
}

#[test]
fn tile_override_marker_hydrates_only_with_full_fences() {
    use crate::tiling::{parse_tile_override_marker, tile_override_marker_value};
    use crate::workspace_owner::{
        MarkerSlot, RestartMarker, classify_restart_marker, restart_tile_override_eligible,
    };
    assert!(parse_tile_override_marker(tile_override_marker_value()).is_some());
    assert!(parse_tile_override_marker(0).is_none());
    let tile = classify_restart_marker(
        MarkerSlot::Absent,
        MarkerSlot::Absent,
        MarkerSlot::Value(tile_override_marker_value()),
    );
    assert_eq!(tile, RestartMarker::TileOverride);
    // Full fences hydrate; any missing fence fails closed with no
    // intent (recompute fallback covers the window through its hints).
    assert!(restart_tile_override_eligible(true, true, true, tile));
    assert!(!restart_tile_override_eligible(false, true, true, tile));
    assert!(!restart_tile_override_eligible(true, false, true, tile));
    assert!(!restart_tile_override_eligible(true, true, false, tile));
    // Every other lane never overrides here.
    for marker in [
        RestartMarker::Absent,
        RestartMarker::FloatIntent,
        RestartMarker::Sticky(false),
        RestartMarker::Sticky(true),
        RestartMarker::Corrupt,
        RestartMarker::Unreadable,
    ] {
        assert!(
            !restart_tile_override_eligible(true, true, true, marker),
            "{marker:?} never hydrates an override"
        );
    }
}

#[test]
fn stamp_helper_sets_only_listed_windows() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let correlation = CorrelationId::parse("stamp-1").expect("correlation");
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let key = DomainKey {
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds,
        gap: 8,
        adjacent: BTreeMap::new(),
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        0,
        7,
        &domain,
        &key,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                WindowSizeHints::none(),
                true,
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    for window in &event.windows {
        assert!(!window.fullscreen);
        assert!(!window.fixed_auto);
        assert!(!window.fixed_suppress);
    }
    let flags: HashMap<WindowId, FixedAdmissionFlags> = [(
        WindowId("w2".to_owned()),
        FixedAdmissionFlags {
            fullscreen: true,
            fixed_auto: true,
            fixed_suppress: true,
        },
    )]
    .into_iter()
    .collect();
    stamp_fixed_admission_flags(&mut event.windows, &flags);
    let w1 = event
        .windows
        .iter()
        .find(|w| w.window.0 == "w1")
        .expect("w1");
    let w2 = event
        .windows
        .iter()
        .find(|w| w.window.0 == "w2")
        .expect("w2");
    assert!(!w1.fullscreen && !w1.fixed_auto && !w1.fixed_suppress);
    assert!(w2.fullscreen && w2.fixed_auto && w2.fixed_suppress);
    // Sticky/maximized stay adapter-owned (false) through the stamp.
    assert!(!w2.sticky);
    assert!(!w2.maximized);
}

#[test]
fn fixed_brand_new_floats_automatic_with_bounded_report_and_no_geometry() {
    let (mut engine, owner, generation, domain, bounds) = rig();
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "admission converges, got {reply:?}"
    );
    assert!(is_float(&engine, &domain, "wfix"), "fixed floats");
    assert!(is_automatic(&engine, &domain, "wfix"), "automatic mark");
    assert!(is_tiled(&engine, &domain, "w1"), "ordinary tiles");
    // D8: the automatic admission plans no native target writes: only
    // the ordinary tile takes geometry.
    if let CoreReply::Tiled(plan) = &reply {
        assert!(
            plan.geometry.iter().all(|g| g.window.0 != "wfix"),
            "no geometry for the automatic float"
        );
        assert!(
            plan.geometry.iter().any(|g| g.window.0 == "w1"),
            "ordinary tile placed"
        );
    }
    let report = engine.last_fixed_admission().expect("bounded report");
    assert_eq!(report.reason, "fixed-equal");
    assert_eq!(report.admitted, 1);
}

#[test]
fn partial_zero_tiles_under_default_and_floats_under_either_axis() {
    // G-05: zero is unset per axis. Both partial-zero orientations tile
    // under both-axes-fixed and float through the nonzero axis under
    // either-axis-fixed. Missing and full-zero tile under both.
    for (name, hints) in [("width", partial_w_hints()), ("height", partial_h_hints())] {
        let (mut engine, owner, generation, domain, bounds) = rig();
        let rows = vec![
            (
                WindowId(format!("p-{name}")),
                bounds,
                hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "{name} converges, got {reply:?}"
        );
        assert!(
            is_tiled(&engine, &domain, &format!("p-{name}")),
            "{name} partial-zero tiles under the default"
        );
        assert!(
            engine.last_fixed_admission().is_none(),
            "{name} default admits nothing automatic"
        );
        engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        let rows = vec![
            (
                WindowId(format!("q-{name}")),
                bounds,
                hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-2",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "{name} either-axis converges, got {reply:?}"
        );
        assert!(
            is_automatic(&engine, &domain, &format!("q-{name}")),
            "{name} partial-zero floats under either-axis"
        );
    }
    // Missing and full-zero tile under both predicates.
    for hints in [
        WindowSizeHints::none(),
        WindowSizeHints {
            min_w: Some(0),
            min_h: Some(0),
            max_w: Some(0),
            max_h: Some(0),
        },
    ] {
        for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
            let (mut engine, owner, generation, domain, bounds) = rig();
            engine.set_fixed_size_predicate(predicate);
            let rows = vec![
                (
                    WindowId("z".to_owned()),
                    bounds,
                    hints,
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            assert!(
                is_tiled(&engine, &domain, "z"),
                "zero/missing tiles under {predicate:?}"
            );
        }
    }
}

#[test]
fn predicate_switch_never_reclassifies_retained_windows() {
    // D2: a one-axis-fixed window tiles under the default; switching to
    // either-axis leaves the retained tile alone. Only subsequent
    // admissions use the new predicate.
    let (mut engine, owner, generation, domain, bounds) = rig();
    let one_axis = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(960),
    };
    let rows = vec![
        (
            WindowId("wone".to_owned()),
            bounds,
            one_axis,
            false,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-1",
    );
    assert!(is_tiled(&engine, &domain, "wone"));
    engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-2",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "switch converges, got {reply:?}"
    );
    assert!(
        is_tiled(&engine, &domain, "wone"),
        "retained tile survives the switch"
    );
    assert!(!is_automatic(&engine, &domain, "wone"));
    // A subsequent identical admission floats under the new predicate.
    let rows = vec![
        (
            WindowId("wone".to_owned()),
            bounds,
            one_axis,
            false,
            FixedAdmissionFlags::default(),
        ),
        (
            WindowId("wtwo".to_owned()),
            bounds,
            one_axis,
            false,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-3",
    );
    assert!(is_tiled(&engine, &domain, "wone"));
    assert!(
        is_automatic(&engine, &domain, "wtwo"),
        "later admission uses either-axis"
    );
}

#[allow(clippy::too_many_arguments)]
fn toggle_float(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &(OutputDomain, DomainKey),
    rows: &[(WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags)],
    focused: &str,
    window: &str,
    correlation: &str,
    float_rect: Option<Rect>,
) -> CoreReply {
    let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|(id, rect, hints, floating, _)| (id.clone(), *rect, *hints, *floating))
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(id, rect, _, _, _)| (id.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse(correlation).expect("correlation");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        owner,
        generation,
        &correlation,
        revision_of(engine, domain),
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    let flags: HashMap<WindowId, FixedAdmissionFlags> = rows
        .iter()
        .map(|(id, _, _, _, flags)| (id.clone(), *flags))
        .collect();
    stamp_fixed_admission_flags(&mut event.windows, &flags);
    event.command = CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect,
    };
    engine.handle(&event)
}

#[test]
fn explicit_unfloat_wins_and_survives_release_with_suppress() {
    // D3/D6/D7: unfloating an automatic fixed float records the
    // override; after a domain release (workspace enable), the stamped
    // suppress signal re-tiles it instead of re-floating. Without the
    // signal the same hints re-float (control leg).
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    assert!(is_automatic(&engine, &domain, "wfix"));
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            fixed_hints(),
            true,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    let reply = toggle_float(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "wfix",
        "wfix",
        "unfloat-1",
        None,
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "unfloat commits, got {reply:?}"
    );
    assert!(is_tiled(&engine, &domain, "wfix"), "explicit tile wins");
    assert!(
        !is_automatic(&engine, &domain, "wfix"),
        "automatic mark withdraws"
    );
    assert!(has_override(&engine, &domain, "wfix"), "override recorded");
    // Workspace enable: release drops the session including the marks.
    let correlation = CorrelationId::parse("release-1").expect("correlation");
    let mut release = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        11,
        &domain.0,
        &domain.1,
        8,
        &[
            (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    release.command = CoreCommand::ReleaseDomain;
    assert!(matches!(engine.handle(&release), CoreReply::Released));
    // Fresh re-admission with the adapter suppress signal (override set /
    // marker): the fixed window tiles instead of re-floating.
    let suppress = FixedAdmissionFlags {
        fullscreen: false,
        fixed_auto: false,
        fixed_suppress: true,
    };
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            fixed_hints(),
            false,
            suppress,
        ),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-2",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "enable converges, got {reply:?}"
    );
    assert!(
        is_tiled(&engine, &domain, "wfix"),
        "override stays tiled at enable"
    );
    assert!(!is_automatic(&engine, &domain, "wfix"));
    // Control: a fresh fixed window without the signal floats.
    let rows = vec![
        fixed_row("wnew", bounds),
        (
            WindowId("wfix".to_owned()),
            bounds,
            fixed_hints(),
            false,
            suppress,
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-3",
    );
    assert!(
        is_automatic(&engine, &domain, "wnew"),
        "unsuppressed fixed floats"
    );
    assert!(
        is_tiled(&engine, &domain, "wfix"),
        "suppressed override keeps tiling"
    );
}

#[test]
fn born_fullscreen_first_exit_uses_current_hints_then_retains_identity() {
    // D5: born-fullscreen arrivals hold slotless; the first exit
    // classifies with current hints and predicate (fixed floats
    // untouched, otherwise tiles); later toggles retain identity (D2).
    let (mut engine, owner, generation, domain, bounds) = rig();
    let held = FixedAdmissionFlags {
        fullscreen: true,
        fixed_auto: false,
        fixed_suppress: false,
    };
    let rows = vec![
        (
            WindowId("efix".to_owned()),
            bounds,
            fixed_hints(),
            true,
            held,
        ),
        (
            WindowId("nplain".to_owned()),
            bounds,
            WindowSizeHints::none(),
            true,
            held,
        ),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-1",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "birth hold converges, got {reply:?}"
    );
    assert!(is_float(&engine, &domain, "efix"), "born hold floats");
    assert!(
        !is_automatic(&engine, &domain, "efix"),
        "hold carries no automatic mark"
    );
    // First exit: fixed floats automatic, non-fixed tiles.
    let rows = vec![
        fixed_row("efix", bounds),
        plain_row("nplain", bounds),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "exit-1",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "first exit converges, got {reply:?}"
    );
    assert!(
        is_automatic(&engine, &domain, "efix"),
        "fixed first exit floats automatic"
    );
    assert!(
        is_tiled(&engine, &domain, "nplain"),
        "non-fixed first exit tiles"
    );
    // Repeated toggles retain identity: re-enter fullscreen, exit again,
    // and the layers persist instead of reclassifying. A later
    // fullscreen transition on the slotted member rides tiled with the
    // overlay flag (managed overlay transition, never a float flip).
    let held_full = FixedAdmissionFlags {
        fullscreen: true,
        fixed_auto: false,
        fixed_suppress: false,
    };
    let rows = vec![
        (
            WindowId("efix".to_owned()),
            bounds,
            fixed_hints(),
            true,
            held,
        ),
        (
            WindowId("nplain".to_owned()),
            bounds,
            WindowSizeHints::none(),
            false,
            held_full,
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "refull-1",
    );
    assert!(is_float(&engine, &domain, "efix"));
    assert!(
        is_tiled(&engine, &domain, "nplain"),
        "tiled member keeps its slot under fullscreen"
    );
    let rows = vec![
        fixed_row("efix", bounds),
        plain_row("nplain", bounds),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "exit-2",
    );
    assert!(
        is_float(&engine, &domain, "efix"),
        "automatic stays floating"
    );
    assert!(is_tiled(&engine, &domain, "nplain"), "tiled stays tiled");
}

#[test]
fn workspace_enable_rechecks_every_candidate_and_keeps_intent() {
    // D6: enabling workspace tiling checks every window being tiled,
    // including arrivals while floating: fixed stays/becomes untouched
    // float, others tile. Explicit overrides stay tiled; intentional and
    // sticky floats keep their existing rules.
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    assert!(is_automatic(&engine, &domain, "wfix"));
    let correlation = CorrelationId::parse("release-1").expect("correlation");
    let mut release = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        11,
        &domain.0,
        &domain.1,
        8,
        &[
            (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    release.command = CoreCommand::ReleaseDomain;
    assert!(matches!(engine.handle(&release), CoreReply::Released));
    // Enable re-admission: the fixed arrival floats, the hint-changed
    // ordinary arrival tiles, the explicit override tiles, and the
    // intentional float stays intentional (never automatic).
    let suppress = FixedAdmissionFlags {
        fullscreen: false,
        fixed_auto: false,
        fixed_suppress: true,
    };
    let rows = vec![
        fixed_row("wfix", bounds),
        plain_row("wchanged", bounds),
        (
            WindowId("wover".to_owned()),
            bounds,
            fixed_hints(),
            false,
            suppress,
        ),
        (
            WindowId("wintent".to_owned()),
            bounds,
            WindowSizeHints::none(),
            true,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "enable-1",
    );
    assert!(
        !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
        "enable converges, got {reply:?}"
    );
    assert!(
        is_automatic(&engine, &domain, "wfix"),
        "fixed stays automatic float"
    );
    assert!(
        is_tiled(&engine, &domain, "wchanged"),
        "hint-changed ordinary tiles"
    );
    assert!(
        is_tiled(&engine, &domain, "wover"),
        "explicit override stays tiled"
    );
    assert!(!is_automatic(&engine, &domain, "wover"));
    assert!(
        is_float(&engine, &domain, "wintent"),
        "intentional stays floating"
    );
    assert!(
        !is_automatic(&engine, &domain, "wintent"),
        "intentional never marked automatic"
    );
}

#[test]
fn id_reuse_after_removal_classifies_again() {
    // D3: removal drops fixed-size bookkeeping, so a reused id with
    // different hints classifies fresh.
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    assert!(is_automatic(&engine, &domain, "wfix"));
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[plain_row("w1", bounds)],
        "w1",
        "reconcile-2",
    );
    assert!(
        engine
            .session(&domain.1)
            .is_some_and(|s| !s.is_exception(&WindowId("wfix".to_owned()))),
        "removal drops membership"
    );
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[plain_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-3",
    );
    assert!(
        is_tiled(&engine, &domain, "wfix"),
        "reused id with plain hints tiles"
    );
    assert!(!is_automatic(&engine, &domain, "wfix"));
    assert!(!has_override(&engine, &domain, "wfix"));
}

#[test]
fn send_carries_suppress_so_override_mover_stays_tiled() {
    // An explicit tile override survives a workspace send when the
    // adapter stamps its suppress origin on the mover.
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            fixed_hints(),
            true,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    toggle_float(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "wfix",
        "wfix",
        "unfloat-1",
        None,
    );
    assert!(has_override(&engine, &domain, "wfix"));
    let target_domain = {
        let key = DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
            bounds,
            gap: 8,
            adjacent: BTreeMap::new(),
        };
        (domain, key)
    };
    let source_rows = vec![
        OwnerRow {
            token: "wfix".to_owned(),
            rect: bounds,
            hints: fixed_hints(),
            floating: false,
        },
        OwnerRow {
            token: "w1".to_owned(),
            rect: bounds,
            hints: WindowSizeHints::none(),
            floating: false,
        },
    ];
    let target_rows = vec![OwnerRow {
        token: "w2".to_owned(),
        rect: bounds,
        hints: WindowSizeHints::none(),
        floating: false,
    }];
    // The target workspace session exists (per-workspace sessions);
    // seed it so the two-domain plan has a retained target.
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &target_domain,
        &[plain_row("w2", bounds)],
        "w2",
        "seed-target",
    );
    let correlation = CorrelationId::parse("send-1").expect("correlation");
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        23,
        (domain.0.clone(), domain.1.clone()),
        (target_domain.0.clone(), target_domain.1.clone()),
        &source_rows,
        &target_rows,
        "wfix",
        8,
        true,
    )
    .expect("send event builds");
    stamp_send_target(&mut event, &target_domain.1);
    let flags: HashMap<WindowId, FixedAdmissionFlags> = [(
        WindowId("wfix".to_owned()),
        FixedAdmissionFlags {
            fullscreen: false,
            fixed_auto: false,
            fixed_suppress: true,
        },
    )]
    .into_iter()
    .collect();
    stamp_fixed_admission_flags(&mut event.windows, &flags);
    stamp_fixed_admission_flags(&mut event.target_windows, &flags);
    let reply = engine.handle(&event);
    assert!(
        !matches!(
            reply,
            CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
        ),
        "send converges, got {reply:?}"
    );
    assert!(
        engine.session(&target_domain.1).is_some_and(|s| s
            .snapshot()
            .windows
            .iter()
            .any(|w| w.window.0 == "wfix")),
        "mover arrives in the target"
    );
}

fn converted(min_outer: Option<(i32, i32)>, max_outer: Option<(i32, i32)>) -> WindowSizeHints {
    // Production conversion path with a nonzero frame (8px every side):
    // the same inset subtraction the native query feeds the Engine.
    crate::tiling::hints_from_outer(
        min_outer,
        max_outer,
        crate::tiling::FrameInsets {
            left: 8,
            top: 8,
            right: 8,
            bottom: 8,
        },
    )
}

#[test]
fn production_conversion_partial_zero_width_floats_only_under_either_axis() {
    // G-05 through the real production conversion (raw outer tracks plus
    // nonzero frame insets) into a real Engine: min=max=(640,0) outer
    // converts to visible (624,0)/(624,0), which tiles under the default
    // and floats through its nonzero axis under either-axis-fixed.
    for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
        let (mut engine, owner, generation, domain, bounds) = rig();
        engine.set_fixed_size_predicate(predicate);
        let hints = converted(Some((640, 0)), Some((640, 0)));
        assert_eq!(
            (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
            (Some(624), Some(0), Some(624), Some(0)),
            "conversion preserves the unset axis"
        );
        let rows = vec![
            (
                WindowId("p".to_owned()),
                bounds,
                hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(
            !matches!(
                reply,
                CoreReply::Rejected { .. }
                    | CoreReply::Diverged(_)
                    | CoreReply::SnapshotInvalid { .. }
            ),
            "converges under {predicate:?}, got {reply:?}"
        );
        match predicate {
            FixedSizePredicate::BothAxes => assert!(
                is_tiled(&engine, &domain, "p"),
                "partial-zero width tiles under the default"
            ),
            FixedSizePredicate::EitherAxis => assert!(
                is_automatic(&engine, &domain, "p"),
                "partial-zero width floats under either-axis"
            ),
        }
    }
}

#[test]
fn production_conversion_partial_zero_height_floats_only_under_either_axis() {
    // Mirror orientation: min=max=(0,480) outer converts to visible
    // (0,464)/(0,464) and behaves symmetrically.
    for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
        let (mut engine, owner, generation, domain, bounds) = rig();
        engine.set_fixed_size_predicate(predicate);
        let hints = converted(Some((0, 480)), Some((0, 480)));
        assert_eq!(
            (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
            (Some(0), Some(464), Some(0), Some(464)),
            "conversion preserves the unset axis"
        );
        let rows = vec![
            (
                WindowId("p".to_owned()),
                bounds,
                hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        match predicate {
            FixedSizePredicate::BothAxes => assert!(
                is_tiled(&engine, &domain, "p"),
                "partial-zero height tiles under the default"
            ),
            FixedSizePredicate::EitherAxis => assert!(
                is_automatic(&engine, &domain, "p"),
                "partial-zero height floats under either-axis"
            ),
        }
    }
}

#[test]
fn production_conversion_fixed_with_insets_floats_and_reports() {
    // A 640x480 fixed client behind an 8px frame converts to visible
    // 624x464 on both sides: still equal and nonzero, so it floats
    // automatic with the bounded fixed-equal report and no geometry.
    let (mut engine, owner, generation, domain, bounds) = rig();
    let hints = converted(Some((640, 480)), Some((640, 480)));
    assert_eq!(
        (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
        (Some(624), Some(464), Some(624), Some(464))
    );
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            hints,
            false,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-1",
    );
    assert!(is_automatic(&engine, &domain, "wfix"));
    if let CoreReply::Tiled(plan) = &reply {
        assert!(
            plan.geometry.iter().all(|g| g.window.0 != "wfix"),
            "no native target writes for the automatic float"
        );
    } else {
        panic!("admission plans, got {reply:?}");
    }
    let report = engine.last_fixed_admission().expect("bounded report");
    assert_eq!((report.reason, report.admitted), ("fixed-equal", 1));
}

#[test]
fn first_exit_with_converted_current_hints_and_predicate() {
    // D5 through production conversion: the exit classifies with the
    // hints and predicate current at exit, not at birth. Birth holds
    // slotless; a predicate flip plus hint change mid-hold selects the
    // exit layer; a fixed exit floats untouched.
    let (mut engine, owner, generation, domain, bounds) = rig();
    let held = FixedAdmissionFlags {
        fullscreen: true,
        fixed_auto: false,
        fixed_suppress: false,
    };
    // Born one-axis-fixed under the default: held, not classified.
    let birth_hints = converted(Some((640, 480)), Some((640, 960)));
    let rows = vec![
        (WindowId("e".to_owned()), bounds, birth_hints, true, held),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-1",
    );
    assert!(is_float(&engine, &domain, "e"));
    assert!(!is_automatic(&engine, &domain, "e"));
    // Mid-hold the predicate flips to either-axis: the first exit now
    // sees a fixed window and floats it automatic.
    engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    let exit_hints = converted(Some((640, 480)), Some((640, 960)));
    let rows = vec![
        (
            WindowId("e".to_owned()),
            bounds,
            exit_hints,
            false,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "exit-1",
    );
    assert!(
        is_automatic(&engine, &domain, "e"),
        "exit uses the current predicate, not the birth one"
    );
    // Reverse: hints lose fixedness before exit (ordinary at exit).
    let (mut engine, owner, generation, domain, bounds) = rig();
    let rows = vec![
        (
            WindowId("e".to_owned()),
            bounds,
            converted(Some((640, 480)), Some((640, 480))),
            true,
            held,
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "reconcile-1",
    );
    let rows = vec![
        (
            WindowId("e".to_owned()),
            bounds,
            converted(Some((640, 480)), Some((800, 600))),
            false,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "exit-1",
    );
    assert!(
        is_tiled(&engine, &domain, "e"),
        "hint loss before exit tiles (admission-only, current hints)"
    );
}

#[test]
fn suppress_on_non_fixed_window_is_behavior_neutral() {
    // Verified inert-suppress evidence (D3/D7 scope): stamping an
    // explicit tile win on a window whose hints never classify fixed
    // changes no outcome across fresh admission, release, and enable.
    // Core records an override on every unfloat; the marker mirrors it,
    // and neutrality here is why mirroring is safe.
    let run = |suppress: bool| {
        let (mut engine, owner, generation, domain, bounds) = rig();
        let flags = FixedAdmissionFlags {
            fullscreen: false,
            fixed_auto: false,
            fixed_suppress: suppress,
        };
        let rows = vec![
            (
                WindowId("w".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
                flags,
            ),
            plain_row("w1", bounds),
        ];
        let first = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        let tiled_first = is_tiled(&engine, &domain, "w");
        let correlation = CorrelationId::parse("release-1").expect("correlation");
        let mut release = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            11,
            &domain.0,
            &domain.1,
            8,
            &[
                (
                    WindowId("w".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        assert!(matches!(engine.handle(&release), CoreReply::Released));
        let second = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-2",
        );
        let tiled_second = is_tiled(&engine, &domain, "w");
        let auto_second = is_automatic(&engine, &domain, "w");
        (
            std::mem::discriminant(&first),
            std::mem::discriminant(&second),
            tiled_first,
            tiled_second,
            auto_second,
        )
    };
    let plain = run(false);
    let suppressed = run(true);
    // Identical reply shapes, identical tiled outcomes, and no
    // automatic mark in either leg: the signal is inert here.
    assert_eq!(
        plain, suppressed,
        "suppress changes nothing for non-fixed hints"
    );
    assert!(plain.2 && plain.3 && !plain.4);
}

#[test]
fn override_survives_hint_loss_at_enable() {
    // D3/D7 durability: an explicit tile win is hint-independent. After
    // the hints are lost entirely, the stamped suppress signal still
    // tiles the window at enable instead of floating or dropping it.
    // (A fixed window with lost hints and no suppress tiles too, but
    // only the suppress preserves the override identity.)
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            fixed_hints(),
            true,
            FixedAdmissionFlags::default(),
        ),
        plain_row("w1", bounds),
    ];
    toggle_float(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "wfix",
        "wfix",
        "unfloat-1",
        None,
    );
    assert!(has_override(&engine, &domain, "wfix"));
    let correlation = CorrelationId::parse("release-1").expect("correlation");
    let mut release = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        11,
        &domain.0,
        &domain.1,
        8,
        &[
            (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
            (
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    release.command = CoreCommand::ReleaseDomain;
    assert!(matches!(engine.handle(&release), CoreReply::Released));
    // Enable with the hints lost AND the suppress signal: still tiled.
    let suppress = FixedAdmissionFlags {
        fullscreen: false,
        fixed_auto: false,
        fixed_suppress: true,
    };
    let rows = vec![
        (
            WindowId("wfix".to_owned()),
            bounds,
            WindowSizeHints::none(),
            false,
            suppress,
        ),
        plain_row("w1", bounds),
    ];
    let reply = reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "enable-1",
    );
    assert!(
        !matches!(
            reply,
            CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
        ),
        "enable converges, got {reply:?}"
    );
    assert!(
        is_tiled(&engine, &domain, "wfix"),
        "override persists through hint loss"
    );
}

#[test]
fn fresh_hintless_admission_tiles_fail_closed() {
    // Unknown hints never classify: a fresh window with no usable hints
    // tiles, even under either-axis-fixed. Fail-closed evidence for the
    // enable path when a query fails on the classification tick.
    for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
        let (mut engine, owner, generation, domain, bounds) = rig();
        engine.set_fixed_size_predicate(predicate);
        let rows = vec![
            (
                WindowId("wnew".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(
            is_tiled(&engine, &domain, "wnew"),
            "hintless fresh admission tiles under {predicate:?}"
        );
        assert!(engine.last_fixed_admission().is_none());
    }
}

#[test]
fn enable_admits_brand_new_fixed_arrival_automatic() {
    // D6 every-candidate recheck includes arrivals: a fixed window first
    // seen on the enable observation floats automatic like any other
    // fresh fixed admission.
    let (mut engine, owner, generation, domain, bounds) = rig();
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &[plain_row("w1", bounds)],
        "w1",
        "reconcile-1",
    );
    let correlation = CorrelationId::parse("release-1").expect("correlation");
    let mut release = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        11,
        &domain.0,
        &domain.1,
        8,
        &[(
            WindowId("w1".to_owned()),
            bounds,
            WindowSizeHints::none(),
            false,
        )],
        Some(&WindowId("w1".to_owned())),
    );
    release.command = CoreCommand::ReleaseDomain;
    assert!(matches!(engine.handle(&release), CoreReply::Released));
    let rows = vec![fixed_row("warr", bounds), plain_row("w1", bounds)];
    reconcile(
        &mut engine,
        &owner,
        &generation,
        &domain,
        &rows,
        "w1",
        "enable-1",
    );
    assert!(
        is_automatic(&engine, &domain, "warr"),
        "fixed arrival at enable floats automatic"
    );
}

mod directional_foundation {
    use super::*;
    use tiler_core::directional::Direction;

    fn dkey(hwnd: u64) -> WindowKey {
        WindowKey {
            hwnd,
            pid: 1000 + hwnd as u32,
            creation: format!("c{hwnd:016x}"),
        }
    }

    fn full(id: &str, x: i32, y: i32, w: i32, h: i32) -> DirectionalOutput {
        DirectionalOutput {
            id: id.to_owned(),
            full: Rect { x, y, w, h },
        }
    }

    fn mover(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn primary_left_right_round_trip_selects_reciprocal_pair() {
        let outputs = vec![
            full("primary", 0, 0, 1920, 1080),
            full("right", 1920, 0, 1920, 1080),
        ];
        let window = mover(1600, 400, 200, 200);
        assert_eq!(
            select_directional_target(Direction::Right, "primary", &window, &outputs)
                .expect("readable"),
            Some("right".to_owned())
        );
        let back = mover(2000, 400, 200, 200);
        assert_eq!(
            select_directional_target(Direction::Left, "right", &back, &outputs).expect("readable"),
            Some("primary".to_owned())
        );
    }

    #[test]
    fn other_compass_and_negative_coords_select_without_wrap() {
        let outputs = vec![
            full("south", 0, 0, 1920, 1080),
            full("north", 0, -1080, 1920, 1080),
            full("west", -1920, -1080, 1920, 1080),
        ];
        let window = mover(800, -200, 200, 200);
        assert_eq!(
            select_directional_target(Direction::Up, "south", &window, &outputs).expect("readable"),
            Some("north".to_owned())
        );
        assert_eq!(
            select_directional_target(Direction::Left, "north", &window, &outputs)
                .expect("readable"),
            Some("west".to_owned())
        );
        // No candidate is a local no-op, never a wrap.
        let lonely = vec![full("solo", 0, 0, 1920, 1080)];
        assert_eq!(
            select_directional_target(Direction::Right, "solo", &window, &lonely)
                .expect("readable"),
            None
        );
    }

    #[test]
    fn unreadable_topology_and_unknown_source_refuse() {
        let outputs = vec![full("a", 0, 0, 1920, 1080), full("bad", 1920, 0, 0, 1080)];
        let window = mover(1600, 400, 200, 200);
        assert!(select_directional_target(Direction::Right, "a", &window, &outputs).is_err());
        let outputs = vec![full("a", 0, 0, 1920, 1080)];
        assert!(select_directional_target(Direction::Right, "ghost", &window, &outputs).is_err());
    }

    #[test]
    fn centre_projection_then_span_then_left_top_ranking() {
        // Two stacked candidates on the right: the mover centre picks the
        // containing edge; otherwise the larger span wins; then left/top.
        let outputs = vec![
            full("src", 0, 0, 800, 1000),
            full("upper", 800, 0, 800, 500),
            full("lower", 800, 500, 800, 500),
        ];
        let centred_upper = mover(100, 100, 200, 100);
        assert_eq!(
            select_directional_target(Direction::Right, "src", &centred_upper, &outputs)
                .expect("readable"),
            Some("upper".to_owned())
        );
        let centred_lower = mover(100, 700, 200, 100);
        assert_eq!(
            select_directional_target(Direction::Right, "src", &centred_lower, &outputs)
                .expect("readable"),
            Some("lower".to_owned())
        );
    }

    #[test]
    fn all_four_directions_resolve_stacked_candidates() {
        // Right/Left use stacked (vertical) candidates; Up/Down use
        // side-by-side (horizontal) candidates, including negative
        // origins. The mover centre projection picks each side.
        let horizontal = vec![
            full("src", 0, 0, 800, 1000),
            full("upper", 800, 0, 800, 500),
            full("lower", 800, 500, 800, 500),
        ];
        assert_eq!(
            select_directional_target(
                Direction::Left,
                "upper",
                &mover(900, 100, 200, 100),
                &horizontal
            )
            .expect("readable"),
            Some("src".to_owned())
        );
        let vertical = vec![
            full("src", 0, 0, 1000, 800),
            full("west", 0, 800, 400, 800),
            full("east", 400, 800, 600, 800),
        ];
        assert_eq!(
            select_directional_target(
                Direction::Down,
                "src",
                &mover(100, 100, 100, 200),
                &vertical
            )
            .expect("readable"),
            Some("west".to_owned())
        );
        assert_eq!(
            select_directional_target(
                Direction::Down,
                "src",
                &mover(700, 100, 100, 200),
                &vertical
            )
            .expect("readable"),
            Some("east".to_owned())
        );
        assert_eq!(
            select_directional_target(Direction::Up, "east", &mover(500, 900, 100, 200), &vertical)
                .expect("readable"),
            Some("src".to_owned())
        );
        // Negative stacked origins behave alike: no wrap, centre wins.
        let negative = vec![
            full("src", -800, -1000, 800, 1000),
            full("above", -800, -2000, 800, 1000),
        ];
        assert_eq!(
            select_directional_target(
                Direction::Up,
                "src",
                &mover(-700, -1400, 200, 100),
                &negative
            )
            .expect("readable"),
            Some("above".to_owned())
        );
    }

    #[test]
    fn span_overlap_fallback_then_left_top_then_identity() {
        // Gapped candidates on the right: mover centres in the gap miss
        // both shared edges, so the larger window-span overlap wins.
        let outputs = vec![
            full("src", 0, 0, 800, 1000),
            full("upper", 800, 0, 800, 300),
            full("lower", 800, 700, 800, 300),
        ];
        let favour_upper = mover(100, 100, 200, 700);
        assert_eq!(
            select_directional_target(Direction::Right, "src", &favour_upper, &outputs)
                .expect("readable"),
            Some("upper".to_owned())
        );
        let favour_lower = mover(100, 300, 200, 500);
        assert_eq!(
            select_directional_target(Direction::Right, "src", &favour_lower, &outputs)
                .expect("readable"),
            Some("lower".to_owned())
        );
        // Zero overlap on both clears to left/top: the topmost wins.
        let clears_both = mover(100, 400, 200, 100);
        assert_eq!(
            select_directional_target(Direction::Right, "src", &clears_both, &outputs)
                .expect("readable"),
            Some("upper".to_owned())
        );
        // Mirrored twins tie on left/top; stable identity breaks it.
        let twins = vec![
            full("src", 0, 0, 800, 600),
            full("twin-b", 800, 0, 800, 600),
            full("twin-a", 800, 0, 800, 600),
        ];
        assert_eq!(
            select_directional_target(Direction::Right, "src", &mover(100, 100, 200, 200), &twins)
                .expect("readable"),
            Some("twin-a".to_owned())
        );
    }

    #[test]
    fn odd_extent_centre_boundary_is_exact() {
        // Half-open [start,end) containment in doubled coordinates: a
        // centre of exactly 499.5 stays inside [0,500), 500.5 does not.
        let outputs = vec![
            full("src", 0, 0, 800, 1000),
            full("upper", 800, 0, 800, 500),
            full("lower", 800, 500, 800, 500),
        ];
        assert_eq!(
            select_directional_target(
                Direction::Right,
                "src",
                &mover(100, 449, 200, 101),
                &outputs
            )
            .expect("readable"),
            Some("upper".to_owned())
        );
        assert_eq!(
            select_directional_target(
                Direction::Right,
                "src",
                &mover(100, 450, 200, 101),
                &outputs
            )
            .expect("readable"),
            Some("lower".to_owned())
        );
    }

    #[test]
    fn pair_attaches_reciprocal_adjacency_only() {
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-b", "ws-2", bounds, 8);
        let ((source_domain, _), (target_domain, _)) =
            directional_pair_with_adjacency(source, target, Direction::Right);
        assert_eq!(
            source_domain
                .adjacent
                .get(&Direction::Right)
                .map(|id| id.0.as_str()),
            Some("mon-b")
        );
        assert_eq!(
            target_domain
                .adjacent
                .get(&Direction::Left)
                .map(|id| id.0.as_str()),
            Some("mon-a")
        );
        assert_eq!(source_domain.adjacent.len(), 1);
        assert_eq!(target_domain.adjacent.len(), 1);
    }

    #[test]
    fn per_monitor_identities_stay_distinct_for_same_workspace_index() {
        // Same workspace id string on two outputs still yields distinct
        // domain keys: the output scope is part of the identity.
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let left = workspace_domain("mon-a", "ws-same", bounds, 8);
        let right = workspace_domain("mon-b", "ws-same", bounds, 8);
        assert_ne!(left.1, right.1);
        assert_ne!(left.0.key(), right.0.key());
    }

    fn fence_snapshot() -> DirectionalFenceSnapshot {
        DirectionalFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_output: "mon-a".to_owned(),
            source_workspace: "ws-1".to_owned(),
            target_output: "mon-b".to_owned(),
            target_workspace: "ws-2".to_owned(),
            source_tiled: true,
            target_tiled: true,
            direction: Direction::Right,
            overlay_maximized: false,
            overlay_fullscreen: false,
            source_full: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            target_full: Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            source_bounds: Rect {
                x: 8,
                y: 8,
                w: 1904,
                h: 1064,
            },
            target_bounds: Rect {
                x: 1928,
                y: 8,
                w: 1904,
                h: 1064,
            },
            owner: "owner".to_owned(),
            generation: "gen".to_owned(),
        }
    }

    fn fence_live() -> DirectionalFenceLive {
        DirectionalFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            source_is_active: true,
            target_is_active: true,
            topology_ok: true,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
            owner_ok: true,
        }
    }

    #[test]
    fn fences_hold_when_fresh_and_refuse_each_drift() {
        let snapshot = fence_snapshot();
        assert_eq!(directional_fences_hold(&snapshot, &fence_live()), None);
        let mut stale_view = fence_live();
        stale_view.target_is_active = false;
        assert_eq!(
            directional_fences_hold(&snapshot, &stale_view),
            Some("view-changed")
        );
        let mut stale_topology = fence_live();
        stale_topology.topology_ok = false;
        assert_eq!(
            directional_fences_hold(&snapshot, &stale_topology),
            Some("topology-changed")
        );
        let mut stale_lifetime = fence_live();
        stale_lifetime.lifetime_ok = false;
        assert_eq!(
            directional_fences_hold(&snapshot, &stale_lifetime),
            Some("identity-changed")
        );
        let mut stale_scope = fence_live();
        stale_scope.scope_ok = false;
        assert_eq!(
            directional_fences_hold(&snapshot, &stale_scope),
            Some("scope-changed")
        );
        let mut floating = fence_live();
        floating.target_tiled = false;
        assert_eq!(
            directional_fences_hold(&snapshot, &floating),
            Some("workspace-floating")
        );
        let mut fullscreen = fence_live();
        fullscreen.overlay_fullscreen = true;
        assert_eq!(
            directional_fences_hold(&snapshot, &fullscreen),
            Some("overlay-changed")
        );
    }

    #[test]
    fn output_send_fences_route_floating_but_refuse_mode_drift() {
        // Same pinned snapshot/live shapes as the directional gate, but a
        // floating boundary is a membership-only route: matching modes
        // hold even when floating, while drift still refuses.
        let mut snapshot = fence_snapshot();
        snapshot.target_tiled = false;
        let mut live = fence_live();
        live.target_tiled = false;
        assert_eq!(output_send_fences_hold(&snapshot, &live), None);
        let mut source_floating = fence_live();
        source_floating.source_tiled = false;
        let mut source_snapshot = fence_snapshot();
        source_snapshot.source_tiled = false;
        assert_eq!(
            output_send_fences_hold(&source_snapshot, &source_floating),
            None
        );
        // Mode drift on either side refuses instead of routing.
        let mut drifted = fence_live();
        drifted.target_tiled = true;
        assert_eq!(
            output_send_fences_hold(&snapshot, &drifted),
            Some("mode-changed")
        );
        // Every other arm matches the directional gate.
        let mut stale_view = fence_live();
        stale_view.target_tiled = false;
        stale_view.target_is_active = false;
        assert_eq!(
            output_send_fences_hold(&snapshot, &stale_view),
            Some("view-changed")
        );
        let mut stale_scope = fence_live();
        stale_scope.target_tiled = false;
        stale_scope.scope_ok = false;
        assert_eq!(
            output_send_fences_hold(&snapshot, &stale_scope),
            Some("scope-changed")
        );
    }

    #[test]
    fn transfer_requires_absence_source_and_presence_target() {
        let mover = dkey(7);
        let other = dkey(8);
        let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
        let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
        assert!(verify_directional_transfer(&mover, &source, &target));
        let bad_source: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
        assert!(!verify_directional_transfer(&mover, &bad_source, &target));
        assert!(!verify_directional_transfer(&mover, &source, &source));
    }

    #[test]
    fn focus_target_prefers_last_focus_then_visible_member() {
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        spaces.ensure_output("mon-b");
        let active_b = spaces.active_id("mon-b").expect("active");
        let (first, second) = (dkey(21), dkey(22));
        assert!(spaces.assign(first.clone(), "mon-b", &active_b, false));
        assert!(spaces.assign(second.clone(), "mon-b", &active_b, false));
        spaces.note_foreground(&second);
        let visible: BTreeSet<WindowKey> = [first.clone(), second.clone()].into_iter().collect();
        assert_eq!(
            directional_focus_target(&spaces, "mon-b", &active_b, &visible),
            Some(second.clone())
        );
        // Hidden last-focus falls back to a visible member, never a hidden one.
        spaces.set_hidden(&second, true);
        let visible: BTreeSet<WindowKey> = [first.clone()].into_iter().collect();
        assert_eq!(
            directional_focus_target(&spaces, "mon-b", &active_b, &visible),
            Some(first.clone())
        );
        // Empty target refuses.
        let empty: BTreeSet<WindowKey> = BTreeSet::new();
        assert_eq!(
            directional_focus_target(&spaces, "mon-b", &active_b, &empty),
            None
        );
    }

    #[test]
    fn focus_crosses_horizontally_only() {
        assert!(directional_focus_crosses(Direction::Left));
        assert!(directional_focus_crosses(Direction::Right));
        assert!(!directional_focus_crosses(Direction::Up));
        assert!(!directional_focus_crosses(Direction::Down));
    }
}

mod migration_tests {
    // Item 14/19 portable migration policy: DISTINCT `MigrateWorkspace`
    // event construction (SAME id, DIFFERENT output, empty target),
    // `MigrateWorkspace` reply extraction, whole-membership/both-view
    // verification, largest-shared-edge selection, and fence evaluation.
    // Real-Engine planning runs through the production builder with
    // retained nested topology below.
    use super::*;
    use tiler_core::directional::Direction;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn domains() -> ((OutputDomain, DomainKey), (OutputDomain, DomainKey)) {
        let source = workspace_domain("primary", "ws-2", rect(0, 40, 1920, 1040), 8);
        let target = workspace_domain("right", "ws-2", rect(1920, 0, 1920, 1080), 8);
        (source, target)
    }

    fn row(token: &str) -> MigrateRow {
        MigrateRow {
            token: token.to_owned(),
            rect: rect(100, 100, 400, 300),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            fit_excluded: false,
        }
    }

    fn ids() -> (OwnerId, GenerationId, CorrelationId) {
        (
            OwnerId::parse("owner-a").expect("owner"),
            GenerationId::parse("gen-1").expect("generation"),
            CorrelationId::parse("corr-migrate").expect("correlation"),
        )
    }

    #[test]
    fn migrate_event_carries_same_id_different_output_with_empty_target() {
        let (owner, generation, correlation) = ids();
        let (source, target) = domains();
        let rows = vec![row("a"), row("b")];
        let event = build_migrate_event(
            &owner,
            &generation,
            &correlation,
            7,
            9,
            source.clone(),
            target.clone(),
            &rows,
            "a",
            "right",
            8,
        )
        .expect("migrate event");
        assert!(matches!(
            event.command,
            CoreCommand::MigrateWorkspace { ref direction } if direction == "right"
        ));
        assert_eq!(event.domain_key.workspace.0, "ws-2");
        assert_eq!(
            event.target_domain.as_ref().expect("target").1.workspace.0,
            "ws-2"
        );
        assert_eq!(
            event.target_domain.as_ref().expect("target").1.output.0,
            "right"
        );
        assert!(
            event.target_windows.is_empty(),
            "migration target carries no observation"
        );
        assert_eq!(event.focused_window.0, "a");
        assert_eq!(event.windows.len(), 2);
        // Empty focus migrates without fabricated focus.
        let empty = build_migrate_event(
            &owner,
            &generation,
            &correlation,
            7,
            9,
            source,
            target,
            &rows,
            "",
            "right",
            8,
        )
        .expect("empty focus event");
        assert!(empty.focused_window.0.is_empty());
    }

    #[test]
    fn migrate_event_refuses_split_id_same_output_and_dangling_focus() {
        let (owner, generation, correlation) = ids();
        let (source, target) = domains();
        let rows = vec![row("a")];
        // SAME output refuses.
        let same = workspace_domain("primary", "ws-2", rect(0, 40, 1920, 1040), 8);
        assert!(
            build_migrate_event(
                &owner,
                &generation,
                &correlation,
                7,
                9,
                source.clone(),
                same,
                &rows,
                "a",
                "right",
                8,
            )
            .is_none()
        );
        // DIFFERENT workspace id refuses (destination current is never
        // carried as the target).
        let split = workspace_domain("right", "ws-9", rect(1920, 0, 1920, 1080), 8);
        assert!(
            build_migrate_event(
                &owner,
                &generation,
                &correlation,
                7,
                9,
                source.clone(),
                split,
                &rows,
                "a",
                "right",
                8,
            )
            .is_none()
        );
        // Dangling focus (named but not carried) refuses.
        assert!(
            build_migrate_event(
                &owner,
                &generation,
                &correlation,
                7,
                9,
                source,
                target,
                &rows,
                "ghost",
                "right",
                8,
            )
            .is_none()
        );
    }

    #[test]
    fn migrate_reply_extracts_planned_geometry() {
        use tiler_core::boundary::{CoreReply, MigrateWorkspacePlan};
        use tiler_core::session::DomainKey as Key;
        let plan = MigrateWorkspacePlan {
            base_revision: 7,
            direction: Direction::Right,
            source: Key {
                output: OutputId("primary".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
            },
            target: Key {
                output: OutputId("right".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
            },
            geometry: vec![tiler_core::session::DesiredGeometry {
                window: WindowId("a".to_owned()),
                leaf: tiler_core::directional::NodeId("n0".to_owned()),
                output: OutputId("right".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                rect: rect(1920, 0, 960, 1040),
                overconstrained: false,
                client_clamped: false,
            }],
            focus_domain: None,
            focus_leaf: None,
            active_window: Some(WindowId("a".to_owned())),
            members: 1,
            floats: 0,
            preconditions: Vec::new(),
        };
        let writes = planned_writes(&CoreReply::MigrateWorkspace(plan)).expect("extracts");
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].window.0, "a");
        assert_eq!(writes[0].rect, rect(1920, 0, 960, 1040));
    }

    #[test]
    fn migration_transfer_verifies_members_and_both_views() {
        use crate::workspace::WindowKey as Key;
        let k = |hwnd: u64| Key {
            hwnd,
            pid: 100,
            creation: format!("c{hwnd:016x}"),
        };
        let (a, b, s) = (k(1), k(2), k(3));
        let migrating: BTreeSet<Key> = [a.clone(), b.clone()].into_iter().collect();
        let sticky: BTreeSet<Key> = [s.clone()].into_iter().collect();
        let source: BTreeSet<Key> = BTreeSet::new();
        let target: BTreeSet<Key> = [a.clone(), b.clone()].into_iter().collect();
        assert!(verify_migration_transfer(
            &migrating,
            &sticky,
            &source,
            &target,
            "ws-2",
            Some("ws-1"),
            Some("ws-1"),
            Some("ws-2"),
        ));
        // Sticky in the target set never verifies.
        let target_bad: BTreeSet<Key> = [a.clone(), b.clone(), s.clone()].into_iter().collect();
        assert!(!verify_migration_transfer(
            &migrating,
            &sticky,
            &source,
            &target_bad,
            "ws-2",
            Some("ws-1"),
            Some("ws-1"),
            Some("ws-2"),
        ));
        // Missing member never verifies.
        let target_short: BTreeSet<Key> = [a.clone()].into_iter().collect();
        assert!(!verify_migration_transfer(
            &migrating,
            &sticky,
            &source,
            &target_short,
            "ws-2",
            Some("ws-1"),
            Some("ws-1"),
            Some("ws-2"),
        ));
        // Wrong views never verify.
        assert!(!verify_migration_transfer(
            &migrating,
            &sticky,
            &source,
            &target,
            "ws-2",
            Some("ws-1"),
            Some("ws-9"),
            Some("ws-2"),
        ));
        assert!(!verify_migration_transfer(
            &migrating,
            &sticky,
            &source,
            &target,
            "ws-2",
            Some("ws-1"),
            Some("ws-1"),
            Some("ws-9"),
        ));
    }

    #[test]
    fn migration_selection_uses_largest_edge_never_window_position() {
        let outputs = vec![
            DirectionalOutput {
                id: "src".to_owned(),
                full: rect(0, 0, 800, 800),
            },
            DirectionalOutput {
                id: "wide".to_owned(),
                full: rect(800, 0, 800, 600),
            },
            DirectionalOutput {
                id: "narrow".to_owned(),
                full: rect(800, 600, 800, 200),
            },
        ];
        assert_eq!(
            select_migration_target(Direction::Right, "src", &outputs).expect("readable"),
            Some("wide".to_owned())
        );
        // Equal edges tie by left/top.
        let tied = vec![
            DirectionalOutput {
                id: "src".to_owned(),
                full: rect(0, 0, 800, 800),
            },
            DirectionalOutput {
                id: "low".to_owned(),
                full: rect(800, 400, 800, 400),
            },
            DirectionalOutput {
                id: "high".to_owned(),
                full: rect(800, 0, 800, 400),
            },
        ];
        assert_eq!(
            select_migration_target(Direction::Right, "src", &tied).expect("readable"),
            Some("high".to_owned())
        );
        // No candidate is a no-op; unreadable refuses; unknown source refuses.
        let far = vec![
            DirectionalOutput {
                id: "src".to_owned(),
                full: rect(0, 0, 800, 800),
            },
            DirectionalOutput {
                id: "far".to_owned(),
                full: rect(2000, 0, 800, 800),
            },
        ];
        assert_eq!(
            select_migration_target(Direction::Right, "src", &far).expect("readable"),
            None
        );
        let mut bad = tied.clone();
        bad[1].full.w = 0;
        assert!(select_migration_target(Direction::Right, "src", &bad).is_err());
        assert!(select_migration_target(Direction::Right, "ghost", &tied).is_err());
    }

    #[test]
    fn migration_fences_distinguish_currency_failures() {
        let snap = MigrateFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_output: "primary".to_owned(),
            target_output: "right".to_owned(),
            moved_workspace: "ws-2".to_owned(),
            source_refill: None,
            target_prior: "ws-3".to_owned(),
            source_tiled: true,
            target_tiled: true,
            direction: Direction::Right,
            source_full: rect(0, 0, 1920, 1080),
            target_full: rect(1920, 0, 1920, 1080),
            source_bounds: rect(0, 40, 1920, 1040),
            target_bounds: rect(1920, 0, 1920, 1080),
            owner: "owner-a".to_owned(),
            generation: "gen-1".to_owned(),
            member_count: 2,
        };
        let live = MigrateFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_active: Some("ws-2".to_owned()),
            target_active: Some("ws-3".to_owned()),
            source_tiled: true,
            target_tiled: true,
            topology_ok: true,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            views_ok: true,
            owner_ok: true,
        };
        assert_eq!(migrate_fences_hold(&snap, &live), None);
        let mut drifted = live.clone();
        drifted.topology_ok = false;
        assert_eq!(
            migrate_fences_hold(&snap, &drifted),
            Some("topology-changed")
        );
        let mut drifted = live.clone();
        drifted.views_ok = false;
        assert_eq!(migrate_fences_hold(&snap, &drifted), Some("view-changed"));
        let mut drifted = live.clone();
        drifted.membership_ok = false;
        assert_eq!(migrate_fences_hold(&snap, &drifted), Some("unverified"));
        let mut drifted = live.clone();
        drifted.lifetime_ok = false;
        assert_eq!(
            migrate_fences_hold(&snap, &drifted),
            Some("identity-changed")
        );
        let mut drifted = live.clone();
        drifted.scope_ok = false;
        assert_eq!(migrate_fences_hold(&snap, &drifted), Some("scope-excluded"));
        let mut drifted = live.clone();
        drifted.source_tiled = false;
        assert_eq!(migrate_fences_hold(&snap, &drifted), Some("mode-changed"));
        let mut drifted = live.clone();
        drifted.owner_ok = false;
        assert_eq!(migrate_fences_hold(&snap, &drifted), Some("owner-changed"));
    }

    #[test]
    fn real_engine_migrate_preserves_nested_shares_focus_and_floats() {
        use tiler_core::boundary::{CoreCommand, CoreReply};
        use tiler_core::contract::Observation;
        use tiler_core::directional::{Axis, Node, SameAxisMove};
        use tiler_core::seed::{EngineWindow, seed_session};
        use tiler_core::session::{ObservedWindow, SessionObservation};
        let (owner, generation, correlation) = ids();
        let mut engine = tiler_core::engine::Engine::new();
        engine.sync_binding(&owner, &generation);
        let (source, target) = domains();
        let seed_order = ["win-a", "win-b", "win-c", "win-d"]
            .iter()
            .enumerate()
            .map(|(i, window)| EngineWindow {
                window: WindowId(window.to_string()),
                output: tiler_core::directional::OutputId("primary".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                rect: rect((i as i32) * 100, 0, 100, 100),
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            })
            .collect::<Vec<_>>();
        let seeded =
            seed_session(&owner, &generation, 7, &source.0, &seed_order).expect("seeds four");
        engine.store_committed(source.1.clone(), seeded, 8);
        // Float win-d first so the tiled tree stays the flat triple for
        // the nesting move below (a floated nested member would dissolve
        // its group instead). Always carries all four windows so later
        // converges never drop the slotless exception.
        let float_d = |engine: &mut tiler_core::engine::Engine, focus: &str| {
            let session = engine.session(&source.1).expect("session").clone();
            let base = session.accepted_revision();
            let obs: Vec<ObservedWindow> = ["win-a", "win-b", "win-c", "win-d"]
                .iter()
                .map(|w| ObservedWindow {
                    window: WindowId(w.to_string()),
                    output: tiler_core::directional::OutputId("primary".to_owned()),
                    workspace: WorkspaceId("ws-2".to_owned()),
                    floating: *w == "win-d",
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                })
                .collect();
            let mut session = session;
            session
                .converge_observation(
                    &SessionObservation {
                        observation: Observation::new(owner.clone(), generation.clone(), base, 7),
                        windows: obs,
                    },
                    Some(&WindowId(focus.to_owned())),
                )
                .expect("float converges");
            session
        };
        let op_event = |focused: &str,
                        command: CoreCommand,
                        correlation: &str|
         -> tiler_core::boundary::CoreEvent {
            tiler_core::boundary::CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("valid"),
                revision: 0,
                fingerprint: 7,
                domain: source.0.clone(),
                domain_key: source.1.clone(),
                outer_gap: 8,
                focused_window: WindowId(focused.to_owned()),
                windows: seed_order.clone(),
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: vec![],
                closed_windows: vec![],
                command,
            }
        };
        let floated = float_d(&mut engine, "win-b");
        engine.store_committed(source.1.clone(), floated, 8);
        // Nest win-b under win-c: the perpendicular move restructures the
        // flat triple into H[win-a, V[win-c, win-b]] with focus on B.
        match engine.handle(&op_event(
            "win-b",
            CoreCommand::Move {
                window: "win-b".to_owned(),
                direction: "down".to_owned(),
                cross_output_transfer: false,
                same_axis_move: SameAxisMove::GroupWithNeighbor,
            },
            "corr-nest",
        )) {
            CoreReply::MoveDirectional(_) => {}
            other => panic!("nesting move must plan, got {other:?}"),
        }
        // Unbalance the root shares with a keyboard resize of win-a.
        match engine.handle(&op_event(
            "win-a",
            CoreCommand::Resize {
                window: "win-a".to_owned(),
                direction: "right".to_owned(),
                mode: "outwards".to_owned(),
                press_index: 0,
            },
            "corr-resize",
        )) {
            CoreReply::Resize(_) => {}
            other => panic!("share resize must plan, got {other:?}"),
        }
        // Re-sync remembered focus to B without touching the tree.
        let refocused = float_d(&mut engine, "win-b");
        engine.store_committed(source.1.clone(), refocused.clone(), 8);
        let nested = refocused;
        // Assert the actual nested unequal-share precondition explicitly:
        // H[win-a, V[win-b, win-c]] with unequal root shares, focus B.
        let pre_tree = nested.tree_for(&source.1).cloned().expect("nested tree");
        match &pre_tree {
            Node::Group {
                axis: Axis::Horizontal,
                children,
                shares,
                id: _,
            } => {
                assert_eq!(children.len(), 2, "root pairs A against one group");
                assert!(
                    matches!(&children[0], Node::Leaf { id } if id.0 == "leaf-win-a"),
                    "first root child is A, got {:?}",
                    children[0]
                );
                assert_ne!(
                    shares[0], shares[1],
                    "root shares unequal after resize, got {shares:?}"
                );
                match &children[1] {
                    Node::Group {
                        axis: Axis::Vertical,
                        children: inner,
                        shares: vshares,
                        id: _,
                    } => {
                        let ids: Vec<&str> = inner
                            .iter()
                            .map(|n| match n {
                                Node::Leaf { id } => id.0.as_str(),
                                _ => "?",
                            })
                            .collect();
                        assert_eq!(ids, vec!["leaf-win-b", "leaf-win-c"]);
                        assert_eq!(vshares.len(), 2);
                    }
                    other => panic!("second root child is the V group, got {other:?}"),
                }
            }
            other => panic!("nested H root, got {other:?}"),
        }
        let (pre_focus_domain, pre_focus_leaf) = nested.focus();
        assert_eq!(pre_focus_domain, Some(source.1.clone()));
        assert_eq!(
            pre_focus_leaf.map(|l| l.0.clone()),
            Some("leaf-win-b".to_owned()),
            "remembered focus is B"
        );
        assert!(
            nested.is_exception(&WindowId("win-d".to_owned())),
            "D floats"
        );
        let pre_float_geometry = nested.floating_geometry(&WindowId("win-d".to_owned()));
        let pre_revision = nested.accepted_revision();
        engine.store_committed(source.1.clone(), nested, 8);
        // Production builder carries the retained set: three tiled
        // members plain, the float exception with floating plus
        // fit-excluded.
        let plain = |token: &str| MigrateRow {
            token: token.to_owned(),
            rect: rect(0, 0, 10, 10),
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            fit_excluded: false,
        };
        let rows = vec![
            plain("win-a"),
            plain("win-b"),
            plain("win-c"),
            MigrateRow {
                token: "win-d".to_owned(),
                rect: rect(0, 0, 10, 10),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                fit_excluded: true,
            },
        ];
        let event = build_migrate_event(
            &owner,
            &generation,
            &correlation,
            0,
            7,
            source.clone(),
            target.clone(),
            &rows,
            "win-b",
            "right",
            8,
        )
        .expect("migrate event");
        assert!(event.target_windows.is_empty());
        let reply = engine.handle(&event);
        let plan = match reply {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("migrate must plan, got {other:?}"),
        };
        // Actual reply extraction through the production helper.
        let writes =
            planned_writes(&CoreReply::MigrateWorkspace(plan.clone())).expect("plan extracts");
        assert_eq!(plan.members, 4, "complete member set relocates");
        assert_eq!(plan.floats, 1, "float exception carried");
        assert_eq!(plan.active_window, Some(WindowId("win-b".to_owned())));
        assert_eq!(plan.source.workspace.0, "ws-2");
        assert_eq!(plan.target.workspace.0, "ws-2");
        assert_eq!(plan.target.output.0, "right");
        assert_eq!(
            plan.base_revision, pre_revision,
            "revision binding preserved"
        );
        assert!(!writes.is_empty(), "retained geometry extracts");
        // Retained nested tree, unequal shares, remembered focus, float
        // class/origin, and tiled slot count survive the rekey.
        let moved = engine.session(&plan.target).expect("relocated session");
        assert_eq!(
            moved.tree_for(&plan.target),
            Some(&pre_tree),
            "nested unequal-share tree preserved"
        );
        let (focus_domain, focus_leaf) = moved.focus();
        assert_eq!(focus_domain, Some(plan.target.clone()));
        assert_eq!(
            focus_leaf.map(|l| l.0.clone()),
            Some("leaf-win-b".to_owned()),
            "remembered B focus retained"
        );
        assert!(
            moved.is_exception(&WindowId("win-d".to_owned())),
            "D stays float"
        );
        assert_eq!(
            moved.floating_geometry(&WindowId("win-d".to_owned())),
            pre_float_geometry,
            "float origin geometry retained"
        );
        assert_eq!(moved.snapshot().windows.len(), 3, "three tiled slots kept");
        assert_eq!(moved.exception_count(), 1, "one slotless exception kept");
        assert!(
            engine.session(&plan.source).is_none(),
            "source slot released"
        );
        let _ = CoreCommand::Reconcile;
    }
}
