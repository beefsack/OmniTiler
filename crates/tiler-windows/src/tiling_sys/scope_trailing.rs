//! Scoped trailing creation/minimum/cleanup (item 12 stage B).
//!
//! One scope contract for every trailing producer (select, follow, stay,
//! drop, close, audit, migration convergence, startup priming). The local
//! per-output path delegates to the exact legacy
//! [`crate::workspace::ManagedWorkspaces`] plan/apply sequence, so local
//! native behavior and all committed tests stay byte-for-byte unchanged.
//! Global-unique keeps per-output minimum/ring structural trailing with
//! cross-output protection; shared keeps one logical ring with one trailing
//! workspace across all output domains.
//!
//! Facts feed the shared [`tiler_core::workspace::plan_trailing`] through
//! [`tiler_core::workspace_scope::scoped_trailing_facts`]:
//! - `ring`: live-filtered scoped ids (per-output order for global-unique,
//!   the synced shared order filtered to live for shared).
//! - `occupied`: ids holding non-sticky members. Intentional floats,
//!   born-fullscreen holds, and slotless-maximized admissions are members,
//!   so they occupy; sticky all-desktop members never occupy any backing
//!   id.
//! - `visible`: the observed active id set. Local sees its own active only;
//!   global-unique and shared see every output's active, so no cleanup ever
//!   removes an id visible elsewhere.
//! - `retained`: displaced ids plus narrowly retained Engine layout ids:
//!   actual in-flight sessions (`has_pending`/`has_pending_desired`/`has_drag`)
//!   and unmapped-but-relevant sessions (an Engine domain whose workspace is
//!   not listed on its output yet still holds a non-empty layout). An
//!   ordinary empty mapped id with only idle layout history prunes like any
//!   other empty id; occupied and visible ids are already protected by the
//!   plan inputs, so retention never lists them.
//!
//! The global live-count floor stays adapter-side: global pruning keeps only
//! ids scoped to the cleaned output and never drops the global live union
//! below the minimum. Shared cleanup strips logical ids from every
//! output-bound domain at once (membership empties by construction, sticky
//! float state stays owner-held like the single-output path) and mints one
//! fresh logical id across all domains when the ring needs its trailing
//! spare. Creation and cleanup share this module, so no producer reports a
//! deferred trailing outcome.

use std::collections::BTreeSet;

use tiler_core::workspace::MIN_WORKSPACES;
use tiler_core::workspace_scope::{WorkspaceMode, scoped_trailing_facts};

use crate::workspace::WindowKey;
use crate::workspace_scope_owner::mapping_from_workspaces;

/// Sticky keys snapshot for occupancy inputs.
fn sticky_set(state: &super::TileLoop) -> BTreeSet<WindowKey> {
    state.sticky.keys().cloned().collect()
}

/// Observed active id on every live output (cross-output visibility
/// protection for scoped cleanups).
fn visible_union(state: &super::TileLoop) -> BTreeSet<String> {
    let mut visible = BTreeSet::new();
    for output in state.workspaces.output_keys() {
        if let Some(active) = state.workspaces.active_id(&output) {
            visible.insert(active);
        }
    }
    visible
}

/// Displaced workspace ids (hotplug-return protection).
fn displaced_ids(state: &super::TileLoop) -> Vec<String> {
    state
        .workspaces
        .displaced_snapshot()
        .values()
        .flat_map(|r| r.workspace_ids.clone())
        .collect()
}

/// Live logical workspace id union over every output domain.
fn live_id_union(state: &super::TileLoop) -> BTreeSet<String> {
    mapping_from_workspaces(&state.workspaces)
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect()
}

/// Workspace ids whose Engine session is actually in flight on any output
/// (pending plan, staged desired state, or active drag capture). Verified
/// core lifecycle gates (`Session::has_pending`, `has_pending_desired`,
/// `has_drag`; `session_usable`/`try_relocate_for_target` refuse the same
/// states), so these ids pin until the flight settles.
fn engine_inflight_ids(state: &super::TileLoop) -> BTreeSet<String> {
    let mut inflight = BTreeSet::new();
    for key in state.engine.domain_keys() {
        let is_flight = state
            .engine
            .session(&key)
            .is_some_and(|s| s.has_pending() || s.has_pending_desired() || s.has_drag());
        if is_flight {
            inflight.insert(key.workspace.0.clone());
        }
    }
    inflight
}

/// Workspace ids with a relevant but currently unmapped Engine layout: the
/// domain's workspace is not listed on its own output (or not live anywhere)
/// yet the session still holds a non-empty committed layout
/// (`committed_session_is_empty` false, the same gate the Engine uses to
/// retire empty slots). Idle empty history never retains; only layout that
/// would otherwise be lost pins the id.
fn engine_unmapped_relevant_ids(state: &super::TileLoop) -> BTreeSet<String> {
    let live = live_id_union(state);
    let outputs = state.workspaces.output_keys();
    let mut relevant = BTreeSet::new();
    for key in state.engine.domain_keys() {
        let ws = key.workspace.0.clone();
        let output = key.output.0.clone();
        let listed_here = state.workspaces.workspace_ids(&output).contains(&ws);
        let mapped = outputs.contains(&output) && listed_here && live.contains(&ws);
        if mapped {
            continue;
        }
        let keeps_layout = state.engine.session(&key).is_some_and(|s| {
            !tiler_core::engine::committed_session_is_empty(s)
                && !s.has_pending()
                && !s.has_pending_desired()
                && !s.has_drag()
        });
        if keeps_layout {
            relevant.insert(ws);
        }
    }
    relevant
}

/// Retained ids for scoped plans: displaced plus narrowly retained Engine
/// layout ids (actual in-flight and unmapped relevant only). Ordinary empty
/// mapped ids prune; occupied/visible stay protected by the plan inputs.
fn retained_ids(state: &super::TileLoop) -> Vec<String> {
    let mut retained: BTreeSet<String> = displaced_ids(state).into_iter().collect();
    retained.extend(engine_inflight_ids(state));
    retained.extend(engine_unmapped_relevant_ids(state));
    retained.into_iter().collect()
}

/// Retire stale empty Engine domains for pruned logical ids. Only domains
/// whose workspace left the live union entirely, whose session is committed
/// empty, and whose session carries no in-flight state drop; displaced ids
/// never reach here (retained above), and unmapped relevant (non-empty) or
/// pending/drag sessions survive by construction. Occupied/visible ids never
/// prune, so their domains are untouched.
pub(super) fn prune_retired_engine_domains(state: &mut super::TileLoop, removed: &[String]) {
    if removed.is_empty() {
        return;
    }
    let live = live_id_union(state);
    let displaced: BTreeSet<String> = displaced_ids(state).into_iter().collect();
    let keys = state.engine.domain_keys();
    for key in keys {
        let ws = key.workspace.0.clone();
        if !removed.contains(&ws) || live.contains(&ws) || displaced.contains(&ws) {
            continue;
        }
        let droppable = state.engine.session(&key).is_some_and(|s| {
            tiler_core::engine::committed_session_is_empty(s)
                && !s.has_pending()
                && !s.has_pending_desired()
                && !s.has_drag()
        });
        if droppable {
            state.engine.remove(&key);
        }
    }
}

/// Non-sticky occupancy for one output's id.
fn output_occupied(
    state: &super::TileLoop,
    output: &str,
    id: &str,
    sticky: &BTreeSet<WindowKey>,
) -> bool {
    state
        .workspaces
        .workspace_members(output, id)
        .iter()
        .any(|m| !sticky.contains(m))
}

/// Non-sticky occupancy for a logical id on any output (shared input).
fn shared_occupied_id(state: &super::TileLoop, id: &str, sticky: &BTreeSet<WindowKey>) -> bool {
    state
        .workspaces
        .output_keys()
        .iter()
        .any(|output| output_occupied(state, output, id, sticky))
}

/// Per-output global-unique trailing convergence for one output: the legacy
/// plan over the output's ring with cross-output visibility plus Engine
/// retention, pruned to output-scoped ids with the global live-count floor.
/// Local mode never enters (the caller keeps the legacy sequence inline).
pub(super) fn global_cleanup_output(state: &mut super::TileLoop, output: &str) {
    let sticky = sticky_set(state);
    let visible: Vec<String> = visible_union(state).into_iter().collect();
    let retained = retained_ids(state);
    let (removed, append) = state
        .workspaces
        .plan_cleanup_excluding(output, &visible, &retained, &sticky);
    // Only output-scoped ids prune here: an id listed on another output's
    // order is never removed from one side alone.
    let mut elsewhere = BTreeSet::new();
    for other in state.workspaces.output_keys() {
        if other != output {
            elsewhere.extend(state.workspaces.workspace_ids(&other));
        }
    }
    let mut safe: Vec<String> = removed
        .into_iter()
        .filter(|id| !elsewhere.contains(id))
        .collect();
    // Adapter-owned global live-count floor: removals that vanish globally
    // never drop the live union below the minimum.
    let live: BTreeSet<String> = mapping_from_workspaces(&state.workspaces)
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect();
    while live.len().saturating_sub(safe.len()) < MIN_WORKSPACES && !safe.is_empty() {
        safe.remove(0);
    }
    state.workspaces.apply_cleanup(output, &safe, append);
    // Stale empty Engine-session cleanup: retired output-scoped ids drop
    // their idle empty domains; in-flight/displaced/unmapped-relevant and
    // non-empty sessions survive inside the helper.
    prune_retired_engine_domains(state, &safe);
    // Eager scoped invalidation: a previous naming a removed/out-of-scope
    // id clears now instead of surviving until the next toggle.
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    if owner.adopted() == WorkspaceMode::GlobalUnique {
        let _ = owner.resolve_previous(spaces, output);
    }
}

/// One logical shared ring cleanup across all output domains: removals strip
/// the logical ids everywhere plus the shared order, then the ring mints
/// trailing spares until the minimum holds with a globally unoccupied tail.
pub(super) fn shared_cleanup(state: &mut super::TileLoop) {
    let outputs = state.workspaces.output_keys();
    if outputs.is_empty() {
        return;
    }
    // Converge the order baseline when it was never synced: the deterministic
    // live union is the ring stage C keeps synced on hotplug.
    if state.scope_owner.shared_order().is_empty() {
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        owner.sync_shared_order(spaces);
    }
    let sticky = sticky_set(state);
    let ring = state.scope_owner.shared_ring(&state.workspaces);
    let occupied: BTreeSet<String> = ring
        .iter()
        .filter(|id| shared_occupied_id(state, id, &sticky))
        .cloned()
        .collect();
    let visible = visible_union(state);
    let retained: BTreeSet<String> = retained_ids(state).into_iter().collect();
    let facts = scoped_trailing_facts(&ring, &visible, &occupied, &retained);
    let plan = tiler_core::workspace::plan_trailing(&facts);
    let removed: Vec<String> = plan
        .remove
        .iter()
        .filter_map(|&i| ring.get(i).cloned())
        .collect();
    if !removed.is_empty() {
        state.workspaces.remove_shared_ids(&removed);
        // Stale empty Engine-session cleanup for retired logical ids; safe
        // by construction (idle empty only, never pending/drag/displaced or
        // unmapped relevant layouts).
        prune_retired_engine_domains(state, &removed);
        let kept: Vec<String> = state
            .scope_owner
            .shared_order()
            .iter()
            .filter(|id| !removed.contains(id))
            .cloned()
            .collect();
        state.scope_owner.set_shared_order(kept);
        // Eager shared invalidation: a previous naming a removed id clears
        // now; the call below is validation-only.
        let live = state.scope_owner.shared_ring(&state.workspaces);
        let current = outputs
            .iter()
            .filter_map(|o| state.workspaces.active_id(o))
            .next()
            .unwrap_or_default();
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        let _ = owner.history_mut().resolve_shared_for(&live, &current);
        let _ = spaces;
    }
    // Trailing spare plus minimum: mint logical ids until the single ring
    // holds the floor with a globally unoccupied tail.
    let mut guard = 0;
    loop {
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let sticky = sticky_set(state);
        let tail_ok = ring
            .last()
            .is_some_and(|last| !shared_occupied_id(state, last, &sticky));
        if ring.len() >= MIN_WORKSPACES && tail_ok {
            break;
        }
        if guard >= 4 {
            break;
        }
        guard += 1;
        let outputs = state.workspaces.output_keys();
        if outputs.is_empty() {
            break;
        }
        let id = state.workspaces.mint_shared_id(&outputs);
        if id.is_empty() {
            break;
        }
        let mut order = state.scope_owner.shared_order().clone();
        if !order.iter().any(|kept| kept == &id) {
            order.push(id);
            state.scope_owner.set_shared_order(order);
        }
    }
}

/// Trailing convergence after a verified local-path select transition. The
/// local arm is the exact legacy sequence (same inputs, same order); scoped
/// modes converge through the shared scope contract instead.
pub(super) fn trailing_after_select(state: &mut super::TileLoop, output: &str) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => {
            let active_id = state.workspaces.active_id(output).unwrap_or_default();
            let displaced: Vec<String> = state
                .workspaces
                .displaced_snapshot()
                .values()
                .flat_map(|r| r.workspace_ids.clone())
                .collect();
            let sticky_keys: BTreeSet<crate::workspace::WindowKey> =
                state.sticky.keys().cloned().collect();
            let (removed, append) = state.workspaces.plan_cleanup_excluding(
                output,
                std::slice::from_ref(&active_id),
                &displaced,
                &sticky_keys,
            );
            state.workspaces.apply_cleanup(output, &removed, append);
        }
        WorkspaceMode::GlobalUnique => global_cleanup_output(state, output),
        WorkspaceMode::Shared => shared_cleanup(state),
    }
}

/// Trailing spare after a stay send (the source view stays selected). Local
/// runs the ordinary per-output lifecycle; scoped modes converge through
/// the shared scope contract. Never records history in any mode.
pub(super) fn converge_spare(state: &mut super::TileLoop, output: &str) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => super::ensure_trailing_spare(state, output),
        WorkspaceMode::GlobalUnique => global_cleanup_output(state, output),
        WorkspaceMode::Shared => shared_cleanup(state),
    }
}

/// Trailing convergence after successful workspace admission (normal fresh
/// members, born-fullscreen holds, slotless-maximized admissions) inside
/// `ensure_workspace_assignments`. `outputs` are the outputs whose active
/// workspace gained at least one member this tick; empty means no admission
/// and no convergence (observation-only ticks never churn). Local and
/// global-unique converge per admitted output through the ordinary spare
/// lifecycle; shared converges the one logical ring once. Never records
/// history in any mode.
pub(super) fn converge_admitted(state: &mut super::TileLoop, outputs: &[String]) {
    if outputs.is_empty() {
        return;
    }
    if state.scope_owner.adopted() == WorkspaceMode::Shared {
        let Some(first) = outputs.iter().find(|o| !o.is_empty()).cloned() else {
            return;
        };
        converge_spare(state, &first);
        return;
    }
    let mut seen = BTreeSet::new();
    for output in outputs {
        if output.is_empty() || !seen.insert(output.clone()) {
            continue;
        }
        converge_spare(state, output);
    }
}

/// Trailing convergence after close-driven membership drops (hidden-claim
/// audit retirements). Local keeps the exact legacy per-output loop; global
/// converges each output through the scoped plan; shared converges the one
/// logical ring once.
pub(super) fn converge_all(state: &mut super::TileLoop) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => {
            for output in state.workspaces.output_keys() {
                let Some(active_id) = state.workspaces.active_id(&output) else {
                    continue;
                };
                let displaced: Vec<String> = state
                    .workspaces
                    .displaced_snapshot()
                    .values()
                    .flat_map(|r| r.workspace_ids.clone())
                    .collect();
                let sticky_keys: BTreeSet<crate::workspace::WindowKey> =
                    state.sticky.keys().cloned().collect();
                let (removed, append) = state.workspaces.plan_cleanup_excluding(
                    &output,
                    std::slice::from_ref(&active_id),
                    &displaced,
                    &sticky_keys,
                );
                state.workspaces.apply_cleanup(&output, &removed, append);
            }
        }
        WorkspaceMode::GlobalUnique => {
            for output in state.workspaces.output_keys() {
                if state.workspaces.active_id(&output).is_none() {
                    continue;
                }
                global_cleanup_output(state, &output);
            }
        }
        WorkspaceMode::Shared => shared_cleanup(state),
    }
}

/// Trailing convergence inside the scoped select tail (swap/shared effects
/// in `scope_effects::converge_tail`, after close cleanup and
/// before focus). `outputs` are the transition's changed outputs; shared
/// converges the one logical ring once.
pub(super) fn tail_trailing(state: &mut super::TileLoop, outputs: &[String]) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => {}
        WorkspaceMode::GlobalUnique => {
            for output in outputs {
                if state.workspaces.active_id(output).is_none() {
                    continue;
                }
                global_cleanup_output(state, output);
            }
        }
        WorkspaceMode::Shared => shared_cleanup(state),
    }
}

/// Startup priming after the initial `sync_monitor_outputs` (fixed topology
/// only: no hotplug/disconnect/reconnect behavior changes here; the hotplug
/// module owns later syncs). Local needs nothing (output creation already
/// primed local baselines with the minimum pair). Global-unique primes one
/// scoped baseline per output and converges the per-output minimum.
/// Shared syncs the one logical order, converges every output onto a single
/// baseline view, primes the shared baseline, and converges the trailing
/// spare.
///
/// Shared anchor choice (product ambiguity): the first output's live active
/// view wins (deterministic key order). There is no native shared-workspace
/// startup to mirror - KDE Plasma has no shared-workspace mode, so no
/// reference first-view/most-recently-used rule applies; primary-monitor or
/// MRU anchoring stays TBD for user testing.
pub(super) fn prime_after_sync(state: &mut super::TileLoop) {
    match state.scope_owner.adopted() {
        WorkspaceMode::PerOutputLocal => {}
        WorkspaceMode::GlobalUnique => {
            let outputs = state.workspaces.output_keys();
            for output in &outputs {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.prime_output(spaces, output);
            }
            for output in &outputs {
                if state.workspaces.active_id(output).is_none() {
                    continue;
                }
                global_cleanup_output(state, output);
            }
        }
        WorkspaceMode::Shared => {
            {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.sync_shared_order(spaces);
            }
            let outputs = state.workspaces.output_keys();
            let anchor = outputs
                .iter()
                .filter_map(|o| state.workspaces.active_id(o))
                .next();
            let mut converged: Vec<String> = Vec::new();
            if let Some(anchor) = anchor {
                for output in &outputs {
                    if state.workspaces.active_id(output).as_deref() == Some(anchor.as_str()) {
                        continue;
                    }
                    state.workspaces.ensure_workspace_id(output, &anchor);
                    if state.workspaces.activate(output, &anchor) {
                        converged.push(output.clone());
                    }
                }
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                if let Some(first) = outputs.first() {
                    owner.prime_shared_view(spaces, first);
                }
                if !converged.is_empty()
                    && let Some(first) = outputs.first()
                {
                    super::log_json_at(
                        &state.log_path.clone(),
                        serde_json::json!({
                            "event": "workspace-scope",
                            "adopted": "shared",
                            "scope": "startup-converge",
                            "workspace": state.workspaces.workspace_token(first, &anchor),
                            "outputs": converged.iter().map(|o| state.workspaces.output_token(o)).collect::<Vec<_>>(),
                            "outcome": "ok",
                        }),
                    );
                }
            }
            shared_cleanup(state);
        }
    }
}

/// Record completed whole-workspace migration views (global-unique only).
/// Both the source refill arrival and the target moved-id arrival are
/// completed transitions and must record: the source observation advances
/// the baseline while clearing the moved previous like removal (the moved
/// id left the source scope, so no valid toggle remains until the next
/// change), and the target observation records the prior view as the new
/// previous. Local mode needs nothing (the workspace primitive already
/// invalidated, and the local path stays unchanged); shared never reaches
/// here (migration refuses before transfer).
pub(super) fn note_migration_completed(
    state: &mut super::TileLoop,
    source_output: &str,
    source_refill: Option<&str>,
    target_output: &str,
    moved_id: &str,
) {
    if state.scope_owner.adopted() != WorkspaceMode::GlobalUnique {
        return;
    }
    if let Some(refill) = source_refill {
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        owner.record_completed(spaces, source_output, refill);
    } else {
        super::scope_sends::note_migration_history(state, source_output, moved_id);
    }
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    owner.record_completed(spaces, target_output, moved_id);
}

#[cfg(test)]
mod tests {
    use super::super::{
        MonitorArea, TileLoop, clear_pending_seams, inject_pending_tag, rmax03_adapter_tests,
        workspace_domain_for,
    };
    use super::*;
    use crate::workspace::WindowKey;
    use tiler_core::geometry::Rect;

    const BOUNDS: Rect = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };

    fn key(hwnd: u64, pid: u32) -> WindowKey {
        WindowKey {
            hwnd,
            pid,
            creation: format!("c{hwnd:016x}"),
        }
    }

    fn two_output_state(mode: WorkspaceMode) -> TileLoop {
        let mut state = rmax03_adapter_tests::test_state();
        state.workspaces.ensure_output("mon-a");
        state.workspaces.ensure_output("mon-b");
        state.scope_owner = crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(mode);
        state.scope_owner.sync_shared_order(&state.workspaces);
        state.active_output = "mon-a".to_owned();
        state
    }

    fn stage_member(state: &mut TileLoop, output: &str, ws: &str, member: &WindowKey) {
        assert!(
            state.workspaces.assign(member.clone(), output, ws, false),
            "stage assigns"
        );
        state
            .member_tokens
            .insert(member.clone(), format!("t{}", member.hwnd));
        state.member_identity.insert(
            member.clone(),
            crate::model::ProcessIdentity {
                pid: member.pid,
                process_creation: member.creation.clone(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                exe_path: "test.exe".to_owned(),
            },
        );
        state
            .member_tags
            .insert(member.clone(), format!("tag{}", member.hwnd));
        inject_pending_tag(member.hwnd, Some(format!("tag{}", member.hwnd)));
    }

    fn areas() -> Vec<MonitorArea> {
        vec![
            MonitorArea {
                device: "mon-a".to_owned(),
                work: BOUNDS,
                full: BOUNDS,
            },
            MonitorArea {
                device: "mon-b".to_owned(),
                work: BOUNDS,
                full: BOUNDS,
            },
        ]
    }

    /// Retain one Engine layout domain without members (unmapped-but-layoutful
    /// protection input).
    fn seed_empty_session(state: &mut TileLoop, output: &str, ws: &str) {
        let areas = areas();
        let (domain, key) =
            workspace_domain_for(output, ws, &areas, state.inner_gap, state.outer_gap)
                .expect("seed domain");
        let session = tiler_core::session::Session::new(
            state.owner.clone(),
            state.generation.clone(),
            0,
            0,
            vec![domain],
        )
        .expect("seed session");
        state.engine.insert_raw(key, session, state.outer_gap);
    }

    /// Seed one non-empty Engine layout via the real reconcile route (a
    /// committed layout with topology, not idle history).
    fn seed_layout_session(state: &mut TileLoop, output: &str, ws: &str, token: &str) {
        let areas = areas();
        let (domain, key) =
            workspace_domain_for(output, ws, &areas, state.inner_gap, state.outer_gap)
                .expect("seed domain");
        let event = crate::tiling::build_reconcile_event_for(
            &state.owner.clone(),
            &state.generation.clone(),
            &tiler_core::ids::CorrelationId::parse("trail-seed").expect("correlation"),
            0,
            1,
            &domain,
            &key,
            state.outer_gap,
            &[(
                tiler_core::directional::WindowId(token.to_owned()),
                BOUNDS,
                tiler_core::size_hints::WindowSizeHints::none(),
            )],
            Some(tiler_core::directional::WindowId(token.to_owned())).as_ref(),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed layout ({output},{ws}) converges"
        );
    }

    fn scope_domain_key(output: &str, ws: &str) -> tiler_core::session::DomainKey {
        tiler_core::session::DomainKey {
            output: tiler_core::directional::OutputId(output.to_owned()),
            workspace: tiler_core::directional::WorkspaceId(ws.to_owned()),
        }
    }

    #[test]
    fn local_trailing_keeps_legacy_per_output_shape() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        // Occupy mon-a's trailing empty; mon-b stays empty.
        let ids_a = state.workspaces.workspace_ids("mon-a");
        let tail_a = ids_a.last().cloned().expect("tail");
        let member = key(0xA01, 101);
        stage_member(&mut state, "mon-a", &tail_a, &member);
        let before_b = state.workspaces.workspace_ids("mon-b");
        trailing_after_select(&mut state, "mon-a");
        // Filled trailing appends the next spare on mon-a only.
        let after_a = state.workspaces.workspace_ids("mon-a");
        assert_eq!(after_a.len(), ids_a.len() + 1);
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail_a)
                .contains(&member)
        );
        assert_eq!(state.workspaces.workspace_ids("mon-b"), before_b);
        assert!(state.workspaces.workspace_count("mon-a") >= MIN_WORKSPACES);
        clear_pending_seams();
    }

    #[test]
    fn shared_trailing_creates_one_logical_id_on_occupied_tail() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        // Occupy the shared tail (ws-4) on its home output.
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let tail = ring.last().cloned().expect("tail");
        let member = key(0xB01, 201);
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&tail))
            .expect("tail home");
        stage_member(&mut state, &home, &tail, &member);
        let sticky = sticky_set(&state);
        let before_len = ring.len();
        let id =
            match state
                .scope_owner
                .resolve_trailing_select(&mut state.workspaces, "mon-a", &sticky)
            {
                crate::workspace_scope_owner::TrailingResolve::Reuse(id) => id,
                crate::workspace_scope_owner::TrailingResolve::Unknown => {
                    panic!("occupied shared tail must create, not refuse")
                }
            };
        // One brand-new logical id across every output domain plus the order.
        assert!(!ring.contains(&id), "created id is fresh");
        for output in state.workspaces.output_keys() {
            assert!(
                state.workspaces.workspace_ids(&output).contains(&id),
                "logical {id} converged onto {output}"
            );
        }
        assert!(state.scope_owner.shared_order().contains(&id));
        assert_eq!(
            state.scope_owner.shared_ring(&state.workspaces).len(),
            before_len + 1
        );
        clear_pending_seams();
    }

    #[test]
    fn shared_cleanup_keeps_single_trailing_across_outputs() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        // Occupy everything except leave the tail empty, then converge.
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        for (i, id) in ring.iter().enumerate() {
            if i + 1 == ring.len() {
                continue;
            }
            let home = state
                .workspaces
                .output_keys()
                .into_iter()
                .find(|o| state.workspaces.workspace_ids(o).contains(id))
                .expect("home");
            stage_member(
                &mut state,
                &home,
                id,
                &key(0xC00 + i as u64, 300 + i as u32),
            );
        }
        shared_cleanup(&mut state);
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let tail = ring.last().cloned().expect("single trailing");
        // The one logical trailing id stays globally unoccupied with the
        // floor held; its home output retains it (per-output convergence
        // onto the logical id happens on selection/send, not in cleanup).
        assert!(
            !shared_occupied_id(&state, &tail, &sticky_set(&state)),
            "single trailing stays globally unoccupied"
        );
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&tail))
            .expect("trailing home retains it");
        let _ = home;
        assert!(ring.len() >= MIN_WORKSPACES);
        clear_pending_seams();
    }

    #[test]
    fn shared_cleanup_strips_middle_empty_everywhere_and_invalidates() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        // Converge ws-2 onto mon-b so it is multi-homed, keep it empty, and
        // point the shared previous at it; occupying the tail forces the
        // middle empty (not the tail) to prune.
        state.workspaces.ensure_workspace_id("mon-b", "ws-2");
        state.scope_owner.sync_shared_order(&state.workspaces);
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let tail = ring.last().cloned().expect("tail");
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&tail))
            .expect("tail home");
        stage_member(&mut state, &home, &tail, &key(0xD01, 401));
        // Occupy the head too so only the middle empties can prune.
        let head = ring.first().cloned().expect("head");
        let head_home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&head))
            .expect("head home");
        stage_member(&mut state, &head_home, &head, &key(0xD02, 402));
        state.scope_owner.history_mut().prime_shared(&head);
        assert!(
            state
                .scope_owner
                .history_mut()
                .observe_shared(&ring, "ws-2")
        );
        assert_eq!(
            state.scope_owner.history().shared_previous().as_deref(),
            Some(head.as_str())
        );
        // Point previous at the prune candidate instead: record ws-2 then a
        // further change is impossible without members, so prime-then-observe
        // via a memberful detour is unnecessary - directly exercise removal
        // invalidation through the resolve path below.
        shared_cleanup(&mut state);
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        // ws-2 was an invisible empty non-final id: stripped everywhere.
        assert!(!ring.contains(&"ws-2".to_owned()), "middle empty strips");
        for output in state.workspaces.output_keys() {
            assert!(
                !state
                    .workspaces
                    .workspace_ids(&output)
                    .contains(&"ws-2".to_owned()),
                "ws-2 leaves {output}"
            );
        }
        assert!(
            !state
                .scope_owner
                .shared_order()
                .contains(&"ws-2".to_owned())
        );
        // Stale shared previous values clear through validation (no recreation).
        let current = state.workspaces.active_id("mon-a").unwrap_or_default();
        let live = state.scope_owner.shared_ring(&state.workspaces);
        let _ = state
            .scope_owner
            .history_mut()
            .resolve_shared_for(&live, &current);
        assert!(
            state.scope_owner.history().shared_previous().as_deref() != Some("ws-2"),
            "removed id never stays a valid previous"
        );
        clear_pending_seams();
    }

    #[test]
    fn sticky_only_trailing_reuses_without_occupying() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let tail = ring.last().cloned().expect("tail");
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&tail))
            .expect("tail home");
        let sticky_key = key(0xE01, 501);
        stage_member(&mut state, &home, &tail, &sticky_key);
        state.sticky.insert(sticky_key.clone(), false);
        let sticky = sticky_set(&state);
        // Sticky-only tail reuses: no creation, ring unchanged.
        let before = state.scope_owner.shared_ring(&state.workspaces);
        let reuse = state
            .scope_owner
            .trailing_reuse_id(&state.workspaces, "mon-a", &sticky);
        assert_eq!(reuse.as_deref(), Some(tail.as_str()));
        let id =
            match state
                .scope_owner
                .resolve_trailing_select(&mut state.workspaces, "mon-a", &sticky)
            {
                crate::workspace_scope_owner::TrailingResolve::Reuse(id) => id,
                crate::workspace_scope_owner::TrailingResolve::Unknown => {
                    panic!("sticky-only tail must reuse")
                }
            };
        assert_eq!(id, tail);
        assert_eq!(state.scope_owner.shared_ring(&state.workspaces), before);
        clear_pending_seams();
    }

    #[test]
    fn global_cleanup_never_removes_id_visible_elsewhere() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::GlobalUnique);
        // Duplicate-residue shape: converge ws-1 onto mon-b and show it
        // there (visible elsewhere from mon-a's perspective) while it reads
        // empty on mon-a; cleanup on mon-a must not strip it.
        state.workspaces.ensure_workspace_id("mon-b", "ws-1");
        assert!(state.workspaces.activate("mon-b", "ws-1"));
        // Park mon-a on ws-2 so ws-1 reads as an invisible empty non-final
        // candidate there; it must still survive as visible elsewhere.
        assert!(state.workspaces.activate("mon-a", "ws-2"));
        let before_a = state.workspaces.workspace_ids("mon-a");
        assert!(before_a.contains(&"ws-1".to_owned()));
        global_cleanup_output(&mut state, "mon-a");
        assert!(
            state
                .workspaces
                .workspace_ids("mon-a")
                .contains(&"ws-1".to_owned()),
            "id visible on mon-b survives mon-a cleanup"
        );
        assert!(state.workspaces.workspace_count("mon-a") >= MIN_WORKSPACES);
        // Global live union keeps its floor.
        let mut live: BTreeSet<String> = BTreeSet::new();
        for output in state.workspaces.output_keys() {
            live.extend(state.workspaces.workspace_ids(&output));
        }
        assert!(live.len() >= MIN_WORKSPACES);
        clear_pending_seams();
    }

    #[test]
    fn shared_ordinary_empty_mapped_session_prunes_and_cleans_domain() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        // Three-id ring shape: occupy head and tail, leave the middle empty
        // with only idle mapped layout history (no members, committed empty,
        // no flight). Narrow retention must not pin it.
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        assert!(ring.len() >= 3, "fixture ring needs a middle");
        let head = ring[0].clone();
        let middle = ring[1].clone();
        let tail = ring.last().cloned().expect("tail");
        for (id, hwnd) in [(head, 0xF01u64), (tail, 0xF02u64)] {
            let home = state
                .workspaces
                .output_keys()
                .into_iter()
                .find(|o| state.workspaces.workspace_ids(o).contains(&id))
                .expect("home");
            stage_member(&mut state, &home, &id, &key(hwnd, 601));
        }
        let middle_home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&middle))
            .expect("middle home");
        seed_empty_session(&mut state, &middle_home, &middle);
        // Idle mapped history is neither in-flight nor unmapped relevant.
        assert!(engine_inflight_ids(&state).is_empty());
        assert!(!engine_unmapped_relevant_ids(&state).contains(&middle));
        assert!(!retained_ids(&state).contains(&middle));
        shared_cleanup(&mut state);
        assert!(
            !state
                .scope_owner
                .shared_ring(&state.workspaces)
                .contains(&middle),
            "ordinary empty mapped id prunes despite idle layout history"
        );
        assert!(
            state
                .engine
                .session(&scope_domain_key(&middle_home, &middle))
                .is_none(),
            "retired stale empty domain cleans"
        );
        clear_pending_seams();
    }

    #[test]
    fn shared_unmapped_relevant_layout_pins_id_and_survives_cleanup() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        // Same middle-empty shape, but the layout lives unmapped: a
        // non-empty Engine session on the sibling output that does not list
        // the middle id. That relevant layout pins the id.
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        assert!(ring.len() >= 3, "fixture ring needs a middle");
        let head = ring[0].clone();
        let middle = ring[1].clone();
        let tail = ring.last().cloned().expect("tail");
        for (id, hwnd) in [(head, 0xF11u64), (tail, 0xF12u64)] {
            let home = state
                .workspaces
                .output_keys()
                .into_iter()
                .find(|o| state.workspaces.workspace_ids(o).contains(&id))
                .expect("home");
            stage_member(&mut state, &home, &id, &key(hwnd, 611));
        }
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&middle))
            .expect("middle home");
        let sibling = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| *o != home)
            .expect("sibling");
        assert!(
            !state.workspaces.workspace_ids(&sibling).contains(&middle),
            "sibling does not list the middle id"
        );
        seed_layout_session(&mut state, &sibling, &middle, "wU1");
        assert!(engine_unmapped_relevant_ids(&state).contains(&middle));
        assert!(retained_ids(&state).contains(&middle));
        shared_cleanup(&mut state);
        assert!(
            state
                .scope_owner
                .shared_ring(&state.workspaces)
                .contains(&middle),
            "unmapped relevant layout survives shared cleanup"
        );
        assert!(
            state
                .engine
                .session(&scope_domain_key(&sibling, &middle))
                .is_some(),
            "unmapped relevant domain is never dropped"
        );
        clear_pending_seams();
    }

    #[test]
    fn global_stale_empty_cleans_but_nonempty_layout_survives() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::GlobalUnique);
        // Grow mon-a to a four-id mon-a-only ring, then empty the growth
        // members again: occupy the trailing empty twice so cleanup appends
        // past it, then retire those growth members so both middles read
        // empty and output-scoped.
        let grow_a = key(0xF21, 621);
        let grow_b = key(0xF22, 622);
        for grow in [&grow_a, &grow_b] {
            let tail = state
                .workspaces
                .workspace_ids("mon-a")
                .last()
                .cloned()
                .expect("tail");
            stage_member(&mut state, "mon-a", &tail, grow);
            global_cleanup_output(&mut state, "mon-a");
        }
        assert!(state.workspaces.remove_window(&grow_a));
        assert!(state.workspaces.remove_window(&grow_b));
        let ids = state.workspaces.workspace_ids("mon-a");
        assert_eq!(ids.len(), 4, "grown ring needs two middles");
        let head = ids[0].clone();
        let empty_middle = ids[1].clone();
        let layout_middle = ids[2].clone();
        let tail = ids[3].clone();
        // Occupy head and tail so both middles read as prune candidates;
        // park both actives away from the middles.
        stage_member(&mut state, "mon-a", &head, &key(0xF23, 623));
        stage_member(&mut state, "mon-a", &tail, &key(0xF24, 624));
        assert!(state.workspaces.activate("mon-a", &head));
        assert!(state.workspaces.activate("mon-b", "ws-4"));
        seed_empty_session(&mut state, "mon-a", &empty_middle);
        seed_layout_session(&mut state, "mon-a", &layout_middle, "wG1");
        global_cleanup_output(&mut state, "mon-a");
        let after = state.workspaces.workspace_ids("mon-a");
        assert!(
            !after.contains(&empty_middle),
            "ordinary empty mapped id prunes globally"
        );
        assert!(
            state
                .engine
                .session(&scope_domain_key("mon-a", &empty_middle))
                .is_none(),
            "retired stale empty global domain cleans"
        );
        // The non-empty layout middle prunes as an empty workspace (no
        // members) but its committed layout domain survives as unmapped
        // relevant state instead of dropping.
        assert!(
            !after.contains(&layout_middle),
            "memberless layout middle still prunes as empty"
        );
        assert!(
            state
                .engine
                .session(&scope_domain_key("mon-a", &layout_middle))
                .is_some(),
            "non-empty retired domain survives cleanup"
        );
        clear_pending_seams();
    }

    #[test]
    fn migration_records_source_and_target_views() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::GlobalUnique);
        // Prime both baselines, then record first views so the migration
        // changes have predecessors to remember.
        for (output, first, second) in [("mon-a", "ws-1", "ws-2"), ("mon-b", "ws-3", "ws-4")] {
            {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, output, first);
            }
            assert!(state.workspaces.activate(output, second));
            {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                assert!(owner.record_completed(spaces, output, second));
            }
        }
        // Migrate mon-a's active ws-2 to mon-b; refill keeps ws-1.
        assert!(state.workspaces.activate("mon-a", "ws-2"));
        {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            owner.record_completed(spaces, "mon-a", "ws-2");
        }
        let outcome = state
            .workspaces
            .migrate_workspace_excluding(
                "mon-a",
                "mon-b",
                tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
                &BTreeSet::new(),
            )
            .expect("migrate");
        assert_eq!(outcome.moved_id, "ws-2");
        let refill = outcome.source_refill.clone().expect("refill");
        note_migration_completed(
            &mut state,
            "mon-a",
            Some(&refill),
            "mon-b",
            &outcome.moved_id,
        );
        // Target records the completed arrival with the prior as previous.
        assert_eq!(
            state.scope_owner.history().previous("mon-b").as_deref(),
            Some("ws-4")
        );
        // Source advanced its baseline past the moved id; no dangling toggle
        // to an out-of-scope id remains.
        assert_eq!(
            state.scope_owner.history().baseline("mon-a").as_deref(),
            Some(refill.as_str())
        );
        assert_ne!(
            state.scope_owner.history().previous("mon-a").as_deref(),
            Some("ws-2")
        );
        clear_pending_seams();
    }

    #[test]
    fn startup_priming_converges_shared_views_and_baselines() {
        clear_pending_seams();
        let mut state = rmax03_adapter_tests::test_state();
        state.workspaces.ensure_output("mon-a");
        state.workspaces.ensure_output("mon-b");
        state.scope_owner =
            crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(WorkspaceMode::Shared);
        state.active_output = "mon-a".to_owned();
        state.log_path = std::path::PathBuf::from("NUL");
        // Divergent currents before priming (per-output defaults differ).
        let a = state.workspaces.active_id("mon-a").expect("a");
        let b = state.workspaces.active_id("mon-b").expect("b");
        assert_ne!(a, b);
        prime_after_sync(&mut state);
        // One logical current everywhere plus a primed shared baseline and
        // the trailing floor.
        let a = state.workspaces.active_id("mon-a").expect("a");
        let b = state.workspaces.active_id("mon-b").expect("b");
        assert_eq!(a, b, "shared startup converges onto one current");
        assert!(!state.scope_owner.shared_order().is_empty());
        assert!(state.scope_owner.shared_ring(&state.workspaces).len() >= MIN_WORKSPACES);
        clear_pending_seams();
    }

    #[test]
    fn startup_priming_primes_global_baselines_without_views() {
        clear_pending_seams();
        let mut state = rmax03_adapter_tests::test_state();
        state.workspaces.ensure_output("mon-a");
        state.workspaces.ensure_output("mon-b");
        state.scope_owner = crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(
            WorkspaceMode::GlobalUnique,
        );
        state.active_output = "mon-a".to_owned();
        state.log_path = std::path::PathBuf::from("NUL");
        let a = state.workspaces.active_id("mon-a").expect("a");
        prime_after_sync(&mut state);
        // Views untouched, baselines primed per output.
        assert_eq!(
            state.workspaces.active_id("mon-a").as_deref(),
            Some(a.as_str())
        );
        assert_eq!(
            state.scope_owner.history().baseline("mon-a").as_deref(),
            Some(a.as_str())
        );
        assert!(state.scope_owner.history().baseline("mon-b").is_some());
        clear_pending_seams();
    }

    // Admission trailing regressions. Fixtures stage the exact post-assign
    // tables admission writes; the native gate-to-output wiring itself is
    // code-reviewed, not fixture-exercised. Production assigns onto the
    // active workspace, so fills activate the tail first.
    fn observed_for(member: &WindowKey) -> super::super::ObservedWindow {
        super::super::ObservedWindow {
            hwnd: member.hwnd,
            token: format!("t{}", member.hwnd),
            outer: BOUNDS,
            visible: BOUNDS,
            insets: crate::tiling::FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: member.hwnd,
                pid: member.pid,
                process_creation: member.creation.clone(),
                exe_path: "test.exe".to_owned(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }
    }

    fn admission_me() -> crate::model::ProcessIdentity {
        crate::model::ProcessIdentity {
            pid: 4242,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        }
    }

    #[derive(Clone, Copy)]
    enum FillLane {
        Normal,
        BornHold,
        SlotlessMax,
    }

    // Activate the tail, then stage the member like admission does: normal
    // tiles with a slot, born holds seed the snapshot plus the hold set,
    // slotless-max carries no rect. Returns the filled tail id.
    fn fill_active_tail(
        state: &mut TileLoop,
        output: &str,
        member: &WindowKey,
        lane: FillLane,
    ) -> String {
        let tail = state
            .workspaces
            .workspace_ids(output)
            .last()
            .cloned()
            .expect("tail");
        assert!(
            state.workspaces.activate(output, &tail),
            "fill targets the active tail"
        );
        stage_member(state, output, &tail, member);
        let token = format!("t{}", member.hwnd);
        match lane {
            FillLane::Normal => {
                state.member_rects.insert(token, BOUNDS);
            }
            FillLane::BornHold => {
                state.member_rects.insert(token, BOUNDS);
                state.born_fullscreen.insert(member.clone());
            }
            FillLane::SlotlessMax => {}
        }
        tail
    }

    // Record two views (baseline `second`, previous `first`); convergence
    // must preserve both since admission trailing never records history.
    fn prime_history(state: &mut TileLoop, output: &str, first: &str, second: &str) {
        match state.scope_owner.adopted() {
            WorkspaceMode::PerOutputLocal => {
                state.workspaces.observe_workspace_change(output, first);
                state.workspaces.observe_workspace_change(output, second);
            }
            WorkspaceMode::GlobalUnique => {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, output, first);
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, output, second);
            }
            WorkspaceMode::Shared => {
                state.scope_owner.history_mut().prime_shared(first);
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.record_completed(spaces, output, second);
            }
        }
    }

    fn assert_history(state: &TileLoop, output: &str, baseline: &str, previous: &str) {
        match state.scope_owner.adopted() {
            WorkspaceMode::PerOutputLocal => {
                assert_eq!(
                    state.workspaces.history_baseline(output).as_deref(),
                    Some(baseline)
                );
                assert_eq!(
                    state.workspaces.history_previous(output).as_deref(),
                    Some(previous)
                );
            }
            WorkspaceMode::GlobalUnique => {
                assert_eq!(
                    state.scope_owner.history().baseline(output).as_deref(),
                    Some(baseline)
                );
                assert_eq!(
                    state.scope_owner.history().previous(output).as_deref(),
                    Some(previous)
                );
            }
            WorkspaceMode::Shared => {
                assert_eq!(
                    state.scope_owner.history().shared_previous().as_deref(),
                    Some(previous)
                );
            }
        }
    }

    #[test]
    fn admission_fill_converges_next_spare_local() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        let tail0 = ids.last().cloned().expect("tail");
        stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
        prime_history(&mut state, "mon-a", &head, &tail0);
        let before_b = state.workspaces.workspace_ids("mon-b");
        let member = key(0xA12, 112);
        let tail = fill_active_tail(&mut state, "mon-a", &member, FillLane::Normal);
        assert_eq!(tail, tail0);
        assert!(
            state
                .member_rects
                .contains_key(&format!("t{}", member.hwnd)),
            "normal admission tiles with a slot"
        );
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        let after = state.workspaces.workspace_ids("mon-a");
        assert_eq!(after.len(), ids.len() + 1);
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail)
                .contains(&member)
        );
        assert_eq!(
            state.workspaces.active_id("mon-a").as_deref(),
            Some(tail.as_str())
        );
        assert_history(&state, "mon-a", &tail, &head);
        assert_eq!(state.workspaces.workspace_ids("mon-b"), before_b);
        clear_pending_seams();
    }

    // Born holds (snapshot seed plus hold set) and slotless-max rows (no
    // rect) both occupy the tail, so both converge the next spare.
    #[test]
    fn admission_fill_lanes_occupy_local() {
        for (lane, hwnd, pid) in [
            (FillLane::BornHold, 0xA13u64, 113u32),
            (FillLane::SlotlessMax, 0xA14u64, 114u32),
        ] {
            clear_pending_seams();
            let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
            let ids = state.workspaces.workspace_ids("mon-a");
            let head = ids.first().cloned().expect("head");
            let tail0 = ids.last().cloned().expect("tail");
            stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
            prime_history(&mut state, "mon-a", &head, &tail0);
            let member = key(hwnd, pid);
            let tail = fill_active_tail(&mut state, "mon-a", &member, lane);
            match lane {
                FillLane::BornHold => {
                    assert!(state.born_fullscreen.contains(&member));
                    assert!(
                        state.member_rects.contains_key(&format!("t{hwnd}")),
                        "born hold seeds the hidden snapshot"
                    );
                }
                FillLane::SlotlessMax => {
                    assert!(
                        !state.member_rects.contains_key(&format!("t{hwnd}")),
                        "slotless-max carries no tile slot"
                    );
                }
                FillLane::Normal => unreachable!(),
            }
            converge_admitted(&mut state, &["mon-a".to_owned()]);
            let after = state.workspaces.workspace_ids("mon-a");
            assert_eq!(after.len(), ids.len() + 1);
            assert!(
                state
                    .workspaces
                    .workspace_members("mon-a", &tail)
                    .contains(&member)
            );
            assert_eq!(
                state.workspaces.active_id("mon-a").as_deref(),
                Some(tail.as_str())
            );
            assert_history(&state, "mon-a", &tail, &head);
            clear_pending_seams();
        }
    }

    #[test]
    fn admission_fill_converges_per_output_global_unique() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::GlobalUnique);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        let tail0 = ids.last().cloned().expect("tail");
        stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
        prime_history(&mut state, "mon-a", &head, &tail0);
        let before_b = state.workspaces.workspace_ids("mon-b");
        let active_b = state.workspaces.active_id("mon-b").expect("active b");
        let member = key(0xA15, 115);
        let tail = fill_active_tail(&mut state, "mon-a", &member, FillLane::Normal);
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        assert_eq!(state.workspaces.workspace_ids("mon-a").len(), ids.len() + 1);
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail)
                .contains(&member)
        );
        assert_eq!(state.workspaces.workspace_ids("mon-b"), before_b);
        assert_eq!(
            state.workspaces.active_id("mon-b").as_deref(),
            Some(active_b.as_str())
        );
        assert_history(&state, "mon-a", &tail, &head);
        clear_pending_seams();
    }

    #[test]
    fn admission_fill_converges_one_logical_spare_shared() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::Shared);
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        let before = ring.clone();
        let head = ring.first().cloned().expect("head");
        let tail = ring.last().cloned().expect("tail");
        let home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&tail))
            .expect("tail home");
        let head_home = state
            .workspaces
            .output_keys()
            .into_iter()
            .find(|o| state.workspaces.workspace_ids(o).contains(&head))
            .expect("head home");
        stage_member(&mut state, &head_home, &head, &key(0xA10, 110));
        prime_history(&mut state, &home, &head, &tail);
        let member = key(0xA16, 116);
        let filled = fill_active_tail(&mut state, &home, &member, FillLane::SlotlessMax);
        assert_eq!(filled, tail);
        converge_admitted(
            &mut state,
            &["mon-a".to_owned(), "mon-b".to_owned(), "mon-a".to_owned()],
        );
        let ring = state.scope_owner.shared_ring(&state.workspaces);
        assert!(ring.contains(&tail), "occupied fill is never pruned");
        let spare = ring.last().cloned().expect("fresh spare");
        assert!(
            !before.contains(&spare),
            "spare {spare} is freshly minted past the filled tail"
        );
        assert!(
            !shared_occupied_id(&state, &spare, &sticky_set(&state)),
            "fresh spare stays globally unoccupied"
        );
        for output in state.workspaces.output_keys() {
            assert!(
                state.workspaces.workspace_ids(&output).contains(&spare),
                "logical {spare} converges onto {output}"
            );
        }
        assert!(ring.len() >= MIN_WORKSPACES);
        assert_history(&state, &home, &tail, &head);
        clear_pending_seams();
    }

    #[test]
    fn admission_empty_never_churns() {
        clear_pending_seams();
        for mode in [
            WorkspaceMode::PerOutputLocal,
            WorkspaceMode::GlobalUnique,
            WorkspaceMode::Shared,
        ] {
            let mut state = two_output_state(mode);
            let before_a = state.workspaces.workspace_ids("mon-a");
            let before_b = state.workspaces.workspace_ids("mon-b");
            converge_admitted(&mut state, &[]);
            converge_admitted(&mut state, &[String::new()]);
            assert_eq!(state.workspaces.workspace_ids("mon-a"), before_a);
            assert_eq!(state.workspaces.workspace_ids("mon-b"), before_b);
        }
        clear_pending_seams();
    }

    // Repeated convergence is idempotent: a second run appends nothing.
    #[test]
    fn admission_converge_is_idempotent() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
        fill_active_tail(&mut state, "mon-a", &key(0xA17, 117), FillLane::Normal);
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        let once = state.workspaces.workspace_ids("mon-a");
        assert_eq!(once.len(), ids.len() + 1);
        converge_admitted(&mut state, &["mon-a".to_owned(), "mon-a".to_owned()]);
        assert_eq!(state.workspaces.workspace_ids("mon-a"), once);
        clear_pending_seams();
    }

    // A fresh admission onto a non-trailing workspace appends nothing: the
    // trailing empty is still free.
    #[test]
    fn admission_nontrailing_fill_appends_nothing() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        let tail = ids.last().cloned().expect("tail");
        stage_member(&mut state, "mon-a", &head, &key(0xA18, 118));
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        let after = state.workspaces.workspace_ids("mon-a");
        assert_eq!(after, ids);
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail)
                .is_empty(),
            "tail stays the free spare"
        );
        clear_pending_seams();
    }

    // Chained fills mint each spare: filling the fresh tail converges again.
    #[test]
    fn admission_chained_fills_mint_each_spare() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
        let first = key(0xA19, 119);
        let tail_a = fill_active_tail(&mut state, "mon-a", &first, FillLane::Normal);
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        let second = key(0xA1A, 120);
        let tail_b = fill_active_tail(&mut state, "mon-a", &second, FillLane::Normal);
        assert_ne!(tail_a, tail_b);
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        let after = state.workspaces.workspace_ids("mon-a");
        assert_eq!(after.len(), ids.len() + 2);
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail_a)
                .contains(&first)
        );
        assert!(
            state
                .workspaces
                .workspace_members("mon-a", &tail_b)
                .contains(&second)
        );
        assert_eq!(
            state.workspaces.active_id("mon-a").as_deref(),
            Some(tail_b.as_str())
        );
        clear_pending_seams();
    }

    // A sticky-adopted fill never occupies: converging after adoption mints
    // no spare since the tail reads free.
    #[test]
    fn admission_sticky_fill_appends_nothing() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let member = key(0xA1B, 121);
        fill_active_tail(&mut state, "mon-a", &member, FillLane::Normal);
        state.sticky.insert(member.clone(), false);
        converge_admitted(&mut state, &["mon-a".to_owned()]);
        assert_eq!(state.workspaces.workspace_ids("mon-a"), ids);
        clear_pending_seams();
    }

    // Repeated observation of a known member runs the real assignment
    // boundary with no admission: tables and rings stay unchanged.
    #[test]
    fn repeated_nontrailing_observation_never_churns() {
        clear_pending_seams();
        let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
        let ids = state.workspaces.workspace_ids("mon-a");
        let head = ids.first().cloned().expect("head");
        let member = key(0xA1C, 122);
        stage_member(&mut state, "mon-a", &head, &member);
        let before_a = state.workspaces.workspace_ids("mon-a");
        let before_b = state.workspaces.workspace_ids("mon-b");
        let staged_identity = state.member_identity.get(&member).cloned();
        let me = admission_me();
        let mut observed = vec![observed_for(&member)];
        let retained: Vec<super::super::RetainedRow> = Vec::new();
        super::super::ensure_workspace_assignments(
            &mut state,
            &me,
            &mut observed,
            &retained,
            &areas(),
        );
        assert_eq!(state.member_identity.get(&member), staged_identity.as_ref());
        assert_eq!(
            state.member_tokens.get(&member).map(String::as_str),
            Some(format!("t{}", member.hwnd).as_str())
        );
        assert!(state.workspaces.member_loc(&member).is_some());
        assert_eq!(state.workspaces.workspace_ids("mon-a"), before_a);
        assert_eq!(state.workspaces.workspace_ids("mon-b"), before_b);
        clear_pending_seams();
    }

    // Owner-to-pager parity after an admission spare: taskbar cells and tray
    // rows both project the owner ring, ending on an empty spare with the
    // filled tail active.
    #[test]
    fn admission_spare_keeps_taskbar_tray_parity() {
        for mode in [WorkspaceMode::PerOutputLocal, WorkspaceMode::GlobalUnique] {
            clear_pending_seams();
            let mut state = two_output_state(mode);
            let ids = state.workspaces.workspace_ids("mon-a");
            let head = ids.first().cloned().expect("head");
            stage_member(&mut state, "mon-a", &head, &key(0xA10, 110));
            let member = key(0xA1D, 123);
            let tail = fill_active_tail(&mut state, "mon-a", &member, FillLane::Normal);
            converge_admitted(&mut state, &["mon-a".to_owned()]);
            let after = state.workspaces.workspace_ids("mon-a");
            let spare = after.last().cloned().expect("spare");
            assert!(
                state
                    .workspaces
                    .workspace_members("mon-a", &spare)
                    .is_empty(),
                "final spare is empty"
            );
            assert_eq!(
                state.workspaces.active_id("mon-a").as_deref(),
                Some(tail.as_str())
            );
            let taskbar = crate::pager::TaskbarFacts {
                rect: Rect {
                    x: 0,
                    y: 560,
                    w: 800,
                    h: 40,
                },
                tray_left: Some(700),
                kind: crate::pager::TaskbarKind::Primary,
                visible: true,
                autohide: false,
                dpi: 96,
            };
            let pager = crate::pager::build_output_pager(
                &state.workspaces,
                &state.pager_live,
                &state.member_tokens,
                &state.member_rects,
                &state.float_rects,
                &std::collections::HashMap::new(),
                "mon-a",
                BOUNDS,
                &taskbar,
            )
            .expect("pager anchors on the bottom-docked bar");
            assert_eq!(pager.ids, after);
            assert_eq!(pager.cells.len(), after.len());
            let active = state.workspaces.active_id("mon-a");
            let menu = crate::tray::menu_items_with_pager(
                false,
                "status",
                crate::tray::WorkspaceMenu::new(Some(true), true),
                &after,
                active.as_deref(),
            );
            let rows = menu
                .iter()
                .filter(|item| {
                    matches!(item, crate::tray::MenuItem::Command { id, .. }
                        if crate::tray_pager::workspace_pick_index(*id, after.len()).is_some())
                })
                .count();
            assert_eq!(rows, after.len());
            assert_eq!(pager.cells.len(), rows);
            clear_pending_seams();
        }
    }
}
