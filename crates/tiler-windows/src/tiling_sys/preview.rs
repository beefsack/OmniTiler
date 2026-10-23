#![forbid(unsafe_code)]
/// START-frozen preview source binding: the exact mover token plus the
/// source output/workspace captured at gesture START (native MoveSizeStart
/// drain). The preview never rebinds mid-gesture: the core owns every
/// drift decision from the staged observation. The token match below is a
/// native lifetime fence (the mover is still the START lifetime), never a
/// target decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewStartBinding {
    pub(super) token: String,
    pub(super) output: String,
    pub(super) workspace: String,
}

pub(super) struct PreviewBound {
    /// Carried sticky group-edge hover prior for the next sample and the
    /// final drop (exact 32/80 semantics live in the Engine resolver; this
    /// only carries the opaque value across samples).
    pub(super) hover_prior: Option<tiler_core::session::DragHoverPrior>,
}
