use std::collections::BTreeMap;

use crate::directional::{OutputId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::ObservedWindow;
use crate::size_hints::WindowSizeHints;

use super::*;

fn domain() -> OutputDomain {
    OutputDomain {
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

fn engine_window(id: &str, floating: bool) -> EngineWindow {
    let d = domain();
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: d.id,
        workspace: d.workspace,
        rect: Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
        floating,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn observed_window(id: &str, floating: bool) -> ObservedWindow {
    let d = domain();
    ObservedWindow {
        window: WindowId(id.to_owned()),
        output: d.id,
        workspace: d.workspace,
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

#[test]
fn tiled_seed_advances_one_revision_per_member_without_retained_writes() {
    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 5;
    let d = domain();
    let mut order = vec![engine_window("a", false), engine_window("b", false)];
    order[0].floating = true;
    let (candidate, counter) = layout.build_tiled_seed(&d, 0, &order).expect("seeds");
    let a = WindowId("a".to_owned());
    let b = WindowId("b".to_owned());
    assert_eq!(candidate.accepted_revision, 2);
    assert_eq!(candidate.leaves.len(), 2);
    assert!(candidate.exceptions.is_empty());
    assert_eq!(candidate.focused_leaf, candidate.leaves.get(&b).cloned());
    assert_eq!(
        candidate.focus_stack,
        vec![
            candidate.leaves.get(&a).cloned().unwrap(),
            candidate.leaves.get(&b).cloned().unwrap(),
        ]
    );
    assert_eq!(candidate.admission_order.get(&a), Some(&5));
    assert_eq!(candidate.admission_order.get(&b), Some(&6));
    assert_eq!(counter, 7);
    assert!(layout.domains.is_empty());
    assert_eq!(layout.admission_seq_next, 5);
    assert!(layout.group_alloc.next_value() > 0);
}

#[test]
fn fresh_mixed_float_tiled_convergence_commits_domain_once() {
    let d = domain();
    let key = d.key();
    let tile = WindowId("tile".to_owned());
    let float = WindowId("float".to_owned());
    let observed = vec![
        observed_window("float", true),
        observed_window("tile", false),
    ];

    let mut layout = WorldLayout::new();
    let counts = layout
        .converge_fresh_domain(&d, 0, &observed, Some(&tile))
        .expect("converges");
    assert_eq!(
        counts,
        ObservationConvergence {
            removed: 0,
            admitted: 1,
            flags_adopted: 1,
        }
    );
    assert_eq!(layout.domains.len(), 1);
    let domain = layout.domain(&key).expect("domain installed");
    assert_eq!(domain.accepted_revision, 1);
    assert!(domain.leaves.contains_key(&tile));
    assert!(domain.exceptions.contains_key(&float));
    assert_eq!(domain.focused_leaf, domain.leaves.get(&tile).cloned());

    let mut empty_layout = WorldLayout::new();
    let empty = empty_layout
        .converge_fresh_domain(&d, 0, &[], None)
        .expect("empty converges");
    assert_eq!(empty.admitted, 0);
    assert!(empty_layout.domains.is_empty());
}
