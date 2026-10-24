//! Retained transfer settlement (D3=A): view, window/migration, and float
//! settlement against staged homing, raw echoes, and raw correlated attempt
//! facts.
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. No behavior change.
//!
//! Completion is core-owned over raw mechanical attempt facts
//! ([`MemberAttempt`](crate::world_report::MemberAttempt) setter plus
//! pre/post homing, [`ViewAttempt`](crate::world_report::ViewAttempt)
//! setter plus pre/post/baseline views) plus the staged observation. The
//! adapter reports write/re-read facts only and stages no verdict: every
//! confirmation, proven refusal, ambiguity latch, and external
//! supersession below is derived here. External transfer/view verdict
//! ingress is retired; internal [`AttemptOutcome`] latch vocabulary
//! (migration terminal) is retained.

use crate::directional::{OutputId, WindowId};
use crate::session::DomainKey;
use crate::world_engine::{RetainedFloat, RetainedTransfer, WorldEngine};
use crate::world_report::{
    AttemptOutcome, FrameSetter, MemberAttempt, TransferKind, ViewAttempt, WorldReport,
};

/// Exact-correlated raw membership evidence for one retained operation
/// window plus target. Forged rows (operation, window, or target mismatch)
/// are never evidence: they read as absent.
fn member_attempt_for<'a>(
    report: &'a WorldReport,
    operation_id: &str,
    window: &WindowId,
    target_output: &OutputId,
    target_workspace: &str,
) -> Option<&'a MemberAttempt> {
    report.member_attempts.iter().find(|attempt| {
        attempt.operation_id == operation_id
            && attempt.window == *window
            && attempt.target_output == *target_output
            && attempt.target_workspace == target_workspace
    })
}

/// Exact-correlated raw view evidence for one owning phase plus target.
/// Same forged-row rule as [`member_attempt_for`].
fn view_attempt_for<'a>(
    report: &'a WorldReport,
    operation_id: &str,
    target_output: &OutputId,
    target_workspace: &str,
) -> Option<&'a ViewAttempt> {
    report.view_attempts.iter().find(|attempt| {
        attempt.operation_id == operation_id
            && attempt.target_output == *target_output
            && attempt.target_workspace == target_workspace
    })
}

/// Proven refusal from one membership attempt: an explicit native refusal
/// with an unchanged readable preimage that is not the target. Anything
/// else (accepted mismatch, throw, unreadable proof, moved preimage)
/// latches ambiguity, never refusal.
fn member_refused(
    attempt: &MemberAttempt,
    target_output: &OutputId,
    target_workspace: &str,
) -> bool {
    matches!(attempt.setter, FrameSetter::Refused)
        && attempt.pre.is_some()
        && attempt.pre == attempt.post
        && attempt.pre.as_ref().is_some_and(|(output, workspace)| {
            output != target_output || workspace != target_workspace
        })
}

/// Proven refusal from one view attempt: same unchanged-readable-preimage
/// rule as [`member_refused`]. A follow miss is never a refused window:
/// callers hold the parent ambiguous instead of settling refusal.
fn view_refused(attempt: &ViewAttempt, target_workspace: &str) -> bool {
    matches!(attempt.setter, FrameSetter::Refused)
        && attempt.pre.is_some()
        && attempt.pre == attempt.post
        && attempt.pre.as_deref() != Some(target_workspace)
}

/// Third-location proof from one view attempt: a readable post that is
/// neither the target nor the KNOWN observation-time baseline proves newer
/// external input. Unknown or empty baselines never prove third locations:
/// a post matching an unknown baseline reads as ambiguity, never
/// supersession. An unchanged preimage (`pre == post`) is never external
/// proof even when the baseline differs: the write left the view where it
/// was, so the attempt latches ambiguity without re-actuation. Target
/// observation wins elsewhere (callers check exact echo before consulting
/// this): this predicate never overrides an exact target proof.
fn view_third_location(attempt: &ViewAttempt, target_workspace: &str) -> bool {
    let Some(post) = attempt.post.as_deref() else {
        return false;
    };
    let Some(baseline) = attempt.baseline.as_deref() else {
        return false;
    };
    if post.is_empty() || baseline.is_empty() {
        return false;
    }
    if post == target_workspace {
        return false;
    }
    if post == baseline {
        return false;
    }
    if attempt.pre.as_deref() == Some(post) {
        return false;
    }
    true
}

impl WorldEngine {
    /// Settle retained view targets against raw echoes before staging.
    /// Confirms move core views to the proven logical id and record
    /// history; externally moved echoes (differing from both the
    /// expectation and the last echo) are newer input and win: the flight
    /// drops and the echo is adopted as the view. Stale echoes hold.
    /// Raw refusal (explicit refusal with an unchanged readable preimage)
    /// or raw third-location proof settles to observed; any other raw
    /// attempt latches ambiguity without re-actuation (the adapter's spent
    /// view-phase fence blocks repeats). Paired migration refill children
    /// (`{parent}-refill` with a live migration parent) are skipped here
    /// and settled with their parent in [`WorldEngine::settle_migration`].
    /// Runs before [`WorldEngine::build_staged`] so homing assigns against
    /// adapter truth, never a stale desire.
    pub(crate) fn settle_views_pre(&mut self, report: &WorldReport) {
        // Migration parents live in the same vec: refill children with a
        // live parent settle with the parent, never alone.
        let parent_ops: std::collections::BTreeSet<String> = self
            .retained
            .iter()
            .filter(|retained| matches!(&retained.target.kind, TransferKind::Migration))
            .map(|retained| retained.target.operation_id.clone())
            .collect();
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for mut retained in std::mem::take(&mut self.retained) {
            if !matches!(&retained.target.kind, TransferKind::View) {
                keep.push(retained);
                continue;
            }
            if let Some(prefix) = retained.target.operation_id.strip_suffix("-refill")
                && parent_ops.contains(prefix)
            {
                keep.push(retained);
                continue;
            }
            // Stage exact-correlated raw evidence before any hold so a
            // gapped report never erases knowledge the sequencer pending
            // map forgets after acknowledgement.
            if let Some(attempt) = view_attempt_for(
                report,
                &retained.target.operation_id,
                &retained.target.target_output,
                &retained.target.target_workspace,
            ) {
                retained.pending_view = Some(attempt.clone());
            }
            let raw = retained.pending_view.clone();
            // Proof echo: the explicit logical echo, else the
            // native-derived echo under the capability. A conflicting
            // pair holds (neither proves); unknown holds.
            let echo = self.view_echo_for(report, &retained.target.target_output);
            let echo = echo.as_deref();
            if echo == Some(retained.target.target_workspace.as_str()) {
                // Confirmed by echo: adopt as the core view.
                self.views.insert(
                    retained.target.target_output.0.clone(),
                    retained.target.target_workspace.clone(),
                );
                let ring = self
                    .mapping
                    .get(&retained.target.target_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.history.observe(
                    &retained.target.target_output.0,
                    &retained.target.target_workspace,
                    &ring,
                );
            } else if raw
                .as_ref()
                .is_some_and(|attempt| view_refused(attempt, &retained.target.target_workspace))
            {
                // Proven refusal reconciled against readable evidence, never
                // from raw alone across missing facts: target exact proof
                // already won above; missing/gapped echoes hold with the raw
                // staged for later reconciliation; only a readable echo
                // naming the refused preimage settles to observed. Gaps
                // (incomplete inventory, incomplete screen, gesture) hold
                // persistently with identities preserved and no retry. The
                // screen bar reads the CURRENT report screens (never the
                // previous staged topology): an incomplete current screen
                // holds even after a complete prior.
                let current_complete = report
                    .screens
                    .iter()
                    .find(|screen| screen.output == retained.target.target_output)
                    .map(|screen| screen.complete);
                if !report.inventory_complete
                    || current_complete != Some(true)
                    || report.gesture_barrier
                {
                    keep.push(retained);
                    continue;
                }
                let pre_matches = raw.as_ref().and_then(|attempt| attempt.pre.clone());
                if echo.is_some() && echo == pre_matches.as_deref() {
                    // Settle to observed: the refused preimage survived.
                } else {
                    keep.push(retained);
                }
            } else if raw.as_ref().is_some_and(|attempt| {
                view_third_location(attempt, &retained.target.target_workspace)
                    && attempt.post.as_deref().is_some_and(|post| {
                        self.mapping
                            .get(&retained.target.target_output.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == post))
                    })
            }) {
                // Newer external input proven by the raw post: settle to
                // observed instead of fighting the user.
            } else {
                let last = self
                    .last_views
                    .get(&retained.target.target_output)
                    .map(String::as_str);
                let Some(echo) = echo else {
                    // Missing/empty echo is a gap, never external
                    // supersession and never a ring-head revert: hold the
                    // flight with the retained view intact. Raw evidence
                    // stays staged above for later reconciliation.
                    keep.push(retained);
                    continue;
                };
                if Some(echo) != last {
                    // External view input supersedes: adopt it when it
                    // names a known logical workspace. Unknown ids hold
                    // (gap, never external proof); stale echoes hold.
                    if self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == echo))
                    {
                        self.views
                            .insert(retained.target.target_output.0.clone(), echo.to_owned());
                        let ring = self
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        self.history
                            .observe(&retained.target.target_output.0, echo, &ring);
                    } else {
                        keep.push(retained);
                    }
                } else {
                    // Stale echo holds; retained raw evidence (if any)
                    // latches ambiguity without re-actuation.
                    keep.push(retained);
                }
            }
        }
        self.retained = keep;
    }

    /// Settle outstanding window and migration transfers plus explicit
    /// float placements against the staged homing, raw echoes, and raw
    /// correlated attempt facts. Runs after [`WorldEngine::build_staged`]
    /// and before the sweep.
    pub(crate) fn settle_rest(&mut self, report: &WorldReport, consumed: bool) {
        // Directional maximize-clears settle first so a proven clear
        // resumes its original intent (tiled engine move or retained float
        // half) in this same pass: the sweep and publish below then apply
        // the continuation coherently in one reply.
        self.settle_clears(report, consumed);
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for mut retained in std::mem::take(&mut self.retained) {
            // Stage exact-correlated raw membership evidence before any
            // hold so gapped reports never erase knowledge the sequencer
            // pending map forgets after acknowledgement. Window follow
            // phases stage their view evidence here too (as migration
            // does): the follow must persist across acknowledgement and
            // gaps, never silently settle on membership alone.
            if let TransferKind::Window(window) = &retained.target.kind {
                let target_output = retained.target.target_output.clone();
                let target_workspace = retained.target.target_workspace.clone();
                if let Some(attempt) = member_attempt_for(
                    report,
                    &retained.target.operation_id,
                    window,
                    &target_output,
                    &target_workspace,
                ) {
                    retained.pending_member = Some(attempt.clone());
                }
                if retained.target.follow
                    && let Some(attempt) = view_attempt_for(
                        report,
                        &retained.target.operation_id,
                        &target_output,
                        &target_workspace,
                    )
                {
                    retained.pending_view = Some(attempt.clone());
                }
            }
            let raw = retained.pending_member.clone();
            let raw_refused = match &retained.target.kind {
                TransferKind::Window(_) => raw.as_ref().is_some_and(|attempt| {
                    member_refused(
                        attempt,
                        &retained.target.target_output,
                        &retained.target.target_workspace,
                    )
                }),
                _ => false,
            };
            match &retained.target.kind {
                TransferKind::Window(window) => {
                    let homing = self.staged_homing(window);
                    let closed = self.staged.closes.contains(window);
                    let source_key = DomainKey {
                        output: retained.source_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.source_workspace.clone(),
                        ),
                    };
                    let target_key = DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    };
                    // Ambiguous raw evidence (any correlated attempt that
                    // is neither proof nor proven refusal) holds the
                    // target without re-actuation; no raw at all holds
                    // plainly so a later report can still dispatch.
                    let ambiguous = raw.is_some() && !raw_refused;
                    if homing.as_ref() == Some(&target_key) {
                        // Target observation wins over stale refusal: arrival
                        // proves before any raw refusal reconciles.
                        // Explicit close settles even on arrival proof.
                        if closed {
                            // Explicit close settles; the sweep forgets it.
                        } else {
                            // Confirmed by staged homing -- except same-output
                            // transfers, where the committed Engine session
                            // already homes at the target before the native
                            // moves. Retained session homing must not confirm
                            // itself: a same-output send settles only on
                            // observed logical membership naming the exact
                            // target workspace (unknown or wrong membership
                            // holds the flight).
                            let same_output =
                                retained.source_output == retained.target.target_output;
                            // Exact native receipt feeds the same proof lane:
                            // under the capability the native-derived logical
                            // receipt (not the raw observed field) decides,
                            // without bypassing at-most-once/refusal/unknown
                            // handling around it.
                            let observed_proves = self
                                .staged
                                .facts
                                .get(window)
                                .and_then(|fact| {
                                    if report.host_owned_workspace_domains {
                                        self.effective_logical_receipt(report, fact)
                                    } else {
                                        fact.observed_workspace.clone()
                                    }
                                })
                                .as_deref()
                                == Some(retained.target.target_workspace.as_str());
                            if same_output && !observed_proves {
                                let mut held = retained;
                                if ambiguous {
                                    held.target.held = true;
                                }
                                keep.push(held);
                            } else if retained.target.follow && !same_output {
                                // Cross-output follow only: same-output
                                // membership settles before its separate
                                // `{op}-follow` view flight (membership-
                                // before-follow), so the Window never waits
                                // here; cross-output follow shares the parent
                                // operation and must prove the exact view.
                                let echo_exact = self
                                    .staged
                                    .echoes
                                    .get(&retained.target.target_output.0)
                                    .map(String::as_str)
                                    == Some(retained.target.target_workspace.as_str());
                                if echo_exact {
                                    // Follow proves on the exact observed
                                    // view: no new write is required when the
                                    // view already shows the target.
                                } else if retained.pending_view.as_ref().is_some_and(|attempt| {
                                    view_third_location(attempt, &retained.target.target_workspace)
                                        && attempt.post.as_deref().is_some_and(|post| {
                                            self.mapping
                                                .get(&retained.target.target_output.0)
                                                .is_some_and(|ring| {
                                                    ring.iter().any(|id| id == post)
                                                })
                                        })
                                }) {
                                    // Genuine known third-location external
                                    // supersession settles the parent instead
                                    // of fighting newer input.
                                } else {
                                    // Membership proves but the follow view
                                    // is absent/stale/failed/ambiguous: hold
                                    // persistently across acknowledgement and
                                    // gaps with the staged follow evidence
                                    // retained (never a refused window, never
                                    // a view outcome on the window id). The
                                    // spent view-phase fence blocks repeats;
                                    // the next report settles only on exact
                                    // echo or genuine external proof.
                                    let mut held = retained;
                                    held.target.held = true;
                                    keep.push(held);
                                }
                            }
                        }
                        // Otherwise confirmed: the flight drops.
                    } else if closed {
                        // Explicit close settles; the sweep forgets the slot.
                    } else if raw_refused {
                        // Proven refusal reconciles only against current
                        // readable complete source-unchanged proof: raw alone
                        // across missing facts never restores/deletes state.
                        // Target arrival already won above; gaps hold with
                        // the raw staged (as floats do), never settling from
                        // absence. Minimized/unreadable facts hold.
                        let gapped = !report.inventory_complete
                            || self.screen_complete(&retained.source_output) != Some(true)
                            || self.screen_complete(&retained.target.target_output) != Some(true)
                            || report.gesture_barrier;
                        if gapped {
                            let mut held = retained;
                            held.target.held = true;
                            keep.push(held);
                        } else {
                            let fact_ok = self.staged.facts.get(window).is_some_and(|fact| {
                                fact.rect.is_some()
                                    && !fact.minimized
                                    && !self.staged.closes.contains(window)
                            });
                            let pre_key = raw.as_ref().and_then(|attempt| {
                                attempt.pre.as_ref().map(|(out, ws)| DomainKey {
                                    output: out.clone(),
                                    workspace: crate::directional::WorkspaceId(ws.clone()),
                                })
                            });
                            if fact_ok && pre_key.is_some_and(|key| Some(&key) == homing.as_ref()) {
                                // Settle to observed: the refused preimage
                                // survived unchanged on readable evidence.
                                // Deferred until after the sweep: the
                                // unshielded sweep may re-store the slot
                                // while converging back to observed.
                                self.pending_forget.push((
                                    DomainKey {
                                        output: retained.target.target_output.clone(),
                                        workspace: crate::directional::WorkspaceId(
                                            retained.target.target_workspace.clone(),
                                        ),
                                    },
                                    window.clone(),
                                ));
                            } else {
                                // Missing/gapped echo, unreadable/minimized
                                // fact, or moved preimage: hold with the raw
                                // retained for later readable reconciliation.
                                let mut held = retained;
                                held.target.held = true;
                                keep.push(held);
                            }
                        }
                    } else if let Some(key) = homing {
                        if key != source_key {
                            // Newer external input off both transfer screens
                            // wins; a contradictory receipt on either screen
                            // holds (never proves arrival, never fought).
                            // Hotplug policy intents are the exception: the
                            // screen vanished under the member (no newer
                            // user input), so the intent holds instead of
                            // dropping, and the outstanding flight keeps the
                            // member homed until arrival proves it.
                            if retained.policy {
                                keep.push(retained);
                            } else if key.output == target_key.output
                                || key.output == source_key.output
                            {
                                let mut held = retained;
                                if ambiguous {
                                    held.target.held = true;
                                }
                                keep.push(held);
                            }
                        } else if ambiguous {
                            let mut held = retained;
                            held.target.held = true;
                            keep.push(held);
                        } else {
                            keep.push(retained);
                        }
                    } else if ambiguous {
                        let mut held = retained;
                        held.target.held = true;
                        keep.push(held);
                    } else {
                        // Absent everywhere and not closed: never infer a
                        // close; retain across the gap.
                        keep.push(retained);
                    }
                }
                TransferKind::View => {
                    // Retained by the pre-stage view pass only when the
                    // echo is stale (refill children settle with their
                    // migration parent); nothing left to do here.
                    keep.push(retained);
                }
                TransferKind::Migration => {
                    self.settle_migration(report, retained, &mut keep);
                }
            }
        }
        self.retained = keep;
        self.settle_floats(report);
    }

    /// Settle one outstanding whole-workspace migration against staged
    /// member homing plus both view proofs (no premature settlement):
    /// every surviving member is at the target key, externally superseded,
    /// or explicitly closed; the target output echoes the destination
    /// logical id; and the source output echoes the stored refill (when one
    /// was selected). Each member proves its own arrival: homed at the
    /// target key settles it into the surviving set, an explicit close
    /// drops it in either direction, and a homing at neither source nor
    /// target drops it as newer external input (never fought). Members
    /// still homed at the source (delayed arrival) or absent everywhere
    /// without a close (partial/unreadable arrival) stay outstanding with
    /// arrived members preserved, and so does a migration whose members
    /// arrived but either view is still old: the transfer is never settled
    /// prematurely and never re-actuated. The expected refill rides the
    /// parent (never the retained refill child), so refill-child settlement
    /// before the parent never loses the proof. An originally-empty
    /// migration still verifies both views exactly like arrived ones; a
    /// non-empty migration whose members all externally superseded/closed
    /// settles to observed without fighting either view. A target view
    /// that moved externally (echo differs from both the expectation and
    /// the last echo) settles the parent instead of fighting newer input,
    /// and a settled parent never reactivates. Proven refusal (every member
    /// refused with an unchanged readable preimage) or external
    /// supersession (third-location view proof) derived from raw attempt
    /// facts latches the terminal outcome under the operation identity;
    /// the retained logical mapping persists (core authority), while
    /// ambiguous attempts hold the flight without re-actuation.
    pub(crate) fn settle_migration(
        &mut self,
        report: &WorldReport,
        mut retained: RetainedTransfer,
        keep: &mut Vec<RetainedTransfer>,
    ) {
        // Stage exact-correlated raw evidence before any hold so gapped
        // reports never erase knowledge the sequencer pending maps forget
        // after acknowledgement.
        let target_out = retained.target.target_output.clone();
        let target_ws = retained.target.target_workspace.clone();
        let op = retained.target.operation_id.clone();
        for member in retained.members.clone() {
            if let Some(attempt) = member_attempt_for(report, &op, &member, &target_out, &target_ws)
            {
                retained.pending_members.insert(member, attempt.clone());
            }
        }
        retained
            .pending_members
            .retain(|member, _| retained.members.contains(member));
        if let Some(attempt) = view_attempt_for(report, &op, &target_out, &target_ws) {
            retained.pending_view = Some(attempt.clone());
        }
        // Settle the paired refill child here: settle_views_pre skips
        // `{parent}-refill` children with a live parent so the two phases
        // reconcile together. An orphan child (no live parent) is never
        // passed here.
        let refill_child_op = format!("{op}-refill");
        if let Some(pos) = keep.iter().position(|r| {
            matches!(&r.target.kind, TransferKind::View) && r.target.operation_id == refill_child_op
        }) {
            let child_out = keep[pos].target.target_output.clone();
            let child_ws = keep[pos].target.target_workspace.clone();
            if let Some(attempt) = view_attempt_for(report, &refill_child_op, &child_out, &child_ws)
            {
                keep[pos].pending_view = Some(attempt.clone());
            }
            let child_raw = keep[pos].pending_view.clone();
            let echo = self.staged.echoes.get(&child_out.0).map(String::as_str);
            if echo == Some(child_ws.as_str()) {
                keep.remove(pos);
            } else if child_raw
                .as_ref()
                .is_some_and(|attempt| view_refused(attempt, &child_ws))
            {
                // A refused refill never retries: the spent view-phase fence
                // blocks repeats, so mere child absence can never itself
                // prove external. The parent holds below while the child is
                // present; a later exact refill echo settles the child. A
                // later genuine external source move (a known current echo
                // naming a different workspace than both the refill and the
                // refused preimage) settles the parent instead of holding
                // forever: drop the child so the source-echo check below
                // reconciles as external. Unknown/gap echoes hold.
                let pre = child_raw.as_ref().and_then(|attempt| attempt.pre.clone());
                let known_third = echo.is_some_and(|current| {
                    current != child_ws.as_str()
                        && Some(current) != pre.as_deref()
                        && self
                            .mapping
                            .get(&child_out.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == current))
                });
                if known_third {
                    keep.remove(pos);
                } else {
                    keep[pos].target.held = true;
                }
            } else if child_raw.as_ref().is_some_and(|attempt| {
                view_third_location(attempt, &child_ws)
                    && attempt.post.as_deref().is_some_and(|post| {
                        self.mapping
                            .get(&child_out.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == post))
                    })
            }) {
                // Externally moved refill: drop the child; the parent
                // reconciles against the source echo below.
                keep.remove(pos);
            } else {
                // True lag (views never proven) or ambiguous evidence:
                // hold without re-actuation. The spent view-phase fence
                // blocks repeats after a dispatch; nothing here marks the
                // child held, so an undispatched refill still retries on a
                // later report (a held flag would deadlock the only
                // actuation path, since the core never clears it).
            }
        }
        // Latch the terminal outcome under the operation identity so it
        // survives acknowledgement: `held` alone cannot distinguish
        // ambiguous attempts from proven refusal/supersession. Only
        // unanimous member refusal and third-location target-view proof
        // latch; later readable evidence reconciles against the outcome.
        // Ambiguity never latches and never rolls back.
        let live_members: Vec<WindowId> = retained
            .members
            .iter()
            .filter(|m| !self.staged.closes.contains(m))
            .cloned()
            .collect();
        if !live_members.is_empty()
            && live_members.iter().all(|member| {
                retained
                    .pending_members
                    .get(member)
                    .is_some_and(|attempt| member_refused(attempt, &target_out, &target_ws))
            })
        {
            retained.migration_terminal = Some(AttemptOutcome::RefusedProven);
        } else if retained.pending_view.as_ref().is_some_and(|attempt| {
            view_third_location(attempt, &target_ws)
                && attempt.post.as_deref().is_some_and(|post| {
                    self.mapping
                        .get(&target_out.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == post))
                })
                // Target observation wins over stale third-location proof:
                // an exact target echo confirms instead of latching external
                // supersession.
                && self
                    .staged
                    .echoes
                    .get(&target_out.0)
                    .map(String::as_str)
                    != Some(target_ws.as_str())
        }) && retained.migration_terminal.is_none()
        {
            retained.migration_terminal = Some(AttemptOutcome::SupersededExternal);
        }
        let terminal = retained.migration_terminal;
        // Paired refill-child presence (child pushed before the parent, so a
        // held child is already in `keep`; a settled child is gone). Computed
        // before the `hold` closure borrows `keep`. Any correlated attempt
        // that is neither proof nor proven refusal holds without
        // re-actuation.
        let refill_child_held = keep.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::View) && r.target.operation_id == refill_child_op
        });
        let ambiguous = !retained.pending_members.is_empty() || retained.pending_view.is_some();
        let mut hold = |mut held: RetainedTransfer| {
            if ambiguous {
                held.target.held = true;
            }
            keep.push(held);
        };
        if matches!(
            terminal,
            Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
        ) {
            // Reconcile to observed evidence instead of holding the target
            // desires forever: classify arrivals from staged homing. Gap
            // bars apply first: never fabricate wholly-unarrived rollback
            // from absence. Explicit closes still settle across gaps.
            let target_key = DomainKey {
                output: retained.target.target_output.clone(),
                workspace: crate::directional::WorkspaceId(
                    retained.target.target_workspace.clone(),
                ),
            };
            let source_key = DomainKey {
                output: retained.source_output.clone(),
                workspace: crate::directional::WorkspaceId(retained.source_workspace.clone()),
            };
            let pruned: Vec<WindowId> = retained
                .members
                .iter()
                .filter(|m| !self.staged.closes.contains(m))
                .cloned()
                .collect();
            if pruned.is_empty()
                && !retained.migration_initial_empty
                && !retained.members.is_empty()
            {
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
            // Gap bars: partial/unreadable inventory, incomplete screens,
            // or a held gesture never prove non-arrival. Hold narrowly
            // with identities preserved, marked held persistently so the
            // flight survives the refusal acknowledgement disappearing.
            if !report.inventory_complete {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if self.screen_complete(&retained.source_output) != Some(true)
                || self.screen_complete(&retained.target.target_output) != Some(true)
            {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if report.gesture_barrier {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            let unreadable_member = pruned.iter().any(|m| {
                self.staged.facts.get(m).is_some_and(|f| {
                    f.rect.is_none() && !f.minimized && !self.staged.closes.contains(m)
                })
            });
            if unreadable_member {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            // Classify only genuine evidence: arrived at the target,
            // proven at the source, or gap (absent/unreadable/unknown or
            // contradictory receipts that never prove arrival).
            let mut arrived = 0usize;
            let mut proven = 0usize;
            let mut gap = 0usize;
            for member in &pruned {
                match self.staged_homing(member) {
                    Some(key) if key == target_key => arrived += 1,
                    Some(key) if key == source_key => proven += 1,
                    Some(_) | None => gap += 1,
                }
            }
            if gap > 0 {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived > 0 && proven > 0 {
                // Partial ambiguity: hold narrowly with identities
                // preserved, never fabricated rollback or proof. Marked
                // held persistently so the adapter never retries after the
                // refusal attempt row is acknowledged.
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived == 0 && proven == 0 && !pruned.is_empty() {
                // No genuine evidence either way (should be unreachable
                // after the gap bars, but never roll back from it).
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            if arrived == 0 && !retained.migration_initial_empty && !pruned.is_empty() {
                // Wholly proven at the source: roll the retained logical
                // unit back with stable identities, no reissued token. Gate
                // on hostability first: if the source screen cannot host
                // the session, hold narrowly through the gap instead of a
                // destructive best-effort fallback that would delete the
                // target session and reseed the tree. Tree/shares/focus
                // ride the rekey only when the relocate preserves them.
                let ws = retained.target.target_workspace.clone();
                let rollback_source = DomainKey {
                    output: retained.source_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let rollback_target = DomainKey {
                    output: retained.target.target_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                if !self.can_relocate_session_for_move(&rollback_target, &rollback_source) {
                    let mut held = retained;
                    held.members = pruned;
                    held.target.held = true;
                    keep.push(held);
                    return;
                }
                if let Some(ring) = self.mapping.get_mut(&retained.target.target_output.0) {
                    ring.retain(|id| id != &ws);
                }
                {
                    let source_ring = self
                        .mapping
                        .entry(retained.source_output.0.clone())
                        .or_default();
                    if !source_ring.iter().any(|id| id == &ws) {
                        let at = retained
                            .migration_source_index
                            .unwrap_or(source_ring.len())
                            .min(source_ring.len());
                        source_ring.insert(at, ws.clone());
                    }
                }
                self.carry_tiling(
                    &ws,
                    &retained.target.target_output.0,
                    &retained.source_output.0,
                );
                self.rekey_members(
                    &ws,
                    &retained.target.target_output.0,
                    &retained.source_output.0,
                );
                // Session rollback preserves the original unit: relocate
                // the committed target session back to the source geometry.
                // Hostability was gated above, so this preserves
                // tree/shares/focus; a failed relocate here leaves state
                // exactly as validated.
                let source_key = DomainKey {
                    output: retained.source_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let target_key = DomainKey {
                    output: retained.target.target_output.clone(),
                    workspace: crate::directional::WorkspaceId(ws.clone()),
                };
                let relocated = self.relocate_session_for_move(&target_key, &source_key);
                debug_assert!(relocated, "rollback gated on hostability");
                let source_view = retained
                    .migration_source_view
                    .clone()
                    .filter(|view| {
                        self.mapping
                            .get(&retained.source_output.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == view))
                    })
                    .unwrap_or_else(|| ws.clone());
                self.views
                    .insert(retained.source_output.0.clone(), source_view.clone());
                if let Some(prev_target) = retained.migration_target_view.clone() {
                    let known = self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == &prev_target));
                    if known {
                        self.views
                            .insert(retained.target.target_output.0.clone(), prev_target);
                    } else {
                        let target_ring = self
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        if let Some(head) = target_ring.first().cloned() {
                            self.views
                                .insert(retained.target.target_output.0.clone(), head);
                        } else {
                            self.views.remove(&retained.target.target_output.0);
                        }
                    }
                } else {
                    let target_ring = self
                        .mapping
                        .get(&retained.target.target_output.0)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(head) = target_ring.first().cloned() {
                        self.views
                            .insert(retained.target.target_output.0.clone(), head);
                    } else {
                        self.views.remove(&retained.target.target_output.0);
                    }
                }
                let source_ring = self
                    .mapping
                    .get(&retained.source_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.history
                    .observe(&retained.source_output.0, &source_view, &source_ring);
            }
            // Settle to observed; the retained logical mapping persists
            // (rolled back above for the wholly-unarrived case).
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        // Fresh-report bar: partial inventory, an incomplete screen on
        // either output, or a held scope at either key holds the flight
        // with identities preserved, never settles from a gap. Explicit
        // closes are pruned even across gaps so a fully-closed migration
        // settles without waiting for views that will never arrive.
        let pruned: Vec<WindowId> = retained
            .members
            .iter()
            .filter(|m| !self.staged.closes.contains(m))
            .cloned()
            .collect();
        if pruned.is_empty() && !retained.migration_initial_empty && !retained.members.is_empty() {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        if !report.inventory_complete {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        let source_key = DomainKey {
            output: retained.source_output.clone(),
            workspace: crate::directional::WorkspaceId(retained.source_workspace.clone()),
        };
        let target_key = DomainKey {
            output: retained.target.target_output.clone(),
            workspace: crate::directional::WorkspaceId(retained.target.target_workspace.clone()),
        };
        if self.screen_complete(&retained.source_output) != Some(true)
            || self.screen_complete(&retained.target.target_output) != Some(true)
        {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        if report.gesture_barrier {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        // Unreadable member frames hold narrowly: partial/unreadable arrival
        // stays outstanding with arrived members preserved.
        let unreadable_member = pruned.iter().any(|m| {
            self.staged.facts.get(m).is_some_and(|f| {
                f.rect.is_none() && !f.minimized && !self.staged.closes.contains(m)
            })
        });
        if unreadable_member {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        let mut outstanding = Vec::new();
        let mut surviving = Vec::new();
        for member in &pruned {
            if self.staged.closes.contains(member) {
                continue;
            }
            match self.staged_homing(member) {
                Some(key) if key == target_key => {
                    surviving.push(member.clone());
                }
                Some(key) if key == source_key => {
                    outstanding.push(member.clone());
                    surviving.push(member.clone());
                }
                Some(key) => {
                    // Contradictory receipts on either transfer screen hold
                    // (never arrival proof); truly off-screen moves drop as
                    // newer input without a fight.
                    if key.output == target_key.output || key.output == source_key.output {
                        outstanding.push(member.clone());
                        surviving.push(member.clone());
                    }
                }
                None => {
                    outstanding.push(member.clone());
                    surviving.push(member.clone());
                }
            }
        }
        surviving.sort();
        // Deterministic outstanding order for stable retained state.
        outstanding.sort();
        if !outstanding.is_empty() {
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        // No outstanding members: all surviving arrived, all settled
        // externally/closed, or originally empty. A non-empty migration with
        // no survivors settled every member through newer input or close:
        // settle to observed without fighting either view.
        if surviving.is_empty() && !retained.migration_initial_empty {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.operation_id == refill_child_op)
            });
            return;
        }
        // Both views: the adapter actuates the destination follow and the
        // stored source refill; settlement waits for each echo proof. Empty
        // migrations verify both views exactly like arrived ones. The
        // expected refill rides the parent, so refill-child settlement
        // before the parent never loses the proof: when the child is gone
        // (settled) but the source echo is not the refill, the source moved
        // externally and the parent settles instead of holding forever
        // waiting for a refill that will never be re-actuated (no
        // reactivation, never fighting newer input). When the child is still
        // held (true lag, views never proven), the parent holds and waits.
        // A target echo that moved externally (differs from both the
        // expectation and the last echo) settles instead of fighting.
        if let Some(refill) = retained.migration_refill.as_deref() {
            let source_echo = self
                .staged
                .echoes
                .get(&retained.source_output.0)
                .map(String::as_str);
            if source_echo.is_none() {
                // Missing echo is a gap: hold, never settle as external.
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            if !refill_child_held && source_echo != Some(refill) {
                // Unknown source echoes hold (gap); only a known different
                // logical id proves external movement.
                let known = self
                    .mapping
                    .get(&retained.source_output.0)
                    .is_some_and(|ring| {
                        source_echo.is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
        }
        let target_echo = self
            .staged
            .echoes
            .get(&retained.target.target_output.0)
            .map(String::as_str);
        if target_echo != Some(retained.target.target_workspace.as_str()) {
            // Missing echo is a gap: hold, never external supersession and
            // never a ring-head revert.
            if target_echo.is_none() {
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            let last = self
                .last_views
                .get(&retained.target.target_output)
                .map(String::as_str);
            if target_echo != last {
                // Externally moved target view settles, but unknown ids
                // hold (gap, never external proof).
                let known = self
                    .mapping
                    .get(&retained.target.target_output.0)
                    .is_some_and(|ring| {
                        target_echo.is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                // Externally moved target view: settle, drop the refill child.
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.target.operation_id == refill_child_op)
                });
                return;
            }
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        if let Some(refill) = retained.migration_refill.as_deref()
            && self
                .staged
                .echoes
                .get(&retained.source_output.0)
                .map(String::as_str)
                != Some(refill)
        {
            let mut held = retained;
            held.members = surviving;
            hold(held);
            return;
        }
        // Settled: record both views in history and drop the refill child.
        let target_ring = self
            .mapping
            .get(&retained.target.target_output.0)
            .cloned()
            .unwrap_or_default();
        self.history.observe(
            &retained.target.target_output.0,
            &retained.target.target_workspace,
            &target_ring,
        );
        if let Some(refill) = retained.migration_refill.as_deref() {
            let source_ring = self
                .mapping
                .get(&retained.source_output.0)
                .cloned()
                .unwrap_or_default();
            self.history
                .observe(&retained.source_output.0, refill, &source_ring);
        }
        keep.retain(|r| {
            !(matches!(&r.target.kind, TransferKind::View)
                && r.target.operation_id == refill_child_op)
        });
    }

    /// Settle outstanding explicit float-placement targets against
    /// operation-correlated raw frame attempts plus the staged observation.
    /// The adapter reports mechanical write facts only (setter disposition,
    /// immediate pre-write and post-write frames, DPI pair); the core owns
    /// every verdict here. Correlation is exact: an attempt settles a float
    /// only when operation id, window, and target all match the retained
    /// target. Window/geometry-only matches never count.
    ///
    /// Evidence stages BEFORE any readability or authority hold: a gapped
    /// report (incomplete inventory, gesture barrier, minimized or
    /// unreadable fact) never erases knowledge the shared consumed-frames
    /// watermark forgets after acknowledgement. The latest fresh attempt
    /// rides under the operation as [`RetainedFloat::pending`] until
    /// settlement, close, replacement, or reset, and reconciles against
    /// later readable complete observations with no replay from either
    /// side (a spent operation is never re-dispatched or reissued).
    ///
    /// - Confirmation needs the staged observation at the target (native
    ///   truth, never setter success). A correlated readback at the target
    ///   corroborates, but the observation decides.
    /// - Explicit refusal needs a readable preimage that survived the
    ///   refused setter unchanged (readback equals prewrite, both readable,
    ///   prewrite differs from the target, staged fact still names the
    ///   preimage). Refusal evidence received during a gap is preserved
    ///   and reconciles once a readable preimage arrives after the
    ///   acknowledgement, instead of degrading into a permanent ambiguous
    ///   hold.
    /// - Anything else fresh and correlated latches no-reactuation
    ///   ambiguity under the operation identity: accepted clamp/mismatch,
    ///   throw, unreadable proof, and DPI-invalidated writes all hold
    ///   without re-actuation. A DPI transition never confirms and never
    ///   retries; it holds exactly like any other ambiguous write, and a
    ///   fresh transition additionally suspends same-report confirmation
    ///   until the next stable observation.
    /// - With no fresh correlated evidence the observation still decides,
    ///   but only on truthful readable complete data: at-target confirms,
    ///   an ambiguous latch holds (an accepted clamp never masquerades as
    ///   external supersession), and a frame matching neither the
    ///   command-time source nor the target proves newer native input and
    ///   drops the stale target without a fight.
    /// - Explicit closes always settle, even across gaps; absent facts hold
    ///   across the gap (never an inferred close). Incomplete inventory and
    ///   gesture barriers hold every flight without settling.
    /// - Every terminal verdict above (confirmation, proven refusal,
    ///   external supersession, close) emits one `operation_settled` event
    ///   with the original op/window/outcome; holds emit nothing.
    pub(crate) fn settle_floats(&mut self, report: &WorldReport) {
        use crate::world_frames::{dpi_basis_changed, is_stale_replay};
        use crate::world_report::{FrameSetter, OperationOutcome};
        let mut keep: Vec<RetainedFloat> = Vec::new();
        for mut retained in std::mem::take(&mut self.retained_floats) {
            if self.staged.closes.contains(&retained.window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::FloatAbandoned,
                    Some("closed"),
                );
                continue;
            }
            // Stage exact-correlated evidence first: a gap hold below must
            // never erase what the shared consumed watermark will forget.
            // At most one attempt per window rides a report, so the latest
            // fresh evidence simply replaces the retained copy. A DPI
            // transition on fresh evidence suspends same-report verdicts
            // (the write straddles coordinate bases); the evidence still
            // latches below and reconciles from later stable observations.
            let mut fresh_evidence = false;
            let mut fresh_dpi_flip = false;
            for attempt in report.frames.iter().filter(|attempt| {
                attempt.window == retained.window
                    && attempt.target == retained.target
                    && attempt.operation_id.as_deref() == Some(retained.operation_id.as_str())
            }) {
                if self
                    .consumed_frames
                    .get(&retained.window)
                    .is_some_and(|prior| is_stale_replay(prior, attempt))
                {
                    continue;
                }
                if dpi_basis_changed(attempt.dpi_before, attempt.dpi_after) {
                    fresh_dpi_flip = true;
                }
                retained.pending = Some(attempt.clone());
                fresh_evidence = true;
            }
            // Every verdict below except close needs truthful readable
            // complete data: complete inventory, no gesture barrier, and a
            // non-minimized readable fact. Anything less holds without
            // settling, but latches conservatively from retained evidence
            // so a later readable report reconciles instead of dropping
            // the operation as external input.
            let readable = report.inventory_complete
                && !report.gesture_barrier
                && self
                    .staged
                    .facts
                    .get(&retained.window)
                    .is_some_and(|fact| !fact.minimized && fact.rect.is_some());
            let Some(current) = readable
                .then(|| {
                    self.staged
                        .facts
                        .get(&retained.window)
                        .and_then(|fact| fact.rect)
                })
                .flatten()
            else {
                if retained.pending.is_some() {
                    // Accepted clamp/throw/unreadable/DPI evidence and
                    // not-yet-provable refusal all latch no-reactuation
                    // ambiguity: the spent operation never retries, and
                    // the core never reissues it.
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            };
            if current == retained.target && !fresh_dpi_flip {
                // Confirmed by native observation. A DPI transition on
                // this report's fresh evidence suspends the verdict: the
                // write straddles coordinate bases, so confirmation waits
                // for the next stable observation. Only a readable observed
                // target proves success, never the setter alone.
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::FloatFrameProven,
                    None,
                );
                continue;
            }
            if let Some(pending) = retained.pending.as_ref()
                && !dpi_basis_changed(pending.dpi_before, pending.dpi_after)
                && matches!(pending.setter, FrameSetter::Refused)
                && let (Some(pre), Some(readback)) = (pending.prewrite, pending.readback)
                && pre != retained.target
                && readback == pre
                && current == pre
            {
                // Proven refusal reconciled against readable evidence: the
                // retained refusal preimage survived unchanged, possibly
                // across the acknowledgement gap, on a stable DPI basis.
                // No replay is involved; the stale target simply drops.
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::FloatRefusedProven,
                    None,
                );
                continue;
            }
            if fresh_evidence {
                // Any fresh correlated evidence that is neither confirmation
                // nor proven refusal latches ambiguity: accepted mismatch,
                // throw, unreadable proof, refused-with-moved-preimage, and
                // DPI-invalidated writes all hold without re-actuation.
                retained.ambiguous = true;
                keep.push(retained);
                continue;
            }
            if retained.ambiguous {
                keep.push(retained);
                continue;
            }
            if retained.source.is_some_and(|source| current != source) {
                // Native frame matches neither the command-time
                // source nor the target: newer external input
                // (manual move/resize) wins; the stale target drops
                // rather than fighting it.
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::FloatAbandoned,
                    Some("external"),
                );
            } else {
                keep.push(retained);
            }
        }
        self.retained_floats = keep;
    }
}
