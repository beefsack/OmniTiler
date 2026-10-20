#![forbid(unsafe_code)]

//! Scoped native tile backend: per-app AX owner with pre-effect fences and
//! honest partial-write truth.
//!
//! The [`ScopedAxFactory`] implements [`crate::runtime_backend::AxBackendFactory`]:
//! it binds one launch lifetime per observed PID (bundle plus executable
//! plus `launchDate` — a missing launch date refuses, never falls back) and
//! creates the [`ScopedAxBackend`] that runs on the owning per-app AX
//! thread. The backend reads with [`crate::native_facts::read_app_window_facts`]
//! plus [`classify_all`](crate::native_facts::NativeReadOutcome::classify_all)
//! (whole-snapshot refusal on the first gap — an incomplete read never
//! publishes as a complete replacement), classifies through the existing
//! [`crate::classifier`], attaches per-window Space membership, and actuates
//! through fenced setters. Observer notifications trigger rereads (same
//! [`crate::ffi::Phase1Observer`] shape as the Phase 1 backend).
//!
//! Safety contract (every AX setter and every focus dispatch):
//! 1. Revalidate the bound [`crate::scope::ProcessIdentity`] (bundle plus
//!    executable plus launch date: PID reuse cannot authorize a new
//!    process; a missing launch date refuses outright).
//! 2. Revalidate the product selection at the effect boundary (scoped:
//!    Ghostty and the own bundle never authorize, even when listed;
//!    unscoped: both own bundles and exact exclusions never authorize while
//!    Ghostty authorizes as an ordinary candidate; unknown bundles never
//!    authorize in either mode). Direct-spawn probe-target helpers carry no LaunchServices
//!    bundle entry and therefore cannot bind here; the signed bundled helper
//!    (stable helper bundle ID, LaunchServices entry plus launch date) binds
//!    under an explicit helper-bundle scope.
//! 3. Revalidate the CG owner join (`wid` still owned by exactly `pid`).
//! 4. Revalidate exact current-USER membership (`spaces == [current]` and the
//!    current Space reads user type 0): sticky, other-Space, unknown, and
//!    non-user currents refuse with no writes to any Space. Membership is
//!    always read live — never inferred from geometry.
//! 5. Revalidate the current display topology revision: a mid-plan display
//!    or Space change supersedes the batch (stop-fenced, never applied).
//! 6. Recheck fullscreen/minimized/hidden immediately before effects:
//!    minimized, hidden-app, and fullscreen-overlay windows take no geometry
//!    writes (omission, never destruction; no hides, no restyling, no Space
//!    writes, no geometry restoration on stop/crash).
//!
//! TOCTOU cannot be atomic: these are honest narrow checks at each boundary,
//! never a guarantee that races are impossible.
//!
//! Frame order is SIZE→POSITION→SIZE (Silica `SIAccessibilityElement`
//! `setFrame`: "we set the size before and after setting the position
//! because the accessibility APIs are really finicky with setting size"):
//! https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIAccessibilityElement.m
//! Each leg is fenced separately (identity/CG/membership/revision/scope
//! rechecked between legs) and read back separately, so per-leg truth
//! survives: an ordinary AX failure on one leg still attempts the
//! independent remaining legs, and exactly one bounded readback per
//! attempted leg reports what actually landed. No infinite retry, no
//! fighting loops: a single pass, then the completion (`Applied`, `Failed`,
//! `Timeout`, or the extended `PartialApplied`) tells the worker what
//! actually happened.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::dev_selection::TileScope;
use crate::ffi::AxFrame;
use crate::focus_route::FocusRoute;
use crate::native_facts::{NativeAppFacts, NativeReadOutcome};
use crate::runtime_backend::{AxBackend, AxBackendFactory};
#[cfg(target_os = "macos")]
use crate::runtime_types::NativeContext;
use crate::runtime_types::{
    AxCompletion, FocusCommand, FrameCommand, ObservedWindow, ScopedBackendSnapshot,
    ScopedWindowMeta,
};
use crate::scope::{AppScope, ProcessIdentity, is_ghostty_bundle};

/// Window show state rechecked immediately before effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowShowState {
    pub fullscreen: bool,
    pub minimized: bool,
    pub hidden_app: bool,
}

impl WindowShowState {
    #[must_use]
    pub fn visible_tilable(self) -> bool {
        !self.fullscreen && !self.minimized && !self.hidden_app
    }
}

/// Read-side fences for one scoped effect. All methods are bounded; any
/// `Err` fails the effect with no writes. Reasons carry OS status names or
/// static tokens only — never titles, content, bundle strings, or paths.
pub trait TileGates {
    /// Revalidate the bound process identity (PID reuse fence: exact live
    /// bundle/executable/launch-date equality; anything unreadable refuses).
    fn revalidate_identity(&self) -> Result<(), String>;
    /// Live scope bundle for the owner PID (`None` when unreadable).
    fn scope_bundle(&self) -> Option<String>;
    /// CG owner join: `wid` still owned by exactly `pid`.
    fn cg_owner(&self, pid: u32, wid: u32) -> Result<(), String>;
    /// Live Space membership for `wid` (exact list, empty = unknown).
    fn membership(&self, wid: u32) -> Result<Vec<u64>, String>;
    /// The current user Space homing this operation (re-homed live: the
    /// target display's row must still report it current and user-typed).
    fn current_user_space(&self) -> Result<u64, String>;
    /// Live display topology revision, read from the shared cell on every
    /// call (never a bound snapshot): a topology update mid-plan fences the
    /// very next leg. Cleared cell refuses outright.
    fn topology_revision(&self) -> Result<u64, String>;
    /// Fullscreen/minimized/hidden recheck immediately before effects.
    fn show_state(&self, wid: u32) -> Result<WindowShowState, String>;
    /// Worker-liveness fence rechecked between setter legs: a set worker
    /// stop flag or a superseding desired set withholds the remaining legs
    /// (the in-flight call always finishes). Scripted gates without
    /// lifetimes stay alive.
    fn effect_alive(&self) -> Result<(), String>;
    /// Session-safety fence: the last-known session must be a readable
    /// ordinary unlocked desktop. A loginwindow frontmost (lock screen) or
    /// an unknown/unreadable session denies narrowly — frames and focus
    /// never write the lock screen. Scripted gates model the verdict.
    fn session_safe(&self) -> Result<(), String>;
}

/// Write-side setters for one scoped effect. Size and position are
/// independent legs (applied SIZE→POSITION→SIZE per Silica); focus is the
/// same-owner SLPS/public-seam dispatch.
pub trait AxSetter {
    fn set_position(&mut self, wid: u32, frame: &AxFrame) -> Result<(), String>;
    fn set_size(&mut self, wid: u32, frame: &AxFrame) -> Result<(), String>;
    fn set_focus(&mut self, wid: u32, route: FocusRoute) -> Result<(), String>;
    /// One bounded readback after an attempted leg (`Ok(None)` =
    /// unreadable, never guessed). No retry loops.
    fn readback(&mut self, wid: u32) -> Result<Option<AxFrame>, String>;
}

/// Sequenced SLPS focus legs (yabai `window_manager_focus_window_with_raise`
/// parity: SLPS front plus key-window posts, then `AXRaise` on the exact
/// joined window). Dispatch runs first; a dispatch refusal never reaches the
/// raise leg, and a raise refusal after a sent dispatch reports both — the
/// fronting side effect already happened, so a bare raise reason would hide
/// it. Pure sequencing seam so the order and error precedence stay
/// unit-pinned without live AX.
pub fn slps_focus_sequence(
    dispatch: impl FnOnce() -> Result<(), String>,
    raise: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    dispatch()?;
    raise().map_err(|reason| format!("dispatch sent but {reason}"))
}

/// One leg's outcome in a scoped frame transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegOutcome {
    Applied,
    Refused(String),
    SkippedSafety(String),
}

/// Actual partial truth for one scoped frame transaction: per-leg outcomes
/// in Silica order (size-first, position, size) plus one bounded readback
/// per attempted leg. `readback` aliases the final leg's readback.
/// Re-observation consumes this; nothing here retries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartialWriteReport {
    pub pid: u32,
    pub wid: u32,
    pub size_first: LegOutcome,
    pub position: LegOutcome,
    pub size: LegOutcome,
    pub leg_readbacks: [Option<AxFrame>; 3],
    pub readback: Option<AxFrame>,
    pub readback_unreadable: bool,
}

/// Pure session-safety verdict for one scoped effect: Secure Input active,
/// a loginwindow frontmost (lock screen), an unknown/unreadable frontmost,
/// a locked window-server session, or an unknown session each refuse with a
/// static token. Unknown Secure Input proceeds (existing contract); every
/// other unknown refuses. No titles, no content — IDs and tokens only.
pub fn session_safe_verdict(
    secure_active: Option<bool>,
    frontmost: Option<(u32, Option<String>)>,
    lock: crate::ffi::SessionLock,
) -> Result<(), String> {
    if secure_active == Some(true) {
        return Err("Secure Input active; refusing".to_owned());
    }
    match &frontmost {
        Some((_, bundle))
            if crate::focus::is_loginwindow_bundle(bundle.as_deref().unwrap_or_default()) =>
        {
            return Err("locked session (loginwindow frontmost); refusing".to_owned());
        }
        Some(_) => {}
        None => {
            return Err("session unknown; refusing without a readable unlocked desktop".to_owned());
        }
    }
    match lock {
        crate::ffi::SessionLock::Unlocked => Ok(()),
        crate::ffi::SessionLock::Locked => {
            Err("locked session (window server); refusing".to_owned())
        }
        crate::ffi::SessionLock::Unknown => {
            Err("session unknown; refusing without a readable unlocked desktop".to_owned())
        }
    }
}

/// Shared membership fence for scoped effects: liveness, session safety,
/// identity, scope, CG join, exact current-user membership, and topology
/// revision. Returns the current Space on success. No writes happen inside
/// this function; show state is NOT consulted here (geometry and focus add
/// their own gates below).
fn fence_membership(
    scope: &TileScope,
    identity: &ProcessIdentity,
    gates: &dyn TileGates,
    pid: u32,
    wid: u32,
    expect_revision: u64,
) -> Result<u64, String> {
    if wid == 0 {
        return Err("refusing window id 0".to_owned());
    }
    // Liveness and session safety first: a dead worker or a locked/unknown
    // session refuses before any live read, so teardown and the lock screen
    // never pay for (or observe) AX traffic.
    gates.effect_alive()?;
    gates.session_safe()?;
    gates.revalidate_identity()?;
    // Identity carries the PID binding; the effect target must be the bound
    // process, otherwise this is a stale/foreign command.
    if identity.pid != pid {
        return Err(format!(
            "stale command pid {pid} != bound pid {}; refusing",
            identity.pid
        ));
    }
    let bundle = gates.scope_bundle();
    if !scope.allows_effect(bundle.as_deref()) {
        return Err(match bundle {
            Some(id) if is_ghostty_bundle(&id) => {
                "refusing Ghostty at the effect boundary".to_owned()
            }
            Some(id) if id == crate::BUNDLE_ID => {
                "refusing own bundle at the effect boundary".to_owned()
            }
            Some(id) if TileScope::is_product_bundle(&id) => {
                "refusing own helper bundle at the effect boundary".to_owned()
            }
            Some(_) => "out of scope: refusing write".to_owned(),
            None => "unknown bundle: refusing write".to_owned(),
        });
    }
    gates.cg_owner(pid, wid)?;
    let live_revision = gates.topology_revision()?;
    if live_revision != expect_revision {
        return Err(format!(
            "topology revision drift (want {expect_revision}, live {live_revision}); superseded"
        ));
    }
    let current = gates.current_user_space()?;
    let membership = gates.membership(wid)?;
    if membership.len() != 1 || membership[0] != current {
        return Err("window not exactly on the current user Space; refusing".to_owned());
    }
    Ok(current)
}

/// Fence a scoped frame effect: membership plus the show-state recheck.
/// Minimized, hidden-app, and fullscreen-overlay windows take no geometry
/// writes (omission, never destruction; no hides, no restyling, no Space
/// writes, no geometry restoration on stop/crash).
pub fn fence_scoped_effect(
    scope: &TileScope,
    identity: &ProcessIdentity,
    gates: &dyn TileGates,
    pid: u32,
    wid: u32,
    expect_revision: u64,
) -> Result<u64, String> {
    let current = fence_membership(scope, identity, gates, pid, wid, expect_revision)?;
    let state = gates.show_state(wid)?;
    if state.minimized {
        return Err("minimized: withholding geometry write".to_owned());
    }
    if state.hidden_app {
        return Err("hidden app: withholding geometry write".to_owned());
    }
    if state.fullscreen {
        return Err("fullscreen overlay: withholding geometry write".to_owned());
    }
    Ok(current)
}

/// Apply one scoped frame SIZE→POSITION→SIZE (Silica order) with one bounded
/// readback per attempted leg. An ordinary AX failure on one leg never
/// blocks the independent remaining legs; only a safety fence refusal
/// (identity/CG/membership/revision/scope/show-state, rechecked between
/// every leg) skips what follows. Single pass: no retry, no fighting loops.
/// Per-leg readbacks preserve exactly what landed for re-observation.
#[allow(clippy::too_many_arguments)]
pub fn apply_scoped_frame(
    scope: &TileScope,
    identity: &ProcessIdentity,
    gates: &dyn TileGates,
    setter: &mut dyn AxSetter,
    pid: u32,
    wid: u32,
    frame: &AxFrame,
    expect_revision: u64,
    started_ms: u64,
) -> (AxCompletion, PartialWriteReport) {
    let transaction_start = std::time::Instant::now();
    let report_of = |size_first, position, size, leg_readbacks: [Option<AxFrame>; 3]| {
        let readback = leg_readbacks[2].or(leg_readbacks[1]).or(leg_readbacks[0]);
        let unreadable = leg_readbacks.iter().all(|r| r.is_none());
        PartialWriteReport {
            pid,
            wid,
            size_first,
            position,
            size,
            leg_readbacks,
            readback,
            readback_unreadable: unreadable,
        }
    };
    if !frame.valid_for_set() {
        let refused = LegOutcome::Refused("degenerate frame".to_owned());
        let report = report_of(
            refused.clone(),
            refused.clone(),
            refused,
            [None, None, None],
        );
        return (
            AxCompletion::Failed {
                reason: "degenerate frame".to_owned(),
            },
            report,
        );
    }
    if let Err(reason) = fence_scoped_effect(scope, identity, gates, pid, wid, expect_revision) {
        let leg = LegOutcome::Refused(reason.clone());
        let report = report_of(leg.clone(), leg.clone(), leg, [None, None, None]);
        if reason.contains("timeout") {
            return (AxCompletion::Timeout { timeout_ms: 2000 }, report);
        }
        return (AxCompletion::Failed { reason }, report);
    }
    // Leg 1 (size-first): an ordinary refusal still attempts the rest.
    let size_first = match setter.set_size(wid, frame) {
        Ok(()) => LegOutcome::Applied,
        Err(reason) => LegOutcome::Refused(reason),
    };
    let readback_0 = setter.readback(wid).ok().flatten();
    // Narrow safety recheck before the position leg.
    let position = match fence_scoped_effect(scope, identity, gates, pid, wid, expect_revision) {
        Err(reason) => LegOutcome::SkippedSafety(reason),
        Ok(_) => match setter.set_position(wid, frame) {
            Ok(()) => LegOutcome::Applied,
            Err(reason) => LegOutcome::Refused(reason),
        },
    };
    let readback_1 = match &position {
        LegOutcome::SkippedSafety(_) => None,
        _ => setter.readback(wid).ok().flatten(),
    };
    // Narrow safety recheck before the final size leg.
    let size = match fence_scoped_effect(scope, identity, gates, pid, wid, expect_revision) {
        Err(reason) => LegOutcome::SkippedSafety(reason),
        Ok(_) => match setter.set_size(wid, frame) {
            Ok(()) => LegOutcome::Applied,
            Err(reason) => LegOutcome::Refused(reason),
        },
    };
    let readback_2 = match &size {
        LegOutcome::SkippedSafety(_) => None,
        _ => setter.readback(wid).ok().flatten(),
    };
    let report = report_of(
        size_first.clone(),
        position.clone(),
        size.clone(),
        [readback_0, readback_1, readback_2],
    );
    let readback_xywh = report
        .readback
        .map(|f| (f.x as i32, f.y as i32, f.w as i32, f.h as i32));
    let elapsed_ms = started_ms.saturating_add(
        transaction_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    );
    let completion = match (&size_first, &position, &size) {
        (LegOutcome::Applied, LegOutcome::Applied, LegOutcome::Applied) => {
            AxCompletion::Applied { elapsed_ms }
        }
        (LegOutcome::SkippedSafety(reason), _, _)
        | (_, LegOutcome::SkippedSafety(reason), _)
        | (_, _, LegOutcome::SkippedSafety(reason)) => AxCompletion::Failed {
            reason: reason.clone(),
        },
        _ => AxCompletion::PartialApplied {
            elapsed_ms,
            size_first_applied: size_first == LegOutcome::Applied,
            position_applied: position == LegOutcome::Applied,
            size_applied: size == LegOutcome::Applied,
            readback_xywh,
            readback_unreadable: report.readback_unreadable,
        },
    };
    (completion, report)
}

/// Fence a scoped focus dispatch: the full membership fence plus the same
/// show-state recheck as geometry. Minimized, hidden-app, and
/// fullscreen-overlay windows take no focus dispatch either (conservative:
/// focusing a window the compositor hides has no verifiable arrival — the
/// agreement oracle can never confirm it — so the fence withholds rather
/// than dispatch blind). Unknown/out-of-scope never dispatches.
pub fn fence_scoped_focus(
    scope: &TileScope,
    identity: &ProcessIdentity,
    gates: &dyn TileGates,
    pid: u32,
    wid: u32,
    expect_revision: u64,
) -> Result<(), String> {
    fence_scoped_effect(scope, identity, gates, pid, wid, expect_revision).map(|_| ())
}

/// Route classified window IDs into their scoped tile domain for one
/// display. Thin wrapper over [`crate::topology::select_domain`] so the
/// main-thread controller has one safe-API entry:
/// snapshot in, domain out, with per-window exclusion evidence for the
/// current-Space omission rule (other-Space windows are never written).
pub fn route_display_domain(
    snapshot: &crate::topology::TopologySnapshot,
    cg_id: u32,
    memberships: &[crate::topology::WindowMembership],
    space_type: &dyn Fn(u64) -> Option<crate::ffi::SlsSpaceType>,
) -> Result<crate::topology::TileDomain, crate::topology::SelectError> {
    crate::topology::select_domain(snapshot, cg_id, memberships, space_type)
}

/// Validate one live identity triple for backend admission (pure): PID
/// range, selection at the boundary, and launch currency. Scoped mode
/// refuses Ghostty, the own bundle, protected terminals, and unknown
/// bundles even when listed; unscoped mode refuses both own bundles, exact
/// exclusions, and unknown bundles while admitting Ghostty as an ordinary
/// candidate. A missing launch date refuses outright — an executable-only
/// fallback cannot distinguish PID reuse (same binary, new process), so it
/// must never authorize. Returns the bound lifetime (bundle plus
/// executable plus start token) that every effect revalidates exactly.
pub fn validate_identity_triple(
    scope: &TileScope,
    pid: u32,
    bundle: Option<&str>,
    exe: Option<&str>,
    launch_date: Option<&str>,
) -> Result<ProcessIdentity, String> {
    if pid <= 1 || pid > i32::MAX as u32 {
        return Err(format!("refusing out-of-range pid {pid}"));
    }
    match bundle {
        Some(id) if scope.allows_effect(Some(id)) => {}
        Some(id) if is_ghostty_bundle(id) => {
            return Err("refusing Ghostty identity".to_owned());
        }
        Some(id) if id == crate::BUNDLE_ID => {
            return Err("refusing own-bundle identity".to_owned());
        }
        Some(id) if TileScope::is_product_bundle(id) => {
            return Err(
                "refusing own helper-bundle identity (unscoped product run; scoped fixtures list it explicitly)"
                    .to_owned(),
            );
        }
        Some(_) => return Err("out of scope: refusing owner".to_owned()),
        None => return Err("unknown bundle: refusing owner".to_owned()),
    }
    let exe = exe.unwrap_or_default();
    if exe.is_empty() {
        return Err("refusing identity without an executable path".to_owned());
    }
    let Some(start) = launch_date.filter(|s| !s.is_empty()) else {
        return Err("refusing identity without a launch date (no PID-reuse protection)".to_owned());
    };
    // Unscoped product mode binds Ghostty as an ordinary candidate (only
    // the scoped mode refuses it); every other lifetime check
    // is identical, and scoped keeps the refusing bind.
    if scope.is_unscoped() {
        ProcessIdentity::bind_unscoped(pid, bundle, exe, start)
    } else {
        ProcessIdentity::bind(pid, bundle, exe, start)
    }
}

/// Build one scoped snapshot from a complete native read (pure over
/// injected readers, so offline tests drive the exact admission shape):
/// whole-snapshot classification refusal on the first gap, classifier
/// mapping per window (omitted shapes counted, never published), and
/// per-window Space membership with whole-snapshot refusal on ANY
/// unreadable membership — a transient read error and an empty (unknown)
/// home both refuse the whole snapshot instead of publishing a partial one
/// that would churn minimize slots downstream. Known other-Space
/// membership is distinct: it admits with metadata (the engine omits
/// non-homed windows), because a known home is evidence, not a gap.
/// `focused_wid` plus `focus_cg_owned` ride for the worker-stamped evidence
/// gate.
#[allow(clippy::too_many_arguments)]
pub fn build_scoped_snapshot(
    pid: u32,
    revision: u64,
    outcome: &NativeReadOutcome,
    app_facts: &NativeAppFacts,
    spaces_of: &dyn Fn(u32) -> Result<Vec<u64>, String>,
    focused_wid: Option<u32>,
    focus_cg_owned: bool,
    modal_fallback: bool,
) -> Result<ScopedBackendSnapshot, String> {
    use crate::classifier::{WindowFrame, classify_window};

    let classified = outcome.classify_all(modal_fallback).map_err(|gap| {
        format!(
            "pid {pid}: refusing whole snapshot (wid {}: {} unknown)",
            gap.wid, gap.missing
        )
    })?;
    let app = app_facts.to_app_facts();
    let mut windows = Vec::with_capacity(classified.len());
    let mut meta = Vec::with_capacity(classified.len());
    let mut excluded_reasons =
        Vec::<crate::runtime_types::ExcludedOmission>::with_capacity(classified.len());
    for read in &classified {
        let frame = read.facts.frame.ok_or_else(|| {
            format!(
                "pid {pid}: refusing whole snapshot (wid {}: frame unknown)",
                read.facts.wid
            )
        })?;
        let classified_window = classify_window(
            WindowFrame {
                wid: read.facts.wid,
                x: frame.x as i32,
                y: frame.y as i32,
                w: frame.w as i32,
                h: frame.h as i32,
            },
            &read.ax,
            &read.cg,
            &app,
        );
        // Exclusion reasons ride the snapshot as diagnostic detail (wid
        // plus status name, never titles/content): the engine still sees
        // only the omitted count, so omission here stays omission.
        if let crate::classifier::Disposition::Excluded(reason) = classified_window.disposition {
            excluded_reasons.push(crate::runtime_types::ExcludedOmission {
                wid: read.facts.wid,
                reason,
            });
            continue;
        }
        let observed = classified_window.to_observed().unwrap_or_else(|| {
            unreachable!(
                "pid {pid}: non-excluded disposition must observe (wid {})",
                read.facts.wid
            )
        });
        // Membership is homing evidence: a failed read or an empty
        // (unknown-home) list refuses the WHOLE snapshot — publishing the
        // readable siblings as complete would invent the unreadable
        // window's destruction downstream (same rule as classification
        // gaps). A known home on another Space admits with metadata; the
        // engine omits it per domain without destroying.
        let spaces = spaces_of(observed.wid).map_err(|_| {
            format!(
                "pid {pid}: refusing whole snapshot (wid {}: membership unreadable)",
                observed.wid
            )
        })?;
        if spaces.is_empty() {
            return Err(format!(
                "pid {pid}: refusing whole snapshot (wid {}: home Space unknown)",
                observed.wid
            ));
        }
        meta.push(ScopedWindowMeta {
            wid: observed.wid,
            spaces,
            fullscreen: read.facts.fullscreen.unwrap_or(false),
            minimized: read.facts.minimized.unwrap_or(false),
            hidden: app_facts.hidden,
        });
        windows.push(observed);
    }
    Ok(ScopedBackendSnapshot {
        windows,
        focused_wid,
        focus_cg_owned,
        focus_evidence: None,
        sys_focus: None,
        global_focus: None,
        meta,
        excluded: excluded_reasons.len(),
        excluded_reasons,
        revision,
    })
}

/// Scoped-reread trigger token for one bounded metadata-only reread line:
/// `notification` (an observer event woke the owner), `topology` (a
/// revision-changing publication landed since the owner's last consumed
/// generation), `arrival` (a verified native arrival requested a reread at
/// an unchanged revision), `backstop` (silence past one interval forced the
/// reread for a lost/unsupported notification), or `initial` (first
/// snapshot through the poll path with no observer yet). `now_ms` is the
/// process-common clock (comparable across owner threads); the owner-local
/// backstop clock never reaches this line. Pure.
#[must_use]
pub fn format_scoped_reread_line(pid: u32, trigger: &str, now_ms: u64) -> String {
    format!("tiler-macos tile reread pid={pid} trigger={trigger} now_ms={now_ms}")
}

/// One bounded metadata-only native frame-set line: the requested rect,
/// the actual exact AX readback rect (`None` when unreadable, never
/// guessed), and whether the readback names the request exactly. A posted
/// dispatch is never read as arrival: `outcome` is the dispatch truth
/// (`applied` only when every leg reported success) while `exact` is the
/// observed truth (readback equals request). IDs and geometry only -
/// never titles, content, or paths. Pure.
// The attribution join needs request, readback, outcome, exact, clocks,
// and identity together; callers pass literals, never user text.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn format_frame_set_line(
    pid: u32,
    wid: u32,
    revision: u64,
    requested_xywh: (i32, i32, i32, i32),
    readback_xywh: Option<(i32, i32, i32, i32)>,
    outcome: &str,
    exact: bool,
    elapsed_ms: u64,
    now_ms: u64,
) -> String {
    format!(
        "tiler-macos tile frame-set pid={pid} wid={wid} revision={revision} requested={requested_xywh:?} readback={readback_xywh:?} outcome={outcome} exact={exact} elapsed_ms={elapsed_ms} now_ms={now_ms}"
    )
}

/// Consume one owner-side invalidation generation (pure over explicit
/// inputs; the live side pairs both reads under one cell lock via
/// [`crate::runtime::NativeContextCell::load_versioned`]). Returns the
/// pending mark plus the generation to retain: a bump since the last
/// consumed generation requests exactly one whole-owner reread (`true`),
/// and the caller adopts the current generation even when the reread then
/// fails - so bursts coalesce into one reread, a failed reread never
/// replays, and only a new bump rereads again. `None` (never consumed)
/// adopts without requesting: startup already published, so the first pump
/// must not manufacture a reread.
#[must_use]
pub fn consume_reread_generation(seen: Option<u64>, current: u64) -> (bool, u64) {
    match seen {
        None => (false, current),
        Some(last) => (last != current, current),
    }
}

/// Select one scoped-reread trigger for the next owner pump (pure):
/// a delivered notification rereads immediately; otherwise a consumed
/// invalidation bump rereads immediately - attributed `topology` when the
/// publication revision moved since the owner's last reread, `arrival`
/// when a verified arrival requested the reread at an unchanged revision.
/// Silence past the interval rereads on the backstop; a never-read owner
/// reads immediately (`initial`). Returns `None` when nothing is due, so
/// the owner stays quiet. Stop withholds everything (checked by the
/// caller; a stopped owner never reaches this selection).
#[must_use]
pub fn select_scoped_reread_trigger(
    notified: bool,
    generation_changed: bool,
    revision_changed: bool,
    backstop_due: bool,
    never_read: bool,
) -> Option<&'static str> {
    if notified {
        return Some("notification");
    }
    if generation_changed {
        return Some(if revision_changed {
            "topology"
        } else {
            "arrival"
        });
    }
    if never_read {
        return Some("initial");
    }
    if backstop_due {
        return Some("backstop");
    }
    None
}

/// Interval between autonomous complete re-reads when no observer
/// notification arrived (missed/lost/unsupported-notification backstop).
/// Native observer events stay primary: a delivered notification rereads
/// immediately. The backstop only covers silence — a minimize/close the
/// observer never reported still surfaces within about one interval. Reads
/// stay on the owning per-app AX thread (never the main thread, which
/// performs no AX reads); no extra threads, no new dependencies. Unchanged
/// rereads never republish: the worker stages against the last sent state
/// (`stage_scoped`/`scoped_snapshots_equal`) and drops equal snapshots, so
/// the backstop cannot manufacture a reassertion storm. Incomplete reads
/// refuse whole (`build_scoped_snapshot`) and preserve retained state, so
/// the backstop never publishes a partial replacement. Failure backoff uses
/// the same cadence (a failed reread never retries faster than this).
pub const SCOPED_BACKSTOP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Pure backstop decision over millisecond timestamps (fake-clock seam for
/// the missed-notification regression: callers pass a monotonic clock, tests
/// pass scripted values). `now_ms`/`last_read_ms`/`last_failure_ms` share one
/// arbitrary epoch (e.g. millis since backend creation); `None` last-read
/// means never read (due — a read is always safe), `None` last-failure means
/// no backoff constraint. `stop` withholds everything; a delivered
/// notification rereads immediately; otherwise a full interval of read
/// silence (and, after a failure, of failure silence) must elapse.
#[must_use]
pub fn scoped_backstop_due(
    stop: bool,
    notified: bool,
    now_ms: u64,
    last_read_ms: Option<u64>,
    last_failure_ms: Option<u64>,
    interval_ms: u64,
) -> bool {
    if stop {
        return false;
    }
    if notified {
        return true;
    }
    // Never read at all: due immediately (a read is always safe).
    let Some(last_read) = last_read_ms else {
        return true;
    };
    if now_ms.saturating_sub(last_read) < interval_ms {
        return false;
    }
    match last_failure_ms {
        Some(failed_at) => now_ms.saturating_sub(failed_at) >= interval_ms,
        None => true,
    }
}

/// Per-app AX backend factory over one product selection: validates the
/// selection at construction AND binds the launch lifetime at backend
/// creation. Unknown, out-of-selection, launch-date-less owners, and (in
/// scoped mode) Ghostty never construct a writer; unscoped mode additionally
/// refuses both own bundles plus exact exclusions while admitting Ghostty as
/// an ordinary candidate. Created backends run on the owning per-app AX
/// thread (see [`crate::runtime`]); every `AxElement` is constructed, used,
/// and dropped there.
#[derive(Debug, Clone)]
pub struct ScopedAxFactory {
    scope: TileScope,
    /// Shared per-PID eligibility facts: each created backend publishes only
    /// its own bound PID from snapshot data it already read. `None` keeps
    /// the factory observation-only (existing fixtures/tests).
    facts: Option<Arc<crate::input_eligibility::EligibilityFactsStore>>,
    /// Shared per-PID border facts: same publish shape for the focused
    /// window (frame plus AX/CG/app facts plus Space membership). `None`
    /// disables border publishing.
    border: Option<Arc<crate::border_runtime::BorderFactsStore>>,
}

impl ScopedAxFactory {
    pub fn new(scope: AppScope) -> Result<Self, String> {
        let selection = TileScope::Scoped(scope);
        selection.validate()?;
        Ok(Self {
            scope: selection,
            facts: None,
            border: None,
        })
    }

    /// Unscoped product factory: all ordinary apps minus own bundles and
    /// the exact exclusions (Ghostty stays an ordinary candidate; only the
    /// scoped constructor refuses it).
    pub fn unscoped(excluded: Vec<String>) -> Result<Self, String> {
        let selection = TileScope::Unscoped { excluded };
        selection.validate()?;
        Ok(Self {
            scope: selection,
            facts: None,
            border: None,
        })
    }

    /// Share the eligibility facts store: created backends publish their
    /// own PID's candidate facts from already-read snapshots (never new AX
    /// for the publisher; never any AX for the input consumer).
    #[must_use]
    pub fn with_facts_store(
        mut self,
        store: Arc<crate::input_eligibility::EligibilityFactsStore>,
    ) -> Self {
        self.facts = Some(store);
        self
    }

    /// Share the border facts store: created backends publish their own
    /// PID's focused-window facts from already-read snapshots (never new
    /// AX for the publisher; never any AX for the main-thread consumer).
    #[must_use]
    pub fn with_border_store(
        mut self,
        store: Arc<crate::border_runtime::BorderFactsStore>,
    ) -> Self {
        self.border = Some(store);
        self
    }

    #[must_use]
    pub fn scope(&self) -> &TileScope {
        &self.scope
    }
}

impl AxBackendFactory for ScopedAxFactory {
    fn create(&self, pid: u32) -> Result<Box<dyn AxBackend>, String> {
        // Live identity triple on the owning thread (`NSRunningApplication`
        // model reads plus `proc_pidpath`: the one justified off-main
        // AppKit exception — no view, window, runloop, or notification
        // state; every other AppKit/UI read stays main-thread-only).
        // Failure fails closed before any AX element exists.
        let live = crate::ffi::ns_app_identity_for_pid(pid as i32);
        let identity = validate_identity_triple(
            &self.scope,
            pid,
            live.bundle.as_deref(),
            live.exe.as_deref(),
            live.launch_date.as_deref(),
        )?;
        Ok(Box::new(ScopedAxBackend::bind(
            pid,
            self.scope.clone(),
            identity,
            self.facts.clone(),
            self.border.clone(),
        )))
    }
}

/// Scoped per-app AX backend over the existing safe FFI wrappers plus the
/// native fact readers. Constructed with bound plain-data identity only;
/// every `AxElement` is created, used, and dropped inside the owning worker
/// thread on each call. The live effect hook (revision cell, stop flag,
/// frame slot, freshest frontmost) binds via
/// [`AxBackend::bind_effect_hook`] every worker iteration; unhooked
/// backends fail every scoped call closed instead of actuating blind.
pub struct ScopedAxBackend {
    pid: u32,
    // Portable construction retains these for the macOS effect path; offmac
    // they are write-only (no live reads), so silence dead_code there only.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    scope: TileScope,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    identity: ProcessIdentity,
    hook: Option<crate::runtime_backend::EffectHook>,
    /// Shared eligibility facts: this owner publishes ONLY `pid` from
    /// snapshot data already read below. `None` disables publishing.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    facts: Option<Arc<crate::input_eligibility::EligibilityFactsStore>>,
    /// Shared border facts: this owner publishes ONLY `pid`'s focused
    /// window from snapshot data already read below. `None` disables
    /// border publishing.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    border: Option<Arc<crate::border_runtime::BorderFactsStore>>,
    #[cfg(target_os = "macos")]
    observer: Option<crate::ffi::Phase1Observer>,
    /// Monotonic origin for the missed-notification backstop clock (set on
    /// first poll entry; millisecond offsets stay on this owner thread).
    #[cfg(target_os = "macos")]
    backstop_origin: Option<std::time::Instant>,
    /// Millis-since-origin of the last completed poll reread (success or
    /// startup snapshot through the poll path).
    #[cfg(target_os = "macos")]
    backstop_last_read_ms: Option<u64>,
    /// Millis-since-origin of the last failed poll reread (same-cadence
    /// backoff; cleared on success).
    #[cfg(target_os = "macos")]
    backstop_last_failure_ms: Option<u64>,
    /// Last consumed owner invalidation generation (`None` until the first
    /// pump adopts without requesting; startup already published).
    #[cfg(target_os = "macos")]
    reread_seen_generation: Option<u64>,
    /// Revision of the owner's last performed poll reread (`None` until
    /// the first one): attributes a consumed generation bump as `topology`
    /// (revision moved) or `arrival` (same revision, verified arrival).
    #[cfg(target_os = "macos")]
    last_poll_revision: Option<u64>,
}

impl ScopedAxBackend {
    fn bind(
        pid: u32,
        scope: TileScope,
        identity: ProcessIdentity,
        facts: Option<Arc<crate::input_eligibility::EligibilityFactsStore>>,
        border: Option<Arc<crate::border_runtime::BorderFactsStore>>,
    ) -> Self {
        Self {
            pid,
            scope,
            identity,
            facts,
            border,
            hook: None,
            #[cfg(target_os = "macos")]
            observer: None,
            #[cfg(target_os = "macos")]
            backstop_origin: None,
            #[cfg(target_os = "macos")]
            backstop_last_read_ms: None,
            #[cfg(target_os = "macos")]
            backstop_last_failure_ms: None,
            #[cfg(target_os = "macos")]
            reread_seen_generation: None,
            #[cfg(target_os = "macos")]
            last_poll_revision: None,
        }
    }

    fn hook(&self) -> Result<crate::runtime_backend::EffectHook, String> {
        self.hook.clone().ok_or_else(|| {
            format!(
                "pid {}: no effect hook bound; refusing without a live revision source",
                self.pid
            )
        })
    }
}

impl AxBackend for ScopedAxBackend {
    /// Frame actuation runs the fenced SIZE→POSITION→SIZE transaction
    /// (never the legacy two-leg path, which cannot fence membership or
    /// revision). The gates and setter are constructed fresh per command
    /// on the owning thread; every leg is fenced and read back separately.
    fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
        use std::time::Instant;
        let start = Instant::now();
        let elapsed_ms = || start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        if cmd.pid != self.pid {
            return AxCompletion::Failed {
                reason: "pid mismatch (stale command)".to_owned(),
            };
        }
        let frame = AxFrame {
            x: f64::from(cmd.x),
            y: f64::from(cmd.y),
            w: f64::from(cmd.w),
            h: f64::from(cmd.h),
        };
        #[cfg(target_os = "macos")]
        {
            let hook = match self.hook() {
                Ok(hook) => hook,
                Err(reason) => {
                    return AxCompletion::Failed { reason };
                }
            };
            let Some(ctx) = hook.cell.load() else {
                return AxCompletion::Failed {
                    reason: "native context cleared; refusing".to_owned(),
                };
            };
            // Pin the slot revision at apply entry: a newer desired set
            // landing mid-apply withholds the remaining legs below (the
            // in-flight leg always finishes).
            let taken = hook.frames.current_rev();
            let gates = native::NativeGates::for_op(
                self.pid,
                cmd.wid,
                &self.identity,
                &hook,
                cmd.domain.clone(),
                Some(taken),
                self.scope.is_unscoped(),
            );
            let mut setter = native::NativeSetter::new(self.pid, cmd.wid);
            let (completion, report) = apply_scoped_frame(
                &self.scope,
                &self.identity,
                &gates,
                &mut setter,
                self.pid,
                cmd.wid,
                &frame,
                cmd.topology_revision,
                elapsed_ms(),
            );
            let _ = ctx;
            // Frame-set boundary: the log carries the ACTUAL exact AX
            // readback, never the request alone. `outcome` is dispatch
            // truth (every leg reported success); `exact` is observed
            // truth (readback equals the request). A dispatch success with
            // `exact=false` (or an unreadable readback) is not arrival -
            // arrival is proven only by a later owned-snapshot join.
            // Token-only: IDs, revision, geometry, static outcome.
            let requested_xywh = (cmd.x, cmd.y, cmd.w, cmd.h);
            let readback_xywh = report
                .readback
                .map(|f| (f.x as i32, f.y as i32, f.w as i32, f.h as i32));
            let outcome = match &completion {
                AxCompletion::Applied { .. } => "applied",
                AxCompletion::PartialApplied { .. } => "partial-applied",
                AxCompletion::Failed { .. } => "failed",
                AxCompletion::Timeout { .. } => "timeout",
            };
            eprintln!(
                "{}",
                format_frame_set_line(
                    self.pid,
                    cmd.wid,
                    cmd.topology_revision,
                    requested_xywh,
                    readback_xywh,
                    outcome,
                    readback_xywh == Some(requested_xywh),
                    elapsed_ms(),
                    crate::input_eligibility::now_ms(),
                )
            );
            completion
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = frame;
            let _ = elapsed_ms();
            AxCompletion::Failed {
                reason: "frame actuation is macOS-only".to_owned(),
            }
        }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        Err(format!(
            "pid {}: scoped backend requires snapshot_scoped (metadataless reads cannot home)",
            self.pid
        ))
    }

    fn bind_effect_hook(&mut self, hook: &crate::runtime_backend::EffectHook) {
        self.hook = Some(hook.clone());
    }

    fn snapshot_scoped(&mut self) -> Result<crate::runtime_types::ScopedBackendSnapshot, String> {
        let hook = self.hook()?;
        let Some(ctx) = hook.cell.load() else {
            return Err(format!(
                "pid {}: native context cleared; refusing snapshot",
                self.pid
            ));
        };
        #[cfg(target_os = "macos")]
        {
            self.snapshot_scoped_native(&ctx)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = ctx;
            Err(format!(
                "pid {}: scoped snapshots are macOS-only; use offline fakes",
                self.pid
            ))
        }
    }

    fn poll_scoped(
        &mut self,
        stop: &AtomicBool,
    ) -> Option<Result<crate::runtime_types::ScopedBackendSnapshot, String>> {
        #[cfg(target_os = "macos")]
        {
            self.poll_scoped_native(stop)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = stop;
            None
        }
    }

    fn dispatch_focus(&mut self, cmd: &FocusCommand) -> AxCompletion {
        #[cfg(target_os = "macos")]
        {
            self.dispatch_focus_native(cmd)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cmd;
            AxCompletion::Failed {
                reason: "focus dispatch is macOS-only".to_owned(),
            }
        }
    }
}

/// Native scoped owner reads over live FFI (macOS only): the [`TileGates`]
/// plus [`AxSetter`] implementation behind [`ScopedAxBackend`]. Constructed
/// fresh per operation on the owning thread; no element outlives one fence
/// check plus its legs. Title-free throughout; reasons name OS statuses
/// only.
#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use crate::ffi::{
        AxElement, ax_fullscreen_state, ax_get_frame, ax_raise, ax_set_focused_window,
        ax_set_messaging_timeout, ax_set_position, ax_set_size, ax_window_id,
    };
    use crate::runtime_types::AX_CALL_TIMEOUT;

    fn messaging_secs() -> f32 {
        AX_CALL_TIMEOUT.as_secs_f32().max(0.5)
    }

    /// Bound for one scoped observer pump slice (mirrors the Phase 1
    /// backend): wake-driven delivery still applies (notifications interrupt
    /// the slice); the bound only caps command/stop latency.
    const NATIVE_PUMP_SLICE: std::time::Duration = std::time::Duration::from_millis(20);

    impl super::ScopedAxBackend {
        pub(super) fn snapshot_scoped_native(
            &mut self,
            ctx: &Arc<NativeContext>,
        ) -> Result<ScopedBackendSnapshot, String> {
            use crate::native_facts::{read_app_facts, read_app_window_facts};

            // Whole-read refusal on the first gap (classification or
            // per-read legs): an incomplete read never publishes as a
            // complete replacement (which would invent destruction).
            let outcome = read_app_window_facts(self.pid)?;
            // Owner-thread app facts: the same `NSRunningApplication`
            // model reads as the main-thread supplier (justified
            // off-main exception); hide/policy dynamics need fresh reads.
            let app_facts = read_app_facts(self.pid)?;
            // Per-window subscription sync on the facts just read (same
            // shape as the Phase 1 backend's `read_observed`): newly
            // observed windows join (minimize/deminimize/destroyed/move/
            // resize watches), gone windows leave. Without this the
            // window-level set is never installed in the tile path, so
            // minimize/close deliver zero observations. A failed sync
            // reports honestly and publishes nothing (retained state
            // preserved) — a failed subscription is never assumed success.
            if let Some(observer) = self.observer.as_mut() {
                let wids: Vec<u32> = outcome.windows.iter().map(|w| w.wid).collect();
                observer
                    .sync_windows(&wids, messaging_secs())
                    .map_err(|reason| format!("subscription sync pid {}: {reason}", self.pid))?;
            }
            // Observer focus (app-local; global gating happens engine-side
            // on worker-stamped evidence) plus its CG ownership join.
            let (focused_wid, focus_cg_owned) = match self.observer.as_ref() {
                Some(observer) => {
                    let wid = observer.focused_wid(messaging_secs());
                    let owned = wid
                        .map(|w| {
                            crate::ffi::cg_window_list_basic()
                                .map(|list| {
                                    list.iter()
                                        .any(|entry| entry.wid == w && entry.pid == self.pid)
                                })
                                .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    (wid, owned)
                }
                None => (None, false),
            };
            let spaces_of = |wid: u32| crate::ffi::sls_spaces_for_window(wid);
            let mut snapshot = build_scoped_snapshot(
                self.pid,
                ctx.revision,
                &outcome,
                &app_facts,
                &spaces_of,
                focused_wid,
                focus_cg_owned,
                false,
            )?;
            // Bounded selected-oracle focus leg on THIS owning thread (never
            // the main thread, which performs no AX reads): the worker
            // stamps it over the evidence `ax_focus` slot at send time. The
            // selected SLPS+direct read only — production never consults
            // the raw system-wide leg (`ffi::ax_focused_preimage_detailed`,
            // diagnostic-only) nor the experimental diagnostic fallback
            // (`ffi::ax_focused_preimage_evidence`, behind
            // `focus-diagnose --fallback`, unproven and lacking acceptance),
            // and `AXFrontmost` never gates. `None` is honest unknown
            // (withholds promotion, never a claim).
            snapshot.sys_focus = crate::ffi::ax_selected_focus_evidence()
                .ok()
                .map(|evidence| (evidence.pid as u32, evidence.wid));
            // Eligibility publish (best-effort side channel, never fails the
            // observation): ONLY this owner's bound PID, from snapshot data
            // already read above (window facts, Space membership, selected
            // focus, context revision). A hung peer stalls only its own
            // record (which then goes stale and denies); records are keyed
            // by PID so this write cannot touch another PID's record.
            if let Some(store) = self.facts.as_ref() {
                let window = snapshot
                    .focused_wid
                    .and_then(|wid| snapshot.meta.iter().find(|m| m.wid == wid));
                store.publish_app(crate::input_eligibility::AppEligibilityRecord {
                    pid: self.pid,
                    bundle: self.identity.bundle.clone(),
                    focused_wid: snapshot.focused_wid,
                    selected: snapshot.sys_focus,
                    spaces: window.map(|w| w.spaces.clone()).unwrap_or_default(),
                    fullscreen: window.map(|w| w.fullscreen),
                    minimized: window.map(|w| w.minimized),
                    hidden_app: window.map(|w| w.hidden),
                    revision: ctx.revision,
                    updated_ms: store.now_ms(),
                });
            }
            // Border publish (best-effort side channel, never fails the
            // observation): ONLY this owner's bound PID, focused-window
            // facts from the outcome just read above (frame, AX/CG/app
            // facts, Space membership). A missing join anywhere publishes
            // `window: None` (unknown hides tick-side) rather than a
            // guessed target; classifier-excluded shapes keep their facts
            // here (the tick-side mapper hides them with a reason) while
            // Space membership falls back to one direct read. Placement
            // truth (engine plan membership, including ToggleFloat) is a
            // tick-side join in `tile` — never a second classifier call
            // here: disposition is eligibility, not placement.
            if let Some(store) = self.border.as_ref() {
                let window = snapshot.focused_wid.and_then(|wid| {
                    let facts = outcome.windows.iter().find(|w| w.wid == wid)?;
                    let frame = facts.frame?;
                    let ax = facts.to_ax_facts(false)?;
                    let cg = facts.to_cg_facts()?;
                    let app = app_facts.to_app_facts();
                    let spaces = snapshot
                        .meta
                        .iter()
                        .find(|m| m.wid == wid)
                        .map(|m| m.spaces.clone())
                        .or_else(|| {
                            #[cfg(target_os = "macos")]
                            {
                                crate::ffi::sls_spaces_for_window(wid).ok()
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                None
                            }
                        })?;
                    Some(crate::border_runtime::BorderWindowFacts {
                        wid,
                        frame,
                        ax,
                        cg,
                        app,
                        spaces,
                    })
                });
                store.publish_app(crate::border_runtime::BorderAppRecord {
                    pid: self.pid,
                    bundle: self.identity.bundle.clone(),
                    focused_wid: snapshot.focused_wid,
                    selected: snapshot.sys_focus,
                    window,
                    revision: ctx.revision,
                    updated_ms: store.now_ms(),
                });
            }
            Ok(snapshot)
        }

        pub(super) fn poll_scoped_native(
            &mut self,
            stop: &AtomicBool,
        ) -> Option<Result<ScopedBackendSnapshot, String>> {
            use std::sync::atomic::Ordering;
            if stop.load(Ordering::SeqCst) {
                return None;
            }
            let hook = match self.hook() {
                Ok(hook) => hook,
                Err(reason) => return Some(Err(reason)),
            };
            // Pair the context with its invalidation generation under one
            // cell lock (brief; no AX): a revision-changing publication or
            // a verified arrival since the last consumed generation
            // requests one whole-owner reread on this pump, even when the
            // backstop is silent and the revision is unchanged.
            let (ctx, generation) = hook.cell.load_versioned();
            let (generation_changed, seen) =
                super::consume_reread_generation(self.reread_seen_generation, generation);
            let Some(ctx) = ctx else {
                // A cleared cell refuses (existing fence); the loss is
                // still consumed once so it never re-requests on retry.
                self.reread_seen_generation = Some(seen);
                return Some(Err(format!(
                    "pid {}: native context cleared; refusing poll",
                    self.pid
                )));
            };
            // Backstop clock origin: first poll entry pins the epoch;
            // millisecond offsets stay on this owner thread.
            let origin = *self
                .backstop_origin
                .get_or_insert_with(std::time::Instant::now);
            let now_ms = origin.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            // Lazy observer creation inside the owner thread
            // (thread-affine). Creation failure reports honestly (fail
            // closed) rather than claiming an empty stream.
            if self.observer.is_none() {
                match crate::ffi::Phase1Observer::create(self.pid as i32, messaging_secs()) {
                    Ok(observer) => {
                        self.observer = Some(observer);
                        // Initial sync regardless of notification: static
                        // window sets need subscriptions now, not on the
                        // next app-level event (which may never come). The
                        // snapshot path syncs the window-level watches.
                        eprintln!(
                            "{}",
                            super::format_scoped_reread_line(
                                self.pid,
                                "initial",
                                crate::input_eligibility::now_ms()
                            )
                        );
                        // The initial read settles both marks: the
                        // generation is adopted (startup already published)
                        // and the revision anchors arrival attribution.
                        self.reread_seen_generation = Some(seen);
                        self.last_poll_revision = Some(ctx.revision);
                        let result = self.snapshot_scoped_native(&ctx);
                        self.note_backstop_read(now_ms, result.is_err());
                        return Some(result);
                    }
                    Err(reason) => {
                        // Creation failure reports honestly (fail closed)
                        // rather than claiming an empty stream, and still
                        // consumes the bump - no stacking; the backstop
                        // retries on its cadence and a new bump rereads.
                        self.reread_seen_generation = Some(seen);
                        return Some(Err(reason));
                    }
                }
            }
            let observer = self.observer.as_mut().expect("created above");
            observer.pump(NATIVE_PUMP_SLICE);
            if stop.load(Ordering::SeqCst) {
                return None;
            }
            let notified = observer.take_notified() > 0;
            // Observer notifications trigger rereads (the notification
            // itself carries no window data, only the wake to re-read); a
            // consumed invalidation bump rereads too - a changed topology
            // revision immediately, or a verified arrival even at an
            // unchanged revision. Silence past one interval rereads anyway
            // (bounded backstop for lost/unsupported notifications), with
            // the same cadence of backoff after a failed reread. Unchanged
            // rereads never republish: the worker drops equal snapshots, so
            // this cannot storm. Stop withholds everything.
            let backstop_due = super::scoped_backstop_due(
                stop.load(Ordering::SeqCst),
                notified,
                now_ms,
                self.backstop_last_read_ms,
                self.backstop_last_failure_ms,
                super::SCOPED_BACKSTOP_INTERVAL
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            );
            let never_read =
                self.backstop_last_read_ms.is_none() && self.backstop_last_failure_ms.is_none();
            let revision_changed = self
                .last_poll_revision
                .is_none_or(|last| last != ctx.revision);
            let trigger = super::select_scoped_reread_trigger(
                notified,
                generation_changed,
                revision_changed,
                backstop_due,
                never_read,
            )?;
            // Scoped reread boundary: name the trigger so a baseline can
            // tell a notification-driven reread from the silence backstop,
            // a topology publication, or a verified arrival.
            // Token-only (pid plus trigger plus the process-common clock,
            // comparable across the send-effect owner lines; the owner-local
            // backstop clock stays on the backstop arithmetic above, never
            // on this line).
            eprintln!(
                "{}",
                super::format_scoped_reread_line(
                    self.pid,
                    trigger,
                    crate::input_eligibility::now_ms()
                )
            );
            // Consume the bump with the reread: a burst rereads once and a
            // failed reread never replays - the backstop recovers and only
            // a new bump rereads again.
            self.reread_seen_generation = Some(seen);
            self.last_poll_revision = Some(ctx.revision);
            let result = self.snapshot_scoped_native(&ctx);
            self.note_backstop_read(now_ms, result.is_err());
            Some(result)
        }

        /// Record one completed poll reread for the backstop clock: success
        /// advances the read mark and clears failure backoff; failure marks
        /// backoff at the same cadence (never a fast retry).
        fn note_backstop_read(&mut self, now_ms: u64, failed: bool) {
            if failed {
                self.backstop_last_failure_ms = Some(now_ms);
            } else {
                self.backstop_last_read_ms = Some(now_ms);
                self.backstop_last_failure_ms = None;
            }
        }

        pub(super) fn dispatch_focus_native(&mut self, cmd: &FocusCommand) -> AxCompletion {
            use std::time::Instant;
            let start = Instant::now();
            let elapsed_ms = || start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            if cmd.pid != self.pid {
                return AxCompletion::Failed {
                    reason: "pid mismatch (stale command)".to_owned(),
                };
            }
            if cmd.wid == 0 {
                return AxCompletion::Failed {
                    reason: "refusing window id 0".to_owned(),
                };
            }
            let hook = match self.hook() {
                Ok(hook) => hook,
                Err(reason) => {
                    return AxCompletion::Failed { reason };
                }
            };
            let Some(ctx) = hook.cell.load() else {
                return AxCompletion::Failed {
                    reason: "no native context published; refusing".to_owned(),
                };
            };
            if cmd.revision != ctx.revision {
                return AxCompletion::Failed {
                    reason: "topology revision drift; superseded".to_owned(),
                };
            }
            let gates = NativeGates::for_op(
                self.pid,
                cmd.wid,
                &self.identity,
                &hook,
                None,
                None,
                self.scope.is_unscoped(),
            );
            if let Err(reason) = fence_scoped_focus(
                &self.scope,
                &self.identity,
                &gates,
                self.pid,
                cmd.wid,
                cmd.revision,
            ) {
                return AxCompletion::Failed { reason };
            }
            let mut setter = NativeSetter::new(self.pid, cmd.wid);
            match setter.set_focus(cmd.wid, cmd.route) {
                Ok(()) => AxCompletion::Applied {
                    elapsed_ms: elapsed_ms(),
                },
                Err(reason) => AxCompletion::Failed { reason },
            }
        }
    }

    /// Join the exact window element (fresh per call): finite messaging
    /// timeout before the app join, the window list, and every candidate
    /// join; a refused timeout fails the join, never an unbounded read.
    /// The owned handle never escapes the calling operation.
    fn join_window_element(pid: u32, wid: u32) -> Result<AxElement, String> {
        if pid <= 1 || pid > i32::MAX as u32 {
            return Err(format!("refusing out-of-range pid {pid}"));
        }
        let Some(app) = AxElement::application(pid as i32) else {
            return Err(format!("pid {pid}: no AX element for pid"));
        };
        if ax_set_messaging_timeout(&app, messaging_secs()) != 0 {
            return Err(format!("pid {pid}: messaging timeout unset"));
        }
        let windows = app
            .windows()
            .map_err(|code| format!("pid {pid}: windows: {}", crate::ffi::ax_error_name(code)))?;
        let total = windows.len();
        let mut refused = 0usize;
        for window in windows.into_iter() {
            if ax_set_messaging_timeout(&window, messaging_secs()) != 0 {
                refused += 1;
                continue;
            }
            if ax_window_id(&window) == Some(wid) {
                if ax_set_messaging_timeout(&window, messaging_secs()) != 0 {
                    return Err(format!("pid {pid} wid {wid}: messaging timeout unset"));
                }
                return Ok(window);
            }
        }
        if refused > 0 && refused == total {
            return Err(format!(
                "pid {pid} wid {wid}: messaging timeout unset for all candidates"
            ));
        }
        Err(format!("pid {pid} wid {wid}: not joined to owned pid"))
    }

    /// Live gates for one scoped operation on the owning thread. The
    /// allowlist itself rides the fence call (shared with the effect
    /// boundary); the gates only supply fresh live reads. Every fence read
    /// goes to the live source — revision from the shared cell, stop and
    /// supersession from the worker hook, frontmost from the freshest
    /// main-thread evidence — so a topology update, terminate, or lock
    /// mid-plan fences the very next leg. `target` pins the engine-planned
    /// domain (`None` derives live instead); `taken` pins the slot revision
    /// at apply entry for mid-batch supersession checks (`None` checks stop
    /// only, e.g. focus dispatch).
    pub struct NativeGates {
        pid: u32,
        wid: u32,
        identity: ProcessIdentity,
        hook: crate::runtime_backend::EffectHook,
        target: Option<tiler_core::session::DomainKey>,
        taken: Option<u64>,
        /// Product mode for identity revalidation: unscoped admits
        /// Ghostty as an ordinary candidate (scoped keeps refusing).
        unscoped: bool,
    }

    impl NativeGates {
        pub fn for_op(
            pid: u32,
            wid: u32,
            identity: &ProcessIdentity,
            hook: &crate::runtime_backend::EffectHook,
            target: Option<tiler_core::session::DomainKey>,
            taken: Option<u64>,
            unscoped: bool,
        ) -> Self {
            Self {
                pid,
                wid,
                identity: identity.clone(),
                hook: hook.clone(),
                target,
                taken,
                unscoped,
            }
        }

        fn live_identity(&self) -> Result<crate::ffi::NsAppIdentity, String> {
            if self.pid <= 1 || self.pid > i32::MAX as u32 {
                return Err(format!("refusing out-of-range pid {}", self.pid));
            }
            Ok(crate::ffi::ns_app_identity_for_pid(self.pid as i32))
        }

        fn live_context(&self) -> Result<Arc<NativeContext>, String> {
            self.hook.cell.load().ok_or_else(|| {
                format!(
                    "pid {}: native context cleared mid-plan; refusing",
                    self.pid
                )
            })
        }
    }

    impl TileGates for NativeGates {
        fn revalidate_identity(&self) -> Result<(), String> {
            let live = self.live_identity()?;
            let bundle = live.bundle.as_deref();
            let exe = live.exe.as_deref().unwrap_or_default();
            let start = live.launch_date.as_deref().unwrap_or_default();
            if live.launch_date.as_deref().is_none_or(|s| s.is_empty()) {
                return Err("identity launch date unreadable; refusing".to_owned());
            }
            // No bundle/exe/path values cross into the reason: PID numbers
            // and static tokens only. Unscoped product gates admit Ghostty
            // as an ordinary candidate; scoped gates keep refusing it.
            if self.unscoped {
                self.identity
                    .revalidate_unscoped(self.pid, bundle, exe, start)
                    .map_err(|_| "identity drift (possible PID reuse); refusing".to_owned())
            } else {
                self.identity
                    .revalidate(self.pid, bundle, exe, start)
                    .map_err(|_| "identity drift (possible PID reuse); refusing".to_owned())
            }
        }

        fn scope_bundle(&self) -> Option<String> {
            // Fresh live read per fence (never the bound copy): a bundle
            // change mid-lifetime must refuse here, not ride stale state.
            self.live_identity().ok().and_then(|live| live.bundle)
        }

        fn effect_alive(&self) -> Result<(), String> {
            use std::sync::atomic::Ordering;
            if self.hook.stop.load(Ordering::SeqCst) {
                return Err("worker stopped mid-plan; withholding".to_owned());
            }
            if let Some(taken) = self.taken
                && self.hook.frames.current_rev() != taken
            {
                return Err("superseded by a newer desired set mid-plan; withholding".to_owned());
            }
            Ok(())
        }

        fn session_safe(&self) -> Result<(), String> {
            // Last-known session from the freshest main-thread evidence
            // (never read off-main here) plus live Secure Input and the
            // actual window-server verdict (both live C reads, callable
            // off-main). Frames and focus never write the lock screen, an
            // unknown session, or Secure Input.
            super::session_safe_verdict(
                crate::ffi::secure_input_known(),
                self.hook.frontmost.clone(),
                crate::ffi::session_state().lock,
            )
        }

        fn cg_owner(&self, pid: u32, wid: u32) -> Result<(), String> {
            if pid != self.pid || wid != self.wid {
                return Err("fence target mismatch; refusing".to_owned());
            }
            let list = crate::ffi::cg_window_list_basic()
                .map_err(|_| "CG window list unreadable; refusing".to_owned())?;
            match list.into_iter().find(|w| w.wid == wid) {
                None => Err("wid absent from CG list; refusing".to_owned()),
                Some(entry) if entry.pid == pid => Ok(()),
                Some(_) => Err("wid owned by another pid; refusing".to_owned()),
            }
        }

        fn membership(&self, wid: u32) -> Result<Vec<u64>, String> {
            if wid != self.wid {
                return Err("fence target mismatch; refusing".to_owned());
            }
            crate::ffi::sls_spaces_for_window(wid)
                .map_err(|_| "Space membership unreadable; refusing".to_owned())
        }

        fn current_user_space(&self) -> Result<u64, String> {
            // Re-home live: the window's exact membership must equal some
            // domain's current Space, that display's row must still report
            // it current, and it must still read user-typed. Anything else
            // refuses (mid-transition duplicates included).
            let spaces = self.membership(self.wid)?;
            if spaces.len() != 1 {
                return Err("window not exactly on one Space; refusing".to_owned());
            }
            let current = spaces[0];
            let ctx = self.live_context()?;
            let domains: Vec<_> = ctx
                .domains
                .iter()
                .filter(|d| d.current && d.space == current)
                .collect();
            if domains.len() != 1 {
                return Err("current Space homed by != 1 domain; refusing".to_owned());
            }
            // Engine-planned domain pin: a window that crosses displays
            // mid-plan refuses the remaining legs instead of actuating
            // outside its planned domain (`None` derives live instead).
            if let Some(target) = &self.target
                && (domains[0].output != target.output || domains[0].workspace != target.workspace)
            {
                return Err("planned domain drifted mid-plan; refusing".to_owned());
            }
            let rows = crate::ffi::sls_display_rows()
                .map_err(|_| "display rows unreadable; refusing".to_owned())?;
            // Exact-UUID re-home (never positional): the domain's display
            // UUID must still map a row reporting this Space current. A
            // display without a UUID cannot re-home and refuses.
            let uuid = domains[0]
                .uuid
                .as_deref()
                .ok_or_else(|| "display UUID unknown; refusing positional home".to_owned())?;
            let home = rows
                .iter()
                .find(|row| row.identifier.as_deref() == Some(uuid));
            match home {
                Some(row) if row.current == Some(current) => {}
                _ => return Err("current Space drifted mid-plan; refusing".to_owned()),
            }
            match crate::ffi::sls_space_type(current) {
                Ok(crate::ffi::SlsSpaceType::User) => Ok(current),
                _ => Err("current Space no longer user-typed; refusing".to_owned()),
            }
        }

        fn topology_revision(&self) -> Result<u64, String> {
            Ok(self.live_context()?.revision)
        }

        fn show_state(&self, wid: u32) -> Result<WindowShowState, String> {
            if wid != self.wid {
                return Err("fence target mismatch; refusing".to_owned());
            }
            // Failure-closed AX legs: an unreadable fullscreen/minimized
            // state refuses the fence (never assumed clear), and an
            // unreadable hidden flag reads as hidden (withholds).
            let window = join_window_element(self.pid, self.wid)?;
            let fullscreen = ax_fullscreen_state(&window)
                .map_err(|_| "fullscreen unreadable; refusing".to_owned())?;
            let minimized = crate::ffi::ax_bool_attribute(&window, "AXMinimized")
                .map_err(|_| "minimized unreadable; refusing".to_owned())?;
            let hidden = crate::ffi::app_hidden_for_pid(self.pid as i32).unwrap_or(true);
            Ok(WindowShowState {
                fullscreen,
                minimized,
                hidden_app: hidden,
            })
        }
    }

    /// Live setters for one scoped operation on the owning thread. Each leg
    /// re-joins its element (bounded); nothing is retained across legs, so
    /// a mid-pass teardown cannot strand a stale handle.
    pub struct NativeSetter {
        pid: u32,
        wid: u32,
    }

    impl NativeSetter {
        pub fn new(pid: u32, wid: u32) -> Self {
            Self { pid, wid }
        }

        fn join(&self) -> Result<AxElement, String> {
            join_window_element(self.pid, self.wid)
        }
    }

    impl AxSetter for NativeSetter {
        fn set_position(&mut self, wid: u32, frame: &AxFrame) -> Result<(), String> {
            if wid != self.wid {
                return Err("setter target mismatch; refusing".to_owned());
            }
            let window = self.join()?;
            ax_set_position(&window, frame)
                .map_err(|code| format!("position: {}", crate::ffi::ax_error_name(code)))
        }

        fn set_size(&mut self, wid: u32, frame: &AxFrame) -> Result<(), String> {
            if wid != self.wid {
                return Err("setter target mismatch; refusing".to_owned());
            }
            let window = self.join()?;
            ax_set_size(&window, frame)
                .map_err(|code| format!("size: {}", crate::ffi::ax_error_name(code)))
        }

        fn set_focus(&mut self, wid: u32, route: FocusRoute) -> Result<(), String> {
            if wid != self.wid {
                return Err("setter target mismatch; refusing".to_owned());
            }
            match route {
                FocusRoute::Slps => {
                    // yabai `window_manager_focus_window_with_raise` parity:
                    // `_SLPSSetFrontProcessWithOptions` + the two key-window
                    // posts only front the app; the same-app key window moves
                    // via the `AXRaise` leg (`AXUIElementPerformAction`
                    // `kAXRaiseAction` on the exact joined window). SLPS-only
                    // dispatch can succeed without same-PID arrival; earlier
                    // cross-PID proofs did not exercise this missing leg.
                    // Dispatch is send-only: arrival needs the agreement
                    // oracle on a later observation, never this completion.
                    super::slps_focus_sequence(
                        || {
                            crate::focus::dispatch_focus(self.pid, wid)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        || {
                            let window = self.join()?;
                            ax_raise(&window).map_err(|code| {
                                format!("raise: {}", crate::ffi::ax_error_name(code))
                            })
                        },
                    )
                }
                FocusRoute::PublicApiSeam => {
                    // Exact public seam only: set the app's focused window
                    // to exactly this window, raise exactly this window,
                    // then activate exactly this PID. All three legs attempt
                    // regardless of individual failure (independent); the
                    // first failure cause reports. Nothing broader runs —
                    // bare activation alone is not target focus.
                    let window = self.join()?;
                    let app = AxElement::application(self.pid as i32)
                        .ok_or_else(|| "no AX element for pid".to_owned())?;
                    let mut first_err: Option<String> = None;
                    if let Err(code) = ax_set_focused_window(&app, &window) {
                        first_err =
                            Some(format!("set-focused: {}", crate::ffi::ax_error_name(code)));
                    }
                    if let Err(code) = ax_raise(&window)
                        && first_err.is_none()
                    {
                        first_err = Some(format!("raise: {}", crate::ffi::ax_error_name(code)));
                    }
                    if !crate::ffi::activate_app(self.pid as i32) && first_err.is_none() {
                        first_err = Some("public activation refused".to_owned());
                    }
                    match first_err {
                        None => Ok(()),
                        Some(reason) => Err(reason),
                    }
                }
            }
        }

        fn readback(&mut self, wid: u32) -> Result<Option<AxFrame>, String> {
            if wid != self.wid {
                return Err("setter target mismatch; refusing".to_owned());
            }
            let window = self.join()?;
            match ax_get_frame(&window) {
                Ok(frame) => Ok(Some(frame)),
                Err(code) => Err(format!("readback: {}", crate::ffi::ax_error_name(code))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn slps_raise_runs_after_dispatch_and_reports_both_on_raise_failure() {
        // Order pin: a dispatch refusal never reaches the raise leg.
        let raise_ran = Rc::new(Cell::new(false));
        let probe = Rc::clone(&raise_ran);
        let refused = slps_focus_sequence(
            || Err("slps: refused".to_owned()),
            || {
                probe.set(true);
                Ok(())
            },
        );
        assert_eq!(refused, Err("slps: refused".to_owned()));
        assert!(
            !raise_ran.get(),
            "raise must not run after refused dispatch"
        );

        // Partial-truth pin: dispatch sent, raise refused — the error says
        // both, since fronting already happened. Completion stays
        // send-truth only; arrival needs the agreement oracle later.
        let order = Rc::new(RefCell::new(Vec::new()));
        let dispatch_order = Rc::clone(&order);
        let raise_order = Rc::clone(&order);
        let failed = slps_focus_sequence(
            || {
                dispatch_order.borrow_mut().push("dispatch");
                Ok(())
            },
            || {
                raise_order.borrow_mut().push("raise");
                Err("raise: no error".to_owned())
            },
        );
        assert_eq!(*order.borrow(), vec!["dispatch", "raise"]);
        assert_eq!(failed, Err("dispatch sent but raise: no error".to_owned()));

        // Happy path sequences both legs exactly once.
        let order = Rc::new(RefCell::new(Vec::new()));
        let dispatch_order = Rc::clone(&order);
        let raise_order = Rc::clone(&order);
        assert_eq!(
            slps_focus_sequence(
                || {
                    dispatch_order.borrow_mut().push("dispatch");
                    Ok(())
                },
                || {
                    raise_order.borrow_mut().push("raise");
                    Ok(())
                },
            ),
            Ok(())
        );
        assert_eq!(*order.borrow(), vec!["dispatch", "raise"]);
    }

    fn reason_fixture(
        wid: u32,
        minimized: bool,
        subrole: Option<&str>,
    ) -> crate::native_facts::NativeReadOutcome {
        use crate::native_facts::NativeWindowFacts;
        crate::native_facts::NativeReadOutcome {
            pid: 4242,
            requested: 1,
            observed: 1,
            failed: 0,
            first_cause: String::new(),
            windows: vec![NativeWindowFacts {
                wid,
                role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                subrole: subrole.map(str::to_owned),
                fullscreen: Some(false),
                minimized: Some(minimized),
                modal: Some(false),
                size_settable: Some(true),
                min_size: None,
                max_size: None,
                frame: Some(AxFrame {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 600.0,
                }),
                cg_layer: Some(0),
                cg_onscreen: Some(true),
            }],
        }
    }

    fn reason_app() -> NativeAppFacts {
        NativeAppFacts {
            pid: 4242,
            hidden: false,
            activation_policy: crate::classifier::ActivationPolicy::Regular,
            is_own_bundle: false,
        }
    }

    #[test]
    fn scoped_snapshot_records_omission_reasons_without_changing_counts() {
        use crate::classifier::ExcludeReason;
        // A minimized window omits with its reason attached (wid plus
        // status name; never titles/content): the engine still sees an
        // omitted count of one and zero observed windows.
        let snap = build_scoped_snapshot(
            4242,
            9,
            &reason_fixture(7, true, Some(crate::classifier::SUBROLE_STANDARD)),
            &reason_app(),
            &|_| Ok(vec![11u64]),
            None,
            false,
            false,
        )
        .expect("complete facts snapshot");
        assert!(snap.windows.is_empty());
        assert!(snap.meta.is_empty());
        assert_eq!(snap.excluded, 1);
        assert_eq!(snap.excluded_reasons.len(), 1);
        assert_eq!(snap.excluded_reasons[0].wid, 7);
        assert_eq!(snap.excluded_reasons[0].reason, ExcludeReason::Minimized);
        assert_eq!(
            snap.excluded_reasons[0].reason.log_token(),
            "classifier-omitted-minimized"
        );

        // An unreadable subrole omits as UnknownRole (the same shape a
        // transient background-thread AX gap would take), not as a close.
        let snap = build_scoped_snapshot(
            4242,
            9,
            &reason_fixture(9, false, None),
            &reason_app(),
            &|_| Ok(vec![11u64]),
            None,
            false,
            false,
        )
        .expect("complete facts snapshot");
        assert!(snap.windows.is_empty());
        assert_eq!(snap.excluded, 1);
        assert_eq!(snap.excluded_reasons[0].wid, 9);
        assert_eq!(snap.excluded_reasons[0].reason, ExcludeReason::UnknownRole);

        // An ordinary standard window still admits with no reasons.
        let snap = build_scoped_snapshot(
            4242,
            9,
            &reason_fixture(7, false, Some(crate::classifier::SUBROLE_STANDARD)),
            &reason_app(),
            &|_| Ok(vec![11u64]),
            None,
            false,
            false,
        )
        .expect("complete facts snapshot");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.excluded, 0);
        assert!(snap.excluded_reasons.is_empty());
    }
}
