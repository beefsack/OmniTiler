use super::chords::{
    digit_repeat_live, history_repeat_live, index_direction, is_letter_l, resize_mode_for_action,
    resize_repeat_live, snap_op_admits, snap_repeat_live,
};
use super::*;

impl SnapClassify {
    /// Owned-chord dispatch for [`SnapClassify::push`]: `physical` opened the
    /// hold (or repeats it), `vk` is its down-time canonical key and `action`
    /// its explicit arm. Family pairing, op pinning, gate riding, and mask
    /// semantics are unchanged; the central guard above guarantees only true
    /// owners arrive here.
    pub(super) fn push_owned(
        &mut self,
        physical: u32,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        match action {
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                return self.push_history(physical, vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                return self.push_digit(vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                return self.push_relative_send(vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceOutputFollow | ChordAction::WorkspaceOutputStay => {
                return self.push_output(vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceMigrate => {
                return self.push_migrate(vk, is_up, foreground, route_live);
            }
            ChordAction::Maximize => {
                return self.push_maximize(is_up, foreground, route_live);
            }
            ChordAction::Orientation => {
                return self.push_orientation(is_up, foreground, route_live);
            }
            ChordAction::WorkspaceTiling => {
                return self.push_workspace_tiling(is_up, foreground, route_live);
            }
            ChordAction::Fullscreen => {
                return self.push_fullscreen(is_up, foreground, route_live);
            }
            ChordAction::Float => {
                debug_assert!(is_float_vk(vk) || is_sticky_vk(vk));
                return self.push_g(is_up, foreground, route_live);
            }
            ChordAction::Sticky => {
                debug_assert!(is_float_vk(vk) || is_sticky_vk(vk));
                return self.push_g(is_up, foreground, route_live);
            }
            ChordAction::LaunchBrowser => {
                debug_assert!(is_launch_browser_vk(vk));
                return self.push_launch(LaunchKind::Browser, is_up, foreground, route_live);
            }
            ChordAction::LaunchTerminal => {
                debug_assert!(is_launch_terminal_vk(vk));
                return self.push_launch(LaunchKind::Terminal, is_up, foreground, route_live);
            }
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                return self.push_resize(vk, action, is_up, foreground, route_live);
            }
            ChordAction::Directional => {}
        }
        let physical_lock = physical == VK_L;
        let idx = catalog_index(vk)?;
        let direction = index_direction(idx);
        if is_up {
            if !self.key_down[idx] {
                return None;
            }
            self.key_down[idx] = false;
            let origin = self.key_origin[idx];
            self.key_origin[idx] = false;
            let op = self.key_op[idx].unwrap_or(SnapOp::Focus);
            self.key_op[idx] = None;
            self.counts[idx].up += 1;
            if origin {
                self.counts[idx].consumed += 1;
                Some(Classified::Snap(SnapIntent {
                    op,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.counts[idx].passed += 1;
                Some(Classified::Snap(SnapIntent {
                    op,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify BEFORE the fresh-chord guard: a
            // consumed hold stays swallowed until its matching up even when
            // Ctrl/Alt arrive mid-hold, Win is released early, or Shift
            // flips. Only the dispatch (`announce`) needs the live
            // combination to still match the pinned op.
            if self.key_down[idx] {
                self.counts[idx].repeat += 1;
                let op = self.key_op[idx].unwrap_or(SnapOp::Focus);
                if self.key_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && snap_repeat_live(
                            self.shift,
                            self.ctrl,
                            self.alt,
                            self.win_l || self.win_r,
                            op,
                        )
                        && snap_op_admits(physical, op, self.allow_win_l);
                    self.counts[idx].consumed += 1;
                    // Only a Win-held repeat rearms the Start-menu mask: a
                    // bare repeat after Win-up stays swallowed but must not
                    // create a new mask obligation, or a later naked Win tap
                    // would mask Start incorrectly.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Snap { op, direction });
                    }
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.counts[idx].passed += 1;
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                let op = if self.shift {
                    SnapOp::Move
                } else {
                    SnapOp::Focus
                };
                // Unshifted physical Win+L is the OS lock chord: without explicit
                // opt-in it passes through untracked, so its paired key-up also
                // passes. The fence keys on the physical chord: a safe rebound
                // into the canonical L slot works without opt-in.
                // Established opt-in exception: an ordinary LL hook cannot
                // reliably intercept it; left unchanged.
                if op == SnapOp::Focus && is_letter_l(idx) && physical_lock && !self.allow_win_l {
                    return None;
                }
                self.key_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.key_origin[idx] = origin;
                self.key_op[idx] = Some(op);
                self.counts[idx].down += 1;
                if origin {
                    self.counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Snap { op, direction });
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.counts[idx].passed += 1;
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Resize half of the unified classifier (Win+Alt+key outwards,
    /// Win+Shift+Alt+key inwards, over the directional catalog letters plus
    /// arrow aliases). Same Win/armed-hold/gate/mask contract as the
    /// directional catalog, but Alt selects the arm: fresh downs require
    /// Win+Alt with no Ctrl, Shift selecting the mode. The mode is fixed at
    /// down time from the explicit action (rebinds keep their action even
    /// when the physical key differs) and rides the paired key-up; repeats of
    /// a consumed hold stay swallowed until the matching up but only announce
    /// while the live Win+Alt(+Shift) combination still matches the pinned
    /// mode, so a bare-key repeat after Win-up never re-dispatches. Extra
    /// Ctrl or a missing Win/Alt passes through untracked with no pin.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live (disabled or rebound away
    /// mid-hold) stays swallowed without dispatching until its matching up.
    fn push_resize(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = catalog_index(vk)?;
        let mode = resize_mode_for_action(action)?;
        let direction = index_direction(idx);
        if is_up {
            if !self.resize_down[idx] {
                return None;
            }
            self.resize_down[idx] = false;
            let origin = self.resize_origin[idx];
            self.resize_origin[idx] = false;
            let pinned = self.resize_mode[idx].unwrap_or(mode);
            self.resize_mode[idx] = None;
            self.resize_counts[idx].up += 1;
            if origin {
                self.resize_counts[idx].consumed += 1;
                Some(Classified::Resize(ResizeIntent {
                    mode: pinned,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.resize_counts[idx].passed += 1;
                Some(Classified::Resize(ResizeIntent {
                    mode: pinned,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify BEFORE the fresh-chord guard: a
            // consumed hold stays swallowed until its matching up even when
            // Alt/Shift/Ctrl/Win flip mid-hold. Only the dispatch
            // (`announce`) needs the live combination to still match the
            // pinned mode/action.
            if self.resize_down[idx] {
                self.resize_counts[idx].repeat += 1;
                let pinned = self.resize_mode[idx].unwrap_or(mode);
                if self.resize_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && resize_repeat_live(
                            self.shift,
                            self.ctrl,
                            self.alt,
                            self.win_l || self.win_r,
                            pinned,
                        );
                    self.resize_counts[idx].consumed += 1;
                    // Only a Win-held repeat rearms the Start-menu mask: a
                    // bare repeat after Win-up stays swallowed but must not
                    // create a new mask obligation.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Resize {
                            mode: pinned,
                            direction,
                        });
                    }
                    Some(Classified::Resize(ResizeIntent {
                        mode: pinned,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.resize_counts[idx].passed += 1;
                    Some(Classified::Resize(ResizeIntent {
                        mode: pinned,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                // Fresh down: Win+Alt with no Ctrl; Shift must select the
                // bound mode (outwards unshifted, inwards shifted). The Alt
                // arm is exact: unknown extra Ctrl or a missing Win/Alt
                // refuses with no pin, so the paired key-up passes too.
                if !(self.win_l || self.win_r) || !self.alt || self.ctrl {
                    return None;
                }
                if self.shift != (mode == ResizeMode::Inwards) {
                    return None;
                }
                self.resize_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.resize_origin[idx] = origin;
                self.resize_mode[idx] = Some(mode);
                self.resize_counts[idx].down += 1;
                if origin {
                    self.resize_counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Resize { mode, direction });
                    Some(Classified::Resize(ResizeIntent {
                        mode,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.resize_counts[idx].passed += 1;
                    Some(Classified::Resize(ResizeIntent {
                        mode,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// History half of the unified classifier (item 1: Win+Ctrl+Tab toggle,
    /// Win+Ctrl+H/K/Left/Up previous, Win+Ctrl+J/L/Down/Right next). Same
    /// Win/armed-hold/gate/mask contract as digits, but Ctrl selects the arm:
    /// fresh downs require Win+Ctrl with no Shift/Alt. The op is fixed at
    /// down time from the explicit action (rebinds keep their action even
    /// when the physical key differs); relative-step repeats ride the armed
    /// hold and dispatch while the live Win+Ctrl combination still matches,
    /// so held-key autorepeat steps repeatedly, while Previous-toggle repeats
    /// stay swallowed (maximize parity, no pingpong). Ups close the pair. Fresh downs
    /// consume iff takeover is on with the shortcut gate active; the owner
    /// rechecks fresh scope/suspension before any select.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    fn push_history(
        &mut self,
        _physical: u32,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = history_index(vk)?;
        let op = match action {
            ChordAction::WorkspacePrevious => WorkspaceHistoryOp::Previous,
            ChordAction::WorkspacePrev => WorkspaceHistoryOp::Prev,
            ChordAction::WorkspaceNext => WorkspaceHistoryOp::Next,
            _ => return None,
        };
        if is_up {
            if !self.hist_down[idx] {
                return None;
            }
            self.hist_down[idx] = false;
            let origin = self.hist_origin[idx];
            self.hist_origin[idx] = false;
            self.hist_op[idx] = None;
            self.hist_counts[idx].up += 1;
            if origin {
                self.hist_counts[idx].consumed += 1;
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.hist_counts[idx].passed += 1;
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed hold stays swallowed across mid-hold modifier/Win
            // transitions (only `announce` follows the live combination), a
            // passed hold stays passed.
            if self.hist_down[idx] {
                self.hist_counts[idx].repeat += 1;
                let pinned = self.hist_op[idx].unwrap_or(op);
                if self.hist_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && history_repeat_live(
                            self.ctrl,
                            self.shift,
                            self.alt,
                            self.win_l || self.win_r,
                        );
                    self.hist_counts[idx].consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::WorkspaceHistory { op: pinned });
                    }
                    // The Previous toggle never re-dispatches on hold
                    // (maximize parity): a held Tab would otherwise pingpong
                    // between the two views. Relative steps keep digit-like
                    // repeat dispatch while live.
                    let announce = live && pinned != WorkspaceHistoryOp::Previous;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op: pinned,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce,
                    }))
                } else {
                    self.hist_counts[idx].passed += 1;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op: pinned,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if !self.ctrl || self.shift || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                self.hist_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.hist_origin[idx] = origin;
                self.hist_op[idx] = Some(op);
                self.hist_counts[idx].down += 1;
                if origin {
                    self.hist_counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceHistory { op });
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.hist_counts[idx].passed += 1;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Digit half of the unified classifier: same Win/Ctrl/Alt/armed-hold/
    /// gate/mask contract as the directional catalog. Shift flips select into
    /// send and is fixed at down time; the op rides the paired key-up.
    /// Consumed-hold repeats stay swallowed until the matching up but only
    /// announce while the live combination still matches the pinned op. Fresh
    /// downs consume iff takeover is on with the shortcut gate active; the
    /// owner rechecks fresh identity/scope/elevation/fullscreen/gesture
    /// before any action (send without a live managed origin settles as
    /// unmanaged, never acts on protected windows).
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    fn push_digit(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if !(VK_0..=VK_9).contains(&vk) {
            return None;
        }
        let slot = (vk - VK_0) as usize;
        // Explicit stay action pins follow=false; the direct digit arm pins
        // follow from Shift (Shift sends and follows, unshifted selects).
        let stay = action == ChordAction::WorkspaceStayDigit;
        if is_up {
            if !self.digit_down[slot] {
                return None;
            }
            self.digit_down[slot] = false;
            let origin = self.digit_origin[slot];
            self.digit_origin[slot] = false;
            let op = self.digit_op[slot].unwrap_or(WorkspaceOp::Select);
            self.digit_op[slot] = None;
            let follow = self.digit_follow[slot];
            self.digit_follow[slot] = true;
            self.digit_ctrl[slot] = false;
            self.digit_counts[slot].up += 1;
            if origin {
                self.digit_counts[slot].consumed += 1;
                Some(Classified::Workspace(WorkspaceIntent {
                    op,
                    index: slot as u8,
                    follow,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.digit_counts[slot].passed += 1;
                Some(Classified::Workspace(WorkspaceIntent {
                    op,
                    index: slot as u8,
                    follow,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard, same
            // contract as the directional catalog: consumed holds stay
            // swallowed across mid-hold modifier/Win/Shift transitions (only
            // `announce` follows the live combination), passed holds stay
            // passed.
            if self.digit_down[slot] {
                self.digit_counts[slot].repeat += 1;
                let op = self.digit_op[slot].unwrap_or(WorkspaceOp::Select);
                let follow = self.digit_follow[slot];
                if self.digit_origin[slot] {
                    // Stay repeats dispatch only while the live combination
                    // still matches the pinned arm (Win+Shift plus the pinned
                    // Ctrl); follow/select repeats ride `digit_repeat_live`,
                    // which refuses Ctrl. Either way a mid-hold modifier flip
                    // swallows without dispatching, leaking no repeat and
                    // never flipping a stay into a follow.
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && if follow {
                            digit_repeat_live(
                                self.shift,
                                self.ctrl,
                                self.alt,
                                self.win_l || self.win_r,
                                op,
                            )
                        } else {
                            self.shift
                                && self.ctrl == self.digit_ctrl[slot]
                                && !self.alt
                                && (self.win_l || self.win_r)
                        };
                    self.digit_counts[slot].consumed += 1;
                    // Win-held repeats only rearm the mask; bare repeats
                    // after Win-up stay swallowed without a new obligation.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Workspace {
                            op,
                            index: slot as u8,
                        });
                    }
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.digit_counts[slot].passed += 1;
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                // Digit follow/select refuse Ctrl/Alt outright; numbered stay
                // additionally accepts the Win+Ctrl+Shift backlog arm (Shift
                // required, Alt never). Direct Ctrl+Shift+digit chords still
                // refuse above in `direct_action`: only an explicit stay
                // rebind routes here.
                if !(self.win_l || self.win_r) || self.alt {
                    return None;
                }
                if stay {
                    if !self.shift {
                        return None;
                    }
                } else if self.ctrl {
                    return None;
                }
                let op = if self.shift {
                    WorkspaceOp::Send
                } else {
                    WorkspaceOp::Select
                };
                // Unshifted chords never stay: only shifted sends carry the
                // follow/stay distinction.
                let follow = if op == WorkspaceOp::Send { !stay } else { true };
                self.digit_down[slot] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.digit_origin[slot] = origin;
                self.digit_op[slot] = Some(op);
                self.digit_follow[slot] = follow;
                self.digit_ctrl[slot] = self.ctrl;
                self.digit_counts[slot].down += 1;
                if origin {
                    self.digit_counts[slot].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Workspace {
                        op,
                        index: slot as u8,
                    });
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.digit_counts[slot].passed += 1;
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }
}
