//! Commandless born maximize-clear settlement (N6): one outstanding
//! authorized clear per born window, keyed by a stable core-minted
//! operation identity (`op-born-<n>`), never a command sequence.
//!
//! Split from [`crate::world_settle`]: same `impl WorldEngine` ownership,
//! only relocated for source size. Owns the born binding lifecycle: minting
//! from first-seen maximized/fullscreen facts through the shared stateless
//! [`crate::window_policy`] predicates (never reimplemented inline),
//! publishing the one-shot `clear_operation` desire, and settling over the
//! correlated raw [`ClearAttempt`](crate::world_report::ClearAttempt) plus
//! the staged observation. No behavior change to the directional lane in
//! [`crate::world_clear`].
//!
//! Two lanes, one binding shape:
//! - Born lane: a post-startup first-seen maximized, non-fullscreen window on a
//!   tiled/controlled scope clears once under `tile-on-launch`. The
//!   keep-overlay decision latches per lifetime at tiled-first evaluation
//!   ([`born_keep_latch`](crate::window_policy::born_keep_latch)), so a
//!   later setting flip never reclears a previously kept lifetime.
//! - Held-fullscreen exit lane: a first-seen fullscreen birth whose
//!   fullscreen first exits while still maximized clears once under BOTH
//!   modes, unless the exit reads fixed (then it floats automatic, like a
//!   fixed birth). Later fullscreen on admitted tiles never enters the held
//!   set, and floating-first windows never clear through either lane.
//!
//! Startup fact/held identities keep native maximize through delayed admission;
//! their existing floating/fixed-size and fullscreen gates still take precedence.
//! For later arrivals, birth identity is first evaluable sighting, not session
//! pre-membership:
//! arrivals that cannot be evaluated yet (unhomed, held/unreadable scope,
//! minimized, unreadable rect) mark nothing and decide on the first report
//! that proves scope, so late admission is never defeated by an earlier
//! non-decision. Fixed-size births float untouched; a birth that reads fixed
//! only after minting abandons with no retry. Gaps never gate the decision
//! (matching the retired adapter path, which decided on every accepted
//! reply): settlement and publish still withhold while gapped. A refused
//! setter still records the attempt (no repeats). Never assumes a setter
//! `accepted` means synchronous completion: the correlated pending attempt
//! survives unknown/unchanged reads and only unmaximized facts plus an
//! accepted attempt prove the clear.
//!
//! Boundedness: every lifetime set is pruned on explicit close and on
//! authoritative complete-inventory departure (absent from a complete
//! inventory with no close pending; never incomplete absence, never
//! minimized omission, both of which keep the lifetime homed). A returning
//! or replacement lifetime therefore re-evaluates as first-seen.

use std::collections::BTreeSet;

use crate::directional::WindowId;
use crate::session::DomainKey;
use crate::size_hints::is_fixed_size_with;
use crate::window_policy::{
    FirstExitDisposition, born_keep_latch, classify_born_fullscreen_exit,
    should_clear_maximize_at_admission_with_born_mode, should_hold_born_fullscreen,
};
use crate::world_engine::{RetainedBornClear, WorldEngine};
use crate::world_report::{ClearAttempt, FrameSetter, OperationOutcome, TilingMode};

/// Exact-correlated raw clear evidence for one retained born operation.
/// Forged rows (operation or window mismatch) are never evidence: they
/// read as absent.
fn born_attempt_for<'a>(
    attempts: &'a [ClearAttempt],
    operation_id: &str,
    window: &WindowId,
) -> Option<&'a ClearAttempt> {
    attempts
        .iter()
        .find(|attempt| attempt.operation_id == operation_id && attempt.window == *window)
}

/// Proven clear refusal: an explicit native refusal with an unchanged
/// readable maximized preimage. Anything else (accepted mismatch, throw,
/// unreadable proof) latches ambiguity, never refusal.
fn born_refused(attempt: &ClearAttempt) -> bool {
    matches!(attempt.setter, FrameSetter::Refused)
        && attempt.pre_maximized == Some(true)
        && attempt.post_maximized == Some(true)
}

/// Proven clear dispatch: the setter ran (accepted, possibly completing
/// asynchronously) against a readable maximized preimage. Never proof on
/// its own: the staged observation must also show complete readable
/// unmaximized facts (checked by the caller).
fn born_accepted(attempt: &ClearAttempt) -> bool {
    matches!(attempt.setter, FrameSetter::Accepted) && attempt.pre_maximized == Some(true)
}

impl WorldEngine {
    /// Next born operation identity. Monotonic within the generation, never
    /// reset on adapter-incarnation rotation (like `native_creation_seq`):
    /// the adapter's at-most-once spent map is generation-scoped, so a
    /// reused id could read as already dispatched.
    pub(crate) fn mint_born_operation_id(&mut self) -> String {
        self.born_seq += 1;
        format!("op-born-{}", self.born_seq)
    }

    /// Whether the window's own birth facts authorize a maximize-clear
    /// mode transition on its home domain (N6 tentative property
    /// authority): readable staged capture, tiled scope, and the shared
    /// `window_policy` birth gates proven per window (checked by the
    /// caller). Sweep geometry holds (overlap `Unreadable`, constraint
    /// clamp) do NOT veto: a maximized birth's frames stay
    /// host-controlled until the clear lands, so requiring converged
    /// geometry deadlocks the cohort. Safety still vetoes: game lane
    /// (`presentation.paused`), gesture barrier, unreadable/minimized
    /// per-window facts, sticky/floating/fullscreen flips, fixed facts,
    /// explicit session float intent (ToggleFloat placement, same-report
    /// included), floating or untiled scope, host-owned unknown
    /// membership (unknown native receipt or order conflict), and
    /// migration-gap scopes. Those reuse existing trusted roster/scope
    /// facts; no `SafetyFacts` are invented. KWin lifecycle normalization
    /// closes deleted windows and omits application-handled windows before
    /// they become WindowFacts; no new core controllability field is used.
    fn born_clear_authorized(
        &self,
        presentation_paused: bool,
        window: &WindowId,
        home: &DomainKey,
    ) -> bool {
        if presentation_paused || self.staged.gesture_barrier {
            return false;
        }
        let Some(fact) = self.staged.facts.get(window) else {
            return false;
        };
        let derived = self.derived_for(window);
        // Safety exclusions and held classifications never authorize a
        // born clear: no membership, no geometry.
        if derived.excluded || derived.own_excluded || derived.hold || derived.hidden {
            return false;
        }
        if fact.rect.is_none() {
            return false;
        }
        // A sticky, floating, or fullscreen flip ends automatic birth
        // authority: sticky lifetimes keep their overlay, floats ride
        // session intent, and fullscreen re-arms the held lane through
        // settlement. Fixed births float beneath their overlay. All are
        // existing trusted per-window facts, re-proven every report so a
        // retained desire never outlives its authority. Raw null overlays
        // already hold above via `derived.hold`.
        if derived.sticky || derived.floating || derived.overlay_fullscreen {
            return false;
        }
        if derived.fixed_auto
            || derived.fixed_suppress
            || fact
                .hints
                .is_some_and(|hints| is_fixed_size_with(hints, self.fixed_size_predicate_setting()))
        {
            return false;
        }
        // Explicit session float intent supersedes the automatic birth:
        // the user chose float (same-report ToggleFloat included). A
        // held-fullscreen exit carries no such intent, so the legitimate
        // exception-lane exit still passes here.
        if self
            .retained_floats
            .iter()
            .any(|held| held.window == *window)
        {
            return false;
        }
        let readable = self
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == *home)
            .is_some_and(|domain| domain.readable);
        if !readable {
            return false;
        }
        if self.floating_scopes.contains(home) {
            return false;
        }
        if self.logical_tiling(&home.output.0, &home.workspace.0) != TilingMode::Tiled {
            return false;
        }
        // Host-owned safety: unknown native membership never proves a
        // home, and an order-conflicting projection holds every scope.
        // This mirrors the accept-time fence without consulting the
        // conflated sweep-hold set. Unknown output never authorizes.
        if self.host_owned_workspace_domains() {
            if self.native_order_conflict() {
                return false;
            }
            let authoritative = match fact.output.as_ref() {
                Some(_) => self.native_authoritative(),
                None => true,
            };
            if authoritative && self.effective_logical_receipt(fact).is_none() {
                return false;
            }
        }
        // Migration-gap safety: incomplete inventory with an outstanding
        // migration touching home holds, mirroring the sweep gap fence.
        if !self.staged.observation.inventory_complete
            && self.retained.iter().any(|r| {
                matches!(&r.target.kind, crate::world_report::TransferKind::Migration)
                    && (DomainKey {
                        output: r.source_output.clone(),
                        workspace: crate::directional::WorkspaceId(r.source_workspace.clone()),
                    } == *home
                        || DomainKey {
                            output: r.target.target_output.clone(),
                            workspace: crate::directional::WorkspaceId(
                                r.target.target_workspace.clone(),
                            ),
                        } == *home)
            })
        {
            return false;
        }
        true
    }

    /// One-shot born `clear_operation` desire for one window, if a binding
    /// is outstanding, undispatched, and its home still proves property
    /// authority under the current report. The scope re-proof here covers
    /// command-flipped workspaces in the same reply: settlement ran before
    /// the command, so a toggle to floating in this report must not ride a
    /// stale desire. The per-window re-proof covers same-report fact flips
    /// (sticky/floating/fullscreen/fixed) and session float intent, so a
    /// retained desire never publishes after its authority lapsed.
    pub(crate) fn born_desire_for(
        &self,
        window: &WindowId,
        presentation_paused: bool,
    ) -> Option<String> {
        // Neutral opt-in: without the automatic birth-clear capability no
        // retained binding ever publishes, even if one somehow survived an
        // opt-out or rebind report (mint drops them; this is the publish
        // fence). The explicit directional lane never reads this gate.
        if !self.birth_clear() {
            return None;
        }
        self.retained_born
            .iter()
            .find(|retained| retained.window == *window && retained.pending.is_none())
            .filter(|retained| {
                self.born_clear_authorized(presentation_paused, window, &retained.home)
            })
            .map(|retained| retained.operation_id.clone())
    }

    /// Report-independent born-clear minting for the parallel v4 accepted
    /// run: the same sweep-time admission pass with no game-lane pause (v4
    /// carries no paused lane).
    pub(crate) fn mint_born_clears_v4(
        &mut self,
        pre_known: &BTreeSet<WindowId>,
        pre_slotted: &BTreeSet<WindowId>,
    ) {
        self.mint_born_clears_inner(false, pre_known, pre_slotted);
    }

    fn mint_born_clears_inner(
        &mut self,
        presentation_paused: bool,
        pre_known: &BTreeSet<WindowId>,
        pre_slotted: &BTreeSet<WindowId>,
    ) {
        // Neutral opt-in: without the automatic birth-clear capability no
        // birth behavior mints, tracks, or publishes. Outstanding bindings
        // and lifetime marks drop outright so a rebind or an opt-out report
        // can never leave a publishable token behind; a later opt-in
        // re-evaluates admitted windows as known (never a re-clear). The
        // explicit directional clear lane is untouched (separate retained
        // lane, separate settlement).
        if !self.birth_clear() {
            self.retained_born.clear();
            self.born_attempted.clear();
            self.born_held_fullscreen.clear();
            self.born_floating_first.clear();
            self.born_seen_nonfullscreen.clear();
            self.born_keep_overlay.clear();
            return;
        }
        // Startup adoption: on the first accepted usable complete inventory
        // (`!self.placement_bootstrapped`), seed `born_keep_overlay` with all
        // fact and held identities so pre-existing maximized windows retain
        // their native maximize overlay (and reserved tile slot) even when
        // admission is delayed through held/minimized/unreadable/unhomed
        // states. Fullscreen-born windows still enter `born_held_fullscreen`
        // and clear on first exit (checked before `born_keep_overlay` below).
        if !self.placement_bootstrapped {
            for window in self.staged.facts.keys() {
                self.born_keep_overlay.insert(window.clone());
            }
            for held in &self.staged.held_windows {
                self.born_keep_overlay.insert(held.window.clone());
            }
        }
        // Explicit closes prune every born lifetime set, so id reuse after
        // a close starts a fresh lifetime that may clear again.
        for closed in self.staged.closes.clone() {
            self.prune_born_lifetime(&closed);
            self.retained_born.retain(|held| held.window != closed);
        }
        // Complete-inventory departures prune every born lifetime set (the
        // same authoritative implicit-close signal the session converge
        // uses: absent from a complete inventory with no close pending;
        // never incomplete absence, never minimized omission, both of which
        // keep the lifetime homed). Narrowly held rows are live presence,
        // never a departure; excluded windows never reset identity (only
        // an explicit native close does). A returning lifetime therefore
        // re-evaluates as first-seen. Gapped reports prune nothing.
        if self.staged.observation.inventory_complete && !self.staged.gesture_barrier {
            let held: BTreeSet<&WindowId> = self
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            self.born_attempted
                .retain(|w| self.staged.facts.contains_key(w) || held.contains(w));
            self.born_held_fullscreen
                .retain(|w| self.staged.facts.contains_key(w) || held.contains(w));
            self.born_floating_first
                .retain(|w| self.staged.facts.contains_key(w) || held.contains(w));
            self.born_seen_nonfullscreen
                .retain(|w| self.staged.facts.contains_key(w) || held.contains(w));
            self.born_keep_overlay
                .retain(|w| self.staged.facts.contains_key(w) || held.contains(w));
        }
        // Track non-fullscreen sightings: a later fullscreen on an admitted
        // tile is a managed overlay transition, never a birth. Minimized
        // and unreadable facts carry no sighting either way, so a
        // minimized-then-restored lifetime decides on its first readable
        // report like a first sighting. Unknown fullscreen carries no
        // sighting: only proven `Some(false)` counts.
        for (window, fact) in &self.staged.facts {
            let derived = self.derived_for(window);
            if derived.hidden || derived.hold || derived.excluded || fact.rect.is_none() {
                continue;
            }
            if fact.fullscreen == Some(false) {
                self.born_seen_nonfullscreen.insert(window.clone());
            }
        }
        // Deterministic mint order: sorted window ids.
        let mut ids: Vec<WindowId> = self.staged.facts.keys().cloned().collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        for window in ids {
            let Some(fact) = self.staged.facts.get(&window).cloned() else {
                continue;
            };
            let derived = self.derived_for(&window);
            if derived.hidden || derived.hold || derived.excluded || derived.own_excluded {
                continue;
            }
            if fact.rect.is_none() {
                continue;
            }
            if self.retained_born.iter().any(|held| held.window == window) {
                continue;
            }
            if self.born_attempted.contains(&window) {
                continue;
            }
            let Some(home) = self.staged.homing.get(&window).cloned() else {
                continue;
            };
            // Floating scopes prove floating-first and cancel held markers,
            // so the held exit lane can never bypass floating safety.
            if self.floating_scopes.contains(&home)
                || self.logical_tiling(&home.output.0, &home.workspace.0) != TilingMode::Tiled
            {
                self.born_floating_first.insert(window.clone());
                self.born_held_fullscreen.remove(&window);
                continue;
            }
            if derived.sticky {
                continue;
            }
            // Fullscreen births hold slotless until their first exit; the
            // engine already holds them as floating exceptions with no tile
            // slot. The shared hold gate runs on honest retained
            // slot/lifetime context, and only unadmitted lifetimes enter:
            // later fullscreen on an admitted tile (including a minimized
            // restore, which rides session membership) is a managed overlay
            // transition, never a birth.
            if derived.overlay_fullscreen {
                if !pre_known.contains(&window)
                    && !self.born_floating_first.contains(&window)
                    && should_hold_born_fullscreen(
                        true,
                        pre_slotted.contains(&window),
                        self.born_seen_nonfullscreen.contains(&window),
                    )
                {
                    self.born_held_fullscreen.insert(window);
                }
                continue;
            }
            // A first exit to plain (proven non-maximized) ends the held
            // birth with no clear, so a later user maximize is never
            // mistaken for the held exit. Unknown maximized never proves
            // plain: with maximize support it already holds above; without
            // it, absence is the native-free ordinary state and still
            // counts as plain via the derived overlay.
            let plain_exit = if self
                .capabilities()
                .map(|caps| caps.maximize)
                .unwrap_or(true)
            {
                fact.maximized == Some(false)
            } else {
                !derived.overlay_maximized
            };
            if plain_exit {
                self.born_held_fullscreen.remove(&window);
                continue;
            }
            // From here the window reads maximized (or maximize-unsupported
            // fullscreen-cleared): if maximize is supported, unknown already
            // held above, so only `Some(true)` reaches here.
            if self
                .capabilities()
                .map(|caps| caps.maximize)
                .unwrap_or(true)
                && fact.maximized != Some(true)
            {
                continue;
            }
            // Maximized, non-fullscreen from here. The held-fullscreen exit
            // lane runs first and owns its marker: like the retired adapter
            // path it decides before the born lane's floating/fixed skips,
            // consuming the marker for every outcome below.
            if self.born_held_fullscreen.contains(&window) {
                self.born_held_fullscreen.remove(&window);
                if self.born_floating_first.contains(&window) {
                    continue;
                }
                // A fixed exit floats automatic with no clear, like a fixed
                // birth: the automatic latch, the explicit tile-win
                // suppress signal, and the shared D5 exit disposition on
                // the wire hints must all read non-fixed. Unknown hints
                // already hold above, so `None` never reaches a decision.
                if derived.fixed_auto || derived.fixed_suppress {
                    continue;
                }
                let fixed_exit = fact.hints.is_some_and(|hints| {
                    classify_born_fullscreen_exit(hints, self.fixed_size_predicate_setting())
                        == FirstExitDisposition::FloatAutomatic
                });
                if fixed_exit {
                    continue;
                }
                if !self.born_clear_authorized(presentation_paused, &window, &home) {
                    continue;
                }
                let operation_id = self.mint_born_operation_id();
                self.retained_born.push(RetainedBornClear {
                    operation_id,
                    window,
                    home,
                    pending: None,
                    ambiguous: false,
                });
                continue;
            }
            // Born lane: floating identities and fixed births float beneath
            // their overlay with no clear; only genuine first-seen births
            // proceed. Later maximizes of admitted tiles keep their
            // overlay, and floating-first sightings keep theirs under both
            // modes.
            if derived.floating {
                continue;
            }
            if derived.fixed_suppress
                || derived.fixed_auto
                || fact.hints.is_some_and(|hints| {
                    is_fixed_size_with(hints, self.fixed_size_predicate_setting())
                })
            {
                continue;
            }
            if pre_known.contains(&window) {
                continue;
            }
            if self.born_floating_first.contains(&window) {
                continue;
            }
            if !self.born_clear_authorized(presentation_paused, &window, &home) {
                continue;
            }
            // R-MAX-06 latch at tiled-first evaluation under the live value:
            // keep-overlay binds to this exact lifetime, so a later flip to
            // tile-on-launch never reclears it.
            if born_keep_latch(
                true,
                !self.born_maximized_setting().should_clear_at_admission(),
            ) {
                self.born_keep_overlay.insert(window.clone());
                continue;
            }
            if self.born_keep_overlay.contains(&window) {
                continue;
            }
            if should_clear_maximize_at_admission_with_born_mode(
                false,
                true,
                pre_slotted.contains(&window),
                self.born_attempted.contains(&window),
                true,
                self.born_maximized_setting(),
            ) {
                let operation_id = self.mint_born_operation_id();
                self.retained_born.push(RetainedBornClear {
                    operation_id,
                    window,
                    home,
                    pending: None,
                    ambiguous: false,
                });
            }
        }
    }

    /// Prune every born lifetime set for one lifetime (explicit close and
    /// authoritative complete-inventory departure). The retained binding is
    /// dropped by the caller; a returning or replacement lifetime
    /// re-evaluates as first-seen.
    fn prune_born_lifetime(&mut self, window: &WindowId) {
        self.born_attempted.remove(window);
        self.born_held_fullscreen.remove(window);
        self.born_floating_first.remove(window);
        self.born_seen_nonfullscreen.remove(window);
        self.born_keep_overlay.remove(window);
    }

    /// Settle outstanding born maximize-clears against
    /// operation-correlated raw clear attempts plus the staged observation.
    /// Runs in [`WorldEngine::settle_rest`] after the directional lane so a
    /// proven born clear retires before the sweep publishes the next reply.
    ///
    /// Evidence stages BEFORE any readability or authority hold, mirroring
    /// the directional lane. The window never re-actuates a spent
    /// operation: after an attempt the binding holds ambiguity or retires,
    /// never retries. A correlated accepted attempt plus complete readable
    /// unmaximized facts proves the clear; only then does the binding
    /// retire (no directional continuation follows: the window already
    /// rides its admitted session row).
    pub(crate) fn settle_born_clears(&mut self) {
        // Neutral opt-in: without the capability no binding may survive to
        // publish. Mint drops the same state later in the pass; this fence
        // covers direct settlement callers. Silent: no verdict attaches to
        // a lane that never ran.
        if !self.birth_clear() {
            self.retained_born.clear();
            return;
        }
        let mut keep: Vec<RetainedBornClear> = Vec::new();
        for mut retained in std::mem::take(&mut self.retained_born) {
            // Explicit close prunes even across gaps, and re-arms the id:
            // a replacement lifetime after this close may clear again.
            if self.staged.closes.contains(&retained.window) {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedClosed,
                    None,
                );
                self.prune_born_lifetime(&retained.window);
                continue;
            }
            // Stage exact-correlated raw evidence before any hold.
            if let Some(attempt) = born_attempt_for(
                &self.staged.observation.clear_attempts,
                &retained.operation_id,
                &retained.window,
            ) {
                retained.pending = Some(attempt.clone());
            }
            // Gap bars hold with identities preserved, never settling from
            // absence and never erasing staged evidence.
            if !self.staged.observation.inventory_complete || self.staged.gesture_barrier {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Narrowly held rows keep the binding with no desire: live
            // presence with no usable facts, never a departure. Safety
            // exclusions keep the binding too (pending work must not
            // write/focus an excluded window); only an explicit close
            // prunes above.
            if self.is_held_window(&retained.window) || self.is_excluded_window(&retained.window) {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            let homing = self.staged_homing(&retained.window);
            let Some(home) = homing.clone() else {
                // Complete inventory with no homing and no close: a
                // complete departure. Drop the binding and prune every
                // lifetime set, so a returning lifetime re-evaluates as
                // first-seen instead of hitting stale marks.
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                self.prune_born_lifetime(&retained.window);
                continue;
            };
            let Some(fact) = self.staged.facts.get(&retained.window).cloned() else {
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedHomingChanged,
                    Some("homing-loss"),
                );
                self.prune_born_lifetime(&retained.window);
                continue;
            };
            let derived_settle = self.derived_for(&retained.window);
            // Minimized/hidden or unreadable facts hold without weakening
            // the retained authority. Raw null overlay already holds via
            // the derived hold; unknown minimized holds its dependent
            // clear without revoking membership.
            if derived_settle.hidden
                || derived_settle.hold
                || derived_settle.excluded
                || fact.rect.is_none()
            {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Delayed fixed transition: a birth that reads fixed only after
            // minting abandons with no retry (fixed births float beneath
            // their overlay). The automatic latch, the explicit suppress
            // signal, and the live wire-hint predicate must all read
            // non-fixed to continue.
            if derived_settle.fixed_auto
                || derived_settle.fixed_suppress
                || fact.hints.is_some_and(|hints| {
                    is_fixed_size_with(hints, self.fixed_size_predicate_setting())
                })
            {
                self.born_attempted.insert(retained.window.clone());
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearContinuationRefused,
                    Some("fixed"),
                );
                continue;
            }
            // A sticky flip ends the automatic clear with no retry: sticky
            // lifetimes keep their overlay, like fixed births. Authority
            // is re-proven every report, so a retained desire never
            // outlives the flip.
            if derived_settle.sticky {
                self.born_attempted.insert(retained.window.clone());
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearContinuationRefused,
                    Some("sticky"),
                );
                continue;
            }
            // Explicit float intent ends the automatic clear with no retry:
            // the user chose float, whether observed as a floating fact or
            // retained as outstanding session placement (same-report
            // ToggleFloat included). The flip latches floating-first so a
            // later tiled admission keeps its overlay with no second clear.
            if derived_settle.floating
                || self
                    .retained_floats
                    .iter()
                    .any(|held| held.window == retained.window)
            {
                self.born_floating_first.insert(retained.window.clone());
                self.born_attempted.insert(retained.window.clone());
                self.born_held_fullscreen.remove(&retained.window);
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearContinuationRefused,
                    Some("floating"),
                );
                continue;
            }
            // Born intent follows the window, not the home: a homing change
            // re-points the binding instead of abandoning it.
            if home != retained.home {
                retained.home = home.clone();
            }
            // Held-domain defers until authority returns (never silently
            // drops).
            let home_held = self
                .staged
                .domains
                .iter()
                .find(|domain| domain.key == retained.home)
                .is_none_or(|domain| !domain.readable);
            if home_held {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // A tiling flip to floating abandons with no clear and latches
            // floating-first, so the later tiled admission keeps its
            // overlay with no second clear.
            if self.logical_tiling(&retained.home.output.0, &retained.home.workspace.0)
                != TilingMode::Tiled
                || self.floating_scopes.contains(&retained.home)
            {
                self.born_floating_first.insert(retained.window.clone());
                self.born_attempted.insert(retained.window.clone());
                self.born_held_fullscreen.remove(&retained.window);
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearContinuationRefused,
                    Some("tiling-changed"),
                );
                continue;
            }
            // A fullscreen flip abandons the born binding and re-arms the
            // held lane: the exit may still clear through it. Raw null
            // overlay already holds above, so only proven `Some(true)`
            // abandons here.
            if fact.fullscreen == Some(true) {
                self.born_held_fullscreen.insert(retained.window.clone());
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedFullscreen,
                    None,
                );
                continue;
            }
            if derived_settle.overlay_maximized {
                // Still maximized: never success. Proven refusal retires
                // with no loop; anything else holds.
                if retained.pending.as_ref().is_some_and(born_refused) {
                    self.born_attempted.insert(retained.window.clone());
                    self.emit_operation(
                        retained.operation_id.clone(),
                        retained.window.clone(),
                        OperationOutcome::ClearRefusedProven,
                        None,
                    );
                    continue;
                }
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Unmaximized, complete, readable facts: resume needs a
            // correlated accepted attempt. An external clear alone (no
            // staged attempt at all) abandons, never continues. Ambiguous
            // staged evidence (throw, accepted mismatch, unreadable proof)
            // holds with no retry: it may yet be our own clear completing
            // asynchronously.
            let Some(pending) = retained.pending.clone() else {
                self.born_attempted.insert(retained.window.clone());
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedExternalClear,
                    None,
                );
                continue;
            };
            if born_refused(&pending) {
                self.born_attempted.insert(retained.window.clone());
                self.emit_operation(
                    retained.operation_id.clone(),
                    retained.window.clone(),
                    OperationOutcome::ClearAbandonedExternalClear,
                    Some("refused-with-unmaximized"),
                );
                continue;
            }
            if !born_accepted(&pending) {
                retained.pending = Some(pending);
                retained.ambiguous = true;
                keep.push(retained);
                continue;
            }
            // Proven: correlated accepted attempt plus truthful readable
            // unmaximized facts. Retire silently with no continuation: the
            // window already rides its admitted session row, and the muted
            // operation never re-actuates.
            self.born_attempted.insert(retained.window.clone());
        }
        self.retained_born = keep;
    }
}

/// Test-only view over born retention for the focused regression suite.
#[cfg(test)]
impl WorldEngine {
    pub(crate) fn born_retained_len(&self) -> usize {
        self.retained_born.len()
    }

    pub(crate) fn born_operation_for(&self, window: &WindowId) -> Option<String> {
        self.retained_born
            .iter()
            .find(|held| held.window == *window)
            .map(|held| held.operation_id.clone())
    }

    /// Total born lifetime marks (all sets plus outstanding bindings): the
    /// boundedness gauge for the departure-prune regressions.
    pub(crate) fn born_tracked_count(&self) -> usize {
        self.born_attempted.len()
            + self.born_held_fullscreen.len()
            + self.born_floating_first.len()
            + self.born_seen_nonfullscreen.len()
            + self.born_keep_overlay.len()
            + self.retained_born.len()
    }
}
