#![forbid(unsafe_code)]
use super::*;

/// Oldest snap-batch queue age in microseconds. `u64::MAX` saturates the
/// `u128` conversion; empty batches report zero.
#[must_use]
pub fn snap_batch_age_us(batch: &[QueuedSnapEvent], now: Instant) -> u64 {
    let mut oldest: Option<Instant> = None;
    for event in batch {
        let tick = match event {
            QueuedSnapEvent::Intent(i) => i.tick,
            QueuedSnapEvent::Resize(i) => i.tick,
            QueuedSnapEvent::Workspace(i) => i.tick,
            QueuedSnapEvent::WorkspaceSend(i) => i.tick,
            QueuedSnapEvent::WorkspaceOutput(i) => i.tick,
            QueuedSnapEvent::WorkspaceMigrate(i) => i.tick,
            QueuedSnapEvent::WorkspaceHistory(i) => i.tick,
            QueuedSnapEvent::Maximize(i) => i.tick,
            QueuedSnapEvent::Fullscreen(i) => i.tick,
            QueuedSnapEvent::Float(i) => i.tick,
            QueuedSnapEvent::Sticky(i) => i.tick,
            QueuedSnapEvent::Orientation(i) => i.tick,
            QueuedSnapEvent::WorkspaceTiling(i) => i.tick,
            QueuedSnapEvent::Launch(i) => i.tick,
            QueuedSnapEvent::Mask(i) => i.tick,
        };
        oldest = Some(match oldest {
            Some(cur) => cur.min(tick),
            None => tick,
        });
    }
    match oldest {
        Some(tick) => now
            .duration_since(tick)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64,
        None => 0,
    }
}

/// Build one whole owner snapshot behind an `Arc` without touching input
/// state. Pure constructor for the owner thread.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn shared_snapshot(
    generation: u64,
    keyboard: KeyboardConfig,
    remap: Vec<crate::snapkey::ChordRemap>,
    disabled: Vec<crate::snapkey::ChordDisable>,
    snap_origins: std::collections::HashMap<u64, crate::snapkey::SnapOrigin>,
    windrag_origins: std::collections::HashMap<u64, crate::input_handoff::WindragOrigin>,
    active: bool,
    trace: bool,
) -> SharedSnapshot {
    Arc::new(InputSnapshot {
        generation,
        keyboard,
        remap,
        disabled,
        snap_origins,
        windrag_origins,
        active,
        trace,
    })
}
