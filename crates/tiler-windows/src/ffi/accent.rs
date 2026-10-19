//! Owner-thread-only system-accent source: the Personalization accent via
//! WinRT `UISettings::GetColorValue(UIColorType::Accent)`.
//!
//! Lifetime model: the owner thread initializes one STA apartment lazily on
//! the first resolve of a scope and holds it for the whole scope; every
//! resolve in the scope then activates a fresh local `UISettings` with
//! `RoActivateInstance` (never the process-wide static `FactoryCache` behind
//! `UISettings::new`, avoiding the cached-factory lifetime hazard consistent
//! with the observed invalid-vtable dispatch). All COM handles drop before
//! the matching `RoUninitialize` at scope shutdown. Repeated owner scopes in
//! one process each balance their own init/uninit pair sequentially.
//!
//! Broadcasts (`WM_DWMCOLORIZATIONCOLORCHANGED`, `WM_SETTINGCHANGE`) bump a
//! process-wide monotonic version; the single shared versioned cache (one
//! `AccentCache` in the owner state, consumed by both the border and the
//! pager) re-resolves only when the version moved or when it has no cache
//! yet. Versions are observed, never consumed: one consumer can never starve
//! another, and a broadcast landing mid-query re-triggers the next refresh
//! instead of being lost. The renderer never queries: it consumes the plain
//! snapshot the owner posts.

use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::System::WinRT::RoUninitialize;
use windows_sys::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};

use crate::active_border::{AccentRaw, SystemAccent};

static ACCENT_VERSION: AtomicU64 = AtomicU64::new(0);

/// Current accent broadcast version (observed, never consumed).
#[must_use]
pub fn accent_version() -> u64 {
    ACCENT_VERSION.load(Ordering::SeqCst)
}

/// Record one accent/theme broadcast. Coalesced by construction: any number
/// of broadcasts needs at most one re-resolve per scope.
pub(super) fn note_accent_changed() {
    ACCENT_VERSION.fetch_add(1, Ordering::SeqCst);
}

/// Documented WinRT init-result policy for the single lazy `RoInitialize`
/// of a scope. `S_OK` created a new apartment; `S_FALSE` entered an
/// already-initialized one. Both increment the apartment init count, so both
/// own exactly one matching `RoUninitialize` at shutdown. Failures own
/// nothing: `RPC_E_CHANGED_MODE` (a different apartment, e.g. host-owned
/// MTA) and any other error must not uninitialize, and the caller must not
/// query WinRT in that state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApartmentInit {
    NewApartment,
    AlreadyInitialized,
    ForeignOrFailed,
}

impl ApartmentInit {
    /// Classify one raw `RoInitialize` result. `Ok` maps from `S_OK`;
    /// anything else maps from its `HRESULT` code.
    const fn classify(hr: i32) -> Self {
        if hr == S_OK {
            Self::NewApartment
        } else if hr == S_FALSE {
            Self::AlreadyInitialized
        } else if hr == RPC_E_CHANGED_MODE {
            // A differently-initialized (e.g. host-owned MTA) apartment:
            // untouched, unqueried.
            Self::ForeignOrFailed
        } else {
            Self::ForeignOrFailed
        }
    }

    /// True only when this scope owns its matching `RoUninitialize` at
    /// shutdown. Both successes increment the init count, so both own one
    /// cleanup; failures own nothing.
    const fn owns_uninit(self) -> bool {
        !matches!(self, Self::ForeignOrFailed)
    }

    /// True only when WinRT may be queried on this thread.
    const fn winrt_usable(self) -> bool {
        !matches!(self, Self::ForeignOrFailed)
    }
}

// Raw `RoInitialize` import: the safe `windows` projection maps every
// non-negative HRESULT (including `S_FALSE`) to `Ok(())`, collapsing the
// new-apartment vs already-initialized distinction the init policy needs.
// This import observes the real HRESULT with the identical init mode
// (`RO_INIT_SINGLETHREADED`, 0); only the observed value differs.
#[link(name = "runtimeobject")]
unsafe extern "system" {
    fn RoInitialize(init_type: i32) -> i32;
}

/// Trace-only stage within one actual owner resolve. The owner observes
/// these synchronously to mark how far a resolve got before an abrupt exit;
/// steady-state cache hits never invoke the callback. The order is fixed:
/// `start` on entry, `init` only when `RoInitialize` actually runs (lazy,
/// once per scope - never per query), `activate` before `RoActivateInstance`,
/// `query` before `GetColorValue`, `done` on every return path. There is
/// deliberately no per-query release marker: the apartment stays alive for
/// the whole scope and uninitializes once at shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentStage {
    Start,
    Init,
    Activate,
    Query,
    Done,
}

impl AccentStage {
    /// Closed log vocabulary for the resolve stage.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Init => "init",
            Self::Activate => "activate",
            Self::Query => "query",
            Self::Done => "done",
        }
    }
}

/// Owner-thread-only WinRT accent source with a scope-lifetime apartment.
///
/// Lazy by construction (`new` performs no WinRT call, so offline fixtures
/// never touch WinRT); the first [`OwnerAccentSource::query_raw`] initializes
/// the STA apartment once and holds it until shutdown. The init result is
/// cached for the scope lifetime: no resolve re-runs `RoInitialize`, and a
/// failed init never retries (a foreign apartment cannot heal mid-scope).
/// Every resolve activates a fresh local `UISettings` via `RoActivateInstance`
/// on the runtime class name and casts to `UISettings`: no process-static
/// `windows-rs` `UISettings::new` `FactoryCache` is ever consulted.
///
/// RAII: [`OwnerAccentSource::shutdown`] (also run by `Drop`) balances
/// exactly the one `RoUninitialize` a successful init owns, on this thread.
/// Thread confinement is structural (`!Send`/`!Sync`): the value lives in the
/// owner state and only the owner thread resolves or shuts it down. All COM
/// objects drop before the matching `RoUninitialize`.
pub struct OwnerAccentSource {
    /// Cached `RoInitialize` HRESULT of the lazy scope init (`None` until
    /// the first resolve attempted it; never re-run afterwards).
    init_hr: Option<i32>,
    /// True while this scope holds a usable apartment.
    initialized: bool,
    /// Scope cleanup: the matching `RoUninitialize` (a plain function
    /// pointer so offline tests can account cleanups without WinRT).
    uninit: fn(),
    /// Owner-thread confinement: neither the source nor its apartment may
    /// cross threads.
    _confine: std::marker::PhantomData<*mut ()>,
}

/// Real scope cleanup: the matching `RoUninitialize` on this thread. Runs
/// only after a successful init, when every per-resolve `UISettings`
/// reference is already dropped.
fn real_uninit() {
    unsafe {
        RoUninitialize();
    }
}

impl OwnerAccentSource {
    /// New lazy source: no WinRT call, safe for offline fixtures.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            init_hr: None,
            initialized: false,
            uninit: real_uninit,
            _confine: std::marker::PhantomData,
        }
    }

    /// New lazy source with an injected cleanup (offline tests account
    /// `RoUninitialize` balance without WinRT).
    #[cfg(test)]
    fn new_with_uninit(uninit: fn()) -> Self {
        Self {
            init_hr: None,
            initialized: false,
            uninit,
            _confine: std::marker::PhantomData,
        }
    }

    /// True while this scope holds a usable apartment.
    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Ensure the scope apartment, initializing lazily on the first call.
    /// The `init` stage fires only when `RoInitialize` actually runs (once
    /// per scope at most); later resolves reuse the cached result silently,
    /// including a cached failure (no retry: a foreign apartment cannot heal
    /// mid-scope). [`OwnerAccentSource::shutdown`] clears the cache, so a
    /// later explicit query starts a new scope and re-initializes.
    /// Returns true only when WinRT may be queried on this thread.
    fn ensure_apartment(
        &mut self,
        init: impl FnOnce() -> i32,
        stage: &dyn Fn(AccentStage),
    ) -> bool {
        if let Some(hr) = self.init_hr {
            self.initialized = ApartmentInit::classify(hr).winrt_usable();
            return self.initialized;
        }
        stage(AccentStage::Init);
        let hr = init();
        self.init_hr = Some(hr);
        self.initialized = ApartmentInit::classify(hr).winrt_usable();
        self.initialized
    }

    /// One owner resolve: the cached `RoInitialize` HRESULT of the lazy
    /// scope init (reported on every record, but `RoInitialize` itself ran
    /// at most once per scope - see [`OwnerAccentSource::ensure_apartment`])
    /// plus the activation/query HRESULTs and the returned accent, if any.
    /// Activation/query stages that never ran report `None` (serialized as
    /// null, never a fabricated error). `trace`, when given, observes each
    /// stage synchronously within this resolve only.
    /// The activated `UISettings` is a local per-resolve reference dropped
    /// before return; the scope apartment stays alive across resolves.
    #[must_use]
    pub fn query_raw(&mut self, trace: Option<&dyn Fn(AccentStage)>) -> AccentRaw {
        self.query_raw_with(trace, raw_initialize, default_resolve)
    }

    /// One owner resolve with injected native steps (offline tests; the
    /// production [`OwnerAccentSource::query_raw`] supplies the real ones).
    /// `init` runs at most once per scope (the result is cached);
    /// `resolve` runs only after a usable init and reports the activation
    /// (merged `RoActivateInstance` plus interface-cast HRESULT), the query
    /// HRESULT, and the accent, if any. Stage order is fixed: `start`, then
    /// `init` only when `RoInitialize` actually runs, then `activate`
    /// before activation, `query` before `GetColorValue`, `done` on every
    /// return path.
    fn query_raw_with(
        &mut self,
        trace: Option<&dyn Fn(AccentStage)>,
        init: impl FnOnce() -> i32,
        resolve: impl FnOnce(&dyn Fn(AccentStage)) -> (Option<SystemAccent>, Option<i32>, Option<i32>),
    ) -> AccentRaw {
        let stage = |stage: AccentStage| {
            if let Some(trace) = trace {
                trace(stage);
            }
        };
        stage(AccentStage::Start);
        if !self.ensure_apartment(init, &stage) {
            let init_hr = self.init_hr.unwrap_or(RPC_E_CHANGED_MODE);
            stage(AccentStage::Done);
            return AccentRaw {
                init_hr,
                activate_hr: None,
                query_hr: None,
                accent: None,
            };
        }
        let init_hr = self.init_hr.unwrap_or(S_OK);
        let (accent, activate_hr, query_hr) = resolve(&stage);
        stage(AccentStage::Done);
        AccentRaw {
            init_hr,
            activate_hr,
            query_hr,
            accent,
        }
    }

    /// Shut the scope source down: balance exactly the one `RoUninitialize`
    /// a successful init owns (see [`ApartmentInit::owns_uninit`]). A failed
    /// init owns nothing and is left undisturbed. Local per-resolve
    /// `UISettings` references are already dropped, so nothing outlives the
    /// apartment. The cached init result is cleared too: a later explicit
    /// query starts a new scope and re-initializes (a torn-down apartment
    /// is never queried).
    /// Idempotent: safe to call on a never-initialized or shut-down source,
    /// and safe to repeat before `Drop` (which also shuts down).
    pub fn shutdown(&mut self) {
        let owns = self
            .init_hr
            .is_some_and(|hr| ApartmentInit::classify(hr).owns_uninit());
        if self.initialized && owns {
            (self.uninit)();
        }
        self.initialized = false;
        self.init_hr = None;
    }
}

impl Drop for OwnerAccentSource {
    /// RAII scope cleanup: an early return, an error path, or an unwind
    /// still balances the owned `RoUninitialize` exactly once.
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Default for OwnerAccentSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Real scope init: one `RoInitialize` in single-threaded mode.
/// `RO_INIT_SINGLETHREADED` is 0 in the `windows` projection; passed
/// literally so the init mode can never drift from the checked call.
fn raw_initialize() -> i32 {
    unsafe { RoInitialize(0) }
}

/// Real resolve step: activate one local `UISettings`, read the accent,
/// drop the reference before return. Emits `activate` before activation and
/// `query` before `GetColorValue`; both run only after a usable init.
fn default_resolve(
    stage: &dyn Fn(AccentStage),
) -> (Option<SystemAccent>, Option<i32>, Option<i32>) {
    stage(AccentStage::Activate);
    let settings = activate_ui_settings();
    let (settings, activate_hr) = match settings {
        Ok(settings) => (Some(settings), Some(S_OK)),
        Err(hr) => (None, Some(hr)),
    };
    let Some(settings) = settings else {
        return (None, activate_hr, None);
    };
    stage(AccentStage::Query);
    let (color, query_hr) =
        match settings.GetColorValue(windows::UI::ViewManagement::UIColorType::Accent) {
            Ok(color) => (Some(color), Some(S_OK)),
            Err(error) => (None, Some(error.code().0)),
        };
    let accent =
        color.map(|color| SystemAccent::from_winrt_color(color.A, color.R, color.G, color.B));
    drop(settings);
    (accent, activate_hr, query_hr)
}

/// Activate one local `UISettings` via `RoActivateInstance` on the runtime
/// class name, bypassing the process-static `windows-rs` `FactoryCache`.
/// The single activation HRESULT merges the `RoActivateInstance` and the
/// interface-cast (QI) failures: either step can produce it, and success
/// means both ran. Returns the raw code on failure. The returned object is
/// an uncached per-resolve reference valid only while the owner apartment
/// stays alive; the caller drops it before the scope uninitializes.
fn activate_ui_settings() -> Result<windows::UI::ViewManagement::UISettings, i32> {
    use windows::Win32::System::WinRT::RoActivateInstance;
    use windows::core::{HSTRING, Interface};
    let name = HSTRING::from("Windows.UI.ViewManagement.UISettings");
    // Safe wrapper owns the required `unsafe`: the activation contract
    // (valid class id, inspectable out-param, interface cast) is checked
    // here, and every failure maps to its raw `HRESULT` code.
    let inspect = unsafe { RoActivateInstance(&name) }.map_err(|error| error.code().0)?;
    inspect
        .cast::<windows::UI::ViewManagement::UISettings>()
        .map_err(|error| error.code().0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_source_starts_lazy_with_no_winrt() {
        // Offline fixtures construct the source but never resolve: no
        // apartment, no observed HRESULT, no WinRT call.
        let source = OwnerAccentSource::new();
        assert!(!source.is_initialized());
        let again = OwnerAccentSource::default();
        assert!(!again.is_initialized());
    }

    #[test]
    fn accent_version_is_observed_never_consumed() {
        // Two consumers observe the same bump independently: the first read
        // must not steal the notification from the second (the old
        // consume-once flag starved whichever consumer read second). Exact
        // increments are not asserted: concurrent tests may bump the shared
        // version between reads.
        let before = accent_version();
        note_accent_changed();
        let first = accent_version();
        assert_ne!(first, before, "bump must move the version");
        assert_eq!(accent_version(), first, "observation must not consume");
        note_accent_changed();
        assert_ne!(
            accent_version(),
            first,
            "second bump must move the version again"
        );
    }

    #[test]
    fn accent_stage_vocabulary_is_closed() {
        assert_eq!(crate::ffi::AccentStage::Start.name(), "start");
        assert_eq!(crate::ffi::AccentStage::Init.name(), "init");
        assert_eq!(crate::ffi::AccentStage::Activate.name(), "activate");
        assert_eq!(crate::ffi::AccentStage::Query.name(), "query");
        assert_eq!(crate::ffi::AccentStage::Done.name(), "done");
    }

    #[test]
    fn apartment_init_policy_balances_every_success() {
        // COM contract: `RoInitialize` increments the apartment init count
        // even when an apartment already exists, so `S_OK` and `S_FALSE`
        // each own exactly one matching `RoUninitialize`. Failures own
        // nothing and forbid querying.
        let ok = ApartmentInit::classify(S_OK);
        assert_eq!(ok, ApartmentInit::NewApartment);
        assert!(ok.owns_uninit());
        assert!(ok.winrt_usable());
        let already = ApartmentInit::classify(S_FALSE);
        assert_eq!(already, ApartmentInit::AlreadyInitialized);
        assert!(
            already.owns_uninit(),
            "S_FALSE still balances its own reference"
        );
        assert!(already.winrt_usable());
        // A differently-initialized (e.g. host-owned MTA) apartment:
        // untouched, unqueried.
        let changed_mode = ApartmentInit::classify(RPC_E_CHANGED_MODE);
        assert_eq!(changed_mode, ApartmentInit::ForeignOrFailed);
        assert!(!changed_mode.owns_uninit());
        assert!(!changed_mode.winrt_usable());
        let failed = ApartmentInit::classify(-1);
        assert_eq!(failed, ApartmentInit::ForeignOrFailed);
        assert!(!failed.owns_uninit());
        assert!(!failed.winrt_usable());
    }

    #[test]
    fn shutdown_without_init_is_a_no_op() {
        // Offline fixtures construct the source but never resolve: shutdown
        // must balance nothing and never touch WinRT.
        let mut source = OwnerAccentSource::new();
        source.shutdown();
        assert!(!source.is_initialized());
        source.shutdown();
        assert!(!source.is_initialized());
    }

    /// Offline cleanup accounting: counts injected cleanups without WinRT.
    /// A plain function pointer (no captures) keeps the source `!Send`.
    /// Tests using it hold the serial lock: the counter is process-wide
    /// and the harness runs tests in parallel.
    static UNINIT_CALLS: AtomicU64 = AtomicU64::new(0);
    static UNINIT_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn fake_uninit() {
        UNINIT_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    fn take_uninit_calls() -> u64 {
        UNINIT_CALLS.swap(0, Ordering::SeqCst)
    }

    fn live_accent() -> SystemAccent {
        SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        }
    }

    fn ok_resolve() -> (Option<SystemAccent>, Option<i32>, Option<i32>) {
        (Some(live_accent()), Some(S_OK), Some(S_OK))
    }

    #[test]
    fn scope_init_ok_balances_exactly_one_cleanup() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let raw = source.query_raw_with(None, || S_OK, |_| ok_resolve());
        assert!(source.is_initialized());
        assert_eq!(raw.init_hr, S_OK);
        assert_eq!(raw.accent, Some(live_accent()));
        drop(source);
        assert_eq!(take_uninit_calls(), 1, "S_OK owns one RoUninitialize");
    }

    #[test]
    fn scope_init_already_balances_exactly_one_cleanup() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let raw = source.query_raw_with(None, || S_FALSE, |_| ok_resolve());
        assert!(source.is_initialized());
        assert_eq!(raw.init_hr, S_FALSE);
        assert_eq!(raw.accent, Some(live_accent()));
        drop(source);
        assert_eq!(
            take_uninit_calls(),
            1,
            "S_FALSE incremented the count too: it owns one RoUninitialize"
        );
    }

    #[test]
    fn failed_init_queries_nothing_cleans_nothing_and_caches() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let mut inits = 0u32;
        let raw = source.query_raw_with(
            None,
            || {
                inits += 1;
                RPC_E_CHANGED_MODE
            },
            |_| panic!("failed init must never resolve"),
        );
        assert!(!source.is_initialized());
        assert_eq!(raw.init_hr, RPC_E_CHANGED_MODE);
        assert_eq!(raw.activate_hr, None);
        assert_eq!(raw.query_hr, None);
        assert_eq!(raw.accent, None);
        // The failure is cached for the scope lifetime: the next resolve
        // must not re-run `RoInitialize` (a foreign apartment cannot heal
        // mid-scope) and still resolves nothing.
        let again = source.query_raw_with(
            None,
            || panic!("no re-init after failure"),
            |_| panic!("failed init must never resolve"),
        );
        assert_eq!(inits, 1, "init runs once per scope even on failure");
        assert_eq!(again.init_hr, RPC_E_CHANGED_MODE);
        assert!(!source.is_initialized());
        drop(source);
        assert_eq!(take_uninit_calls(), 0, "failed init owns no cleanup");
    }

    #[test]
    fn repeated_resolve_initializes_once() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let mut inits = 0u32;
        for _ in 0..3 {
            let raw = source.query_raw_with(
                None,
                || {
                    inits += 1;
                    S_OK
                },
                |_| ok_resolve(),
            );
            assert_eq!(raw.accent, Some(live_accent()));
        }
        assert_eq!(inits, 1, "scope init runs once, later resolves reuse it");
        drop(source);
        assert_eq!(take_uninit_calls(), 1);
    }

    #[test]
    fn explicit_shutdown_is_idempotent() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        source.query_raw_with(None, || S_OK, |_| ok_resolve());
        source.shutdown();
        assert!(!source.is_initialized());
        source.shutdown();
        drop(source);
        assert_eq!(take_uninit_calls(), 1, "shutdown + drop balance once");
    }

    #[test]
    fn shutdown_starts_a_new_scope_on_next_query() {
        // Latent-bug regression: shutdown must clear the cached init, so a
        // later query re-initializes instead of querying a torn-down
        // apartment. Query, shutdown, query: two inits, two cleanups.
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let mut inits = 0u32;
        let raw = source.query_raw_with(
            None,
            || {
                inits += 1;
                S_OK
            },
            |_| ok_resolve(),
        );
        assert_eq!(raw.accent, Some(live_accent()));
        source.shutdown();
        assert!(!source.is_initialized());
        let again = source.query_raw_with(
            None,
            || {
                inits += 1;
                S_OK
            },
            |_| ok_resolve(),
        );
        assert_eq!(inits, 2, "post-shutdown query starts a new scope");
        assert!(source.is_initialized());
        assert_eq!(again.accent, Some(live_accent()));
        drop(source);
        assert_eq!(take_uninit_calls(), 2, "each scope balances its own init");
    }

    #[test]
    fn failed_init_retries_only_after_shutdown() {
        // Failed init never retries mid-scope, but shutdown clears the
        // cache: the next scope may try again instead of pinning the
        // failure forever.
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let mut inits = 0u32;
        let no_resolve =
            |_: &dyn Fn(AccentStage)| -> (Option<SystemAccent>, Option<i32>, Option<i32>) {
                panic!("failed init must never resolve")
            };
        let first = source.query_raw_with(
            None,
            || {
                inits += 1;
                RPC_E_CHANGED_MODE
            },
            no_resolve,
        );
        assert_eq!(first.activate_hr, None);
        let second = source.query_raw_with(
            None,
            || {
                inits += 1;
                RPC_E_CHANGED_MODE
            },
            no_resolve,
        );
        assert_eq!(second.activate_hr, None);
        assert_eq!(inits, 1, "no retry while the failed scope lives");
        source.shutdown();
        let third = source.query_raw_with(
            None,
            || {
                inits += 1;
                RPC_E_CHANGED_MODE
            },
            no_resolve,
        );
        assert_eq!(third.activate_hr, None);
        assert_eq!(inits, 2, "new scope after shutdown may retry");
        drop(source);
        assert_eq!(take_uninit_calls(), 0, "failed inits own no cleanup");
    }
    #[test]
    fn unwind_after_init_still_balances_cleanup() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
            source.query_raw_with(None, || S_FALSE, |_| ok_resolve());
            panic!("simulated abrupt exit with a live apartment");
        }));
        assert!(result.is_err());
        assert_eq!(take_uninit_calls(), 1, "Drop balances the unwind path");
    }

    #[test]
    fn drop_without_init_balances_nothing() {
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        drop(OwnerAccentSource::new_with_uninit(fake_uninit));
        assert_eq!(take_uninit_calls(), 0);
    }

    #[test]
    fn real_source_through_real_cache_across_versions() {
        // Combined offline integration: the real source lifecycle seam
        // (`query_raw_with`) drives the real shared `AccentCache` across
        // two broadcast versions with injected native steps (no WinRT).
        // Init runs once, the same version reuses without any source call,
        // and drop balances exactly one cleanup.
        use crate::active_border::AccentCache;
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut shared = AccentCache::default();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let mut inits = 0u32;
        let first = shared
            .refresh_detailed(7, || {
                source.query_raw_with(
                    None,
                    || {
                        inits += 1;
                        S_OK
                    },
                    |_| ok_resolve(),
                )
            })
            .expect("startup resolves");
        assert_eq!(first.accent, Some(live_accent()));
        assert_eq!(inits, 1);
        assert_eq!(shared.get(), Some(live_accent()));
        // Same version: cache hit, the source is not even entered.
        let hit = shared.refresh_detailed(7, || panic!("steady version must not reach the source"));
        assert_eq!(hit, None);
        assert_eq!(inits, 1);
        assert_eq!(shared.get(), Some(live_accent()));
        // Next version: re-resolves through the same live scope, no re-init.
        let second = shared
            .refresh_detailed(8, || {
                source.query_raw_with(
                    None,
                    || {
                        inits += 1;
                        S_OK
                    },
                    |_| ok_resolve(),
                )
            })
            .expect("broadcast re-resolves");
        assert_eq!(second.accent, Some(live_accent()));
        assert_eq!(inits, 1, "one scope init across versions");
        drop(source);
        assert_eq!(take_uninit_calls(), 1, "one scope cleanup");
    }

    #[test]
    fn error_init_through_cache_skips_activation_and_query() {
        // Error-init integration: the failed resolve still records through
        // the real cache, but activation and query never run (their report
        // fields stay skipped) and no cleanup is owned.
        use crate::active_border::AccentCache;
        let _serial = UNINIT_SERIAL.lock().expect("uninit serial");
        take_uninit_calls();
        let mut shared = AccentCache::default();
        let mut source = OwnerAccentSource::new_with_uninit(fake_uninit);
        let raw = shared
            .refresh_detailed(3, || {
                source.query_raw_with(
                    None,
                    || RPC_E_CHANGED_MODE,
                    |_| panic!("failed init must never resolve"),
                )
            })
            .expect("failed resolve still records");
        assert_eq!(raw.init_hr, RPC_E_CHANGED_MODE);
        assert_eq!(raw.activate_hr, None, "activation skipped");
        assert_eq!(raw.query_hr, None, "query skipped");
        assert_eq!(raw.accent, None);
        assert_eq!(shared.get(), None);
        drop(source);
        assert_eq!(take_uninit_calls(), 0, "failed init owns no cleanup");
    }
}
