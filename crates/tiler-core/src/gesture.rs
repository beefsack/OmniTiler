//! Normalized gesture lifecycle ingress and pure gesture decisions (stage 4a).
//!
//! The adapter reports one [`GestureIngress`] per world report while a native
//! move/size gesture is open: the exact window lifetime, the START-sampled
//! kind (move or resize from host input classification), the live cursor,
//! and the terminal phase (released or cancelled). The [`WorldEngine`] owns
//! the retained session, the pointer target resolution, the drop-overlay
//! shape, and the release/commit decision over the existing
//! session drag/resize machinery. The adapter only observes, captures,
//! renders, and applies.
//!
//! Pure decisions moved here from the Windows adapter (no behavior change):
//! release-intent classification from pre/post frames plus cursor
//! ([`classify_gesture`], [`prefer_modal_move`]), the domain-bounds fence
//! ([`drop_point_in_domain`]),
//! and pointer-output containment ([`pointer_output`]). Native identity,
//! freshness, lifetime, scope, coordinate conversion, Esc detection, and the
//! render-hide acknowledgement stay adapter-mechanical fences around these.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::DragHoverPrior;

/// Normalized gesture kind, sampled once at START from host input
/// classification (caption grab starts a move, sizing borders a resize).
/// The core retains it for preview gating; the release intent re-derives
/// from the retained start frame versus the release frame plus cursor
/// ([`classify_gesture`]), so the frames stay ground truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GestureKind {
    Move,
    Resize,
}

impl GestureKind {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Resize => "resize",
        }
    }

    /// Parse one wire token. `None` on unknown values (the report refuses).
    #[must_use]
    pub fn parse_wire(text: &str) -> Option<Self> {
        match text {
            "move" => Some(Self::Move),
            "resize" => Some(Self::Resize),
            _ => None,
        }
    }
}

/// Normalized gesture phase for one report: exactly one terminal or
/// non-terminal phase rides each report while the gesture is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GesturePhase {
    /// A newly opened gesture: binds the exact window lifetime, kind,
    /// START cursor, and modal truth in core.
    Started,
    /// An open gesture with a fresh cursor sample (or none when the
    /// adapter could not read it): refreshes the pointer target and the
    /// drop-overlay shape. Never commits.
    Updated,
    /// The native gesture ended: resolve the pointer target and commit a
    /// move or resize release, or snap back with no commit when the
    /// pointer proves no change.
    Released,
    /// The gesture was cancelled (explicit Esc/cancel fact from the
    /// adapter): clear the retained session with no commit. The adapter
    /// detects cancellation; the core only clears.
    Cancelled,
}

impl GesturePhase {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Updated => "updated",
            Self::Released => "released",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse one wire token. `None` on unknown values (the report refuses).
    #[must_use]
    pub fn parse_wire(text: &str) -> Option<Self> {
        match text {
            "started" => Some(Self::Started),
            "updated" => Some(Self::Updated),
            "released" => Some(Self::Released),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Normalized gesture ingress for one world report: the exact window
/// lifetime under gesture, the START-sampled kind, the live cursor in
/// adapter units (same space as screen FULL rectangles and fact frames;
/// `None` when unreadable or deliberately unsampled under a focused
/// fullscreen game), the phase, and the adapter-observed Esc/cancel fact.
///
/// `id` is the stable adapter gesture sequence for this gesture: nonzero,
/// allocated once at native START within the adapter incarnation, and
/// carried unchanged on every ingress of the gesture (Started, Updated,
/// terminal). The core orders sessions and at-most-once terminals by this
/// id alone, never by window plus frame.
///
/// `start_frame` is the PRE-gesture frame frozen at START (never a
/// mid-gesture read): the ground truth the release intent derives from,
/// constant across every ingress of one gesture. `start_cursor` is the
/// START down point (never a mid-gesture read), riding every ingress so a
/// first Updated after a lost Started still zero-gates; `None` only when
/// the down point itself was unreadable. `modal_move` is the
/// START-sampled modal truth (a caption-started loop carries user move
/// intent even through a system rescale); it rides every phase but only
/// the release derivation reads it. `esc` is the host-owned cancellation
/// edge: when set, the core clears regardless of phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GestureIngress {
    pub id: u64,
    pub window: WindowId,
    pub kind: GestureKind,
    pub cursor: Option<(i32, i32)>,
    pub start_cursor: Option<(i32, i32)>,
    pub phase: GesturePhase,
    pub esc: bool,
    pub modal_move: bool,
    pub start_frame: Rect,
}

/// Precise result of one incorporated gesture terminal, drained by the
/// adapter after the tick that ran it. `committed` is true only when the
/// core actually committed a move or resize release; every other terminal
/// (no-change snap-back, refusal, cancel, close-clear) reports false.
/// Never equate an accepted or stale reply with completion: only this
/// record names what the gesture run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestureSettled {
    /// The stable gesture id this terminal incorporated.
    pub id: u64,
    /// True only on a real move/resize commit.
    pub committed: bool,
}

/// One settled manual gesture, derived from the retained start frame and the
/// release frame plus the settle-time cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureIntent {
    /// Position changed: re-place at the cursor drop point.
    MoveDrop { x: i32, y: i32 },
    /// Size changed: pointer-resize the grabbed edges. Horizontal axis
    /// first, matching the dual-axis route.
    PointerResize {
        direction: &'static str,
        boundary: i32,
        direction2: Option<&'static str>,
        boundary2: Option<i32>,
    },
}

/// Derive the settle intent from the stable start frame and the release
/// frame plus the settle-time cursor position.
///
/// Same size at a new origin is a true move. Any size change makes it a
/// resize: a changed origin means the left/top edge moved, never a move.
/// Edges derive per axis from start/release; when both edges of one axis
/// moved, the larger-magnitude edge wins (ties go to the origin edge).
/// `None` when nothing changed, the cursor is missing for a move, or a
/// boundary overflows `i32`.
#[must_use]
pub fn classify_gesture(
    before: &Rect,
    after: &Rect,
    cursor: Option<(i32, i32)>,
) -> Option<GestureIntent> {
    let pos_changed = before.x != after.x || before.y != after.y;
    let size_changed = before.w != after.w || before.h != after.h;
    if !pos_changed && !size_changed {
        return None;
    }
    if !size_changed {
        let (x, y) = cursor?;
        return Some(GestureIntent::MoveDrop { x, y });
    }
    let before_right = i64::from(before.x) + i64::from(before.w);
    let after_right = i64::from(after.x) + i64::from(after.w);
    let before_bottom = i64::from(before.y) + i64::from(before.h);
    let after_bottom = i64::from(after.y) + i64::from(after.h);
    let horizontal = pick_edge(
        before.x,
        after.x,
        before_right,
        after_right,
        "left",
        "right",
    )?;
    let vertical = pick_edge(before.y, after.y, before_bottom, after_bottom, "up", "down")?;
    match (horizontal, vertical) {
        (None, None) => None,
        (Some((direction, boundary)), None) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (None, Some((direction, boundary))) => Some(GestureIntent::PointerResize {
            direction,
            boundary,
            direction2: None,
            boundary2: None,
        }),
        (Some((direction, boundary)), Some((direction2, boundary2))) => {
            Some(GestureIntent::PointerResize {
                direction,
                boundary,
                direction2: Some(direction2),
                boundary2: Some(boundary2),
            })
        }
    }
}

/// Pick one axis edge from origin/edge deltas. `None` when neither moved;
/// the larger-magnitude delta wins when both moved, ties to the origin edge.
fn pick_edge(
    before_origin: i32,
    after_origin: i32,
    before_far: i64,
    after_far: i64,
    origin_name: &'static str,
    far_name: &'static str,
) -> Option<Option<(&'static str, i32)>> {
    let origin_moved = before_origin != after_origin;
    let far_moved = before_far != after_far;
    if !origin_moved && !far_moved {
        return Some(None);
    }
    if origin_moved
        && (!far_moved
            || (after_origin as i64 - before_origin as i64).abs() >= (after_far - before_far).abs())
    {
        return Some(Some((origin_name, after_origin)));
    }
    if far_moved {
        return Some(Some((far_name, after_far.try_into().ok()?)));
    }
    Some(Some((origin_name, after_origin)))
}

/// Prefer START-sampled modal truth over rect comparison for one release
/// intent: a caption-started move loop carries user move intent even when
/// the system rescaled the frame mid-gesture (mixed-DPI `WM_DPICHANGED`
/// across outputs), which rect comparison alone would misread as a user
/// resize. A user resize always starts on a sizing border instead, so only
/// a resize verdict is overridden, and only with a captured release cursor
/// to place the drop. Move verdicts pass through; cursor-less resizes keep
/// the geometry route (no point to place).
#[must_use]
pub fn prefer_modal_move(
    classified: GestureIntent,
    cursor: Option<(i32, i32)>,
    modal_move: bool,
) -> GestureIntent {
    if modal_move
        && matches!(classified, GestureIntent::PointerResize { .. })
        && let Some((x, y)) = cursor
    {
        return GestureIntent::MoveDrop { x, y };
    }
    classified
}

/// Domain-bounds fence for a settle-time drop point: the release cursor must
/// still sit inside the given domain bounds. The resolver applies this per
/// output: same-output points keep the local route, pointer-output points
/// join through the cross route, and outside points refuse with snap-back
/// and no transfer.
#[must_use]
pub fn drop_point_in_domain(
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: i32,
    bounds_h: i32,
    x: i32,
    y: i32,
) -> bool {
    if bounds_w <= 0 || bounds_h <= 0 {
        return false;
    }
    let dx = i64::from(x) - i64::from(bounds_x);
    let dy = i64::from(y) - i64::from(bounds_y);
    dx >= 0 && dy >= 0 && dx < i64::from(bounds_w) && dy < i64::from(bounds_h)
}

/// Resolve the output under a pointer point over retained FULL rectangles:
/// the first containing rectangle wins, in stable output order. `None`
/// outside every output: the caller hides/refuses without a drop. This is
/// pointer containment, never the directional adjacency selector.
#[must_use]
pub fn pointer_output(outputs: &[(OutputId, Rect)], x: i32, y: i32) -> Option<OutputId> {
    for (id, rect) in outputs {
        if rect.w > 0
            && rect.h > 0
            && x >= rect.x
            && x < rect.x.saturating_add(rect.w)
            && y >= rect.y
            && y < rect.y.saturating_add(rect.h)
        {
            return Some(id.clone());
        }
    }
    None
}

/// Drop-destination resolution for one move-drop point, over core-retained
/// state. `SameOutput` keeps the local route when the point also sits
/// inside the source inset work area; `Cross` carries the pointer output
/// (the engine resolves its current view); `Refused` carries the honest
/// vocabulary for unplaceable points (outside every output, a floating
/// destination, outside the inset work area, or an unreadable/incomplete
/// topology).
#[derive(Debug, PartialEq, Eq)]
pub enum DropDestination {
    SameOutput,
    Cross { output: OutputId },
    Refused(&'static str),
}

/// Resolve one move-drop point: pointer FULL containment decides the
/// output (panels never break output identity); sideways, stacked, and
/// negative-coordinate arrangements behave alike. The destination view is
/// never hidden or switched: only tiled geometry writes plus mover
/// follow-focus run.
#[must_use]
pub fn resolve_drop_destination(
    fulls: &[(OutputId, Rect)],
    source_output: &OutputId,
    x: i32,
    y: i32,
    source_inset: Option<Rect>,
    dest_tiled: impl Fn(&OutputId) -> Option<bool>,
    dest_inset: impl Fn(&OutputId) -> Option<Rect>,
) -> DropDestination {
    let Some(dest) = pointer_output(fulls, x, y) else {
        return DropDestination::Refused("outside-output");
    };
    if dest == *source_output {
        return match source_inset {
            Some(bounds) if drop_point_in_domain(bounds.x, bounds.y, bounds.w, bounds.h, x, y) => {
                DropDestination::SameOutput
            }
            _ => DropDestination::Refused("outside-work-area"),
        };
    }
    match dest_tiled(&dest) {
        Some(true) => {}
        Some(false) => return DropDestination::Refused("floating-dest"),
        None => return DropDestination::Refused("no-view"),
    }
    let Some(bounds) = dest_inset(&dest) else {
        return DropDestination::Refused("unknown-output");
    };
    if !drop_point_in_domain(bounds.x, bounds.y, bounds.w, bounds.h, x, y) {
        return DropDestination::Refused("outside-work-area");
    }
    DropDestination::Cross { output: dest }
}

/// Closed bounded hover-prior trace descriptor: `none`, or the sticky
/// group-edge side (`group-edge:left`, ...). Never carries raw NodeIds,
/// tokens, or geometry: sides are a fixed vocabulary, sufficient to prove
/// prior carry equality preview-to-drop.
#[must_use]
pub fn preview_prior_desc(prior: &Option<DragHoverPrior>) -> String {
    match prior {
        None => "none".to_owned(),
        Some(carried) => match &carried.prior {
            None => "none".to_owned(),
            Some(edge) => format!("group-edge:{}", edge.edge.as_str()),
        },
    }
}
