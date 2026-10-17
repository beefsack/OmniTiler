#![forbid(unsafe_code)]
//! Source-first full-world observation capture (W1, before sequencing).
//!
//! Pure, offline and read-only over the real owner plus real per-tick
//! products: [`TileLoop`] tables, [`ObservedWindow`](super::observe::ObservedWindow),
//! [`RetainedRow`]/[`HiddenRecord`]/[`PendingClosed`](super::retained::RetainedRow),
//! [`MonitorArea`](super::monitors::MonitorArea) and
//! [`ManagedWorkspaces`](crate::workspace::ManagedWorkspaces). No parallel
//! DTOs: fixtures instantiate these exact production structures through the
//! same [`capture_world`] constructor the runtime will use. No Engine
//! mutation, no FFI, no sequencing: observation/request/command counters and
//! acknowledgement belong to the dependent sequencer unit, which is
//! deliberately not built here.
//!
//! Source map:
//! - `observe.rs` `ObservedWindow`: identity-first observation with
//!   iconic-before-frame ordering. Lifetimes derive directly from the
//!   observed identity (`hwnd`, `pid`, `process_creation`) as the full
//!   [`WindowKey`](crate::workspace::WindowKey), never HWND alone.
//! - `observation.rs` `TileLoop::observe`: minimized omission
//!   (`minimized_omitted`), scope/hosted gates and the per-tick retained
//!   vec. Identity-less `Unknown` evidence never becomes a member row.
//! - `retained.rs` `RetainedRow` (`rect: None` plus `facts: None` is
//!   frameless: unreadable or scope-excluded, never minimized),
//!   `HiddenRecord` (durable hide-time show preimage
//!   `claim.show: WindowShowState` plus hide-time `iconic`) and
//!   `PendingClosed` (homed verified-close obligations).
//! - `workspace.rs` `ManagedWorkspaces`: committed membership, workspace
//!   order/active view, displacement records and the session output tokens
//!   that serve as the wire-opaque output ids (native device keys such as
//!   `\\.\DISPLAY1` fail the core `is_opaque_id` gate, so the existing
//!   `output_token` source maps them collision-free).
//! - `monitors.rs` `MonitorArea`: device-keyed work (placement) and full
//!   (overlay) rectangles. `MonitorArea` carries no primary flag; the owner
//!   orders `all_monitors` primary-first, so capture derives primary from
//!   the `areas.first()` device, never from managed output order (which may
//!   be reordered relative to the native list).
//! - Owner intent lanes (`floated`, `sticky`, `restart_reject_hold`,
//!   `fixed_auto_seen`, `fixed_tile_overrides`, `fixed_origin_overrides`,
//!   `born_fullscreen`, `born_first_exit`), lifetime tags (`member_tags`),
//!   stored identities (`member_identity`), snapshots (`member_rects` /
//!   `float_rects`), last-good constraints (`hint_logged`) and the Engine
//!   exception read (`engine_is_float`) are all consumed read-only.
//!
//! Truthfulness rules enforced here (W1 discriminators):
//! - Domains carry raw work (placement) bounds plus the configured
//!   `outer_gap` separately: the core insets exactly once through
//!   `inset_world_domain`/`world_publish`. The adapter-side inset helper
//!   runs offline only as a validates-the-margin check (a work area that
//!   cannot carry the margin holds instead of tiling un-inset).
//! - Every carried row re-validates its token against `member_tokens` for
//!   the same `WindowKey`. A mismatched lifetime inherits no flags and no
//!   lanes: its domain holds instead of carrying a partial controlled list.
//! - The stored member identity must name the same lifetime (`pid` and
//!   `process_creation` equal the key): drifted tables hold instead of
//!   scoping or classifying another process.
//! - Fresh native facts ride only when the typed report can express them:
//!   eligible, or an overlay (`fullscreen`/`maximized`) with a dedicated
//!   lane. Cloaked, elevated, shell, tool, owned, dialog, no-activate,
//!   hidden, or minimized facts have no window authority lane
//!   (`fit_excluded` is fit semantics only and is dropped at the session
//!   boundary in `observed_window_from_engine`), so the domain holds
//!   instead of authorizing geometry, membership, or effects. Scope
//!   exclusion holds the same way: unlike the legacy row assembly (which
//!   rides a row and gates writes in `writable_tokens`), the typed report
//!   has no scope authority lane, so riding would authorize writes.
//!   Maximized/fullscreen overlays keep their reserved tiled allocation
//!   where the shared contract supports it and are never blanket-floated:
//!   born-fullscreen slotless (floating) stays distinct from ordinary
//!   tiled-fullscreen reserved (tiled with fullscreen set).
//! - Hidden members additionally prove the claim association: the
//!   `HiddenRecord` claim identity must equal the member key, the claim tag
//!   must satisfy `model::valid_claim_tag`, and a stamped lifetime tag must
//!   exist. The claim tag (hidden-claim nonce) and the member tag
//!   (visible-membership lifetime) are different native properties: only
//!   presence is required of the member tag, never equality. Unproven
//!   hidden lifetimes hold even when iconic: minimization is never granted
//!   on unverified identity.
//! - Hidden non-iconic members ride their snapshot rectangles with the
//!   durable hide-time maximized preimage (`claim.show.maximized`), the
//!   adapter intent lanes and fresh same-tick rows when present. Fullscreen
//!   rides only verified sources (a token-matched fresh observed/retained
//!   row, or the `born_fullscreen` hold lane); a hidden member with no
//!   verifiable fullscreen holds its domain. Ordinary hidden fullscreen
//!   rides reserved (tiled lane with fullscreen set), never floated:
//!   floating comes only from the intent lanes. Nothing hidden defaults
//!   flags to `false` to authorize writes.
//! - Frameless retained rows (`facts: None`) and scope-excluded members are
//!   never minimized: frameless iconic omission yields an explicit
//!   minimized entry, anything else frameless holds its domain as
//!   unreadable. Scope exclusion derives from the stored identity through
//!   production `scope_allows`; the hosted-child gate needs live child
//!   enumeration so it cannot re-run offline, but its retained consequence
//!   (membership kept with a frameless row) holds identically.
//! - Omission is never a close: only queued `PendingClosed` obligations
//!   ride `DomainReport.closed`, homed per domain. A close observed live
//!   again is stale and drops; closes homed on unknown domains stay pending
//!   with the caller.
//! - A failed enumeration (`None`, mirroring `TileLoop::observe`) holds
//!   every inventoried scope as incomplete and clears completeness: a failed
//!   enum never encodes as successful emptiness, and absence on a failed
//!   tick infers no close and no minimize. Explicit queued closes still
//!   ride their homed domain beside the hold, matching the incomplete-
//!   inventory contract.
//! - Domains carry no adjacency: production `workspace_domain` emits an
//!   empty map and Windows has no native adjacency routing source, so the
//!   KDE-style edge-touch computation is withheld rather than copied.
//! - Unknown inventory never fabricates an empty row: any member that
//!   cannot resolve to a verified row holds its domain (`readable = false`
//!   with an explicit
//!   [`HoldReason`](tiler_core::world_report::HoldReason)) and clears the
//!   global completeness flag.
//!
//! Production prerequisites consumed read-only (no FFI here):
//! - `assignments::audit_hidden_claims` retires dead claims through the
//!   identity-safe reveal path; `queue_verified_close` gates close queueing
//!   on the stored lifetime tag no longer reading live; admission stamps and
//!   re-verifies the visible-membership tag via `visible_lifetime_ok` (plus
//!   same-HWND reuse repair). Capture re-checks only what its inputs carry
//!   (claim-tag format, identity triple, token match, tag presence) and
//!   holds on any gap.

use super::*;
use std::collections::{BTreeSet, HashSet};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::seed::EngineWindow;
use tiler_core::world_report::{
    DomainReport, HoldReason, MinimizedEntry, OutputTopology, WorkspaceBinding,
};

/// Captured world: the typed core observation pieces the sequencer unit
/// will envelope into a `WorldReport`. Sequencing fields (contract version,
/// owner/incarnation/generation, observation/request/command counters,
/// commands, feedback) are deliberately absent here.
#[derive(Debug, Clone)]
pub(super) struct CapturedWorld {
    pub(super) mode: tiler_core::workspace_scope::WorkspaceMode,
    pub(super) topology: Vec<OutputTopology>,
    pub(super) bindings: Vec<WorkspaceBinding>,
    pub(super) domains: Vec<DomainReport>,
    pub(super) minimized: Vec<MinimizedEntry>,
    pub(super) inventory_complete: bool,
    pub(super) gesture_barrier: bool,
    pub(super) focused_global: String,
    pub(super) default_tiled: bool,
    pub(super) fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate,
    pub(super) migration_source_refill: tiler_core::output_selection::MigrationSourceRefill,
    /// Closes observed live again: stale, dropped, never carried.
    pub(super) dropped_stale_closed: Vec<String>,
    /// Closes homed on unknown domains: uncarried, retained by the caller.
    pub(super) unhomed_closed: Vec<String>,
    /// Member tokens withheld by domain holds (never silently dropped).
    pub(super) held_members: Vec<String>,
}

/// Floating membership for one row (production row-assembly parity):
/// sticky wins unconditionally (returning before the first-exit check, so a
/// sticky first exit stays floating with no extra conjunct); the D4
/// automatic-fixed latch floats unless an explicit tile win dominates;
/// adapter float intent (floats, restart-holds, committed Engine exceptions,
/// born holds) floats unless a first-exit mark already returned the member
/// to the tiled lane. Never inferred from hints alone.
fn capture_floating(
    state: &TileLoop,
    key: &crate::workspace::WindowKey,
    output: &str,
    workspace: &str,
    token: &str,
    hidden: bool,
) -> bool {
    let sticky = state.sticky.contains_key(key);
    if sticky {
        return true;
    }
    let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
    if fixed_auto && !fixed_suppress {
        return true;
    }
    // Production first-exit parity: visible first exits return to the tiled
    // lane; hidden first exits do the same unless the member is an
    // intentional float (the hidden `first_exit_hidden` conjunction).
    let first_exit =
        state.born_first_exit.contains(key) && (!hidden || !state.floated.contains(key));
    if first_exit {
        return false;
    }
    state.floated.contains(key)
        || state.restart_reject_hold.contains(key)
        || state.born_fullscreen.contains(key)
        || admission::engine_is_float(state, output, workspace, token)
}

/// The three fixed-size lanes, preserved distinctly: the explicit tile-win
/// lanes (`fixed_tile_overrides` runtime plus `fixed_origin_overrides`
/// durable) dominate as `fixed_suppress`; the D4 automatic latch
/// (`fixed_auto_seen`) reports only when neither sticky intent nor a
/// durable win reframes it (production stamp parity: sticky dominates
/// automatic origin and durable wins never acquire it).
fn capture_fixed(state: &TileLoop, key: &crate::workspace::WindowKey) -> (bool, bool) {
    let durable =
        state.fixed_tile_overrides.contains(key) || state.fixed_origin_overrides.contains(key);
    let sticky = state.sticky.contains_key(key);
    let automatic = state.fixed_auto_seen.contains(key) && !sticky && !durable;
    (automatic, durable)
}

/// Whether fresh native facts may ride a typed report row: eligible, or an
/// overlay (`fullscreen`/`maximized`) with a dedicated `EngineWindow` lane.
/// Every other `classify` refusal (cloaked, elevated, shell, tool, owned,
/// dialog, no-activate, hidden, minimized) has no window authority lane:
/// `fit_excluded` is fit semantics only and is dropped at the session
/// boundary (`observed_window_from_engine`), so carrying such a row as
/// tiled would publish controlled frame authority for a window the adapter
/// must not touch. Callers hold the domain instead.
fn capture_facts_rideable(facts: &WindowFacts) -> bool {
    match crate::tiling::classify(facts) {
        Ok(()) => true,
        Err(SkipReason::Fullscreen | SkipReason::Maximized) => true,
        Err(_) => false,
    }
}

/// Capture one complete world observation from the real owner plus real
/// per-tick products. Read-only over `state`; `observed` is the current
/// tick's enumeration output (`None` when enumeration itself failed, exactly
/// like `TileLoop::observe`: every inventoried scope holds and completeness
/// clears, never a successful empty); `retained`/`areas` are the current
/// tick's products; `foreground_hwnd` is the caller-read live foreground
/// (0 when none). A member that cannot resolve to a verified row holds its
/// domain rather than riding a partial controlled list.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(super) fn capture_world(
    state: &TileLoop,
    observed: Option<&[ObservedWindow]>,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
) -> CapturedWorld {
    let enum_failed = observed.is_none();
    let observed: &[ObservedWindow] = observed.unwrap_or(&[]);
    let mode = state.scope_owner.adopted();
    // Liveness for stale-close detection: every member token plus every
    // carried observed/retained token. A queued close observed live again
    // is stale and drops; it never rides as a contradictory live+closed row.
    let mut live: HashSet<&str> = HashSet::new();
    for token in state.member_tokens.values() {
        live.insert(token.as_str());
    }
    for window in observed {
        live.insert(window.token.as_str());
    }
    for row in retained {
        live.insert(row.token.as_str());
    }

    let mut domains: Vec<DomainReport> = Vec::new();
    let mut minimized: Vec<MinimizedEntry> = Vec::new();
    let mut held_members: Vec<String> = Vec::new();
    let mut topology_complete = true;
    if enum_failed {
        // A failed enumeration proves nothing about any scope: every
        // inventoried scope holds below and completeness clears, even for
        // empty managed workspaces (which would otherwise read as a
        // successful empty and authorize cleanup).
        topology_complete = false;
    }

    for output in state.workspaces.output_keys() {
        let mapped = state.workspaces.output_token(&output);
        // `output_token` falls back to `"o?"` for unknown outputs, which is
        // not wire-opaque: keys from `output_keys` are always known, so this
        // arm is defensive only and holds without emitting.
        let mapped_valid = mapped != "o?";
        if !mapped_valid {
            topology_complete = false;
        }
        let area = areas.iter().find(|a| a.device == output);
        // Raw work bounds ride the report; the core insets exactly once via
        // `inset_world_domain`. The inset runs here only as a validates-the-
        // margin check: a work area that cannot carry the margin holds
        // instead of tiling an un-inset (or double-inset) domain.
        let inset_ok = area.is_some_and(|a| {
            crate::tiling::tiling_domain_bounds_with(a.work, state.outer_gap).is_some()
        });
        let raw_bounds = area.map(|a| a.work);
        let area_missing = area.is_none() || !inset_ok;
        if area_missing {
            topology_complete = false;
        }
        for workspace in state.workspaces.workspace_ids(&output) {
            let mut windows: Vec<EngineWindow> = Vec::new();
            let mut closed: Vec<WindowId> = Vec::new();
            let mut hold: Option<HoldReason> = if area_missing || !mapped_valid {
                Some(HoldReason::Unreadable)
            } else if enum_failed {
                Some(HoldReason::IncompleteInventory)
            } else {
                None
            };
            for key in state.workspaces.workspace_members(&output, &workspace) {
                if enum_failed {
                    // Failed observation resolves nothing: no rows, no
                    // minimized entries, no lifetime inheritance. Explicit
                    // queued closes still ride their homed domain below
                    // (verified evidence beside a hold, per contract); bare
                    // absence never becomes a close.
                    hold = Some(HoldReason::IncompleteInventory);
                    if let Some(token) = state.member_tokens.get(&key) {
                        held_members.push(token.clone());
                    }
                    continue;
                }
                let Some(live_token) = state.member_tokens.get(&key) else {
                    hold = Some(HoldReason::IncompleteInventory);
                    continue;
                };
                let hidden = state
                    .workspaces
                    .member_loc(&key)
                    .is_some_and(|loc| loc.hidden);
                // Scope exclusion derives from the stored identity through
                // production `scope_allows`. Without a stored identity the
                // scope fence cannot be verified, so the domain holds. The
                // stored identity must also name this exact lifetime
                // (`pid`/`process_creation` equal the key): drifted tables
                // hold instead of scoping another process.
                let Some(identity) = state.member_identity.get(&key) else {
                    hold = Some(HoldReason::IncompleteInventory);
                    held_members.push(live_token.clone());
                    continue;
                };
                if identity.pid != key.pid || identity.process_creation != key.creation {
                    hold = Some(HoldReason::IncompleteInventory);
                    held_members.push(live_token.clone());
                    continue;
                }
                let excluded = !crate::tiling::scope_allows(&state.scope, &identity.exe_path);
                // Scope exclusion has no window authority lane in the typed
                // report, so the domain holds instead of authorizing
                // geometry, membership, or effects for an unmanaged
                // executable. (Legacy row assembly rides a row and gates
                // writes in `writable_tokens`; that gate does not exist on
                // the world path, where reply authority is the only gate.)
                if excluded {
                    hold = Some(HoldReason::IncompleteInventory);
                    held_members.push(live_token.clone());
                    continue;
                }
                // Lifetime-matched rows for this key: identity first, then
                // the token must equal the stable member token or the row
                // is a recycled lifetime and inherits nothing.
                let observed_row = observed.iter().find(|w| {
                    w.hwnd == key.hwnd
                        && w.identity.pid == key.pid
                        && w.identity.process_creation == key.creation
                });
                if observed_row.is_some_and(|w| w.token != *live_token) {
                    hold = Some(HoldReason::IncompleteInventory);
                    held_members.push(live_token.clone());
                    continue;
                }
                let retained_row = retained.iter().find(|r| r.key == key);
                if retained_row.is_some_and(|r| r.token != *live_token) {
                    hold = Some(HoldReason::IncompleteInventory);
                    held_members.push(live_token.clone());
                    continue;
                }
                let sticky = state.sticky.contains_key(&key);
                let (fixed_auto, fixed_suppress) = capture_fixed(state, &key);
                if hidden {
                    // Hidden lane dominates like production assembly: the
                    // claim association must prove before any row, snapshot
                    // or minimized entry. The claim tag (hidden-claim nonce)
                    // must be well-formed, the claim identity must equal the
                    // member key, and a stamped lifetime tag must exist; the
                    // member tag itself is a different native property from
                    // the claim tag, so only presence (never equality) is
                    // required of it.
                    let associated = state.hidden_claims.get(&key).is_some_and(|record| {
                        crate::model::valid_claim_tag(&record.claim.tag)
                            && record.claim.hwnd == key.hwnd
                            && record.claim.process.pid == key.pid
                            && record.claim.process.process_creation == key.creation
                    }) && state
                        .member_tags
                        .get(&key)
                        .is_some_and(|tag| !tag.is_empty());
                    if !associated {
                        hold = Some(HoldReason::IncompleteInventory);
                        held_members.push(live_token.clone());
                        continue;
                    }
                    let record = state
                        .hidden_claims
                        .get(&key)
                        .expect("associated claim exists");
                    // Iconic minimization reads the durable preimage
                    // (`show.minimized`, minimized wins per
                    // `ProductShowRestore`) plus the hide-time and
                    // observed-iconic state.
                    if record.iconic
                        || record.claim.show.minimized
                        || state.minimized_omitted.contains(&key)
                    {
                        minimized.push(MinimizedEntry {
                            window: WindowId(live_token.clone()),
                            output: OutputId(mapped.clone()),
                            workspace: WorkspaceId(workspace.clone()),
                        });
                        continue;
                    }
                    // A token-matched fresh row carrying facts the report
                    // cannot express (cloaked, tool, shell, owned, dialog,
                    // no-activate, ...) holds the hidden domain too: the
                    // snapshot carrier has no authority lane for them, and
                    // only the overlay flags below are read from fresh rows.
                    let fresh_blocked = observed_row
                        .is_some_and(|w| !capture_facts_rideable(&w.facts))
                        || retained_row.is_some_and(|r| {
                            r.facts.is_some_and(|facts| !capture_facts_rideable(&facts))
                        });
                    if fresh_blocked {
                        hold = Some(HoldReason::IncompleteInventory);
                        held_members.push(live_token.clone());
                        continue;
                    }
                    let floating =
                        capture_floating(state, &key, &output, &workspace, live_token, true);
                    let snapshot = crate::workspace_owner::hidden_snapshot_rect(
                        floating,
                        state.float_rects.get(live_token).copied(),
                        state.member_rects.get(live_token).copied(),
                    );
                    let Some(rect) = snapshot else {
                        // Frameless hidden non-iconic: unknown frame, never
                        // minimized, never dropped.
                        hold = Some(HoldReason::Unreadable);
                        held_members.push(live_token.clone());
                        continue;
                    };
                    // Fresh same-tick rows verify overlay directly (a stale
                    // hidden flag with live evidence). Otherwise maximized
                    // rides the durable hide-time preimage and fullscreen
                    // rides only the verified born-fullscreen hold lane;
                    // with no verifiable fullscreen the domain holds rather
                    // than defaulting `false` to authorize writes.
                    let fresh_fullscreen = observed_row
                        .map(|w| w.facts.captionless_fullscreen)
                        .or_else(|| {
                            retained_row
                                .filter(|r| r.facts.is_some())
                                .map(|r| r.fullscreen)
                        });
                    let fresh_maximized = observed_row.map(|w| w.facts.maximized).or_else(|| {
                        retained_row
                            .filter(|r| r.facts.is_some())
                            .map(|r| r.maximized)
                    });
                    let maximized = fresh_maximized.unwrap_or(record.claim.show.maximized);
                    let Some(fullscreen) = fresh_fullscreen
                        .or_else(|| state.born_fullscreen.contains(&key).then_some(true))
                    else {
                        hold = Some(HoldReason::Unreadable);
                        held_members.push(live_token.clone());
                        continue;
                    };
                    let hints = state
                        .hint_logged
                        .get(live_token)
                        .copied()
                        .unwrap_or_else(WindowSizeHints::none);
                    windows.push(EngineWindow {
                        window: WindowId(live_token.clone()),
                        output: OutputId(mapped.clone()),
                        workspace: WorkspaceId(workspace.clone()),
                        rect,
                        // Ordinary hidden fullscreen rides reserved (tiled
                        // lane with fullscreen set), never floated: only the
                        // intent lanes inside `floating` (born holds,
                        // floats, sticky, Engine exceptions) ride floating.
                        floating,
                        fit_excluded: floating || fullscreen || sticky || maximized,
                        fullscreen,
                        maximized,
                        sticky,
                        fixed_auto: fixed_auto && !fixed_suppress,
                        fixed_suppress,
                        hints,
                    });
                    continue;
                }
                // Visible lane: eligible observation first, then retained
                // occupancy, mirroring production assembly order.
                if let Some(window) = observed_row {
                    // Facts the report cannot express hold the domain
                    // instead of publishing controlled authority for a
                    // window the adapter must not touch.
                    if !capture_facts_rideable(&window.facts) {
                        hold = Some(HoldReason::IncompleteInventory);
                        held_members.push(live_token.clone());
                        continue;
                    }
                    let floating =
                        capture_floating(state, &key, &output, &workspace, live_token, false);
                    let hints = state
                        .hint_logged
                        .get(live_token)
                        .copied()
                        .unwrap_or_else(WindowSizeHints::none);
                    windows.push(EngineWindow {
                        window: WindowId(live_token.clone()),
                        output: OutputId(mapped.clone()),
                        workspace: WorkspaceId(workspace.clone()),
                        rect: window.visible,
                        floating,
                        fit_excluded: floating
                            || sticky
                            || window.facts.captionless_fullscreen
                            || window.facts.maximized,
                        fullscreen: window.facts.captionless_fullscreen,
                        maximized: window.facts.maximized,
                        sticky,
                        fixed_auto: fixed_auto && !fixed_suppress,
                        fixed_suppress,
                        hints,
                    });
                    continue;
                }
                if let Some(row) = retained_row {
                    // Iconic omission minimizes without any frame: the only
                    // frameless lane that resolves. Frameless non-iconic
                    // rows (unreadable or scope-excluded) hold the domain.
                    if state.minimized_omitted.contains(&key)
                        && row.rect.is_none()
                        && !row.maximized
                        && !row.fullscreen
                    {
                        minimized.push(MinimizedEntry {
                            window: WindowId(live_token.clone()),
                            output: OutputId(mapped.clone()),
                            workspace: WorkspaceId(workspace.clone()),
                        });
                        continue;
                    }
                    if row.facts.is_none() {
                        hold = Some(HoldReason::Unreadable);
                        held_members.push(live_token.clone());
                        continue;
                    }
                    // Facts the report cannot express hold the domain:
                    // overlays (maximized/fullscreen) ride their reserved
                    // lane below, everything else ineligible authorizes
                    // nothing.
                    if row
                        .facts
                        .is_some_and(|facts| !capture_facts_rideable(&facts))
                    {
                        hold = Some(HoldReason::IncompleteInventory);
                        held_members.push(live_token.clone());
                        continue;
                    }
                    let overlay = row.maximized || row.fullscreen;
                    // Production `canonical_retained_rect` parity: an
                    // overlay rides its last-known tile allocation, never
                    // the compositor-owned maximum frame; first sightings
                    // fall back to the fresh frame.
                    let kept = state.member_rects.get(live_token).copied();
                    let rect = match (overlay, row.rect, kept) {
                        (true, _, Some(slot)) => Some(slot),
                        (_, Some(fresh), _) => Some(fresh),
                        _ => None,
                    };
                    let Some(rect) = rect else {
                        hold = Some(HoldReason::Unreadable);
                        held_members.push(live_token.clone());
                        continue;
                    };
                    let floating =
                        capture_floating(state, &key, &output, &workspace, live_token, false);
                    let hints = crate::tiling::retained_overlay_hint(
                        state.hint_logged.get(live_token).copied(),
                        true,
                        overlay,
                        floating,
                        state.born_fullscreen.contains(&key),
                        true,
                    );
                    windows.push(EngineWindow {
                        window: WindowId(live_token.clone()),
                        output: OutputId(mapped.clone()),
                        workspace: WorkspaceId(workspace.clone()),
                        rect,
                        floating,
                        fit_excluded: floating || sticky || row.fullscreen || row.maximized,
                        fullscreen: row.fullscreen,
                        maximized: row.maximized,
                        sticky,
                        fixed_auto: fixed_auto && !fixed_suppress,
                        fixed_suppress,
                        hints,
                    });
                    continue;
                }
                // Iconic omission without any row (the production iconic
                // arm omits before pushing a retained row): minimizes with
                // no frame. Any other viewless member defers with retained
                // state, never a falsely complete row.
                if state.minimized_omitted.contains(&key) {
                    minimized.push(MinimizedEntry {
                        window: WindowId(live_token.clone()),
                        output: OutputId(mapped.clone()),
                        workspace: WorkspaceId(workspace.clone()),
                    });
                    continue;
                }
                hold = Some(HoldReason::IncompleteInventory);
                held_members.push(live_token.clone());
            }
            // Explicit closes ride their homed domain even beside a hold:
            // a queued verified close is honored inside incomplete
            // inventory; only implicit missing=closed inference needs a
            // provably complete list, which this capture never performs.
            for (token, homing) in &state.pending_closed {
                if homing.output == output
                    && homing.workspace == workspace
                    && !live.contains(token.as_str())
                {
                    closed.push(WindowId(token.clone()));
                }
            }
            closed.sort_by(|a, b| a.0.cmp(&b.0));
            windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
            let readable = hold.is_none();
            if !readable {
                // Rows carried before a domain-level hold (missing area,
                // unmapped output) are withheld, never published: their
                // tokens join the held set like every other withheld row.
                for window in &windows {
                    held_members.push(window.window.0.clone());
                }
                windows.clear();
                topology_complete = false;
            }
            let focused_global = observed
                .iter()
                .find(|w| w.hwnd == foreground_hwnd)
                .map(|w| w.token.clone())
                .unwrap_or_default();
            let focused = if readable
                && !focused_global.is_empty()
                && let Some(key) = state
                    .member_tokens
                    .iter()
                    .find_map(|(key, token)| (*token == focused_global).then_some(key))
                && let Some(loc) = state.workspaces.member_loc(key)
                && loc.output == output
                && loc.workspace == workspace
                && !loc.hidden
            {
                WindowId(focused_global.clone())
            } else {
                WindowId(String::new())
            };
            // Production `workspace_domain` parity: Windows carries no
            // adjacency source, so the map stays empty rather than copying
            // the KDE edge-touch computation without native evidence. Bounds
            // are the raw work area: the core insets once via
            // `inset_world_domain` (the fallback below only rides beside a
            // hold, when the area is missing).
            let (domain_value, _) = crate::workspace_owner::workspace_domain(
                &mapped,
                &workspace,
                raw_bounds.unwrap_or(tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                }),
                state.inner_gap,
            );
            domains.push(DomainReport {
                domain: domain_value,
                outer_gap: state.outer_gap,
                focused,
                windows,
                closed,
                readable,
                hold,
            });
        }
    }
    minimized.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    held_members.sort();
    held_members.dedup();

    let mut dropped_stale_closed: Vec<String> = Vec::new();
    let mut unhomed_closed: Vec<String> = Vec::new();
    for (token, homing) in &state.pending_closed {
        if live.contains(token.as_str()) {
            dropped_stale_closed.push(token.clone());
            continue;
        }
        let homed = state.workspaces.output_keys().iter().any(|device| {
            device == &homing.output
                && state
                    .workspaces
                    .workspace_ids(device)
                    .iter()
                    .any(|id| id == &homing.workspace)
        });
        if !homed {
            unhomed_closed.push(token.clone());
        }
    }
    dropped_stale_closed.sort();
    dropped_stale_closed.dedup();
    unhomed_closed.sort();
    unhomed_closed.dedup();

    // Topology from the same device order the owner syncs: live backing ids
    // in native order, the current view, the live full output geometry
    // (hotplug placement facts, never a frame proxy) and the primary flag.
    // `all_monitors` orders primary-first, so primary derives from the
    // `areas.first()` device: managed output order may be reordered relative
    // to the native list and must never decide it.
    let primary_device = areas.first().map(|area| area.device.clone());
    let mut topology: Vec<OutputTopology> = Vec::new();
    let outputs = state.workspaces.output_keys();
    for output in outputs.iter() {
        // Defensive only: keys from `output_keys` are always known, so the
        // `"o?"` fallback never fires on real tables.
        let mapped = state.workspaces.output_token(output);
        if mapped == "o?" {
            topology_complete = false;
            continue;
        }
        let area = areas.iter().find(|a| a.device == *output);
        if area.is_none() {
            topology_complete = false;
        }
        // Defensive only: synced outputs always carry an active view.
        let Some(current_native) = state.workspaces.active_id(output) else {
            topology_complete = false;
            continue;
        };
        topology.push(OutputTopology {
            output: OutputId(mapped),
            native_order: state.workspaces.workspace_ids(output),
            current_native,
            complete: topology_complete,
            geometry: area.map(|a| a.full),
            primary: primary_device.as_deref() == Some(output.as_str()),
        });
    }

    // Bindings mirror actual ownership: Windows workspace ids are both the
    // logical and the native backing id (synchronous native lifecycle, no
    // pending logical tokens; `mint_shared_id` may place one id on several
    // outputs and each placement binds separately). Every listed workspace
    // is managed; displacement rides the owner records; Windows carries no
    // pin source so nothing pins. The carried `mode` drives the core
    // scope-key mapping (`workspace_scope_key`: `shared` vs per-output).
    let mut bindings: Vec<WorkspaceBinding> = Vec::new();
    let displaced: BTreeSet<String> = state
        .workspaces
        .displaced_snapshot()
        .values()
        .flat_map(|record| record.workspace_ids.iter().cloned())
        .collect();
    for output in state.workspaces.output_keys() {
        let mapped = state.workspaces.output_token(&output);
        if mapped == "o?" {
            continue;
        }
        for id in state.workspaces.workspace_ids(&output) {
            bindings.push(WorkspaceBinding {
                logical_id: id.clone(),
                native_id: id.clone(),
                output: OutputId(mapped.clone()),
                pinned: false,
                displaced: displaced.contains(&id),
                managed: true,
            });
        }
    }

    let inventory_complete =
        topology_complete && domains.iter().all(|d| d.readable) && !outputs.is_empty();
    CapturedWorld {
        mode,
        topology,
        bindings,
        domains,
        minimized,
        inventory_complete,
        gesture_barrier: !state.active.is_empty(),
        focused_global: observed
            .iter()
            .find(|w| w.hwnd == foreground_hwnd)
            .map(|w| w.token.clone())
            .unwrap_or_default(),
        default_tiled: state.workspaces.default_tiled(),
        fixed_size_predicate: admission::fixed_predicate_for(state.settings_live.as_ref()),
        migration_source_refill: state
            .settings_live
            .as_ref()
            .map(|live| live.settings.core.migration_source_refill_mode())
            .unwrap_or_default(),
        dropped_stale_closed,
        unhomed_closed,
        held_members,
    }
}

#[cfg(test)]
mod tests {
    use super::super::rmax03_adapter_tests::test_state;
    use super::*;
    use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};
    use crate::workspace::WindowKey;
    use crate::workspace_scope_owner::WorkspaceScopeOwner;
    use tiler_core::workspace_scope::WorkspaceMode;
    use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
    use tiler_core::world_report::{WORLD_CONTRACT_VERSION, WorldReport, validate_report_reason};

    const DEV1: &str = "\\\\.\\DISPLAY1";
    const DEV2: &str = "\\\\.\\DISPLAY2";
    const EXE: &str = "C:\\Apps\\app.exe";

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn win_key(hwnd: u64, pid: u32, creation: &str) -> WindowKey {
        WindowKey {
            hwnd,
            pid,
            creation: creation.to_owned(),
        }
    }

    fn observed_window(
        key: &WindowKey,
        token: &str,
        vis: Rect,
        facts: WindowFacts,
    ) -> ObservedWindow {
        let outer = Rect {
            x: vis.x - 8,
            y: vis.y - 8,
            w: vis.w + 16,
            h: vis.h + 16,
        };
        ObservedWindow {
            hwnd: key.hwnd,
            token: token.to_owned(),
            outer,
            visible: vis,
            insets: FrameInsets::measure(outer, vis),
            identity: ObservedTarget {
                hwnd: key.hwnd,
                pid: key.pid,
                process_creation: key.creation.clone(),
                exe_path: EXE.to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts,
        }
    }

    fn plain_facts() -> WindowFacts {
        WindowFacts {
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
        }
    }

    fn retained_row(
        key: &WindowKey,
        token: &str,
        rect: Option<Rect>,
        maximized: bool,
        fullscreen: bool,
        facts: Option<WindowFacts>,
    ) -> RetainedRow {
        RetainedRow {
            key: key.clone(),
            token: token.to_owned(),
            rect,
            maximized,
            fullscreen,
            facts,
        }
    }

    fn process_identity(key: &WindowKey) -> ProcessIdentity {
        ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: EXE.to_owned(),
        }
    }

    fn hidden_claim(key: &WindowKey, iconic: bool, show_min: bool, show_max: bool) -> HiddenRecord {
        HiddenRecord {
            claim: WindowIdentity {
                hwnd: key.hwnd,
                process: process_identity(key),
                // Well-formed product nonce (16 lowercase hex, nonzero):
                // `capture_world` requires `valid_claim_tag`, matching the
                // production commit-before-hide gate. Malformed tags are
                // covered by dedicated hold fixtures below.
                tag: "0123456789abcdef".to_owned(),
                kind: WindowClaimKind::Product,
                show: WindowShowState {
                    minimized: show_min,
                    maximized: show_max,
                },
            },
            iconic,
        }
    }

    fn two_areas() -> Vec<MonitorArea> {
        vec![
            MonitorArea {
                device: DEV1.to_owned(),
                work: rect(0, 0, 1920, 1040),
                full: rect(0, 0, 1920, 1080),
            },
            MonitorArea {
                device: DEV2.to_owned(),
                work: rect(1920, 0, 1920, 1040),
                full: rect(1920, 0, 1920, 1080),
            },
        ]
    }

    /// Real owner harness plus synced outputs; workspace ids read back from
    /// the actual tables (minted `ws-N`, active-first).
    fn synced_state() -> TileLoop {
        let mut state = test_state();
        state.workspaces.ensure_output(DEV1);
        state.workspaces.ensure_output(DEV2);
        state
    }

    fn ws_id(state: &TileLoop, dev: &str, index: usize) -> String {
        state.workspaces.workspace_ids(dev)[index].clone()
    }

    /// Admit one member through the real tables: committed membership plus
    /// the stable token, stamped lifetime tag and stored identity.
    fn admit(
        state: &mut TileLoop,
        key: &WindowKey,
        dev: &str,
        ws: &str,
        token: &str,
        hidden: bool,
    ) {
        assert!(state.workspaces.assign(key.clone(), dev, ws, hidden));
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state
            .member_identity
            .insert(key.clone(), process_identity(key));
    }

    /// Envelope helper: the sequencer unit owns these fields; fixtures
    /// supply constants to prove the captured pieces validate and the
    /// Windows engine profile accepts them.
    fn enveloped(captured: &CapturedWorld) -> WorldReport {
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: "tiler-windows".to_owned(),
            adapter_incarnation: "test-adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: 1,
            request_seq: 1,
            request_id: "req-1".to_owned(),
            command_seq: None,
            command: None,
            workspace_mode: captured.mode,
            topology: captured.topology.clone(),
            bindings: captured.bindings.clone(),
            domains: captured.domains.clone(),
            minimized: captured.minimized.clone(),
            fixed_size_predicate: captured.fixed_size_predicate,
            migration_source_refill: captured.migration_source_refill,
            default_tiled: captured.default_tiled,
            focused_global: captured.focused_global.clone(),
            inventory_complete: captured.inventory_complete,
            gesture_barrier: captured.gesture_barrier,
            feedback: Vec::new(),
        }
    }

    fn windows_engine() -> WorldEngine {
        WorldEngine::new_with_config(
            "gen-1",
            WorldEngineConfig {
                fixed_send_admission: true,
                maximized_focus_fence: false,
            },
        )
        .expect("valid generation")
    }

    #[test]
    fn output_token_source_is_opaque_and_stable() {
        let state = synced_state();
        // The existing session token source maps native device keys to
        // wire-opaque output ids; unknown outputs fall back to `"o?"`,
        // which capture treats as unemittable (held, never encoded).
        assert_eq!(state.workspaces.output_token(DEV1), "o1");
        assert_eq!(state.workspaces.output_token(DEV2), "o2");
        assert_eq!(state.workspaces.output_token("bogus"), "o?");
        assert!(tiler_core::bounds::is_opaque_id(
            &state.workspaces.output_token(DEV1)
        ));
    }

    #[test]
    fn controlled_two_output_world_assembles_and_engine_accepts() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws3 = ws_id(&state, DEV2, 0);
        let k1 = win_key(1001, 2001, "proc-1");
        let k2 = win_key(1002, 2002, "proc-2");
        admit(&mut state, &k1, DEV1, &ws1, "w1", false);
        admit(&mut state, &k2, DEV2, &ws3, "w2", false);
        state.hint_logged.insert(
            "w1".to_owned(),
            WindowSizeHints {
                min_w: Some(200),
                min_h: None,
                max_w: None,
                max_h: None,
            },
        );
        let observed = vec![
            observed_window(&k1, "w1", rect(100, 100, 800, 600), plain_facts()),
            observed_window(&k2, "w2", rect(2020, 100, 800, 600), plain_facts()),
        ];

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 1001);
        assert!(captured.inventory_complete);
        assert!(captured.held_members.is_empty());
        assert!(captured.minimized.is_empty());
        assert!(!captured.gesture_barrier);
        assert_eq!(captured.focused_global, "w1");
        // Windows carries no adjacency source: production `workspace_domain`
        // emits an empty map and capture withholds the KDE-style
        // computation rather than copying it without native evidence.
        for domain in &captured.domains {
            assert!(domain.readable);
            assert!(domain.hold.is_none());
            assert!(domain.domain.adjacent.is_empty());
        }
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert_eq!(home.focused.0, "w1");
        assert_eq!(home.windows.len(), 1);
        // Last-good constraints ride verified rows; defaults seed the rest.
        assert_eq!(
            home.windows[0].hints,
            WindowSizeHints {
                min_w: Some(200),
                min_h: None,
                max_w: None,
                max_h: None,
            }
        );
        // Topology uses the session token source with primary-first order.
        assert_eq!(captured.topology.len(), 2);
        assert_eq!(captured.topology[0].output.0, "o1");
        assert!(captured.topology[0].primary);
        assert!(!captured.topology[1].primary);
        assert_eq!(captured.topology[0].current_native, ws1);
        assert_eq!(captured.topology[0].geometry, Some(rect(0, 0, 1920, 1080)));
        // Bindings mirror ownership: logical and native are the workspace
        // id, every entry managed, nothing pinned (no Windows pin source).
        for binding in &captured.bindings {
            assert_eq!(binding.logical_id, binding.native_id);
            assert!(binding.managed);
            assert!(!binding.pinned);
        }
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn hidden_rows_carry_verified_overlay_or_hold() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        // h1: stale hidden flag with live fullscreen evidence for the same
        // lifetime: fresh row verifies overlay directly.
        let h1 = win_key(1001, 2001, "proc-1");
        // h2: verified born-fullscreen hold lane, no fresh row.
        let h2 = win_key(1002, 2002, "proc-2");
        // h3: durable maximized preimage only: fullscreen has no verifiable
        // source, so the domain holds rather than defaulting `false`.
        let h3 = win_key(1003, 2003, "proc-3");
        admit(&mut state, &h1, DEV1, &ws1, "w1", true);
        admit(&mut state, &h2, DEV1, &ws1, "w2", true);
        admit(&mut state, &h3, DEV1, &ws2, "w3", true);
        state
            .hidden_claims
            .insert(h1.clone(), hidden_claim(&h1, false, false, false));
        state
            .hidden_claims
            .insert(h2.clone(), hidden_claim(&h2, false, false, false));
        state
            .hidden_claims
            .insert(h3.clone(), hidden_claim(&h3, false, false, true));
        state.born_fullscreen.insert(h2.clone());
        state
            .member_rects
            .insert("w1".to_owned(), rect(101, 101, 800, 600));
        state
            .member_rects
            .insert("w2".to_owned(), rect(102, 102, 800, 600));
        state
            .member_rects
            .insert("w3".to_owned(), rect(103, 103, 800, 600));
        let mut fresh_facts = plain_facts();
        fresh_facts.captionless_fullscreen = true;
        fresh_facts.maximized = true;
        let observed = vec![observed_window(
            &h1,
            "w1",
            rect(0, 0, 1920, 1080),
            fresh_facts,
        )];

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        // ws-1 stays controlled: h1 carries the fresh fullscreen/maximized
        // over its hidden snapshot rect (hidden lane dominates geometry),
        // h2 carries the verified born hold.
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(home.readable);
        assert_eq!(home.windows.len(), 2);
        let r1 = home
            .windows
            .iter()
            .find(|w| w.window.0 == "w1")
            .expect("w1 row");
        assert_eq!(r1.rect, rect(101, 101, 800, 600));
        assert!(r1.fullscreen);
        assert!(r1.maximized);
        assert!(r1.fit_excluded);
        let r2 = home
            .windows
            .iter()
            .find(|w| w.window.0 == "w2")
            .expect("w2 row");
        assert!(r2.fullscreen);
        assert!(r2.floating);
        // ws-2 holds: h3's maximized preimage is verified but its
        // fullscreen is not, so nothing rides and nothing minimizes.
        let held = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws2)
            .expect("ws2 domain");
        assert!(!held.readable);
        assert_eq!(held.hold, Some(HoldReason::Unreadable));
        assert!(held.windows.is_empty());
        assert_eq!(captured.held_members, vec!["w3".to_owned()]);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn iconic_omission_minimizes_and_only_explicit_closes_ride() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let visible_iconic = win_key(1001, 2001, "proc-1");
        let hidden_iconic = win_key(1002, 2002, "proc-2");
        let survivor = win_key(1003, 2003, "proc-3");
        admit(&mut state, &visible_iconic, DEV1, &ws1, "w1", false);
        admit(&mut state, &hidden_iconic, DEV1, &ws1, "w2", true);
        admit(&mut state, &survivor, DEV1, &ws1, "w3", false);
        // Visible iconic: the production iconic arm omits before any
        // retained row, so no row exists for this member.
        state.minimized_omitted.insert(visible_iconic.clone());
        // Hidden iconic: hide-time claim plus iconic refresh; the durable
        // preimage independently records minimized.
        state.hidden_claims.insert(
            hidden_iconic.clone(),
            hidden_claim(&hidden_iconic, true, true, false),
        );
        state.minimized_omitted.insert(hidden_iconic.clone());
        state
            .member_rects
            .insert("w2".to_owned(), rect(120, 120, 800, 600));
        let observed = vec![observed_window(
            &survivor,
            "w3",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];
        // Explicit verified close for a non-live token; omission of the
        // iconic members must never fabricate closes for them.
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(captured.inventory_complete);
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(domain.readable);
        assert_eq!(domain.windows.len(), 1);
        assert_eq!(domain.windows[0].window.0, "w3");
        assert_eq!(captured.minimized.len(), 2);
        assert_eq!(captured.minimized[0].window.0, "w1");
        assert_eq!(captured.minimized[1].window.0, "w2");
        for entry in &captured.minimized {
            assert_eq!(entry.output.0, "o1");
            assert_eq!(entry.workspace.0, ws1);
        }
        assert_eq!(domain.closed.len(), 1);
        assert_eq!(domain.closed[0].0, "w9");
        assert!(captured.dropped_stale_closed.is_empty());
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn frameless_unreadable_holds_instead_of_minimizing() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let unreadable = win_key(1001, 2001, "proc-1");
        let survivor = win_key(1002, 2002, "proc-2");
        admit(&mut state, &unreadable, DEV1, &ws1, "w1", false);
        admit(&mut state, &survivor, DEV1, &ws1, "w2", false);
        // Frameless retained row, not iconic: unreadable evidence with no
        // verifiable frame or flags.
        let retained = vec![retained_row(&unreadable, "w1", None, false, false, None)];
        let observed = vec![observed_window(
            &survivor,
            "w2",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];

        let captured = capture_world(&state, Some(&observed), &retained, &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(!domain.readable);
        assert_eq!(domain.hold, Some(HoldReason::Unreadable));
        assert!(domain.windows.is_empty());
        // Both tokens are withheld from the report: the unreadable member
        // and the verified survivor cleared beside it (never silently
        // dropped).
        assert_eq!(
            captured.held_members,
            vec!["w1".to_owned(), "w2".to_owned()]
        );
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn scope_exclusion_holds_instead_of_authorizing() {
        let mut state = synced_state();
        // Narrow the scope so the stored identities fall outside it.
        state.scope = vec!["other.exe".to_owned()];
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        let visible = win_key(1001, 2001, "proc-1");
        let hidden_born = win_key(1002, 2002, "proc-2");
        let frameless = win_key(1003, 2003, "proc-3");
        admit(&mut state, &visible, DEV1, &ws1, "w1", false);
        admit(&mut state, &hidden_born, DEV1, &ws1, "w2", true);
        admit(&mut state, &frameless, DEV1, &ws2, "w3", false);
        state.hidden_claims.insert(
            hidden_born.clone(),
            hidden_claim(&hidden_born, false, false, false),
        );
        state.born_fullscreen.insert(hidden_born.clone());
        state
            .member_rects
            .insert("w2".to_owned(), rect(102, 102, 800, 600));
        let observed = vec![observed_window(
            &visible,
            "w1",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];
        let retained = vec![retained_row(&frameless, "w3", None, false, false, None)];

        let captured = capture_world(&state, Some(&observed), &retained, &two_areas(), 0);
        assert!(!captured.inventory_complete);
        // Scope exclusion has no window authority lane in the typed report
        // (`fit_excluded` is fit semantics only and never reaches the
        // session), so verified frames must not ride: both ws-1 members
        // hold even with fresh frames and snapshots.
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(!home.readable);
        assert_eq!(home.hold, Some(HoldReason::IncompleteInventory));
        assert!(home.windows.is_empty());
        // Frameless exclusion holds its own domain and never minimizes.
        let held = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws2)
            .expect("ws2 domain");
        assert!(!held.readable);
        assert!(captured.minimized.is_empty());
        assert_eq!(
            captured.held_members,
            vec!["w1".to_owned(), "w2".to_owned(), "w3".to_owned()]
        );
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        // Reply level: excluded scopes publish holds and no row for the
        // affected lifetimes, so no write can be authorized for them.
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                for scope in &reply.scopes {
                    if scope.scope.ends_with(&format!("/{ws1}"))
                        || scope.scope.ends_with(&format!("/{ws2}"))
                    {
                        assert!(
                            matches!(
                                scope.authority,
                                tiler_core::world_report::ScopeAuthority::Hold(_)
                            ),
                            "excluded scope holds: {scope:?}"
                        );
                    }
                }
                for window in &reply.windows {
                    assert!(
                        window.lifetime.0 != "w1"
                            && window.lifetime.0 != "w2"
                            && window.lifetime.0 != "w3",
                        "excluded lifetime publishes no row: {window:?}"
                    );
                }
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn fixed_lanes_stay_distinct_and_never_fabricate() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let auto = win_key(1001, 2001, "proc-1");
        let overridden = win_key(1002, 2002, "proc-2");
        let durable = win_key(1003, 2003, "proc-3");
        let latched_and_won = win_key(1004, 2004, "proc-4");
        let sticky_win = win_key(1005, 2005, "proc-5");
        admit(&mut state, &auto, DEV1, &ws1, "w1", false);
        admit(&mut state, &overridden, DEV1, &ws1, "w2", false);
        admit(&mut state, &durable, DEV1, &ws1, "w3", false);
        admit(&mut state, &latched_and_won, DEV1, &ws1, "w4", false);
        admit(&mut state, &sticky_win, DEV1, &ws1, "w5", false);
        state.fixed_auto_seen.insert(auto.clone());
        state.fixed_tile_overrides.insert(overridden.clone());
        state.fixed_origin_overrides.insert(durable.clone());
        state.fixed_auto_seen.insert(latched_and_won.clone());
        state.fixed_tile_overrides.insert(latched_and_won.clone());
        state.sticky.insert(sticky_win.clone(), true);
        let observed = vec![
            observed_window(&auto, "w1", rect(100, 100, 800, 600), plain_facts()),
            observed_window(&overridden, "w2", rect(110, 110, 800, 600), plain_facts()),
            observed_window(&durable, "w3", rect(120, 120, 800, 600), plain_facts()),
            observed_window(
                &latched_and_won,
                "w4",
                rect(130, 130, 800, 600),
                plain_facts(),
            ),
            observed_window(&sticky_win, "w5", rect(140, 140, 800, 600), plain_facts()),
        ];

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(captured.inventory_complete);
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(domain.readable);
        let row = |id: &str| {
            domain
                .windows
                .iter()
                .find(|w| w.window.0 == id)
                .unwrap_or_else(|| panic!("row {id} rides"))
        };
        // D4 latch: automatic origin floats.
        assert!(row("w1").fixed_auto);
        assert!(row("w1").floating);
        assert!(!row("w1").fixed_suppress);
        // Runtime override: explicit tile win dominates, stays tiled.
        assert!(row("w2").fixed_suppress);
        assert!(!row("w2").fixed_auto);
        assert!(!row("w2").floating);
        // Durable origin override: same dominance without the runtime set.
        assert!(row("w3").fixed_suppress);
        assert!(!row("w3").floating);
        // Stale latch beside an explicit win: the win dominates, automatic
        // origin is never reported alongside suppression.
        assert!(row("w4").fixed_suppress);
        assert!(!row("w4").fixed_auto);
        // Sticky intent: sticky and floating, never automatic.
        assert!(row("w5").sticky);
        assert!(row("w5").floating);
        assert!(!row("w5").fixed_auto);
    }

    #[test]
    fn mismatched_lifetime_or_tag_holds_without_inheriting() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        let ws3 = ws_id(&state, DEV2, 0);
        // Recycled token: the row carries the predecessor's token while the
        // stable member token moved on. Maximized/sticky must not inherit.
        let recycled = win_key(1001, 2001, "proc-1");
        // Recycled process: the hidden claim names a different creation
        // while the member key moved on. Iconic must not minimize this.
        let reused = win_key(1002, 2002, "proc-2");
        // Unstamped lifetime: hidden claim matches but no member tag was
        // ever stamped for this key.
        let untagged = win_key(1003, 2003, "proc-3");
        admit(&mut state, &recycled, DEV1, &ws1, "w9", false);
        admit(&mut state, &reused, DEV1, &ws2, "w2", true);
        admit(&mut state, &untagged, DEV2, &ws3, "w3", true);
        let mut overlaid = plain_facts();
        overlaid.maximized = true;
        let observed = vec![observed_window(
            &recycled,
            "w1",
            rect(100, 100, 800, 600),
            overlaid,
        )];
        let mut stale_claim = hidden_claim(&reused, true, true, false);
        stale_claim.claim.process.process_creation = "proc-other".to_owned();
        state.hidden_claims.insert(reused.clone(), stale_claim);
        state.minimized_omitted.insert(reused.clone());
        state
            .member_rects
            .insert("w2".to_owned(), rect(120, 120, 800, 600));
        state.hidden_claims.insert(
            untagged.clone(),
            hidden_claim(&untagged, false, false, false),
        );
        state.member_tags.remove(&untagged);
        state
            .member_rects
            .insert("w3".to_owned(), rect(130, 130, 800, 600));

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        assert_eq!(captured.domains.len(), 4);
        // Explicit holds for every affected domain, even with empty rows:
        // a recycled token (ws-1), a stale hidden claim (ws-2) and an
        // unstamped lifetime (ws-3) each hold without inheriting flags.
        for ws in [&ws1, &ws2, &ws3] {
            let domain = captured
                .domains
                .iter()
                .find(|d| d.domain.workspace.0 == **ws)
                .unwrap_or_else(|| panic!("domain for {ws} exists"));
            assert!(!domain.readable, "mismatched domain {ws} holds");
            assert!(domain.hold.is_some(), "mismatched domain {ws} names a hold");
            assert!(
                domain.windows.is_empty(),
                "mismatched domain {ws} rides nothing"
            );
        }
        assert_eq!(
            captured.held_members,
            vec!["w2".to_owned(), "w3".to_owned(), "w9".to_owned()]
        );
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn stale_and_unhomed_closes_never_ride() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let live = win_key(1001, 2001, "proc-1");
        admit(&mut state, &live, DEV1, &ws1, "w1", false);
        let observed = vec![observed_window(
            &live,
            "w1",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];
        // Queued close for a live token: stale, drops.
        state.pending_closed.insert(
            "w1".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );
        // Queued close homed on an unknown domain: uncarried, caller-held.
        state.pending_closed.insert(
            "w7".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: "ws-9".to_owned(),
            },
        );
        // An open gesture suspends geometry effects.
        state.active.insert(1001);

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 1001);
        assert!(captured.inventory_complete);
        assert!(captured.gesture_barrier);
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(domain.closed.is_empty());
        assert_eq!(captured.dropped_stale_closed, vec!["w1".to_owned()]);
        assert_eq!(captured.unhomed_closed, vec!["w7".to_owned()]);
    }

    #[test]
    fn hidden_member_without_snapshot_holds_and_is_not_minimized() {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let hidden = win_key(1001, 2001, "proc-1");
        admit(&mut state, &hidden, DEV1, &ws1, "w1", true);
        state
            .hidden_claims
            .insert(hidden.clone(), hidden_claim(&hidden, false, false, false));
        // No snapshot rectangle anywhere: unknown frame, never minimized.

        let captured = capture_world(&state, Some(&[]), &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(!domain.readable);
        assert_eq!(domain.hold, Some(HoldReason::Unreadable));
    }

    #[test]
    fn assembly_is_stable_across_real_mode_shaped_ownership() {
        // Per-output-local and global-unique share the same table shape:
        // the carried mode selects the core scope-key mapping, never the
        // capture shape.
        for mode in [WorkspaceMode::PerOutputLocal, WorkspaceMode::GlobalUnique] {
            let mut state = synced_state();
            state.scope_owner = WorkspaceScopeOwner::adopt_startup(mode);
            let ws1 = ws_id(&state, DEV1, 0);
            let key = win_key(1001, 2001, "proc-1");
            admit(&mut state, &key, DEV1, &ws1, "w1", false);
            let observed = vec![observed_window(
                &key,
                "w1",
                rect(100, 100, 800, 600),
                plain_facts(),
            )];
            let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
            assert_eq!(captured.mode, mode);
            assert!(captured.inventory_complete);
            let report = enveloped(&captured);
            assert_eq!(validate_report_reason(&report), None);
        }
        // Shared-shaped ownership mints one logical id across outputs via
        // the real `mint_shared_id` path; each placement binds separately
        // under the shared scope key.
        let mut state = synced_state();
        state.scope_owner = WorkspaceScopeOwner::adopt_startup(WorkspaceMode::Shared);
        let shared = state
            .workspaces
            .mint_shared_id(&[DEV1.to_owned(), DEV2.to_owned()]);
        assert!(!shared.is_empty());
        let key = win_key(1001, 2001, "proc-1");
        admit(&mut state, &key, DEV1, &shared, "w1", false);
        let observed = vec![observed_window(
            &key,
            "w1",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];
        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 1001);
        assert_eq!(captured.mode, WorkspaceMode::Shared);
        assert!(captured.inventory_complete);
        assert_eq!(
            tiler_core::world_report::workspace_scope_key(
                captured.mode,
                &tiler_core::directional::OutputId("o1".to_owned())
            ),
            "shared"
        );
        assert_eq!(
            tiler_core::world_report::workspace_scope_key(
                WorkspaceMode::PerOutputLocal,
                &tiler_core::directional::OutputId("o1".to_owned())
            ),
            "o1"
        );
        let placements = captured
            .bindings
            .iter()
            .filter(|b| b.logical_id == shared)
            .count();
        assert_eq!(placements, 2);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn outer_gap_insets_exactly_once() {
        // Double-inset guard: capture carries raw work bounds plus the gap,
        // and the core insets once. A lone tiled window must fill the
        // single-inset domain, never the double-inset one.
        let mut state = synced_state();
        state.outer_gap = 12;
        let ws1 = ws_id(&state, DEV1, 0);
        let key = win_key(1001, 2001, "proc-1");
        admit(&mut state, &key, DEV1, &ws1, "w1", false);
        let observed = vec![observed_window(
            &key,
            "w1",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(captured.inventory_complete);
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(domain.readable);
        // Raw work bounds ride; the gap rides separately for the core inset.
        assert_eq!(domain.domain.bounds, rect(0, 0, 1920, 1040));
        assert_eq!(domain.outer_gap, 12);
        assert_eq!(domain.domain.gap, state.inner_gap);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        let once =
            tiler_core::geometry::inset_bounds(rect(0, 0, 1920, 1040), 12).expect("margin fits");
        let twice = tiler_core::geometry::inset_bounds(once, 12).expect("second margin fits");
        assert_ne!(once, twice, "test needs distinct single/double insets");
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                let row = reply
                    .windows
                    .iter()
                    .find(|w| w.lifetime.0 == "w1")
                    .expect("w1 desired row");
                assert_eq!(row.allocation, Some(once));
                assert_eq!(row.effective_frame, Some(once));
                assert_eq!(
                    row.frame_authority,
                    tiler_core::world_report::ScopeAuthority::Controlled
                );
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    fn flagged(mutate: impl FnOnce(&mut WindowFacts)) -> WindowFacts {
        let mut facts = plain_facts();
        mutate(&mut facts);
        facts
    }

    #[test]
    fn inexpressible_facts_hold_without_authority() {
        // Every native safety flag without a typed report lane holds its
        // domain: carrying any of these as tiled would publish controlled
        // frame authority for a window the adapter must not touch.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let cases: Vec<(&str, WindowFacts)> = vec![
            ("w1", flagged(|f| f.cloaked = true)),
            ("w2", flagged(|f| f.elevated = true)),
            ("w3", flagged(|f| f.shell = true)),
            ("w4", flagged(|f| f.tool_window = true)),
            ("w5", flagged(|f| f.owned = true)),
            ("w6", flagged(|f| f.dialog = true)),
            ("w7", flagged(|f| f.no_activate = true)),
            ("w8", flagged(|f| f.visible = false)),
            (
                "w9",
                flagged(|f| {
                    f.minimized = true;
                }),
            ),
        ];
        let mut retained = Vec::new();
        for (index, (token, facts)) in cases.iter().enumerate() {
            let key = win_key(
                1001 + index as u64,
                2001 + index as u32,
                &format!("proc-{index}"),
            );
            admit(&mut state, &key, DEV1, &ws1, token, false);
            retained.push(retained_row(
                &key,
                token,
                Some(rect(100, 100, 800, 600)),
                false,
                false,
                Some(*facts),
            ));
        }

        let captured = capture_world(&state, Some(&[]), &retained, &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(!domain.readable);
        assert_eq!(domain.hold, Some(HoldReason::IncompleteInventory));
        assert!(domain.windows.is_empty());
        let mut expected: Vec<String> =
            cases.iter().map(|(token, _)| (*token).to_owned()).collect();
        expected.sort();
        assert_eq!(captured.held_members, expected);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        // Reply level: the held scope publishes a hold and no row carries
        // controlled frame authority for any affected lifetime.
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                let scope = reply
                    .scopes
                    .iter()
                    .find(|s| s.scope.ends_with(&format!("/{ws1}")))
                    .expect("ws1 scope");
                assert!(
                    matches!(
                        scope.authority,
                        tiler_core::world_report::ScopeAuthority::Hold(_)
                    ),
                    "ineligible scope holds: {scope:?}"
                );
                for window in &reply.windows {
                    assert!(
                        !expected.iter().any(|token| token == &window.lifetime.0),
                        "ineligible lifetime publishes no row: {window:?}"
                    );
                }
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn visible_ineligible_observation_holds() {
        // The visible lane trusts no observed facts blindly: a cloaked live
        // frame holds instead of riding controlled geometry.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let key = win_key(1001, 2001, "proc-1");
        admit(&mut state, &key, DEV1, &ws1, "w1", false);
        let observed = vec![observed_window(
            &key,
            "w1",
            rect(100, 100, 800, 600),
            flagged(|f| f.cloaked = true),
        )];

        let captured = capture_world(&state, Some(&observed), &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        let domain = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(!domain.readable);
        assert_eq!(domain.hold, Some(HoldReason::IncompleteInventory));
        assert!(domain.windows.is_empty());
        assert_eq!(captured.held_members, vec!["w1".to_owned()]);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn overlay_lanes_stay_reserved_and_distinct() {
        // Overlay lanes stay distinct and never blanket-float: maximized
        // keeps its reserved tiled allocation, ordinary fullscreen (tiled
        // or hidden) rides with fullscreen set and unfloated at capture and
        // defers to a writeless exception row at the sweep, and the
        // born-fullscreen hold rides the floating exception lane throughout.
        // Capture-level rows still carry the canonical tile allocation for
        // ordinary overlays (never the compositor-owned maximum frame).
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        let ws3 = ws_id(&state, DEV2, 0);
        let ws4 = ws_id(&state, DEV2, 1);
        let tiled_full = win_key(1001, 2001, "proc-1");
        let tiled_max = win_key(1002, 2002, "proc-2");
        let hidden_full = win_key(1003, 2003, "proc-3");
        let hidden_born = win_key(1004, 2004, "proc-4");
        let plain_float = win_key(1005, 2005, "proc-5");
        admit(&mut state, &tiled_full, DEV2, &ws4, "w1", false);
        admit(&mut state, &tiled_max, DEV1, &ws1, "w2", false);
        admit(&mut state, &hidden_full, DEV1, &ws2, "w3", true);
        admit(&mut state, &hidden_born, DEV2, &ws3, "w4", true);
        admit(&mut state, &plain_float, DEV1, &ws1, "w5", false);
        state.floated.insert(plain_float.clone());
        let mut full_facts = plain_facts();
        full_facts.captionless_fullscreen = true;
        let mut max_facts = plain_facts();
        max_facts.maximized = true;
        state
            .member_rects
            .insert("w1".to_owned(), rect(101, 101, 800, 600));
        state
            .member_rects
            .insert("w2".to_owned(), rect(102, 102, 800, 600));
        state
            .member_rects
            .insert("w3".to_owned(), rect(103, 103, 800, 600));
        state
            .member_rects
            .insert("w4".to_owned(), rect(104, 104, 800, 600));
        state.hidden_claims.insert(
            hidden_full.clone(),
            hidden_claim(&hidden_full, false, false, false),
        );
        state.hidden_claims.insert(
            hidden_born.clone(),
            hidden_claim(&hidden_born, false, false, false),
        );
        state.born_fullscreen.insert(hidden_born.clone());
        let retained = vec![
            retained_row(
                &tiled_full,
                "w1",
                Some(rect(0, 0, 1920, 1080)),
                false,
                true,
                Some(full_facts),
            ),
            retained_row(
                &tiled_max,
                "w2",
                Some(rect(0, 0, 1920, 1080)),
                true,
                false,
                Some(max_facts),
            ),
            retained_row(
                &hidden_full,
                "w3",
                Some(rect(0, 0, 1920, 1080)),
                false,
                true,
                Some(full_facts),
            ),
        ];
        let observed = vec![observed_window(
            &plain_float,
            "w5",
            rect(120, 120, 640, 480),
            plain_facts(),
        )];

        let captured = capture_world(&state, Some(&observed), &retained, &two_areas(), 0);
        assert!(captured.inventory_complete);
        // Each overlay sits alone in its domain so capture lanes are
        // attributable: ordinary overlays ride the reserved tiled lane with
        // the canonical tile allocation (never the compositor-owned maximum
        // frame, never floated); the born hold rides floating.
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws4)
            .expect("ws4 domain");
        assert!(home.readable);
        let r1 = home
            .windows
            .iter()
            .find(|w| w.window.0 == "w1")
            .expect("w1 row");
        assert_eq!(r1.rect, rect(101, 101, 800, 600));
        assert!(r1.fullscreen);
        assert!(!r1.floating);
        assert!(r1.fit_excluded);
        let max_home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(max_home.readable);
        let r2 = max_home
            .windows
            .iter()
            .find(|w| w.window.0 == "w2")
            .expect("w2 row");
        assert_eq!(r2.rect, rect(102, 102, 800, 600));
        assert!(r2.maximized);
        assert!(!r2.floating);
        // The plain intentional float rides the floating lane on its live
        // frame beside the maximized tile.
        let r5 = max_home
            .windows
            .iter()
            .find(|w| w.window.0 == "w5")
            .expect("w5 row");
        assert_eq!(r5.rect, rect(120, 120, 640, 480));
        assert!(r5.floating);
        assert!(!r5.fullscreen);
        // Hidden ordinary fullscreen rides reserved too (tiled lane with
        // fullscreen set); the born hold rides the floating exception lane.
        // Each overlay sits in its own domain so the sweep outcome below is
        // attributable per lane.
        let hid = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws2)
            .expect("ws2 domain");
        assert!(hid.readable);
        let r3 = hid
            .windows
            .iter()
            .find(|w| w.window.0 == "w3")
            .expect("w3 row");
        assert!(r3.fullscreen);
        assert!(!r3.floating);
        let born = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws3)
            .expect("ws3 domain");
        assert!(born.readable);
        let r4 = born
            .windows
            .iter()
            .find(|w| w.window.0 == "w4")
            .expect("w4 row");
        assert!(r4.fullscreen);
        assert!(r4.floating);
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        // Reply level: the sweep owns admission policy. Maximized seeds a
        // controlled tile with its reserved allocation; the plain float
        // seeds a writeless exception row. Fullscreen first-seeds hold
        // under the current shared contract (no explicit exception-behavior
        // selection on the world fresh route): the scope holds and the rows
        // carry holds, so nothing authorizes a frame write. The born
        // dual-flag hold reports geometric exhaustion for the same safe
        // reason. Capture stays truthful in all three lanes (no
        // blanket-floating); liveness of fullscreen first-seeds belongs to
        // a future shared-contract slice, not to capture.
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                let desire = |id: &str| {
                    reply
                        .windows
                        .iter()
                        .find(|w| w.lifetime.0 == id)
                        .unwrap_or_else(|| panic!("row {id} desired"))
                };
                let scope_of = |ws: &str| {
                    reply
                        .scopes
                        .iter()
                        .find(|s| s.scope.ends_with(&format!("/{ws}")))
                        .unwrap_or_else(|| panic!("scope {ws} desired"))
                };
                // Maximized: controlled tile with the reserved allocation.
                let d2 = desire("w2");
                assert_eq!(d2.allocation, Some(rect(8, 8, 1904, 1024)));
                assert_eq!(d2.effective_frame, Some(rect(8, 8, 1904, 1024)));
                assert_eq!(
                    d2.frame_authority,
                    tiler_core::world_report::ScopeAuthority::Controlled
                );
                // Plain float: writeless exception row beside the tile.
                let d5 = desire("w5");
                assert_eq!(d5.allocation, None);
                assert_eq!(
                    d5.frame_authority,
                    tiler_core::world_report::ScopeAuthority::Untouched
                );
                // Ordinary fullscreen (tiled and hidden): held scope with
                // held rows, never controlled authority.
                for (id, ws) in [("w1", ws4.as_str()), ("w3", ws2.as_str())] {
                    let scope = scope_of(ws);
                    assert!(
                        matches!(
                            scope.authority,
                            tiler_core::world_report::ScopeAuthority::Hold(_)
                        ),
                        "fullscreen first-seed scope holds: {scope:?}"
                    );
                    let row = desire(id);
                    assert_eq!(row.allocation, None);
                    assert!(
                        matches!(
                            row.frame_authority,
                            tiler_core::world_report::ScopeAuthority::Hold(_)
                        ),
                        "fullscreen first-seed row holds: {row:?}"
                    );
                }
                // Born dual-flag hold: same safe shape, geometric reason.
                let born_scope = scope_of(ws3.as_str());
                assert_eq!(
                    born_scope.authority,
                    tiler_core::world_report::ScopeAuthority::Hold(
                        tiler_core::world_report::HoldReason::ConstraintClamp
                    )
                );
                let d4 = desire("w4");
                assert_eq!(d4.allocation, None);
                assert!(
                    matches!(
                        d4.frame_authority,
                        tiler_core::world_report::ScopeAuthority::Hold(_)
                    ),
                    "born hold row holds: {d4:?}"
                );
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn sticky_first_exit_stays_floating() {
        // Sticky early-returns in `capture_floating` before the first-exit
        // check, so a sticky first exit stays floating with no extra
        // conjunct; an ordinary visible first exit returns to the tiled lane.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        let sticky_exit = win_key(1001, 2001, "proc-1");
        let plain_exit = win_key(1002, 2002, "proc-2");
        admit(&mut state, &sticky_exit, DEV1, &ws1, "w1", true);
        admit(&mut state, &plain_exit, DEV1, &ws2, "w2", false);
        state.hidden_claims.insert(
            sticky_exit.clone(),
            hidden_claim(&sticky_exit, false, false, false),
        );
        state
            .member_rects
            .insert("w1".to_owned(), rect(101, 101, 800, 600));
        state.sticky.insert(sticky_exit.clone(), true);
        state.born_first_exit.insert(sticky_exit.clone());
        state.born_first_exit.insert(plain_exit.clone());
        // Verifiable fullscreen for the hidden sticky member: the hidden
        // lane holds without a verified fullscreen source, which would mask
        // the floating assertion below.
        let mut sticky_facts = plain_facts();
        sticky_facts.captionless_fullscreen = true;
        let retained = vec![retained_row(
            &sticky_exit,
            "w1",
            Some(rect(0, 0, 1920, 1080)),
            false,
            true,
            Some(sticky_facts),
        )];
        let observed = vec![observed_window(
            &plain_exit,
            "w2",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];

        let captured = capture_world(&state, Some(&observed), &retained, &two_areas(), 0);
        assert!(captured.inventory_complete);
        let hid = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert!(hid.readable);
        let r1 = hid
            .windows
            .iter()
            .find(|w| w.window.0 == "w1")
            .expect("w1 row");
        assert!(r1.floating);
        assert!(r1.sticky);
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws2)
            .expect("ws2 domain");
        assert!(home.readable);
        let r2 = home
            .windows
            .iter()
            .find(|w| w.window.0 == "w2")
            .expect("w2 row");
        assert!(!r2.floating);
    }

    #[test]
    fn enum_failure_holds_populated_inventory() {
        // A failed enumeration (`None`, mirroring `TileLoop::observe`) never
        // encodes as a successful empty: every inventoried scope holds,
        // completeness clears, and bare absence infers no close and no
        // minimize. An explicitly queued close still rides its homed domain
        // beside the hold, per the incomplete-inventory contract.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let member = win_key(1001, 2001, "proc-1");
        admit(&mut state, &member, DEV1, &ws1, "w1", false);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );

        let captured = capture_world(&state, None, &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        for domain in &captured.domains {
            assert!(!domain.readable);
            assert_eq!(domain.hold, Some(HoldReason::IncompleteInventory));
            assert!(domain.windows.is_empty());
        }
        // No close inference: only the explicitly queued close rides.
        let home = captured
            .domains
            .iter()
            .find(|d| d.domain.workspace.0 == ws1)
            .expect("ws1 domain");
        assert_eq!(home.closed.len(), 1);
        assert_eq!(home.closed[0].0, "w9");
        assert_eq!(captured.held_members, vec!["w1".to_owned()]);
        for topology in &captured.topology {
            assert!(!topology.complete);
        }
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
        let mut engine = windows_engine();
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                for scope in &reply.scopes {
                    assert!(
                        matches!(
                            scope.authority,
                            tiler_core::world_report::ScopeAuthority::Hold(_)
                        ),
                        "failed-enum scope holds: {scope:?}"
                    );
                }
                assert!(reply.windows.is_empty());
            }
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn enum_failure_holds_empty_inventory() {
        // Entirely empty managed workspaces hold too: failed observation of
        // nothing is still not a successful empty, so cleanup stays
        // unauthorized.
        let state = synced_state();
        assert!(!state.workspaces.output_keys().is_empty());

        let captured = capture_world(&state, None, &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(!captured.domains.is_empty());
        for domain in &captured.domains {
            assert!(!domain.readable);
            assert_eq!(domain.hold, Some(HoldReason::IncompleteInventory));
            assert!(domain.windows.is_empty());
            assert!(domain.closed.is_empty());
        }
        assert!(captured.minimized.is_empty());
        assert!(captured.held_members.is_empty());
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn monitor_absence_holds_all_domains() {
        // No monitor areas (failed monitor enumeration): every domain holds
        // as unreadable with no primary and no complete topology, even with
        // a live observed member.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let key = win_key(1001, 2001, "proc-1");
        admit(&mut state, &key, DEV1, &ws1, "w1", false);
        let observed = vec![observed_window(
            &key,
            "w1",
            rect(100, 100, 800, 600),
            plain_facts(),
        )];

        let captured = capture_world(&state, Some(&observed), &[], &[], 0);
        assert!(!captured.inventory_complete);
        for domain in &captured.domains {
            assert!(!domain.readable);
            assert_eq!(domain.hold, Some(HoldReason::Unreadable));
            assert!(domain.windows.is_empty());
        }
        assert_eq!(captured.held_members, vec!["w1".to_owned()]);
        for topology in &captured.topology {
            assert!(!topology.complete);
            assert_eq!(topology.geometry, None);
            assert!(!topology.primary);
        }
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }

    #[test]
    fn monitor_reorder_primary_follows_native_order() {
        // Primary derives from the native `areas.first()` device, never from
        // managed output order: reordered areas flip the flag.
        let state = synced_state();
        let mut reordered = two_areas();
        reordered.reverse();
        assert_eq!(reordered[0].device, DEV2);

        let captured = capture_world(&state, Some(&[]), &[], &reordered, 0);
        assert!(captured.inventory_complete);
        let first = captured
            .topology
            .iter()
            .find(|t| t.output.0 == "o1")
            .expect("o1 topology");
        assert!(!first.primary);
        let second = captured
            .topology
            .iter()
            .find(|t| t.output.0 == "o2")
            .expect("o2 topology");
        assert!(second.primary);
    }

    #[test]
    fn hidden_claim_tag_and_identity_hold() {
        // Hidden association needs all three proofs: a well-formed claim tag,
        // a matching claim identity, and a stored member identity naming the
        // same lifetime. A malformed tag, an empty tag, and a drifted stored
        // identity each hold their domain without minimizing.
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        let ws3 = ws_id(&state, DEV2, 0);
        let bad_tag = win_key(1001, 2001, "proc-1");
        let empty_tag = win_key(1002, 2002, "proc-2");
        let drifted = win_key(1003, 2003, "proc-3");
        admit(&mut state, &bad_tag, DEV1, &ws1, "w1", true);
        admit(&mut state, &empty_tag, DEV1, &ws2, "w2", true);
        admit(&mut state, &drifted, DEV2, &ws3, "w3", true);
        let mut malformed = hidden_claim(&bad_tag, false, false, false);
        malformed.claim.tag = "nonce-1".to_owned();
        state.hidden_claims.insert(bad_tag.clone(), malformed);
        let mut empty = hidden_claim(&empty_tag, false, false, false);
        empty.claim.tag = String::new();
        state.hidden_claims.insert(empty_tag.clone(), empty);
        state
            .hidden_claims
            .insert(drifted.clone(), hidden_claim(&drifted, false, false, false));
        // Drift the stored identity away from the member key: the claim
        // itself still matches, but the scope/classification source no
        // longer names this lifetime.
        let mut identity = process_identity(&drifted);
        identity.pid = 9999;
        state.member_identity.insert(drifted.clone(), identity);
        state
            .member_rects
            .insert("w1".to_owned(), rect(101, 101, 800, 600));
        state
            .member_rects
            .insert("w2".to_owned(), rect(102, 102, 800, 600));
        state
            .member_rects
            .insert("w3".to_owned(), rect(103, 103, 800, 600));

        let captured = capture_world(&state, Some(&[]), &[], &two_areas(), 0);
        assert!(!captured.inventory_complete);
        assert!(captured.minimized.is_empty());
        for ws in [&ws1, &ws2, &ws3] {
            let domain = captured
                .domains
                .iter()
                .find(|d| d.domain.workspace.0 == *ws)
                .unwrap_or_else(|| panic!("domain for {ws} exists"));
            assert!(!domain.readable, "unproven hidden domain {ws} holds");
            assert_eq!(domain.hold, Some(HoldReason::IncompleteInventory));
            assert!(domain.windows.is_empty());
        }
        assert_eq!(
            captured.held_members,
            vec!["w1".to_owned(), "w2".to_owned(), "w3".to_owned()]
        );
        let report = enveloped(&captured);
        assert_eq!(validate_report_reason(&report), None);
    }
}
