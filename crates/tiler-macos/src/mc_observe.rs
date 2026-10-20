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
    pub(crate) ax_windows_absent: Option<bool>,
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
    let mut ax_absent: std::collections::HashMap<u32, Option<bool>> =
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
        // Optional AX-window inventory proof for the ControlCenter rule:
        // only an exact-shape window consults (and populates) the per-PID
        // cache; every other window carries `None` without inserting, so an
        // earlier nonmatching window never blocks a later matching one.
        // Same calling thread, bounded by the existing messaging timeout;
        // any failure reads `None` (fail closed).
        let shape_matches = crate::mc_probe::controlcenter_shape_matches(
            &bundle,
            exe.as_deref(),
            w.layer,
            w.onscreen,
            w.bounds.w,
            w.bounds.h,
        );
        let ax_windows_absent = ax_proof_cached(&mut ax_absent, w.pid, shape_matches, || {
            read_ax_windows_absent(w.pid, &bundle, exe.as_deref(), exe_alias.as_deref())
        });
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
            ax_windows_absent,
            spaces,
        });
    }
    Ok(out)
}

/// Cache discipline for the optional AX proof: only an exact-shape window
/// consults the per-PID cache; any other window returns `None` without
/// inserting, so an earlier nonmatching window (onscreen item, panel, or
/// foreign owner) never blocks a later matching one. A genuine read result
/// (including a `None` failure) is cached once per PID per scan, so a
/// second matching window reuses it without a second IPC. Pure over the
/// injected read; the live caller passes [`read_ax_windows_absent`].
fn ax_proof_cached(
    cache: &mut std::collections::HashMap<u32, Option<bool>>,
    pid: u32,
    shape_matches: bool,
    read: impl FnOnce() -> Option<bool>,
) -> Option<bool> {
    if !shape_matches {
        return None;
    }
    if let Some(cached) = cache.get(&pid) {
        return *cached;
    }
    let proof = read();
    cache.insert(pid, proof);
    proof
}

/// Optional owner AX-window inventory proof for the ControlCenter rule:
/// `Some(true)` only when a fresh readable `AXWindows` read on the exact
/// owner PID completes empty; `Some(false)` when it completes nonempty -
/// an owner popup or any other inventory entry refuses, never exempts.
/// `None` when the app element, messaging timeout, or window list is
/// unreadable, or when the owner identity re-read after the IPC no longer
/// matches (fail closed on PID reuse). Constructs, uses, and drops every
/// `AxElement` on the calling thread; the only AX setting touched is the
/// existing finite messaging timeout. Counts every returned element of the
/// exact owner's inventory. No titles, no content, no paths.
fn read_ax_windows_absent(
    pid: u32,
    bundle: &str,
    exe: Option<&str>,
    exe_alias: Option<&str>,
) -> Option<bool> {
    if bundle != crate::mc_probe::CONTROLCENTER_BUNDLE
        || exe != Some(crate::mc_probe::CONTROLCENTER_EXE)
    {
        return None;
    }
    let pid_i32 = i32::try_from(pid).ok()?;
    let app = crate::ffi::AxElement::application(pid_i32)?;
    if crate::ffi::ax_set_messaging_timeout(&app, crate::mc_probe::MC_AX_TIMEOUT_SECS) != 0 {
        return None;
    }
    let windows = app.windows().ok()?;
    // PID-reuse fence: the identity that authorized this read must still
    // hold after the IPC; any drift voids the proof.
    let fresh_bundle = crate::ffi::bundle_id_for_pid(pid_i32);
    if fresh_bundle.as_deref() != Some(bundle) {
        return None;
    }
    let fresh_exe = crate::ffi::process_executable_path(pid_i32);
    if fresh_exe.as_deref() != exe_alias.or(exe) {
        return None;
    }
    Some(windows.is_empty())
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
                    ax_windows_absent: m.ax_windows_absent,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_proof_cache_skips_nonmatching_without_read_or_insert() {
        // Same owner PID, enumeration order as captured on Space 1: an
        // onscreen nonmatching item first, then the exact offscreen 120x30
        // shape. The first must neither read nor poison the cache; the
        // second reads exactly once and proves; a third matching window
        // reuses the cache.
        let mut cache = std::collections::HashMap::new();
        let mut reads = 0u32;
        assert_eq!(
            ax_proof_cached(&mut cache, 656, false, || {
                reads += 1;
                Some(true)
            }),
            None
        );
        assert_eq!(reads, 0, "nonmatching window must not read AX");
        assert!(!cache.contains_key(&656));
        assert_eq!(
            ax_proof_cached(&mut cache, 656, true, || {
                reads += 1;
                Some(true)
            }),
            Some(true)
        );
        assert_eq!(reads, 1);
        assert_eq!(
            ax_proof_cached(&mut cache, 656, true, || {
                reads += 1;
                Some(false)
            }),
            Some(true),
            "later matching window reuses the cached proof"
        );
        assert_eq!(reads, 1);
    }

    #[test]
    fn ax_proof_cache_records_genuine_failure_once() {
        // A genuine AX failure is cached once per PID per scan and stays
        // fail closed without a second IPC.
        let mut cache = std::collections::HashMap::new();
        let mut reads = 0u32;
        assert_eq!(
            ax_proof_cached(&mut cache, 656, true, || {
                reads += 1;
                None
            }),
            None
        );
        assert_eq!(
            ax_proof_cached(&mut cache, 656, true, || {
                reads += 1;
                Some(true)
            }),
            None
        );
        assert_eq!(reads, 1);
    }
}
