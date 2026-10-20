//! Live winit backend.
//!
//! The main thread owns BOTH the [`Server`] and every [`Window`]. IO threads
//! never touch shared window state: each connection sends a whole protocol
//! request line as one [`MainJob`] that wakes the loop via
//! [`EventLoopProxy`], the main thread handles it against the server and the
//! live windows, and the IO thread waits for the reply outside any state
//! lock. There is no shared `Server` mutex at all, so a window event can
//! never deadlock against a request round trip: both serialize as sequential
//! main-thread callbacks. Redraws and state events happen only on host
//! notifications and completed jobs, never on a polling timer.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Fullscreen, Window, WindowId};

#[cfg(target_os = "linux")]
use crate::protocol::APP_ID;
use crate::protocol::{
    Attention, FieldOutcome, Fields, Observed, Theme, WindowButton, WindowLevel, WindowMode,
    X11WindowType,
};
use crate::state::{
    ApplyReport, Capabilities, CreatedWindow, Host, HostEvent, Server, capabilities_for_backend,
    logical_size_for, restore_blocked,
};

/// Reply to one protocol request, sent back to the waiting IO thread.
pub(crate) struct RequestReply {
    pub reply: String,
    pub shutting_down: bool,
}

/// Work for the main thread: a whole protocol request, or a disconnect
/// notice so the server can forget that connection's subscription.
pub(crate) enum MainJob {
    Request {
        line: String,
        conn: u64,
        events_tx: mpsc::Sender<String>,
        reply_tx: mpsc::SyncSender<RequestReply>,
    },
    Disconnect {
        conn: u64,
    },
}

/// IO-side handle: submit whole requests and wait outside any state lock.
#[derive(Clone)]
pub(crate) struct MainSender {
    jobs: mpsc::Sender<MainJob>,
    proxy: EventLoopProxy<()>,
}

impl MainSender {
    pub(crate) fn new(jobs: mpsc::Sender<MainJob>, proxy: EventLoopProxy<()>) -> Self {
        Self { jobs, proxy }
    }

    /// Send one request line to the main thread and block for its reply.
    /// No state lock is held while waiting: the main thread owns the server.
    pub(crate) fn submit_request(
        &self,
        line: String,
        conn: u64,
        events_tx: &mpsc::Sender<String>,
    ) -> Option<RequestReply> {
        let (tx, rx) = mpsc::sync_channel(1);
        if self
            .jobs
            .send(MainJob::Request {
                line,
                conn,
                events_tx: events_tx.clone(),
                reply_tx: tx,
            })
            .is_err()
        {
            return None;
        }
        if self.proxy.send_event(()).is_err() {
            return None;
        }
        rx.recv().ok()
    }

    pub(crate) fn submit_disconnect(&self, conn: u64) {
        let _ = self.jobs.send(MainJob::Disconnect { conn });
        let _ = self.proxy.send_event(());
    }

    /// Wake the event loop so a pending shutdown is noticed.
    pub(crate) fn wake(&self) {
        let _ = self.proxy.send_event(());
    }
}

struct LiveWindow {
    caller_id: String,
    window: Arc<Window>,
    surface: Option<softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>>,
    scroll_rows: usize,
    requested: Fields,
    /// Last accepted minimize request. Drives the shared restore gate;
    /// never derived from `is_minimized()`, which is always `None` on
    /// Wayland and could therefore never fire the gate.
    minimized: bool,
}

/// Window storage. The main thread owns this together with the [`Server`];
/// [`LiveHost`] adapts it to [`Host`] for one request using the active loop.
struct Windows {
    backend_shared: Arc<OnceLock<String>>,
    windows: HashMap<WindowId, LiveWindow>,
    ids: HashMap<String, WindowId>,
    /// Set when the pixel context fails: spawns are then refused honestly
    /// instead of producing blank windows.
    renderer_error: Option<String>,
}

/// One accepted outcome for a field the host took.
fn accept(outcomes: &mut Vec<FieldOutcome>, field: &str) {
    outcomes.push(FieldOutcome::accepted(field));
}

/// One refused outcome plus its persistent snapshot tag.
fn deny(
    outcomes: &mut Vec<FieldOutcome>,
    unsupported: &mut Vec<String>,
    field: &str,
    tag: &str,
    detail: impl Into<String>,
) {
    outcomes.push(FieldOutcome::unsupported(field, detail));
    unsupported.push(tag.to_string());
}

fn map_window_level(level: WindowLevel) -> winit::window::WindowLevel {
    match level {
        WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
        WindowLevel::Normal => winit::window::WindowLevel::Normal,
        WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
    }
}

fn map_attention(attention: Attention) -> Option<winit::window::UserAttentionType> {
    match attention {
        Attention::Informational => Some(winit::window::UserAttentionType::Informational),
        Attention::Critical => Some(winit::window::UserAttentionType::Critical),
        Attention::Clear => None,
    }
}

fn map_theme(theme: Theme) -> Option<winit::window::Theme> {
    match theme {
        Theme::System => None,
        Theme::Light => Some(winit::window::Theme::Light),
        Theme::Dark => Some(winit::window::Theme::Dark),
    }
}

fn map_buttons(buttons: &[WindowButton]) -> winit::window::WindowButtons {
    let mut out = winit::window::WindowButtons::empty();
    for button in buttons {
        out |= match button {
            WindowButton::Close => winit::window::WindowButtons::CLOSE,
            WindowButton::Minimize => winit::window::WindowButtons::MINIMIZE,
            WindowButton::Maximize => winit::window::WindowButtons::MAXIMIZE,
        };
    }
    out
}

/// Pure renderer gate, unit-tested below: no pixel context means no windows.
fn renderer_gate(renderer_error: &Option<String>) -> Result<(), String> {
    match renderer_error {
        Some(e) => Err(format!("renderer unavailable, spawn refused: {e}")),
        None => Ok(()),
    }
}

impl Windows {
    fn backend_name(&self) -> String {
        self.backend_shared
            .get()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn capabilities(&self) -> Capabilities {
        capabilities_for_backend(&self.backend_name())
    }

    #[allow(clippy::too_many_lines)]
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: &str,
        title: &str,
        requested: &Fields,
    ) -> Result<CreatedWindow, String> {
        renderer_gate(&self.renderer_error)?;
        let caps = self.capabilities();
        let backend = self.backend_name();
        let mut unsupported = Vec::new();
        let mut outcomes: Vec<FieldOutcome> = Vec::new();
        let mut attrs = winit::window::WindowAttributes::default().with_title(title);

        // Identity: exact sentinel class on X11, exact app_id on Wayland.
        #[cfg(target_os = "linux")]
        {
            use winit::platform::wayland::WindowAttributesExtWayland;
            use winit::platform::x11::{ActiveEventLoopExtX11, WindowAttributesExtX11};
            if event_loop.is_x11() {
                attrs = WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID);
                let kind = requested.x11_window_type.unwrap_or(X11WindowType::Normal);
                attrs = attrs.with_x11_window_type(crate::platform::winit_x11_types(kind));
                if requested.x11_window_type.is_some() {
                    accept(&mut outcomes, "x11_window_type");
                }
                if let Some(redirect) = requested.override_redirect {
                    attrs = attrs.with_override_redirect(redirect);
                    accept(&mut outcomes, "override_redirect");
                }
            } else {
                attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
                if requested
                    .x11_window_type
                    .is_some_and(|k| k != X11WindowType::Normal)
                {
                    deny(
                        &mut outcomes,
                        &mut unsupported,
                        "x11_window_type",
                        "x11_window_type",
                        "creation-only window type is only supported on X11",
                    );
                } else if requested.x11_window_type.is_some() {
                    accept(&mut outcomes, "x11_window_type");
                }
                if requested.override_redirect == Some(true) {
                    deny(
                        &mut outcomes,
                        &mut unsupported,
                        "override_redirect",
                        "override_redirect",
                        "override-redirect is X11-only",
                    );
                } else if requested.override_redirect.is_some() {
                    accept(&mut outcomes, "override_redirect");
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            if requested
                .x11_window_type
                .is_some_and(|k| k != X11WindowType::Normal)
            {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "x11_window_type",
                    "x11_window_type",
                    "creation-only window type is only supported on X11",
                );
            } else if requested.x11_window_type.is_some() {
                accept(&mut outcomes, "x11_window_type");
            }
            if requested.override_redirect == Some(true) {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "override_redirect",
                    "override_redirect",
                    "override-redirect is X11-only",
                );
            } else if requested.override_redirect.is_some() {
                accept(&mut outcomes, "override_redirect");
            }
        }

        if let Some(size) = requested.size_px {
            attrs = attrs.with_inner_size(PhysicalSize::new(size[0], size[1]));
            accept(&mut outcomes, "size_px");
        }
        // Hints are physical pixels throughout: no scale conversion, no
        // re-application on scale changes, correct on high DPI.
        if let Some(min) = requested.min_size_px {
            attrs = attrs.with_min_inner_size(PhysicalSize::new(min[0], min[1]));
            accept(&mut outcomes, "min_size_px");
        }
        if let Some(max) = requested.max_size_px {
            attrs = attrs.with_max_inner_size(PhysicalSize::new(max[0], max[1]));
            accept(&mut outcomes, "max_size_px");
        }
        if let Some(resizable) = requested.resizable {
            attrs = attrs.with_resizable(resizable);
            accept(&mut outcomes, "resizable");
        }
        if let Some(decorations) = requested.decorations {
            attrs = attrs.with_decorations(decorations);
            accept(&mut outcomes, "decorations");
        }
        // Visibility is a no-op on Wayland: only honor explicit requests
        // where the backend supports them, and report the rest.
        if let Some(visible) = requested.visible {
            if caps.visible {
                attrs = attrs.with_visible(visible);
                accept(&mut outcomes, "visible");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "visible",
                    "visible",
                    "showing/hiding windows is not supported on Wayland",
                );
            }
        }
        let mode = requested.mode.unwrap_or(WindowMode::Windowed);
        match mode {
            WindowMode::Windowed | WindowMode::Minimized => {
                accept(&mut outcomes, "mode");
            }
            WindowMode::Maximized => {
                attrs = attrs.with_maximized(true);
                accept(&mut outcomes, "mode");
            }
            WindowMode::BorderlessFullscreen => {
                attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
                accept(&mut outcomes, "mode");
            }
            WindowMode::ExclusiveFullscreen => {
                if caps.exclusive_fullscreen {
                    match exclusive_mode(event_loop) {
                        Some(video_mode) => {
                            attrs = attrs.with_fullscreen(Some(Fullscreen::Exclusive(video_mode)));
                            accept(&mut outcomes, "mode");
                        }
                        None => deny(
                            &mut outcomes,
                            &mut unsupported,
                            "mode",
                            "mode:exclusive_fullscreen",
                            "no display video mode available for exclusive fullscreen",
                        ),
                    }
                } else {
                    deny(
                        &mut outcomes,
                        &mut unsupported,
                        "mode",
                        "mode:exclusive_fullscreen",
                        "exclusive fullscreen is not supported on this backend",
                    );
                }
            }
        }
        if let Some(pos) = requested.position_outer_px {
            if caps.position {
                attrs = attrs.with_position(PhysicalPosition::new(pos[0], pos[1]));
                accept(&mut outcomes, "position_outer_px");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "position_outer_px",
                    "position_outer_px",
                    "global position is not supported on Wayland",
                );
            }
        }
        if requested.description.is_some() {
            // Rendered inside the window, never host-sent.
            accept(&mut outcomes, "description");
        }
        if requested.title.is_some() {
            // The full server-built title is already the window title.
            accept(&mut outcomes, "title");
        }
        if let Some(level) = requested.window_level {
            if caps.window_level || level == WindowLevel::Normal {
                attrs = attrs.with_window_level(map_window_level(level));
                accept(&mut outcomes, "window_level");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "window_level",
                    "window_level",
                    "stacking level is not supported on Wayland",
                );
            }
        }
        if let Some(buttons) = requested.enabled_buttons.as_deref() {
            if caps.enabled_buttons {
                attrs = attrs.with_enabled_buttons(map_buttons(buttons));
                accept(&mut outcomes, "enabled_buttons");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "enabled_buttons",
                    "enabled_buttons",
                    "window button hints are not implemented on X11/Wayland",
                );
            }
        }
        if let Some(increments) = requested.resize_increments_px {
            if caps.resize_increments {
                attrs =
                    attrs.with_resize_increments(PhysicalSize::new(increments[0], increments[1]));
                accept(&mut outcomes, "resize_increments_px");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "resize_increments_px",
                    "resize_increments_px",
                    "resize increments have no protocol support on Wayland",
                );
            }
        }
        if let Some(theme) = requested.theme {
            attrs = attrs.with_theme(map_theme(theme));
            accept(&mut outcomes, "theme");
        }
        if let Some(transparent) = requested.transparent {
            // Creation-time 32-bit visual on X11, runtime hint elsewhere.
            // The software renderer stays opaque: this forwards the host
            // hint only, it never demonstrates alpha rendering.
            attrs = attrs.with_transparent(transparent);
            accept(&mut outcomes, "transparent");
        }
        if let Some(blur) = requested.blur {
            if caps.blur || !blur {
                attrs = attrs.with_blur(blur);
                accept(&mut outcomes, "blur");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "blur",
                    "blur",
                    "blur is not supported on this backend",
                );
            }
        }

        let window = event_loop
            .create_window(attrs)
            .map_err(|e| format!("window creation failed: {e}"))?;
        if mode == WindowMode::Minimized {
            window.set_minimized(true);
        }
        // Post-creation actions: no winit creation attribute exists for
        // these, so they run against the fresh window. Outcomes stay in
        // field order (focus/attention/cursor) exactly like the fake host.
        if let Some(focus) = requested.focus {
            if focus && !caps.focus {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "focus",
                    "focus",
                    "focus requests are not supported on Wayland",
                );
            } else {
                if focus {
                    window.focus_window();
                }
                accept(&mut outcomes, "focus");
            }
        }
        if let Some(attention) = requested.attention {
            if attention == Attention::Clear && backend == "wayland" {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "attention",
                    "attention:clear",
                    "clearing attention has no effect on Wayland: urgency is compositor-owned and setting it requires xdg_activation_v1",
                );
            } else {
                // Informational/critical on Wayland forwards to the
                // compositor activation request, which needs the
                // xdg_activation_v1 protocol; without it winit logs and the
                // call is a no-op. Accepted means sent, never confirmed.
                window.request_user_attention(map_attention(attention));
                accept(&mut outcomes, "attention");
            }
        }
        if let Some(visible) = requested.cursor_visible {
            window.set_cursor_visible(visible);
            accept(&mut outcomes, "cursor_visible");
        }
        if let Some(hittest) = requested.cursor_hittest {
            match window.set_cursor_hittest(hittest) {
                Ok(()) => accept(&mut outcomes, "cursor_hittest"),
                Err(e) => deny(
                    &mut outcomes,
                    &mut unsupported,
                    "cursor_hittest",
                    "cursor_hittest",
                    format!("host refused cursor hit-test change: {e}"),
                ),
            }
        }

        let native_handle = describe_handle(&window);
        let observed = read_observed(&window, &backend);
        let window = Arc::new(window);
        window.request_redraw();
        self.ids.insert(id.to_string(), window.id());
        self.windows.insert(
            window.id(),
            LiveWindow {
                caller_id: id.to_string(),
                window: Arc::clone(&window),
                surface: None,
                scroll_rows: 0,
                requested: requested.clone(),
                minimized: mode == WindowMode::Minimized,
            },
        );
        drop(window);
        Ok(CreatedWindow {
            key: id.to_string(),
            backend,
            native_handle,
            title: title.to_string(),
            observed,
            configured: requested.clone(),
            unsupported,
            outcomes,
        })
    }

    fn apply_patch(&mut self, id: &str, patch: &Fields, full_title: Option<&str>) -> ApplyReport {
        let caps = self.capabilities();
        let mut outcomes: Vec<FieldOutcome> = Vec::new();
        let mut unsupported = Vec::new();
        let wid = match self.ids.get(id) {
            Some(wid) => *wid,
            None => {
                return ApplyReport {
                    outcomes,
                    observed: Observed::default(),
                    configured: Fields::default(),
                    unsupported,
                };
            }
        };
        // The shared restore gate keys off the last accepted minimize
        // request, never off `is_minimized()` (always `None` on Wayland, so
        // the gate could never fire there). A refused restore skips only the
        // mode change; every other patch field still applies below.
        let restore_refused = self
            .windows
            .get(&wid)
            .is_some_and(|live| restore_blocked(&caps, live.minimized, patch.mode));
        let mut skip_mode = false;
        if restore_refused {
            outcomes.push(FieldOutcome::unsupported(
                "mode",
                "restoring a minimized window is not supported on this backend",
            ));
            unsupported.push("mode:restore".to_string());
            skip_mode = true;
        }
        let live = match self.windows.get(&wid) {
            Some(live) => live,
            None => {
                return ApplyReport {
                    outcomes,
                    observed: Observed::default(),
                    configured: Fields::default(),
                    unsupported,
                };
            }
        };
        let window = Arc::clone(&live.window);

        if let Some(size) = patch.size_px {
            // The return is the size the host will try; observed values arrive
            // via Resized notifications, so only the request is recorded here.
            let _ = window.request_inner_size(PhysicalSize::new(size[0], size[1]));
            outcomes.push(FieldOutcome::accepted("size_px"));
        }
        if let Some(min) = patch.min_size_px {
            window.set_min_inner_size(Some(PhysicalSize::new(min[0], min[1])));
            outcomes.push(FieldOutcome::accepted("min_size_px"));
        }
        if let Some(max) = patch.max_size_px {
            window.set_max_inner_size(Some(PhysicalSize::new(max[0], max[1])));
            outcomes.push(FieldOutcome::accepted("max_size_px"));
        }
        if let Some(resizable) = patch.resizable {
            window.set_resizable(resizable);
            outcomes.push(FieldOutcome::accepted("resizable"));
        }
        if let Some(decorations) = patch.decorations {
            // Sent to the host; on negotiated-decoration backends the host may
            // keep its own choice. Readback below reports only real state.
            window.set_decorations(decorations);
            outcomes.push(FieldOutcome::accepted("decorations"));
        }
        if let Some(pos) = patch.position_outer_px {
            if caps.position {
                window.set_outer_position(PhysicalPosition::new(pos[0], pos[1]));
                outcomes.push(FieldOutcome::accepted("position_outer_px"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "position_outer_px",
                    "global position is not supported on Wayland",
                ));
                unsupported.push("position_outer_px".to_string());
            }
        }
        if let Some(visible) = patch.visible {
            if caps.visible {
                window.set_visible(visible);
                outcomes.push(FieldOutcome::accepted("visible"));
            } else {
                outcomes.push(FieldOutcome::unsupported(
                    "visible",
                    "showing/hiding windows is not supported on Wayland",
                ));
                unsupported.push("visible".to_string());
            }
        }
        if let Some(mode) = patch.mode {
            if skip_mode {
                // Already reported above; do not touch the mode.
            } else {
                apply_mode(&window, mode, caps, &mut outcomes, &mut unsupported);
            }
        }
        if patch.description.is_some() {
            // Rendered inside the window on the next redraw, not a host call.
            outcomes.push(FieldOutcome::accepted("description"));
        }
        if let Some(kind) = patch.x11_window_type {
            // Creation-only: report, never recreate the window.
            outcomes.push(FieldOutcome::unsupported(
                "x11_window_type",
                format!("creation-only (requested {kind:?}); close and spawn again to change it"),
            ));
            unsupported.push("x11_window_type".to_string());
        }
        if let Some(title) = full_title {
            // Server-constructed full title; the host setter cannot fail.
            window.set_title(title);
            accept(&mut outcomes, "title");
        }
        if let Some(level) = patch.window_level {
            if caps.window_level || level == WindowLevel::Normal {
                window.set_window_level(map_window_level(level));
                accept(&mut outcomes, "window_level");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "window_level",
                    "window_level",
                    "stacking level is not supported on Wayland",
                );
            }
        }
        if let Some(focus) = patch.focus {
            if focus && !caps.focus {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "focus",
                    "focus",
                    "focus requests are not supported on Wayland",
                );
            } else {
                if focus {
                    window.focus_window();
                }
                accept(&mut outcomes, "focus");
            }
        }
        if let Some(attention) = patch.attention {
            let backend_name = self.backend_name();
            if attention == Attention::Clear && backend_name == "wayland" {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "attention",
                    "attention:clear",
                    "clearing attention has no effect on Wayland: urgency is compositor-owned and setting it requires xdg_activation_v1",
                );
            } else {
                window.request_user_attention(map_attention(attention));
                accept(&mut outcomes, "attention");
            }
        }
        if let Some(visible) = patch.cursor_visible {
            window.set_cursor_visible(visible);
            accept(&mut outcomes, "cursor_visible");
        }
        if let Some(hittest) = patch.cursor_hittest {
            match window.set_cursor_hittest(hittest) {
                Ok(()) => accept(&mut outcomes, "cursor_hittest"),
                Err(e) => deny(
                    &mut outcomes,
                    &mut unsupported,
                    "cursor_hittest",
                    "cursor_hittest",
                    format!("host refused cursor hit-test change: {e}"),
                ),
            }
        }
        if let Some(buttons) = patch.enabled_buttons.as_deref() {
            if caps.enabled_buttons {
                window.set_enabled_buttons(map_buttons(buttons));
                accept(&mut outcomes, "enabled_buttons");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "enabled_buttons",
                    "enabled_buttons",
                    "window button hints are not implemented on X11/Wayland",
                );
            }
        }
        if let Some(increments) = patch.resize_increments_px {
            if caps.resize_increments {
                window.set_resize_increments(Some(PhysicalSize::new(increments[0], increments[1])));
                accept(&mut outcomes, "resize_increments_px");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "resize_increments_px",
                    "resize_increments_px",
                    "resize increments have no protocol support on Wayland",
                );
            }
        }
        if let Some(theme) = patch.theme {
            window.set_theme(map_theme(theme));
            accept(&mut outcomes, "theme");
        }
        if let Some(transparent) = patch.transparent {
            if caps.transparent_runtime {
                window.set_transparent(transparent);
                accept(&mut outcomes, "transparent");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "transparent",
                    "transparent",
                    "transparency is creation-only on X11: close and spawn again to change it",
                );
            }
        }
        if let Some(blur) = patch.blur {
            if caps.blur || !blur {
                window.set_blur(blur);
                accept(&mut outcomes, "blur");
            } else {
                deny(
                    &mut outcomes,
                    &mut unsupported,
                    "blur",
                    "blur",
                    "blur is not supported on this backend",
                );
            }
        }
        if let Some(redirect) = patch.override_redirect {
            // Creation-only everywhere: report, never recreate the window.
            deny(
                &mut outcomes,
                &mut unsupported,
                "override_redirect",
                "override_redirect",
                format!(
                    "creation-only (requested {redirect:?}); close and spawn again to change it"
                ),
            );
        }

        let backend = self.backend_name();
        let live = match self.windows.get_mut(&wid) {
            Some(live) => live,
            None => {
                return ApplyReport {
                    outcomes,
                    observed: Observed::default(),
                    configured: Fields::default(),
                    unsupported,
                };
            }
        };
        crate::state::merge_for_host(&mut live.requested, patch);
        if let Some(mode) = patch.mode
            && outcomes.iter().any(|o| {
                o.field == "mode" && matches!(o.status, crate::protocol::FieldStatus::Accepted)
            })
        {
            live.minimized = mode == WindowMode::Minimized;
        }
        let observed = read_observed(&live.window, &backend);
        live.window.request_redraw();
        // `configured` is filtered by the server to accepted host-sent fields
        // only; the full patch is returned here so the server can decide.
        ApplyReport {
            outcomes,
            observed,
            configured: patch.clone(),
            unsupported,
        }
    }

    fn observe(&self, id: &str) -> Option<Observed> {
        let wid = self.ids.get(id)?;
        let backend = self.backend_name();
        self.windows
            .get(wid)
            .map(|live| read_observed(&live.window, &backend))
    }

    fn destroy(&mut self, id: &str) {
        if let Some(wid) = self.ids.remove(id) {
            // Dropping the window and its surface closes the host window.
            self.windows.remove(&wid);
        }
    }
}

/// One-request [`Host`] over the main-thread window storage. Created fresh
/// for each request inside `user_event`, where the active loop is available.
struct LiveHost<'a> {
    windows: &'a mut Windows,
    event_loop: &'a ActiveEventLoop,
}

impl Host for LiveHost<'_> {
    fn create(
        &mut self,
        id: &str,
        title: &str,
        requested: &Fields,
    ) -> Result<CreatedWindow, String> {
        self.windows
            .create_window(self.event_loop, id, title, requested)
    }

    fn apply(&mut self, key: &str, patch: &Fields, full_title: Option<&str>) -> ApplyReport {
        self.windows.apply_patch(key, patch, full_title)
    }

    fn observe(&mut self, key: &str) -> Option<Observed> {
        self.windows.observe(key)
    }

    fn destroy(&mut self, key: &str) {
        self.windows.destroy(key);
    }
}

struct App {
    server: Server,
    windows: Windows,
    shutdown: Arc<AtomicBool>,
    jobs: mpsc::Receiver<MainJob>,
    context: Option<softbuffer::Context<winit::event_loop::OwnedDisplayHandle>>,
}

impl App {
    fn handle_main_job(&mut self, event_loop: &ActiveEventLoop, job: MainJob) {
        match job {
            MainJob::Request {
                line,
                conn,
                events_tx,
                reply_tx,
            } => {
                let mut host = LiveHost {
                    windows: &mut self.windows,
                    event_loop,
                };
                let reply = self
                    .server
                    .handle_line(&mut host, &line, conn, Some(&events_tx));
                let shutting_down = self.server.is_shutting_down();
                if shutting_down {
                    self.shutdown.store(true, Ordering::SeqCst);
                }
                let _ = reply_tx.send(RequestReply {
                    reply,
                    shutting_down,
                });
            }
            MainJob::Disconnect { conn } => {
                self.server.remove_connection(conn);
            }
        }
    }

    fn render(&mut self, window: WindowId) {
        let caller = match self.windows.windows.get(&window) {
            Some(live) => live.caller_id.clone(),
            None => return,
        };
        let lines = self.server.render_lines(&caller);
        let context = match &self.context {
            Some(context) => context,
            None => return,
        };
        let live = match self.windows.windows.get_mut(&window) {
            Some(live) => live,
            None => return,
        };
        let size = live.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        if live.surface.is_none() {
            match softbuffer::Surface::new(context, Arc::clone(&live.window)) {
                Ok(surface) => live.surface = Some(surface),
                Err(e) => {
                    self.server
                        .note_error(Some(&caller), format!("pixel surface failed: {e}"));
                    return;
                }
            }
        }
        let surface = live.surface.as_mut().expect("surface was just created");
        let (width, height) = (size.width, size.height);
        let width_nz = NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN);
        let height_nz = NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN);
        if let Err(e) = surface.resize(width_nz, height_nz) {
            self.server
                .note_error(Some(&caller), format!("pixel surface resize failed: {e}"));
            return;
        }
        let cols = (width / crate::render::GLYPH_W).max(1) as usize;
        let wrapped = crate::render::wrap_lines(&lines, cols);
        let pixels = crate::render::rasterize(&wrapped, width, height, live.scroll_rows);
        match surface.buffer_mut() {
            Ok(mut buffer) => {
                buffer.copy_from_slice(&pixels);
                if let Err(e) = buffer.present() {
                    self.server
                        .note_error(Some(&caller), format!("present failed: {e}"));
                }
            }
            Err(e) => {
                self.server
                    .note_error(Some(&caller), format!("pixel buffer failed: {e}"));
            }
        }
    }
}

fn exclusive_mode(event_loop: &ActiveEventLoop) -> Option<winit::monitor::VideoModeHandle> {
    let monitor = event_loop
        .available_monitors()
        .next()
        .or_else(|| event_loop.primary_monitor())?;
    monitor.video_modes().next()
}

fn apply_mode(
    window: &Window,
    mode: WindowMode,
    caps: Capabilities,
    outcomes: &mut Vec<FieldOutcome>,
    unsupported: &mut Vec<String>,
) {
    // Restoring from minimized means explicitly un-minimizing first: no
    // caller before this change ever called `set_minimized(false)`, so a
    // minimized window could never come back on supporting backends.
    match mode {
        WindowMode::Windowed => {
            window.set_fullscreen(None);
            window.set_maximized(false);
            window.set_minimized(false);
            outcomes.push(FieldOutcome::accepted("mode"));
        }
        WindowMode::Maximized => {
            window.set_fullscreen(None);
            window.set_maximized(true);
            window.set_minimized(false);
            outcomes.push(FieldOutcome::accepted("mode"));
        }
        WindowMode::Minimized => {
            window.set_minimized(true);
            outcomes.push(FieldOutcome::accepted("mode"));
        }
        WindowMode::BorderlessFullscreen => {
            window.set_minimized(false);
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            outcomes.push(FieldOutcome::accepted("mode"));
        }
        WindowMode::ExclusiveFullscreen => {
            if !caps.exclusive_fullscreen {
                outcomes.push(FieldOutcome::unsupported(
                    "mode",
                    "exclusive fullscreen is not supported on this backend",
                ));
                unsupported.push("mode:exclusive_fullscreen".to_string());
                return;
            }
            // A display mode must actually exist; without one there is nothing
            // honest to request, so report unsupported instead of a fake mode.
            let monitor = window
                .current_monitor()
                .or_else(|| window.primary_monitor());
            match monitor.and_then(|m| m.video_modes().next()) {
                Some(video_mode) => {
                    window.set_minimized(false);
                    window.set_fullscreen(Some(Fullscreen::Exclusive(video_mode)));
                    outcomes.push(FieldOutcome::accepted("mode"));
                }
                None => {
                    outcomes.push(FieldOutcome::unsupported(
                        "mode",
                        "no display video mode available for exclusive fullscreen",
                    ));
                    unsupported.push("mode:exclusive_fullscreen".to_string());
                }
            }
        }
    }
}

/// Read back only what the host offers, never the request echoed.
///
/// Focus comes from the real `has_focus` readback on every backend, so
/// `Focused` window events refresh it without claiming geometry feedback.
/// Fullscreen is trustworthy only where the backend reports the compositor
/// state: on Wayland `fullscreen()` reflects the last configure. On X11 the
/// getter is a setter cache (winit only writes it on the request path, never
/// from window-manager events), so a set fullscreen there stays null
/// (unknown) in `observed` while `configured` keeps the sent value.
/// Maximized comes from [`Window::is_maximized`]; minimized and visible are
/// `Option` readbacks where `None` means unknown and stays null on the wire.
/// Decorations, resizable and min/max hints have no trustworthy
/// cross-backend readback, so they stay out of `observed` and live in
/// `configured` (labeled cache) instead.
fn read_observed(window: &Window, backend: &str) -> Observed {
    let size = window.inner_size();
    let scale = window.scale_factor();
    let position = window.outer_position().ok().map(|p| [p.x, p.y]);
    // Verified in winit 0.30.13 source: Wayland `fullscreen()` reads
    // `last_configure.is_fullscreen()` (platform_impl/linux/wayland/
    // window/mod.rs), while X11 `fullscreen()` returns
    // `desired_fullscreen.or(fullscreen)` written only by `set_fullscreen`
    // (platform_impl/linux/x11/window.rs), i.e. a cache, not a readback.
    let compositor_fullscreen = backend == "wayland";
    let mode = match window.fullscreen() {
        Some(Fullscreen::Borderless(_)) if compositor_fullscreen => {
            Some(WindowMode::BorderlessFullscreen)
        }
        Some(Fullscreen::Exclusive(_)) if compositor_fullscreen => {
            Some(WindowMode::ExclusiveFullscreen)
        }
        Some(_) => None,
        None => {
            if window.is_maximized() {
                Some(WindowMode::Maximized)
            } else {
                match window.is_minimized() {
                    Some(true) => Some(WindowMode::Minimized),
                    Some(false) => Some(WindowMode::Windowed),
                    // Wayland reports no minimize state: unknown, not an
                    // echo of the request.
                    None => None,
                }
            }
        }
    };
    Observed {
        size_px: Some([size.width, size.height]),
        position_outer_px: position,
        mode,
        visible: window.is_visible(),
        scale_factor: Some(scale),
        logical_size_px: Some(logical_size_for([size.width, size.height], scale)),
        focused: Some(window.has_focus()),
    }
}

/// Safe native-handle diagnostic: debug formatting only, no raw pointer use.
fn describe_handle(window: &Window) -> String {
    match window.window_handle() {
        Ok(handle) => format!("{:?}", handle.as_raw()),
        Err(e) => format!("unavailable:{e}"),
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "linux")]
        {
            use winit::platform::x11::ActiveEventLoopExtX11;
            let backend = if event_loop.is_x11() {
                "x11"
            } else {
                "wayland"
            };
            let _ = self.windows.backend_shared.set(backend.to_string());
        }
        #[cfg(target_os = "windows")]
        {
            let _ = self.windows.backend_shared.set("windows".to_string());
        }
        #[cfg(target_os = "macos")]
        {
            let _ = self.windows.backend_shared.set("macos".to_string());
        }
        if self.context.is_none() && self.windows.renderer_error.is_none() {
            match softbuffer::Context::new(event_loop.owned_display_handle()) {
                Ok(context) => self.context = Some(context),
                Err(e) => {
                    // No silent blank windows: record the failure so spawns
                    // are refused honestly, and say so on stderr.
                    let message = format!("pixel context failed: {e}");
                    eprintln!("tiler-live-fixture: {message}");
                    self.windows.renderer_error = Some(message);
                }
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        while let Ok(job) = self.jobs.try_recv() {
            self.handle_main_job(event_loop, job);
        }
        if self.shutdown.load(Ordering::SeqCst) {
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window: WindowId,
        event: WindowEvent,
    ) {
        let caller = match self.windows.windows.get(&window) {
            Some(live) => live.caller_id.clone(),
            None => return,
        };
        match event {
            WindowEvent::CloseRequested => {
                self.windows.destroy(&caller);
                self.server
                    .note_host_event(&caller, HostEvent::ClosedByUser);
            }
            WindowEvent::Resized(_) => {
                // Full fresh observation: a resize may also carry a mode or
                // visibility change made through host controls.
                if let Some(live) = self.windows.windows.get(&window) {
                    let observed = read_observed(&live.window, &self.windows.backend_name());
                    self.server.note_full_observation(&caller, observed, true);
                    live.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Hints are physical: nothing to re-apply. Re-observe at the
                // new scale so scale and logical size stay fresh.
                if let Some(live) = self.windows.windows.get(&window) {
                    let observed = read_observed(&live.window, &self.windows.backend_name());
                    let mut observed = observed;
                    observed.scale_factor = Some(scale_factor);
                    if let Some(size) = observed.size_px {
                        observed.logical_size_px = Some(logical_size_for(size, scale_factor));
                    }
                    self.server.note_full_observation(&caller, observed, true);
                    live.window.request_redraw();
                }
            }
            WindowEvent::Moved(_position) => {
                // Position plus anything else the move carried: refresh and
                // redraw, since the rendered snapshot shows the position.
                if let Some(live) = self.windows.windows.get(&window) {
                    let observed = read_observed(&live.window, &self.windows.backend_name());
                    self.server.note_full_observation(&caller, observed, true);
                    live.window.request_redraw();
                }
            }
            WindowEvent::Focused(_) | WindowEvent::Occluded(_) => {
                // Host-side state may have changed under us; refresh without
                // claiming geometry feedback and repaint.
                if let Some(live) = self.windows.windows.get(&window) {
                    let observed = read_observed(&live.window, &self.windows.backend_name());
                    self.server.note_full_observation(&caller, observed, false);
                    live.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.render(window);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::event::ElementState;
                use winit::keyboard::{KeyCode, PhysicalKey};
                if event.state == ElementState::Pressed
                    && let Some(live) = self.windows.windows.get_mut(&window)
                {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Home) => live.scroll_rows = 0,
                        PhysicalKey::Code(KeyCode::PageUp) => {
                            live.scroll_rows = live.scroll_rows.saturating_sub(10);
                        }
                        PhysicalKey::Code(KeyCode::PageDown) => {
                            // Unbounded saturating scroll: rows past the end
                            // simply render background, so all text stays
                            // reachable.
                            live.scroll_rows = live.scroll_rows.saturating_add(10);
                        }
                        PhysicalKey::Code(KeyCode::ArrowUp) => {
                            live.scroll_rows = live.scroll_rows.saturating_sub(1);
                        }
                        PhysicalKey::Code(KeyCode::ArrowDown) => {
                            live.scroll_rows = live.scroll_rows.saturating_add(1);
                        }
                        _ => return,
                    }
                    live.window.request_redraw();
                }
            }
            WindowEvent::Destroyed => {
                self.windows.destroy(&caller);
                self.server
                    .note_host_event(&caller, HostEvent::ClosedByUser);
            }
            _ => {}
        }
    }
}

/// Run the fixture event loop on the calling (main) thread. The main thread
/// owns the [`Server`] and all windows; IO threads submit whole requests
/// through `requests_rx` via [`MainSender`].
pub(crate) fn run(
    event_loop: EventLoop<()>,
    server: Server,
    requests_rx: mpsc::Receiver<MainJob>,
    backend_shared: Arc<OnceLock<String>>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut app = App {
        server,
        windows: Windows {
            backend_shared,
            windows: HashMap::new(),
            ids: HashMap::new(),
            renderer_error: None,
        },
        shutdown,
        jobs: requests_rx,
        context: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_gate_refuses_spawn_without_context() {
        assert!(renderer_gate(&None).is_ok());
        let err = renderer_gate(&Some("no display".to_string())).unwrap_err();
        assert!(err.contains("spawn refused"));
    }

    #[test]
    fn mapping_helpers_cover_every_variant() {
        use winit::window::{
            Theme as WTheme, UserAttentionType, WindowButtons, WindowLevel as WLevel,
        };
        assert_eq!(
            map_window_level(WindowLevel::AlwaysOnBottom),
            WLevel::AlwaysOnBottom
        );
        assert_eq!(map_window_level(WindowLevel::Normal), WLevel::Normal);
        assert_eq!(
            map_window_level(WindowLevel::AlwaysOnTop),
            WLevel::AlwaysOnTop
        );
        assert_eq!(
            map_attention(Attention::Critical),
            Some(UserAttentionType::Critical)
        );
        assert_eq!(
            map_attention(Attention::Informational),
            Some(UserAttentionType::Informational)
        );
        assert_eq!(map_attention(Attention::Clear), None);
        assert_eq!(map_theme(Theme::System), None);
        assert_eq!(map_theme(Theme::Light), Some(WTheme::Light));
        assert_eq!(map_theme(Theme::Dark), Some(WTheme::Dark));
        assert!(map_buttons(&[]).is_empty());
        let all = map_buttons(&[
            WindowButton::Close,
            WindowButton::Minimize,
            WindowButton::Maximize,
        ]);
        assert!(all.contains(WindowButtons::CLOSE));
        assert!(all.contains(WindowButtons::MINIMIZE));
        assert!(all.contains(WindowButtons::MAXIMIZE));
        assert_eq!(map_buttons(&[WindowButton::Close]), WindowButtons::CLOSE);
    }

    #[test]
    fn physical_hints_need_no_scale_math() {
        // Physical pixels are passed straight through: the helper the old
        // logical conversion needed is gone, and the shared logical-size
        // helper agrees with the inline readback formula.
        assert_eq!(logical_size_for([800, 600], 2.0), [400, 300]);
        assert_eq!(logical_size_for([640, 480], 1.0), [640, 480]);
    }
}
