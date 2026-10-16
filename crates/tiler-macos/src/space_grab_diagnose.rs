#![forbid(unsafe_code)]

//! Bounded owned-helper discriminator: unfiltered, title-free CG snapshots
//! beside AX and WindowServer input identity, before/after Mission Control.
//! No synthetic mouse/key posts, settings writes, or Space creation.

#[cfg(target_os = "macos")]
pub fn run() -> Result<String, (i32, String)> {
    use crate::{ffi, mc_lifecycle::live};
    live::action_gates().map_err(|e| (1, e.to_string()))?;
    if live::mc_group_present().map_err(|e| (1, e.to_string()))? {
        return Err((
            1,
            "Mission Control already present; baseline refused".to_owned(),
        ));
    }
    let topology = live::fresh_snapshot().map_err(|e| (1, e.to_string()))?;
    let protected =
        crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| (1, e.to_string()))?;
    let mut awake = live::CaffeinateGuard::spawn(30).map_err(|e| (1, e.to_string()))?;
    let nonce = crate::probe_target::mint_nonce().map_err(|e| (1, e))?;
    let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, 30)
        .map_err(|e| (2, e))?
        .with_regular_policy(true);
    let mut raw = crate::probe_target::start_helper_process(&spec).map_err(|e| (1, e))?;
    let pid = raw.pid;
    // Every exit after spawn passes through exact held-Child teardown.
    let result = (|| -> Result<serde_json::Value, String> {
        crate::probe_target::await_handshake(&mut raw, &spec)?;
        crate::probe_target::record_spawn(pid, &spec, &raw.helper_path)?;
        let wid = crate::focus::join_helper_wid(pid).map_err(|e| e.to_string())?;
        crate::focus::dispatch_focus(pid, wid).map_err(|e| e.to_string())?;
        crate::focus::await_agreement(pid, wid, std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let inspect = || -> Result<serde_json::Value, String> {
            live::action_gates().map_err(|e| e.to_string())?;
            let owned =
                crate::ax::authorize_owned_action(pid, &nonce).map_err(|e| e.to_string())?;
            let frame = crate::ax::read_window_owned(&owned, pid, wid)
                .map_err(|e| e.to_string())?
                .ax_frame
                .ok_or("AX frame missing")?;
            let all = ffi::cg_window_list_basic()?;
            let points = crate::space_drag_transfer::candidate_grab_points(frame)?
                .into_iter()
                .map(|point| {
                    let element = ffi::ax_element_at_position(point.0, point.1);
                    let role = element
                        .as_ref()
                        .ok()
                        .map(|e| ffi::ax_string_attribute(e, "AXRole"));
                    let subrole = element
                        .as_ref()
                        .ok()
                        .map(|e| ffi::ax_string_attribute(e, "AXSubrole"));
                    let ax = ffi::ax_hit_test_owner(point.0, point.1);
                    let cg = ffi::sls_window_at_point(point.0, point.1);
                    let cg_pid = cg
                        .as_ref()
                        .ok()
                        .and_then(|wid| all.iter().find(|w| w.wid == *wid))
                        .map(|w| w.pid);
                    serde_json::json!({"point":point, "role":role, "subrole":subrole,
                        "ax_owner":ax, "cg_hit":cg, "cg_pid":cg_pid})
                })
                .collect::<Vec<_>>();
            let windows = all
                .iter()
                .map(|w| {
                    serde_json::json!({
                        "wid":w.wid,"pid":w.pid,"layer":w.layer,"onscreen":w.onscreen,
                        "bounds":w.bounds,"bundle":ffi::bundle_id_for_pid(w.pid as i32),
                        "membership":ffi::sls_spaces_for_windows(&[w.wid])
                    })
                })
                .collect::<Vec<_>>();
            Ok(
                serde_json::json!({"pid":pid,"wid":wid,"frame":frame,"points":points,
                "windows":windows,"session":ffi::session_state(),"mc_present":live::mc_group_present().map_err(|e|e.to_string())}),
            )
        };
        let before = inspect()?;
        live::mc_open().map_err(|e| e.to_string())?;
        live::mc_close().map_err(|e| e.to_string())?;
        crate::mc_observe::await_ghostty_restored(&protected).map_err(|e| e.to_string())?;
        let after = inspect()?;
        Ok(serde_json::json!({"before":before,"after":after}))
    })();
    let close = live::mc_close().map_err(|e| e.to_string());
    let stop = crate::session::stop_exact_child(&mut raw.child, pid);
    let absent = !ffi::process_exists(pid as i32)
        && ffi::cg_window_list_basic().is_ok_and(|all| all.iter().all(|w| w.pid != pid));
    let topology_after = live::fresh_snapshot().map_err(|e| e.to_string());
    let topology_equal = topology_after.as_ref().is_ok_and(|after| {
        topology.displays.len() == after.displays.len()
            && topology
                .displays
                .iter()
                .zip(&after.displays)
                .all(|(a, b)| a.spaces == b.spaces && a.current == b.current)
    });
    let protected_equal = crate::mc_observe::await_ghostty_restored(&protected).is_ok();
    let reaped = awake.stop();
    let complete = result.is_ok()
        && close.is_ok()
        && stop.exited
        && stop.reaped
        && stop.record_removed
        && absent
        && topology_equal
        && protected_equal
        && reaped;
    let output = serde_json::json!({"action":"spaces-grab-diagnose","evidence":result,
        "helper_stop":{"exited":stop.exited,"reaped":stop.reaped,"record_removed":stop.record_removed},
        "helpers_absent":absent,"topology_restored":topology_equal,"protected_stable":protected_equal,
        "mc_close":close,"caffeinate_reaped":reaped,"complete":complete,"synthetic_posts":0}).to_string();
    if complete {
        Ok(output)
    } else {
        Err((1, output))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn run() -> Result<String, (i32, String)> {
    Err((1, "spaces-grab-diagnose is macOS-only".to_owned()))
}
