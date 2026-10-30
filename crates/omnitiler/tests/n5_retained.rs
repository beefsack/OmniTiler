//! N5 failing-first: producer-shaped raw capture with the REQUIRED
//! native `hidden` lane plus dialog/transient admission through the real
//! broker normalizer, strict codec, and retained `WorldEngine`.
//!
//! New-code expectations (green): hidden/minimized ordinary,
//! transient, and dialog rows are retained members but ineligible for
//! layout (no `alloc`, no `frame`, no `clear_op`); eligible
//! transient/dialog rows float via the retained membership path.
//! Hidden/minimized transient rows project `floating:true` at the
//! normalizer but the engine's pre-existing minimized publish lane
//! reports `floating:false` while the member stays minimized (float
//! resumes on restore); legacy safety exclusions stay unmanaged.

use omnitiler::world_service::WorldService;
use serde_json::{Value, json};

const ADAPTER: &str = "kwin-actuator-1";
const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440001";
const UUID_B: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const SENDER: &str = ":1.42";

fn parse(reply: &str) -> Value {
    serde_json::from_str(reply).expect("reply is JSON")
}

fn discovered_service() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(&service.discovery_body(ADAPTER, "req-0"));
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    (service, generation)
}

/// Producer-shaped raw row with the REQUIRED `hidden` lane (distinct from
/// `minimized`): KWin `hidden` is `m_hidden`, never minimized, Activity,
/// off-desktop, or show-desktop state.
fn n5_raw(
    win: &str,
    hidden: bool,
    minimized: bool,
    normal_window: bool,
    transient: bool,
    dialog: bool,
    modal: bool,
) -> Value {
    n5_raw_full(
        win,
        "DP-1",
        100,
        UUID_A,
        hidden,
        minimized,
        normal_window,
        transient,
        dialog,
        modal,
    )
}

/// Producer-shaped raw row with explicit output, frame origin, and native desktop.
// Deliberate producer-shape constructor: one arg per raw lane.
#[allow(clippy::too_many_arguments)]
fn n5_raw_full(
    win: &str,
    out: &str,
    x: i32,
    desktop: &str,
    hidden: bool,
    minimized: bool,
    normal_window: bool,
    transient: bool,
    dialog: bool,
    modal: bool,
) -> Value {
    json!({
        "win": win,
        "out": out,
        "frameGeometry": {"x": x, "y": 100, "width": 400, "height": 300},
        "clientGeometry": {"x": x, "y": 100, "width": 400, "height": 300},
        "minSize": null,
        "maxSize": null,
        "desktops": [desktop],
        "normalWindow": normal_window,
        "managed": true,
        "deleted": false,
        "transient": transient,
        "transientFor": null,
        "dialog": dialog,
        "modal": modal,
        "wantsInput": true,
        "fullScreen": false,
        "maximizeMode": 0,
        "onAllDesktops": false,
        "minimized": minimized,
        "hidden": hidden,
    })
}

fn presentation() -> Value {
    json!({
        "foreground": "",
        "minimized": false,
        "maximized": false,
        "fullscreen": false,
        "cloaked": false,
        "hiddenWorkspace": false,
        "shell": false,
        "frame": null,
        "borderEnabled": false,
        "underlayEnabled": false,
        "borderGap": 0,
        "borderWidth": 0,
        "underlayExtension": 0,
        "chordHeld": false,
        "moveWindow": "",
        "paused": false,
        "pagerOrder": [],
        "pagerFrames": [],
    })
}

fn inner_report(windows: &[Value]) -> Value {
    json!({
        "v": 3,
        "mode": "per-output-local",
        "screens": [{"out": "DP-1", "full": [0, 0, 1920, 1080],
            "work": [0, 0, 1920, 1040], "gap": 8, "outer": 4,
            "complete": true, "primary": true}],
        "windows": windows,
        "views": [{"out": "DP-1", "view": ""}],
        "closed": [],
        "hostOwned": true,
        "equalCount": false,
        "birthClear": false,
        "native": [{"out": "DP-1", "order": [UUID_A], "current": UUID_A, "occupancy": [{"native": UUID_A, "count": null}]}],
        "predicate": "both-axes-fixed",
        "refill": "last-remaining-workspace",
        "defaultTiled": true,
        "focused": "",
        "complete": true,
        "gesture": false,
        "switchTarget": "cursor-output",
        "freshPlacement": "cursor-output-last-focused",
        "cursor": null,
        "actionForeground": null,
        "actionCached": null,
        "actionOutputs": ["DP-1"],
        "focusFollowsCursor": false,
        "cursorWarp": false,
        "presentation": presentation(),
    })
}

/// Two-output inner report for whole-workspace migration: DP-1 carries the
/// migrated workspace, DP-2 a settled bystander.
fn two_output_inner(windows: &[Value]) -> Value {
    json!({
        "v": 3,
        "mode": "per-output-local",
        "screens": [
            {"out": "DP-1", "full": [0, 0, 1920, 1080],
                "work": [0, 0, 1920, 1040], "gap": 8, "outer": 4,
                "complete": true, "primary": true},
            {"out": "DP-2", "full": [1920, 0, 1920, 1080],
                "work": [1920, 0, 1920, 1040], "gap": 8, "outer": 4,
                "complete": true, "primary": false},
        ],
        "windows": windows,
        "views": [{"out": "DP-1", "view": ""}, {"out": "DP-2", "view": ""}],
        "closed": [],
        "hostOwned": true,
        "equalCount": false,
        "birthClear": false,
        "native": [
            {"out": "DP-1", "order": [UUID_A], "current": UUID_A, "occupancy": [{"native": UUID_A, "count": null}]},
            {"out": "DP-2", "order": [UUID_B], "current": UUID_B, "occupancy": [{"native": UUID_B, "count": null}]},
        ],
        "predicate": "both-axes-fixed",
        "refill": "last-remaining-workspace",
        "defaultTiled": true,
        "focused": "",
        "complete": true,
        "gesture": false,
        "switchTarget": "cursor-output",
        "freshPlacement": "cursor-output-last-focused",
        "cursor": null,
        "actionForeground": null,
        "actionCached": null,
        "actionOutputs": ["DP-1", "DP-2"],
        "focusFollowsCursor": false,
        "cursorWarp": false,
        "presentation": presentation(),
    })
}

fn capture(generation: &str, seq: u64, inner: &Value) -> String {
    json!({
        "v": 3,
        "adapter": ADAPTER,
        "gen": generation,
        "source": format!("src-{seq}-t"),
        "sourceSeq": format!("{seq}"),
        "nativeInvalid": format!("{seq}"),
        "report": inner,
    })
    .to_string()
}

fn queue_req(generation: &str, client_cmd: &str, command: &Value) -> String {
    json!({
        "v": 3,
        "gen": generation,
        "client": "panel-1",
        "clientCmd": client_cmd,
        "command": command,
    })
    .to_string()
}

fn take_req(generation: &str, client_cmd: &str) -> String {
    json!({
        "v": 3,
        "gen": generation,
        "client": "panel-1",
        "clientCmd": client_cmd,
    })
    .to_string()
}

fn dispatch(service: &WorldService, generation: &str, seq: u64, inner: &Value) -> Value {
    parse(
        &service
            .evaluate_capture(&capture(generation, seq, inner))
            .expect("in-band"),
    )
}

fn row_of(reply: &Value, win: &str) -> Value {
    reply["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .find(|w| w["win"] == win)
        .unwrap_or_else(|| panic!("{win} retained: {}", reply["windows"]))
        .clone()
}

fn accepted_row(
    win: &str,
    hidden: bool,
    minimized: bool,
    normal: bool,
    tr: bool,
    dg: bool,
    md: bool,
) -> Value {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[n5_raw(win, hidden, minimized, normal, tr, dg, md)]);
    let reply = parse(
        &service
            .evaluate_capture(&capture(&generation, 1, &inner))
            .expect("in-band"),
    );
    assert_eq!(reply["ready"], true, "{win} capture accepts: {reply}");
    row_of(&reply, win)
}

#[test]
fn n5_hidden_ordinary_retained_without_layout() {
    let row = accepted_row("win-h", true, false, true, false, false, false);
    assert_eq!(row["alloc"], Value::Null, "no allocation");
    assert_eq!(row["frame"], Value::Null, "no frame");
    assert_eq!(row["clear_op"], json!(""), "no clear");
}

#[test]
fn n5_hidden_transient_retained_minimized_without_layout() {
    // The normalizer projects floating:true but the engine's pre-existing
    // minimized publish lane reports floating:false while minimized.
    let row = accepted_row("win-ht", true, false, true, true, false, false);
    assert_eq!(row["floating"], json!(false), "minimized lane");
    assert_eq!(row["alloc"], Value::Null, "no allocation");
    assert_eq!(row["frame"], Value::Null, "no frame");
}

#[test]
fn n5_dialog_admitted_as_retained_float() {
    // Actual KWin dialog: normalWindow=false, dialog=true (never the
    // impossible normal=true/dialog=true combination).
    let row = accepted_row("win-d", false, false, false, false, true, false);
    assert_eq!(row["floating"], json!(true), "dialog floats");
    assert_eq!(row["alloc"], Value::Null, "no allocation");
}

#[test]
fn n5_minimized_transient_retained_without_layout() {
    // Same minimized-lane contract as hidden-transient above.
    let row = accepted_row("win-mt", false, true, true, true, false, false);
    assert_eq!(row["floating"], json!(false), "minimized lane");
    assert_eq!(row["alloc"], Value::Null, "no allocation");
}

#[test]
fn n5_minimized_ordinary_retained_without_layout() {
    let row = accepted_row("win-m", false, true, true, false, false, false);
    assert_eq!(row["alloc"], Value::Null, "no allocation");
    assert_eq!(row["frame"], Value::Null, "no frame");
    assert_eq!(row["clear_op"], json!(""), "no clear");
}

#[test]
fn n5_legacy_windows_exclusions_stay_unmanaged() {
    // Legacy parity pin: Windows-owned safety exclusions stay unmanaged
    // even when transient. No production Windows edits in this unit.
    use tiler_core::window_policy::{MembershipVerdict, SafetyFacts, classify_membership};
    for facts in [
        SafetyFacts {
            visible: true,
            owned_dialog: true,
            ..SafetyFacts::default()
        },
        SafetyFacts {
            dialog: true,
            visible: true,
            ..SafetyFacts::default()
        },
        SafetyFacts {
            tool: true,
            visible: true,
            ..SafetyFacts::default()
        },
        SafetyFacts {
            no_activate: true,
            visible: true,
            ..SafetyFacts::default()
        },
        // Legacy parity: hidden/minimized stay unmanaged under the legacy
        // classifier even when transient (the retained classifier is the
        // only lane that keeps them); the exhaustive 512-case legacy pin
        // lives in tiler-core window_policy tests.
        SafetyFacts::default(),
        SafetyFacts {
            minimized: true,
            visible: true,
            ..SafetyFacts::default()
        },
    ] {
        assert_eq!(
            classify_membership(&facts, true),
            MembershipVerdict::LeaveUnmanaged,
            "exclusion stays unmanaged when transient: {facts:?}"
        );
    }
}

#[test]
fn n5_hidden_minimized_share_workspace_occupancy() {
    // Retained workspace occupancy: hidden and minimized members stay homed
    // on the workspace with held geometry and no layout writes, while the
    // visible member keeps its allocation on the same workspace.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[
        n5_raw_full(
            "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-h", "DP-1", 600, UUID_A, true, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-m", "DP-1", 1100, UUID_A, false, true, true, false, false, false,
        ),
    ]);
    let reply = dispatch(&service, &generation, 1, &inner);
    assert_eq!(reply["ready"], true);
    for win in ["win-h", "win-m"] {
        let row = row_of(&reply, win);
        assert_eq!(row["ws"], json!("ws-1"), "{win} stays homed");
        assert_eq!(row["member_auth"], json!("controlled"), "{win} member");
        assert_eq!(row["frame_auth"], json!("hold"), "{win} geometry held");
        assert_eq!(row["alloc"], Value::Null, "{win} no allocation");
        assert_eq!(row["frame"], Value::Null, "{win} no frame");
        assert_eq!(row["frame_op"], json!(""), "{win} no frame write");
        assert_eq!(row["clear_op"], json!(""), "{win} no clear");
    }
    let visible = row_of(&reply, "win-a");
    assert_eq!(visible["ws"], json!("ws-1"), "visible stays homed");
    assert!(!visible["alloc"].is_null(), "visible keeps its allocation");
}

#[test]
fn n5_focus_hidden_and_minimized_refuses_without_effects() {
    // Focus-by-id on a hidden or minimized member refuses as unknown with
    // no focus effect: the members are retained but never focusable.
    for (win, hidden, minimized) in [("win-h", true, false), ("win-m", false, true)] {
        let (service, generation) = discovered_service();
        let inner = inner_report(&[
            n5_raw_full(
                "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
            ),
            n5_raw_full(
                win, "DP-1", 600, UUID_A, hidden, minimized, true, false, false, false,
            ),
        ]);
        let baseline = dispatch(&service, &generation, 1, &inner);
        assert_eq!(baseline["ready"], true);
        row_of(&baseline, win);
        let queued = parse(
            &service
                .queue_command(
                    SENDER,
                    &queue_req(
                        &generation,
                        "1",
                        &json!({"op": "focus", "win": win, "direction": "right"}),
                    ),
                )
                .expect("in-band"),
        );
        assert_eq!(queued["outcome"], "queued");
        let drain = dispatch(&service, &generation, 2, &inner);
        assert_eq!(drain["ready"], true);
        assert!(
            drain["effects"].as_array().expect("effects").is_empty(),
            "{win}: no focus effect"
        );
        let take = parse(
            &service
                .take_result(SENDER, &take_req(&generation, "1"))
                .expect("in-band"),
        );
        assert_eq!(take["outcome"], "failed", "{win}: focus refuses");
        assert_eq!(take["kind"], "focus-unknown", "{win}: unknown focus target");
        assert_ne!(drain["focused"], json!(win), "{win}: never focused");
    }
}

#[test]
fn n5_directional_nav_skips_hidden_and_minimized() {
    // Directional navigation moves between the visible tiles only: the
    // hidden and minimized members are never navigation targets.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[
        n5_raw_full(
            "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-h", "DP-1", 600, UUID_A, true, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-m", "DP-1", 700, UUID_A, false, true, true, false, false, false,
        ),
        n5_raw_full(
            "win-b", "DP-1", 1100, UUID_A, false, false, true, false, false, false,
        ),
    ]);
    let baseline = dispatch(&service, &generation, 1, &inner);
    assert_eq!(baseline["ready"], true);
    // Precondition: the concealed members are actually present; without
    // N5 retention this navigation would run on visible tiles only.
    row_of(&baseline, "win-h");
    row_of(&baseline, "win-m");
    let queued = parse(
        &service
            .queue_command(
                SENDER,
                &queue_req(
                    &generation,
                    "1",
                    &json!({"op": "focus-directional", "win": "win-a", "direction": "left"}),
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    let drain = dispatch(&service, &generation, 2, &inner);
    assert_eq!(drain["ready"], true);
    let effects = drain["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 1, "one focus effect");
    assert_eq!(effects[0]["target"], json!("win-b"), "nav lands on win-b");
    let take = parse(
        &service
            .take_result(SENDER, &take_req(&generation, "1"))
            .expect("in-band"),
    );
    assert_eq!(take["outcome"], "ready", "directional nav succeeds");
}

#[test]
fn n5_send_hidden_and_minimized_refuse_without_transfer() {
    // An ordinary send of a hidden or minimized member refuses with no
    // window transfer: minimized facts never enter the send source domain.
    for (win, hidden, minimized) in [("win-h", true, false), ("win-m", false, true)] {
        let (service, generation) = discovered_service();
        let inner = inner_report(&[
            n5_raw_full(
                "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
            ),
            n5_raw_full(
                win, "DP-1", 600, UUID_A, hidden, minimized, true, false, false, false,
            ),
        ]);
        let baseline = dispatch(&service, &generation, 1, &inner);
        assert_eq!(baseline["ready"], true);
        let queued = parse(
            &service
                .queue_command(
                    SENDER,
                    &queue_req(
                        &generation,
                        "1",
                        &json!({"op": "send", "win": win, "out": "DP-1", "ws": "ws-2", "follow": false}),
                    ),
                )
                .expect("in-band"),
        );
        assert_eq!(queued["outcome"], "queued");
        let drain = dispatch(&service, &generation, 2, &inner);
        assert_eq!(drain["ready"], true);
        assert!(
            drain["transfers"].as_array().expect("transfers").is_empty(),
            "{win}: no transfer"
        );
        assert!(
            drain["effects"].as_array().expect("effects").is_empty(),
            "{win}: no effects"
        );
        let take = parse(
            &service
                .take_result(SENDER, &take_req(&generation, "1"))
                .expect("in-band"),
        );
        assert_eq!(take["outcome"], "failed", "{win}: send refuses");
        assert_eq!(
            take["kind"], "send-window-unknown",
            "{win}: not in source domain"
        );
        assert_eq!(
            row_of(&drain, win)["ws"],
            json!("ws-1"),
            "{win}: membership unmoved"
        );
    }
}

#[test]
fn n5_restore_unhide_transient_resumes_float() {
    // A hidden or minimized transient stays a retained member in the
    // minimized lane; unhiding/restoring resumes the float member path.
    // Normalized input projects floating:true; the reply's minimized lane
    // reports floating:false until restore (pre-existing invariant).
    for (win, hidden, minimized) in [("win-h", true, false), ("win-m", false, true)] {
        let (service, generation) = discovered_service();
        let concealed = inner_report(&[n5_raw_full(
            win, "DP-1", 100, UUID_A, hidden, minimized, true, true, false, false,
        )]);
        let first = dispatch(&service, &generation, 1, &concealed);
        assert_eq!(first["ready"], true);
        let held = row_of(&first, win);
        assert_eq!(held["floating"], json!(false), "{win}: minimized lane");
        assert_eq!(held["alloc"], Value::Null, "{win}: no allocation");
        let restored = inner_report(&[n5_raw_full(
            win, "DP-1", 100, UUID_A, false, false, true, true, false, false,
        )]);
        let second = dispatch(&service, &generation, 2, &restored);
        assert_eq!(second["ready"], true);
        let floated = row_of(&second, win);
        assert_eq!(floated["floating"], json!(true), "{win}: float resumes");
        assert_eq!(floated["ws"], json!("ws-1"), "{win}: same workspace");
    }
}

#[test]
fn n5_pending_command_survives_hide_capture() {
    // A command filed before a hide still dispatches through the
    // hide-carrying capture: the pending lane is never wedged by
    // hidden/minimized members. This pins no-wedge only, not a pending
    // membership-transfer exception.
    let (service, generation) = discovered_service();
    let shown = inner_report(&[n5_raw_full(
        "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
    )]);
    let baseline = dispatch(&service, &generation, 1, &shown);
    assert_eq!(baseline["ready"], true);
    let queued = parse(
        &service
            .queue_command(
                SENDER,
                &queue_req(
                    &generation,
                    "1",
                    &json!({"op": "select-numbered", "out": null, "index": 2}),
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    let concealed = inner_report(&[n5_raw_full(
        "win-a", "DP-1", 100, UUID_A, true, false, true, false, false, false,
    )]);
    let drain = dispatch(&service, &generation, 2, &concealed);
    assert_eq!(drain["ready"], true);
    let held = row_of(&drain, "win-a");
    assert_eq!(
        held["member_auth"],
        json!("controlled"),
        "hide keeps membership"
    );
    assert_eq!(held["frame_auth"], json!("hold"), "hide holds geometry");
    let take = parse(
        &service
            .take_result(SENDER, &take_req(&generation, "1"))
            .expect("in-band"),
    );
    assert_eq!(take["outcome"], "ready", "pending command settles");
    assert_eq!(take["globalCmd"], "1");
}

#[test]
fn n5_migrate_moves_hidden_and_minimized_without_geometry() {
    let (service, generation) = discovered_service();
    let inner = two_output_inner(&[
        n5_raw_full(
            "win-a", "DP-1", 100, UUID_A, false, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-h", "DP-1", 600, UUID_A, true, false, true, false, false, false,
        ),
        n5_raw_full(
            "win-m", "DP-1", 700, UUID_A, false, true, true, false, false, false,
        ),
        n5_raw_full(
            "win-c", "DP-2", 2000, UUID_B, false, false, true, false, false, false,
        ),
    ]);
    let baseline = dispatch(&service, &generation, 1, &inner);
    assert_eq!(baseline["ready"], true);
    row_of(&baseline, "win-h");
    row_of(&baseline, "win-m");
    let logical = baseline["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|w| w["out"] == "DP-1" && w["bind"] == UUID_A)
        .unwrap_or_else(|| panic!("DP-1 workspace: {}", baseline["workspaces"]))["id"]
        .as_str()
        .expect("logical id")
        .to_owned();
    let queued = parse(
        &service
            .queue_command(
                SENDER,
                &queue_req(
                    &generation,
                    "1",
                    &json!({"op": "migrate-workspace", "from": "DP-1", "to": "DP-2", "ws": logical}),
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    // Members still natively behind on the drain: the core moves the
    // homing and retains the migration transfer.
    let drain = dispatch(&service, &generation, 2, &inner);
    assert_eq!(drain["ready"], true);
    let migrations: Vec<&Value> = drain["transfers"]
        .as_array()
        .expect("transfers")
        .iter()
        .filter(|t| t["kind"] == "migration")
        .collect();
    assert_eq!(migrations.len(), 1, "drain retains the migration transfer");
    assert_eq!(migrations[0]["ws"], json!(logical));
    assert_eq!(migrations[0]["out"], json!("DP-2"));
    assert!(
        drain["effects"].as_array().expect("effects").is_empty(),
        "no focus to moved members"
    );
    // Hidden and minimized membership moves with the workspace: new
    // output, same logical workspace, still no geometry, clear, or frame.
    for win in ["win-h", "win-m"] {
        let row = row_of(&drain, win);
        assert_eq!(row["out"], json!("DP-2"), "{win} moves output");
        assert_eq!(row["ws"], json!(logical), "{win} moves membership");
        assert_eq!(row["alloc"], Value::Null, "{win} no allocation");
        assert_eq!(row["frame"], Value::Null, "{win} no frame");
        assert_eq!(row["frame_op"], json!(""), "{win} no frame write");
        assert_eq!(row["clear_op"], json!(""), "{win} no clear");
    }
    // The visible member moves with geometry on the same workspace.
    let moved = row_of(&drain, "win-a");
    assert_eq!(moved["out"], json!("DP-2"));
    assert_eq!(moved["ws"], json!(logical));
    assert!(!moved["alloc"].is_null(), "visible member keeps geometry");
    let take = parse(
        &service
            .take_result(SENDER, &take_req(&generation, "1"))
            .expect("in-band"),
    );
    assert_eq!(take["outcome"], "ready");
    assert_eq!(take["result"]["op"], "migrate-workspace");
    let mut members: Vec<&str> = take["result"]["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|m| m.as_str().expect("member id"))
        .collect();
    members.sort_unstable();
    assert_eq!(
        members,
        vec!["win-a", "win-h", "win-m"],
        "whole workspace moves"
    );
}
