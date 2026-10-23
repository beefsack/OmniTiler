#![forbid(unsafe_code)]
#[cfg(test)]
use super::*;

// Pending-drop owner-integration seams (test-only, offline, native-free).
// Each injector is `None` in ordinary tests (fail-closed native behavior);
// pending fixtures set them to script deterministic owner reads/effects
// without creating windows, hooks, or mutating the desktop. All use safe
// `std` only, mirroring the existing `INJECTED_HINTS` pattern.
#[cfg(test)]
thread_local! {
    #[allow(clippy::type_complexity)]
    pub(super) static INJECTED_OBSERVE: std::cell::RefCell<
        Option<(Vec<ObservedWindow>, Vec<RetainedRow>, bool)>,
    > = const { std::cell::RefCell::new(None) };
    #[allow(clippy::missing_const_for_thread_local)]
    static INJECTED_TAGS: std::cell::RefCell<std::collections::HashMap<u64, Option<String>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_TAGS_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static INJECTED_SUSPEND: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    pub(super) static INJECTED_ELEVATED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// Scripted native inputs for the elevation probe (test-only, offline):
    /// the captured foreground HWND, its PID, the process-open verdict, and
    /// the integrity read (read only when the open succeeds; `None` reads as
    /// a read failure). Runs the production classifier without touching
    /// windows or processes. Coarse `INJECTED_ELEVATED` still wins when set
    /// (honest `skipped-injected`). Cleared by [`clear_pending_seams`].
    pub(super) static INJECTED_ELEV_PROBE: std::cell::RefCell<Option<InjectedElevProbe>> =
        const { std::cell::RefCell::new(None) };
    /// Scripted window-class reads by HWND (test-only, offline): stands in
    /// for the native class query when activated. Unscripted HWNDs read as
    /// `None` (honest unknown) without a native call. Otherwise the
    /// production read runs. Cleared by [`clear_pending_seams`].
    #[allow(clippy::missing_const_for_thread_local)]
    static INJECTED_FG_CLASS: std::cell::RefCell<std::collections::HashMap<u64, Option<String>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_FG_CLASS_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static INJECTED_STOP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Scripted foreground HWND for the pending/cross settle command routes
    /// (test-only, offline): stands in for `GetForegroundWindow` when set.
    /// `Some(0)` requests no foreground (unknown, fail closed like the
    /// production zero read). `None` (unset) runs the production read.
    /// Cleared by [`clear_pending_seams`].
    pub(super) static INJECTED_FOREGROUND: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
    /// Scripted parent handles for taskbar-subtree validation (test-only):
    /// stands in for `GetParent` without native windows. When active,
    /// unscripted HWNDs read as `0` (top-level) without a native call.
    /// Cleared by [`clear_pending_seams`].
    static INJECTED_PARENTS: std::cell::RefCell<std::collections::HashMap<u64, u64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static INJECTED_PARENTS_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Scripted visible stacking sequence (top-to-bottom native order) for
    /// the next injected observation (test-only): the authoritative verified
    /// live-frame member sequence the real path derives from its single
    /// `EnumWindows` enumeration. Unset behaves as observed-then-retained,
    /// so existing fixtures without an explicit order read exactly as before.
    /// Entries must use full member identities already present in the
    /// injected observed/retained frames; non-members are ignored by the same
    /// retain rule as production. Cleared by [`clear_pending_seams`].
    pub(super) static INJECTED_ZORDER: std::cell::RefCell<
        Option<Vec<crate::native_guards::WindowKey>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Inject one observed/enumeration snapshot for offline ticks (test-only).
#[cfg(test)]
pub(super) fn inject_pending_observe(
    observed: Vec<ObservedWindow>,
    retained: Vec<RetainedRow>,
    enum_failed: bool,
) {
    INJECTED_OBSERVE.with(|slot| {
        *slot.borrow_mut() = Some((observed, retained, enum_failed));
    });
}

#[cfg(test)]
pub(super) fn inject_pending_tag(hwnd: u64, tag: Option<String>) {
    INJECTED_TAGS.with(|map| {
        map.borrow_mut().insert(hwnd, tag);
    });
    INJECTED_TAGS_ACTIVE.with(|active| active.set(true));
}

#[cfg(test)]
pub(super) fn inject_pending_suspend(blocked: Option<bool>) {
    INJECTED_SUSPEND.with(|slot| slot.set(blocked));
}

#[cfg(test)]
pub(super) fn inject_pending_stop(stopped: bool) {
    INJECTED_STOP.with(|slot| slot.set(stopped));
}

/// Script the foreground HWND for the pending/cross settle command routes
/// (test-only, offline): `Some(hwnd)` stands in for `GetForegroundWindow`;
/// `Some(0)` is no foreground. Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_foreground(foreground: Option<u64>) {
    INJECTED_FOREGROUND.with(|slot| slot.set(foreground));
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct InjectedElevProbe {
    pub(super) foreground: u64,
    pub(super) pid: u32,
    pub(super) open_ok: bool,
    pub(super) integrity: Option<std::result::Result<u32, ()>>,
}

#[cfg(test)]
pub(super) fn inject_pending_elevated(elevated: Option<bool>) {
    INJECTED_ELEVATED.with(|slot| slot.set(elevated));
}

/// Script the elevation probe's native inputs (test-only, offline): the
/// captured foreground HWND, its PID, the process-open verdict, and the
/// integrity read. Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_elev_probe(
    foreground: u64,
    pid: u32,
    open_ok: bool,
    integrity: Option<std::result::Result<u32, ()>>,
) {
    INJECTED_ELEV_PROBE.with(|slot| {
        *slot.borrow_mut() = Some(InjectedElevProbe {
            foreground,
            pid,
            open_ok,
            integrity,
        });
    });
}

/// Script one window-class read (test-only, offline): `None` is an honest
/// unreadable class. Activates the class seam like [`inject_pending_tag`].
/// Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_fg_class(hwnd: u64, class: Option<String>) {
    INJECTED_FG_CLASS.with(|map| {
        map.borrow_mut().insert(hwnd, class);
    });
    INJECTED_FG_CLASS_ACTIVE.with(|active| active.set(true));
}

/// Script parent handles for taskbar-subtree validation (test-only):
/// child HWND to its `GetParent` value (`0` for top-level). Unscripted
/// HWNDs read as `0` without a native call once activated.
/// Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_parents(parents: Vec<(u64, u64)>) {
    INJECTED_PARENTS.with(|map| {
        *map.borrow_mut() = parents.into_iter().collect();
    });
    INJECTED_PARENTS_ACTIVE.with(|active| active.set(true));
}

/// Script the visible stacking sequence (top-to-bottom) for the next
/// injected observation (test-only). Cleared by [`clear_pending_seams`].
#[cfg(test)]
pub(super) fn inject_pending_zorder(order: Vec<crate::native_guards::WindowKey>) {
    INJECTED_ZORDER.with(|slot| {
        *slot.borrow_mut() = Some(order);
    });
}

#[cfg(test)]
pub(super) fn clear_pending_seams() {
    INJECTED_OBSERVE.with(|slot| {
        *slot.borrow_mut() = None;
    });
    INJECTED_TAGS.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_TAGS_ACTIVE.with(|active| active.set(false));
    INJECTED_SUSPEND.with(|slot| slot.set(None));
    INJECTED_ELEVATED.with(|slot| slot.set(None));
    INJECTED_ELEV_PROBE.with(|slot| {
        *slot.borrow_mut() = None;
    });
    INJECTED_FG_CLASS.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_FG_CLASS_ACTIVE.with(|active| active.set(false));
    INJECTED_STOP.with(|slot| slot.set(false));
    INJECTED_FOREGROUND.with(|slot| slot.set(None));
    INJECTED_PARENTS.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_PARENTS_ACTIVE.with(|active| active.set(false));
    INJECTED_ZORDER.with(|slot| {
        *slot.borrow_mut() = None;
    });
    clear_test_hints();
}

/// Test-aware live member-tag read: injected tags stand in for the OS
/// property read when the pending fixture activated them (unit tests only;
/// compiled out of production builds). When active but unscripted for an
/// HWND, returns `None` without a native call so fixtures stay native-free.
/// Otherwise the production read runs.
pub(super) fn live_member_tag(hwnd: u64) -> Option<String> {
    #[cfg(test)]
    {
        if INJECTED_TAGS_ACTIVE.with(|active| active.get()) {
            return INJECTED_TAGS.with(|map| map.borrow().get(&hwnd).cloned().unwrap_or(None));
        }
    }
    crate::product_hide::sys::read_member_tag(hwnd)
}

/// Test-aware iconic-state read: fake-backend windows read their scripted
/// iconic flag first (offline hide effects); otherwise the production read
/// runs (fail-closed `false` on invalid HWNDs, so dead evidence never
/// refreshes).
pub(super) fn live_is_iconic(hwnd: u64) -> bool {
    #[cfg(test)]
    {
        if crate::product_hide::sys::fake::active(hwnd) {
            return crate::product_hide::sys::fake::with(hwnd, |w| w.iconic).unwrap_or(false);
        }
    }
    crate::ffi::is_iconic(hwnd)
}

/// Test-aware liveness read for the external-hide close fence: an invisible
/// member must still be a live window (`IsWindow`). Fake-backend windows
/// read their scripted liveness first (offline hide effects); otherwise the
/// production read runs. Compiled out differences only affect tests.
pub(super) fn live_is_window(hwnd: u64) -> bool {
    #[cfg(test)]
    {
        if crate::product_hide::sys::fake::active(hwnd) {
            return crate::product_hide::sys::fake::with(hwnd, |w| w.alive).unwrap_or(false);
        }
    }
    crate::ffi::is_window(hwnd)
}

/// Test-aware visibility read for the external-hide close fence.
/// Fake-backend windows read their scripted visibility first (offline hide
/// effects); otherwise the production read runs.
pub(super) fn live_is_visible(hwnd: u64) -> bool {
    #[cfg(test)]
    {
        if crate::product_hide::sys::fake::active(hwnd) {
            return crate::product_hide::sys::fake::with(hwnd, |w| w.visible).unwrap_or(false);
        }
    }
    crate::ffi::is_visible(hwnd)
}

/// Test-aware cloak read for the external-hide close fence: only a readable
/// uncloaked window may close (`Some(false)`). Fake-backend windows read
/// `Some(false)` while alive (hidden fakes are uncloaked, just hidden) and
/// `None` while dead. Otherwise the production read runs.
pub(super) fn live_cloaked(hwnd: u64) -> Option<bool> {
    #[cfg(test)]
    {
        if crate::product_hide::sys::fake::active(hwnd) {
            return crate::product_hide::sys::fake::with(hwnd, |w| w.alive.then_some(false))
                .flatten();
        }
    }
    crate::ffi::cloaked(hwnd)
}

/// Test-aware owning-PID read for the external-hide lifetime fence.
/// Fake-backend windows read their scripted pid first (offline hide
/// effects); otherwise the production read runs.
pub(super) fn live_window_pid(hwnd: u64) -> u32 {
    #[cfg(test)]
    {
        if crate::product_hide::sys::fake::active(hwnd) {
            return crate::product_hide::sys::fake::with(hwnd, |w| w.pid).unwrap_or(0);
        }
    }
    crate::ffi::window_pid(hwnd)
}

/// Test-aware foreground read for the pending/cross settle command routes:
/// the injected HWND stands in for `GetForegroundWindow` when scripted
/// (unit tests only; compiled out of production builds). Otherwise the
/// production read runs.
pub(super) fn live_foreground_window() -> u64 {
    #[cfg(test)]
    {
        if let Some(foreground) = INJECTED_FOREGROUND.with(|slot| slot.get()) {
            return foreground;
        }
    }
    crate::ffi::foreground_window()
}

/// Test-aware parent read for taskbar-subtree validation: injected parents
/// stand in for `GetParent` when the fixture activated them (unit tests
/// only; compiled out of production builds). When active but unscripted for
/// an HWND, returns `0` (top-level) without a native call so fixtures stay
/// native-free. Otherwise the production read runs.
pub(super) fn live_parent_of(hwnd: u64) -> u64 {
    #[cfg(test)]
    {
        if INJECTED_PARENTS_ACTIVE.with(|active| active.get()) {
            return INJECTED_PARENTS.with(|map| map.borrow().get(&hwnd).copied().unwrap_or(0));
        }
    }
    crate::ffi::parent_of(hwnd)
}

/// Test-aware window-class read: injected classes stand in for the native
/// `GetClassNameW` query when the fixture activated them (unit tests only;
/// compiled out of production builds). When active but unscripted for an
/// HWND, returns `None` without a native call so fixtures stay native-free.
/// Otherwise the production read runs (fail-closed `None` on unreadable
/// handles, never titles or paths).
pub(super) fn live_fg_class(hwnd: u64) -> Option<String> {
    #[cfg(test)]
    {
        if INJECTED_FG_CLASS_ACTIVE.with(|active| active.get()) {
            return INJECTED_FG_CLASS.with(|map| map.borrow().get(&hwnd).cloned().unwrap_or(None));
        }
    }
    crate::ffi::try_class_name(hwnd)
}
