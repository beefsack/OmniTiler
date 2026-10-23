#![forbid(unsafe_code)]
use super::*;

/// Observed rectangle grounding: the device of the monitor containing the
/// rectangle centre (half-open containment); otherwise the device with the
/// largest overlap area, first in enumeration order on ties (the all-zero
/// case included, so the first topology entry wins there); `"display-?"`
/// with no monitors. Core-owned
/// ([`tiler_core::output_selection::output_for_rect`]); this only maps the
/// core output id back to the session device key. Rectangles decide
/// placement, device names decide session identity.
pub(super) fn output_for_rect(areas: &[MonitorArea], rect: &Rect) -> String {
    use tiler_core::directional::OutputId;
    use tiler_core::output_selection::{OutputEntry, output_for_rect as core_output_for_rect};
    let all: Vec<OutputEntry> = areas
        .iter()
        .map(|area| OutputEntry {
            id: OutputId(area.device.clone()),
            rect: area.full,
        })
        .collect();
    core_output_for_rect(&all, rect)
        .map(|id| id.0)
        .unwrap_or_else(|| "display-?".to_owned())
}

pub(super) fn output_for_point(areas: &[MonitorArea], x: i32, y: i32) -> Option<String> {
    use tiler_core::directional::OutputId;
    let screens: Vec<(OutputId, Rect)> = areas
        .iter()
        .map(|area| (OutputId(area.device.clone()), area.full))
        .collect();
    tiler_core::cursor_targeting::output_at_cursor(&screens, (x, y)).map(|id| id.0)
}

/// Windows multi-output directional foundation (REQ-OUT-01..03, REQ-MOV-08).
///
/// Exhausted directional tiled moves cross all four directions into the
/// selected adjacent output's current workspace; exhausted horizontal focus
/// crosses Left/Right only (vertical stays local). Selection reuses the
/// shared window-based FULL-rectangle rule; domain bounds use per-output work
/// areas with the live outer-gap inset; topology uses FULL rectangles. Core
/// rects are consumed in DPI-aware physical coordinates with no rescaling.
///
/// Stage-4a note: native gesture settle no longer snapshots drag-drop
/// fences here. The retired `drag_drop_live` reader is deleted with the
/// adapter settle route: the core commits the release from the normalized
/// ingress and the host diff applies the accepted reply with fresh proof.
/// Verify post-apply arrival for one mover row with fresh native proof,
/// never the setter result and never core desire.
///
/// Tiled rows under Controlled frame authority prove through the exact
/// published frame target on the row's device: the live frame must equal
/// `effective_frame` (the only write target; `allocation` is the canonical
/// slot, never a write target and never arrival proof) with screen affinity
/// on the same device.
///
/// Every other row (membership-only redirect lanes with no published frame,
/// fullscreen rows without a frame target, floats) proves through the
/// proven applied-homing receipt the tick merged with independent fresh
/// revalidation: non-floating rows must additionally still sit on the row's
/// device, while floating rows preserve their physical frame by design and
/// settle on the receipt alone. Anything unproven fails closed so the
/// caller holds instead of claiming arrival.
#[allow(clippy::too_many_arguments)]
pub(super) fn drop_arrival_verified(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    reply: &tiler_core::world_report::WorldReply,
    mover_token: &str,
    mover_hwnd: u64,
    expect_device: Option<&str>,
) -> bool {
    let Some(row) = reply.windows.iter().find(|w| w.lifetime.0 == mover_token) else {
        return false;
    };
    let Some(device) = state.device_of(&row.output.0) else {
        return false;
    };
    if expect_device.is_some_and(|expected| expected != device) {
        return false;
    }
    if !row.floating && row.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled
    {
        let Some(target) = row.effective_frame else {
            return false;
        };
        let Some(live) = state.readback_frame(me, fulls, mover_hwnd) else {
            return false;
        };
        return live == target && output_for_rect(areas, &live) == device;
    }
    let Some((wire, workspace)) = state.applied_homing.get(mover_token) else {
        return false;
    };
    if wire != &row.output.0 || workspace != &row.workspace {
        return false;
    }
    if row.floating {
        return true;
    }
    let Some(live) = state.readback_frame(me, fulls, mover_hwnd) else {
        return false;
    };
    output_for_rect(areas, &live) == device
}

#[cfg(test)]
mod tests {
    use super::*;

    fn areas() -> Vec<MonitorArea> {
        vec![
            MonitorArea {
                device: "left".to_owned(),
                work: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                full: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            },
            MonitorArea {
                device: "right".to_owned(),
                work: Rect {
                    x: 800,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                full: Rect {
                    x: 800,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            },
        ]
    }

    fn negative_areas() -> Vec<MonitorArea> {
        vec![
            MonitorArea {
                device: "west".to_owned(),
                work: Rect {
                    x: -1600,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                full: Rect {
                    x: -1600,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            },
            MonitorArea {
                device: "east".to_owned(),
                work: Rect {
                    x: -800,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                full: Rect {
                    x: -800,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            },
        ]
    }

    #[test]
    fn rect_sliver_off_screen_grounds_overlapping_output() {
        let areas = areas();
        // Tail sliver on the right monitor with the centre past its edge:
        // grounds right, never the first entry.
        assert_eq!(
            output_for_rect(
                &areas,
                &Rect {
                    x: 1500,
                    y: 100,
                    w: 400,
                    h: 200
                }
            ),
            "right"
        );
        // Odd-extent centre rounding on the shared edge: width 1 stays
        // left, width 2 lands on 800 and belongs to the right screen.
        assert_eq!(
            output_for_rect(
                &areas,
                &Rect {
                    x: 799,
                    y: 100,
                    w: 1,
                    h: 100
                }
            ),
            "left"
        );
        assert_eq!(
            output_for_rect(
                &areas,
                &Rect {
                    x: 799,
                    y: 100,
                    w: 2,
                    h: 100
                }
            ),
            "right"
        );
        // Negative origins: centre inside east, then a centre-outside
        // sliver with its only overlap on the non-first east output.
        let negative = negative_areas();
        assert_eq!(
            output_for_rect(
                &negative,
                &Rect {
                    x: -750,
                    y: 100,
                    w: 100,
                    h: 100
                }
            ),
            "east"
        );
        assert_eq!(
            output_for_rect(
                &negative,
                &Rect {
                    x: -100,
                    y: 100,
                    w: 300,
                    h: 100
                }
            ),
            "east"
        );
        assert_eq!(output_for_point(&areas, 1500, 10), Some("right".to_owned()));
        assert_eq!(output_for_point(&areas, 5000, 10), None);
        assert_eq!(
            output_for_rect(
                &[],
                &Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100
                }
            ),
            "display-?"
        );
    }
}
