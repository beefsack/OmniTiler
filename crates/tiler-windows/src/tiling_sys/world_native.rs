#![forbid(unsafe_code)]
//! Production native actuator plus truthful applied-receipt capture.
//!
//! [`LiveNative`] is the real [`NativeOps`](super::world_diff::NativeOps)
//! implementation for the world route: every read is a fresh native
//! visibility/frame/claim/identity re-read, every setter goes through the
//! existing identity/claim-verified mechanisms, and every receipt is proven
//! by a fresh read afterward. The setter boolean is never proof.
//!
//! Owner route: both world-tick entry points (`run_observation_tick` and
//! `run_world_command` in [`super::world_tick`]) assemble it per turn via
//! [`LiveNative::assemble`] with [`NativePrimitives::default()`], apply the
//! accepted reply through it, and merge only the proven receipt updates
//! into the adapter's applied homing/views. Nothing here runs outside an
//! owner tick or command.
//!
//! Native rules reused, never reinvented:
//! - Hide/reveal run the accepted `product_hide::sys` claim path
//!   (`admit_managed_claim_for_hide` + `hide_managed_claim` /
//!   `reveal_product_claim_to`) with the same admission fences as the hide
//!   effects (allowlist/proof, scope, hosted child, visible lifetime,
//!   topmost exception). Commit owns the claim before any post; mechanical caches (`workspaces` assignment, `hidden_claims`)
//!   update only at commit, and visibility proof afterwards decides the
//!   receipt. A mismatch between cache and live visibility reads back as
//!   unknown (hold), never as a fabricated Applied.
//! - Geometry writes convert the desired visible frame to the native outer
//!   frame through the measured insets (`FrameInsets::visible_to_outer`,
//!   the same conversion as the legacy write path) and post one
//!   `SetWindowPos` with `SWP_NOACTIVATE | SWP_NOZORDER`. A cross-monitor
//!   write that changes the window DPI under the call reasserts the same
//!   intended frame once through the fresh insets (invalidated placement,
//!   never a clamp). A recycled HWND
//!   (pid/creation drift across the call) is ambiguous, never success.
//! - Focus is one `SetForegroundWindow`; the async activation may land
//!   after the setter returns, so only a fresh independent foreground
//!   read proves arrival. Event-driven in-flight tracking in the diff
//!   tells the product's own prior landing apart from a genuine foreign
//!   competitor on a later switch; no barrier, timer, retry, or replay.
//! - The core is the sole logical authority. No native workspace property
//!   is written or read as policy: views and membership live in the core,
//!   and the adapter retains only proven applied homes/views plus output
//!   wire ids, proved through live visibility plus claim/identity reads.
//! - Invalidation is polled before every native effect; at-most-once
//!   ambiguity follows the shared [`NativeOps`](super::world_diff::NativeOps)
//!   contract (throw/uncertain map to `Err`, explicit refusal to
//!   `Ok(false)`, and only a fresh proven read settles Applied upstream).
//!
//! Mockable seam: [`NativePrimitives`] carries every native interaction as a
//! plain function pointer defaulting to the real implementation. Unit tests
//! override individual pointers with scripted fakes while keeping the rest
//! production, so setter lies, partial hide/reveal, identity drift,
//! fullscreen/hidden holds, and racing invalidation are proven offline
//! against this exact type. The defaults are the auditable call sites.
//!
//! Receipt capture lives in [`super::world_native_receipts`]:
//! [`attach_workspace_receipts`](super::world_native_receipts::attach_workspace_receipts)
//! and [`observed_view_receipts`](super::world_native_receipts::observed_view_receipts)
//! derive the capture's `observed_views` and per-window `observed_workspace`
//! from fresh visibility plus owner tables. Only exactly-one-visible-workspace (non-sticky) views prove, plus the
//! memberless-active-with-zero-visible batch-completion case; an empty
//! workspace never proves by echoing desired state. Unknown stays unknown
//! so the core retains pending flights instead of settling on a guess.

use super::world_diff::{FreshWindow, NativeOps};
use crate::product_hide::sys::{VisibilityPostDiag, VisibilityPostTrace};

/// Trace-only native hide-claim lane: the world actuator's post sink arms
/// the executed-post record; `None` runs the identical native effect
/// untraced.
pub(super) type HideClaimFn = fn(
    &LedgerStore,
    &ProcessIdentity,
    &WindowIdentity,
    &Path,
    Option<&mut VisibilityPostTrace>,
) -> Result<WindowIdentity, String>;
/// Trace-only native reveal-claim lane (see [`HideClaimFn`]).
pub(super) type RevealClaimFn = fn(
    &LedgerStore,
    &ProcessIdentity,
    &WindowIdentity,
    bool,
    Option<&mut VisibilityPostTrace>,
) -> Result<Revealed, String>;
use super::{AllowEntry, HiddenRecord, MonitorArea, ObservedWindow, ScopeHostChild, TokenMap};
use crate::model::{ProcessIdentity, WindowIdentity};
use crate::native::HeldProcess;
use crate::native_guards::WindowKey;
use crate::storage::LedgerStore;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use tiler_core::geometry::Rect;
use windows_sys::Win32::Foundation::HWND;

#[path = "world_native_frame.rs"]
mod frame;

/// Outcome classes for one hide admission, mirroring the hide-effect
/// admission fences without the owner borrow: proof-mode gate, scope,
/// hosted child, visible lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmitOutcome {
    OriginVanished,
    Uncertain,
    IdentityChanged,
    Refused,
}

/// Map one committed hide/reveal attempt onto the [`NativeOps`] contract:
/// success stays success, explicit refusal (`Refused`, vanished origin)
/// is `Ok(false)` so the diff can settle proven refusal, and torn state
/// (`Uncertain`, identity drift) is ambiguous (`Err`) so the operation
/// arms at-most-once instead of repeating.
fn settle_commit(result: Result<(), AdmitOutcome>) -> Result<bool, ()> {
    match result {
        Ok(()) => Ok(true),
        Err(AdmitOutcome::Refused) | Err(AdmitOutcome::OriginVanished) => Ok(false),
        Err(AdmitOutcome::Uncertain) | Err(AdmitOutcome::IdentityChanged) => Err(()),
    }
}

/// Scriptable result of one reveal post: shown (or already visible, proof
/// decides) versus a verifiably gone window whose claim retires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Revealed {
    Shown,
    Retired,
}

/// Every native interaction behind [`LiveNative`], as plain function
/// pointers. Defaults are the real implementations (see `real_*` below);
/// tests override individual lanes with scripted fakes.
#[derive(Clone, Copy)]
pub(super) struct NativePrimitives {
    pub observe:
        fn(u64, &ProcessIdentity, &[Rect], &mut TokenMap) -> Result<ObservedWindow, ObserveError>,
    pub window_pid: fn(u64) -> u32,
    pub is_visible: fn(u64) -> bool,
    pub is_iconic: fn(u64) -> bool,
    pub set_outer: fn(u64, Rect) -> bool,
    pub foreground: fn() -> u64,
    pub focus: fn(u64) -> bool,
    /// Bounded last-input prime: one unassigned-key down/up pair through
    /// the real input path, returning the accepted-event count (never
    /// trusted as proof). Runs on the owner thread after every
    /// safety/lifetime/scope/supersession veto and immediately before the
    /// focus setter; vetoed effects never prime. No wait, retry, or input
    /// attachment.
    pub prime: fn(u16) -> u8,
    pub member_tag: fn(u64) -> Option<String>,
    pub hook_pending: fn() -> bool,
    /// Live cursor read in adapter units (`None` when unreadable).
    /// The warp path skips its write when already at the target.
    pub read_cursor: fn() -> Option<(i32, i32)>,
    /// Mechanical cursor warp: one position setter, no input synthesis,
    /// no click, no wait. True is setter success (the caller re-reads;
    /// success alone never proves arrival).
    pub warp_cursor: fn(i32, i32) -> bool,
    /// Live window DPI in physical pixels per inch (`None` when unreadable,
    /// never guessed). Cross-monitor writes can change this under the call;
    /// the frame path re-bases once through it instead of pinning a clamp.
    pub dpi: fn(u64) -> Option<u32>,
    pub process_identity: fn(u64) -> Option<ProcessIdentity>,
    pub read_topmost: fn(u64) -> bool,
    pub admit: fn(
        u64,
        &ProcessIdentity,
        &ProcessIdentity,
        &str,
        bool,
    ) -> Result<WindowIdentity, AdmitOutcome>,
    pub hide_claim: HideClaimFn,
    pub reveal_claim: RevealClaimFn,
    /// Pump-free nonactivating hide/reveal for remembered-focus pure views.
    /// No-effect and no-memory switches never take these lanes. The sink
    /// carries the actual executed post record when the caller arms trace;
    /// `None` keeps the untraced behavior exactly.
    pub hide_claim_na: HideClaimFn,
    pub reveal_claim_na: RevealClaimFn,
    /// Live move/size modal-loop probe for one HWND's thread.
    /// `None` when unreadable (fail closed).
    pub move_size: fn(u64) -> Option<bool>,
    /// Fresh facts read of a hidden window, bypassing only the visible
    /// gate. The caller proves the exact owned committed hidden claim
    /// first (ledger plus identity/lifetime gates); this lane only reads.
    pub observe_hidden:
        fn(u64, &ProcessIdentity, &[Rect], &mut TokenMap) -> Result<ObservedWindow, ObserveError>,
}

/// Erased observation failure: the production actuator only needs to know
/// the window did not freshly observe (hold narrowly). Identity-bearing
/// `Known` failures still carry no frame, so both lanes hold identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObserveError {
    Unknown,
}

fn real_observe(
    hwnd: u64,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
) -> Result<ObservedWindow, ObserveError> {
    super::observe::observe_window(hwnd as isize as HWND, me, fulls, tokens)
        .map_err(|_| ObserveError::Unknown)
}

fn real_observe_hidden(
    hwnd: u64,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
) -> Result<ObservedWindow, ObserveError> {
    super::observe::observe_window_hidden_verified(hwnd as isize as HWND, me, fulls, tokens)
        .map_err(|_| ObserveError::Unknown)
}

fn real_window_pid(hwnd: u64) -> u32 {
    crate::ffi::window_pid(hwnd)
}

fn real_is_visible(hwnd: u64) -> bool {
    super::pending_seams::live_is_visible(hwnd)
}

fn real_is_iconic(hwnd: u64) -> bool {
    super::pending_seams::live_is_iconic(hwnd)
}

fn real_set_outer(hwnd: u64, outer: Rect) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER};
    crate::ffi::set_window_pos(
        hwnd,
        outer.x,
        outer.y,
        outer.w,
        outer.h,
        SWP_NOACTIVATE | SWP_NOZORDER,
    )
}

fn real_foreground() -> u64 {
    crate::ffi::foreground_window()
}

fn real_focus(hwnd: u64) -> bool {
    crate::ffi::set_foreground_window(hwnd)
}

/// Unassigned virtual key for the bounded last-input prime (the approved
/// E8 menu-mask mechanism, `decisions.md` focus mechanism): down/up with
/// no scancode masquerading. The product input filter already drops
/// injected E8 before command dispatch, so the prime never mints input.
const FOCUS_PRIME_VK: u16 = 0xE8;

/// Real bounded last-input prime: one `vk` down/up pair through
/// `SendInput` (the OS grants foreground rights to the last input
/// provider). Bounded single-release included; never trusted as proof.
fn real_prime(vk: u16) -> u8 {
    crate::ffi::prime_foreground_key(vk)
}

fn real_member_tag(hwnd: u64) -> Option<String> {
    super::pending_seams::live_member_tag(hwnd)
}

fn real_hook_pending() -> bool {
    super::hook_edges_pending()
}

fn real_read_cursor() -> Option<(i32, i32)> {
    crate::ffi::cursor_pos()
}

fn real_warp_cursor(x: i32, y: i32) -> bool {
    crate::ffi::set_cursor_pos(x, y)
}

fn real_dpi(hwnd: u64) -> Option<u32> {
    let dpi = crate::ffi::dpi_for_window(hwnd);
    (dpi != 0).then_some(dpi)
}

fn real_process_identity(hwnd: u64) -> Option<ProcessIdentity> {
    let pid = crate::ffi::window_pid(hwnd);
    if pid == 0 {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let ident = held.identity().ok()?;
    (ident.pid == pid).then_some(ident)
}

fn real_read_topmost(hwnd: u64) -> bool {
    super::workspace_mode::read_topmost_now(hwnd)
}

fn real_move_size(hwnd: u64) -> Option<bool> {
    crate::ffi::move_size_active(hwnd)
}

fn real_admit(
    hwnd: u64,
    me: &ProcessIdentity,
    expected: &ProcessIdentity,
    member_tag: &str,
    allow_project_topmost: bool,
) -> Result<WindowIdentity, AdmitOutcome> {
    use crate::product_hide::sys::ManagedAdmitError;
    match crate::product_hide::sys::admit_managed_claim_for_hide(
        hwnd,
        me,
        expected,
        member_tag,
        allow_project_topmost,
    ) {
        Ok(claim) => Ok(claim),
        Err(ManagedAdmitError::Absent) => Err(AdmitOutcome::OriginVanished),
        Err(ManagedAdmitError::Uncertain) => Err(AdmitOutcome::Uncertain),
        Err(ManagedAdmitError::WrongIdentity) => Err(AdmitOutcome::IdentityChanged),
        Err(ManagedAdmitError::Refused(_)) => Err(AdmitOutcome::Refused),
    }
}

fn real_hide_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
    sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
) -> Result<WindowIdentity, String> {
    use crate::product_hide::sys::VisibilityMode;
    crate::product_hide::sys::hide_managed_claim_with_mode_traced(
        store,
        owner,
        claim,
        dir,
        VisibilityMode::PumpedShowAsync,
        sink,
    )
    .map_err(|e| e.to_string())
}

fn real_reveal_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
) -> Result<Revealed, String> {
    use crate::product_hide::sys::VisibilityMode;
    match crate::product_hide::sys::reveal_product_claim_to_with_mode_traced(
        store,
        owner,
        claim,
        minimized,
        VisibilityMode::PumpedShowAsync,
        sink,
    )
    .map_err(|e| e.to_string())?
    {
        crate::product_hide::ProductTeardown::Retired => Ok(Revealed::Retired),
        _ => Ok(Revealed::Shown),
    }
}

fn real_hide_claim_nonactivating(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    dir: &Path,
    sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
) -> Result<WindowIdentity, String> {
    crate::product_hide::sys::hide_managed_claim_nonactivating_traced(
        store, owner, claim, dir, sink,
    )
    .map_err(|e| e.to_string())
}

fn real_reveal_claim_nonactivating(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
    sink: Option<&mut crate::product_hide::sys::VisibilityPostTrace>,
) -> Result<Revealed, String> {
    match crate::product_hide::sys::reveal_product_claim_to_nonactivating_traced(
        store, owner, claim, minimized, sink,
    )
    .map_err(|e| e.to_string())?
    {
        crate::product_hide::ProductTeardown::Retired => Ok(Revealed::Retired),
        _ => Ok(Revealed::Shown),
    }
}

impl Default for NativePrimitives {
    fn default() -> Self {
        Self {
            observe: real_observe,
            window_pid: real_window_pid,
            is_visible: real_is_visible,
            is_iconic: real_is_iconic,
            set_outer: real_set_outer,
            foreground: real_foreground,
            focus: real_focus,
            prime: real_prime,
            member_tag: real_member_tag,
            hook_pending: real_hook_pending,
            read_cursor: real_read_cursor,
            warp_cursor: real_warp_cursor,
            dpi: real_dpi,
            process_identity: real_process_identity,
            read_topmost: real_read_topmost,
            admit: real_admit,
            hide_claim: real_hide_claim,
            reveal_claim: real_reveal_claim,
            hide_claim_na: real_hide_claim_nonactivating,
            reveal_claim_na: real_reveal_claim_nonactivating,
            move_size: real_move_size,
            observe_hidden: real_observe_hidden,
        }
    }
}

/// Production native actuator over the world-route diff.
///
/// Holds the exact owner pieces the diff needs: shared identity/lifetime
/// tables, the mutable mechanical caches (updated only at claim commit,
/// never on desire), and the environment (owner identity, monitor fulls,
/// allowlist/proof gates, ledger, stable dir, gesture set). The reverse
/// token map is snapshotted at construction; generation rotation rebuilds
/// the actuator, so stale tokens can never address a recycled lifetime.
///
/// Policy authority is the core reply alone: logical homing, rings, and
/// views arrive as owned core projections cloned per pass (never a second
/// policy, never a legacy table read). The mechanical device-to-wire map
/// resolves outputs; hidden membership is the hide-claim ledger only.
pub(super) struct LiveNative<'a> {
    pub me: &'a ProcessIdentity,
    pub fulls: &'a [Rect],
    pub tokens: TokenMap,
    pub output_tokens: &'a HashMap<String, String>,
    /// Desired logical homing per Engine token (post-command core target).
    /// Read-only policy target for this pass, never proof.
    pub core_homing: HashMap<String, (String, String)>,
    /// Desired logical rings per output (core target side).
    pub core_rings: HashMap<String, Vec<String>>,
    /// Desired logical views per output (core target side).
    pub core_views: HashMap<String, String>,
    /// Desired visible frames per Engine token (core target side) for
    /// tiled rows only. Actuation TARGET only, never proof: the frame
    /// write runs inside `move_membership` under the same gates as
    /// `write_frame`, and only an exact fresh readback proves arrival.
    /// Membership-only lanes (floats/sticky/overlays) carry no entry
    /// and preserve the physical frame.
    pub desired_frames: HashMap<String, Rect>,
    /// Live monitor areas for screen-affinity arrival proof (device
    /// order, same source as capture). Arrival on a cross-output target
    /// requires the live frame to sit on the desired output.
    pub areas: Vec<MonitorArea>,
    /// Retained actually applied native receipt state (proven homes/views).
    /// Keyed logical home/view but only updated after exact production
    /// readback proof; never a second policy owner.
    pub proven_homing: HashMap<String, (String, String)>,
    /// Retained actually applied views per output (proven only).
    pub proven_views: HashMap<String, String>,
    pub member_tags: &'a BTreeMap<WindowKey, String>,
    pub member_identity: &'a BTreeMap<WindowKey, ProcessIdentity>,
    pub hidden_claims: &'a mut BTreeMap<WindowKey, HiddenRecord>,
    /// Hide-commit snapshot seeds drained by the owner tick after apply:
    /// exact-lifetime freshly proven physical frames read before the hide
    /// post made them unreadable. The tick routes each seed into the
    /// tile/float snapshot cache under the same lane row assembly uses, so
    /// the next hidden report stages the proven frame instead of the
    /// pre-write slot.
    pub hidden_seeds: Vec<(WindowKey, Rect)>,
    /// Same-pass nonactivating departure hidden by this actuator instance:
    /// the entry foreground when the nonactivating view switch started,
    /// kept only when its hide committed and it still read invisible. The
    /// focus lane reads hidden facts only for this exact key; a stale
    /// hidden claim from a prior pass never matches because a fresh
    /// `assemble` starts with `None`.
    na_hidden_departure: Option<(WindowKey, ProcessIdentity)>,
    pub sticky: &'a BTreeMap<WindowKey, bool>,
    pub float_topmost_prev: &'a BTreeMap<WindowKey, bool>,
    pub scope: &'a [String],
    pub scope_hosts: &'a [ScopeHostChild],
    pub allowlist: Option<&'a Vec<AllowEntry>>,
    pub workspace_proof: bool,
    pub store: &'a LedgerStore,
    pub dir: &'a Path,
    pub active: &'a HashSet<u64>,
    pub token_key: HashMap<String, WindowKey>,
    pub prims: NativePrimitives,
    /// Diagnostics trace switch (see `NativeOps::set_diagnostics_trace`):
    /// armed by the owner before each apply. Gates the pre-call GUI read
    /// and `gui_focus_of` (no native `GetGUIThreadInfo` while false), and
    /// the visibility-post sink below (no post record while false).
    /// Never a behavior input: vetos and fences ignore it.
    trace: bool,
    /// Trace-only executed visibility posts this pass (actual host API at
    /// the native post site, not the configured lane). Drained by
    /// [`NativeOps::take_visibility_posts`] into the pass stats with an
    /// honest omitted count. Empty unless trace is armed.
    vis_posts: VisibilityPostTrace,
}

impl<'a> LiveNative<'a> {
    /// Assemble from disjoint owner borrows. The caller keeps `&mut
    /// WorldOwner` alongside: every reference below borrows a different
    /// field, never the whole loop. Core projections arrive as owned clones
    /// of the last applied reply (homing, rings, views): read-only policy
    /// facts for this pass, never a legacy table. `desired_frames` carries
    /// the reply's tiled effective frames as write targets only; `areas`
    /// carries the live monitor areas for affinity proof.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble(
        me: &'a ProcessIdentity,
        fulls: &'a [Rect],
        output_tokens: &'a HashMap<String, String>,
        core_homing: HashMap<String, (String, String)>,
        core_rings: HashMap<String, Vec<String>>,
        core_views: HashMap<String, String>,
        desired_frames: HashMap<String, Rect>,
        areas: Vec<MonitorArea>,
        proven_homing: HashMap<String, (String, String)>,
        proven_views: HashMap<String, String>,
        member_tokens: &'a BTreeMap<WindowKey, String>,
        member_tags: &'a BTreeMap<WindowKey, String>,
        member_identity: &'a BTreeMap<WindowKey, ProcessIdentity>,
        hidden_claims: &'a mut BTreeMap<WindowKey, HiddenRecord>,
        sticky: &'a BTreeMap<WindowKey, bool>,
        float_topmost_prev: &'a BTreeMap<WindowKey, bool>,
        scope: &'a [String],
        scope_hosts: &'a [ScopeHostChild],
        allowlist: Option<&'a Vec<AllowEntry>>,
        workspace_proof: bool,
        store: &'a LedgerStore,
        dir: &'a Path,
        active: &'a HashSet<u64>,
        prims: NativePrimitives,
    ) -> Self {
        let token_key: HashMap<String, WindowKey> = member_tokens
            .iter()
            .map(|(key, token)| (token.clone(), key.clone()))
            .collect();
        Self {
            me,
            fulls,
            tokens: TokenMap::default(),
            output_tokens,
            core_homing,
            core_rings,
            core_views,
            proven_homing,
            proven_views,
            member_tags,
            member_identity,
            hidden_claims,
            na_hidden_departure: None,
            sticky,
            float_topmost_prev,
            scope,
            scope_hosts,
            allowlist,
            workspace_proof,
            store,
            dir,
            active,
            token_key,
            prims,
            trace: false,
            vis_posts: VisibilityPostTrace::default(),
            desired_frames,
            areas,
            hidden_seeds: Vec::new(),
        }
    }

    fn key_of(&self, token: &str) -> Option<WindowKey> {
        self.token_key.get(token).cloned()
    }

    /// Device key for one wire output id (`o1..`), if the mechanical map
    /// names it.
    fn device_for_output(&self, output: &str) -> Option<String> {
        self.output_tokens
            .iter()
            .find(|(_, token)| token.as_str() == output)
            .map(|(device, _)| device.clone())
    }

    /// Committed hidden flag: the hide-claim ledger only, never a policy
    /// table.
    fn committed_hidden(&self, key: &WindowKey) -> bool {
        self.hidden_claims.contains_key(key)
    }

    fn stored_tag(&self, key: &WindowKey) -> Option<&str> {
        self.member_tags.get(key).map(String::as_str)
    }

    fn identity_ok(&mut self, key: &WindowKey, expected: &ProcessIdentity) -> bool {
        if key.pid != expected.pid || key.creation != expected.process_creation {
            return false;
        }
        let Some(live) = (self.prims.process_identity)(key.hwnd) else {
            return false;
        };
        live.pid == expected.pid
            && live.process_creation == expected.process_creation
            && live.exe_path == expected.exe_path
            && live.user_sid == expected.user_sid
            && live.session_id == expected.session_id
            && live.user_sid == self.me.user_sid
            && live.session_id == self.me.session_id
    }

    fn lifetime_ok(&mut self, key: &WindowKey) -> bool {
        let stored = self.stored_tag(key).unwrap_or_default().to_owned();
        if stored.is_empty() {
            return false;
        }
        let live = (self.prims.member_tag)(key.hwnd);
        crate::native_guards::visible_lifetime_ok(&stored, live.as_deref())
    }

    /// Shared admission fences for one hide (proof-mode gate, scope,
    /// hosted child, visible lifetime).
    fn admit_gates(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
    ) -> Result<(), AdmitOutcome> {
        if self.allowlist.is_some() && !self.workspace_proof {
            return Err(AdmitOutcome::Refused);
        }
        if !crate::tiling::scope_allows(self.scope, &expected.exe_path) {
            return Err(AdmitOutcome::Refused);
        }
        if !super::inspection::hosted_gate_allows(
            &expected.exe_path,
            key.hwnd,
            expected.pid,
            self.scope_hosts,
        ) {
            return Err(AdmitOutcome::Refused);
        }
        let stored = self.stored_tag(key).unwrap_or_default().to_owned();
        if stored.is_empty() {
            return Err(AdmitOutcome::IdentityChanged);
        }
        let live = (self.prims.member_tag)(key.hwnd);
        if !crate::native_guards::visible_lifetime_ok(&stored, live.as_deref()) {
            return Err(AdmitOutcome::IdentityChanged);
        }
        if let Some(entries) = self.allowlist {
            let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
                return Err(AdmitOutcome::IdentityChanged);
            };
            if super::observe::verify_proof_owned(key.hwnd, entry, self.me).is_err() {
                return Err(AdmitOutcome::IdentityChanged);
            }
        }
        Ok(())
    }

    /// Fresh fullscreen-foreground veto before an unsafe write: a live
    /// fullscreen foreground (or an unreadable one, fail closed) that is not
    /// the write target blocks, exactly like the legacy geometry/focus
    /// fences. Verified managed overlays bypass like a managed maximize.
    fn write_vetoed(&mut self, hwnd: u64) -> bool {
        let fg = (self.prims.foreground)();
        if fg == 0 || fg == hwnd {
            return false;
        }
        match (self.prims.observe)(fg, self.me, self.fulls, &mut self.tokens) {
            Ok(w) => w.facts.captionless_fullscreen,
            Err(_) => true,
        }
    }

    /// Focus-only fullscreen-foreground read with one narrow same-pass
    /// path: when the foreground is the departure this actuator instance
    /// just hid in its nonactivating switch (entry foreground, committed
    /// hide, still invisible), hidden facts are read fresh and run through
    /// the same fullscreen plus focus-eligibility predicate as a visible
    /// read. Anything else unreadable still vetoes, and frame writes keep
    /// the strict [`Self::write_vetoed`] unknown veto.
    ///
    /// Ownership for the same-pass departure rests on the ephemeral record
    /// plus live rechecks, not on any older ledger entry: the hide commit
    /// verified the product nonce against the claim before posting (see the
    /// hide claim path), the record exists only in this `LiveNative`
    /// lifetime, and this lane additionally requires the exact committed
    /// claim shape, full live process identity, the current
    /// visible-membership tag, continued invisibility, and the foreground
    /// still naming the same window before the setter runs.
    fn focus_write_vetoed(&mut self, hwnd: u64) -> bool {
        let fg = (self.prims.foreground)();
        if fg == 0 || fg == hwnd {
            return false;
        }
        if let Ok(w) = (self.prims.observe)(fg, self.me, self.fulls, &mut self.tokens) {
            return w.facts.captionless_fullscreen;
        }
        // Hidden lane, same-pass departure only. A newly visible
        // foreground never rides here (its visible read above decided).
        if (self.prims.is_visible)(fg) {
            return true;
        }
        let Some((dep_key, dep_expected)) = self.na_hidden_departure.clone() else {
            return true;
        };
        if fg != dep_key.hwnd {
            return true;
        }
        let Some(expected) = self.member_identity.get(&dep_key).cloned() else {
            return true;
        };
        if expected != dep_expected {
            return true;
        }
        let Some(record) = self.hidden_claims.get(&dep_key).cloned() else {
            return true;
        };
        if !crate::model::valid_claim_tag(&record.claim.tag)
            || record.claim.hwnd != dep_key.hwnd
            || record.claim.process.pid != dep_key.pid
            || record.claim.process.process_creation != dep_key.creation
        {
            return true;
        }
        if !self.identity_ok(&dep_key, &expected) || !self.lifetime_ok(&dep_key) {
            return true;
        }
        let w = match (self.prims.observe_hidden)(fg, self.me, self.fulls, &mut self.tokens) {
            Ok(w) => w,
            Err(_) => return true,
        };
        if w.hwnd != dep_key.hwnd
            || w.identity.pid != dep_key.pid
            || w.identity.process_creation != dep_key.creation
            || w.identity.exe_path != expected.exe_path
            || w.identity.user_sid != expected.user_sid
            || w.identity.session_id != expected.session_id
        {
            return true;
        }
        if self
            .stored_tag(&dep_key)
            .is_some_and(|stored| w.member_tag.as_deref().is_none_or(|live| live != stored))
        {
            return true;
        }
        if w.facts.captionless_fullscreen {
            return true;
        }
        // Hidden is expected here, so the focus-eligibility predicate runs
        // with visible coerced: cloaked, elevated, shell, tool, owned,
        // dialog, no-activate, or minimized still veto.
        let mut facts = w.facts;
        facts.visible = true;
        if crate::tiling::classify_focus(&facts).is_err() {
            return true;
        }
        // Finish rechecks against a foreign reappearance between the entry
        // read and the hidden read: still invisible, foreground still the
        // same window, identity still live.
        if (self.prims.is_visible)(fg)
            || (self.prims.foreground)() != fg
            || (self.prims.window_pid)(fg) != dep_key.pid
            || !self.identity_ok(&dep_key, &expected)
        {
            return true;
        }
        false
    }

    /// Live move/size veto independent of the hook queue: the target or
    /// foreground thread runs a move/size modal loop, or the probe is
    /// unreadable (fail closed).
    fn move_size_vetoed(&mut self, target: u64) -> bool {
        let fg = (self.prims.foreground)();
        let mut hwnds = [target, fg];
        if hwnds[0] == hwnds[1] {
            hwnds[1] = 0;
        }
        for hwnd in hwnds {
            if hwnd == 0 {
                continue;
            }
            match (self.prims.move_size)(hwnd) {
                Some(false) => continue,
                _ => return true,
            }
        }
        false
    }

    /// Screen-affinity arrival: the live frame sits on the desired wire
    /// output through the mechanical device map. Plain visibility never
    /// proves cross-output arrival.
    fn affinity_of(&self, rect: &Rect) -> Option<String> {
        let device = super::directional_resolution::output_for_rect(&self.areas, rect);
        self.output_tokens.get(&device).cloned()
    }

    /// Expected live visibility for one desired home: sticky members stay
    /// visible everywhere; iconic minimized reads through elsewhere;
    /// otherwise visible exactly when the desired workspace is the
    /// desired view on its output.
    fn expected_visible(&self, key: &WindowKey, desired: &(String, String)) -> bool {
        if self.sticky.contains_key(key) {
            return true;
        }
        self.core_views
            .get(desired.0.as_str())
            .is_some_and(|view| view == &desired.1)
    }

    /// Desired-arrival proof for one token under full identity/lifetime
    /// gates (checked by the caller): the live native state matches the
    /// desired home exactly. Tiled targets require the exact desired
    /// frame on the desired output (a lying setter or a held app size
    /// proves nothing); membership-only targets (no frame entry: floats,
    /// sticky, overlays) prove through visibility plus the proven view;
    /// hidden desires prove through the hide-claim ledger plus continued
    /// invisibility. The bootstrap first binding arrives here: no prior
    /// proven receipt is required, only fresh live evidence equalling
    /// desire. Never an echo: any gap holds unknown.
    fn desired_arrival(&mut self, key: &WindowKey, token: &str) -> Option<(String, String)> {
        let desired = self.core_homing.get(token)?.clone();
        if (self.prims.is_iconic)(key.hwnd) {
            return Some(desired);
        }
        let hidden = self.committed_hidden(key);
        let visible = (self.prims.is_visible)(key.hwnd);
        let expected = self.expected_visible(key, &desired);
        if hidden {
            if visible || expected {
                return None;
            }
            return Some(desired);
        }
        if visible != expected {
            return None;
        }
        if !visible {
            // Unhidden yet invisible with no ledger explanation (cloaked,
            // occluded, minimized omission): fail closed, never proof.
            return None;
        }
        if self.sticky.contains_key(key) {
            return Some(desired);
        }
        if let Some(target) = self.desired_frames.get(token).copied() {
            let live = self.read_frame(token)?;
            if live != target {
                return None;
            }
            if self.affinity_of(&live).as_deref() != Some(desired.0.as_str()) {
                return None;
            }
            return Some(desired);
        }
        // Membership-only tiled arrival (redirect lane, no published
        // frame): the live frame must sit on the desired output AND the
        // proven view must name the workspace. Plain visibility never
        // proves cross-output arrival; floats preserve their frame by
        // design and settle through the membership-only path instead
        // (never here: their affinity names the source).
        let live = self.read_frame(token)?;
        if self.affinity_of(&live).as_deref() != Some(desired.0.as_str()) {
            return None;
        }
        if self.read_view(&desired.0).as_deref() == Some(desired.1.as_str()) {
            return Some(desired);
        }
        None
    }

    /// Visibility-consistent homing: the retained proven homing where
    /// live visibility still agrees with it (hidden iff actually
    /// invisible; iconic minimized reads through), else the desired
    /// arrival proof when the live state already matches the target
    /// (bootstrap first binding and post-effect arrival). Any mismatch
    /// is unknown (hold), never a fabricated receipt. Desired core
    /// targets never prove by echo: only exact fresh evidence proves.
    fn proven_homing(&mut self, key: &WindowKey, token: &str) -> Option<(String, String)> {
        // Prefer the desired arrival proof while a flight is outstanding:
        // after the effect lands, the live state names the target while
        // the retained map still names the source until the caller
        // merges the proven transfer.
        if let Some(desired) = self.core_homing.get(token).cloned()
            && Some(&desired) != self.proven_homing.get(token)
            && let Some(arrived) = self.desired_arrival(key, token)
            && arrived == desired
        {
            return Some(desired);
        }
        let homing = self.proven_homing.get(token)?.clone();
        // Steady-state tiled exactness: a visible proven home with a
        // frame target still requires the exact live frame on the
        // proven output, so external geometry drift holds instead of
        // echoing a stale receipt while the frame phase reconverges.
        // The arrival path above already proved the desired case.
        let hidden = self.committed_hidden(key);
        if (self.prims.is_iconic)(key.hwnd) {
            return Some(homing);
        }
        let visible = (self.prims.is_visible)(key.hwnd);
        if hidden == visible {
            return None;
        }
        if !hidden
            && visible
            && !self.sticky.contains_key(key)
            && let Some(target) = self.desired_frames.get(token).copied()
            && self.core_homing.get(token).is_some_and(|d| d == &homing)
        {
            let live = self.read_frame(token)?;
            if live != target || self.affinity_of(&live).as_deref() != Some(homing.0.as_str()) {
                return None;
            }
        }
        Some(homing)
    }

    /// Commit one hide: admit, post the claim, then record the mechanical
    /// hide claim at commit exactly like the legacy phases. The logical
    /// homing stays core-owned (the next applied reply projects it);
    /// visibility proof afterwards decides the receipt.
    fn commit_hide(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
        iconic: bool,
    ) -> Result<(), AdmitOutcome> {
        self.commit_hide_with(key, expected, iconic, false)
    }

    /// Mode-parameterized hide; `true` takes the nonactivating lane.
    fn commit_hide_with(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
        iconic: bool,
        nonactivating: bool,
    ) -> Result<(), AdmitOutcome> {
        self.admit_gates(key, expected)?;
        if !self.identity_ok(key, expected) {
            return Err(AdmitOutcome::IdentityChanged);
        }
        // Fresh proven frame for the hidden snapshot cache: the
        // exact-lifetime physical frame read before the hide post makes it
        // unreadable. Identity and lifetime already proved above; a failed
        // read keeps the existing fallback (nothing fabricated).
        let seed_frame = (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens)
            .ok()
            .filter(|w| {
                w.hwnd == key.hwnd
                    && w.identity.pid == key.pid
                    && w.identity.process_creation == key.creation
            })
            .map(|w| w.visible);
        let stored = self.stored_tag(key).unwrap_or_default().to_owned();
        let allow_topmost = crate::product_hide::project_topmost_hide_allowed(
            self.float_topmost_prev.get(key).copied(),
            (self.prims.read_topmost)(key.hwnd),
        );
        let claim = (self.prims.admit)(key.hwnd, self.me, expected, &stored, allow_topmost)?;
        let hide = if nonactivating {
            self.prims.hide_claim_na
        } else {
            self.prims.hide_claim
        };
        // Trace-armed sink only: unarmed passes `None` so the native post
        // records nothing and skips the clock. Early vetos above return
        // before any post, so no record is ever inferred.
        let (store, me, dir, hide_fn) = (self.store, self.me, self.dir, hide);
        let sink: Option<&mut VisibilityPostTrace> = if self.trace {
            Some(&mut self.vis_posts)
        } else {
            None
        };
        let committed = hide_fn(store, me, &claim, dir, sink).map_err(|message| {
            if message.starts_with("absent:") {
                AdmitOutcome::OriginVanished
            } else if message.starts_with("uncertain:") {
                AdmitOutcome::Uncertain
            } else {
                AdmitOutcome::Refused
            }
        })?;
        self.hidden_claims.insert(
            key.clone(),
            HiddenRecord {
                claim: committed,
                iconic,
            },
        );
        if let Some(frame) = seed_frame {
            self.hidden_seeds.push((key.clone(), frame));
        }
        Ok(())
    }

    /// Commit one reveal against the stored claim; the claim record leaves
    /// only at commit. A retired (verifiably gone) window still clears its
    /// caches without writes. Logical homing stays core-owned throughout.
    fn commit_reveal(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
    ) -> Result<(), AdmitOutcome> {
        self.commit_reveal_with(key, expected, false)
    }

    /// Mode-parameterized reveal; `true` takes the nonactivating lane.
    fn commit_reveal_with(
        &mut self,
        key: &WindowKey,
        expected: &ProcessIdentity,
        nonactivating: bool,
    ) -> Result<(), AdmitOutcome> {
        let record = self.hidden_claims.get(key).cloned();
        let Some(record) = record else {
            // Nothing hidden: no native effect.
            return Ok(());
        };
        if !self.identity_ok(key, expected) {
            return Err(AdmitOutcome::IdentityChanged);
        }
        if let Some(entries) = self.allowlist {
            if !self.workspace_proof {
                return Err(AdmitOutcome::Refused);
            }
            let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
                return Err(AdmitOutcome::IdentityChanged);
            };
            if super::observe::verify_proof_owned(key.hwnd, entry, self.me).is_err() {
                return Err(AdmitOutcome::IdentityChanged);
            }
        }
        let iconic = record.iconic || (self.prims.is_iconic)(key.hwnd);
        let reveal = if nonactivating {
            self.prims.reveal_claim_na
        } else {
            self.prims.reveal_claim
        };
        let (store, me, reveal_fn) = (self.store, self.me, reveal);
        let sink: Option<&mut VisibilityPostTrace> = if self.trace {
            Some(&mut self.vis_posts)
        } else {
            None
        };
        let outcome = reveal_fn(store, me, &record.claim, iconic, sink);
        match outcome {
            Ok(_) => {
                self.hidden_claims.remove(key);
                Ok(())
            }
            Err(message) => {
                if message.starts_with("absent:") {
                    self.hidden_claims.remove(key);
                    Ok(())
                } else if message.starts_with("uncertain:") {
                    Err(AdmitOutcome::Uncertain)
                } else {
                    Err(AdmitOutcome::Refused)
                }
            }
        }
    }

    /// Retained proven homing for one token, if any. Never the desired core
    /// target.
    fn committed_homing(&self, token: &str) -> Option<(String, String)> {
        self.proven_homing.get(token).cloned()
    }

    /// Non-sticky committed members homed to one output, from the core
    /// projection (inverted per pass).
    fn output_members(&self, output: &str) -> Vec<(String, String, WindowKey)> {
        let mut out = Vec::new();
        for (token, (member_out, ws)) in &self.core_homing {
            if member_out != output {
                continue;
            }
            let Some(key) = self.key_of(token) else {
                continue;
            };
            if self.sticky.contains_key(&key) {
                continue;
            }
            out.push((token.clone(), ws.clone(), key));
        }
        out.sort();
        out
    }

    /// Shared view-batch core; only the hide/reveal lane differs.
    /// Full native view proof still gates the caller exactly as before.
    fn switch_view_inner(
        &mut self,
        output: &str,
        logical: &str,
        nonactivating: bool,
    ) -> Result<bool, ()> {
        if (self.prims.hook_pending)() {
            return Err(());
        }
        if !self
            .core_rings
            .get(output)
            .is_some_and(|ring| ring.iter().any(|id| id == logical))
        {
            return Ok(false);
        }
        if self
            .proven_views
            .get(output)
            .is_some_and(|view| view == logical)
            && self.read_view(output).as_deref() == Some(logical)
        {
            return Ok(true);
        }
        let leaving: Vec<(WindowKey, ProcessIdentity)> = self
            .core_homing
            .iter()
            .filter(|(_, (member_out, ws))| member_out == output && ws != logical)
            .filter_map(|(token, _)| {
                let key = self.key_of(token)?;
                if self.sticky.contains_key(&key) {
                    return None;
                }
                let identity = self.member_identity.get(&key).cloned()?;
                Some((key, identity))
            })
            .collect();
        let entering: Vec<(WindowKey, ProcessIdentity)> = self
            .core_homing
            .iter()
            .filter(|(_, (member_out, ws))| member_out == output && ws == logical)
            .filter_map(|(token, _)| {
                let key = self.key_of(token)?;
                if !self.hidden_claims.contains_key(&key) {
                    return None;
                }
                let identity = self.member_identity.get(&key).cloned()?;
                Some((key, identity))
            })
            .collect();
        let mut ambiguous = false;
        let mut refused = false;
        // Same-pass departure record for the focus lane: only the entry
        // foreground hidden by this nonactivating switch, only on a
        // successful commit that still reads invisible. Pumped switches
        // never record.
        let entry_fg = nonactivating.then(|| (self.prims.foreground)());
        for (key, expected) in &leaving {
            if self.committed_hidden(key) {
                continue;
            }
            let iconic = (self.prims.is_iconic)(key.hwnd);
            match self.commit_hide_with(key, expected, iconic, nonactivating) {
                Ok(()) => {
                    if nonactivating
                        && Some(key.hwnd) == entry_fg
                        && !(self.prims.is_visible)(key.hwnd)
                    {
                        self.na_hidden_departure = Some((key.clone(), expected.clone()));
                    }
                }
                Err(AdmitOutcome::Refused) | Err(AdmitOutcome::OriginVanished) => {
                    refused = true;
                }
                Err(_) => {
                    ambiguous = true;
                }
            }
        }
        for (key, expected) in &entering {
            match self.commit_reveal_with(key, expected, nonactivating) {
                Ok(()) => {}
                Err(AdmitOutcome::Refused) | Err(AdmitOutcome::OriginVanished) => {
                    refused = true;
                }
                Err(_) => {
                    ambiguous = true;
                }
            }
        }
        if ambiguous {
            return Err(());
        }
        if refused {
            return Ok(false);
        }
        Ok(true)
    }

    /// Drain hide-commit snapshot seeds for the owner tick: exact-lifetime
    /// freshly proven frames, oldest first.
    pub(super) fn take_hidden_seeds(&mut self) -> Vec<(WindowKey, Rect)> {
        std::mem::take(&mut self.hidden_seeds)
    }
}

/// Drain hide-commit seeds into the owner snapshot caches. Each seed lands
/// in the tile or float cache under the same lane row assembly uses, keyed
/// by the exact member lifetime: a recycled HWND (different key) matches
/// nothing and seeds nothing. Unreadable-at-hide members carry no seed and
/// keep the existing fallback.
pub(super) fn apply_hidden_seeds(
    seeds: Vec<(WindowKey, Rect)>,
    member_tokens: &BTreeMap<WindowKey, String>,
    member_rects: &mut HashMap<String, Rect>,
    float_rects: &mut HashMap<String, Rect>,
    is_float: impl Fn(&WindowKey) -> bool,
) {
    for (key, frame) in seeds {
        let Some(token) = member_tokens.get(&key) else {
            continue;
        };
        if is_float(&key) {
            float_rects.insert(token.clone(), frame);
        } else {
            member_rects.insert(token.clone(), frame);
        }
    }
}

impl NativeOps for LiveNative<'_> {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        let key = self.key_of(token)?;
        match (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens) {
            Ok(w) => {
                let live_key = WindowKey {
                    hwnd: w.hwnd,
                    pid: w.identity.pid,
                    creation: w.identity.process_creation.clone(),
                };
                // A recycled HWND reports its live identity: the diff stops
                // the pass on the mismatch instead of trusting the frame.
                // Committed homing comes from the core projection; the
                // hidden flag from the hide-claim ledger only.
                let (output, workspace) = self.committed_homing(token).unwrap_or_default();
                let sticky = self.sticky.contains_key(&live_key);
                let float_marked = crate::product_hide::sys::peek_float_intent_raw(key.hwnd);
                let floating =
                    sticky || !matches!(float_marked, crate::native_guards::MarkerSlot::Absent);
                Some(FreshWindow {
                    key: live_key.clone(),
                    token: token.to_owned(),
                    frame: Some(w.visible),
                    observed_frame: Some(w.visible),
                    facts: Some(w.facts),
                    output,
                    workspace,
                    iconic: w.facts.minimized,
                    hidden: self.committed_hidden(&live_key),
                    sticky,
                    floating,
                    // Fit exclusion has no fresh adapter lane: the core never
                    // issues frame placements for excluded rows (Untouched
                    // frame authority), so the row authority already holds.
                    fit_excluded: false,
                    live_tag: (self.prims.member_tag)(key.hwnd),
                })
            }
            Err(_) => {
                // Not freshly observable: the hidden-claim lane or unknown.
                // Unproven lifetimes hold as unknown, never as empty frames.
                // A stale claim association alone never proves: the live
                // lifetime tag must still match and the window must read
                // back invisible. Only committed-hidden members ride here.
                if !self.committed_hidden(&key) {
                    return None;
                }
                if (self.prims.is_visible)(key.hwnd) {
                    return None;
                }
                if !self.lifetime_ok(&key) {
                    return None;
                }
                let record = self.hidden_claims.get(&key)?;
                if !crate::model::valid_claim_tag(&record.claim.tag)
                    || record.claim.hwnd != key.hwnd
                    || record.claim.process.pid != key.pid
                    || record.claim.process.process_creation != key.creation
                {
                    return None;
                }
                let stored = self.stored_tag(&key)?;
                if stored.is_empty() {
                    return None;
                }
                let (output, workspace) = self.committed_homing(token).unwrap_or_default();
                let sticky = self.sticky.contains_key(&key);
                Some(FreshWindow {
                    key,
                    token: token.to_owned(),
                    frame: None,
                    observed_frame: None,
                    facts: None,
                    output,
                    workspace,
                    iconic: (self.prims.is_iconic)(record.claim.hwnd),
                    hidden: true,
                    sticky,
                    floating: sticky,
                    fit_excluded: false,
                    live_tag: (self.prims.member_tag)(record.claim.hwnd),
                })
            }
        }
    }

    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        let key = self.key_of(token)?;
        // Fresh observation only: a hidden-claim snapshot is never a frame
        // proof, so hidden windows read unreadable and hold upstream.
        let w = (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens).ok()?;
        if w.hwnd != key.hwnd
            || w.identity.pid != key.pid
            || w.identity.process_creation != key.creation
        {
            return None;
        }
        Some(w.visible)
    }

    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        let key = self.key_of(token)?;
        // Full identity plus lifetime proof first: a recycled HWND (same
        // number, new creation) or a stale tag never proves homing.
        let expected = self.member_identity.get(&key).cloned()?;
        if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
            return None;
        }
        self.proven_homing(&key, token)
    }

    fn read_view(&mut self, output: &str) -> Option<String> {
        let active_ws = self.core_views.get(output)?.clone();
        // Visibility proof over fresh reads, excluding sticky (visible on
        // every view, so never evidence for one). Every non-sticky member
        // must prove its own side: all active members visible, all inactive
        // members invisible, each under full identity plus lifetime proof
        // plus scope/held authority gates. A partial reveal never proves.
        // The desired id returns only when visibility proves it, never echo.
        let members = self.output_members(output);
        for (_, _, key) in members.iter() {
            let expected = self.member_identity.get(key).cloned()?;
            if !self.identity_ok(key, &expected) || !self.lifetime_ok(key) {
                return None;
            }
            if !crate::tiling::scope_allows(self.scope, &expected.exe_path) {
                return None;
            }
            if !super::inspection::hosted_gate_allows(
                &expected.exe_path,
                key.hwnd,
                expected.pid,
                self.scope_hosts,
            ) {
                return None;
            }
            if self.allowlist.is_some() && !self.workspace_proof {
                return None;
            }
        }
        let active_live: Vec<WindowKey> = members
            .iter()
            .filter(|(_, ws, _)| ws == &active_ws)
            .map(|(_, _, key)| key.clone())
            .collect();
        if active_live.is_empty() {
            // Memberless active proves only through explicit completed batch
            // proof: a prior proven view exists for this output, every
            // proven-homed member on the output is hidden/invisible under
            // exact identity/tag/claim proof, and zero members read visible.
            // Never inferred from the target id over zero windows.
            if !self.proven_views.contains_key(output) {
                return None;
            }
            let mut visible_total = 0usize;
            for (_, _, key) in members.iter() {
                let expected = self.member_identity.get(key).cloned()?;
                if !self.identity_ok(key, &expected) || !self.lifetime_ok(key) {
                    return None;
                }
                if (self.prims.is_visible)(key.hwnd) && !(self.prims.is_iconic)(key.hwnd) {
                    visible_total += 1;
                }
            }
            // Proven-homed members must also all read hidden/invisible.
            let proven_snapshot: Vec<(String, (String, String))> = self
                .proven_homing
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (token, (proven_out, _)) in &proven_snapshot {
                if proven_out != output {
                    continue;
                }
                let key = self.key_of(token)?;
                if self.sticky.contains_key(&key) {
                    continue;
                }
                let expected = self.member_identity.get(&key).cloned()?;
                if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
                    return None;
                }
                if (self.prims.is_visible)(key.hwnd) && !(self.prims.is_iconic)(key.hwnd) {
                    return None;
                }
                if !self.committed_hidden(&key) && !(self.prims.is_iconic)(key.hwnd) {
                    // Proven member still unhidden but invisible: only counts
                    // when the hide ledger or iconic explains it.
                    return None;
                }
            }
            if visible_total == 0 {
                return Some(active_ws);
            }
            return None;
        }
        for key in &active_live {
            let expected = self.member_identity.get(key).cloned()?;
            if !self.identity_ok(key, &expected) || !self.lifetime_ok(key) {
                return None;
            }
            if !(self.prims.is_visible)(key.hwnd) || (self.prims.is_iconic)(key.hwnd) {
                return None;
            }
        }
        for (_, ws, key) in members.iter() {
            if ws == &active_ws {
                continue;
            }
            let expected = self.member_identity.get(key).cloned()?;
            if !self.identity_ok(key, &expected) || !self.lifetime_ok(key) {
                return None;
            }
            if (self.prims.is_visible)(key.hwnd) && !(self.prims.is_iconic)(key.hwnd) {
                return None;
            }
        }
        Some(active_ws)
    }

    fn is_foreground(&mut self, token: &str) -> bool {
        let Some(key) = self.key_of(token) else {
            return false;
        };
        // HWND equality plus full identity and lifetime proof: the effect
        // path already revalidated before dispatching focus, and the receipt
        // never trusts a bare HWND/pid hit across a same-pid creation drift.
        if (self.prims.foreground)() != key.hwnd || (self.prims.window_pid)(key.hwnd) != key.pid {
            return false;
        }
        let Some(expected) = self.member_identity.get(&key).cloned() else {
            return false;
        };
        self.identity_ok(&key, &expected) && self.lifetime_ok(&key)
    }

    fn gesture_barrier(&mut self) -> bool {
        !self.active.is_empty()
    }

    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        let key = self.key_of(token)?;
        (self.prims.dpi)(key.hwnd)
    }

    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        let key = self.key_of(token).ok_or(())?;
        if (self.prims.hook_pending)() {
            return Err(());
        }
        let expected = self.member_identity.get(&key).cloned().ok_or(())?;
        self.frame_write_inner(&key, &expected, rect)
    }

    fn clear_maximized(&mut self, token: &str) -> Result<bool, ()> {
        let key = self.key_of(token).ok_or(())?;
        if (self.prims.hook_pending)() {
            return Err(());
        }
        let expected = self.member_identity.get(&key).cloned().ok_or(())?;
        if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
            return Err(());
        }
        match super::probes::restore_zoom_placement(key.hwnd) {
            "restored" | "dispatched" => Ok(true),
            _ => Err(()),
        }
    }

    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        let key = self.key_of(token).ok_or(())?;
        if (self.prims.hook_pending)() {
            return Err(());
        }
        let expected = self.member_identity.get(&key).cloned().ok_or(())?;
        let _device = self.device_for_output(out).ok_or(())?;
        if !self
            .core_rings
            .get(out)
            .is_some_and(|ring| ring.iter().any(|id| id == ws))
        {
            return Ok(false);
        }
        // Already arrived only when the target proves under live proof
        // (retained re-proof or fresh desired arrival). Desired core
        // targets never short-circuit by echo: incorporation is not
        // native arrival.
        if self
            .read_homing(token)
            .is_some_and(|(o, w)| o == out && w == ws)
        {
            return Ok(true);
        }
        // Sticky members never hide: physical visibility stays unchanged.
        // Still a validated no-effect transaction: full identity, lifetime,
        // scope/hosted gates, and live visibility must all prove. Never a
        // bare setter boolean.
        if self.sticky.contains_key(&key) {
            if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
                return Err(());
            }
            if !crate::tiling::scope_allows(self.scope, &expected.exe_path) {
                return Ok(false);
            }
            if !super::inspection::hosted_gate_allows(
                &expected.exe_path,
                key.hwnd,
                expected.pid,
                self.scope_hosts,
            ) {
                return Ok(false);
            }
            if !(self.prims.is_visible)(key.hwnd) {
                return Err(());
            }
            return Ok(true);
        }
        let target_active = self.core_views.get(out).is_some_and(|view| view == ws);
        if target_active {
            // Reveal path (or plain reassignment when already visible),
            // then the core-owned frame write when published. Cross-output
            // tiled movers and sessionless migration members carry their
            // actionable placement in `frame` (core-translated and
            // work-area constrained); membership-only lanes (floats,
            // sticky, overlays, frameless holds) carry none and preserve
            // the physical frame. Membership alone never proves a
            // cross-output arrival: the frame must physically land on the
            // target output first.
            // Iconic minimized reads through with no placement.
            if self.committed_hidden(&key) {
                let revealed = settle_commit(self.commit_reveal(&key, &expected))?;
                if !revealed {
                    return Ok(false);
                }
            } else if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
                return Err(());
            }
            if (self.prims.is_iconic)(key.hwnd) {
                return Ok(true);
            }
            if let Some(target) = frame {
                return self.frame_write_inner(&key, &expected, target);
            }
        } else {
            // Hide path: the window leaves the visible set. Hidden
            // windows take no placement: the reveal writes the frame.
            let iconic = (self.prims.is_iconic)(key.hwnd);
            return settle_commit(self.commit_hide(&key, &expected, iconic));
        }
        Ok(true)
    }

    fn migration_arrival_proven(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
        floating: bool,
        baseline_frame: Option<Rect>,
    ) -> bool {
        // View-independent member proof for the migration membership
        // phase: the target view flips only after every member proves,
        // so this proof must never wait for it. Tiled arrivals prove
        // through the core-published actuation frame landing on the target
        // output; floats prove through their preserved frame. Identity,
        // lifetime, scope, hosted-child, and visibility gates match the
        // receipt path; the setter boolean is never proof.
        let Some(key) = self.key_of(token) else {
            return false;
        };
        let Some(expected) = self.member_identity.get(&key).cloned() else {
            return false;
        };
        if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
            return false;
        }
        if !crate::tiling::scope_allows(self.scope, &expected.exe_path) {
            return false;
        }
        if !super::inspection::hosted_gate_allows(
            &expected.exe_path,
            key.hwnd,
            expected.pid,
            self.scope_hosts,
        ) {
            return false;
        }
        if self.allowlist.is_some() && !self.workspace_proof {
            return false;
        }
        if (self.prims.is_iconic)(key.hwnd) {
            return true;
        }
        let hidden = self.committed_hidden(&key);
        let visible = (self.prims.is_visible)(key.hwnd);
        let expected_visible = self.expected_visible(&key, &(out.to_owned(), ws.to_owned()));
        if hidden {
            return !visible && !expected_visible;
        }
        if visible != expected_visible || !visible {
            return false;
        }
        if self.sticky.contains_key(&key) {
            return true;
        }
        if floating {
            // Floats preserve their physical frame by design (the
            // membership move carries no placement): the live frame must
            // still equal the observation baseline exactly. Never affinity
            // and never the deferred view.
            return self
                .read_frame(token)
                .is_some_and(|live| Some(live) == baseline_frame);
        }
        let Some(live) = self.read_frame(token) else {
            return false;
        };
        if let Some(target) = frame {
            return live == target && self.affinity_of(&live).as_deref() == Some(out);
        }
        // Membership-only hold (no published frame): the live frame must
        // already sit on the target output with the visibility proof above.
        // The deferred target view is not required. Core publishes
        // placements for cross-output migration members, so this lane only
        // serves holds and same-output proofs, never a guessed placement.
        self.affinity_of(&live).as_deref() == Some(out)
    }

    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.switch_view_inner(output, logical, false)
    }

    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.switch_view_inner(output, logical, true)
    }

    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.focus_window_traced(token).0
    }

    fn take_visibility_posts(&mut self) -> (Vec<VisibilityPostDiag>, usize) {
        self.vis_posts.take()
    }

    fn set_diagnostics_trace(&mut self, trace: bool) {
        self.trace = trace;
    }

    fn diagnostics_trace(&self) -> bool {
        self.trace
    }

    fn focus_window_traced(
        &mut self,
        token: &str,
    ) -> (Result<bool, ()>, super::world_diff::FocusCallDiag) {
        use super::world_diff::{FOCUS_SETTER_API, FocusCallDiag, GuiFocusState};
        // Pre-primitive vetos report no primitive detail: the adapter
        // dispatched, the native setter never ran. Behavior (outcome)
        // matches the previous `focus_window` exactly; only the record is
        // new.
        let vetoed = |outcome: Result<bool, ()>| {
            (
                outcome,
                FocusCallDiag {
                    primitive_called: false,
                    native_return: None,
                    prime_inserted: None,
                    api: None,
                    call_mono_ms: None,
                    ret_mono_ms: None,
                    gui_before: None,
                },
            )
        };
        let Some(key) = self.key_of(token) else {
            return vetoed(Err(()));
        };
        if (self.prims.hook_pending)() {
            return vetoed(Err(()));
        }
        let Some(expected) = self.member_identity.get(&key).cloned() else {
            return vetoed(Err(()));
        };
        let decoded = (self.prims.observe)(key.hwnd, self.me, self.fulls, &mut self.tokens);
        let Ok(w) = decoded else {
            return vetoed(Err(()));
        };
        if w.hwnd != key.hwnd
            || w.identity.pid != key.pid
            || w.identity.process_creation != key.creation
        {
            return vetoed(Err(()));
        }
        if !crate::tiling::scope_allows(self.scope, &w.identity.exe_path) {
            return vetoed(Ok(false));
        }
        if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
            return vetoed(Err(()));
        }
        // Focus carries the overlay exemption (maximized/fullscreen members
        // focus, like the legacy actuation); everything else still refuses.
        if crate::tiling::classify_focus(&w.facts).is_err() {
            return vetoed(Ok(false));
        }
        if self.focus_write_vetoed(key.hwnd) {
            return vetoed(Ok(false));
        }
        if self.move_size_vetoed(key.hwnd) {
            return vetoed(Ok(false));
        }
        // Bounded last-input prime immediately before the single setter:
        // one E8 down/up pair so the owner holds last-input rights when
        // the setter runs. After every veto above, so a vetoed effect
        // never synthesizes input; stamps below stay strictly around the
        // setter. No wait, retry, or queue coupling; injected E8 never
        // dispatches a command (the product filter drops it first).
        let prime_inserted = (self.prims.prime)(FOCUS_PRIME_VK);
        // Trace-gated pre-call GUI read: no native call while trace is off.
        let gui_before = if self.trace {
            match crate::ffi::gui_thread_focus(key.hwnd) {
                Some(focus) => Some(GuiFocusState::Known {
                    thread: focus.thread,
                    active: focus.active,
                    focus: focus.focus,
                }),
                None => Some(GuiFocusState::Unknown),
            }
        } else {
            None
        };
        // Stamps strictly around the primitive, never the preflight above.
        let call_mono_ms = super::trace::log_mono_ms();
        let ok = (self.prims.focus)(key.hwnd);
        let ret_mono_ms = super::trace::log_mono_ms();
        // Full post-setter fences, not just the pid: a same-HWND recycle
        // or a swapped membership tag across the call must read ambiguous
        // (`Err`), never accepted on a stale identity. The async
        // activation itself may still land later; only the fresh
        // independent foreground read proves arrival.
        if (self.prims.window_pid)(key.hwnd) != key.pid
            || !self.identity_ok(&key, &expected)
            || !self.lifetime_ok(&key)
        {
            return (
                Err(()),
                FocusCallDiag {
                    primitive_called: true,
                    native_return: Some(ok),
                    prime_inserted: Some(prime_inserted),
                    api: Some(FOCUS_SETTER_API),
                    call_mono_ms: Some(call_mono_ms),
                    ret_mono_ms: Some(ret_mono_ms),
                    gui_before,
                },
            );
        }
        (
            Ok(ok),
            FocusCallDiag {
                primitive_called: true,
                native_return: Some(ok),
                prime_inserted: Some(prime_inserted),
                api: Some(FOCUS_SETTER_API),
                call_mono_ms: Some(call_mono_ms),
                ret_mono_ms: Some(ret_mono_ms),
                gui_before,
            },
        )
    }

    fn poll_external_invalidation(&mut self) -> bool {
        (self.prims.hook_pending)()
    }

    fn foreground_state(&mut self) -> super::world_diff::ForegroundState {
        use super::world_diff::{ForegroundIdentity, ForegroundState};
        let hwnd = (self.prims.foreground)();
        if hwnd == 0 {
            return ForegroundState::None;
        }
        let pid = (self.prims.window_pid)(hwnd);
        if pid == 0 {
            return ForegroundState::Unknown;
        }
        let ident = match (self.prims.process_identity)(hwnd) {
            Some(ident) => ident,
            None => return ForegroundState::Unknown,
        };
        if ident.pid != pid {
            return ForegroundState::Unknown;
        }
        ForegroundState::Known(ForegroundIdentity {
            hwnd,
            pid,
            creation: ident.process_creation,
        })
    }

    fn foreground_identity_of(&mut self, hwnd: u64) -> super::world_diff::ForegroundState {
        use super::world_diff::{ForegroundIdentity, ForegroundState};
        if hwnd == 0 {
            return ForegroundState::None;
        }
        let pid = (self.prims.window_pid)(hwnd);
        if pid == 0 {
            return ForegroundState::Unknown;
        }
        let ident = match (self.prims.process_identity)(hwnd) {
            Some(ident) => ident,
            None => return ForegroundState::Unknown,
        };
        if ident.pid != pid {
            return ForegroundState::Unknown;
        }
        ForegroundState::Known(ForegroundIdentity {
            hwnd,
            pid,
            creation: ident.process_creation,
        })
    }

    fn gui_focus_of(&mut self, hwnd: u64) -> super::world_diff::GuiFocusState {
        use super::world_diff::GuiFocusState;
        if !self.trace {
            return GuiFocusState::Unknown;
        }
        match crate::ffi::gui_thread_focus(hwnd) {
            Some(focus) => GuiFocusState::Known {
                thread: focus.thread,
                active: focus.active,
                focus: focus.focus,
            },
            None => GuiFocusState::Unknown,
        }
    }

    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        (self.prims.read_cursor)()
    }

    /// Mechanical warp with mechanical-only vetoes: a racing external
    /// invalidation or an out-of-bound core point fails closed (the core
    /// never emits those, so this is backstop, never policy). Game,
    /// lifetime, and focus vetoes are owned upstream: the core pauses
    /// under a focused fullscreen game, and the owner game lane reports
    /// no cursor, so no warp desire can exist there.
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        if (self.prims.hook_pending)() {
            return Err(());
        }
        if !(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND).contains(&x)
            || !(-tiler_core::bounds::GEOMETRY_BOUND..=tiler_core::bounds::GEOMETRY_BOUND)
                .contains(&y)
        {
            return Ok(false);
        }
        Ok((self.prims.warp_cursor)(x, y))
    }
}

impl<'a> LiveNative<'a> {
    /// Changed-homing arrival for one reply row under the same gates as
    /// `desired_arrival`, but against the row's own target instead of the
    /// core projection (the two agree post-delivery; the row additionally
    /// carries the effective frame). Tiled rows require the exact live
    /// frame on the row output; membership-only rows (floating, no
    /// frame) require the proven view to name the row workspace.
    /// Sticky and iconic read through. Never an echo: any gap fails.
    fn arrival_proven_for_row(
        &mut self,
        key: &WindowKey,
        token: &str,
        row: &tiler_core::world_report::WindowDesire,
    ) -> bool {
        if (self.prims.is_iconic)(key.hwnd) {
            return true;
        }
        let hidden = self.committed_hidden(key);
        let visible = (self.prims.is_visible)(key.hwnd);
        let expected = self.expected_visible(key, &(row.output.0.clone(), row.workspace.clone()));
        if hidden {
            return !visible && !expected;
        }
        if visible != expected {
            return false;
        }
        if !visible {
            return false;
        }
        if self.sticky.contains_key(key) {
            return true;
        }
        if !row.floating {
            // Tiled rows: exact frame when published, else affinity plus
            // the proven view. Plain visibility never proves cross-output
            // arrival (membership-only lanes prove through the screen plus
            // the view batch; core publishes placements for cross-output
            // migration members).
            if let Some(target) = row.effective_frame {
                let Some(live) = self.read_frame(token) else {
                    return false;
                };
                return live == target
                    && self.affinity_of(&live).as_deref() == Some(row.output.0.as_str());
            }
            let Some(live) = self.read_frame(token) else {
                return false;
            };
            if self.affinity_of(&live).as_deref() != Some(row.output.0.as_str()) {
                return false;
            }
        }
        self.read_view(&row.output.0).as_deref() == Some(row.workspace.as_str())
    }

    /// Post-apply proven receipt snapshot: desired values only where live
    /// native evidence proves arrival. For each reply row, the desired
    /// homing stores only when identity/lifetime/scope/hosted gates pass and
    /// either the proven prior already names the same home with matching
    /// live visibility (re-proof), or the exact arrival evidence proves a
    /// changed home (tiled: exact target frame on the target output;
    /// membership-only: proven view; hidden: ledger plus invisibility).
    /// For each reply output, the desired view stores only when `read_view`
    /// proves it (full entering/leaving visibility plus gates, explicit
    /// batch proof when empty). Never wholesale desire: unproven entries
    /// stay absent so the caller retains prior receipts.
    pub(super) fn receipt_updates(
        &mut self,
        reply: &tiler_core::world_report::WorldReply,
    ) -> (HashMap<String, (String, String)>, HashMap<String, String>) {
        let mut homing = HashMap::new();
        for row in &reply.windows {
            let token = row.lifetime.0.as_str();
            let Some(key) = self.key_of(token) else {
                continue;
            };
            let Some(expected) = self.member_identity.get(&key).cloned() else {
                continue;
            };
            if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
                continue;
            }
            if !crate::tiling::scope_allows(self.scope, &expected.exe_path) {
                continue;
            }
            if !super::inspection::hosted_gate_allows(
                &expected.exe_path,
                key.hwnd,
                expected.pid,
                self.scope_hosts,
            ) {
                continue;
            }
            if self.allowlist.is_some() && !self.workspace_proof {
                continue;
            }
            // Re-proof plus exact arrival: an unchanged homing re-proves
            // through visibility, while a changed homing proves only
            // through the same exact evidence the transfer effect
            // required (full identity/lifetime/scope gates above plus,
            // for tiled rows, the exact target frame on the target
            // output, else the proven view). Visibility alone never
            // names a changed workspace, so a bare visible match without
            // frame/view proof still holds. The caller merges changed
            // homing from applied-transfer evidence with independent
            // revalidation; this snapshot additionally records arrivals
            // the effect already landed so the next capture echoes them.
            let prior_matches = self
                .proven_homing
                .get(token)
                .is_some_and(|(out, ws)| out == &row.output.0 && ws == &row.workspace);
            if !prior_matches && !self.arrival_proven_for_row(&key, token, row) {
                continue;
            }
            if (self.prims.is_iconic)(key.hwnd) {
                homing.insert(
                    token.to_owned(),
                    (row.output.0.clone(), row.workspace.clone()),
                );
                continue;
            }
            let expected_visible = if self.sticky.contains_key(&key) {
                true
            } else {
                self.core_views
                    .get(row.output.0.as_str())
                    .is_some_and(|view| view == &row.workspace)
            };
            let live_visible = (self.prims.is_visible)(key.hwnd);
            if live_visible == expected_visible {
                homing.insert(
                    token.to_owned(),
                    (row.output.0.clone(), row.workspace.clone()),
                );
            }
        }
        let mut views = HashMap::new();
        for output in &reply.outputs {
            let wire = output.output.0.as_str();
            let Some(view) = output.logical_view.clone() else {
                continue;
            };
            if self.read_view(wire).as_deref() == Some(view.as_str()) {
                views.insert(wire.to_owned(), view);
            }
        }
        (homing, views)
    }
}

#[cfg(test)]
#[path = "world_native_tests.rs"]
mod world_native_tests;
