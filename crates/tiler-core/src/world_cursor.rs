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
use crate::world_report::{EffectCause, EffectKind, OneShotEffect};

/// Whether the staged observation runs under a focused fullscreen game
/// pause: the staged focused lifetime names a staged fact that is
/// fullscreen. Normalized staged facts only. Under the pause no
/// focus-follows effect mints and no warp point publishes.
pub(crate) fn cursor_game_paused(engine: &WorldEngine) -> bool {
    if engine.staged.observation.focused_global.is_empty() {
        return false;
    }
    let focused = WindowId(engine.staged.observation.focused_global.clone());
    engine
        .staged
        .facts
        .get(&focused)
        .is_some_and(|fact| fact.fullscreen == Some(true))
}

/// Whether one staged window is focus-eligible for hover coupling: homed to
/// a readable unheld non-floating scope, with no unexplained fit exclusion
/// (ordinary floats stay eligible for admission, never for hover), and never
/// a fullscreen, sticky, floating, or excluded target per the normative
/// automatic restrictions.
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
    let derived = engine.derived_for(window);
    if derived.hidden || derived.hold || derived.excluded {
        return false;
    }
    // Normative automatic target restrictions: hover never targets
    // floating, sticky, or fullscreen victims. Admission alone keeps its
    // ordinary-float exception; hover gains none.
    if derived.floating || derived.sticky || derived.overlay_fullscreen {
        return false;
    }
    // Unexplained fit-excluded newcomers never take automatic focus
    // (same rule as admission intent): ordinary floats stay eligible.
    if derived.fit_excluded
        && !derived.floating
        && !derived.overlay_fullscreen
        && !derived.overlay_maximized
    {
        return false;
    }
    true
}

/// Pointer window for the staged observation: the staged normalized
/// cursor-hit managed lifetime, gated by the core's own eligibility and
/// current view. The hit is adapter-observed native truth (which topmost
/// managed window is actually under the cursor); the core never re-derives
/// it from rectangles, so hidden snapshots, non-current views, foreign
/// pixels, and overlapping floats cannot win by geometry or token order. An
/// absent hit, a hit outside visible staged membership of the retained
/// view, or an ineligible hit mints nothing.
fn hover_candidate(engine: &WorldEngine, held_scopes: &BTreeSet<DomainKey>) -> Option<WindowId> {
    let hit = engine.staged.observation.cursor_hit.clone()?;
    let fact = engine.staged.facts.get(&hit)?;
    if fact.minimized == Some(true) {
        return None;
    }
    // Normative automatic target restrictions: hover never targets
    // fullscreen, sticky, or host-hidden victims, even when classification
    // lags. Admission alone keeps its ordinary-float path.
    if fact.fullscreen == Some(true) || fact.sticky == Some(true) || fact.host_hidden == Some(true)
    {
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
    // Visible staged membership: the hit must ride the tiled carriers of
    // its home domain. Float exceptions never authorize hover: admission
    // alone keeps the ordinary-float path, and broadening hover to floats
    // or overlays would violate the normative automatic restrictions.
    // Minimized, unreadable, and unhomed lifetimes never match, whatever
    // the adapter reported.
    let domain = engine
        .staged
        .domains
        .iter()
        .find(|domain| domain.key == home)?;
    let tiled = domain.windows.iter().any(|window| window.window == hit);
    if tiled { Some(hit) } else { None }
}

/// Push one focus-follows-cursor effect unless a stronger focus effect was
/// already minted for this staged observation (fresh admission or an explicit
/// command focus). Runs after command execution on the final staged state, so
/// hover resolves against committed rings/views.
pub(crate) fn maybe_push_focus_follows(
    engine: &mut WorldEngine,
    held_scopes: &BTreeSet<DomainKey>,
    effects: &mut Vec<OneShotEffect>,
) {
    if effects
        .iter()
        .any(|effect| effect.kind == EffectKind::Focus)
    {
        return;
    }
    if let Some(follow) = engine.take_focus_follows(held_scopes) {
        // The accepted hover advances the execution-time cursor
        // immediately (best-effort, like every other accepted focus
        // choice), so a later command in the same batch resolves its
        // implicit subject from post-hover core state. Only the final
        // winner publishes.
        let _ = engine.record_canonical(&follow.target, held_scopes, false);
        effects.push(follow);
    }
}

impl WorldEngine {
    /// One focus-follows-cursor effect for this staged observation, if any.
    /// Mints only when the setting is on, the inventory is complete, the normalized
    /// cursor hit names an eligible hover target, no game/gesture pause
    /// holds, no stronger focus effect was minted for this observation (the
    /// caller gates on that), the hovered window differs from the observed
    /// focus, the cursor moved since the previous accepted observation or the hover
    /// target transitioned, and the hover target is not the one already
    /// coupled. Records the hover target either way, so a static cursor
    /// never re-steals focus after a keyboard command moves focus
    /// elsewhere. The `ffc-<token>` id scopes at-most-once per hover
    /// target: the adapter spent fence drops repeats, and arrival (focus
    /// catching up to hover) stops minting. The token is the first
    /// automatic-focus candidate's issuing revision interned per live
    /// lifetime (see [`crate::world_focus`]), not necessarily the first
    /// emitted effect revision, reused across hover transitions and
    /// observations, never the window spelling.
    pub(crate) fn take_focus_follows(
        &mut self,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<OneShotEffect> {
        if !self.focus_follows_cursor_setting() {
            return None;
        }
        if !self.staged.observation.inventory_complete {
            return None;
        }
        if cursor_game_paused(self) {
            return None;
        }
        if self.staged.gesture_barrier || self.gesture.is_some() {
            return None;
        }
        // Departure tracking (Q3): leaving the live hover target onto an
        // empty or ineligible pointer position clears the last-hover memory
        // without minting, so a later return to the same target counts as a
        // fresh qualifying entry even when the pending row survived while
        // away. Pauses above return without touching the memory.
        let Some(hovered) = hover_candidate(self, held_scopes) else {
            self.ffc_last_hover = None;
            return None;
        };
        // Focused float/sticky ownership: never steal toward the hover
        // target while native foreground names a different floating or
        // sticky window. Hover presence memory still advances, so a static
        // cursor never re-steals once the foreground clears.
        if !self.staged.observation.focused_global.is_empty()
            && self.staged.observation.focused_global != hovered.0
        {
            let focused = WindowId(self.staged.observation.focused_global.clone());
            let shape = self.derived_for(&focused);
            if shape.floating || shape.sticky {
                self.ffc_last_hover = Some(hovered);
                return None;
            }
        }
        let previous = self.ffc_last_hover.clone();
        self.ffc_last_hover = Some(hovered.clone());
        if hovered.0 == self.staged.observation.focused_global {
            return None;
        }
        // Q3: pointer movement over an already-pending hover target keeps
        // the live id only while the pointer never left: a fresh candidate
        // here would mint a new id and a new adapter call, so continuous
        // presence over the pending target suppresses the duplicate. After
        // a departure (previous cleared or moved elsewhere) the same target
        // is a fresh entry and must remint below.
        if previous.as_ref() == Some(&hovered)
            && self.pending_focus.as_ref().is_some_and(|pending| {
                pending.cause == crate::world_report::FocusRequestCause::FocusFollowsCursor
                    && pending.target == hovered
            })
        {
            return None;
        }
        // Genuine movement or hover transition only: a static cursor over
        // an unchanged hover never re-steals focus after a keyboard
        // command moves focus elsewhere.
        let moved = self.staged.observation.cursor != self.prev_cursor;
        let transitioned = previous.as_ref() != Some(&hovered);
        if !moved && !transitioned {
            return None;
        }
        let token = self.automatic_focus_token(&hovered);
        Some(OneShotEffect {
            id: format!("ffc-{token}"),
            kind: EffectKind::Focus,
            target: hovered,
            issued_revision: self.state_revision + 1,
            cause: EffectCause::FocusFollowsCursor,
            command_seq: None,
            await_shown: false,
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
        effects: &[OneShotEffect],
        windows: &[crate::world_report::WindowDesire],
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<(i32, i32)> {
        if !self.cursor_warp_setting() {
            return None;
        }
        if !self.staged.observation.inventory_complete {
            return None;
        }
        if cursor_game_paused(self) {
            return None;
        }
        if self.staged.gesture_barrier || self.gesture.is_some() {
            return None;
        }
        let cursor = self.staged.observation.cursor?;
        let screens: Vec<(crate::directional::OutputId, crate::geometry::Rect)> = self
            .staged
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
        let clamped = self
            .staged
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
