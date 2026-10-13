#![forbid(unsafe_code)]
//! Position-based FULL-rectangle candidate selection (2026-10-09).
//!
//! Exhausted moves/sends rank by window-centre projection, then window-span
//! overlap, then left/top. Migration ranks by largest shared edge, then
//! left/top, never window position. FULL rects select; work areas place.

use tiler_core::directional::{Direction, OutputId};
use tiler_core::geometry::Rect;
use tiler_core::output_selection::{
    OutputEntry, SelectionError, select_for_migration, select_for_window,
};

fn entry(id: &str, x: i32, y: i32, w: i32, h: i32) -> OutputEntry {
    OutputEntry {
        id: OutputId(id.to_owned()),
        rect: Rect { x, y, w, h },
    }
}

fn all(entries: &[OutputEntry]) -> Vec<OutputEntry> {
    entries.to_vec()
}

fn pick_window(
    direction: Direction,
    source: &OutputEntry,
    window: &Rect,
    topology: &[OutputEntry],
) -> OutputId {
    select_for_window(direction, source, window, topology)
        .expect("readable topology")
        .expect("candidate")
        .id
}

// Two stacked candidates above a wide source: left/right centre decides.
#[test]
fn centre_precedence_up_two_candidates() {
    let source = entry("src", 0, 600, 1600, 600);
    let left = entry("out-l", 0, 0, 800, 600);
    let right = entry("out-r", 800, 0, 800, 600);
    let topology = all(&[source.clone(), left.clone(), right.clone()]);
    // Window on the left half: centre x=400 falls in left shared edge [0,800).
    let win_left = Rect {
        x: 100,
        y: 700,
        w: 200,
        h: 200,
    };
    assert_eq!(
        pick_window(Direction::Up, &source, &win_left, &topology),
        OutputId("out-l".to_owned())
    );
    // Window on the right half: centre x=1200 falls in right edge [800,1600).
    let win_right = Rect {
        x: 1100,
        y: 700,
        w: 200,
        h: 200,
    };
    assert_eq!(
        pick_window(Direction::Up, &source, &win_right, &topology),
        OutputId("out-r".to_owned())
    );
}

// Horizontal mirror: two candidates right of source, centre y decides.
#[test]
fn centre_precedence_right_two_candidates() {
    let source = entry("src", 0, 0, 800, 1200);
    let top = entry("out-t", 800, 0, 800, 600);
    let bottom = entry("out-b", 800, 600, 800, 600);
    let topology = all(&[source.clone(), top.clone(), bottom.clone()]);
    let win_top = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Right, &source, &win_top, &topology),
        OutputId("out-t".to_owned())
    );
    let win_bottom = Rect {
        x: 100,
        y: 900,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Right, &source, &win_bottom, &topology),
        OutputId("out-b".to_owned())
    );
}

// All four directions resolve two-candidate topologies through the same rule.
#[test]
fn all_four_directions_select_two_candidates() {
    // Left: two candidates west of source; centre y decides.
    let src = entry("src", 800, 0, 800, 1200);
    let north_west = entry("nw", 0, 0, 800, 600);
    let south_west = entry("sw", 0, 600, 800, 600);
    let west = all(&[src.clone(), north_west.clone(), south_west.clone()]);
    let win_north = Rect {
        x: 900,
        y: 100,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Left, &src, &win_north, &west),
        OutputId("nw".to_owned())
    );
    let win_south = Rect {
        x: 900,
        y: 900,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Left, &src, &win_south, &west),
        OutputId("sw".to_owned())
    );
    // Right mirror.
    let north_east = entry("ne", 1600, 0, 800, 600);
    let south_east = entry("se", 1600, 600, 800, 600);
    let east = all(&[src.clone(), north_east.clone(), south_east.clone()]);
    assert_eq!(
        pick_window(Direction::Right, &src, &win_north, &east),
        OutputId("ne".to_owned())
    );
    assert_eq!(
        pick_window(Direction::Right, &src, &win_south, &east),
        OutputId("se".to_owned())
    );
    // Up: two candidates above a wide source; centre x decides.
    let wide = entry("wide", 0, 600, 1600, 600);
    let up_l = entry("up-l", 0, 0, 800, 600);
    let up_r = entry("up-r", 800, 0, 800, 600);
    let up = all(&[wide.clone(), up_l.clone(), up_r.clone()]);
    let win_west = Rect {
        x: 100,
        y: 700,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Up, &wide, &win_west, &up),
        OutputId("up-l".to_owned())
    );
    let win_east = Rect {
        x: 1100,
        y: 700,
        w: 200,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Up, &wide, &win_east, &up),
        OutputId("up-r".to_owned())
    );
    // Down mirror.
    let down_l = entry("down-l", 0, 1200, 800, 600);
    let down_r = entry("down-r", 800, 1200, 800, 600);
    let down = all(&[wide.clone(), down_l.clone(), down_r.clone()]);
    assert_eq!(
        pick_window(Direction::Down, &wide, &win_west, &down),
        OutputId("down-l".to_owned())
    );
    assert_eq!(
        pick_window(Direction::Down, &wide, &win_east, &down),
        OutputId("down-r".to_owned())
    );
}

// Half-open seam: a centre exactly on the shared boundary belongs to the
// upper interval ([800,1600)), not the lower ([0,800)).
#[test]
fn half_open_seam_prefers_upper_interval() {
    let source = entry("src", 0, 600, 1600, 600);
    let left = entry("out-l", 0, 0, 800, 600);
    let right = entry("out-r", 800, 0, 800, 600);
    let topology = all(&[source.clone(), left, right]);
    // Centre x=800 exactly: 2*800=1600 is excluded from [0,800) and included
    // in [800,1600).
    let win = Rect {
        x: 700,
        y: 700,
        w: 200,
        h: 200,
    };
    assert_eq!(
        pick_window(Direction::Up, &source, &win, &topology),
        OutputId("out-r".to_owned())
    );
}

// Odd extents keep their exact `.5` centre in doubled coordinates: a window
// with h=101 at y=700 centres on 750.5, inside [700,800), not rounded away.
#[test]
fn odd_extent_centre_is_exact() {
    let source = entry("src", 0, 0, 800, 1200);
    let top = entry("out-t", 800, 0, 800, 700);
    let bottom = entry("out-b", 800, 700, 800, 500);
    let topology = all(&[source.clone(), top, bottom]);
    let win = Rect {
        x: 100,
        y: 700,
        w: 100,
        h: 101,
    };
    assert_eq!(
        pick_window(Direction::Right, &source, &win, &topology),
        OutputId("out-b".to_owned())
    );
}

// Centre in the gap between candidates: largest window-span overlap wins.
#[test]
fn overlap_fallback_when_centre_in_gap() {
    // Candidates cover [0,400) and [1200,1600); centre x=800 is in neither.
    let source = entry("src2", 0, 600, 1600, 600);
    let narrow_l = entry("nl", 0, 0, 400, 600);
    let narrow_r = entry("nr", 1200, 0, 400, 600);
    let topo = all(&[source.clone(), narrow_l.clone(), narrow_r.clone()]);
    let win = Rect {
        x: 700,
        y: 700,
        w: 200,
        h: 200,
    }; // centre x=800 in neither; span [700,900) overlaps neither either...
    // Span overlaps neither: overlap ties at 0, left/top picks nl.
    assert_eq!(
        pick_window(Direction::Up, &source, &win, &topo),
        OutputId("nl".to_owned())
    );
    // Wider span overlapping right more than left picks right.
    let wide = Rect {
        x: 300,
        y: 700,
        w: 1100,
        h: 200,
    }; // span [300,1400): overlaps nl [0,400)=100, nr [1200,1600)=200.
    assert_eq!(
        pick_window(Direction::Up, &source, &wide, &topo),
        OutputId("nr".to_owned())
    );
}

// Deterministic left/top when overlaps tie.
#[test]
fn deterministic_left_top_on_overlap_tie() {
    let source = entry("src", 0, 600, 1600, 600);
    let left = entry("out-l", 0, 0, 400, 600);
    let right = entry("out-r", 1200, 0, 400, 600);
    let topology = all(&[source.clone(), left, right]);
    // Window span covers both equally: [0,1600) x-span overlaps both 400.
    let win = Rect {
        x: 0,
        y: 700,
        w: 1600,
        h: 100,
    };
    // Centre x=800 is in neither shared edge ([0,400)/[1200,1600)), so the
    // overlap fallback ties 400 vs 400 and left/top picks out-l.
    assert_eq!(
        pick_window(Direction::Up, &source, &win, &topology),
        OutputId("out-l".to_owned())
    );
}

// Migration uses largest shared edge, never window position.
#[test]
fn migration_largest_edge_ignores_window() {
    let source = entry("src", 0, 600, 1600, 600);
    let narrow = entry("narrow", 0, 0, 400, 600);
    let wide = entry("wide", 400, 0, 1200, 600);
    let topology = all(&[source.clone(), narrow.clone(), wide.clone()]);
    // Windowed selection with a left-half window picks narrow by centre.
    let win_left = Rect {
        x: 50,
        y: 700,
        w: 100,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Up, &source, &win_left, &topology),
        OutputId("narrow".to_owned())
    );
    // Migration ignores the window and picks the largest shared edge.
    assert_eq!(
        select_for_migration(Direction::Up, &source, &topology)
            .expect("readable")
            .expect("candidate")
            .id,
        OutputId("wide".to_owned())
    );
    // Migration tie goes left/top.
    let a = entry("a", 0, 0, 800, 600);
    let b = entry("b", 800, 0, 800, 600);
    let tied = all(&[source.clone(), a, b]);
    assert_eq!(
        select_for_migration(Direction::Up, &source, &tied)
            .expect("readable")
            .expect("candidate")
            .id,
        OutputId("a".to_owned())
    );
}

// FULL rectangles select; work-area panel gaps must not block.
#[test]
fn full_rects_touch_while_work_areas_gap() {
    // FULL rects edge-touch at y=600.
    let full_src = entry("src", 0, 0, 800, 600);
    let full_up = entry("up", 0, 600, 800, 600);
    let full = all(&[full_src.clone(), full_up.clone()]);
    let win = Rect {
        x: 100,
        y: 100,
        w: 100,
        h: 100,
    };
    assert!(
        select_for_window(Direction::Down, &full_src, &win, &full)
            .expect("readable")
            .is_some(),
        "FULL touch must candidate"
    );
    // Work areas leave a 60px panel gap: 570 vs 630, no touch.
    let work_src = entry("src", 0, 0, 800, 570);
    let work_up = entry("up", 0, 630, 800, 570);
    let work = all(&[work_src.clone(), work_up]);
    assert!(
        select_for_window(Direction::Down, &work_src, &win, &work)
            .expect("readable")
            .is_none(),
        "work-area gap must not candidate; FULL selects, work areas place"
    );
}

// Unreadable window rect refuses so callers refuse fail-closed.
#[test]
fn unreadable_window_refuses() {
    let source = entry("src", 0, 600, 800, 600);
    let up = entry("up", 0, 0, 800, 600);
    let topology = all(&[source.clone(), up]);
    let bad = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 10,
    };
    assert_eq!(
        select_for_window(Direction::Up, &source, &bad, &topology),
        Err(SelectionError::UnreadableTopology)
    );
}

// Unreadable non-source topology refuses rather than silently skipping.
#[test]
fn unreadable_non_source_topology_refuses() {
    let source = entry("src", 0, 600, 800, 600);
    let good = entry("good", 0, 0, 800, 600);
    let bad = entry("bad", 800, 0, 0, 600);
    let topology = all(&[source.clone(), good, bad]);
    let win = Rect {
        x: 100,
        y: 650,
        w: 100,
        h: 100,
    };
    assert_eq!(
        select_for_window(Direction::Up, &source, &win, &topology),
        Err(SelectionError::UnreadableTopology)
    );
}

// Valid reverse ambiguity is accepted at the core layer: the presented pair
// only needs reciprocity existence, not reverse uniqueness. In FULL topology
// a third output may also touch the target on the same side, but the adapter
// resolves the forward pair before presenting it, so the core plans the
// reciprocal pair. Session maps hold one target per direction and cannot
// represent the unresolved triple; this asserts the directional outcome.
#[test]
fn core_accepts_valid_reverse_ambiguity() {
    use std::collections::BTreeMap;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::directional::{
        MoveIntent, Node, NodeId, Output, SameAxisMove, Snapshot, WindowLink,
    };
    use tiler_core::directional::{Rule, WindowId};

    let snapshot = Snapshot {
        outputs: vec![
            Output {
                id: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                tree: Some(Node::Leaf {
                    id: NodeId("leaf-a".to_owned()),
                }),
                adjacent: BTreeMap::from([(Direction::Right, OutputId("out-2".to_owned()))]),
            },
            Output {
                id: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                tree: Some(Node::Leaf {
                    id: NodeId("leaf-x".to_owned()),
                }),
                adjacent: BTreeMap::from([(Direction::Left, OutputId("out-1".to_owned()))]),
            },
        ],
        windows: vec![
            WindowLink {
                window: WindowId("win-a".to_owned()),
                leaf: NodeId("leaf-a".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
            WindowLink {
                window: WindowId("win-x".to_owned()),
                leaf: NodeId("leaf-x".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
        ],
    };
    let intent = MoveIntent {
        source_output: OutputId("out-1".to_owned()),
        focused_leaf: NodeId("leaf-a".to_owned()),
        focused_window: WindowId("win-a".to_owned()),
        direction: Direction::Right,
        same_axis_move: SameAxisMove::GroupWithNeighbor,
    };
    match tiler_core::directional::plan_move(&snapshot, &intent) {
        tiler_core::directional::MoveOutcome::Planned(plan) => {
            assert_eq!(plan.rule, Rule::R4);
        }
        other => {
            panic!("reciprocal pair must plan despite third-output reverse touch, got {other:?}")
        }
    }
}

// Overlapping shared edges both containing the centre skip span overlap
// straight to left/top: the later candidate wins more window overlap here
// yet must lose.
#[test]
fn both_centred_skips_overlap_to_left_top() {
    let source = entry("src", 0, 600, 1600, 600);
    // Overlapping (mirrored) rectangles above: shared edges [0,1200) and
    // [400,1600). Both contain centre x=1100; the later edge overlaps the
    // window span more (400 vs 300) but left/top still picks "a".
    let a = entry("a", 0, 0, 1200, 600);
    let b = entry("b", 400, 0, 1200, 600);
    let topology = all(&[source.clone(), a, b]);
    let win = Rect {
        x: 900,
        y: 700,
        w: 400,
        h: 100,
    };
    assert_eq!(
        pick_window(Direction::Up, &source, &win, &topology),
        OutputId("a".to_owned())
    );
}

// The topology entry matching the source id must carry the same rect;
// a mismatch refuses rather than selecting against stale geometry.
#[test]
fn source_rect_mismatch_refuses() {
    let source = entry("src", 0, 600, 1600, 600);
    let stale = entry("src", 0, 0, 1600, 600);
    let up = entry("up", 0, 0, 1600, 600);
    let topology = all(&[stale, up]);
    let win = Rect {
        x: 100,
        y: 700,
        w: 100,
        h: 100,
    };
    assert_eq!(
        select_for_window(Direction::Up, &source, &win, &topology),
        Err(SelectionError::UnreadableTopology)
    );
}
