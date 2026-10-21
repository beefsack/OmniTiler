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

/// Adopt committed capacity floats after one tick leg: joins the ordinary
/// float lanes with the live frame plus its geometry effects. Idempotent
/// across replays; staged-only candidates never read as committed; diverged
/// sessions stay untouched; no focus change. Callers invoke this after the
/// world observation tick applied, never on a discarded candidate.
pub(super) fn adopt_tick_capacity_floats(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    tick: u64,
    correlation: &str,
    observed: &[ObservedWindow],
    domain_key: &tiler_core::session::DomainKey,
) {
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

/// Adopt committed capacity floats across every live domain after one world
/// observation tick: the core carries committed `capacity` origins on its
/// applied reply rows, and each joins the ordinary float lanes with its
/// live frame plus marker repair. No Engine session, no geometry writes.
pub(super) fn adopt_capacity_from_reply(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    tick: u64,
    correlation: &str,
    observed: &[ObservedWindow],
) {
    let frames: HashMap<String, Rect> = observed
        .iter()
        .map(|window| (window.token.clone(), window.visible))
        .collect();
    for output in state.output_devices() {
        let Some(active) = state.active_view(&output) else {
            continue;
        };
        let domain_key = tiler_core::session::DomainKey {
            output: tiler_core::directional::OutputId(output.clone()),
            workspace: tiler_core::directional::WorkspaceId(active),
        };
        for adopted in register_committed_capacity_floats(state, me, &domain_key, &frames) {
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
}
/// Register committed capacity floats into the ordinary float lanes:
/// every committed `capacity` origin on the last applied world reply joins
/// `floated` with its live frame in `float_rects`. Sticky members keep
/// their own lane.
///
/// Lane registration is idempotent (later passes never rewrite the
/// frame), while the float-intent marker repairs independently: a
/// present marker needs no write, and a missing value retries on every
/// event-driven pass until it reads back. Only first-time and repaired
/// outcomes report. Reads the last applied reply rows only; no core
/// mutation, geometry writes, or focus changes.
pub(super) fn register_committed_capacity_floats(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    domain_key: &tiler_core::session::DomainKey,
    frames: &HashMap<String, Rect>,
) -> Vec<CapacityFloatAdopted> {
    // Gather under shared borrows first: the applied reply plus the member
    // tables are read-only here, and the mutation pass below must not run
    // under any borrow.
    let mut candidates: Vec<(crate::native_guards::WindowKey, String, Option<Rect>)> = Vec::new();
    if let Some(reply) = state.world.last_reply() {
        for row in &reply.windows {
            if !row.floating
                || row.float_origin != Some(tiler_core::world_report::FloatOrigin::Capacity)
            {
                continue;
            }
            if state.device_of(&row.output.0).as_deref() != Some(domain_key.output.0.as_str())
                || row.workspace != domain_key.workspace.0
            {
                continue;
            }
            let Some((key, _)) = state
                .member_tokens
                .iter()
                .find(|(_, token)| token.as_str() == row.lifetime.0.as_str())
            else {
                continue;
            };
            let key = key.clone();
            let homed_here = state.member_home(&key).is_some_and(|loc| {
                loc.output == domain_key.output.0 && loc.workspace == domain_key.workspace.0
            });
            if !homed_here {
                continue;
            }
            // Applied-reply frame first, caller-carried observed frame
            // second; intent registers without a snapshot either way.
            let frame = row
                .effective_frame
                .or_else(|| row.allocation)
                .or_else(|| frames.get(&row.lifetime.0).copied());
            candidates.push((key, row.lifetime.0.clone(), frame));
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
