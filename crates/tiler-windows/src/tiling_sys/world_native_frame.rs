#![forbid(unsafe_code)]
//! Mechanical native frame actuation for the world route.
//!
//! [`LiveNative::frame_write_inner`](super::LiveNative) is the shared
//! host-mechanical frame write behind every tiled placement: fresh
//! eligibility, scope, hosted-child, identity, and lifetime gates, the
//! visible-to-outer conversion, one `SetWindowPos`, plus the single bounded
//! cross-DPI reassertion when the window DPI changes under the call.

use super::LiveNative;
use crate::model::ProcessIdentity;
use crate::native_guards::WindowKey;
use tiler_core::geometry::Rect;

impl<'a> LiveNative<'a> {
    /// Shared host-mechanical frame write: fresh eligibility, scope,
    /// hosted-child fence, full identity, and lifetime tag. Row-local
    /// drift refuses without writes (`Ok(false)`); torn state across the
    /// call is ambiguous (`Err`). The setter boolean is never proof: the
    /// caller re-reads the exact frame before claiming arrival.
    ///
    /// Cross-monitor writes can change the window's DPI under the call while
    /// the app framework synchronously rescales the frame to preserve logical
    /// size (position kept): the pre-write observation (insets plus DPI basis)
    /// is then invalidated placement, never an app-held clamp. The write
    /// re-observes fresh and reasserts the same intended visible rectangle
    /// through the fresh insets once, under the same gates as the initial
    /// write. A second flip falls through to the tick readback lanes, so
    /// flapping DPI terminates without timers or counters.
    pub(super) fn frame_write_inner(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
        rect: Rect,
    ) -> Result<bool, ()> {
        // TOCTOU revalidation before the setter: fresh eligibility, scope,
        // hosted-child fence, full identity, and lifetime tag.
        let w = (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens)
            .map_err(|_| ())?;
        if w.hwnd != key.hwnd
            || w.identity.pid != key.pid
            || w.identity.process_creation != key.creation
        {
            return Err(());
        }
        if !crate::tiling::scope_allows(self.scope, &w.identity.exe_path) {
            return Ok(false);
        }
        if !super::super::inspection::hosted_gate_allows(
            &w.identity.exe_path,
            key.hwnd,
            key.pid,
            self.scope_hosts,
        ) {
            return Ok(false);
        }
        if !self.identity_ok(key, expected) {
            return Err(());
        }
        if !self.lifetime_ok(key) {
            return Err(());
        }
        if self.write_vetoed(key.hwnd) {
            return Ok(false);
        }
        if crate::tiling::classify(&w.facts).is_err() {
            // No window authority lane (cloaked, elevated, shell, tool,
            // owned, dialog, no-activate, hidden, minimized): overlays keep
            // membership but take no tiled writes here.
            return Ok(false);
        }
        let dpi_before = (self.prims.dpi)(key.hwnd);
        let Some(outer) = w.insets.visible_to_outer(rect) else {
            return Ok(false);
        };
        let ok = (self.prims.set_outer)(key.hwnd, outer);
        if (self.prims.window_pid)(key.hwnd) != key.pid {
            return Err(());
        }
        if !ok {
            return Ok(false);
        }
        // Stable DPI (or unreadable on either side, never guessed): the
        // single write stands and the caller proves it by readback.
        let dpi_after = (self.prims.dpi)(key.hwnd);
        let changed = matches!(
            (dpi_before, dpi_after),
            (Some(before), Some(after)) if before != after
        );
        if !changed {
            return Ok(true);
        }
        // Invalidated placement: re-observe fresh (never the stale frame)
        // and reassert the same intended rectangle through the fresh insets.
        // Any torn state across the transition is ambiguous, never success.
        let fresh = match (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens) {
            Ok(fresh) => fresh,
            Err(_) => return Err(()),
        };
        if fresh.hwnd != key.hwnd
            || fresh.identity.pid != key.pid
            || fresh.identity.process_creation != key.creation
        {
            return Err(());
        }
        if crate::tiling::classify(&fresh.facts).is_err() {
            // Freshly ineligible across the transition (hidden, minimized,
            // cloaked, owned, privileged, ...): no correction, the final
            // readback decides.
            return Ok(true);
        }
        let Some(corrected) =
            crate::tiling::dpi_transition_reouter(dpi_before, dpi_after, &rect, &fresh.insets)
        else {
            return Ok(true);
        };
        // Same gates as the initial write before the correction: racing
        // input, torn identity/lifetime, or a newly vetoed foreground holds
        // instead of rewriting.
        if (self.prims.hook_pending)() {
            return Err(());
        }
        if !self.identity_ok(key, expected) || !self.lifetime_ok(key) {
            return Err(());
        }
        if self.write_vetoed(key.hwnd) {
            return Ok(true);
        }
        let corrected_ok = (self.prims.set_outer)(key.hwnd, corrected);
        if (self.prims.window_pid)(key.hwnd) != key.pid {
            return Err(());
        }
        Ok(corrected_ok)
    }
}
