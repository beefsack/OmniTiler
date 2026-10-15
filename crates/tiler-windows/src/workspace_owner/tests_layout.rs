/// R-MOV-03 retained-Engine coverage through the Windows reconcile route.
///
/// The rig builds `CoreEvent`s exactly like `keyboard_tick` does
/// (`build_reconcile_event_for` plus a `CoreCommand::Move` carrying the
/// typed mode) and drives the retained `Engine`, so these tests prove the
/// Windows plumbing honors `core.same_axis_move`. Core planning itself is
/// owned by `tiler-core` (`session_movement.rs` `flat_swap_*`, `r2c_*`);
/// what is proven here is mode threading plus the observable Windows-side
/// geometry/topology/focus outcomes. The admitted 4-window tree nests, so
/// the R2b insert grows the vertical pair `(w1, w2)` into the N-ary R2c
/// position; moves then run along that vertical axis.
mod same_axis_move {
    use std::collections::BTreeMap;

    use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply, MovePlanReply};
    use tiler_core::directional::{Node, Rule, SameAxisMove, WindowId};
    use tiler_core::engine::Engine;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::session::DomainKey;

    use super::super::workspace_domain;
    use crate::settings::Settings;

    struct Rig {
        engine: Engine,
        owner: OwnerId,
        generation: GenerationId,
        domain: (tiler_core::session::OutputDomain, DomainKey),
        rects: Vec<(String, Rect)>,
        focused: String,
        corr: u64,
    }

    impl Rig {
        fn seed_n(names: &[&str]) -> Self {
            let bounds = Rect {
                x: 0,
                y: 0,
                w: 1600,
                h: 900,
            };
            let pairs: Vec<(&str, Rect)> = names.iter().map(|w| (*w, bounds)).collect();
            Self::seed_rects(&pairs)
        }

        fn seed_rects(pairs: &[(&str, Rect)]) -> Self {
            let mut engine = Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let bounds = Rect {
                x: 0,
                y: 0,
                w: 1600,
                h: 900,
            };
            let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
            let last = (*pairs.last().expect("names").0).to_owned();
            let mut rig = Self {
                engine,
                owner,
                generation,
                domain,
                rects: pairs.iter().map(|(w, r)| ((*w).to_owned(), *r)).collect(),
                focused: last,
                corr: 0,
            };
            let seed = rig.event(CoreCommand::Reconcile);
            let reply = rig.engine.handle(&seed);
            assert!(
                matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
                "seed must converge, got {reply:?}"
            );
            rig
        }

        fn seed4() -> Self {
            Self::seed_n(&["w1", "w2", "w3", "w4"])
        }

        /// Seed 4, walk focus to w1, grow the V group via the R2b insert:
        /// w1's up-neighbor is then the direct leaf sibling w2 (R2c pair).
        fn setup_vertical_pair() -> Self {
            let mut rig = Self::seed4();
            rig.focus("left");
            rig.focus("left");
            let reply = rig.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            assert!(
                matches!(reply, CoreReply::MoveDirectional(_)),
                "setup insert must plan, got {reply:?}"
            );
            rig
        }

        fn event(&mut self, command: CoreCommand) -> CoreEvent {
            self.corr += 1;
            let revision = self
                .engine
                .session(&self.domain.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            let fp = crate::tiling::fingerprint(&self.rects);
            let correlation =
                CorrelationId::parse(&format!("sam-{}", self.corr)).expect("correlation");
            let windows: Vec<(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)> = self
                .rects
                .iter()
                .map(|(w, r)| {
                    (
                        WindowId(w.clone()),
                        *r,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect();
            let mut event = crate::tiling::build_reconcile_event_for(
                &self.owner,
                &self.generation,
                &correlation,
                revision,
                fp,
                &self.domain.0,
                &self.domain.1,
                8,
                &windows,
                Some(&WindowId(self.focused.clone())),
            );
            event.command = command;
            event
        }

        fn focus(&mut self, direction: &str) -> CoreReply {
            let window = self.focused.clone();
            let event = self.event(CoreCommand::Focus {
                window,
                direction: direction.to_owned(),
                cross_output_transfer: false,
                float_subject: false,
            });
            let reply = self.engine.handle(&event);
            if let CoreReply::FocusDirectional(plan) = &reply {
                self.focused = plan.to_window.0.clone();
                self.rects = plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect();
            }
            reply
        }

        fn mv(&mut self, window: &str, direction: &str, mode: SameAxisMove) -> CoreReply {
            let event = self.event(CoreCommand::Move {
                window: window.to_owned(),
                direction: direction.to_owned(),
                cross_output_transfer: false,
                same_axis_move: mode,
            });
            let reply = self.engine.handle(&event);
            if let CoreReply::MoveDirectional(plan) = &reply {
                self.rects = plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect();
            }
            reply
        }

        fn rz(&mut self, window: &str, direction: &str, mode: &str) -> CoreReply {
            let event = self.event(CoreCommand::Resize {
                window: window.to_owned(),
                direction: direction.to_owned(),
                mode: mode.to_owned(),
                press_index: 0,
            });
            let reply = self.engine.handle(&event);
            if let CoreReply::Resize(plan) = &reply {
                self.rects = plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect();
            }
            reply
        }

        fn snapshot(&self) -> tiler_core::session::SessionSnapshot {
            self.engine
                .session(&self.domain.1)
                .expect("session")
                .snapshot()
        }

        fn root_group(&self) -> (Vec<String>, Vec<u64>) {
            let snap = self.snapshot();
            let Some(Node::Group {
                children, shares, ..
            }) = &snap.domains[0].tree
            else {
                panic!("root must be a group, got {:?}", snap.domains[0].tree);
            };
            (
                children.iter().map(|c| c.id().0.clone()).collect(),
                shares.clone(),
            )
        }
    }

    fn move_plan(reply: CoreReply, what: &str) -> MovePlanReply {
        match reply {
            CoreReply::MoveDirectional(plan) => plan,
            other => panic!("{what} must plan directionally, got {other:?}"),
        }
    }

    fn geom(plan: &MovePlanReply, window: &str) -> Rect {
        plan.geometry
            .iter()
            .find(|g| g.window.0 == window)
            .unwrap_or_else(|| panic!("geometry for {window}"))
            .rect
    }

    fn leaf_id(window: &str) -> String {
        format!("leaf-{window}")
    }

    #[test]
    fn swap_exchanges_unequal_pair_both_directions_with_traveling_shares() {
        // Every move below threads its mode from live settings text, the
        // same selection `keyboard_tick` makes per move.
        let mut swap_settings = Settings::default();
        swap_settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
        crate::settings::validate_settings(&swap_settings).expect("valid token");
        let swap = swap_settings.core.same_axis_move_mode();
        assert_eq!(swap, SameAxisMove::SwapWithNeighbor);

        let mut rig = Rig::setup_vertical_pair();
        // Equal-share pair first: the swap exchanges positions exactly,
        // keeps every share, adds no wrapper, and retains focus.
        let pre_w1 = geom_of(&rig.rects, "w1");
        let pre_w2 = geom_of(&rig.rects, "w2");
        let plan = move_plan(rig.mv("w1", "up", swap), "swap up");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
        assert_eq!(geom(&plan, "w1"), pre_w2);
        assert_eq!(geom(&plan, "w2"), pre_w1);
        let (children, shares) = rig.root_group();
        assert_eq!(children.len(), 3, "no wrapper group appears");
        assert_eq!(shares, vec![1, 1, 1]);
        // Unbalance the pair through the ordinary keyboard-resize path,
        // then swap down: extents travel with their windows.
        let resize = rig.rz("w1", "down", "outwards");
        assert!(
            matches!(resize, CoreReply::Resize(_)),
            "resize must plan, got {resize:?}"
        );
        let (children, shares) = rig.root_group();
        assert_eq!(children[..2], [leaf_id("w1"), leaf_id("w2")]);
        assert_ne!(shares[0], shares[1], "pair must be unbalanced");
        let pre = rig.rects.clone();
        let plan = move_plan(rig.mv("w1", "down", swap), "unequal swap down");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
        assert_eq!(geom(&plan, "w1").h, geom_of(&pre, "w1").h);
        assert_eq!(geom(&plan, "w1").w, geom_of(&pre, "w1").w);
        assert_eq!(geom(&plan, "w2").h, geom_of(&pre, "w2").h);
        assert_eq!(geom(&plan, "w2").w, geom_of(&pre, "w2").w);
        let (children, post_shares) = rig.root_group();
        assert_eq!(children[..2], [leaf_id("w2"), leaf_id("w1")]);
        assert_eq!(post_shares[0], shares[1]);
        assert_eq!(post_shares[1], shares[0]);
        assert_eq!(post_shares[2], shares[2]);
        // Opposite direction restores order, shares, and geometry exactly.
        let back = move_plan(rig.mv("w1", "up", swap), "swap back up");
        assert_eq!(back.rule, Rule::R2c);
        assert_eq!(back.focus_leaf.0, leaf_id("w1"));
        let (children, restored) = rig.root_group();
        assert_eq!(children[..2], [leaf_id("w1"), leaf_id("w2")]);
        assert_eq!(restored, shares);
        for (window, rect) in &pre {
            assert_eq!(geom(&back, window), *rect, "{window} restores");
        }
    }

    #[test]
    fn group_wrap_default_halves_combined_extent_without_reordering() {
        let group = Settings::default().core.same_axis_move_mode();
        assert_eq!(group, SameAxisMove::GroupWithNeighbor);
        let mut rig = Rig::setup_vertical_pair();
        let pre_w1 = geom_of(&rig.rects, "w1");
        let pre_w2 = geom_of(&rig.rects, "w2");
        let start = pre_w2.y;
        let end = pre_w1.y + pre_w1.h;
        let plan = move_plan(rig.mv("w1", "up", group), "wrap up");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
        // Wrapper keeps the pair combined extent and halves inside, with
        // the neighbor first (the mover moved up into it from below).
        let snap = rig.snapshot();
        let Some(Node::Group {
            children, shares, ..
        }) = &snap.domains[0].tree
        else {
            panic!("root must be a group");
        };
        assert_eq!(*shares, vec![2, 1]);
        let [wrapper, _] = children.as_slice() else {
            panic!("root must keep two children, got {children:?}");
        };
        let Node::Group {
            children: inner,
            shares: inner_shares,
            ..
        } = wrapper
        else {
            panic!("first root child must be the wrapper, got {wrapper:?}");
        };
        assert_eq!(
            inner.iter().map(|c| c.id().0.clone()).collect::<Vec<_>>(),
            [leaf_id("w2"), leaf_id("w1")]
        );
        assert_eq!(*inner_shares, vec![1, 1]);
        let post_w1 = geom(&plan, "w1");
        let post_w2 = geom(&plan, "w2");
        assert_eq!(post_w2.y, start, "top edge retained");
        assert_eq!(post_w1.h, post_w2.h, "halves inside");
        assert_eq!(post_w1.w, pre_w1.w);
        // Bottom edge retains the combined extent up to core projection
        // rounding (one inner gap); the topology pins the wrap outcome.
        assert!(
            (post_w1.y + post_w1.h).abs_diff(end) <= 8,
            "combined extent retained: {} vs {end}",
            post_w1.y + post_w1.h
        );
    }

    #[test]
    fn mode_change_alone_mutates_no_topology_until_next_move() {
        let mut rig = Rig::setup_vertical_pair();
        let before = rig.snapshot();
        // Flipping only this setting touches no Engine state: the
        // validated document changes, the retained session does not.
        let mut settings = Settings::default();
        assert_eq!(
            settings.core.same_axis_move_mode(),
            SameAxisMove::GroupWithNeighbor
        );
        settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
        crate::settings::validate_settings(&settings).expect("valid token");
        assert_eq!(rig.snapshot(), before);
        // The next move then uses the new mode: a flat swap, no wrapper.
        let plan = move_plan(
            rig.mv("w1", "up", settings.core.same_axis_move_mode()),
            "post-change move",
        );
        assert_eq!(plan.rule, Rule::R2c);
        let (children, _) = rig.root_group();
        assert_eq!(children.len(), 3, "no wrapper group appears");
        assert_eq!(children[0], leaf_id("w1"));
    }

    #[test]
    fn binary_group_neighbor_and_boundary_parity_across_modes() {
        // Binary R2a: identical plan under both modes (mover is the
        // seed-focused w2, moving left).
        let plan_group = move_plan(
            Rig::seed_n(&["w1", "w2"]).mv("w2", "left", SameAxisMove::GroupWithNeighbor),
            "binary group",
        );
        let plan_swap = move_plan(
            Rig::seed_n(&["w1", "w2"]).mv("w2", "left", SameAxisMove::SwapWithNeighbor),
            "binary swap",
        );
        assert_eq!(plan_group.rule, Rule::R2a);
        assert_eq!(plan_group, plan_swap);
        // Group neighbor (R2b insertion after the R1 wrap): identical.
        let mut rig_group = Rig::seed4();
        rig_group.focus("left");
        rig_group.focus("left");
        rig_group.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        rig_group.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        let left_group = rig_group.mv("w1", "left", SameAxisMove::GroupWithNeighbor);
        let mut rig_swap = Rig::seed4();
        rig_swap.focus("left");
        rig_swap.focus("left");
        rig_swap.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        rig_swap.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        let left_swap = rig_swap.mv("w1", "left", SameAxisMove::SwapWithNeighbor);
        assert_eq!(left_group, left_swap);
        assert_eq!(move_plan(left_group, "group neighbor").rule, Rule::R2b);
        // Sole root leaf boundary: identical refusal under both modes.
        let lone_group = Rig::seed_n(&["w1"]).mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        let lone_swap = Rig::seed_n(&["w1"]).mv("w1", "right", SameAxisMove::SwapWithNeighbor);
        assert_eq!(lone_group, lone_swap);
        assert!(
            matches!(lone_group, CoreReply::Rejected { .. }),
            "sole leaf refuses, got {lone_group:?}"
        );
    }

    fn geom_of(rects: &[(String, Rect)], window: &str) -> Rect {
        rects
            .iter()
            .find(|(w, _)| w == window)
            .unwrap_or_else(|| panic!("rect for {window}"))
            .1
    }

    /// Window to retained-leaf map from the session snapshot links: the
    /// fit admission names leaves `fit-lN`, so tests must resolve the
    /// mover's leaf instead of assuming `leaf-{window}`.
    fn leaf_map(rig: &Rig) -> BTreeMap<String, String> {
        rig.snapshot()
            .windows
            .iter()
            .map(|link| (link.window.0.clone(), link.leaf.0.clone()))
            .collect()
    }

    /// Rect equality up to core projection rounding: reprojected slot
    /// geometry lands within a pixel of the observed share rects.
    fn assert_rect_near(actual: Rect, expected: Rect, what: &str) {
        for (a, e, axis) in [
            (actual.x, expected.x, "x"),
            (actual.y, expected.y, "y"),
            (actual.w, expected.w, "w"),
            (actual.h, expected.h, "h"),
        ] {
            assert!(
                a.abs_diff(e) <= 1,
                "{what}: {axis} {a} vs {e} ({actual:?} vs {expected:?})"
            );
        }
    }

    /// Seed the checklist row: four pre-tiled unequal columns admit as a
    /// flat horizontal N-ary group `H[w1,w2,w3,w4]` with proportional
    /// shares `[300,500,376,400]`, then walk focus to `w2` (`B*`).
    fn setup_checklist_row() -> (Rig, BTreeMap<String, String>) {
        let cols = [
            (
                "w1",
                Rect {
                    x: 0,
                    y: 0,
                    w: 300,
                    h: 900,
                },
            ),
            (
                "w2",
                Rect {
                    x: 308,
                    y: 0,
                    w: 500,
                    h: 900,
                },
            ),
            (
                "w3",
                Rect {
                    x: 816,
                    y: 0,
                    w: 376,
                    h: 900,
                },
            ),
            (
                "w4",
                Rect {
                    x: 1200,
                    y: 0,
                    w: 400,
                    h: 900,
                },
            ),
        ];
        let mut rig = Rig::seed_rects(&cols);
        let leaves = leaf_map(&rig);
        assert_eq!(
            rig.root_group(),
            (
                vec![
                    leaves["w1"].clone(),
                    leaves["w2"].clone(),
                    leaves["w3"].clone(),
                    leaves["w4"].clone(),
                ],
                vec![300, 500, 376, 400],
            ),
            "checklist row must admit flat with proportional shares"
        );
        for (step, want) in [("left", "w3"), ("left", "w2")] {
            match rig.focus(step) {
                CoreReply::FocusDirectional(plan) => {
                    assert_eq!(plan.to_window.0, want, "focus walk step");
                }
                other => panic!("focus walk must plan, got {other:?}"),
            }
        }
        assert_eq!(rig.focused, "w2");
        (rig, leaves)
    }

    #[test]
    fn horizontal_swaps_both_directions_with_traveling_shares() {
        // Every move threads its mode from live settings text, the same
        // selection `keyboard_tick` makes per move.
        let mut swap_settings = Settings::default();
        swap_settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
        crate::settings::validate_settings(&swap_settings).expect("valid token");
        let swap = swap_settings.core.same_axis_move_mode();
        assert_eq!(swap, SameAxisMove::SwapWithNeighbor);

        // Right: `H[w1,w2*,w3,w4]` -> `H[w1,w3,w2*,w4]`, shares travel.
        // Slots reproject from the new share order, so positions derive
        // from order while widths travel with their windows.
        let (mut rig, leaves) = setup_checklist_row();
        let pre = rig.rects.clone();
        let plan = move_plan(rig.mv("w2", "right", swap), "swap right");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaves["w2"]);
        assert_eq!(plan.geometry.len(), 4);
        let (children, shares) = rig.root_group();
        assert_eq!(
            children,
            [
                leaves["w1"].clone(),
                leaves["w3"].clone(),
                leaves["w2"].clone(),
                leaves["w4"].clone()
            ]
        );
        assert_eq!(
            shares,
            vec![300, 376, 500, 400],
            "shares travel with windows"
        );
        assert!(geom(&plan, "w2").w.abs_diff(geom_of(&pre, "w2").w) <= 1);
        assert!(geom(&plan, "w3").w.abs_diff(geom_of(&pre, "w3").w) <= 1);
        assert!(
            geom(&plan, "w2").x > geom_of(&pre, "w2").x,
            "w2 moves right"
        );
        assert!(geom(&plan, "w3").x < geom_of(&pre, "w3").x, "w3 moves left");
        assert!(geom(&plan, "w1").x < geom(&plan, "w3").x);
        assert!(geom(&plan, "w3").x < geom(&plan, "w2").x);
        assert!(geom(&plan, "w2").x < geom(&plan, "w4").x);
        // Left back: exact order, shares, and focus restore.
        let back = move_plan(rig.mv("w2", "left", swap), "swap left back");
        assert_eq!(back.rule, Rule::R2c);
        assert_eq!(back.focus_leaf.0, leaves["w2"]);
        let (children, shares) = rig.root_group();
        assert_eq!(
            children,
            [
                leaves["w1"].clone(),
                leaves["w2"].clone(),
                leaves["w3"].clone(),
                leaves["w4"].clone()
            ]
        );
        assert_eq!(shares, vec![300, 500, 376, 400]);
        for (window, rect) in &pre {
            assert_rect_near(geom(&back, window), *rect, &format!("{window} restores"));
        }
        // Left into the most unequal pair (500 vs 300), then right back.
        let plan = move_plan(rig.mv("w2", "left", swap), "swap left");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaves["w2"]);
        let (children, shares) = rig.root_group();
        assert_eq!(
            children,
            [
                leaves["w2"].clone(),
                leaves["w1"].clone(),
                leaves["w3"].clone(),
                leaves["w4"].clone()
            ]
        );
        assert_eq!(
            shares,
            vec![500, 300, 376, 400],
            "shares travel with windows"
        );
        assert!(geom(&plan, "w2").w.abs_diff(geom_of(&pre, "w2").w) <= 1);
        assert!(geom(&plan, "w1").w.abs_diff(geom_of(&pre, "w1").w) <= 1);
        assert!(geom(&plan, "w2").x < geom_of(&pre, "w2").x, "w2 moves left");
        assert!(
            geom(&plan, "w1").x > geom_of(&pre, "w1").x,
            "w1 moves right"
        );
        let back = move_plan(rig.mv("w2", "right", swap), "swap right back");
        assert_eq!(back.rule, Rule::R2c);
        assert_eq!(back.focus_leaf.0, leaves["w2"]);
        let (children, shares) = rig.root_group();
        assert_eq!(
            children,
            [
                leaves["w1"].clone(),
                leaves["w2"].clone(),
                leaves["w3"].clone(),
                leaves["w4"].clone()
            ]
        );
        assert_eq!(shares, vec![300, 500, 376, 400]);
        for (window, rect) in &pre {
            assert_rect_near(geom(&back, window), *rect, &format!("{window} restores"));
        }
    }

    #[test]
    fn horizontal_wrap_default_groups_neighbor_pair() {
        let group = Settings::default().core.same_axis_move_mode();
        assert_eq!(group, SameAxisMove::GroupWithNeighbor);
        let (mut rig, leaves) = setup_checklist_row();
        let pre_w2 = geom_of(&rig.rects, "w2");
        let pre_w3 = geom_of(&rig.rects, "w3");
        let start = pre_w2.x;
        let end = pre_w3.x + pre_w3.w;
        // Default right wraps the pair: `H[w1,H[w2*,w3],w4]`.
        let plan = move_plan(rig.mv("w2", "right", group), "wrap right");
        assert_eq!(plan.rule, Rule::R2c);
        assert_eq!(plan.focus_leaf.0, leaves["w2"]);
        let snap = rig.snapshot();
        let Some(Node::Group {
            children, shares, ..
        }) = &snap.domains[0].tree
        else {
            panic!("root must be a group");
        };
        assert_eq!(
            *shares,
            vec![300, 876, 400],
            "wrapper carries the summed pair share"
        );
        let [first, wrapper, _] = children.as_slice() else {
            panic!("root must keep three children, got {children:?}");
        };
        assert_eq!(first.id().0, leaves["w1"]);
        let Node::Group {
            children: inner,
            shares: inner_shares,
            ..
        } = wrapper
        else {
            panic!("middle root child must be the wrapper, got {wrapper:?}");
        };
        assert_eq!(
            inner.iter().map(|c| c.id().0.clone()).collect::<Vec<_>>(),
            [leaves["w2"].clone(), leaves["w3"].clone()]
        );
        // New splits inside the wrapper are `[1, 1]`; the summed pair
        // share rides the wrapper itself (outer `[300, 876, 400]`).
        assert_eq!(*inner_shares, vec![1, 1]);
        // Wrapper keeps the pair combined extent and halves inside.
        let post_w2 = geom(&plan, "w2");
        let post_w3 = geom(&plan, "w3");
        assert!(post_w2.x.abs_diff(start) <= 1, "left edge retained");
        assert!(
            (post_w3.x + post_w3.w).abs_diff(end) <= 8,
            "combined extent retained"
        );
        assert!(
            post_w2.w.abs_diff(post_w3.w) <= 1,
            "halves inside: {} vs {}",
            post_w2.w,
            post_w3.w
        );
    }

    #[test]
    fn nested_escape_and_edge_parity_across_modes() {
        // Nested R3 escape (depth-2 leaf at its group edge): identical.
        let escape_group = Rig::seed4();
        let mut escape_group = escape_group;
        escape_group.focus("left");
        let escape_group = escape_group.mv("w3", "left", SameAxisMove::GroupWithNeighbor);
        let mut escape_swap = Rig::seed4();
        escape_swap.focus("left");
        let escape_swap = escape_swap.mv("w3", "left", SameAxisMove::SwapWithNeighbor);
        assert_eq!(escape_group, escape_swap);
        let plan = move_plan(escape_group, "nested escape");
        assert_eq!(plan.rule, Rule::R3);
        // Perpendicular R1 wrap at a nested edge: identical.
        let wrap_group =
            Rig::setup_vertical_pair().mv("w1", "right", SameAxisMove::GroupWithNeighbor);
        let wrap_swap =
            Rig::setup_vertical_pair().mv("w1", "right", SameAxisMove::SwapWithNeighbor);
        assert_eq!(wrap_group, wrap_swap);
        assert_eq!(move_plan(wrap_group, "nested wrap").rule, Rule::R1);
        // R2c with a group neighbor keeps the wrap under both modes.
        let groupwrap_group =
            Rig::setup_vertical_pair().mv("w1", "down", SameAxisMove::GroupWithNeighbor);
        let groupwrap_swap =
            Rig::setup_vertical_pair().mv("w1", "down", SameAxisMove::SwapWithNeighbor);
        assert_eq!(groupwrap_group, groupwrap_swap);
        assert_eq!(
            move_plan(groupwrap_group, "group-neighbor wrap").rule,
            Rule::R2c
        );
        // Root-edge boundary noop on a group: identical refusal.
        let mut edge_group = Rig::seed_n(&["w1", "w2"]);
        edge_group.focus("left");
        let edge_group = edge_group.mv("w1", "left", SameAxisMove::GroupWithNeighbor);
        let mut edge_swap = Rig::seed_n(&["w1", "w2"]);
        edge_swap.focus("left");
        let edge_swap = edge_swap.mv("w1", "left", SameAxisMove::SwapWithNeighbor);
        assert_eq!(edge_group, edge_swap);
        assert!(
            matches!(edge_group, CoreReply::Rejected { .. }),
            "root edge refuses, got {edge_group:?}"
        );
    }
}
