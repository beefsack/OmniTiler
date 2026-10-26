//! Stage 5 production-route fixture proof (broker leg, GREEN).
//!
//! Feeds the REAL JS-built capture envelopes recorded under
//! `test-fixtures/stage5/envelope-*.json` (produced by the production
//! `startWorldAdapterEntry` route over the verbatim `facts-seed.json`
//! waiter bytes) through the production `WorldService` capture/read
//! path, and asserts the served projections equal the recorded
//! `projection-*.json` bytes exactly.
//!
//! Volatile-token normalization (disclosed identity seam, shape-preserving):
//! - `gen` is the harness-fixed `gen-stage5` in every fixture; the test
//!   substitutes the live broker generation at feed time and normalizes it
//!   back before comparing. No other field is touched.
//! - `source`/`sourceSeq` are the compact `src-N-fix`/`N` sequence (the
//!   entry's random suffix and the pre-facts genesis capture are folded
//!   out at record time; see the generator header).
//! - `req` is the caller-fixed `fix-*` correlation.
//!
//! A second test pins the chain join: each recorded envelope's witness
//! lane equals the covered witness of the projection it produced.

use omnitiler::world_service::WorldService;

const ADAPTER: &str = "kwin-actuator-1";

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("../../test-fixtures/stage5/{name}"))
        .expect("stage5 fixture is checked in")
}

fn discovered() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery: serde_json::Value =
        serde_json::from_str(&service.discovery_body(ADAPTER, "req-0")).expect("valid JSON");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    (service, generation)
}

/// Feed one recorded envelope (live generation substituted in) and return
/// the served projection with the live generation normalized back out.
fn route_fixture(
    service: &WorldService,
    generation: &str,
    envelope_name: &str,
    req: &str,
) -> String {
    let live = read_fixture(envelope_name).replace("gen-stage5", generation);
    let reply: serde_json::Value =
        serde_json::from_str(&service.evaluate_capture(&live).expect("in-band"))
            .expect("capture reply is JSON");
    assert!(
        reply.get("kind").is_none(),
        "{envelope_name}: real-JS envelope must be accepted, got {reply}"
    );
    let body = service
        .read_presentation(&format!("{{\"v\":2,\"req\":\"{req}\"}}"))
        .expect("in-band");
    body.replace(generation, "gen-stage5")
}

/// Drive the full recorded capture sequence in order (rev/obs sequencing
/// is order-dependent) and assert every served projection equals the
/// recorded broker bytes exactly.
#[test]
fn stage5_fixture_route_matches_recorded_projections() {
    let (service, generation) = discovered();
    for (envelope, req, projection) in [
        ("envelope-p5.json", "fix-p5", "projection-p5.json"),
        ("envelope-p6.json", "fix-p6", "projection-p6.json"),
        ("envelope-cover.json", "fix-cover", "projection-cover.json"),
        ("envelope-preview.json", "fix-preview", "projection-preview.json"),
    ] {
        let served = route_fixture(&service, &generation, envelope, req);
        let recorded = read_fixture(projection);
        assert_eq!(
            served, recorded,
            "{envelope}: served projection must equal the recorded broker bytes"
        );
    }
}

/// Each recorded envelope's witness lane (built by the real JS route from
/// the real facts bytes) equals the covered witness of the projection the
/// broker served for it: the facts -> capture -> coverage chain joins on
/// identical values, not merely identical key names.
#[test]
fn stage5_fixture_witness_chain_joins() {
    for (envelope, projection) in [
        ("envelope-p5.json", "projection-p5.json"),
        ("envelope-p6.json", "projection-p6.json"),
        ("envelope-cover.json", "projection-cover.json"),
        ("envelope-preview.json", "projection-preview.json"),
    ] {
        let envelope_value: serde_json::Value =
            serde_json::from_str(&read_fixture(envelope)).expect("envelope is JSON");
        let witness = envelope_value["report"]["nativeInvalidWitness"].clone();
        assert!(
            witness.is_object(),
            "{envelope}: capture must carry a proven witness lane"
        );
        let projection_value: serde_json::Value =
            serde_json::from_str(&read_fixture(projection)).expect("projection is JSON");
        assert_eq!(
            projection_value["coveredInstance"], witness["instance"],
            "{envelope}: broker must serve the captured witness instance"
        );
        assert_eq!(
            projection_value["coveredInvalid"], witness["counter"],
            "{envelope}: broker must serve the captured witness counter"
        );
    }
    // The seed facts name the same witness the first capture covers.
    let seed: serde_json::Value =
        serde_json::from_str(&read_fixture("facts-seed.json")).expect("facts are JSON");
    let p5: serde_json::Value =
        serde_json::from_str(&read_fixture("projection-p5.json")).expect("projection is JSON");
    assert_eq!(p5["coveredInstance"], seed["instance"]);
    assert_eq!(p5["coveredInvalid"], seed["invalidation"]);
}
