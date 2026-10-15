use super::*;

/// Send target selector resolved once before transfer (item 2): numbered
/// digits resolve through the existing ordinal/trailing resolvers (index 0
/// reuses or appends the trailing empty); relative steps resolve from the
/// mover's source workspace through the item 1 scoped ring (wrapping through
/// the trailing empty and ordinals beyond 9, never MRU, never creating).
/// Filling the trailing empty invokes ordinary lifecycle for the next spare
/// after transfer; the frozen target never re-resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendTarget {
    Numbered(u8),
    Relative(i32),
}

impl SendTarget {
    pub(super) fn log_index(self) -> u8 {
        match self {
            Self::Numbered(index) => index,
            Self::Relative(_) => 0,
        }
    }

    pub(super) fn log_op(self) -> &'static str {
        match self {
            Self::Numbered(_) => "send",
            Self::Relative(delta) if delta < 0 => "send-prev",
            Self::Relative(_) => "send-next",
        }
    }
}

/// Carried pre-transfer send fences (item 2 + item 20): the pure fence
/// snapshot (both gaps, both modes, overlay flags) plus the retained-row
/// fullscreen fallback for unreadable live frames (never fabricated).
/// Tails re-verify before source writes AND hide/focus; any drift refuses
/// with no replay.
#[derive(Debug, Clone, Copy)]
pub(super) struct SendPreflight {
    pub(super) fences: crate::workspace_owner::SendFenceSnapshot,
    pub(super) fallback_fullscreen: bool,
}

impl SendPreflight {
    /// Live tail-time readings for [`crate::workspace_owner::send_fences_hold`]:
    /// fresh view/mode/gap/scope/lifetime/membership/overlay state. Scope and
    /// membership are in-memory session reads; lifetime re-reads the live
    /// member tag; overlay re-reads the live OS flags against the monitor
    /// full rects with the carried fallback for unreadable frames.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn live(
        &self,
        state: &TileLoop,
        stored: &ProcessIdentity,
        mover_key: &crate::workspace::WindowKey,
        mover_hwnd: u64,
        output: &str,
        source_id: &str,
        target_id: &str,
        fulls: &[Rect],
    ) -> crate::workspace_owner::SendFenceLive {
        let active = state.workspaces.active_id(output);
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        crate::workspace_owner::SendFenceLive {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled: state.workspaces.is_tiled(output, source_id),
            target_tiled: state.workspaces.is_tiled(output, target_id),
            active_is_source: active.as_deref() == Some(source_id),
            target_is_active: active.as_deref() == Some(target_id),
            scope_ok: scope_allows(&state.scope, &stored.exe_path)
                && hosted_gate_allows(&stored.exe_path, mover_hwnd, stored.pid, &state.scope_hosts),
            lifetime_ok: {
                let live_tag = live_member_tag(mover_hwnd);
                state.member_tags.get(mover_key).is_some_and(|tag| {
                    crate::workspace_owner::visible_lifetime_ok(tag, live_tag.as_deref())
                })
            },
            membership_ok: state
                .workspaces
                .member_loc(mover_key)
                .is_some_and(|loc| loc.workspace == target_id),
            overlay_maximized: live_max,
            overlay_fullscreen: live_full_opt.unwrap_or(self.fallback_fullscreen),
        }
    }

    /// Live overlay-only recheck before native effects (item 9): a newly
    /// arrived fullscreen refuses, other drift defers. Narrower than
    /// [`crate::workspace_owner::send_fences_hold`]; follow tails (whose
    /// selects re-verify everything else) use this.
    pub(super) fn overlay_hold(
        &self,
        mover_hwnd: u64,
        fulls: &[Rect],
    ) -> std::result::Result<(), &'static str> {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        crate::workspace_owner::send_overlay_gate(
            self.fences.overlay_maximized,
            self.fences.overlay_fullscreen,
            live_max,
            live_full_opt,
            self.fallback_fullscreen,
        )
    }
}
