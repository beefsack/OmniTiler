#![forbid(unsafe_code)]

//! Owned CGWindowList identity with no AX dependency.
//!
//! Owned helper window IDs come from the public `CGWindowListCopyWindowInfo`
//! snapshot (maintained `core-graphics` binding, see [`crate::ffi`]), filtered
//! to the exact owned PID. No Accessibility consent is involved and no AX
//! call is made on any path here. Cross-command authentication needs CURRENT
//! live proof, not just caller-record equality: [`verify_live_nonce`]
//! requires a fresh snapshot to show a window owned by exactly the recorded
//! PID whose title carries exactly the recorded per-run nonce (see
//! [`crate::probe_target::probe_window_title`]). A stale record plus a
//! same-executable PID reuse carries a different nonce title, or none, and
//! is refused; an unprovable title (window server withholds names) fails
//! closed too. Only IDs, layers, on-screen flags, geometry, and the owned
//! nonce title are reported — never owner names, foreign titles, or content.

use crate::ownership::OwnedIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CgError {
    Unsupported(String),
    Ownership(String),
}

impl std::fmt::Display for CgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::Ownership(r) => write!(f, "ownership refused: {r}"),
        }
    }
}

impl std::error::Error for CgError {}

/// Resolve the live bundle ID for a PID and require the owned one. Caller
/// claims are not proof; the running-application table is. Directly spawned
/// helpers have no LaunchServices bundle entry: when no bundle is live the
/// spawn record's executable-path plus liveness check (done by the caller
/// via `authorize_owned_action` before this call) plus the fresh CG
/// owner-PID join below carry ownership, so a missing bundle with a live PID
/// defers to the record instead of failing.
fn live_owned_bundle(target_pid: u32) -> Result<String, CgError> {
    let pid_i32: i32 = target_pid
        .try_into()
        .map_err(|_| CgError::Ownership(format!("pid {target_pid} out of range")))?;
    match crate::ffi::bundle_id_for_pid(pid_i32).filter(|b| !b.is_empty()) {
        Some(bundle) => Ok(bundle),
        None if crate::ffi::process_exists(pid_i32) => Ok(crate::BUNDLE_ID.to_owned()),
        None => Err(CgError::Ownership(format!(
            "no live bundle for pid {target_pid} (not running or no bundle)"
        ))),
    }
}

/// Windows owned by exactly `pid` from a fresh CGWindowList snapshot.
/// Authenticates the owned session before touching the window server.
pub fn owned_windows(
    owned: &OwnedIdentity,
    pid: u32,
) -> Result<Vec<crate::ffi::CgWindow>, CgError> {
    if pid <= 1 {
        return Err(CgError::Ownership(format!("refusing reserved pid {pid}")));
    }
    let live_bundle = live_owned_bundle(pid)?;
    owned
        .check_action(pid, &live_bundle)
        .map_err(CgError::Ownership)?;
    let list = crate::ffi::cg_window_list_all().map_err(CgError::Unsupported)?;
    Ok(list.into_iter().filter(|w| w.pid == pid).collect())
}

/// Exact PID association plus live recheck for one window ID: the WID must be
/// present in a fresh snapshot and owned by exactly `pid`. A zero ID is
/// refused before the window server is touched. Used as the write gate for
/// every Space mutation including restore.
pub fn verify_wid_owned_by_pid(pid: u32, wid: u32) -> Result<crate::ffi::CgWindow, CgError> {
    if pid <= 1 {
        return Err(CgError::Ownership(format!("refusing reserved pid {pid}")));
    }
    if wid == 0 {
        return Err(CgError::Ownership("refusing window id 0".to_owned()));
    }
    let list = crate::ffi::cg_window_list_all().map_err(CgError::Unsupported)?;
    match list.into_iter().find(|w| w.wid == wid) {
        None => Err(CgError::Ownership(format!(
            "wid {wid} not present in CGWindowList; refusing without exact PID association"
        ))),
        Some(entry) if entry.pid == pid => Ok(entry),
        Some(entry) => Err(CgError::Ownership(format!(
            "wid {wid} belongs to pid {} (not owned {pid}); refusing",
            entry.pid
        ))),
    }
}

/// Pure live-nonce check over one fresh snapshot: at least one window owned
/// by exactly `pid` must carry exactly `expected_nonce` in its title.
/// `windows` is the full fresh snapshot (caller fetched it); only the
/// PID-filtered titles are compared. A same-executable PID reuse shows a
/// different nonce title (or none) and is refused — this is the check that
/// caller-record equality alone cannot do. Unit-tested below; live callers
/// must go through [`verify_live_nonce`] for a fresh snapshot.
pub fn check_live_nonce(
    pid: u32,
    expected_nonce: &str,
    windows: &[crate::ffi::CgWindow],
) -> Result<(), CgError> {
    if pid <= 1 {
        return Err(CgError::Ownership(format!("refusing reserved pid {pid}")));
    }
    let mut seen_owned_window = false;
    for w in windows.iter().filter(|w| w.pid == pid) {
        seen_owned_window = true;
        if crate::probe_target::title_nonce(w.title.as_deref()) == Some(expected_nonce.to_owned()) {
            return Ok(());
        }
    }
    if seen_owned_window {
        Err(CgError::Ownership(
            "live window nonce mismatch (possible PID reuse); refusing".to_owned(),
        ))
    } else {
        Err(CgError::Ownership(format!(
            "no live window for pid {pid} with the recorded nonce (unprovable or stale); refusing"
        )))
    }
}

/// Fresh-snapshot live-nonce proof for `pid`/`expected_nonce`: fetches a new
/// CGWindowList snapshot and applies [`check_live_nonce`]. A window-server
/// error or withheld titles fail closed — never an empty claim.
pub fn verify_live_nonce(pid: u32, expected_nonce: &str) -> Result<(), CgError> {
    let list = crate::ffi::cg_window_list_all().map_err(CgError::Unsupported)?;
    check_live_nonce(pid, expected_nonce, &list)
}

/// Owned AX title proof fallback for a withheld CG title.
///
/// Used ONLY when the CG live-nonce proof fails (window server withholds
/// `kCGWindowName`) AND Accessibility trust is actually observed true AND
/// the live executable already matches `expected_exe`. Flow: fresh CG
/// snapshot joined to exactly `pid` for the candidate WID set, then a
/// dedicated bounded AX worker queries ONLY those owned windows' `AXTitle`
/// attributes and compares the exact expected nonce title
/// ([`crate::probe_target::probe_window_title`]). Titles are never logged:
/// mismatches and failures report counts/reasons only.
///
/// Threading: every `AxElement` is constructed, used, and dropped on the
/// dedicated worker; the caller waits bounded (8s) and reports a timeout
/// rather than stalling. Off-macOS (or untrusted, or no live owned window)
/// this fails closed without touching AX.
pub fn verify_ax_title_nonce(
    pid: u32,
    expected_nonce: &str,
    expected_exe: &str,
) -> Result<(), CgError> {
    if pid <= 1 {
        return Err(CgError::Ownership(format!("refusing reserved pid {pid}")));
    }
    crate::ownership::validate_nonce(expected_nonce).map_err(CgError::Ownership)?;
    // Gate 1: AX trust actually observed true (non-prompting read only).
    // Anything else (false, unknown/off-platform) refuses the fallback.
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err(CgError::Ownership(
            "AX title fallback unavailable (Accessibility trust not observed true); refusing"
                .to_owned(),
        ));
    }
    // Gate 2: live executable already matches before any AX touch.
    crate::ownership::verify_live_process(pid, Some(expected_exe)).map_err(CgError::Ownership)?;
    // Gate 3: fresh CG PID/WID join — the candidate window set. No live
    // owned window means no fallback (never a bare-PID AX scan).
    let list = crate::ffi::cg_window_list_all().map_err(CgError::Unsupported)?;
    let mut candidate_wids: Vec<u32> = list
        .iter()
        .filter(|w| w.pid == pid)
        .map(|w| w.wid)
        .filter(|w| *w != 0)
        .take(crate::config::MAX_WINDOWS)
        .collect();
    if candidate_wids.is_empty() {
        return Err(CgError::Ownership(format!(
            "no live CG window for pid {pid}; AX title fallback refused"
        )));
    }
    candidate_wids.sort_unstable();
    candidate_wids.dedup();
    let expected_title = crate::probe_target::probe_window_title(expected_nonce);
    #[cfg(target_os = "macos")]
    {
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<bool, String>>();
        let pid_i32: i32 = pid
            .try_into()
            .map_err(|_| CgError::Ownership(format!("pid {pid} out of range")))?;
        std::thread::Builder::new()
            .name(format!("ax-title-{pid}"))
            .spawn(move || {
                // Every AX handle lives and dies on this worker.
                let matched: Result<bool, String> = (|| {
                    let app = crate::ffi::AxElement::application(pid_i32)
                        .ok_or_else(|| "no AX element".to_owned())?;
                    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
                    let windows = app.windows().map_err(crate::ffi::ax_error_name)?;
                    let mut checked = 0usize;
                    for window in &windows {
                        let wid = match crate::ffi::ax_window_id(window) {
                            Some(w) => w,
                            None => continue,
                        };
                        if !candidate_wids.contains(&wid) {
                            continue;
                        }
                        checked += 1;
                        // Exact nonce-title compare only; titles never leave
                        // this thread (no logging, no collection).
                        match crate::ffi::ax_window_title(window) {
                            Ok(Some(title)) if title == expected_title => return Ok(true),
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                    if checked == 0 {
                        return Err("no owned AX window joined the CG set".to_owned());
                    }
                    Err("owned AX titles carry no matching nonce".to_owned())
                })();
                let _ = done_tx.send(matched);
            })
            .map_err(|e| CgError::Unsupported(format!("AX title worker spawn: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(CgError::Ownership(
                "AX title fallback: no matching owned title".to_owned(),
            )),
            Ok(Err(e)) => Err(CgError::Ownership(format!(
                "AX title fallback refused: {e}"
            ))),
            Err(_) => Err(CgError::Unsupported(
                "AX title fallback timed out (bounded 8s)".to_owned(),
            )),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = expected_title;
        Err(CgError::Unsupported(
            "AX title fallback is macOS-only".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned() -> OwnedIdentity {
        OwnedIdentity::new(4242, crate::BUNDLE_ID, "0123456789abcdef").unwrap()
    }

    #[test]
    fn wid_zero_refused_before_os_touch() {
        assert!(matches!(
            verify_wid_owned_by_pid(4242, 0).unwrap_err(),
            CgError::Ownership(_)
        ));
    }

    #[test]
    fn reserved_pid_refused_before_os_touch() {
        assert!(matches!(
            verify_wid_owned_by_pid(1, 7).unwrap_err(),
            CgError::Ownership(_)
        ));
        assert!(matches!(
            owned_windows(&owned(), 1).unwrap_err(),
            CgError::Ownership(_)
        ));
    }

    #[test]
    fn foreign_pid_never_lists_owned_windows() {
        // No spawn record and no live owned bundle exist for this PID on any
        // platform: the live check fails closed before any window is listed.
        let id = owned();
        assert!(matches!(
            owned_windows(&id, 4243).unwrap_err(),
            CgError::Ownership(_)
        ));
    }

    #[test]
    fn pid_mismatch_fails_closed() {
        let id = owned();
        assert!(matches!(
            owned_windows(&id, 4242).unwrap_err(),
            CgError::Ownership(_) | CgError::Unsupported(_)
        ));
    }

    fn window(pid: u32, title: Option<&str>) -> crate::ffi::CgWindow {
        crate::ffi::CgWindow {
            wid: 7,
            pid,
            layer: 0,
            onscreen: true,
            bounds: crate::ffi::CgBounds {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            title: title.map(str::to_owned),
        }
    }

    #[test]
    fn live_nonce_requires_exact_current_title() {
        use crate::probe_target::probe_window_title;
        let nonce = "0123456789abcdef0123456789abcdef";
        let live = probe_window_title(nonce);
        // Exact current title passes.
        assert!(
            check_live_nonce(
                4242,
                nonce,
                &[
                    window(4242, Some(live.as_str())),
                    window(9999, Some("other"))
                ]
            )
            .is_ok()
        );
    }

    #[test]
    fn live_nonce_rejects_same_exe_stale_pid() {
        use crate::probe_target::probe_window_title;
        let recorded = "0123456789abcdef0123456789abcdef";
        // Same PID reused by a same-executable helper on a new run shows the
        // NEW run's nonce title: the stale recorded nonce is refused even
        // though PID and executable still match.
        let reused = probe_window_title("ffffffffffffffffffffffffffffffff");
        assert!(matches!(
            check_live_nonce(4242, recorded, &[window(4242, Some(reused.as_str()))]).unwrap_err(),
            CgError::Ownership(_)
        ));
        // Reused PID with no windows at all (or untitled windows) is
        // refused too — never an empty pass.
        assert!(matches!(
            check_live_nonce(4242, recorded, &[]).unwrap_err(),
            CgError::Ownership(_)
        ));
        assert!(matches!(
            check_live_nonce(4242, recorded, &[window(4242, None)]).unwrap_err(),
            CgError::Ownership(_)
        ));
        // Windows of other PIDs never satisfy this PID's nonce.
        assert!(matches!(
            check_live_nonce(
                4242,
                recorded,
                &[window(4243, Some(probe_window_title(recorded).as_str()))]
            )
            .unwrap_err(),
            CgError::Ownership(_)
        ));
        assert!(matches!(
            check_live_nonce(1, recorded, &[window(1, None)]).unwrap_err(),
            CgError::Ownership(_)
        ));
    }

    #[test]
    fn ax_title_fallback_refuses_reserved_pid_before_any_touch() {
        let err = verify_ax_title_nonce(1, "0123456789abcdef0123456789abcdef", "/nonexistent")
            .unwrap_err();
        assert!(matches!(err, CgError::Ownership(_)));
    }

    #[test]
    fn ax_title_fallback_needs_observed_trust_and_never_logs_titles() {
        use crate::probe_target::{probe_window_title, title_nonce};
        let nonce = "0123456789abcdef0123456789abcdef";
        // The scheme is exact-compare only: the helper title round-trips
        // the nonce, and anything else parses to None.
        assert_eq!(
            title_nonce(Some(probe_window_title(nonce).as_str())),
            Some(nonce.to_owned())
        );
        // Dead PID with no live exe: refused before any AX query, with no
        // title material in the error.
        let err = verify_ax_title_nonce(4242, nonce, "/nonexistent-helper").unwrap_err();
        let text = err.to_string();
        assert!(!text.contains("OmniTiler owned probe "));
        assert!(matches!(
            err,
            CgError::Ownership(_) | CgError::Unsupported(_)
        ));
    }
}
