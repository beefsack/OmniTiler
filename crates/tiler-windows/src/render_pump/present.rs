use super::*;

/// Native present outcome for one carrier: what actually happened plus the
/// owned-pixel checksum of what was presented (0 when hidden). `resolved`
/// is the actual presented colour (accent-resolved for the border): the ack
/// logs this, never the owner fallback.
pub struct PresentOutcome {
    pub outcome: RenderOutcome,
    pub checksum: u64,
    pub error: Option<String>,
    pub resolved: Option<(u8, u8, u8)>,
}

impl PresentOutcome {
    /// Successful present with a checksum and the actual presented colour.
    #[must_use]
    pub fn shown(outcome: RenderOutcome, checksum: u64, resolved: (u8, u8, u8)) -> Self {
        Self {
            outcome,
            checksum,
            error: None,
            resolved: Some(resolved),
        }
    }

    /// Narrow failure: the carrier hid instead, with bounded detail.
    #[must_use]
    pub fn failed(detail: String) -> Self {
        Self {
            outcome: RenderOutcome::Hidden,
            checksum: 0,
            error: Some(truncate_render_error(&detail)),
            resolved: None,
        }
    }
}

/// Freshness query for one present: the band to follow plus the identities
/// and suppression inputs the renderer rechecks itself.
pub struct FreshQuery<'a> {
    /// Band to follow (border target band; underlay resolved-anchor band).
    pub topmost: bool,
    /// Lifetime the target/anchor window must still match.
    pub expected: &'a WindowLifetime,
    /// Foreground identity that must be unchanged since post time (`None`
    /// means the post-time foreground read was unreadable and refuses
    /// narrowly per the binding contract; used solely for preview frames).
    pub foreground: Option<&'a WindowLifetime>,
    /// Monitor full rects for the captionless-fullscreen predicate.
    pub fulls: &'a [tiler_core::geometry::Rect],
    /// Explicit owner proof behind the managed-fullscreen bypass.
    pub fullscreen_bypass: bool,
}

/// Renderer-resolved underlay anchor with its correctly paired identity: a
/// foreign member carries its own lifetime for individual validation, while
/// the renderer's own border surface carries no foreign lifetime (it
/// validates as renderer-owned: visible, same-group, same-band).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Foreign { hwnd: u64, lifetime: WindowLifetime },
    OwnBorder { hwnd: u64 },
}

impl ResolvedAnchor {
    /// Anchor window handle either way.
    #[must_use]
    pub const fn hwnd(&self) -> u64 {
        match self {
            Self::Foreign { hwnd, .. } | Self::OwnBorder { hwnd } => *hwnd,
        }
    }
}

/// Renderer-side present seam. Implementations run entirely on the renderer
/// thread: native reads and owned-window writes only, never owner
/// observation, Engine, foreign writes, or file logging.
pub trait Present {
    /// Fresh native facts for one target/anchor HWND per `query`: liveness,
    /// lifetime identity match (pid/creation/tag against `expected`),
    /// visibility (folding non-iconic and DWM-uncloaked), maximized and
    /// captionless-fullscreen suppression, foreground-identity match, and
    /// band match. Read-only, renderer-local; unreadable reads fail closed.
    fn check_fresh(&self, carrier: RenderCarrier, hwnd: u64, query: &FreshQuery<'_>) -> FreshFacts;
    /// Fresh facts for the renderer's own border surface as an underlay
    /// anchor: renderer-owned validation (visible, same-band) plus the
    /// unchanged-foreground fence. Never a foreign member tag.
    fn check_own_anchor(
        &self,
        hwnd: u64,
        topmost: bool,
        foreground: Option<&WindowLifetime>,
    ) -> FreshFacts;
    /// Fresh lowest-in-z anchor among the individually validated live
    /// candidates (`None` when no candidate validates or the order is
    /// unreadable). The native backend additionally sorts its own border
    /// surface into the comparison only when that surface is visible,
    /// same-group, and same-band.
    fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor>;
    /// Read the current `WS_EX_TOPMOST` band of one HWND (`None` when
    /// unreadable). Lets the core follow the resolved anchor's band
    /// like-for-like with the same-thread path.
    fn read_band(&self, hwnd: u64) -> Option<bool>;
    /// Currently owned carrier window, if created.
    fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64>;
    /// Whether the owned carrier surface currently reads visible.
    fn carrier_visible(&self, carrier: RenderCarrier) -> bool;
    /// Opaque target/anchor token the owned carrier currently shows, if any.
    /// Lets the core admit the own border as an anchor only for its own
    /// group.
    fn carrier_token(&self, carrier: RenderCarrier) -> Option<String>;
    /// Creator-side ensure: create every missing carrier window hidden.
    /// Fails closed with bounded detail; the core refuses the pending work
    /// narrowly until the next retry.
    fn ensure_created(&mut self) -> Result<(), String>;
    /// Present one border frame for an already-gated target.
    fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome;
    /// Present one underlay frame beneath the already-resolved anchor, in
    /// the resolved anchor's band (like-for-like with the same-thread path,
    /// which reads the band from the resolved anchor).
    fn show_underlay(
        &mut self,
        frame: &UnderlayDesired,
        anchor: u64,
        topmost: bool,
    ) -> PresentOutcome;
    /// Present one preview frame for an already-gated mover.
    fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome;
    /// Hide one carrier with a creator-side verified hidden readback
    /// (no-op when already hidden). True only when the surface reads hidden
    /// afterwards; a failed hide keeps its state for a bounded retry.
    fn hide(&mut self, carrier: RenderCarrier) -> bool;
    /// Destroy every carrier on the creator thread. True when every carrier
    /// reports destroyed.
    fn destroy_all(&mut self) -> bool;
}
