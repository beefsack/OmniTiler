#![forbid(unsafe_code)]
use omnitiler::tray_endpoint::{FRESHNESS_MS, Snapshot, StateView, TrayError, TrayState};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    contract: Contract,
    routes: Routes,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Contract {
    service: String,
    object: String,
    interface: String,
    method: String,
    signature: String,
    schema: i32,
    #[serde(rename = "generationPattern")]
    generation_pattern: String,
    #[serde(rename = "freshnessMs")]
    freshness_ms: u64,
}

#[derive(Debug, Deserialize)]
struct Routes {
    accepted: Route,
    rejected: Vec<Route>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Route {
    service: String,
    object: String,
    interface: String,
    method: String,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct Event {
    op: String,
    args: Option<Vec<serde_json::Value>>,
    ms: Option<u64>,
    expected: ExpectedState,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ExpectedState {
    owner: bool,
    snapshot: Option<SnapshotExpectation>,
    #[serde(rename = "refreshedAt")]
    refreshed_at: Option<u64>,
    current: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct SnapshotExpectation {
    generation: String,
    revision: i32,
    enabled: bool,
    #[serde(rename = "currentScope")]
    current_scope: String,
    tiled: bool,
    #[serde(rename = "defaultTiled")]
    default_tiled: bool,
}

#[test]
fn fixture_defines_the_fixed_route_and_signature() {
    let fixture = fixture();
    assert_eq!(fixture.contract.service, "com.omnitiler.Tray");
    assert_eq!(fixture.contract.object, "/com/omnitiler/Tray");
    assert_eq!(fixture.contract.interface, "com.omnitiler.Tray1");
    assert_eq!(fixture.contract.method, "PublishSnapshot");
    assert_eq!(fixture.contract.signature, "isibsbb");
    assert_eq!(fixture.contract.schema, 2);
    assert_eq!(
        fixture.contract.generation_pattern,
        "^[a-z0-9-]{1,32}$(?![\\s\\S])"
    );
    assert_eq!(fixture.contract.freshness_ms, FRESHNESS_MS);
    assert_eq!(fixture.routes.accepted.method, "PublishSnapshot");
    assert_eq!(fixture.routes.rejected.len(), 4);
    for route in &fixture.routes.rejected {
        assert_ne!(route, &fixture.routes.accepted);
    }
}

#[test]
fn fixture_vectors_drive_cache_owner_and_freshness_transitions() {
    let fixture = fixture();

    for scenario in fixture.scenarios {
        let mut state = TrayState::default();
        let mut now_ms = 0;

        for event in scenario.events {
            match event.op.as_str() {
                "publish" => apply_publish(&mut state, event.args.expect("publish args"), now_ms),
                "owner-acquired" => state.owner_changed(Some(":kwin")),
                "owner-lost" => state.owner_changed(None),
                "restart" => state = TrayState::default(),
                "advance" => now_ms += event.ms.unwrap_or_default(),
                "transport-failure" => {}
                operation => panic!("unknown fixture operation: {operation}"),
            }

            let view = state.view(now_ms);
            assert_eq!(view, expected_view(event.expected));
        }
    }
}

#[test]
fn semantic_invalid_input_surfaces_invalid_snapshot_and_preserves_trusted_state() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 1, true, String::new(), true, true, 0)
        .unwrap();

    let error = state
        .publish_snapshot(
            2,
            "alpha\n".to_owned(),
            2,
            false,
            String::new(),
            true,
            true,
            1,
        )
        .unwrap_err();
    assert_eq!(error.name(), "com.omnitiler.Tray1.InvalidSnapshot");
    assert_eq!(
        state.view(1).snapshot,
        Some(snapshot("alpha", 1, true, "", true, true))
    );

    let error = state
        .publish_snapshot(
            2,
            "alpha".to_owned(),
            0,
            false,
            String::new(),
            true,
            true,
            2,
        )
        .unwrap_err();
    assert_eq!(error.name(), "com.omnitiler.Tray1.InvalidSnapshot");
    assert_eq!(
        state.view(2).snapshot,
        Some(snapshot("alpha", 1, true, "", true, true))
    );
    assert_eq!(state.view(2).refreshed_at, Some(0));

    // Schema 1 is now invalid (current contract is schema 2).
    let error = state
        .publish_snapshot(1, "alpha".to_owned(), 1, true, String::new(), true, true, 3)
        .unwrap_err();
    assert_eq!(error.name(), "com.omnitiler.Tray1.InvalidSnapshot");
}

#[test]
fn schema2_workspace_fields_drive_menu_state_without_identity_logging() {
    // Fresh snapshot projects tiled/default for the menu; scope is retained
    // but never logged. Stale scope changes still bump revision.
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(
            2,
            "alpha".to_owned(),
            0,
            true,
            "ws-1".to_owned(),
            true,
            true,
            0,
        )
        .unwrap();
    assert_eq!(
        state.view(0).snapshot,
        Some(snapshot("alpha", 0, true, "ws-1", true, true))
    );
    // Same revision, flipped tiled: contradiction revokes (no optimistic
    // menu flip; the menu waits for the next fresh snapshot).
    assert!(
        state
            .publish_snapshot(
                2,
                "alpha".to_owned(),
                0,
                true,
                "ws-1".to_owned(),
                false,
                true,
                1
            )
            .is_err()
    );
    assert_eq!(state.view(1).snapshot, None);
    // Overlong scope refuses without echo.
    let long_scope = "x".repeat(257);
    let error = state
        .publish_snapshot(2, "alpha".to_owned(), 1, true, long_scope, true, true, 2)
        .unwrap_err();
    assert_eq!(error.name(), "com.omnitiler.Tray1.InvalidSnapshot");
}

#[test]
fn stale_lower_revision_is_refused_without_state_change_and_heartbeat_refreshes() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 4, true, String::new(), true, true, 0)
        .unwrap();

    assert!(
        state
            .publish_snapshot(2, "alpha".to_owned(), 3, true, String::new(), true, true, 1)
            .is_err()
    );
    let view = state.view(1);
    assert_eq!(
        view.snapshot,
        Some(snapshot("alpha", 4, true, "", true, true))
    );
    assert_eq!(view.refreshed_at, Some(0));

    state
        .publish_snapshot(2, "alpha".to_owned(), 4, true, String::new(), true, true, 2)
        .unwrap();
    let view = state.view(2);
    assert_eq!(
        view.snapshot,
        Some(snapshot("alpha", 4, true, "", true, true))
    );
    assert_eq!(view.refreshed_at, Some(2));
}

#[test]
fn equal_revision_contradiction_still_revokes_state() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 4, true, String::new(), true, true, 0)
        .unwrap();

    assert!(
        state
            .publish_snapshot(
                2,
                "alpha".to_owned(),
                4,
                false,
                String::new(),
                true,
                true,
                1
            )
            .is_err()
    );
    assert_eq!(state.view(1), empty_view(true));
    assert!(
        state
            .publish_snapshot(2, "alpha".to_owned(), 4, true, String::new(), true, true, 2)
            .is_err()
    );
    assert_eq!(state.view(2), empty_view(true));

    state
        .publish_snapshot(
            2,
            "alpha".to_owned(),
            5,
            false,
            String::new(),
            true,
            true,
            3,
        )
        .unwrap();
    assert_eq!(
        state.view(3).snapshot,
        Some(snapshot("alpha", 5, false, "", true, true))
    );
}

#[test]
fn rejected_generation_transition_quarantines_the_incoming_generation() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 4, true, String::new(), true, true, 0)
        .unwrap();

    assert!(
        state
            .publish_snapshot(2, "beta".to_owned(), 1, false, String::new(), true, true, 1)
            .is_err()
    );
    assert_eq!(state.view(1), empty_view(true));
    assert!(
        state
            .publish_snapshot(2, "beta".to_owned(), 0, false, String::new(), true, true, 2)
            .is_err()
    );
    assert_eq!(state.view(2), empty_view(true));
}

#[test]
fn changing_the_kwin_owner_clears_the_accepted_state_before_reacquisition() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":old"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 1, true, String::new(), true, true, 0)
        .unwrap();

    state.owner_changed(Some(":new"));
    assert_eq!(state.view(1), empty_view(true));

    state
        .publish_snapshot(2, "beta".to_owned(), 7, false, String::new(), true, true, 1)
        .unwrap();
    assert_eq!(
        state.view(1).snapshot,
        Some(snapshot("beta", 7, false, "", true, true))
    );
}

#[test]
fn retired_publisher_generation_cannot_overwrite_new_generation() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "old".to_owned(), 1, true, String::new(), true, true, 0)
        .unwrap();
    state
        .publish_snapshot(2, "new".to_owned(), 0, false, String::new(), true, true, 1)
        .unwrap();

    assert!(
        state
            .publish_snapshot(2, "old".to_owned(), 0, true, String::new(), true, true, 2)
            .is_err()
    );
    assert_eq!(
        state.view(2).snapshot,
        Some(snapshot("new", 0, false, "", true, true))
    );
}

#[test]
fn every_retired_generation_stays_rejected_across_three_generations() {
    let mut state = TrayState::default();
    state.owner_changed(Some(":kwin"));
    state
        .publish_snapshot(2, "alpha".to_owned(), 1, true, String::new(), true, true, 0)
        .unwrap();
    state
        .publish_snapshot(2, "beta".to_owned(), 0, false, String::new(), true, true, 1)
        .unwrap();
    state
        .publish_snapshot(2, "gamma".to_owned(), 0, true, String::new(), true, true, 2)
        .unwrap();

    assert!(
        state
            .publish_snapshot(
                2,
                "alpha".to_owned(),
                0,
                false,
                String::new(),
                true,
                true,
                3
            )
            .is_err()
    );
    assert_eq!(
        state.view(3).snapshot,
        Some(snapshot("gamma", 0, true, "", true, true))
    );
}

#[test]
fn fixture_type_range_and_dispatch_failures_do_not_enter_the_typed_endpoint() {
    let too_large = serde_json::json!(2147483648_i64);
    let revision: Result<i32, _> = too_large.as_i64().unwrap().try_into();
    assert!(revision.is_err());

    let wrong_boolean = serde_json::json!("false");
    assert!(wrong_boolean.as_bool().is_none());
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../../../test-fixtures/tray-bridge-v2.json")).unwrap()
}

fn apply_publish(state: &mut TrayState, args: Vec<serde_json::Value>, now_ms: u64) {
    assert_eq!(args.len(), 7);
    let schema = args[0].as_i64().and_then(|value| value.try_into().ok());
    let generation = args[1].as_str().map(str::to_owned);
    let revision = args[2].as_i64().and_then(|value| value.try_into().ok());
    let enabled = args[3].as_bool();
    let current_scope = args[4].as_str().map(str::to_owned);
    let tiled = args[5].as_bool();
    let default_tiled = args[6].as_bool();

    if let (
        Some(schema),
        Some(generation),
        Some(revision),
        Some(enabled),
        Some(current_scope),
        Some(tiled),
        Some(default_tiled),
    ) = (
        schema,
        generation,
        revision,
        enabled,
        current_scope,
        tiled,
        default_tiled,
    ) {
        let _ = state.publish_snapshot(
            schema,
            generation,
            revision,
            enabled,
            current_scope,
            tiled,
            default_tiled,
            now_ms,
        );
    }
}

fn expected_view(expected: ExpectedState) -> StateView {
    StateView {
        owner: expected.owner,
        snapshot: expected.snapshot.map(|snapshot| Snapshot {
            generation: snapshot.generation,
            revision: snapshot.revision,
            enabled: snapshot.enabled,
            current_scope: snapshot.current_scope,
            tiled: snapshot.tiled,
            default_tiled: snapshot.default_tiled,
        }),
        refreshed_at: expected.refreshed_at,
        current: expected.current,
    }
}

fn snapshot(
    generation: &str,
    revision: i32,
    enabled: bool,
    current_scope: &str,
    tiled: bool,
    default_tiled: bool,
) -> Snapshot {
    Snapshot {
        generation: generation.to_owned(),
        revision,
        enabled,
        current_scope: current_scope.to_owned(),
        tiled,
        default_tiled,
    }
}

fn empty_view(owner: bool) -> StateView {
    StateView {
        owner,
        snapshot: None,
        refreshed_at: None,
        current: false,
    }
}

trait ErrorName {
    fn name(&self) -> String;
}

impl ErrorName for TrayError {
    fn name(&self) -> String {
        zbus::DBusError::name(self).to_string()
    }
}
