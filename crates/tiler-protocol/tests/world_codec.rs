//! Portable JSON codec coverage for the world boundary: DTO validation,
//! u64-as-string enforcement, golden reply bytes, encoder equality with the
//! core canonical writer, and actual 800/2000/large-id window payloads with
//! exact-overbound transactional behavior. Offline only: no D-Bus, no KWin.

use std::time::Instant;

use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{WORLD_CONTRACT_VERSION, WorldReply, WorldReport};
use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, decode_request, encode_reply,
};

/// Minimal observation-only request envelope; windows splice into the
/// single domain.
fn request_json(
    owner: &str,
    adapter: &str,
    generation: &str,
    obs: &str,
    req_seq: &str,
    req: &str,
    domain_windows: &str,
) -> String {
    format!(
        "{{\"v\":1,\"owner\":\"{owner}\",\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"{req}\",\"cmdSeq\":null,\
         \"command\":null,\"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[\"ws\"],\"current\":\"ws\",\"complete\":true,\"rect\":null,\"primary\":false}}],\
         \"bindings\":[],\
         \"domains\":[{{\"id\":\"out\",\"ws\":\"ws\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
         \"windows\":[{domain_windows}],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}"
    )
}

fn window_json(id: &str, x: i32) -> String {
    window_in_json(id, "out", "ws", x)
}

fn window_in_json(id: &str, out: &str, ws: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"{out}\",\"ws\":\"{ws}\",\"rect\":[{x},0,200,200],\
         \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
         \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
         \"fixedSuppress\":false,\
         \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}}}"
    )
}

fn one_window_request() -> String {
    request_json(
        "owner-a",
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &window_json("win-a", 0),
    )
}

#[test]
fn decode_minimal_observation_only_request() {
    let report: WorldReport = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.contract_version, WORLD_CONTRACT_VERSION);
    assert_eq!(report.owner, "owner-a");
    assert_eq!(report.observation_seq, 1);
    assert_eq!(report.request_seq, 1);
    assert_eq!(report.domains.len(), 1);
    assert_eq!(report.domains[0].windows.len(), 1);
    assert_eq!(report.domains[0].windows[0].window.0, "win-a");
    assert!(report.command.is_none());
}

#[test]
fn counters_reject_json_numbers_and_non_canonical_strings() {
    // JSON numbers where counters belong refuse: no f64 precision loss path.
    let numeric = one_window_request().replace("\"obs\":\"1\"", "\"obs\":1");
    assert!(decode_request(&numeric).is_err());
    let float = one_window_request().replace("\"obs\":\"1\"", "\"obs\":1.0");
    assert!(decode_request(&float).is_err());
    for bad in [
        "007",
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "18446744073709551616",
        "12a",
    ] {
        let request = one_window_request().replace("\"obs\":\"1\"", &format!("\"obs\":\"{bad}\""));
        assert!(decode_request(&request).is_err(), "obs {bad:?} must refuse");
    }
    // u64::MAX is the largest accepted counter.
    let max = one_window_request()
        .replace("\"obs\":\"1\"", "\"obs\":\"18446744073709551615\"")
        .replace("\"reqSeq\":\"1\"", "\"reqSeq\":\"18446744073709551615\"");
    let report = decode_request(&max).expect("u64 max decodes");
    assert_eq!(report.observation_seq, u64::MAX);
    assert_eq!(report.request_seq, u64::MAX);
}

#[test]
fn unknown_fields_and_values_refuse() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["extra"] = serde_json::Value::Bool(true);
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    let bad_mode = one_window_request().replace("per-output-local", "sideways");
    let error = decode_request(&bad_mode).expect_err("unknown mode refuses");
    assert_eq!(error.kind, "unknown-value");
    let bad_version = one_window_request().replace("\"v\":1", "\"v\":2");
    let error = decode_request(&bad_version).expect_err("version refuses");
    assert_eq!(error.kind, "unsupported-version");
    // Command without sequence (and sequence without command) refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["command"] = serde_json::json!({"op": "select-trailing", "out": "out"});
    let error = decode_request(&value.to_string()).expect_err("unpaired command refuses");
    assert_eq!(error.kind, "snapshot-invalid");
}

#[test]
fn minimized_predicate_and_new_commands_decode() {
    use tiler_core::world_report::WorldCommand;
    // Explicit minimized lifetime plus either-axis ingress decode typed.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["minimized"] = serde_json::json!([{"win": "win-m", "out": "out", "ws": "ws"}]);
    value["predicate"] = serde_json::Value::String("either-axis-fixed".to_owned());
    // Minimized entries live outside domain membership: drop win-a from
    // the domain so the report stays consistent.
    value["domains"][0]["windows"] = serde_json::json!([]);
    value["domains"][0]["focused"] = serde_json::Value::String(String::new());
    let report = decode_request(&value.to_string()).expect("decodes");
    assert_eq!(report.minimized.len(), 1);
    assert_eq!(report.minimized[0].window.0, "win-m");
    assert_eq!(
        report.fixed_size_predicate,
        tiler_core::size_hints::FixedSizePredicate::EitherAxis
    );
    // Unknown predicate tokens refuse without echo.
    let bad = one_window_request().replace("both-axes-fixed", "sometimes-fixed");
    let error = decode_request(&bad).expect_err("bad predicate refuses");
    assert_eq!(error.kind, "unknown-value");
    // Ingress migration source-refill decodes typed: the default rides the
    // helper envelope, MRU decodes explicitly, unknown tokens refuse without
    // echo, and a missing key refuses (no silent default at this boundary).
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(
        report.migration_source_refill,
        tiler_core::output_selection::MigrationSourceRefill::LastRemaining
    );
    let mru =
        one_window_request().replace("last-remaining-workspace", "most-recently-used-workspace");
    let report = decode_request(&mru).expect("mru decodes");
    assert_eq!(
        report.migration_source_refill,
        tiler_core::output_selection::MigrationSourceRefill::MostRecentlyUsed
    );
    let bad = one_window_request().replace("last-remaining-workspace", "mru");
    let error = decode_request(&bad).expect_err("bad refill refuses");
    assert_eq!(error.kind, "unknown-value");
    let mut missing: serde_json::Value =
        serde_json::from_str(&one_window_request()).expect("valid");
    missing.as_object_mut().expect("object").remove("refill");
    decode_request(&missing.to_string()).expect_err("missing refill refuses");
    // Minimized overlap with live membership refuses as snapshot-invalid.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["minimized"] = serde_json::json!([{"win": "win-a", "out": "out", "ws": "ws"}]);
    let error = decode_request(&value.to_string()).expect_err("overlap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    // Numbered sends (0..=9) and the tiling toggle decode typed.
    for (command, index) in [
        (
            serde_json::json!({"op": "send-numbered", "win": "win-a", "out": "out", "index": 0, "follow": false}),
            0u8,
        ),
        (
            serde_json::json!({"op": "send-numbered", "win": "win-a", "out": "out", "index": 9, "follow": true}),
            9u8,
        ),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cmdSeq"] = serde_json::Value::String("7".to_owned());
        value["command"] = command;
        let report = decode_request(&value.to_string()).expect("decodes");
        assert!(matches!(
            report.command,
            Some(WorldCommand::SendNumbered { index: i, .. }) if i == index
        ));
    }
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cmdSeq"] = serde_json::Value::String("8".to_owned());
    value["command"] =
        serde_json::json!({"op": "set-workspace-tiling", "out": "out", "ws": "ws", "tiled": false});
    let report = decode_request(&value.to_string()).expect("decodes");
    assert!(matches!(
        report.command,
        Some(WorldCommand::SetWorkspaceTiling { tiled: false, .. })
    ));
    // Index 10 is outside the 0..=9 vocabulary.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cmdSeq"] = serde_json::Value::String("9".to_owned());
    value["command"] = serde_json::json!({"op": "send-numbered", "win": "win-a", "out": "out", "index": 10, "follow": false});
    let error = decode_request(&value.to_string()).expect_err("index 10 refuses");
    assert_eq!(error.kind, "unknown-value");
}

#[test]
#[allow(clippy::type_complexity)]
fn directional_intents_decode_typed_and_refuse_bad_vocab() {
    use tiler_core::directional::SameAxisMove;
    use tiler_core::world_report::WorldCommand;
    // Direction-only intents carry no endpoint: win+direction or
    // source+direction only. The core resolves the strict target.
    let cases: Vec<(serde_json::Value, Box<dyn Fn(&WorldCommand)>)> = vec![
        (
            serde_json::json!({"op": "focus-directional", "win": "win-a", "direction": "right", "floatSubject": true}),
            Box::new(|cmd| assert!(matches!(cmd, WorldCommand::FocusDirectional { float_subject: true, .. }))),
        ),
        (
            serde_json::json!({"op": "move-directional", "win": "win-a", "direction": "left", "sameAxis": "swap-with-neighbor"}),
            Box::new(|cmd| assert!(matches!(cmd, WorldCommand::MoveDirectional { same_axis: SameAxisMove::SwapWithNeighbor, .. }))),
        ),
        (
            serde_json::json!({"op": "send-directional", "win": "win-a", "direction": "down", "follow": false}),
            Box::new(|cmd| assert!(matches!(cmd, WorldCommand::SendDirectional { follow: false, .. }))),
        ),
        (
            serde_json::json!({"op": "migrate-directional", "from": "out", "ws": "ws", "direction": "up"}),
            Box::new(|cmd| assert!(matches!(cmd, WorldCommand::MigrateDirectional { .. }))),
        ),
    ];
    for (command, check) in cases {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cmdSeq"] = serde_json::Value::String("77".to_owned());
        value["command"] = command;
        let report = decode_request(&value.to_string()).expect("directional decodes");
        check(report.command.as_ref().expect("command present"));
    }
    // Bad direction, bad sameAxis, and missing keys refuse.
    for bad in [
        serde_json::json!({"op": "focus-directional", "win": "win-a", "direction": "sideways", "floatSubject": false}),
        serde_json::json!({"op": "move-directional", "win": "win-a", "direction": "left", "sameAxis": "sideways"}),
        serde_json::json!({"op": "send-directional", "win": "win-a", "direction": "left"}),
        serde_json::json!({"op": "migrate-directional", "from": "out", "ws": "ws"}),
        serde_json::json!({"op": "focus-directional", "win": "win-a", "direction": "left", "floatSubject": "yes"}),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cmdSeq"] = serde_json::Value::String("78".to_owned());
        value["command"] = bad;
        assert!(decode_request(&value.to_string()).is_err(), "bad directional refuses");
    }
}

#[test]
fn request_estimate_bounds_actual_wire_bytes() {
    // The structural request estimator gates before any convergence: it
    // must bound the actual wire bytes for every shape, not just deltas.
    let long = "w".repeat(128);
    let extreme = format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"{long}\",\"gen\":\"gen-1\",\
         \"obs\":\"18446744073709551615\",\"reqSeq\":\"18446744073709551615\",\"req\":\"req-9\",\
         \"cmdSeq\":null,\"command\":null,\"mode\":\"global-unique\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[\"ws\"],\"current\":\"ws\",\"complete\":true,\"rect\":null,\"primary\":false}}],\
         \"bindings\":[{{\"logical\":\"{long}\",\"native\":\"ws\",\"out\":\"out\",\
         \"pin\":true,\"disp\":true,\"managed\":true}}],\
         \"domains\":[{{\"id\":\"out\",\"ws\":\"ws\",\"bounds\":[-16384,-16384,16384,16384],\
         \"gap\":8,\"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\
         \"adjacent\":{{\"right\":\"out-2\"}},\
         \"windows\":[{{\"win\":\"{long}\",\"out\":\"out\",\"ws\":\"ws\",\
         \"rect\":[-16384,-16384,16384,16384],\
         \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
         \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
         \"fixedSuppress\":false,\
         \"hints\":{{\"minW\":1,\"minH\":null,\"maxW\":16384,\"maxH\":null}}}}],\
         \"closed\":[]}}],\
         \"minimized\":[{{\"win\":\"win-m\",\"out\":\"out\",\"ws\":\"ws\"}}],\
         \"predicate\":\"either-axis-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\
         \"feedback\":[{{\"op\":\"op-9\",\"kind\":\"property\",\"outcome\":\"applied\",\
         \"hold\":\"constraint-clamp\"}}]}}"
    );
    for json in [one_window_request(), extreme] {
        let report = decode_request(&json).expect("decodes");
        let estimate = tiler_core::world_report::report_estimate(&report);
        assert!(
            estimate >= json.len(),
            "request estimate {estimate} bounds wire {} (shortfall {})",
            json.len(),
            json.len().saturating_sub(estimate),
        );
    }
}

#[test]
fn request_estimate_bounds_every_command_shape() {
    // Every command variant at maximal values must fit inside the
    // structural estimator: the pre-mutation gate relies on exact
    // per-variant punctuation plus carried widths, never bare constants.
    let long = "w".repeat(128);
    let commands = [
        serde_json::json!({"op": "focus", "win": long, "direction": "left"}),
        serde_json::json!({"op": "send", "win": long, "out": "out", "ws": "ws", "follow": true}),
        serde_json::json!({"op": "switch-view", "out": "out", "ws": "ws"}),
        serde_json::json!({"op": "select-workspace", "out": "out", "ws": "ws"}),
        serde_json::json!({"op": "select-numbered", "out": "out", "index": 9}),
        serde_json::json!({"op": "select-relative", "out": "out", "delta": -1}),
        serde_json::json!({"op": "select-previous", "out": "out"}),
        serde_json::json!({"op": "select-trailing", "out": "out"}),
        serde_json::json!({"op": "send-relative", "win": long, "out": "out", "delta": 1, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": long, "out": "out", "index": 9, "follow": true}),
        serde_json::json!({"op": "set-workspace-tiling", "out": "out", "ws": "ws", "tiled": true}),
        serde_json::json!({"op": "migrate-workspace", "from": "out", "to": "out-2", "ws": "ws"}),
        serde_json::json!({"op": "focus-directional", "win": long, "direction": "right", "floatSubject": true}),
        serde_json::json!({"op": "move-directional", "win": long, "direction": "left", "sameAxis": "swap-with-neighbor"}),
        serde_json::json!({"op": "send-directional", "win": long, "direction": "down", "follow": false}),
        serde_json::json!({"op": "migrate-directional", "from": "out", "ws": "ws", "direction": "up"}),
        serde_json::json!({"op": "operation", "command": {"op": "reconcile"}}),
        serde_json::json!({"op": "operation", "command": {"op": "update-gaps"}}),
        serde_json::json!({"op": "operation", "command": {"op": "active-group"}}),
        serde_json::json!({"op": "operation", "command": {"op": "release-domain"}}),
        serde_json::json!({"op": "operation", "command": {"op": "move", "win": long, "direction": "down", "crossOutput": true, "sameAxis": "swap-with-neighbor"}}),
        serde_json::json!({"op": "operation", "command": {"op": "focus", "win": long, "direction": "up", "crossOutput": false, "floatSubject": true}}),
        serde_json::json!({"op": "operation", "command": {"op": "resize", "win": long, "direction": "left", "mode": "vertical", "press": 4294967295u64}}),
        serde_json::json!({"op": "operation", "command": {"op": "pointer-resize", "win": long, "direction": "right", "boundary": -16384, "direction2": "up", "boundary2": 16384}}),
        serde_json::json!({"op": "operation", "command": {"op": "toggle-float", "win": long, "rect": [-16384, -16384, 16384, 16384]}}),
        serde_json::json!({"op": "operation", "command": {"op": "toggle-orientation", "win": long}}),
        serde_json::json!({"op": "operation", "command": {"op": "send-to-workspace", "win": long, "out": "out", "ws": "ws", "follow": true}}),
        serde_json::json!({"op": "operation", "command": {"op": "send-to-output", "win": long, "out": "out", "ws": "ws", "follow": false}}),
        serde_json::json!({"op": "operation", "command": {"op": "migrate-workspace", "direction": "right"}}),
        serde_json::json!({"op": "operation", "command": {"op": "drag-drop", "win": long, "x": -16384, "y": 16384,
            "hover": {"domainOut": "out", "domainWs": "ws", "leaf": long, "win": long, "rev": "18446744073709551615",
                      "prior": {"group": long, "edge": "bottom"}, "priorOut": "out", "priorWs": "ws"},
            "source": {"out": "out", "ws": "ws"}}}),
        serde_json::json!({"op": "operation", "command": {"op": "drag-preview", "win": long, "x": 0, "y": 0,
            "hover": null, "source": null}}),
    ];
    for command in &commands {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cmdSeq"] = serde_json::Value::String("99".to_owned());
        value["command"] = command.clone();
        let json = value.to_string();
        let report = decode_request(&json).expect("command decodes");
        let estimate = tiler_core::world_report::report_estimate(&report);
        assert!(
            estimate >= json.len(),
            "command {} estimate {estimate} bounds wire {}",
            command["op"],
            json.len(),
        );
    }
}

#[test]
fn request_estimate_covers_new_field_deltas() {
    // The structural estimator must cover the wire bytes added by
    // minimized rows, the predicate, and the new commands: the commit
    // gate relies on it before any mutation. Asserted as a delta over
    // the minimal request so pre-existing envelope slack cannot mask a
    // shortfall in the new sections.
    let base_json = one_window_request();
    let base = decode_request(&base_json).expect("decodes");
    let base_est = tiler_core::world_report::report_estimate(&base);
    for (minimized, predicate, cmd_seq, command) in [
        (
            serde_json::json!([
                {"win": "win-m1", "out": "out", "ws": "ws"},
                {"win": "win-m2", "out": "out", "ws": "ws"},
            ]),
            "either-axis-fixed",
            "3",
            serde_json::json!({"op": "send-numbered", "win": "win-a", "out": "out", "index": 0, "follow": true}),
        ),
        (
            serde_json::json!([]),
            "both-axes-fixed",
            "4",
            serde_json::json!({"op": "set-workspace-tiling", "out": "out", "ws": "ws", "tiled": false}),
        ),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["minimized"] = minimized;
        value["predicate"] = serde_json::Value::String(predicate.to_owned());
        value["cmdSeq"] = serde_json::Value::String(cmd_seq.to_owned());
        value["command"] = command;
        let json = value.to_string();
        let report = decode_request(&json).expect("decodes");
        let est = tiler_core::world_report::report_estimate(&report);
        assert!(
            est.saturating_sub(base_est) >= json.len().saturating_sub(base_json.len()),
            "estimate covers the new-field wire delta"
        );
    }
}

#[test]
fn capacity_settings_decode_tolerant_and_apply_without_reflow() {
    use tiler_core::admission::{AnchorFallback, NoSpaceResult};
    // Missing keys backfill defaults so older adapters keep decoding.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.anchor_fallback, AnchorFallback::LargestSplittable);
    assert_eq!(report.no_space_result, NoSpaceResult::Float);
    // Explicit tokens decode typed.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["anchorFallback"] = serde_json::Value::String("closest-splittable-relative".to_owned());
    value["noSpaceResult"] = serde_json::Value::String("allow-overlap".to_owned());
    let report = decode_request(&value.to_string()).expect("decodes");
    assert_eq!(report.anchor_fallback, AnchorFallback::ClosestRelative);
    assert_eq!(report.no_space_result, NoSpaceResult::AllowOverlap);
    // Invalid or mistyped tokens backfill the default instead of refusing.
    for (key, bad, fallback, no_space) in [
        (
            "anchorFallback",
            serde_json::Value::String("biggest".to_owned()),
            AnchorFallback::LargestSplittable,
            NoSpaceResult::Float,
        ),
        (
            "anchorFallback",
            serde_json::Value::String(String::new()),
            AnchorFallback::LargestSplittable,
            NoSpaceResult::Float,
        ),
        (
            "anchorFallback",
            serde_json::Value::Bool(true),
            AnchorFallback::LargestSplittable,
            NoSpaceResult::Float,
        ),
        (
            "noSpaceResult",
            serde_json::Value::String("stack".to_owned()),
            AnchorFallback::LargestSplittable,
            NoSpaceResult::Float,
        ),
        (
            "noSpaceResult",
            serde_json::Value::Null,
            AnchorFallback::LargestSplittable,
            NoSpaceResult::Float,
        ),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value[key] = bad;
        let report = decode_request(&value.to_string()).expect("tolerant decode");
        assert_eq!(report.anchor_fallback, fallback, "key {key}");
        assert_eq!(report.no_space_result, no_space, "key {key}");
    }
    // Genuinely unknown fields still refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["anchor"] = serde_json::Value::String("largest-splittable-window".to_owned());
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    // D10: a settings-only Apply never reflows retained state. Two
    // accepted reports differing only in the D2/D3 tokens publish
    // identical window rows and scopes.
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let first = decode_request(&one_window_request()).expect("decodes");
    let before = match engine.handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut changed: serde_json::Value =
        serde_json::from_str(&one_window_request()).expect("valid");
    changed["anchorFallback"] = serde_json::Value::String("closest-splittable-relative".to_owned());
    changed["noSpaceResult"] = serde_json::Value::String("allow-overlap".to_owned());
    changed["obs"] = serde_json::Value::String("2".to_owned());
    changed["reqSeq"] = serde_json::Value::String("2".to_owned());
    changed["req"] = serde_json::Value::String("req-2".to_owned());
    let second = decode_request(&changed.to_string()).expect("decodes");
    let after = match engine.handle_report(&second) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(after.windows, before.windows, "no reflow on settings Apply");
    assert_eq!(after.scopes, before.scopes, "no rehold on settings Apply");
}

#[test]
fn floating_rows_encode_identically_and_drive_end_to_end() {
    // Encoder equivalence over floating workspace rows and untouched
    // scopes: the transport gate and the core commit gate cannot differ.
    let reply = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-9".to_owned(),
        based_on_observation_seq: 3,
        state_revision: 4,
        ready: true,
        command_consumed: false,
        scopes: vec![tiler_core::world_report::ScopeStatus {
            scope: "out/ws".to_owned(),
            authority: tiler_core::world_report::ScopeAuthority::Untouched,
        }],
        workspaces: vec![tiler_core::world_report::WorkspaceDesire {
            id: "ws".to_owned(),
            output: tiler_core::directional::OutputId("out".to_owned()),
            native_binding: Some("ws".to_owned()),
            logical_id: None,
            scope: "out".to_owned(),
            order: 0,
            tiling_mode: tiler_core::world_report::TilingMode::Floating,
            existence: true,
            hold: None,
        }],
        outputs: Vec::new(),
        windows: Vec::new(),
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    };
    assert_eq!(
        encode_reply(&reply),
        tiler_core::world_report::encode_reply_canonical(&reply)
    );
    let encoded = encode_reply(&reply);
    assert!(encoded.contains("\"mode\":\"floating\""));
    assert!(encoded.contains("\"auth\":\"untouched\""));
    // End to end through JSON: the tiling toggle floats the workspace
    // with explicit untouched rows (never emptiness) and no frame writes.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cmdSeq"] = serde_json::Value::String("11".to_owned());
    value["command"] =
        serde_json::json!({"op": "set-workspace-tiling", "out": "out", "ws": "ws", "tiled": false});
    // Seed first so the toggle has a session to release, then toggle.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&one_window_request()).expect("decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut toggle = decode_request(&value.to_string()).expect("decodes");
    toggle.request_seq = 2;
    toggle.request_id = "req-2".to_owned();
    toggle.observation_seq = 2;
    let reply = match engine.handle_report(&toggle) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected toggle accepted, got {other:?}"),
    };
    assert_eq!(reply.windows.len(), 1);
    let row = &reply.windows[0];
    assert_eq!(row.lifetime.0, "win-a");
    assert!(row.allocation.is_none());
    assert_eq!(
        row.frame_authority,
        tiler_core::world_report::ScopeAuthority::Untouched
    );
    assert_eq!(
        row.membership_authority,
        tiler_core::world_report::ScopeAuthority::Untouched
    );
    let encoded = encode_reply(&reply);
    assert!(encoded.contains("\"mode\":\"floating\""));
    assert!(encoded.contains("\"auth\":\"untouched\""));
}

#[test]
fn golden_reply_bytes_lock_wire_shape() {
    // A fixed reply must encode byte-identically every run: keys, order,
    // string-quoted u64 counters, and hold tokens are frozen here.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let report = decode_request(&one_window_request()).expect("decodes");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let encoded = encode_reply(&reply);
    // Single occupied workspace: the trailing policy appends the stable
    // wl-1 spare, and the lone window takes the fullscreen single admit.
    let expected = "{\"v\":1,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"req\":\"req-1\",\
        \"obs\":\"1\",\"rev\":\"1\",\"ready\":true,\"consumed\":false,\
        \"scopes\":[{\"scope\":\"out/ws\",\"auth\":\"controlled\",\"reason\":\"\"}],\
        \"workspaces\":[{\"id\":\"ws\",\"out\":\"out\",\"bind\":\"ws\",\"logical\":\"\",\
        \"scope\":\"out\",\"order\":0,\"mode\":\"tiled\",\"exist\":true,\"hold\":\"\"},\
        {\"id\":\"wl-1\",\"out\":\"out\",\"bind\":\"\",\"logical\":\"wl-1\",\
        \"scope\":\"out\",\"order\":1,\"mode\":\"tiled\",\"exist\":true,\"hold\":\"\"}],\
        \"outputs\":[{\"out\":\"out\",\"view\":\"\",\"mode\":\"tiled\",\"hold\":\"\"}],\
        \"windows\":[{\"win\":\"win-a\",\"out\":\"out\",\"ws\":\"ws\",\
        \"alloc\":[4,4,792,592],\"frame\":[4,4,792,592],\
        \"frame_auth\":\"controlled\",\"frame_hold\":\"\",\
        \"member_auth\":\"controlled\",\"member_hold\":\"\",\"frame_op\":\"\",\
        \"floating\":false,\"float_origin\":\"\"}],\
        \"transfers\":[],\"effects\":[],\"result\":null}";
    let expected: String = expected.split_whitespace().collect::<Vec<_>>().join("");
    assert_eq!(encoded, expected);
}

#[test]
fn protocol_encoder_matches_core_canonical_writer() {
    // The transport gate and the core commit gate cannot differ: every
    // reply shape must encode identically through both writers.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let long = "w".repeat(120);
    let windows: Vec<String> = (0..3)
        .map(|i| window_json(&format!("{long}-{i}"), 0))
        .collect();
    let json = request_json(
        "owner-a",
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &windows.join(","),
    );
    let report = decode_request(&json).expect("decodes");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(
        encode_reply(&reply),
        tiler_core::world_report::encode_reply_canonical(&reply)
    );
    // Extrema reply through both writers.
    let extreme = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: long.clone(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-9".to_owned(),
        based_on_observation_seq: u64::MAX,
        state_revision: u64::MAX,
        ready: true,
        command_consumed: true,
        scopes: vec![tiler_core::world_report::ScopeStatus {
            scope: long.clone(),
            authority: tiler_core::world_report::ScopeAuthority::Hold(
                tiler_core::world_report::HoldReason::Minimized,
            ),
        }],
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: vec![tiler_core::world_report::WindowDesire {
            lifetime: tiler_core::directional::WindowId(long.clone()),
            output: tiler_core::directional::OutputId(long.clone()),
            workspace: long.clone(),
            allocation: None,
            effective_frame: None,
            frame_authority: tiler_core::world_report::ScopeAuthority::Hold(
                tiler_core::world_report::HoldReason::Minimized,
            ),
            membership_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            frame_operation: None,
            floating: false,
            float_origin: None,
        }],
        transfers: Vec::new(),
        effects: vec![tiler_core::world_report::OneShotEffect {
            id: long.clone(),
            kind: tiler_core::world_report::EffectKind::Focus,
            target: tiler_core::directional::WindowId(long),
            issued_revision: u64::MAX,
        }],
        command_result: None,
        presentation_bytes: 0,
    };
    assert_eq!(
        encode_reply(&extreme),
        tiler_core::world_report::encode_reply_canonical(&extreme)
    );
    // u64 counters never encode as bare numbers.
    let encoded = encode_reply(&extreme);
    assert!(encoded.contains("\"obs\":\"18446744073709551615\""));
    assert!(encoded.contains("\"rev\":\"18446744073709551615\""));
}

fn many_window_request(count: usize, id_len: usize) -> String {
    many_window_request_spread(count, id_len, 1)
}

/// Realistic bulk inventory: windows spread across `domains` workspaces on
/// one output (a handful per domain, as on a live desktop), never bulked
/// into a single fresh seed. Total id length stays within the 128 opaque
/// bound: base + pad + the dash plus six digits.
fn many_window_request_capped(
    count: usize,
    id_len: usize,
    domains: usize,
    cap_per_domain: Option<usize>,
    obs: &str,
    req_seq: &str,
) -> String {
    let base = "win-".to_owned();
    let pad_len = id_len
        .saturating_sub(base.len())
        .saturating_sub(7)
        .min(128 - base.len() - 7);
    let pad = "w".repeat(pad_len);
    let domains = domains.clamp(1, 64);
    let mut domain_rows = Vec::with_capacity(domains);
    let mut order = Vec::with_capacity(domains);
    for d in 0..domains {
        let ws = format!("ws-{d:03}");
        order.push(format!("\"{ws}\""));
        let rows: Vec<String> = (0..count)
            .filter(|i| i % domains == d)
            .take(cap_per_domain.unwrap_or(usize::MAX))
            .map(|i| {
                window_in_json(
                    &format!("{base}{pad}-{i:06}"),
                    "out",
                    &ws,
                    ((i * 37) % 600) as i32,
                )
            })
            .collect();
        domain_rows.push(format!(
            "{{\"id\":\"out\",\"ws\":\"{ws}\",\"bounds\":[0,0,800,600],\"gap\":8,\
             \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
             \"windows\":[{}],\"closed\":[]}}",
            rows.join(",")
        ));
    }
    format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"req-{req_seq}\",\"cmdSeq\":null,\
         \"command\":null,\"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out\",\"order\":[{}],\"current\":\"ws-000\",\
         \"complete\":true,\"rect\":null,\"primary\":false}}],\"bindings\":[],\"domains\":[{}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        order.join(","),
        domain_rows.join(",")
    )
}

fn many_window_request_spread(count: usize, id_len: usize, domains: usize) -> String {
    many_window_request_capped(count, id_len, domains, None, "1", "1")
}

fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse::<u64>().ok();
        }
    }
    None
}

#[test]
fn eight_hundred_window_json_succeeds_within_budget() {
    // Spread inventory (~12-13 windows per domain across 64 domains) tiles
    // fully in one fresh report: every scope controlled with a row per
    // lifetime carrying an allocation, not a narrow hold standing in for
    // the inventory. Single-domain bulk past the geometric threshold holds
    // instead (see the capacity tests); that workload is never counted as
    // a successful layout.
    let json = many_window_request_spread(800, 24, 64);
    assert!(
        json.len() < WORLD_MAX_REQUEST_BYTES,
        "800-window request is {} bytes",
        json.len()
    );
    let before = rss_kb();
    let start = Instant::now();
    let report = decode_request(&json).expect("800-window request decodes");
    let decode_ms = start.elapsed();
    assert!(
        tiler_core::world_report::report_estimate(&report) >= json.len(),
        "request estimator bounds the 800-window wire bytes"
    );
    assert_eq!(
        report
            .domains
            .iter()
            .map(|d| d.windows.len())
            .sum::<usize>(),
        800
    );
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let core_start = Instant::now();
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    // Genuine full-state success: every scope controlled with a row per
    // lifetime, not a narrow hold standing in for the inventory.
    assert!(
        reply
            .scopes
            .iter()
            .all(|s| s.authority == tiler_core::world_report::ScopeAuthority::Controlled),
        "all 64 scopes controlled"
    );
    assert_eq!(reply.windows.len(), 800);
    let core_ms = core_start.elapsed();
    let encoded = encode_reply(&reply);
    assert_eq!(
        encoded,
        tiler_core::world_report::encode_reply_canonical(&reply),
        "transport and commit gates cannot differ on bulk replies"
    );
    assert!(
        tiler_core::world_report::reply_estimate(&reply) >= encoded.len(),
        "reply estimator bounds the 800-window wire bytes"
    );
    assert!(
        tiler_core::world_report::reply_upper_bound(
            &report,
            0,
            engine.engine().domain_keys().len(),
            0
        ) >= tiler_core::world_report::reply_estimate(&reply),
        "pre-mutation upper bound covers the 800-window reply"
    );
    let total_ms = start.elapsed();
    assert!(
        encoded.len() < WORLD_MAX_REPLY_BYTES,
        "800-window reply is {} bytes",
        encoded.len()
    );
    eprintln!(
        "800-window: request={}B reply={}B decode={}ms core={}ms total={}ms rss_before={:?}KB rss_after={:?}KB",
        json.len(),
        encoded.len(),
        decode_ms.as_millis(),
        core_ms.as_millis(),
        total_ms.as_millis(),
        before,
        rss_kb(),
    );
}

#[test]
fn two_thousand_large_id_windows_seed_completely_within_budget() {
    // Fresh full inventory, never sharded: ~31 windows per 800x600/gap-8
    // domain all tile under largest-fallback packing, so every domain
    // commits a complete controlled roster with one allocated row per
    // lifetime (best effort, selected 2026-10-17: no whole-seed hold).
    // Both reports stay far inside the 4 MiB budget.
    let json = many_window_request_capped(2000, 128, 64, None, "1", "1");
    assert!(
        json.len() < WORLD_MAX_REQUEST_BYTES,
        "2000-window large-id request is {} bytes",
        json.len()
    );
    let report = decode_request(&json).expect("2000-window request decodes");
    assert!(
        tiler_core::world_report::report_estimate(&report) >= json.len(),
        "request estimator bounds the 2000-window wire bytes"
    );
    let start = Instant::now();
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let core_ms = start.elapsed();
    assert_eq!(reply.scopes.len(), 64);
    assert!(
        reply
            .scopes
            .iter()
            .all(|s| s.authority == tiler_core::world_report::ScopeAuthority::Controlled),
        "every domain commits its complete roster"
    );
    assert_eq!(reply.windows.len(), 2000, "complete lifetime rows");
    let usable = reply
        .windows
        .iter()
        .filter(|w| w.allocation.is_some())
        .count();
    assert_eq!(usable, 2000, "largest-fallback packing tiles every member");
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    let encoded = encode_reply(&reply);
    assert_eq!(
        encoded,
        tiler_core::world_report::encode_reply_canonical(&reply),
        "transport and commit gates cannot differ on bulk replies"
    );
    assert!(
        estimate >= encoded.len(),
        "reply estimator bounds the wire bytes"
    );
    eprintln!(
        "2000-window large-id SEEDED: request={}B reply={}B usable=2000 core={}ms",
        json.len(),
        encoded.len(),
        core_ms.as_millis(),
    );
    assert!(encoded.len() < WORLD_MAX_REPLY_BYTES);
    // No topology churn: the same inventory under a fresh sequence
    // republishes identical desired rows.
    let repeat = many_window_request_capped(2000, 128, 64, None, "2", "2");
    let report = decode_request(&repeat).expect("repeat decodes");
    let again = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(again.scopes, reply.scopes);
    assert_eq!(again.windows, reply.windows);
    // Fewer windows: a bare omission on a retained session minimizes
    // (never an inferred close), so the roster keeps 13 tiles plus 18
    // minimized rows per domain with the scopes still controlled.
    let fitting = many_window_request_capped(2000, 128, 64, Some(13), "3", "3");
    let report = decode_request(&fitting).expect("fitting decodes");
    let recovered = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        recovered
            .scopes
            .iter()
            .all(|s| s.authority == tiler_core::world_report::ScopeAuthority::Controlled),
        "shrunk inventory stays controlled"
    );
    assert_eq!(recovered.windows.len(), 2000, "no lifetime vanishes");
    let tiled = recovered
        .windows
        .iter()
        .filter(|w| w.allocation.is_some())
        .count();
    assert_eq!(tiled, 13 * 64, "thirteen tiles per domain");
    assert!(
        recovered
            .windows
            .iter()
            .filter(|w| w.allocation.is_none())
            .all(|w| w.frame_authority.hold_reason()
                == Some(tiler_core::world_report::HoldReason::Minimized)),
        "every unallocated row is an explicit minimized row"
    );
}

#[test]
fn exact_overbound_request_refuses_before_core_mutation() {
    // Grow past exactly 4 MiB with large-id windows: the protocol byte gate
    // refuses before any decode or engine work.
    let mut count = 8000usize;
    let json = loop {
        let candidate = many_window_request(count, 128);
        if candidate.len() > WORLD_MAX_REQUEST_BYTES {
            break candidate;
        }
        count += 4000;
        assert!(count < 100_000, "bound unreachable in test");
    };
    assert!(json.len() > WORLD_MAX_REQUEST_BYTES);
    let error = decode_request(&json).expect_err("overbound refuses");
    assert_eq!(error.kind, "oversized");
    // Typed-level overbound: the structural estimate refuses before any
    // convergence, and the engine keeps prior ordering intact.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = decode_request(&one_window_request()).expect("decodes");
    let accepted = match engine.handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let rows = tiler_core::world_report::WindowDesire {
        lifetime: tiler_core::directional::WindowId("w".repeat(128)),
        output: tiler_core::directional::OutputId("o".repeat(128)),
        workspace: "s".repeat(128),
        allocation: None,
        effective_frame: None,
        frame_authority: tiler_core::world_report::ScopeAuthority::Controlled,
        membership_authority: tiler_core::world_report::ScopeAuthority::Controlled,
        frame_operation: None,
        floating: false,
        float_origin: None,
    };
    let big = WorldReply {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-1".to_owned(),
        based_on_observation_seq: 1,
        state_revision: 1,
        ready: true,
        command_consumed: false,
        scopes: Vec::new(),
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: vec![rows; 9000],
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    };
    assert!(
        tiler_core::world_report::reply_estimate(&big) > WORLD_MAX_REPLY_BYTES,
        "estimator bounds the 9000-row reply"
    );
    // Engine state is untouched by the refused work: the exact first
    // report still duplicates with no new revision.
    match engine.handle_report(&first) {
        WorldOutcome::Duplicate(reply) => assert_eq!(reply.state_revision, accepted.state_revision),
        other => panic!("expected duplicate, got {other:?}"),
    }
}

/// Regenerates the shared reply golden from the actual TS-captured
/// observation fixture. Runs only with UPDATE_WORLD_FIXTURES=1 (the TS
/// suite writes report-observe.json first); the gate below includes the
/// checked-in bytes and needs no runtime.
#[test]
fn shared_fixture_reply_golden_regeneration() {
    if std::env::var("UPDATE_WORLD_FIXTURES").as_deref() != Ok("1") {
        return;
    }
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire");
    let json =
        std::fs::read_to_string(dir.join("report-observe.json")).expect("TS fixture present");
    let report = decode_request(&json).expect("TS fixture decodes");
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    std::fs::write(dir.join("reply-golden.json"), encode_reply(&reply)).expect("write golden");
}

/// Outer-gap fixture: real TS producer bytes (800x600, inner 8, outer
/// 12) regenerate the inset reply golden. Runs only with
/// UPDATE_WORLD_FIXTURES=1; the gate below pins semantic edge geometry,
/// never a bare byte compare.
#[test]
fn shared_fixture_outer_gap_golden_regeneration() {
    if std::env::var("UPDATE_WORLD_FIXTURES").as_deref() != Ok("1") {
        return;
    }
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire");
    let json =
        std::fs::read_to_string(dir.join("report-outer-gap.json")).expect("TS fixture present");
    let report = decode_request(&json).expect("TS fixture decodes");
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    std::fs::write(dir.join("reply-outer-gap.json"), encode_reply(&reply)).expect("write golden");
}

/// Correlated-result fixtures: real engine replies carrying `result`
/// sections (drag-preview rect+hover, whole-workspace migration members)
/// for the kwin production-entry suite to parse and apply. Regenerated
/// with UPDATE_WORLD_FIXTURES=1; the gate below pins the checked-in bytes.
fn preview_seed_like() -> String {
    request_json(
        "owner-a",
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &format!("{},{}", window_json("win-a", 0), window_json("win-b", 400)),
    )
    .replace("\"focused\":\"\"", "\"focused\":\"win-a\"")
}

fn preview_fixture_request() -> String {
    let windows = format!("{},{}", window_json("win-a", 0), window_json("win-b", 400));
    let mut value: serde_json::Value = serde_json::from_str(&request_json(
        "owner-a",
        "adapter-1",
        "gen-1",
        "2",
        "2",
        "req-2",
        &windows,
    ))
    .expect("valid");
    value["cmdSeq"] = serde_json::Value::String("21".to_owned());
    value["command"] = serde_json::json!({
        "op": "operation",
        "command": {"op": "drag-preview", "win": "win-a", "x": 100, "y": 300, "hover": null, "source": null},
    });
    value.to_string()
}

fn migrate_fixture_seed() -> String {
    format!(
        "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\
         \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\"command\":null,\
         \"mode\":\"per-output-local\",\
         \"topology\":[{{\"out\":\"out-a\",\"order\":[\"wa1\"],\"current\":\"wa1\",\"complete\":true,\"rect\":[0,0,800,600],\"primary\":true}},\
         {{\"out\":\"out-b\",\"order\":[\"wb1\"],\"current\":\"wb1\",\"complete\":true,\"rect\":[800,0,800,600],\"primary\":false}}],\
         \"bindings\":[{{\"logical\":\"la\",\"native\":\"wa1\",\"out\":\"out-a\",\"pin\":false,\"disp\":false,\"managed\":true}},\
         {{\"logical\":\"lb\",\"native\":\"wb1\",\"out\":\"out-b\",\"pin\":false,\"disp\":false,\"managed\":true}}],\
         \"domains\":[{{\"id\":\"out-a\",\"ws\":\"wa1\",\"bounds\":[0,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"win-a\",\"readable\":true,\"hold\":\"\",\
         \"adjacent\":{{\"right\":\"out-b\"}},\"windows\":[{win_a}],\"closed\":[]}},\
         {{\"id\":\"out-b\",\"ws\":\"wb1\",\"bounds\":[800,0,800,600],\"gap\":8,\
         \"outer\":4,\"focused\":\"win-b\",\"readable\":true,\"hold\":\"\",\
         \"adjacent\":{{\"left\":\"out-a\"}},\"windows\":[{win_b}],\"closed\":[]}}],\
         \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}",
        win_a = window_in_json("win-a", "out-a", "wa1", 0),
        win_b = window_in_json("win-b", "out-b", "wb1", 0),
    )
}

#[test]
fn shared_fixture_result_golden_regeneration() {
    if std::env::var("UPDATE_WORLD_FIXTURES").as_deref() != Ok("1") {
        return;
    }
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire");
    // Drag-preview: seed the pair, then preview from the live pointer.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(
        &request_json(
            "owner-a",
            "adapter-1",
            "gen-1",
            "1",
            "1",
            "req-1",
            &format!("{},{}", window_json("win-a", 0), window_json("win-b", 400)),
        )
        .replace("\"focused\":\"\"", "\"focused\":\"win-a\""),
    )
    .expect("preview seed decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let preview = decode_request(&preview_fixture_request()).expect("preview decodes");
    let reply = match engine.handle_report(&preview) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected preview accepted, got {other:?}"),
    };
    assert!(encode_reply(&reply).contains("\"op\":\"drag-preview\""));
    std::fs::write(dir.join("reply-preview.json"), encode_reply(&reply)).expect("write preview");
    // Intentional float: the correlated result carries the exact one-shot
    // placement while the snapshot retains the desired frame with no write
    // authority, so a lost reply never loses the property.
    let mut float_engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&preview_seed_like()).expect("float seed decodes");
    match float_engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value = serde_json::from_str(&preview_seed_like()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("24".to_owned());
    value["command"] = serde_json::json!({
        "op": "operation",
        "command": {"op": "toggle-float", "win": "win-a", "rect": null},
    });
    let float = decode_request(&value.to_string()).expect("float decodes");
    let reply = match float_engine.handle_report(&float) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected float accepted, got {other:?}"),
    };
    assert!(encode_reply(&reply).contains("\"op\":\"toggle-float\""));
    std::fs::write(dir.join("reply-float.json"), encode_reply(&reply)).expect("write float");
    // Read-only active-group highlight: the renderer payload rides the
    // correlated result, never a border policy.
    let mut group_engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&preview_seed_like()).expect("group seed decodes");
    match group_engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value = serde_json::from_str(&preview_seed_like()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("23".to_owned());
    value["command"] = serde_json::json!({"op": "operation", "command": {"op": "active-group"}});
    let query = decode_request(&value.to_string()).expect("group decodes");
    let reply = match group_engine.handle_report(&query) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected group accepted, got {other:?}"),
    };
    assert!(encode_reply(&reply).contains("\"op\":\"active-group\""));
    std::fs::write(dir.join("reply-group.json"), encode_reply(&reply)).expect("write group");
    // Whole-workspace migration: seed both outputs, then migrate wa1 right.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&migrate_fixture_seed()).expect("migration seed decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value =
        serde_json::from_str(&migrate_fixture_seed()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("22".to_owned());
    value["command"] =
        serde_json::json!({"op": "migrate-workspace", "from": "out-a", "to": "out-b", "ws": "wa1"});
    let migrate = decode_request(&value.to_string()).expect("migration decodes");
    let reply = match engine.handle_report(&migrate) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected migration accepted, got {other:?}"),
    };
    assert!(encode_reply(&reply).contains("\"op\":\"migrate-workspace\""));
    std::fs::write(dir.join("reply-migrate.json"), encode_reply(&reply)).expect("write migrate");
}

const FIXTURE_PREVIEW: &str = include_str!("../../../test-fixtures/world-wire/reply-preview.json");
const FIXTURE_MIGRATE: &str = include_str!("../../../test-fixtures/world-wire/reply-migrate.json");
const FIXTURE_GROUP: &str = include_str!("../../../test-fixtures/world-wire/reply-group.json");
const FIXTURE_FLOAT: &str = include_str!("../../../test-fixtures/world-wire/reply-float.json");

#[test]
fn shared_fixture_result_bytes_gate() {
    // The checked-in result fixtures reproduce byte-exactly from real
    // engine runs: the kwin suite parses and applies these same bytes.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&preview_seed_like()).expect("preview seed decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let preview = decode_request(&preview_fixture_request()).expect("preview decodes");
    let reply = match engine.handle_report(&preview) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected preview accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_PREVIEW);
    assert!(FIXTURE_PREVIEW.contains("\"op\":\"drag-preview\""));
    let mut float_engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&preview_seed_like()).expect("float seed decodes");
    match float_engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value = serde_json::from_str(&preview_seed_like()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("24".to_owned());
    value["command"] = serde_json::json!({
        "op": "operation",
        "command": {"op": "toggle-float", "win": "win-a", "rect": null},
    });
    let float = decode_request(&value.to_string()).expect("float decodes");
    let reply = match float_engine.handle_report(&float) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected float accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_FLOAT);
    assert!(FIXTURE_FLOAT.contains("\"op\":\"toggle-float\""));
    let mut group_engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&preview_seed_like()).expect("group seed decodes");
    match group_engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value = serde_json::from_str(&preview_seed_like()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("23".to_owned());
    value["command"] = serde_json::json!({"op": "operation", "command": {"op": "active-group"}});
    let query = decode_request(&value.to_string()).expect("group decodes");
    let reply = match group_engine.handle_report(&query) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected group accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_GROUP);
    assert!(FIXTURE_GROUP.contains("\"op\":\"active-group\""));
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let seed = decode_request(&migrate_fixture_seed()).expect("migration seed decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected seed accepted, got {other:?}"),
    }
    let mut value: serde_json::Value =
        serde_json::from_str(&migrate_fixture_seed()).expect("valid");
    value["obs"] = serde_json::Value::String("2".to_owned());
    value["reqSeq"] = serde_json::Value::String("2".to_owned());
    value["req"] = serde_json::Value::String("req-2".to_owned());
    value["cmdSeq"] = serde_json::Value::String("22".to_owned());
    value["command"] =
        serde_json::json!({"op": "migrate-workspace", "from": "out-a", "to": "out-b", "ws": "wa1"});
    let migrate = decode_request(&value.to_string()).expect("migration decodes");
    let reply = match engine.handle_report(&migrate) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected migration accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_MIGRATE);
    assert!(FIXTURE_MIGRATE.contains("\"members\":[\"win-a\"]"));
}

/// Cross-language byte gate over the shared TS-captured fixtures: the same
/// artifact bytes the kwin suite asserts its production payloads against
/// must decode fully here, with the structural estimator bounding the
/// exact length, and the engine must reproduce the golden reply bytes the
/// TS suite parses and applies. No node runtime on this side.
const FIXTURE_OBSERVE: &str = include_str!("../../../test-fixtures/world-wire/report-observe.json");
const FIXTURE_SEND: &str = include_str!("../../../test-fixtures/world-wire/report-send.json");
const FIXTURE_DRAGDROP: &str =
    include_str!("../../../test-fixtures/world-wire/report-dragdrop.json");
const FIXTURE_GOLDEN: &str = include_str!("../../../test-fixtures/world-wire/reply-golden.json");
const FIXTURE_SIZE_HINTS: &str =
    include_str!("../../../test-fixtures/world-wire/report-kwin-size-hints.json");
const FIXTURE_SIZE_HINTS_SELECT: &str =
    include_str!("../../../test-fixtures/world-wire/report-kwin-size-hints-select.json");
const FIXTURE_OUTER_GAP_REPORT: &str =
    include_str!("../../../test-fixtures/world-wire/report-outer-gap.json");
const FIXTURE_OUTER_GAP_GOLDEN: &str =
    include_str!("../../../test-fixtures/world-wire/reply-outer-gap.json");

#[test]
fn shared_fixture_actual_bytes_gate() {
    use tiler_core::boundary::CoreCommand;
    use tiler_core::world_report::WorldCommand;
    // Every captured payload decodes fully with the estimator bounding
    // the exact byte length (proves the repaired report_estimate on
    // actual production bytes, not just synthetic shapes).
    for (name, json) in [
        ("report-observe", FIXTURE_OBSERVE),
        ("report-send", FIXTURE_SEND),
        ("report-dragdrop", FIXTURE_DRAGDROP),
    ] {
        let report = decode_request(json).unwrap_or_else(|e| panic!("{name} decodes fully: {e:?}"));
        let estimate = tiler_core::world_report::report_estimate(&report);
        assert!(
            estimate >= json.len(),
            "{name} estimate {estimate} bounds actual {len}",
            len = json.len(),
        );
    }
    // Full observation: two outputs, three bindings, minimized lifetime,
    // 128-char id, extrema, and the excluded game row.
    let observe = decode_request(FIXTURE_OBSERVE).expect("decodes");
    assert_eq!(observe.topology.len(), 2);
    // Live native output facts ride every topology entry: real screen
    // geometry plus the active-screen primary (never window frames).
    assert!(
        observe
            .topology
            .iter()
            .all(|t| t.geometry.is_some() && t.complete)
    );
    assert_eq!(
        observe.topology.iter().filter(|t| t.primary).count(),
        1,
        "exactly one wired primary"
    );
    assert!(observe.bindings.len() >= 3);
    assert!(
        observe
            .bindings
            .iter()
            .all(|b| !b.pinned && !b.displaced && b.managed)
    );
    assert!(observe.minimized.iter().any(|m| m.window.0 == "win-min"));
    assert!(
        observe
            .domains
            .iter()
            .flat_map(|d| d.windows.iter())
            .any(|w| w.window.0.len() == 128)
    );
    assert!(
        observe
            .domains
            .iter()
            .flat_map(|d| d.windows.iter())
            .any(|w| w.window.0 == "win-game" && w.fullscreen && w.fit_excluded)
    );
    // Send command with exactly the target binding pinned.
    let send = decode_request(FIXTURE_SEND).expect("decodes");
    match &send.command {
        Some(WorldCommand::Send {
            window,
            target_workspace,
            follow,
            ..
        }) => {
            assert_eq!(window.0, "win-a");
            assert_eq!(target_workspace, "ws-2");
            assert!(!follow);
        }
        other => panic!("expected send, got {other:?}"),
    }
    let pinned: Vec<_> = send.bindings.iter().filter(|b| b.pinned).collect();
    assert_eq!(pinned.len(), 1, "exactly the send target pins");
    assert_eq!(pinned[0].native_id, "ws-2");
    // Drag-drop operation with the Started source and no invented hover.
    let drag = decode_request(FIXTURE_DRAGDROP).expect("decodes");
    match &drag.command {
        Some(WorldCommand::Operation(CoreCommand::DragDrop {
            window,
            x,
            y,
            hover_prior,
            source,
        })) => {
            assert_eq!(window, "win-a");
            assert_eq!((*x, *y), (1500, 900));
            assert!(hover_prior.is_none());
            let key = source.as_ref().expect("source carried");
            assert_eq!(key.output.0, "out-1");
            assert_eq!(key.workspace.0, "ws-1");
        }
        other => panic!("expected drag-drop, got {other:?}"),
    }
    // The engine reproduces the golden reply bytes exactly; the kwin
    // world-bytes suite parses and applies these same bytes in-flow.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = match engine.handle_report(&observe) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_GOLDEN);
}

#[test]
fn shared_fixture_outer_gap_insets_all_edges() {
    // Real TS producer bytes: 800x600 work area, inner 8, outer 12, two
    // windows. Semantic edge assertions (never a bare golden compare):
    // inset gives 776x576 at (12,12) with one canonical inner gap of 8.
    let report = decode_request(FIXTURE_OUTER_GAP_REPORT).expect("decodes");
    assert_eq!(report.domains.len(), 1);
    assert_eq!(report.domains[0].domain.gap, 8);
    assert_eq!(report.domains[0].outer_gap, 12);
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(encode_reply(&reply), FIXTURE_OUTER_GAP_GOLDEN);
    let mut frames: Vec<(i32, i32, i32, i32)> = reply
        .windows
        .iter()
        .filter_map(|w| w.effective_frame.map(|r| (r.x, r.y, r.w, r.h)))
        .collect();
    assert_eq!(frames.len(), 2);
    frames.sort();
    assert_eq!(frames[0], (12, 12, 384, 576), "left tile at inset edge");
    assert_eq!(frames[1], (404, 12, 384, 576), "right tile at inset edge");
    assert_eq!(
        frames[1].0 - (frames[0].0 + frames[0].2),
        8,
        "canonical inner gap"
    );
}

#[test]
fn kwin_real_size_hints_accept_through_repeated_observes() {
    // Native Qt QSize shapes (see the TS-captured
    // report-kwin-size-hints.json gate below for the actual producer
    // bytes): a 0x0 minimum means no minimum, QWIDGETSIZE_MAX 16777215
    // and i32::MAX mean no maximum, fixed dialogs carry equal nonzero
    // pins, and width-fixed clients carry partial-zero vectors. The dev
    // log carries no payload (bounded summaries only), so these shapes
    // are inference from the deterministic Qt/KWin field defaults, not
    // observed log bytes. All are advisory (sanitize-at-use in
    // size_hints::meaningful) and must never fail validation: the pre-fix
    // gate refused them as snapshot-invalid, rejecting every DescribeWorld
    // carrying one from the first report on. The loop below mirrors the
    // production failure (back-to-back observation-only reports with fresh
    // obs/reqSeq/req, as the adapter re-dispatches after each rejection).
    fn window_with_hints(id: &str, x: i32, hints: &str) -> String {
        format!(
            "{{\"win\":\"{id}\",\"out\":\"out\",\"ws\":\"ws\",\"rect\":[{x},0,200,200],\
             \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
             \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
             \"fixedSuppress\":false,\"hints\":{hints}}}"
        )
    }
    fn observe_request(obs: u64, req_seq: u64, req: &str, windows: &str) -> String {
        format!(
            "{{\"v\":1,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\
             \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"{req}\",\"cmdSeq\":null,\
             \"command\":null,\"mode\":\"per-output-local\",\
             \"topology\":[{{\"out\":\"out\",\"order\":[\"ws\"],\"current\":\"ws\",\
             \"complete\":true,\"rect\":null,\"primary\":false}}],\
             \"bindings\":[],\
             \"domains\":[{{\"id\":\"out\",\"ws\":\"ws\",\"bounds\":[0,0,800,600],\"gap\":8,\
             \"outer\":4,\"focused\":\"\",\"readable\":true,\"hold\":\"\",\"adjacent\":{{}},\
             \"windows\":[{windows}],\"closed\":[]}}],\
             \"minimized\":[],\"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\"defaultTiled\":true,\
             \"focused\":\"\",\"complete\":true,\"gesture\":false,\"feedback\":[]}}"
        )
    }
    let windows = [
        // KWin default: no minimum, no maximum.
        window_with_hints(
            "win-plain",
            0,
            "{\"minW\":0,\"minH\":0,\"maxW\":16777215,\"maxH\":16777215}",
        ),
        // Fixed dialog: equal nonzero pins.
        window_with_hints(
            "win-fixed",
            210,
            "{\"minW\":400,\"minH\":300,\"maxW\":400,\"maxH\":300}",
        ),
        // Width-fixed client: partial-zero vector.
        window_with_hints(
            "win-wide",
            420,
            "{\"minW\":640,\"minH\":0,\"maxW\":640,\"maxH\":0}",
        ),
        // Unbounded max sentinel: never a real cap.
        window_with_hints(
            "win-sentinel",
            630,
            "{\"minW\":null,\"minH\":null,\"maxW\":2147483647,\"maxH\":2147483647}",
        ),
    ]
    .join(",");
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    for round in 1..=6u64 {
        let json = observe_request(round, round, &format!("req-{round}"), &windows);
        let report = decode_request(&json).expect("real KWin hints decode");
        match engine.handle_report(&report) {
            WorldOutcome::Accepted(reply) => {
                assert_eq!(reply.state_revision, round);
            }
            other => panic!("round {round}: expected accepted, got {other:?}"),
        }
    }
}

#[test]
fn snapshot_rejection_names_fixed_check_reason() {
    // Validation failures share kind snapshot-invalid but must name the
    // failed check in fixed vocabulary (never payload bytes), so one log
    // line diagnoses the report without source archaeology.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["domains"][0]["windows"] = serde_json::json!([
        serde_json::from_str::<serde_json::Value>(&window_json("win-a", 0)).expect("valid"),
        serde_json::from_str::<serde_json::Value>(&window_json("win-a", 210)).expect("valid"),
    ]);
    let error = decode_request(&value.to_string()).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-window");
}

#[test]
fn shared_fixture_size_hints_gate() {
    // TS-captured actual producer bytes (see kwin/tests/world-bytes
    // makeSizeHintsSurface): native QSize default minimums (0x0),
    // QWIDGETSIZE_MAX and i32::MAX unbounded maximums, a partial-zero
    // width pin, and an equal nonzero fixed pin. The dev log carries no
    // payload (summaries only), so this shape is inference from the
    // deterministic Qt/KWin field defaults, not observed log bytes; the
    // pre-fix gate refused exactly these shapes as snapshot-invalid.
    for (name, json) in [
        ("report-kwin-size-hints", FIXTURE_SIZE_HINTS),
        ("report-kwin-size-hints-select", FIXTURE_SIZE_HINTS_SELECT),
    ] {
        let report = decode_request(json).unwrap_or_else(|e| panic!("{name} decodes fully: {e:?}"));
        let estimate = tiler_core::world_report::report_estimate(&report);
        assert!(
            estimate >= json.len(),
            "{name} estimate {estimate} bounds actual {len}",
            len = json.len(),
        );
    }
    let observe = decode_request(FIXTURE_SIZE_HINTS).expect("size hints decode");
    let rows: Vec<_> = observe
        .domains
        .iter()
        .flat_map(|d| d.windows.iter())
        .collect();
    let row = |id: &str| {
        rows.iter()
            .find(|w| w.window.0 == id)
            .unwrap_or_else(|| panic!("expected row {id}"))
    };
    // Raw Qt shapes survive verbatim into the typed report.
    assert_eq!(row("win-default").hints.min_w, Some(0));
    assert_eq!(row("win-default").hints.min_h, Some(0));
    assert_eq!(row("win-default").hints.max_w, Some(16777215));
    assert_eq!(row("win-default").hints.max_h, Some(16777215));
    assert_eq!(row("win-sentinel").hints.max_w, Some(i32::MAX));
    assert_eq!(row("win-sentinel").hints.max_h, Some(i32::MAX));
    assert_eq!(row("win-wide").hints.min_w, Some(640));
    assert_eq!(row("win-wide").hints.min_h, Some(0));
    assert_eq!(row("win-wide").hints.max_w, Some(640));
    assert_eq!(row("win-wide").hints.max_h, Some(0));
    // fixedAuto classification stays meaningful under the default
    // both-axes predicate: only the equal nonzero pin floats.
    assert!(row("win-fixed").fixed_auto);
    assert!(row("win-fixed").floating);
    assert!(!row("win-default").fixed_auto);
    assert!(!row("win-sentinel").fixed_auto);
    assert!(!row("win-wide").fixed_auto);
    // The follow-up select executes on the engine (never catchup-dropped
    // by the adapter, never re-refused here): rev advances past the seed.
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    match engine.handle_report(&observe) {
        WorldOutcome::Accepted(reply) => assert_eq!(reply.state_revision, 1),
        other => panic!("expected observe accepted, got {other:?}"),
    }
    let select = decode_request(FIXTURE_SIZE_HINTS_SELECT).expect("select decodes");
    assert_eq!(select.command_seq, Some(1));
    match &select.command {
        Some(tiler_core::world_report::WorldCommand::SelectNumbered { output, index }) => {
            assert_eq!(output.0, "out-1");
            assert_eq!(*index, 2);
        }
        other => panic!("expected select-numbered, got {other:?}"),
    }
    match engine.handle_report(&select) {
        WorldOutcome::Accepted(reply) => assert_eq!(reply.state_revision, 2),
        other => panic!("expected select accepted, got {other:?}"),
    }
    // Advisory extras rooted in the same captured bytes: negative and
    // out-of-bound hints still decode and accept (sanitize-at-use), while
    // non-integer JSON and out-of-i32 numbers still refuse.
    let mut advisory =
        serde_json::from_str::<serde_json::Value>(FIXTURE_SIZE_HINTS).expect("valid");
    advisory["domains"][0]["windows"][0]["hints"] =
        serde_json::json!({"minW": -5, "minH": 0, "maxW": 99999, "maxH": null});
    let report = decode_request(&advisory.to_string()).expect("advisory hints decode");
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected advisory accepted, got {other:?}"),
    }
    let mut bad = serde_json::from_str::<serde_json::Value>(FIXTURE_SIZE_HINTS).expect("valid");
    bad["domains"][0]["windows"][0]["hints"] =
        serde_json::json!({"minW": "640", "minH": 0, "maxW": 640, "maxH": 0});
    let error = decode_request(&bad.to_string()).expect_err("string hint refuses");
    assert_eq!(error.kind, "malformed");
    let mut bad = serde_json::from_str::<serde_json::Value>(FIXTURE_SIZE_HINTS).expect("valid");
    bad["domains"][0]["windows"][0]["hints"] =
        serde_json::json!({"minW": 0, "minH": 0, "maxW": 2147483648u64, "maxH": 0});
    let error = decode_request(&bad.to_string()).expect_err("out-of-i32 hint refuses");
    assert_eq!(error.kind, "malformed");
}
