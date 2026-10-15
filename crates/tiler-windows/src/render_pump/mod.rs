//! Dedicated render thread (stage 2, unit: option B renderer).
//!
//! The renderer creates, pumps, presents and destroys the active border,
//! group underlay and drop-preview carriers on its own thread. The owner
//! keeps observation, Engine, foreign actuation, recovery and all logging.
//!
//! Transport contract (from [`crate::render_handoff`]):
//! - Owner posts replaceable desired frames (latest-wins) plus safety
//!   epochs; the renderer rechecks fresh native facts against the desired
//!   epoch and refuses stale or unreadable targets narrowly.
//! - The renderer publishes immutable overlay identities for owner
//!   observation/WinEvent exclusion and acknowledges applied generations
//!   with outcomes, owned-pixel checksums and bounded failures. Owner logs
//!   consume acks, never desired state.
//! - Preview finish hides with an acknowledgement the owner waits for
//!   outside callbacks with a bounded timeout; a missing ack is reported
//!   truthfully while the bumped safety epoch alone prevents stale-show
//!   replay.
//! - Readiness and creator-side destroy carry bounded failure reporting. A
//!   stuck renderer keeps the process render slot so no overlapping
//!   replacement can start; recovery is process exit.
//!
//! The portable [`RenderCore`] drives a [`Present`] seam: deterministic
//! [`FakePresent`] fixtures cover races without native calls, while the
//! `cfg(windows)` native backend owns the real carriers on the renderer
//! thread. Pixels, follow and z-order are unchanged: the native backend
//! calls the same carrier methods as the same-thread path, so raster
//! checksums stay comparable.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::render_handoff::{
    AppliedContent, BorderDesired, CarrierAck, DesiredFrame, FreshFacts, PreviewDesired,
    RenderCarrier, RenderChannels, RenderOutcome, UnderlayDesired, WindowLifetime, freshness_gate,
    queue_age_us_since, truncate_render_error,
};

/// How long the owner waits for renderer readiness before reporting failure
/// without claiming readiness.
pub const RENDER_READY_WAIT: Duration = Duration::from_secs(2);
/// Bounded stop acknowledgement wait: a missing ack returns failure and
/// retains `Stopping` (never claims clean, never starts an overlapping
/// worker).
pub const RENDER_STOP_WAIT: Duration = Duration::from_secs(5);
/// Bounded preview-finish hide acknowledgement wait: a missing ack is logged
/// truthfully and the owner proceeds, with the bumped safety epoch alone
/// preventing any stale-show replay.
pub const FINISH_WAIT: Duration = Duration::from_millis(500);

mod fake;
#[cfg(windows)]
pub mod native;
mod present;
mod render_core;

#[cfg(test)]
use self::render_core::hide_backoff;
pub use fake::FakePresent;
pub use present::{FreshQuery, Present, PresentOutcome, ResolvedAnchor};
pub use render_core::RenderCore;

#[cfg(test)]
mod tests;
