//! Core-owned focus-follows-cursor and cursor-warp (stage 4b).
//!
//! The adapter reports one cursor sample plus the normalized cursor-hit
//! managed lifetime per report; the core owns every focus/warp decision.
//! The hit is the only pointer window: the core never guesses stacking by
//! rectangle overlap or token order, so hidden snapshots, non-current
//! views, foreign pixels, and genuine overlaps cannot win. Eligibility
//! still requires the core's current logical view plus visible staged
//! membership. Both lanes pause under a focused fullscreen game,
//! incomplete inventory, and gesture barriers.

use std::collections::BTreeSet;

use crate::directional::WindowId;
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{EffectKind, OneShotEffect, WorldReport};

/// Whether one report runs under a focused fullscreen game pause: the
/// adapter-observed focused lifetime names a staged fact that is fullscreen.
/// Normalized report facts only. Under the pause no focus-follows effect
/// mints and no warp point publishes.
pub(crate) fn cursor_game_paused(engine: &WorldEngine, report: &WorldReport) -> bool {
    if report.focused_global.is_empty() {
        return false;
    }
    let focused = WindowId(report.focused_global.clone());
    engine
        .staged
        .facts
        .get(&focused)
        .is_some_and(|fact| fact.fullscreen)
}

/// Whether one staged window is focus-eligible for hover coupling: homed to
/// a readable unheld non-floating scope, with no unexplained fit exclusion
/// (ordinary floats stay eligible).
fn hover_eligible(
    engine: &WorldEngine,
    held_scopes: &BTreeSet<DomainKey>,
    home: &DomainKey,
    window: &WindowId,
) -> bool {
    if held_scopes.contains(home) {
        return false;
    }
    if engine.floating_scopes.contains(home) {
        return false;
    }
    if !engine
        .staged
        .domains
        .iter()
        .any(|domain| domain.key == *home && domain.readable)
    {
        return false;
    }
    let Some(fact) = engine.staged.facts.get(window) else {
        return false;
    };
    if fact.minimized {
        return false;
    }
    // Unexplained fit-excluded newcomers never take automatic focus
    // (same rule as admission intent): ordinary floats stay eligible.
    if fact.fit_excluded && !fact.floating && !fact.fullscreen && !fact.maximized {
        return false;
    }
    true
}

/// Pointer window for one report: the normalized cursor-hit managed
/// lifetime, gated by the core's own eligibility and current view. The hit
/// is adapter-observed native truth (which topmost managed window is
/// actually under the cursor); the core never re-derives it from
/// rectangles, so hidden snapshots, non-current views, foreign pixels,
/// and overlapping floats cannot win by geometry or token order. An
/// absent hit, a hit outside visible staged membership of the retained
/// view, or an ineligible hit mints nothing.
fn hover_candidate(
    engine: &WorldEngine,
    report: &WorldReport,
    held_scopes: &BTreeSet<DomainKey>,
) -> Option<WindowId> {
    let hit = report.cursor_hit.clone()?;
    let fact = engine.staged.facts.get(&hit)?;
    if fact.minimized {
        return None;
    }
    let home = engine.staged.homing.get(&hit)?.clone();
    // Current-view gate: only the retained logical view of the hit
    // window's output is actually shown.
    if engine
        .views
        .get(&home.output.0)
        .is_none_or(|view| *view != home.workspace.0)
    {
        return None;
    }
    if !hover_eligible(engine, held_scopes, &home, &hit) {
        return None;
    }
    // Visible staged membership: the hit must ride the tiled carriers or
    // the float exceptions of its home domain. Minimized, unreadable, and
    // unhomed lifetimes never match, whatever the adapter reported.
    let domain = engine
        .staged
        .domains
        .iter()
        .find(|domain| domain.key == home)?;
    let tiled = domain.windows.iter().any(|window| window.window == hit);
    let floated = engine.engine.session(&home).is_some_and(|session| {
        session.exception_observed().iter().any(|observed| {
            observed.window == hit
                && observed.output == home.output
                && observed.workspace == home.workspace
        })
    });
    if tiled || floated { Some(hit) } else { None }
}

/// Push one focus-follows-cursor effect unless a stronger focus effect was
/// already minted this report (fresh admission or an explicit command
/// focus). Runs after command execution on the final staged state, so hover
/// resolves against committed rings/views.
pub(crate) fn maybe_push_focus_follows(
    engine: &mut WorldEngine,
    report: &WorldReport,
    held_scopes: &BTreeSet<DomainKey>,
    effects: &mut Vec<OneShotEffect>,
) {
    if effects
        .iter()
        .any(|effect| effect.kind == EffectKind::Focus)
    {
        return;
    }
    if let Some(follow) = engine.take_focus_follows(report, held_scopes) {
        effects.push(follow);
    }
}

impl WorldEngine {
    /// One focus-follows-cursor effect for this report, if any. Mints only
    /// when the setting is on, the inventory is complete, the normalized
    /// cursor hit names an eligible hover target, no game/gesture pause
    /// holds, no stronger focus effect was minted this report (the caller
    /// gates on that), the hovered window differs from the observed focus,
    /// the cursor moved since the previous accepted report or the hover
    /// target transitioned, and the hover target is not the one already
    /// coupled. Records the hover target either way, so a static cursor
    /// never re-steals focus after a keyboard command moves focus
    /// elsewhere. The `ffc-<window>` id scopes at-most-once per hover
    /// target: the adapter spent fence drops repeats, and arrival (focus
    /// catching up to hover) stops minting.
    pub(crate) fn take_focus_follows(
        &mut self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<OneShotEffect> {
        if !report.focus_follows_cursor {
            return None;
        }
        if !report.inventory_complete {
            return None;
        }
        if cursor_game_paused(self, report) {
            return None;
        }
        if report.gesture_barrier || self.gesture.is_some() {
            return None;
        }
        let hovered = hover_candidate(self, report, held_scopes)?;
        let previous = self.ffc_last_hover.clone();
        self.ffc_last_hover = Some(hovered.clone());
        if hovered.0 == report.focused_global {
            return None;
        }
        // Genuine movement or hover transition only: a static cursor over
        // an unchanged hover never re-steals focus after a keyboard
        // command moves focus elsewhere.
        let moved = report.cursor != self.prev_cursor;
        let transitioned = previous.as_ref() != Some(&hovered);
        if !moved && !transitioned {
            return None;
        }
        Some(OneShotEffect {
            id: format!("ffc-{}", hovered.0),
            kind: EffectKind::Focus,
            target: hovered,
            issued_revision: self.state_revision + 1,
        })
    }

    /// Desired cursor-warp point for this publish, if any (cross-output
    /// only). Mints only when the setting is on, the inventory is complete,
    /// the cursor reads with a known cursor output, no game/gesture pause
    /// holds, and the last focus effect in order (the focus that actually
    /// results, since the adapter applies effects in order) targets a
    /// different output than the cursor output: the focused target's
    /// effective-frame centre clamped into its work area. Same-output final
    /// focus and missing target frames never warp. Unknown, failed, or held
    /// focus resolves adapter-side: the point rides only alongside its
    /// effect, and the adapter warps only when that exact target holds the
    /// observed foreground.
    pub(crate) fn warp_for_publish(
        &self,
        report: &WorldReport,
        effects: &[OneShotEffect],
        windows: &[crate::world_report::WindowDesire],
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<(i32, i32)> {
        if !report.cursor_warp {
            return None;
        }
        if !report.inventory_complete {
            return None;
        }
        if cursor_game_paused(self, report) {
            return None;
        }
        if report.gesture_barrier || self.gesture.is_some() {
            return None;
        }
        let cursor = report.cursor?;
        let screens: Vec<(crate::directional::OutputId, crate::geometry::Rect)> = report
            .screens
            .iter()
            .filter_map(|screen| screen.full_rect.map(|rect| (screen.output.clone(), rect)))
            .collect();
        let cursor_output = crate::cursor_targeting::output_at_cursor(&screens, cursor)?;
        // The last focus effect in order dictates the resulting focus, so
        // it alone decides the warp: an earlier cross-output effect
        // followed by a same-output one warps nothing.
        let effect = effects
            .iter()
            .rfind(|effect| effect.kind == EffectKind::Focus)?;
        let row = windows.iter().find(|row| row.lifetime == effect.target)?;
        if row.output == cursor_output {
            return None;
        }
        if held_scopes.contains(&DomainKey {
            output: row.output.clone(),
            workspace: crate::directional::WorkspaceId(row.workspace.clone()),
        }) {
            return None;
        }
        let frame = row.effective_frame?;
        let centre = (
            frame.x.saturating_add(frame.w / 2),
            frame.y.saturating_add(frame.h / 2),
        );
        let clamped = report
            .screens
            .iter()
            .find(|screen| screen.output == row.output)
            .and_then(|screen| screen.work_area)
            .map(|area| clamp_point_into_area(centre, area))
            .unwrap_or(centre);
        if (-crate::bounds::GEOMETRY_BOUND..=crate::bounds::GEOMETRY_BOUND).contains(&clamped.0)
            && (-crate::bounds::GEOMETRY_BOUND..=crate::bounds::GEOMETRY_BOUND).contains(&clamped.1)
        {
            Some(clamped)
        } else {
            None
        }
    }
}

/// Constrain one warp point inside the destination work area: clamp each axis
/// into the area when the area has positive extent, else pin to the area
/// origin. Pure [`crate::geometry::Rect`] math.
fn clamp_point_into_area(point: (i32, i32), area: crate::geometry::Rect) -> (i32, i32) {
    if area.w <= 0 || area.h <= 0 {
        return (area.x, area.y);
    }
    (
        point
            .0
            .clamp(area.x, area.x.saturating_add(area.w.saturating_sub(1))),
        point
            .1
            .clamp(area.y, area.y.saturating_add(area.h.saturating_sub(1))),
    )
}
