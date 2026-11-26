//! Pure arrangement proofs across the restart: float slots compare by exact
//! window, tile rectangles compare as multisets, and the M4 overlay/base
//! shapes must be distinct. No live effect.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::fixture_frame::FramePoint;

use super::frame::{WindowFrames, rect_multiset};

/// Pre-stop arrangement split by role: floats compare by exact window, tiles
/// compare as an unordered rectangle multiset.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ArrangementEvidence {
    pub floats: Vec<WindowFrames>,
    pub tiles: Vec<WindowFrames>,
    pub tile_reflowed: bool,
}

/// Split one settled frame set into the first `float_count` float windows and
/// the remaining tile windows. Pure; unit-tested.
pub(crate) fn partition_arrangement(
    frames: &[WindowFrames],
    float_count: usize,
) -> ArrangementEvidence {
    let floats: Vec<WindowFrames> = frames.iter().take(float_count).cloned().collect();
    let tiles: Vec<WindowFrames> = frames.iter().skip(float_count).cloned().collect();
    ArrangementEvidence {
        floats,
        tiles,
        tile_reflowed: false,
    }
}

fn frames_by_caller(frames: &[WindowFrames]) -> BTreeMap<&str, &WindowFrames> {
    frames
        .iter()
        .map(|frame| (frame.caller.as_str(), frame))
        .collect()
}

fn require_exact_frame(
    label: &str,
    before: &WindowFrames,
    after: &WindowFrames,
) -> Result<(), String> {
    if !after.cg_onscreen {
        return Err(format!(
            "restart {label}: window {} ({}) is not CG-onscreen after restart; refusing",
            after.caller, after.wid
        ));
    }
    if before.ax != after.ax {
        return Err(format!(
            "restart {label}: window {} AX frame changed (before {:?}, after {:?}); refusing",
            after.caller, before.ax, after.ax
        ));
    }
    if before.cg != after.cg {
        return Err(format!(
            "restart {label}: window {} CG bounds changed (before {:?}, after {:?}); refusing",
            after.caller, before.cg, after.cg
        ));
    }
    Ok(())
}

/// M3 verdict: every float must exist after restart with the same caller and
/// exact AX plus CG frame; the tile window set must be unchanged and the
/// unordered AX-rectangle and CG-rectangle multisets must match exactly,
/// including duplicate counts. Slots may swap between tiles. Pure;
/// unit-tested.
pub(crate) fn compare_restart_arrangement(
    before: &ArrangementEvidence,
    after_all: &[WindowFrames],
) -> Result<(), String> {
    let after = frames_by_caller(after_all);
    let mut after_tiles = Vec::new();
    for float in &before.floats {
        let Some(observed) = after.get(float.caller.as_str()) else {
            return Err(format!(
                "restart m3: float window {} is absent after restart; refusing",
                float.caller
            ));
        };
        require_exact_frame("m3", float, observed)?;
    }
    for tile in &before.tiles {
        let Some(observed) = after.get(tile.caller.as_str()) else {
            return Err(format!(
                "restart m3: tile window {} is absent after restart; refusing",
                tile.caller
            ));
        };
        if !observed.cg_onscreen {
            return Err(format!(
                "restart m3: tile window {} is not CG-onscreen after restart; refusing",
                observed.caller
            ));
        }
        after_tiles.push((*observed).clone());
    }
    let mut before_callers: Vec<&str> = before
        .floats
        .iter()
        .chain(before.tiles.iter())
        .map(|frame| frame.caller.as_str())
        .collect();
    let mut after_callers: Vec<&str> = after_all
        .iter()
        .map(|frame| frame.caller.as_str())
        .collect();
    before_callers.sort_unstable();
    after_callers.sort_unstable();
    if before_callers != after_callers {
        return Err(format!(
            "restart m3: window set changed (before {before_callers:?}, after {after_callers:?}); refusing"
        ));
    }
    let before_ax = rect_multiset(before.tiles.iter().map(|frame| frame.ax));
    let after_ax = rect_multiset(after_tiles.iter().map(|frame| frame.ax));
    if before_ax != after_ax {
        return Err(
            "restart m3: tile AX rectangle multiset changed across restart; refusing".to_owned(),
        );
    }
    let before_cg = rect_multiset(before.tiles.iter().map(|frame| frame.cg));
    let after_cg = rect_multiset(after_tiles.iter().map(|frame| frame.cg));
    if before_cg != after_cg {
        return Err(
            "restart m3: tile CG rectangle multiset changed across restart; refusing".to_owned(),
        );
    }
    Ok(())
}

/// Require at least one tile window to have reflowed from the initial tile
/// placement after the float toggles: the leg exercised an active tiled
/// arrangement, not two coincidental float windows.
pub(crate) fn require_tile_reflow(
    initial: &[WindowFrames],
    tiles: &[WindowFrames],
) -> Result<bool, String> {
    let initial = frames_by_caller(initial);
    let mut reflowed = false;
    for tile in tiles {
        let Some(before) = initial.get(tile.caller.as_str()) else {
            return Err(format!(
                "restart m3: tile window {} was absent before the float chords; refusing",
                tile.caller
            ));
        };
        if before.ax != tile.ax || before.cg != tile.cg {
            reflowed = true;
        }
    }
    if !reflowed {
        return Err(
            "restart m3: no tile window reflowed after the float chords (tiled arrangement unproven); refusing"
                .to_owned(),
        );
    }
    Ok(true)
}

/// Reject a duplicate caller or window ID in one independent capture: two
/// entries for one window are not a window set, so multiset counts could be
/// satisfied by a phantom window.
fn require_unique_windows<'a>(
    label: &str,
    frames: impl IntoIterator<Item = &'a WindowFrames>,
) -> Result<(), String> {
    let mut callers = BTreeSet::new();
    let mut wids = BTreeSet::new();
    for frame in frames {
        if !callers.insert(frame.caller.as_str()) {
            return Err(format!(
                "restart {label}: caller {} appears twice in one capture; refusing",
                frame.caller
            ));
        }
        if !wids.insert(frame.wid) {
            return Err(format!(
                "restart {label}: window id {} appears twice in one capture; refusing",
                frame.wid
            ));
        }
    }
    Ok(())
}

/// Require the identical joined caller+window-ID set across two captures,
/// order-free and duplicate-free.
fn require_same_windows_and_wids(
    label: &str,
    before: &[WindowFrames],
    after: &[WindowFrames],
) -> Result<(), String> {
    require_unique_windows(label, before)?;
    require_unique_windows(label, after)?;
    let mut before_set: Vec<(&str, u32)> = before
        .iter()
        .map(|frame| (frame.caller.as_str(), frame.wid))
        .collect();
    let mut after_set: Vec<(&str, u32)> = after
        .iter()
        .map(|frame| (frame.caller.as_str(), frame.wid))
        .collect();
    before_set.sort_unstable();
    after_set.sort_unstable();
    if before_set != after_set {
        return Err(format!(
            "restart {label}: joined caller+window-ID set changed (before {before_set:?}, after {after_set:?}); refusing"
        ));
    }
    Ok(())
}

/// Require one capture to reproduce the exact per-window frames of another:
/// identical caller+WID set and, for every window, identical AX frame,
/// identical CG bounds and the CG onscreen fact. Slots are not inferred from
/// order; each window is compared by caller. Pure; unit-tested.
fn require_exact_per_window<'a>(
    label: &str,
    expected: impl IntoIterator<Item = &'a WindowFrames>,
    observed: &[WindowFrames],
) -> Result<(), String> {
    let expected: Vec<&WindowFrames> = expected.into_iter().collect();
    require_unique_windows(label, expected.iter().copied())?;
    require_unique_windows(label, observed)?;
    let mut expected_set: Vec<(&str, u32)> = expected
        .iter()
        .map(|frame| (frame.caller.as_str(), frame.wid))
        .collect();
    let mut observed_set: Vec<(&str, u32)> = observed
        .iter()
        .map(|frame| (frame.caller.as_str(), frame.wid))
        .collect();
    expected_set.sort_unstable();
    observed_set.sort_unstable();
    if expected_set != observed_set {
        return Err(format!(
            "restart {label}: joined caller+window-ID set changed (before {expected_set:?}, after {observed_set:?}); refusing"
        ));
    }
    for frame in expected {
        let Some(current) = observed
            .iter()
            .find(|candidate| candidate.caller == frame.caller)
        else {
            return Err(format!(
                "restart {label}: window {} is absent from the independent capture; refusing",
                frame.caller
            ));
        };
        if !current.cg_onscreen {
            return Err(format!(
                "restart {label}: window {} is not CG-onscreen; refusing",
                current.caller
            ));
        }
        if current.ax != frame.ax {
            return Err(format!(
                "restart {label}: window {} AX frame changed (before {:?}, after {:?}); refusing",
                current.caller, frame.ax, current.ax
            ));
        }
        if current.cg != frame.cg {
            return Err(format!(
                "restart {label}: window {} CG bounds changed (before {:?}, after {:?}); refusing",
                current.caller, frame.cg, current.cg
            ));
        }
    }
    Ok(())
}

/// The exact pre-stop arrangement must still be in place in one independent
/// post-stop capture with no product alive: same caller+WID set and, per
/// window, identical AX and CG frames plus CG onscreen. Any drift (including
/// the nudge target or a peer) refuses before the effect. Pure; unit-tested.
pub(crate) fn require_post_stop_arrangement(
    label: &str,
    expected: &ArrangementEvidence,
    observed: &[WindowFrames],
) -> Result<(), String> {
    require_exact_per_window(
        label,
        expected.floats.iter().chain(expected.tiles.iter()),
        observed,
    )
}

/// Require the exact pre-stop per-window overlay to still be in place in one
/// independent between-product capture. Early restoration to the base or any
/// per-window drift refuses: M4's overlay is only proven live if it survives
/// the first product stop untouched. Pure; unit-tested.
pub(crate) fn compare_post_stop_overlay(
    overlay: &[WindowFrames],
    observed: &[WindowFrames],
) -> Result<(), String> {
    require_exact_per_window("m4-post-stop", overlay, observed)
}

/// Require the requested post-stop nudge frame to differ from every pre-stop
/// tile in BOTH independent reads. A requested frame that reproduces a tile's
/// AX or CG rectangle would make the effect indistinct from the settled
/// arrangement (a slot-swap no-op). Pure; unit-tested.
pub(crate) fn require_nudge_preimage_distinct(
    requested: &FramePoint,
    tiles: &[WindowFrames],
) -> Result<(), String> {
    for tile in tiles {
        if tile.ax == *requested {
            return Err(format!(
                "restart effect: the requested nudge frame equals the pre-stop AX frame of {}; refusing",
                tile.caller
            ));
        }
        if tile.cg == *requested {
            return Err(format!(
                "restart effect: the requested nudge frame equals the pre-stop CG frame of {}; refusing",
                tile.caller
            ));
        }
    }
    Ok(())
}

/// Raw native `AXZoomed` fact for one caller, read off the same joined AX
/// element as its capture. `None` when the attribute was unreadable, never a
/// guessed value.
pub(crate) fn target_native_zoom(frames: &[WindowFrames], target: &str) -> Option<bool> {
    frames
        .iter()
        .find(|frame| frame.caller == target)
        .and_then(|frame| frame.zoomed)
}

/// M4 native-zoom verdict: the composed maximize must never be native zoom.
/// `Some(true)` at any phase refuses; `Some(false)` and an unreadable (`None`)
/// attribute are reported as raw evidence, never promoted to success.
/// Pure; unit-tested.
pub(crate) fn require_no_native_zoom(phase: &str, zoomed: Option<bool>) -> Result<(), String> {
    if zoomed == Some(true) {
        return Err(format!(
            "restart m4: native AXZoomed:true observed at {phase} (the composed overlay must not use native zoom); refusing"
        ));
    }
    Ok(())
}

/// M4 verdict: the pre-stop overlay must be a real, distinct arrangement, and
/// the settled base must be restored after restart as the same exact AX and
/// CG rectangle multisets over the identical joined caller+WID set (slots may
/// swap; counts count). The overlay target must not still carry its overlay
/// frame in either independent read. Any missing/duplicated/drifted window
/// or surviving overlay refuses. Pure; unit-tested.
pub(crate) fn compare_restart_base(
    base: &[WindowFrames],
    overlay: &[WindowFrames],
    after: &[WindowFrames],
    target: &str,
) -> Result<(), String> {
    require_distinct_overlay(base, overlay, target)?;
    require_same_windows_and_wids("m4", base, after)?;
    for frame in after {
        if !frame.cg_onscreen {
            return Err(format!(
                "restart m4: window {} is not CG-onscreen after restart; refusing",
                frame.caller
            ));
        }
    }
    let base_ax = rect_multiset(base.iter().map(|frame| frame.ax));
    let after_ax = rect_multiset(after.iter().map(|frame| frame.ax));
    if base_ax != after_ax {
        return Err(
            "restart m4: the AX rectangle multiset after restart is not the settled base; refusing"
                .to_owned(),
        );
    }
    let base_cg = rect_multiset(base.iter().map(|frame| frame.cg));
    let after_cg = rect_multiset(after.iter().map(|frame| frame.cg));
    if base_cg != after_cg {
        return Err(
            "restart m4: the CG rectangle multiset after restart is not the settled base; refusing"
                .to_owned(),
        );
    }
    let Some(overlay_target) = overlay.iter().find(|frame| frame.caller == target) else {
        return Err(format!(
            "restart m4: target window {target} is absent from the pre-stop overlay; refusing"
        ));
    };
    let Some(after_target) = after.iter().find(|frame| frame.caller == target) else {
        return Err(format!(
            "restart m4: target window {target} is absent after restart; refusing"
        ));
    };
    if after_target.ax == overlay_target.ax || after_target.cg == overlay_target.cg {
        return Err(format!(
            "restart m4: target window {target} still carries its pre-stop overlay frame after restart (AX or CG not restored); refusing"
        ));
    }
    Ok(())
}

/// Require the pre-stop composed overlay to differ from the settled base as a
/// whole in both independent reads, and to move the target window in both
/// reads, over the identical joined caller+WID set. A slot-swapped no-op
/// leaves the base multisets unchanged and refuses. This is the M4 proof that
/// a live overlay existed before the first stop. Pure; unit-tested.
pub(crate) fn require_distinct_overlay(
    base: &[WindowFrames],
    overlay: &[WindowFrames],
    target: &str,
) -> Result<(), String> {
    require_same_windows_and_wids("m4", base, overlay)?;
    let base_target = base.iter().find(|frame| frame.caller == target);
    let overlay_target = overlay.iter().find(|frame| frame.caller == target);
    let (Some(base_target), Some(overlay_target)) = (base_target, overlay_target) else {
        return Err(format!(
            "restart m4: target window {target} is absent from the settled base or the pre-stop overlay; refusing"
        ));
    };
    if base_target.ax == overlay_target.ax || base_target.cg == overlay_target.cg {
        return Err(format!(
            "restart m4: target window {target} has no distinct composed overlay before stop (AX and CG must both move); refusing"
        ));
    }
    let base_ax = rect_multiset(base.iter().map(|frame| frame.ax));
    let overlay_ax = rect_multiset(overlay.iter().map(|frame| frame.ax));
    if base_ax == overlay_ax {
        return Err(
            "restart m4: the overlay AX rectangle multiset equals the settled base (a slot swap is not a live overlay); refusing"
                .to_owned(),
        );
    }
    let base_cg = rect_multiset(base.iter().map(|frame| frame.cg));
    let overlay_cg = rect_multiset(overlay.iter().map(|frame| frame.cg));
    if base_cg == overlay_cg {
        return Err(
            "restart m4: the overlay CG rectangle multiset equals the settled base (a slot swap is not a live overlay); refusing"
                .to_owned(),
        );
    }
    Ok(())
}
