//! Stage 5 production-route fixture proof (broker leg).
//!
//! Replays the genuine v4 producer captures recorded under
//! `test-fixtures/stage5-v4-active/{full,s2}` (offline `broker_world_eval`
//! entry recordings over the `kwin-stage5-v4` adapter, carrying the required
//! `sharedNativeRoster` handshake capability, the `foregroundKnown` report
//! bit, and the nullable `closeFocus` reply lane) through the production
//! `WorldService` capture/read path, and asserts the served projections equal
//! the recorded `projection-*.json` bytes exactly.
//!
//! Frozen `test-fixtures/stage5-v4` predates those three keys and stays
//! untouched history: the frozen-shape test below pins its in-band
//! rejection under the current strict shape.
//!
//! Volatile-token normalization (gen-only, shape-preserving):
//! - `gen` is the harness-fixed `gen-stage5-v4-active` in every fixture; the test
//!   substitutes the live broker generation at feed time and normalizes it
//!   back before comparing. No other field is touched: `source`,
//!   `sourceSeq`, `nativeInvalid`, `req`, and every report/facts/gesture
//!   lane stay byte-identical.
//!
//! Read requests are exactly `{"v":4,"req":"<recorded projection req>"}`:
//! the broker read parser accepts precisely `{v,req}`, so no `gen` is sent
//! (the served `gen` is stamped by the broker and normalized back for the
//! byte comparison).
//!
//! Clock-epoch replay seam: the broker's foreign-epoch guard requires every
//! recorded `attemptMs` to fit the dispatch clock, and the trailing spare
//! creation is minted at genesis-0 against the handshake-reset clock while
//! its fulfillment receipt rides genesis-1 with the recorded stamp (the
//! recorder holds the first capture send one bounded 100ms, so both chains
//! stamp ~=100+: a postdated mint latches `ambiguous-creation`
//! permanently). The record run was slower than a fresh replay, so the test pre-reads every replay byte before discovery, feeds
//! genesis-0 immediately on the fresh clock, then settles one bounded sleep
//! past the maximum recorded stamp before feeding genesis-1 on. Attempt
//! bytes and the clock oracle are untouched; only replay timing adapts.
//!
//! A third test pins the chain join on the same v4 bytes: each recorded
//! envelope's witness lane equals the covered witness of the projection it
//! produced, and each facts input names the witness its captures cover.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-stage5-v4";
const GEN: &str = "gen-stage5-v4-active";

fn read_fixture(chain: &str, name: &str) -> String {
    std::fs::read_to_string(format!(
        "../../test-fixtures/stage5-v4-active/{chain}/{name}"
    ))
    .expect("stage5-v4-active fixture is checked in")
}

/// One recorded step: every envelope replays in order, then the served
/// projection must equal the recorded bytes. `preview` is the required
/// `(producerInstance, producerSerial)` on preview steps, `None` where the
/// recorded projection carries a null preview. `hold_barrier` is true where
/// the recorded windows carry `frame_hold="gesture-barrier"` (native gesture
/// open) and false where every hold is empty.
struct Step {
    envelopes: &'static [&'static str],
    projection: &'static str,
    preview: Option<(&'static str, &'static str)>,
    hold_barrier: bool,
}

fn discovered(chain: &str) -> (WorldService, String) {
    let service = WorldService::new();
    // Actual recorded handshake bytes, verbatim (no generation inside).
    let handshake = read_fixture(chain, "discovery-handshake.request.json");
    let discovery: serde_json::Value =
        serde_json::from_str(&service.discover(&handshake).expect("handshake routes"))
            .expect("handshake reply is JSON");
    assert_eq!(discovery["type"], "handshake-reply");
    assert_eq!(discovery["v"], 4);
    assert_eq!(discovery["ready"], false);
    assert_eq!(discovery["adapter"], ADAPTER);
    assert_eq!(discovery["req"], "disc-1");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    // Actual recorded settings bytes with only the live generation
    // substituted in.
    let settings = read_fixture(chain, "discovery-settings.request.json").replace(GEN, &generation);
    let settings_reply: serde_json::Value =
        serde_json::from_str(&service.discover(&settings).expect("settings route"))
            .expect("settings reply is JSON");
    assert_eq!(settings_reply["type"], "settings-reply");
    assert_eq!(settings_reply["v"], 4);
    assert_eq!(settings_reply["adapter"], ADAPTER);
    assert_eq!(
        settings_reply["gen"],
        serde_json::Value::String(generation.clone())
    );
    assert_eq!(settings_reply["req"], "set-1");
    assert_eq!(settings_reply["rev"], "1");
    assert_eq!(settings_reply["status"], "accepted");
    (service, generation)
}

/// Recorded creation-receipt margin: every `creationAttempts` stamp in the
/// replay must clear 100ms. The recorder holds the first capture send one
/// bounded 100ms so the genuine receipt stamps solidly positive; this pins
/// that seam (a zero-stamp receipt would need a sub-ms mint to settle).
fn assert_creation_receipt_margin(chain: &str, replay: &[(&str, String)]) {
    let mut count = 0;
    for (envelope, body) in replay {
        let value: serde_json::Value = serde_json::from_str(body).expect("envelope is JSON");
        if let Some(items) = value["report"]["creationAttempts"].as_array() {
            for item in items {
                let stamp = item["attemptMs"]
                    .as_str()
                    .expect("creation receipt stamps attemptMs");
                let ms: u64 = stamp.parse().expect("creation stamp is u64");
                assert!(
                    ms >= 100,
                    "{chain}/{envelope}: creation receipt stamp {ms}ms lacks the positive-stamp margin"
                );
                count += 1;
            }
        }
    }
    assert!(
        count > 0,
        "{chain}: replay carries no creation receipt to settle"
    );
}

/// Maximum recorded `attemptMs` over pre-read replay bodies across the
/// exact five mechanical attempt lanes the broker's foreign-epoch guard
/// checks. Bounded so the settle sleep can never wait unboundedly.
fn clock_settle_ms(chain: &str, replay: &[(&str, String)]) -> u64 {
    let mut max = 0u64;
    for (envelope, body) in replay {
        let value: serde_json::Value = serde_json::from_str(body).expect("envelope is JSON");
        let report = &value["report"];
        for lane in [
            "frameAttempts",
            "memberAttempts",
            "viewAttempts",
            "clearAttempts",
            "creationAttempts",
        ] {
            if let Some(items) = report[lane].as_array() {
                for item in items {
                    if let Some(stamp) = item["attemptMs"].as_str()
                        && let Ok(ms) = stamp.parse::<u64>()
                    {
                        max = max.max(ms);
                    }
                }
            }
        }
    }
    assert!(
        max <= 1000,
        "{chain}: recorded attempt clock {max}ms exceeds the bounded settle wait"
    );
    max
}

/// Feed one pre-read envelope (live generation substituted in) and assert the
/// capture is a genuine v4 acceptance bound to the right adapter, generation,
/// source, and watermark, with ready set.
fn feed_raw(
    service: &WorldService,
    generation: &str,
    envelope_name: &str,
    recorded: &str,
) -> serde_json::Value {
    let recorded_value: serde_json::Value =
        serde_json::from_str(&recorded).expect("envelope is JSON");
    let live = recorded.replace(GEN, generation);
    let reply: serde_json::Value =
        serde_json::from_str(&service.evaluate_capture(&live).expect("in-band"))
            .expect("capture reply is JSON");
    assert!(
        reply.get("kind").is_none(),
        "{envelope_name}: genuine v4 capture must be accepted, got {reply}"
    );
    assert_eq!(reply["v"], 4, "{envelope_name}: reply must be v4");
    assert_eq!(reply["ready"], true, "{envelope_name}: reply must be ready");
    assert_eq!(reply["adapter"], ADAPTER);
    assert_eq!(
        reply["gen"],
        serde_json::Value::String(generation.to_owned())
    );
    assert_eq!(reply["source"], recorded_value["source"]);
    assert_eq!(reply["nativeInvalid"], recorded_value["nativeInvalid"]);
    reply
}

/// Read the served projection for one recorded step and assert it equals the
/// recorded broker bytes exactly after gen-only normalization, plus the
/// step's semantic pins.
fn read_step(
    service: &WorldService,
    chain: &str,
    generation: &str,
    step: &Step,
    last_envelope: &str,
) {
    let recorded = read_fixture(chain, step.projection);
    let recorded_value: serde_json::Value =
        serde_json::from_str(&recorded).expect("projection is JSON");
    let req = recorded_value["req"]
        .as_str()
        .expect("recorded req")
        .to_owned();
    let body = service
        .read_presentation(&format!("{{\"v\":4,\"req\":\"{req}\"}}"))
        .expect("in-band");
    assert_eq!(
        body.replace(generation, GEN),
        recorded,
        "{}: served projection must equal the recorded broker bytes",
        step.projection
    );
    let served: serde_json::Value = serde_json::from_str(&body).expect("served is JSON");
    assert_eq!(served["v"], 4);
    // Underlay is found for win-a with both members on every step.
    assert_eq!(served["presentation"]["underlay"]["status"], "found");
    assert_eq!(served["presentation"]["underlay"]["win"], "win-a");
    let mut members: Vec<&str> = served["presentation"]["underlay"]["members"]
        .as_array()
        .expect("underlay members")
        .iter()
        .map(|m| m["win"].as_str().expect("member win"))
        .collect();
    members.sort_unstable();
    assert_eq!(members, vec!["win-a", "win-b"]);
    // Coverage equals the latest captured nativeInvalidWitness.
    let envelope_value: serde_json::Value =
        serde_json::from_str(&read_fixture(chain, last_envelope)).expect("envelope is JSON");
    let witness = &envelope_value["report"]["nativeInvalidWitness"];
    assert!(
        witness.is_object(),
        "{last_envelope}: capture must prove a witness"
    );
    assert_eq!(served["coveredInstance"], witness["instance"]);
    assert_eq!(served["coveredInvalid"], witness["counter"]);
    // Preview lives at the projection top level: required on the recorded
    // preview steps with the recorded native producer identity, and null
    // elsewhere.
    match step.preview {
        Some((instance, serial)) => {
            assert_eq!(served["preview"]["producerInstance"], instance);
            assert_eq!(served["preview"]["producerSerial"], serial);
            assert_eq!(served["preview"]["win"], "win-b");
        }
        None => assert!(
            served["preview"].is_null(),
            "{}: no preview without an open native producer",
            step.projection
        ),
    }
    // Every frame is held by the gesture barrier while a native gesture is
    // open, and no frame is held otherwise.
    let windows = served["windows"].as_array().expect("served windows");
    assert!(!windows.is_empty());
    for window in windows {
        if step.hold_barrier {
            assert_eq!(window["frame_auth"], "hold");
            assert_eq!(window["frame_hold"], "gesture-barrier");
        } else {
            assert_eq!(window["frame_hold"], "");
        }
    }
    // The covered steps carry the genuine visible anchored border: the exact
    // 7-key shape, visible with no hide reason, subject win-a, and a witness
    // that joins win-a as both window and foreground over a canonical
    // watermark and a rect frame. (Byte equality with the recorded broker
    // bytes is asserted above; this pins the meaning, not the bytes.)
    let border = &served["presentation"]["border"];
    let mut keys: Vec<&str> = border
        .as_object()
        .expect("border is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "outsets", "radius", "reason", "subject", "visible", "width", "witness"
        ]
    );
    assert_eq!(border["visible"], true);
    assert_eq!(border["reason"], "");
    assert_eq!(border["subject"], "win-a");
    let witness = &border["witness"];
    assert_eq!(witness["win"], "win-a");
    assert_eq!(witness["foreground"], "win-a");
    let obs = witness["obs"].as_str().expect("border witness obs");
    assert!(!obs.is_empty() && obs.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(witness["frame"].as_array().expect("border frame").len(), 4);
}

fn run_chain(chain: &str, genesis: &[&str], steps: &[Step], post_discovery_delay_ms: u64) {
    // Every replay byte is pre-read before discovery: the handshake resets
    // the broker clock, genesis-0 mints the trailing spare creation against
    // it, and the recorded receipt in genesis-1 must postdate that mint.
    // The recorded receipts clear 100ms (recorder seam), so even a
    // double-digit mint holds with wide margin; no sub-ms race is required.
    let mut replay: Vec<(&str, String)> = Vec::new();
    for &name in genesis {
        replay.push((name, read_fixture(chain, name)));
    }
    for step in steps {
        for &name in step.envelopes {
            replay.push((name, read_fixture(chain, name)));
        }
    }
    let settle = clock_settle_ms(chain, &replay);
    assert_creation_receipt_margin(chain, &replay);
    let (service, generation) = discovered(chain);
    if post_discovery_delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(post_discovery_delay_ms));
    }
    let mut cursor = 0;
    // Mint on the fresh clock: no file IO or sleep may sit between the
    // handshake reset and this feed.
    {
        let entry: &(&str, String) = &replay[cursor];
        feed_raw(&service, &generation, entry.0, &entry.1);
        cursor += 1;
    }
    // Settle one bounded sleep past the maximum recorded attempt stamp so
    // the foreign-epoch guard and the mint-before-receipt order both hold
    // for the receipt in genesis-1 and every later capture.
    std::thread::sleep(std::time::Duration::from_millis(settle));
    for _ in 1..genesis.len() {
        let entry: &(&str, String) = &replay[cursor];
        feed_raw(&service, &generation, entry.0, &entry.1);
        cursor += 1;
    }
    for step in steps {
        let mut last = "";
        for _ in step.envelopes {
            let entry: &(&str, String) = &replay[cursor];
            feed_raw(&service, &generation, entry.0, &entry.1);
            last = entry.0;
            cursor += 1;
        }
        read_step(&service, chain, &generation, step, last);
    }
}

/// Full chain: three genesis captures (initial, attempt, settled), then P5,
/// pre-move P6, the covering capture, the proven A preview, the cover2 double
/// capture (one omits the idle gesture lane), the still-open-A drain, the
/// inv3 drain, and the valid B repaint from a new B instance.
#[test]
fn stage5_fixture_route_matches_recorded_projections() {
    run_chain("full", GENESIS, &full_steps(), 0);
}

/// Same-source serial handover: gesture A (1/1) opens and shows, then B (1/2
/// on the same native source) goes live with no covering capture between
/// them. The B capture carries the advanced witness with the same-source
/// serial as its proven producer.
#[test]
fn stage5_fixture_route_same_source_serial_b() {
    run_chain("s2", GENESIS, &s2_steps(), 0);
}

/// Delayed-discovery replay regression: both chains replayed with a bounded
/// 10ms hold between discovery and genesis-0 still settle the recorded
/// creation exactly. This permanently guards the recorder's positive-stamp
/// seam: double-digit mints precede the >=100 receipts with wide margin, so
/// no sub-ms handshake-to-genesis race is required. Against the old
/// zero-stamp bytes this delay goes deterministically red (a ~10ms mint
/// postdates receipt 0 and latches), never timeout-flaky.
#[test]
fn stage5_fixture_delayed_discovery_replay_settles() {
    run_chain("full", GENESIS, &full_steps(), 10);
    run_chain("s2", GENESIS, &s2_steps(), 10);
}

const GENESIS: &[&str] = &[
    "envelope-genesis-0.json",
    "envelope-genesis-1.json",
    "envelope-genesis-2.json",
];

fn full_steps() -> Vec<Step> {
    vec![
        Step {
            envelopes: &["envelope-p5-0.json"],
            projection: "projection-p5.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-p6-0.json"],
            projection: "projection-p6.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-cover-0.json"],
            projection: "projection-cover.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-preview-0.json"],
            projection: "projection-preview.json",
            preview: Some(("gesture-1", "1")),
            hold_barrier: true,
        },
        Step {
            envelopes: &["envelope-cover2-0.json", "envelope-cover2-1.json"],
            projection: "projection-cover2.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-a2-0.json"],
            projection: "projection-a2.json",
            preview: Some(("gesture-1", "1")),
            hold_barrier: true,
        },
        Step {
            envelopes: &["envelope-a3-0.json"],
            projection: "projection-a3.json",
            preview: Some(("gesture-1", "1")),
            hold_barrier: true,
        },
        Step {
            envelopes: &["envelope-preview-b-0.json"],
            projection: "projection-preview-b.json",
            preview: Some(("9d4e6a2c-1b3f-4d8e-9f0a-2c5e7a1b3d4f", "1")),
            hold_barrier: true,
        },
    ]
}

fn s2_steps() -> Vec<Step> {
    vec![
        Step {
            envelopes: &["envelope-p5-0.json"],
            projection: "projection-p5.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-p6-0.json"],
            projection: "projection-p6.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-cover-0.json"],
            projection: "projection-cover.json",
            preview: None,
            hold_barrier: false,
        },
        Step {
            envelopes: &["envelope-preview-0.json"],
            projection: "projection-preview.json",
            preview: Some(("gesture-1", "1")),
            hold_barrier: true,
        },
        Step {
            envelopes: &["envelope-s2-0.json", "envelope-s2-1.json"],
            projection: "projection-s2.json",
            preview: Some(("gesture-1", "2")),
            hold_barrier: true,
        },
    ]
}

/// Each recorded final envelope's witness lane (built by the real entry
/// route from the real facts bytes) equals the covered witness of the
/// projection the broker served for it; an intermediate capture that predates
/// its step's facts joins those predecessor facts instead. The facts ->
/// capture -> coverage chain joins on identical values, not merely identical
/// key names.
#[test]
fn stage5_fixture_witness_chain_joins() {
    for (chain, envelopes) in [
        (
            "full",
            vec![
                ("envelope-p5-0.json", "projection-p5.json"),
                ("envelope-p6-0.json", "projection-p6.json"),
                ("envelope-cover-0.json", "projection-cover.json"),
                ("envelope-preview-0.json", "projection-preview.json"),
                ("envelope-cover2-1.json", "projection-cover2.json"),
                ("envelope-a2-0.json", "projection-a2.json"),
                ("envelope-a3-0.json", "projection-a3.json"),
                ("envelope-preview-b-0.json", "projection-preview-b.json"),
            ],
        ),
        (
            "s2",
            vec![
                ("envelope-p5-0.json", "projection-p5.json"),
                ("envelope-p6-0.json", "projection-p6.json"),
                ("envelope-cover-0.json", "projection-cover.json"),
                ("envelope-preview-0.json", "projection-preview.json"),
                ("envelope-s2-0.json", "projection-s2.json"),
                ("envelope-s2-1.json", "projection-s2.json"),
            ],
        ),
    ] {
        for (envelope, projection) in envelopes {
            let envelope_value: serde_json::Value =
                serde_json::from_str(&read_fixture(chain, envelope)).expect("envelope is JSON");
            let witness = envelope_value["report"]["nativeInvalidWitness"].clone();
            assert!(
                witness.is_object(),
                "{chain}/{envelope}: capture must carry a proven witness lane"
            );
            let projection_value: serde_json::Value =
                serde_json::from_str(&read_fixture(chain, projection)).expect("projection is JSON");
            assert_eq!(
                projection_value["coveredInstance"], witness["instance"],
                "{chain}/{envelope}: broker must serve the captured witness instance"
            );
            assert_eq!(
                projection_value["coveredInvalid"], witness["counter"],
                "{chain}/{envelope}: broker must serve the captured witness counter"
            );
        }
    }
    // Intermediate captures join the facts actually present at their
    // arrival: the first cover2 capture predates the step's inv2 facts and
    // still covers the inv1 predecessor, while the first same-source capture
    // already arrives after its step's inv2 facts and covers them.
    for (chain, envelope, facts) in [
        ("full", "envelope-cover2-0.json", "facts-cover-0.json"),
        ("s2", "envelope-s2-0.json", "facts-s2-0.json"),
    ] {
        let envelope_value: serde_json::Value =
            serde_json::from_str(&read_fixture(chain, envelope)).expect("envelope is JSON");
        let witness = envelope_value["report"]["nativeInvalidWitness"].clone();
        assert!(
            witness.is_object(),
            "{chain}/{envelope}: capture must carry a proven witness lane"
        );
        let facts_value: serde_json::Value =
            serde_json::from_str(&read_fixture(chain, facts)).expect("facts are JSON");
        assert_eq!(
            facts_value["instance"], witness["instance"],
            "{chain}/{envelope}: capture must cover the arrived facts instance"
        );
        assert_eq!(
            facts_value["invalidation"], witness["counter"],
            "{chain}/{envelope}: capture must cover the arrived facts invalidation"
        );
    }
    // Each facts fixture names the witness its captures cover: inv0 the
    // P5/P6 baseline, inv1 the covering capture and the A preview, inv2 the
    // second-activation return, the A drains, and the same-source serial,
    // inv3 the start-B boundary and the valid-B repaint.
    for (chain, facts, projection) in [
        ("full", "facts-p5-0.json", "projection-p5.json"),
        ("full", "facts-p5-0.json", "projection-p6.json"),
        ("full", "facts-cover-0.json", "projection-cover.json"),
        ("full", "facts-cover-0.json", "projection-preview.json"),
        ("full", "facts-cover2-0.json", "projection-cover2.json"),
        ("full", "facts-cover2-0.json", "projection-a2.json"),
        ("full", "facts-a3-0.json", "projection-a3.json"),
        ("full", "facts-a3-0.json", "projection-preview-b.json"),
        ("s2", "facts-p5-0.json", "projection-p5.json"),
        ("s2", "facts-p5-0.json", "projection-p6.json"),
        ("s2", "facts-cover-0.json", "projection-cover.json"),
        ("s2", "facts-cover-0.json", "projection-preview.json"),
        ("s2", "facts-s2-0.json", "projection-s2.json"),
    ] {
        let facts_value: serde_json::Value =
            serde_json::from_str(&read_fixture(chain, facts)).expect("facts are JSON");
        let projection_value: serde_json::Value =
            serde_json::from_str(&read_fixture(chain, projection)).expect("projection is JSON");
        assert_eq!(
            projection_value["coveredInstance"], facts_value["instance"],
            "{chain}/{facts}: broker must serve the published facts instance for {projection}"
        );
        assert_eq!(
            projection_value["coveredInvalid"], facts_value["invalidation"],
            "{chain}/{facts}: broker must serve the published invalidation witness for {projection}"
        );
    }
}

/// Frozen `stage5-v4` history predates the required `sharedNativeRoster`
/// handshake capability and the `foregroundKnown` report bit, so the current
/// strict shape rejects those bytes in-band. The frozen tree stays
/// untouched; the active target lives in `stage5-v4-active`.
#[test]
fn stage5_fixture_frozen_v4_shape_rejects() {
    let frozen_handshake = std::fs::read_to_string(
        "../../test-fixtures/stage5-v4/full/discovery-handshake.request.json",
    )
    .expect("frozen stage5-v4 handshake is checked in");
    assert!(
        !frozen_handshake.contains("sharedNativeRoster"),
        "frozen handshake predates the new capability key"
    );
    let fresh = WorldService::new();
    let reply: serde_json::Value =
        serde_json::from_str(&fresh.discover(&frozen_handshake).expect("in-band"))
            .expect("rejection is JSON");
    assert_eq!(reply["outcome"], "rejected");
    assert!(reply.get("kind").is_some());

    // The envelope rejection is proven past discovery on the live active
    // route, so a generation mismatch cannot mask the shape verdict: the
    // frozen bytes ride the serving generation and still refuse.
    let (service, generation) = discovered("full");
    let frozen_envelope =
        std::fs::read_to_string("../../test-fixtures/stage5-v4/full/envelope-p5-0.json")
            .expect("frozen stage5-v4 envelope is checked in");
    assert!(
        !frozen_envelope.contains("foregroundKnown"),
        "frozen envelope predates the new report bit"
    );
    let live = frozen_envelope.replace("gen-stage5-v4", &generation);
    let reply: serde_json::Value =
        serde_json::from_str(&service.evaluate_capture(&live).expect("in-band"))
            .expect("rejection is JSON");
    assert_eq!(reply["outcome"], "rejected");
    assert!(reply.get("kind").is_some());
}
