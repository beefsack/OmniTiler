#![forbid(unsafe_code)]

//! Read-only Mission Control observation helpers shared by the lifecycle,
//! recovery, members, and switch-probe batteries. Split out of
//! [`crate::mc_lifecycle`] to keep that module under the production cap;
//! [`crate::mc_lifecycle`] re-exports these paths so existing callers are
//! unaffected. No live dispatch here: fresh CG/SLS reads plus the pure
//! [`crate::mc_probe`] gates only. No `unsafe`, no writes, no synthetic
//! events.

use crate::mc_probe::{
    FocusLegs, GHOSTTY_SUBSTR, GhosttyWindow, McError, verify_space_empty_members,
};

/// One enumerated window with its owner bundle and membership read. The
/// membership `Err` is data (unknown), never skipped: candidates with unknown
/// membership block removal.
pub(crate) struct WindowMemberLive {
    pub(crate) wid: u32,
    pub(crate) pid: u32,
    pub(crate) layer: i32,
    pub(crate) onscreen: bool,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) bundle: String,
    pub(crate) exe: Option<String>,
    pub(crate) exe_alias: Option<String>,
    pub(crate) spaces: Result<Vec<u64>, String>,
}

/// Fresh full CG + SLS membership enumeration. Fails closed at the
/// truncation bound (completeness unknown) or when unreadable. Read-only:
/// IDs, geometry, owner identity (bundle plus fresh executable path),
/// membership — never titles. No geometry is ever written here.
pub(crate) fn enumerate_window_members() -> Result<Vec<WindowMemberLive>, McError> {
    let all = crate::ffi::cg_window_list_all()
        .map_err(|e| McError::Unsupported(format!("CG window list unreadable: {e}")))?;
    if all.len() >= crate::config::MAX_WINDOWS {
        return Err(McError::Blocked(format!(
            "CG snapshot at the {}-window bound; completeness unknown — refusing",
            crate::config::MAX_WINDOWS
        )));
    }
    let mut bundles = std::collections::HashMap::new();
    let mut exes: std::collections::HashMap<u32, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    let mut out = Vec::with_capacity(all.len());
    for w in &all {
        let bundle = bundles
            .entry(w.pid)
            .or_insert_with(|| {
                i32::try_from(w.pid)
                    .ok()
                    .and_then(crate::ffi::bundle_id_for_pid)
                    .unwrap_or_default()
            })
            .clone();
        // Versioned-alias normalization happens once per PID here (never in
        // the pure classifier): only the compositor rule consumes it, and
        // ownership paths keep the raw `process_executable_path` read.
        let (exe, exe_alias) = exes
            .entry(w.pid)
            .or_insert_with(|| {
                let raw = i32::try_from(w.pid)
                    .ok()
                    .and_then(crate::ffi::process_executable_path);
                crate::mc_probe::normalize_windowserver_alias(raw)
            })
            .clone();
        let spaces = crate::ffi::sls_spaces_for_windows(&[w.wid]);
        out.push(WindowMemberLive {
            wid: w.wid,
            pid: w.pid,
            layer: w.layer,
            onscreen: w.onscreen,
            x: w.bounds.x,
            y: w.bounds.y,
            w: w.bounds.w,
            h: w.bounds.h,
            bundle,
            exe,
            exe_alias,
            spaces,
        });
    }
    Ok(out)
}

/// Fresh topology user-SID set for the sticky-chrome rule: every Space in
/// the current snapshot whose type reads User, in snapshot order.
/// Non-user Spaces are simply not members of the set; an unreadable Space
/// type fails closed — the user set is unprovable, so no stickiness
/// exemption can be granted.
pub(crate) fn fresh_user_spaces() -> Result<Vec<u64>, McError> {
    let snapshot = crate::mc_lifecycle::live::fresh_snapshot()?;
    let mut user = Vec::new();
    for sid in snapshot.all_sids() {
        match crate::ffi::sls_space_type(sid) {
            Ok(crate::ffi::SlsSpaceType::User) => {
                if !user.contains(&sid) {
                    user.push(sid);
                }
            }
            Ok(_) => {}
            Err(cause) => {
                return Err(McError::Unsupported(format!(
                    "user-Space set: Space {sid} type unreadable: {cause}"
                )));
            }
        }
    }
    Ok(user)
}

/// Temp-emptiness scan: fresh full enumeration plus the fresh user-SID set,
/// through the pure user-window gate. Construction history is not evidence;
/// only this scan adopts removal.
pub(crate) fn verify_space_empty(temp: u64) -> Result<crate::mc_probe::EmptySummary, McError> {
    let user_spaces = fresh_user_spaces()?;
    if !user_spaces.contains(&temp) {
        return Err(McError::Ownership(format!(
            "temp Space {temp} is not in the fresh user-Space set {user_spaces:?}; ownership uncertain — cleanup blocked"
        )));
    }
    let live = enumerate_window_members()?;
    let pairs: Vec<(crate::mc_probe::WindowMember, Result<Vec<u64>, String>)> = live
        .into_iter()
        .map(|m| {
            (
                crate::mc_probe::WindowMember {
                    wid: m.wid,
                    pid: m.pid,
                    bundle: m.bundle,
                    layer: m.layer,
                    onscreen: m.onscreen,
                    w: m.w,
                    h: m.h,
                    exe: m.exe,
                    exe_alias: m.exe_alias,
                },
                m.spaces,
            )
        })
        .collect();
    verify_space_empty_members(&pairs, temp, &user_spaces)
}

/// Ghostty protection snapshot: every Ghostty-owned window's identity,
/// geometry, and membership, sorted by WID. Read-only; compared for equality
/// before/after. Inclusion is substring-broad so no ordinary Ghostty window
/// is ever missed; nothing here writes.
pub(crate) fn snapshot_ghostty_windows() -> Result<Vec<GhosttyWindow>, McError> {
    let live = enumerate_window_members()?;
    let mut out: Vec<GhosttyWindow> = live
        .into_iter()
        .filter(|m| m.bundle.to_ascii_lowercase().contains(GHOSTTY_SUBSTR))
        .map(|m| GhosttyWindow {
            wid: m.wid,
            pid: m.pid,
            layer: m.layer,
            onscreen: m.onscreen,
            x: m.x,
            y: m.y,
            w: m.w,
            h: m.h,
            spaces: m.spaces.ok(),
        })
        .collect();
    out.sort_by_key(|g| g.wid);
    Ok(out)
}

/// MC's AX group disappears before its CG thumbnail transforms settle.
/// Observe exact protected-window restoration for up to three seconds; never
/// write/focus those windows. A persistent difference remains a hard refusal.
pub(crate) fn await_ghostty_restored(before: &[GhosttyWindow]) -> Result<(), McError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let after = snapshot_ghostty_windows()?;
        if crate::mc_probe::ghostty_stable_across_switch(before, &after) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(McError::Blocked(
                "protected CG frame/membership did not return after MC transition".to_owned(),
            ));
        }
        crate::ffi::pump_runloop_once(50);
    }
}

/// Selected-oracle focus legs (read-only): workspace frontmost, SLPS front
/// PID, and the direct per-app focused-window read when the two agree.
/// Unknown legs stay unknown; callers (not this capture) decide.
pub(crate) fn capture_focus_legs() -> FocusLegs {
    let ws = crate::ffi::frontmost_app();
    let slps = crate::ffi::slps_front_pid().ok();
    let (direct, readable) = match (ws.as_ref().map(|(pid, _)| *pid), slps) {
        (Some(w), Some(s)) if w == s && w > 1 => {
            let read = crate::ffi::ax_direct_focused_read(w);
            let ok = read.ax_pid == Some(w);
            (Some(read), ok)
        }
        _ => (None, false),
    };
    FocusLegs {
        ws_pid: ws.as_ref().map(|(pid, _)| *pid),
        ws_bundle: ws.and_then(|(_, bundle)| {
            if bundle.is_empty() {
                None
            } else {
                Some(bundle)
            }
        }),
        slps_pid: slps,
        direct,
        direct_readable: readable,
    }
}
