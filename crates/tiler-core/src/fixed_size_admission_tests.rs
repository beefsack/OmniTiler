#![forbid(unsafe_code)]
//! Q2 fixed-size float admission rows (D1-D8, authorized batch).
//!
//! Real `WorldLayout` fixtures (no flag mocks): the shared
//! `is_fixed_size` predicate floats newly admitted fixed windows as
//! membership-only automatic exceptions, honors explicit user tile wins
//! (retained set, command origin, or observed suppress signal) for the
//! same live client, bypasses born fullscreen, floats born maximized
//! without a reserved tile, retiles automatic (never intentional) floats,
//! and keeps opt-out refusals intact. Failed admissions stage nothing.

use crate::directional::{Node, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::{LayoutFailure, PairOperationContext, PairSendKind, WorldLayout};
use std::collections::{BTreeMap, BTreeSet};

fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}
fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}
fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    }
}
#[allow(clippy::too_many_arguments)]
fn obs(
    window: &str,
    floating: bool,
    fullscreen: bool,
    maximized: bool,
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating,
        fullscreen,
        maximized,
        sticky,
        fixed_auto,
        fixed_suppress,
        hints,
    }
}
fn plain(window: &str) -> ObservedWindow {
    obs(
        window,
        false,
        false,
        false,
        false,
        false,
        false,
        WindowSizeHints::none(),
    )
}
fn fixed(window: &str) -> ObservedWindow {
    obs(
        window,
        false,
        false,
        false,
        false,
        false,
        false,
        fixed_hints(),
    )
}
fn fixed_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    layout
}
fn current_observed(layout: &WorldLayout, k: &DomainKey) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(k)
        .map(|w| ObservedWindow {
            window: w.clone(),
            output: k.output.clone(),
            workspace: k.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        })
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(k) {
        windows.push(ObservedWindow {
            window: w,
            output: k.output.clone(),
            workspace: k.workspace.clone(),
            floating: flags.floating,
            fullscreen: flags.fullscreen,
            maximized: flags.maximized,
            sticky: flags.sticky,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        });
    }
    windows
}
fn converge_at(
    layout: &mut WorldLayout,
    dom: &OutputDomain,
    windows: Vec<ObservedWindow>,
) -> Result<crate::session::ObservationConvergence, LayoutFailure> {
    let k = dom.key();
    if layout.contains_domain(&k) {
        layout.converge_domain(&k, &windows, None, &BTreeSet::new())
    } else {
        layout.converge_fresh_domain(dom, 0, &windows, None)
    }
}
fn converge(
    layout: &mut WorldLayout,
    windows: Vec<ObservedWindow>,
) -> Result<crate::session::ObservationConvergence, LayoutFailure> {
    converge_at(layout, &domain(), windows)
}
fn admit_plain(layout: &mut WorldLayout, window: &str) {
    admit_with(layout, window, false, WindowSizeHints::none());
}
fn admit_with(layout: &mut WorldLayout, window: &str, suppress: bool, hints: WindowSizeHints) {
    let dom = domain();
    let k = dom.key();
    let mut windows = current_observed(layout, &k);
    let mut admitted = plain(window);
    admitted.hints = hints;
    windows.push(admitted);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    if !layout.contains_domain(&k) {
        if suppress && let Some(entry) = windows.iter_mut().find(|e| e.window.0 == window) {
            entry.fixed_suppress = true;
        }
        layout
            .converge_fresh_domain(&dom, 0, &windows, None)
            .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
        return;
    }
    layout
        .admit_window(
            &k,
            &WindowId(window.to_owned()),
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            suppress,
            false,
            &windows,
        )
        .unwrap_or_else(|e| panic!("seed admit {window}: {e:?}"));
}
fn tiled_ids(layout: &WorldLayout) -> Vec<String> {
    let mut ids: Vec<String> = layout.tiled_windows(&key()).map(|w| w.0.clone()).collect();
    ids.sort();
    ids
}
fn tree_leaf_count(layout: &WorldLayout) -> usize {
    fn collect(node: &Node, out: &mut usize) {
        match node {
            Node::Leaf { .. } => *out += 1,
            Node::Group { children, .. } => {
                for child in children {
                    collect(child, out);
                }
            }
        }
    }
    match layout.tree(&key()) {
        None => 0,
        Some(tree) => {
            let mut count = 0;
            collect(tree, &mut count);
            count
        }
    }
}

#[test]
fn fixed_brand_new_floats_without_tile_focus_or_geometry() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    let focus_before = layout.focus_leaf(&key()).cloned();
    let result = converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("converge");
    assert_eq!(result.flags_adopted, 1, "fixed admission adopts flags");
    assert_eq!(result.admitted, 0, "fixed admission takes no tiled slot");
    assert_eq!(result.removed, 0);
    assert!(
        layout.is_exception(&key(), &WindowId("win-f".to_owned())),
        "fixed is exception"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())),
        "automatic marker"
    );
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-a".to_owned()],
        "tree excludes fixed"
    );
    assert_eq!(tree_leaf_count(&layout), 1, "no leaf reserved for fixed");
    assert_eq!(
        layout.focus_leaf(&key()).cloned(),
        focus_before,
        "admission never refocuses to fixed"
    );
}

#[test]
fn nonfixed_hint_variants_tile() {
    let variants: Vec<(&str, WindowSizeHints)> = vec![
        (
            "missing-max",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: None,
                max_h: None,
            },
        ),
        (
            "missing-min",
            WindowSizeHints {
                min_w: None,
                min_h: None,
                max_w: Some(640),
                max_h: Some(480),
            },
        ),
        (
            "one-axis",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(100),
                max_w: Some(640),
                max_h: Some(480),
            },
        ),
        (
            "unequal",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(800),
                max_h: Some(600),
            },
        ),
        (
            "full-zero",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(0),
                max_w: Some(0),
                max_h: Some(0),
            },
        ),
        ("unset", WindowSizeHints::none()),
        (
            "negative",
            WindowSizeHints {
                min_w: Some(-1),
                min_h: Some(480),
                max_w: Some(-1),
                max_h: Some(480),
            },
        ),
        (
            "sentinel",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(i32::MAX),
                max_h: Some(i32::MAX),
            },
        ),
        (
            "out-of-range",
            WindowSizeHints {
                min_w: Some(16385),
                min_h: Some(16385),
                max_w: Some(16385),
                max_h: Some(16385),
            },
        ),
    ];
    for (name, hints) in variants {
        let mut layout = fixed_layout();
        let result = converge(
            &mut layout,
            vec![obs(
                "win-v", false, false, false, false, false, false, hints,
            )],
        )
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(result.admitted, 1, "{name} tiles");
        assert!(
            !layout.is_exception(&key(), &WindowId("win-v".to_owned())),
            "{name} not exception"
        );
        assert!(
            !layout.is_automatic_fixed_float(&key(), &WindowId("win-v".to_owned())),
            "{name} not automatic"
        );
        assert_eq!(
            tiled_ids(&layout),
            vec!["win-v".to_owned()],
            "{name} in tree"
        );
    }
}

#[test]
fn partial_zero_vectors_tile_under_both_axes_and_float_under_either_axis() {
    use crate::size_hints::FixedSizePredicate;
    // G-05: zero is unset per axis. Both transposed equal partial-zero
    // vectors pin nothing under `both-axes-fixed` (tile) but pin via their
    // nonzero axis under `either-axis-fixed` (float). Full-zero and the
    // unbounded sentinel stay tiled under both predicates.
    let partials = [
        (
            "w-fixed",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(0),
                max_w: Some(640),
                max_h: Some(0),
            },
        ),
        (
            "h-fixed",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(480),
                max_w: Some(0),
                max_h: Some(480),
            },
        ),
    ];
    for (name, hints) in partials {
        let mut both = fixed_layout();
        let result = converge(
            &mut both,
            vec![obs(
                "win-p", false, false, false, false, false, false, hints,
            )],
        )
        .unwrap_or_else(|e| panic!("{name} both-axes: {e:?}"));
        assert_eq!(result.admitted, 1, "{name} tiles under both-axes");
        assert!(
            !both.is_automatic_fixed_float(&key(), &WindowId("win-p".to_owned())),
            "{name} not automatic under both-axes"
        );
        assert_eq!(
            tiled_ids(&both),
            vec!["win-p".to_owned()],
            "{name} keeps a slot under both-axes"
        );

        let mut either = fixed_layout();
        either.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        converge(
            &mut either,
            vec![obs(
                "win-p", false, false, false, false, false, false, hints,
            )],
        )
        .unwrap_or_else(|e| panic!("{name} either-axis: {e:?}"));
        assert!(
            either.is_automatic_fixed_float(&key(), &WindowId("win-p".to_owned())),
            "{name} floats under either-axis"
        );
        assert_eq!(
            tiled_ids(&either),
            Vec::<String>::new(),
            "{name} takes no slot under either-axis"
        );
    }
    // Guards unchanged: full-zero and sentinel tile under either-axis too.
    for (name, hints) in [
        (
            "full-zero",
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(0),
                max_w: Some(0),
                max_h: Some(0),
            },
        ),
        (
            "sentinel",
            WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(i32::MAX),
                max_h: Some(i32::MAX),
            },
        ),
    ] {
        let mut either = fixed_layout();
        either.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        converge(
            &mut either,
            vec![obs(
                "win-p", false, false, false, false, false, false, hints,
            )],
        )
        .unwrap_or_else(|e| panic!("{name} either-axis: {e:?}"));
        assert!(
            !either.is_automatic_fixed_float(&key(), &WindowId("win-p".to_owned())),
            "{name} still tiles under either-axis"
        );
    }
}

#[test]
fn partial_zero_predicate_switch_never_reclassifies_retained_tile() {
    use crate::size_hints::FixedSizePredicate;
    // Admissions-only (D1): a partial-zero client admitted tiled under
    // `both-axes-fixed` stays tiled after the switch to `either-axis-fixed`.
    let hints = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(0),
        max_w: Some(640),
        max_h: Some(0),
    };
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-p", false, false, false, false, false, false, hints,
        )],
    )
    .expect("partial-zero tiles by default");
    assert_eq!(tiled_ids(&layout), vec!["win-p".to_owned()]);
    layout.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    let result = converge(
        &mut layout,
        vec![obs(
            "win-p", false, false, false, false, false, false, hints,
        )],
    )
    .expect("switch converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "switch changes nothing retained"
    );
    assert_eq!(tiled_ids(&layout), vec!["win-p".to_owned()]);
    assert!(!layout.is_automatic_fixed_float(&key(), &WindowId("win-p".to_owned())));
}

#[test]
fn hint_transitions_never_reclassify_either_direction() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    // Tiled window becomes fixed: stays tiled (D2).
    let result = converge(&mut layout, vec![fixed("win-a")]).expect("hint change converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "no change"
    );
    assert_eq!(tiled_ids(&layout), vec!["win-a".to_owned()]);
    assert!(!layout.is_automatic_fixed_float(&key(), &WindowId("win-a".to_owned())));
    // Automatic float loses fixed hints: stays floating (D2).
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
    let result = converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                true,
                false,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ],
    )
    .expect("hint loss converges");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "identity kept"
    );
    assert!(
        layout.is_exception(&key(), &WindowId("win-f".to_owned())),
        "still floating"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())),
        "still automatic"
    );
}

#[test]
fn automatic_hint_loss_with_native_unfloat_stays_floating_until_explicit_win() {
    // D2: a known automatic float observed native floating:false with
    // hintless hints keeps floating with its marker (no fresh
    // admission, so no new geometry or focus); only a genuine
    // explicit win (suppress signal, retained override) tiles.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "automatic marker set"
    );
    let result =
        converge(&mut layout, vec![plain("win-a"), plain("win-f")]).expect("hint loss converges");
    assert_eq!(
        (result.removed, result.admitted),
        (0, 0),
        "no fresh admission"
    );
    assert!(layout.is_exception(&key(), &id), "still floating");
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "marker kept across hint loss"
    );
    assert!(
        !layout.has_fixed_tile_override(&key(), &id),
        "no inferred win"
    );
    // An observed suppress signal with the same hintless hints is a
    // genuine explicit win: it tiles and records the override.
    let suppressed = obs(
        "win-f",
        false,
        false,
        false,
        false,
        false,
        true,
        WindowSizeHints::none(),
    );
    converge(&mut layout, vec![plain("win-a"), suppressed]).expect("suppress converges");
    assert!(!layout.is_exception(&key(), &id), "tiled");
    assert!(layout.has_fixed_tile_override(&key(), &id), "win recorded");
    // The retained override keeps it tiled across restored or lost hints.
    for hints in [fixed_hints(), WindowSizeHints::none()] {
        let shown = obs("win-f", false, false, false, false, false, false, hints);
        let result = converge(&mut layout, vec![plain("win-a"), shown]).expect("resync converges");
        assert_eq!(
            (result.removed, result.admitted),
            (0, 0),
            "override keeps tile"
        );
        assert_eq!(tiled_ids(&layout).len(), 2, "stays tiled");
    }
}

#[test]
fn floating_fixed_without_origin_signal_stays_intentional() {
    // Known floating intent is never inferred automatic from hints
    // alone: without the adapter's explicit fixed_auto origin the
    // window floats as an intentional exception.
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-f",
            true,
            false,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("converge");
    let id = WindowId("win-f".to_owned());
    assert!(layout.is_exception(&key(), &id), "floating membership");
    assert!(
        !layout.is_automatic_fixed_float(&key(), &id),
        "no inferred origin"
    );
    // Re-observed tiled without a suppress signal becomes automatic
    // (D5/D6 fixed becomes float): no override is inferred, the
    // automatic marker is. An explicit suppress still tiles (D3, see
    // suppress_signal_records_override_on_retile).
    converge(&mut layout, vec![fixed("win-f")]).expect("retile");
    assert!(layout.is_exception(&key(), &id), "fixed stays floating");
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "fresh automatic marker"
    );
    assert!(
        !layout.has_fixed_tile_override(&key(), &id),
        "no inferred override"
    );
}

#[test]
fn fixed_auto_signal_marks_automatic() {
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-f",
            true,
            false,
            false,
            false,
            true,
            false,
            fixed_hints(),
        )],
    )
    .expect("converge");
    let id = WindowId("win-f".to_owned());
    assert!(layout.is_exception(&key(), &id));
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "explicit origin honored"
    );
}

#[test]
fn suppress_signal_tiles_and_records_override() {
    // The adapter-retained tile win crosses admission: a suppressed
    // fixed window tiles and the win stages into the retained set, so
    // later unsuppressed observations keep it tiled.
    let mut layout = fixed_layout();
    let result = converge(
        &mut layout,
        vec![obs(
            "win-f",
            false,
            false,
            false,
            false,
            false,
            true,
            fixed_hints(),
        )],
    )
    .expect("converge");
    assert_eq!(result.admitted, 1, "suppressed tiles");
    let id = WindowId("win-f".to_owned());
    assert!(!layout.is_exception(&key(), &id));
    assert!(
        layout.has_fixed_tile_override(&key(), &id),
        "win mirrored into retained set"
    );
    let result = converge(&mut layout, vec![fixed("win-f")]).expect("resync");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(tiled_ids(&layout), vec!["win-f".to_owned()], "stays tiled");
}

fn unfloat_committed(layout: &mut WorldLayout, window: &str) {
    let mut windows = current_observed(layout, &key());
    // The unfloat target rides floating until the toggle commits.
    for entry in windows.iter_mut() {
        if entry.window.0 == window {
            entry.floating = true;
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    layout
        .toggle_float(&key(), &WindowId(window.to_owned()), None, &windows)
        .unwrap_or_else(|e| panic!("unfloat {window}: {e:?}"));
}

#[test]
fn explicit_unfloat_wins_across_hide_show() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    unfloat_committed(&mut layout, "win-f");
    let id = WindowId("win-f".to_owned());
    assert!(
        layout.has_fixed_tile_override(&key(), &id),
        "override recorded"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key(), &id),
        "automatic cleared"
    );
    assert_eq!(tiled_ids(&layout).len(), 2, "win-f tiled");
    // Hide/show and hint churn keep the explicit tile (D3, D2).
    for round in 0..2 {
        let hints = if round == 0 {
            fixed_hints()
        } else {
            WindowSizeHints::none()
        };
        let shown = obs("win-f", false, false, false, false, false, false, hints);
        let result = converge(&mut layout, vec![plain("win-a"), shown]).expect("resync");
        assert_eq!(
            (result.removed, result.admitted, result.flags_adopted),
            (0, 0, 0),
            "no re-float"
        );
        assert!(!layout.is_exception(&key(), &id), "stays tiled");
    }
    assert!(
        layout.has_fixed_tile_override(&key(), &id),
        "override survives re-observation"
    );
}

#[test]
fn removal_clears_override_and_replacement_reclassifies() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    unfloat_committed(&mut layout, "win-f");
    let id = WindowId("win-f".to_owned());
    assert!(layout.has_fixed_tile_override(&key(), &id));
    // Removal drops bookkeeping (D3 fence).
    let result = converge(&mut layout, vec![plain("win-a")]).expect("remove");
    assert_eq!(result.removed, 1);
    assert!(
        !layout.has_fixed_tile_override(&key(), &id),
        "override cleared on removal"
    );
    assert!(!layout.is_automatic_fixed_float(&key(), &id));
    // A replaced native reference with the same id classifies again.
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("re-admit");
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "new client floats again"
    );
}

#[test]
fn fixed_maximized_floats_without_tile_nonfixed_maxima_unchanged() {
    let mut layout = fixed_layout();
    // Fixed born maximized: floating base under the native overlay (D4).
    converge(
        &mut layout,
        vec![obs(
            "win-m",
            false,
            false,
            true,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("max fixed converges");
    assert!(
        layout.is_exception(&key(), &WindowId("win-m".to_owned())),
        "max fixed floats"
    );
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-m".to_owned())));
    assert_eq!(
        tree_leaf_count(&layout),
        0,
        "no tile reserved under maximize"
    );
    // Nonfixed born maxima tile exactly as before (Q3 unchanged). The
    // maximize overlay itself stays native-only: core membership rides
    // the floating flag, so the retained exception observes floating.
    converge(
        &mut layout,
        vec![
            obs(
                "win-m",
                true,
                false,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
            obs(
                "win-n",
                false,
                false,
                true,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ],
    )
    .expect("nonfixed max converges");
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-n".to_owned()],
        "nonfixed max tiles"
    );
}

#[test]
fn born_fullscreen_holds_then_exits_fresh_fixed_floats() {
    let mut layout = fixed_layout();
    // Born fullscreen holds slotless with no writes (D5): plain
    // exception, no tiled slot, no automatic marker yet.
    let result = converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("born FS converges");
    assert_eq!(result.admitted, 0, "born FS takes no tiled slot while held");
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "held slotless"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "no marker while held"
    );
    // First exit is fresh admission with the CURRENT predicate (D5):
    // fixed floats untouched automatic, no tiled slot.
    let result = converge(&mut layout, vec![fixed("win-s")]).expect("FS exit converges");
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "fixed exit floats"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "automatic marker on exit"
    );
    assert_eq!(tiled_ids(&layout), Vec::<String>::new(), "no tile taken");
    let _ = result;
}

#[test]
fn born_fullscreen_nonfixed_exit_tiles() {
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-n",
            false,
            true,
            false,
            false,
            false,
            false,
            WindowSizeHints::none(),
        )],
    )
    .expect("born FS converges");
    assert!(
        layout.is_exception(&key(), &WindowId("win-n".to_owned())),
        "held"
    );
    converge(&mut layout, vec![plain("win-n")]).expect("exit converges");
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-n".to_owned()],
        "nonfixed exit tiles"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key(), &WindowId("win-n".to_owned())),
        "no marker"
    );
}

#[test]
fn born_fullscreen_exit_uses_current_predicate() {
    // One-axis-fixed born fullscreen: exits tiled under both-axes,
    // floats under either-axis (CURRENT predicate, D5).
    let mut both = fixed_layout();
    converge(
        &mut both,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("born holds");
    converge(
        &mut both,
        vec![obs(
            "win-s",
            false,
            false,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("exit");
    assert_eq!(
        tiled_ids(&both),
        vec!["win-s".to_owned()],
        "both-axes exit tiles one-axis"
    );

    let mut either = fixed_layout();
    either.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    converge(
        &mut either,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("born holds");
    converge(
        &mut either,
        vec![obs(
            "win-s",
            false,
            false,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("exit");
    assert!(
        either.is_exception(&key(), &WindowId("win-s".to_owned())),
        "either-axis exit floats one-axis"
    );
    assert!(
        either.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "automatic under either-axis"
    );
}

#[test]
fn born_fullscreen_hint_change_while_held_uses_exit_hints() {
    // Hints changing while still fullscreen must not classify: the
    // first exit decides with its own hints.
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            WindowSizeHints::none(),
        )],
    )
    .expect("born holds hintless");
    // Still fullscreen but now reporting fixed: still held, no marker.
    let result = converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("fullscreen hint churn holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "no classification while fullscreen"
    );
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "held"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "no marker while held"
    );
    // Exit with fixed hints floats.
    converge(&mut layout, vec![fixed("win-s")]).expect("exit");
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "exit floats"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "automatic on exit"
    );
}

#[test]
fn born_fullscreen_predicate_switch_while_held_uses_current() {
    // Predicate switches after birth but before exit: the exit uses
    // the CURRENT predicate, not birth-time state.
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("born holds under both-axes");
    layout.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            false,
            false,
            false,
            false,
            false,
            one_axis_hints(),
        )],
    )
    .expect("exit");
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "switched predicate floats one-axis on exit"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "automatic under current predicate"
    );
}

#[test]
fn repeated_fullscreen_exits_follow_retained_identity() {
    // First exit classifies fresh; later fullscreen cycles keep
    // retained identity instead of re-admitting.
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("born holds");
    converge(&mut layout, vec![fixed("win-s")]).expect("first exit floats");
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())));
    // Second fullscreen cycle: rides as floating with origin, exits
    // retained even though hints are now plain (D2: no re-admission).
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            true,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("second fullscreen rides");
    let result = converge(
        &mut layout,
        vec![obs(
            "win-s",
            true,
            false,
            false,
            false,
            false,
            false,
            WindowSizeHints::none(),
        )],
    )
    .expect("second exit retains");
    assert_eq!(
        (result.removed, result.admitted),
        (0, 0),
        "no fresh admission on later exit"
    );
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "retained"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "marker retained"
    );
    // Tiled windows keep their slot across later fullscreen too,
    // even when fixed hints appear (D2: tiled hint changes never move).
    let mut tiled = fixed_layout();
    admit_plain(&mut tiled, "win-t");
    converge(&mut tiled, vec![plain("win-t"), fixed("win-f")]).expect("admit fixed");
    let id = WindowId("win-t".to_owned());
    converge(
        &mut tiled,
        vec![
            obs(
                "win-t",
                false,
                true,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
            obs(
                "win-f",
                true,
                false,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("tiled fullscreen keeps slot");
    assert_eq!(tiled_ids(&tiled), vec!["win-t".to_owned()]);
    converge(
        &mut tiled,
        vec![
            obs(
                "win-t",
                false,
                false,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
            obs(
                "win-f",
                true,
                false,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("later exit keeps tile despite fixed hints");
    assert_eq!(tiled_ids(&tiled), vec!["win-t".to_owned()], "no re-float");
    assert!(
        !tiled.is_automatic_fixed_float(&key(), &id),
        "no marker inferred"
    );
}

#[test]
fn repeated_fullscreen_hold_takes_no_slot() {
    // Unchanged fullscreen observations never classify: direct
    // fullscreen holds across repeats with no marker, even fixed.
    let mut layout = fixed_layout();
    converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("born holds");
    let result = converge(
        &mut layout,
        vec![obs(
            "win-s",
            false,
            true,
            false,
            false,
            false,
            false,
            fixed_hints(),
        )],
    )
    .expect("repeat holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "repeat fullscreen is converged hold"
    );
    assert!(
        layout.is_exception(&key(), &WindowId("win-s".to_owned())),
        "held"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key(), &WindowId("win-s".to_owned())),
        "still no marker before exit"
    );
    assert!(tiled_ids(&layout).is_empty(), "no slot while held");
}

#[test]
fn optout_exception_retile_with_fullscreen_tiles_legacy() {
    // Without the opt-in there is no fullscreen hold: a floating
    // exception re-observed tiled while fullscreen re-tiles exactly
    // like before.
    let mut layout = WorldLayout::new();
    admit_plain(&mut layout, "win-a");
    float_intentional(&mut layout, "win-a");
    let id = WindowId("win-a".to_owned());
    assert!(layout.is_exception(&key(), &id), "intentional floats");
    converge(
        &mut layout,
        vec![obs(
            "win-a",
            false,
            true,
            false,
            false,
            false,
            false,
            WindowSizeHints::none(),
        )],
    )
    .expect("retile converges");
    assert!(!layout.is_exception(&key(), &id), "legacy retile tiles");
    assert_eq!(tiled_ids(&layout), vec!["win-a".to_owned()]);
}

#[test]
fn optin_exception_retile_with_fullscreen_holds() {
    // Same observation shape with the opt-in on holds instead: the
    // automatic keeps floating with its marker and no tiled slot.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(layout.is_automatic_fixed_float(&key(), &id));
    let result = converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                false,
                true,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("fullscreen holds");
    assert_eq!(
        (result.removed, result.admitted, result.flags_adopted),
        (0, 0, 0),
        "hold converges clean"
    );
    assert!(layout.is_exception(&key(), &id), "still floating");
    assert!(layout.is_automatic_fixed_float(&key(), &id), "marker kept");
    assert_eq!(tiled_ids(&layout), vec!["win-a".to_owned()]);
}

#[test]
fn floating_fullscreen_cycle_restores_floating() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    // A formerly fixed-floating client entering fullscreen keeps floating.
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                true,
                true,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("enter FS");
    assert!(layout.is_exception(&key(), &id), "floating through FS");
    // Exiting restores floating (D5).
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                true,
                false,
                false,
                false,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("exit FS");
    assert!(layout.is_exception(&key(), &id), "restores floating");
    assert!(
        layout.is_automatic_fixed_float(&key(), &id),
        "automatic kept"
    );
}

fn float_intentional(layout: &mut WorldLayout, window: &str) {
    let mut windows = current_observed(layout, &key());
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    layout
        .toggle_float(
            &key(),
            &WindowId(window.to_owned()),
            Some(Rect {
                x: 10,
                y: 10,
                w: 100,
                h: 80,
            }),
            &windows,
        )
        .unwrap_or_else(|e| panic!("float {window}: {e:?}"));
}

#[test]
fn workspace_enable_keeps_automatic_floats_others_tile() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    admit_plain(&mut layout, "win-g");
    converge(
        &mut layout,
        vec![plain("win-a"), plain("win-g"), fixed("win-f")],
    )
    .expect("admit fixed");
    float_intentional(&mut layout, "win-g");
    let auto = WindowId("win-f".to_owned());
    let manual = WindowId("win-g".to_owned());
    assert!(layout.is_automatic_fixed_float(&key(), &auto));
    assert!(
        !layout.is_automatic_fixed_float(&key(), &manual),
        "intentional never automatic"
    );
    // Enabling workspace tiling checks every window (D6): the automatic
    // float stays floating untouched (no override recorded) while the
    // intentional float keeps its existing rule (re-tiles when observed
    // tiled without a fixed win).
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-g",
                true,
                false,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
            fixed("win-f"),
        ],
    )
    .expect("retile");
    assert!(
        layout.is_exception(&key(), &auto),
        "automatic stays floating"
    );
    assert!(
        layout.is_automatic_fixed_float(&key(), &auto),
        "automatic marker kept"
    );
    assert!(
        !layout.has_fixed_tile_override(&key(), &auto),
        "no tile win recorded for automatic"
    );
    assert!(
        layout.is_exception(&key(), &manual),
        "intentional keeps floating"
    );
    assert!(
        !layout.has_fixed_tile_override(&key(), &manual),
        "intentional untouched"
    );
    // An explicit suppress signal still tiles the automatic (D3 user
    // override wins for the same live client).
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-g",
                true,
                false,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
            obs(
                "win-f",
                false,
                false,
                false,
                false,
                false,
                true,
                fixed_hints(),
            ),
        ],
    )
    .expect("explicit retile");
    assert!(
        !layout.is_exception(&key(), &auto),
        "suppressed automatic tiles"
    );
    assert!(
        layout.has_fixed_tile_override(&key(), &auto),
        "explicit win recorded"
    );
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-a".to_owned(), "win-f".to_owned()],
        "retiled joins tree"
    );
}

#[test]
fn suppress_signal_records_override_on_retile() {
    // An intentional float re-tiled with the adapter's suppress signal
    // (sticky-off-to-tile) records the win; without the signal the
    // re-admit stays plain.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    admit_plain(&mut layout, "win-g");
    float_intentional(&mut layout, "win-g");
    let manual = WindowId("win-g".to_owned());
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-g",
                false,
                false,
                false,
                false,
                false,
                true,
                WindowSizeHints::none(),
            ),
        ],
    )
    .expect("retile with signal");
    assert!(!layout.is_exception(&key(), &manual));
    assert!(
        layout.has_fixed_tile_override(&key(), &manual),
        "signal recorded"
    );
}

#[test]
fn sticky_adopts_automatic_as_intentional() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed");
    let id = WindowId("win-f".to_owned());
    assert!(layout.is_automatic_fixed_float(&key(), &id));
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                true,
                false,
                false,
                true,
                false,
                false,
                fixed_hints(),
            ),
        ],
    )
    .expect("sticky on");
    assert!(layout.is_exception(&key(), &id), "stays floating");
    assert!(
        !layout.is_automatic_fixed_float(&key(), &id),
        "sticky is intentional now"
    );
}

#[test]
fn explicit_admit_floats_fixed_as_deferred_without_geometry() {
    // The explicit admit_window route classifies too: no tile,
    // no geometry, focus kept.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    let focus_before = layout.focus_leaf(&key()).cloned();
    let mut windows = current_observed(&layout, &key());
    windows.push(fixed("win-f"));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let outcome = layout
        .admit_window(
            &key(),
            &WindowId("win-f".to_owned()),
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            false,
            false,
            &windows,
        )
        .expect("fixed admit");
    assert!(
        outcome.geometry.is_empty(),
        "membership only, no geometry (D8)"
    );
    assert!(outcome.admitted_leaf.is_none());
    assert_eq!(
        layout.focus_leaf(&key()).cloned(),
        focus_before,
        "focus untouched"
    );
    assert!(layout.is_exception(&key(), &WindowId("win-f".to_owned())));
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
}

#[test]
fn failed_propose_stages_nothing() {
    // A refused fixed admission stages no state and no markers: the
    // committed sets are exactly untouched.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    let mut windows = current_observed(&layout, &key());
    windows.push(fixed("win-f"));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = layout
        .admit_window(
            &key(),
            &WindowId("win-f".to_owned()),
            // Degenerate bounds refuse before anything stages.
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            false,
            false,
            &windows,
        )
        .expect_err("degenerate bounds refuse");
    assert_eq!(error, LayoutFailure::Semantics(RefusalKind::MalformedInput));
    assert!(!layout.is_exception(&key(), &WindowId("win-f".to_owned())));
    assert!(!layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
    assert!(!layout.has_fixed_tile_override(&key(), &WindowId("win-f".to_owned())));
}

// Retired: `divergence_after_propose_discards_staged_markers` tested
// `Session::note_adapter_loss` between `propose` and `verify` on the deleted
// two-phase pending state machine. Candidate atomicity on synchronous refusal
// is covered by `failed_propose_stages_nothing`.

#[test]
fn optout_keeps_original_refusals_and_tiles_fixed() {
    // With the opt-in off, legacy behavior is preserved: fixed
    // hints tile (no classifier), overlay admissions refuse as
    // partial-observation, and unfloat bindings compare exact flags.
    let mut layout = WorldLayout::new();
    admit_plain(&mut layout, "win-a");
    // Fixed hints tile without the opt-in.
    let result = converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("converge");
    assert_eq!(result.admitted, 1, "opt-out tiles fixed");
    assert!(!layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
    assert!(!layout.has_fixed_tile_override(&key(), &WindowId("win-f".to_owned())));
    // Overlay admission refuses exactly as before.
    let mut windows = current_observed(&layout, &key());
    windows.push(obs(
        "win-o",
        false,
        true,
        false,
        false,
        false,
        false,
        fixed_hints(),
    ));
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = layout
        .admit_window(
            &key(),
            &WindowId("win-o".to_owned()),
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            false,
            false,
            &windows,
        )
        .expect_err("overlay admit refuses");
    assert_eq!(
        error,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}

#[test]
fn optout_unfloat_keeps_exact_flag_binding() {
    // Without the opt-in, an unfloat whose observed flags drift from the
    // record (sticky overlay) refuses exactly as before.
    let mut layout = WorldLayout::new();
    admit_plain(&mut layout, "win-a");
    float_intentional(&mut layout, "win-a");
    let mut windows = current_observed(&layout, &key());
    for entry in windows.iter_mut() {
        if entry.window.0 == "win-a" {
            entry.sticky = true;
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let error = layout
        .toggle_float(&key(), &WindowId("win-a".to_owned()), None, &windows)
        .expect_err("drifted flags refuse");
    assert_eq!(
        error,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}

#[test]
fn fixed_size_reason_vocabulary() {
    use crate::size_hints::{fixed_size_reason, is_fixed_size};
    assert!(is_fixed_size(fixed_hints()));
    assert_eq!(fixed_size_reason(fixed_hints()), "fixed-equal");
    assert_eq!(
        fixed_size_reason(WindowSizeHints::none()),
        "not-fixed-missing"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(-1),
            min_h: None,
            max_w: Some(-1),
            max_h: None
        }),
        "not-fixed-negative"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(1),
            min_h: Some(1),
            max_w: Some(i32::MAX),
            max_h: Some(i32::MAX)
        }),
        "not-fixed-sentinel"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(16385),
            min_h: Some(16385),
            max_w: Some(16385),
            max_h: Some(16385)
        }),
        "not-fixed-out-of-range"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(0),
            min_h: Some(0),
            max_w: Some(0),
            max_h: Some(0)
        }),
        "not-fixed-zero"
    );
    assert_eq!(
        fixed_size_reason(WindowSizeHints {
            min_w: Some(1),
            min_h: Some(1),
            max_w: Some(2),
            max_h: Some(2)
        }),
        "not-fixed-unequal"
    );
}

fn carried(
    window: &str,
    floating: bool,
    fullscreen: bool,
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        floating,
        fit_excluded: floating || fullscreen,
        fullscreen,
        maximized: false,
        sticky,
        fixed_auto,
        fixed_suppress,
        hints,
    }
}
fn plain_carried(window: &str) -> EngineWindow {
    carried(
        window,
        false,
        false,
        false,
        false,
        false,
        WindowSizeHints::none(),
    )
}
fn fixed_carried(window: &str) -> EngineWindow {
    carried(window, false, false, false, false, false, fixed_hints())
}

#[test]
fn engine_fresh_fixed_reports_bounded_diagnostic() {
    // Opt-in layout: fresh foreground+hidden-style adoption classifies (D7);
    // `Engine::last_fixed_admission` diagnostic was retired with `Engine`.
    let mut layout = fixed_layout();
    let out = layout
        .reconcile_fresh(
            &domain(),
            0,
            &WindowId("win-a".to_owned()),
            &[plain_carried("win-a"), fixed_carried("win-f")],
        )
        .expect("fresh fixed");
    let ids: Vec<&str> = out
        .projection
        .geometry
        .iter()
        .map(|g| g.window.0.as_str())
        .collect();
    assert!(ids.contains(&"win-a"), "sibling tiles {ids:?}");
    assert!(
        !ids.contains(&"win-f"),
        "fixed takes no geometry (D8) {ids:?}"
    );
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));

    // A suppressed re-observation after domain release tiles without
    // reclassifying: the adapter-retained win crosses domains.
    let mut layout2 = fixed_layout();
    let out2 = layout2
        .reconcile_fresh(
            &domain(),
            0,
            &WindowId("win-f".to_owned()),
            &[carried(
                "win-f",
                false,
                false,
                false,
                false,
                true,
                fixed_hints(),
            )],
        )
        .expect("suppressed must tile");
    let ids2: Vec<&str> = out2
        .projection
        .geometry
        .iter()
        .map(|g| g.window.0.as_str())
        .collect();
    assert!(ids2.contains(&"win-f"), "suppressed tiles {ids2:?}");
    assert!(!layout2.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));

    // Opt-in off keeps exact legacy behavior: fixed tiles.
    let mut plain_layout = WorldLayout::new();
    let out3 = plain_layout
        .reconcile_fresh(
            &domain(),
            0,
            &WindowId("win-a".to_owned()),
            &[plain_carried("win-a"), fixed_carried("win-f")],
        )
        .expect("opt-out must tile");
    let ids3: Vec<&str> = out3
        .projection
        .geometry
        .iter()
        .map(|g| g.window.0.as_str())
        .collect();
    assert!(ids3.contains(&"win-f"), "opt-out tiles fixed {ids3:?}");
    assert!(!plain_layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
}

fn dom_pair(output: &str, workspace: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn obs_homed(
    window: &str,
    floating: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
    output: &str,
    workspace: &str,
) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress,
        hints,
    }
}

fn carried_homed(
    window: &str,
    floating: bool,
    fixed_suppress: bool,
    hints: WindowSizeHints,
    output: &str,
    workspace: &str,
) -> EngineWindow {
    EngineWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        floating,
        fit_excluded: floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress,
        hints,
    }
}

/// Full pair/send/split metadata roundtrip for one suppressed tile win
/// plus one automatic float (D3, actual `run_pair_send` fixture API).
///
/// Source holds win-keep (plain tile), win-auto (automatic fixed float),
/// and win-sup (fixed, explicitly untiled with a retained override);
/// target holds win-t. An automatic send runs as `PairSendKind::Deferred`
/// with no mover allocation; a suppressed send moves the mover tiled onto
/// the target with its override, while the automatic sibling never moves.
#[test]
fn pair_send_split_preserves_automatic_and_override() {
    let dom_a = dom_pair("out-1", "ws-a");
    let dom_b = dom_pair("out-1", "ws-b");
    let key_a = dom_a.key();
    let key_b = dom_b.key();
    let mut layout = fixed_layout();
    converge_at(
        &mut layout,
        &dom_a,
        vec![obs_homed(
            "win-keep",
            false,
            false,
            WindowSizeHints::none(),
            "out-1",
            "ws-a",
        )],
    )
    .expect("admit win-keep");
    // Both fixed windows admit as automatic floats in one convergence.
    let result = converge_at(
        &mut layout,
        &dom_a,
        vec![
            obs_homed(
                "win-keep",
                false,
                false,
                WindowSizeHints::none(),
                "out-1",
                "ws-a",
            ),
            obs_homed("win-auto", false, false, fixed_hints(), "out-1", "ws-a"),
            obs_homed("win-sup", false, false, fixed_hints(), "out-1", "ws-a"),
        ],
    )
    .expect("converge fixed pair");
    assert_eq!(result.flags_adopted, 2, "both fixed float");
    let auto = WindowId("win-auto".to_owned());
    let sup = WindowId("win-sup".to_owned());
    assert!(layout.is_automatic_fixed_float(&key_a, &auto));
    assert!(layout.is_automatic_fixed_float(&key_a, &sup));
    // Explicit unfloat records the suppress override for win-sup only.
    layout
        .toggle_float(
            &key_a,
            &sup,
            None,
            &[
                obs_homed(
                    "win-keep",
                    false,
                    false,
                    WindowSizeHints::none(),
                    "out-1",
                    "ws-a",
                ),
                obs_homed(
                    "win-auto",
                    true,
                    false,
                    WindowSizeHints::none(),
                    "out-1",
                    "ws-a",
                ),
                obs_homed(
                    "win-sup",
                    true,
                    false,
                    WindowSizeHints::none(),
                    "out-1",
                    "ws-a",
                ),
            ],
        )
        .expect("unfloat win-sup");
    assert!(
        layout.has_fixed_tile_override(&key_a, &sup),
        "override recorded"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key_a, &sup),
        "automatic cleared"
    );
    assert!(
        layout.is_automatic_fixed_float(&key_a, &auto),
        "sibling stays automatic"
    );

    converge_at(
        &mut layout,
        &dom_b,
        vec![obs_homed(
            "win-t",
            false,
            false,
            WindowSizeHints::none(),
            "out-1",
            "ws-b",
        )],
    )
    .expect("admit win-t");

    let pair_observed = || {
        vec![
            carried_homed(
                "win-keep",
                false,
                false,
                WindowSizeHints::none(),
                "out-1",
                "ws-a",
            ),
            carried_homed("win-sup", false, true, fixed_hints(), "out-1", "ws-a"),
            carried_homed(
                "win-auto",
                true,
                false,
                WindowSizeHints::none(),
                "out-1",
                "ws-a",
            ),
            carried_homed(
                "win-t",
                false,
                false,
                WindowSizeHints::none(),
                "out-1",
                "ws-b",
            ),
        ]
    };

    // An automatic float is a send mover: the transfer plans slotless
    // Deferred with no mover allocation.
    let mut auto_layout = layout.clone();
    let observed_auto = pair_observed();
    let ctx_auto = PairOperationContext {
        source_domain: &dom_a,
        target_domain: &dom_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &auto,
        observed: &observed_auto,
        closed: &[],
    };
    let out_auto = auto_layout
        .run_pair_send(&ctx_auto, &auto, true, false)
        .expect("automatic send");
    assert_eq!(out_auto.kind, PairSendKind::Deferred);
    assert_eq!(out_auto.window, auto);
    assert!(
        out_auto.geometry.iter().all(|g| g.window != auto),
        "slotless mover takes no allocation"
    );

    // The suppressed tile win sends: the mover lands tiled on the target
    // with its override, while the automatic sibling never moves.
    let observed_sup = pair_observed();
    let ctx_sup = PairOperationContext {
        source_domain: &dom_a,
        target_domain: &dom_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &sup,
        observed: &observed_sup,
        closed: &[],
    };
    let out_sup = layout
        .run_pair_send(&ctx_sup, &sup, true, false)
        .expect("suppressed send");
    assert_eq!(out_sup.kind, PairSendKind::Tiled);
    assert!(
        out_sup
            .geometry
            .iter()
            .any(|g| g.window == sup && g.workspace.0 == "ws-b"),
        "mover desired on target"
    );
    assert!(
        layout.is_automatic_fixed_float(&key_a, &auto),
        "automatic never moves"
    );
    assert!(
        !layout.has_fixed_tile_override(&key_a, &sup),
        "override travels with the mover, never duplicates"
    );
    assert!(
        layout.has_fixed_tile_override(&key_b, &sup),
        "target keeps the tile win"
    );
    assert!(
        !layout.is_automatic_fixed_float(&key_b, &sup),
        "moved tile is not automatic"
    );
    assert!(
        layout.is_tiled_window(&key_b, &sup),
        "mover homed on target"
    );
    assert!(
        layout.is_tiled_window(&key_b, &WindowId("win-t".to_owned())),
        "target survivor undisturbed"
    );
}

fn one_axis_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(100),
        max_w: Some(640),
        max_h: Some(480),
    }
}

#[test]
fn predicate_defaults_to_both_axes_matching_delivered_behavior() {
    use crate::size_hints::{FixedSizePredicate, fixed_size_reason_with, is_fixed_size_with};
    assert_eq!(FixedSizePredicate::default(), FixedSizePredicate::BothAxes);
    assert_eq!(
        FixedSizePredicate::BothAxes.as_wire_str(),
        "both-axes-fixed"
    );
    assert_eq!(
        FixedSizePredicate::EitherAxis.as_wire_str(),
        "either-axis-fixed"
    );
    assert_eq!(
        FixedSizePredicate::parse_wire("both-axes-fixed"),
        Some(FixedSizePredicate::BothAxes)
    );
    assert_eq!(
        FixedSizePredicate::parse_wire("either-axis-fixed"),
        Some(FixedSizePredicate::EitherAxis)
    );
    for invalid in ["", "both", "either", "fixed", "both_axes_fixed", "COSMIC"] {
        assert_eq!(FixedSizePredicate::parse_wire(invalid), None, "{invalid:?}");
    }
    // One-axis fixture: tiles under the default, floats under either-axis.
    assert!(!is_fixed_size_with(
        one_axis_hints(),
        FixedSizePredicate::BothAxes
    ));
    assert!(is_fixed_size_with(
        one_axis_hints(),
        FixedSizePredicate::EitherAxis
    ));
    assert_eq!(
        fixed_size_reason_with(one_axis_hints(), FixedSizePredicate::BothAxes),
        "not-fixed-unequal"
    );
    assert_eq!(
        fixed_size_reason_with(one_axis_hints(), FixedSizePredicate::EitherAxis),
        "fixed-equal"
    );
    // Guards preserved under either-axis: missing/sentinel still tile.
    let missing = WindowSizeHints {
        min_w: Some(640),
        min_h: None,
        max_w: Some(640),
        max_h: Some(480),
    };
    assert!(!is_fixed_size_with(missing, FixedSizePredicate::EitherAxis));
    let sentinel = WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(i32::MAX),
    };
    assert!(!is_fixed_size_with(
        sentinel,
        FixedSizePredicate::EitherAxis
    ));
    // WorldLayout default matches BothAxes.
    let layout = WorldLayout::new();
    assert_eq!(layout.fixed_size_predicate(), FixedSizePredicate::BothAxes);
}

#[test]
fn either_axis_admits_one_axis_fixed_while_both_axes_tiles() {
    let mut both = fixed_layout();
    admit_plain(&mut both, "win-a");
    let result = converge(
        &mut both,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                false,
                false,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            ),
        ],
    )
    .expect("converge");
    assert_eq!(result.flags_adopted, 0, "one-axis tiles under both-axes");
    assert!(!both.is_exception(&key(), &WindowId("win-f".to_owned())));
    assert_eq!(
        tiled_ids(&both),
        vec!["win-a".to_owned(), "win-f".to_owned()]
    );

    let mut either = fixed_layout();
    either.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    admit_plain(&mut either, "win-a");
    let result = converge(
        &mut either,
        vec![
            plain("win-a"),
            obs(
                "win-f",
                false,
                false,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            ),
        ],
    )
    .expect("converge");
    assert_eq!(result.flags_adopted, 1, "one-axis floats under either-axis");
    assert!(either.is_exception(&key(), &WindowId("win-f".to_owned())));
    assert!(either.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
    assert_eq!(tiled_ids(&either), vec!["win-a".to_owned()]);
}

#[test]
fn predicate_switch_applies_to_subsequent_admissions_without_reclassifying() {
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    // win-b admitted tiled under the both-axes default (one-axis hints tile).
    converge(
        &mut layout,
        vec![
            plain("win-a"),
            obs(
                "win-b",
                false,
                false,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            ),
        ],
    )
    .expect("converge");
    assert!(!layout.is_exception(&key(), &WindowId("win-b".to_owned())));
    // Switch to either-axis: retained win-b stays tiled (no reclassification).
    layout.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    converge(&mut layout, vec![plain("win-a"), plain("win-b")]).expect("converge");
    assert!(
        !layout.is_exception(&key(), &WindowId("win-b".to_owned())),
        "switch must not reclassify retained windows"
    );
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-a".to_owned(), "win-b".to_owned()]
    );
    // Subsequent one-axis admission floats under the new predicate.
    let result = converge(
        &mut layout,
        vec![
            plain("win-a"),
            plain("win-b"),
            obs(
                "win-c",
                false,
                false,
                false,
                false,
                false,
                false,
                one_axis_hints(),
            ),
        ],
    )
    .expect("converge");
    assert_eq!(result.flags_adopted, 1);
    assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-c".to_owned())));
}

#[test]
fn live_tile_override_wins_under_either_axis_predicate() {
    let mut layout = fixed_layout();
    layout.set_fixed_size_predicate(crate::size_hints::FixedSizePredicate::EitherAxis);
    admit_plain(&mut layout, "win-a");
    // Explicit user tile (suppress origin) wins for the live client.
    admit_with(&mut layout, "win-f", true, one_axis_hints());
    assert!(
        !layout.is_exception(&key(), &WindowId("win-f".to_owned())),
        "override wins over either-axis admission"
    );
    assert!(layout.has_fixed_tile_override(&key(), &WindowId("win-f".to_owned())));
    assert_eq!(
        tiled_ids(&layout),
        vec!["win-a".to_owned(), "win-f".to_owned()]
    );
    // Later re-observation keeps it tiled (no reclassification).
    converge(&mut layout, vec![plain("win-a"), plain("win-f")]).expect("converge");
    assert!(!layout.is_exception(&key(), &WindowId("win-f".to_owned())));
}

// B9 maximized intentional unfloat (M09=A): shared-core regression evidence.
// No production change: a floating window still observed maximized refuses
// the unfloat (pre-clear state), while the same float observed unmaximized
// (post-clear state) fresh-admits as a new tiled window with the D3 suppress
// pin, so fixed clients never re-auto-float. Both the ordinary and the
// fixed-size legs are covered.
#[test]
fn b9_maximized_float_unfloat_refuses_until_clear_then_fresh_admits() {
    // Ordinary non-fixed leg.
    let mut layout = fixed_layout();
    admit_plain(&mut layout, "win-a");
    admit_plain(&mut layout, "win-b");
    float_intentional(&mut layout, "win-b");
    // Pre-clear: floating + maximized refuses.
    {
        let windows = vec![
            plain("win-a"),
            obs(
                "win-b",
                true,
                false,
                true,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ];
        let err = layout
            .toggle_float(
                &key(),
                &WindowId("win-b".to_owned()),
                Some(Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                }),
                &windows,
            )
            .unwrap_err();
        assert_eq!(
            err,
            LayoutFailure::Semantics(RefusalKind::PartialObservation),
            "maximized float must refuse before the native clear"
        );
    }
    // Post-clear: floating + unmaximized fresh-admits with the D3 pin.
    {
        let windows = vec![
            plain("win-a"),
            obs(
                "win-b",
                true,
                false,
                false,
                false,
                false,
                false,
                WindowSizeHints::none(),
            ),
        ];
        layout
            .toggle_float(
                &key(),
                &WindowId("win-b".to_owned()),
                Some(Rect {
                    x: 100,
                    y: 100,
                    w: 400,
                    h: 300,
                }),
                &windows,
            )
            .unwrap_or_else(|e| panic!("cleared unfloat admits: {e:?}"));
        assert_eq!(
            tiled_ids(&layout),
            vec!["win-a".to_owned(), "win-b".to_owned()],
            "cleared float rejoins the tiled topology"
        );
        assert!(
            layout.has_fixed_tile_override(&key(), &WindowId("win-b".to_owned())),
            "explicit unfloat records the user tile win"
        );
    }

    // Fixed-size leg: automatic float, maximized, then cleared.
    let mut fixed_state = fixed_layout();
    admit_plain(&mut fixed_state, "win-a");
    converge(&mut fixed_state, vec![plain("win-a"), fixed("win-f")])
        .expect("admit fixed automatic");
    assert!(fixed_state.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
    // Pre-clear refuses without staging.
    {
        let mut windows = current_observed(&fixed_state, &key());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
                entry.maximized = true;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        let err = fixed_state
            .toggle_float(
                &key(),
                &WindowId("win-f".to_owned()),
                Some(Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                }),
                &windows,
            )
            .unwrap_err();
        assert_eq!(
            err,
            LayoutFailure::Semantics(RefusalKind::PartialObservation),
            "maximized automatic float must refuse before the native clear"
        );
    }
    // Post-clear admits with the suppress pin and never re-auto-floats.
    {
        let mut windows = current_observed(&fixed_state, &key());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
                entry.maximized = false;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        fixed_state
            .toggle_float(
                &key(),
                &WindowId("win-f".to_owned()),
                Some(Rect {
                    x: 100,
                    y: 100,
                    w: 400,
                    h: 300,
                }),
                &windows,
            )
            .unwrap_or_else(|e| panic!("cleared fixed unfloat admits: {e:?}"));
        let id = WindowId("win-f".to_owned());
        assert!(
            fixed_state.has_fixed_tile_override(&key(), &id),
            "explicit unfloat wins over fixed re-float"
        );
        assert!(
            !fixed_state.is_automatic_fixed_float(&key(), &id),
            "automatic identity withdrawn"
        );
        assert_eq!(
            tiled_ids(&fixed_state),
            vec!["win-a".to_owned(), "win-f".to_owned()],
            "fixed client tiles after the clear"
        );
    }
}

// Retained-float unfloat exact-refusal shapes: every reachable refusal
// on the explicit-tile route after convergence, each row mutating
// exactly one fact so the earliest guard is proved.
#[test]
fn retained_float_unfloat_exact_refusals() {
    fn setup() -> WorldLayout {
        let mut layout = fixed_layout();
        admit_plain(&mut layout, "win-a");
        converge(&mut layout, vec![plain("win-a"), fixed("win-f")]).expect("admit fixed automatic");
        assert!(layout.is_automatic_fixed_float(&key(), &WindowId("win-f".to_owned())));
        layout
    }
    fn complete(layout: &WorldLayout) -> Vec<ObservedWindow> {
        let mut windows = current_observed(layout, &key());
        for entry in windows.iter_mut() {
            if entry.window.0 == "win-f" {
                entry.floating = true;
            }
        }
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        windows
    }
    #[derive(Clone, Copy)]
    enum RefusalCase {
        MissingTarget,
        StaleFloating,
        StaleHoming,
        MaximizedTarget,
        MissingPeer,
        ExtraPeer,
        StalePeerFlags,
        StalePeerHoming,
        StaleSubjectSticky,
    }
    impl RefusalCase {
        fn site(self) -> &'static str {
            match self {
                Self::MissingTarget => "toggle_float missing-target",
                Self::StaleFloating => "toggle_float stale-floating-binding",
                Self::StaleHoming => "toggle_float stale-homing-binding",
                Self::MaximizedTarget => "toggle_float maximized-target",
                Self::MissingPeer => "admit completeness missing-peer",
                Self::ExtraPeer => "admit completeness extra-peer",
                Self::StalePeerFlags => "admit known-match stale-peer-flags",
                Self::StalePeerHoming => "admit known-match stale-peer-homing",
                Self::StaleSubjectSticky => "admit subject-binding stale-subject-sticky",
            }
        }
        fn expected_refusal(self) -> RefusalKind {
            match self {
                // Single-domain `toggle_float` checks foreign homing before
                // target/peer binding guards.
                Self::StaleHoming | Self::StalePeerHoming => RefusalKind::CrossDomainMismatch,
                _ => RefusalKind::PartialObservation,
            }
        }
        fn mutate(self, windows: &mut Vec<ObservedWindow>) {
            match self {
                Self::MissingTarget => windows.retain(|e| e.window.0 != "win-f"),
                Self::MissingPeer => windows.retain(|e| e.window.0 != "win-a"),
                Self::ExtraPeer => windows.push(plain("win-x")),
                Self::StaleFloating => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.floating = false;
                        }
                    }
                }
                Self::StaleHoming => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.workspace = WorkspaceId("ws-2".to_owned());
                        }
                    }
                }
                Self::MaximizedTarget => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.maximized = true;
                        }
                    }
                }
                Self::StalePeerFlags => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-a" {
                            e.floating = true;
                        }
                    }
                }
                Self::StalePeerHoming => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-a" {
                            e.workspace = WorkspaceId("ws-2".to_owned());
                        }
                    }
                }
                Self::StaleSubjectSticky => {
                    for e in windows.iter_mut() {
                        if e.window.0 == "win-f" {
                            e.sticky = true;
                        }
                    }
                }
            }
        }
    }
    // Each case names its earliest guard; the mutation proves it by
    // changing exactly one fact of the complete observation.
    let rows = [
        RefusalCase::MissingTarget,
        RefusalCase::StaleFloating,
        RefusalCase::StaleHoming,
        RefusalCase::MaximizedTarget,
        RefusalCase::MissingPeer,
        RefusalCase::ExtraPeer,
        RefusalCase::StalePeerFlags,
        RefusalCase::StalePeerHoming,
        RefusalCase::StaleSubjectSticky,
    ];
    for case in rows {
        let site = case.site();
        let mut layout = setup();
        let mut windows = complete(&layout);
        case.mutate(&mut windows);
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        let err = layout
            .toggle_float(&key(), &WindowId("win-f".to_owned()), None, &windows)
            .unwrap_err();
        assert_eq!(
            err,
            LayoutFailure::Semantics(case.expected_refusal()),
            "{site} refuses exact"
        );
        let id = WindowId("win-f".to_owned());
        assert!(layout.is_exception(&key(), &id), "{site} keeps float");
        assert!(
            layout.is_automatic_fixed_float(&key(), &id),
            "{site} keeps automatic marker"
        );
        assert!(
            !layout.has_fixed_tile_override(&key(), &id),
            "{site} records no override"
        );
        assert_eq!(
            tiled_ids(&layout),
            vec!["win-a".to_owned()],
            "{site} mutates no topology"
        );
        // Restored complete facts accept in the same layout: the refusal
        // staged nothing, so the genuine toggle still succeeds.
        layout
            .toggle_float(
                &key(),
                &WindowId("win-f".to_owned()),
                None,
                &complete(&layout),
            )
            .unwrap_or_else(|e| panic!("{site} restored facts accept: {e:?}"));
    }
    // Corrected full facts accept, directly and after two converging
    // re-observations.
    for (tag, resyncs) in [("direct", 0), ("resync", 2)] {
        let mut layout = setup();
        for _ in 0..resyncs {
            let windows = complete(&layout);
            converge(&mut layout, windows).expect("resync converges");
        }
        layout
            .toggle_float(
                &key(),
                &WindowId("win-f".to_owned()),
                None,
                &complete(&layout),
            )
            .unwrap_or_else(|e| panic!("{tag} complete facts accept: {e:?}"));
    }
}
