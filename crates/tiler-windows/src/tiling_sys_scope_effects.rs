//! Scoped multi-output select effects (item 12 stage B).
//!
//! Native global-unique visible-elsewhere swap plus shared multi-output
//! selection for [`super::workspace_do_select`], which routes here through
//! thin call sites once the scope owner plans a swap or a shared select.
//! The local single-output path never enters this module and stays exactly
//! as it was: every shape below mirrors the corresponding
//! `workspace_do_select` phase (same hide fences, same narrow recovery,
//! same activation/history ordering, same focus-before-geometry tail), only
//! generalized from one output to the planned output set.
//!
//! Transfer design:
//! - Global swap exchanges the two [`crate::workspace::ManagedWorkspaces`]
//!   views per the stage A core contract
//!   ([`tiler_core::workspace_scope::apply_swap_assignments`]): both ids
//!   leave every scope, the target appends to the requesting output's scope
//!   end and the requesting current appends to the other output's scope end
//!   (KDE assignment order). Members, remembered focus (`last_focus`), and
//!   entry tokens travel with the entries while per-member output locations
//!   rekey by id to the new home output. Sticky
//!   all-desktop members stay homed on their output exactly like the
//!   migration rule. Session tiled state is keyed by workspace id and
//!   travels unchanged; per-key float/topmost/max/fullscreen/seen sets and
//!   hidden-claim recovery ownership are keyed by full member identity, so
//!   they ride along with zero rekeying and are never dropped except on the
//!   same verified-dead paths as the single-output select.
//! - Output-bound Engine layout/domains relocate per workspace id through
//!   the existing [`tiler_core::engine::Engine::try_relocate_for_target`]
//!   operation (the `sync_monitor_outputs` displacement reference), so each
//!   view keeps its layout tree, contents, and remembered focus on its new
//!   output. Relocation is best-effort with honest counts: a refused move
//!   (ambiguous or unusable retained session) reconciles fresh below instead
//!   of failing the switch, exactly like displacement.
//! - Shared select converges every planned live output onto the same logical
//!   id: outputs missing the entry converge it via `ensure_workspace_id`
//!   (empty entry, fresh token, no activation), then every output runs the
//!   ordinary hide/reveal/activate tail. Per-output Engine domains are never
//!   relocated for shared selects, so each output keeps its own layout
//!   domain; history records once through the single shared entry. Because a
//!   shared id is legitimately multi-homed, memberful sides missing their
//!   session claim the Engine key with an empty placeholder first
//!   (`claim_shared_side_session`): without the claim the reconcile would
//!   steal the sibling output's session through the single-homed relocate
//!   arm, destroying its layout (and retiring both sides when the rows are
//!   empty). Memberless sides run no reconcile at all; an unclaimable
//!   memberful side skips geometry fail-closed rather than risk the steal.
//! - `WorkspaceDomain` output (the Engine `(output, workspace)` key) is
//!   never conflated with the logical global workspace id: transfers rekey
//!   one or the other explicitly, never by renaming ids.
//!
//! Commit fences and partial-failure honesty match the single-output
//! contract on every side: any hide or reveal failure before activation
//! reveals exactly the claims committed in this transition (exact identity,
//! no blind replay) and returns `partial` with both views kept and no
//! history on either output. History records only after verified
//! hide/reveal plus activation, on both swap outputs and once for shared.
//! Trailing lifecycle converges through the shared scope contract in the
//! tail below; send resolvers stay untouched.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::workspace_scope_owner::SharedSelectPlan;
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::workspace_scope::SwapPlan;
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::{
    ActionCtx, ApplySummary, HintCx, MonitorArea, ObservedWindow, RetainedRow, SelectEffect,
    TileLoop,
};
use crate::model::ProcessIdentity;
use crate::storage::LedgerStore;
use crate::workspace::WindowKey;

/// Settled hide phase for one output side: whether the phase failed plus
/// the windows newly committed to hidden in this transition (exact keys
/// for narrow recovery).
struct HideSide {
    failed: bool,
    newly_hidden: Vec<WindowKey>,
}

/// Hide one output's leaving set with the exact `workspace_do_select`
/// fences: out-of-scope members stay visible with membership intact, sticky
/// members stay visible, recycled-HWND membership drops with no writes, and
/// every committed claim registers before any failure can strand it.
#[allow(clippy::too_many_arguments)]
fn hide_side_members(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    by_hwnd: &HashMap<u64, &ObservedWindow>,
    output: &str,
    current: &str,
    ctx: &ActionCtx,
    hide_diag_logged: &mut usize,
) -> HideSide {
    let leaving = state.workspaces.workspace_members(output, current);
    let mut failed = false;
    let mut newly_hidden: Vec<WindowKey> = Vec::new();
    for key in &leaving {
        if state.workspaces.is_hidden(key) {
            continue;
        }
        let Some(expected) = state.member_identity.get(key).cloned() else {
            continue;
        };
        if !crate::tiling::scope_allows(&state.scope, &expected.exe_path) {
            continue;
        }
        let (live_ok, iconic) = match by_hwnd.get(&key.hwnd) {
            Some(window) => {
                let live_ok = crate::workspace_owner::member_matches(
                    key,
                    window.hwnd,
                    window.identity.pid,
                    &window.identity.process_creation,
                ) && window.identity.exe_path == expected.exe_path
                    && window.identity.user_sid == expected.user_sid
                    && window.identity.session_id == expected.session_id;
                (live_ok, window.facts.minimized)
            }
            None => match crate::product_hide::sys::query_recovery(key.hwnd, me) {
                Ok(snap) => {
                    let live_ok = snap.pid == expected.pid
                        && snap.process == expected
                        && snap.nonce.is_none();
                    (live_ok, snap.iconic)
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                        super::drop_member_state(state, key);
                    } else {
                        failed = true;
                    }
                    continue;
                }
            },
        };
        if !live_ok {
            super::drop_member_state(state, key);
            continue;
        }
        if state.sticky.contains_key(key) {
            continue;
        }
        let had_claim = state.hidden_claims.contains_key(key);
        let project_topmost = crate::product_hide::project_topmost_hide_allowed(
            state.float_topmost_prev.get(key).copied(),
            super::read_topmost_now(key.hwnd),
        );
        let (outcome, detail) =
            super::workspace_hide_one(state, me, store, dir, key, &expected, iconic);
        if *hide_diag_logged < super::MAX_HIDE_DIAG_PER_SELECT {
            let token = state.member_tokens.get(key).cloned().unwrap_or_default();
            super::log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-hide",
                    "correlation": ctx.correlation,
                    "window": if token.is_empty() { serde_json::Value::Null } else { serde_json::Value::from(token) },
                    "outcome": outcome,
                    "detail": detail,
                    "project_topmost": project_topmost,
                }),
            );
            *hide_diag_logged += 1;
        }
        if state.hidden_claims.contains_key(key) || outcome == "hidden" {
            state.workspaces.set_hidden(key, true);
            if !had_claim {
                newly_hidden.push(key.clone());
            }
        } else if outcome == "origin-vanished" || outcome == "identity-changed" {
            super::drop_member_state(state, key);
        } else if outcome == "scope-excluded" {
            // Explicit scope fence: out-of-scope members stay visible with
            // membership intact, never hidden, never a failure.
        } else {
            failed = true;
        }
    }
    HideSide {
        failed,
        newly_hidden,
    }
}

/// Reveal one entering set with the exact `workspace_do_select`
/// semantics: claim-free members simply unflag, claimed members reveal
/// through the proof gate plus the verified claim path, and retired claims
/// drop their full session state. True when the phase failed. The entering
/// set is caller-provided because swap sides reveal entries still homed on
/// the other output until the assignment transfer rekeys them.
fn reveal_side_members(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    entering: &BTreeSet<WindowKey>,
) -> bool {
    let mut failed = false;
    for key in entering {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            continue;
        };
        if super::workspace_proof_reveal_gate(state, me, key.hwnd).is_err() {
            failed = true;
            continue;
        }
        match crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        ) {
            Ok(crate::product_hide::ProductTeardown::Retired) => {
                state.hidden_claims.remove(key);
                if let Some(token) = state.member_tokens.remove(key) {
                    state.member_rects.remove(&token);
                    state.float_rects.remove(&token);
                    state.hint_logged.remove(&token);
                }
                if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
                    state.restore_wake = None;
                }
                state.float_topmost_prev.remove(key);
                state.floated.remove(key);
                state.member_identity.remove(key);
                state.member_tags.remove(key);
                state.workspaces.remove_window(key);
            }
            Ok(_) => {
                state.hidden_claims.remove(key);
                state.workspaces.set_hidden(key, false);
            }
            Err(_) => {
                failed = true;
            }
        }
    }
    failed
}

/// Narrow recovery shared by every scoped transition: reveal exactly the
/// claims committed above (exact identity, no blind replay, no fabricated
/// rollback). Retired claims drop member state; revealed claims unflag.
fn recover_newly_hidden(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    newly_hidden: &[WindowKey],
) {
    for key in newly_hidden {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            continue;
        };
        match crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        ) {
            Ok(crate::product_hide::ProductTeardown::Retired) => {
                state.hidden_claims.remove(key);
                super::drop_member_state(state, key);
            }
            Ok(_) => {
                state.hidden_claims.remove(key);
                state.workspaces.set_hidden(key, false);
            }
            Err(_) => {}
        }
    }
}

/// Relocate one retained Engine session to its post-transfer output domain
/// (the displacement reference): layout tree, contents, and remembered
/// focus move with the workspace id instead of reseeding. Best-effort with
/// an honest verdict: refusal keeps the retained session and the geometry
/// tail below reconciles fresh.
fn relocate_workspace_domain(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    output: &str,
    ws: &str,
) -> bool {
    let target_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws.to_owned()),
    };
    let Some(bounds) = areas
        .iter()
        .find(|a| a.device == output)
        .and_then(|a| crate::tiling::tiling_domain_bounds_with(a.work, state.outer_gap))
    else {
        return false;
    };
    let target_domain = tiler_core::session::OutputDomain {
        id: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws.to_owned()),
        bounds,
        gap: state.inner_gap,
        adjacent: std::collections::BTreeMap::new(),
    };
    state
        .engine
        .try_relocate_for_target(&target_key, &target_domain, state.outer_gap)
}

/// Claim one shared side's Engine target key with an empty placeholder
/// session before the tail reconciles.
///
/// Background: the Engine reconcile seeds a missing target key by
/// relocating the unique same-workspace-id session from another output
/// (`try_relocate_for_target`). That move is correct for single-homed ids
/// (global swap, displacement), but a shared id is legitimately multi-homed:
/// reconciling a missing `(output, target)` key would steal the sibling
/// output's session - destroying its layout, and with empty rows even
/// retiring both sides. Claiming the key first (an empty domain-listed
/// session, self-cleaned by the take-usable empty arm) forces the reconcile
/// onto the fresh-seed path with the sibling untouched. Existing sessions
/// are never disturbed here: the caller only claims keys the Engine reports
/// missing. `false` when the key already exists or the claim failed; a
/// failed claim must skip that side's geometry (fail closed, never steal).
fn claim_shared_side_session(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    output: &str,
    target: &str,
) -> bool {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(target.to_owned()),
    };
    if state.engine.contains(&key) {
        return true;
    }
    let Some((domain, _)) =
        super::workspace_domain_for(output, target, areas, state.inner_gap, state.outer_gap)
    else {
        return false;
    };
    let Ok(empty) = tiler_core::session::Session::new(
        state.owner.clone(),
        state.generation.clone(),
        0,
        0,
        vec![domain],
    ) else {
        return false;
    };
    state.engine.insert_raw(key, empty, state.outer_gap);
    true
}

/// Settled post-activation tail shared by swap and shared selects: one fresh
/// observation, assignment convergence, requesting-output focus, and one
/// Engine reconcile plus geometry pass per changed `(output, workspace)`
/// view. Mirrors the `workspace_do_select` tail phase for phase; trailing
/// lifecycle converges through the shared scope contract below (per-output
/// scoped plans, one logical shared ring).
struct ScopeTail {
    observation_ms: u64,
    focus: &'static str,
    focus_ms: u64,
    geometry: Option<ApplySummary>,
    geometry_ms: u64,
}

/// One reconciled geometry side for the tail below.
struct TailSide {
    output: String,
    workspace: String,
    summary: Option<ApplySummary>,
    geometry_ms: u64,
}

#[allow(clippy::too_many_arguments)]
fn converge_tail(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    focus_output: &str,
    focus_target: &str,
    sides: &[(String, String)],
    ctx: &ActionCtx,
    focus_hint: Option<&WindowKey>,
    suppress_focus: bool,
    skip_geometry: bool,
) -> Option<ScopeTail> {
    let observation_start = Instant::now();
    let mut fresh_skipped: Vec<(String, String)> = Vec::new();
    let mut fresh_retained: Vec<RetainedRow> = Vec::new();
    let mut fresh_observed = state.observe(me, fulls, &mut fresh_skipped, &mut fresh_retained)?;
    let observation_ms = observation_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    super::publish_managed(state, me, &fresh_observed, &fresh_retained);
    super::ensure_workspace_assignments(state, me, &mut fresh_observed, &fresh_retained, areas);
    super::clear_maximize_at_admission(state, me, &fresh_retained);
    super::workspace_close_cleanup(state);
    // Scoped trailing convergence (item 12 stage B): per-output scoped plans
    // for swap sides, one logical ring for shared selects. Local never
    // reaches this tail.
    {
        let outputs: Vec<String> = sides.iter().map(|(o, _)| o.clone()).collect();
        super::tiling_sys_scope_trailing::tail_trailing(state, &outputs);
    }
    let mut focus_outcome: &'static str = "no-focus";
    let mut focus_window: Option<String> = None;
    let mut focus_ms: u64 = 0;
    if suppress_focus {
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        let observed = focus_hint.is_some_and(|hint| hint.hwnd != 0 && foreground == hint.hwnd);
        if observed {
            focus_outcome = "focus-ok";
            if let Some(hint) = focus_hint {
                focus_window = state.member_tokens.get(hint).cloned();
                state.workspaces.note_foreground(hint);
            }
        } else {
            focus_outcome = "focus-suppressed";
        }
    } else {
        let members = state
            .workspaces
            .workspace_members(focus_output, focus_target);
        let mut fresh_tokens: HashSet<String> =
            fresh_observed.iter().map(|w| w.token.clone()).collect();
        for row in &fresh_retained {
            if row.maximized || row.fullscreen {
                fresh_tokens.insert(row.token.clone());
            }
        }
        let eligible =
            state
                .workspaces
                .eligible_focus_set(&members, &state.member_tokens, &fresh_tokens);
        let focus_key = focus_hint
            .filter(|hint| eligible.contains(*hint))
            .cloned()
            .or_else(|| {
                state
                    .workspaces
                    .focus_target(focus_output, focus_target, &eligible)
            });
        if let Some(ref focus_key) = focus_key {
            let token = state
                .member_tokens
                .get(focus_key)
                .cloned()
                .unwrap_or_default();
            if !token.is_empty() {
                focus_window = Some(token.clone());
                if crate::workspace_owner::focus_before_geometry(
                    true,
                    super::suspend_read(state, me, fulls).veto.block,
                    super::foreground_elevated(me),
                ) {
                    let focus_start = Instant::now();
                    let actuation = super::actuate_focus(
                        state,
                        me,
                        fulls,
                        &fresh_observed,
                        &fresh_retained,
                        &token,
                    );
                    focus_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    focus_outcome = actuation.outcome;
                    if actuation.outcome == "focus-ok" {
                        state.workspaces.note_foreground(focus_key);
                    }
                } else {
                    focus_outcome = "focus-skipped-fence";
                }
            }
        }
    }
    super::log_json_at(
        &state.log_path.clone(),
        serde_json::json!({"event":"select-focus","correlation":ctx.correlation,"window":focus_window,"focus":focus_outcome}),
    );
    let mut hint_cx = HintCx::new();
    let mut tail_sides: Vec<TailSide> = Vec::new();
    for (output, workspace) in sides {
        let mut summary: Option<ApplySummary> = None;
        let mut geometry_ms: u64 = 0;
        if super::workspace_mode_tiled(state, output, workspace)
            && let Some((domain, domain_key)) = super::workspace_domain_for(
                output,
                workspace,
                areas,
                state.inner_gap,
                state.outer_gap,
            )
            && let Some(rows) = super::assemble_domain_rows(
                state,
                output,
                workspace,
                &fresh_observed,
                &fresh_retained,
                "select",
                ctx.correlation.as_str(),
                &mut hint_cx,
            )
        {
            let windows: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let focused = state
                .focused_token(&fresh_observed)
                .filter(|f| windows.iter().any(|(w, _, _, _)| w == f));
            state.tick += 1;
            let tick = state.tick;
            let correlation = CorrelationId::parse(&ctx.correlation)
                .expect("action correlation is a valid token");
            {
                let revision = super::revision_for(state, output, workspace);
                let outer_gap = state.outer_gap;
                super::adopt_gaps_for_route(
                    state,
                    &domain,
                    &domain_key,
                    outer_gap,
                    &windows,
                    focused.as_ref(),
                    revision,
                    fp,
                    &correlation,
                );
            }
            let plan_start = Instant::now();
            let mut event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner,
                &state.generation,
                &correlation,
                super::revision_for(state, output, workspace),
                fp,
                &domain,
                &domain_key,
                state.outer_gap,
                &windows,
                focused.as_ref(),
            );
            let reply = super::handle_engine_event(state, &mut event);
            let plan_ms = plan_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let _ = plan_ms;
            geometry_ms = if skip_geometry {
                0
            } else {
                let writable = super::writable_tokens(state, output, workspace, &fresh_observed);
                let geometry_start = Instant::now();
                summary = super::apply_geometry(
                    state,
                    super::ApplyInput {
                        me,
                        fulls,
                        reply: &reply,
                        observed: &fresh_observed,
                        op: "reconcile",
                        tick,
                        correlation: ctx.correlation.as_str(),
                        skipped: fresh_skipped.clone(),
                        writable: &writable,
                        output_token: state.workspaces.output_token(output),
                        workspace_token: state.workspaces.workspace_token(output, workspace),
                        revision: super::revision_for(state, output, workspace),
                    },
                );
                geometry_start
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64
            };
        }
        // Non-requesting sides still applied natively above: their geometry
        // evidence rides this bounded line since the caller's action line
        // carries only the requesting side.
        if output != focus_output || workspace != focus_target {
            let phase = |s: &Option<ApplySummary>| match s {
                Some(s) => serde_json::json!({
                    "applied": s.applied,
                    "mismatched": s.mismatched,
                    "readback_ok": s.readback_ok,
                }),
                None => serde_json::Value::Null,
            };
            super::log_json_at(
                &state.log_path.clone(),
                serde_json::json!({
                    "event": "workspace-scope-side",
                    "correlation": ctx.correlation,
                    "output": state.workspaces.output_token(output),
                    "workspace": state.workspaces.workspace_token(output, workspace),
                    "geometry_ms": geometry_ms,
                    "geometry": phase(&summary),
                }),
            );
        }
        tail_sides.push(TailSide {
            output: output.clone(),
            workspace: workspace.clone(),
            summary,
            geometry_ms,
        });
    }
    let requesting = tail_sides
        .into_iter()
        .find(|s| s.output == focus_output && s.workspace == focus_target);
    let (geometry, geometry_ms) = match requesting {
        Some(side) => (side.summary, side.geometry_ms),
        None => (None, 0),
    };
    Some(ScopeTail {
        observation_ms,
        focus: focus_outcome,
        focus_ms,
        geometry,
        geometry_ms,
    })
}

/// Global-unique visible-elsewhere swap (item 12 stage B): exchange the
/// requesting output's current view with the other output's current view
/// instead of duplicating the selection.
///
/// Both outputs hide their leaving sets and reveal the incoming sets with
/// the single-output fences; any failure before activation recovers both
/// sides narrowly and returns `partial` with both views kept and no history
/// on either output. Engine sessions relocate per workspace id
/// (best-effort, counted), entries exchange with member-output rekeying,
/// both outputs activate, both completed transitions record scoped history,
/// and the tail converges focus (requesting output) plus geometry on both
/// new views. Returns `ok` on the verified switch.
#[allow(clippy::too_many_arguments)]
pub(super) fn global_swap_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    output: &str,
    plan: &SwapPlan,
    ctx: &ActionCtx,
    focus_hint: Option<&WindowKey>,
    suppress_focus: bool,
    skip_geometry: bool,
) -> SelectEffect {
    let none =
        |outcome: &'static str, source_workspace: String, target_workspace: String| SelectEffect {
            outcome,
            geometry: None,
            focus: "none",
            transition_ms: 0,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace,
            target_workspace,
        };
    let active_current = plan.active_current.clone();
    let target = plan.target.clone();
    let other = plan.other_output.clone();
    let source_token = state.workspaces.workspace_token(output, &active_current);
    let target_token = state.workspaces.workspace_token(output, &target);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let mut hide_diag_logged: usize = 0;
    let hide_start = Instant::now();
    let first = hide_side_members(
        state,
        me,
        store,
        dir,
        &by_hwnd,
        output,
        &active_current,
        ctx,
        &mut hide_diag_logged,
    );
    let second = hide_side_members(
        state,
        me,
        store,
        dir,
        &by_hwnd,
        &other,
        &target,
        ctx,
        &mut hide_diag_logged,
    );
    let hide_ms = hide_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let mut newly_hidden = first.newly_hidden;
    newly_hidden.extend(second.newly_hidden);
    if first.failed || second.failed {
        recover_newly_hidden(state, me, store, &newly_hidden);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    let reveal_start = Instant::now();
    // Pre-transfer homing: `target` still lives on `other`, `active_current`
    // still on `output` (`swap_entries` rekeys below), so each side reveals
    // the entry still homed on the opposite output. Hide/reveal never
    // activate, so these sets address the right members directly.
    let entering_output = state.workspaces.workspace_members(&other, &target);
    let entering_other = state.workspaces.workspace_members(output, &active_current);
    let first_reveal_failed = reveal_side_members(state, me, store, &entering_output);
    let second_reveal_failed = reveal_side_members(state, me, store, &entering_other);
    let reveal_ms = reveal_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if first_reveal_failed || second_reveal_failed {
        recover_newly_hidden(state, me, store, &newly_hidden);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    // Engine layout/domains relocate per workspace id to the post-transfer
    // destination homes `swap_entries` appends below (`(output, target)` and
    // `(other, active_current)`): best-effort with honest counts.
    let mut relocated: u32 = 0;
    let mut retained_count: u32 = 0;
    for (relocate_output, ws) in [
        (output, target.as_str()),
        (other.as_str(), active_current.as_str()),
    ] {
        if relocate_workspace_domain(state, areas, relocate_output, ws) {
            relocated += 1;
        } else {
            retained_count += 1;
        }
    }
    // Assignment transfer: entries exchange with member-output rekeying.
    // Recovery ownership, float/topmost/max flags, and remembered focus ride
    // the entries/keys with no rekeying and no drops.
    let sticky: BTreeSet<WindowKey> = state.sticky.keys().cloned().collect();
    if !state
        .workspaces
        .swap_entries(output, &active_current, &other, &target, &sticky)
    {
        recover_newly_hidden(state, me, store, &newly_hidden);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    state.active_output = output.to_owned();
    // Both completed transitions record scoped history, including when later
    // focus/geometry fails below. Same-view activation never reaches here
    // (the plan guarantees distinct views); failed hide/reveal returned
    // partial above with no history on either output.
    {
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        owner.record_completed(spaces, output, &target);
        owner.record_completed(spaces, &other, &active_current);
    }
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let post_source = state.workspaces.workspace_token(output, &target);
    let post_other = state.workspaces.workspace_token(&other, &active_current);
    super::log_json_at(
        &state.log_path.clone(),
        serde_json::json!({
            "event": "workspace-swap",
            "correlation": ctx.correlation,
            "tick": ctx.tick,
            "output": state.workspaces.output_token(output),
            "workspace": post_source,
            "other_output": state.workspaces.output_token(&other),
            "other_workspace": post_other,
            "relocated": relocated,
            "retained": retained_count,
            "outcome": "ok",
        }),
    );
    let sides = vec![
        (output.to_owned(), target.clone()),
        (other.clone(), active_current.clone()),
    ];
    let Some(tail) = converge_tail(
        state,
        me,
        fulls,
        areas,
        output,
        &target,
        &sides,
        ctx,
        focus_hint,
        suppress_focus,
        skip_geometry,
    ) else {
        let observation_ms = 0;
        return SelectEffect {
            outcome: "observation-failed",
            transition_ms,
            observation_ms,
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none(
                "observation-failed",
                source_token.clone(),
                target_token.clone(),
            )
        };
    };
    SelectEffect {
        outcome: "ok",
        geometry: tail.geometry,
        focus: tail.focus,
        transition_ms,
        observation_ms: tail.observation_ms,
        geometry_ms: tail.geometry_ms,
        hide_ms,
        reveal_ms,
        focus_ms: tail.focus_ms,
        plan_ms: 0,
        source_workspace: source_token,
        target_workspace: target_token,
    }
}

/// Shared multi-output selection (item 12 stage B): converge every planned
/// live output onto the same logical id.
///
/// Outputs already on the target converge with no native effect; the rest
/// hide their current view and reveal the target members homed on that
/// output (converging the entry first when the output lacks it). Any hide
/// or reveal failure before activation recovers every side narrowly and
/// returns `partial` with all views kept and no history. Activation lands
/// on every converged output, history records once through the single
/// shared entry, and the tail converges focus (requesting output) plus
/// geometry per output against preserved per-output layout domains.
/// Single-output shared selections run the same path with one side.
#[allow(clippy::too_many_arguments)]
pub(super) fn shared_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    output: &str,
    plan: &SharedSelectPlan,
    ctx: &ActionCtx,
    focus_hint: Option<&WindowKey>,
    suppress_focus: bool,
    skip_geometry: bool,
) -> SelectEffect {
    let none =
        |outcome: &'static str, source_workspace: String, target_workspace: String| SelectEffect {
            outcome,
            geometry: None,
            focus: "none",
            transition_ms: 0,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace,
            target_workspace,
        };
    let target = plan.target.clone();
    let Some(current) = state.workspaces.active_id(output) else {
        return none("unknown-output", "ws?".to_owned(), "ws?".to_owned());
    };
    let source_token = state.workspaces.workspace_token(output, &current);
    let target_token = state.workspaces.workspace_token(output, &target);
    // Converge sides: live outputs from the plan that still exist. The
    // requesting output always converges; vanished outputs skip honestly
    // (their views are gone, never pretended).
    let mut sides: Vec<(String, String)> = Vec::new();
    for planned in &plan.outputs {
        let Some(side_current) = state.workspaces.active_id(planned) else {
            continue;
        };
        if side_current != target {
            sides.push((planned.clone(), side_current));
        }
    }
    if sides.is_empty() {
        // Every live output already shows the target: already there with no
        // native effect and no history, like the single-output guard.
        state.active_output = output.to_owned();
        let token = state.workspaces.workspace_token(output, &target);
        return none("already-active", token.clone(), token);
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let mut hide_diag_logged: usize = 0;
    let hide_start = Instant::now();
    let mut hide_failed = false;
    let mut newly_hidden: Vec<WindowKey> = Vec::new();
    for (side_output, side_current) in &sides {
        let side = hide_side_members(
            state,
            me,
            store,
            dir,
            &by_hwnd,
            side_output,
            side_current,
            ctx,
            &mut hide_diag_logged,
        );
        hide_failed = hide_failed || side.failed;
        newly_hidden.extend(side.newly_hidden);
    }
    let hide_ms = hide_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if hide_failed {
        recover_newly_hidden(state, me, store, &newly_hidden);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    // Converge entries before revealing: outputs lacking the logical id gain
    // an empty entry (fresh token, no activation yet) so every live output
    // can show the same id. Brand-new id creation still owns trailing
    // lifecycle elsewhere; this only converges an already-live shared id.
    for (side_output, _) in &sides {
        state.workspaces.ensure_workspace_id(side_output, &target);
    }
    let reveal_start = Instant::now();
    let mut reveal_failed = false;
    for (side_output, _) in &sides {
        let entering = state.workspaces.workspace_members(side_output, &target);
        reveal_failed = reveal_side_members(state, me, store, &entering) || reveal_failed;
    }
    let reveal_ms = reveal_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if reveal_failed {
        recover_newly_hidden(state, me, store, &newly_hidden);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    for (side_output, _) in &sides {
        state.workspaces.activate(side_output, &target);
    }
    state.active_output = output.to_owned();
    // One shared history entry for the completed multi-output transition,
    // including when later focus/geometry fails below.
    {
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        owner.record_completed(spaces, output, &target);
    }
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let converged: Vec<String> = sides
        .iter()
        .map(|(o, _)| state.workspaces.output_token(o))
        .collect();
    super::log_json_at(
        &state.log_path.clone(),
        serde_json::json!({
            "event": "workspace-shared",
            "correlation": ctx.correlation,
            "tick": ctx.tick,
            "workspace": state.workspaces.workspace_token(output, &target),
            "outputs": converged,
            "outcome": "ok",
        }),
    );
    // Geometry converges per output against preserved per-output layout
    // domains: memberless sides run no reconcile (nothing to lay out, and a
    // missing key must never trigger the Engine's single-homed relocate
    // steal), while memberful sides missing their session claim the key
    // first so the reconcile fresh-seeds with the sibling session untouched.
    // An unclaimable memberful side skips geometry fail-closed (logged with
    // a null geometry on its scope-side line) rather than risk the steal.
    let mut geometry_sides: Vec<(String, String)> = Vec::new();
    for (side_output, _) in &sides {
        if state
            .workspaces
            .workspace_members(side_output, &target)
            .is_empty()
        {
            continue;
        }
        if claim_shared_side_session(state, areas, side_output, &target) {
            geometry_sides.push((side_output.clone(), target.clone()));
        } else {
            super::log_json_at(
                &state.log_path.clone(),
                serde_json::json!({
                    "event": "workspace-scope-side",
                    "correlation": ctx.correlation,
                    "output": state.workspaces.output_token(side_output),
                    "workspace": state.workspaces.workspace_token(side_output, &target),
                    "geometry_ms": 0,
                    "geometry": serde_json::Value::Null,
                    "note": "session-claim-failed",
                }),
            );
        }
    }
    let Some(tail) = converge_tail(
        state,
        me,
        fulls,
        areas,
        output,
        &target,
        &geometry_sides,
        ctx,
        focus_hint,
        suppress_focus,
        skip_geometry,
    ) else {
        return SelectEffect {
            outcome: "observation-failed",
            transition_ms,
            observation_ms: 0,
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none(
                "observation-failed",
                source_token.clone(),
                target_token.clone(),
            )
        };
    };
    SelectEffect {
        outcome: "ok",
        geometry: tail.geometry,
        focus: tail.focus,
        transition_ms,
        observation_ms: tail.observation_ms,
        geometry_ms: tail.geometry_ms,
        hide_ms,
        reveal_ms,
        focus_ms: tail.focus_ms,
        plan_ms: 0,
        source_workspace: source_token,
        target_workspace: target_token,
    }
}
