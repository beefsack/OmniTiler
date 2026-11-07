//! Stage 5 production-route fixture proof (broker leg).
//!
//! Replays the genuine v4 producer captures recorded under
//! `test-fixtures/stage5-v4/{full,s2}` (offline `broker_world_eval` entry
//! recordings over the `kwin-stage5-v4` adapter) through the production
//! `WorldService` capture/read path, and asserts the served projections equal
//! the recorded `projection-*.json` bytes exactly.
//!
//! Volatile-token normalization (gen-only, shape-preserving):
//! - `gen` is the harness-fixed `gen-stage5-v4` in every fixture; the test
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
//! recorded `attemptMs` to fit the dispatch clock, which starts at service
//! construction. The record run was slower than a fresh replay, so the test
//! settles one bounded sleep past the maximum recorded stamp before feeding
//! genesis. Attempt bytes and the clock oracle are untouched; only replay
//! timing adapts.
//!
//! A third test pins the chain join on the same v4 bytes: each recorded
//! envelope's witness lane equals the covered witness of the projection it
//! produced, and each facts input names the witness its captures cover.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-stage5-v4";
const GEN: &str = "gen-stage5-v4";

fn read_fixture(chain: &str, name: &str) -> String {
    std::fs::read_to_string(format!("../../test-fixtures/stage5-v4/{chain}/{name}"))
        .expect("stage5-v4 fixture is checked in")
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

/// Maximum recorded `attemptMs` over a chain's replay envelopes across the
/// exact five mechanical attempt lanes the broker's foreign-epoch guard
/// checks. Bounded so the settle sleep can never wait unboundedly.
fn clock_settle_ms(chain: &str, envelopes: &[&str]) -> u64 {
    let mut max = 0u64;
    for &envelope in envelopes {
        let value: serde_json::Value =
            serde_json::from_str(&read_fixture(chain, envelope)).expect("envelope is JSON");
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

/// Feed one recorded envelope (live generation substituted in) and assert the
/// capture is a genuine v4 acceptance bound to the right adapter, generation,
/// source, and watermark, with ready set.
fn feed_accepted(
    service: &WorldService,
    chain: &str,
    generation: &str,
    envelope_name: &str,
) -> serde_json::Value {
    let recorded = read_fixture(chain, envelope_name);
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
    // The border stays hidden intentionally (K1).
    assert_eq!(served["presentation"]["border"]["visible"], false);
}

fn run_chain(chain: &str, genesis: &[&str], steps: &[Step]) {
    let (service, generation) = discovered(chain);
    // Settle one bounded sleep past the maximum recorded attempt stamp so
    // the foreign-epoch guard accepts the replayed captures.
    let mut replay: Vec<&str> = genesis.to_vec();
    for step in steps {
        replay.extend(step.envelopes.iter().copied());
    }
    let settle = clock_settle_ms(chain, &replay);
    std::thread::sleep(std::time::Duration::from_millis(settle));
    for &envelope in genesis {
        feed_accepted(&service, chain, &generation, envelope);
    }
    for step in steps {
        let mut last = "";
        for &envelope in step.envelopes {
            feed_accepted(&service, chain, &generation, envelope);
            last = envelope;
        }
        read_step(&service, chain, &generation, step, last);
    }
}

/// Full chain: two genesis captures, then P5, pre-move P6, the covering
/// capture, the proven A preview, the cover2 double capture (one omits the
/// idle gesture lane), the still-open-A drain, the inv3 drain, and the valid
/// B repaint from a new B instance.
#[test]
fn stage5_fixture_route_matches_recorded_projections() {
    run_chain(
        "full",
        &["envelope-genesis-0.json", "envelope-genesis-1.json"],
        &[
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
        ],
    );
}

/// Same-source serial handover: gesture A (1/1) opens and shows, then B (1/2
/// on the same native source) goes live with no covering capture between
/// them. The B capture carries the advanced witness with the same-source
/// serial as its proven producer.
#[test]
fn stage5_fixture_route_same_source_serial_b() {
    run_chain(
        "s2",
        &["envelope-genesis-0.json", "envelope-genesis-1.json"],
        &[
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
        ],
    );
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
