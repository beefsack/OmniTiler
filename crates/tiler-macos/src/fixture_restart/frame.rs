//! Exact per-window frame facts shared by every restart proof: one window's
//! independent AX/CG reads plus the bit-exact rectangle key used by the
//! multiset comparisons. No live effect.

use serde::Serialize;

use crate::fixture_frame::FramePoint;

/// One native window's exact independent frame reads: AX frame plus CG
/// bounds plus the CG onscreen fact, plus the raw native `AXZoomed` value
/// read off the same joined AX window. Both geometric legs are captured from
/// independent APIs; neither is derived from the other. `zoomed` is the
/// unmodified optional AX fact (`None` when the attribute is unreadable).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WindowFrames {
    pub caller: String,
    pub wid: u32,
    pub ax: FramePoint,
    pub cg: FramePoint,
    pub cg_onscreen: bool,
    pub zoomed: Option<bool>,
}

/// Exact multisets use the bit pattern of every finite float component:
/// equality is bit equality and the sort order is deterministic without a
/// total-order float claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RectKey {
    x: u64,
    y: u64,
    w: u64,
    h: u64,
}

impl From<&FramePoint> for RectKey {
    fn from(rect: &FramePoint) -> Self {
        Self {
            x: rect.x.to_bits(),
            y: rect.y.to_bits(),
            w: rect.w.to_bits(),
            h: rect.h.to_bits(),
        }
    }
}

pub(super) fn rect_multiset(frames: impl Iterator<Item = FramePoint>) -> Vec<RectKey> {
    let mut keys: Vec<RectKey> = frames.map(|frame| RectKey::from(&frame)).collect();
    keys.sort_unstable();
    keys
}
