#![forbid(unsafe_code)]
use super::*;

/// Whether one applied reply row is a committed float: the ordinary
/// `floating` flag alone. The diagnostic `float_origin` is never read:
/// ordinary and capacity floats adopt identically through this one path.
#[must_use]
pub(super) fn is_committed_float_row(row: &tiler_core::world_report::WindowDesire) -> bool {
    row.floating
}

/// Adopt committed floats after one world observation tick: every floating
/// row on the applied reply that binds a live non-sticky member caches its
/// live frame in the ordinary float lane. Frame caching only: intent
/// persistence belongs to the core's `intent_desired` plus host intent
/// reconciliation, the single writer, so adoption never writes a marker,
/// never re-stamps a sticky member's intent, and never invents a float in a
/// floating-mode workspace. No Engine session, no geometry writes.
pub(super) fn adopt_floats_from_reply(
    state: &mut TileLoop,
    reply: &tiler_core::world_report::WorldReplyV4,
    tick: u64,
    correlation: &str,
    observed: &[ObservedWindow],
) {
    let frames: HashMap<String, Rect> = observed
        .iter()
        .map(|window| (window.token.clone(), window.visible))
        .collect();
    // Gather under shared borrows first: the applied reply plus the member
    // and intent tables are read-only here, and the mutation pass below must
    // not run under any borrow.
    let mut candidates: Vec<(String, Rect)> = Vec::new();
    for row in &reply.windows {
        if !is_committed_float_row(row) {
            continue;
        }
        let Some((key, token)) = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == row.lifetime.0.as_str())
        else {
            continue;
        };
        // Sticky members keep their own lane: adoption never relabels them.
        if state.sticky_of(key) {
            continue;
        }
        // Applied-reply frame first, caller-carried observed frame second;
        // a row with no frame at all seeds nothing.
        let Some(frame) = row
            .effective_frame
            .or(row.allocation)
            .or_else(|| frames.get(&row.lifetime.0).copied())
        else {
            continue;
        };
        candidates.push((token.clone(), frame));
    }
    for (token, frame) in candidates {
        // Lane registration is idempotent: a lane that already holds a frame
        // is never rewritten. Only first-time adoption reports.
        if state.float_rects.contains_key(&token) {
            continue;
        }
        state.float_rects.insert(token.clone(), frame);
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-adopted",
                "tick": tick,
                "correlation": correlation,
                "window": token,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::rmax03_adapter_tests::test_state;
    use super::super::TileLoop;
    use super::*;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::world_report::{
        BorderHideReasonV4, BorderPresentationV4, ScopeAuthority, WindowDesire,
        WorldPresentationV4, WorldReplyV4, WORLD_CONTRACT_VERSION,
    };

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
            command_seq: None,
            visibility: None,
            visibility_authority: tiler_core::world_report::ScopeAuthority::Untouched,
            state: Vec::new(),
        }
    }

    /// Applied reply shell carrying only the float rows under test.
    fn reply_with(windows: Vec<WindowDesire>) -> WorldReplyV4 {
        WorldReplyV4 {
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
            windows,
            transfers: Vec::new(),
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            warp_request: None,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: None,
                border: BorderPresentationV4 {
                    visible: false,
                    reason: Some(BorderHideReasonV4::NoTarget),
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: tiler_core::world_presentation::PresentationUnderlay::default(),
                pager: Vec::new(),
            },
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn member_state() -> (TileLoop, crate::native_guards::WindowKey) {
        let mut state = test_state();
        let key = crate::native_guards::WindowKey {
            hwnd: 700_101,
            pid: 201,
            creation: "proc-a".to_owned(),
        };
        state.member_tokens.insert(key.clone(), "w1".to_owned());
        state.member_tags.insert(key.clone(), "tag-w1".to_owned());
        (state, key)
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

    #[test]
    fn adoption_caches_the_frame_without_writing_intent() {
        // Regression: adoption used to stamp `OmniTiler.IntentSlot.v2` on
        // every reconcile tick, clobbering sticky intent and poisoning
        // floating-mode workspaces. Frame caching stays; intent writes are
        // core-owned.
        let (mut state, key) = member_state();
        crate::product_hide::sys::fake::unregister_all();
        crate::product_hide::sys::fake::register_marks(key.hwnd);
        assert!(crate::product_hide::sys::fake::remove_float_mark(key.hwnd));
        assert!(crate::product_hide::sys::fake::remove_sticky_mark(key.hwnd));
        assert!(crate::product_hide::sys::fake::remove_tile_mark(key.hwnd));
        let snapshot_before = state.intent_snapshot.clone();
        let mut row = float_row(None);
        row.effective_frame = Some(Rect {
            x: 40,
            y: 60,
            w: 400,
            h: 300,
        });
        let reply = reply_with(vec![row]);
        adopt_floats_from_reply(&mut state, &reply, 9, "corr-9", &[]);
        assert_eq!(
            state.float_rects.get("w1"),
            Some(&Rect {
                x: 40,
                y: 60,
                w: 400,
                h: 300,
            }),
            "the applied float frame seeds the ordinary lane"
        );
        assert_eq!(
            state.intent_snapshot, snapshot_before,
            "adoption never mutates the durable intent snapshot"
        );
        assert_eq!(
            crate::product_hide::sys::fake::peek_float_mark(key.hwnd),
            Some(None),
            "adoption never writes the float intent lane"
        );
        assert_eq!(
            crate::product_hide::sys::fake::peek_sticky_mark(key.hwnd),
            Some(None),
            "adoption never rewrites sticky intent"
        );
        assert_eq!(
            crate::product_hide::sys::fake::peek_tile_mark(key.hwnd),
            Some(None),
            "adoption never writes a tile override"
        );
        crate::product_hide::sys::fake::unregister_all();
    }

    #[test]
    fn adoption_never_rewrites_a_cached_float_frame() {
        let (mut state, _key) = member_state();
        let old = Rect {
            x: 10,
            y: 10,
            w: 200,
            h: 200,
        };
        state.float_rects.insert("w1".to_owned(), old);
        let mut row = float_row(None);
        row.effective_frame = Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        });
        let reply = reply_with(vec![row]);
        adopt_floats_from_reply(&mut state, &reply, 10, "corr-10", &[]);
        assert_eq!(
            state.float_rects.get("w1"),
            Some(&old),
            "lane registration is idempotent"
        );
    }

    #[test]
    fn sticky_member_keeps_its_own_lane() {
        let (mut state, key) = member_state();
        state.seed_sticky(key, true);
        let mut row = float_row(None);
        row.effective_frame = Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        });
        let reply = reply_with(vec![row]);
        adopt_floats_from_reply(&mut state, &reply, 11, "corr-11", &[]);
        assert!(
            !state.float_rects.contains_key("w1"),
            "sticky members are never adopted as ordinary floats"
        );
    }
}
