#![forbid(unsafe_code)]
use super::chords::{
    PIN_KEYS, PinnedRoute, Route, collide_refusal, collide_trigger, index_direction,
    is_modifier_vk, migrate_direction, resize_action_for_mode, resize_mode_for_shift,
};
use super::*;

impl SnapClassify {
    #[must_use]
    pub fn new(config: KeyboardConfig) -> Self {
        Self {
            win_l: false,
            win_r: false,
            shift: false,
            ctrl: false,
            alt: false,
            key_down: [false; 8],
            key_origin: [false; 8],
            key_op: [None; 8],
            digit_down: [false; 10],
            digit_origin: [false; 10],
            digit_op: [None; 10],
            digit_follow: [true; 10],
            digit_ctrl: [false; 10],
            send_down: [false; 8],
            send_origin: [false; 8],
            send_follow: [true; 8],
            send_delta: [0; 8],
            output_down: [false; 8],
            output_origin: [false; 8],
            output_follow: [true; 8],
            output_shift: [false; 8],
            migrate_down: [false; 4],
            migrate_origin: [false; 4],
            maximize_down: false,
            maximize_origin: false,
            fullscreen_down: false,
            fullscreen_origin: false,
            float_down: false,
            float_origin: false,
            sticky_down: false,
            sticky_origin: false,
            orientation_down: false,
            orientation_origin: false,
            workspace_tiling_down: false,
            workspace_tiling_origin: false,
            launch_browser_down: false,
            launch_browser_origin: false,
            launch_terminal_down: false,
            launch_terminal_origin: false,
            resize_down: [false; 8],
            resize_origin: [false; 8],
            resize_mode: [None; 8],
            enabled: config.takeover,
            gate_active: true,
            allow_win_l: config.allow_win_l,
            vk_remap: Vec::new(),
            vk_disabled: Vec::new(),
            phys_pin: [None; 256],
            collide: [None; 256],
            counts: [SnapCounts::default(); 8],
            resize_counts: [SnapCounts::default(); 8],
            digit_counts: [SnapCounts::default(); 10],
            output_counts: [SnapCounts::default(); 8],
            max_counts: SnapCounts::default(),
            fullscreen_counts: SnapCounts::default(),
            float_counts: SnapCounts::default(),
            sticky_counts: SnapCounts::default(),
            orientation_counts: SnapCounts::default(),
            workspace_tiling_counts: SnapCounts::default(),
            launch_browser_counts: SnapCounts::default(),
            launch_terminal_counts: SnapCounts::default(),
            hist_down: [false; 9],
            hist_origin: [false; 9],
            hist_op: [None; 9],
            hist_counts: [SnapCounts::default(); 9],
            migrate_counts: [SnapCounts::default(); 4],
            mask_pending: false,
            mask_trigger: None,
            hold_masked: false,
            mask_attempted: 0,
            mask_ok: 0,
            mask_failed: 0,
            mask_skipped: 0,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_gate_active(&mut self, gate_active: bool) {
        self.gate_active = gate_active;
    }

    /// Publish the owner-side rebound table. Takes effect on the next event;
    /// in-flight held keys keep their down-time slots: consumed holds ride
    /// their stored verdict to the paired key-up, and the Start-menu mask
    /// obligation survives, so no orphan key-up reaches the OS and no naked
    /// Win tap opens Start. Down-time physical routing survives a removed
    /// mapping through the paired release; held repeats keep their consumed
    /// verdict and dispatch only while the pinned route still matches.
    pub fn set_remap(&mut self, remap: Vec<ChordRemap>) {
        self.vk_remap = remap;
    }

    /// Publish the owner-side suppression table (disabled or rebound-away
    /// physical chords). Same in-flight contract as [`SnapClassify::set_remap`].
    pub fn set_disabled(&mut self, disabled: Vec<ChordDisable>) {
        self.vk_disabled = disabled;
    }

    /// True when the physical chord is suppressed: it passes through untracked
    /// instead of entering the classifier. Rebound chords never reach here
    /// (the rebound table wins); paired key-ups never reach here either (they
    /// ride their stored down verdict).
    fn is_suppressed(&self, vk: u32, shift: bool, ctrl: bool, alt: bool) -> bool {
        self.vk_disabled.iter().any(|entry| {
            entry.vk == vk && entry.shift == shift && entry.ctrl == ctrl && entry.alt == alt
        })
    }

    /// True for any physical key the classifier owns: catalog chord keys plus
    /// rebound sources. The callback's preheld-modifier guard and async Shift
    /// sync must treat rebound sources exactly like catalog keys.
    #[must_use]
    pub fn is_owned_physical(&self, vk: u32) -> bool {
        is_chord_vk(vk) || self.vk_remap.iter().any(|entry| entry.from_vk == vk)
    }

    /// Fresh-down routing for one physical chord at its live modifier state:
    /// rebound wins over suppression; suppression passes through; otherwise
    /// the physical key classifies as itself.
    fn fresh_route(&self, physical: u32, shift: bool, ctrl: bool, alt: bool) -> Route {
        if let Some(entry) = self.vk_remap.iter().find(|entry| {
            entry.from_vk == physical
                && entry.from_shift == shift
                && entry.from_ctrl == ctrl
                && entry.from_alt == alt
        }) {
            return Route::Remap(entry.action, entry.to_vk);
        }
        if self.is_suppressed(physical, shift, ctrl, alt) {
            return Route::Suppressed;
        }
        Route::Direct
    }

    /// True when the physical chord still routes to the pinned canonical
    /// route under the live tables. Pinned-hold repeats dispatch only while
    /// this holds; otherwise they stay swallowed without dispatching.
    fn route_matches(&self, physical: u32, pinned: PinnedRoute) -> bool {
        match self.fresh_route(physical, self.shift, self.ctrl, self.alt) {
            Route::Remap(action, target) => action == pinned.action && target == pinned.canon,
            Route::Direct => {
                physical == pinned.canon && self.direct_action(physical) == Some(pinned.action)
            }
            Route::Suppressed => false,
        }
    }

    /// Explicit action for a direct (non-remapped) physical chord at its live
    /// modifiers: item 2 relative send wins on Win+Ctrl+Shift, history wins on
    /// Win+Ctrl (no Shift/Alt), resize wins on Win+Alt([+Shift]) with no Ctrl,
    /// otherwise the existing VK-derived arm. `None` means the chord refuses
    /// (bare, extra-modified, or arm-refused) and leaves no pin. Fresh Alt is
    /// accepted only here on catalog keys through the documented resize arms;
    /// any other Alt chord (unknown key, extra Ctrl) refuses.
    fn direct_action(&self, physical: u32) -> Option<ChordAction> {
        let win = self.win_l || self.win_r;
        if !win {
            return None;
        }
        // Item 2 relative send: Win+Ctrl+Shift with no Alt on a relative key.
        // Direct chords follow; stay rides only the explicit stay actions
        // through the rebound table (unbound by default).
        if self.ctrl
            && self.shift
            && !self.alt
            && let Some(delta) = relative_send_delta(physical)
        {
            return Some(if delta < 0 {
                ChordAction::WorkspaceSendPrev
            } else {
                ChordAction::WorkspaceSendNext
            });
        }
        // Item 5 explicit output send: Win+Ctrl+Alt with no Shift on a catalog
        // key. Direct chords follow; stay rides only the explicit stay action
        // through the rebound table (unbound by default). A shifted
        // Win+Ctrl+Alt chord refuses here: only an explicit stay rebind routes
        // through the rebound table above.
        if self.ctrl && self.alt && catalog_index(physical).is_some() {
            if self.shift {
                return None;
            }
            return Some(ChordAction::WorkspaceOutputFollow);
        }
        // Item 1 history: Win+Ctrl with no Shift/Alt on a history key.
        if self.ctrl
            && !self.shift
            && !self.alt
            && let Some(op) = history_op_for_vk(physical)
        {
            return Some(match op {
                WorkspaceHistoryOp::Previous => ChordAction::WorkspacePrevious,
                WorkspaceHistoryOp::Prev => ChordAction::WorkspacePrev,
                WorkspaceHistoryOp::Next => ChordAction::WorkspaceNext,
            });
        }
        // Keyboard resize: Win+Alt+key grows outwards, Win+Shift+Alt+key
        // shrinks inwards, over the directional catalog (letters plus arrow
        // aliases). The mode rides the explicit bound action, never a move.
        // Extra Ctrl refuses here; an exact effective rebind still routes
        // through the rebound table above.
        if self.alt && !self.ctrl && catalog_index(physical).is_some() {
            return Some(resize_action_for_mode(resize_mode_for_shift(self.shift)));
        }
        if self.ctrl || self.alt {
            return None;
        }
        if catalog_index(physical).is_some() {
            return Some(ChordAction::Directional);
        }
        if is_digit_vk(physical) {
            return Some(ChordAction::WorkspaceDigit);
        }
        if is_maximize_vk(physical) && !self.shift {
            return Some(ChordAction::Maximize);
        }
        if is_fullscreen_vk(physical) && !self.shift {
            return Some(ChordAction::Fullscreen);
        }
        if is_orientation_vk(physical) && !self.shift {
            return Some(ChordAction::Orientation);
        }
        if is_workspace_tiling_vk(physical) && !self.shift {
            return Some(ChordAction::WorkspaceTiling);
        }
        if is_launch_browser_vk(physical) && !self.shift {
            return Some(ChordAction::LaunchBrowser);
        }
        if is_launch_terminal_vk(physical) && !self.shift {
            return Some(ChordAction::LaunchTerminal);
        }
        if is_float_vk(physical) {
            return Some(if self.shift {
                ChordAction::Sticky
            } else {
                ChordAction::Float
            });
        }
        None
    }

    fn pin_get(&self, physical: u32) -> Option<PinnedRoute> {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize]
        } else {
            None
        }
    }

    fn pin_set(&mut self, physical: u32, route: PinnedRoute) {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize] = Some(route);
        }
    }

    fn pin_take(&mut self, physical: u32) -> Option<PinnedRoute> {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize].take()
        } else {
            None
        }
    }

    /// Arm the Start-menu mask for a consumed project Win+Left gesture. The
    /// mouse hook consumes the click so no keyboard chord ever lands, but a
    /// physical Win hold still needs the E8 pair at Win-up or the OS opens
    /// Start. Idempotent per hold: a second call keeps the first trigger.
    /// NOOP absent tracked physical Win held: a synthetic Win hold never
    /// enters classifier tracking (injected input is filtered), so arming
    /// for it would persist pending and wrongly mask the next bare physical
    /// Win tap. The existing Win-up reserve fires it once per hold and the
    /// both-Wins terminal clears it, so no stale flag survives the hold.
    pub fn note_windrag_consumed(&mut self) {
        if !(self.win_l || self.win_r) {
            return;
        }
        if self.mask_trigger.is_none() {
            self.mask_trigger = Some(MaskTrigger::WinDrag);
        }
        self.mask_pending = true;
    }

    /// Tracked modifier state for the callback's preheld check
    /// `(ctrl, alt, shift)`. A physically held Ctrl/Alt the classifier never
    /// saw (held before install, or injected past the callback guard) forces
    /// pass-through to preserve unowned chords. Supported Shift binds from
    /// the official async sample instead (see `sync_shift_from_async`), so a
    /// preheld Shift correctly selects move/sticky/send rather than leaking.
    pub fn tracked_modifiers(&self) -> (bool, bool, bool) {
        (self.ctrl, self.alt, self.shift)
    }

    /// Bind supported Shift from the official async sample for a fresh chord
    /// down. Preheld Shift (held before install) still selects move/sticky/
    /// send; a missed Shift-up still resolves to focus. Holds already down
    /// keep their down-time op via stored origin, so syncing here never flips
    /// an armed hold.
    #[cfg(any(windows, test))]
    pub(crate) fn sync_shift_from_async(&mut self, async_shift: bool) {
        self.shift = async_shift;
    }

    /// Whether either Win key is currently held (tracked state).
    pub fn win_held(&self) -> bool {
        self.win_l || self.win_r
    }

    /// Whether a physical key currently holds a down without its up: its own
    /// live safe-refusal hold, else its own down-time pin resolved against
    /// the canonical slot it opened. Never reports another physical's
    /// canonical occupant: an unpressed rebound source reads false even
    /// while its canonical target is held elsewhere.
    pub fn key_is_down(&self, vk: u32) -> bool {
        if (vk as usize) < PIN_KEYS && self.collide[vk as usize].is_some() {
            return true;
        }
        match self.pin_get(vk) {
            Some(pinned) => self.canon_is_down(pinned.action, pinned.canon),
            None => false,
        }
    }

    /// Live shape of the hold owning a canonical route, for a colliding
    /// physical's safe refusal: same variant and action identity as the
    /// owner's down (the op stays pinned at the owner's down time, so a
    /// later modifier flip cannot rewrite it). `None` when no hold owns it.
    /// History arms own separate slots from directional/digit arms sharing
    /// one VK, so cross-arm holds never collide.
    fn held_shape(&self, action: ChordAction, canon: u32) -> Option<Classified> {
        match action {
            ChordAction::Directional => {
                let idx = catalog_index(canon)?;
                if !self.key_down[idx] {
                    return None;
                }
                Some(Classified::Snap(SnapIntent {
                    op: self.key_op[idx].unwrap_or(SnapOp::Focus),
                    direction: index_direction(idx),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.key_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                let idx = catalog_index(canon)?;
                if !self.resize_down[idx] {
                    return None;
                }
                Some(Classified::Resize(ResizeIntent {
                    mode: self.resize_mode[idx].unwrap_or(ResizeMode::Outwards),
                    direction: index_direction(idx),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.resize_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                if !is_digit_vk(canon) {
                    return None;
                }
                let slot = (canon - VK_0) as usize;
                if !self.digit_down[slot] {
                    return None;
                }
                Some(Classified::Workspace(WorkspaceIntent {
                    op: self.digit_op[slot].unwrap_or(WorkspaceOp::Select),
                    index: slot as u8,
                    follow: self.digit_follow[slot],
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.digit_origin[slot],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                let idx = relative_send_index(canon)?;
                if !self.send_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                    delta: self.send_delta[idx],
                    follow: self.send_follow[idx],
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.send_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceOutputFollow | ChordAction::WorkspaceOutputStay => {
                let idx = catalog_index(canon)?;
                if !self.output_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceOutput(WorkspaceOutputIntent {
                    direction: index_direction(idx),
                    follow: self.output_follow[idx],
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.output_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceMigrate => {
                let idx = migrate_index(canon)?;
                if !self.migrate_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceMigrate(WorkspaceMigrateIntent {
                    direction: migrate_direction(idx),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.migrate_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                let idx = history_index(canon)?;
                if !self.hist_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op: self.hist_op[idx].unwrap_or(WorkspaceHistoryOp::Prev),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.hist_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::Maximize => {
                if !self.maximize_down {
                    return None;
                }
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.maximize_origin,
                    announce: false,
                }))
            }
            ChordAction::Orientation => {
                if !self.orientation_down {
                    return None;
                }
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.orientation_origin,
                    announce: false,
                }))
            }
            ChordAction::WorkspaceTiling => {
                if !self.workspace_tiling_down {
                    return None;
                }
                Some(Classified::WorkspaceTiling(WorkspaceTilingIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.workspace_tiling_origin,
                    announce: false,
                }))
            }
            ChordAction::Fullscreen => {
                if !self.fullscreen_down {
                    return None;
                }
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.fullscreen_origin,
                    announce: false,
                }))
            }
            ChordAction::Float => {
                if !self.float_down {
                    return None;
                }
                Some(Classified::Float(FloatIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.float_origin,
                    announce: false,
                }))
            }
            ChordAction::Sticky => {
                if !self.sticky_down {
                    return None;
                }
                Some(Classified::Sticky(StickyIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.sticky_origin,
                    announce: false,
                }))
            }
            ChordAction::LaunchBrowser => {
                if !self.launch_browser_down {
                    return None;
                }
                Some(Classified::Launch(LaunchIntent {
                    kind: LaunchKind::Browser,
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.launch_browser_origin,
                    announce: false,
                }))
            }
            ChordAction::LaunchTerminal => {
                if !self.launch_terminal_down {
                    return None;
                }
                Some(Classified::Launch(LaunchIntent {
                    kind: LaunchKind::Terminal,
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.launch_terminal_origin,
                    announce: false,
                }))
            }
        }
    }

    /// Whether a canonical route currently holds a down without its up.
    /// History arms own separate slots from directional arms sharing one VK;
    /// resize arms share one slot across both modes (one physical key holds
    /// one resize at a time).
    fn canon_is_down(&self, action: ChordAction, vk: u32) -> bool {
        match action {
            ChordAction::Directional => catalog_index(vk).is_some_and(|idx| self.key_down[idx]),
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                catalog_index(vk).is_some_and(|idx| self.resize_down[idx])
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                is_digit_vk(vk) && self.digit_down[(vk - VK_0) as usize]
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                relative_send_index(vk).is_some_and(|idx| self.send_down[idx])
            }
            ChordAction::WorkspaceOutputFollow | ChordAction::WorkspaceOutputStay => {
                catalog_index(vk).is_some_and(|idx| self.output_down[idx])
            }
            ChordAction::WorkspaceMigrate => {
                migrate_index(vk).is_some_and(|idx| self.migrate_down[idx])
            }
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                history_index(vk).is_some_and(|idx| self.hist_down[idx])
            }
            ChordAction::Maximize => self.maximize_down,
            ChordAction::Orientation => self.orientation_down,
            ChordAction::WorkspaceTiling => self.workspace_tiling_down,
            ChordAction::Fullscreen => self.fullscreen_down,
            ChordAction::Float => self.float_down,
            ChordAction::Sticky => self.sticky_down,
            ChordAction::LaunchBrowser => self.launch_browser_down,
            ChordAction::LaunchTerminal => self.launch_terminal_down,
        }
    }

    fn note_win_down(&mut self, vk: u32) {
        let already = if vk == VK_LWIN {
            self.win_l
        } else {
            self.win_r
        };
        if already && self.hold_masked {
            // Naked Win auto-repeat after our mask re-arms the Start trigger.
            self.hold_masked = false;
            self.mask_pending = true;
        }
        if vk == VK_LWIN {
            self.win_l = true;
        } else {
            self.win_r = true;
        }
    }

    /// Physical Win-up bookkeeping + mask-fire decision. Returns the arming
    /// chord when the caller must send the E8 pair before passing this Win-up
    /// through. The Win-up itself always passes. `installed` is false on the
    /// state-only path, which updates bookkeeping without arming a send.
    pub(super) fn win_up_needs_mask(&mut self, vk: u32, installed: bool) -> Option<MaskTrigger> {
        if vk == VK_LWIN {
            self.win_l = false;
        } else {
            self.win_r = false;
        }
        let trigger = if installed && self.mask_pending && !self.hold_masked {
            self.mask_trigger
        } else {
            None
        };
        // A project Win+Left hold masks only at the terminal release: with
        // both Win sides genuinely held, the first Win-up keeps the hold
        // masked-pending instead of firing into a still-held Win (which
        // would leave the second release unmasked). Catalog chord triggers
        // keep the existing first-up policy; only WinDrag waits.
        if trigger == Some(MaskTrigger::WinDrag) && (self.win_l || self.win_r) {
            return None;
        }
        if trigger.is_some() {
            self.mask_pending = false;
            self.hold_masked = true;
        }
        if !(self.win_l || self.win_r) {
            self.mask_pending = false;
            self.hold_masked = false;
            self.mask_trigger = None;
        }
        trigger
    }

    pub fn push(
        &mut self,
        vk: u32,
        is_up: bool,
        foreground: bool,
        injected: bool,
    ) -> Option<Classified> {
        if injected {
            return None;
        }
        if is_win_vk(vk) {
            if is_up {
                let _ = self.win_up_needs_mask(vk, false);
            } else {
                self.note_win_down(vk);
            }
            return None;
        }
        if is_modifier_vk(vk) {
            let held = !is_up;
            match vk {
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
                    self.ctrl = held;
                }
                VK_MENU | VK_LMENU | VK_RMENU => {
                    self.alt = held;
                }
                _ => {
                    self.shift = held;
                }
            }
            return None;
        }
        // Down-time physical routing pinned through the matching up (see
        // `phys_pin`): fresh downs route live (rebound wins over
        // suppression); pinned-hold repeats ride the down-time routing with
        // dispatch gated on the chord still routing live, so a mid-hold
        // table change swallows without dispatching instead of leaking. The
        // Win+L fence keys on the physical chord only: a safe rebound into
        // the canonical L slot works without opt-in, while physical
        // unshifted Win+L stays gated.
        //
        // Physical identity safety: only a hold the family actually owns
        // pins. Unpinned key-ups always pass untracked (no remap/canonical
        // fallback), so an unrelated release can never consume or clear
        // another physical's slot. A fresh physical routing to an
        // already-held canonical is a collision, never the owner's repeat:
        // it is swallowed as a safe consumed refusal from the live owner's
        // hold shape with its own down-time verdict, paired repeat/up
        // included, never touching the canonical slot and never dispatching.
        let physical = vk;
        let pidx = physical as usize;
        if pidx < PIN_KEYS
            && let Some(tmpl) = self.collide[pidx]
        {
            if is_up {
                self.collide[pidx] = None;
                return Some(collide_refusal(
                    tmpl,
                    SnapEdge::Up,
                    foreground,
                    tmpl.consumed(),
                ));
            }
            if tmpl.consumed() && (self.win_l || self.win_r) {
                self.mask_pending = true;
                self.mask_trigger = collide_trigger(&tmpl);
            }
            return Some(collide_refusal(
                tmpl,
                SnapEdge::Repeat,
                foreground,
                tmpl.consumed(),
            ));
        }
        if is_up {
            // The pin proves this physical opened the hold; the family
            // closes its own paired slot below.
            let pinned = self.pin_take(physical)?;
            return self.push_owned(
                physical,
                pinned.canon,
                pinned.action,
                true,
                foreground,
                true,
            );
        }
        if let Some(pinned) = self.pin_get(physical) {
            let live = self.route_matches(physical, pinned);
            return self.push_owned(
                physical,
                pinned.canon,
                pinned.action,
                false,
                foreground,
                live,
            );
        }
        // Fresh down: refuse before pinning, so a refused chord leaves no
        // orphan pin and its paired key-up passes through as well. Win must
        // be held; modifiers select the arm (Ctrl selects history, existing
        // arms refuse Ctrl/Alt inside their family guards).
        if !(self.win_l || self.win_r) {
            return None;
        }
        let (action, canon, remapped) =
            match self.fresh_route(physical, self.shift, self.ctrl, self.alt) {
                Route::Suppressed => return None,
                Route::Remap(action, target) => (action, target, true),
                Route::Direct => {
                    let action = self.direct_action(physical)?;
                    (action, physical, false)
                }
            };
        // Unshifted physical Win+L is the OS lock chord: without explicit
        // opt-in it passes through untracked, so its paired key-up also
        // passes. The fence keys on the physical chord: a safe rebound
        // into the canonical L slot works without opt-in.
        if action == ChordAction::Directional
            && !self.shift
            && physical == VK_L
            && !self.allow_win_l
            && catalog_index(canon) == Some(3)
        {
            return None;
        }
        if self.canon_is_down(action, canon) {
            // Owned by a different physical: swallow as a safe consumed
            // refusal with its own down-time verdict. Passed while the gate
            // is off never holds: it passes through like a suppression.
            if !(self.enabled && self.gate_active) {
                return None;
            }
            let shape = self.held_shape(action, canon)?;
            let down = collide_refusal(shape, SnapEdge::Down, foreground, true);
            if pidx < PIN_KEYS {
                self.collide[pidx] = Some(down);
            }
            self.mask_pending = true;
            self.mask_trigger = collide_trigger(&shape);
            return Some(down);
        }
        if remapped || is_chord_vk(physical) {
            self.pin_set(physical, PinnedRoute { canon, action });
        }
        self.push_owned(physical, canon, action, false, foreground, true)
    }
}
