#![forbid(unsafe_code)]
use super::*;

/// Honest foreground-elevation probe: the suppression bool plus the separate
/// sub-outcomes the bool conflates. The bool preserves the existing
/// fail-closed gate exactly (process-open failure, integrity-read failure,
/// and non-medium all suppress); the detail names which leg fired so logs
/// never imply more certainty than is known. `captured` is false only when a
/// test seam short-circuits the native reads (honest `skipped-injected`).
pub(super) struct ElevatedFacts {
    pub(super) elevated: bool,
    pub(super) detail: &'static str,
    pub(super) open: &'static str,
    pub(super) integrity_read: &'static str,
    pub(super) integrity: &'static str,
    pub(super) foreground: u64,
    pub(super) captured: bool,
}

/// Bounded OS integrity level for a successfully read RID, independent of the
/// suppression verdict: exact SDK anchor values name their level
/// (`untrusted`, `low`, `medium`, `medium-plus`, `high`, `system`,
/// `protected`); gaps and out-of-band values report `unknown` rather than
/// asserting a level, and no numeric RID ever leaves this function. The
/// legacy gate still uses exactly [`is_medium_rid`]: in particular a
/// `medium-plus` read stays non-elevated while a `high` read suppresses.
fn integrity_category(rid: u32) -> &'static str {
    const UNTRUSTED: u32 = 0x0000;
    const LOW: u32 = 0x1000;
    const MEDIUM: u32 = 0x2000;
    const MEDIUM_PLUS: u32 = 0x2100;
    const HIGH: u32 = 0x3000;
    const SYSTEM: u32 = 0x4000;
    const PROTECTED: u32 = 0x5000;
    if rid == UNTRUSTED {
        "untrusted"
    } else if (LOW..MEDIUM).contains(&rid) {
        "low"
    } else if (MEDIUM..MEDIUM_PLUS).contains(&rid) {
        "medium"
    } else if rid == MEDIUM_PLUS {
        "medium-plus"
    } else if (HIGH..SYSTEM).contains(&rid) {
        "high"
    } else if (SYSTEM..PROTECTED).contains(&rid) {
        "system"
    } else if rid == PROTECTED {
        "protected"
    } else {
        "unknown"
    }
}

/// Pager suppression threshold: only at-or-above HIGH suppresses. Medium,
/// medium-plus, low (including AppContainer low), untrusted, and below-high
/// gaps stay non-elevated for the pager; open/read failures stay fail-closed
/// elevated. Tiling admission and dispatch keep the medium-only
/// [`is_medium_rid`] gate deliberately (conservative security); only the
/// pager relaxes to this threshold.
pub(super) const PAGER_ELEVATED_RID_MIN: u32 = 0x3000;

/// True when one successfully read RID suppresses the pager: at-or-above
/// HIGH. Never touches native state, so unit tests prove the table offline.
#[must_use]
pub(super) const fn is_pager_elevated_rid(rid: u32) -> bool {
    rid >= PAGER_ELEVATED_RID_MIN
}

/// Pure classifier over already-captured native inputs: the single place the
/// elevation bool and its honest detail are decided together. `open_ok` is
/// the process-open verdict; `integrity` is read only when the open
/// succeeded (`None` reads as a read failure). The bool follows exactly
/// [`is_medium_rid`]; the category names the OS level independently, so a
/// `medium-plus` read stays non-elevated. Never touches native state, so unit
/// tests prove the bool table offline.
fn classify_elevated(
    own_pid: u32,
    foreground: u64,
    pid: u32,
    open_ok: bool,
    integrity: Option<std::result::Result<u32, ()>>,
) -> ElevatedFacts {
    let facts = |elevated: bool,
                 detail: &'static str,
                 open: &'static str,
                 integrity_read: &'static str,
                 integrity: &'static str| ElevatedFacts {
        elevated,
        detail,
        open,
        integrity_read,
        integrity,
        foreground,
        captured: true,
    };
    if foreground == 0 {
        return facts(false, "no-foreground", "skipped", "skipped", "unknown");
    }
    if pid == 0 {
        return facts(false, "no-pid", "skipped", "skipped", "unknown");
    }
    if pid == own_pid {
        return facts(false, "own-process", "skipped", "skipped", "unknown");
    }
    if !open_ok {
        return facts(
            true,
            "process-open-failed",
            "open-failed",
            "skipped",
            "unknown",
        );
    }
    match integrity {
        None => facts(
            true,
            "integrity-read-failed",
            "opened",
            "read-failed",
            "unknown",
        ),
        Some(Ok(rid)) if is_medium_rid(rid) => {
            facts(false, "medium", "opened", "ok", integrity_category(rid))
        }
        Some(Ok(rid)) => facts(true, "non-medium", "opened", "ok", integrity_category(rid)),
        Some(Err(())) => facts(
            true,
            "integrity-read-failed",
            "opened",
            "read-failed",
            "unknown",
        ),
    }
}

/// Pure pager classifier over the same captured inputs as
/// [`classify_elevated`]: the single place the pager suppression bool and
/// its honest detail are decided together. Skip legs (`no-foreground`,
/// `no-pid`, `own-process`) and fail-closed legs (`process-open-failed`,
/// `integrity-read-failed`) match the shared table exactly; only the
/// successfully read RID leg differs: `true` only at-or-above HIGH
/// (`>= 0x3000`, so high/system/protected suppress), `false` for
/// medium-or-below (medium, medium-plus, low/AppContainer, untrusted, and
/// below-high gaps). The Explorer/shell surface is not an exemption here:
/// the class/surface labels stay diagnostic-only. Never touches native
/// state, so unit tests prove the bool table offline.
fn classify_pager_elevated(
    own_pid: u32,
    foreground: u64,
    pid: u32,
    open_ok: bool,
    integrity: Option<std::result::Result<u32, ()>>,
) -> ElevatedFacts {
    let facts = |elevated: bool,
                 detail: &'static str,
                 open: &'static str,
                 integrity_read: &'static str,
                 integrity: &'static str| ElevatedFacts {
        elevated,
        detail,
        open,
        integrity_read,
        integrity,
        foreground,
        captured: true,
    };
    if foreground == 0 {
        return facts(false, "no-foreground", "skipped", "skipped", "unknown");
    }
    if pid == 0 {
        return facts(false, "no-pid", "skipped", "skipped", "unknown");
    }
    if pid == own_pid {
        return facts(false, "own-process", "skipped", "skipped", "unknown");
    }
    if !open_ok {
        return facts(
            true,
            "process-open-failed",
            "open-failed",
            "skipped",
            "unknown",
        );
    }
    match integrity {
        None => facts(
            true,
            "integrity-read-failed",
            "opened",
            "read-failed",
            "unknown",
        ),
        Some(Ok(rid)) if is_pager_elevated_rid(rid) => facts(
            true,
            "above-medium",
            "opened",
            "ok",
            integrity_category(rid),
        ),
        Some(Ok(rid)) => facts(
            false,
            "medium-or-below",
            "opened",
            "ok",
            integrity_category(rid),
        ),
        Some(Err(())) => facts(
            true,
            "integrity-read-failed",
            "opened",
            "read-failed",
            "unknown",
        ),
    }
}

/// True when the foreground window's process is not medium integrity:
/// elevated foreground gates workspace dispatch. Bool behavior is exactly
/// the legacy table (fail closed on unreadable facts); pair with
/// [`foreground_elevated_detailed`] for the honest sub-outcomes.
pub(super) fn foreground_elevated(me: &ProcessIdentity) -> bool {
    foreground_elevated_detailed(me).elevated
}

/// Elevation bool plus honest sub-outcomes over one native capture. The
/// returned bool is exactly [`foreground_elevated`]; the detail splits the
/// conflated legs (process-open failure vs integrity-read failure vs
/// non-medium) without changing any skip case or product policy.
pub(super) fn foreground_elevated_detailed(me: &ProcessIdentity) -> ElevatedFacts {
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED_SEQ.with(|slot| slot.borrow_mut().pop_front()) {
        let _ = me;
        return ElevatedFacts {
            elevated,
            detail: "skipped-injected",
            open: "skipped",
            integrity_read: "skipped",
            integrity: "unknown",
            foreground: 0,
            captured: false,
        };
    }
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED.with(|slot| slot.get()) {
        let _ = me;
        return ElevatedFacts {
            elevated,
            detail: "skipped-injected",
            open: "skipped",
            integrity_read: "skipped",
            integrity: "unknown",
            foreground: 0,
            captured: false,
        };
    }
    #[cfg(test)]
    if let Some(script) = INJECTED_ELEV_PROBE.with(|slot| slot.borrow().clone()) {
        return classify_elevated(
            me.pid,
            script.foreground,
            script.pid,
            script.open_ok,
            script.integrity,
        );
    }
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return classify_elevated(me.pid, foreground, 0, false, None);
    }
    let pid = crate::ffi::window_pid(foreground);
    if pid == 0 || pid == me.pid {
        return classify_elevated(me.pid, foreground, pid, false, None);
    }
    let Ok(held) = HeldProcess::open(pid) else {
        return classify_elevated(me.pid, foreground, pid, false, None);
    };
    classify_elevated(
        me.pid,
        foreground,
        pid,
        true,
        Some(held.integrity().map_err(|_| ())),
    )
}

/// Pager-only elevation bool: true only for an above-medium (at-or-above
/// HIGH) foreground plus the unchanged fail-closed open/read failures.
/// Low/AppContainer never suppress the pager. Pair with
/// [`foreground_pager_elevated_detailed`] for the honest sub-outcomes.
/// Tiling dispatch keeps [`foreground_elevated`] (medium-only) deliberately.
#[cfg(test)]
pub(super) fn foreground_pager_elevated(me: &ProcessIdentity) -> bool {
    foreground_pager_elevated_detailed(me).elevated
}

/// Pager elevation bool plus honest sub-outcomes over one native capture.
/// Same skip/fail-closed legs as [`foreground_elevated_detailed`]; only the
/// read-RID leg uses [`is_pager_elevated_rid`].
pub(super) fn foreground_pager_elevated_detailed(me: &ProcessIdentity) -> ElevatedFacts {
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED_SEQ.with(|slot| slot.borrow_mut().pop_front()) {
        let _ = me;
        return ElevatedFacts {
            elevated,
            detail: "skipped-injected",
            open: "skipped",
            integrity_read: "skipped",
            integrity: "unknown",
            foreground: 0,
            captured: false,
        };
    }
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED.with(|slot| slot.get()) {
        let _ = me;
        return ElevatedFacts {
            elevated,
            detail: "skipped-injected",
            open: "skipped",
            integrity_read: "skipped",
            integrity: "unknown",
            foreground: 0,
            captured: false,
        };
    }
    #[cfg(test)]
    if let Some(script) = INJECTED_ELEV_PROBE.with(|slot| slot.borrow().clone()) {
        return classify_pager_elevated(
            me.pid,
            script.foreground,
            script.pid,
            script.open_ok,
            script.integrity,
        );
    }
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return classify_pager_elevated(me.pid, foreground, 0, false, None);
    }
    let pid = crate::ffi::window_pid(foreground);
    if pid == 0 || pid == me.pid {
        return classify_pager_elevated(me.pid, foreground, pid, false, None);
    }
    let Ok(held) = HeldProcess::open(pid) else {
        return classify_pager_elevated(me.pid, foreground, pid, false, None);
    };
    classify_pager_elevated(
        me.pid,
        foreground,
        pid,
        true,
        Some(held.integrity().map_err(|_| ())),
    )
}

/// Actual-foreground hidden member selects its workspace. Only a real
/// foreground change to an exactly verified hidden member switches; the
/// `Wake` event alone never does. Returns the settled select effect or `None`.
pub(super) fn poll_foreground_workspace(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
) -> Option<SelectEffect> {
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 || foreground == state.last_foreground {
        return None;
    }
    state.last_foreground = foreground;
    // Keyed by full member identity: find the HWND's key, then guard full
    // equality before any lookup so a recycled HWND never selects.
    let key = state
        .hidden_claims
        .keys()
        .find(|k| k.hwnd == foreground)
        .cloned()?;
    let record = state.hidden_claims.get(&key).cloned()?;
    // Exact verification: live PID, full process identity, exact nonce.
    let snap = crate::product_hide::sys::query_recovery(foreground, me).ok()?;
    if !crate::native_guards::member_matches(
        &key,
        foreground,
        snap.pid,
        &snap.process.process_creation,
    ) || snap.process != record.claim.process
        || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
    {
        return None;
    }
    // Externally foregrounded: route the member's applied homing (wire
    // output plus logical workspace receipts, never a second policy)
    // through the world owner. No queued chord here: the external
    // foreground is the trigger, so the queue wait is zero by construction.
    let token = state.member_tokens.get(&key).cloned()?;
    let (wire, workspace) = state.applied_homing.get(&token).cloned()?;
    let output = state
        .output_tokens
        .iter()
        .find(|(_, id)| *id == &wire)
        .map(|(device, _)| device.clone())?;
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    let effect = workspace_do_select(
        state, me, store, dir, fulls, areas, observed, &output, &workspace, &ctx, None, false,
        false,
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: "select",
            index: 0,
            edge: "foreground",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
    Some(effect)
}

/// Externally revealed hidden claims without foreground return to hidden
/// under the same claim and identity: never re-admitted, never moved to the
/// wrong domain. Destroyed/recycled claims retire their ledger notes.
///
/// Diagnostics: every actual actuation attempt accumulates into the
/// per-second `rehide-sweep` buckets (one summary line per second at most),
/// so a corrected transient is never silently lost even when no reconcile
/// samples it, and persistent failures cannot flood. `corrected` means the
/// drift was re-hidden; `retired` means the claim died and the retirement
/// reveal outcome rides the detail; anything else names the failing leg.
fn rehide_pass(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let foreground = crate::ffi::foreground_window();
    let keys: Vec<crate::native_guards::WindowKey> = state.hidden_claims.keys().cloned().collect();
    for key in keys {
        if key.hwnd == foreground {
            continue;
        }
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let visible = crate::ffi::is_visible(key.hwnd);
        if !visible {
            continue;
        }
        // Same-claim guard: only the exact stored identity returns to hidden.
        let snap = match crate::product_hide::sys::query_recovery(key.hwnd, me) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !crate::native_guards::member_matches(
            &key,
            key.hwnd,
            snap.pid,
            &snap.process.process_creation,
        ) || snap.process != record.claim.process
            || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
        {
            continue;
        }
        // Opaque sample window for the sweep below (null when unknown).
        let token = state.member_tokens.get(&key).cloned().unwrap_or_default();
        let sample = if token.is_empty() {
            None
        } else {
            Some(token.as_str())
        };
        match crate::product_hide::sys::rehide_known_claim(me, &record.claim) {
            Ok(true) => {
                note_rehide_diag(state, sample, "corrected", "rehidden");
            }
            Ok(false) => {
                // Retired: drop the ledger note through the identity-safe
                // reveal path and forget session membership safely. The
                // retirement reveal outcome logs before the state removal.
                match crate::product_hide::sys::reveal_product_claim_to(
                    store,
                    me,
                    &record.claim,
                    record.iconic,
                ) {
                    Ok(teardown) => {
                        note_rehide_diag(state, sample, "retired", reveal_outcome(&teardown));
                    }
                    Err(e) => {
                        let (outcome, _) = reveal_err_outcome(&e.to_string());
                        note_rehide_diag(state, sample, outcome, rehide_retired_detail(outcome));
                    }
                }
                state.hidden_claims.remove(&key);
            }
            Err(e) => {
                let (outcome, detail) = rehide_err_outcome(&e.to_string());
                note_rehide_diag(state, sample, outcome, detail);
            }
        }
    }
    if let Some(line) = flush_rehide_diag(state) {
        log_json_at(&state.log_path.clone(), line);
    }
}
/// Mechanical monitor topology sync: stable device-to-wire output ids plus
/// known-geometry/active-output bookkeeping. Displacement, return mapping,
/// and lifecycle are core policy from live screens now.
pub(super) fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    scope_hotplug::sync_monitor_outputs(state, areas);
}

/// Per-tick workspace maintenance on quiet ticks: mechanical output sync,
/// one world observation tick through the single owner, a foreground-driven
/// select on an actual foreground change to a verified hidden member, and
/// the same-claim rehide sweep. Admission, assignment, and close cleanup
/// ride the world observation path now.
pub(super) fn workspace_maintenance(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    sync_monitor_outputs(state, areas);
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        return;
    };
    let foreground = crate::ffi::foreground_window();
    let _ = super::world_tick::run_observation_tick(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        super::observation::sample_cursor(),
        false,
    );
    // Foreground-driven select only on an actual foreground change to an
    // exactly verified hidden member; event-only wakes never switch.
    if poll_foreground_workspace(state, me, store, dir, fulls, areas, &mut observed).is_some() {
        return;
    }
    rehide_pass(state, me, store);
}
