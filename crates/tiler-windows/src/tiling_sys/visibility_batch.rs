#![forbid(unsafe_code)]
//! Joint hide/reveal batch phases for workspace select/send/rollback.
//!
//! For every workspace select/send phase operation this driver posts all
//! eligible effects first and confirms them together under one overall
//! 2000 ms deadline per phase (posts included), reusing the accepted
//! [`crate::product_hide::batch`] state machine. Success is observed state
//! only: an event without a matching snapshot never confirms.
//!
//! Safety sharing: preparation and finish run through the same
//! `product_hide::sys` prepare/post/finish helpers as the remaining
//! single-window paths (`hide_managed_claim`, `reveal_product_claim_to`),
//! so batch and single paths cannot diverge in proof, identity, nonce,
//! recycle, ledger-ownership, commit-before-hide, or failure recovery.
//! Only the wait is joint: posts run strictly before the first confirmation
//! snapshot, the initial snapshot covers events that arrived before the
//! first wait, rechecks run only after genuine SHOW/HIDE notifications, and
//! one final snapshot runs at the deadline. Unrelated wakes neither confirm
//! nor reset the deadline.
//!
//! Event discipline: the production clock compares the bounded
//! window-visibility generation (bumped once per window-scoped SHOW/HIDE
//! callback alongside the unchanged `Wake` queue push) across the existing
//! owner message pump. The pump dispatches callbacks on this same thread
//! while busy; it never drains the hook queue and never runs intake or
//! reconcile reentrantly, so unrelated [`HookEvent`]s stay queued for the
//! outer loop. When WinEvents are unavailable the generation never bumps
//! and the phase settles bounded `Uncertain`/`partial` at the deadline:
//! never a poll fallback, never an extension.
//!
//! Timing: each phase returns a [`PhaseBreakdown`] with the actual
//! post-call, visibility-wait, ledger-write, and log-write durations plus
//! the action total on the log line. Ledger time covers only the recovery
//! store commits (threaded into the sys helpers); log time covers only the
//! diagnostic line writes measured caller-side. Nonce property writes are
//! native effects, not ledger writes, and ride the post phase. The timing
//! line's own write is explicitly excluded (it cannot include itself).
//! Correlated per-phase lines sum cumulatively for follow/stay/native sides.

use super::*;
use crate::product_hide::batch::{BatchNative, BatchOutcome, BatchSignal, BatchWaitClock};

/// Actual per-phase durations for one joint batch phase. Every field times
/// only its named operation; fencing reads and admission are fast syscalls
/// and stay unattributed rather than mislabeled.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PhaseBreakdown {
    /// Windows the phase acted on (posts issued for hide; claims prepared
    /// for reveal, including no-post settlements).
    pub(super) count: usize,
    /// `ShowWindowAsync` post calls only.
    pub(super) post_ms: u64,
    /// Joint confirmation wait only (accumulated genuine waits).
    pub(super) wait_ms: u64,
    /// Recovery ledger commits only.
    pub(super) ledger_ms: u64,
    /// Diagnostic line writes only (never the timing line itself).
    pub(super) log_ms: u64,
}

/// Production joint-wait clock: monotonic owner time plus the
/// visibility-generation comparison across the existing owner message pump.
/// One pump per wait call carries the FULL remaining budget: unrelated
/// messages return early as [`BatchSignal::Unrelated`] and the batch loop
/// waits again on the same fixed deadline, so they can neither confirm nor
/// reset it nor spin indefinitely.
pub(super) struct OwnerBatchClock {
    start: Instant,
    last_gen: u64,
}

impl OwnerBatchClock {
    pub(super) fn new() -> Self {
        Self {
            start: Instant::now(),
            last_gen: visibility_generation(),
        }
    }
}

impl BatchWaitClock for OwnerBatchClock {
    fn now_ms(&mut self) -> u64 {
        self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn wait(&mut self, remaining_ms: u64) -> BatchSignal {
        // No time slicing and no stop polling: one pump carries the whole
        // remaining budget, exactly like the legacy per-window bound the
        // batch replaces (same worst-case stop latency, no new behavior).
        // The pump only services this thread's own queue: hook callbacks
        // (including SHOW/HIDE, which bump the generation) dispatch here,
        // while queued hook edges stay queued and intake/reconcile never
        // run. The initial snapshot before the first wait covers
        // notifications delivered before the wait, so no wake is lost.
        let slice = remaining_ms.min(u64::from(u32::MAX)) as u32;
        let woke = pump_wait_with_intent(slice, None, None, &[]);
        let observed = visibility_generation();
        if observed != self.last_gen {
            self.last_gen = observed;
            return BatchSignal::Notified;
        }
        if woke.timeout {
            BatchSignal::Timeout
        } else {
            BatchSignal::Unrelated
        }
    }
}

/// Owner action clock for one select/send switch: production runs the
/// event-driven owner clock (genuine SHOW/HIDE wakes over the existing
/// message pump); offline tests run the scripted fake clock (no live waits,
/// no hooks, no timers). Both feed the same accepted batch state machine.
#[cfg(not(test))]
pub(super) fn action_clock() -> OwnerBatchClock {
    OwnerBatchClock::new()
}

#[cfg(test)]
pub(super) fn action_clock() -> test_clock::TestBatchClock {
    test_clock::TestBatchClock::new(test_clock::take_script())
}

/// Batch deps gluing the accepted generic to the shared sys post/snapshot
/// boundary: posts go through the sys async setters (fake-gated offline),
/// snapshots through the sys visibility readback. Post durations time the
/// actual setter calls; wait durations accumulate in the clock.
struct PhaseDeps<'a, C> {
    hide: bool,
    show_cmds: HashMap<u64, i32>,
    post_ms: u64,
    wait_ms: u64,
    clock: &'a mut C,
}

impl<C: BatchWaitClock> BatchNative for PhaseDeps<'_, C> {
    fn post(&mut self, hwnd: u64, hide: bool) -> bool {
        debug_assert_eq!(hide, self.hide);
        let t0 = Instant::now();
        let ok = if hide {
            crate::product_hide::sys::batch_post_hide(hwnd)
        } else {
            match self.show_cmds.get(&hwnd) {
                Some(cmd) => crate::product_hide::sys::batch_post_reveal(hwnd, *cmd),
                None => false,
            }
        };
        self.post_ms = self
            .post_ms
            .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        ok
    }

    fn is_visible(&mut self, hwnd: u64) -> Option<bool> {
        crate::product_hide::sys::batch_is_visible(hwnd)
    }
}

impl<C: BatchWaitClock> BatchWaitClock for PhaseDeps<'_, C> {
    fn now_ms(&mut self) -> u64 {
        self.clock.now_ms()
    }

    fn wait(&mut self, remaining_ms: u64) -> BatchSignal {
        let t0 = Instant::now();
        let signal = self.clock.wait(remaining_ms);
        self.wait_ms = self
            .wait_ms
            .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        signal
    }
}

/// One hide candidate with its caller-resolved liveness facts.
pub(super) struct HideAttempt {
    pub(super) key: crate::workspace::WindowKey,
    pub(super) expected: ProcessIdentity,
    pub(super) iconic: bool,
}

/// Settled per-window hide outcome in the `workspace_hide_one` vocabulary,
/// so callers keep their exact existing arms.
pub(super) struct HideResult {
    pub(super) key: crate::workspace::WindowKey,
    pub(super) outcome: &'static str,
    pub(super) detail: Option<&'static str>,
}

/// Settled joint hide phase: per-window outcomes, the newly committed keys
/// for narrow recovery, the diagnostic attempt/log counts for the shared
/// omission summary, whether the phase failed, and the actual breakdown.
pub(super) struct HidePhase {
    pub(super) results: Vec<HideResult>,
    pub(super) newly_hidden: Vec<crate::workspace::WindowKey>,
    pub(super) attempted: usize,
    pub(super) logged: usize,
    pub(super) failed: bool,
    pub(super) breakdown: PhaseBreakdown,
}

/// Shared hide admission: the exact `workspace_hide_one` fences
/// (allowlist/proof, scope, hosted child, visible lifetime) plus managed
/// admission. Returns the admitted claim or the caller's existing
/// `(outcome, detail)` vocabulary. Sticky and missing-identity filtering
/// stay with the callers, exactly as today.
pub(super) fn admit_hide_item(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    key: &crate::workspace::WindowKey,
    expected: &ProcessIdentity,
) -> std::result::Result<crate::model::WindowIdentity, (&'static str, Option<&'static str>)> {
    use crate::product_hide::sys::ManagedAdmitError;
    if state.allowlist.is_some() && !state.workspace_proof {
        return Err(("workspace-disabled", None));
    }
    if !scope_allows(&state.scope, &expected.exe_path) {
        return Err(("scope-excluded", None));
    }
    if !hosted_gate_allows(
        &expected.exe_path,
        key.hwnd,
        expected.pid,
        &state.scope_hosts,
    ) {
        return Err(("scope-excluded", None));
    }
    let live_tag = live_member_tag(key.hwnd);
    let stored_tag = state.member_tags.get(key).cloned().unwrap_or_default();
    if !crate::workspace_owner::visible_lifetime_ok(&stored_tag, live_tag.as_deref()) {
        drop_member_state(state, key);
        return Err(("identity-changed", None));
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let Some(entry) = entries.iter().find(|e| e.hwnd == key.hwnd) else {
            return Err(("allowlist-changed", None));
        };
        if verify_proof_owned(key.hwnd, entry, me).is_err() {
            return Err(("identity-changed", None));
        }
    }
    let allow_project_topmost = crate::product_hide::project_topmost_hide_allowed(
        state.float_topmost_prev.get(key).copied(),
        read_topmost_now(key.hwnd),
    ) && key.pid == expected.pid
        && key.creation == expected.process_creation;
    match crate::product_hide::sys::admit_managed_claim_for_hide(
        key.hwnd,
        me,
        expected,
        &stored_tag,
        allow_project_topmost,
    ) {
        Ok(claim) => Ok(claim),
        Err(ManagedAdmitError::Absent) => Err(("origin-vanished", None)),
        Err(ManagedAdmitError::Uncertain) => Err(("uncertain", None)),
        Err(ManagedAdmitError::WrongIdentity) => Err(("identity-changed", None)),
        Err(ManagedAdmitError::Refused(code)) => Err(("refused", Some(code))),
    }
}

/// Post failure detail for one hide whose async setter refused: mirrors the
/// single path's `post_visibility` post failure, which keeps the committed
/// ledger and reports a fence (never a silent retire, never a poisoned
/// peer).
fn hide_post_failed_outcome() -> (&'static str, Option<&'static str>) {
    ("refused", None)
}

/// Map one hide finish error into the caller vocabulary, mirroring
/// `workspace_hide_one`: `absent:*` drops membership (the committed claim
/// stays rescue-registered exactly like the single path), `uncertain:*`
/// keeps the recoverable ledger for rollback, anything else refuses.
fn hide_finish_outcome(message: &str) -> (&'static str, Option<&'static str>) {
    if message.starts_with("absent:") {
        ("origin-vanished", None)
    } else if message.starts_with("uncertain:") {
        ("uncertain", None)
    } else {
        ("refused", None)
    }
}

/// Joint hide phase for one workspace select/send hide set: admit every
/// candidate, durably commit every admission, post every eligible hide,
/// then jointly confirm observed invisibility under one shared deadline.
/// A failed phase never proceeds to reveal: the caller rolls back exactly
/// [`HidePhase::newly_hidden`] and keeps the current view.
///
/// Commit owns the claim before any post (the owner table plus the hidden
/// flag register immediately, mirroring the single path's rescue), so a
/// post/readback failure keeps a recoverable ledger instead of a lost
/// window. Focus and z-order stay strictly after the caller's reveal
/// confirmation: this phase never focuses.
pub(super) fn hide_phase<C: BatchWaitClock>(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    ctx: &ActionCtx,
    attempts: Vec<HideAttempt>,
    clock: &mut C,
) -> HidePhase {
    use crate::product_hide::sys;
    let mut results: Vec<HideResult> = Vec::with_capacity(attempts.len());
    let mut newly_hidden: Vec<crate::workspace::WindowKey> = Vec::new();
    let mut attempted: usize = 0;
    let mut logged: usize = 0;
    let mut failed = false;
    let mut breakdown = PhaseBreakdown::default();
    // Admitted plus committed, in input order: every entry owns its target
    // through a live hold guard plus its durable ledger preimage.
    struct Committed {
        key: crate::workspace::WindowKey,
        claim: crate::model::WindowIdentity,
        held: crate::native::HeldProcess,
        pid: u32,
        project_topmost: bool,
    }
    let mut committed: Vec<Committed> = Vec::new();
    for attempt in attempts {
        let key = attempt.key.clone();
        let project_topmost = crate::product_hide::project_topmost_hide_allowed(
            state.float_topmost_prev.get(&key).copied(),
            read_topmost_now(key.hwnd),
        );
        let claim = match admit_hide_item(state, me, &key, &attempt.expected) {
            Ok(claim) => claim,
            Err((outcome, detail)) => {
                // Membership drops stay with the callers (select drops
                // vanished members, send legs keep their existing
                // handling): the driver only reports the shared vocabulary.
                // The admission itself already dropped stale same-process
                // reuse exactly like the single path.
                let t0 = Instant::now();
                note_hide_diag(
                    state,
                    ctx,
                    &key,
                    outcome,
                    detail,
                    project_topmost,
                    &mut logged,
                );
                breakdown.log_ms = breakdown
                    .log_ms
                    .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                attempted += 1;
                if outcome != "origin-vanished"
                    && outcome != "identity-changed"
                    && outcome != "scope-excluded"
                {
                    failed = true;
                }
                results.push(HideResult {
                    key,
                    outcome,
                    detail,
                });
                continue;
            }
        };
        attempted += 1;
        // Shared preparation: live checks plus durable preimage, then the
        // actual ledger commit. Only the commit lands in ledger time.
        let prepared = match sys::pre_hide_checks(me, &claim, dir) {
            Ok((pre, held)) => {
                let durable = sys::durable_hide_claim(&claim, &pre);
                match sys::commit_hide_claim(store, me, &durable, &mut breakdown.ledger_ms) {
                    Ok(()) => Some((durable, held, pre.pid)),
                    Err(e) => {
                        sys::cleanup_orphan_nonce(store, me, &claim);
                        let (outcome, detail) = hide_finish_outcome(&e.to_string());
                        let t0 = Instant::now();
                        note_hide_diag(
                            state,
                            ctx,
                            &key,
                            outcome,
                            detail,
                            project_topmost,
                            &mut logged,
                        );
                        breakdown.log_ms = breakdown.log_ms.saturating_add(
                            t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                        );
                        if outcome != "origin-vanished"
                            && outcome != "identity-changed"
                            && outcome != "scope-excluded"
                        {
                            failed = true;
                        }
                        results.push(HideResult {
                            key: key.clone(),
                            outcome,
                            detail,
                        });
                        None
                    }
                }
            }
            Err(e) => {
                sys::cleanup_orphan_nonce(store, me, &claim);
                let (outcome, detail) = hide_finish_outcome(&e.to_string());
                let t0 = Instant::now();
                note_hide_diag(
                    state,
                    ctx,
                    &key,
                    outcome,
                    detail,
                    project_topmost,
                    &mut logged,
                );
                breakdown.log_ms = breakdown
                    .log_ms
                    .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                if outcome != "origin-vanished"
                    && outcome != "identity-changed"
                    && outcome != "scope-excluded"
                {
                    failed = true;
                }
                results.push(HideResult {
                    key: key.clone(),
                    outcome,
                    detail,
                });
                None
            }
        };
        let Some((durable, held, pid)) = prepared else {
            continue;
        };
        // Commit owns the claim before any post: register immediately so a
        // later post/readback failure keeps recoverable ownership instead
        // of a lost window (single-path rescue mirror).
        let had_claim = state.hidden_claims.contains_key(&key);
        state.hidden_claims.insert(
            key.clone(),
            HiddenRecord {
                claim: durable.clone(),
                iconic: attempt.iconic,
            },
        );
        state.workspaces.set_hidden(&key, true);
        if !had_claim {
            newly_hidden.push(key.clone());
        }
        committed.push(Committed {
            key,
            claim: durable,
            held,
            pid,
            project_topmost,
        });
    }
    if !committed.is_empty() {
        let hwnds: Vec<u64> = committed.iter().map(|c| c.claim.hwnd).collect();
        let mut deps = PhaseDeps {
            hide: true,
            show_cmds: HashMap::new(),
            post_ms: 0,
            wait_ms: 0,
            clock,
        };
        let outcomes = crate::product_hide::batch::run_visibility_batch(&hwnds, true, &mut deps);
        breakdown.post_ms = breakdown.post_ms.saturating_add(deps.post_ms);
        breakdown.wait_ms = breakdown.wait_ms.saturating_add(deps.wait_ms);
        let _ = deps.clock;
        let by_hwnd: HashMap<u64, crate::product_hide::batch::BatchOutcome> =
            outcomes.into_iter().collect();
        for c in committed {
            let outcome = by_hwnd
                .get(&c.claim.hwnd)
                .copied()
                .unwrap_or(BatchOutcome::Uncertain);
            let (result, detail) = match outcome {
                BatchOutcome::Confirmed => {
                    match sys::finish_hide_confirm(me, &c.claim, &c.held, c.pid) {
                        Ok(()) => ("hidden", None),
                        Err(e) => hide_finish_outcome(&e.to_string()),
                    }
                }
                BatchOutcome::PostFailed => hide_post_failed_outcome(),
                BatchOutcome::Uncertain => ("uncertain", None),
                BatchOutcome::Absent => {
                    let mut ledger_ms = 0;
                    let _ = sys::remove_product_note(store, me, c.claim.hwnd, &mut ledger_ms);
                    breakdown.ledger_ms = breakdown.ledger_ms.saturating_add(ledger_ms);
                    state.hidden_claims.remove(&c.key);
                    ("origin-vanished", None)
                }
            };
            if result != "hidden"
                && result != "scope-excluded"
                && result != "origin-vanished"
                && result != "identity-changed"
            {
                failed = true;
            }
            // Membership drops stay with the callers (select drops vanished
            // members, send legs keep their existing handling): the driver
            // only reports the shared vocabulary.
            let t0 = Instant::now();
            note_hide_diag(
                state,
                ctx,
                &c.key,
                result,
                detail,
                c.project_topmost,
                &mut logged,
            );
            breakdown.log_ms = breakdown
                .log_ms
                .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
            results.push(HideResult {
                key: c.key,
                outcome: result,
                detail,
            });
        }
    }
    breakdown.count = results.len();
    HidePhase {
        results,
        newly_hidden,
        attempted,
        logged,
        failed,
        breakdown,
    }
}

/// Retire one revealed claim whose window is verifiably gone or recycled:
/// drop the claim plus the full session state, exactly like the select
/// reveal path. No writes, no focus.
fn retire_revealed(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
    state.hidden_claims.remove(key);
    if let Some(token) = state.member_tokens.remove(key) {
        state.member_rects.remove(&token);
        state.float_rects.remove(&token);
        state.hint_logged.remove(&token);
        state.hint_fallback_uses.remove(&token);
        drop_diag_sample(state, &token);
        drop_visibility_sample(state, &token);
    }
    if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
        state.restore_wake = None;
    }
    state.float_topmost_prev.remove(key);
    state.floated.remove(key);
    state.member_identity.remove(key);
    state.member_tags.remove(key);
    state.hint_last_good.remove(key);
    state.workspaces.remove_window(key);
}

/// Complete one verified reveal: drop the claim and unflag the member. The
/// caller focuses strictly afterwards, never here.
fn complete_revealed(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
    state.hidden_claims.remove(key);
    state.workspaces.set_hidden(key, false);
}

/// Settled joint reveal phase: every prepared reveal posts, then all posts
/// confirm together under one shared deadline. Already-visible and retired
/// claims settle during preparation with no post. Returns whether the phase
/// failed plus the diagnostic counts and the actual breakdown.
pub(super) struct RevealPhase {
    pub(super) failed: bool,
    pub(super) attempted: usize,
    pub(super) logged: usize,
    pub(super) breakdown: PhaseBreakdown,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reveal_phase<C: BatchWaitClock>(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    ctx: &ActionCtx,
    keys: &[crate::workspace::WindowKey],
    phase: Option<&'static str>,
    retire_full_drop: bool,
    clock: &mut C,
) -> RevealPhase {
    use crate::product_hide::sys;
    let mut failed = false;
    let mut attempted: usize = 0;
    let mut logged: usize = 0;
    let mut breakdown = PhaseBreakdown::default();
    struct Posting {
        key: crate::workspace::WindowKey,
        claim: crate::model::WindowIdentity,
        held: crate::native::HeldProcess,
        pid: u32,
    }
    let mut posting: Vec<Posting> = Vec::new();
    let mut show_cmds: HashMap<u64, i32> = HashMap::new();
    for key in keys {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            continue;
        };
        attempted += 1;
        if let Err(cause) = workspace_proof_reveal_gate(state, me, key.hwnd) {
            let t0 = Instant::now();
            note_reveal_diag(state, ctx, key, "refused", Some(cause), phase, &mut logged);
            breakdown.log_ms = breakdown
                .log_ms
                .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
            failed = true;
            continue;
        }
        let mut ledger_ms = 0;
        let ready = sys::prepare_reveal(store, me, &record.claim, record.iconic, &mut ledger_ms);
        breakdown.ledger_ms = breakdown.ledger_ms.saturating_add(ledger_ms);
        match ready {
            Ok(sys::RevealReady::Done(teardown)) => {
                let outcome = reveal_outcome(&teardown);
                let t0 = Instant::now();
                note_reveal_diag(state, ctx, key, outcome, None, phase, &mut logged);
                breakdown.log_ms = breakdown
                    .log_ms
                    .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                if matches!(teardown, crate::product_hide::ProductTeardown::Retired) {
                    if retire_full_drop {
                        // Rollback parity: the legacy rollback dropped the
                        // full member state (not just the select reveal
                        // subset) for retired claims.
                        state.hidden_claims.remove(key);
                        drop_member_state(state, key);
                    } else {
                        retire_revealed(state, key);
                    }
                } else {
                    complete_revealed(state, key);
                }
            }
            Ok(sys::RevealReady::Post(prep)) => {
                show_cmds.insert(record.claim.hwnd, prep.show_cmd);
                posting.push(Posting {
                    key: key.clone(),
                    claim: record.claim,
                    held: prep.held,
                    pid: prep.pid,
                });
            }
            Err(e) => {
                let (outcome, detail) = reveal_err_outcome(&e.to_string());
                let t0 = Instant::now();
                note_reveal_diag(state, ctx, key, outcome, Some(detail), phase, &mut logged);
                breakdown.log_ms = breakdown
                    .log_ms
                    .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                failed = true;
            }
        }
    }
    if !posting.is_empty() {
        let hwnds: Vec<u64> = posting.iter().map(|p| p.claim.hwnd).collect();
        let mut deps = PhaseDeps {
            hide: false,
            show_cmds,
            post_ms: 0,
            wait_ms: 0,
            clock,
        };
        let outcomes = crate::product_hide::batch::run_visibility_batch(&hwnds, false, &mut deps);
        breakdown.post_ms = breakdown.post_ms.saturating_add(deps.post_ms);
        breakdown.wait_ms = breakdown.wait_ms.saturating_add(deps.wait_ms);
        let by_hwnd: HashMap<u64, BatchOutcome> = outcomes.into_iter().collect();
        for p in posting {
            let outcome = by_hwnd
                .get(&p.claim.hwnd)
                .copied()
                .unwrap_or(BatchOutcome::Uncertain);
            match outcome {
                BatchOutcome::Confirmed => {
                    let mut ledger_ms = 0;
                    let finish = sys::finish_reveal_guarded(
                        store,
                        me,
                        &p.claim,
                        Some((&p.held, p.pid)),
                        &mut ledger_ms,
                    );
                    breakdown.ledger_ms = breakdown.ledger_ms.saturating_add(ledger_ms);
                    match finish {
                        Ok(_) => {
                            let t0 = Instant::now();
                            note_reveal_diag(
                                state,
                                ctx,
                                &p.key,
                                "revealed",
                                None,
                                phase,
                                &mut logged,
                            );
                            breakdown.log_ms = breakdown.log_ms.saturating_add(
                                t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                            );
                            complete_revealed(state, &p.key);
                        }
                        Err(e) => {
                            let (outcome, detail) = reveal_err_outcome(&e.to_string());
                            let t0 = Instant::now();
                            note_reveal_diag(
                                state,
                                ctx,
                                &p.key,
                                outcome,
                                Some(detail),
                                phase,
                                &mut logged,
                            );
                            breakdown.log_ms = breakdown.log_ms.saturating_add(
                                t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                            );
                            failed = true;
                        }
                    }
                }
                BatchOutcome::PostFailed => {
                    let (outcome, detail) = reveal_err_outcome("error: visibility post failed");
                    let t0 = Instant::now();
                    note_reveal_diag(
                        state,
                        ctx,
                        &p.key,
                        outcome,
                        Some(detail),
                        phase,
                        &mut logged,
                    );
                    breakdown.log_ms = breakdown
                        .log_ms
                        .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                    failed = true;
                }
                BatchOutcome::Uncertain => {
                    let (outcome, detail) =
                        reveal_err_outcome("uncertain: visibility not observed");
                    let t0 = Instant::now();
                    note_reveal_diag(
                        state,
                        ctx,
                        &p.key,
                        outcome,
                        Some(detail),
                        phase,
                        &mut logged,
                    );
                    breakdown.log_ms = breakdown
                        .log_ms
                        .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                    failed = true;
                }
                BatchOutcome::Absent => {
                    let mut ledger_ms = 0;
                    let _ = sys::remove_product_note(store, me, p.claim.hwnd, &mut ledger_ms);
                    breakdown.ledger_ms = breakdown.ledger_ms.saturating_add(ledger_ms);
                    let t0 = Instant::now();
                    note_reveal_diag(state, ctx, &p.key, "retired", None, phase, &mut logged);
                    breakdown.log_ms = breakdown
                        .log_ms
                        .saturating_add(t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                    if retire_full_drop {
                        state.hidden_claims.remove(&p.key);
                        drop_member_state(state, &p.key);
                    } else {
                        retire_revealed(state, &p.key);
                    }
                }
            }
        }
    }
    breakdown.count = attempted;
    RevealPhase {
        failed,
        attempted,
        logged,
        breakdown,
    }
}

/// Bounded normal-level per-switch visibility timing line: the actual
/// post-call, visibility-wait, ledger-write, and log-write durations for
/// the hide and reveal phases plus the action total so far, under the
/// action correlation. Follow/stay/native sides log their own correlated
/// lines, which sum cumulatively. The timing line's own write is excluded
/// by construction (it is never timed).
pub(super) fn log_visibility_timing(
    state: &TileLoop,
    ctx: &ActionCtx,
    side: &'static str,
    hide: &PhaseBreakdown,
    reveal: &PhaseBreakdown,
) {
    let action_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    log_json_at(
        &state.log_path.clone(),
        serde_json::json!({
            "event": "workspace-visibility-timing",
            "tick": ctx.tick,
            "correlation": ctx.correlation,
            "side": side,
            "hide": {
                "count": hide.count,
                "post_ms": hide.post_ms,
                "wait_ms": hide.wait_ms,
                "ledger_ms": hide.ledger_ms,
                "log_ms": hide.log_ms,
            },
            "reveal": {
                "count": reveal.count,
                "post_ms": reveal.post_ms,
                "wait_ms": reveal.wait_ms,
                "ledger_ms": reveal.ledger_ms,
                "log_ms": reveal.log_ms,
            },
            "action_ms": action_ms,
        }),
    );
}

#[cfg(test)]
pub(crate) mod test_clock {
    //! Offline joint-wait clock for batch regression tests: a fake
    //! monotonic clock plus scripted wait signals. Drives the real phase
    //! state machine with the real ledger; native posts and snapshots run
    //! through the sys fake backend. No live effects.

    use crate::product_hide::batch::{BatchSignal, BatchWaitClock};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    thread_local! {
        static SCRIPT: RefCell<Vec<WaitScript>> = const { RefCell::new(Vec::new()) };
    }

    /// Script the joint-wait signals for the next action clock (test-only).
    /// An empty script settles every wait at the shared deadline with no
    /// real waiting.
    pub(crate) fn inject_script(script: Vec<WaitScript>) {
        SCRIPT.with(|slot| *slot.borrow_mut() = script);
    }

    pub(crate) fn take_script() -> Vec<WaitScript> {
        SCRIPT.with(|slot| std::mem::take(&mut *slot.borrow_mut()))
    }

    /// One scripted wait verdict. `NotifyWith` applies host-side visibility
    /// flips before waking, proving events complete without any other
    /// external traffic.
    #[derive(Debug)]
    pub(crate) enum WaitScript {
        Notify,
        NotifyWith(Vec<(u64, bool)>),
        Unrelated,
    }

    pub(crate) struct TestBatchClock {
        pub now: u64,
        pub script: VecDeque<WaitScript>,
        pub waits: Vec<u64>,
        pub waited_ms: u64,
    }

    impl TestBatchClock {
        pub(crate) fn new(script: Vec<WaitScript>) -> Self {
            Self {
                now: 0,
                script: script.into(),
                waits: Vec::new(),
                waited_ms: 0,
            }
        }
    }

    impl BatchWaitClock for TestBatchClock {
        fn now_ms(&mut self) -> u64 {
            self.now
        }

        fn wait(&mut self, remaining_ms: u64) -> BatchSignal {
            self.waits.push(remaining_ms);
            match self.script.pop_front() {
                Some(WaitScript::Notify) => {
                    self.now += 5;
                    self.waited_ms += 5;
                    BatchSignal::Notified
                }
                Some(WaitScript::NotifyWith(flips)) => {
                    for (hwnd, visible) in flips {
                        crate::product_hide::sys::fake::with_mut(hwnd, |w| {
                            w.visible = visible;
                        });
                    }
                    self.now += 5;
                    self.waited_ms += 5;
                    BatchSignal::Notified
                }
                Some(WaitScript::Unrelated) => {
                    self.now += 5;
                    self.waited_ms += 5;
                    BatchSignal::Unrelated
                }
                None => {
                    self.now = self.now.saturating_add(remaining_ms);
                    self.waited_ms = self.waited_ms.saturating_add(remaining_ms);
                    BatchSignal::Timeout
                }
            }
        }
    }
}
