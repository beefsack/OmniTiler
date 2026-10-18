#![forbid(unsafe_code)]
use super::*;

/// One committed capacity float adopted into the ordinary float lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CapacityFloatAdopted {
    pub(super) token: String,
    pub(super) marker_ok: bool,
}

/// Whether one Engine reply is a committed capacity-float transfer
/// (`MoveToFloat` under a send intent). Shared by the cross-output
/// refusal arms so both routes classify the shape identically.
#[must_use]
pub(super) fn reply_is_capacity_float_transfer(reply: &tiler_core::boundary::CoreReply) -> bool {
    match reply {
        tiler_core::boundary::CoreReply::SendWorkspace(plan)
        | tiler_core::boundary::CoreReply::SendOutput(plan) => matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveToFloat { .. }
        ),
        _ => false,
    }
}

/// Adopt committed capacity floats after one tick leg: joins the
/// ordinary float lanes with the live frame after the commit plus its
/// geometry effects. Idempotent across replays; staged-only candidates
/// never read as committed; diverged sessions stay untouched; no focus
/// change. Callers invoke this after `apply_geometry`, never on a
/// discarded candidate.
pub(super) fn adopt_tick_capacity_floats(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    tick: u64,
    correlation: &str,
    reply: &tiler_core::boundary::CoreReply,
    observed: &[ObservedWindow],
    domain_key: &tiler_core::session::DomainKey,
) {
    if matches!(reply, tiler_core::boundary::CoreReply::Diverged(_)) {
        return;
    }
    let frames: HashMap<String, Rect> = observed
        .iter()
        .map(|window| (window.token.clone(), window.visible))
        .collect();
    for adopted in register_committed_capacity_floats(state, me, domain_key, &frames) {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "capacity-float-adopted",
                "tick": tick,
                "correlation": correlation,
                "window": adopted.token,
                "marker": if adopted.marker_ok { "ok" } else { "unverified" },
            }),
        );
    }
}
/// Register committed capacity floats into the ordinary float lanes:
/// every committed `capacity` origin in `domain_key` joins `floated`
/// with its live frame in `float_rects`. Sticky members keep their own
/// lane.
///
/// Lane registration is idempotent (later passes never rewrite the
/// frame), while the float-intent marker repairs independently: a
/// present marker needs no write, and a missing value retries on every
/// event-driven pass until it reads back. Only first-time and repaired
/// outcomes report. Reads committed Session facts only; no Engine
/// mutation, geometry writes, or focus changes.
pub(super) fn register_committed_capacity_floats(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    domain_key: &tiler_core::session::DomainKey,
    frames: &HashMap<String, Rect>,
) -> Vec<CapacityFloatAdopted> {
    // Gather under shared borrows first: the committed Session plus the
    // member tables are read-only here, and the mutation pass below must
    // not run under the Session borrow.
    let mut candidates: Vec<(crate::workspace::WindowKey, String, Option<Rect>)> = Vec::new();
    if let Some(session) = state.engine.session(domain_key) {
        for (key, token) in state.member_tokens.iter() {
            let id = WindowId(token.clone());
            if !session.is_capacity_float(&id) {
                continue;
            }
            let homed_here = state.workspaces.member_loc(key).is_some_and(|loc| {
                loc.output == domain_key.output.0 && loc.workspace == domain_key.workspace.0
            });
            if !homed_here {
                continue;
            }
            // Engine-retained live frame first, caller-carried observed
            // frame second; intent registers without a snapshot either way.
            let frame = session
                .retained_float_geometry(&id)
                .or_else(|| frames.get(token).copied());
            candidates.push((key.clone(), token.clone(), frame));
        }
    }
    let mut adopted = Vec::new();
    for (key, token, frame) in candidates {
        if state.sticky.contains_key(&key) {
            continue;
        }
        let fresh_lane = !state.floated.contains(&key);
        if fresh_lane {
            state.floated.insert(key.clone());
            if let Some(frame) = frame {
                state.float_rects.insert(token.clone(), frame);
            }
        }
        let present = crate::product_hide::sys::read_float_intent_marker(key.hwnd);
        let marker_ok = if present {
            true
        } else {
            install_float_mark_held(state, me, &key)
        };
        if fresh_lane || (!present && marker_ok) {
            adopted.push(CapacityFloatAdopted { token, marker_ok });
        }
    }
    adopted
}
