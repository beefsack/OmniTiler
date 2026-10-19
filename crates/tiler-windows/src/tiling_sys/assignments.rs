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
        let key = crate::workspace::WindowKey {
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
        let stale = crate::workspace_owner::reused_hwnd_stale(
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
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        if !lifetime_ok {
            drop_member_state(state, &key);
        }
        if !lifetime_ok || state.workspaces.member_loc(&key).is_none() {
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
            state.workspaces.ensure_output(&output);
            let Some(active) = state.workspaces.active_id(&output) else {
                continue;
            };
            if state
                .workspaces
                .assign(key.clone(), &output, &active, false)
            {
                state.member_tokens.insert(key.clone(), fresh_token.clone());
                state.member_rects.insert(fresh_token, window.visible);
                state.member_identity.insert(key.clone(), identity);
                state.member_tags.insert(key, fresh_tag);
                admitted_outputs.push(output);
            }
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
    // Born-held first-seen fullscreen members join the workspace slotless
    // (floating Engine observation, no tile slot); later fullscreen on a
    // slotted or lifetime-known window stays a managed overlay transition.
    admitted_outputs.extend(admit_born_fullscreen(state, me, retained, areas));
    // One aggregate hint-query budget for the whole preamble (item 13):
    // the slotless fixed verdicts and the restart-adoption assemblies below
    // share it, so many fresh maximized windows cannot reset the timer per
    // row or per domain. Op assemblies keep their existing per-op budget:
    // coupling multi-intent ticks (notably keyboard_tick) to this timer
    // would change pre-existing move/focus budget behavior.
    let mut hint_cx = HintCx::new();
    // First-seen maximized members join slotless the same way (membership
    // without a tile slot); the mode-gated clear restores them on tiled.
    admitted_outputs.extend(admit_slotless_maximized(
        state,
        me,
        retained,
        areas,
        &mut hint_cx,
    ));
    adopt_restart_markers(state, me, observed, retained, areas, &mut hint_cx);
    // Admission-filled trailing empties mint the next spare now (same tick,
    // gated on successful admission only: observation-only ticks never
    // churn). Restart adoption grants no membership, so it never admits.
    scope_trailing::converge_admitted(state, &admitted_outputs);
    if state.active_output.is_empty()
        && let Some(first) = state.workspaces.output_keys().into_iter().next()
    {
        state.active_output = first;
    }
}

/// Periodic hidden-claim audit: destroyed/recycled claims retire through the
/// identity-safe reveal path (no foreign writes), then drop session
/// membership, tokens, identity, and prune emptied workspaces. A merely
/// invisible window never retires anything: only recovery decides. Unknown
/// identities retain with the ledger kept.
///
/// Death classification truth table (never a close on live evidence):
/// - `Ok` with a different process, or a nonce that no longer equals the
///   claim tag, means this window is gone or recycled: the property dies
///   with its window, so a live claimed window always reads both back.
/// - `absent:*` means the HWND/PID is destroyed or recycled.
/// - `refuse: sid/session mismatch` means the HWND no longer belongs to our
///   session: membership must drop even though something is alive there.
/// - Every other error (`uncertain:*` from access-denied reads, `error:*`
///   owner/nonce failures, `refuse: zero hwnd`) retains with the ledger
///   kept: a removed tag or an access failure on a live window is never a
///   close. The close queue additionally re-verifies the member tag
///   (see [`queue_verified_close`]), so a live window never reports even if
///   retirement below were ever reached for it.
///
/// Diagnostics: each retirement logs one `workspace-reveal` line in the
/// `audit` phase before the state removal below, with the safe opaque member
/// token (null when unknown) and the categorical reveal outcome. The
/// `maintenance` correlation names the acting sweep (there is no action
/// context here) and elapsed reads zero (no action clock). Volume needs no
/// cap: each claim retires at most once, and uncertain claims retain
/// silently.
pub(super) fn audit_hidden_claims(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let keys: Vec<crate::workspace::WindowKey> = state.hidden_claims.keys().cloned().collect();
    let mut retired_any = false;
    for key in keys {
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let dead = match live_query_recovery(key.hwnd, me) {
            Ok(snap) => {
                snap.process != record.claim.process
                    || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
            }
            Err(msg) => {
                if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                    true
                } else {
                    // Unknown: retain with the ledger kept.
                    continue;
                }
            }
        };
        if !dead {
            continue;
        }
        let token = state.member_tokens.get(&key).cloned().unwrap_or_default();
        let sample = if token.is_empty() {
            None
        } else {
            Some(token.as_str())
        };
        let line = match crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        ) {
            Ok(teardown) => reveal_log_line(
                state.tick,
                "maintenance",
                sample,
                "retired",
                Some(reveal_outcome(&teardown)),
                Some("audit"),
                0,
            ),
            Err(e) => {
                let (outcome, detail) = reveal_err_outcome(&e.to_string());
                reveal_log_line(
                    state.tick,
                    "maintenance",
                    sample,
                    outcome,
                    Some(detail),
                    Some("audit"),
                    0,
                )
            }
        };
        log_json_at(&state.log_path.clone(), line);
        state.hidden_claims.remove(&key);
        // D23: a retired hidden close queues exactly like a visible one, so
        // hidden-workspace closes clean their Engine slots on the next valid
        // observation for that domain. Death is already verified above.
        queue_verified_close(state, &key);
        drop_member_state(state, &key);
        retired_any = true;
    }
    // Authoritative uncertain-hide resolution: visibility never clears the
    // hold; only the durable ledger does. A readable ledger with no product
    // entry for the HWND proves absence and clears, so a later app hide can
    // close; a kept entry retains across visible recovery and a second hide;
    // an unreadable ledger retains. Proven-absent clears drop the marker
    // only, never membership, so the member still converges on its retained
    // slot.
    if !state.vis_diag.uncertain_hides.is_empty() {
        match store.committed() {
            Err(_) => {}
            Ok(record) => {
                let mut proven_absent = Vec::new();
                for k in state.vis_diag.uncertain_hides.iter() {
                    let kept = record.as_ref().is_some_and(|r| {
                        r.owner == *me
                            && r.windows.iter().any(|w| {
                                w.hwnd == k.hwnd && w.kind == crate::model::WindowClaimKind::Product
                            })
                    });
                    if !kept {
                        proven_absent.push(k.clone());
                    }
                }
                for k in proven_absent {
                    state.vis_diag.uncertain_hides.remove(&k);
                }
            }
        }
    }
    if retired_any {
        // Trailing convergence through the shared scope contract (item 12
        // stage B): local keeps the exact legacy per-output loop.
        scope_trailing::converge_all(state);
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
fn queue_verified_close(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
    let Some(token) = state.member_tokens.get(key).cloned() else {
        return;
    };
    if token.is_empty() {
        return;
    }
    let Some(loc) = state.workspaces.member_loc(key).cloned() else {
        return;
    };
    let Some(stored) = state.member_tags.get(key).cloned() else {
        return;
    };
    if stored.is_empty() {
        return;
    }
    if crate::workspace_owner::visible_lifetime_ok(&stored, live_member_tag(key.hwnd).as_deref()) {
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
fn external_hide_candidate(state: &TileLoop, key: &crate::workspace::WindowKey) -> bool {
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
    if !crate::workspace_owner::visible_lifetime_ok(stored, live_member_tag(key.hwnd).as_deref()) {
        return false;
    }
    let pid = live_window_pid(key.hwnd);
    if pid == 0 || pid != key.pid {
        return false;
    }
    state.workspaces.member_loc(key).is_some()
}

/// Queue one app-initiated hide as a verified close through the existing
/// `pending_closed`/`closed_windows` path. Requires the same token/location/
/// tag presence as the destroyed path, but requires the tag to still read
/// live (the window is alive and hidden, not destroyed). Empty ids never
/// queue.
fn queue_external_hide_close(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
    let Some(token) = state.member_tokens.get(key).cloned() else {
        return;
    };
    if token.is_empty() {
        return;
    }
    let Some(loc) = state.workspaces.member_loc(key).cloned() else {
        return;
    };
    let Some(stored) = state.member_tags.get(key).cloned() else {
        return;
    };
    if stored.is_empty() {
        return;
    }
    if !crate::workspace_owner::visible_lifetime_ok(&stored, live_member_tag(key.hwnd).as_deref()) {
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
    let gone: Vec<crate::workspace::WindowKey> = state
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
    let hidden_close: Vec<crate::workspace::WindowKey> = state
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
        for output in state.workspaces.output_keys() {
            for id in state.workspaces.workspace_ids(&output) {
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

/// Hide one managed member bound to its stored full identity: the live
/// window must equal the stored [`ProcessIdentity`] exactly, so a recycled
/// HWND refuses with no writes. Fresh proof gate on every write in proof
/// modes; commit-before-hide through the central watcher/ledger. Outcomes
/// are typed at the source, never selected by error strings. The detail
/// carries the managed-admission refusal code when the outcome is
/// `refused`, so the select hide loop can log the exact failure.
///
/// Bounded per-window hide diagnostics cap for one select transition.
pub(super) const MAX_HIDE_DIAG_PER_SELECT: usize = 64;
pub(super) fn workspace_hide_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    key: &crate::workspace::WindowKey,
    expected: &ProcessIdentity,
    iconic: bool,
) -> (&'static str, Option<&'static str>) {
    // Shared admission with the joint batch phases (identical fences), so
    // the remaining single-window path (migration) cannot diverge in safety.
    // Only the wait below stays per-window; select/send/rollback phases use
    // the joint batch instead.
    let claim = match super::visibility_batch::admit_hide_item(state, me, key, expected) {
        Ok(claim) => claim,
        Err((outcome, detail)) => return (outcome, detail),
    };
    match crate::product_hide::sys::hide_managed_claim(store, me, &claim, dir) {
        Ok(committed) => {
            state.hidden_claims.insert(
                key.clone(),
                HiddenRecord {
                    claim: committed,
                    iconic,
                },
            );
            state.vis_diag.uncertain_hides.remove(key);
            ("hidden", None)
        }
        Err(e) => {
            // Commit-before-hide appends the durable enriched claim BEFORE the
            // async visibility post, so an already-committed claim should
            // register in the owner table even when the post-hide readback
            // fails. That rescue re-read is fallible, however: when it fails
            // the window may be hidden with a kept ledger but no owner-table
            // entry. The uncertain-hide hold marker covers that gap (never a
            // close); the joint batch path needs none of this because it
            // registers before posting. Never re-admit the claimed HWND nonce
            // and never fabricate a rollback. The hold applies to every
            // post-commit failure with unreadable rescue evidence, not just
            // the `uncertain:` prefix: `error:` posts, `refuse:` readbacks and
            // any other kept-ledger error with a failed rescue must also
            // hold. A readable ledger with no durable entry proves no commit
            // (pre-commit refusal or an `absent:` post that already removed
            // its note) and takes no marker; unreadable rescue always holds
            // until authoritative ledger evidence resolves it.
            let (rescued, ledger_unreadable) = if rescue_read_failed() {
                (false, true)
            } else {
                match store.committed() {
                    Ok(Some(record)) if record.owner == *me => {
                        if let Some(durable) = record.windows.iter().find(|w| {
                            w.hwnd == claim.hwnd
                                && w.tag == claim.tag
                                && w.process == claim.process
                                && w.kind == crate::model::WindowClaimKind::Product
                        }) {
                            state.hidden_claims.insert(
                                key.clone(),
                                HiddenRecord {
                                    claim: durable.clone(),
                                    iconic,
                                },
                            );
                            state.vis_diag.uncertain_hides.remove(key);
                            (true, false)
                        } else {
                            (false, false)
                        }
                    }
                    Ok(_) => (false, false),
                    Err(_) => (false, true),
                }
            };
            let msg = e.to_string();
            if !rescued && ledger_unreadable {
                state.vis_diag.uncertain_hides.insert(key.clone());
            }
            if msg.starts_with("absent:") {
                ("origin-vanished", None)
            } else if msg.starts_with("uncertain:") {
                ("uncertain", None)
            } else {
                ("refused", None)
            }
        }
    }
}

/// Fresh proof-owned gate for one reveal write: in proof modes the helper
/// must verify against the frozen allowlist immediately before the reveal
/// effect, exactly like the hide path.
pub(super) fn workspace_proof_reveal_gate(
    state: &TileLoop,
    me: &ProcessIdentity,
    hwnd: u64,
) -> std::result::Result<(), &'static str> {
    let Some(entries) = state.allowlist.as_ref() else {
        return Ok(());
    };
    if !state.workspace_proof {
        return Err("workspace-disabled");
    }
    let Some(entry) = entries.iter().find(|e| e.hwnd == hwnd) else {
        return Err("allowlist-changed");
    };
    verify_proof_owned(hwnd, entry, me).map_err(|_| "identity-changed")
}
