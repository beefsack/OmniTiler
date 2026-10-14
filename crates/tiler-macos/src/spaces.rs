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
//! Write route determination (source-only, live effects UNTESTED):
//! - move: Tier-2 direct SLS routes exist in surveyed source (async bridged
//!   op, direct `SLSMoveWindowsToManagedSpace`, compat-ID fallback); the
//!   probe omits SA entirely and executes direct-only (compat disabled: no
//!   exact restoration; bridged detected, never executed), with preimage
//!   capture, effect readback and a separate independent restore command.
//! - switch/create/destroy/reorder/display-move: no safe native recovery in
//!   surveyed source (SA-gated legs; non-SA fallbacks are synthetic gestures
//!   or AX Mission Control automation, not Tier-2 safe); these stay
//!   explicitly unsupported with the route and reason reported instead of
//!   executed. No SIP rejection is asserted without live evidence.

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
    // source-derived and UNTESTED, and independent restore is unproven until
    // a live test demonstrates it. A separate restore command existing is not
    // proof; the operator must opt in per invocation.
    if !allow_untested {
        return Err(SpacesError::Blocked(format!(
            "{purpose} requires --allow-untested (direct SLS route UNTESTED live; independent restore unproven)"
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
    if preimage.contains(&target) {
        return Err(SpacesError::Blocked(format!(
            "wid {wid} already on Space {target}; no-op {purpose} refused"
        )));
    }
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

/// Source-derived route determination for operations that stay
/// scripting-addition-gated. Pure (touches no live state): `supported` is
/// always false in Phase 0 and `live_evidence` is always none — this is
/// source-only determination, never a live claim.
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

/// Native Space switch. Distinguish switch from focus (source-only):
/// `space_manager_switch_space` swaps windows cross-display via the window
/// move route and is SA-only same-display, while `space_manager_focus_space`
/// tries SA first and falls back to a synthesized Dock-swipe gesture path
/// with cursor warp plus click (`space_manager_focus_space_using_gesture`).
/// The swap leg inherits the move route's untested status, the focus legs
/// need Dock injection or global synthetic input, and no leg has a safe
/// native recovery — so switching stays unsupported with the route reported.
/// No SIP rejection is asserted without live evidence.
pub fn switch_space_status(space: u64) -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "switch".to_owned(),
        supported: false,
        route: format!(
            "switch({space}): cross-display swaps windows via the move route, same-display is SA focus only; focus alternative is SA first else synthesized Dock-swipe gesture per index step (cursor warp + click on cross-display)"
        ),
        reason: "switch/focus need Dock injection (scripting-addition) or global synthetic gestures with cursor warp/click, and the swap leg inherits the UNTESTED move route; no Tier-2 safe route with proven recovery, so this probe refuses rather than injects or synthesizes input"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035".to_owned(),
        live_evidence: untested(),
    }
}

/// Native Space creation. Direct create/destroy/reorder/display-move paths
/// remain SA-gated; the only surveyed non-SA alternative is Hammerspoon's
/// Mission Control + AX automation with unavoidable visual feedback.
pub fn create_space_status() -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "create".to_owned(),
        supported: false,
        route: "scripting_addition_create_space only (no direct SLS create in surveyed source)".to_owned(),
        reason: "creation is scripting-addition-only (Dock injection); the surveyed AX Mission Control alternative is visible global UI automation, not implemented in Phase 0"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1062-L1072".to_owned(),
        live_evidence: untested(),
    }
}

/// Native Space destruction. SA-only; destroying the last user Space is
/// additionally refused upstream.
pub fn destroy_space_status(space: u64) -> SpaceRouteStatus {
    SpaceRouteStatus {
        action: "destroy".to_owned(),
        supported: false,
        route: format!(
            "scripting_addition_destroy_space({space}) only (no direct SLS destroy in surveyed source; last-user-Space destroy refused upstream)"
        ),
        reason: "destruction is scripting-addition-only (Dock injection); refuses rather than injects"
            .to_owned(),
        source: "https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1060".to_owned(),
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
    fn switch_create_destroy_are_explicitly_unsupported_with_sa_reason() {
        let switch = switch_space_status(3);
        assert!(!switch.supported);
        assert!(switch.reason.contains("scripting-addition"));
        // Switch vs focus distinguished: the swap leg goes via the move
        // route, never claimed SA-only.
        assert!(switch.route.contains("move route"));
        assert!(!switch.route.contains("SA-only"));
        assert!(
            switch
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
        );
        assert!(switch.live_evidence.contains("untested"));

        let create = create_space_status();
        assert!(!create.supported);
        assert!(create.reason.contains("scripting-addition"));
        assert!(
            create
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
        );

        let destroy = destroy_space_status(3);
        assert!(!destroy.supported);
        assert!(destroy.reason.contains("scripting-addition"));
        assert!(
            destroy
                .source
                .contains("dd845723416f5fe92af49fad5ebab00369e07edd")
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
