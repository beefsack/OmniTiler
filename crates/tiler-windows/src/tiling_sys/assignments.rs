#![forbid(unsafe_code)]
use super::*;

/// Assign newly seen eligible windows to the active workspace of their
/// monitor output. Hidden-claim HWNDs are never re-admitted into another
/// domain here. A nonempty scope admits only the named executables.
///
/// Visible-membership lifetime: every admitted window carries our inert
/// window-lifetime tag, verified live against the stored copy on every tick.
/// A same-process HWND reuse (same HWND/PID/creation, fresh window without
/// our tag) repairs here as a brand-new window: stale membership, Engine
/// token, identity, tag, and focus memory drop first, then a fresh tag is
/// stamped, a fresh Engine token is issued, and the current tick's row is
/// patched to it, so the new generation inherits no membership, focus, or
/// layout. Stamping writes only our own property name (dies with the window,
/// never trusted across runs, never removed by us); a failed stamp skips
/// admission fail-closed for the tick.
pub(super) fn ensure_workspace_assignments(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    areas: &[MonitorArea],
) {
    let mut admitted_outputs: Vec<String> = Vec::new();
    for window in observed.iter_mut() {
        if !scope_allows(&state.scope, &window.identity.exe_path) {
            continue;
        }
        let key = crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        // Uncertain-hide ownership persists across visible observations: a
        // prior single-path hide may have committed a durable ledger claim
        // while its rescue re-read failed, so visibility alone never proves
        // absence. Only authoritative durable-ledger evidence (see
        // `audit_hidden_claims` resolution) clears the hold, never a visible
        // read. Hidden-claim and HWND-recycle paths below stay authoritative.
        // Never re-admit a claimed HWND nonce into another domain: any
        // same-HWND hidden claim blocks assignment regardless of key.
        if state.hidden_claims.keys().any(|k| k.hwnd == window.hwnd) {
            continue;
        }
        // Same-HWND reuse repair (cross-process): the fresh enumeration read
        // is authoritative, so stale same-HWND members (a destroyed window
        // whose number the OS recycled while visible) drop here instead of
        // stalling Engine convergence with a viewless member. Live hidden
        // claims never drop here; they retire through the ledger/audit path.
        let stale = crate::native_guards::reused_hwnd_stale(
            &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
            &state.hidden_claims.keys().cloned().collect(),
            &key,
        );
        for dead in stale {
            drop_member_state(state, &dead);
        }
        // Visible lifetime gate: the live tag must equal the stored tag. A
        // mismatch with the same key is a same-process reuse (or an untracked
        // window): drop every stale table so the admission below treats the
        // live window as brand new with no inheritance. The test-aware read
        // is behavior-identical in production (same native call when the
        // fixture seam is inactive).
        let live_tag = live_member_tag(window.hwnd);
        let lifetime_ok = state.member_tags.get(&key).is_some_and(|stored| {
            crate::native_guards::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        if !lifetime_ok {
            drop_member_state(state, &key);
        }
        if !lifetime_ok || state.member_home(&key).is_none() {
            // Fresh admission binds the stamp to the expected full identity:
            // the live window is re-read (identity, eligibility, hosted
            // children) and exact proof ownership is re-verified before any
            // SetProp effect. Anything stale or foreign skips fail-closed.
            let identity = ProcessIdentity {
                pid: window.identity.pid,
                process_creation: window.identity.process_creation.clone(),
                user_sid: window.identity.user_sid.clone(),
                session_id: window.identity.session_id,
                exe_path: window.identity.exe_path.clone(),
            };
            if let Some(entries) = state.allowlist.as_ref() {
                let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
                    continue;
                };
                if verify_proof_owned(key.hwnd, entry, me).is_err() {
                    continue;
                }
            }
            let admitted = match crate::product_hide::sys::classify_candidate(window.hwnd, me) {
                crate::product_hide::sys::CandidateStatus::Admissible(snap)
                | crate::product_hide::sys::CandidateStatus::Retained(snap) => snap,
                _ => continue,
            };
            if admitted.pid != identity.pid || admitted.process != identity {
                continue;
            }
            if !hosted_gate_allows(
                &identity.exe_path,
                window.hwnd,
                identity.pid,
                &state.scope_hosts,
            ) {
                continue;
            }
            let fresh_tag = match crate::product_hide::sys::install_member_tag(window.hwnd, me.pid)
            {
                Ok(tag) => tag,
                Err(_) => continue,
            };
            // Post-stamp postcheck: the expected identity must still be live
            // with the fresh tag reading back, or no membership is stored.
            let post_ok = crate::product_hide::sys::hold_target_verified(identity.pid, &identity)
                .is_ok()
                && pid_current(window.hwnd, identity.pid)
                && crate::product_hide::sys::read_member_tag(window.hwnd).as_deref()
                    == Some(fresh_tag.as_str());
            if !post_ok {
                continue;
            }
            let fresh_token = state
                .tokens
                .reissue(window.hwnd, &window.identity.process_creation);
            window.token = fresh_token.clone();
            let output = output_for_rect(areas, &window.visible);
            state.ensure_device(&output);
            // Membership homing is core-owned (logical rings/views from the
            // first report screens): the adapter binds only the mechanical
            // token/identity/tag triple, never a workspace.
            state.member_tokens.insert(key.clone(), fresh_token.clone());
            state
                .member_rects
                .insert(fresh_token.clone(), window.visible);
            state.member_identity.insert(key.clone(), identity);
            state.member_tags.insert(key.clone(), fresh_tag);
            // Trace-only token<->HWND join for native correlation (see the
            // world-tick admission path): stable token plus the full member
            // identity, no titles or app content. Fresh admissions only.
            if state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "member-admitted",
                        "tick": state.tick,
                        "window": fresh_token,
                        "hwnd": key.hwnd,
                        "pid": key.pid,
                        "creation": key.creation,
                    }),
                );
            }
            admitted_outputs.push(output);
            continue;
        }
        let identity = ProcessIdentity {
            pid: window.identity.pid,
            process_creation: window.identity.process_creation.clone(),
            user_sid: window.identity.user_sid.clone(),
            session_id: window.identity.session_id,
            exe_path: window.identity.exe_path.clone(),
        };
        state
            .member_tokens
            .insert(key.clone(), window.token.clone());
        state
            .member_rects
            .insert(window.token.clone(), window.visible);
        state.member_identity.insert(key, identity);
    }
    // Born-held first-seen fullscreen members join slotless the same way
    // (membership without homing); later fullscreen on a lifetime-known
    // window stays a managed overlay transition.
    let _ = admit_born_fullscreen(state, me, retained, areas);
    // One aggregate hint-query budget for the whole preamble (item 13):
    // the slotless fixed verdicts and the restart-adoption assemblies below
    // share it, so many fresh maximized windows cannot reset the timer per
    // row or per domain. Op assemblies keep their existing per-op budget:
    // coupling multi-intent ticks (notably keyboard_tick) to this timer
    // would change pre-existing move/focus budget behavior.
    let mut hint_cx = HintCx::new();
    // First-seen maximized members bind slotless the same way; the
    // mode-gated clear restores them on tiled.
    let _ = admit_slotless_maximized(state, me, retained, areas, &mut hint_cx);
    adopt_restart_markers(state, me, observed, retained, areas, &mut hint_cx);
    // Trailing lifecycle is core-owned: the adapter mints nothing here.
    let _ = admitted_outputs;
    if state.active_output.is_empty()
        && let Some(first) = state.output_devices().into_iter().next()
    {
        state.active_output = first;
    }
}

/// D23 close-versus-minimize classification at the observation/lifetime
/// boundary: queue one verified native close for Engine slot cleanup. The
/// caller already established enumeration absence (the HWND left the raw
/// inventory, so this is never minimize, transient, or unreadable evidence);
/// the stored lifetime tag must additionally no longer read live (a destroyed
/// window loses its tag with the window, while a live window still reads it).
/// Members without a stored tag keep the existing drop-only behavior, and
/// empty ids never queue. No production slot leaks through that lane: every
/// production admission stamps a tag (born-fullscreen, slotless-maximized,
/// and eligible-visible admission), hiding refuses without one, and restart
/// adoption admits no new members, so a tagless member is only ever a test
/// fixture. Homing is read now, while the member still has a workspace
/// location, and covers hidden workspaces and every domain, so multi-domain
/// close bursts all queue before their member state drops.
fn queue_verified_close(state: &mut TileLoop, key: &crate::native_guards::WindowKey) {
    let Some(token) = state.member_tokens.get(key).cloned() else {
        return;
    };
    if token.is_empty() {
        return;
    }
    let Some(loc) = state.member_home(key) else {
        return;
    };
    let Some(stored) = state.member_tags.get(key).cloned() else {
        return;
    };
    if stored.is_empty() {
        return;
    }
    if crate::native_guards::visible_lifetime_ok(&stored, live_member_tag(key.hwnd).as_deref()) {
        return;
    }
    state.pending_closed.insert(
        token,
        PendingClosed {
            output: loc.output,
            workspace: loc.workspace,
        },
    );
}

/// App-initiated hide as close (adapter-only, no shared-core change):
/// a managed, lifetime-fenced, alive, invisible, uncloaked window with no
/// tiler hide claim queues through the existing `closed_windows` path, so
/// survivors reflow and MRU resolves under the existing close rules. Reshow
/// is an ordinary fresh admission with no remembered slot (member state
/// drops here, the Engine forgets the slot on acceptance).
///
/// Fences (all must pass, fail closed to hold with retained membership):
/// - member with workspace location, token, and non-empty stored tag;
/// - present in the raw enumeration inventory (`last_hwnds`);
/// - no tiler hide claim for the key or the HWND (workspace hides hold);
/// - no single-path uncertain-hide hold for the key or the HWND (a hide
///   whose ledger may be kept but whose rescue re-read failed holds);
/// - not minimized-omitted and not natively iconic (minimize holds);
/// - live window (`IsWindow`), invisible (`IsWindowVisible` false),
///   readable uncloaked (`DWMWA_CLOAKED` false; `None` or `true` holds as
///   unknown/unreadable);
/// - lifetime-fenced alive: stored tag still reads live plus owning PID
///   still current and non-zero (unknown/unreadable holds).
///
/// Ledger-backed hidden retirement stays with the audit path. The joint
/// batch registers its in-memory claim before posting, but the single-path
/// rescue re-read is fallible: an uncertain hide whose rescue read fails
/// leaves a durable ledger claim with no owner-table entry, covered here by
/// the uncertain-hide hold (never a close). Restart needs no marker:
/// ledger-hidden windows cannot admit while invisible, and the tagged
/// lifetime fences already hold.
fn external_hide_candidate(state: &TileLoop, key: &crate::native_guards::WindowKey) -> bool {
    if state.hidden_claims.contains_key(key)
        || state.hidden_claims.keys().any(|k| k.hwnd == key.hwnd)
    {
        return false;
    }
    if state.vis_diag.uncertain_hides.contains(key)
        || state
            .vis_diag
            .uncertain_hides
            .iter()
            .any(|k| k.hwnd == key.hwnd)
    {
        return false;
    }
    if !state.last_hwnds.contains(&key.hwnd) {
        return false;
    }
    if state.minimized_omitted.contains(key) {
        return false;
    }
    if live_is_iconic(key.hwnd) {
        return false;
    }
    if !live_is_window(key.hwnd) {
        return false;
    }
    if live_is_visible(key.hwnd) {
        return false;
    }
    if live_cloaked(key.hwnd) != Some(false) {
        return false;
    }
    let Some(stored) = state.member_tags.get(key) else {
        return false;
    };
    if stored.is_empty() {
        return false;
    }
    if !crate::native_guards::visible_lifetime_ok(stored, live_member_tag(key.hwnd).as_deref()) {
        return false;
    }
    let pid = live_window_pid(key.hwnd);
    if pid == 0 || pid != key.pid {
        return false;
    }
    state.member_home(key).is_some()
}

/// Queue one app-initiated hide as a verified close through the existing
/// `pending_closed`/`closed_windows` path. Requires the same token/location/
/// tag presence as the destroyed path, but requires the tag to still read
/// live (the window is alive and hidden, not destroyed). Empty ids never
/// queue.
fn queue_external_hide_close(state: &mut TileLoop, key: &crate::native_guards::WindowKey) {
    let Some(token) = state.member_tokens.get(key).cloned() else {
        return;
    };
    if token.is_empty() {
        return;
    }
    let Some(loc) = state.member_home(key) else {
        return;
    };
    let Some(stored) = state.member_tags.get(key).cloned() else {
        return;
    };
    if stored.is_empty() {
        return;
    }
    if !crate::native_guards::visible_lifetime_ok(&stored, live_member_tag(key.hwnd).as_deref()) {
        return;
    }
    state.pending_closed.insert(
        token,
        PendingClosed {
            output: loc.output,
            workspace: loc.workspace,
        },
    );
}

/// Drop membership for windows that vanished from the raw enumeration
/// inventory. Iconic (minimized) members stay enumerated with identity, as do
/// maximized/fullscreen/overlaid members, so only a truly absent HWND cleans
/// up; hidden claims retire through the reveal path, never here. Unknown
/// identities (enumeration without resolution) retain membership. Born holds
/// clean here too: a closed born window releases its hold with the
/// once-per-lifecycle line and drops its snapshot seed.
///
/// D23 close-versus-minimize distinction: an absent member whose stored
/// lifetime tag no longer reads live queues its Engine token with live-time
/// homing, so the next valid domain observation reports it as
/// `closed_windows` and the Engine forgets the slot instead of minting one.
/// Minimize, transient, and unreadable evidence never queue: those HWNDs stay
/// enumerated (present in the inventory), and members without a stored tag
/// keep the existing drop-only behavior.
pub(super) fn workspace_close_cleanup(state: &mut TileLoop) {
    let gone: Vec<crate::native_guards::WindowKey> = state
        .member_tokens
        .keys()
        .filter(|k| !state.last_hwnds.contains(&k.hwnd) && !state.hidden_claims.contains_key(k))
        .cloned()
        .collect();
    let log_path = state.log_path.clone();
    for key in gone {
        queue_verified_close(state, &key);
        let born = state.born_fullscreen.remove(&key);
        let token = state.member_tokens.get(&key).cloned();
        if born && let Some(token) = token {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
        drop_member_state(state, &key);
    }
    // App-initiated hide as close: managed, fenced, alive, invisible,
    // uncloaked members with no tiler claim queue through the same
    // `closed_windows` path and drop member state, so survivors reflow and
    // reshow is a fresh admission. Unknown/unreadable (cloak/tag/pid) and
    // tiler workspace hides hold with retained membership.
    let hidden_close: Vec<crate::native_guards::WindowKey> = state
        .member_tokens
        .keys()
        .filter(|k| external_hide_candidate(state, k))
        .cloned()
        .collect();
    for key in hidden_close {
        queue_external_hide_close(state, &key);
        let born = state.born_fullscreen.remove(&key);
        let token = state.member_tokens.get(&key).cloned();
        if born && let Some(token) = token {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "initial-fullscreen-released",
                    "window": token,
                }),
            );
        }
        drop_member_state(state, &key);
    }
    state
        .born_fullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    state
        .born_keep_overlay
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .retile_keep_overlay
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .fixed_tile_overrides
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .fixed_origin_overrides
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .fixed_auto_seen
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .born_first_exit
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .overlay_now
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .seen_nonfullscreen
        .retain(|key| state.last_hwnds.contains(&key.hwnd));
    // Prune deferred-domain signatures by live domains so removed outputs or
    // workspaces never leak a stale transition baseline.
    {
        let mut live: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        for output in state.output_devices() {
            for id in state.workspace_ids(&output) {
                live.insert((output.clone(), id));
            }
        }
        state
            .vis_diag
            .defer_last
            .retain(|domain, _| live.contains(domain));
    }
    // Uncertain-hide holds never outlive their member and never shadow a
    // real claim: drop with membership, and resolve where the owner table
    // now carries the durable claim (joint-path and rescue-success inserts
    // land between ticks without touching this file).
    state
        .vis_diag
        .uncertain_hides
        .retain(|k| state.member_tokens.contains_key(k) && !state.hidden_claims.contains_key(k));
    prune_float_state(state);
}

/// Bounded per-window hide diagnostics cap for one select transition.
#[cfg(test)]
pub(super) const MAX_HIDE_DIAG_PER_SELECT: usize = 64;
