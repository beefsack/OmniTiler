use super::*;

/// Portable `TileOptions` construction from settings. Every explicit CLI
/// switch later overrides its corresponding value (the parser starts from
/// this base), so saved values are defaults, never fences.
#[must_use]
pub fn tile_options_from_settings(settings: &Settings) -> crate::tiling::TileOptions {
    let core = &settings.core;
    crate::tiling::TileOptions {
        seconds: None,
        trace: false,
        user_start: true,
        no_keyboard_snap_takeover: !core.keyboard.takeover,
        allow_win_l: core.keyboard.allow_win_l,
        no_mouse_snap_prevention: !core.mouse.snap_prevention,
        border: crate::active_border::ActiveBorderOptions {
            enabled: core.border.enabled,
            style: crate::active_border::ActiveBorderStyle {
                width: core.border.width,
                gap: core.border.gap,
                radius: core.border.radius,
                color: crate::active_border::parse_color(&core.border.color)
                    .unwrap_or(crate::active_border::DEFAULT_COLOR_RGB),
                use_theme: core.border.use_theme,
            },
        },
        underlay: crate::group_underlay::GroupUnderlayOptions {
            enabled: core.underlay.enabled,
            style: crate::group_underlay::GroupUnderlayStyle {
                color: crate::group_underlay::parse_color_argb(&core.underlay.color)
                    .map(|(rgb, _)| rgb)
                    .unwrap_or((0x2a, 0x82, 0xda)),
                alpha: crate::group_underlay::parse_color_argb(&core.underlay.color)
                    .map(|(_, alpha)| alpha)
                    .unwrap_or(0x66),
                extension: core.underlay.extension,
            },
        },
        inner_gap: core.inner_gap,
        outer_gap: core.outer_gap,
        scope_exes: Vec::new(),
        scope_hosts: Vec::new(),
    }
}

/// True when the per-domain reconcile path may use `Reconcile`: either no
/// session is retained yet (fresh domains seed through `Reconcile`) or the
/// retained inner/outer gaps already equal the carried values. A retained
/// mismatch must issue `UpdateGaps` so topology survives: plain reconcile
/// refuses gap drift instead of adopting it.
#[must_use]
pub fn retained_gaps_match(
    engine: &tiler_core::engine::Engine,
    key: &tiler_core::session::DomainKey,
    gap: i32,
    outer_gap: i32,
) -> bool {
    let Some(session) = engine.session(key) else {
        return true;
    };
    let domain_matches = session
        .domains()
        .iter()
        .any(|domain| domain.key() == *key && domain.gap == gap);
    domain_matches && engine.outer_gap_matches(key, outer_gap)
}
