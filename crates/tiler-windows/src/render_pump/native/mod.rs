#![forbid(unsafe_code)]
//! `cfg(windows)` renderer thread: creator-side carrier lifecycle,
//! message pump, readiness/stop/failure reporting. All Win32 effects go
//! through safe [`crate::ffi`] primitives; all handoff state stays in safe
//! [`crate::render_handoff`] primitives.

use super::{FreshQuery, Present, PresentOutcome, RenderCore, ResolvedAnchor};
use crate::active_border_sys::{
    BorderOverlay, OverlayOutcome, PreviewOverlay, UnderlayOverlay, lowest_in_z,
};
use crate::native::HeldProcess;
use crate::render_handoff::{
    BorderDesired, FreshFacts, PreviewDesired, RenderCarrier, RenderChannels, RenderLifecycle,
    RenderOutcome, RenderStopReport, UnderlayCandidate, UnderlayDesired, WindowLifetime,
    render_slot_release, render_slot_try_reserve,
};
use std::os::windows::io::OwnedHandle;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{SyncSender, sync_channel},
};
use std::time::{Duration, Instant};

/// Native renderer backend: the three owned carriers live here, on the
/// renderer thread, for the whole renderer lifetime. Creation,
/// presentation, hide and destroy all run on this thread (creator
/// affinity); the owner only posts data.
pub struct NativePresent {
    border: BorderOverlay,
    underlay: UnderlayOverlay,
    preview: PreviewOverlay,
}

impl NativePresent {
    /// New backend with uncreated carriers (windows appear on the
    /// creator-side ensure before readiness, exactly like the
    /// same-thread path creates them on first show).
    #[must_use]
    pub fn new() -> Self {
        Self {
            border: BorderOverlay::default(),
            underlay: UnderlayOverlay::default(),
            preview: PreviewOverlay::default(),
        }
    }

    /// Creator-side ensure: create all three hidden carrier windows up
    /// front so readiness never signals before the carriers exist and
    /// the owner exclusion snapshot is armed before the first show.
    /// Windows are created hidden (never shown here). Fails closed with
    /// bounded detail; the caller retries narrowly with backoff.
    /// (Also serves the [`Present`] seam: the portable core re-ensures
    /// before every present so a mid-life loss recreates before the
    /// show, with the snapshot published first.)
    fn ensure_present(&mut self) -> Result<(), String> {
        self.border
            .ensure_created()
            .map_err(|detail| format!("error: border carrier create failed: {detail}"))?;
        self.underlay
            .ensure_created()
            .map_err(|detail| format!("error: underlay carrier create failed: {detail}"))?;
        self.preview
            .ensure_created()
            .map_err(|detail| format!("error: preview carrier create failed: {detail}"))?;
        Ok(())
    }

    /// All-false facts: unreadable or mismatched reads fail closed.
    fn dead_facts() -> FreshFacts {
        FreshFacts {
            alive: false,
            identity_ok: false,
            visible: false,
            zoom_ok: false,
            fullscreen: false,
            foreground_ok: false,
            band_ok: false,
        }
    }

    /// Identity match against the owner-observed lifetime: fresh pid,
    /// process creation, and visible-membership tag. Renderer read-only
    /// (process query plus a property read); never owner observation,
    /// Engine, or foreign writes. Unreadable fails closed.
    fn lifetime_matches(hwnd_u64: u64, expected: &WindowLifetime) -> bool {
        let pid = crate::ffi::window_pid(hwnd_u64);
        if pid == 0 || pid != expected.pid {
            return false;
        }
        let creation_ok = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .is_some_and(|ident| ident.process_creation == expected.creation);
        if !creation_ok {
            return false;
        }
        crate::product_hide::sys::read_member_tag(hwnd_u64).as_deref()
            == expected.member_tag.as_deref()
    }

    /// DWM cloak read on a live window: uncloaked only; query failure
    /// fails closed (not renderable, not visible).
    fn dwm_uncloaked(hwnd_u64: u64) -> bool {
        crate::ffi::cloaked(hwnd_u64) == Some(false)
    }

    /// Fresh renderable probe for one HWND: live, visible, non-iconic,
    /// non-cloaked. Mirrors the owner anchor probe so the renderer
    /// re-resolves over the same population the owner chose from.
    fn anchor_renderable(hwnd_u64: u64) -> bool {
        if hwnd_u64 == 0 {
            return false;
        }
        if !crate::ffi::is_window(hwnd_u64) {
            return false;
        }
        if !crate::ffi::is_visible(hwnd_u64) {
            return false;
        }
        if crate::ffi::is_iconic(hwnd_u64) {
            return false;
        }
        Self::dwm_uncloaked(hwnd_u64)
    }

    /// One enriched candidate validates individually: the pid fence is
    /// cheap and runs first, then the renderable probe, then maximized
    /// and captionless-fullscreen suppression, then the full lifetime
    /// identity (creation plus membership tag). Any failure fails that
    /// candidate closed; other candidates still resolve.
    fn candidate_renderable(
        candidate: &UnderlayCandidate,
        fulls: &[tiler_core::geometry::Rect],
    ) -> bool {
        if candidate.hwnd == 0 {
            return false;
        }
        let pid = crate::ffi::window_pid(candidate.hwnd);
        if pid == 0 || pid != candidate.lifetime.pid {
            return false;
        }
        if !Self::anchor_renderable(candidate.hwnd) {
            return false;
        }
        if crate::ffi::is_zoomed(candidate.hwnd) {
            return false;
        }
        if Self::captionless_fullscreen(candidate.hwnd, fulls).unwrap_or(true) {
            return false;
        }
        Self::lifetime_matches(candidate.hwnd, &candidate.lifetime)
    }

    /// Captionless-fullscreen predicate over the carried monitor fulls:
    /// the same pure [`crate::tiling::is_borderless_fullscreen`] check
    /// the owner path uses, evaluated renderer-side from a fresh style
    /// caption bit plus a fresh DWM extended-frame read. `None` when
    /// either read is unreadable (the caller fails closed).
    fn captionless_fullscreen(hwnd_u64: u64, fulls: &[tiler_core::geometry::Rect]) -> Option<bool> {
        use windows_sys::Win32::UI::WindowsAndMessaging::WS_CAPTION;
        if hwnd_u64 == 0 {
            return None;
        }
        let style = crate::ffi::window_style_checked(hwnd_u64)?;
        if style & WS_CAPTION != 0 {
            return Some(false);
        }
        let frame = crate::ffi::extended_frame_bounds(hwnd_u64)?;
        let visible = tiler_core::geometry::Rect {
            x: frame.left,
            y: frame.top,
            w: frame.right.saturating_sub(frame.left),
            h: frame.bottom.saturating_sub(frame.top),
        };
        Some(crate::tiling::is_borderless_fullscreen(
            true, visible, fulls,
        ))
    }

    /// Fresh `WS_EX_TOPMOST` band read on one HWND (read-only).
    fn read_topmost(hwnd_u64: u64) -> Option<bool> {
        use windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST;
        if hwnd_u64 == 0 {
            return None;
        }
        let band = crate::ffi::window_exstyle(hwnd_u64) & WS_EX_TOPMOST != 0;
        Some(band)
    }

    fn map_outcome(
        outcome: OverlayOutcome,
        checksum: u64,
        error: Option<String>,
        resolved: (u8, u8, u8),
        accent_sourced: bool,
        accent_query: Option<crate::active_border::AccentQueryReport>,
    ) -> PresentOutcome {
        match outcome {
            OverlayOutcome::Hidden => {
                let mut failed = PresentOutcome::failed(
                    error.unwrap_or_else(|| "error: renderer present failed".to_owned()),
                );
                failed.accent_query = accent_query;
                failed
            }
            OverlayOutcome::Shown => {
                let mut shown =
                    PresentOutcome::shown(RenderOutcome::Shown, checksum, resolved, accent_sourced);
                shown.accent_query = accent_query;
                shown
            }
            OverlayOutcome::Moved => {
                let mut shown =
                    PresentOutcome::shown(RenderOutcome::Moved, checksum, resolved, accent_sourced);
                shown.accent_query = accent_query;
                shown
            }
            OverlayOutcome::Redrew => {
                let mut shown = PresentOutcome::shown(
                    RenderOutcome::Redrew,
                    checksum,
                    resolved,
                    accent_sourced,
                );
                shown.accent_query = accent_query;
                shown
            }
            OverlayOutcome::Unchanged => {
                let mut shown = PresentOutcome::shown(
                    RenderOutcome::Unchanged,
                    checksum,
                    resolved,
                    accent_sourced,
                );
                shown.accent_query = accent_query;
                shown
            }
        }
    }
}

impl Default for NativePresent {
    fn default() -> Self {
        Self::new()
    }
}

impl Present for NativePresent {
    fn check_fresh(&self, carrier: RenderCarrier, hwnd: u64, query: &FreshQuery<'_>) -> FreshFacts {
        if hwnd == 0 {
            return Self::dead_facts();
        }
        if !crate::ffi::is_window(hwnd) {
            return Self::dead_facts();
        }
        // Recycled-HWND fence first: a live window with a dead identity
        // keeps `alive` but fails `identity_ok`, so the gate refuses
        // with the narrow recycled-ID reason instead of presenting.
        if !Self::lifetime_matches(hwnd, query.expected) {
            return FreshFacts {
                alive: true,
                identity_ok: false,
                visible: false,
                zoom_ok: false,
                fullscreen: false,
                foreground_ok: false,
                band_ok: false,
            };
        }
        // Visibility folds non-iconic and DWM-uncloaked, mirroring the
        // owner anchor probe: iconic, cloaked, or cloak-query failure
        // reads not visible (fail closed, never a weaker game-safety
        // gate than the same-thread path). An unreadable fullscreen
        // frame read fails the same way: async safety never trusts a
        // torn frame.
        let fs_read = if carrier == RenderCarrier::Preview {
            Some(false)
        } else {
            Self::captionless_fullscreen(hwnd, query.fulls)
        };
        let visible = crate::ffi::is_visible(hwnd)
            && !crate::ffi::is_iconic(hwnd)
            && Self::dwm_uncloaked(hwnd)
            && fs_read.is_some();
        let zoom_ok = !crate::ffi::is_zoomed(hwnd);
        // Renderer-evaluated suppression: the captionless-fullscreen
        // predicate over the carried monitor fulls, honoring only the
        // explicit owner bypass behind a tag-proven lifetime. Preview
        // movers are exempt (their plane is above; the foreground fence
        // below still guards the switch).
        let fullscreen = fs_read.is_some_and(|fs| {
            fs && !(query.fullscreen_bypass && query.expected.member_tag.is_some())
        });
        // Foreground fence on every carrier: the live foreground
        // lifetime must still match the desired foreground. A late
        // switch (game, suspend, another window) refuses independently
        // of mover focus. An unreadable post-time
        // foreground (`None`) refuses narrowly per the binding
        // contract: no unprovable stale show, while readable unchanged
        // foregrounds keep ordinary drag behavior.
        let foreground_ok = match query.foreground {
            None => false,
            Some(expected) => {
                let fg = crate::ffi::foreground_window();
                fg != 0 && Self::lifetime_matches(fg, expected)
            }
        };
        let band_ok = Self::read_topmost(hwnd).is_some_and(|band| band == query.topmost);
        FreshFacts {
            alive: true,
            identity_ok: true,
            visible,
            zoom_ok,
            fullscreen,
            foreground_ok,
            band_ok,
        }
    }

    fn check_own_anchor(
        &self,
        hwnd: u64,
        topmost: bool,
        foreground: Option<&WindowLifetime>,
    ) -> FreshFacts {
        if hwnd == 0 {
            return Self::dead_facts();
        }
        if !crate::active_border_sys::overlay_window_alive(hwnd) {
            return Self::dead_facts();
        }
        let visible = crate::ffi::is_visible(hwnd);
        let band_ok = Self::read_topmost(hwnd).is_some_and(|band| band == topmost);
        let fg = crate::ffi::foreground_window();
        let foreground_ok = match foreground {
            None => false,
            Some(expected) => fg != 0 && Self::lifetime_matches(fg, expected),
        };
        FreshFacts {
            alive: true,
            identity_ok: true,
            visible,
            zoom_ok: true,
            fullscreen: false,
            foreground_ok,
            band_ok,
        }
    }

    fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor> {
        // Each foreign candidate validates individually against fresh
        // native reads (pid fence, renderable probe, maximized and
        // fullscreen suppression, lifetime identity); stale Engine
        // members between reconciles fail closed here, never winning
        // the anchor.
        let mut live: Vec<u64> = frame
            .candidates
            .iter()
            .filter(|candidate| Self::candidate_renderable(candidate, &frame.fulls))
            .map(|candidate| candidate.hwnd)
            .collect();
        // The renderer's own border surface sorts into the same
        // comparison only when visible, same-group, and same-band: a
        // hidden or unrelated border never wins the anchor.
        let own = self
            .border
            .hwnd()
            .filter(|_| self.border.is_visible())
            .filter(|_| self.border.target_token().as_deref() == Some(frame.anchor_token.as_str()))
            .filter(|hwnd| Self::read_topmost(*hwnd) == Some(frame.topmost));
        if let Some(border) = own {
            live.push(border);
        }
        let anchor = lowest_in_z(&live)?;
        if Some(anchor) == own {
            Some(ResolvedAnchor::OwnBorder { hwnd: anchor })
        } else {
            let lifetime = frame
                .candidates
                .iter()
                .find(|candidate| candidate.hwnd == anchor)
                .map(|candidate| candidate.lifetime.clone())?;
            Some(ResolvedAnchor::Foreign {
                hwnd: anchor,
                lifetime,
            })
        }
    }

    fn read_band(&self, hwnd: u64) -> Option<bool> {
        Self::read_topmost(hwnd)
    }

    fn carrier_visible(&self, carrier: RenderCarrier) -> bool {
        match carrier {
            RenderCarrier::Border => self.border.is_visible(),
            RenderCarrier::Underlay => self.underlay.is_visible(),
            RenderCarrier::Preview => self.preview.is_visible(),
        }
    }

    fn carrier_token(&self, carrier: RenderCarrier) -> Option<String> {
        match carrier {
            RenderCarrier::Border => self.border.target_token(),
            RenderCarrier::Underlay => self.underlay.anchor_token(),
            RenderCarrier::Preview => self.preview.anchor_token(),
        }
    }

    fn ensure_created(&mut self) -> Result<(), String> {
        self.ensure_present()
    }

    fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome {
        // The renderer paints the posted shared snapshot: the frame carries
        // the configured fallback plus the theme choice and the
        // already-observed owner accent for the current broadcast version
        // (plain data), so an accent change repaints with no renderer-side
        // query. Geometry and style scale stay owner-resolved physical
        // pixels in the frame.
        let outcome = self.border.show_at(
            frame.outer,
            frame.style_px.0,
            frame.style_px.1,
            frame.style_px.2,
            &crate::active_border::ActiveBorderStyle {
                width: 0.0,
                gap: 0.0,
                radius: 0.0,
                color: frame.color,
                use_theme: frame.use_theme,
            },
            &frame.target_token,
            frame.topmost,
            frame.target_hwnd,
            frame.accent,
        );
        // Actual presented colour read back from the carrier (snapshot
        // resolved or fallback): the ack logs this, never the owner
        // fallback, so themed presents report honest pixels. The source
        // flag rides alongside so a source flip logs even at equal RGB.
        // No per-query diagnostic rides the ack: the single shared owner
        // resolve already logged its one record, and cache reads never
        // fabricate a query record.
        let resolved = self.border.current_color().unwrap_or(frame.color);
        Self::map_outcome(
            outcome,
            self.border.dib_checksum(),
            self.border.last_error().map(str::to_owned),
            resolved,
            self.border.from_accent(),
            None,
        )
    }

    fn show_underlay(
        &mut self,
        frame: &UnderlayDesired,
        anchor: u64,
        topmost: bool,
    ) -> PresentOutcome {
        let outcome = self.underlay.show_fill_at(
            frame.outer,
            frame.color,
            &frame.anchor_token,
            topmost,
            anchor,
        );
        let resolved = self
            .underlay
            .current_fill()
            .map(|(_, r, g, b)| (r, g, b))
            .unwrap_or((frame.color.1, frame.color.2, frame.color.3));
        Self::map_outcome(
            outcome,
            self.underlay.dib_checksum(),
            self.underlay.last_error().map(str::to_owned),
            resolved,
            false,
            None,
        )
    }

    fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome {
        let outcome = self.preview.show_fill_above(
            frame.outer,
            frame.color,
            &frame.anchor_token,
            frame.anchor_hwnd,
        );
        let resolved = self
            .preview
            .current_fill()
            .map(|(_, r, g, b)| (r, g, b))
            .unwrap_or((frame.color.1, frame.color.2, frame.color.3));
        Self::map_outcome(
            outcome,
            self.preview.dib_checksum(),
            self.preview.last_error().map(str::to_owned),
            resolved,
            false,
            None,
        )
    }

    fn hide(&mut self, carrier: RenderCarrier) -> bool {
        match carrier {
            RenderCarrier::Border => self.border.hide_verified(),
            RenderCarrier::Underlay => self.underlay.hide_verified(),
            RenderCarrier::Preview => self.preview.hide_verified(),
        }
    }

    fn destroy_all(&mut self) -> bool {
        // Verified creator-side destruction: each carrier keeps its live
        // handle on failure so a bounded retry can re-drive it; success
        // requires every HWND to read destroyed.
        let border_ok = self.border.destroy();
        let underlay_ok = self.underlay.destroy();
        let preview_ok = self.preview.destroy();
        border_ok && underlay_ok && preview_ok
    }

    fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64> {
        match carrier {
            RenderCarrier::Border => self.border.hwnd(),
            RenderCarrier::Underlay => self.underlay.hwnd(),
            RenderCarrier::Preview => self.preview.hwnd(),
        }
    }
}

/// Bounded renderer lifecycle event for the owner writer. The renderer
/// offers through a capacity-8 slot and drops (counted) under owner
/// stall; readiness and stop completion ride dedicated lossless slots.
/// The renderer never logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFailEvent {
    /// `"create"`, `"present"` or `"destroy"`.
    pub op: &'static str,
    /// Consecutive attempt number.
    pub attempt: u32,
    /// Backoff seconds until the next attempt (0 when none).
    pub retry_secs: u64,
    /// True when a previous failure recovered.
    pub recovered: bool,
    /// Bounded creator-side cause (`None` on recovery notes).
    pub detail: Option<String>,
}

/// Bounded renderer -> owner signals. `ready` and `stopped` are lossless
/// single slots; `failures` is a bounded FIFO (drops counted, never
/// blocking the renderer).
pub struct RenderSignals {
    ready: AtomicBool,
    stopped: std::sync::Mutex<Option<bool>>,
    failures: SyncSender<RenderFailEvent>,
    /// Failure events dropped under owner stall (bounded evidence).
    pub events_dropped: AtomicU64,
}

impl RenderSignals {
    /// New signals with an empty bounded failure FIFO.
    #[must_use]
    pub fn new(failures: SyncSender<RenderFailEvent>) -> Self {
        Self {
            ready: AtomicBool::new(false),
            stopped: std::sync::Mutex::new(None),
            failures,
            events_dropped: AtomicU64::new(0),
        }
    }

    /// Note readiness (idempotent, lossless).
    pub fn note_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Readiness snapshot.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Record one lifecycle event without blocking; counts drops.
    pub fn note_failure(&self, event: RenderFailEvent) {
        if self.failures.try_send(event).is_err() {
            self.events_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record stop completion (terminal, lossless).
    pub fn note_stopped(&self, destroyed_ok: bool) {
        *self.stopped.lock().expect("stopped lock") = Some(destroyed_ok);
    }

    /// Stop completion snapshot.
    #[must_use]
    pub fn stopped(&self) -> Option<bool> {
        *self.stopped.lock().expect("stopped lock")
    }
}

/// Create one auto-reset kernel event, initially non-signaled.
fn create_event() -> Result<OwnedHandle, String> {
    crate::ffi::create_auto_reset_event().ok_or_else(|| {
        let code = crate::ffi::process::last_error();
        format!("error: render wake creation failed: win32-{code}")
    })
}

/// Duplicate one wake handle for the renderer thread: each side owns its
/// lifetime, so a stuck stop that drops the owner handle can never leave
/// the renderer signaling a recycled kernel object.
fn duplicate_event(handle: &OwnedHandle) -> Result<OwnedHandle, String> {
    handle
        .try_clone()
        .map_err(|error| format!("error: render wake duplicate failed: {error}"))
}

/// Signal one owned wake best-effort. Never waits, never logs.
fn signal_owned(handle: &OwnedHandle) {
    crate::ffi::signal_event(handle);
}

/// Owner handle for exactly one renderer thread. All logging stays on
/// the owner: the renderer reports readiness, bounded failure events,
/// per-carrier acks and generation echoes for the owner writer.
pub struct RenderHandle {
    channels: Arc<RenderChannels>,
    signals: Arc<RenderSignals>,
    failures: std::sync::mpsc::Receiver<RenderFailEvent>,
    lifecycle: RenderLifecycle,
    thread: Option<std::thread::JoinHandle<RenderStopReport>>,
    /// Completed stop report: makes a second `wait_stopped` return the
    /// same report instead of panicking on the consumed join handle.
    stop_report: Option<RenderStopReport>,
    /// Renderer -> owner ack wake (auto-reset, owner-owned): the owner
    /// outer wait includes it alongside the input wake. Always present:
    /// spawn refuses cleanly when creation fails, so event-driven
    /// startup is never claimed over a polling fallback.
    ack_wake: OwnedHandle,
    /// Owner -> renderer work wake (auto-reset, owner-owned): signaled
    /// after every desired post, hide, and stop request. Always present
    /// for the same reason.
    work_wake: OwnedHandle,
}

impl RenderHandle {
    /// Spawn the dedicated renderer thread. `Err` before any product
    /// effect on wake-creation failure, thread-spawn failure (the owner
    /// logs and continues degraded with same-thread overlays), or while
    /// another renderer holds the process slot (a hung or unclean stop,
    /// including a dropped handle, keeps the reserve so no overlapping
    /// worker can start; recovery is process exit); otherwise the worker
    /// starts in `Starting` and the owner must [`RenderHandle::wait_ready`]
    /// with a bounded wait. The slot is never released on `Drop`: only
    /// a clean stop or a spawn failure frees it. Both cross-thread wakes
    /// are duplicated so each thread owns its handle lifetime.
    pub fn spawn() -> Result<Self, String> {
        if !render_slot_try_reserve() {
            return Err("error: render thread already active".to_owned());
        }
        let fail = |cause: String| {
            render_slot_release();
            cause
        };
        let channels = Arc::new(RenderChannels::new());
        let (fail_tx, fail_rx) = sync_channel(8);
        let signals = Arc::new(RenderSignals::new(fail_tx));
        // Event-driven wakes are required, not best-effort: a creation
        // failure refuses the spawn cleanly (like the input wake) so
        // the owner never claims event-driven behavior over polling.
        let ack_wake = create_event().map_err(fail)?;
        let work_wake = create_event().map_err(fail)?;
        let thread_ack = duplicate_event(&ack_wake).map_err(fail)?;
        let thread_work = duplicate_event(&work_wake).map_err(fail)?;
        let thread_channels = Arc::clone(&channels);
        let thread_signals = Arc::clone(&signals);
        let thread = std::thread::Builder::new()
            .name("omnitiler-render".to_owned())
            .spawn(move || run_renderer(thread_channels, thread_signals, thread_ack, thread_work))
            .map_err(|err| {
                render_slot_release();
                format!("error: render thread spawn failed: {err}")
            })?;
        let mut lifecycle = RenderLifecycle::new();
        lifecycle.start().expect("fresh handle starts");
        Ok(Self {
            channels,
            signals,
            failures: fail_rx,
            lifecycle,
            thread: Some(thread),
            stop_report: None,
            ack_wake,
            work_wake,
        })
    }

    /// Shared channels for owner post/poll/wait.
    #[must_use]
    pub fn channels(&self) -> &Arc<RenderChannels> {
        &self.channels
    }

    /// Current lifecycle state.
    #[must_use]
    pub fn state(&self) -> crate::render_handoff::RenderState {
        self.lifecycle.state()
    }

    /// Borrow the renderer -> owner ack wake. The owner outer wait
    /// includes it alongside the input wake.
    #[must_use]
    pub fn ack_wake(&self) -> &OwnedHandle {
        &self.ack_wake
    }

    /// Signal the owner -> renderer work wake (new desired, hide, or
    /// stop): the renderer pump breaks its quantum early. Never waits,
    /// never logs.
    pub fn notify_work(&self) {
        signal_owned(&self.work_wake);
    }

    /// Bounded readiness wait. Drains failure events for the owner
    /// writer while waiting. Returns true when the renderer reported
    /// ready (lifecycle `Running`): carriers exist and the exclusion
    /// snapshot is published. False on timeout or when the creator
    /// thread exited first (lifecycle `create_failed`, no worker holds
    /// resources so a later start is safe) without claiming readiness.
    pub fn wait_ready(
        &mut self,
        timeout: Duration,
        on_event: &mut dyn FnMut(&RenderFailEvent),
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            self.drain_failures(on_event);
            if self.signals.is_ready() {
                let _ = self.lifecycle.note_ready();
                return true;
            }
            if self.lifecycle.state() == crate::render_handoff::RenderState::Running {
                return true;
            }
            // The creator thread exited before readiness (or reported
            // stop): wire the creator-side failure into the lifecycle
            // instead of waiting out the bound for a dead thread.
            if self.signals.stopped().is_some() {
                let _ = self.lifecycle.create_failed();
                return false;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Drain pending lifecycle events without waiting (owner tick).
    pub fn drain_failures(&mut self, on_event: &mut dyn FnMut(&RenderFailEvent)) {
        while let Ok(event) = self.failures.try_recv() {
            on_event(&event);
        }
        if self.signals.is_ready() {
            let _ = self.lifecycle.note_ready();
        }
    }

    /// Request graceful stop: closes admission (owner stops posting) and
    /// signals the renderer through the work wake. Idempotent in
    /// `Stopping` so the owner can re-drive after a failure. The renderer
    /// destroys on its creator thread and acks.
    pub fn request_stop(&mut self) -> Result<(), crate::render_handoff::RenderLifecycleError> {
        self.lifecycle.request_stop()?;
        self.channels.request_stop();
        // Wake the pump so a fully idle wait observes the stop promptly;
        // owner call sites must not signal again.
        signal_owned(&self.work_wake);
        Ok(())
    }

    /// Bounded stop wait. Clean requires the destroy acknowledgement
    /// *and* a successful join: a thread that panicked after its ack, or
    /// a join that fails, reports unclean even when the ack claimed
    /// success, so clean is never fabricated from an ack alone. The
    /// renderer records its stop acknowledgement as its terminal act
    /// before returning, so a finished thread joins immediately. With no
    /// ack the thread joins only when already finished (`is_finished`
    /// first, so a wedged thread never blocks the owner past the bound);
    /// a still-running thread keeps its handle (no detach) and the call
    /// returns unclean while retaining `Stopping`, so no overlapping
    /// worker can start and a later call can still join. Only a clean
    /// stop releases the process render slot; every unclean path keeps
    /// the reserve, including a dropped handle. A second call returns
    /// the stored report instead of panicking. Never waits indefinitely
    /// and never holds channel locks while joining.
    pub fn wait_stopped(&mut self, timeout: Duration) -> RenderStopReport {
        if let Some(report) = self.stop_report {
            return report;
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(destroyed_ok) = self.signals.stopped() {
                // Ack arrived: complete only against a finished thread
                // whose join succeeds. A thread wedged between its ack
                // and its return, or one that panicked, keeps unclean:
                // the handle is retained for a later bounded wait (or
                // the panic is recorded) instead of blocking this call
                // past the deadline or fabricating clean.
                if let Some(thread) = self.thread.take() {
                    if thread.is_finished() {
                        let joined_ok = thread
                            .join()
                            .map(|report| report.destroyed_ok)
                            .unwrap_or(false);
                        let report = self.complete_stop(destroyed_ok && joined_ok);
                        self.stop_report = Some(report);
                        return report;
                    }
                    self.thread = Some(thread);
                } else {
                    let report = self.complete_stop(destroyed_ok);
                    self.stop_report = Some(report);
                    return report;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(thread) = self.thread.take() {
            if thread.is_finished() {
                let destroyed_ok = thread
                    .join()
                    .map(|report| report.destroyed_ok)
                    .unwrap_or(false);
                let report = self.complete_stop(destroyed_ok);
                self.stop_report = Some(report);
                return report;
            }
            // Still running (hung native call): keep the handle so a
            // later bounded wait can still join; never detach.
            self.thread = Some(thread);
        }
        RenderStopReport {
            destroyed_ok: false,
            clean: false,
        }
    }

    /// Fold one destroy acknowledgement into the lifecycle, releasing
    /// the process slot only on a clean stop.
    fn complete_stop(&mut self, destroyed_ok: bool) -> RenderStopReport {
        match self.lifecycle.note_destroyed(destroyed_ok) {
            Ok(report) => {
                if report.clean {
                    render_slot_release();
                }
                report
            }
            Err(_) => RenderStopReport {
                destroyed_ok,
                clean: false,
            },
        }
    }
}

/// Native renderer-wait return reason: exactly one per wait. Counts
/// native `MsgWaitForMultipleObjectsEx` returns only, never thread
/// context switches and never ack posts (acks ride `ack_seq`). `Timeout`
/// is the genuine hide-retry deadline (`WAIT_TIMEOUT` 258) only; `Failed`
/// is `WAIT_FAILED` or any unrecognized code, counted separately and never
/// merged into timeout. Classification is telemetry only: the loop keeps
/// its existing path with no new recovery and no timing claim (a failed
/// wait returns immediately; not proven delayed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderWait {
    Work,
    Message,
    Timeout,
    Failed,
}

/// Classify one native renderer-wait return code. Pure (no native calls)
/// so tests prove the work/message/timeout/failed split without a desktop.
#[must_use]
fn classify_render_wait(result: u32) -> RenderWait {
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    if result == WAIT_TIMEOUT {
        RenderWait::Timeout
    } else if result == WAIT_OBJECT_0 {
        RenderWait::Work
    } else if result == WAIT_OBJECT_0 + 1 {
        RenderWait::Message
    } else {
        RenderWait::Failed
    }
}

/// One renderer message turn on the creator thread: delivers broadcast
/// messages (accent/theme) for windows created here, then waits for more,
/// owner work, or the next hide-retry deadline. Windows created on this
/// thread require a queue on this thread; no other thread pumps it. The
/// thread-owned work wake breaks the wait early on new desired frames,
/// hides, or stop; otherwise the hide-retry deadline (or an indefinite
/// idle wait) applies. No fixed idle quantum: every wait source is an
/// explicit wake or deadline. Returns the native wait reason (one per
/// wait) for the cumulative trace counters.
fn pump_messages(timeout_ms: u32, work_wake: &OwnedHandle) -> RenderWait {
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
    crate::ffi::drain_messages();
    let raw = work_wake.as_handle().as_raw_handle();
    let result = crate::ffi::msg_wait_raw(std::slice::from_ref(&raw), timeout_ms);
    let reason = classify_render_wait(result);
    crate::ffi::drain_messages();
    reason
}

/// Renderer thread entry: creator-side ensure with narrow bounded retry
/// (carriers exist before readiness, and the exclusion snapshot is
/// published before the first show), then pump broadcasts, drain latest
/// frames, ack applied state, and destroy every carrier on this creator
/// thread at stop with verified readbacks. Both wakes are thread-owned
/// duplicates, so neither outlives the owner handle. Returns the
/// truthful stop report; never logs (owner writer owns all
/// timing/outcome lines from acks and signals).
fn run_renderer(
    channels: Arc<RenderChannels>,
    signals: Arc<RenderSignals>,
    ack_wake: OwnedHandle,
    work_wake: OwnedHandle,
) -> RenderStopReport {
    let mut core = RenderCore::new(NativePresent::new(), Arc::clone(&channels));
    // Creation attempts ride bounded backoff (2s, 4s, 8s cap) with one
    // failure event per attempt (bounded cause included) for the owner
    // writer; a stop request wins over the next retry. Recovery after
    // attempts reports a recovered event so the owner log pairs
    // unavailable/available.
    let mut attempt: u32 = 0;
    let ready = loop {
        match core.present_mut().ensure_present() {
            Ok(()) => {
                core.publish_all();
                if attempt > 0 {
                    signals.note_failure(RenderFailEvent {
                        op: "create",
                        attempt,
                        retry_secs: 0,
                        recovered: true,
                        detail: None,
                    });
                }
                signals.note_ready();
                break true;
            }
            Err(detail) => {
                attempt += 1;
                let backoff = (1u64 << attempt.min(3)).min(8);
                signals.note_failure(RenderFailEvent {
                    op: "create",
                    attempt,
                    retry_secs: backoff,
                    recovered: false,
                    detail: Some(crate::render_handoff::truncate_render_error(&detail)),
                });
                let until = Instant::now() + Duration::from_secs(backoff);
                while Instant::now() < until {
                    if channels.stop_requested() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if channels.stop_requested() {
                    break false;
                }
            }
        }
    };
    if ready {
        // Deadline-driven pump: the wait breaks on owner work, queued
        // broadcast messages, or the next hide-retry deadline - never
        // on a fixed idle quantum. Every ack post signals the owner
        // wake on real ack traffic only (tracked by sequence, never a
        // busy loop); owner work breaks the wait early through its wake.
        let mut last_seq = channels.ack_seq();
        // Accent-only fanout baseline: the overlay procedure on this
        // thread bumps the shared version when an accent/theme broadcast
        // lands here while the owner sleeps.
        let mut last_accent = crate::ffi::accent_version();
        loop {
            let timeout_ms = match core.next_timer() {
                Some(due) => due
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u128::from(u32::MAX)) as u32,
                // Fully idle: INFINITE. The work wake and the message
                // queue still wake the pump; nothing polls.
                None => u32::MAX,
            };
            let reason = pump_messages(timeout_ms, &work_wake);
            // Cumulative native wait returns (one per wait, by reason);
            // ack posts stay on `ack_seq`, never here. A stop wait still
            // counts its return: the break below only skips the step.
            // Failed returns keep the existing path (step, then stop
            // check); only the counter differs.
            match reason {
                RenderWait::Work => channels.note_wait_work(),
                RenderWait::Message => channels.note_wait_msg(),
                RenderWait::Timeout => channels.note_wait_timeout(),
                RenderWait::Failed => channels.note_wait_failed(),
            }
            if channels.stop_requested() {
                break;
            }
            core.step();
            let seq = channels.ack_seq();
            if seq != last_seq {
                last_seq = seq;
                signal_owned(&ack_wake);
            }
            // Accent-only fanout: a broadcast reached this thread's
            // overlay windows while the owner slept (no desired frame was
            // posted, so the step above presents nothing and posts no
            // ack). Signal the existing ack wake so the owner promotes to
            // a full refresh turn and reposts from fresh observation; no
            // ack payload is faked and no stale frame replays.
            if crate::active_border::accent_version_moved(
                &mut last_accent,
                crate::ffi::accent_version(),
            ) {
                signal_owned(&ack_wake);
            }
        }
    }
    // Creator-side destruction with bounded retry: failed destroys keep
    // their live handles inside the carriers, so each re-drive retries
    // the same windows. Report truthfully and keep the slot on failure.
    let mut destroyed_ok = core.present_mut().destroy_all();
    for _ in 0..2 {
        if destroyed_ok {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        destroyed_ok = core.present_mut().destroy_all();
    }
    signals.note_stopped(destroyed_ok);
    RenderStopReport {
        destroyed_ok,
        clean: destroyed_ok,
    }
}

/// Handle-level tests without a live renderer thread (no carrier windows,
/// no native effects): the stop wake is asserted as bare kernel state.
#[cfg(test)]
mod native_tests;
