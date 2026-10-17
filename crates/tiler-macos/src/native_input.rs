#![forbid(unsafe_code)]

//! Scoped-tile native input: exact-chord catalogue plus owned sources.
//!
//! The catalogue mirrors the Windows `FOCUS_MOVE_ROWS` + `RESIZE_ROWS` rows
//! (read-only reference: letters plus separate arrow rows, Shift selecting
//! the move arm, Alt selecting the resize arm) with the Ctrl role filled by
//! the configured [`ProbeModifier`] (default Ctrl; pass `--ctrl meta` for
//! the Cmd chord space), the float toggle on `G` (Windows `Win+G` / KDE
//! `Meta+G` parity), and the parent orientation toggle on `O` (Windows
//! `Win+O` / KDE `Meta+O` parity):
//!
//! - focus: `<Mod>+H/J/K/L` and `<Mod>+arrows` (8 chords);
//! - move: `<Mod>+Shift+H/J/K/L` and `<Mod>+Shift+arrows` (8 chords);
//! - float: `<Mod>+G` (1 chord);
//! - grow: `<Mod>+Alt+H/J/K/L` and `<Mod>+Alt+arrows` (8 chords, outwards);
//! - shrink: `<Mod>+Alt+Shift+H/J/K/L` and `<Mod>+Alt+Shift+arrows`
//!   (8 chords, inwards);
//! - orientation: `<Mod>+O` (1 chord).
//!
//! Matching is exact everywhere (extra modifiers fail; Caps/Fn bits sit
//! outside the mask per the existing matcher contract). Key codes are Mac
//! virtual keycodes (`CGKeyCode` space).
//!
//! This catalogue is macOS-local glue over the shared core commands (it
//! mirrors the Windows/KDE defaults chord-for-chord but lives here because
//! no shared pure catalogue exists in `tiler-core` — the Windows
//! `binding_catalog` and the KDE `planShortcutCatalog` are each local too).
//! Digits and workspace selects/sends/maximize, fullscreen, sticky, and
//! workspace-tiling toggles stay out of THIS catalogue: numbered and
//! auxiliary workspace rows live in [`crate::workspace_input`] (defined and
//! exactly matchable, but with no product execution yet, so never armed
//! here), and maximize/fullscreen/sticky need native
//! state writes with no core command behind them (left for a native-state
//! unit; fullscreen windows stay untouched by ordinary tiling per the
//! selected product rule).
//!
//! Sources are continuous and owned: the input owner creates, drives, and
//! destroys its guard on its own thread; callbacks stay minimal
//! (exact-chord compare, non-blocking forward of exact matches only,
//! atomics) and never touch the core, AX, logging, or blocking calls. The
//! tap is the sole input route; off parks. No raw input is logged anywhere
//! (counters only).
//!
//! Scoped eligibility (see [`crate::input_eligibility`]) gates AFTER
//! suppression (consume-first, Windows parity): the owner attests fresh
//! scope/foreground/Space/showstate facts outside the callback and publishes
//! to lock-free caches; the tap callback always consumes owned exact chords
//! while interception is held, and the input owner refuses each consumed hit
//! owner-side without effects while its own category reads ineligible.
//! Fullscreen/game foregrounds consume-then-refuse instead of passing
//! through natively. Only the global lock / Secure Input / unknown-session
//! release passes events untouched from inside the callback. The tap owner
//! recreates its guard on return from a global release to avoid a
//! user-disabled sticky cycle. Scoped sources start ineligible until attested
//! (hits refuse until then); the `input-session` fixture uses an explicit
//! unrestricted test gate for its known post targets (strict per-edge oracle
//! still enforced).

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc,
};
use std::time::Instant;

use crate::config::ProbeModifier;
use crate::dev_selection::TileScope;
use crate::ffi::{TapChordSpec, TapHit, TapPump, TapSource};
use crate::input::cg_flag_bits;
use crate::input_eligibility::{EligibilityCache, EligibilityFactsStore};
use crate::input_match::TapKeyKind;
use crate::input_policy::{
    DisableAction, DisableKind, disable_action, unnotified_disable_action,
    unverified_reenable_action,
};
/// Mac virtual keycodes live in [`crate::input_policy`] (single home for the
/// product input-policy slice); re-exported here so the catalogue, matcher
/// tables and existing tests keep resolving through this module.
pub use crate::input_policy::{
    KEY_DOWN, KEY_G, KEY_H, KEY_J, KEY_K, KEY_L, KEY_LEFT, KEY_O, KEY_RIGHT, KEY_UP,
};
use crate::runtime::{InputDirection, InputSemanticAction};
use crate::runtime_backend::{InputSource, InputSourceFactory, RawKeyEvent};
use crate::runtime_types::TileBinding;

/// `CGEventFlags` Shift bit (matches [`cg_flag_bits`] mask space).
pub const CG_SHIFT_BIT: u64 = 0x0002_0000;
/// `CGEventFlags` Alternate bit (matches [`cg_flag_bits`] mask space).
pub const CG_ALT_BIT: u64 = 0x0008_0000;

/// The scoped-tile chord catalogue: 8 focus chords, 8 move
/// chords (Shift arm), 8 grow chords (Alt arm, outwards), 8 shrink chords
/// (Alt+Shift arm, inwards), the float toggle, and the orientation toggle.
/// The Ctrl-role modifier applies at tuning time ([`tap_specs_for`]), so the table itself is modifier-free.
/// Duplicate-free by construction; validated by
/// [`RuntimeConfig::validate`]. No row carries the auxiliary workspace arm
/// (workspace rows live in [`crate::workspace_input`]).
#[must_use]
pub fn tile_catalogue() -> Vec<TileBinding> {
    let mut out = Vec::with_capacity(34);
    let focus = [
        (KEY_H, InputDirection::Left),
        (KEY_J, InputDirection::Down),
        (KEY_K, InputDirection::Up),
        (KEY_L, InputDirection::Right),
        (KEY_LEFT, InputDirection::Left),
        (KEY_DOWN, InputDirection::Down),
        (KEY_UP, InputDirection::Up),
        (KEY_RIGHT, InputDirection::Right),
    ];
    for (key_code, direction) in focus {
        out.push(TileBinding {
            key_code,
            shift: false,
            alt: false,
            aux: false,
            action: InputSemanticAction::FocusStep { direction },
        });
    }
    for (key_code, direction) in focus {
        out.push(TileBinding {
            key_code,
            shift: true,
            alt: false,
            aux: false,
            action: InputSemanticAction::MoveStep { direction },
        });
    }
    for (key_code, direction) in focus {
        out.push(TileBinding {
            key_code,
            shift: false,
            alt: true,
            aux: false,
            action: InputSemanticAction::ResizeStep {
                direction,
                mode: tiler_core::contract::ResizeMode::Outwards,
            },
        });
    }
    for (key_code, direction) in focus {
        out.push(TileBinding {
            key_code,
            shift: true,
            alt: true,
            aux: false,
            action: InputSemanticAction::ResizeStep {
                direction,
                mode: tiler_core::contract::ResizeMode::Inwards,
            },
        });
    }
    out.push(TileBinding {
        key_code: KEY_G,
        shift: false,
        alt: false,
        aux: false,
        action: InputSemanticAction::ToggleFloat,
    });
    out.push(TileBinding {
        key_code: KEY_O,
        shift: false,
        alt: false,
        aux: false,
        action: InputSemanticAction::ToggleOrientation,
    });
    out
}

/// Tap specs for one catalogue: exact `(want, mask)` per chord with the
/// Shift, Alt, and auxiliary workspace arms folded into `want` under the
/// same mask. `select` marks armed workspace-select chords so the tap
/// callback gates them on the select eligibility cache instead of the
/// basic one (single home with [`crate::workspace_input::is_armed_select`]).
#[must_use]
pub fn tap_specs_for(modifier: ProbeModifier, bindings: &[TileBinding]) -> Vec<TapChordSpec> {
    bindings
        .iter()
        .map(|binding| {
            let (mut want_flags, flags_mask) = cg_flag_bits(modifier);
            if binding.shift {
                want_flags |= CG_SHIFT_BIT;
            }
            if binding.alt {
                want_flags |= CG_ALT_BIT;
            }
            if binding.aux {
                want_flags |= crate::input_policy::aux_flag_for(modifier);
            }
            TapChordSpec {
                want_flags,
                flags_mask,
                want_code: i64::from(binding.key_code),
                key_code: binding.key_code,
                select: crate::workspace_input::is_armed_select(&binding.action),
            }
        })
        .collect()
}

/// Pure interception gate: `Some(reason)` while interception must be
/// released, `None` in the ordinary unlocked state. The actual session lock
/// verdict (login-done plus on-console legs, never just the loginwindow
/// heuristic) gates first and unknown refuses; Secure Input `Active` gates;
/// the lock screen (`loginwindow` frontmost) and an unreadable frontmost
/// gate; `Unknown` Secure Input proceeds per the existing contract. Static
/// tokens only — never titles, keys, or content.
#[must_use]
pub fn interception_gate(
    secure_active: Option<bool>,
    frontmost: Option<(i32, String)>,
    session: crate::ffi::SessionLock,
) -> Option<&'static str> {
    match session {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => return Some("session-locked"),
        crate::ffi::SessionLock::Unknown => return Some("session-unknown"),
    }
    if secure_active == Some(true) {
        return Some("secure-input-active");
    }
    match frontmost {
        None => Some("session-unknown"),
        Some((_, bundle)) if crate::focus::is_loginwindow_bundle(&bundle) => Some("lock-screen"),
        Some((pid, _)) if pid <= 1 => Some("session-unknown"),
        Some(_) => None,
    }
}

/// Live interception gate over current OS state (the tap owner releases
/// interception while gated). Unknown Secure Input proceeds; anything gated
/// holds until the ordinary unlocked state returns.
///
/// Threading: the session-dict read is a plain thread-safe C call and the
/// frontmost read is the justified off-main model query (see
/// [`crate::ffi::frontmost_app`]).
fn live_gate() -> Option<&'static str> {
    interception_gate(
        crate::ffi::secure_input_known(),
        crate::ffi::frontmost_app(),
        crate::ffi::session_state().lock,
    )
}

/// Refuse input-source creation outside the ordinary unlocked desktop.
/// Checked BEFORE any tap install (never intercept
/// while locked, unknown, Secure Input active, or frontmost unreadable):
/// an unknown session refuses rather than arming blind. Runs on the input
/// owner thread; reads are the thread-safe session C call, Secure Input
/// state, and the justified off-main frontmost query (see
/// [`crate::ffi::frontmost_app`]).
fn creation_gate() -> Result<(), String> {
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err("refusing input source: session locked".to_owned());
        }
        crate::ffi::SessionLock::Unknown => {
            return Err("refusing input source: session unknown".to_owned());
        }
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err("refusing input source: Secure Input active".to_owned());
    }
    match crate::ffi::frontmost_app() {
        None => Err("refusing input source: frontmost unreadable".to_owned()),
        Some((_, bundle)) if crate::focus::is_loginwindow_bundle(&bundle) => {
            Err("refusing input source: lock screen".to_owned())
        }
        Some((pid, _)) if pid <= 1 => Err("refusing input source: frontmost unreadable".to_owned()),
        Some(_) => Ok(()),
    }
}

/// Attest both scoped input eligibilities from cheap live reads OUTSIDE
/// the callback and publish each to its own cache: the focused-window
/// verdict to `basic_cache` (basic chords) and the no-focus select verdict
/// to `select_cache` (armed select chords on empty Spaces). Returns true
/// when EITHER attests; the caller never parks on false — ineligible
/// foregrounds consume, and each consumed hit refuses owner-side
/// (`consumed_refusal`) without effects. Unsafe contexts deny both. No AX,
/// no SLS, no logging anywhere on this path.
pub fn attest_tile_eligibility(
    scope: &TileScope,
    basic_cache: &EligibilityCache,
    select_cache: &EligibilityCache,
    store: Option<&Arc<EligibilityFactsStore>>,
    allow_fullscreen: bool,
    last_attest: &mut Option<Instant>,
) -> bool {
    use crate::input_eligibility::{attest_verdict, select_attest_verdict};
    if basic_cache.is_unrestricted() {
        return basic_cache.is_eligible();
    }
    if let Some(last) = *last_attest
        && last.elapsed() < crate::input_eligibility::ATTEST_CADENCE
    {
        return basic_cache.is_eligible() || select_cache.is_eligible();
    }
    *last_attest = Some(Instant::now());
    let now = crate::input_eligibility::now_ms();
    let snapshot = match attest_live(scope, store, allow_fullscreen, now) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            // Unreadable facts deny both caches (unknown/stale denies).
            let pid = crate::ffi::frontmost_app()
                .map(|(pid, _)| if pid > 1 { pid as u32 } else { 0 })
                .unwrap_or(0);
            basic_cache.publish(false, pid);
            select_cache.publish(false, pid);
            return false;
        }
    };
    // Owner PID for the caches: the live frontmost when readable, else 0
    // (unknown denies downstream anyway).
    let pid = crate::ffi::frontmost_app()
        .map(|(pid, _)| if pid > 1 { pid as u32 } else { 0 })
        .unwrap_or(0);
    let basic = attest_verdict(&snapshot).is_ok();
    let select = select_attest_verdict(&snapshot).is_ok();
    basic_cache.publish(basic, pid);
    select_cache.publish(select, pid);
    basic || select
}

/// One cheap live attest snapshot (owner thread only, never the callback):
/// session, Secure Input, workspace frontmost, SLPS front, exact scope,
/// plus one plain store read. No verdict here — the caller applies the
/// focused-window verdict to the basic cache and the no-focus select
/// verdict to the select cache. No AX, no SLS, no logging anywhere on
/// this path.
fn attest_live(
    scope: &TileScope,
    store: Option<&Arc<EligibilityFactsStore>>,
    allow_fullscreen: bool,
    now_ms: u64,
) -> Result<crate::input_eligibility::AttestSnapshot, String> {
    use crate::input_eligibility::AttestSnapshot;
    let session_unlocked = matches!(
        crate::ffi::session_state().lock,
        crate::ffi::SessionLock::Unlocked
    );
    let secure_active = crate::ffi::secure_input_known();
    let frontmost = crate::ffi::frontmost_app();
    let (frontmost_pid, frontmost_bundle) = match frontmost {
        Some((pid, bundle)) if pid > 1 => (Some(pid as u32), Some(bundle)),
        _ => (None, None),
    };
    let slps_pid = crate::ffi::slps_front_pid()
        .ok()
        .and_then(|p| u32::try_from(p).ok());
    let workspace_pid = frontmost_pid;
    let lock_screen = frontmost_bundle
        .as_deref()
        .is_some_and(crate::focus::is_loginwindow_bundle);
    let scope_allowed = match frontmost_bundle.as_deref() {
        Some(bundle) => scope.allows_effect(Some(bundle)),
        None => false,
    };
    let store = store.ok_or_else(|| "facts-unavailable".to_owned())?;
    let (context_currents, context_revision) = store.context();
    let pid = frontmost_pid.ok_or_else(|| "frontmost-unknown".to_owned())?;
    let record = store.snapshot_for(pid);
    let record_updated_ms = record.as_ref().map(|r| r.updated_ms).unwrap_or(0);
    Ok(AttestSnapshot {
        session_unlocked,
        secure_active,
        lock_screen,
        frontmost_pid,
        frontmost_bundle,
        slps_pid,
        workspace_pid,
        scope_allowed,
        record,
        record_updated_ms,
        context_currents,
        context_revision,
        allow_fullscreen,
        now_ms,
    })
}

// -- Tap owned source --------------------------------------------------------

/// Continuous tap input factory over the tile catalogue (explicit opt-in
/// only). Exact-chord KeyDown/KeyUp pairs suppress; everything unmatched
/// passes. Secure Input / lock gating releases interception while gated
/// and resumes narrowly when the ordinary unlocked state returns; every
/// observed disable (timeout notification, user-input notification, or an
/// enabled readback that disagrees with a Held pump turn) is a logged
/// recoverable state — the same tap re-enables when it verifies, otherwise
/// a fresh tap installs over the same chords — and only a hard creation
/// denial refuses. Scoped eligibility gates AFTER suppression per chord
/// category (consume-first: owned chords always consume while held; the
/// owner refuses each consumed hit without effects while its own category
/// cache reads ineligible — basic and select alike); the owner recreates
/// its guard on return from a global release to avoid a user-disabled
/// sticky cycle.
/// Tile drag-preview Escape watch wiring: route-armed flag, coalesced
/// pending flag (never lost), unsuppressed forward channel, and the
/// full-queue drop counter. `None` disables (every non-tile caller).
type DragEscapeWiring = Option<(
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    mpsc::SyncSender<TapHit>,
    Arc<AtomicU32>,
)>;

pub struct TapTileFactory {
    modifier: ProbeModifier,
    bindings: Vec<TileBinding>,
    refusal: Arc<Mutex<Option<String>>>,
    gated_ms: Arc<AtomicU64>,
    /// Fixture-only one-shot callback stall (milliseconds, 0 = off, max
    /// 2000): provokes a REAL OS `TapDisabledByTimeout` on the first
    /// exact-match KeyDown. Set only by [`TapTileFactory::with_stall_once`],
    /// which only the `input-session --stall-once-ms` leg uses; the tile
    /// controller never sets it.
    stall_once_ms: u64,
    /// Latest tap counters published by the live source (per pump slice
    /// plus a final write on drop): matched/suppressed/deliveries plus
    /// timeout re-enables, user-input notifications, and
    /// enabled-readback-only recoveries, all carried across fresh-guard
    /// recreations. Read after the owner joins for terminal evidence,
    /// or mid-run for the fault-injection leg.
    report: Arc<Mutex<crate::ffi::TapSourceReport>>,
    /// Fixture-only DIRECT disable hold (false = off): while set, the input
    /// owner parks the tap directly OS-disabled (`CGEventTapEnable(false)`)
    /// instead of pumping, so exact chords pass through. Set only by the
    /// `input-session --direct-disable-hold-ms` leg via
    /// [`TapTileFactory::set_direct_hold`]; the tile controller never sets
    /// it (production behavior unchanged: the flag stays false).
    direct_hold: Arc<AtomicBool>,
    /// Latest actual OS tap-state readback (`CGEventTapIsEnabled`, published
    /// by the owner thread): 0 = directly disabled, 1 = enabled,
    /// 2 = unknown (no reading yet). Read-only; the DIRECT leg gates on
    /// this, never on a callback atomic alone.
    direct_enabled: Arc<AtomicU64>,
    /// Fixture-only EXPLICIT re-arm request (false = idle): while set, the
    /// input owner drops its old tap guard WITHOUT a same-tap re-enable and
    /// installs a FRESH `TapSource` over the same chords and tap placement
    /// on the same owner thread, then clears the hold and bumps
    /// `rearm_epoch`. Set only by the `input-session --direct-rearm` leg via
    /// [`TapTileFactory::request_rearm`]; the tile controller never sets it
    /// (production behavior unchanged: the flag stays false).
    rearm_requested: Arc<AtomicBool>,
    /// Fixture-only one-shot disable-once fault (false = idle): while set,
    /// the input owner disables the tap directly ONCE
    /// (`CGEventTapEnable(false)`) WITHOUT setting the DIRECT hold and
    /// WITHOUT assuming which notification the OS emits for a self-disable
    /// (measured live: a UserInput notification with a verified same-tap
    /// re-enable) — then keeps pumping so the tap recovers automatically and
    /// the leg reports the ACTUAL kind (user-input vs unnotified
    /// enabled-readback counters, never a timeout claim without the actual
    /// counter). Set only by the `input-session --disable-once` leg via
    /// [`TapTileFactory::request_disable_once`]; the tile controller never
    /// sets it (production behavior unchanged: the flag stays false).
    disable_once_fault: Arc<AtomicBool>,
    /// Fresh-source epoch: bumped once per completed EXPLICIT re-arm on the
    /// owner thread. The `--direct-rearm` leg gates its new-generation proof
    /// on this increment plus a fresh `==true` state readback.
    rearm_epoch: Arc<AtomicU64>,
    /// Product selection for eligibility (scoped allowlist or unscoped
    /// ordinary apps; disabled = never eligible until set).
    scope: TileScope,
    /// Lock-free cached basic eligibility (starts ineligible until attested;
    /// fixture unrestricted gate starts eligible with per-edge oracle).
    eligibility: Arc<EligibilityCache>,
    /// Lock-free cached select eligibility for armed workspace-select
    /// chords (starts ineligible until attested; the tap callback gates
    /// each chord on exactly its own category).
    eligibility_select: Arc<EligibilityCache>,
    /// Shared per-PID publisher facts (each per-app AX owner publishes only
    /// its own PID from already-read snapshots; `None` until the tile wires
    /// it — unknown reads deny). No AX crosses into the input owner through
    /// this handle: plain cloned records plus a brief `Mutex`, consulted
    /// outside the callback only.
    facts: Option<Arc<EligibilityFactsStore>>,
    /// Tile drag-preview Escape watch (`None` = unset: every non-drag
    /// caller): while the flag reads true, an otherwise-unmatched Escape
    /// KeyDown forwards unsuppressed on the escape channel so the client
    /// still receives the key and the main-thread drag session observes
    /// the cancel edge. Applied to every tap guard this factory installs
    /// (initial plus fresh-guard recreations). Set once via
    /// [`TapTileFactory::with_drag_escape_watch`]; the tile drag route is
    /// the only production caller.
    drag_escape: DragEscapeWiring,
}

impl TapTileFactory {
    pub fn new(modifier: ProbeModifier, bindings: Vec<TileBinding>) -> Result<Self, String> {
        if bindings.is_empty() {
            return Err("tap tile input needs a non-empty chord table".to_owned());
        }
        for binding in &bindings {
            binding.validate()?;
        }
        Ok(Self {
            modifier,
            bindings,
            refusal: Arc::new(Mutex::new(None)),
            gated_ms: Arc::new(AtomicU64::new(0)),
            stall_once_ms: 0,
            report: Arc::new(Mutex::new(crate::ffi::TapSourceReport {
                matched: 0,
                suppressed: 0,
                deliveries: 0,
                reenables_after_timeout: 0,
                user_disables: 0,
                unnotified_disables: 0,
            })),
            direct_hold: Arc::new(AtomicBool::new(false)),
            direct_enabled: Arc::new(AtomicU64::new(2)),
            rearm_requested: Arc::new(AtomicBool::new(false)),
            rearm_epoch: Arc::new(AtomicU64::new(0)),
            disable_once_fault: Arc::new(AtomicBool::new(false)),
            scope: TileScope::scoped_disabled(),
            eligibility: Arc::new(EligibilityCache::new_scoped()),
            eligibility_select: Arc::new(EligibilityCache::new_scoped()),
            facts: None,
            drag_escape: None,
        })
    }

    /// Arm the tile drag-preview Escape watch: while `active` reads true,
    /// an otherwise-unmatched Escape KeyDown forwards unsuppressed on
    /// `escape_tx`. The tile drag route is the only production caller (set
    /// before `Runtime::start`; applied to every guard this factory
    /// installs, including fresh-guard recreations).
    #[must_use]
    pub fn with_drag_escape_watch(
        mut self,
        active: Arc<AtomicBool>,
        escape_pending: Arc<AtomicBool>,
        escape_tx: mpsc::SyncSender<TapHit>,
        escape_dropped: Arc<AtomicU32>,
    ) -> Self {
        self.drag_escape = Some((active, escape_pending, escape_tx, escape_dropped));
        self
    }

    /// Apply the drag Escape watch to a freshly installed tap guard
    /// (initial plus every fresh-guard recreation); `None` is a no-op.
    fn arm_drag_escape(&self, tap: &TapSource) {
        if let Some((active, pending, tx, dropped)) = &self.drag_escape {
            tap.set_drag_escape_watch(
                Arc::clone(active),
                Arc::clone(pending),
                tx.clone(),
                Arc::clone(dropped),
            );
        }
    }

    /// Set the product selection for eligibility (normal tile path;
    /// ineligible until attested facts).
    #[must_use]
    pub fn with_scope(mut self, scope: TileScope) -> Self {
        self.scope = scope;
        self
    }

    /// Share the per-PID publisher facts store (tile wires this before
    /// `Runtime::start`; `None` (default) denies scoped attests until wired).
    #[must_use]
    pub fn with_facts_store(mut self, store: Arc<EligibilityFactsStore>) -> Self {
        self.facts = Some(store);
        self
    }

    /// Fixture-only explicit unrestricted test gate (input-session known
    /// post targets with strict per-edge oracle). Normal tile must never
    /// call this.
    #[must_use]
    pub fn with_unrestricted_test_gate(mut self) -> Self {
        self.eligibility = Arc::new(EligibilityCache::new_unrestricted_test_gate());
        self.eligibility_select = Arc::new(EligibilityCache::new_unrestricted_test_gate());
        self
    }

    /// Lock-free cached basic eligibility (owner-published, callback-read).
    #[must_use]
    pub fn eligibility(&self) -> Arc<EligibilityCache> {
        Arc::clone(&self.eligibility)
    }

    /// Lock-free cached select eligibility (owner-published, callback-read
    /// for armed workspace-select chords only).
    #[must_use]
    pub fn eligibility_select(&self) -> Arc<EligibilityCache> {
        Arc::clone(&self.eligibility_select)
    }

    /// Opt in to the fixture-only one-shot callback stall (see field docs).
    /// Builder; `input-session --stall-once-ms` only, never production tile.
    pub fn with_stall_once(mut self, stall_ms: u64) -> Self {
        self.stall_once_ms = stall_ms;
        self
    }

    /// Probe-only stall seam state: true only after [`TapTileFactory::with_stall_once`].
    /// The product tile construction never calls that builder, so a
    /// production factory always reads false here (pinned by test).
    /// Test-only: production code paths never branch on the stall seam.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn stall_armed(&self) -> bool {
        self.stall_once_ms > 0
    }

    /// Fixture-only DIRECT disable hold control (see field docs): `true`
    /// parks the tap directly OS-disabled on the owner thread, `false`
    /// resumes the standard source pump with a single direct re-enable on
    /// the hold-to-release transition. `input-session`
    /// `--direct-disable-hold-ms` only; never production tile.
    pub fn set_direct_hold(&self, hold: bool) {
        self.direct_hold.store(hold, Ordering::SeqCst);
    }

    /// Fixture-only EXPLICIT re-arm request (see field docs): the owner drops
    /// its old tap guard without a same-tap re-enable and installs a FRESH
    /// source on the same thread, then clears the hold and bumps the epoch.
    /// `input-session --direct-rearm` only; never production tile. A hard
    /// creation-denial refusal is never masked: the owner records it and the
    /// leg fails closed.
    pub fn request_rearm(&self) {
        self.rearm_requested.store(true, Ordering::SeqCst);
    }

    /// Fixture-only one-shot disable-once fault (see field docs): the owner
    /// disables the tap directly once without a fixture hold and without
    /// assuming the OS notification kind, so the leg reports the actual
    /// recovery kind. `input-session --disable-once` only; never production
    /// tile.
    pub fn request_disable_once(&self) {
        self.disable_once_fault.store(true, Ordering::SeqCst);
    }

    /// Fresh-source epoch (see field docs): 0 until the first completed
    /// EXPLICIT re-arm on the owner thread.
    #[must_use]
    pub fn rearm_epoch(&self) -> u64 {
        self.rearm_epoch.load(Ordering::SeqCst)
    }

    /// Latest actual OS tap-state readback (`CGEventTapIsEnabled` as
    /// published by the owner thread): `Some(true)` enabled, `Some(false)`
    /// directly disabled, `None` no reading yet. The DIRECT leg gates on
    /// this.
    #[must_use]
    pub fn latest_direct_enabled(&self) -> Option<bool> {
        match self.direct_enabled.load(Ordering::SeqCst) {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    /// Truthful refusal recorded by the owner (user disable); `None` means
    /// no refusal.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        self.refusal
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Milliseconds the owner spent gated (interception released).
    #[must_use]
    pub fn gated_ms(&self) -> u64 {
        self.gated_ms.load(Ordering::SeqCst)
    }

    /// Latest tap counters published by the live source (see field docs).
    #[must_use]
    pub fn latest_report(&self) -> crate::ffi::TapSourceReport {
        *self
            .report
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl InputSourceFactory for TapTileFactory {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        // Gate BEFORE the tap install: never intercept while locked, unknown,
        // Secure Input active, or frontmost unreadable.
        creation_gate()?;
        let specs = tap_specs_for(self.modifier, &self.bindings);
        let (tx, rx) = mpsc::sync_channel::<TapHit>(64);
        let tap = if self.stall_once_ms > 0 {
            // Fixture-only stall leg (input-session, unrestricted gate):
            // no scoped eligibility on the stalled guard itself — the leg's
            // per-edge oracle plus the stall/timeout proof carry it.
            TapSource::create_stall_once(&specs, true, &tx, self.stall_once_ms)
                .map_err(|e| e.to_string())?
        } else {
            TapSource::create_scoped(&specs, true, &tx).map_err(|e| e.to_string())?
        };
        self.arm_drag_escape(&tap);
        Ok(Box::new(TapTileSource {
            tap,
            rx,
            specs,
            refusal: Arc::clone(&self.refusal),
            gated_ms: Arc::clone(&self.gated_ms),
            report: Arc::clone(&self.report),
            carried: crate::ffi::TapSourceReport {
                matched: 0,
                suppressed: 0,
                deliveries: 0,
                reenables_after_timeout: 0,
                user_disables: 0,
                unnotified_disables: 0,
            },
            direct_hold: Arc::clone(&self.direct_hold),
            direct_enabled: Arc::clone(&self.direct_enabled),
            rearm_requested: Arc::clone(&self.rearm_requested),
            rearm_epoch: Arc::clone(&self.rearm_epoch),
            disable_once_fault: Arc::clone(&self.disable_once_fault),
            holding_direct: false,
            scope: self.scope.clone(),
            eligibility: Arc::clone(&self.eligibility),
            eligibility_select: Arc::clone(&self.eligibility_select),
            facts: self.facts.clone(),
            last_attest: None,
            was_ineligible: false,
            consumed_refused_basic: 0,
            consumed_refused_select: 0,
            drag_escape: self.drag_escape.clone(),
        }))
    }

    fn describe(&self) -> &'static str {
        "tile-tap"
    }
}

struct TapTileSource {
    tap: TapSource,
    rx: mpsc::Receiver<TapHit>,
    /// Same-chord specs for an EXPLICIT fresh re-arm (same chords and tap
    /// placement, new guard and new epoch). Cloned at creation; never mutated.
    specs: Vec<TapChordSpec>,
    refusal: Arc<Mutex<Option<String>>>,
    gated_ms: Arc<AtomicU64>,
    report: Arc<Mutex<crate::ffi::TapSourceReport>>,
    /// Cumulative counters from retired guards: each fresh-guard install
    /// folds the old guard's final snapshot in here, so every published
    /// report is cumulative across recreations (kinds stay distinct — a
    /// readback-only recovery never lands in the timeout count). The final
    /// outcome after the latest stop rides the same slot.
    carried: crate::ffi::TapSourceReport,
    direct_hold: Arc<AtomicBool>,
    direct_enabled: Arc<AtomicU64>,
    rearm_requested: Arc<AtomicBool>,
    rearm_epoch: Arc<AtomicU64>,
    disable_once_fault: Arc<AtomicBool>,
    holding_direct: bool,
    scope: TileScope,
    eligibility: Arc<EligibilityCache>,
    eligibility_select: Arc<EligibilityCache>,
    facts: Option<Arc<EligibilityFactsStore>>,
    last_attest: Option<Instant>,
    /// True while the last loop observed the global release gate (lock
    /// screen, Secure Input, unknown session) with interception released.
    /// On return to the ordinary unlocked state the owner drops the old
    /// guard and installs a FRESH scoped source on this same thread (new
    /// epoch) to avoid a disabled sticky cycle; a recorded hard refusal
    /// still fails closed and is never auto-overridden. Scoped
    /// ineligibility never sets this: ineligible foregrounds consume, and
    /// each consumed hit refuses owner-side below without effects.
    was_ineligible: bool,
    /// Consumed-then-refused KeyDown hits per category (owner-side, after
    /// suppression): bounded evidence that ineligible foregrounds consume
    /// without effects. Rate-limited logging in `note_consumed_refusal`
    /// (static tokens and counts only — never keys, flags, or content).
    consumed_refused_basic: u64,
    consumed_refused_select: u64,
    /// Tile drag-preview Escape watch (cloned from the factory at
    /// creation; applied to every guard this owner installs, including
    /// fresh-guard recreations). `None` = unset.
    drag_escape: DragEscapeWiring,
}

/// Match one consumed hit back to its armed spec category (pure, no OS
/// touch): `Some(select)` for an exact-chord hit, `None` for an event the
/// callback should never have forwarded (drift: the caller forwards, never
/// drops — the input owner re-matches exactly and ignores it, so an
/// unowned key can never be swallowed here). Key code plus exact masked
/// flags identify the chord, mirroring the callback's match loop (spec
/// `want_code` equals the spec key code by construction).
#[must_use]
pub(crate) fn consumed_hit_category(
    specs: &[crate::ffi::TapChordSpec],
    key_code: u32,
    flags: u64,
) -> Option<bool> {
    specs.iter().find_map(|spec| {
        if spec.key_code == key_code && (flags & spec.flags_mask) == spec.want_flags {
            Some(spec.select)
        } else {
            None
        }
    })
}

/// Pure drain decision for one consumed hit (no OS touch): exactly what
/// `TapTileSource::next` applies after the pump. KeyUp hits always forward
/// (they release held presses downstream — dropping them would wedge the
/// orientation hold); unowned events forward, never drop (the input owner
/// re-matches exactly and ignores them, so nothing unowned is swallowed
/// here); KeyDown hits dispatch or refuse per category
/// (`consumed-refused-basic` / `consumed-refused-select`). Returns the
/// static outcome token; `dispatch` (plus both forwards) means the hit
/// becomes a `RawKeyEvent`. Tokens only, never content.
#[must_use]
pub(crate) fn triage_consumed_hit(
    specs: &[crate::ffi::TapChordSpec],
    key_code: u32,
    flags: u64,
    down: bool,
    basic_eligible: bool,
    select_eligible: Option<bool>,
) -> &'static str {
    if !down {
        return "forward-up";
    }
    let Some(select) = consumed_hit_category(specs, key_code, flags) else {
        return "forward-unowned";
    };
    crate::input_match::consumed_refusal(select, basic_eligible, select_eligible)
        .unwrap_or("dispatch")
}

/// Pure channel-drain mapping for one received hit (no OS touch, no
/// logging): the exact continue-vs-return `TapTileSource::next` applies
/// after triage. Refused tokens drop (`None`: the caller counts the refusal
/// and keeps pumping), while `dispatch`, `forward-up`, and `forward-unowned`
/// become the `RawKeyEvent` the input owner re-matches exactly downstream —
/// so a refused channel input can never escape as an action, and eligible
/// downs plus all ups always do.
#[must_use]
pub(crate) fn drain_received_hit(
    hit: &crate::ffi::TapHit,
    decision: &'static str,
) -> Option<RawKeyEvent> {
    match decision {
        "dispatch" | "forward-up" | "forward-unowned" => Some(RawKeyEvent {
            kind: if hit.down {
                TapKeyKind::KeyDown
            } else {
                TapKeyKind::KeyUp
            },
            flags: hit.flags,
            key_code: hit.key_code,
        }),
        _ => None,
    }
}

/// Pure cumulative tap report: fold one retired guard's final snapshot into
/// the running total (saturating per field). Fresh guards start their own
/// counters at zero, so without this fold every recreation would reset the
/// terminal evidence; with it the published report stays cumulative across
/// recoveries while the timeout / user-input / readback-only kinds stay
/// distinct. No desktop effect.
#[must_use]
pub(crate) fn accumulate_tap_report(
    total: crate::ffi::TapSourceReport,
    add: crate::ffi::TapSourceReport,
) -> crate::ffi::TapSourceReport {
    crate::ffi::TapSourceReport {
        matched: total.matched.saturating_add(add.matched),
        suppressed: total.suppressed.saturating_add(add.suppressed),
        deliveries: total.deliveries.saturating_add(add.deliveries),
        reenables_after_timeout: total
            .reenables_after_timeout
            .saturating_add(add.reenables_after_timeout),
        user_disables: total.user_disables.saturating_add(add.user_disables),
        unnotified_disables: total
            .unnotified_disables
            .saturating_add(add.unnotified_disables),
    }
}

/// Pure explicit-fixture-hold predicate: true while the input owner is
/// deliberately parking the tap directly OS-disabled for observation
/// (`direct_hold` set) or is finishing the hold-to-release transition
/// (`holding_direct`). The production health check bypasses recovery only
/// here — production code never sets either flag, so the bypass is
/// structurally fixture-only. No desktop effect.
#[must_use]
pub(crate) const fn fixture_hold_active(direct_hold: bool, holding_direct: bool) -> bool {
    direct_hold || holding_direct
}

impl TapTileSource {
    /// Apply the drag Escape watch to a freshly installed tap guard
    /// (initial plus every fresh-guard recreation); `None` is a no-op.
    fn arm_drag_escape(&self, tap: &TapSource) {
        if let Some((active, pending, tx, dropped)) = &self.drag_escape {
            tap.set_drag_escape_watch(
                Arc::clone(active),
                Arc::clone(pending),
                tx.clone(),
                Arc::clone(dropped),
            );
        }
    }

    fn publish_report(&self) {
        let snapshot = accumulate_tap_report(self.carried, self.tap.report());
        *self
            .report
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = snapshot;
    }

    fn publish_direct_enabled(&self) {
        let enabled = self.tap.is_tap_direct_enabled();
        self.direct_enabled
            .store(u64::from(enabled), Ordering::SeqCst);
    }

    fn note_refusal(&self, reason: String) {
        *self
            .refusal
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(reason);
    }

    /// Owner-side triage for one consumed hit (owner thread only, never the
    /// callback): fresh per-category cache bits through the pure
    /// [`triage_consumed_hit`]. Only `dispatch`, `forward-up`, and
    /// `forward-unowned` become `RawKeyEvent`s; both refusal tokens drop
    /// without effects (counted in [`Self::note_consumed_refusal`]).
    fn triage_drain_decision(&self, hit: &TapHit) -> &'static str {
        triage_consumed_hit(
            &self.specs,
            hit.key_code,
            hit.flags,
            hit.down,
            self.eligibility.is_eligible(),
            Some(self.eligibility_select.is_eligible()),
        )
    }

    /// Count one consumed-then-refused KeyDown hit with bounded logging:
    /// the first sighting plus every 50th per category. Static tokens and
    /// counts only — never keys, flags, bundles, or content.
    fn note_consumed_refusal(&mut self, reason: &'static str) {
        let count = if reason == "consumed-refused-select" {
            self.consumed_refused_select += 1;
            self.consumed_refused_select
        } else {
            self.consumed_refused_basic += 1;
            self.consumed_refused_basic
        };
        if count == 1 || count % 50 == 0 {
            eprintln!(
                "tiler-macos tile-tap: {reason} (consumed-refused-basic={} consumed-refused-select={})",
                self.consumed_refused_basic, self.consumed_refused_select,
            );
        }
    }

    /// Swap in a freshly installed guard on this same owner thread: fold the
    /// retired guard's final snapshot into the cumulative counters FIRST
    /// (terminal evidence survives the recreation with kinds intact), then
    /// replace the guard and channel, bump the eligibility epoch, and
    /// republish. Shared by every fresh-guard install (gate return,
    /// timeout recovery, explicit re-arm) so no path resets the counters.
    fn swap_fresh_guard(&mut self, new_tap: TapSource, rx: mpsc::Receiver<TapHit>) {
        // Bounded overlap: the fresh guard installs before the old guard
        // drops; no synthetic posts flow during the window (callers gate
        // posts on the epoch/readback), so no chord is observed twice and
        // no hit is claimed from the transition itself.
        self.carried = accumulate_tap_report(self.carried, self.tap.report());
        let old = std::mem::replace(&mut self.tap, new_tap);
        self.rx = rx;
        drop(old);
        self.holding_direct = false;
        self.eligibility.bump_epoch();
        self.eligibility_select.bump_epoch();
        self.publish_report();
        self.publish_direct_enabled();
    }

    /// EXPLICIT fresh re-arm when requested (owner thread only): drops the old
    /// tap guard WITHOUT a same-tap re-enable and installs a FRESH `TapSource`
    /// over the same chord specs (same placement, `HeadInsertEventTap`,
    /// session tap, exact suppression armed), then clears the DIRECT hold and
    /// bumps the epoch. Returns `Ok(true)` once per completed re-arm.
    /// A recreation refusal (for example revoked consent) is recorded
    /// truthfully and returns `Err` — never masked, never a fake callback.
    fn maybe_rearm(&mut self) -> Result<bool, String> {
        if !self.rearm_requested.swap(false, Ordering::SeqCst) {
            return Ok(false);
        }
        let (tx, rx) = mpsc::sync_channel::<TapHit>(64);
        let new_tap = match TapSource::create_scoped(&self.specs, true, &tx) {
            Ok(tap) => tap,
            Err(reason) => {
                let cause = format!("explicit re-arm refused: {reason}");
                self.note_refusal(cause.clone());
                return Err(cause);
            }
        };
        self.arm_drag_escape(&new_tap);
        self.swap_fresh_guard(new_tap, rx);
        self.direct_hold.store(false, Ordering::SeqCst);
        self.rearm_epoch.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }

    /// Install a FRESH scoped source on this same owner thread (same chords,
    /// same placement), dropping the old guard WITHOUT a same-tap re-enable.
    /// A recreation refusal (for example revoked Accessibility consent)
    /// returns `Err` — never masked, never a fake callback. Shared by
    /// gate-return and disable recovery; callers add their own refusal
    /// context.
    fn install_fresh_scoped(&mut self) -> Result<(), crate::ffi::TapSetupError> {
        let (tx, rx) = mpsc::sync_channel::<TapHit>(64);
        let new_tap = TapSource::create_scoped(&self.specs, true, &tx)?;
        self.arm_drag_escape(&new_tap);
        self.swap_fresh_guard(new_tap, rx);
        Ok(())
    }

    /// Owner-side recovery for one disabled tap (timeout notification with
    /// an unverified re-enable already consumed by the pump, a user-input
    /// notification, or an enabled readback that disagrees with a Held pump
    /// turn): re-enable the SAME tap only with an enabled readback, else
    /// install a FRESH scoped source over the same chords and eligibility on
    /// this same owner thread (never a second same-tap enable, never a new
    /// owner or channel). `origin` names the kind for the log line
    /// (timeout vs user-input vs enabled-readback-only); `unnotified`
    /// counts the recovery under `unnotified_disables` instead of the
    /// timeout count. Lock / Secure Input / eligibility gates are consulted
    /// by the caller BEFORE this runs: while blocked the caller parks
    /// released and retries later, so this never overrides trust or
    /// privacy gates. A fresh-creation denial fails closed with `Err` (the
    /// caller records the refusal and ends truthfully).
    fn recover_disabled(&mut self, origin: &'static str, unnotified: bool) -> Result<(), String> {
        if self.tap.reenable_same_tap_verified() {
            if unnotified {
                self.tap.note_unnotified_disabled();
            }
            self.publish_report();
            self.publish_direct_enabled();
            let counts = accumulate_tap_report(self.carried, self.tap.report());
            eprintln!(
                "tiler-macos tile-tap: disabled by {origin}; same-tap re-enable verified (reenables_after_timeout={} user_disables={} unnotified_disables={})",
                counts.reenables_after_timeout, counts.user_disables, counts.unnotified_disables,
            );
            return Ok(());
        }
        self.install_fresh_scoped()
            .map_err(|cause| format!("{origin} recovery re-arm refused: {cause}"))?;
        let counts = accumulate_tap_report(self.carried, self.tap.report());
        eprintln!(
            "tiler-macos tile-tap: {origin} re-enable unverified; fresh tap installed (reenables_after_timeout={} user_disables={} unnotified_disables={})",
            counts.reenables_after_timeout, counts.user_disables, counts.unnotified_disables,
        );
        Ok(())
    }

    /// Fresh guard after a global-release interval (owner thread only):
    /// after a lock / Secure Input / unknown-session interval the old guard
    /// drops WITHOUT a same-tap re-enable and a FRESH scoped source
    /// installs on this same thread (same chords, same placement, same
    /// eligibility caches), with the eligibility epoch bumped. A recorded
    /// hard refusal (for example a denied recreation) is never masked: when
    /// a refusal is already recorded this returns `Err` and the run fails
    /// closed instead of auto-overriding it. Scoped ineligibility never
    /// reaches this path (ineligible foregrounds consume, then refuse
    /// per-hit without effects).
    fn recreate_on_return_to_held(&mut self) -> Result<(), String> {
        if self
            .refusal
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
        {
            return Err("tap recreation refused earlier; refusing (no auto-override)".to_owned());
        }
        self.install_fresh_scoped().map_err(|reason| {
            let cause = format!("scope-return re-arm refused: {reason}");
            self.note_refusal(cause.clone());
            cause
        })
    }
}

impl Drop for TapTileSource {
    fn drop(&mut self) {
        // Final counters before the tap port invalidates: terminal evidence
        // survives the source (the factory slot outlives the owner thread).
        self.publish_report();
    }
}

impl InputSource for TapTileSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
        loop {
            if stop.load(Ordering::SeqCst) {
                return None;
            }
            // Fixture-only EXPLICIT re-arm (`--direct-rearm` only; the flag
            // stays false in every production run): the old guard drops
            // without a same-tap re-enable and a FRESH source installs on
            // this same owner thread. A genuine refusal fails closed here
            // (recorded, never masked). Takes precedence over the hold below
            // so the old disabled guard is never re-enabled on the same tap.
            match self.maybe_rearm() {
                Ok(false) => {}
                Ok(true) => continue,
                Err(_) => return None,
            }
            // Fixture-only one-shot disable-once fault (`--disable-once`
            // only; the flag stays false in every production run, so
            // production behavior below is unchanged): disable the tap
            // directly ONCE with no fixture hold, then keep pumping so the
            // tap recovers automatically through whichever path the OS takes
            // (notification or readback-only); the leg reports the actual
            // kind from the counters.
            if self.disable_once_fault.swap(false, Ordering::SeqCst) {
                self.tap.set_tap_direct_enabled(false);
                self.publish_direct_enabled();
            }
            // Fixture-only DIRECT disable hold (input-session
            // `--direct-disable-hold-ms` only; the flag stays false in every
            // production run, so production behavior below is unchanged):
            // while held, the owner parks the tap directly OS-disabled and
            // publishes the actual `CGEventTapIsEnabled` readback instead
            // of pumping; the hold-to-release transition does a single
            // direct re-enable, then the standard pump resumes.
            if self.direct_hold.load(Ordering::SeqCst) {
                self.tap.set_tap_direct_enabled(false);
                self.publish_direct_enabled();
                self.holding_direct = true;
                std::thread::park_timeout(std::time::Duration::from_millis(50));
                continue;
            }
            if self.holding_direct {
                self.tap.set_tap_direct_enabled(true);
                self.publish_direct_enabled();
                self.holding_direct = false;
            }
            // Gated (lock screen, Secure Input, unknown session):
            // interception releases until the ordinary unlocked state
            // returns. Bounded park slices keep stop latency finite; the
            // shutdown `unpark` wakes a parked slice early.
            if let Some(_reason) = live_gate() {
                self.tap.set_enabled(false);
                std::thread::park_timeout(std::time::Duration::from_millis(50));
                self.gated_ms.fetch_add(50, Ordering::SeqCst);
                self.was_ineligible = true;
                continue;
            }
            // Scoped eligibility AFTER suppression (consume-first): refresh
            // both caches from cheap live reads plus a plain store snapshot
            // (cadence-throttled; unknown/stale denies) but never park or
            // release here — ineligible foregrounds (out-of-scope,
            // fullscreen/game, wrong Space, stale/unknown facts, zero user
            // currents, focus mismatches) consume, and each consumed hit
            // refuses owner-side below without effects. Only the global
            // gate above releases interception. No AX runs on this thread:
            // per-app facts arrive via the shared store from each AX
            // owner's already-read snapshots.
            attest_tile_eligibility(
                &self.scope,
                &self.eligibility,
                &self.eligibility_select,
                self.facts.as_ref(),
                false,
                &mut self.last_attest,
            );
            if self.was_ineligible {
                self.was_ineligible = false;
                if let Err(reason) = self.recreate_on_return_to_held() {
                    let _ = reason;
                    return None;
                }
                continue;
            }
            self.tap.set_enabled(true);
            match self.tap.pump(20) {
                TapPump::Held => {
                    // Production owner-side health on every bounded pump
                    // turn: the OS can leave a tap directly disabled with NO
                    // notification (measured live: disable_kind=0 with
                    // reenables=0, later tap_is_enabled=false with frozen
                    // counters — `pump` then returns Held forever). A Held
                    // turn that is not actually enabled recovers here with
                    // its own unnotified-disabled counter (never the timeout
                    // count), except under the EXPLICIT fixture hold, which
                    // owns its deliberate disabled observation and release
                    // (production never sets it, so the bypass is
                    // structurally fixture-only).
                    let held = fixture_hold_active(
                        self.direct_hold.load(Ordering::SeqCst),
                        self.holding_direct,
                    );
                    match unnotified_disable_action(self.tap.is_tap_direct_enabled(), held) {
                        None => {}
                        Some(DisableAction::ReenableSameTap) => {
                            if let Err(reason) =
                                self.recover_disabled("enabled-readback-only", true)
                            {
                                self.note_refusal(reason);
                                self.publish_report();
                                eprintln!(
                                    "tiler-macos tile-tap: enabled-readback-only recovery refused; ending truthfully"
                                );
                                return None;
                            }
                            continue;
                        }
                        Some(_) => {
                            unreachable!("readback-only recovery re-enables the same tap first")
                        }
                    }
                }
                TapPump::TimeoutRecovered => {
                    // Same-tap re-enable verified by the pump (counted in
                    // the report). Static tokens and counts only.
                    let counts = accumulate_tap_report(self.carried, self.tap.report());
                    eprintln!(
                        "tiler-macos tile-tap: disabled by timeout; same-tap re-enable verified (reenables_after_timeout={} user_disables={} unnotified_disables={})",
                        counts.reenables_after_timeout,
                        counts.user_disables,
                        counts.unnotified_disables,
                    );
                }
                TapPump::NeedsRecreate => {
                    // The same-tap re-enable did not verify: install a FRESH
                    // tap over the same chords on this same owner thread
                    // (never a second same-tap enable). A recreation refusal
                    // fails closed with the refusal.
                    match unverified_reenable_action() {
                        DisableAction::RecreateFresh => {}
                        DisableAction::ReenableSameTap | DisableAction::RefuseUserDisabled => {
                            unreachable!("timeout recovery never re-enables twice or refuses")
                        }
                    }
                    if let Err(reason) = self
                        .install_fresh_scoped()
                        .map_err(|cause| format!("timeout recovery re-arm refused: {cause}"))
                    {
                        self.note_refusal(reason);
                        self.publish_report();
                        eprintln!(
                            "tiler-macos tile-tap: timeout recovery refused; ending truthfully"
                        );
                        return None;
                    }
                    let counts = accumulate_tap_report(self.carried, self.tap.report());
                    eprintln!(
                        "tiler-macos tile-tap: timeout re-enable unverified; fresh tap installed (reenables_after_timeout={} user_disables={} unnotified_disables={})",
                        counts.reenables_after_timeout,
                        counts.user_disables,
                        counts.unnotified_disables,
                    );
                    continue;
                }
                TapPump::UserDisabled => {
                    // Logged recoverable state, never terminal by name
                    // alone: the notification name never proves consent
                    // revocation (the surveyed skhd tap-disabled restart
                    // re-enables both kinds). Recovery is
                    // category-independent: the same tap re-enables with a
                    // readback regardless of scoped eligibility (consumed
                    // hits refuse per-hit without effects, so re-enabling
                    // into an ineligible foreground is harmless), while the
                    // lock / Secure Input gates still release interception —
                    // while globally gated the owner parks released and
                    // retries later, so recovery never overrides trust or
                    // privacy gates. Only a hard creation denial refuses.
                    match disable_action(DisableKind::UserInput) {
                        DisableAction::ReenableSameTap => {}
                        DisableAction::RecreateFresh | DisableAction::RefuseUserDisabled => {
                            unreachable!("user-input notification recovers via same-tap re-enable")
                        }
                    }
                    if live_gate().is_some() {
                        self.tap.set_enabled(false);
                        std::thread::park_timeout(std::time::Duration::from_millis(50));
                        self.gated_ms.fetch_add(50, Ordering::SeqCst);
                        continue;
                    }
                    if let Err(reason) = self.recover_disabled("user-input", false) {
                        self.note_refusal(reason);
                        self.publish_report();
                        eprintln!(
                            "tiler-macos tile-tap: user-input recovery refused; ending truthfully"
                        );
                        return None;
                    }
                    continue;
                }
            }
            // Publish per slice so the fault-injection leg observes the
            // timeout re-enable mid-run (terminal evidence lands on drop).
            // The DIRECT state readback rides along read-only (no scope
            // writes on the production path).
            self.publish_report();
            self.publish_direct_enabled();
            if stop.load(Ordering::SeqCst) {
                return None;
            }
            match self.rx.try_recv() {
                Ok(hit) => {
                    // Consume-first owner refusal: a consumed hit triages on
                    // the fresh per-category caches and only `dispatch`,
                    // `forward-up`, and `forward-unowned` become actions —
                    // refused hits (fullscreen/game, out-of-scope,
                    // stale/unknown facts, zero user currents, focus/Space
                    // mismatch, basic and select alike) drop here without
                    // effects (see `drain_received_hit`).
                    let decision = self.triage_drain_decision(&hit);
                    match drain_received_hit(&hit, decision) {
                        Some(event) => return Some(event),
                        None => {
                            self.note_consumed_refusal(decision);
                            continue;
                        }
                    }
                }
                Err(_) => {
                    // No hit this slice: pump again (the pump bound caps
                    // stop latency; never a busy spin without the slice).
                }
            }
        }
    }
}

// -- Off source (explicit no-input) ------------------------------------------

/// Explicit no-input factory: parks until stop (clean exit, never a dirty
/// EOS) so the tile run proceeds open-loop on AX observations.
pub struct OffTileFactory;

impl InputSourceFactory for OffTileFactory {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        Ok(Box::new(OffTileSource))
    }

    fn describe(&self) -> &'static str {
        "tile-off"
    }
}

struct OffTileSource;

impl InputSource for OffTileSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
        // True park (no timeout polling): stop/shutdown wakes via `unpark`
        // (an `unpark` before `park` grants a permit, so no wake is lost).
        while !stop.load(Ordering::SeqCst) {
            std::thread::park();
        }
        None
    }
}
