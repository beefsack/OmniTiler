#![allow(unsafe_code)]
//! Sole `unsafe` module in the crate. Everything else forbids unsafe.
//!
//! Policy: public AX/CF/CG surface uses maintained bindings
//! (`objc2-application-services` for AXUIElement/AXObserver/AXValue,
//! `objc2-core-foundation` for CFString/CFArray/CFDictionary/CFNumber/CFRunLoop,
//! `objc2-app-kit`/`objc2-foundation` for process identity, `core-foundation`/
//! `core-graphics` for the event tap, `libc` for `dlopen`/`dlsym`/`kill`/
//! `proc_pidpath`). The sole hand-written `extern "C"` declaration is the
//! private `_AXUIElementGetWindow` in the small `private_raw` module. The
//! Carbon Event Manager and SkyLight (SLS) dynamic tables live in this module
//! beside it (not inside it): each entry point is resolved at runtime via
//! maintained `libc::dlopen`/`libc::dlsym` with a null-checked signature, and
//! a single missing symbol fails the table closed. Each `unsafe` block carries
//! an inline justification: null checks before use, bounded lengths, no
//! retained foreign references escaping, and no panics across FFI boundaries
//! (callback bodies never panic).
//!
//! Soundness rule: no raw foreign pointer crosses this module's boundary.
//! Callers receive owned opaque handles (`AxElement`), plain data
//! (`SlsDisplay`, reports), or errors. There is deliberately no public
//! function accepting an arbitrary non-null foreign pointer.
//!
//! Honest boundary note: calling maintained `objc2` AX/CF wrappers still
//! requires isolated `unsafe` blocks at each call site (their APIs are
//! `unsafe` because they touch live AX objects). The win is one hand-written
//! `extern "C"` declaration (the private join above) instead of dozens for
//! public AX/CF/CG/libc APIs; the remaining `unsafe` is wrapper-call boundary
//! plus the small private dynamic-symbol tables described above. Private SLS
//! top-level results are wrapped as `CFType` and checked-downcast to
//! `CFArray` before any typed view (fail closed on a non-array); nested SLS
//! container kinds are verified before typed use (see
//! `sls_display_spaces`); elements are indexed within `len().min(...)`
//! rather than materialized unbounded. No custom `unsafe impl Send/Sync` anywhere.

#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicBool;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
    mpsc::SyncSender,
};

use serde::{Deserialize, Serialize};

use crate::engine::EngineMessage;

/// AX error codes (`AXError.h`). Only named codes are reported by name;
/// unknown codes surface numerically, never guessed.
pub const AX_NOT_IMPLEMENTED: i32 = -25208;

pub fn ax_error_name(code: i32) -> String {
    match code {
        0 => "kAXErrorSuccess".to_owned(),
        -25200 => "kAXErrorFailure".to_owned(),
        -25201 => "kAXErrorIllegalArgument".to_owned(),
        -25202 => "kAXErrorInvalidUIElement".to_owned(),
        -25203 => "kAXErrorInvalidUIElementObserver".to_owned(),
        -25204 => "kAXErrorCannotComplete".to_owned(),
        -25205 => "kAXErrorAttributeUnsupported".to_owned(),
        -25206 => "kAXErrorActionUnsupported".to_owned(),
        -25207 => "kAXErrorNotificationUnsupported".to_owned(),
        -25208 => "kAXErrorNotImplemented".to_owned(),
        -25209 => "kAXErrorNotificationAlreadyRegistered".to_owned(),
        -25210 => "kAXErrorNoValue".to_owned(),
        -25211 => "kAXErrorParameterizedAttributeUnsupported".to_owned(),
        -25212 => "kAXErrorNotEnoughPrecision".to_owned(),
        other => format!("AXError {other}"),
    }
}

/// Secure Input observation. `Unknown` (missing symbol) is distinct from
/// `Inactive`: a missing symbol is never reported as inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureState {
    Active,
    Inactive,
    Unknown,
}

/// Current UID for owned-state-dir ownership checks. The single `libc`
/// call in the crate lives here (the allowed `unsafe` module) so every
/// other module stays `#![forbid(unsafe_code)]`. `libc` was already a
/// transitive dependency; no new crate enters the lockfile for this.
#[cfg(unix)]
pub fn current_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments and only reads process state.
    unsafe { libc::geteuid() }
}

/// Non-Unix fallback: UID checks are skipped by callers that gate them on
/// `#[cfg(unix)]`; this only satisfies the shared code path.
#[cfg(not(unix))]
pub fn current_uid() -> u32 {
    0
}

/// Thread-safe event counter shared with C callbacks via this module only.
#[derive(Debug, Default)]
pub struct EventCounter {
    hits: AtomicU32,
}

impl EventCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hits(&self) -> u32 {
        self.hits.load(Ordering::SeqCst)
    }

    #[cfg(any(test, target_os = "macos"))]
    fn bump(&self) {
        self.hits.fetch_add(1, Ordering::SeqCst);
    }
}

/// Outcome of one bounded per-app AX observation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxObserveOutcome {
    pub notifications_added: u32,
    pub notifications_received: u32,
    pub messaging_timeout_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxObserveError {
    InvalidPid(String),
    Unsupported(String),
    System(String),
}

impl std::fmt::Display for AxObserveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPid(r) => write!(f, "invalid pid: {r}"),
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::System(r) => write!(f, "AX system error: {r}"),
        }
    }
}

impl std::error::Error for AxObserveError {}

/// App-level session notification names (observed on the application
/// element; each fires for any window of the app). Names per the surveyed
/// AX notification taxonomy (AXSwift `AXNotification`: windowCreated,
/// focusedWindowChanged, windowMoved, windowResized); registration success
/// is still verified per name at runtime — an unsupported name fails to add.
pub const AX_SESSION_APP_NOTIFICATIONS: &[&str] = &[
    "AXWindowCreated",
    "AXFocusedWindowChanged",
    "AXWindowMoved",
    "AXWindowResized",
];

/// Window-level session notification names (observed on the joined owned
/// window element, never a foreign one). `AXUIElementDestroyed` covers the
/// owned window's teardown; `AXMoved`/`AXResized` are the element-level
/// companions of the app-level move/resize notifications.
pub const AX_SESSION_WINDOW_NOTIFICATIONS: &[&str] =
    &["AXMoved", "AXResized", "AXUIElementDestroyed"];

/// Outcome of one bounded owned-session observation run. `notifications_added`
/// names exactly what registered; `notifications_received` carries per-name
/// delivery counts (names absent from it delivered zero). Counts only — never
/// titles, frames, or content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxSessionOutcome {
    pub notifications_added: Vec<String>,
    pub notifications_received: Vec<(String, u32)>,
    pub messaging_timeout_set: bool,
}

/// One managed display's Space membership. IDs only; no names, no content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlsDisplay {
    pub uuid: Option<String>,
    pub spaces: Vec<u64>,
    pub current: Option<u64>,
}

/// Outcome of one bounded event-tap run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapRunReport {
    pub created: bool,
    /// Exact-chord matches observed (KeyDown/KeyUp with exact modifiers).
    pub matched: u32,
    /// Exact-chord events suppressed (only when armed via `--suppress`;
    /// always 0 in observe-only runs).
    pub suppressed: u32,
    /// Key deliveries seen (KeyDown/KeyUp only; FlagsChanged excluded).
    pub deliveries: u32,
    pub reenabled_after_timeout_disable: bool,
    pub stopped_on_second_disable: bool,
    /// Whether this run armed exact-chord suppression (`--suppress`).
    pub suppress_armed: bool,
    pub requested_secs: u64,
    pub elapsed_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapSetupError {
    Denied(String),
    Runloop(String),
}

impl std::fmt::Display for TapSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(r) => write!(f, "event tap denied: {r}"),
            Self::Runloop(r) => write!(f, "event tap runloop error: {r}"),
        }
    }
}

impl std::error::Error for TapSetupError {}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CString;
    use std::ptr::NonNull;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use objc2_application_services::{AXObserver, AXUIElement, AXValue, AXValueType};
    use objc2_core_foundation::{
        CFArray, CFDictionary, CFNumber, CFRetained, CFRunLoop, CFString, CFType, CGPoint, CGSize,
        kCFRunLoopDefaultMode,
    };

    // -- Sole residual hand declaration (genuinely uncovered private SPI) --
    //
    // `_AXUIElementGetWindow` has no maintained Rust wrapper; it stays
    // hand-declared here with its safe wrapper (`ax_window_id`). Everything
    // else dynamic in this module (Carbon Event Manager and SkyLight entry
    // points below) is resolved via maintained `libc::dlopen`/`libc::dlsym`
    // with null-checked signatures because no maintained Rust wrapper covers
    // those tables; missing symbols fail closed, never guessed. Process
    // signaling/identity (`kill`, `proc_pidpath`) likewise uses `libc`.

    mod private_raw {
        unsafe extern "C" {
            pub fn _AXUIElementGetWindow(element: *const std::ffi::c_void, wid: *mut u32) -> i32;
        }
    }

    // -- Carbon hotkeys (no trusted Rust wrapper; symbols resolved
    // dynamically because the CLT SDK does not link the Carbon Event Manager
    // for non-listed clients; missing symbols fail closed, never guessed) --

    type RawRef = *mut std::ffi::c_void;
    type RawConstRef = *const std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct EventTypeSpec {
        event_class: u32,
        event_kind: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct EventHotKeyID {
        signature: u32,
        id: u32,
    }

    const K_EVENT_CLASS_KEYBOARD: u32 = 0x6b65_7962; // 'keyb'
    const K_EVENT_HOTKEY_PRESSED: u32 = 5;

    /// Carbon Event Manager entry points, resolved at runtime.
    /// HotKey's dispatcher-target route installs the handler proc directly,
    /// without a UPP wrapper (pinned source in the Phase 0 change record).
    /// `NewEventHandlerUPP`, `InstallApplicationEventHandler` and
    /// `DisposeEventHandlerUPP` were unavailable on the observed runtime;
    /// the previous application-target + UPP route therefore failed closed.
    struct CarbonTable {
        get_dispatcher_target: unsafe extern "C" fn() -> RawRef,
        install: unsafe extern "C" fn(
            RawRef,
            unsafe extern "C" fn(RawRef, RawRef, RawRef) -> i32,
            u32,
            *const EventTypeSpec,
            RawRef,
            *mut RawRef,
        ) -> i32,
        register: unsafe extern "C" fn(u32, u32, EventHotKeyID, RawRef, u32, *mut RawRef) -> i32,
        unregister: unsafe extern "C" fn(RawRef) -> i32,
        remove: unsafe extern "C" fn(RawRef) -> i32,
        kind: unsafe extern "C" fn(RawConstRef) -> u32,
    }

    static CARBON_TABLE: OnceLock<Option<CarbonTable>> = OnceLock::new();

    const CARBON_PATH: &[u8] = b"/System/Library/Frameworks/Carbon.framework/Carbon\0";

    fn carbon_table() -> Option<&'static CarbonTable> {
        CARBON_TABLE
            .get_or_init(|| {
                // SAFETY: every symbol null-checked before transmuting to its
                // documented Carbon signature; a single missing symbol fails
                // the whole table closed.
                unsafe {
                    let handle = libc::dlopen(CARBON_PATH.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    macro_rules! resolve {
                        ($name:literal as $ty:ty) => {{
                            let cname = CString::new($name).expect("static");
                            let sym = libc::dlsym(handle, cname.as_ptr());
                            if sym.is_null() {
                                return None;
                            }
                            std::mem::transmute::<*mut std::ffi::c_void, $ty>(sym)
                        }};
                    }
                    Some(CarbonTable {
                        get_dispatcher_target: resolve!(
                            "GetEventDispatcherTarget" as unsafe extern "C" fn() -> RawRef
                        ),
                        install: resolve!(
                            "InstallEventHandler"
                                as unsafe extern "C" fn(
                                    RawRef,
                                    unsafe extern "C" fn(RawRef, RawRef, RawRef) -> i32,
                                    u32,
                                    *const EventTypeSpec,
                                    RawRef,
                                    *mut RawRef,
                                ) -> i32
                        ),
                        register: resolve!(
                            "RegisterEventHotKey"
                                as unsafe extern "C" fn(
                                    u32,
                                    u32,
                                    EventHotKeyID,
                                    RawRef,
                                    u32,
                                    *mut RawRef,
                                ) -> i32
                        ),
                        unregister: resolve!(
                            "UnregisterEventHotKey" as unsafe extern "C" fn(RawRef) -> i32
                        ),
                        remove: resolve!(
                            "RemoveEventHandler" as unsafe extern "C" fn(RawRef) -> i32
                        ),
                        kind: resolve!("GetEventKind" as unsafe extern "C" fn(RawConstRef) -> u32),
                    })
                }
            })
            .as_ref()
    }

    struct CarbonRefcon {
        tx: SyncSender<EngineMessage>,
        keycode: u32,
        counter: Arc<EventCounter>,
    }

    unsafe extern "C" fn carbon_hotkey_handler(
        _next: RawRef,
        event: RawRef,
        user_data: RawRef,
    ) -> i32 {
        // SAFETY: `user_data` is the live `Box<CarbonRefcon>` installed by
        // `carbon_register`; it is reclaimed only after the handler is
        // removed. Reads only; `try_send` never blocks or panics.
        if !user_data.is_null() && !event.is_null() {
            // The table is initialized at registration, before any event can
            // fire; this is the cached path.
            if let Some(table) = carbon_table()
                && unsafe { (table.kind)(event as RawConstRef) } == K_EVENT_HOTKEY_PRESSED
            {
                let refcon = unsafe { &*(user_data as *const CarbonRefcon) };
                refcon.counter.bump();
                let _ = refcon.tx.try_send(EngineMessage::InputEvent {
                    key_code: refcon.keycode,
                });
            }
        }
        0
    }

    /// Owned Carbon hotkey registration. Main-thread-bound by construction
    /// (contains raw refs; intentionally `!Send`). `Drop` unregisters.
    /// The handler proc installs directly (no UPP wrapper on this route).
    pub struct CarbonHotkey {
        hotkey_ref: RawRef,
        handler_ref: RawRef,
        refcon: *mut CarbonRefcon,
        // `Rc` is `!Send + !Sync`, so this handle stays thread-bound without
        // any `unsafe impl`. Must be created/used/dropped on the same thread.
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for CarbonHotkey {
        fn drop(&mut self) {
            // SAFETY: refs were returned live by Carbon; unregister before
            // reclaiming the refcon so no callback can fire afterwards. If
            // the table vanished (impossible after successful registration),
            // the refcon is still reclaimed.
            unsafe {
                if let Some(table) = carbon_table() {
                    if !self.hotkey_ref.is_null() {
                        (table.unregister)(self.hotkey_ref);
                    }
                    if !self.handler_ref.is_null() {
                        (table.remove)(self.handler_ref);
                    }
                }
                if !self.refcon.is_null() {
                    drop(Box::from_raw(self.refcon));
                }
            }
        }
    }

    /// Register one Carbon hotkey chord. Returns the live guard on OS
    /// acceptance (`noErr`); delivery additionally needs a live keypress and
    /// a running main-thread runloop, verified separately.
    pub fn carbon_register(
        keycode: u32,
        carbon_modifiers: u32,
        counter: Arc<EventCounter>,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<CarbonHotkey, i32> {
        let table = carbon_table().ok_or(-2)?;
        let refcon = Box::new(CarbonRefcon {
            tx: tx.clone(),
            keycode,
            counter,
        });
        let refcon_ptr = Box::into_raw(refcon);
        // SAFETY: all pointers null-checked; Carbon refs released in `Drop`
        // in reverse order; the refcon outlives the handler registration.
        // The dispatcher-target route installs the proc directly (no UPP).
        unsafe {
            let spec = EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOTKEY_PRESSED,
            };
            let mut handler_ref: RawRef = std::ptr::null_mut();
            let target = (table.get_dispatcher_target)();
            if target.is_null() {
                drop(Box::from_raw(refcon_ptr));
                return Err(-1);
            }
            let status = (table.install)(
                target,
                carbon_hotkey_handler,
                1,
                &spec,
                refcon_ptr as RawRef,
                &mut handler_ref,
            );
            if status != 0 {
                drop(Box::from_raw(refcon_ptr));
                return Err(status);
            }
            let hot_id = EventHotKeyID {
                signature: 0x4f4d_5450, // 'OMTP'
                id: 1,
            };
            let mut hotkey_ref: RawRef = std::ptr::null_mut();
            let status = (table.register)(
                keycode,
                carbon_modifiers,
                hot_id,
                (table.get_dispatcher_target)(),
                0,
                &mut hotkey_ref,
            );
            if status != 0 {
                (table.remove)(handler_ref);
                drop(Box::from_raw(refcon_ptr));
                return Err(status);
            }
            Ok(CarbonHotkey {
                hotkey_ref,
                handler_ref,
                refcon: refcon_ptr,
                _not_send: std::marker::PhantomData,
            })
        }
    }

    // -- Typed AX surface (maintained `objc2` bindings) --

    /// Owned AX element handle. Constructible only through this module's
    /// checked constructors; `Drop` is handled by `CFRetained`. Create and use
    /// the handle on its application's AX thread; do not send foreign handles
    /// across threads. `!Send + !Sync` via `PhantomData<Rc>` (no `unsafe
    /// impl`): the compiler rejects moving a handle to another thread.
    pub struct AxElement {
        inner: CFRetained<AXUIElement>,
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl AxElement {
        /// AX element for an application PID. Creation itself never fails
        /// visibly (the API does not validate); failures surface on use.
        /// Note: the maintained binding asserts non-null return; a null
        /// return would panic rather than yield `None`. PIDs are pre-validated
        /// (`pid > 0`) and the OS contract is non-null here.
        pub fn application(pid: i32) -> Option<Self> {
            if pid <= 0 {
                return None;
            }
            // SAFETY: maintained `objc2-application-services` binding; pid
            // validated above; retained object owned by `CFRetained`.
            let inner = unsafe { AXUIElement::new_application(pid) };
            Some(Self {
                inner,
                _not_send: std::marker::PhantomData,
            })
        }

        pub fn system_wide() -> Option<Self> {
            // SAFETY: no-arg creator in the maintained binding; retained
            // object owned by `CFRetained`.
            let inner = unsafe { AXUIElement::new_system_wide() };
            Some(Self {
                inner,
                _not_send: std::marker::PhantomData,
            })
        }

        fn copy_attribute(&self, name: &str) -> Result<CFRetained<CFType>, i32> {
            let attr = CFString::from_str(name);
            let mut value: *const CFType = std::ptr::null();
            // SAFETY: live element + live name from the maintained crate;
            // `value` is a valid out-pointer; nonzero status returns Err with
            // no use of `value`. Copy rule: success yields +1 retained.
            let status = unsafe {
                self.inner.copy_attribute_value(
                    &attr,
                    NonNull::new(std::ptr::addr_of_mut!(value)).expect("out-pointer non-null"),
                )
            };
            if status.0 == 0 {
                NonNull::new(value as *mut CFType)
                    .map(|ptr| unsafe { CFRetained::from_raw(ptr) })
                    .ok_or(AX_NOT_IMPLEMENTED)
            } else {
                Err(status.0)
            }
        }

        /// Window elements of this application element (each retained by the
        /// maintained `CFArray::get`; caller owns each via `AxElement`).
        pub fn windows(&self) -> Result<Vec<AxElement>, i32> {
            let value = self.copy_attribute("AXWindows")?;
            let untyped: CFRetained<CFArray> = value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
            // SAFETY: `AXWindows` is documented to be an array of
            // `AXUIElement`; `CFArray` layout is identical across element
            // types, and elements are accessed via the checked `get`.
            let array: CFRetained<CFArray<AXUIElement>> =
                unsafe { CFRetained::cast_unchecked(untyped) };
            let count = array.len().min(crate::config::MAX_WINDOWS);
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                if let Some(elem) = array.get(i) {
                    out.push(AxElement {
                        inner: elem,
                        _not_send: std::marker::PhantomData,
                    });
                }
            }
            Ok(out)
        }
    }

    /// Whether this process is trusted for accessibility (consent state).
    pub fn ax_is_process_trusted() -> Option<bool> {
        // SAFETY: no-arg getter in the maintained binding, no pointers.
        Some(unsafe { objc2_application_services::AXIsProcessTrusted() })
    }

    /// Best-effort per-element messaging timeout (seconds). Returns the raw
    /// AX status; callers record rather than assume it.
    pub fn ax_set_messaging_timeout(element: &AxElement, seconds: f32) -> i32 {
        // SAFETY: live element from a checked constructor, plain float, in
        // the maintained binding.
        unsafe { element.inner.set_messaging_timeout(seconds) }.0
    }

    /// Identity join: CG window ID for an AX window element (private SPI kept
    /// in `private_raw`; missing/failing join is `None`, never guessed).
    pub fn ax_window_id(element: &AxElement) -> Option<u32> {
        let mut wid: u32 = 0;
        let ptr = CFRetained::as_ptr(&element.inner).as_ptr() as *const std::ffi::c_void;
        // SAFETY: live element from a checked constructor; valid out-pointer;
        // status gates the read.
        let err = unsafe { private_raw::_AXUIElementGetWindow(ptr, std::ptr::addr_of_mut!(wid)) };
        if err == 0 { Some(wid) } else { None }
    }

    /// Exact focus preimage: the currently focused application PID plus its
    /// focused window ID (when queryable). `None` means the preimage could
    /// not be proven (no consent or no focused window): callers must fail
    /// closed before changing focus rather than steal it blindly.
    pub fn ax_focused_preimage() -> Option<(i32, Option<u32>)> {
        let system = AxElement::system_wide()?;
        let app_value = system.copy_attribute("AXFocusedApplication").ok()?;
        let app_inner: CFRetained<AXUIElement> = app_value.downcast().ok()?;
        let app = AxElement {
            inner: app_inner,
            _not_send: std::marker::PhantomData,
        };
        let mut pid: libc::pid_t = 0;
        // SAFETY: live element; valid out-pointer; status gates the read.
        let err = unsafe {
            app.inner
                .pid(NonNull::new(std::ptr::addr_of_mut!(pid)).expect("non-null"))
        };
        if err.0 != 0 || pid <= 1 {
            return None;
        }
        let wid = app
            .copy_attribute("AXFocusedWindow")
            .ok()
            .and_then(|window_value| {
                let window_inner: CFRetained<AXUIElement> = window_value.downcast().ok()?;
                let window = AxElement {
                    inner: window_inner,
                    _not_send: std::marker::PhantomData,
                };
                ax_window_id(&window)
            });
        Some((pid as i32, wid))
    }

    /// Window frame in screen points.
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct AxFrame {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    impl AxFrame {
        pub fn valid_for_set(&self) -> bool {
            self.x.is_finite()
                && self.y.is_finite()
                && self.w.is_finite()
                && self.h.is_finite()
                && self.w > 0.0
                && self.h > 0.0
        }
    }

    pub fn ax_get_frame(element: &AxElement) -> Result<AxFrame, i32> {
        let pos_value = element.copy_attribute("AXPosition")?;
        // `CFRetained` drops both values on every path; no manual release.
        let size_value = element.copy_attribute("AXSize")?;
        let pos: CFRetained<AXValue> = pos_value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        let size: CFRetained<AXValue> = size_value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        let mut point = CGPoint { x: 0.0, y: 0.0 };
        let mut size_s = CGSize {
            width: 0.0,
            height: 0.0,
        };
        // SAFETY: both values live in maintained wrappers; out-pointers are
        // locals; boolean results gate use.
        let ok_pos = unsafe {
            pos.value(
                AXValueType::CGPoint,
                NonNull::new(std::ptr::addr_of_mut!(point) as *mut std::ffi::c_void)
                    .expect("non-null"),
            )
        };
        let ok_size = unsafe {
            size.value(
                AXValueType::CGSize,
                NonNull::new(std::ptr::addr_of_mut!(size_s) as *mut std::ffi::c_void)
                    .expect("non-null"),
            )
        };
        if ok_pos && ok_size {
            Ok(AxFrame {
                x: point.x,
                y: point.y,
                w: size_s.width,
                h: size_s.height,
            })
        } else {
            Err(AX_NOT_IMPLEMENTED)
        }
    }

    /// Position leg of a frame write. Split from [`ax_set_frame`] so the
    /// partial-write hazard (position accepted, size refused) is explicit at
    /// the boundary: callers must restore the preimage after ANY failed
    /// frame write, never treat the first leg's acceptance as success.
    pub fn ax_set_position(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        if !frame.valid_for_set() {
            return Err(-25201);
        }
        let pos_name = CFString::from_str("AXPosition");
        let point = CGPoint {
            x: frame.x,
            y: frame.y,
        };
        // SAFETY: stack point lives for the call; maintained `AXValue::new`
        // copies it into a retained object; null means allocation failure.
        let pos_value = unsafe {
            AXValue::new(
                AXValueType::CGPoint,
                NonNull::new(std::ptr::addr_of!(point) as *mut std::ffi::c_void).expect("non-null"),
            )
        }
        .ok_or(-25200)?;
        // SAFETY: live element/name/value in maintained wrappers; status is
        // the only effect read. `Deref` chains `&AXValue` to `&CFType`.
        let status = unsafe { element.inner.set_attribute_value(&pos_name, &pos_value) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// Size leg of a frame write. See [`ax_set_position`]: a size refusal
    /// after an accepted position write is a partial mutation, never a clean
    /// no-op — the caller must restore.
    pub fn ax_set_size(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        if !frame.valid_for_set() {
            return Err(-25201);
        }
        let size_name = CFString::from_str("AXSize");
        let size = CGSize {
            width: frame.w,
            height: frame.h,
        };
        // SAFETY: stack size lives for the call; maintained `AXValue::new`
        // copies it into a retained object; null means allocation failure.
        let size_value = unsafe {
            AXValue::new(
                AXValueType::CGSize,
                NonNull::new(std::ptr::addr_of!(size) as *mut std::ffi::c_void).expect("non-null"),
            )
        }
        .ok_or(-25200)?;
        // SAFETY: live element/name/value in maintained wrappers; status is
        // the only effect read.
        let status = unsafe { element.inner.set_attribute_value(&size_name, &size_value) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    pub fn ax_set_frame(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        // Ordered legs: position first, then size. A size refusal after an
        // accepted position write leaves the window partially mutated; the
        // `Err` return carries that hazard to the caller, which must attempt
        // preimage restoration on EVERY error path after this call.
        ax_set_position(element, frame)?;
        ax_set_size(element, frame)
    }

    /// Owned-window title (`AXTitle`) for the nonce-title identity fallback.
    /// `Ok(None)` means the attribute was present but not a usable title
    /// (missing/non-string/empty); `Err(code)` means the read itself failed.
    /// Callers compare the exact expected nonce title and never log titles.
    pub fn ax_window_title(element: &AxElement) -> Result<Option<String>, i32> {
        let value = element.copy_attribute("AXTitle")?;
        match value.downcast::<CFString>() {
            Ok(s) => {
                let title = s.to_string();
                if title.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(title))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Non-prompting synthetic-input preflight: whether the window server
    /// would accept posted events from this process
    /// (`CGPreflightPostEventAccess` via the maintained
    /// `objc2-core-graphics` binding). Preflight only — never requests access
    /// and never posts. Always `Some` on macOS (the symbol links at build
    /// time); `None` off-macOS (stub reports no state).
    pub fn cg_post_event_access_known() -> Option<bool> {
        Some(objc2_core_graphics::CGPreflightPostEventAccess())
    }

    pub fn ax_raise(element: &AxElement) -> Result<(), i32> {
        let action = CFString::from_str("AXRaise");
        // SAFETY: live element + action name in the maintained binding;
        // status is the only effect read.
        let status = unsafe { element.inner.perform_action(&action) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    struct ObserverRefcon {
        tx: SyncSender<EngineMessage>,
        pid: u32,
        received: Arc<AtomicU32>,
    }

    unsafe extern "C-unwind" fn ax_observer_callback(
        _observer: NonNull<AXObserver>,
        _element: NonNull<AXUIElement>,
        _notification: NonNull<CFString>,
        refcon: *mut std::ffi::c_void,
    ) {
        // SAFETY: `refcon` is the live `Box<ObserverRefcon>` installed below,
        // reclaimed only after notifications are removed and the observer is
        // released. Minimal work: bump + non-blocking send. No panics.
        if refcon.is_null() {
            return;
        }
        let state = unsafe { &*(refcon as *const ObserverRefcon) };
        state.received.fetch_add(1, Ordering::SeqCst);
        let _ = state.tx.try_send(EngineMessage::AxEvent { pid: state.pid });
    }

    /// Bounded per-app AX observation on the calling thread's runloop.
    /// Registers window-created + focused-window-changed notifications with a
    /// finite messaging timeout, pumps the runloop until `timeout`, then
    /// tears everything down. A hung app stalls at most one slice past its
    /// messaging timeout; failures (bad PID, no consent) return fast.
    pub fn ax_observe_bounded(
        pid: i32,
        timeout: Duration,
        messaging_timeout_secs: f32,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<AxObserveOutcome, AxObserveError> {
        if pid <= 1 {
            return Err(AxObserveError::InvalidPid(format!(
                "refusing reserved pid {pid}"
            )));
        }
        let app = AxElement::application(pid)
            .ok_or_else(|| AxObserveError::InvalidPid(format!("bad pid {pid}")))?;
        let messaging_timeout_set = ax_set_messaging_timeout(&app, messaging_timeout_secs) == 0;
        let mut observer_raw: *mut AXObserver = std::ptr::null_mut();
        // SAFETY: out-pointer valid; maintained `AXObserver::create`; nonzero
        // status leaves `observer_raw` null and returns fast.
        let status = unsafe {
            AXObserver::create(
                pid,
                Some(ax_observer_callback),
                NonNull::new(std::ptr::addr_of_mut!(observer_raw)).expect("non-null"),
            )
        };
        if status.0 != 0 || observer_raw.is_null() {
            return Err(AxObserveError::System(format!(
                "AXObserverCreate pid {pid}: {}",
                ax_error_name(status.0)
            )));
        }
        // SAFETY: create-rule +1 retained; owned by `CFRetained` exactly once.
        let observer: CFRetained<AXObserver> =
            unsafe { CFRetained::from_raw(NonNull::new(observer_raw).expect("non-null")) };
        let received = Arc::new(AtomicU32::new(0));
        let refcon = Box::new(ObserverRefcon {
            tx: tx.clone(),
            pid: pid as u32,
            received: received.clone(),
        });
        let refcon_ptr = Box::into_raw(refcon);
        let mut added: u32 = 0;
        // SAFETY: observer/app live in maintained wrappers; names are fresh
        // `CFString`s; refcon valid until reclaimed below after notifications
        // are removed.
        unsafe {
            for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                let cf_name = CFString::from_str(name);
                let status = observer.add_notification(
                    &app.inner,
                    &cf_name,
                    refcon_ptr as *mut std::ffi::c_void,
                );
                if status.0 == 0 {
                    added += 1;
                }
            }
            if added == 0 {
                drop(Box::from_raw(refcon_ptr));
                return Err(AxObserveError::System(
                    "no AX notifications registered (bad pid or no consent)".to_owned(),
                ));
            }
            // Note: the maintained `run_loop_source` asserts non-null and
            // would panic on null; a null source here would mean an OS
            // contract violation (live observer must vend a source).
            let source = observer.run_loop_source();
            let rl = CFRunLoop::current().ok_or_else(|| {
                for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                    let cf_name = CFString::from_str(name);
                    let _ = observer.remove_notification(&app.inner, &cf_name);
                }
                drop(Box::from_raw(refcon_ptr));
                AxObserveError::System("AX observer runloop unavailable".to_owned())
            })?;
            let mode = kCFRunLoopDefaultMode;
            rl.add_source(Some(&source), mode);
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                CFRunLoop::run_in_mode(mode, 0.25, false);
            }
            rl.remove_source(Some(&source), mode);
            source.invalidate();
            for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                let cf_name = CFString::from_str(name);
                let _ = observer.remove_notification(&app.inner, &cf_name);
            }
            drop(Box::from_raw(refcon_ptr));
        }
        Ok(AxObserveOutcome {
            notifications_added: added,
            notifications_received: received.load(Ordering::SeqCst),
            messaging_timeout_set,
        })
    }

    struct SessionRefcon {
        tx: SyncSender<EngineMessage>,
        pid: u32,
        counts: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    }

    unsafe extern "C-unwind" fn ax_session_callback(
        _observer: NonNull<AXObserver>,
        _element: NonNull<AXUIElement>,
        notification: NonNull<CFString>,
        refcon: *mut std::ffi::c_void,
    ) {
        // SAFETY: `refcon` is the live `Box<SessionRefcon>` installed below,
        // reclaimed only after notifications are removed and the observer is
        // released. `notification` is borrowed (no ownership transfer); only
        // its string copy leaves. Minimal work: count + non-blocking send.
        // No panics.
        if refcon.is_null() {
            return;
        }
        let state = unsafe { &*(refcon as *const SessionRefcon) };
        let name = unsafe { notification.as_ref() }.to_string();
        if let Ok(mut counts) = state.counts.lock() {
            *counts.entry(name).or_insert(0) += 1;
        }
        let _ = state.tx.try_send(EngineMessage::AxEvent { pid: state.pid });
    }

    /// Bounded owned-session observation on the calling thread's runloop.
    /// Registers the app-level session set on the application element plus,
    /// when `window_wid` joins an owned window, the window-level set on that
    /// exact window element (never a foreign one). Pumps the runloop until
    /// `timeout`, then tears everything down and reports per-name delivery
    /// counts. A hung app stalls at most one slice past its messaging
    /// timeout; failures (bad PID, unjoined WID, no consent) return fast.
    /// Every `AxElement` is constructed, used, and dropped on the calling
    /// (observer) thread.
    pub fn ax_observe_session(
        pid: i32,
        window_wid: Option<u32>,
        timeout: Duration,
        messaging_timeout_secs: f32,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<AxSessionOutcome, AxObserveError> {
        if pid <= 1 {
            return Err(AxObserveError::InvalidPid(format!(
                "refusing reserved pid {pid}"
            )));
        }
        let app = AxElement::application(pid)
            .ok_or_else(|| AxObserveError::InvalidPid(format!("bad pid {pid}")))?;
        let messaging_timeout_set = ax_set_messaging_timeout(&app, messaging_timeout_secs) == 0;
        // Join the requested window before creating the observer: an
        // unjoined WID fails here, never as a silent empty watch.
        let window: Option<AxElement> = match window_wid {
            None => None,
            Some(wid) => {
                let windows = app.windows().map_err(|code| {
                    AxObserveError::System(format!("windows: {}", ax_error_name(code)))
                })?;
                match windows.into_iter().find(|w| ax_window_id(w) == Some(wid)) {
                    Some(w) => {
                        let _ = ax_set_messaging_timeout(&w, messaging_timeout_secs);
                        Some(w)
                    }
                    None => {
                        return Err(AxObserveError::System(format!(
                            "wid {wid} not joined to pid {pid}"
                        )));
                    }
                }
            }
        };
        let mut observer_raw: *mut AXObserver = std::ptr::null_mut();
        // SAFETY: out-pointer valid; maintained `AXObserver::create`; nonzero
        // status leaves `observer_raw` null and returns fast.
        let status = unsafe {
            AXObserver::create(
                pid,
                Some(ax_session_callback),
                NonNull::new(std::ptr::addr_of_mut!(observer_raw)).expect("non-null"),
            )
        };
        if status.0 != 0 || observer_raw.is_null() {
            return Err(AxObserveError::System(format!(
                "AXObserverCreate pid {pid}: {}",
                ax_error_name(status.0)
            )));
        }
        // SAFETY: create-rule +1 retained; owned by `CFRetained` exactly once.
        let observer: CFRetained<AXObserver> =
            unsafe { CFRetained::from_raw(NonNull::new(observer_raw).expect("non-null")) };
        let refcon = Box::new(SessionRefcon {
            tx: tx.clone(),
            pid: pid as u32,
            counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        let refcon_ptr = Box::into_raw(refcon);
        let mut added: Vec<String> = Vec::new();
        // SAFETY: observer/app/window live in maintained wrappers; names are
        // fresh `CFString`s; refcon valid until reclaimed below after
        // notifications are removed.
        unsafe {
            for name in AX_SESSION_APP_NOTIFICATIONS {
                let cf_name = CFString::from_str(name);
                let status = observer.add_notification(
                    &app.inner,
                    &cf_name,
                    refcon_ptr as *mut std::ffi::c_void,
                );
                if status.0 == 0 {
                    added.push((*name).to_owned());
                }
            }
            if let Some(window) = window.as_ref() {
                for name in AX_SESSION_WINDOW_NOTIFICATIONS {
                    let cf_name = CFString::from_str(name);
                    let status = observer.add_notification(
                        &window.inner,
                        &cf_name,
                        refcon_ptr as *mut std::ffi::c_void,
                    );
                    if status.0 == 0 {
                        added.push((*name).to_owned());
                    }
                }
            }
            if added.is_empty() {
                drop(Box::from_raw(refcon_ptr));
                return Err(AxObserveError::System(
                    "no AX notifications registered (bad pid or no consent)".to_owned(),
                ));
            }
            // Note: the maintained `run_loop_source` asserts non-null and
            // would panic on null; a null source here would mean an OS
            // contract violation (live observer must vend a source).
            let source = observer.run_loop_source();
            let rl = CFRunLoop::current().ok_or_else(|| {
                for name in added.clone() {
                    let cf_name = CFString::from_str(&name);
                    let _ = observer.remove_notification(&app.inner, &cf_name);
                    if let Some(window) = window.as_ref() {
                        let _ = observer.remove_notification(&window.inner, &cf_name);
                    }
                }
                drop(Box::from_raw(refcon_ptr));
                AxObserveError::System("AX observer runloop unavailable".to_owned())
            })?;
            let mode = kCFRunLoopDefaultMode;
            rl.add_source(Some(&source), mode);
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                CFRunLoop::run_in_mode(mode, 0.25, false);
            }
            rl.remove_source(Some(&source), mode);
            source.invalidate();
            // Counts are collected while the refcon is still owned (shared
            // borrow of the live Box); reclaim happens last, after removals,
            // so no callback can observe a freed refcon.
            // SAFETY: `refcon_ptr` is the live Box installed above; no
            // callback runs past `invalidate()` on this thread's runloop, and
            // the observer belongs to this thread only.
            let mut received: Vec<(String, u32)> = (&*refcon_ptr)
                .counts
                .lock()
                .map(|counts| {
                    let mut out: Vec<(String, u32)> =
                        counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    out.sort_unstable();
                    out
                })
                .unwrap_or_default();
            // Removal targets both elements (a name registered on one is a
            // harmless miss on the other); statuses are teardown-only.
            // `added_names` preserves the registered set for the outcome:
            // added-but-silent names must appear there too.
            let added_names = added.clone();
            for name in added {
                let cf_name = CFString::from_str(&name);
                let _ = observer.remove_notification(&app.inner, &cf_name);
                if let Some(window) = window.as_ref() {
                    let _ = observer.remove_notification(&window.inner, &cf_name);
                }
            }
            // SAFETY: notifications removed and the runloop source
            // invalidated above; no callback can fire afterwards, so the
            // refcon is reclaimed exactly once here.
            drop(Box::from_raw(refcon_ptr));
            Ok(AxSessionOutcome {
                notifications_added: added_names,
                notifications_received: std::mem::take(&mut received),
                messaging_timeout_set,
            })
        }
    }

    /// Observe Secure Input state. `None` (missing symbol) is unknown, never
    /// inactive.
    pub fn secure_input_known() -> Option<bool> {
        type FnTy = unsafe extern "C" fn() -> bool;
        // SAFETY: `libc::dlsym` lookups null-checked; the getter takes no
        // args. Failure to resolve means "unknown", reported as `None`.
        unsafe {
            const CANDIDATES: &[&[u8]] = &[
                b"/System/Library/Frameworks/Carbon.framework/Carbon\0",
                b"/System/Library/PrivateFrameworks/HIServices.framework/HIServices\0",
            ];
            for path in CANDIDATES {
                let handle = libc::dlopen(path.as_ptr().cast(), 0x1);
                if handle.is_null() {
                    continue;
                }
                let name = CString::new("IsSecureEventInputEnabled").expect("static");
                let sym = libc::dlsym(handle, name.as_ptr());
                if !sym.is_null() {
                    let f: FnTy = std::mem::transmute::<*mut std::ffi::c_void, FnTy>(sym);
                    return Some(f());
                }
            }
            let self_handle = libc::dlopen(std::ptr::null(), 0x1);
            if !self_handle.is_null() {
                let name = CString::new("IsSecureEventInputEnabled").expect("static");
                let sym = libc::dlsym(self_handle, name.as_ptr());
                if !sym.is_null() {
                    let f: FnTy = std::mem::transmute::<*mut std::ffi::c_void, FnTy>(sym);
                    return Some(f());
                }
            }
            None
        }
    }

    // -- Dynamic SkyLight reads (private symbols; missing => Err) --

    const SKYLIGHT: &[u8] = b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0";

    struct SlsTable {
        connection: unsafe extern "C" fn() -> i32,
        copy_spaces: unsafe extern "C" fn(i32) -> RawRef,
        copy_displays: unsafe extern "C" fn(i32) -> RawRef,
        current_space: unsafe extern "C" fn(i32, RawConstRef) -> u64,
    }

    static SLS_TABLE: OnceLock<Option<SlsTable>> = OnceLock::new();

    fn sls_table() -> Option<&'static SlsTable> {
        SLS_TABLE
            .get_or_init(|| {
                // SAFETY: every symbol null-checked; signatures match the
                // reference declarations (`int SLSMainConnectionID(void)`,
                // `CFArrayRef SLSCopyManagedDisplaySpaces(int)`,
                // `CFArrayRef SLSCopyManagedDisplays(int)`,
                // `uint64_t SLSManagedDisplayGetCurrentSpace(int, CFStringRef)`).
                // Returned arrays are retained and wrapped by the caller by
                // copy-rule contract (top-level kind not rechecked).
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let sym = |name: &str| {
                        let cname = CString::new(name).expect("static");
                        libc::dlsym(handle, cname.as_ptr())
                    };
                    let connection = sym("SLSMainConnectionID");
                    let copy_spaces = sym("SLSCopyManagedDisplaySpaces");
                    let copy_displays = sym("SLSCopyManagedDisplays");
                    let current_space = sym("SLSManagedDisplayGetCurrentSpace");
                    if connection.is_null()
                        || copy_spaces.is_null()
                        || copy_displays.is_null()
                        || current_space.is_null()
                    {
                        return None;
                    }
                    Some(SlsTable {
                        connection: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn() -> i32,
                        >(connection),
                        copy_spaces: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32) -> RawRef,
                        >(copy_spaces),
                        copy_displays: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32) -> RawRef,
                        >(copy_displays),
                        current_space: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, RawConstRef) -> u64,
                        >(current_space),
                    })
                }
            })
            .as_ref()
    }

    pub fn sls_connection_available() -> bool {
        sls_table().is_some()
    }

    pub fn sls_main_connection_id() -> Option<i32> {
        let table = sls_table()?;
        // SAFETY: resolved non-null symbol, no args.
        Some(unsafe { (table.connection)() })
    }

    const MAX_DISPLAYS: usize = 64;
    const MAX_SPACES_PER_DISPLAY: usize = 256;
    const MAX_MEMBERSHIP_WINDOWS: usize = 16;

    /// Read-only managed display/Space/current read. IDs and counts only;
    /// no names, no window content. Every container null-checked; nested
    /// container kinds verified via `downcast_ref` before typed use. The two
    /// top-level SLS arrays are wrapped by copy-rule contract (kind not
    /// rechecked); UUID strings and Space IDs come from checked per-element
    /// downcasts.
    pub fn sls_display_spaces() -> Result<Vec<SlsDisplay>, String> {
        let table = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        // SAFETY: resolved symbols; retained top-level results are wrapped
        // as `CFType` and checked-downcast to `CFArray` before any typed
        // view; elements are indexed within `len().min(...)` (no unbounded
        // materialization); nested container kinds verified below before
        // typed use; strings via `to_string`.
        unsafe {
            use objc2_core_foundation::CFDictionary as CFDictUntyped;
            let cid = (table.connection)();
            let spaces_raw = (table.copy_spaces)(cid);
            if spaces_raw.is_null() {
                return Err("SLSCopyManagedDisplaySpaces returned null".to_owned());
            }
            let spaces_top: CFRetained<CFType> =
                CFRetained::from_raw(NonNull::new(spaces_raw as *mut CFType).expect("non-null"));
            let spaces_untyped: CFRetained<CFArray> = spaces_top.downcast().map_err(|_| {
                "SLSCopyManagedDisplaySpaces returned a non-array top-level container".to_owned()
            })?;
            // SAFETY: top-level kind verified by the downcast above; viewed
            // as `CFArray<CFType>` (layouts identical; all elements CFTypes).
            let spaces_array: CFRetained<CFArray<CFType>> =
                CFRetained::cast_unchecked(spaces_untyped);
            let displays_raw = (table.copy_displays)(cid);
            let display_uuids: Vec<Option<String>> = if displays_raw.is_null() {
                Vec::new()
            } else {
                let displays_top: CFRetained<CFType> = CFRetained::from_raw(
                    NonNull::new(displays_raw as *mut CFType).expect("non-null"),
                );
                let displays_untyped: CFRetained<CFArray> =
                    displays_top.downcast().map_err(|_| {
                        "SLSCopyManagedDisplays returned a non-array top-level container".to_owned()
                    })?;
                // SAFETY: top-level kind verified; managed-display entries
                // are CFStrings per SkyLight copy-rule contract, checked per
                // element below.
                let displays_array: CFRetained<CFArray<CFType>> =
                    CFRetained::cast_unchecked(displays_untyped);
                let mut uuids = Vec::new();
                for i in 0..displays_array.len().min(MAX_DISPLAYS) {
                    uuids.push(
                        displays_array
                            .get(i)
                            .and_then(|item| item.downcast::<CFString>().ok())
                            .map(|s| s.to_string()),
                    );
                }
                uuids
            };
            let spaces_key = CFString::from_str("Spaces");
            let id_key = CFString::from_str("id64");
            let mut out = Vec::new();
            for index in 0..spaces_array.len().min(MAX_DISPLAYS) {
                let display_item = match spaces_array.get(index) {
                    Some(item) => item,
                    None => continue,
                };
                let mut entry = SlsDisplay {
                    uuid: display_uuids.get(index).cloned().flatten(),
                    spaces: Vec::new(),
                    current: None,
                };
                // Verify dictionary kind untyped, then view typed via a
                // retained clone (owner stays alive; no borrowed raw view).
                if display_item.downcast_ref::<CFDictUntyped>().is_some() {
                    // SAFETY: kind verified above; SLS display entries map
                    // `CFString` keys to `CFType` values; layouts identical.
                    let display_dict: CFRetained<CFDictionary<CFString, CFType>> =
                        CFRetained::cast_unchecked(display_item.clone());
                    if let Some(spaces_val) = display_dict.get(&spaces_key)
                        && spaces_val.downcast_ref::<CFArray>().is_some()
                    {
                        // SAFETY: kind verified; space list holds
                        // dictionaries (each a CFType).
                        let spaces_list: CFRetained<CFArray<CFType>> =
                            CFRetained::cast_unchecked(spaces_val.clone());
                        for j in 0..spaces_list.len().min(MAX_SPACES_PER_DISPLAY) {
                            let space_item = match spaces_list.get(j) {
                                Some(item) => item,
                                None => continue,
                            };
                            if space_item.downcast_ref::<CFDictUntyped>().is_some() {
                                // SAFETY: kind verified; space entries map
                                // `CFString` keys to `CFType` values.
                                let space_dict: CFRetained<CFDictionary<CFString, CFType>> =
                                    CFRetained::cast_unchecked(space_item.clone());
                                if let Some(sid_val) = space_dict.get(&id_key)
                                    && let Ok(num) = sid_val.downcast::<CFNumber>()
                                    && let Some(sid) = num.as_i64()
                                    && sid >= 0
                                {
                                    entry.spaces.push(sid as u64);
                                }
                            }
                        }
                    }
                }
                if let Some(uuid) = entry.uuid.clone() {
                    let uuid_cf = CFString::from_str(&uuid);
                    let uuid_ptr = CFRetained::as_ptr(&uuid_cf).as_ptr() as *const std::ffi::c_void;
                    entry.current = Some((table.current_space)(cid, uuid_ptr));
                }
                out.push(entry);
            }
            Ok(out)
        }
    }

    // -- Read-only per-window Space membership (pinned yabai signature) --
    //
    // Signature pinned against yabai `src/misc/extern.h`:
    //   `CFArrayRef SLSCopySpacesForWindows(int cid, int selector,
    //   CFArrayRef window_list);`
    // Call shape pinned against yabai `src/window.c` (`window_space_list`):
    // selector `0x7`, `window_list` a CFArray of CFNumber/SInt32 window IDs,
    // returning a CFArray of CFNumber Space IDs (NULL on failure, in which
    // case yabai falls back to the window's display Space). Read-only: no
    // SLS write symbol is resolved or called anywhere in this module.
    // Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L22-L22
    // and https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c#L89-L111
    // (live behavior still UNVERIFIED on a Mac — a
    // missing symbol or null result is `Err`, never guessed data).

    const SLS_SPACES_SELECTOR: i32 = 0x7;

    struct SlsMembership {
        copy_spaces_for_windows: unsafe extern "C" fn(i32, i32, RawConstRef) -> RawRef,
    }

    static SLS_MEMBERSHIP: OnceLock<Option<SlsMembership>> = OnceLock::new();

    fn sls_membership() -> Option<&'static SlsMembership> {
        SLS_MEMBERSHIP
            .get_or_init(|| {
                // SAFETY: symbol null-checked; signature matches the pinned
                // yabai declaration above. Returned array is retained and
                // wrapped by the caller by copy-rule contract.
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let cname = CString::new("SLSCopySpacesForWindows").expect("static");
                    let sym = libc::dlsym(handle, cname.as_ptr());
                    if sym.is_null() {
                        return None;
                    }
                    Some(SlsMembership {
                        copy_spaces_for_windows: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, i32, RawConstRef) -> RawRef,
                        >(sym),
                    })
                }
            })
            .as_ref()
    }

    /// Read-only Space membership for up to [`MAX_MEMBERSHIP_WINDOWS`]
    /// window IDs. Returns the Space IDs owning those windows (IDs only).
    /// Missing symbols, null results, or malformed containers are `Err`
    /// with the cause named — never fabricated membership.
    pub fn sls_spaces_for_windows(wids: &[u32]) -> Result<Vec<u64>, String> {
        if wids.is_empty() || wids.len() > MAX_MEMBERSHIP_WINDOWS {
            return Err(format!(
                "window count out of bound 1..={MAX_MEMBERSHIP_WINDOWS}"
            ));
        }
        let table = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        let membership =
            sls_membership().ok_or_else(|| "SLSCopySpacesForWindows unavailable".to_owned())?;
        // SAFETY: window list built with the maintained `CFNumber`/`CFArray`
        // wrappers; the private call receives a live pointer; the retained
        // top-level result is wrapped as `CFType` and checked-downcast to
        // `CFArray` before any typed view; elements are indexed within
        // `len().min(...)` (no unbounded materialization); Space IDs come
        // from checked per-element downcasts below.
        unsafe {
            let cid = (table.connection)();
            let numbers: Vec<CFRetained<CFNumber>> = wids
                .iter()
                .map(|wid| CFNumber::new_i32(*wid as i32))
                .collect();
            let window_list: CFRetained<CFArray<CFNumber>> =
                CFArray::from_retained_objects(&numbers);
            let list_ptr = CFRetained::as_ptr(&window_list).as_ptr() as *const std::ffi::c_void;
            let spaces_raw =
                (membership.copy_spaces_for_windows)(cid, SLS_SPACES_SELECTOR, list_ptr);
            if spaces_raw.is_null() {
                return Err("SLSCopySpacesForWindows returned null".to_owned());
            }
            let spaces_top: CFRetained<CFType> =
                CFRetained::from_raw(NonNull::new(spaces_raw as *mut CFType).expect("non-null"));
            let spaces_untyped: CFRetained<CFArray> = spaces_top.downcast().map_err(|_| {
                "SLSCopySpacesForWindows returned a non-array top-level container".to_owned()
            })?;
            // SAFETY: top-level kind verified by the downcast above; viewed
            // as `CFArray<CFType>` (layouts identical; all elements CFTypes).
            let spaces: CFRetained<CFArray<CFType>> = CFRetained::cast_unchecked(spaces_untyped);
            let mut out = Vec::new();
            for i in 0..spaces.len().min(MAX_SPACES_PER_DISPLAY) {
                if let Some(item) = spaces.get(i)
                    && let Ok(num) = item.downcast::<CFNumber>()
                    && let Some(sid) = num.as_i64()
                    && sid >= 0
                {
                    out.push(sid as u64);
                }
            }
            Ok(out)
        }
    }

    // -- Public CGWindowList identity (maintained `core-graphics` binding) --
    //
    // Owned helper window IDs come from `CGWindowListCopyWindowInfo` via the
    // maintained `core-graphics` 0.25.0 crate (pinned in Cargo.lock), never
    // from AX: no Accessibility consent and no AX dependency. The list uses
    // `kCGWindowListOptionAll` (not on-screen-only) so owned windows on
    // inactive Spaces still join; an on-screen-only snapshot cannot serve
    // hidden-membership reads. IDs, PIDs, layers, on-screen flags, bounds,
    // and the window title are surfaced — never owner names or content. The
    // owned helper's title carries its per-run nonce (see
    // [`crate::probe_target`]); a missing title fails closed at the live
    // nonce check, never guessed.

    /// Bounds in screen points (`kCGWindowBounds` X/Y/Width/Height).
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgBounds {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    /// One CGWindowList entry: identity plus geometry plus window title.
    /// The title is identity only (the owned helper's nonce title); owner
    /// names are never surfaced.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct CgWindow {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
        /// `kCGWindowName` when the window server reports one; `None` when
        /// absent (e.g. untitled windows or withheld names). A missing owned
        /// title fails closed at the live nonce check.
        pub title: Option<String>,
    }

    /// Full window list via the maintained `core-graphics` binding, parsed
    /// with maintained `core-foundation` accessors. Bounded to
    /// [`crate::config::MAX_WINDOWS`]; entries missing a positive numeric ID
    /// or PID are skipped (never guessed); a null list is `Err`, never an
    /// empty claim.
    pub fn cg_window_list_all() -> Result<Vec<CgWindow>, String> {
        use core_foundation::array::CFArray;
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::{CFNumber, CFNumberRef};
        use core_foundation::string::CFString;
        use core_graphics::window as cgwin;

        let list: CFArray =
            cgwin::copy_window_info(cgwin::kCGWindowListOptionAll, cgwin::kCGNullWindowID)
                .ok_or_else(|| "CGWindowListCopyWindowInfo returned null".to_owned())?;
        // SAFETY: `kCGWindow*` are live framework constants;
        // `wrap_under_get_rule` borrows without ownership transfer; the list
        // outlives this call; numeric reads are gated by `to_*` results and
        // every raw pointer is null-checked before wrapping.
        unsafe {
            let key_number = CFString::wrap_under_get_rule(cgwin::kCGWindowNumber);
            let key_pid = CFString::wrap_under_get_rule(cgwin::kCGWindowOwnerPID);
            let key_layer = CFString::wrap_under_get_rule(cgwin::kCGWindowLayer);
            let key_onscreen = CFString::wrap_under_get_rule(cgwin::kCGWindowIsOnscreen);
            let key_bounds = CFString::wrap_under_get_rule(cgwin::kCGWindowBounds);
            let key_name = CFString::wrap_under_get_rule(cgwin::kCGWindowName);
            let raw_key = |key: &CFString| key.as_concrete_TypeRef() as *const std::ffi::c_void;
            let true_ref = CFBoolean::true_value().as_concrete_TypeRef() as *const std::ffi::c_void;
            let as_number =
                |dict: &CFDictionary, key: *const std::ffi::c_void| -> Option<CFNumber> {
                    let raw: *const std::ffi::c_void = *dict.find(key)?;
                    if raw.is_null() {
                        return None;
                    }
                    Some(CFNumber::wrap_under_get_rule(raw as CFNumberRef))
                };
            let bounds_of = |dict: &CFDictionary| -> Option<CgBounds> {
                let raw: *const std::ffi::c_void = *dict.find(raw_key(&key_bounds))?;
                if raw.is_null() {
                    return None;
                }
                let sub: CFDictionary = CFDictionary::wrap_under_get_rule(raw as CFDictionaryRef);
                let get = |name: &str| -> Option<f64> {
                    let k = CFString::new(name);
                    let r: *const std::ffi::c_void =
                        *sub.find(k.as_concrete_TypeRef() as *const std::ffi::c_void)?;
                    if r.is_null() {
                        return None;
                    }
                    CFNumber::wrap_under_get_rule(r as CFNumberRef).to_f64()
                };
                Some(CgBounds {
                    x: get("X")?,
                    y: get("Y")?,
                    w: get("Width")?,
                    h: get("Height")?,
                })
            };
            let mut out = Vec::new();
            for i in 0..list.len() {
                if out.len() >= crate::config::MAX_WINDOWS {
                    break;
                }
                let item = match list.get(i) {
                    Some(v) => v,
                    None => continue,
                };
                let dict_ref: *const std::ffi::c_void = *item;
                if dict_ref.is_null() {
                    continue;
                }
                let dict = CFDictionary::wrap_under_get_rule(dict_ref as CFDictionaryRef);
                let wid = as_number(&dict, raw_key(&key_number))
                    .and_then(|n| n.to_i64())
                    .filter(|v| *v > 0);
                let pid = as_number(&dict, raw_key(&key_pid))
                    .and_then(|n| n.to_i64())
                    .filter(|v| *v > 0);
                let (wid, pid) = match (wid, pid) {
                    (Some(w), Some(p)) => (w as u32, p as u32),
                    _ => continue,
                };
                if wid == 0 {
                    continue;
                }
                let layer = as_number(&dict, raw_key(&key_layer))
                    .and_then(|n| n.to_i32())
                    .unwrap_or(0);
                let onscreen = dict
                    .find(raw_key(&key_onscreen))
                    .map(|v| *v == true_ref)
                    .unwrap_or(false);
                let bounds = bounds_of(&dict).unwrap_or(CgBounds {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                });
                // Window title (`kCGWindowName`) when the window server
                // reports one; absent (nil/missing/non-string) is `None`,
                // never guessed. Only the owned helper's nonce title is
                // ever compared; foreign titles are opaque here.
                let title = dict
                    .find(raw_key(&key_name))
                    .and_then(|v| {
                        let raw = *v;
                        if raw.is_null() {
                            None
                        } else {
                            Some(CFString::wrap_under_get_rule(
                                raw as core_foundation::string::CFStringRef,
                            ))
                        }
                    })
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty());
                out.push(CgWindow {
                    wid,
                    pid,
                    layer,
                    onscreen,
                    bounds,
                    title,
                });
            }
            Ok(out)
        }
    }

    // -- Tier-2 Space-move writes (direct SLS only; SA and compat DISABLED) --
    //
    // Route determination pinned against yabai `src/space_manager.c`:
    // `space_manager_move_window_list_to_space` /
    // `space_manager_move_window_to_space` try (1) the async bridged operation
    // when `SLSPerformAsynchronousBridgedWindowManagementOperation` resolves
    // (bound in yabai `src/yabai.c`), else (2) direct
    // `SLSMoveWindowsToManagedSpace` when `workspace_use_macos_space_workaround()`
    // is false, else (3) an SA attempt first and then the compat-ID fallback
    // (`SLSSpaceSetCompatID(sid, 0x79616265)` +
    // `SLSSetWindowListWorkspace(..., 0x79616265)` + reset to `0x0`).
    // Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
    // Declarations pinned against yabai `src/misc/extern.h`:
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L63-L63
    // (`SLSMoveWindowsToManagedSpace`) and
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L96-L97
    // (compat pair). The workaround gate forcing branch (3) on macOS 15 is in
    // yabai `src/workspace.m`:
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26
    // Tier-2 omits every SA attempt. The compat-ID route is additionally
    // DISABLED as a mutation: `SLSSpaceSetCompatID` mutates global Space
    // metadata, only the setter is covered locally (no getter, no exact
    // preimage), the `0x79616265` workspace tag collides with yabai, and a
    // reset-to-zero on an unproven baseline is not restoration — so compat
    // stays symbol-availability diagnostics only, never a callable write.
    // The bridged async operation is likewise detected but NOT executed
    // (async ObjC API with no synchronous effect readback in surveyed
    // source). The sole callable write is direct
    // `SLSMoveWindowsToManagedSpace`: source-derived, UNTESTED live, and
    // potentially ineffective on the macOS 15 floor (where upstream prefers
    // the workaround branch). It returns void: dispatch is never effect
    // evidence — callers must read back `SLSCopySpacesForWindows`
    // membership afterwards. Live behavior of the path below is UNTESTED;
    // dispatch plus readbacks are the evidence.

    struct SlsMoveTable {
        move_direct: Option<unsafe extern "C" fn(i32, RawConstRef, u64)>,
        /// Compat pair presence (diagnostics only; never called).
        compat_pair_present: bool,
        bridged_async: bool,
    }

    static SLS_MOVE_TABLE: OnceLock<Option<SlsMoveTable>> = OnceLock::new();

    fn sls_move_table() -> Option<&'static SlsMoveTable> {
        SLS_MOVE_TABLE
            .get_or_init(|| {
                // SAFETY: the one transmuted symbol is null-checked before
                // transmuting to its pinned yabai declaration; compat/bridged
                // symbols are presence-checked only and never transmuted to a
                // callable type.
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let sym = |name: &str| {
                        let cname = CString::new(name).expect("static");
                        libc::dlsym(handle, cname.as_ptr())
                    };
                    let direct = sym("SLSMoveWindowsToManagedSpace");
                    let set_id = sym("SLSSpaceSetCompatID");
                    let set_ws = sym("SLSSetWindowListWorkspace");
                    let bridged = sym("SLSPerformAsynchronousBridgedWindowManagementOperation");
                    Some(SlsMoveTable {
                        move_direct: if direct.is_null() {
                            None
                        } else {
                            Some(std::mem::transmute::<
                                *mut std::ffi::c_void,
                                unsafe extern "C" fn(i32, RawConstRef, u64),
                            >(direct))
                        },
                        compat_pair_present: !set_id.is_null() && !set_ws.is_null(),
                        bridged_async: !bridged.is_null(),
                    })
                }
            })
            .as_ref()
    }

    /// Which Tier-2 move symbols resolved. Fail-closed readback surface for
    /// route selection; never guessed. `compat_pair` reports presence only:
    /// the compat route is blocked (no exact restoration) and has no callable
    /// wrapper.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsMoveSymbols {
        pub direct: bool,
        pub compat_pair: bool,
        pub bridged_async: bool,
    }

    pub fn sls_move_symbols() -> SlsMoveSymbols {
        match sls_move_table() {
            None => SlsMoveSymbols {
                direct: false,
                compat_pair: false,
                bridged_async: false,
            },
            Some(t) => SlsMoveSymbols {
                direct: t.move_direct.is_some(),
                compat_pair: t.compat_pair_present,
                bridged_async: t.bridged_async,
            },
        }
    }

    /// Direct dispatch. The API returns void: `Ok` means dispatched, never
    /// effected — the caller must read back membership.
    pub fn sls_move_windows_direct(sid: u64, wids: &[u32]) -> Result<(), String> {
        if wids.is_empty() || wids.len() > MAX_MEMBERSHIP_WINDOWS {
            return Err(format!(
                "window count out of bound 1..={MAX_MEMBERSHIP_WINDOWS}"
            ));
        }
        let table = sls_move_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        let direct = table
            .move_direct
            .ok_or_else(|| "SLSMoveWindowsToManagedSpace unavailable".to_owned())?;
        let base = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        // SAFETY: window list built with maintained wrappers; the private
        // call receives a live pointer; void return carries no data.
        unsafe {
            let cid = (base.connection)();
            let numbers: Vec<CFRetained<CFNumber>> = wids
                .iter()
                .map(|wid| CFNumber::new_i32(*wid as i32))
                .collect();
            let window_list: CFRetained<CFArray<CFNumber>> =
                CFArray::from_retained_objects(&numbers);
            let list_ptr = CFRetained::as_ptr(&window_list).as_ptr() as *const std::ffi::c_void;
            direct(cid, list_ptr, sid);
            Ok(())
        }
    }

    // -- Process identity (live proof, not caller claims) --

    /// Live (pid, bundle-id) pairs from NSWorkspace. Trusted `objc2`
    /// wrappers; no custom bindings.
    pub fn running_apps() -> Vec<(i32, String)> {
        use objc2_app_kit::NSWorkspace;
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        let count = apps.count();
        let mut out = Vec::with_capacity(count.min(4096));
        for i in 0..count.min(4096) {
            let app = apps.objectAtIndex(i);
            let pid = app.processIdentifier();
            let bundle = app
                .bundleIdentifier()
                .map(|b| b.to_string())
                .unwrap_or_default();
            out.push((pid, bundle));
        }
        out
    }

    pub fn bundle_id_for_pid(pid: i32) -> Option<String> {
        running_apps()
            .into_iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, b)| b)
            .filter(|b| !b.is_empty())
    }

    /// Frontmost application identity (PID plus bundle ID) from the maintained
    /// `objc2-app-kit` binding. No AX involved (works without consent) and no
    /// titles/content: PIDs and bundle IDs only. `None` when unreadable.
    /// Used for focus preimage baselines and frontmost-restore verification.
    pub fn frontmost_app() -> Option<(i32, String)> {
        use objc2_app_kit::NSWorkspace;
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let pid = app.processIdentifier();
        if pid <= 1 {
            return None;
        }
        let bundle = app
            .bundleIdentifier()
            .map(|b| b.to_string())
            .unwrap_or_default();
        Some((pid, bundle))
    }

    /// Normal application activation (no synthetic input, no window actions):
    /// asks the running application for `pid` to activate with default
    /// options. Returns whether the request was sent — focus arrival is async
    /// and MUST be verified separately (AX focused preimage or frontmost
    /// poll). Callers restrict targets to the owned helper (focus leg) and
    /// the recorded pre-frontmost app (restore leg); anything else,
    /// especially the controlling terminal's windows, is out of scope.
    pub fn activate_app(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => app.activateWithOptions(NSApplicationActivationOptions::empty()),
            None => false,
        }
    }

    /// Live executable path via maintained `libc::proc_pidpath`. Bounded buffer.
    pub fn process_executable_path(pid: i32) -> Option<String> {
        if pid <= 1 {
            return None;
        }
        let mut buf = vec![0u8; 4096];
        // SAFETY: scratch buffer with valid length; return value gates use.
        let ret = unsafe {
            libc::proc_pidpath(
                pid,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                buf.len() as u32,
            )
        };
        if ret > 0 {
            String::from_utf8(buf[..ret as usize].to_vec()).ok()
        } else {
            None
        }
    }

    pub fn process_exists(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        // SAFETY: signal 0 performs no delivery; return code is existence.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Send SIGTERM once. Returns whether the signal was delivered (not
    /// whether the process exited; callers poll `process_exists`).
    pub fn terminate_process(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        // SAFETY: exact PID supplied by the caller after identity checks.
        unsafe { libc::kill(pid, 15) == 0 }
    }

    // -- Owned probe window (AppKit boundary; main thread only) --

    /// Pump the current thread's runloop once, bounded, via the maintained
    /// `objc2-core-foundation` wrapper. The single `unsafe` is the
    /// extern-static default-mode read.
    pub fn pump_runloop_once(millis: u64) {
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, millis.min(1000) as f64 / 1000.0, false);
    }

    /// Drain pending AppKit events without blocking: dequeue whatever is
    /// available (distant past = return immediately, never wait) and dispatch
    /// each through the application. Without this, machinery that arrives as
    /// application events — notably activation requests — is never serviced:
    /// the request reports sent while focus never transfers and AX focus
    /// legs fail. Bounded: at most 64 dequeues per call (`None` ends the
    /// drain early), zero sleep. Maintained `objc2-app-kit` wrappers only.
    pub fn drain_app_events(app: &objc2_app_kit::NSApplication) {
        use objc2_app_kit::NSEventMask;
        use objc2_foundation::{NSDate, NSDefaultRunLoopMode};
        // SAFETY: extern-static default-mode read (same pattern as
        // `pump_runloop_once`); the static is a framework constant.
        let mode = unsafe { NSDefaultRunLoopMode };
        let expiration = NSDate::distantPast();
        for _ in 0..64 {
            let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&expiration),
                mode,
                true,
            );
            match event {
                Some(event) => app.sendEvent(&event),
                None => break,
            }
        }
    }

    /// Owned probe window runner. Creates the window on the calling (main)
    /// thread with the given title (the caller's per-run nonce title),
    /// pumps the runloop until it is serviced, invokes `ready`, and
    /// only then applies the optional stall (so a hung target is a *live*
    /// unresponsive window, not a missing one). Never touches other windows.
    /// A nonzero `close_window_ms` closes only this window at that deadline
    /// past `ready` (owned teardown stimulus for the destroyed leg) while the
    /// process keeps running to its deadline.
    ///
    /// No focus theft: the app uses the `Accessory` policy (no Dock icon,
    /// never key) and the window is shown with `orderFront:` (never
    /// `makeKeyAndOrderFront:`), so spawning the probe never steals keyboard
    /// focus from the user's session.
    pub fn run_owned_probe_window(
        seconds: u64,
        hung_ms: u64,
        title: &str,
        fixed_size: bool,
        defer_window_ms: u64,
        close_window_ms: u64,
        ready: &dyn Fn(),
    ) -> Result<(), String> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{
            NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow,
            NSWindowStyleMask,
        };
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "owned probe window must run on the main thread".to_owned())?;
        // SAFETY: fresh `alloc()` on the main thread; no pointers escape.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        // Complete the launch sequence without entering `run`: a bare
        // `sharedApplication` plus manual runloop pumping shows the window at
        // the window-server level (CG sees it) but never registers the app
        // with the accessibility system (AX sees zero windows), which fails
        // every owned AX leg. `finishLaunching` publishes the AX hierarchy
        // while the manual bounded pump below keeps lifetime control. It does
        // not activate, so focus is never stolen.
        app.finishLaunching();
        // Deferred creation for the creation-notification leg: pump bounded
        // (events drained, runloop sliced) with no window yet, so a
        // pre-window observer deterministically covers the creation below.
        // Still no focus theft (nothing to show until the window exists).
        let defer_left = defer_window_ms.min(20_000);
        let defer_deadline = Instant::now() + Duration::from_millis(defer_left);
        while Instant::now() < defer_deadline {
            // Each pump iteration drains an autorelease pool: a manual pump
            // without one never finalizes deferred AppKit teardown (e.g. a
            // programmatic window close announces destruction but its surface
            // lingers), and leaks autoreleased objects for the whole run.
            // Real NSApplication loops drain per event; this is that hygiene
            // for the manual loop.
            objc2::rc::autoreleasepool(|_| {
                drain_app_events(&app);
                pump_runloop_once(50);
            });
        }
        let frame = NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(480.0, 320.0));
        // Fault control for the live partial-write leg: without `Resizable`
        // the window accepts AX position writes but refuses AX size writes,
        // so a frame transaction against it must restore the exact preimage.
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | if fixed_size {
                NSWindowStyleMask::empty()
            } else {
                NSWindowStyleMask::Resizable
            };
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSWindow>(),
                frame,
                style,
                NSBackingStoreType(2),
                false,
            )
        };
        window.setTitle(&NSString::from_str(title));
        window.orderFront(None);
        objc2::rc::autoreleasepool(|_| {
            pump_runloop_once(200);
        });
        ready();
        if hung_ms > 0 {
            std::thread::sleep(Duration::from_millis(hung_ms.min(20_000)));
        }
        // Owned teardown stimulus: close only this window at the deadline,
        // then keep running windowless until the seconds deadline or the
        // holder stop. Programmatic close of the owned window (no user
        // action, no other window touched). With a stall active the close
        // waits for the stall (documented); the session holder uses no stall
        // here, so the deadline is exact.
        let close_at = if close_window_ms > 0 {
            Some(Instant::now() + Duration::from_millis(close_window_ms.min(20_000)))
        } else {
            None
        };
        let mut closed = false;
        let deadline = Instant::now() + Duration::from_secs(seconds.clamp(1, 600));
        while Instant::now() < deadline {
            if !closed
                && let Some(close_deadline) = close_at
                && Instant::now() >= close_deadline
            {
                window.close();
                closed = true;
                // First-party close report: execution proof, the window's own
                // visibility readback, plus the helper's own window-server
                // readback (own-PID window count after the close). The holder
                // awaits this marker AND an independent CG absence readback;
                // either alone proves nothing.
                let own_windows = cg_window_list_all()
                    .map(|list| {
                        let me = std::process::id();
                        list.iter().filter(|w| w.pid == me).count()
                    })
                    .unwrap_or(usize::MAX);
                println!(
                    "{}",
                    serde_json::json!({"closed": true, "windows": own_windows, "visible": window.isVisible()})
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            // Service application events (activation and friends) as well as
            // the bare runloop: focus machinery arrives as NSEvents, and a
            // pump that never dequeues them leaves activation parked.
            objc2::rc::autoreleasepool(|_| {
                drain_app_events(&app);
                pump_runloop_once(50);
            });
        }
        Ok(())
    }

    // -- Bounded event tap (trusted core-graphics wrapper) --

    struct TapShared {
        tx: SyncSender<EngineMessage>,
        tuning: crate::input_match::TapTuning,
        suppress_armed: bool,
        matched: AtomicU32,
        suppressed: AtomicU32,
        deliveries: AtomicU32,
        disabled: AtomicBool,
        down_pending: AtomicBool,
        disable_notified: AtomicBool,
    }

    /// Bounded tap for exactly one chord with an independent RAII lifetime.
    /// Default (observe-only): the callback always returns `Keep`; matches
    /// are counted and forwarded as exact-chord engine events, everything
    /// else only counted as deliveries. Armed (`--suppress`): exact-chord
    /// `KeyDown`/`KeyUp` pairs return `Drop` (see
    /// [`crate::input_match::decide_tap_event`]); unrelated keys, inexact
    /// modifiers, lone ups, and `FlagsChanged` always `Keep`. Timeout-disable
    /// re-enables once and notifies the engine exactly once (informational:
    /// a disable is not recovery evidence); a second disable stops the run.
    /// Drop of the tap invalidates it (RAII).
    pub fn run_event_tap_bounded(
        want_flags: u64,
        flags_mask: u64,
        want_keycode: i64,
        seconds: u64,
        suppress_armed: bool,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<TapRunReport, TapSetupError> {
        use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
        use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
        use core_graphics::event::{
            CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
            CallbackResult, EventField,
        };

        let shared = Arc::new(TapShared {
            tx: tx.clone(),
            tuning: TapTuning {
                want_flags,
                flags_mask,
                want_code: want_keycode,
            },
            suppress_armed,
            matched: AtomicU32::new(0),
            suppressed: AtomicU32::new(0),
            deliveries: AtomicU32::new(0),
            disabled: AtomicBool::new(false),
            down_pending: AtomicBool::new(false),
            disable_notified: AtomicBool::new(false),
        });
        let callback_shared = shared.clone();
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, etype, event| {
                // `CGEventType` has no `PartialEq`; compare discriminants.
                let kind = etype as u32;
                if kind == CGEventType::TapDisabledByTimeout as u32
                    || kind == CGEventType::TapDisabledByUserInput as u32
                {
                    callback_shared.disabled.store(true, Ordering::SeqCst);
                    return CallbackResult::Keep;
                }
                let tap_kind = if kind == CGEventType::KeyDown as u32 {
                    TapKeyKind::KeyDown
                } else if kind == CGEventType::KeyUp as u32 {
                    TapKeyKind::KeyUp
                } else if kind == CGEventType::FlagsChanged as u32 {
                    TapKeyKind::Other
                } else {
                    return CallbackResult::Keep;
                };
                if matches!(tap_kind, TapKeyKind::KeyDown | TapKeyKind::KeyUp) {
                    callback_shared.deliveries.fetch_add(1, Ordering::SeqCst);
                }
                let flags = event.get_flags().bits();
                let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                let mut pending = callback_shared.down_pending.load(Ordering::SeqCst);
                let decision = decide_tap_event(
                    tap_kind,
                    flags,
                    code,
                    callback_shared.tuning,
                    callback_shared.suppress_armed,
                    &mut pending,
                );
                callback_shared.down_pending.store(pending, Ordering::SeqCst);
                if decision.matched {
                    callback_shared.matched.fetch_add(1, Ordering::SeqCst);
                    let _ = callback_shared.tx.try_send(EngineMessage::InputEvent {
                        key_code: code as u32,
                    });
                }
                if decision.suppress {
                    callback_shared.suppressed.fetch_add(1, Ordering::SeqCst);
                    return CallbackResult::Drop;
                }
                CallbackResult::Keep
            },
        )
        .map_err(|()| {
            TapSetupError::Denied(
                "CGEventTapCreate returned null (needs Accessibility consent for the stable bundle identity; grant in System Settings, then retry)"
                    .to_owned(),
            )
        })?;
        let source = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|()| TapSetupError::Runloop("runloop source creation failed".to_owned()))?;
        // SAFETY: extern-static mode read; source added/removed symmetrically
        // below; tap Drop invalidates the port (RAII disable).
        unsafe {
            CFRunLoop::get_current().add_source(&source, kCFRunLoopDefaultMode);
        }
        tap.enable();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(seconds.max(1));
        let mut reenabled = false;
        let mut second_disable = false;
        while Instant::now() < deadline {
            unsafe {
                CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(250), false);
            }
            if shared.disabled.load(Ordering::SeqCst) {
                // Notify the engine at most once per run. A disable is
                // lifecycle information only, never recovery evidence.
                if !shared.disable_notified.swap(true, Ordering::SeqCst) {
                    let _ = shared.tx.try_send(EngineMessage::TapDisabled);
                }
                if !reenabled {
                    tap.enable();
                    shared.disabled.store(false, Ordering::SeqCst);
                    reenabled = true;
                } else {
                    second_disable = true;
                    break;
                }
            }
        }
        unsafe {
            CFRunLoop::get_current().remove_source(&source, kCFRunLoopDefaultMode);
        }
        Ok(TapRunReport {
            created: true,
            matched: shared.matched.load(Ordering::SeqCst),
            suppressed: shared.suppressed.load(Ordering::SeqCst),
            deliveries: shared.deliveries.load(Ordering::SeqCst),
            reenabled_after_timeout_disable: reenabled,
            stopped_on_second_disable: second_disable,
            suppress_armed: shared.suppress_armed,
            requested_secs: seconds,
            elapsed_secs: start.elapsed().as_secs(),
        })
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    AxElement, AxFrame, CarbonHotkey, CgBounds, CgWindow, SlsMoveSymbols, activate_app,
    ax_focused_preimage, ax_get_frame, ax_is_process_trusted, ax_observe_bounded,
    ax_observe_session, ax_raise, ax_set_frame, ax_set_messaging_timeout, ax_set_position,
    ax_set_size, ax_window_id, ax_window_title, bundle_id_for_pid, carbon_register,
    cg_post_event_access_known, cg_window_list_all, frontmost_app, process_executable_path,
    process_exists, pump_runloop_once, run_event_tap_bounded, run_owned_probe_window, running_apps,
    secure_input_known, sls_connection_available, sls_display_spaces, sls_main_connection_id,
    sls_move_symbols, sls_move_windows_direct, sls_spaces_for_windows, terminate_process,
};

#[cfg(not(target_os = "macos"))]
mod stub {
    use super::*;
    use std::time::Duration;

    pub struct AxElement {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl AxElement {
        pub fn application(_pid: i32) -> Option<Self> {
            None
        }

        pub fn system_wide() -> Option<Self> {
            None
        }

        pub fn windows(&self) -> Result<Vec<AxElement>, i32> {
            Err(AX_NOT_IMPLEMENTED)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct AxFrame {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    impl AxFrame {
        pub fn valid_for_set(&self) -> bool {
            self.x.is_finite()
                && self.y.is_finite()
                && self.w.is_finite()
                && self.h.is_finite()
                && self.w > 0.0
                && self.h > 0.0
        }
    }

    pub struct CarbonHotkey {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgBounds {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct CgWindow {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
        pub title: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsMoveSymbols {
        pub direct: bool,
        pub compat_pair: bool,
        pub bridged_async: bool,
    }

    pub fn cg_window_list_all() -> Result<Vec<CgWindow>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_move_symbols() -> SlsMoveSymbols {
        SlsMoveSymbols {
            direct: false,
            compat_pair: false,
            bridged_async: false,
        }
    }

    pub fn sls_move_windows_direct(_sid: u64, _wids: &[u32]) -> Result<(), String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_is_process_trusted() -> Option<bool> {
        None
    }

    pub fn ax_set_messaging_timeout(_element: &AxElement, _seconds: f32) -> i32 {
        AX_NOT_IMPLEMENTED
    }

    pub fn ax_window_id(_element: &AxElement) -> Option<u32> {
        None
    }

    pub fn ax_focused_preimage() -> Option<(i32, Option<u32>)> {
        None
    }

    pub fn ax_get_frame(_element: &AxElement) -> Result<AxFrame, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_frame(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_position(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_size(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_window_title(_element: &AxElement) -> Result<Option<String>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn cg_post_event_access_known() -> Option<bool> {
        None
    }

    pub fn ax_raise(_element: &AxElement) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_observe_bounded(
        _pid: i32,
        _timeout: Duration,
        _messaging_timeout_secs: f32,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<AxObserveOutcome, AxObserveError> {
        Err(AxObserveError::Unsupported("macOS-only".to_owned()))
    }

    pub fn ax_observe_session(
        _pid: i32,
        _window_wid: Option<u32>,
        _timeout: Duration,
        _messaging_timeout_secs: f32,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<AxSessionOutcome, AxObserveError> {
        Err(AxObserveError::Unsupported("macOS-only".to_owned()))
    }

    pub fn carbon_register(
        _keycode: u32,
        _carbon_modifiers: u32,
        _counter: Arc<EventCounter>,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<CarbonHotkey, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn secure_input_known() -> Option<bool> {
        None
    }

    pub fn sls_connection_available() -> bool {
        false
    }

    pub fn sls_main_connection_id() -> Option<i32> {
        None
    }

    pub fn sls_display_spaces() -> Result<Vec<SlsDisplay>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_spaces_for_windows(_wids: &[u32]) -> Result<Vec<u64>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn running_apps() -> Vec<(i32, String)> {
        Vec::new()
    }

    pub fn bundle_id_for_pid(_pid: i32) -> Option<String> {
        None
    }

    pub fn frontmost_app() -> Option<(i32, String)> {
        None
    }

    pub fn activate_app(_pid: i32) -> bool {
        false
    }

    pub fn process_executable_path(_pid: i32) -> Option<String> {
        None
    }

    pub fn process_exists(_pid: i32) -> bool {
        false
    }

    pub fn terminate_process(_pid: i32) -> bool {
        false
    }

    pub fn pump_runloop_once(_millis: u64) {}

    pub fn run_event_tap_bounded(
        _want_flags: u64,
        _flags_mask: u64,
        _want_keycode: i64,
        _seconds: u64,
        _suppress_armed: bool,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<TapRunReport, TapSetupError> {
        Err(TapSetupError::Denied("macOS-only".to_owned()))
    }

    pub fn run_owned_probe_window(
        _seconds: u64,
        _hung_ms: u64,
        _title: &str,
        _fixed_size: bool,
        _defer_window_ms: u64,
        _close_window_ms: u64,
        _ready: &dyn Fn(),
    ) -> Result<(), String> {
        Err("owned probe window is macOS-only".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub use stub::{
    AxElement, AxFrame, CarbonHotkey, CgBounds, CgWindow, SlsMoveSymbols, activate_app,
    ax_focused_preimage, ax_get_frame, ax_is_process_trusted, ax_observe_bounded,
    ax_observe_session, ax_raise, ax_set_frame, ax_set_messaging_timeout, ax_set_position,
    ax_set_size, ax_window_id, ax_window_title, bundle_id_for_pid, carbon_register,
    cg_post_event_access_known, cg_window_list_all, frontmost_app, process_executable_path,
    process_exists, pump_runloop_once, run_event_tap_bounded, run_owned_probe_window, running_apps,
    secure_input_known, sls_connection_available, sls_display_spaces, sls_main_connection_id,
    sls_move_symbols, sls_move_windows_direct, sls_spaces_for_windows, terminate_process,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_error_names_cover_known_codes() {
        assert_eq!(ax_error_name(0), "kAXErrorSuccess");
        assert!(ax_error_name(-25204).contains("CannotComplete"));
        assert!(ax_error_name(99999).contains("99999"));
    }

    #[test]
    fn frame_validation_rejects_degenerate() {
        let bad = AxFrame {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 10.0,
        };
        assert!(!bad.valid_for_set());
        let good = AxFrame {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
        };
        assert!(good.valid_for_set());
    }

    #[test]
    fn sls_probe_never_panics() {
        let _ = sls_connection_available();
        let _ = sls_main_connection_id();
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn stub_fails_before_native_methods() {
        // Off-macOS stub: discovery is empty and element construction fails
        // fast, so owned gates refuse before any native AX path is reached.
        assert!(running_apps().is_empty());
        assert!(bundle_id_for_pid(4242).is_none());
        assert!(AxElement::application(4242).is_none());
        assert!(AxElement::system_wide().is_none());
        assert!(ax_is_process_trusted().is_none());
        assert!(sls_display_spaces().is_err());
    }

    #[test]
    fn counter_counts() {
        let c = EventCounter::new();
        assert_eq!(c.hits(), 0);
        c.bump();
        c.bump();
        assert_eq!(c.hits(), 2);
    }

    #[test]
    fn post_event_preflight_never_prompts_or_posts() {
        // Read-only preflight: returns the observed state or unknown, never
        // requests access and never posts an event. Calling it twice must
        // agree (no side effects).
        let first = cg_post_event_access_known();
        let second = cg_post_event_access_known();
        assert_eq!(first, second);
    }
}
