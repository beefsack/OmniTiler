//! Test-only offline eval seam: line-delimited v4 world evaluation.
//!
//! Reads one JSON envelope per stdin line, evaluates each against a single
//! retained [`WorldEngine`], and writes one JSON reply per stdout line
//! (empty lines skipped). The envelope mirrors the production D-Bus split:
//! `{"method":"DiscoverWorld","body":"<connection json>"}` carries a v4
//! connection message (`handshake` or `settings`, dispatched by its `type`)
//! routed authentically through [`route_connection_request`], and
//! `{"method":"DescribeWorld","body":"<report json>"}` runs the retained
//! engine transactionally through the genuine v4 report path
//! (`decode_request` + `WorldEngine::handle_report` + `encode_reply`).
//! Only the handshake resets the lifetime (the core handshake mints the
//! next generation from the deterministic evaluator seed); settings never
//! reset. D-Bus, activation, and live host state are stubbed. Test-only
//! seam; not part of the shipped service.
//!
//! The retired raw-capture purpose of this seam is unportable: the v3 raw
//! KWin classification (`predicate`/`hostOwned` adapter verdicts) has no v4
//! translation, so raw rows are no longer normalized here. Callers must
//! supply genuine v4 facts (screens with scale plus raw window attributes,
//! `presentation`, `rendererInput`, and `focusChanges`, no `views` or settings keys).
//! There is deliberately no `_legacy` conversion path.

use std::io::{BufRead, Write};

use tiler_core::world_report::{BatchDisposition, CommandDisposition, CommandResult};
use tiler_protocol::connection_protocol::route_connection_request;
use tiler_protocol::world_admission::ProtocolAdmissionGate;
use tiler_protocol::world_command_result::encode_command_result;
use tiler_protocol::world_protocol::{
    WorldEngine, WorldOutcome, decode_request_with_wire, encode_error,
};
use tiler_protocol::world_reply_write::write_reply_v4_with_plan;

fn generation_seed() -> String {
    "world-eval-0".to_owned()
}

fn ensure_engine(engine: &mut Option<WorldEngine>, seed: &str) {
    if engine.is_none() {
        *engine = Some(WorldEngine::new(seed).expect("evaluator seed parses"));
    }
}

fn serving_generation(engine: &Option<WorldEngine>, seed: &str) -> String {
    engine
        .as_ref()
        .map(|live| live.core_generation().to_owned())
        .unwrap_or_else(|| seed.to_owned())
}

fn submit_report_with_settlements(
    engine: &mut WorldEngine,
    body: &str,
    generation: &str,
) -> (String, Vec<BatchDisposition>) {
    let decoded = match decode_request_with_wire(body) {
        Ok(decoded) => decoded,
        Err(error) => {
            return (
                encode_error(error.kind, error.message, generation),
                Vec::new(),
            );
        }
    };
    let gate = ProtocolAdmissionGate::with_residual(decoded.residual.as_ref());
    let mut prepared = None;
    let transition = match gate.dispatch_with_prepare(engine, &decoded.report, &[], |candidate| {
        let reply = candidate.reply.reply();
        prepared = Some(write_reply_v4_with_plan(
            reply,
            candidate.shared_roster_plan,
        )?);
        Ok(())
    }) {
        Ok(result) => result,
        Err(error) => {
            let error: tiler_protocol::world_protocol::WorldProtocolError = error;
            return (
                encode_error(error.kind, error.message, generation),
                Vec::new(),
            );
        }
    };
    match transition.outcome {
        WorldOutcome::Accepted(_) | WorldOutcome::CommandConsumed(_) => (
            prepared.expect("accepted candidate was prepared"),
            transition.dispositions,
        ),
        WorldOutcome::Duplicate(reply) => {
            match write_reply_v4_with_plan(&reply, engine.desired_shared_roster_plan()) {
                Ok(bytes) => (bytes, transition.dispositions),
                Err(error) => (
                    encode_error(error.kind, error.message, generation),
                    Vec::new(),
                ),
            }
        }
        WorldOutcome::Rejected { kind, message } => {
            (encode_error(kind, message, generation), Vec::new())
        }
    }
}

fn submit_report(engine: &mut WorldEngine, body: &str, generation: &str) -> String {
    submit_report_with_settlements(engine, body, generation).0
}

fn encode_settlement_result(result: &Option<CommandResult>) -> serde_json::Value {
    let mut encoded = String::new();
    encode_command_result(&mut encoded, result);
    serde_json::from_str(&encoded).unwrap_or(serde_json::Value::Null)
}

fn encode_report_with_settlements(reply: String, dispositions: Vec<BatchDisposition>) -> String {
    let encoded_settlements: Vec<serde_json::Value> = dispositions
        .into_iter()
        .filter_map(|d| {
            let result = match d.disposition {
                CommandDisposition::Executed(r) => r,
                CommandDisposition::Refused(kind) => Some(CommandResult::Rejected { kind }),
                CommandDisposition::AlreadyConsumed | CommandDisposition::NotRun => return None,
            };
            Some(serde_json::json!({
                "seq": d.seq.to_string(),
                "result": encode_settlement_result(&result),
            }))
        })
        .collect();
    serde_json::json!({
        "reply": reply,
        "settlements": encoded_settlements,
    })
    .to_string()
}

fn main() {
    // Every carrier sends automatic fixed-size floats; the evaluator runs
    // the shared default every fixture assumes.
    let seed = generation_seed();
    let mut engine: Option<WorldEngine> = None;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        ensure_engine(&mut engine, &seed);
        let generation = serving_generation(&engine, &seed);
        let reply = (|| {
            let envelope: serde_json::Value = serde_json::from_str(&line).ok()?;
            let method = envelope.get("method")?.as_str()?;
            let body = envelope.get("body")?.as_str()?;
            if method == "DiscoverWorld" {
                let live = engine.as_mut()?;
                match route_connection_request(live, body) {
                    Ok(bytes) => Some(bytes),
                    Err(error) => Some(encode_error(error.0, error.0, live.core_generation())),
                }
            } else if method == "DescribeWorld" {
                let live = engine.as_mut()?;
                let generation = live.core_generation().to_owned();
                Some(submit_report(live, body, &generation))
            } else if method == "DescribeWorldWithSettlements" {
                // Test-only seam returning {reply, settlements} from the real
                // issuing handle_report tuple; canonical DescribeWorld is unchanged.
                let live = engine.as_mut()?;
                let generation = live.core_generation().to_owned();
                let (reply, settlements) = submit_report_with_settlements(live, body, &generation);
                Some(encode_report_with_settlements(reply, settlements))
            } else {
                Some(encode_error(
                    "unknown-value",
                    "request contains an unknown value",
                    &generation,
                ))
            }
        })()
        .unwrap_or_else(|| encode_error("malformed", "request is malformed", &generation));
        out.write_all(reply.as_bytes()).expect("stdout is writable");
        out.write_all(b"\n").expect("stdout is writable");
    }
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tiler_protocol::world_protocol::decode_reply_v4;

    #[test]
    fn shared_roster_publication_and_feedback_pass_through_raw_evaluator() {
        let mut engine = WorldEngine::new("world-eval-0").unwrap();
        let handshake = include_str!(
            "../../../test-fixtures/stage5-v4-active/full/discovery-handshake.request.json"
        );
        route_connection_request(&mut engine, handshake).expect("handshake");
        let generation = engine.core_generation().to_owned();
        let mut settings: Value = serde_json::from_str(include_str!(
            "../../../test-fixtures/stage5-v4-active/full/discovery-settings.request.json"
        ))
        .unwrap();
        settings["gen"] = json!(generation);
        route_connection_request(&mut engine, &settings.to_string()).expect("settings");
        let mut envelope: Value = serde_json::from_str(include_str!(
            "../../../test-fixtures/stage5-v4-active/full/envelope-genesis-0.json"
        ))
        .unwrap();
        let adapter = envelope["adapter"].clone();
        let mut report = envelope["report"].take();
        let map = report.as_object_mut().expect("report object");
        for broker_only in ["nativeGesture", "cursorWindow", "nativeInvalidWitness"] {
            map.remove(broker_only);
        }
        map.insert("v".to_owned(), json!(4));
        map.insert("adapter".to_owned(), adapter);
        map.insert("gen".to_owned(), json!(generation));
        map.insert("obs".to_owned(), json!("1"));
        map.insert("reqSeq".to_owned(), json!("1"));
        map.insert("req".to_owned(), json!("request-a"));
        map.insert("cmdSeq".to_owned(), Value::Null);
        map.insert("command".to_owned(), Value::Null);
        let initial_desktop = report["native"][0]["order"][0]
            .as_str()
            .expect("initial native desktop")
            .to_owned();
        let first = report.to_string();
        let reply = submit_report(&mut engine, &first, &generation);
        let plan = decode_reply_v4(&reply)
            .unwrap_or_else(|err| {
                panic!("decode_reply_v4 failed on first reply: {err:?}; raw reply: {reply}")
            })
            .roster_plan
            .expect("append plan");
        assert_eq!(submit_report(&mut engine, &first, &generation), reply);
        let [_, tiler_core::world_roster::RosterSlot::Append(step)] = plan.slots.as_slice() else {
            panic!("expected startup append: {plan:?}");
        };
        report["obs"] = json!("2");
        report["reqSeq"] = json!("2");
        report["req"] = json!("request-b");
        report["native"][0]["order"] = json!([initial_desktop, "desktop-b"]);
        report["native"][0]["occupancy"]
            .as_array_mut()
            .unwrap()
            .push(json!({"native":"desktop-b","count":0}));
        report["rosterFeedback"] = json!({
            "op":plan.operation_id,"gen":plan.generation,"rev":plan.revision.to_string(),
            "steps":[{"step":step,"status":"appended",
                "before":{"kind":"order","order":[initial_desktop]},
                "after":{"kind":"order","order":[initial_desktop,"desktop-b"]}}]
        });
        let second = report.to_string();
        let reply = submit_report(&mut engine, &second, &generation);
        let settled = decode_reply_v4(&reply).unwrap_or_else(|err| {
            panic!("decode_reply_v4 failed on second reply: {err:?}; raw reply: {reply}")
        });
        assert!(settled.roster_plan.is_none());
        assert_eq!(submit_report(&mut engine, &second, &generation), reply);
    }
}
