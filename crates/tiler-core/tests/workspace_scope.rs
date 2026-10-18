#![forbid(unsafe_code)]
//! Shared workspace scope/order/history/ring/assignment/swap tests.
//!
//! Case mapping to `kwin/tests/workspace-previous-relative.test.ts`:
//! - toggle A<->B twice, no creation (R-WS-08) -> `two_view_toggle_twice`
//! - same activation / output focus alone record nothing (R-WS-08/15)
//!   -> `same_activation_and_focus_alone_record_nothing`
//! - native switches and verified send-follow record alike (R-WS-08)
//!   -> `native_and_send_follow_record_alike`
//! - removed previous invalidates, no recreation/ordinal reuse (R-WS-16)
//!   -> `removed_previous_clears_without_recreation`
//! - surviving empty previous stays valid (R-WS-16)
//!   -> `surviving_empty_previous_stays_valid`
//! - R-WS-15 per-output isolation (local/global-unique) plus one shared
//!   history -> `per_output_isolation_*` / `shared_single_history`
//! - global-unique swap, out-of-scope clear (R-WS-15/16)
//!   -> `global_swap_*`
//! - R-WS-11 wrap incl. trailing empty and ordinals beyond 9, bad deltas
//!   -> `ring_wrap_*` / `bad_delta_refuses`
//! - R-WS-17 hotplug journeys (disconnect records/discards, return never
//!   restores, pre-handler view, in-handler return, shared across hotplug)
//!   -> deferred to stage C (needs displacement mapping); the pure
//!   discard/prime/re-record pieces are pinned here in
//!   `disconnect_discards_and_reconnect_primes_without_consulting`.
//! - shortcut catalog and entry routing rows -> adapter-side, not portable
//!   (Windows: settings shortcut rows plus history ops in the owner).

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::workspace::plan_trailing;
use tiler_core::workspace_scope::{
    DEFAULT_WORKSPACE_MODE, LiveView, MigrationRefill, MigrationRefusal, MigrationRequest,
    PreviousTarget, ScopeHistory, ScopedTopology, TrailingSelection, WORKSPACE_MODE_TOKENS,
    WorkspaceMode, apply_migration, apply_swap_assignments, claim_visible_currents, mode_token,
    parse_workspace_mode, plan_global_swap, plan_migration, rebuild_global_mapping,
    rebuild_local_mapping, rebuild_shared_order, resolve_numbered, resolve_relative_send,
    resolve_relative_step, resolve_trailing, scoped_ring_ids, scoped_trailing_facts,
};

fn topo(outputs: &[&str], live: &[&str], assignments: &[(&str, &[&str])]) -> ScopedTopology {
    ScopedTopology {
        outputs: outputs.iter().map(|s| s.to_string()).collect(),
        live_order: live.iter().map(|s| s.to_string()).collect(),
        assignments: assignments
            .iter()
            .map(|(k, ids)| {
                (
                    k.to_string(),
                    ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )
            })
            .collect(),
        primary: None,
    }
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn ring(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

#[allow(clippy::too_many_arguments)]
fn migration_req(
    mode: WorkspaceMode,
    live: &[&str],
    source: &str,
    target: &str,
    workspace: &str,
    target_current: &str,
    source_current: Option<&str>,
    previous_before: Option<&str>,
    refill: MigrationRefill,
) -> MigrationRequest {
    MigrationRequest {
        mode,
        live: set(live),
        source: source.to_owned(),
        target: target.to_owned(),
        workspace: workspace.to_owned(),
        target_current: target_current.to_owned(),
        source_current: source_current.map(str::to_owned),
        previous_before: previous_before.map(str::to_owned),
        refill,
    }
}

#[test]
fn mode_tokens_default_and_parse() {
    assert_eq!(
        WORKSPACE_MODE_TOKENS,
        ["per-output-local", "global-unique", "shared"]
    );
    assert_eq!(DEFAULT_WORKSPACE_MODE, WorkspaceMode::PerOutputLocal);
    assert_eq!(
        parse_workspace_mode("per-output-local"),
        WorkspaceMode::PerOutputLocal
    );
    assert_eq!(
        parse_workspace_mode("global-unique"),
        WorkspaceMode::GlobalUnique
    );
    assert_eq!(parse_workspace_mode("shared"), WorkspaceMode::Shared);
    assert_eq!(parse_workspace_mode("bogus"), WorkspaceMode::PerOutputLocal);
    assert_eq!(
        mode_token(WorkspaceMode::PerOutputLocal),
        "per-output-local"
    );
    assert_eq!(mode_token(WorkspaceMode::GlobalUnique), "global-unique");
    assert_eq!(mode_token(WorkspaceMode::Shared), "shared");
}

#[test]
fn rebuild_local_adopts_unassigned_onto_primary_and_retains_removed() {
    let topology = topo(
        &["out-l"],
        &["ws-1", "ws-2", "ws-new"],
        &[("out-l", &["ws-1"]), ("out-gone", &["ws-2", "ws-dead"])],
    );
    let rebuilt = rebuild_local_mapping(&topology);
    // ws-2 stays retained on its removed origin (displacement moves it as a
    // unit later), so only the truly unassigned ws-new adopts onto primary.
    assert_eq!(
        rebuilt.mapping.get("out-l"),
        Some(&ring(&["ws-1", "ws-new"]))
    );
    assert_eq!(rebuilt.mapping.get("out-gone"), Some(&ring(&["ws-2"])));
    assert_eq!(rebuilt.adopted, ring(&["ws-new"]));
}

#[test]
fn rebuild_global_matches_local_shape_and_shared_returns_live_order() {
    let topology = topo(
        &["out-l", "out-r"],
        &["ws-a", "ws-b"],
        &[("out-l", &["ws-a"])],
    );
    let local = rebuild_local_mapping(&topology);
    let global = rebuild_global_mapping(&topology);
    assert_eq!(local.mapping, global.mapping);
    assert_eq!(global.mapping.get("out-l"), Some(&ring(&["ws-a", "ws-b"])));
    assert_eq!(
        rebuild_shared_order(&topology.live_order),
        ring(&["ws-a", "ws-b"])
    );
}

#[test]
fn scoped_rings_cover_local_global_shared() {
    let topology = topo(
        &["out-l", "out-r"],
        &["ws-a", "ws-b", "ws-c"],
        &[("out-l", &["ws-a", "ws-b"]), ("out-r", &["ws-c"])],
    );
    let rebuilt = rebuild_local_mapping(&topology);
    let shared = rebuild_shared_order(&topology.live_order);
    assert_eq!(
        scoped_ring_ids(
            WorkspaceMode::PerOutputLocal,
            &rebuilt.mapping,
            &shared,
            "out-l",
            &topology.live_order
        ),
        ring(&["ws-a", "ws-b"])
    );
    assert_eq!(
        scoped_ring_ids(
            WorkspaceMode::GlobalUnique,
            &rebuilt.mapping,
            &shared,
            "out-r",
            &topology.live_order
        ),
        ring(&["ws-c"])
    );
    assert_eq!(
        scoped_ring_ids(
            WorkspaceMode::Shared,
            &rebuilt.mapping,
            &shared,
            "out-l",
            &topology.live_order
        ),
        ring(&["ws-a", "ws-b", "ws-c"])
    );
    assert!(
        scoped_ring_ids(
            WorkspaceMode::PerOutputLocal,
            &rebuilt.mapping,
            &shared,
            "out-gone",
            &topology.live_order
        )
        .is_empty()
    );
}

#[test]
fn numbered_resolution_is_positional_and_zero_refuses() {
    let ring = ring(&["ws-1", "ws-2"]);
    assert_eq!(resolve_numbered(&ring, 1).as_deref(), Some("ws-1"));
    assert_eq!(resolve_numbered(&ring, 2).as_deref(), Some("ws-2"));
    assert_eq!(resolve_numbered(&ring, 0), None);
    assert_eq!(resolve_numbered(&ring, 3), None);
    assert_eq!(resolve_numbered(&ring, 10), None);
}

#[test]
fn two_view_toggle_twice() {
    // R-WS-08: visit A then B; previous toggles B->A then A->B.
    let mut history = ScopeHistory::new();
    let scope = ring(&["ws-1", "ws-2"]);
    assert!(!history.observe("out-1", "ws-1", &scope));
    assert!(history.observe("out-1", "ws-2", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-2"),
        PreviousTarget::Select("ws-1".to_owned())
    );
    assert!(history.observe("out-1", "ws-1", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-1"),
        PreviousTarget::Select("ws-2".to_owned())
    );
    assert!(history.observe("out-1", "ws-2", &scope));
    assert_eq!(history.previous("out-1").as_deref(), Some("ws-1"));
}

#[test]
fn same_activation_and_focus_alone_record_nothing() {
    // R-WS-08/15: same-workspace activation never records; focusing another
    // output without changing a view leaves every history untouched.
    let mut history = ScopeHistory::new();
    let scope_l = ring(&["ws-1", "ws-2"]);
    let scope_r = ring(&["ws-3", "ws-4"]);
    history.prime("out-l", "ws-1");
    history.prime("out-r", "ws-3");
    assert!(!history.observe("out-l", "ws-1", &scope_l));
    assert!(!history.observe("out-l", "ws-1", &scope_l));
    assert_eq!(history.previous("out-l"), None);
    assert!(!history.observe("out-r", "ws-3", &scope_r));
    assert_eq!(history.previous("out-r"), None);
    assert!(history.observe("out-l", "ws-2", &scope_l));
    // Output focus alone: re-observe both unchanged views.
    assert!(!history.observe("out-l", "ws-2", &scope_l));
    assert!(!history.observe("out-r", "ws-3", &scope_r));
    assert_eq!(history.previous("out-l").as_deref(), Some("ws-1"));
    assert_eq!(history.previous("out-r"), None);
}

#[test]
fn native_and_send_follow_record_alike() {
    // R-WS-08: a native shell switch and a verified send-follow arrival both
    // record as actual observations through the same seam.
    let mut history = ScopeHistory::new();
    let scope = ring(&["ws-1", "ws-2"]);
    history.prime("out-1", "ws-1");
    assert!(history.observe("out-1", "ws-2", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-2"),
        PreviousTarget::Select("ws-1".to_owned())
    );
    assert!(history.observe("out-1", "ws-1", &scope));
    assert!(history.observe("out-1", "ws-2", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-2"),
        PreviousTarget::Select("ws-1".to_owned())
    );
}

#[test]
fn removed_previous_clears_without_recreation() {
    // R-WS-16: the visited id is removed by lifecycle; toggle is a no-op,
    // creates nothing, and never reinterprets an ordinal.
    let mut history = ScopeHistory::new();
    history.prime("out-1", "ws-1");
    assert!(history.observe("out-1", "ws-2", &ring(&["ws-1", "ws-2"])));
    let pruned_scope = ring(&["ws-2"]);
    assert_eq!(
        history.resolve_previous_for("out-1", &pruned_scope, "ws-2"),
        PreviousTarget::NoHistory
    );
    assert_eq!(history.previous("out-1"), None);
    // A later removal-shaped observation from the pruned scope records anew
    // only on a real change, never by resurrecting ws-1.
    assert!(!history.observe("out-1", "ws-2", &pruned_scope));
    assert_eq!(history.previous("out-1"), None);
}

#[test]
fn surviving_empty_previous_stays_valid() {
    // R-WS-16: a surviving empty id (unoccupied but live and scoped) is a
    // valid toggle target.
    let mut history = ScopeHistory::new();
    let scope = ring(&["ws-1", "ws-2"]);
    history.prime("out-1", "ws-2");
    assert!(history.observe("out-1", "ws-1", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-1"),
        PreviousTarget::Select("ws-2".to_owned())
    );
}

#[test]
fn already_there_is_a_no_op_target() {
    let mut history = ScopeHistory::new();
    let scope = ring(&["ws-1", "ws-2"]);
    history.prime("out-1", "ws-1");
    assert!(history.observe("out-1", "ws-2", &scope));
    assert_eq!(
        history.resolve_previous_for("out-1", &scope, "ws-1"),
        PreviousTarget::AlreadyThere("ws-1".to_owned())
    );
}

#[test]
fn per_output_isolation_local() {
    // R-WS-15 local leg: L WS1->WS2 then an R change; toggling on L shows
    // WS1 with R untouched, and vice versa.
    let mut history = ScopeHistory::new();
    let scope_l = ring(&["ws-1", "ws-2"]);
    let scope_r = ring(&["ws-3", "ws-4"]);
    history.prime("out-l", "ws-1");
    history.prime("out-r", "ws-3");
    assert!(history.observe("out-l", "ws-2", &scope_l));
    assert!(history.observe("out-r", "ws-4", &scope_r));
    assert_eq!(
        history.resolve_previous_for("out-l", &scope_l, "ws-2"),
        PreviousTarget::Select("ws-1".to_owned())
    );
    assert!(history.observe("out-l", "ws-1", &scope_l));
    assert_eq!(
        history.resolve_previous_for("out-l", &scope_l, "ws-1"),
        PreviousTarget::Select("ws-2".to_owned())
    );
    assert_eq!(
        history.resolve_previous_for("out-r", &scope_r, "ws-4"),
        PreviousTarget::Select("ws-3".to_owned())
    );
}

#[test]
fn per_output_isolation_global_unique() {
    // R-WS-15 global-unique leg: same per-output history isolation as local.
    let mut history = ScopeHistory::new();
    let scope_l = ring(&["ws-1", "ws-2"]);
    let scope_r = ring(&["ws-3", "ws-4"]);
    history.prime("out-l", "ws-1");
    history.prime("out-r", "ws-3");
    assert!(history.observe("out-l", "ws-2", &scope_l));
    assert!(history.observe("out-r", "ws-4", &scope_r));
    assert_eq!(history.previous("out-l").as_deref(), Some("ws-1"));
    assert_eq!(history.previous("out-r").as_deref(), Some("ws-3"));
    assert_eq!(
        history.resolve_previous_for("out-l", &scope_l, "ws-2"),
        PreviousTarget::Select("ws-1".to_owned())
    );
    assert_eq!(
        history.resolve_previous_for("out-r", &scope_r, "ws-4"),
        PreviousTarget::Select("ws-3".to_owned())
    );
}

#[test]
fn shared_single_history() {
    // R-WS-15 shared leg: WS1->WS2->WS3 then previous twice shows WS2, WS3.
    let mut history = ScopeHistory::new();
    let live = ring(&["ws-1", "ws-2", "ws-3"]);
    history.prime_shared("ws-1");
    assert!(history.observe_shared(&live, "ws-2"));
    assert!(history.observe_shared(&live, "ws-3"));
    assert_eq!(history.shared_previous().as_deref(), Some("ws-2"));
    assert_eq!(
        history.resolve_shared_for(&live, "ws-3"),
        PreviousTarget::Select("ws-2".to_owned())
    );
    assert!(history.observe_shared(&live, "ws-2"));
    assert_eq!(history.shared_previous().as_deref(), Some("ws-3"));
    assert_eq!(
        history.resolve_shared_for(&live, "ws-2"),
        PreviousTarget::Select("ws-3".to_owned())
    );
    // Removing the remembered id itself clears without recreation.
    let without_previous = ring(&["ws-1", "ws-2"]);
    assert_eq!(
        history.resolve_shared_for(&without_previous, "ws-2"),
        PreviousTarget::NoHistory
    );
    assert_eq!(history.shared_previous(), None);
}

#[test]
fn global_swap_plans_visible_elsewhere_exchange() {
    // Selecting a target visible on R swaps the two views.
    let current: BTreeMap<String, String> = [
        ("out-l".to_owned(), "ws-a".to_owned()),
        ("out-r".to_owned(), "ws-b".to_owned()),
    ]
    .into_iter()
    .collect();
    let plan = plan_global_swap(&current, "out-l", "ws-b").expect("swap");
    assert_eq!(plan.active_current, "ws-a");
    assert_eq!(plan.other_output, "out-r");
    let mut mapping: BTreeMap<String, Vec<String>> = BTreeMap::new();
    mapping.insert("out-l".to_owned(), ring(&["ws-a"]));
    mapping.insert("out-r".to_owned(), ring(&["ws-b"]));
    let swapped = apply_swap_assignments(&mapping, &plan);
    assert_eq!(swapped.get("out-l"), Some(&ring(&["ws-b"])));
    assert_eq!(swapped.get("out-r"), Some(&ring(&["ws-a"])));
}

#[test]
fn global_swap_refuses_without_visible_elsewhere() {
    let current: BTreeMap<String, String> = [
        ("out-l".to_owned(), "ws-a".to_owned()),
        ("out-r".to_owned(), "ws-c".to_owned()),
    ]
    .into_iter()
    .collect();
    assert_eq!(plan_global_swap(&current, "out-l", "ws-b"), None);
    assert_eq!(plan_global_swap(&current, "out-l", "ws-a"), None);
    assert_eq!(plan_global_swap(&current, "out-gone", "ws-c"), None);
}

#[test]
fn swapped_out_previous_clears_without_recreation() {
    // After a visible-elsewhere swap, the swapped-out source left the
    // active scope: toggle clears and creates nothing.
    let mut history = ScopeHistory::new();
    history.prime("out-l", "ws-a");
    assert!(history.observe("out-l", "ws-b", &ring(&["ws-a", "ws-b"])));
    let post_swap_scope = ring(&["ws-b"]);
    assert_eq!(
        history.resolve_previous_for("out-l", &post_swap_scope, "ws-b"),
        PreviousTarget::NoHistory
    );
    assert_eq!(history.previous("out-l"), None);
}

#[test]
fn ring_wraps_both_directions_including_trailing_and_beyond_nine() {
    // R-WS-11: ten occupied plus trailing empty; next lands on the
    // trailing, wraps to first, previous wraps back. Pure resolution
    // creates nothing by construction.
    let mut ids: Vec<String> = (1..=10).map(|n| format!("ws-{n}")).collect();
    ids.push("ws-trail".to_owned());
    for mode in [
        WorkspaceMode::PerOutputLocal,
        WorkspaceMode::GlobalUnique,
        WorkspaceMode::Shared,
    ] {
        let ring_ids = scoped_ring_ids(
            mode,
            &BTreeMap::from([("out-1".to_owned(), ids.clone())]),
            &ids,
            "out-1",
            &ids,
        );
        assert_eq!(ring_ids.len(), 11);
        assert_eq!(
            resolve_relative_step(&ring_ids, "ws-10", 1).as_deref(),
            Some("ws-trail")
        );
        assert_eq!(
            resolve_relative_step(&ring_ids, "ws-trail", 1).as_deref(),
            Some("ws-1")
        );
        assert_eq!(
            resolve_relative_step(&ring_ids, "ws-1", -1).as_deref(),
            Some("ws-trail")
        );
        assert_eq!(
            resolve_relative_send(&ring_ids, "ws-1", -1).as_deref(),
            Some("ws-trail")
        );
    }
}

#[test]
fn bad_delta_refuses() {
    let ring = ring(&["ws-1", "ws-2"]);
    assert_eq!(resolve_relative_step(&ring, "ws-1", 0), None);
    assert_eq!(resolve_relative_step(&ring, "ws-1", 2), None);
    assert_eq!(resolve_relative_step(&ring, "ws-gone", 1), None);
    assert_eq!(resolve_relative_step(&[], "ws-1", 1), None);
}

#[test]
fn trailing_reuses_live_empty_else_reports_creation() {
    let ring = ring(&["ws-1", "ws-2"]);
    assert_eq!(
        resolve_trailing(&ring, &set(&["ws-1"])),
        TrailingSelection::Reuse("ws-2".to_owned())
    );
    assert_eq!(
        resolve_trailing(&ring, &set(&["ws-1", "ws-2"])),
        TrailingSelection::Create
    );
    assert_eq!(
        resolve_trailing(&ring, &set(&["ws-1"])),
        TrailingSelection::Reuse("ws-2".to_owned())
    );
}

#[test]
fn scoped_trailing_lifecycle_reuses_plan_trailing() {
    // Per-output facts routed through the shared plan_trailing: literal trailing empty
    // retained, invisible non-final empties removed, occupied/visible/
    // retained protected, minimum-two floor holds.
    let scope = ring(&["ws-1", "ws-2", "ws-3", "ws-4"]);
    let facts = scoped_trailing_facts(&scope, &set(&["ws-4"]), &set(&["ws-1"]), &set(&[]));
    let plan = plan_trailing(&facts);
    assert_eq!(plan.remove, vec![1, 2]);
    assert!(!plan.need_append);
    let pair = ring(&["ws-1", "ws-4"]);
    let facts = scoped_trailing_facts(&pair, &set(&["ws-4"]), &set(&["ws-1"]), &set(&[]));
    let plan = plan_trailing(&facts);
    assert!(plan.remove.is_empty() && !plan.need_append);
    let facts = scoped_trailing_facts(&pair, &set(&["ws-4"]), &set(&["ws-1", "ws-4"]), &set(&[]));
    let plan = plan_trailing(&facts);
    assert!(plan.remove.is_empty() && plan.need_append);
    let retained = scoped_trailing_facts(&scope, &set(&["ws-4"]), &set(&["ws-1"]), &set(&["ws-2"]));
    let plan = plan_trailing(&retained);
    assert_eq!(plan.remove, vec![2]);
}

#[test]
fn migration_moves_after_target_current_with_last_remaining_refill() {
    let mapping: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-l".to_owned(), ring(&["ws-1", "ws-2"])),
        ("out-r".to_owned(), ring(&["ws-3"])),
    ]);
    let plan = plan_migration(
        &mapping,
        &migration_req(
            WorkspaceMode::PerOutputLocal,
            &["ws-1", "ws-2", "ws-3"],
            "out-l",
            "out-r",
            "ws-2",
            "ws-3",
            Some("ws-2"),
            Some("ws-1"),
            MigrationRefill::LastRemaining,
        ),
    )
    .expect("plan");
    assert_eq!(plan.insert_after, "ws-3");
    assert_eq!(plan.refill.as_deref(), Some("ws-1"));
    assert!(!plan.invalidate_previous_in_source);
    let applied = apply_migration(&mapping, &plan);
    assert_eq!(applied.get("out-l"), Some(&ring(&["ws-1"])));
    assert_eq!(applied.get("out-r"), Some(&ring(&["ws-3", "ws-2"])));
}

#[test]
fn migration_refill_prefers_live_current_then_mru_then_last() {
    let mapping: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-l".to_owned(), ring(&["ws-1", "ws-2", "ws-4"])),
        ("out-r".to_owned(), ring(&["ws-3"])),
    ]);
    let live = ["ws-1", "ws-2", "ws-3", "ws-4"];
    // Still-scoped live source current wins in both refill modes.
    for refill in [
        MigrationRefill::LastRemaining,
        MigrationRefill::MostRecentlyUsed,
    ] {
        let plan = plan_migration(
            &mapping,
            &migration_req(
                WorkspaceMode::GlobalUnique,
                &live,
                "out-l",
                "out-r",
                "ws-4",
                "ws-3",
                Some("ws-1"),
                Some("ws-2"),
                refill,
            ),
        )
        .expect("plan");
        assert_eq!(plan.refill.as_deref(), Some("ws-1"));
        assert!(!plan.invalidate_previous_in_source);
    }
    // MRU uses the snapshotted previous while it survives the source ring.
    let plan = plan_migration(
        &mapping,
        &migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-4",
            "ws-3",
            Some("ws-9"),
            Some("ws-2"),
            MigrationRefill::MostRecentlyUsed,
        ),
    )
    .expect("plan");
    assert_eq!(plan.refill.as_deref(), Some("ws-2"));
    // Moved previous id invalidates; empty source keeps no refill.
    let plan = plan_migration(
        &BTreeMap::from([
            ("out-l".to_owned(), ring(&["ws-4"])),
            ("out-r".to_owned(), ring(&["ws-3"])),
        ]),
        &migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-4",
            "ws-3",
            None,
            Some("ws-4"),
            MigrationRefill::MostRecentlyUsed,
        ),
    )
    .expect("plan");
    assert_eq!(plan.refill, None);
    assert!(plan.invalidate_previous_in_source);
}

#[test]
fn migration_refusals() {
    let mapping: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-l".to_owned(), ring(&["ws-1", "ws-2"])),
        ("out-r".to_owned(), ring(&["ws-3"])),
    ]);
    let live = ["ws-1", "ws-2", "ws-3"];
    let base_req = || {
        migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-2",
            "ws-3",
            None,
            None,
            MigrationRefill::LastRemaining,
        )
    };
    let base = || plan_migration(&mapping, &base_req());
    assert!(base().is_ok());
    let refused = |request: &MigrationRequest| plan_migration(&mapping, request);
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::Shared,
            &live,
            "out-l",
            "out-r",
            "ws-2",
            "ws-3",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::SharedMode)
    );
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-l",
            "ws-2",
            "ws-1",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::SameOutput)
    );
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-gone",
            "ws-2",
            "ws-3",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::UnknownOutput)
    );
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-gone",
            "ws-3",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::UnknownWorkspace)
    );
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-3",
            "ws-3",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::OutOfScope)
    );
    let dup: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-l".to_owned(), ring(&["ws-1", "ws-2"])),
        ("out-r".to_owned(), ring(&["ws-2", "ws-3"])),
    ]);
    assert_eq!(
        plan_migration(
            &dup,
            &migration_req(
                WorkspaceMode::PerOutputLocal,
                &live,
                "out-l",
                "out-r",
                "ws-2",
                "ws-3",
                None,
                None,
                MigrationRefill::LastRemaining,
            ),
        ),
        Err(MigrationRefusal::DuplicateTarget)
    );
    assert_eq!(
        refused(&migration_req(
            WorkspaceMode::PerOutputLocal,
            &live,
            "out-l",
            "out-r",
            "ws-2",
            "ws-gone",
            None,
            None,
            MigrationRefill::LastRemaining,
        )),
        Err(MigrationRefusal::TargetCurrentUnknown)
    );
}

#[test]
fn migration_never_refills_dead_ids_and_refuses_dead_target_current() {
    // Stale mapping source [ws-1, ws-dead] with live {ws-1, ws-2, ws-3}:
    // the dead id is filtered from the remaining ring, so it never refills
    // as last, MRU, or live-current, and a dead target current refuses.
    let mapping: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-l".to_owned(), ring(&["ws-1", "ws-dead"])),
        ("out-r".to_owned(), ring(&["ws-3", "ws-dead"])),
    ]);
    let live = ["ws-1", "ws-2", "ws-3"];
    for refill in [
        MigrationRefill::LastRemaining,
        MigrationRefill::MostRecentlyUsed,
    ] {
        let plan = plan_migration(
            &mapping,
            &migration_req(
                WorkspaceMode::PerOutputLocal,
                &live,
                "out-l",
                "out-r",
                "ws-1",
                "ws-3",
                Some("ws-dead"),
                Some("ws-dead"),
                refill,
            ),
        )
        .expect("plan");
        assert_eq!(plan.refill, None);
    }
    assert_eq!(
        plan_migration(
            &mapping,
            &migration_req(
                WorkspaceMode::PerOutputLocal,
                &live,
                "out-l",
                "out-r",
                "ws-1",
                "ws-dead",
                None,
                None,
                MigrationRefill::LastRemaining,
            ),
        ),
        Err(MigrationRefusal::TargetCurrentUnknown)
    );
}

#[test]
fn observe_all_reports_primed_baseline_transitions() {
    // A re-primed baseline still reports the next actual transition even
    // when the recorded previous already names that id; an unchanged view
    // reports nothing.
    let mut history = ScopeHistory::new();
    let scope = ring(&["ws-a", "ws-b", "ws-c"]);
    history.prime("out-1", "ws-a");
    assert!(history.observe("out-1", "ws-b", &scope));
    assert!(history.observe("out-1", "ws-c", &scope));
    assert_eq!(history.previous("out-1").as_deref(), Some("ws-b"));
    history.prime("out-1", "ws-b");
    let mut live: BTreeMap<String, LiveView> = BTreeMap::new();
    live.insert(
        "out-1".to_owned(),
        LiveView {
            current: "ws-c".to_owned(),
            scope: scope.clone(),
        },
    );
    let recorded = history.observe_all(&live);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].previous, "ws-b");
    assert_eq!(recorded[0].current, "ws-c");
    assert!(history.observe_all(&live).is_empty());
}

#[test]
fn observe_all_records_changes_and_discards_disconnected() {
    // Batch observation: real changes record, unchanged views do not, and a
    // disconnected output loses its history.
    let mut history = ScopeHistory::new();
    let mut live: BTreeMap<String, LiveView> = BTreeMap::new();
    live.insert(
        "out-l".to_owned(),
        LiveView {
            current: "ws-1".to_owned(),
            scope: ring(&["ws-1", "ws-2"]),
        },
    );
    live.insert(
        "out-r".to_owned(),
        LiveView {
            current: "ws-3".to_owned(),
            scope: ring(&["ws-3", "ws-4"]),
        },
    );
    assert!(history.observe_all(&live).is_empty());
    live.get_mut("out-l").unwrap().current = "ws-2".to_owned();
    let recorded = history.observe_all(&live);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].output, "out-l");
    assert_eq!(recorded[0].previous, "ws-1");
    assert_eq!(recorded[0].current, "ws-2");
    live.remove("out-r");
    assert!(history.observe_all(&live).is_empty());
    assert_eq!(history.previous("out-r"), None);
    assert_eq!(history.baseline("out-r"), None);
    assert_eq!(history.previous("out-l").as_deref(), Some("ws-1"));
}

#[test]
fn disconnect_discards_and_reconnect_primes_without_consulting() {
    // Pure pieces of R-WS-17: disconnect drops the output history,
    // reconnect only re-primes the baseline (never restores history), and
    // later observed changes record anew.
    let mut history = ScopeHistory::new();
    let scope_l = ring(&["ws-1", "ws-2"]);
    let scope_r = ring(&["ws-d", "ws-4"]);
    history.prime("out-l", "ws-1");
    history.prime("out-r", "ws-d");
    assert!(history.observe("out-l", "ws-2", &scope_l));
    assert!(history.observe("out-r", "ws-4", &scope_r));
    history.discard_output("out-r");
    assert_eq!(history.previous("out-r"), None);
    assert_eq!(history.baseline("out-r"), None);
    // Reconnect return: prime only, no history consulted or restored.
    history.prime("out-r", "ws-d");
    assert_eq!(history.previous("out-r"), None);
    assert_eq!(
        history.resolve_previous_for("out-r", &scope_r, "ws-d"),
        PreviousTarget::NoHistory
    );
    // A later observed change records from the primed baseline.
    assert!(history.observe("out-r", "ws-4", &scope_r));
    assert_eq!(history.previous("out-r").as_deref(), Some("ws-d"));
}

#[test]
fn claim_visible_currents_partitions_shared_and_distinct() {
    let assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let live = ring(&["ws-1", "ws-2", "ws-3"]);
    // Primary first: the shared current stays under the primary; the
    // secondary with a distinct current owns it.
    let (claimed, converting) = claim_visible_currents(
        &assignments,
        &[
            ("out-1".to_owned(), "ws-1".to_owned()),
            ("out-2".to_owned(), "ws-1".to_owned()),
            ("out-3".to_owned(), "ws-2".to_owned()),
        ],
        &live,
    );
    assert_eq!(claimed.get("out-1"), Some(&ring(&["ws-1"])));
    assert_eq!(claimed.get("out-2"), None);
    assert_eq!(claimed.get("out-3"), Some(&ring(&["ws-2"])));
    assert_eq!(
        converting,
        vec![("out-2".to_owned(), "ws-1".to_owned())],
        "only the second shower of a shared current converts"
    );
}

#[test]
fn claim_visible_currents_never_steals_or_duplicates() {
    let assignments: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("out-1".to_owned(), ring(&["ws-1", "ws-2"])),
        ("out-2".to_owned(), ring(&["ws-3"])),
    ]);
    let live = ring(&["ws-1", "ws-2", "ws-3"]);
    // Every current is already owned: nothing moves, nothing converts.
    let (claimed, converting) = claim_visible_currents(
        &assignments,
        &[
            ("out-1".to_owned(), "ws-1".to_owned()),
            ("out-2".to_owned(), "ws-1".to_owned()),
        ],
        &live,
    );
    assert_eq!(claimed, assignments);
    assert_eq!(
        converting,
        vec![("out-2".to_owned(), "ws-1".to_owned())],
        "showing a foreign-owned current converts without stealing it"
    );
    // Unknown currents claim nothing.
    let (claimed, converting) = claim_visible_currents(
        &BTreeMap::new(),
        &[("out-1".to_owned(), "ws-gone".to_owned())],
        &live,
    );
    assert!(claimed.values().all(|ids| ids.is_empty()));
    assert!(converting.is_empty());
}
