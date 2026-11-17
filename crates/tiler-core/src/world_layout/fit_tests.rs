use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::{EngineWindow, FitDeclineReason};
use crate::size_hints::WindowSizeHints;

use super::{FittedAdmissionReason, WorldLayout};

fn domain() -> crate::session::OutputDomain {
    crate::session::OutputDomain {
        id: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn window(id: &str, x: i32, w: i32) -> EngineWindow {
    let d = domain();
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: d.id,
        workspace: d.workspace,
        rect: Rect { x, y: 0, w, h: 100 },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

#[test]
fn clean_fit_installs_shared_sequence_and_focus_tail() {
    let d = domain();
    let key = d.key();
    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 5;
    let observed = vec![
        window("a", 0, 100),
        window("b", 100, 100),
        window("c", 200, 100),
    ];
    let anchor = WindowId("b".to_owned());

    let outcome = layout
        .try_fitted_admission(&d, 0, &anchor, &observed)
        .expect("clean fit installs");

    assert_eq!(outcome.base_revision, 0);
    assert_eq!(outcome.revision, 1);
    assert_eq!(outcome.geometry.len(), 3);
    let stored = layout.domain(&key).expect("domain installed");
    assert_eq!(stored.accepted_revision, 1);
    let focus_leaf = stored.leaves.get(&anchor).cloned().expect("anchor bound");
    assert_eq!(outcome.admitted_leaf, Some(focus_leaf.clone()));
    assert_eq!(stored.focused_leaf, Some(focus_leaf.clone()));
    assert_eq!(stored.focus_stack, vec![focus_leaf.clone()]);
    assert!(stored.focus_history.is_empty());
    assert_eq!(stored.last_active, Some(focus_leaf));
    assert_eq!(stored.admission_order.len(), 3);
    assert!(stored.admission_order.values().all(|value| *value == 5));
    assert_eq!(layout.admission_seq_next, 6);
    assert!(layout.group_alloc.next_value() >= 1);
}

#[test]
fn fit_declines_burn_without_install_or_highwater_advance() {
    // Overlap beyond tolerance forces a centre split: the fit declines but
    // its group serial still burns.
    {
        let d = domain();
        let mut layout = WorldLayout::new();
        layout.admission_seq_next = 9;
        let observed = vec![window("a", 0, 100), window("b", 80, 100)];
        let decline = layout
            .try_fitted_admission(&d, 0, &WindowId("a".to_owned()), &observed)
            .expect_err("centre split declines");
        assert_eq!(
            decline.reason,
            FittedAdmissionReason::Fit(FitDeclineReason::CentreSplit)
        );
        assert_eq!(decline.centre_splits, 1);
        assert_eq!(decline.reason.as_str(), "centre_split");
        assert!(layout.domains.is_empty());
        assert_eq!(layout.admission_seq_next, 9);
        assert!(layout.group_alloc.next_value() >= 1);
    }

    // A clean fit whose hinted projection is min-infeasible declines too,
    // with no install or high-water advance but with the fitter burn intact.
    {
        let d = domain();
        let mut layout = WorldLayout::new();
        layout.admission_seq_next = 4;
        let mut tight = window("a", 0, 100);
        tight.hints = WindowSizeHints {
            min_w: Some(500),
            ..WindowSizeHints::none()
        };
        let observed = vec![tight, window("b", 100, 100)];
        let decline = layout
            .try_fitted_admission(&d, 0, &WindowId("a".to_owned()), &observed)
            .expect_err("min-infeasible declines");
        assert_eq!(
            decline.reason,
            FittedAdmissionReason::Fit(FitDeclineReason::MinInfeasible)
        );
        assert_eq!(decline.centre_splits, 0);
        assert_eq!(decline.reason.as_str(), "min_infeasible");
        assert!(layout.domains.is_empty());
        assert_eq!(layout.admission_seq_next, 4);
        assert!(layout.group_alloc.next_value() >= 1);
    }
}

#[test]
fn shared_allocator_across_domains_and_discarded_clones_never_reissues() {
    use std::collections::BTreeSet;

    fn domain_for(output: &str, workspace: &str) -> crate::session::OutputDomain {
        crate::session::OutputDomain {
            id: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn win_for(id: &str, output: &str, workspace: &str, x: i32, w: i32) -> EngineWindow {
        EngineWindow {
            window: WindowId(id.to_owned()),
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            rect: Rect { x, y: 0, w, h: 100 },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn group_ids(node: &crate::directional::Node, out: &mut BTreeSet<String>) {
        if let crate::directional::Node::Group { id, children, .. } = node {
            out.insert(id.0.clone());
            for child in children {
                group_ids(child, out);
            }
        }
    }

    let mut layout = WorldLayout::new();
    let d_a = domain_for("out-a", "ws");
    let d_b = domain_for("out-b", "ws");

    // First domain mints grp-0.
    layout
        .try_fitted_admission(
            &d_a,
            0,
            &WindowId("a1".to_owned()),
            &[
                win_for("a1", "out-a", "ws", 0, 150),
                win_for("a2", "out-a", "ws", 150, 150),
            ],
        )
        .expect("fit a succeeds");
    // Second domain mints grp-1 on the same shared allocator.
    layout
        .try_fitted_admission(
            &d_b,
            0,
            &WindowId("b1".to_owned()),
            &[
                win_for("b1", "out-b", "ws", 0, 150),
                win_for("b2", "out-b", "ws", 150, 150),
            ],
        )
        .expect("fit b succeeds");

    let mut ids_a = BTreeSet::new();
    group_ids(layout.tree(&d_a.key()).unwrap(), &mut ids_a);
    let mut ids_b = BTreeSet::new();
    group_ids(layout.tree(&d_b.key()).unwrap(), &mut ids_b);
    assert_eq!(ids_a, BTreeSet::from(["grp-0".to_owned()]));
    assert_eq!(ids_b, BTreeSet::from(["grp-1".to_owned()]));
    assert_eq!(layout.group_alloc.next_value(), 2);

    // Discarded clone trial burns grp-2 without mutating original domains.
    let mut staged = layout.clone();
    let d_c = domain_for("out-c", "ws");
    staged
        .try_fitted_admission(
            &d_c,
            0,
            &WindowId("c1".to_owned()),
            &[
                win_for("c1", "out-c", "ws", 0, 150),
                win_for("c2", "out-c", "ws", 150, 150),
            ],
        )
        .expect("staged fit succeeds");
    let mut ids_c = BTreeSet::new();
    group_ids(staged.tree(&d_c.key()).unwrap(), &mut ids_c);
    assert_eq!(ids_c, BTreeSet::from(["grp-2".to_owned()]));
    drop(staged);
    assert!(!layout.contains_domain(&d_c.key()));
    assert_eq!(layout.group_alloc.next_value(), 3);

    // Next fit on the original layout continues at grp-3, never reissuing grp-2.
    let d_d = domain_for("out-d", "ws");
    layout
        .try_fitted_admission(
            &d_d,
            0,
            &WindowId("d1".to_owned()),
            &[
                win_for("d1", "out-d", "ws", 0, 150),
                win_for("d2", "out-d", "ws", 150, 150),
            ],
        )
        .expect("fit d succeeds");
    let mut ids_d = BTreeSet::new();
    group_ids(layout.tree(&d_d.key()).unwrap(), &mut ids_d);
    assert_eq!(ids_d, BTreeSet::from(["grp-3".to_owned()]));
    assert_eq!(layout.group_alloc.next_value(), 4);
}
