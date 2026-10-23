#![forbid(unsafe_code)]
use super::*;

/// One committed float adopted into the ordinary float lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommittedFloatAdopted {
    pub(super) token: String,
    pub(super) marker_ok: bool,
}

/// Whether one applied reply row is a committed float: the ordinary
/// `floating` flag alone. The diagnostic `float_origin` is never read:
/// ordinary and capacity floats adopt identically through this one path.
#[must_use]
pub(super) fn is_committed_float_row(row: &tiler_core::world_report::WindowDesire) -> bool {
    row.floating
}

/// Adopt committed floats across every live domain after one world
/// observation tick: every floating row on the last applied world reply
/// joins the ordinary float lanes with its live frame plus marker repair.
/// No Engine session, no geometry writes.
pub(super) fn adopt_floats_from_reply(
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
        for adopted in register_committed_floats(state, me, &domain_key, &frames) {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "float-adopted",
                    "tick": tick,
                    "correlation": correlation,
                    "window": adopted.token,
                    "marker": if adopted.marker_ok { "ok" } else { "unverified" },
                }),
            );
        }
    }
}
/// Register committed floats into the ordinary float lanes:
/// every floating row on the last applied world reply joins
/// `floated` with its live frame in `float_rects`. Sticky members keep
/// their own lane.
///
/// Lane registration is idempotent (later passes never rewrite the
/// frame), while the float-intent marker repairs independently: a
/// present marker needs no write, and a missing value retries on every
/// event-driven pass until it reads back. Only first-time and repaired
/// outcomes report. Reads the last applied reply rows only; no core
/// mutation, geometry writes, or focus changes.
pub(super) fn register_committed_floats(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    domain_key: &tiler_core::session::DomainKey,
    frames: &HashMap<String, Rect>,
) -> Vec<CommittedFloatAdopted> {
    // Gather under shared borrows first: the applied reply plus the member
    // tables are read-only here, and the mutation pass below must not run
    // under any borrow.
    let mut candidates: Vec<(crate::native_guards::WindowKey, String, Option<Rect>)> = Vec::new();
    if let Some(reply) = state.world.last_reply() {
        for row in &reply.windows {
            if !is_committed_float_row(row) {
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
                .or(row.allocation)
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
            adopted.push(CommittedFloatAdopted { token, marker_ok });
        }
    }
    adopted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::world_report::{ScopeAuthority, WindowDesire};

    fn float_row(origin: Option<tiler_core::world_report::FloatOrigin>) -> WindowDesire {
        WindowDesire {
            lifetime: WindowId("w1".to_owned()),
            output: OutputId("out-a".to_owned()),
            workspace: "wa1".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Untouched,
            membership_authority: ScopeAuthority::Untouched,
            floating: true,
            float_origin: origin,
            frame_operation: None,
            clear_operation: None,
        }
    }

    #[test]
    fn float_origin_never_changes_committed_float_parity() {
        assert!(is_committed_float_row(&float_row(None)));
        assert!(is_committed_float_row(&float_row(Some(
            tiler_core::world_report::FloatOrigin::Capacity
        ))));
        let mut tiled = float_row(None);
        tiled.floating = false;
        assert!(!is_committed_float_row(&tiled));
    }
}
