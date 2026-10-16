use super::rows::desired_entries;
use super::*;

/// Bundled inputs for one geometry application pass.
pub(super) struct ApplyInput<'a> {
    pub(super) me: &'a ProcessIdentity,
    pub(super) fulls: &'a [Rect],
    pub(super) reply: &'a CoreReply,
    pub(super) observed: &'a [ObservedWindow],
    pub(super) op: &'a str,
    pub(super) tick: u64,
    pub(super) correlation: &'a str,
    pub(super) skipped: Vec<(String, String)>,
    /// Eligible observed tokens in this domain. Retained and hidden rows
    /// converge Engine membership but never take geometry writes.
    pub(super) writable: &'a HashSet<String>,
    /// Opaque domain tokens for the per-domain tick log (no native ids).
    pub(super) output_token: String,
    pub(super) workspace_token: String,
    /// Engine revision of this domain for the tick summary signature.
    pub(super) revision: u64,
}

/// Settled result of one geometry application pass: verified writes,
/// readback mismatches, and whether the independent readback observation ran
/// at all, plus the first privacy-safe veto diagnostic when a write vetoed.
/// Callers map this to their action outcome vocabulary instead of assuming
/// success. `veto` is `None` when no write vetoed: the action line serializes
/// null with reason, never zeros implying applied.
#[derive(Debug, Clone, Copy)]
pub(super) struct ApplySummary {
    pub(super) applied: usize,
    pub(super) mismatched: usize,
    pub(super) readback_ok: bool,
    pub(super) veto: Option<VetoDiag>,
}

/// Bounded privacy-safe veto diagnostic: reason token plus the native
/// booleans behind it. No handles, paths, PIDs, titles, or content; opaque
/// managed tokens only travel in the existing skip list.
#[derive(Debug, Clone, Copy)]
pub(super) struct VetoDiag {
    pub(super) reason: &'static str,
    pub(super) desktop: bool,
    pub(super) visible: bool,
    pub(super) captioned: bool,
    pub(super) dwm_readable: bool,
    pub(super) cloaked: bool,
}

pub(super) fn audit_json(state: &TileLoop, value: serde_json::Value) {
    if let Some(path) = state.audit_path.as_ref() {
        log_json_at(path, value);
    }
}

/// Effective requested visible rectangle for one planned write: the
/// overconstrained min-clamped tile origin for admitted overconstrained
/// writables, the planned rectangle otherwise. Single definition shared by
/// the write pass, the readback pass, and diagnostics so clamping converges
/// instead of retrying.
pub(super) fn effective_rect(
    state: &TileLoop,
    entry: &crate::workspace_owner::PlannedWrite,
) -> Rect {
    if entry.overconstrained {
        let hints = state
            .hint_logged
            .get(entry.window.0.as_str())
            .copied()
            .unwrap_or(WindowSizeHints::none());
        crate::tiling::overconstrained_effective(entry.rect, hints)
    } else {
        entry.rect
    }
}

pub(super) fn apply_geometry(state: &mut TileLoop, input: ApplyInput<'_>) -> Option<ApplySummary> {
    #[cfg(test)]
    if INJECTED_OBSERVE.with(|slot| slot.borrow().is_some()) {
        PENDING_COUNTERS.with(|slot| {
            slot.borrow_mut().apply += 1;
        });
        APPLY_WRITABLES.with(|slot| {
            slot.borrow_mut()
                .push(input.writable.iter().cloned().collect());
        });
        let _ = input;
        if let Some((readback_ok, mismatched)) =
            INJECTED_APPLY.with(|slot| slot.borrow_mut().pop_front())
        {
            return Some(ApplySummary {
                applied: 0,
                mismatched,
                readback_ok,
                veto: None,
            });
        }
        return Some(ApplySummary {
            applied: 0,
            mismatched: 0,
            readback_ok: true,
            veto: None,
        });
    }
    let ApplyInput {
        me,
        fulls,
        reply,
        observed,
        op,
        tick,
        correlation,
        mut skipped,
        writable,
        output_token,
        workspace_token,
        revision,
    } = input;
    let log_path = state.log_path.clone();
    let Some(desired) = desired_entries(reply) else {
        let outcome = match reply {
            CoreReply::Rejected { kind, .. } => ("rejected", *kind),
            CoreReply::Diverged(reason) => ("diverged", reason.as_str()),
            CoreReply::SnapshotInvalid { detail, .. } => ("snapshot-invalid", *detail),
            _ => ("unexpected-reply", "unhandled-variant"),
        };
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "reply",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "output": output_token,
                "workspace": workspace_token,
                "outcome": outcome.0,
                "kind": outcome.1,
            }),
        );
        // No plan: complete hint-staged samples honestly and flush so a
        // problem row still reports instead of lingering half-staged.
        stage_no_plan(state);
        if let Some(line) = flush_window_diag(state) {
            log_json_at(&log_path, line);
        }
        return None;
    };
    let by_token: HashMap<&str, &ObservedWindow> =
        observed.iter().map(|w| (w.token.as_str(), w)).collect();
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let proof_mode = state.allowlist.is_some();
    let now = Instant::now();
    // Pre-write suppression snapshot for per-window diagnostics: the
    // in-loop skip decisions recompute faithfully against this, and lane
    // transitions read as re-arm reasons after the readback below.
    let refused_before = state.refused.clone();
    let mut applied = 0usize;
    // First veto diagnostic in this pass for the bounded action summary; the
    // per-window skip list keeps the existing `fullscreen-foreground` token.
    let mut veto_diag: Option<VetoDiag> = None;
    // Tokens whose setter succeeded this tick. Only these may enter the
    // app-clamp lane on readback mismatch; failed or skipped setters keep
    // the transient/backoff lanes intact.
    let mut written: HashSet<String> = HashSet::new();
    // Native outer targets for attempted setters, for per-window
    // diagnostics (inserted only where the setter actually runs).
    let mut outer_targets: HashMap<String, Rect> = HashMap::new();
    for entry in &desired {
        if entry.client_clamped {
            skipped.push((entry.window.0.clone(), "client-clamped".to_owned()));
            continue;
        }
        if !writable.contains(entry.window.0.as_str()) {
            // Retained or hidden rows converge Engine membership but never
            // take geometry writes: hidden workspace geometry waits for
            // reveal and minimized/maximized/fullscreen frames stay native.
            // Overconstrained non-writable rows stay here too; only admitted
            // writable overconstrained windows take the min-clamped target
            // below.
            skipped.push((entry.window.0.clone(), "retained".to_owned()));
            continue;
        }
        // Overconstrained writables take the min-clamped tile origin instead
        // of skipping while reserving the tile. All checks below use the
        // effective target so clamping converges instead of retrying.
        let effective = effective_rect(state, entry);
        let Some(expected) = by_token.get(entry.window.0.as_str()) else {
            skipped.push((entry.window.0.clone(), "vanished".to_owned()));
            continue;
        };
        if expected.visible == effective {
            state.refused.note_match(&entry.window.0);
            continue;
        }
        if state
            .refused
            .should_skip(&entry.window.0, &effective, &expected.visible, now)
        {
            skipped.push((entry.window.0.clone(), "refused-intent".to_owned()));
            continue;
        }
        // Snapshot the pre-write identity for the proof audit before the
        // revalidation borrows `state.tokens` mutably.
        let audit_pre = if proof_mode {
            by_hwnd.get(&expected.hwnd).map(|w| {
                (
                    w.identity.pid,
                    w.identity.process_creation.clone(),
                    w.identity.exe_path.clone(),
                    w.identity.user_sid.clone(),
                    w.identity.session_id,
                    w.identity.tag.clone(),
                )
            })
        } else {
            None
        };
        // Stored lifetime tag for the write gate: disjoint-field borrows of
        // `state` coexist with the `&mut state.tokens` below.
        let member_key = crate::workspace::WindowKey {
            hwnd: expected.hwnd,
            pid: expected.identity.pid,
            creation: expected.identity.process_creation.clone(),
        };
        let stored_tag = state.member_tags.get(&member_key).map(String::as_str);
        let target = match revalidate_target(
            expected,
            me,
            fulls,
            &mut state.tokens,
            proof_mode,
            state.allowlist.as_ref(),
            &state.scope,
            stored_tag,
            &state.scope_hosts,
            false,
        ) {
            Ok(target) => target,
            Err(reason) => {
                skipped.push((entry.window.0.clone(), reason.to_owned()));
                if proof_mode {
                    audit_json(
                        state,
                        serde_json::json!({
                            "event": "proof-skip",
                            "tick": tick,
                            "op": op,
                            "window": entry.window.0,
                            "requested": [effective.x, effective.y, effective.w, effective.h],
                            "reason": reason,
                        }),
                    );
                }
                continue;
            }
        };
        let Some(outer) = target.window.insets.visible_to_outer(effective) else {
            skipped.push((entry.window.0.clone(), "frame-overflow".to_owned()));
            continue;
        };
        // User safety across the tick: a vetoing foreground that arrived
        // after the loop guard must veto this write, not ride along. The
        // first veto pins the bounded diagnostic for the action summary.
        // Verified managed overlays bypass like a managed maximize.
        let read = suspend_read(state, me, fulls);
        if read.veto.block {
            if veto_diag.is_none() {
                veto_diag = Some(VetoDiag {
                    reason: read.veto.reason.as_str(),
                    desktop: read.desktop,
                    visible: read.visible,
                    captioned: read.captioned,
                    dwm_readable: read.dwm_readable,
                    cloaked: read.cloaked,
                });
            }
            skipped.push((entry.window.0.clone(), "fullscreen-foreground".to_owned()));
            continue;
        }
        let hwnd = target.window.hwnd as isize as HWND;
        outer_targets.insert(entry.window.0.clone(), outer);
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        // The held process keeps liveness authority across the write; recheck
        // the HWND pid afterward against recycling.
        let _ = &target.held;
        let pid_ok = pid_current(target.window.hwnd, target.window.identity.pid);
        if proof_mode {
            let (pid, creation, exe, sid, session, tag) = audit_pre.unwrap_or_else(|| {
                (
                    target.window.identity.pid,
                    target.window.identity.process_creation.clone(),
                    target.window.identity.exe_path.clone(),
                    target.window.identity.user_sid.clone(),
                    target.window.identity.session_id,
                    target.window.identity.tag.clone(),
                )
            });
            audit_json(
                state,
                serde_json::json!({
                    "event": "proof-write",
                    "tick": tick,
                    "op": op,
                    "window": entry.window.0,
                    "requested": [effective.x, effective.y, effective.w, effective.h],
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "target": {
                        "hwnd": target.window.hwnd,
                        "pid": pid,
                        "process_creation": creation,
                        "exe_path": exe,
                        "user_sid": sid,
                        "session_id": session,
                        "tag": tag,
                    },
                    "flags": "SWP_NOACTIVATE|SWP_NOZORDER",
                    "outcome": if ok != 0 && pid_ok { "written" } else if ok == 0 { "setter-failed" } else { "pid-changed" },
                }),
            );
        }
        if !pid_ok {
            skipped.push((entry.window.0.clone(), "pid-changed".to_owned()));
            continue;
        }
        if ok == 0 {
            skipped.push((entry.window.0.clone(), "setter-failed".to_owned()));
            state
                .refused
                .note_transient(&entry.window.0, &effective, &target.window.visible, now);
            continue;
        }
        applied += 1;
        written.insert(entry.window.0.clone());
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "write",
                    "tick": tick,
                    "correlation": correlation,
                    "op": op,
                    "window": entry.window.0,
                    "desired": [effective.x, effective.y, effective.w, effective.h],
                }),
            );
        }
    }
    // Complete Engine plan for this tick: every desired entry, not only the
    // changed writes, so proof can compare the full requested set.
    if state.trace {
        let plan: Vec<serde_json::Value> = desired
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "window": entry.window.0,
                    "rect": [entry.rect.x, entry.rect.y, entry.rect.w, entry.rect.h],
                })
            })
            .collect();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "plan",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "entries": plan,
            }),
        );
    }
    // Independent readback: a second full observation pass, never the setter
    // result. Successful writes that still mismatch are app-held sizes (clamp
    // lane); failed setters already took the transient lane above. The
    // readback pass reuses the same full-monitor list so DWM pre/post frame
    // math stays coherent within the tick.
    let reread_fulls = fulls.to_vec();
    let mut reread_skipped = Vec::new();
    let mut reread_retained: Vec<RetainedRow> = Vec::new();
    let reread = state.observe(me, &reread_fulls, &mut reread_skipped, &mut reread_retained);
    let mut mismatched = 0usize;
    let mut readback_ok = false;
    let mut post_rects: HashMap<String, Rect> = HashMap::new();
    if let Some(reread) = reread {
        readback_ok = true;
        // Stable pre-gesture state follows the AFTER-actuation observation,
        // never the pre-apply frame the Engine just consumed.
        publish_managed(state, me, &reread, &reread_retained);
        let reread_by_token: HashMap<&str, Rect> = reread
            .iter()
            .map(|w| (w.token.as_str(), w.visible))
            .collect();
        post_rects = reread_by_token
            .iter()
            .map(|(token, rect)| ((*token).to_owned(), *rect))
            .collect();
        for entry in &desired {
            if !writable.contains(entry.window.0.as_str()) {
                // Retained rows never took writes: no readback verdict.
                continue;
            }
            // Same effective target as the write pass so clamping reads back
            // as a match and refused tracking stays stable.
            let effective = effective_rect(state, entry);
            match reread_by_token.get(entry.window.0.as_str()) {
                Some(visible) => {
                    match readback_outcome(
                        written.contains(entry.window.0.as_str()),
                        &effective,
                        visible,
                    ) {
                        ReadbackOutcome::Match => {
                            state.refused.note_match(&entry.window.0);
                        }
                        ReadbackOutcome::Clamp => {
                            mismatched += 1;
                            state
                                .refused
                                .note_clamp(&entry.window.0, &effective, visible);
                        }
                        ReadbackOutcome::Pending => {
                            // No write behind this mismatch (skipped or
                            // refused intent): count it, but leave the
                            // tracker lanes alone so a later matching
                            // re-read still converges instead of sticking
                            // as a permanent clamp.
                            mismatched += 1;
                        }
                    }
                }
                None => {
                    skipped.push((entry.window.0.clone(), "vanished".to_owned()));
                }
            }
        }
        let live: Vec<&str> = reread_by_token.keys().copied().collect();
        state.refused.retain(&live);
        if state.trace {
            let detail: Vec<serde_json::Value> = desired
                .iter()
                .map(|entry| {
                    let readback = reread_by_token.get(entry.window.0.as_str());
                    let effective = effective_rect(state, entry);
                    serde_json::json!({
                        "window": entry.window.0,
                        "desired": [effective.x, effective.y, effective.w, effective.h],
                        "readback": readback.map(|r| [r.x, r.y, r.w, r.h]),
                        "matched": readback.is_some_and(|r| r == &effective),
                    })
                })
                .collect();
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "readback-detail",
                    "tick": tick,
                    "correlation": correlation,
                    "op": op,
                    "entries": detail,
                }),
            );
        }
    }
    skipped.extend(reread_skipped);
    // Per-window mismatch detail rides the batched `window-diag` event below
    // (which subsumes the old count-only line with strictly more evidence
    // under a stricter cap); the tick summary below keeps the count.
    // Per-window diagnostic lane: stage this pass over the structures above
    // and flush one globally rate-capped batch. Steady ticks emit nothing.
    stage_write_samples(
        state,
        &desired,
        &skipped,
        &written,
        &outer_targets,
        observed,
        &post_rects,
        &refused_before,
        now,
    );
    if let Some(line) = flush_window_diag(state) {
        log_json_at(&log_path, line);
    }
    // Routine zero-change ticks stay quiet; anything else logs one summary.
    // The dedupe signature carries no tick (stable-noop): an identical
    // observation must stay quiet no matter how many ticks pass. Bounded: the
    // sorted skip list is the full signal, no arbitrary caps. The emitted
    // line carries only the deduped skip count, never the repeated per-window
    // list (reread observation re-lists the same identities).
    let mut summary_skipped = skipped.clone();
    summary_skipped.sort();
    summary_skipped.dedup();
    let signature = tick_summary_signature(
        observed.len(),
        applied,
        &summary_skipped,
        mismatched,
        op,
        revision,
    );
    let signature = format!("{output_token}/{workspace_token}|{signature}");
    let changed = state.last_summary.as_ref() != Some(&signature);
    if changed || state.trace {
        state.last_summary = Some(signature);
        let skipped_count = summary_skipped.len();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "tick",
                "tick": tick,
                "op": op,
                "correlation": correlation,
                "output": output_token,
                "workspace": workspace_token,
                "windows": observed.len(),
                "applied": applied,
                "mismatched": mismatched,
                "skipped": skipped_count,
            }),
        );
    }
    Some(ApplySummary {
        applied,
        mismatched,
        readback_ok,
        veto: veto_diag,
    })
}

/// Closed outcome vocabulary for a directional Engine reply that is not a
/// success plan: edge no-ops and refusals actuate nothing.
pub(super) fn reply_outcome(reply: &CoreReply) -> &'static str {
    match reply {
        CoreReply::Rejected { kind, .. } => kind,
        CoreReply::Diverged(reason) => reason.as_str(),
        CoreReply::SnapshotInvalid { detail, .. } => detail,
        _ => "unexpected-reply",
    }
}
