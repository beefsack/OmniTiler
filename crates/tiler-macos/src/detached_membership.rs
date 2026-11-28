#![forbid(unsafe_code)]

//! Bounded read-only detached-membership proof bridge.
//!
//! One detached-fullscreen select leg must prove that a frozen member still
//! belongs to exactly the frozen fullscreen surface. That proof is a
//! [`crate::runtime_types::BracketRead`] from the SAME per-app AX owner that
//! observed the window (fresh lifetime identity, the exact AX anchor
//! retained for it, fresh CG geometry, fresh SLS membership), so the select
//! owner cannot read it itself: it asks the owner through this single-flight
//! slot and the owner performs its dedicated read-only
//! [`crate::runtime_backend::AxBackend::read_retained_membership`] operation.
//! It deliberately does NOT re-enumerate `AXWindows`: a true fullscreen
//! member is absent from that listing once it is off the current Space.
//!
//! Contract, fail-closed on every leg:
//! - the requester verifies the generation frozen with the command report
//!   and resolves only that owner slot, never the bare pid or a bare wid;
//! - the request carries that generation and is served only by the owner
//!   thread of exactly that generation (a mismatched owner replies without
//!   reading and the waiter refuses);
//! - exactly one request is outstanding per app (select is single-flight);
//!   a concurrent request refuses instead of queueing behind an unknown
//!   read;
//! - a receipt is accepted only for the exact request id, with the same
//!   generation, and only while the app record still names that generation;
//!   any timeout, stop, replacement, or termination refuses;
//! - classification requires the owner identity attestation and a readable
//!   Space membership: `Ok(true)` only for the exact singleton surface,
//!   `Ok(false)` for a readable different membership, `Err` for anything
//!   unknown.
//!
//! Read-only: no setter, no retry, no clock bump, no new work id.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::runtime_types::BracketRead;

/// Bound for one owner-serviced membership read, from request to receipt.
/// The owner multiplexes at the worker observation slice, so a healthy
/// owner answers within a few tens of milliseconds; the bound only caps a
/// stopped or hung owner, which then refuses (never guesses).
pub(crate) const MEMBERSHIP_PROBE_TIMEOUT: Duration = Duration::from_millis(1_000);

/// One per-app-owner request slot. Created with the app lifetime, owned by
/// its [`crate::runtime_shared::AppRecord`], serviced only on that app's AX
/// owner thread, and stopped with the same lifetime.
#[derive(Debug, Default)]
pub(crate) struct MembershipProbeSlot {
    state: Mutex<ProbeState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct ProbeState {
    next_request: u64,
    /// The outstanding request, if any (at most one).
    pending: Option<MembershipProbeRequest>,
    /// The receipt for `pending`'s request id (never a later request's).
    reply: Option<MembershipProbeReply>,
    stopped: bool,
}

/// One immutable owner request: the exact app generation, plus the window
/// leg. The surface is not needed owner-side (the owner returns the raw
/// membership facts); the requester classifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MembershipProbeRequest {
    pub(crate) request: u64,
    pub(crate) generation: u64,
    pub(crate) wid: u32,
}

#[derive(Debug)]
struct MembershipProbeReply {
    request: u64,
    generation: u64,
    read: Option<BracketRead>,
}

impl MembershipProbeSlot {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Begin exactly one bounded read on this owner slot. Refuses while the
    /// slot is stopped or another request is outstanding (never queues an
    /// unreceipted read).
    pub(crate) fn begin(
        &self,
        generation: u64,
        wid: u32,
    ) -> Result<MembershipProbeWaiter<'_>, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return Err("detached membership owner stopped".to_owned());
        }
        if state.pending.is_some() || state.reply.is_some() {
            return Err("detached membership owner busy with another read".to_owned());
        }
        state.next_request = state.next_request.wrapping_add(1).max(1);
        let request = state.next_request;
        state.pending = Some(MembershipProbeRequest {
            request,
            generation,
            wid,
        });
        Ok(MembershipProbeWaiter {
            slot: self,
            request,
            generation,
        })
    }

    /// Owner side: the outstanding request, unchanged until receipt or
    /// timeout. `None` when stopped or idle.
    pub(crate) fn take(&self) -> Option<MembershipProbeRequest> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return None;
        }
        state.pending
    }

    /// Owner side: post the receipt for `request`. A stale receipt (the
    /// waiter already timed out and a newer request took the slot) is
    /// dropped, never applied to the newer request. `read` is the owner's
    /// raw bracket leg for exactly `request.wid` (`None` when the owner did
    /// not read it).
    pub(crate) fn complete(
        &self,
        request: MembershipProbeRequest,
        generation: u64,
        read: Option<BracketRead>,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped || state.pending.map(|pending| pending.request) != Some(request.request) {
            return;
        }
        state.pending = None;
        state.reply = Some(MembershipProbeReply {
            request: request.request,
            generation,
            read,
        });
        self.changed.notify_all();
    }

    /// Stop the slot with its app lifetime: waiters refuse immediately and
    /// no later request can begin.
    pub(crate) fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        state.pending = None;
        state.reply = None;
        self.changed.notify_all();
    }
}

/// One begun read. Waiting is bounded and consumes exactly its own receipt.
pub(crate) struct MembershipProbeWaiter<'a> {
    slot: &'a MembershipProbeSlot,
    request: u64,
    generation: u64,
}

impl MembershipProbeWaiter<'_> {
    pub(crate) fn wait(self) -> Result<BracketRead, String> {
        self.wait_for(MEMBERSHIP_PROBE_TIMEOUT)
    }

    fn wait_for(self, timeout: Duration) -> Result<BracketRead, String> {
        let Self {
            slot,
            request,
            generation,
        } = self;
        let deadline = Instant::now() + timeout;
        let mut state = slot.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if state.stopped {
                return Err("detached membership owner stopped".to_owned());
            }
            if state.reply.as_ref().is_some_and(|reply| reply.request == request) {
                let receipt = state.reply.take().expect("matched membership receipt");
                let receipt_generation = receipt.generation;
                let read = receipt.read;
                drop(state);
                if receipt_generation != generation {
                    return Err(format!(
                        "detached membership owner generation moved (owner {receipt_generation}, request {generation})"
                    ));
                }
                return read.ok_or_else(|| {
                    "detached membership owner read returned no bracket".to_owned()
                });
            }
            let now = Instant::now();
            if now >= deadline {
                // Retract our own request only (a newer request can never
                // share the slot while ours is pending). The owner's later
                // receipt for this id is dropped as stale.
                if state.pending.map(|pending| pending.request) == Some(request) {
                    state.pending = None;
                }
                if state.reply.as_ref().map(|reply| reply.request) == Some(request) {
                    state.reply = None;
                }
                return Err("detached membership owner receipt timed out".to_owned());
            }
            let (next, _) = slot
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(|e| e.into_inner());
            state = next;
        }
    }
}

/// Classify one owner bracket leg as exact detached membership. `Ok(true)`
/// requires the backend's owner attestation AND exactly the one frozen
/// surface in the fresh SLS membership; a readable different membership is
/// `Ok(false)`; an unattested owner, a mismatched window, or an unreadable
/// membership is `Err` (never guessed, never focus evidence).
pub(crate) fn classify_membership(
    read: Option<&BracketRead>,
    pid: u32,
    wid: u32,
    surface: u64,
) -> Result<bool, String> {
    let read = read.ok_or_else(|| {
        format!("detached membership of {pid}_{wid} returned no owner bracket")
    })?;
    if read.wid != wid {
        return Err(format!(
            "detached membership read names window {} not {wid}",
            read.wid
        ));
    }
    if read.owner_pid != Some(pid) {
        return Err(format!(
            "detached membership owner identity unattested for {pid}_{wid}"
        ));
    }
    if read.spaces.is_empty() {
        return Err(format!(
            "detached membership of {pid}_{wid} is unreadable"
        ));
    }
    Ok(read.spaces.len() == 1 && read.spaces[0] == surface)
}

/// Select-side handle: verifies the frozen owner generation and resolves
/// its slot in the shared runtime. Cloneable across the serialized select owner.
#[derive(Clone)]
pub(crate) struct MembershipProbe {
    shared: Arc<Mutex<crate::runtime_shared::Shared>>,
}

impl std::fmt::Debug for MembershipProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MembershipProbe")
    }
}

impl MembershipProbe {
    pub(crate) fn new(shared: Arc<Mutex<crate::runtime_shared::Shared>>) -> Self {
        Self { shared }
    }

    /// Bridge for one runtime handle: the per-app AX owners live behind the
    /// same shared cell.
    pub(crate) fn from_runtime(handle: &crate::runtime::RuntimeHandle) -> Self {
        Self::new(Arc::clone(&handle.shared))
    }

    /// One fresh membership read through the exact app owner that supplied
    /// the command report's member fact. Revalidates the owner on receipt.
    pub(crate) fn read_membership(
        &self,
        member: &str,
        generation: u64,
        surface: u64,
    ) -> Result<bool, String> {
        let (pid, wid) = crate::runtime_types::parse_window_id(member)
            .ok_or_else(|| format!("unreadable detached membership identity {member}"))?;
        let slot = {
            let guard = crate::runtime_shared::lock_shared(&self.shared);
            let Some(app) = guard.apps.get(&pid) else {
                return Err(format!(
                    "detached membership has no live AX owner for pid {pid}"
                ));
            };
            if app.terminated || app.generation != generation {
                return Err(format!(
                    "detached membership is no longer on the frozen AX owner lifetime for {member}"
                ));
            }
            Arc::clone(&app.membership)
        };
        let read = slot.begin(generation, wid)?.wait()?;
        {
            // The receipt is usable only while the exact lifetime that was
            // asked is still the live one: a terminate or relaunch between
            // request and receipt refuses instead of attributing a read
            // across lifetimes.
            let guard = crate::runtime_shared::lock_shared(&self.shared);
            match guard.apps.get(&pid) {
                Some(app) if !app.terminated && app.generation == generation => {}
                _ => {
                    return Err(format!(
                        "detached membership owner lifetime for pid {pid} changed during the read"
                    ));
                }
            }
        }
        classify_membership(Some(&read), pid, wid, surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(wid: u32, owner: Option<u32>, spaces: Vec<u64>) -> BracketRead {
        BracketRead {
            wid,
            owner_pid: owner,
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces,
            frame: Some((0, 0, 100, 100)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }
    }

    #[test]
    fn exact_singleton_on_attested_lifetime_is_proven() {
        assert_eq!(
            classify_membership(Some(&read(7, Some(4242), vec![9])), 4242, 7, 9),
            Ok(true)
        );
    }

    #[test]
    fn readable_other_membership_is_not_the_surface() {
        // A readable move to another surface/Desktop is a definite "not
        // here", never a proof and never an error.
        assert_eq!(
            classify_membership(Some(&read(7, Some(4242), vec![8])), 4242, 7, 9),
            Ok(false)
        );
        // More than one Space (Split View) is readable but not the exact
        // singleton.
        assert_eq!(
            classify_membership(Some(&read(7, Some(4242), vec![9, 10])), 4242, 7, 9),
            Ok(false)
        );
    }

    #[test]
    fn unknown_identity_or_unreadable_membership_refuses() {
        // No bracket at all.
        assert!(classify_membership(None, 4242, 7, 9).is_err());
        // Unattested owner: a reused wid must never be attributed.
        assert!(classify_membership(Some(&read(7, None, vec![9])), 4242, 7, 9).is_err());
        assert!(classify_membership(Some(&read(7, Some(1), vec![9])), 4242, 7, 9).is_err());
        // Empty membership is unreadable, never "not here".
        assert!(classify_membership(Some(&read(7, Some(4242), Vec::new())), 4242, 7, 9).is_err());
        // A mismatched window leg.
        assert!(classify_membership(Some(&read(8, Some(4242), vec![9])), 4242, 7, 9).is_err());
    }

    #[test]
    fn exact_receipt_proves_and_generation_move_refuses() {
        let slot = MembershipProbeSlot::new();
        let waiter = slot.begin(3, 7).expect("request begins");
        let request = slot.take().expect("owner sees the request");
        assert_eq!(request.generation, 3);
        slot.complete(request, 3, Some(read(7, Some(4242), vec![9])));
        assert!(slot.begin(3, 8).is_err(), "unconsumed receipt still owns the slot");
        assert_eq!(waiter.wait_for(Duration::from_millis(50)), Ok(read(7, Some(4242), vec![9])));

        let waiter = slot.begin(4, 7).expect("request begins");
        let request = slot.take().expect("owner sees the request");
        slot.complete(request, 5, Some(read(7, Some(4242), vec![9])));
        assert!(waiter.wait_for(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn stale_receipt_never_completes_a_newer_request() {
        let slot = MembershipProbeSlot::new();
        let first = slot.begin(1, 7).expect("first begins");
        let stale = slot.take().expect("owner sees first");
        // The waiter retracts on its own deadline; the owner's late receipt
        // for the retracted id must not satisfy the next request.
        assert!(first.wait_for(Duration::from_millis(1)).is_err());
        let second = slot.begin(1, 7).expect("second begins");
        slot.complete(stale, 1, Some(read(7, Some(4242), vec![9])));
        assert!(second.wait_for(Duration::from_millis(10)).is_err());
    }

    #[test]
    fn stop_refuses_waiters_and_new_requests() {
        let slot = MembershipProbeSlot::new();
        let waiter = slot.begin(1, 7).expect("request begins");
        slot.stop();
        assert!(waiter.wait_for(Duration::from_millis(50)).is_err());
        assert!(slot.begin(1, 7).is_err());
        assert!(slot.take().is_none());
    }

    #[test]
    fn concurrent_request_refuses_instead_of_queueing() {
        let slot = MembershipProbeSlot::new();
        let _first = slot.begin(1, 7).expect("first begins");
        assert!(slot.begin(1, 8).is_err());
    }
}
