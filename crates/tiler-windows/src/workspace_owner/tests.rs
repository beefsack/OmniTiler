use super::*;

fn rect(x: i32, y: i32) -> Rect {
    Rect {
        x,
        y,
        w: 800,
        h: 600,
    }
}

fn key(hwnd: u64) -> WindowKey {
    WindowKey {
        hwnd,
        pid: 1000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

fn seed_two_tiled(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &(OutputDomain, DomainKey),
    bounds: Rect,
) {
    let correlation = CorrelationId::parse("seed").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        owner,
        generation,
        &correlation,
        0,
        2,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
        ],
        Some(&WindowId("w1".to_owned())),
    );
    let reply = engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge, got {reply:?}"
    );
}

#[allow(clippy::too_many_arguments)]
fn float_event(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &str,
    revision: u64,
    domain: &(OutputDomain, DomainKey),
    rows: &[(WindowId, Rect, bool)],
    focused: &str,
    window: &str,
    float_rect: Option<Rect>,
) -> tiler_core::boundary::CoreEvent {
    use tiler_core::boundary::CoreCommand;
    let correlation = CorrelationId::parse(correlation).expect("correlation");
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(token, rect, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rows
        .iter()
        .map(|(token, rect, floating)| {
            (
                token.clone(),
                *rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                *floating,
            )
        })
        .collect();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        owner,
        generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect,
    };
    event
}

fn revision_of(engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)) -> u64 {
    engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0)
}

/// Float/orientation toggles, hidden snapshots, and unfloat topology.
#[cfg(test)]
#[path = "tests_toggle.rs"]
mod toggle_tests;

/// Send/output builders, routes, observation rows, and dispatch gates.
#[cfg(test)]
#[path = "tests_send.rs"]
mod send_tests;

/// Size-hint propagation plus sticky/restart/tile-override markers.
#[cfg(test)]
#[path = "tests_hints_markers.rs"]
mod hints_markers_tests;

/// Send follow/stay, fullscreen carry, and send fence/overlay gates.
#[cfg(test)]
#[path = "tests_send_follow.rs"]
mod send_follow_tests;

/// Ordinary/combo floating sends with deferred Engine shape.
#[cfg(test)]
#[path = "tests_float_send.rs"]
mod float_send_tests;

/// Same-axis swap/wrap/neighbor layout operations.
#[cfg(test)]
#[path = "tests_layout.rs"]
mod layout_tests;
