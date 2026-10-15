#![forbid(unsafe_code)]

//! Phase 1 dedicated input owner (implementation).
//!
//! Split from [`crate::runtime_thread`] to keep owners small: exact-chord
//! matching over the input source, forwarded as semantic actions. This
//! thread never touches the core.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use crate::input::cg_flag_bits;
use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
use crate::runtime::{Shared, emit, lock_shared};
use crate::runtime_backend::InputSourceFactory;
use crate::runtime_lifecycle::InputExit;
use crate::runtime_thread::{EngineJob, Inbox};
use crate::runtime_types::{InputSemanticAction, TileBinding};

/// Exact-match tuning for one catalogue chord: the Ctrl-role modifier bit
/// plus the Shift arm when set, under the same mask (Caps/Fn bits stay
/// outside the mask, so they neither match nor break a chord — the existing
/// matcher contract).
fn tuning_for_binding(modifier: crate::config::ProbeModifier, binding: &TileBinding) -> TapTuning {
    let (mut want_flags, flags_mask) = cg_flag_bits(modifier);
    if binding.shift {
        want_flags |= 0x0002_0000; // Shift bit in `CGEventFlags` space.
    }
    TapTuning {
        want_flags,
        flags_mask,
        want_code: i64::from(binding.key_code),
    }
}

/// Catalogue input owner: creates, drives, and destroys its input source
/// inside this thread. Exact chord matches on key down only are forwarded
/// as semantic actions; this thread never touches the core. An unexpected
/// source EOS (`None` without stop) is counted, logged, and exits — never
/// spun on. First exact match wins; key-up, inexact modifiers (including
/// extras), and unrelated keys are ignored.
pub(crate) fn input_main_bound(
    shared: Arc<Mutex<Shared>>,
    inbox: Arc<Inbox>,
    stop: Arc<AtomicBool>,
    factory: Arc<dyn InputSourceFactory>,
    modifier: crate::config::ProbeModifier,
    bindings: &[TileBinding],
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> InputExit {
    // Retain the parked thread so stop/shutdown can `unpark` it awake.
    lock_shared(&shared).input_thread = Some(std::thread::current());
    let mut source = match factory.create() {
        Ok(source) => {
            let _ = ready_tx.send(Ok(()));
            source
        }
        Err(reason) => {
            let _ = ready_tx.send(Err(reason));
            return InputExit::Done;
        }
    };
    let tunings: Vec<(TapTuning, InputSemanticAction)> = bindings
        .iter()
        .map(|binding| (tuning_for_binding(modifier, binding), binding.action))
        .collect();
    // The source is destroyed here on thread exit.
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Some(event) = source.next(&stop) else {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            // Unexpected EOS (disconnect/exhaustion): counted, logged, and
            // folded into the shutdown outcome (unclean, never clean) —
            // never spun on.
            lock_shared(&shared).diagnostics.unexpected_eos += 1;
            emit(&shared, 0, 0, 0, "input", "eos-unexpected", 0);
            break;
        };
        // Forward semantic actions only on key down with an exact chord
        // match. Key-up, inexact modifiers (including extras), and unrelated
        // keys are ignored; the core is never consulted here.
        if event.kind != TapKeyKind::KeyDown {
            lock_shared(&shared).diagnostics.input_ignored += 1;
            continue;
        }
        // First exact match wins (the config validator forbids duplicate
        // chords, so at most one arm matches).
        let mut action = None;
        for (tuning, candidate) in &tunings {
            let mut pending = false;
            if decide_tap_event(
                TapKeyKind::KeyDown,
                event.flags,
                i64::from(event.key_code),
                *tuning,
                false,
                &mut pending,
            )
            .matched
            {
                action = Some(*candidate);
                break;
            }
        }
        let Some(action) = action else {
            lock_shared(&shared).diagnostics.input_ignored += 1;
            continue;
        };
        if !inbox.try_send_data(EngineJob::InputAction { action }) {
            lock_shared(&shared).diagnostics.data_dropped_full += 1;
        }
    }
    InputExit::Done
}
