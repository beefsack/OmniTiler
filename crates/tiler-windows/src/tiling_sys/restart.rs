#![forbid(unsafe_code)]
use super::*;

/// Maximum hydration-candidate rows carried in one normal rejection detail
/// record. Restart hydration sets are small (one domain's staged floats
/// plus its tiled rows); the cap keeps normal-log expansion bounded while
/// `total`/`omitted` keep the count exact. Integer geometry plus opaque
/// tokens only: no HWNDs, pids, paths, or titles.
#[cfg(test)]
pub(super) const RESTART_HYDRATION_DETAIL_CAP: usize = 8;

/// Bounded candidate facts for a rejected hydration attempt, for the normal
/// log. `total` is the exact row count; only the first
/// [`RESTART_HYDRATION_DETAIL_CAP`] rows are carried and `omitted` counts
/// the rest, so the record never grows with the domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(super) struct HydrationCandidate<'a> {
    pub token: &'a str,
    pub rect: Rect,
    pub floating: bool,
    pub hints: WindowSizeHints,
}

#[cfg(test)]
pub(super) fn bounded_hydration_candidates(rows: &[HydrationCandidate<'_>]) -> serde_json::Value {
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
        if state.member_home(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd)
        {
            continue;
        }
        let known_slot = state.member_rects.contains_key(&row.token);
        if !tiler_core::window_policy::should_hold_born_fullscreen(
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
        let stale = crate::native_guards::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.member_home(&row.key).is_some() || state.member_tokens.contains_key(&row.key) {
            continue;
        }
        let member_tag = match crate::product_hide::sys::ensure_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(member_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.ensure_device(&output);
        // Homing is core-owned: the adapter binds only the mechanical
        // token/identity/tag triple, never a workspace.
        state.member_tokens.insert(row.key.clone(), token.clone());
        state.member_identity.insert(row.key.clone(), live);
        state.member_tags.insert(row.key.clone(), member_tag);
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
    admitted
}

/// Slotless maximized admission (R-MAX-03 / KDE floating parity): first-seen
/// maximized, non-fullscreen retained windows without workspace membership
/// and without a retained tile slot join the active workspace slotless:
/// membership plus stable token, identity, and lifetime tag for hide/reveal
/// and focus, but no `member_rects` tile slot, no Engine exception, and no
/// hold set. A floating workspace therefore preserves the native frame (the
/// mode-gated clear skips) while select-away/back still hides and reveals.
/// Born-maximized overlay policy (both `core.born_maximized` values) is
/// core-owned from the captured facts: the adapter keeps no keep-overlay
/// latch. An admitted explicit tile override (canonical intent slot, read
/// here before the tick snapshot) only suppresses the D4 automatic fixed
/// latch. Same exact-lifetime, scope, hosted-child, and proof fences as the
/// born-fullscreen admission.
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
        let is_member = state.member_home(&row.key).is_some()
            || state.member_tokens.contains_key(&row.key)
            || state.hidden_claims.keys().any(|k| k.hwnd == row.key.hwnd);
        if !tiler_core::window_policy::should_admit_slotless_maximized(
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
        let stale = crate::native_guards::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &row.key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        if state.member_home(&row.key).is_some() || state.member_tokens.contains_key(&row.key) {
            continue;
        }
        let member_tag = match crate::product_hide::sys::ensure_member_tag(row.key.hwnd, me.pid) {
            Ok(tag) => tag,
            Err(_) => continue,
        };
        let post_ok = crate::product_hide::sys::hold_target_verified(live.pid, &live).is_ok()
            && pid_current(row.key.hwnd, live.pid)
            && crate::product_hide::sys::read_member_tag(row.key.hwnd).as_deref()
                == Some(member_tag.as_str());
        if !post_ok {
            continue;
        }
        let token = row.token.clone();
        let output = output_for_rect(areas, &frame);
        state.ensure_device(&output);
        // R-SPC-04 D7: a durable explicit tile win rides the canonical packed
        // intent slot. This first admission classifies before the tick's
        // intent snapshot refresh, so it reads the slot once for the exact
        // admitted lifetime (pid, creation, and the tag proven above) and
        // keeps no adapter mirror: every later tick reads the same slot
        // through the snapshot, and this admission uses the snapshot's own
        // tile-override projection. Only a verified tile override suppresses
        // the D4 automatic fixed latch below; absent, unverifiable, sticky,
        // and float lanes recompute ordinary fixed classification with no
        // hold or retry.
        let tile_override = super::world_native::intent_storage_of(&row.key, &member_tag)
            .is_some_and(super::IntentStorage::is_tiled);
        // Homing is core-owned: the adapter binds only the mechanical
        // token/identity/tag triple, never a workspace.
        state.member_tokens.insert(row.key.clone(), token.clone());
        state.member_identity.insert(row.key.clone(), live);
        state.member_tags.insert(row.key.clone(), member_tag);
        // R-SPC-04 D4: a fixed max-birth floats beneath native maximize
        // (membership-only, no clear, no reserved tile slot). The verdict
        // uses fresh tracks under the live predicate with the same
        // failure-as-unknown rules; unknown fails closed to the existing
        // lane. The latch is lifetime-bound and pruned with membership;
        // later explicit intent comes from the canonical slot, never this
        // latch.
        // `hint_cx` is the preamble-shared aggregate: many fresh maximized
        // windows share one timer, never one per row. A durable tile
        // override never probes or latches (D7 wins).
        let fixed_birth = !tile_override
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
    admitted
}
