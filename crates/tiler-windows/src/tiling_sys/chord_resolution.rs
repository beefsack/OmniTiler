#![forbid(unsafe_code)]
use super::*;

pub(super) struct ChordObserved {
    pub(super) observed: Vec<ObservedWindow>,
    pub(super) retained: Vec<RetainedRow>,
    pub(super) skipped: Vec<(String, String)>,
}

pub(super) fn observe_chord_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) -> Option<ChordObserved> {
    let mut skipped = Vec::new();
    let mut retained = Vec::new();
    let mut observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    Some(ChordObserved {
        observed,
        retained,
        skipped,
    })
}

/// Resolved chord target: the Engine token, member key, and world-backed home.
pub(super) struct ChordTarget {
    pub(super) from: String,
    pub(super) member_key: crate::native_guards::WindowKey,
    pub(super) loc: super::world_reads::Home,
}

/// Chord rejection: the outcome to log and the window token when the log
/// line carries one.
pub(super) struct ChordReject {
    pub(super) outcome: &'static str,
    pub(super) window: Option<String>,
}

/// Origin-to-member resolution shared by the float and sticky chords: origin
/// re-resolution, member binding, domain, lifetime drop, proof, and scope
/// fences. Drops stale membership as a side effect; the caller logs.
pub(super) fn resolve_chord_target(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    areas: &[MonitorArea],
    origin: &SnapOrigin,
) -> std::result::Result<ChordTarget, ChordReject> {
    let reject = |outcome: &'static str| ChordReject {
        outcome,
        window: None,
    };
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(crate::ffi::foreground_window());
    let from = match resolve_origin(origin, foreground_hwnd, &fresh, state.snap_advance.as_ref()) {
        OriginVerdict::Dispatch { token, .. } => token,
        OriginVerdict::Reject(reason) => {
            state.snap_advance = None;
            return Err(reject(reason));
        }
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if !crate::native_guards::member_matches(&member_key, origin.hwnd, origin.pid, &origin.creation)
    {
        state.snap_advance = None;
        return Err(reject("foreground-changed"));
    }
    let Some(loc) = state.member_home(&member_key) else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if !areas.iter().any(|a| a.device == loc.output) {
        return Err(reject("unknown-output"));
    }
    let live_tag = crate::product_hide::sys::read_member_tag(member_key.hwnd);
    let lifetime_ok = state.member_tags.get(&member_key).is_some_and(|stored| {
        crate::native_guards::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &member_key);
        return Err(reject("identity-changed"));
    }
    let Some(stored) = state.member_identity.get(&member_key).cloned() else {
        state.snap_advance = None;
        return Err(reject("unmanaged"));
    };
    if let Some(entries) = state.allowlist.as_ref() {
        let owned = entries
            .iter()
            .find(|e| e.hwnd == member_key.hwnd)
            .is_some_and(|entry| verify_proof_owned(member_key.hwnd, entry, me).is_ok());
        if !owned {
            return Err(reject("identity-changed"));
        }
    }
    if !scope_allows(&state.scope, &stored.exe_path)
        || !hosted_gate_allows(
            &stored.exe_path,
            member_key.hwnd,
            stored.pid,
            &state.scope_hosts,
        )
    {
        return Err(ChordReject {
            outcome: "scope-excluded",
            window: Some(from.clone()),
        });
    }
    Ok(ChordTarget {
        from,
        member_key,
        loc,
    })
}

/// Settle one rejected chord with its origin (and window when carried).
pub(super) fn settle_chord_reject(
    log_path: &Path,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    origin: &SnapOrigin,
    reject: &ChordReject,
) {
    let mut line = settle(reject.outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    if let Some(window) = &reject.window {
        line["window"] = serde_json::Value::from(window.clone());
    }
    log_json_at(log_path, line);
}

/// Overlay refusal for one chord target, float or sticky vocabulary.
pub(super) fn chord_overlay_refusal(
    sticky_vocab: bool,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::native_guards::WindowKey,
) -> Option<&'static str> {
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let zoomed = is_zoomed_now(member_key.hwnd);
    if let Some(window) = by_hwnd.get(&member_key.hwnd) {
        if sticky_vocab {
            tiler_core::window_policy::sticky_toggle_refusal(
                window.facts.captionless_fullscreen,
                zoomed,
            )
        } else {
            tiler_core::window_policy::float_toggle_refusal(
                window.facts.captionless_fullscreen,
                zoomed,
            )
        }
    } else if let Some(row) = retained.iter().find(|r| r.key == *member_key) {
        if sticky_vocab {
            tiler_core::window_policy::sticky_toggle_refusal(row.fullscreen, zoomed)
        } else {
            tiler_core::window_policy::float_toggle_refusal(row.fullscreen, zoomed)
        }
    } else {
        None
    }
}

/// Overlay refusal for one orientation target, orientation vocabulary.
/// Same live-state sourcing as the float/sticky overlay fence: the live
/// observed fullscreen flag plus a fresh zoom read when visible, else the
/// retained overlay flags plus a fresh zoom read.
pub(super) fn orientation_overlay_refusal(
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::native_guards::WindowKey,
) -> Option<&'static str> {
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let zoomed = is_zoomed_now(member_key.hwnd);
    if let Some(window) = by_hwnd.get(&member_key.hwnd) {
        tiler_core::window_policy::orientation_toggle_refusal(
            window.facts.captionless_fullscreen,
            zoomed,
        )
    } else if let Some(row) = retained.iter().find(|r| r.key == *member_key) {
        tiler_core::window_policy::orientation_toggle_refusal(row.fullscreen, zoomed)
    } else {
        None
    }
}
