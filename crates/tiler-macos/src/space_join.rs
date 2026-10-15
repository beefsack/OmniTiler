#![forbid(unsafe_code)]

//! Owned public Space-join self-action control (cross-command trigger).
//!
//! The helper owns its `NSWindow` exclusively: only the helper process can
//! set that window's `collectionBehavior` via public AppKit. This module is
//! the controller side — it never touches windows itself. Per action it
//! authenticates the exact spawn record (`pid`/`nonce`, expected helper,
//! live exe plus the CURRENT live window nonce, same as every cross-command
//! probe), requires the window-server session to read unlocked, revalidates
//! the record unchanged plus a fresh CG owner join immediately before
//! publishing, writes one nonce-authenticated trigger carrying a fresh
//! cryptographic per-request operation ID, polls bounded for the helper's
//! atomic result file echoing `{session nonce, request_id}`, and reports the
//! helper's own preimage/restore readback. No SLS, no synthetic events, no
//! AX writes.
//!
//! Single-flight exclusivity: the trigger file is created with `create_new`
//! (mode `0600` at creation), so a second concurrent action for the same
//! session is refused instead of overwriting — no concurrent same-session
//! actions, no stale proof. The result binds to exactly one trigger via the
//! echoed `request_id`; survivor files from earlier actions are ignored,
//! never consumed. All trigger/result files survive process exit (plain
//! files under the private session dir), so any separately launched
//! controller holding the same `pid`/`nonce` record can trigger while the
//! helper lives (same currency as `spaces-restore`); the primary holder
//! never owns the trigger exclusively. Helpers stay finite-lifetime: the
//! exact `Child` holder still reaps on its deadline, and this command sends
//! no signals — only the request file, consumed once by the helper (never
//! replayed).
//!
//! Prior art (local checkouts only; pinned upstream URLs): none of the
//! surveyed non-SA movers use `MoveToActiveSpace` as a recovery — yabai
//! moves via bridged/direct/compat SLS routes plus SA, Hammerspoon via
//! private dispatch plus AX Mission Control create/goto/remove, Silica via
//! titlebar drag plus a resolved system chord:
//! https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
//! https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L116-L199
//! https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIWindow.m#L210-L262
//! This self-action is therefore new fixture behavior, not a source-derived
//! route; its Space effect is proven per-run by SLS membership readback
//! harness-side, never assumed from the AppKit flag name or this return.

use serde::Serialize;

/// Bounded outcome poll: helper acts within one pump cycle (100 ms poll)
/// plus the 700 ms bounded AppKit settle inside the self-action, so 10 s
/// covers even a loaded runloop with margin and never waits out a helper
/// lifetime.
pub const JOIN_TIMEOUT_MS: u64 = 10_000;
/// Result-file poll slice.
pub const JOIN_POLL_MS: u64 = 100;
/// Trigger/result body cap: bodies are small fixed JSON; anything larger is
/// never ours and is left in place.
pub const JOIN_FILE_CAP: u64 = 4096;

/// One consumed self-action outcome: the helper's behavior preimage/restore
/// evidence bound to exactly one trigger via the echoed operation ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinEvidence {
    pub request_id: String,
    pub preimage_bits: u64,
    pub restored_bits: u64,
    pub restored_verified: bool,
    pub cg_owner_windows: usize,
}

/// Parse and authenticate one helper result body against the expected
/// session nonce plus the expected per-request operation ID. Returns the
/// behavior evidence. Refuses corrupt JSON, missing fields, nonce mismatch,
/// and request-ID mismatch before any other interpretation — a survivor
/// result from an earlier action with the same session nonce is ignored
/// here (the caller keeps polling for its own), never consumed.
pub fn parse_join_result(
    body: &str,
    expected_nonce: &str,
    expected_request_id: &str,
) -> Result<(u64, u64, bool), String> {
    let value: crate::probe_target::SpaceJoinResult =
        serde_json::from_str(body).map_err(|_| "space-join result not JSON".to_owned())?;
    if value.nonce != expected_nonce {
        return Err("space-join result nonce mismatch; refusing".to_owned());
    }
    if value.request_id != expected_request_id {
        return Err("space-join result request_id mismatch; refusing".to_owned());
    }
    Ok((
        value.preimage_bits,
        value.restored_bits,
        value.restored_verified,
    ))
}

/// Pure outcome-poll over a scripted reader: returns the first `Some` body
/// within `deadline_ms`, else `None`. The live boundary below passes a file
/// reader; tests pass scripted closures (immediate, delayed, never).
pub fn await_result_text(
    read: &mut dyn FnMut() -> Option<String>,
    deadline_ms: u64,
    poll_ms: u64,
) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(deadline_ms);
    loop {
        if let Some(body) = read() {
            return Some(body);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms.min(500)));
    }
}

/// Bounded regular-file read for trigger/result bodies: no symlinks, plain
/// file only, capped length. Returns `None` for absent/unreadable/foreign
/// files — never follows a symlink, never reads unbounded.
pub(crate) fn read_join_file(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return None;
    }
    if meta.len() > JOIN_FILE_CAP {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Publish one trigger for an already-authenticated session: session-lock
/// gate, unchanged-record revalidation, and fresh CG owner join immediately
/// before the write, then an exclusive `create_new` publish (mode `0600` at
/// creation). Returns the fresh per-request operation ID. Refusals publish
/// nothing (exit 2 shapes at the command boundary).
pub(crate) fn publish_join_trigger(
    pid: u32,
    nonce: &str,
    original: &crate::probe_target::SpawnRecord,
) -> Result<String, String> {
    // Session-lock gate immediately before publishing: unknown or locked
    // refuses with no effect (never arm a self-activation under lock).
    if !crate::ffi::session_state().unlocked() {
        return Err(
            "window-server session not unlocked (unknown or locked); refusing to publish"
                .to_owned(),
        );
    }
    // Narrow the check-to-use window: the entry snapshot must still equal
    // current (a restart or PID reuse between entry and publish fails
    // closed), plus a fresh CG owner join — no selection, just existence.
    crate::probe_target::verify_record_unchanged(original, pid, nonce)
        .map_err(|e| format!("spawn record changed since command entry: {e}"))?;
    let owned_count = crate::ffi::cg_window_list_all()
        .map_err(|e| format!("CG window list unreadable: {e}"))?
        .iter()
        .filter(|w| w.pid == pid)
        .count();
    if owned_count == 0 {
        return Err(format!("no live CG window for owned pid {pid}; refusing"));
    }
    crate::probe_target::ensure_private_session_dir()?;
    let request_id = crate::probe_target::mint_nonce()
        .map_err(|e| format!("request_id entropy unavailable: {e}"))?;
    let body = crate::probe_target::space_join_request_body(nonce, &request_id)?;
    publish_body_exclusive(&crate::probe_target::space_join_request_path(nonce), &body)?;
    Ok(request_id)
}

/// Exclusive trigger-body publish shared by both entrypoints: `create_new`
/// (mode `0600` at creation, one small write + sync). An already-pending
/// trigger refuses instead of overwriting — single in-flight per session.
fn publish_body_exclusive(request_path: &std::path::Path, body: &str) -> Result<(), String> {
    #[cfg(unix)]
    let open = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(request_path)
    };
    #[cfg(not(unix))]
    let open = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(request_path);
    let mut file = open.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            "join already in flight for this session (trigger pending); refusing overwrite"
                .to_owned()
        } else {
            format!("trigger publish: {e}")
        }
    })?;
    {
        use std::io::Write;
        file.write_all(body.as_bytes())
            .map_err(|e| format!("trigger publish: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("trigger publish: {e}"))?;
    }
    drop(file);
    Ok(())
}

/// Held-Child proof: the exact unreaped holder `Child` plus its PID, the
/// pipe-proven per-run nonce, the expected helper executable, and the owned
/// window join — bound WITHOUT any window-title read, so it holds
/// off-current where the cross-command title proof cannot. Fields are
/// private: the ONLY constructor is [`prove_held_child`], which re-runs the
/// existing holder verification routine
/// ([`crate::focus::recheck_holder_before_mutation`]: exact-handle
/// `try_wait` liveness, expected exe, fresh CG owner join). There is no
/// bypass flag and no orphan mode: without the live held handle this type
/// cannot exist, and the direct cross-command entrypoint
/// ([`run_owned_space_join`]) never accepts it — it keeps the title check.
/// Capability token: every field was verified at construction time (see
/// [`prove_held_child`]), so later reads are unnecessary — holding the
/// value proves the checks ran. Only `nonce` is consumed downstream (as the
/// trigger body); the rest are construction-checked bindings.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct HeldProof {
    pid: u32,
    nonce: String,
    helper_path: String,
    wid: u32,
    cg_owner_windows: usize,
}

/// Prove holder ownership through the exact held `Child`: pipe-proven nonce
/// format, reserved-ID refusal, handle-to-claim PID equality, then the
/// existing holder routine (live handle, expected exe, fresh CG owner
/// join). Refusals happen before any trigger file is touched.
pub(crate) fn prove_held_child(
    child: &mut std::process::Child,
    pid: u32,
    nonce: &str,
    helper_path: &str,
    wid: u32,
) -> Result<HeldProof, String> {
    crate::ownership::validate_nonce(nonce)?;
    if pid <= 1 {
        return Err(format!("refusing reserved pid {pid}"));
    }
    if wid == 0 {
        return Err("refusing window id 0".to_owned());
    }
    if child.id() != pid {
        return Err("held Child PID differs from claimed pid; refusing".to_owned());
    }
    crate::focus::recheck_holder_before_mutation(child, pid, helper_path, wid)
        .map_err(|e| format!("held-Child recheck refused: {e}"))?;
    let cg_owner_windows = crate::ffi::cg_window_list_all()
        .map_err(|e| format!("CG window list unreadable: {e}"))?
        .iter()
        .filter(|w| w.pid == pid)
        .count();
    Ok(HeldProof {
        pid,
        nonce: nonce.to_owned(),
        helper_path: helper_path.to_owned(),
        wid,
        cg_owner_windows,
    })
}

/// Publish one trigger from a held-Child proof: session-lock gate plus the
/// exclusive single-flight publish. The proof itself is the
/// authentication — proven by the caller through [`prove_held_child`]
/// immediately before this call, with no title read on either side.
/// Returns the fresh per-request operation ID. Refusals publish nothing.
pub(crate) fn publish_join_trigger_held(proof: &HeldProof) -> Result<String, String> {
    // Session-lock gate immediately before publishing: unknown or locked
    // refuses with no effect (never arm a self-activation under lock).
    if !crate::ffi::session_state().unlocked() {
        return Err(
            "window-server session not unlocked (unknown or locked); refusing to publish"
                .to_owned(),
        );
    }
    crate::probe_target::ensure_private_session_dir()?;
    let request_id = crate::probe_target::mint_nonce()
        .map_err(|e| format!("request_id entropy unavailable: {e}"))?;
    let body = crate::probe_target::space_join_request_body(&proof.nonce, &request_id)?;
    publish_body_exclusive(
        &crate::probe_target::space_join_request_path(&proof.nonce),
        &body,
    )?;
    Ok(request_id)
}

/// Await the helper's result echoing our `{session nonce, request_id}`:
/// foreign/stale bodies are skipped (never consumed), the match is consumed
/// (read + removed) exactly once. On timeout the pending trigger is removed
/// only when its content still equals our published body — another
/// controller's trigger is never deleted.
pub(crate) fn await_join_result(
    pid: u32,
    nonce: &str,
    request_id: &str,
    our_body: &str,
) -> Result<JoinEvidence, String> {
    let result_path = crate::probe_target::space_join_result_path(nonce);
    let request_path = crate::probe_target::space_join_request_path(nonce);
    let owned_count = crate::ffi::cg_window_list_all()
        .map_err(|e| format!("CG window list unreadable: {e}"))?
        .iter()
        .filter(|w| w.pid == pid)
        .count();
    let read = || read_join_file(&result_path);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(JOIN_TIMEOUT_MS);
    loop {
        if let Some(raw) = read() {
            match parse_join_result(&raw, nonce, request_id) {
                Ok((preimage_bits, restored_bits, restored_verified)) => {
                    // Consume exactly our result so a later action with the
                    // same session nonce never reads it stale.
                    let _ = std::fs::remove_file(&result_path);
                    return Ok(JoinEvidence {
                        request_id: request_id.to_owned(),
                        preimage_bits,
                        restored_bits,
                        restored_verified,
                        cg_owner_windows: owned_count,
                    });
                }
                Err(e) if e.contains("request_id") || e.contains("nonce") => {
                    // Someone else's (or a survivor) result: leave it, keep
                    // polling for ours.
                }
                Err(e) => return Err(e),
            }
        }
        if std::time::Instant::now() >= deadline {
            // Timeout: remove the pending trigger only when it is still
            // exactly ours (never another controller's, never a fresh one).
            if read_join_file(&request_path).as_deref() == Some(our_body) {
                let _ = std::fs::remove_file(&request_path);
            }
            return Err(format!(
                "helper did not answer within {JOIN_TIMEOUT_MS}ms (was it spawned with --space-join and still alive? trigger consumed once, never replayed)"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(JOIN_POLL_MS));
    }
}

#[derive(Debug, Clone, Serialize)]
struct JoinReport {
    outcome: String,
    pid: u32,
    request_id: String,
    cg_owner_windows: usize,
    preimage_bits: u64,
    restored_bits: u64,
    restored_verified: bool,
    synthetic_events_posted: bool,
    live_evidence: String,
    harness_hint: String,
}

/// Trigger one owned public Space-join: helper performs its own
/// `MoveToActiveSpace` self-action on its window, then restores its exact
/// `collectionBehavior`. Exit codes: 2 usage/authentication/gate refusal
/// (nothing published), 1 trigger/timeout/restore failure with evidence.
///
/// The `self-action-complete` outcome reports helper-side behavior
/// preimage + exact-restore readback only — it never claims a Space
/// membership effect. The membership proof is harness-side SLS readback
/// (`spaces-diagnose`), named in `harness_hint` and required by the
/// recovery harness before any verdict.
pub fn run_owned_space_join(pid: u32, nonce: &str) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    if pid <= 1 {
        return Err((2, format!("refusing reserved pid {pid}")));
    }
    // Full spawn-record proof (pid/nonce, expected helper, live exe plus
    // CURRENT live window nonce) — never caller claims alone. Same currency
    // as every cross-command probe, so this works after the mover exits.
    let original = crate::probe_target::load_verified_record(pid, nonce).map_err(|e| (2, e))?;
    let request_id = publish_join_trigger(pid, nonce, &original).map_err(|e| (2, e))?;
    let our_body =
        crate::probe_target::space_join_request_body(nonce, &request_id).map_err(|e| (2, e))?;
    let evidence =
        await_join_result(pid, nonce, &request_id, &our_body).map_err(|e| err_json("join", e))?;
    let behavior_ok =
        crate::probe_target::space_join_restored_ok(evidence.preimage_bits, evidence.restored_bits);
    let verified = evidence.restored_verified && behavior_ok;
    let report = JoinReport {
        outcome: "self-action-complete".to_owned(),
        pid,
        request_id,
        cg_owner_windows: evidence.cg_owner_windows,
        preimage_bits: evidence.preimage_bits,
        restored_bits: evidence.restored_bits,
        restored_verified: verified,
        synthetic_events_posted: false,
        live_evidence: "helper-side collectionBehavior preimage + exact restore readback; Space membership effect needs harness-side SLS readback (spaces-diagnose exact [current]) plus frame equality, never assumed here"
            .to_owned(),
        harness_hint: format!(
            "spaces-diagnose --wid <WID> / cg-owned-windows --pid {pid} --nonce <NONCE> for exact [current] membership, or spaces-owned-recovery for the full O->T->O battery"
        ),
    };
    let body = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
    if verified { Ok(body) } else { Err((1, body)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_refuses_reserved_pid_before_live_touch() {
        let err = run_owned_space_join(1, "0123456789abcdef0123456789abcdef").unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn join_refuses_foreign_session_before_live_touch() {
        // No spawn record exists: refused on pid/nonce mismatch before any
        // CG read, file write, or poll. Far PID avoids any live process.
        let err =
            run_owned_space_join(u32::MAX - 4242, "0123456789abcdef0123456789abcdef").unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn direct_publish_path_requires_title_record() {
        // The direct (cross-command) publish entrypoint revalidates the
        // spawn record — including the CURRENT live window-nonce title —
        // before any trigger write. With no record at all it refuses, even
        // though no Child handle exists to consult: the title proof is
        // mandatory here and is never skipped by a flag.
        let original = crate::probe_target::SpawnRecord {
            pid: u32::MAX - 4243,
            nonce: "0123456789abcdef0123456789abcdef".to_owned(),
            helper_path: "x".to_owned(),
        };
        let err = publish_join_trigger(original.pid, &original.nonce, &original).unwrap_err();
        assert!(
            err.contains("record") || err.contains("refusing"),
            "unexpected: {err}"
        );
        // No trigger file was published for the refused session.
        assert!(
            read_join_file(&crate::probe_target::space_join_request_path(
                &original.nonce
            ))
            .is_none()
        );
    }

    /// Spawn a short-lived helper process for holder-path tests. The exact
    /// handle is always reaped by the caller (kill + wait); nothing is
    /// killed by name and nothing escapes the test.
    fn spawn_sleep() -> std::process::Child {
        std::process::Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sleep helper for holder test")
    }

    #[test]
    fn held_proof_refuses_exited_child_before_exe_touch() {
        // An already-exited handle is refused on the wait state alone: no
        // exe check, no CG join, no title read. `true` exits immediately.
        let mut child = std::process::Command::new("true")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("true helper for holder test");
        let pid = child.id();
        let _ = child.wait();
        let err = prove_held_child(
            &mut child,
            pid,
            "0123456789abcdef0123456789abcdef",
            "/nonexistent/tiler-macos-probe-target",
            7,
        )
        .unwrap_err();
        assert!(err.contains("exited"), "unexpected: {err}");
    }

    #[test]
    fn held_proof_refuses_pid_mismatch_without_signal() {
        // Handle-to-claim PID equality is checked before the live recheck:
        // a wrong PID refuses without signalling or touching the desktop.
        // The live `sleep` child is reaped exactly below (kill + wait on
        // the held handle only).
        let mut child = spawn_sleep();
        let live = child.id();
        assert!(live > 1);
        let err = prove_held_child(
            &mut child,
            live.wrapping_add(1),
            "0123456789abcdef0123456789abcdef",
            "/nonexistent/tiler-macos-probe-target",
            7,
        )
        .unwrap_err();
        let _ = child.kill();
        let _ = child.wait();
        assert!(err.contains("differs"), "unexpected: {err}");
    }

    #[test]
    fn held_proof_validates_nonce_before_live_touch() {
        // Nonce format fails closed before any handle or OS state is read.
        let mut child = spawn_sleep();
        let pid = child.id();
        let err = prove_held_child(&mut child, pid, "short", "/nonexistent", 7).unwrap_err();
        let _ = child.kill();
        let _ = child.wait();
        assert!(err.contains("nonce"), "unexpected: {err}");
    }

    #[test]
    fn result_auth_binds_nonce_and_request_id() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let req = "ffffffffffffffffffffffffffffffff";
        let body = serde_json::to_string(&crate::probe_target::SpaceJoinResult {
            nonce: nonce.to_owned(),
            request_id: req.to_owned(),
            preimage_bits: 4,
            restored_bits: 4,
            restored_verified: true,
        })
        .unwrap();
        let (pre, restored, ok) = parse_join_result(&body, nonce, req).unwrap();
        assert_eq!((pre, restored, ok), (4, 4, true));
        // Nonce mismatch, request-ID mismatch (stale survivor from an
        // earlier action), corrupt JSON, and missing fields all refuse.
        assert!(parse_join_result(&body, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", req).is_err());
        let stale = parse_join_result(&body, nonce, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        assert!(stale.is_err());
        assert!(stale.unwrap_err().contains("request_id"));
        assert!(parse_join_result("not json", nonce, req).is_err());
        assert!(parse_join_result("{}", nonce, req).is_err());
    }

    #[test]
    fn outcome_poll_returns_first_body_and_times_out_empty() {
        // Immediate hit: no sleep, first body wins.
        let mut calls = 0;
        let mut read = || {
            calls += 1;
            Some("hit".to_owned())
        };
        assert_eq!(
            await_result_text(&mut read, 1_000, 50),
            Some("hit".to_owned())
        );
        assert_eq!(calls, 1);
        // Delayed hit: scripted Nones then a body still resolve.
        let mut n = 0;
        let mut read = || {
            n += 1;
            if n < 3 { None } else { Some("late".to_owned()) }
        };
        assert_eq!(
            await_result_text(&mut read, 5_000, 1),
            Some("late".to_owned())
        );
        // Never: bounded timeout returns None (short deadline keeps the
        // fake fast).
        let mut read = || None;
        assert!(await_result_text(&mut read, 5, 1).is_none());
    }

    #[test]
    fn behavior_restoration_is_exact_not_zero_reset() {
        // Exact preimage equality is the only pass; a zero-reset on a
        // nonzero preimage (the compat-style mistake) fails.
        assert!(crate::probe_target::space_join_restored_ok(4, 4));
        assert!(!crate::probe_target::space_join_restored_ok(4, 0));
        assert!(!crate::probe_target::space_join_restored_ok(4, 6));
        // Join math preserves surrounding bits while setting 1<<1.
        assert_eq!(crate::probe_target::space_join_behavior_bits(4), 6);
    }
}
