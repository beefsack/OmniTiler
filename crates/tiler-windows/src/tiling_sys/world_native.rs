#![forbid(unsafe_code)]
//! Production native actuator plus truthful applied-receipt capture (standby).
//!
//! [`LiveNative`] is the real [`NativeOps`](super::world_diff::NativeOps)
//! implementation for the world route: every read is a fresh native
//! visibility/frame/claim/identity re-read, every setter goes through the
//! existing identity/claim-verified mechanisms, and every receipt is proven
//! by a fresh read afterward. The setter boolean is never proof.
//!
//! Standby status: this module compiles against production but no runtime
//! tick or command route constructs it yet; the follow-up cutover unit
//! wires it (single active owner, legacy path deleted). Nothing here runs
//! live effects behind the current tests.
//!
//! Native rules reused, never reinvented:
//! - Hide/reveal run the accepted `product_hide::sys` claim path
//!   (`admit_managed_claim_for_hide` + `hide_managed_claim` /
//!   `reveal_product_claim_to`) with the same admission fences as the
//!   single-window and batch phases (allowlist/proof, scope, hosted child,
//!   visible lifetime, topmost exception). Commit owns the claim before any
//!   post; mechanical caches (`workspaces` assignment, `hidden_claims`)
//!   update only at commit, and visibility proof afterwards decides the
//!   receipt. A mismatch between cache and live visibility reads back as
//!   unknown (hold), never as a fabricated Applied.
//! - Geometry writes convert the desired visible frame to the native outer
//!   frame through the measured insets (`FrameInsets::visible_to_outer`,
//!   the same conversion as the legacy write path) and post one
//!   `SetWindowPos` with `SWP_NOACTIVATE | SWP_NOZORDER`. A recycled HWND
//!   (pid/creation drift across the call) is ambiguous, never success.
//! - Focus is one verified `SetForegroundWindow` with an exact foreground
//!   readback only (no pumped settle: a miss holds and converges next
//!   tick, never a foreign wait on this path).
//! - The core is the sole logical authority. No native workspace property
//!   is written or read as policy: views and membership live in the owner
//!   tables and prove through live visibility plus claim/identity reads.
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
//! Receipt capture: [`observed_view_receipts`] and
//! [`attach_workspace_receipts`] derive the capture's `observed_views` and
//! per-window `observed_workspace` from fresh visibility plus owner tables.
//! Only exactly-one-visible-workspace (non-sticky) views prove, plus the
//! memberless-active-with-zero-visible batch-completion case; an empty
//! workspace never proves by echoing desired state. Unknown stays unknown
//! so the core retains pending flights instead of settling on a guess.

use super::world_capture::OwnerInputs;
use super::world_diff::{FreshWindow, NativeOps};
use super::{
    AllowEntry, HiddenRecord, MonitorArea, ObservedWindow, RetainedRow, ScopeHostChild, TokenMap,
};
use crate::model::{ProcessIdentity, WindowIdentity};
use crate::native::HeldProcess;
use crate::storage::LedgerStore;
use crate::native_guards::WindowKey;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use tiler_core::geometry::Rect;
use tiler_core::world_report::ScreenView;
use windows_sys::Win32::Foundation::HWND;

/// Outcome classes for one hide admission, mirroring
/// `visibility_batch::admit_hide_item` without its `&mut TileLoop` borrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmitOutcome {
    Admitted,
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
        Err(AdmitOutcome::Admitted) => Ok(true),
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
    pub is_zoomed: fn(u64) -> bool,
    pub outer_rect: fn(u64) -> Option<Rect>,
    pub set_outer: fn(u64, Rect) -> bool,
    pub foreground: fn() -> u64,
    pub focus: fn(u64) -> bool,
    pub member_tag: fn(u64) -> Option<String>,
    pub hook_pending: fn() -> bool,
    pub process_identity: fn(u64) -> Option<ProcessIdentity>,
    pub read_topmost: fn(u64) -> bool,
    pub admit: fn(
        u64,
        &ProcessIdentity,
        &ProcessIdentity,
        &str,
        bool,
    ) -> Result<WindowIdentity, AdmitOutcome>,
    pub hide_claim: fn(
        &LedgerStore,
        &ProcessIdentity,
        &WindowIdentity,
        &Path,
    ) -> Result<WindowIdentity, String>,
    pub reveal_claim:
        fn(&LedgerStore, &ProcessIdentity, &WindowIdentity, bool) -> Result<Revealed, String>,
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

fn real_window_pid(hwnd: u64) -> u32 {
    crate::ffi::window_pid(hwnd)
}

fn real_is_visible(hwnd: u64) -> bool {
    super::pending_seams::live_is_visible(hwnd)
}

fn real_is_iconic(hwnd: u64) -> bool {
    super::pending_seams::live_is_iconic(hwnd)
}

fn real_is_zoomed(hwnd: u64) -> bool {
    crate::ffi::is_zoomed(hwnd)
}

fn real_outer_rect(hwnd: u64) -> Option<Rect> {
    let raw = crate::ffi::window_rect(hwnd)?;
    super::shell::rect_from_win(raw)
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

fn real_member_tag(hwnd: u64) -> Option<String> {
    super::pending_seams::live_member_tag(hwnd)
}

fn real_hook_pending() -> bool {
    super::hook_edges_pending()
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
) -> Result<WindowIdentity, String> {
    crate::product_hide::sys::hide_managed_claim(store, owner, claim, dir)
        .map_err(|e| e.to_string())
}

fn real_reveal_claim(
    store: &LedgerStore,
    owner: &ProcessIdentity,
    claim: &WindowIdentity,
    minimized: bool,
) -> Result<Revealed, String> {
    match crate::product_hide::sys::reveal_product_claim_to(store, owner, claim, minimized)
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
            is_zoomed: real_is_zoomed,
            outer_rect: real_outer_rect,
            set_outer: real_set_outer,
            foreground: real_foreground,
            focus: real_focus,
            member_tag: real_member_tag,
            hook_pending: real_hook_pending,
            process_identity: real_process_identity,
            read_topmost: real_read_topmost,
            admit: real_admit,
            hide_claim: real_hide_claim,
            reveal_claim: real_reveal_claim,
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
    pub member_tokens: &'a BTreeMap<WindowKey, String>,
    pub member_tags: &'a BTreeMap<WindowKey, String>,
    pub member_identity: &'a BTreeMap<WindowKey, ProcessIdentity>,
    pub hidden_claims: &'a mut BTreeMap<WindowKey, HiddenRecord>,
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
            member_tokens,
            member_tags,
            member_identity,
            hidden_claims,
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
            desired_frames,
            areas,
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

    /// Shared admission fences for one hide, mirroring
    /// `visibility_batch::admit_hide_item` without its `&mut TileLoop`
    /// borrow: proof-mode gate, scope, hosted child, visible lifetime.
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

    fn fresh_visible(&mut self, hwnd: u64) -> Option<ObservedWindow> {
        (self.prims.observe)(hwnd, self.me, self.fulls, &mut self.tokens).ok()
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
            if live != target
                || self.affinity_of(&live).as_deref() != Some(homing.0.as_str())
            {
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
        self.admit_gates(key, expected)?;
        if !self.identity_ok(key, expected) {
            return Err(AdmitOutcome::IdentityChanged);
        }
        let stored = self.stored_tag(key).unwrap_or_default().to_owned();
        let allow_topmost = crate::product_hide::project_topmost_hide_allowed(
            self.float_topmost_prev.get(key).copied(),
            (self.prims.read_topmost)(key.hwnd),
        );
        let claim = (self.prims.admit)(key.hwnd, self.me, expected, &stored, allow_topmost)?;
        let committed =
            (self.prims.hide_claim)(self.store, self.me, &claim, self.dir).map_err(|message| {
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
        match (self.prims.reveal_claim)(self.store, self.me, &record.claim, iconic) {
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

    /// Mechanical cross-output translation for tiled arrivals with no
    /// published frame (redirect-lane movers): the live visible frame
    /// shifted by the FULL-monitor delta onto the target output,
    /// position and size preserved, never resized or refit. The sweep
    /// re-tiles to the exact slot after arrival proves. `None` when
    /// already on the target or when either monitor is unknown: hold,
    /// never invent placement.
    fn remap_to_output(&self, live: Rect, target_out: &str) -> Option<Rect> {
        let source_device = super::directional_resolution::output_for_rect(&self.areas, &live);
        if self.output_tokens.get(&source_device).map(String::as_str) == Some(target_out) {
            return None;
        }
        let source_full = self
            .areas
            .iter()
            .find(|area| area.device == source_device)
            .map(|area| area.full)?;
        let target_device = self.device_for_output(target_out)?;
        let target_full = self
            .areas
            .iter()
            .find(|area| area.device == target_device)
            .map(|area| area.full)?;
        Some(Rect {
            x: live.x - source_full.x + target_full.x,
            y: live.y - source_full.y + target_full.y,
            w: live.w,
            h: live.h,
        })
    }

    /// Shared host-mechanical frame write: fresh eligibility, scope,
    /// hosted-child fence, full identity, and lifetime tag. Row-local
    /// drift refuses without writes (`Ok(false)`); torn state across the
    /// call is ambiguous (`Err`). The setter boolean is never proof: the
    /// caller re-reads the exact frame before claiming arrival.
    fn frame_write_inner(
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
        if !super::inspection::hosted_gate_allows(
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
        let Some(outer) = w.insets.visible_to_outer(rect) else {
            return Ok(false);
        };
        let ok = (self.prims.set_outer)(key.hwnd, outer);
        if (self.prims.window_pid)(key.hwnd) != key.pid {
            return Err(());
        }
        Ok(ok)
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

    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        let key = self.key_of(token).ok_or(())?;
        if (self.prims.hook_pending)() {
            return Err(());
        }
        let expected = self.member_identity.get(&key).cloned().ok_or(())?;
        self.frame_write_inner(&key, &expected, rect)
    }

    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
        remap: bool,
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
        if self.read_homing(token).is_some_and(|(o, w)| o == out && w == ws) {
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
            // then the tiled frame write: the caller's desired visible
            // frame when published, else a mechanical remap onto the
            // target output for redirect-lane movers (position preserved,
            // never resized: the sweep re-tiles after arrival proves).
            // Membership alone never proves a cross-output arrival: the
            // frame must physically land on the target output first.
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
            if remap
                && let Some(live) = self.read_frame(token)
                && let Some(translated) = self.remap_to_output(live, out)
            {
                return self.frame_write_inner(&key, &expected, translated);
            }
        } else {
            // Hide path: the window leaves the visible set. Hidden
            // windows take no placement: the reveal writes the frame.
            let iconic = (self.prims.is_iconic)(key.hwnd);
            return settle_commit(self.commit_hide(&key, &expected, iconic));
        }
        Ok(true)
    }

    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
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
        // Act whenever the last proven view differs from the request, even
        // though the core already desires the target. Desired core views
        // never short-circuit: incorporation is not native arrival.
        if self
            .proven_views
            .get(output)
            .is_some_and(|view| view == logical)
            && self.read_view(output).as_deref() == Some(logical)
        {
            return Ok(true);
        }
        // Batch completion with evidence for all prior/next members: hide
        // every leaving non-sticky member that still reads visible, reveal
        // every entering hidden member. Logical views stay core-owned: the
        // adapter performs only native hide/reveal effects and proves the
        // flip afterwards through `read_view`. Any uncertain/throwing leg
        // makes the batch ambiguous (never a partial Applied); explicit
        // refusals hold dirty.
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
        for (key, expected) in &leaving {
            if self.committed_hidden(key) {
                continue;
            }
            let iconic = (self.prims.is_iconic)(key.hwnd);
            match self.commit_hide(key, expected, iconic) {
                Ok(()) => {}
                Err(AdmitOutcome::Refused) | Err(AdmitOutcome::OriginVanished) => {
                    refused = true;
                }
                Err(_) => {
                    ambiguous = true;
                }
            }
        }
        for (key, expected) in &entering {
            match self.commit_reveal(key, expected) {
                Ok(()) => {}
                Err(AdmitOutcome::Refused) | Err(AdmitOutcome::OriginVanished) => {
                    refused = true;
                }
                Err(_) => {
                    ambiguous = true;
                }
            }
        }
        // No local flip: the core owns the view and `read_view` visibility
        // evidence proves it on the next pass. A torn batch never claims its
        // view here.
        if ambiguous {
            return Err(());
        }
        if refused {
            return Ok(false);
        }
        Ok(true)
    }

    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        let key = self.key_of(token).ok_or(())?;
        if (self.prims.hook_pending)() {
            return Err(());
        }
        let expected = self.member_identity.get(&key).cloned().ok_or(())?;
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
        if !self.identity_ok(&key, &expected) || !self.lifetime_ok(&key) {
            return Err(());
        }
        // Focus carries the overlay exemption (maximized/fullscreen members
        // focus, like the legacy actuation); everything else still refuses.
        if crate::tiling::classify_focus(&w.facts).is_err() {
            return Ok(false);
        }
        if self.write_vetoed(key.hwnd) {
            return Ok(false);
        }
        let ok = (self.prims.focus)(key.hwnd);
        if (self.prims.window_pid)(key.hwnd) != key.pid {
            return Err(());
        }
        Ok(ok)
    }

    fn poll_external_invalidation(&mut self) -> bool {
        (self.prims.hook_pending)()
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
        let expected = self.expected_visible(
            key,
            &(row.output.0.clone(), row.workspace.clone()),
        );
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
            // arrival (the redirect lane publishes no frame, so the
            // remapped frame's screen plus the view batch is the proof).
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
    ) -> (
        HashMap<String, (String, String)>,
        HashMap<String, String>,
    ) {
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
            // homing from applied transfer feedback with independent
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

/// Per-window applied-membership receipt for capture: the stable logical
/// workspace only where fresh identity/visibility proves it. Visible
/// windows prove through a matching live observed row plus a stamped
/// lifetime tag plus screen-affinity arrival on the applied output (plain
/// visibility never proves cross-output arrival); hidden windows prove
/// through the claim association (well formed tag, claim identity equals
/// the member key, tag present) plus continued invisibility. Anything else
/// is unknown (`None`), never a proof of absence. Hidden, fullscreen,
/// minimized, and unreadable truth preserved; omission never a close.
pub(super) fn attach_workspace_receipts(
    inputs: &OwnerInputs<'_>,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[super::MonitorArea],
    facts: &mut [tiler_core::world_report::WindowFact],
) {
    let by_token: HashMap<&str, &WindowKey> = inputs
        .member_tokens
        .iter()
        .map(|(key, token)| (token.as_str(), key))
        .collect();
    let affinity_of = |rect: &tiler_core::geometry::Rect| -> Option<String> {
        let device = super::directional_resolution::output_for_rect(areas, rect);
        inputs.output_tokens.get(&device).cloned()
    };
    for fact in facts.iter_mut() {
        let Some(key) = by_token.get(fact.window.0.as_str()).copied() else {
            fact.observed_workspace = None;
            continue;
        };
        // Retained proven homing: receipts prove this commitment with fresh
        // evidence, never a policy-table read and never core desire.
        let Some((applied_out, applied_ws)) = inputs.applied_homing.get(fact.window.0.as_str())
        else {
            fact.observed_workspace = None;
            continue;
        };
        let hidden = inputs.hidden_claims.contains_key(key);
        let tag_ok = inputs
            .member_tags
            .get(key)
            .is_some_and(|tag| !tag.is_empty());
        if !tag_ok {
            fact.observed_workspace = None;
            continue;
        }
        if hidden {
            let proven = inputs.hidden_claims.get(key).is_some_and(|record| {
                crate::model::valid_claim_tag(&record.claim.tag)
                    && record.claim.hwnd == key.hwnd
                    && record.claim.process.pid == key.pid
                    && record.claim.process.process_creation == key.creation
            });
            // External reveal or HWND-recycle drift (any live observation of
            // the number while logically hidden) proves nothing: unknown,
            // never the stale homing. Same-pid creation drift is drift too,
            // so the check is HWND-only, never identity-gated.
            let drifted = observed.iter().any(|w| w.hwnd == key.hwnd);
            fact.observed_workspace = if proven && !drifted {
                Some(applied_ws.clone())
            } else {
                None
            };
            continue;
        }
        // Visible lane: a matching live observed row with full identity plus
        // a stamped lifetime tag plus affinity on the applied output, or a
        // retained row with a fresh frame on the applied output (overlay
        // members ride retained rows). Floating logical crossings preserve
        // the physical frame, so floating facts skip the affinity check:
        // the identity plus preserved visibility proves the logical
        // receipt while the physical screen stays put (the core homes
        // floating receipts on their owning screen). Stale,
        // tag-mismatched, token-mismatched, or wrong-output rows prove
        // nothing.
        let stored_tag = inputs.member_tags.get(key).cloned().unwrap_or_default();
        let expected = inputs.member_identity.get(key).cloned();
        let live_observed = observed.iter().any(|w| {
            w.hwnd == key.hwnd
                && w.identity.pid == key.pid
                && w.identity.process_creation == key.creation
                && w.token == fact.window.0
                && !w.identity.tag.is_empty()
                && w.identity.tag == stored_tag
                && expected.as_ref().is_some_and(|ident| {
                    w.identity.exe_path == ident.exe_path
                        && w.identity.user_sid == ident.user_sid
                        && w.identity.session_id == ident.session_id
                })
                && (fact.floating
                    || affinity_of(&w.visible).as_deref() == Some(applied_out.as_str()))
        });
        let live_retained = !live_observed
            && retained.iter().any(|row| {
                row.key == *key
                    && row.token == fact.window.0
                    && row.rect.is_some_and(|rect| {
                        affinity_of(&rect).as_deref() == Some(applied_out.as_str())
                    })
            });
        fact.observed_workspace = if live_observed || live_retained {
            Some(applied_ws.clone())
        } else {
            None
        };
    }
}

/// Per-screen applied-view receipts: exactly one non-sticky applied
/// workspace with fresh visible members proves the applied view; a
/// memberless applied view with zero visible members on the output proves
/// batch completion only with explicit prior proof (never inferred over
/// zero windows). Sticky members ride every view and are never evidence.
/// Unknown stays unknown: the core retains pending flights rather than
/// settling on an echo of desire. Proven against the adapter-retained
/// last applied views with native observation revalidation, never a
/// policy-table read and never core desire.
pub(super) fn observed_view_receipts(
    inputs: &OwnerInputs<'_>,
    observed: &[ObservedWindow],
    areas: &[super::MonitorArea],
) -> Vec<ScreenView> {
    // Visible members grouped by (output, applied workspace), gated on
    // screen-affinity arrival: plain visibility never proves cross-output
    // arrival, and minimized/unreadable rows never count.
    let mut visible: HashSet<(String, String)> = HashSet::new();
    for w in observed {
        if w.facts.minimized {
            continue;
        }
        let key = WindowKey {
            hwnd: w.hwnd,
            pid: w.identity.pid,
            creation: w.identity.process_creation.clone(),
        };
        if inputs.sticky.contains_key(&key) {
            continue;
        }
        let Some(token) = inputs.member_tokens.get(&key) else {
            continue;
        };
        if token != &w.token {
            continue;
        }
        // Full creation+tag plus identity proof: the observed row must carry
        // the stamped lifetime tag and the expected process identity, so a
        // same-HWND recycle never proves the prior view.
        let Some(stored) = inputs.member_tags.get(&key) else {
            continue;
        };
        if stored.is_empty() || w.identity.tag != *stored {
            continue;
        }
        let Some(expected) = inputs.member_identity.get(&key) else {
            continue;
        };
        if w.identity.exe_path != expected.exe_path
            || w.identity.user_sid != expected.user_sid
            || w.identity.session_id != expected.session_id
        {
            continue;
        }
        let Some((applied_out, applied_ws)) = inputs.applied_homing.get(token.as_str()) else {
            continue;
        };
        // Screen-affinity arrival: the live frame must sit on the applied
        // output, or a cross-output visible window would prove the wrong view.
        let device = super::directional_resolution::output_for_rect(areas, &w.visible);
        if inputs.output_tokens.get(&device) != Some(applied_out) {
            continue;
        }
        visible.insert((applied_out.clone(), applied_ws.clone()));
    }
    let mut out = Vec::new();
    for (output, active) in inputs.applied_views {
        let shown: Vec<&(String, String)> =
            visible.iter().filter(|(dev, _)| dev == output).collect();
        if shown.is_empty() {
            // Memberless-applied batch completion only: no visible member on
            // the whole output while the applied view holds nothing.
            let holds_anything = inputs
                .applied_homing
                .values()
                .any(|(dev, _)| dev == output);
            if !holds_anything {
                out.push(ScreenView {
                    output: tiler_core::directional::OutputId(output.clone()),
                    view: active.clone(),
                });
            }
            continue;
        }
        if shown.len() == 1 && shown[0].1 == *active {
            out.push(ScreenView {
                output: tiler_core::directional::OutputId(output.clone()),
                view: active.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
#[path = "world_native_tests.rs"]
mod world_native_tests;
