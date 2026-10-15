#![forbid(unsafe_code)]

//! Native-Space diagnostics (read-only) plus owned move/restore probes.
//!
//! Reads use private SkyLight symbols resolved dynamically at runtime (see
//! [`crate::ffi`]): managed displays with per-display Space ID lists, the
//! current Space per display, and per-window membership via the pinned yabai
//! `SLSCopySpacesForWindows(cid, 0x7, [wid])` shape. Owned identity comes
//! from the public CGWindowList snapshot ([`crate::cg`]), never AX. A missing
//! symbol is `Unsupported`, never a guessed signature or injection. Reported
//! data is IDs and counts only.
//!
//! Write route determination (source-derived surveyed route, experimental
//! live standing — see below):
//! - move: Tier-2 direct SLS routes exist in surveyed source (async bridged
//!   op, direct `SLSMoveWindowsToManagedSpace`, compat-ID fallback); the
//!   probe omits SA entirely and executes direct-only (compat disabled: no
//!   exact restoration; bridged detected, never executed), with preimage
//!   capture, effect readback and a separate independent restore command.
//!   Upstream bypasses the direct leg on macOS 12.7+/13.6+/14.5+/15+ via
//!   `workspace_use_macos_space_workaround` (SA first, then compat-ID), so
//!   the direct leg is source-derived and experimental live: one gated
//!   dispatch on an owned fixture observed no effect. Target and preimage
//!   are additionally gated to user Spaces (read-only `SLSSpaceGetType`):
//!   fullscreen/system Spaces have no proven restore and fail closed.
//! - switch/create/destroy/reorder/display-move as SINGLE-SHOT commands
//!   (`spaces-switch`, `spaces-create`, `spaces-destroy`): no implemented
//!   single-shot route in surveyed source. Native legs need Dock
//!   injection (scripting-addition); surveyed non-SA legs (synthetic
//!   gestures, AX Mission Control automation, system-chord resolve-and-post)
//!   are unimplemented as single-shot commands here
//!   (global synthetic input or visible UI automation). These single-shot
//!   commands stay explicitly unsupported with the route and reason reported
//!   instead of executed — use the bounded gated probes below, which carry
//!   their own per-run readbacks. Missing implementation/recovery is never
//!   a SIP rejection: no SIP rejection is asserted without live evidence.
//!
//! Bounded gated probe batteries exist SEPARATELY and are not single-shot
//! execution: `spaces-switch-probe` (same-display arrow-vs-gesture
//! comparison: gesture legs read back the current both ways, the arrow
//! session pair observed no effect), `spaces-owned-recovery` (public O->T->O
//! recovery through the held-Child AppKit self-action, proven per run,
//! optional `--probe-direct` gated direct-SLS leg), `mc-lifecycle` /
//! `mc-recover` (bounded temp-Space lifecycle/recovery: AX Mission Control
//! create/goto/remove proven on one temporary same-display Space; the
//! integrated `mc-lifecycle` run is NOT passed — its first run failed on the
//! immediate goto, and only the separate corrected recovery+remove passed),
//! `mc-members` / `mc-inspect` (read-only), `mc-restore-frontmost` (guarded
//! Ghostty-only frontmost restoration), and `spaces-bindings` (read-only
//! hotkey table). Their per-run readbacks are the only effect evidence. No
//! product-workspace claim is made here, and no universal native window-move
//! failure is asserted: the single-shot direct move is experimental
//! (observed no-effect on one owned fixture) with its single-shot inverse
//! unverified.

use serde::{Deserialize, Serialize};

pub use crate::ffi::SlsDisplay;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpacesDiagnosis {
    pub connection: Option<i32>,
    pub displays: Vec<SlsDisplay>,
}

impl SpacesDiagnosis {
    pub fn space_count(&self) -> usize {
        self.displays.iter().map(|d| d.spaces.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpacesError {
    Unsupported(String),
    Ownership(String),
    Blocked(String),
}

impl std::fmt::Display for SpacesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::Ownership(r) => write!(f, "ownership refused: {r}"),
            Self::Blocked(r) => write!(f, "blocked: {r}"),
        }
    }
}

impl std::error::Error for SpacesError {}

/// Read-only diagnostics: managed displays, Space membership per display,
/// and the current Space per display. Never mutates displays, Spaces, or
/// focus.
pub fn diagnose() -> Result<SpacesDiagnosis, SpacesError> {
    let connection = crate::ffi::sls_main_connection_id();
    let displays = crate::ffi::sls_display_spaces().map_err(SpacesError::Unsupported)?;
    Ok(SpacesDiagnosis {
        connection,
        displays,
    })
}

/// Read-only per-window Space membership for one window ID, via the
/// pinned yabai `SLSCopySpacesForWindows(cid, 0x7, [wid])` shape (see
/// [`crate::ffi`]). IDs only, bounded to a single window, never a write.
/// A missing symbol, null result, or malformed container is `Unsupported`
/// with the cause — never fabricated membership. Live behavior is
/// UNVERIFIED on a Mac (no live run in this unit); treat `Ok` from CI-less
/// paths with the same suspicion as any unmeasured SLS read.
pub fn window_spaces(wid: u32) -> Result<Vec<u64>, SpacesError> {
    if wid == 0 {
        return Err(SpacesError::Ownership("refusing window id 0".to_owned()));
    }
    crate::ffi::sls_spaces_for_windows(&[wid]).map_err(SpacesError::Unsupported)
}

/// Resolve the live bundle ID for a PID and require the owned one. Caller
/// claims are not proof; the running-application table is. Directly spawned
/// helpers have no LaunchServices bundle entry: when no bundle is live the
/// spawn record's executable-path plus liveness check (done before this call)
/// plus the fresh CG owner-PID join at use carry ownership, so a missing
/// bundle with a live PID defers to the record instead of failing.
fn live_owned_bundle(target_pid: u32) -> Result<String, SpacesError> {
    let pid_i32: i32 = target_pid
        .try_into()
        .map_err(|_| SpacesError::Ownership(format!("pid {target_pid} out of range")))?;
    match crate::ffi::bundle_id_for_pid(pid_i32).filter(|b| !b.is_empty()) {
        Some(bundle) => Ok(bundle),
        None if crate::ffi::process_exists(pid_i32) => Ok(crate::BUNDLE_ID.to_owned()),
        None => Err(SpacesError::Ownership(format!(
            "no live bundle for pid {target_pid} (not running or no bundle)"
        ))),
    }
}

/// Read-only per-window Space membership for one OWNED window: the owned
/// session is authenticated (spawn record + live bundle), then the WID is
/// joined to the owned PID through a fresh CGWindowList snapshot (no AX)
/// before the SLS membership read. A failed join is `Ownership`, never a
/// guess.
pub fn window_spaces_owned(
    owned: &crate::ownership::OwnedIdentity,
    pid: u32,
    wid: u32,
) -> Result<Vec<u64>, SpacesError> {
    if pid <= 1 {
        return Err(SpacesError::Ownership(format!(
            "refusing reserved pid {pid}"
        )));
    }
    let live_bundle = live_owned_bundle(pid)?;
    owned
        .check_action(pid, &live_bundle)
        .map_err(SpacesError::Ownership)?;
    crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| match e {
        crate::cg::CgError::Ownership(r) => SpacesError::Ownership(r),
        crate::cg::CgError::Unsupported(r) => SpacesError::Unsupported(r),
    })?;
    window_spaces(wid)
}

/// Measured outcome of one owned Space move or independent restore. The API
/// dispatch (`route` + `symbols`) is reported separately from the effect
/// (`effect_spaces` + `effect_verified`): the direct SLS call returns void,
/// so dispatch is never effect evidence. `preimage_spaces` plus the CG
/// geometry/visibility baseline (`cg_baseline`) always carry the restorable
/// state even when the effect is unverified, so `spaces-restore` can run
/// afterwards as a separate command (including after the moving process
/// exits, via the spawn record). Independent restore is NOT considered
/// proven until a live test demonstrates it; the commands additionally
/// require the explicit `--allow-untested` prerequisite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceMoveReport {
    /// `move` or `restore`.
    pub purpose: String,
    pub pid: u32,
    pub wid: u32,
    pub preimage_spaces: Vec<u64>,
    pub cg_baseline: Option<crate::ffi::CgWindow>,
    pub target_space: u64,
    pub route: String,
    pub symbols: crate::ffi::SlsMoveSymbols,
    pub effect_spaces: Vec<u64>,
    pub effect_error: Option<String>,
    pub effect_verified: bool,
}

/// Guarded owned-window Space move. Direct-`SLSMoveWindowsToManagedSpace`
/// probe only (compat route disabled: no exact restoration; SA omitted):
/// authenticate the full spawn record (pid/nonce, expected helper, live
/// bundle-or-exe plus CURRENT live window nonce — never
/// `OwnedIdentity` claims alone), join the WID to the owned PID via
/// CGWindowList with a live recheck (no AX), capture the membership preimage
/// (exactly one Space required) plus the CG geometry/visibility baseline,
/// validate the target against enumerated Spaces, require the explicit
/// `--allow-untested` prerequisite, revalidate the record unchanged
/// immediately before the write and again before the effect readback, then
/// dispatch and read back the effect. Never auto-restores: restoration is the
/// separate `restore_owned_window_to_space` command so the independent
/// restore path is proven on its own (live proof still pending).
pub fn move_owned_window_to_space(
    owned: &crate::ownership::OwnedIdentity,
    pid: u32,
    nonce: &str,
    wid: u32,
    target: u64,
    allow_untested: bool,
) -> Result<SpaceMoveReport, SpacesError> {
    move_owned_window_core(owned, pid, nonce, wid, target, "move", allow_untested)
}

/// Independent restore: moves the owned window back to `space` with the same
/// full record validation as the move (entry proof, CG join with live
/// recheck, unchanged-record revalidation at the write boundary and before
/// the readback). Usable as a separate command after the moving process
/// exits; a reused PID or WID never authorizes another window. Live proof
/// still pending.
pub fn restore_owned_window_to_space(
    owned: &crate::ownership::OwnedIdentity,
    pid: u32,
    nonce: &str,
    wid: u32,
    space: u64,
    allow_untested: bool,
) -> Result<SpaceMoveReport, SpacesError> {
    move_owned_window_core(owned, pid, nonce, wid, space, "restore", allow_untested)
}

/// Pure user-type gate for the Tier-2 move/restore path. Both the target
/// and the single-Space preimage must be user Spaces (type 0 per the
/// pinned yabai `SLSSpaceGetType` ids): fullscreen (4) and system (2)
/// Spaces have no proven independent restore, so they fail closed here as
/// `Blocked`, never executed. A missing guard is not a SIP rejection.
fn require_user_space(
    kind: crate::ffi::SlsSpaceType,
    sid: u64,
    role: &str,
) -> Result<(), SpacesError> {
    match kind {
        crate::ffi::SlsSpaceType::User => Ok(()),
        other => Err(SpacesError::Blocked(format!(
            "{role} Space {sid} is not a user Space ({other:?}); refusing without a proven restore — this is not a SIP failure"
        ))),
    }
}

/// Locate the display holding one Space ID in a diagnosis snapshot. A SID
/// on more than one display, or on none, fails closed with a named reason —
/// never executed, never a SIP rejection.
fn display_of(
    displays: &[crate::ffi::SlsDisplay],
    sid: u64,
    role: &str,
) -> Result<usize, SpacesError> {
    let mut at = None;
    for (i, display) in displays.iter().enumerate() {
        if display.spaces.contains(&sid) {
            if at.is_some() {
                return Err(SpacesError::Blocked(format!(
                    "{role} Space {sid} maps to more than one display; refusing without an unambiguous mapping — this is not a SIP failure"
                )));
            }
            at = Some(i);
        }
    }
    at.ok_or_else(|| {
        SpacesError::Blocked(format!(
            "{role} Space {sid} maps to no known display; refusing without a display mapping — this is not a SIP failure"
        ))
    })
}

/// Pure same-display gate for the Tier-2 move/restore path. The initial
/// live plan covers same-display recovery only: a cross-display target has
/// no proven restore here, so it fails closed as `Blocked` before any
/// write. Ambiguous or missing display mappings fail closed too.
fn require_same_display(
    displays: &[crate::ffi::SlsDisplay],
    source: u64,
    target: u64,
) -> Result<(), SpacesError> {
    let from = display_of(displays, source, "preimage")?;
    let to = display_of(displays, target, "target")?;
    if from != to {
        return Err(SpacesError::Blocked(format!(
            "cross-display move {source} -> {target} refused: only same-display recovery is in plan — this is not a SIP failure"
        )));
    }
    Ok(())
}

/// Actual lock plus frontmost gate for the Tier-2 move/restore path. The
/// window-server session verdict comes first, fresh from
/// `CGSessionCopyCurrentDictionary` ([`crate::ffi::session_state`]): an
/// unreadable session (`Unknown`) or a locked one (`Locked`) fails closed
/// as `Blocked` before any write, regardless of what the frontmost heuristic
/// says. The frontmost rules below are retained unchanged: an unreadable
/// frontmost (`None`) and the loginwindow bundle (lock screen/login context)
/// fail closed as `Blocked`. Same refusal shape as the focus route's
/// lockscreen firewall; no lockscreen recovery is attempted, and a missing
/// guard is never a SIP rejection.
fn require_unlocked_session(
    lock: crate::ffi::SessionLock,
    frontmost: Option<(i32, String)>,
    owned_pid: u32,
) -> Result<(), SpacesError> {
    match lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err(SpacesError::Blocked(
                "window-server session is locked (CGSessionCopyCurrentDictionary); refusing without an unlocked desktop — this is not a SIP failure"
                    .to_owned(),
            ));
        }
        crate::ffi::SessionLock::Unknown => {
            return Err(SpacesError::Blocked(
                "window-server session lock state unreadable (unknown lock state); refusing without a proven session — this is not a SIP failure"
                    .to_owned(),
            ));
        }
    }
    match frontmost {
        None => Err(SpacesError::Blocked(
            "frontmost session unreadable (unknown lock state); refusing without a proven session — this is not a SIP failure"
                .to_owned(),
        )),
        Some((_, bundle)) if crate::focus::is_loginwindow_bundle(&bundle) => {
            Err(SpacesError::Blocked(
                "frontmost is com.apple.loginwindow (lock screen/login context); refusing — no lockscreen recovery attempted"
                    .to_owned(),
            ))
        }
        Some((pid, bundle)) if pid <= 1 || (bundle.is_empty() && pid as u32 != owned_pid) => Err(SpacesError::Blocked(
            "frontmost identity incomplete (unknown lock state); refusing — this is not a SIP failure"
                .to_owned(),
        )),
        // A directly spawned, freshly authenticated owned helper can lack
        // a LaunchServices bundle ID. Its verified PID is sufficient here;
        // an unidentified frontmost process still fails closed.
        Some(_) => Ok(()),
    }
}

fn move_owned_window_core(
    owned: &crate::ownership::OwnedIdentity,
    pid: u32,
    nonce: &str,
    wid: u32,
    target: u64,
    purpose: &str,
    allow_untested: bool,
) -> Result<SpaceMoveReport, SpacesError> {
    if pid <= 1 {
        return Err(SpacesError::Ownership(format!(
            "refusing reserved pid {pid}"
        )));
    }
    if wid == 0 {
        return Err(SpacesError::Ownership("refusing window id 0".to_owned()));
    }
    // Explicit prerequisite gate before any live touch: the direct route is
    // source-derived and experimental (one gated dispatch observed no
    // effect), and the single-shot inverse is unverified — the separate
    // public held-Child recovery is proven per run, never assumed. A
    // separate restore command existing is not proof; the operator must opt
    // in per invocation.
    if !allow_untested {
        return Err(SpacesError::Blocked(format!(
            "{purpose} requires --allow-untested (direct SLS route experimental, observed no-effect on one owned fixture; single-shot inverse unverified)"
        )));
    }
    // Full spawn-record proof at core entry (pid/nonce/exe plus CURRENT live
    // window nonce) — never
    // `OwnedIdentity` claims alone — plus the live-bundle ownership gate.
    let original =
        crate::probe_target::load_verified_record(pid, nonce).map_err(SpacesError::Ownership)?;
    let live_bundle = live_owned_bundle(pid)?;
    owned
        .check_action(pid, &live_bundle)
        .map_err(SpacesError::Ownership)?;
    // Exact PID association with a live recheck before any write (no AX);
    // the entry doubles as the CG geometry/visibility baseline for restore.
    let cg_baseline = crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| match e {
        crate::cg::CgError::Ownership(r) => SpacesError::Ownership(r),
        crate::cg::CgError::Unsupported(r) => SpacesError::Unsupported(r),
    })?;
    // Target must be a known Space, and a second Space must exist at all. A
    // missing second Space (e.g. the last observed one-display/one-Space
    // topology) is an explicit topology block, never mislabeled as a SIP
    // failure. No SIP rejection is asserted anywhere without live evidence.
    let diagnosis = diagnose()?;
    let known: Vec<u64> = diagnosis
        .displays
        .iter()
        .flat_map(|d| d.spaces.iter().copied())
        .collect();
    if known.len() < 2 {
        return Err(SpacesError::Blocked(format!(
            "only {} Space known; no second Space exists for a {purpose} — this is not a SIP failure",
            known.len()
        )));
    }
    if !known.contains(&target) {
        return Err(SpacesError::Blocked(format!(
            "unknown target Space {target} ({} known Spaces); refusing — this is not a SIP failure",
            known.len()
        )));
    }
    // User-type guard (read-only `SLSSpaceGetType`, pinned yabai ids): the
    // target must be a user Space. Fullscreen/system targets have no proven
    // independent restore; an unreadable type fails closed. Never a SIP claim.
    let target_kind = crate::ffi::sls_space_type(target).map_err(SpacesError::Unsupported)?;
    require_user_space(target_kind, target, "target")?;
    // Restorable preimage before any write: exactly one Space. An unreadable
    // membership fails closed, and a multi-Space (sticky) membership fails
    // closed too — restoring one Space out of several is not a proven
    // inverse, so no unrestorable move is attempted.
    let preimage = window_spaces(wid)?;
    if preimage.len() != 1 {
        return Err(SpacesError::Blocked(format!(
            "wid {wid} membership is not a single restorable Space ({} reported); refusing {purpose} without an exact preimage",
            preimage.len()
        )));
    }
    // The preimage Space must itself be a user Space: restoring to a
    // fullscreen/system Space is unproven, so a non-user preimage fails
    // closed before any write (unreadable type fails closed too).
    let preimage_kind =
        crate::ffi::sls_space_type(preimage[0]).map_err(SpacesError::Unsupported)?;
    require_user_space(preimage_kind, preimage[0], "preimage")?;
    if preimage.contains(&target) {
        return Err(SpacesError::Blocked(format!(
            "wid {wid} already on Space {target}; no-op {purpose} refused"
        )));
    }
    // Session-state gate before any write: the fresh window-server lock
    // verdict (`CGSessionCopyCurrentDictionary`) plus the read-only
    // frontmost query (no new private API). An unknown/locked session or an
    // unknown or loginwindow frontmost refuses here. Placed
    // after the ownership/topology refusals so those keep priority and stay
    // deterministic on every platform; this gate only fires once a session
    // is otherwise provable.
    require_unlocked_session(
        crate::ffi::session_state().lock,
        crate::ffi::frontmost_app(),
        pid,
    )?;
    // Same-display gate on the live diagnosis snapshot: the initial plan
    // covers same-display recovery only, so a cross-display (or
    // ambiguously/unmapped) target fails closed before any write.
    require_same_display(&diagnosis.displays, preimage[0], target)?;
    // Direct-only route. The compat pair (when present) stays diagnostics:
    // `SLSSpaceSetCompatID` mutates global Space metadata with only a setter
    // covered locally, no exact preimage, and a yabai-colliding tag — reset
    // to zero on that baseline is not restoration, so compat is blocked, not
    // attempted. The bridged async op is detected, never executed.
    let symbols = crate::ffi::sls_move_symbols();
    if !symbols.direct {
        return Err(SpacesError::Unsupported(
            "SLSMoveWindowsToManagedSpace unavailable; compat route blocked (no exact restoration); refusing rather than guessing SLS writes"
                .to_owned(),
        ));
    }
    // Revalidate the record unchanged immediately before the private write:
    // compare entry snapshot to current instead of refreshing the baseline.
    if let Err(e) = crate::probe_target::verify_record_unchanged(&original, pid, nonce) {
        return Err(SpacesError::Ownership(e));
    }
    crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| match e {
        crate::cg::CgError::Ownership(r) => SpacesError::Ownership(r),
        crate::cg::CgError::Unsupported(r) => SpacesError::Unsupported(r),
    })?;
    // Recheck the session state immediately before the direct writer, after
    // the fresh identity validation above: the lock/frontmost session may
    // have changed since the earlier gate. This narrows the race but is not
    // an atomic lock detector — no atomicity is claimed.
    require_unlocked_session(
        crate::ffi::session_state().lock,
        crate::ffi::frontmost_app(),
        pid,
    )?;
    if let Err(e) = crate::ffi::sls_move_windows_direct(target, &[wid]) {
        return Err(SpacesError::Unsupported(e));
    }
    // After dispatch every path preserves the preimage: revalidation or
    // readback failures are recorded in the report (with the restore hint
    // carried by the caller), never `?`-dropped. Direct returns void, so
    // this readback is the only effect evidence.
    let mut effect_error: Option<String> = None;
    if let Err(e) = crate::probe_target::verify_record_unchanged(&original, pid, nonce) {
        effect_error = Some(format!("post-dispatch record changed; effect suspect: {e}"));
    }
    if effect_error.is_none()
        && let Err(e) = crate::cg::verify_wid_owned_by_pid(pid, wid)
    {
        effect_error = Some(format!("post-dispatch window identity changed: {e}"));
    }
    let (effect_spaces, read_error) = if effect_error.is_some() {
        (Vec::new(), None)
    } else {
        match window_spaces(wid) {
            Ok(spaces) => (spaces, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        }
    };
    if effect_error.is_none()
        && let Some(e) = read_error
    {
        effect_error = Some(e);
    }
    // Exact equality to the one target Space: merely still containing the
    // target while also still on the preimage is NOT success.
    let effect_verified =
        effect_error.is_none() && effect_spaces.len() == 1 && effect_spaces[0] == target;
    Ok(SpaceMoveReport {
        purpose: purpose.to_owned(),
        pid,
        wid,
        preimage_spaces: preimage,
        cg_baseline: Some(cg_baseline),
        target_space: target,
        route: "direct".to_owned(),
        symbols,
        effect_spaces,
        effect_error,
        effect_verified,
    })
}

/// Source-derived route determination for operations that stay explicitly
/// unsupported (no implemented route with proven recovery). Pure (touches
/// no live state): `supported` is always false in Phase 0 and
/// `live_evidence` is always none — this is source-only determination,
/// never a live claim, and never a SIP rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceRouteStatus {
    pub action: String,
    pub supported: bool,
    pub route: String,
    pub reason: String,
    pub source: String,
    pub live_evidence: String,
}

fn untested() -> String {
    "none — source-only route determination, untested live".to_owned()
}

/// Native Space switch (single-shot). Distinguish switch from focus (source-only):
/// `space_manager_switch_space` swaps windows cross-display via the window
/// move route and is SA-only same-display, while `space_manager_focus_space`
/// tries SA first and falls back to a synthesized Dock-swipe gesture path
/// with cursor warp plus click (`space_manager_focus_space_using_gesture`).
/// Surveyed non-SA alternatives are Hammerspoon's AX Mission Control
/// `gotoSpace` (visible UI automation) and the enabled system Space-switch
/// chords (resolve-and-post per Silica `118+space-1`, inherited by Amethyst).
/// Every single-shot leg is unimplemented here — the
/// swap leg additionally inherits the move route's experimental standing —
/// so the single-shot switch stays unsupported with the route reported. The
/// bounded `spaces-switch-probe` comparison exists separately with its own
/// gates and per-run readbacks (gesture legs read back the current both
/// ways; the arrow session pair observed no effect); it is not single-shot
/// switch execution. Missing implementation is never a
/// SIP rejection: no SIP rejection is asserted without live evidence.
pub fn switch_space_status(space: u64) -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "switch".to_owned(),
        supported: false,
        route: format!(
            "switch({space}): cross-display swaps windows via the move route, same-display is SA focus only; focus alternative is SA first else synthesized Dock-swipe gesture per index step (cursor warp + click on cross-display); surveyed non-SA alternatives are AX Mission Control gotoSpace and enabled system Space-switch chords (Silica 118+space-1 resolve-and-post, inherited by Amethyst), unimplemented as single-shot commands here (bounded spaces-switch-probe comparison exists separately: gesture legs read back the current both ways, the arrow session pair observed no effect)"
        ),
        reason: "single-shot switch/focus legs need Dock injection (scripting-addition), global synthetic gestures with cursor warp/click, visible AX Mission Control automation, or posted system chords; no single-shot leg is implemented here, so this probe refuses rather than injects, synthesizes input, or automates UI — use the bounded spaces-switch-probe instead, which carries its own per-run readbacks; missing implementation, not a SIP rejection"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035 + https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L754-L801 + https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SISystemWideElement.m#L29-L65".to_owned(),
        live_evidence: untested(),
    }
}

/// Native Space creation (single-shot). No implemented single-shot route
/// here: the yabai path is scripting-addition-gated, and the only
/// surveyed non-SA alternative is Hammerspoon's Mission Control + AX
/// `addSpaceToScreen` (visible global UI automation, unimplemented as a
/// single-shot command here). The bounded
/// `mc-lifecycle` temp-Space battery exists separately with its own gates
/// and per-run readbacks; it is not single-shot creation.
pub fn create_space_status() -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "create".to_owned(),
        supported: false,
        route: "yabai scripting_addition_create_space only (no direct SLS create in surveyed source); surveyed non-SA alternative is Hammerspoon Mission Control + AX addSpaceToScreen (visible UI automation, not implemented as a single-shot command here; the bounded mc-lifecycle battery covers the temp-Space create leg separately)".to_owned(),
        reason: "single-shot creation has no implemented route here: the native path needs Dock injection (scripting-addition) and the surveyed AX Mission Control alternative is visible global UI automation, unimplemented single-shot here — use the bounded mc-lifecycle battery instead, which carries its own per-run readbacks; missing implementation, not a SIP rejection"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1062-L1072 + https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L689-L738".to_owned(),
        live_evidence: untested(),
    }
}

/// Native Space destruction (single-shot). No implemented single-shot route
/// here: the yabai path is scripting-addition-gated
/// (destroying the last user Space is additionally refused upstream), and
/// the only surveyed non-SA alternative is Hammerspoon's Mission Control +
/// AX `removeSpace` (visible global UI automation, unimplemented as a
/// single-shot command here). The bounded
/// `mc-lifecycle` / `mc-recover` temp-Space removal legs exist separately
/// with their own gates and per-run readbacks; they are not single-shot
/// destruction.
pub fn destroy_space_status(space: u64) -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "destroy".to_owned(),
        supported: false,
        route: format!(
            "yabai scripting_addition_destroy_space({space}) only (no direct SLS destroy in surveyed source; last-user-Space destroy refused upstream); surveyed non-SA alternative is Hammerspoon Mission Control + AX removeSpace (visible UI automation, not implemented as a single-shot command here; the bounded mc-lifecycle/mc-recover batteries cover temp removal separately)"
        ),
        reason: "single-shot destruction has no implemented route here: the native path needs Dock injection (scripting-addition) and the surveyed AX Mission Control alternative is visible global UI automation, unimplemented single-shot here — use the bounded mc-lifecycle/mc-recover batteries instead, which carry their own per-run readbacks; missing implementation, not a SIP rejection; refuses rather than injects"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1060 + https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L821-L882".to_owned(),
        live_evidence: untested(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::OwnedIdentity;

    fn owned() -> OwnedIdentity {
        OwnedIdentity::new(4242, crate::BUNDLE_ID, "0123456789abcdef").unwrap()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn diagnose_returns_ids_not_content() {
        // Real read where available; tolerance where the session has no
        // window server (headless): Ok requires well-formed IDs, Err names
        // the cause. Neither path fabricates data.
        match diagnose() {
            Ok(d) => {
                assert!(
                    d.displays.iter().map(|x| x.spaces.len()).sum::<usize>() == d.space_count()
                );
                for display in &d.displays {
                    if let Some(current) = display.current {
                        assert!(display.spaces.contains(&current));
                    }
                }
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn diagnose_is_unsupported_off_macos() {
        assert!(matches!(
            diagnose().unwrap_err(),
            SpacesError::Unsupported(_)
        ));
    }

    #[test]
    fn move_refuses_foreign_bundle_first() {
        let id = owned();
        let err =
            move_owned_window_to_space(&id, 4242, "0123456789abcdef", 7, 3, true).unwrap_err();
        // No spawn record exists for this PID on any platform, so the full
        // record proof fails closed before any window-server touch.
        assert!(matches!(err, SpacesError::Ownership(_)));
    }

    #[test]
    fn move_refuses_reserved_ids_before_os_touch() {
        let id = owned();
        assert!(matches!(
            move_owned_window_to_space(&id, 1, "0123456789abcdef", 7, 3, true).unwrap_err(),
            SpacesError::Ownership(_)
        ));
        assert!(matches!(
            move_owned_window_to_space(&id, 4242, "0123456789abcdef", 0, 3, true).unwrap_err(),
            SpacesError::Ownership(_)
        ));
    }

    #[test]
    fn move_requires_explicit_untested_prerequisite() {
        // The prerequisite gate fires before any live touch: a separate
        // restore command existing is not proof, so the operator opts in.
        let id = owned();
        let err =
            move_owned_window_to_space(&id, 4242, "0123456789abcdef", 7, 3, false).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(err.to_string().contains("--allow-untested"));
        let err =
            restore_owned_window_to_space(&id, 4242, "0123456789abcdef", 7, 3, false).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
    }

    #[test]
    fn restore_validates_identity_every_write() {
        // Same mandatory validation as the move, including restore: a forged
        // session (no spawn record, no live bundle) is refused.
        let id = owned();
        assert!(matches!(
            restore_owned_window_to_space(&id, 4242, "0123456789abcdef", 7, 3, true).unwrap_err(),
            SpacesError::Ownership(_)
        ));
    }

    #[test]
    fn owned_membership_validates_before_os_membership() {
        let id = owned();
        assert!(matches!(
            window_spaces_owned(&id, 4242, 7).unwrap_err(),
            SpacesError::Ownership(_) | SpacesError::Unsupported(_)
        ));
        assert!(matches!(
            window_spaces_owned(&id, 4242, 0).unwrap_err(),
            SpacesError::Ownership(_)
        ));
    }

    #[test]
    fn membership_rejects_empty_id_before_os_touch() {
        assert!(matches!(
            window_spaces(0).unwrap_err(),
            SpacesError::Ownership(_)
        ));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn membership_is_unsupported_off_macos() {
        assert!(matches!(
            window_spaces(7).unwrap_err(),
            SpacesError::Unsupported(_)
        ));
    }

    #[test]
    fn user_type_gate_refuses_non_user_spaces_without_sip_claim() {
        use crate::ffi::SlsSpaceType;
        assert!(require_user_space(SlsSpaceType::User, 3, "target").is_ok());
        for (kind, role) in [
            (SlsSpaceType::Fullscreen, "target"),
            (SlsSpaceType::System, "target"),
            (SlsSpaceType::Other(9), "target"),
            (SlsSpaceType::Fullscreen, "preimage"),
            (SlsSpaceType::System, "preimage"),
        ] {
            let err = require_user_space(kind, 3, role).unwrap_err();
            assert!(matches!(err, SpacesError::Blocked(_)));
            let msg = err.to_string();
            assert!(msg.contains("not a user Space"), "unexpected: {msg}");
            assert!(
                msg.contains("not a SIP failure"),
                "guard refusal must not read as a SIP rejection: {msg}"
            );
            assert!(msg.contains(role), "role must be named: {msg}");
        }
    }

    #[test]
    fn lock_gate_refuses_unknown_and_loginwindow_before_write() {
        use crate::ffi::SessionLock;
        // Pure gate, no live write: the fresh window-server lock verdict
        // leads — `Locked`/`Unknown` refuse even with a healthy frontmost —
        // and the retained frontmost rules (`None`, loginwindow, incomplete)
        // fail closed as `Blocked`, never a SIP claim.
        for lock in [SessionLock::Locked, SessionLock::Unknown] {
            let err =
                require_unlocked_session(lock, Some((7, "com.apple.finder".to_owned())), 4242)
                    .unwrap_err();
            assert!(matches!(err, SpacesError::Blocked(_)));
            assert!(err.to_string().contains("not a SIP failure"));
        }
        let err = require_unlocked_session(SessionLock::Unlocked, None, 4242).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(err.to_string().contains("unknown lock state"));
        assert!(err.to_string().contains("not a SIP failure"));

        let err = require_unlocked_session(
            SessionLock::Unlocked,
            Some((1, "com.apple.loginwindow".to_owned())),
            4242,
        )
        .unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(err.to_string().contains("loginwindow"));

        for frontmost in [(4343, String::new()), (0, "com.apple.finder".to_owned())] {
            let err =
                require_unlocked_session(SessionLock::Unlocked, Some(frontmost), 4242).unwrap_err();
            assert!(matches!(err, SpacesError::Blocked(_)));
            assert!(err.to_string().contains("identity incomplete"));
        }

        assert!(
            require_unlocked_session(SessionLock::Unlocked, Some((4242, String::new())), 4242)
                .is_ok()
        );
        assert!(
            require_unlocked_session(
                SessionLock::Unlocked,
                Some((4242, crate::BUNDLE_ID.to_owned())),
                4242
            )
            .is_ok()
        );
        assert!(
            require_unlocked_session(
                SessionLock::Unlocked,
                Some((7, "com.apple.finder".to_owned())),
                4242
            )
            .is_ok()
        );
    }

    fn test_display(spaces: &[u64]) -> crate::ffi::SlsDisplay {
        crate::ffi::SlsDisplay {
            uuid: None,
            spaces: spaces.to_vec(),
            current: spaces.first().copied(),
        }
    }

    #[test]
    fn same_display_gate_accepts_refuses_and_names_mapping_failures() {
        // Same display accepted.
        let two = vec![test_display(&[11, 12]), test_display(&[21, 22])];
        assert!(require_same_display(&two, 11, 12).is_ok());

        // Other display refused with a named cross-display reason, never a
        // SIP claim.
        let err = require_same_display(&two, 11, 21).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        let msg = err.to_string();
        assert!(msg.contains("cross-display"), "unexpected: {msg}");
        assert!(msg.contains("not a SIP failure"), "unexpected: {msg}");

        // Ambiguous mapping (SID on two displays) refused with a named
        // reason on either role.
        let dup = vec![test_display(&[11, 12]), test_display(&[12, 22])];
        let err = require_same_display(&dup, 12, 22).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(
            err.to_string().contains("more than one display"),
            "unexpected: {err}"
        );

        // Missing mapping refused with a named reason on either role.
        let err = require_same_display(&two, 99, 12).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(
            err.to_string().contains("no known display"),
            "unexpected: {err}"
        );
        let err = require_same_display(&two, 11, 99).unwrap_err();
        assert!(matches!(err, SpacesError::Blocked(_)));
        assert!(
            err.to_string().contains("no known display"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn switch_create_destroy_are_explicitly_unsupported_with_sa_reason() {
        let switch = switch_space_status(3);
        assert!(!switch.supported);
        assert!(switch.reason.contains("scripting-addition"));
        // Switch vs focus distinguished: the swap leg goes via the move
        // route, never claimed SA-only.
        assert!(switch.route.contains("move route"));
        assert!(!switch.route.contains("SA-only"));
        // Non-SA alternatives named: AX Mission Control goto and enabled
        // system chords; refusal is missing recovery, never a SIP rejection.
        assert!(switch.route.contains("gotoSpace"));
        assert!(switch.route.contains("system Space-switch chords"));
        assert!(switch.reason.contains("Mission Control"));
        assert!(switch.reason.contains("not a SIP rejection"));
        assert!(
            switch
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
        );
        assert!(
            switch
                .source
                .contains("23e387e2805a9890066366e0ac96c71b27f0cfd5")
        );
        assert!(
            switch
                .source
                .contains("ebead82a28486e7719b7ed69633ecfed8a4fe2d2")
        );
        assert!(switch.live_evidence.contains("untested"));

        let create = create_space_status();
        assert!(!create.supported);
        assert!(create.reason.contains("scripting-addition"));
        assert!(!create.route.contains("SA-only"));
        // Non-SA alternative named (Hammerspoon AX Mission Control), and the
        // refusal is missing implementation/recovery, never a SIP rejection.
        assert!(create.route.contains("Hammerspoon"));
        assert!(create.reason.contains("Mission Control"));
        assert!(create.reason.contains("not a SIP rejection"));
        assert!(
            create
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
        );
        assert!(
            create
                .source
                .contains("23e387e2805a9890066366e0ac96c71b27f0cfd5")
        );

        let destroy = destroy_space_status(3);
        assert!(!destroy.supported);
        assert!(destroy.reason.contains("scripting-addition"));
        assert!(!destroy.route.contains("SA-only"));
        assert!(destroy.route.contains("Hammerspoon"));
        assert!(destroy.reason.contains("Mission Control"));
        assert!(destroy.reason.contains("not a SIP rejection"));
        assert!(
            destroy
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
        );
        assert!(
            destroy
                .source
                .contains("23e387e2805a9890066366e0ac96c71b27f0cfd5")
        );
    }

    #[test]
    fn blocked_displays_distinctly_from_unsupported() {
        assert!(
            SpacesError::Blocked("x".to_owned())
                .to_string()
                .starts_with("blocked:")
        );
        assert!(
            SpacesError::Unsupported("x".to_owned())
                .to_string()
                .starts_with("unsupported:")
        );
    }
}
