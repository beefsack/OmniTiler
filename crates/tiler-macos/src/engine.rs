#![forbid(unsafe_code)]

//! Engine thread: sole owner of mutable probe state.
//!
//! Main (AppKit), input-tap, and per-app AX threads never
//! mutate shared state; they forward [`EngineMessage`] over a *bounded*
//! [`sync_channel`](std::sync::mpsc::sync_channel) via non-blocking
//! `try_send`. A full queue drops the event at the sender (backpressure),
//! never blocks a tap/AX callback.

use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Duration;

/// Bounded engine queue depth. Senders use `try_send`; overflow is dropped
/// and countable at the sender, never a block inside a callback.
pub const ENGINE_QUEUE_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineMessage {
    Tick,
    AxEvent {
        pid: u32,
    },
    InputEvent {
        key_code: u32,
    },
    /// Sent at most once per tap run when the OS disables the tap
    /// (timeout or user input). Informational only: a disable is not
    /// recovery evidence; the tap guard still owns teardown.
    TapDisabled,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineState {
    pub processed: u64,
    pub ax_events: u64,
    pub input_events: u64,
    pub tap_disabled_events: u64,
    pub last_ax_pid: Option<u32>,
}

/// Pure transition. Total: never blocks, never touches UI/taps.
pub fn dispatch(state: &mut EngineState, msg: EngineMessage) {
    state.processed += 1;
    match msg {
        EngineMessage::Tick => {}
        EngineMessage::AxEvent { pid } => {
            state.ax_events += 1;
            state.last_ax_pid = Some(pid);
        }
        EngineMessage::InputEvent { .. } => {
            state.input_events += 1;
        }
        EngineMessage::TapDisabled => {
            state.tap_disabled_events += 1;
        }
        EngineMessage::Shutdown => {}
    }
}

pub struct Engine {
    pub tx: SyncSender<EngineMessage>,
}

impl Engine {
    /// Bounded channel pair. `try_send` fails (never blocks) when full.
    pub fn spawn() -> (Self, Receiver<EngineMessage>) {
        let (tx, rx) = std::sync::mpsc::sync_channel(ENGINE_QUEUE_DEPTH);
        (Self { tx }, rx)
    }

    /// Non-blocking forward. `Full` means the event was dropped by
    /// backpressure; `Disconnected` means the engine exited.
    pub fn try_send(&self, msg: EngineMessage) -> Result<(), TrySendError<EngineMessage>> {
        self.tx.try_send(msg)
    }

    /// Own state on this thread until Shutdown or timeout. Returns final state.
    pub fn run(rx: Receiver<EngineMessage>, idle_timeout: Duration) -> EngineState {
        let mut state = EngineState::default();
        loop {
            match rx.recv_timeout(idle_timeout) {
                Ok(EngineMessage::Shutdown) => {
                    dispatch(&mut state, EngineMessage::Shutdown);
                    break;
                }
                Ok(msg) => dispatch(&mut state, msg),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if state.processed > ENGINE_QUEUE_DEPTH as u64 * 1024 {
                break;
            }
        }
        state
    }

    /// Spawn the engine loop on its own thread. Modes hold the `Engine`
    /// (sender side) and join the handle for the final state.
    pub fn spawn_thread(idle_timeout: Duration) -> (Self, std::thread::JoinHandle<EngineState>) {
        let (engine, rx) = Self::spawn();
        let handle = std::thread::Builder::new()
            .name("tiler-macos-engine".to_owned())
            .spawn(move || Self::run(rx, idle_timeout))
            .expect("engine thread spawn");
        (engine, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn dispatch_counts_by_kind() {
        let mut s = EngineState::default();
        dispatch(&mut s, EngineMessage::Tick);
        dispatch(&mut s, EngineMessage::AxEvent { pid: 7 });
        dispatch(&mut s, EngineMessage::InputEvent { key_code: 4 });
        assert_eq!(s.processed, 3);
        assert_eq!(s.ax_events, 1);
        assert_eq!(s.input_events, 1);
        assert_eq!(s.last_ax_pid, Some(7));
    }

    #[test]
    fn backpressure_drops_instead_of_blocking() {
        let (engine, _rx) = Engine::spawn();
        for _ in 0..ENGINE_QUEUE_DEPTH {
            engine.try_send(EngineMessage::Tick).unwrap();
        }
        let err = engine.try_send(EngineMessage::Tick).unwrap_err();
        assert!(matches!(err, TrySendError::Full(EngineMessage::Tick)));
    }

    #[test]
    fn run_owns_state_until_shutdown() {
        let (engine, rx) = Engine::spawn();
        engine.try_send(EngineMessage::AxEvent { pid: 9 }).unwrap();
        engine.try_send(EngineMessage::Shutdown).unwrap();
        let state = Engine::run(rx, Duration::from_secs(1));
        assert_eq!(state.ax_events, 1);
        assert_eq!(state.last_ax_pid, Some(9));
    }

    #[test]
    fn tap_disable_is_counted_once_per_send() {
        let mut s = EngineState::default();
        dispatch(&mut s, EngineMessage::TapDisabled);
        dispatch(&mut s, EngineMessage::TapDisabled);
        assert_eq!(s.tap_disabled_events, 2);
        assert_eq!(s.input_events, 0);
    }

    #[test]
    fn spawn_thread_integrates() {
        let (engine, handle) = Engine::spawn_thread(Duration::from_secs(2));
        engine
            .try_send(EngineMessage::InputEvent { key_code: 4 })
            .unwrap();
        engine.try_send(EngineMessage::Shutdown).unwrap();
        let state = handle.join().unwrap();
        assert_eq!(state.input_events, 1);
    }
}
