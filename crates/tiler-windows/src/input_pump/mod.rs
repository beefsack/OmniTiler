#![forbid(unsafe_code)]
//! Dedicated input pump (stage 1, unit 2).
//!
//! Both low-level hooks and their synchronous consume/pass classifiers share
//! one dedicated input thread. The owner keeps observation, Engine, foreign
//! actuation, overlays, tray, settings, trace writing, Mouse Snap SPI and the
//! single recovery ledger.
//!
//! Transport contract (from [`crate::input_handoff`]):
//! - Owner builds immutable snapshots off the input thread and moves them
//!   into the bounded [`Inbox`] FIFO whole and in order. No latest-wins, no
//!   unbounded batch channel.
//! - The input pump applies FIFO items between message pumps into local
//!   classifier state; callbacks only read that local state plus one
//!   single-entry origin clone. Large maps are never cloned in callbacks.
//! - All input-side exchanges use `try_lock` variants: callbacks and the
//!   pump never wait on an owner-held mutex. Full is an explicit fault with
//!   the item preserved for retry; fresh admission closes immediately and no
//!   unbounded owner backlog accumulates.
//! - Input drains the real bounded classifier queue into single-outstanding
//!   [`Outbox`] batches outside callbacks. While an outbox is occupied the
//!   pump skips draining so items wait in the original queue under their own
//!   cap+priority rules; a refused offer retries from a thread-local pending
//!   slot, never a second lossy queue.
//! - Esc ordering uses the shared atomic [`EscClock`] sampled directly at
//!   WinEvent START/END (owner) and Esc edges (input).
//!   Delayed batch sequences never decide cancellation.
//! - The keyboard hook lives on this pump for the E8 reserve/send/stamp
//!   reentry path.
//! - Moves pass through; no motion intent stream.
//! - Logging stays owner-side: the pump publishes bounded [`InputDiag`]
//!   samples; queue age and heartbeat gaps carry explicit non-latency limits.
//!
//! Proof hook code (`install_proof`) is preserved untouched for the separate
//! `shortcut-proof` command; the product run uses this pump.

use std::collections::VecDeque;
#[cfg(windows)]
use std::os::windows::io::OwnedHandle;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use crate::input_handoff::{
    DiagSlot, INBOX_CAP, Inbox, InboxItem, InputSnapshot, Outbox, SharedSnapshot, TaggedBatch,
};
use crate::snapkey::{KeyboardConfig, QueuedSnapEvent};

mod backoff;
mod channels;
mod fake;
mod inbox_sender;
#[cfg(windows)]
mod native;
mod snapshot;

#[cfg(windows)]
pub(crate) use backoff::classify_input_wait;
pub use backoff::{CONTENTION_RETRY, InputWaitReason, READY_WAIT, STOP_WAIT, install_backoff_secs};
pub use channels::InputChannels;
pub use fake::FakeBackend;
pub use inbox_sender::InboxSender;
pub use snapshot::{shared_snapshot, snap_batch_age_us};

#[cfg(windows)]
pub use native::{FailEvent, InputHandle, NativeHooks, PumpSignals};

/// Test-only real-core seam for the stage-3 combined stress: no production
/// visibility change (the `native` module stays private).
#[cfg(all(windows, test))]
pub(crate) use native::PumpCore;

#[cfg(test)]
mod tests;
