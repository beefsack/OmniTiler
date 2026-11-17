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
