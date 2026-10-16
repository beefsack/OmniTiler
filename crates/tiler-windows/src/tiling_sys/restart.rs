use super::*;

/// Maximum hydration-candidate rows carried in one normal rejection detail
/// record. Restart hydration sets are small (one domain's staged floats
/// plus its tiled rows); the cap keeps normal-log expansion bounded while
/// `total`/`omitted` keep the count exact. Integer geometry plus opaque
/// tokens only: no HWNDs, pids, paths, or titles.
pub(super) const RESTART_HYDRATION_DETAIL_CAP: usize = 8;

/// Bounded candidate facts for a rejected hydration attempt, for the normal
/// log. `total` is the exact row count; only the first
/// [`RESTART_HYDRATION_DETAIL_CAP`] rows are carried and `omitted` counts
/// the rest, so the record never grows with the domain.
pub(super) fn bounded_hydration_candidates(
    rows: &[crate::workspace_owner::OwnerRow],
) -> serde_json::Value {
    let total = rows.len();
    let candidates: Vec<serde_json::Value> = rows
        .iter()
        .take(RESTART_HYDRATION_DETAIL_CAP)
        .map(|row| {
            serde_json::json!({
                "window": row.token,
                "rect": [row.rect.x, row.rect.y, row.rect.w, row.rect.h],
                "floating": row.floating,
                "min": [row.hints.min_w, row.hints.min_h],
                "max": [row.hints.max_w, row.hints.max_h],
            })
        })
        .collect();
    serde_json::json!({
        "total": total,
        "omitted": total.saturating_sub(candidates.len()),
        "candidates": candidates,
    })
}

/// Admit first-seen fullscreen windows as born-held workspace members (KDE
/// initial-fullscreen-hold parity): managed membership with a slotless
/// synthetic floating Engine observation, never a tile slot. First-seen
/// applies regardless of app/project properties after a restart: a
/// restart-owned frame (our restoration metadata still on the window) is a
/// born hold like any first observed fullscreen, and the Win+F11 shortcut
/// still exits through those properties. A later fullscreen on a slotted
/// member or a lifetime-known non-fullscreen window is a managed overlay
/// transition, never born again. Membership carries the lifetime tag,
/// identity, and Engine token (supporting hide/reveal/recovery with no
/// native writes); floating observation is true only for the born hold, and
/// no `member_rects` slot is faked (the row rect / hidden snapshot carries
/// the native frame). Release happens on the first verified non-fullscreen
/// observation (or close), when normal fresh admission takes over; a first
/// exit to maximized converges through the existing admission clear.
pub(super) fn admit_born_fullscreen(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) -> Vec<String> {
    let mut admitted: Vec<String> = Vec::new();
    let log_path = state.log_path.clone();
    for row in retained {
        if !row.fullscreen {
            continue;
        }
        let Some(frame) = row.rect else {
            continue;
        };
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd)
        {
            continue;
        }
        let known_slot = state.member_rects.contains_key(&row.token);
        if !should_hold_born_fullscreen(
            true,
            known_slot,
            state.seen_nonfullscreen.contains(&row.key),
        ) {
            continue;
        }
        if holdable_key(state, me, row.key.hwnd).is_none_or(|key| key != row.key) {
            continue;
        }
        let live = match HeldProcess::open(row.key.pid).and_then(|held| held.identity()) {
            Ok(live) => live,
            Err(_) => continue,
        };
        if live.pid != row.key.pid || live.process_creation != row.key.creation {
            continue;
        }
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
        {
            continue;
        }
        let fresh_tag = match crate::product_hide::sys::install_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        if state
            .workspaces
            .assign(row.key.clone(), &output, &active, false)
        {
            state.member_tokens.insert(row.key.clone(), token.clone());
            state.member_identity.insert(row.key.clone(), live);
            state.member_tags.insert(row.key.clone(), fresh_tag);
            // Hidden-snapshot seed only (never a tile slot): a born hold that
            // is later hidden needs an existing snapshot for row assembly,
            // while visible rows always use the fresh row rect. `known_slot`
            // checks treat born keys as slotless regardless of this seed, and
            // release drops it so a first exit to maximized clears normally.
            state.member_rects.insert(token.clone(), frame);
            if state.born_fullscreen.insert(row.key.clone()) {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "initial-fullscreen-held",
                        "window": token,
                    }),
                );
            }
            admitted.push(output);
        }
    }
    admitted
}

/// Slotless maximized admission (R-MAX-03 / KDE floating parity): first-seen
/// maximized, non-fullscreen retained windows without workspace membership
/// and without a retained tile slot join the active workspace slotless:
/// membership plus stable token, identity, and lifetime tag for hide/reveal
/// and focus, but no `member_rects` tile slot, no Engine exception, and no
/// hold set. A floating workspace therefore preserves the native frame (the
/// mode-gated clear skips) while select-away/back still hides and reveals;
/// the first tiled admission reserves the overlay slot through the R-MAX-03
/// retile latch (both `core.born_maximized` values) and the refetched normal
/// observation tiles into it. Same exact-lifetime, scope, hosted-child, and
/// proof fences as the born-fullscreen admission.
pub(super) fn admit_slotless_maximized(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    hint_cx: &mut HintCx,
) -> Vec<String> {
    let mut admitted: Vec<String> = Vec::new();
    let log_path = state.log_path.clone();
    for row in retained {
        let is_member = state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd);
        if !crate::tiling::should_admit_slotless_maximized(
            row.maximized,
            row.fullscreen,
            is_member,
            state.member_rects.contains_key(&row.token),
        ) {
            continue;
        }
        let Some(frame) = row.rect else {
            continue;
        };
        if holdable_key(state, me, row.key.hwnd).is_none_or(|key| key != row.key) {
            continue;
        }
        let live = match HeldProcess::open(row.key.pid).and_then(|held| held.identity()) {
            Ok(live) => live,
            Err(_) => continue,
        };
        if live.pid != row.key.pid || live.process_creation != row.key.creation {
            continue;
        }
        let stale = crate::workspace_owner::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.workspaces.member_loc(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
        {
            continue;
        }
        let fresh_tag = match crate::product_hide::sys::install_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(fresh_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        if state
            .workspaces
            .assign(row.key.clone(), &output, &active, false)
        {
            state.member_tokens.insert(row.key.clone(), token.clone());
            state.member_identity.insert(row.key.clone(), live);
            state.member_tags.insert(row.key.clone(), fresh_tag);
            // R-SPC-04 D7: a surviving tile-override marker re-adopts the
            // explicit tile win HERE, before any birth probe, automatic
            // origin, clear, slot, or write effect. The helper re-verifies
            // the admission above under the full held fence (live process
            // identity, medium integrity, scope, hosted-child, proof, and
            // the just-installed lifetime tag with freshness rechecks on
            // both sides of the shared-priority marker peeks); only a
            // TileOverride verdict hydrates, with no Engine commit and no
            // writes. Sticky, float, corrupt, and unreadable lanes keep
            // their existing routes. The override rides the reserved-overlay
            // lane under both `core.born_maximized` values: no D4
            // probe/latch, no born-overlay latch, no clear.
            let hydrated_override =
                hydrate_tile_override_marker(state, me, &row.key, &log_path, &token);
            // R-SPC-04 D4: a fixed max-birth floats beneath native maximize
            // (membership-only, no clear, no reserved tile slot), winning
            // over the `core.born_maximized` clear below. The verdict uses
            // fresh tracks under the live predicate with the same
            // failure-as-unknown rules; unknown fails closed to the existing
            // lane. The latch is lifetime-bound and pruned with membership;
            // explicit unfloat clears it and records an override instead.
            // `hint_cx` is the preamble-shared aggregate: many fresh
            // maximized windows share one timer, never one per row.
            // Hydrated overrides never probe or latch (D7 wins).
            let fixed_birth = !hydrated_override
                && fixed_birth_for_slotless(
                    row.key.hwnd,
                    fixed_predicate_for(state.settings_live.as_ref()),
                    hint_cx,
                );
            if fixed_birth {
                state.fixed_auto_seen.insert(row.key.clone());
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "maximize-admitted-slotless",
                        "window": token,
                        "outcome": "fixed-float",
                    }),
                );
            }
            // R-MAX-06 latch, bound here after every ownership fence above
            // (exact-lifetime, scope, hosted-child, proof, fresh tag): only a
            // tiled-first admission under the live overlay value keeps the
            // overlay. Fixed births never latch (D4 wins). A later live flip
            // cannot move this lifetime. Hydrated tile overrides ride their
            // own reserved-overlay lane under both values and never take
            // either latch. R-MAX-03 floating-first rows bind
            // `retile_keep_overlay` instead: their later floating-to-tiled
            // admission keeps the overlay under both `core.born_maximized`
            // values with no one-shot clear.
            if !fixed_birth && !hydrated_override {
                let tiled_first = state.workspaces.is_tiled(&output, &active);
                if crate::tiling::born_keep_latch(
                    tiled_first,
                    born_maximized_keep_overlay_for(state.settings_live.as_ref()),
                ) {
                    state.born_keep_overlay.insert(row.key.clone());
                } else if !tiled_first {
                    state.retile_keep_overlay.insert(row.key.clone());
                }
            }
            // No tile seed here: tiled row assembly reserves the overlay slot,
            // or the one-shot clear permits ordinary observed admission.
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "maximize-admitted-slotless",
                    "window": token,
                }),
            );
            admitted.push(output);
        }
    }
    admitted
}

/// Restart marker adoption: the next owner re-adopts every surviving sticky
/// marker as sticky (keeping its pre-sticky origin), every surviving
/// intentional-float marker as an ordinary float, and every surviving
/// fixed-window tile-override marker as an explicit tile win, each on its
/// current workspace with the live frame preserved, before any tiling. Runs in the
/// shared admission preamble, so every tick and chord adopts before any tile
/// write.
///
/// All admitted valid markers commit in ONE candidate event per domain:
/// sequential per-window commits admit the other marker windows as ordinary
/// tiles first, and flipping the Engine's focused tile to float then rejects
/// with `focus-mismatch` while the window stays tiled. Batching keeps every
/// flip on first contact, where no retained focus binding can reject. The
/// event carries the first tiled row as focus (the fresh-seed anchor rule)
/// so retained-session commits converge focus onto a surviving tiled member.
///
/// Markers restore classification only: admission plus the exact-lifetime
/// fences below must already hold, so a marker alone never grants
/// membership, writes, or recovery authority. Adoption performs no native
/// writes and keeps every durable marker for later restarts; only settled
/// unfloat/sticky-off removes float/sticky markers, and only settled
/// re-float/sticky-off-to-float removes tile-override markers. Missing means
/// empty; a wrong value logs `marker-corrupt` and a failed read logs
/// `marker-unreadable`, both with no intent. Tile-override markers hydrate
/// the runtime override set (D7); unmatched records recompute fixed
/// classification with no hold or retry.
pub(super) fn adopt_restart_markers(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    hint_cx: &mut HintCx,
) {
    /// One staged marker window: `prior` carries the pre-sticky float state
    /// for sticky markers and is `None` for an ordinary float.
    struct Staged {
        key: crate::workspace::WindowKey,
        token: String,
        loc: crate::workspace::MemberLoc,
        prior: Option<bool>,
        frame: Rect,
    }
    let log_path = state.log_path.clone();
    let correlation = state.correlation();
    // Rebuild the observation-scoped write barrier: a rejected staged float
    // from the previous observation holds no authority here until this
    // observation revalidates its marker, lifetime, and pid fences below.
    state.restart_reject_hold.clear();
    // Stage every admitted valid marker before the first domain commit, so
    // no marker window is ever admitted as an ordinary tile first. No Engine
    // commits and no map writes in this pass (the already-floating fast path
    // below excepted).
    let mut staged: Vec<Staged> = Vec::new();
    for window in observed {
        let key = crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        if state.sticky.contains_key(&key) || state.floated.contains(&key) {
            continue;
        }
        if state.hidden_claims.keys().any(|k| k.hwnd == window.hwnd) {
            continue;
        }
        let Some(token) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        let Some(loc) = state.workspaces.member_loc(&key).cloned() else {
            continue;
        };
        let Some(stored) = state.member_identity.get(&key).cloned() else {
            continue;
        };
        if !scope_allows(&state.scope, &stored.exe_path)
            || !hosted_gate_allows(&stored.exe_path, key.hwnd, key.pid, &state.scope_hosts)
        {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let owned = entries
                .iter()
                .find(|e| e.hwnd == key.hwnd)
                .is_some_and(|entry| verify_proof_owned(key.hwnd, entry, me).is_ok());
            if !owned {
                continue;
            }
        }
        // Shared portable classifier (sticky wins over float and the tile
        // override); absent is silent, corrupt/unreadable log no intent.
        let marker = crate::workspace_owner::classify_restart_marker(
            crate::product_hide::sys::peek_float_intent_raw(window.hwnd),
            crate::product_hide::sys::peek_sticky_raw(window.hwnd),
            crate::product_hide::sys::peek_tile_override_raw(window.hwnd),
        );
        // R-SPC-04 D7: a surviving tile-override marker re-adopts the
        // explicit tile win below (same fences, no Engine commit, no
        // writes); missing/degraded/unmatched records fall back to ordinary
        // fixed classification with no hold or retry.
        let is_override = matches!(marker, crate::workspace_owner::RestartMarker::TileOverride);
        let prior = match marker {
            crate::workspace_owner::RestartMarker::Sticky(prior) => Some(prior),
            crate::workspace_owner::RestartMarker::FloatIntent => None,
            crate::workspace_owner::RestartMarker::TileOverride => None,
            crate::workspace_owner::RestartMarker::Absent => continue,
            crate::workspace_owner::RestartMarker::Corrupt => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "sticky-adopt",
                        "window": token,
                        "outcome": "marker-corrupt",
                    }),
                );
                continue;
            }
            crate::workspace_owner::RestartMarker::Unreadable => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "sticky-adopt",
                        "window": token,
                        "outcome": "marker-unreadable",
                    }),
                );
                continue;
            }
        };
        let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
        let lifetime_ok = state.member_tags.get(&key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        // Portable hydration gate: admitted (checked above) plus exact
        // identity plus live lifetime tag. A mismatch fails closed with no
        // intent and no log: the window is simply not ours.
        let eligible = if is_override {
            crate::workspace_owner::restart_tile_override_eligible(true, true, lifetime_ok, marker)
        } else {
            match prior {
                Some(prior) => {
                    crate::workspace_owner::restart_sticky_eligible(true, true, lifetime_ok, marker)
                        == Some(prior)
                }
                None => {
                    crate::workspace_owner::restart_float_eligible(true, true, lifetime_ok, marker)
                }
            }
        };
        if !eligible || !lifetime_ok || !pid_current(key.hwnd, key.pid) {
            continue;
        }
        if is_override {
            // The override restores classification only: the stamped
            // suppress signal on subsequent observations keeps tiling this
            // live client while fixed hints would float it. No Engine
            // commit, no marker write, no geometry/focus/topmost effect;
            // the marker stays for later restarts. Unmatched records never
            // reach here (fences above); recompute covers everything else.
            // Already-hydrated rows skip silently so later ticks never
            // re-log; hint loss and predicate changes never erase the win.
            if state.fixed_origin_overrides.contains(&key) {
                continue;
            }
            state.fixed_tile_overrides.insert(key.clone());
            state.fixed_origin_overrides.insert(key.clone());
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "tile-adopted",
                    "window": token,
                }),
            );
            continue;
        }
        if engine_is_float(state, &loc.output, &loc.workspace, &token) {
            // The Engine already floats this token: track the runtime lane
            // with the live frame. No marker write, no commit.
            let lane = match prior {
                Some(prior) => {
                    state.sticky.insert(key.clone(), prior);
                    "sticky-adopted"
                }
                None => {
                    state.floated.insert(key.clone());
                    "float-adopted"
                }
            };
            state.float_rects.insert(token.clone(), window.visible);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": lane,
                    "window": token,
                }),
            );
            continue;
        }
        staged.push(Staged {
            key,
            token,
            loc,
            prior,
            frame: window.visible,
        });
    }
    // R-SPC-04 D7 retained pass: maximized/fullscreen/born members never
    // appear in `observed`, so the loop above cannot hydrate them. A
    // surviving tile-override marker on an admitted retained member (fresh
    // max-birth admitted slotless this tick, born-fullscreen hold, or
    // overlaid member) re-adopts the explicit tile win here, before any
    // automatic origin, maximize-clear, slot, or write effect. Workspace
    // membership plus a live-token match gate entry; the shared helper
    // owns every identity read under the full held fence (live process
    // identity, medium integrity, scope, hosted-child, proof, lifetime tag
    // and pid with rechecks on both sides of the peeks). No Engine commit,
    // no marker write, no geometry/focus/topmost effect.
    // Missing/degraded/unmatched records recompute with no hold or retry.
    for row in retained {
        let key = &row.key;
        let Some(token) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        if token != row.token {
            continue;
        }
        if state.workspaces.member_loc(key).is_none() {
            continue;
        }
        hydrate_tile_override_marker(state, me, key, &log_path, &token);
    }
    // One candidate commit per domain over the complete staged set, in
    // first-seen domain order. Every marker window rides floating in a
    // single first-contact event; the event focus names the first tiled row
    // so retained sessions converge focus onto a survivor.
    let mut domains: Vec<(String, String)> = Vec::new();
    for s in &staged {
        if !domains
            .iter()
            .any(|(o, w)| o == &s.loc.output && w == &s.loc.workspace)
        {
            domains.push((s.loc.output.clone(), s.loc.workspace.clone()));
        }
    }
    for (output, workspace) in domains {
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            match s.prior {
                Some(prior) => {
                    state.sticky.insert(s.key.clone(), prior);
                }
                None => {
                    state.floated.insert(s.key.clone());
                }
            }
        }
        let rollback = |state: &mut TileLoop| {
            for s in staged
                .iter()
                .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
            {
                state.sticky.remove(&s.key);
                state.floated.remove(&s.key);
                state.float_rects.remove(&s.token);
                // Observation-scoped write barrier for this exact validated
                // lifetime only: row assembly rides the held key floating
                // and the write gate excludes it, so the rejected candidate
                // takes no tiled writes before the next observation
                // revalidates. Durable markers are untouched and no lane is
                // kept: the next preamble clears and rebuilds this set.
                state.restart_reject_hold.insert(s.key.clone());
            }
        };
        let Some(rows) = assemble_domain_rows(
            state,
            &output,
            &workspace,
            observed,
            retained,
            "restart-adopt",
            correlation.as_str(),
            hint_cx,
        ) else {
            rollback(state);
            continue;
        };
        let Some((domain, domain_key)) =
            workspace_domain_for(&output, &workspace, areas, state.inner_gap, state.outer_gap)
        else {
            rollback(state);
            continue;
        };
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let focused = rows
            .iter()
            .find(|r| !r.floating)
            .map(|r| WindowId(r.token.clone()));
        let mut candidate = state.engine.clone();
        let pre_revision = state
            .engine
            .session(&domain_key)
            .map(|session| session.accepted_revision());
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &output, &workspace),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        stamp_engine_event(state, &mut event);
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "restart-adopt-attempt",
                "correlation": correlation.as_str(),
                "windows": rows.len(),
                "floating": rows.iter().filter(|row| row.floating).count(),
                "focus": focused.as_ref().map(|window| window.0.as_str()),
                "session_present_before": pre_revision.is_some(),
                "session_revision_before": pre_revision,
            }),
        );
        let reply = candidate.handle(&event);
        if matches!(
            reply,
            CoreReply::Rejected { .. } | CoreReply::Diverged(_) | CoreReply::SnapshotInvalid { .. }
        ) {
            // Rejection detail: the per-window lane rows below stay
            // unchanged; this one record carries the full bounded
            // candidate facts plus the Engine reports that distinguish a
            // converge-then-seed fall-through (converged counts with a
            // fallback fit reason, then `duplicate-window`) from an
            // immediate convergence refusal (no converged counts).
            let converged = candidate.last_convergence().map(|report| {
                serde_json::json!({
                    "op": report.op,
                    "removed": report.removed,
                    "admitted": report.admitted,
                    "flags_adopted": report.flags_adopted,
                })
            });
            let adoption_fit = candidate.last_adoption_fit().map(|report| {
                serde_json::json!({
                    "outcome": report.outcome,
                    "windows": report.windows,
                    "reason": report.reason,
                })
            });
            let fixed = candidate.last_fixed_admission().map(|report| {
                serde_json::json!({
                    "op": report.op,
                    "evaluated": report.evaluated,
                    "admitted": report.admitted,
                    "reason": report.reason,
                })
            });
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "candidate-rejected-detail",
                    "correlation": correlation.as_str(),
                    "outcome": "candidate-rejected",
                    "reason": reply_outcome(&reply),
                    "focus": focused.as_ref().map(|window| window.0.as_str()),
                    "windows": rows.len(),
                    "floating": rows.iter().filter(|row| row.floating).count(),
                    "session_present_before": pre_revision.is_some(),
                    "session_revision_before": pre_revision,
                    "converged": converged,
                    "adoption_fit": adoption_fit,
                    "fixed": fixed,
                    "detail": bounded_hydration_candidates(&rows),
                }),
            );
            rollback(state);
            for s in staged
                .iter()
                .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
            {
                let lane = match s.prior {
                    Some(_) => "sticky-adopt",
                    None => "float-adopt",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": lane,
                        "window": s.token,
                        "correlation": correlation.as_str(),
                        "outcome": "candidate-rejected",
                        "reason": reply_outcome(&reply),
                    }),
                );
            }
            continue;
        }
        let mut missing = false;
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            if !candidate
                .session(&domain_key)
                .is_some_and(|sess| sess.is_exception(&WindowId(s.token.clone())))
            {
                missing = true;
                let lane = match s.prior {
                    Some(_) => "sticky-adopt",
                    None => "float-adopt",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": lane,
                        "window": s.token,
                        "correlation": correlation.as_str(),
                        "outcome": "candidate-unfloated",
                    }),
                );
            }
        }
        if missing {
            rollback(state);
            continue;
        }
        state.engine = candidate;
        // Kept: the temporary lane entries above are the re-adopted
        // origins (sticky keeps its prior, never a normal float). Every
        // durable marker stays for later restarts; only settled
        // unfloat/sticky-off removes them.
        for s in staged
            .iter()
            .filter(|s| s.loc.output == output && s.loc.workspace == workspace)
        {
            state.float_rects.insert(s.token.clone(), s.frame);
            let lane = match s.prior {
                Some(_) => "sticky-adopted",
                None => "float-adopted",
            };
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": lane,
                    "window": s.token,
                }),
            );
        }
    }
}
