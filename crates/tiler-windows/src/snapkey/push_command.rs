use super::chords::{
    index_direction, migrate_direction, output_repeat_live, relative_send_repeat_live,
};
use super::intents::GArm;
use super::*;

impl SnapClassify {
    /// Item 2 relative-send half of the unified classifier (Win+Ctrl+Shift+
    /// H/K/Left/Up previous, J/L/Down/Right next). Same Win/armed-hold/gate/
    /// mask contract as history, but Shift selects the send arm: fresh downs
    /// require Win+Ctrl+Shift with no Alt. `follow` pins at down time from the
    /// explicit action (direct chords follow; stay rows rebind through the
    /// stay actions); `delta` pins from the canonical key. Repeats ride the
    /// armed hold and dispatch while the live Win+Ctrl+Shift combination still
    /// matches; ups close the pair. Fresh downs consume iff takeover is on
    /// with the shortcut gate active; the owner rechecks fresh scope/
    /// suspension before any send.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    pub(super) fn push_relative_send(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = relative_send_index(vk)?;
        let follow = matches!(
            action,
            ChordAction::WorkspaceSendPrev | ChordAction::WorkspaceSendNext
        );
        let delta = relative_send_delta(vk)?;
        // Defensive: the action's prev/next must agree with the canonical
        // key's direction; a mismatched rebind (prev action onto a next key)
        // refuses instead of sending the wrong way.
        let action_prev = matches!(
            action,
            ChordAction::WorkspaceSendPrev | ChordAction::WorkspaceSendStayPrev
        );
        if (delta < 0) != action_prev {
            return None;
        }
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::WorkspaceSend(WorkspaceSendIntent {
                delta,
                follow,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !self.send_down[idx] {
                return None;
            }
            self.send_down[idx] = false;
            let origin = self.send_origin[idx];
            self.send_origin[idx] = false;
            self.send_follow[idx] = true;
            self.send_delta[idx] = 0;
            // Per-slot counts reuse the history-adjacent vocabulary: count in
            // the directional slot matching the canonical key when available.
            if origin {
                return Some(intent(SnapEdge::Up, true, false));
            }
            return Some(intent(SnapEdge::Up, false, false));
        }
        if self.send_down[idx] {
            let pinned_follow = self.send_follow[idx];
            let pinned_delta = self.send_delta[idx];
            let pinned = Classified::WorkspaceSend(WorkspaceSendIntent {
                delta: pinned_delta,
                follow: pinned_follow,
                edge: SnapEdge::Repeat,
                foreground,
                consumed: true,
                announce: false,
            });
            let _ = pinned;
            if self.send_origin[idx] {
                let live = self.enabled
                    && self.gate_active
                    && route_live
                    && relative_send_repeat_live(
                        self.ctrl,
                        self.shift,
                        self.alt,
                        self.win_l || self.win_r,
                    );
                if self.win_l || self.win_r {
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceSend {
                        delta: pinned_delta,
                        follow: pinned_follow,
                    });
                }
                return Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                    delta: pinned_delta,
                    follow: pinned_follow,
                    edge: SnapEdge::Repeat,
                    foreground,
                    consumed: true,
                    announce: live,
                }));
            }
            return Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                delta: pinned_delta,
                follow: pinned_follow,
                edge: SnapEdge::Repeat,
                foreground,
                consumed: false,
                announce: false,
            }));
        }
        if !self.ctrl || !self.shift || self.alt || !(self.win_l || self.win_r) {
            return None;
        }
        self.send_down[idx] = true;
        let origin = self.enabled && self.gate_active && route_live;
        self.send_origin[idx] = origin;
        self.send_follow[idx] = follow;
        self.send_delta[idx] = delta;
        if origin {
            self.mask_pending = true;
            self.mask_trigger = Some(MaskTrigger::WorkspaceSend { delta, follow });
            Some(intent(SnapEdge::Down, true, true))
        } else {
            Some(intent(SnapEdge::Down, false, false))
        }
    }

    /// Item 5 explicit output-send half of the unified classifier
    /// (Win+Ctrl+Alt+H/J/K/L plus arrow aliases). Same Win/armed-hold/gate/
    /// mask contract as relative sends, but Ctrl+Alt selects the arm: fresh
    /// downs require Win+Ctrl+Alt. Direct chords follow unshifted; a shifted
    /// chord refuses (only an explicit stay rebind routes through the rebound
    /// table, pinning Shift with the stay action). `direction` pins from the
    /// canonical key; `follow` pins at down time from the explicit action.
    /// Repeats ride the armed hold and dispatch while the live Win+Ctrl+Alt
    /// combination still matches the pinned Shift; ups close the pair. Fresh
    /// downs consume iff takeover is on with the shortcut gate active; the
    /// owner rechecks fresh scope/suspension before any send.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    pub(super) fn push_output(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = catalog_index(vk)?;
        let direction = index_direction(idx);
        let follow = action == ChordAction::WorkspaceOutputFollow;
        let stay = action == ChordAction::WorkspaceOutputStay;
        if !follow && !stay {
            return None;
        }
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::WorkspaceOutput(WorkspaceOutputIntent {
                direction,
                follow,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !self.output_down[idx] {
                return None;
            }
            self.output_down[idx] = false;
            let origin = self.output_origin[idx];
            self.output_origin[idx] = false;
            self.output_follow[idx] = true;
            self.output_shift[idx] = false;
            self.output_counts[idx].up += 1;
            if origin {
                self.output_counts[idx].consumed += 1;
                return Some(intent(SnapEdge::Up, true, false));
            }
            self.output_counts[idx].passed += 1;
            return Some(intent(SnapEdge::Up, false, false));
        }
        if self.output_down[idx] {
            self.output_counts[idx].repeat += 1;
            let pinned_follow = self.output_follow[idx];
            let pinned_shift = self.output_shift[idx];
            if self.output_origin[idx] {
                let live = self.enabled
                    && self.gate_active
                    && route_live
                    && output_repeat_live(
                        self.ctrl,
                        self.shift,
                        self.alt,
                        self.win_l || self.win_r,
                        pinned_shift,
                    );
                if self.win_l || self.win_r {
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceOutput {
                        direction,
                        follow: pinned_follow,
                    });
                }
                self.output_counts[idx].consumed += 1;
                return Some(Classified::WorkspaceOutput(WorkspaceOutputIntent {
                    direction,
                    follow: pinned_follow,
                    edge: SnapEdge::Repeat,
                    foreground,
                    consumed: true,
                    announce: live,
                }));
            }
            self.output_counts[idx].passed += 1;
            return Some(Classified::WorkspaceOutput(WorkspaceOutputIntent {
                direction,
                follow: pinned_follow,
                edge: SnapEdge::Repeat,
                foreground,
                consumed: false,
                announce: false,
            }));
        }
        if !self.ctrl || !self.alt || !(self.win_l || self.win_r) {
            return None;
        }
        // Direct follow rides unshifted Win+Ctrl+Alt exactly; a shifted chord
        // refuses unless the stay action claims it through an explicit rebind.
        // Stay accepts either Shift state so a stay row rebound without Shift
        // (e.g. Win+Ctrl+Alt+F6) still routes; the Shift state pins with the
        // hold for repeat gating.
        if follow && self.shift {
            return None;
        }
        self.output_down[idx] = true;
        let origin = self.enabled && self.gate_active && route_live;
        self.output_origin[idx] = origin;
        self.output_follow[idx] = follow;
        self.output_shift[idx] = self.shift;
        self.output_counts[idx].down += 1;
        if origin {
            self.output_counts[idx].consumed += 1;
            self.mask_pending = true;
            self.mask_trigger = Some(MaskTrigger::WorkspaceOutput { direction, follow });
            Some(intent(SnapEdge::Down, true, true))
        } else {
            self.output_counts[idx].passed += 1;
            Some(intent(SnapEdge::Down, false, false))
        }
    }

    /// Item 14 whole-workspace migration half of the unified classifier
    /// (rebound Win+Ctrl+Alt+Shift + letter, arrow aliases route into the
    /// letter slot through the rebound table). Same Win/armed-hold/gate/mask
    /// contract as output sends, but the full-modifier chord selects the arm:
    /// fresh downs require Win+Ctrl+Alt+Shift. There is no direct chord
    /// (unshifted Win+Ctrl+Alt stays output follow; any other combination
    /// refuses), so only the explicit migrate action routes here. `direction`
    /// pins from the canonical letter key. Only the down dispatches: a held
    /// migration never re-fires, so repeats stay swallowed while keeping the
    /// mask armed. Fresh downs consume iff takeover is on with the shortcut
    /// gate active; the owner rechecks fresh scope/suspension before any
    /// migration. `route_live` gates fresh consumption: a pinned hold whose
    /// chord no longer routes live stays swallowed without dispatching until
    /// its matching up.
    pub(super) fn push_migrate(
        &mut self,
        vk: u32,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = migrate_index(vk)?;
        let direction = migrate_direction(idx);
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::WorkspaceMigrate(WorkspaceMigrateIntent {
                direction,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !self.migrate_down[idx] {
                return None;
            }
            self.migrate_down[idx] = false;
            let origin = self.migrate_origin[idx];
            self.migrate_origin[idx] = false;
            self.migrate_counts[idx].up += 1;
            if origin {
                self.migrate_counts[idx].consumed += 1;
                return Some(intent(SnapEdge::Up, true, false));
            }
            self.migrate_counts[idx].passed += 1;
            return Some(intent(SnapEdge::Up, false, false));
        }
        if self.migrate_down[idx] {
            self.migrate_counts[idx].repeat += 1;
            if self.migrate_origin[idx] {
                // Held repeat: swallowed, never re-dispatched. A Win-held
                // hold continues to disguise Win, so the mask stays armed; a
                // bare repeat after Win-up must not create a new mask
                // obligation.
                if self.win_l || self.win_r {
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceMigrate { direction });
                }
                self.migrate_counts[idx].consumed += 1;
                return Some(intent(SnapEdge::Repeat, true, false));
            }
            self.migrate_counts[idx].passed += 1;
            return Some(intent(SnapEdge::Repeat, false, false));
        }
        if !self.ctrl || !self.shift || !self.alt || !(self.win_l || self.win_r) {
            return None;
        }
        self.migrate_down[idx] = true;
        let origin = self.enabled && self.gate_active && route_live;
        self.migrate_origin[idx] = origin;
        self.migrate_counts[idx].down += 1;
        if origin {
            self.migrate_counts[idx].consumed += 1;
            self.mask_pending = true;
            self.mask_trigger = Some(MaskTrigger::WorkspaceMigrate { direction });
            Some(intent(SnapEdge::Down, true, true))
        } else {
            self.migrate_counts[idx].passed += 1;
            Some(intent(SnapEdge::Down, false, false))
        }
    }

    /// Maximize-toggle half of the unified classifier (Win+M, KDE Meta+M
    /// parity): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// directional catalog. Shift selects the directional move arm instead,
    /// so any held Shift (or Ctrl/Alt, or missing Win) passes a fresh M
    /// through untracked and the paired key-up also passes. A consumed hold
    /// stays swallowed across later Shift/Ctrl/Alt/Win transitions until its
    /// matching up. Only the down dispatches: KDE shortcuts are
    /// discrete per press, so held repeats are swallowed (mask stays armed)
    /// instead of re-toggling. Ups close the pair. Fresh downs consume iff
    /// takeover is on with the shortcut gate active; the owner rechecks fresh
    /// identity/scope/elevation/fullscreen/gesture before any action.
    pub(super) fn push_maximize(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.maximize_down {
                return None;
            }
            self.maximize_down = false;
            let origin = self.maximize_origin;
            self.maximize_origin = false;
            self.max_counts.up += 1;
            if origin {
                self.max_counts.consumed += 1;
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.max_counts.passed += 1;
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed M hold stays swallowed across later Shift/Ctrl/Alt/Win
            // transitions (never re-dispatched), a passed hold stays passed.
            if self.maximize_down {
                self.max_counts.repeat += 1;
                if self.maximize_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.max_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Maximize);
                    }
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.max_counts.passed += 1;
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.maximize_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.maximize_origin = origin;
                self.max_counts.down += 1;
                if origin {
                    self.max_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Maximize);
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.max_counts.passed += 1;
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Orientation-toggle half of the unified classifier (Win+O, KDE Meta+O
    /// parity, item 4): same Win/Ctrl/Alt/armed-hold/gate/mask contract as
    /// the maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes
    /// a fresh O through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later Shift/Ctrl/Alt/Win
    /// transitions until its matching up. Tentative Windows repeat policy:
    /// only the down dispatches; held repeats are swallowed (mask stays
    /// armed) instead of re-toggling. Ups close the pair. Fresh downs
    /// consume iff takeover is on with the shortcut gate active; the owner
    /// rechecks fresh identity/scope/elevation/fullscreen/gesture before any
    /// action.
    pub(super) fn push_orientation(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.orientation_down {
                return None;
            }
            self.orientation_down = false;
            let origin = self.orientation_origin;
            self.orientation_origin = false;
            self.orientation_counts.up += 1;
            if origin {
                self.orientation_counts.consumed += 1;
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.orientation_counts.passed += 1;
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed O hold stays swallowed across later Shift/Ctrl/Alt/Win
            // transitions (never re-dispatched), a passed hold stays passed.
            if self.orientation_down {
                self.orientation_counts.repeat += 1;
                if self.orientation_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.orientation_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Orientation);
                    }
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.orientation_counts.passed += 1;
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.orientation_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.orientation_origin = origin;
                self.orientation_counts.down += 1;
                if origin {
                    self.orientation_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Orientation);
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.orientation_counts.passed += 1;
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Workspace-tiling toggle half of the unified classifier (Win+Y, KDE
    /// Meta+Y parity, item 21): same Win/Ctrl/Alt/armed-hold/gate/mask
    /// contract as the orientation arm. Any held Shift (or Ctrl/Alt, or
    /// missing Win) passes a fresh Y through untracked and the paired key-up
    /// also passes. A consumed hold stays swallowed across later transitions
    /// until its matching up. Only the down dispatches: held repeats are
    /// swallowed (mask stays armed) instead of re-toggling the workspace
    /// mode. Ups close the pair. Fresh downs consume iff takeover is on with
    /// the shortcut gate active; the owner rechecks the active scope plus
    /// suspension/elevation before toggling, never touching a window.
    pub(super) fn push_workspace_tiling(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.workspace_tiling_down {
                return None;
            }
            self.workspace_tiling_down = false;
            let origin = self.workspace_tiling_origin;
            self.workspace_tiling_origin = false;
            self.workspace_tiling_counts.up += 1;
            if origin {
                self.workspace_tiling_counts.consumed += 1;
                Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.workspace_tiling_counts.passed += 1;
                Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed Y hold stays swallowed across later Shift/Ctrl/Alt/Win
            // transitions (never re-dispatched), a passed hold stays passed.
            if self.workspace_tiling_down {
                self.workspace_tiling_counts.repeat += 1;
                if self.workspace_tiling_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.workspace_tiling_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::WorkspaceTiling);
                    }
                    Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.workspace_tiling_counts.passed += 1;
                    Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.workspace_tiling_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.workspace_tiling_origin = origin;
                self.workspace_tiling_counts.down += 1;
                if origin {
                    self.workspace_tiling_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceTiling);
                    Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.workspace_tiling_counts.passed += 1;
                    Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Fullscreen-toggle half of the unified classifier (Win+F11, KDE Meta+F11
    /// parity): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes a
    /// fresh F11 through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later transitions until its
    /// matching up. Only the down dispatches:
    /// held repeats are swallowed (mask stays armed) instead of re-toggling.
    /// Ups close the pair. Fresh downs consume iff takeover is on with the
    /// shortcut gate active; the owner rechecks fresh identity/scope/
    /// elevation/fullscreen/gesture before any action.
    pub(super) fn push_fullscreen(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.fullscreen_down {
                return None;
            }
            self.fullscreen_down = false;
            let origin = self.fullscreen_origin;
            self.fullscreen_origin = false;
            self.fullscreen_counts.up += 1;
            if origin {
                self.fullscreen_counts.consumed += 1;
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.fullscreen_counts.passed += 1;
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard, same
            // contract as maximize: consumed holds stay swallowed, passed
            // holds stay passed.
            if self.fullscreen_down {
                self.fullscreen_counts.repeat += 1;
                if self.fullscreen_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.fullscreen_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Fullscreen);
                    }
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.fullscreen_counts.passed += 1;
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.fullscreen_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.fullscreen_origin = origin;
                self.fullscreen_counts.down += 1;
                if origin {
                    self.fullscreen_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Fullscreen);
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.fullscreen_counts.passed += 1;
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Shared G-key dispatcher (Win+G float, Win+Shift+G sticky, KDE
    /// Meta+G / Meta+Shift+G parity). The op is fixed at down time from the
    /// Shift state, so releasing Shift before the key-up cannot flip float
    /// into sticky (or vice versa); repeats ride the armed hold regardless of
    /// later Shift. Ctrl/Alt or a missing Win passes through untracked.
    /// Ups close whichever hold is armed; untracked ups return `None`.
    pub(super) fn push_g(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if self.float_down {
                return self.push_g_arm(GArm::Float, is_up, foreground, route_live);
            }
            if self.sticky_down {
                return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
            }
            return None;
        }
        if self.float_down {
            return self.push_g_arm(GArm::Float, is_up, foreground, route_live);
        }
        if self.sticky_down {
            return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
        }
        if self.shift {
            return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
        }
        self.push_g_arm(GArm::Float, is_up, foreground, route_live)
    }

    /// Shared G-key arm (float and sticky halves): same Win/Ctrl/Alt/
    /// armed-hold/gate/mask contract as the maximize arm. A fresh down routes
    /// by Shift; held repeats ride the armed hold regardless of later Shift.
    /// Only the down dispatches: held repeats are swallowed (mask stays
    /// armed) instead of re-toggling. Ups close the pair. Fresh downs consume
    /// iff takeover is on with the shortcut gate active; the owner rechecks
    /// fresh identity/scope/elevation/fullscreen/gesture before any action.
    fn push_g_arm(
        &mut self,
        arm: GArm,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let sticky = arm == GArm::Sticky;
        let shift = self.shift;
        let trigger = if sticky {
            MaskTrigger::Sticky
        } else {
            MaskTrigger::Float
        };
        let (down, origin, counts) = if sticky {
            (
                &mut self.sticky_down,
                &mut self.sticky_origin,
                &mut self.sticky_counts,
            )
        } else {
            (
                &mut self.float_down,
                &mut self.float_origin,
                &mut self.float_counts,
            )
        };
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            if sticky {
                Classified::Sticky(StickyIntent {
                    edge,
                    foreground,
                    consumed,
                    announce,
                })
            } else {
                Classified::Float(FloatIntent {
                    edge,
                    foreground,
                    consumed,
                    announce,
                })
            }
        };
        if is_up {
            if !*down {
                return None;
            }
            *down = false;
            let had_origin = *origin;
            *origin = false;
            counts.up += 1;
            if had_origin {
                counts.consumed += 1;
                Some(intent(SnapEdge::Up, true, false))
            } else {
                counts.passed += 1;
                Some(intent(SnapEdge::Up, false, false))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed G hold stays swallowed across later Ctrl/Alt/Win/Shift
            // transitions (never flipping arms), a passed hold stays passed.
            if *down {
                counts.repeat += 1;
                if *origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation. Shift is op-fixed at down time: a
                    // later Shift never flips this repeat into the other arm.
                    counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(trigger);
                    }
                    Some(intent(SnapEdge::Repeat, true, false))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Repeat, false, false))
                }
            } else {
                if self.ctrl || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                if shift != sticky {
                    // The other arm owns this Shift state: the dispatcher
                    // routes fresh downs there, never here.
                    return None;
                }
                *down = true;
                let has_origin = self.enabled && self.gate_active && route_live;
                *origin = has_origin;
                counts.down += 1;
                if has_origin {
                    counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(trigger);
                    Some(intent(SnapEdge::Down, true, true))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Down, false, false))
                }
            }
        }
    }

    /// Launcher half of the unified classifier (Win+B browser, Win+T
    /// terminal): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes a
    /// fresh chord through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later Shift/Ctrl/Alt/Win
    /// transitions until its matching up. Only the down dispatches: held
    /// repeats are swallowed (mask stays armed) instead of re-spawning. Ups
    /// close the pair. Fresh downs consume iff takeover is on with the
    /// shortcut gate active; the owner spawns without touching any window.
    pub(super) fn push_launch(
        &mut self,
        kind: LaunchKind,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let trigger = match kind {
            LaunchKind::Browser => MaskTrigger::LaunchBrowser,
            LaunchKind::Terminal => MaskTrigger::LaunchTerminal,
        };
        let (down, origin, counts) = match kind {
            LaunchKind::Browser => (
                &mut self.launch_browser_down,
                &mut self.launch_browser_origin,
                &mut self.launch_browser_counts,
            ),
            LaunchKind::Terminal => (
                &mut self.launch_terminal_down,
                &mut self.launch_terminal_origin,
                &mut self.launch_terminal_counts,
            ),
        };
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::Launch(LaunchIntent {
                kind,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !*down {
                return None;
            }
            *down = false;
            let had_origin = *origin;
            *origin = false;
            counts.up += 1;
            if had_origin {
                counts.consumed += 1;
                Some(intent(SnapEdge::Up, true, false))
            } else {
                counts.passed += 1;
                Some(intent(SnapEdge::Up, false, false))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed launcher hold stays swallowed across later
            // Shift/Ctrl/Alt/Win transitions (never re-dispatched), a passed
            // hold stays passed.
            if *down {
                counts.repeat += 1;
                if *origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(trigger);
                    }
                    Some(intent(SnapEdge::Repeat, true, false))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Repeat, false, false))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                *down = true;
                let has_origin = self.enabled && self.gate_active && route_live;
                *origin = has_origin;
                counts.down += 1;
                if has_origin {
                    counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(trigger);
                    Some(intent(SnapEdge::Down, true, true))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Down, false, false))
                }
            }
        }
    }
}
