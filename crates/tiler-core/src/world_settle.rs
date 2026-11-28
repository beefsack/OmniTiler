//! Retained transfer settlement (D3=A): view, window/migration, and float
//! settlement against staged homing, raw echoes, and raw correlated attempt
//! facts.
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. No behavior change.
//!
//! Completion is core-owned over canonical mechanical attempt facts
//! ([`MemberAttemptEvidence`](crate::world_attempts::MemberAttemptEvidence)
//! disposition plus pre/post homing,
//! [`ViewAttemptEvidence`](crate::world_attempts::ViewAttemptEvidence)
//! disposition plus pre/post/baseline views) plus the staged observation. The
//! adapter reports write/re-read facts only and stages no verdict: every
//! confirmation, proven refusal, ambiguity latch, and external
//! supersession below is derived here. External transfer/view verdict
//! ingress is retired; internal [`AttemptOutcome`] latch vocabulary
//! (migration terminal) is retained.

use crate::world_transaction::WorldTransaction;
use std::collections::BTreeSet;

use crate::directional::{OutputId, WindowId};
use crate::session::DomainKey;
use crate::world_attempts::{
    AttemptDisposition, MemberAttemptDisposition, MemberAttemptEvidence, RetentionPolicy,
    ViewAttemptEvidence, retain_matching_attempt, view_evidence_matches,
};
use crate::world_engine::{RetainedFloat, RetainedTransfer};
use crate::world_report::{AttemptOutcome, TransferKind, WorkspaceRef, WorldCommand};

impl WorldTransaction<'_> {
    /// Effective logical pre homing for one member attempt, capability-gated
    /// to a single evidence lane (never a logical-or-native precedence):
    /// host-owned reports ignore the logical fields and translate only the
    /// native readback through this engine's own bindings on the supplied
    /// source output (where the window sat before the write); tiler-owned
    /// reports use only the logical pre and ignore natives (rejected
    /// without the capability, so accepted tiler-owned rows never carry
    /// them). Unmapped or unreadable natives stay unknown: never a guess,
    /// never an adapter table.
    fn effective_member_pre(
        &self,
        attempt: &MemberAttemptEvidence,
        source_output: &OutputId,
        host_owned: bool,
    ) -> Option<(OutputId, String)> {
        match attempt.pre.as_ref()? {
            WorkspaceRef::Logical { output, workspace }
                if !host_owned && output == source_output =>
            {
                Some((output.clone(), workspace.0.clone()))
            }
            WorkspaceRef::Native { output, workspace } if host_owned && output == source_output => {
                self.native_logical(output, workspace.as_str())
                    .map(|logical| (output.clone(), logical))
            }
            _ => None,
        }
    }

    /// Effective logical post homing for one member attempt on an explicit
    /// output, capability-gated like [`WorldEngine::effective_member_pre`]:
    /// host-owned translates only the native post readback through this
    /// engine's own bindings on the given output; tiler-owned uses only
    /// the logical post. Unmapped or unreadable natives stay unknown.
    fn effective_member_post_on(
        &self,
        attempt: &MemberAttemptEvidence,
        output: &OutputId,
        host_owned: bool,
    ) -> Option<(OutputId, String)> {
        match attempt.post.as_ref()? {
            WorkspaceRef::Logical {
                output: ref_output,
                workspace,
            } if !host_owned && ref_output == output => {
                Some((ref_output.clone(), workspace.0.clone()))
            }
            WorkspaceRef::Native {
                output: ref_output,
                workspace,
            } if host_owned && ref_output == output => self
                .native_logical(ref_output, workspace.as_str())
                .map(|logical| (ref_output.clone(), logical)),
            _ => None,
        }
    }

    /// Effective logical post homing for arrival proof: the post translated
    /// on the retained target output (where arrival lands). Same
    /// capability gate as [`WorldEngine::effective_member_pre`].
    fn effective_member_post(
        &self,
        attempt: &MemberAttemptEvidence,
        host_owned: bool,
    ) -> Option<(OutputId, String)> {
        match attempt.post.as_ref()? {
            WorkspaceRef::Logical { output, workspace } if !host_owned => {
                Some((output.clone(), workspace.0.clone()))
            }
            WorkspaceRef::Native { output, workspace } if host_owned => self
                .native_logical(output, workspace.as_str())
                .map(|logical| (output.clone(), logical)),
            _ => None,
        }
    }

    /// Effective logical pre view for one view attempt, capability-gated
    /// like [`WorldEngine::effective_member_pre`]: host-owned translates
    /// only the native current readback through this engine's own bindings
    /// for the attempt's target output; tiler-owned uses only the logical
    /// pre. Unmapped or unreadable natives stay unknown.
    pub(crate) fn effective_view_pre(
        &self,
        attempt: &ViewAttemptEvidence,
        host_owned: bool,
    ) -> Option<String> {
        if !host_owned {
            return None;
        }
        let reference = attempt.pre.as_ref()?;
        if reference.output() != &attempt.target_output {
            return None;
        }
        self.native_logical(reference.output(), reference.native_workspace()?.as_str())
    }

    /// Effective logical post view for one view attempt: same capability
    /// gate and translation rule as [`WorldEngine::effective_view_pre`].
    fn effective_view_post(
        &self,
        attempt: &ViewAttemptEvidence,
        host_owned: bool,
    ) -> Option<String> {
        if !host_owned {
            return None;
        }
        let reference = attempt.post.as_ref()?;
        if reference.output() != &attempt.target_output {
            return None;
        }
        self.native_logical(reference.output(), reference.native_workspace()?.as_str())
    }

    /// Effective logical baseline view for one view attempt: same capability
    /// gate and translation rule as [`WorldEngine::effective_view_pre`].
    fn effective_view_baseline(
        &self,
        attempt: &ViewAttemptEvidence,
        host_owned: bool,
    ) -> Option<String> {
        if !host_owned {
            return None;
        }
        let reference = attempt.baseline.as_ref()?;
        if reference.output() != &attempt.target_output {
            return None;
        }
        self.native_logical(reference.output(), reference.native_workspace()?.as_str())
    }

    /// Proven refusal from one membership attempt over effective homing: an
    /// explicit refusal with an unchanged readable preimage that is not
    /// the target. Anything else (accepted mismatch, throw, unreadable proof,
    /// moved preimage) latches ambiguity, never refusal. The tiler-owned
    /// logical route is unchanged (natives ignored there; host-owned ignores
    /// logical fields).
    ///
    /// The refusal post translates on the supplied source output (the exact
    /// current call preimage for window transfers, retained source for migration),
    /// never the target: a refused
    /// cross-output write leaves the window on the source, so its unchanged
    /// native post still names the source desktop. Translating that post on
    /// the target would unmap (or mismap a repeated global id) and could
    /// never prove refusal. Arrival proof
    /// ([`WorldEngine::member_post_proves_target`]) still translates on the
    /// target; no new output field is added. Output movement itself is not
    /// proven by native ids alone (the same native string repeated across
    /// outputs maps per output and cannot discriminate a move): the latch
    /// below only proves the preimage is unchanged, and the callers
    /// reconcile it against staged homing before settling. A coincidental
    /// same-string mapping that survives the latch still holds without
    /// staged source homing rather than settling refusal.
    fn member_refused(
        &self,
        attempt: &MemberAttemptEvidence,
        source_output: &OutputId,
        target_output: &OutputId,
        target_workspace: &str,
        host_owned: bool,
    ) -> bool {
        attempt.disposition == MemberAttemptDisposition::Called(AttemptDisposition::Refused) && {
            let pre = self.effective_member_pre(attempt, source_output, host_owned);
            // Refusal-only source-side post: the unchanged readback still
            // names the source desktop.
            let post = self.effective_member_post_on(attempt, source_output, host_owned);
            pre.is_some()
                && pre == post
                && pre.as_ref().is_some_and(|(output, workspace)| {
                    output != target_output || workspace != target_workspace
                })
        }
    }

    /// Proven refusal from one view attempt over effective views: same
    /// unchanged-readable-preimage rule as [`WorldEngine::member_refused`]. A
    /// follow miss is never a refused window: callers hold the parent
    /// ambiguous instead of settling refusal.
    pub(crate) fn view_refused(
        &self,
        attempt: &ViewAttemptEvidence,
        target_workspace: &str,
        host_owned: bool,
    ) -> bool {
        attempt.disposition == AttemptDisposition::Refused && {
            let pre = self.effective_view_pre(attempt, host_owned);
            let post = self.effective_view_post(attempt, host_owned);
            pre.is_some() && pre == post && pre.as_deref() != Some(target_workspace)
        }
    }

    /// Whether one accepted view attempt proves its target was reached: the
    /// effective post (native-only under the capability, logical-only
    /// without it) names the target. The immediate post-write readback proves
    /// the write landed even when the current observation already moved on;
    /// that departure is newer external input, never a reason to re-actuate.
    pub(crate) fn view_post_proves_target(
        &self,
        attempt: &ViewAttemptEvidence,
        host_owned: bool,
    ) -> bool {
        attempt.disposition == AttemptDisposition::Accepted
            && self.effective_view_post(attempt, host_owned).as_deref()
                == Some(attempt.target_workspace.as_str())
    }

    /// Remember one option-1 source release for retry by native intake:
    /// when arrival-gated release defers (partial list or pending
    /// creation), the pair is retried until it releases exactly once, so
    /// settling the flight never orphans a dormant binding. Deduplicated.
    pub(crate) fn defer_source_release(
        &mut self,
        source_output: &OutputId,
        workspace: &str,
    ) -> Vec<String> {
        let entry = (source_output.0.clone(), workspace.to_owned());
        if !self
            .world
            .settlement
            .pending_source_releases
            .contains(&entry)
        {
            self.world.settlement.pending_source_releases.push(entry);
        }
        Vec::new()
    }

    /// Arrival-gated source release for one migration (option 1): the
    /// migrated workspace left `source_output`, so every native pair on
    /// that output still naming it is released, and every released
    /// desktop present in the complete native order adopts at once as a
    /// fresh empty logical row at its native-order position through the
    /// ordinary outside rule — whether selected or not. No
    /// reverse-migration provenance is retained and no eligible spare is
    /// reused: newcomers always mint fresh logical ids.
    ///
    /// Callers reach this only after every surviving member's staged
    /// homing names the target: positive per-member native arrival proof
    /// is required, never a roster-absence predicate — a missing,
    /// unreadable, or contradictory receipt for any member holds the
    /// flight instead. Refusal rollback is unreachable: a refused
    /// migration still has members homed at the source (outstanding),
    /// and a latched refusal returns before this point. No vacancy
    /// inference from window facts (a stale or gapped occupant fact never
    /// gates release) and no attempt-proof gate (settlement keeps its own
    /// proof gates; an unproven refill still holds below). Host-owned
    /// only. A complete native list gates any binding mutation: with a
    /// partial enumeration the core cannot tell a vacated desktop from
    /// an unreported one, so partial reports never release or adopt.
    /// Any pending creation on the output defers (late desktops belong to
    /// it, the ordinary adoption rule). A logical still on the source
    /// ring is never released. The retained core view follows only a
    /// selected adoption (the report's current names the fresh desktop);
    /// a non-current adoption leaves views to echo observation.
    /// Returns the adopted fresh ids in native-order sequence.
    pub(crate) fn release_migrated_source_backing(
        &mut self,
        source_output: &OutputId,
        workspace: &str,
    ) -> Vec<String> {
        // Slot proof advances both projections together; it never re-adopts a
        // vacated source pair through the legacy migration release path.
        if self.connection.shared_roster_slots() {
            return Vec::new();
        }
        if !self.connection.host_owned_workspace_domains() {
            return Vec::new();
        }
        // Operation-scoped: only pairs naming the migrated workspace that
        // no longer lives on this ring. A logical still on the ring is
        // never released, and with no pairs left there is no work to
        // retain.
        if self
            .world
            .workspace
            .mapping
            .get(&source_output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == workspace))
        {
            return Vec::new();
        }
        let has_pairs = self
            .world
            .settlement
            .native_map
            .iter()
            .any(|((out, _), logical)| out == &source_output.0 && logical == workspace);
        if !has_pairs {
            return Vec::new();
        }
        // Deferral retains work: when the gates below cannot proceed, the
        // pair is remembered and retried by native intake until it
        // releases exactly once — settling the flight must never orphan
        // a dormant binding.
        if !self.native_list_complete(&source_output.0) {
            return self.defer_source_release(source_output, workspace);
        }
        // Ordinary adoption rule: while a creation is outstanding on this
        // output, unmapped desktops belong to it, never to fresh adoption.
        if self
            .world
            .settlement
            .retained_creations
            .iter()
            .any(|pending| pending.target_output == *source_output)
        {
            return self.defer_source_release(source_output, workspace);
        }
        let order: Vec<String> = self
            .world
            .staged
            .observation
            .native
            .iter()
            .find(|entry| entry.output == *source_output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default();
        let mut vacated: Vec<String> = Vec::new();
        self.world
            .settlement
            .native_map
            .retain(|(out, native), logical| {
                if out == &source_output.0 && logical == workspace {
                    vacated.push(native.clone());
                    false
                } else {
                    true
                }
            });
        if vacated.is_empty() {
            return Vec::new();
        }
        // The selected desktop must be present in the complete order: a
        // current outside a complete enumeration is contradictory adapter
        // evidence (hold, never mint rows for it). Listed-but-unmapped
        // desktops adopt at order position through the ordinary rule,
        // selected or not. Settled arrival-gated adoption clears any
        // locally retired marker first so the ordinary rule still mints a
        // fresh id (settled behavior preserved).
        let mut adopted: Vec<(String, String)> = Vec::new();
        for native in vacated {
            if !order.iter().any(|id| id == &native) {
                continue;
            }
            self.world
                .settlement
                .retired_natives
                .remove(&(source_output.0.clone(), native.clone()));
            if let Some(fresh) = self.adopt_single_native(&source_output.0, &native) {
                adopted.push((native, fresh));
            }
        }
        let current = self
            .world
            .staged
            .observation
            .native
            .iter()
            .find(|entry| entry.output == *source_output)
            .and_then(|entry| entry.current_native.clone())
            .unwrap_or_default();
        for (native, fresh) in &adopted {
            if *native == current {
                self.world
                    .workspace
                    .views
                    .insert(source_output.0.clone(), fresh.clone());
            }
        }
        adopted.into_iter().map(|(_, fresh)| fresh).collect()
    }

    /// Whether one raw member attempt proves arrival at its retained
    /// target: the effective post (native-only under the capability,
    /// translated on the target output; logical-only without it) names the
    /// target homing. A stale staged reading (delayed capture) never vetoes
    /// this proof; a different known staged homing stays newer external
    /// input and wins elsewhere.
    fn member_post_proves_target(&self, attempt: &MemberAttemptEvidence, host_owned: bool) -> bool {
        let post_proves = self
            .effective_member_post(attempt, host_owned)
            .as_ref()
            .is_some_and(|(output, workspace)| {
                output == &attempt.target_output && workspace == &attempt.target_workspace
            });
        match attempt.disposition {
            MemberAttemptDisposition::Called(AttemptDisposition::Accepted) => post_proves,
            MemberAttemptDisposition::AlreadyAtTarget => {
                // No-call consumption proves only with non-null equal raw
                // pre/post references AND the effective post translating to
                // the attempt target.
                attempt.pre.is_some() && attempt.pre == attempt.post && post_proves
            }
            MemberAttemptDisposition::Called(
                AttemptDisposition::Refused | AttemptDisposition::Threw,
            ) => false,
        }
    }

    /// Proven abandonment of one retained view flight: the correlated attempt
    /// proves the target was reached (see
    /// [`WorldEngine::view_post_proves_target`]) but the current echo names a
    /// different known workspace. The user moved after our proven write, even
    /// back to the previously seen view: adopt the echo instead of holding
    /// forever or re-actuating a settled write.
    fn view_proven_then_abandoned(
        &self,
        attempt: &ViewAttemptEvidence,
        target_workspace: &str,
        echo: Option<&str>,
        host_owned: bool,
    ) -> bool {
        self.view_post_proves_target(attempt, host_owned)
            && echo.is_some_and(|current| {
                current != target_workspace
                    && self
                        .world
                        .workspace
                        .mapping
                        .get(&attempt.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == current))
            })
    }

    /// Third-location proof from one view attempt over effective views: a
    /// readable post that is neither the target nor the KNOWN
    /// observation-time baseline proves newer external input. Unknown or empty
    /// baselines never prove third locations: a post matching an unknown
    /// baseline reads as ambiguity, never supersession. An unchanged preimage
    /// (`pre == post`) is never external proof even when the baseline
    /// differs: the write left the view where it was, so the attempt latches
    /// ambiguity without re-actuation. Target observation wins elsewhere
    /// (callers check exact echo before consulting this): this predicate never
    /// overrides an exact target proof.
    fn view_third_location(
        &self,
        attempt: &ViewAttemptEvidence,
        target_workspace: &str,
        host_owned: bool,
    ) -> bool {
        let post = self.effective_view_post(attempt, host_owned);
        let baseline = self.effective_view_baseline(attempt, host_owned);
        let Some(post) = post.as_deref() else {
            return false;
        };
        let Some(baseline) = baseline.as_deref() else {
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
        if self.effective_view_pre(attempt, host_owned).as_deref() == Some(post) {
            return false;
        }
        true
    }
}

impl WorldTransaction<'_> {
    /// Frozen grant-time physical preimage for one selection view obligation.
    /// `None` when no matching binding/target exists or the target carried no
    /// readable preimage: a supersession never guesses an absent baseline.
    fn selection_view_preimage(
        &self,
        parent_op: &str,
        output: &OutputId,
    ) -> Option<crate::world_report::ViewRef> {
        let binding = self
            .world
            .settlement
            .retained_selection
            .values()
            .find(|binding| {
                binding.parent_op == parent_op && binding.view_targets.contains_key(output)
            })?;
        let target = binding.view_targets.get(output)?;
        target.preimage.clone()
    }

    /// A pending view flight is externally superseded before any attempt
    /// when the output now reads a coherent known Desktop different from its
    /// frozen grant-time preimage. Read gaps, gesture-barrier reports,
    /// unchanged preimages, correlated attempts, and any non-Desktop read
    /// hold for the ordinary settlement lanes.
    fn pre_attempt_desktop_departure(&self, retained: &RetainedTransfer) -> Option<String> {
        if retained.pending_view.is_some() || self.world.staged.gesture_barrier {
            return None;
        }
        let output = &retained.target.target_output;
        let current = self.physical_current_ref(output)?;
        let crate::world_report::ViewRef::Native { workspace, .. } = &current else {
            return None;
        };
        let preimage =
            self.selection_view_preimage(&retained.target.operation_id, output)?;
        if current == preimage {
            return None;
        }
        let logical = self.native_logical(output, workspace.as_str())?;
        self.known_workspace(output, &logical).then_some(logical)
    }

    /// Adopt one externally observed Desktop logical for one output, recording
    /// physical history exactly like the ordinary external echo lane.
    pub(crate) fn adopt_observed_logical_view(&mut self, output: &OutputId, logical: &str) {
        self.world
            .workspace
            .views
            .insert(output.0.clone(), logical.to_owned());
        let ring = self
            .world
            .workspace
            .mapping
            .get(&output.0)
            .cloned()
            .unwrap_or_default();
        self.world
            .workspace
            .history
            .observe(&output.0, logical, &ring);
    }

    /// The common known logical Desktop across every live output, or `None`
    /// for any gap, non-Desktop, unknown, or divergent sibling. Shared-mode
    /// external adoption requires exactly this coherent common view.
    pub(crate) fn shared_common_desktop_logical(&self) -> Option<String> {
        let mut common: Option<String> = None;
        for screen in &self.world.staged.screens {
            let output = &screen.output;
            let Some(crate::world_report::ViewRef::Native { workspace, .. }) =
                self.physical_current_ref(output)
            else {
                return None;
            };
            let logical = self.native_logical(output, workspace.as_str())?;
            if !self.known_workspace(output, &logical) {
                return None;
            }
            match &common {
                None => common = Some(logical),
                Some(existing) if *existing == logical => {}
                Some(_) => return None,
            }
        }
        common
    }

    /// Adopt one coherent common Desktop logical across every live output.
    /// Shared mode only; the ring and single shared history stay canonical.
    pub(crate) fn adopt_shared_logical_view(&mut self, logical: &str) {
        let live: Vec<String> = self
            .world
            .staged
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        let mut changed = false;
        for output in &live {
            if !self
                .world
                .workspace
                .views
                .get(output)
                .is_some_and(|view| view == logical)
            {
                self.world
                    .workspace
                    .views
                    .insert(output.clone(), logical.to_owned());
                changed = true;
            }
        }
        if changed {
            let ring = live
                .first()
                .and_then(|output| self.world.workspace.mapping.get(output))
                .cloned()
                .unwrap_or_default();
            self.world.workspace.history.observe_shared(&ring, logical);
        }
    }

    /// Settle retained view targets with staged native proof before
    /// staging. Host-owned only: native-free views ride the frozen
    /// selection binding (ordinary, follow, migration, and policy routes
    /// all freeze one), so their flights are kept here for the binding to
    /// retire at its common terminal and never settle from echoes.
    /// Confirms move core views to the proven logical id and record
    /// history; externally moved echoes (differing from both the
    /// expectation and the last echo) are newer input and win: the flight
    /// drops and the echo is adopted as the view. Stale echoes hold.
    /// Raw refusal (explicit refusal with an unchanged readable preimage)
    /// or raw third-location proof settles to observed; any other raw
    /// attempt latches ambiguity without re-actuation (the adapter's spent
    /// view-phase fence blocks repeats). Paired migration refill children
    /// Paired migration refill children (carrying `migration_parent_op`
    /// matching a live migration parent) are skipped here
    /// and settled with their parent in [`WorldEngine::settle_migration`].
    /// Runs before [`WorldEngine::build_staged`] so homing assigns against
    /// adapter truth, never a stale desire.
    pub(crate) fn settle_views_pre(&mut self) {
        if self.connection.shared_roster_slots() {
            return;
        }
        // Canonical view correlation/retention: exact (op, output, workspace)
        // evidence stages into every retained view slot before any hold.
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::Views);
        // Migration parents live in the same vec: refill children with a
        // live parent settle with the parent, never alone.
        let parent_ops: std::collections::BTreeSet<String> = self
            .world
            .settlement
            .retained
            .iter()
            .filter(|retained| matches!(&retained.target.kind, TransferKind::Migration))
            .map(|retained| retained.target.operation_id.clone())
            .collect();
        let native_free = !self.connection.host_owned_workspace_domains();
        let shared = self.connection.workspace_mode_setting()
            == crate::workspace_scope::WorkspaceMode::Shared;
        let mut superseded: Vec<(OutputId, String)> = Vec::new();
        let mut ordinary_superseded: Vec<(OutputId, String)> = Vec::new();
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for retained in std::mem::take(&mut self.world.settlement.retained) {
            if !matches!(&retained.target.kind, TransferKind::View) {
                keep.push(retained);
                continue;
            }
            if native_free {
                // Native-free view proof is the frozen selection binding:
                // keep the flight for its terminal and never settle from
                // logical echoes here.
                keep.push(retained);
                continue;
            }
            if retained
                .migration_parent_op
                .as_ref()
                .is_some_and(|parent| parent_ops.contains(parent))
            {
                keep.push(retained);
                continue;
            }
            if retained.detached_destination.is_some() {
                // Detached view proof is the frozen selection binding's
                // typed current-ref check (`settle_selection_switch`), never
                // a logical Desktop echo. Before any correlated attempt, a
                // newer coherent Desktop current different from the frozen
                // preimage is external supersession: adopt it once and retire
                // only this output's flight/obligation, and never replay the
                // command. Gaps, barrier reports, unchanged preimages,
                // attempts, and divergent shared siblings hold for the proof
                // lanes. Ordinary (non-detached) flights never take this
                // lane: their native echo/attempt settlement is unchanged.
                if let Some(logical) = self.pre_attempt_desktop_departure(&retained) {
                    if shared {
                        // Shared adoption only on the coherent common
                        // logical across every live sibling; otherwise the
                        // flight and binding stay for canonical
                        // reconciliation.
                        if let Some(common) = self.shared_common_desktop_logical() {
                            self.adopt_shared_logical_view(&common);
                        } else {
                            keep.push(retained);
                            continue;
                        }
                    } else {
                        self.adopt_observed_logical_view(
                            &retained.target.target_output,
                            &logical,
                        );
                    }
                    superseded.push((
                        retained.target.target_output.clone(),
                        retained.target.operation_id.clone(),
                    ));
                    continue;
                }
                keep.push(retained);
                continue;
            }
            let raw = retained.pending_view.clone();
            let raw_view = raw
                .as_ref()
                .and_then(crate::world_attempts::PropertyAttempt::as_view);
            // Proof echo: the staged native-derived echo under the
            // host-owned capability. Unknown holds.
            let echo = self.native_echo_for(&retained.target.target_output);
            let echo = echo.as_deref();
            if echo == Some(retained.target.target_workspace.as_str()) {
                // Confirmed by echo: adopt as the core view.
                self.world.workspace.views.insert(
                    retained.target.target_output.0.clone(),
                    retained.target.target_workspace.clone(),
                );
                let ring = self
                    .world
                    .workspace
                    .mapping
                    .get(&retained.target.target_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.world.workspace.history.observe(
                    &retained.target.target_output.0,
                    &retained.target.target_workspace,
                    &ring,
                );
            } else if raw_view.is_some_and(|attempt| {
                self.view_refused(
                    attempt,
                    &retained.target.target_workspace,
                    self.connection.host_owned_workspace_domains(),
                )
            }) {
                // Proven refusal reconciled against readable evidence, never
                // from raw alone across missing facts: target exact proof
                // already won above; missing/gapped echoes hold with the raw
                // staged for later reconciliation; only a readable echo
                // naming the refused preimage settles to observed. Gaps
                // (incomplete inventory, incomplete screen, gesture) hold
                // persistently with identities preserved and no retry. The
                // screen bar reads the current staged screens (populated
                // from the current report at `stage_captured_v4`): an incomplete
                // current screen holds even after a complete prior.
                let current_complete = self
                    .world
                    .staged
                    .screens
                    .iter()
                    .find(|screen| screen.output == retained.target.target_output)
                    .map(|screen| screen.complete);
                if !self.world.staged.observation.inventory_complete
                    || current_complete != Some(true)
                    || self.world.staged.gesture_barrier
                {
                    keep.push(retained);
                    continue;
                }
                let pre_matches = raw_view.and_then(|attempt| {
                    self.effective_view_pre(attempt, self.connection.host_owned_workspace_domains())
                });
                if echo.is_some() && echo == pre_matches.as_deref() {
                    // Settle to observed: the refused preimage survived.
                } else {
                    keep.push(retained);
                }
            } else if raw_view.is_some_and(|attempt| {
                self.view_proven_then_abandoned(
                    attempt,
                    &retained.target.target_workspace,
                    echo,
                    self.connection.host_owned_workspace_domains(),
                )
            }) {
                // Proven write then newer external input: the translated
                // post proves the target was reached, but the current echo
                // names a different known workspace. The user moved after
                // our proven write (even back to the previously seen
                // view): adopt the echo and retire the flight forever
                // instead of holding or re-actuating a settled write.
                let current = echo.map(str::to_owned);
                if let Some(current) = current {
                    self.world
                        .workspace
                        .views
                        .insert(retained.target.target_output.0.clone(), current.clone());
                    let ring = self
                        .world
                        .workspace
                        .mapping
                        .get(&retained.target.target_output.0)
                        .cloned()
                        .unwrap_or_default();
                    self.world.workspace.history.observe(
                        &retained.target.target_output.0,
                        &current,
                        &ring,
                    );
                }
                ordinary_superseded.push((
                    retained.target.target_output.clone(),
                    retained.target.operation_id.clone(),
                ));
            } else if raw_view.is_some_and(|attempt| {
                self.view_third_location(
                    attempt,
                    &retained.target.target_workspace,
                    self.connection.host_owned_workspace_domains(),
                ) && self
                    .effective_view_post(attempt, self.connection.host_owned_workspace_domains())
                    .as_deref()
                    .is_some_and(|post| {
                        self.world
                            .workspace
                            .mapping
                            .get(&retained.target.target_output.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == post))
                    })
            }) {
                // Newer external input proven by the raw post: settle to
                // observed instead of fighting the user.
                ordinary_superseded.push((
                    retained.target.target_output.clone(),
                    retained.target.operation_id.clone(),
                ));
            } else {
                let last = self.last_view_logical(&retained.target.target_output);
                let last = last.as_deref();
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
                        .world
                        .workspace
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == echo))
                    {
                        self.world
                            .workspace
                            .views
                            .insert(retained.target.target_output.0.clone(), echo.to_owned());
                        let ring = self
                            .world
                            .workspace
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        self.world.workspace.history.observe(
                            &retained.target.target_output.0,
                            echo,
                            &ring,
                        );
                        ordinary_superseded.push((
                            retained.target.target_output.clone(),
                            retained.target.operation_id.clone(),
                        ));
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
        self.world.settlement.retained = keep;
        // External supersession retires only the output whose own flight was
        // dropped. Sibling flights, their frozen binding targets, and their
        // scopes stay for normal per-output settlement; a sibling may carry a
        // pending/latched attempt proof that must not be erased by one
        // output's departure. Scope and target cleanup is revision-matched:
        // a newer command on the same output owns its own state.
        for (output, parent) in superseded {
            self.retire_superseded_view(&output, &parent, false);
        }
        // An ordinary Desktop echo supersedes the stale binding: with no
        // sibling target left the whole binding drops, as it did before the
        // detached per-output lanes existed.
        for (output, parent) in ordinary_superseded {
            self.retire_superseded_view(&output, &parent, true);
        }
    }

    /// Retire one output's frozen view obligation after its flight was
    /// superseded by external input. For a detached departure only this
    /// output's target retires and siblings keep normal settlement; an
    /// ordinary Desktop supersession is stale as a whole and drops the
    /// covering binding, matching the legacy shared binding lane.
    fn retire_superseded_view(
        &mut self,
        output: &OutputId,
        parent: &str,
        drop_whole_binding: bool,
    ) {
        let removed = self
            .world
            .settlement
            .retained_selection
            .iter_mut()
            .find_map(|(key, binding)| {
                if binding.parent_op != parent {
                    return None;
                }
                let target = binding.view_targets.remove(output)?;
                binding.view_raw.remove(output);
                let empty = binding.view_targets.is_empty()
                    && binding.effect.is_none()
                    && binding.members.is_empty()
                    && binding.switch_members.is_empty()
                    && binding.switch_evidence.is_empty()
                    && binding.feedback.is_none();
                Some((
                    target.home,
                    binding.rev,
                    (drop_whole_binding || empty).then(|| key.clone()),
                ))
            });
        let Some((home, rev, remove_key)) = removed else {
            return;
        };
        if self
            .world
            .settlement
            .selection_scopes
            .get(output)
            .is_some_and(|scope| scope.rev == rev)
        {
            self.world.settlement.selection_scopes.remove(output);
        }
        self.world
            .settlement
            .selection_targets
            .retain(|_, entry| !(entry.home == home && entry.rev == rev));
        if let Some(key) = remove_key {
            self.world.settlement.retained_selection.remove(&key);
        }
    }

    /// Settle outstanding window and migration transfers plus explicit
    /// float placements against the staged homing, raw echoes, and raw
    /// correlated attempt facts. Runs after [`WorldEngine::build_staged`]
    /// and before the sweep.
    pub(crate) fn settle_rest(
        &mut self,
        current_command: Option<&WorldCommand>,
        issue: &crate::world_engine::EventIssue,
        consumed: bool,
    ) {
        // Directional maximize-clears settle first so a proven clear
        // resumes its original intent (tiled engine move or retained float
        // half) in this same pass: the sweep and publish below then apply
        // the continuation coherently in one reply. Born clears settle
        // next: a proven born clear retires before the sweep publishes the
        // next reply, so the desire drops exactly when proof lands. The
        // explicit clear phase does both at this former two-call position,
        // before member continuation below.
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::Clears {
            current_command,
            issue,
            consumed,
        });
        if self.connection.shared_roster_slots() {
            self.settle_floats();
            return;
        }
        // Canonical membership correlation/retention: exact (op, window,
        // target) evidence stages into every retained member/predecessor/
        // migration slot before any hold. Window transfers gate membership
        // independently of the frozen selection binding's destination
        // visibility/focus completion.
        self.settle_property_attempts(crate::world_attempts::PropertyAttemptPhase::Members);
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for mut retained in std::mem::take(&mut self.world.settlement.retained) {
            let raw = retained.pending_member.clone();
            let raw_member = raw
                .as_ref()
                .and_then(crate::world_attempts::PropertyAttempt::as_member);
            // Effective current call source: the exact raw pre names where the
            // window actually sat before this call, kept as an Option so an
            // unreadable pre is not mistaken for proof. Absent, unmapped, or
            // wrong-namespace pre falls back to the retained source.
            let call_source = raw_member.and_then(|attempt| {
                let output = match attempt.pre.as_ref()? {
                    WorkspaceRef::Logical { output, .. } | WorkspaceRef::Native { output, .. } => {
                        output
                    }
                };
                self.effective_member_pre(
                    attempt,
                    output,
                    self.connection.host_owned_workspace_domains(),
                )
            });
            let (call_output, call_workspace) = call_source.clone().unwrap_or_else(|| {
                (
                    retained.source_output.clone(),
                    retained.source_workspace.clone(),
                )
            });
            let raw_refused = match &retained.target.kind {
                TransferKind::Window(_) => raw_member.is_some_and(|attempt| {
                    self.member_refused(
                        attempt,
                        &call_output,
                        &retained.target.target_output,
                        &retained.target.target_workspace,
                        self.connection.host_owned_workspace_domains(),
                    )
                }),
                _ => false,
            };
            // Proven arrival from raw attempt evidence: an accepted write
            // whose effective post names the target homing (logical post,
            // else the host-owned native post readback translated through
            // this engine's own bindings). The immediate readback proves
            // the write landed even when the staged capture is intermediate.
            let raw_arrival_proves = match &retained.target.kind {
                TransferKind::Window(_) => raw_member.is_some_and(|attempt| {
                    self.member_post_proves_target(
                        attempt,
                        self.connection.host_owned_workspace_domains(),
                    )
                }),
                _ => false,
            };
            // Consumption spends the operation, not settlement. Calls always
            // spend; no-call requires exact target proof.
            let host_owned = self.connection.host_owned_workspace_domains();
            let latest_consumed = match &retained.target.kind {
                TransferKind::Window(_) => {
                    raw_member.is_some_and(|attempt| match attempt.disposition {
                        MemberAttemptDisposition::Called(_) => true,
                        MemberAttemptDisposition::AlreadyAtTarget => {
                            self.member_post_proves_target(attempt, host_owned)
                        }
                    })
                }
                _ => false,
            };
            // Only a consumed latest with a readable current source or a
            // proven target write may erase own-write correlation: consumed
            // with an unreadable source and no target proof is not evidence
            // of external input, so predecessor own-write knowledge survives
            // to protect the latest over an exact old own arrival.
            if latest_consumed && (call_source.is_some() || raw_arrival_proves) {
                retained.member_predecessors.clear();
            }
            let outstanding_predecessors = !retained.member_predecessors.is_empty();
            // Without latest consumption, and with any outstanding
            // predecessor, a bare independent target fact must not retire
            // the latest transfer. `raw_arrival_proves` above is left
            // untouched so an exact valid raw arrival still settles.
            let independent_blocked_by_predecessors = !latest_consumed && outstanding_predecessors;
            match &retained.target.kind {
                TransferKind::Window(window) => {
                    let homing = self.staged_homing(window);
                    let closed = self.world.staged.closes.contains(window);
                    let source_key = DomainKey {
                        output: call_output.clone(),
                        workspace: crate::directional::WorkspaceId(call_workspace.clone()),
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
                    let ambiguous = raw_member.is_some() && !raw_refused;
                    if homing.as_ref() == Some(&target_key) {
                        // Target observation wins over stale refusal: arrival
                        // proves before any raw refusal reconciles.
                        // Explicit close settles even on arrival proof.
                        if closed {
                            // Explicit close settles; the sweep forgets it.
                        } else {
                            let same_output = call_output == retained.target.target_output;
                            // Native-derived receipt proof (host-owned only;
                            // native-free carries no receipt vocabulary).
                            let observed_proves = self
                                .world
                                .staged
                                .facts
                                .get(window)
                                .and_then(|fact| {
                                    if self.connection.host_owned_workspace_domains() {
                                        self.effective_logical_receipt(fact)
                                    } else {
                                        None
                                    }
                                })
                                .as_deref()
                                == Some(retained.target.target_workspace.as_str());
                            // Staged homing alone never confirms: an active
                            // gesture shields the mover at its retained home
                            // (already the target after commit), and the
                            // committed session already homes same-output
                            // sends at the target before the native moves.
                            // Same-output needs observed receipt or raw post;
                            // cross-output needs a raw current fact at the
                            // target output (plus the native receipt naming
                            // the target when host-owned) or the exact raw
                            // post. Source facts, unknown outputs, and
                            // shielding alone never prove arrival.
                            let independent_proves = if same_output {
                                observed_proves || raw_arrival_proves
                            } else {
                                let fact_at_target = self
                                    .world
                                    .staged
                                    .facts
                                    .get(window)
                                    .and_then(|fact| fact.output.as_ref())
                                    == Some(&retained.target.target_output);
                                let receipt_ok = !self.connection.host_owned_workspace_domains()
                                    || observed_proves;
                                (fact_at_target && receipt_ok) || raw_arrival_proves
                            };
                            // With an outstanding predecessor and no latest
                            // consumption, a bare independent target fact
                            // never retires the latest transfer; an exact
                            // raw arrival (`raw_arrival_proves`) still
                            // settles via the untouched lane above.
                            let independent_proves = independent_proves
                                && !(independent_blocked_by_predecessors && !raw_arrival_proves);
                            if !independent_proves {
                                let mut held = retained;
                                if ambiguous {
                                    held.target.held = true;
                                }
                                keep.push(held);
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
                        let gapped = !self.world.staged.observation.inventory_complete
                            || self.screen_complete(&call_output) != Some(true)
                            || self.screen_complete(&retained.target.target_output) != Some(true)
                            || self.world.staged.gesture_barrier;
                        if gapped {
                            let mut held = retained;
                            held.target.held = true;
                            keep.push(held);
                        } else {
                            let fact_ok = self.world.staged.facts.get(window).is_some_and(|fact| {
                                fact.rect.is_some()
                                    && fact.minimized != Some(true)
                                    && !self.world.staged.closes.contains(window)
                            });
                            let pre_key = raw_member.and_then(|attempt| {
                                self.effective_member_pre(
                                    attempt,
                                    &call_output,
                                    self.connection.host_owned_workspace_domains(),
                                )
                                .map(|(out, ws)| DomainKey {
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
                                self.world.settlement.pending_forget.push((
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
                        // A translated native post readback proves the
                        // window reached the target even when the staged
                        // capture is intermediate (delayed capture at the
                        // source): the transfer settles on the proven write
                        // instead of holding or adopting the stale reading
                        // as outside input. A different known staged homing
                        // stays newer external input and wins below.
                        if key == source_key && raw_arrival_proves {
                            // Confirmed: the flight drops without
                            // re-actuation.
                        } else if key != source_key {
                            // When the current homing exactly matches a
                            // predecessor target backed by
                            // possibly-executed own evidence, the latest
                            // transfer is preserved: the window may
                            // have been left there by our own earlier write,
                            // so the location is not proven external
                            // supersession. No raw historical correlation
                            // falls through to the untouched external
                            // supersession below. Unconsumed/no-raw evidence
                            // keeps it unheld; ambiguous raw evidence latches
                            // held so spent calls cannot reauthorize
                            // actuation.
                            let predecessor_own_write =
                                retained.member_predecessors.iter().any(
                                    |(pred_target, pred_attempt)| {
                                        pred_target.target_output == key.output
                                        && pred_target.target_workspace == key.workspace.0.as_str()
                                        && pred_attempt.as_ref().and_then(
                                            crate::world_attempts::PropertyAttempt::as_member
                                        ).is_some_and(|attempt| {
                                            match attempt.disposition {
                                                MemberAttemptDisposition::Called(
                                                    AttemptDisposition::Accepted
                                                    | AttemptDisposition::Threw,
                                                ) => true,
                                                MemberAttemptDisposition::Called(
                                                    AttemptDisposition::Refused,
                                                ) => {
                                                    // Unknown pre/post cannot prove no write.
                                                    let Some(pre) = attempt.pre.as_ref() else {
                                                        return true;
                                                    };
                                                    let source_output = match pre {
                                                        WorkspaceRef::Logical {
                                                            output, ..
                                                        }
                                                        | WorkspaceRef::Native { output, .. } => {
                                                            output
                                                        }
                                                    };
                                                    !self.member_refused(
                                                        attempt,
                                                        source_output,
                                                        &attempt.target_output,
                                                        &attempt.target_workspace,
                                                        host_owned,
                                                    )
                                                }
                                                MemberAttemptDisposition::AlreadyAtTarget => false,
                                            }
                                        })
                                    },
                                );
                            if predecessor_own_write {
                                retained.source_output = key.output.clone();
                                retained.source_workspace = key.workspace.0.clone();
                                if ambiguous {
                                    retained.target.held = true;
                                }
                                keep.push(retained);
                            } else if retained.policy {
                                // Newer external input off both transfer screens
                                // wins; a contradictory receipt on either screen
                                // holds (never proves arrival, never fought).
                                // Hotplug policy intents are the exception: the
                                // screen vanished under the member (no newer
                                // user input), so the intent holds instead of
                                // dropping, and the outstanding flight keeps the
                                // member homed until arrival proves it. SOURCE
                                // provenance narrows the hold: a fact still on
                                // a retained source native (or with unknown
                                // native) is stale topology evidence and holds;
                                // a fact on a different genuine native is newer
                                // external input and supersedes the flight
                                // (drop, so reconcile adopts the receipt through
                                // the existing path; no new ledger).
                                // Capability-gated SOURCE narrowing: without
                                // host-owned evidence no Value ever proves
                                // external input, so the policy holds.
                                if !self.connection.host_owned_workspace_domains() {
                                    keep.push(retained);
                                } else {
                                    let source_natives: BTreeSet<String> = self
                                        .world
                                        .settlement
                                        .native_map
                                        .iter()
                                        .filter(|((out, _), logical)| {
                                            out == &retained.source_output.0
                                                && *logical == &retained.source_workspace
                                        })
                                        .map(|((_, native), _)| native.clone())
                                        .collect();
                                    let stale = match self
                                        .world
                                        .staged
                                        .facts
                                        .get(window)
                                        .and_then(|fact| fact.native_workspace.as_deref())
                                    {
                                        Some(native) => {
                                            if !source_natives.is_empty() {
                                                source_natives.contains(native)
                                            } else {
                                                // Unbacked hosted unit (return source has
                                                // no lane yet): use the target binding
                                                // for the same logical as the stale
                                                // proxy. A backed source stays
                                                // authoritative so a genuine move
                                                // still supersedes.
                                                self.world
                                                    .settlement
                                                    .native_map
                                                    .iter()
                                                    .filter(|((out, _), logical)| {
                                                        out == &retained.target.target_output.0
                                                            && *logical
                                                                == &retained.source_workspace
                                                    })
                                                    .map(|((_, native_id), _)| native_id.clone())
                                                    .any(|id| id == native)
                                            }
                                        }
                                        None => true,
                                    };
                                    if stale {
                                        keep.push(retained);
                                    }
                                }
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
                    self.settle_migration(retained, &mut keep);
                }
            }
        }
        self.world.settlement.retained = keep;
        self.settle_floats();
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
    /// and a settled parent never reactivates. A proven view write
    /// followed by a different known current proves the proven view was
    /// abandoned: the flight retires forever and the echo is adopted,
    /// even when the echo still names the last seen view. Proven refusal
    /// (every member refused with an unchanged readable preimage) or external
    /// supersession (third-location view proof) derived from raw attempt
    /// facts latches the terminal outcome under the operation identity;
    /// the retained logical mapping persists (core authority), while
    /// ambiguous attempts hold the flight without re-actuation.
    pub(crate) fn settle_migration(
        &mut self,
        mut retained: RetainedTransfer,
        keep: &mut Vec<RetainedTransfer>,
    ) {
        // Canonical member evidence was staged by the ordered Members phase
        // before any hold; the migration parent's view proof rides this entry
        // and stages here through the shared primitive (native-free included),
        // then policy reconciles it.
        let target_out = retained.target.target_output.clone();
        let target_ws = retained.target.target_workspace.clone();
        let op = retained.target.operation_id.clone();
        let attempts = &self.world.staged.observation.property_attempts;
        let mut parent_slot = retained.pending_view.take();
        retain_matching_attempt(
            attempts,
            |attempt| {
                attempt.as_view().is_some_and(|evidence| {
                    view_evidence_matches(evidence, &op, &target_out, &target_ws)
                })
            },
            &mut parent_slot,
            RetentionPolicy::ReplacePending,
        );
        retained.pending_view = parent_slot;
        retained
            .pending_members
            .retain(|member, _| retained.members.contains(member));
        // Settle the paired refill child here: settle_views_pre skips
        // refill children with a live parent so the two phases reconcile
        // together. An orphan child (no live parent) is never passed here.
        if let Some(pos) = keep.iter().position(|r| {
            matches!(&r.target.kind, TransferKind::View)
                && r.migration_parent_op.as_deref() == Some(op.as_str())
        }) {
            let refill_child_op = keep[pos].target.operation_id.clone();
            let child_out = keep[pos].target.target_output.clone();
            let child_ws = keep[pos].target.target_workspace.clone();
            let mut child_slot = keep[pos].pending_view.take();
            retain_matching_attempt(
                attempts,
                |attempt| {
                    attempt.as_view().is_some_and(|evidence| {
                        view_evidence_matches(evidence, &refill_child_op, &child_out, &child_ws)
                    })
                },
                &mut child_slot,
                RetentionPolicy::ReplacePending,
            );
            keep[pos].pending_view = child_slot;
            let child_raw = keep[pos].pending_view.clone();
            let child_view = child_raw
                .as_ref()
                .and_then(crate::world_attempts::PropertyAttempt::as_view);
            let echo = self
                .world
                .staged
                .echoes
                .get(&child_out.0)
                .map(String::as_str);
            if echo == Some(child_ws.as_str()) {
                keep.remove(pos);
            } else if child_view.is_some_and(|attempt| {
                self.view_refused(
                    attempt,
                    &child_ws,
                    self.connection.host_owned_workspace_domains(),
                )
            }) {
                // A refused refill never retries: the spent view-phase fence
                // blocks repeats, so mere child absence can never itself
                // prove external. The parent holds below while the child is
                // present; a later exact refill echo settles the child. A
                // later genuine external source move (a known current echo
                // naming a different workspace than both the refill and the
                // refused preimage) settles the parent instead of holding
                // forever: drop the child so the source-echo check below
                // reconciles as external. Unknown/gap echoes hold.
                let pre = child_view.and_then(|attempt| {
                    self.effective_view_pre(attempt, self.connection.host_owned_workspace_domains())
                });
                let known_third = echo.is_some_and(|current| {
                    current != child_ws.as_str()
                        && Some(current) != pre.as_deref()
                        && self
                            .world
                            .workspace
                            .mapping
                            .get(&child_out.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == current))
                });
                if known_third {
                    keep.remove(pos);
                } else {
                    keep[pos].target.held = true;
                }
            } else if child_view.is_some_and(|attempt| {
                self.view_proven_then_abandoned(
                    attempt,
                    &child_ws,
                    echo,
                    self.connection.host_owned_workspace_domains(),
                )
            }) {
                // Proven refill then abandoned: the translated post
                // proves our refill view landed, but the current echo
                // names a different known workspace. The user moved after
                // our proven write: adopt the echo and retire the child
                // forever (never re-actuated); the parent reconciles
                // against the source echo below.
                let current = echo.map(str::to_owned);
                if let Some(current) = current {
                    self.world
                        .workspace
                        .views
                        .insert(child_out.0.clone(), current);
                }
                keep.remove(pos);
            } else if child_view.is_some_and(|attempt| {
                self.view_third_location(
                    attempt,
                    &child_ws,
                    self.connection.host_owned_workspace_domains(),
                ) && self
                    .effective_view_post(attempt, self.connection.host_owned_workspace_domains())
                    .as_deref()
                    .is_some_and(|post| {
                        self.world
                            .workspace
                            .mapping
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
            .filter(|m| !self.world.staged.closes.contains(m))
            .cloned()
            .collect();
        if !live_members.is_empty()
            && live_members.iter().all(|member| {
                retained.pending_members.get(member).and_then(
                    crate::world_attempts::PropertyAttempt::as_member
                ).is_some_and(|attempt| {
                    self.member_refused(attempt, &retained.source_output, &target_out, &target_ws, self.connection.host_owned_workspace_domains())
                })
            })
        {
            retained.migration_terminal = Some(AttemptOutcome::RefusedProven);
        } else if retained.pending_view.as_ref().and_then(
            crate::world_attempts::PropertyAttempt::as_view
        ).is_some_and(|attempt| {
            self.view_third_location(attempt, &target_ws, self.connection.host_owned_workspace_domains())
                && self.effective_view_post(attempt, self.connection.host_owned_workspace_domains()).as_deref().is_some_and(|post| {
                    self.world.workspace.mapping
                        .get(&target_out.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == post))
                })
                // Target observation wins over stale third-location proof:
                // an exact target echo confirms instead of latching external
                // supersession.
                && self
                    .world.staged
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
            matches!(&r.target.kind, TransferKind::View)
                && r.migration_parent_op.as_deref() == Some(op.as_str())
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
                .filter(|m| !self.world.staged.closes.contains(m))
                .cloned()
                .collect();
            if pruned.is_empty()
                && !retained.migration_initial_empty
                && !retained.members.is_empty()
            {
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.migration_parent_op.as_deref() == Some(op.as_str()))
                });
                return;
            }
            // Gap bars: partial/unreadable inventory, incomplete screens,
            // or a held gesture never prove non-arrival. Hold narrowly
            // with identities preserved, marked held persistently so the
            // flight survives the refusal acknowledgement disappearing.
            if !self.world.staged.observation.inventory_complete {
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
            if self.world.staged.gesture_barrier {
                let mut held = retained;
                held.members = pruned;
                held.target.held = true;
                keep.push(held);
                return;
            }
            let unreadable_member = pruned.iter().any(|m| {
                self.world.staged.facts.get(m).is_some_and(|f| {
                    f.rect.is_none()
                        && f.minimized != Some(true)
                        && !self.world.staged.closes.contains(m)
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
                if let Some(ring) = self
                    .world
                    .workspace
                    .mapping
                    .get_mut(&retained.target.target_output.0)
                {
                    ring.retain(|id| id != &ws);
                }
                {
                    let source_ring = self
                        .world
                        .workspace
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
                        self.world
                            .workspace
                            .mapping
                            .get(&retained.source_output.0)
                            .is_some_and(|ring| ring.iter().any(|id| id == view))
                    })
                    .unwrap_or_else(|| ws.clone());
                self.world
                    .workspace
                    .views
                    .insert(retained.source_output.0.clone(), source_view.clone());
                if let Some(prev_target) = retained.migration_target_view.clone() {
                    let known = self
                        .world
                        .workspace
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == &prev_target));
                    if known {
                        self.world
                            .workspace
                            .views
                            .insert(retained.target.target_output.0.clone(), prev_target);
                    } else {
                        let target_ring = self
                            .world
                            .workspace
                            .mapping
                            .get(&retained.target.target_output.0)
                            .cloned()
                            .unwrap_or_default();
                        if let Some(head) = target_ring.first().cloned() {
                            self.world
                                .workspace
                                .views
                                .insert(retained.target.target_output.0.clone(), head);
                        } else {
                            self.world
                                .workspace
                                .views
                                .remove(&retained.target.target_output.0);
                        }
                    }
                } else {
                    let target_ring = self
                        .world
                        .workspace
                        .mapping
                        .get(&retained.target.target_output.0)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(head) = target_ring.first().cloned() {
                        self.world
                            .workspace
                            .views
                            .insert(retained.target.target_output.0.clone(), head);
                    } else {
                        self.world
                            .workspace
                            .views
                            .remove(&retained.target.target_output.0);
                    }
                }
                let source_ring = self
                    .world
                    .workspace
                    .mapping
                    .get(&retained.source_output.0)
                    .cloned()
                    .unwrap_or_default();
                self.world.workspace.history.observe(
                    &retained.source_output.0,
                    &source_view,
                    &source_ring,
                );
                // Terminal rollback revokes ONLY the exact parent-issued
                // selection binding/scopes/targets by the stored issuing
                // revision: departing-echo tolerance would otherwise keep
                // the rolled-back grant alive to Complete later. Newer
                // selections on the same outputs carry a newer revision
                // and survive; silent drop, no new event or token.
                if let Some(sel_rev) = retained.migration_selection_rev {
                    let op_id = retained.target.operation_id.clone();
                    self.world
                        .settlement
                        .retained_selection
                        .retain(|_, binding| {
                            !(binding.parent_op == op_id && binding.rev == sel_rev)
                        });
                    self.world
                        .settlement
                        .selection_scopes
                        .retain(|_, scope| scope.rev != sel_rev);
                    self.world
                        .settlement
                        .selection_targets
                        .retain(|_, entry| entry.rev != sel_rev);
                }
            } else {
                // No rollback: members arrived, closed, or the move was
                // empty — settling to observed with no return onto the
                // source backing. Release it here (option 1) rather than
                // orphaning a dormant pair with the retired flight.
                self.release_migrated_source_backing(
                    &retained.source_output,
                    &retained.target.target_workspace,
                );
            }
            // Settle to observed; the retained logical mapping persists
            // (rolled back above for the wholly-unarrived case).
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.migration_parent_op.as_deref() == Some(op.as_str()))
            });
            return;
        }
        // Native-free originally-empty migration: zero members means
        // no membership to await. Retire at the earliest normal
        // publication alongside the existing zero-member Switch
        // completion; never hold the parent on inventory/screens/
        // gesture gaps, never an artificial wake. Native-owned empty
        // keeps its view-proof path below; nonzero visibility
        // obligations stay held in the selection binding.
        if retained.migration_initial_empty && !self.connection.host_owned_workspace_domains() {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.migration_parent_op.as_deref() == Some(op.as_str()))
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
            .filter(|m| !self.world.staged.closes.contains(m))
            .cloned()
            .collect();
        if pruned.is_empty() && !retained.migration_initial_empty && !retained.members.is_empty() {
            // All members explicitly closed: settle without waiting for
            // views that will never arrive, and release the source
            // backing — closed lifetimes never roll back, so no dormant
            // pair may remain with the retired flight.
            self.release_migrated_source_backing(
                &retained.source_output,
                &retained.target.target_workspace,
            );
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.migration_parent_op.as_deref() == Some(op.as_str()))
            });
            return;
        }
        if !self.world.staged.observation.inventory_complete {
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
        if self.world.staged.gesture_barrier {
            let mut held = retained;
            held.members = pruned;
            hold(held);
            return;
        }
        // Unreadable member frames hold narrowly: partial/unreadable arrival
        // stays outstanding with arrived members preserved.
        let unreadable_member = pruned.iter().any(|m| {
            self.world.staged.facts.get(m).is_some_and(|f| {
                f.rect.is_none()
                    && f.minimized != Some(true)
                    && !self.world.staged.closes.contains(m)
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
            if self.world.staged.closes.contains(member) {
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
        // settle to observed without fighting either view, and release the
        // source backing — with no survivors nothing rolls back, so no
        // dormant pair may remain with the retired flight.
        if surviving.is_empty() && !retained.migration_initial_empty {
            self.release_migrated_source_backing(
                &retained.source_output,
                &retained.target.target_workspace,
            );
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.migration_parent_op.as_deref() == Some(op.as_str()))
            });
            return;
        }
        // Arrival-gated source release (option 1): every surviving
        // member's staged homing names the target (positive per-member
        // native arrival proof above — never roster absence), so the
        // migrated workspace never rolls back onto its source backing.
        // Release its source-output native pairs and adopt listed
        // desktops fresh at native order — selected or not. Settlement
        // below still reconciles against the staged echoes: a
        // staged-unknown echo holds (never settled from a gap), and the
        // fresh echo resolves next round with the switch winning.
        self.release_migrated_source_backing(
            &retained.source_output,
            &retained.target.target_workspace,
        );
        // Native-free selection route: the frozen binding owns both views
        // through the existing selection completion, so no logical echo
        // proof gates the Migration. All members arrived (or the move was
        // empty/fully closed above): retire the parent now while the
        // Switch pends on its own visibility and focus proofs. Holds,
        // gaps, refusals, and external supersession above still apply
        // unchanged; only the echo tail is skipped, and no refill child
        // exists on this route. Stable native-free route identity (never
        // the live binding presence: an issued zero-member terminal or a
        // superseded/closed binding must never revert membership to the
        // echo path).
        if !self.connection.host_owned_workspace_domains() {
            keep.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.migration_parent_op.as_deref() == Some(op.as_str()))
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
                .world
                .staged
                .echoes
                .get(&retained.source_output.0)
                .cloned();
            if source_echo.is_none() {
                // Unknown source echo with all members arrived: usually a
                // gap (hold, never settle as external). The release above
                // already adopted any listed vacated desktop fresh; its
                // echo resolves next round and the flight settles with the
                // switch winning and the refill retired forever. Window
                // facts never gate this; settlement keeps its own proof
                // gates below.
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            if !refill_child_held && source_echo.as_deref() != Some(refill) {
                // Unknown source echoes hold (gap); only a known different
                // logical id proves external movement.
                let known = self
                    .world
                    .workspace
                    .mapping
                    .get(&retained.source_output.0)
                    .is_some_and(|ring| {
                        source_echo
                            .as_deref()
                            .is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                // Externally moved source view: settle to observed. The
                // user's switch wins (adopt the echo); the migrated
                // members stay migrated. Drop the refill child.
                if let Some(current) = source_echo {
                    self.world
                        .workspace
                        .views
                        .insert(retained.source_output.0.clone(), current);
                }
                let target_echo = self
                    .world
                    .staged
                    .echoes
                    .get(&retained.target.target_output.0)
                    .cloned();
                if let Some(current) = target_echo {
                    let known_target = self
                        .world
                        .workspace
                        .mapping
                        .get(&retained.target.target_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == &current));
                    if known_target {
                        self.world
                            .workspace
                            .views
                            .insert(retained.target.target_output.0.clone(), current);
                    }
                }
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.migration_parent_op.as_deref() == Some(op.as_str()))
                });
                return;
            }
        }
        let target_echo = self
            .world
            .staged
            .echoes
            .get(&retained.target.target_output.0)
            .cloned();
        if target_echo.as_deref() != Some(retained.target.target_workspace.as_str()) {
            // Missing echo is a gap: hold, never external supersession and
            // never a ring-head revert.
            if target_echo.is_none() {
                let mut held = retained;
                held.members = surviving;
                hold(held);
                return;
            }
            let last = self.last_view_logical(&retained.target.target_output);
            let last = last.as_deref();
            // A proven-then-abandoned target alone never settles here:
            // with the refill still shown that shape is lag (the target
            // switch simply has not landed yet), so the parent holds and
            // the spent view phase re-actuates it. Only the source side
            // disambiguates abandonment (a proven refill that is no longer
            // shown settles through the source check above).
            if target_echo.as_deref() != last {
                // Externally moved target view settles, but unknown ids
                // hold (gap, never external proof).
                let known = self
                    .world
                    .workspace
                    .mapping
                    .get(&retained.target.target_output.0)
                    .is_some_and(|ring| {
                        target_echo
                            .as_deref()
                            .is_some_and(|echo| ring.iter().any(|id| id == echo))
                    });
                if !known {
                    let mut held = retained;
                    held.members = surviving;
                    hold(held);
                    return;
                }
                // Externally moved target view: settle to observed and
                // drop the refill child. The user's switch wins on both
                // outputs (adopt each known echo); the migrated members
                // stay migrated.
                if let Some(current) = target_echo {
                    self.world
                        .workspace
                        .views
                        .insert(retained.target.target_output.0.clone(), current);
                }
                let source_echo = self
                    .world
                    .staged
                    .echoes
                    .get(&retained.source_output.0)
                    .cloned();
                if let Some(current) = source_echo {
                    let known_source = self
                        .world
                        .workspace
                        .mapping
                        .get(&retained.source_output.0)
                        .is_some_and(|ring| ring.iter().any(|id| id == &current));
                    if known_source {
                        self.world
                            .workspace
                            .views
                            .insert(retained.source_output.0.clone(), current);
                    }
                }
                keep.retain(|r| {
                    !(matches!(&r.target.kind, TransferKind::View)
                        && r.migration_parent_op.as_deref() == Some(op.as_str()))
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
                .world
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
            .world
            .workspace
            .mapping
            .get(&retained.target.target_output.0)
            .cloned()
            .unwrap_or_default();
        self.world.workspace.history.observe(
            &retained.target.target_output.0,
            &retained.target.target_workspace,
            &target_ring,
        );
        if let Some(refill) = retained.migration_refill.as_deref() {
            let source_ring = self
                .world
                .workspace
                .mapping
                .get(&retained.source_output.0)
                .cloned()
                .unwrap_or_default();
            self.world
                .workspace
                .history
                .observe(&retained.source_output.0, refill, &source_ring);
        }
        keep.retain(|r| {
            !(matches!(&r.target.kind, TransferKind::View)
                && r.migration_parent_op.as_deref() == Some(op.as_str()))
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
    ///   command-time source nor the target proves newer native input unless
    ///   it matches the latest accepted, clocked, DPI-stable native write.
    ///   That older issued world's echo holds; unrelated input drops the
    ///   stale target without a fight.
    /// - Explicit closes always settle, even across gaps; absent facts hold
    ///   across the gap (never an inferred close). Incomplete inventory and
    ///   gesture barriers hold every flight without settling.
    /// - Every terminal verdict above (confirmation, proven refusal,
    ///   external supersession, close) emits one `operation_settled` event
    ///   with the original op/window/outcome; holds emit nothing.
    pub(crate) fn settle_floats(&mut self) {
        use crate::world_attempts::{
            AttemptDisposition, PropertyAttempt, RetentionPolicy, frame_evidence_matches,
            frame_watermark_stale, retain_candidate,
        };
        use crate::world_frames::dpi_basis_changed;
        use crate::world_report::OperationOutcome;
        let mut keep: Vec<RetainedFloat> = Vec::new();
        for mut retained in std::mem::take(&mut self.world.settlement.retained_floats) {
            if self.world.staged.closes.contains(&retained.window) {
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
            for attempt in self
                .world
                .staged
                .observation
                .property_attempts
                .iter()
                .filter(|attempt| {
                    attempt.as_frame().is_some_and(|evidence| {
                        frame_evidence_matches(
                            evidence,
                            &retained.window,
                            &retained.target,
                            Some(retained.operation_id.as_str()),
                        )
                    })
                })
            {
                let evidence = attempt.as_frame().expect("filtered frame attempt");
                if frame_watermark_stale(&self.world.settlement.consumed_frames, evidence) {
                    continue;
                }
                if dpi_basis_changed(evidence.dpi_before, evidence.dpi_after) {
                    fresh_dpi_flip = true;
                }
                // Clone only the actually retained exact-correlated row; the
                // shared primitive preserves report order (last fresh row
                // wins) and never stages a forged or replayed attempt.
                retain_candidate(
                    attempt,
                    &mut retained.pending,
                    RetentionPolicy::ReplacePending,
                );
                fresh_evidence = true;
            }
            // Safety exclusions hold: pending placement work must not
            // write an excluded window. (Held rows carry no facts, so the
            // readable gate below already holds them.)
            if self.is_excluded_window(&retained.window) {
                if retained.pending.is_some() {
                    retained.ambiguous = true;
                }
                keep.push(retained);
                continue;
            }
            // Every verdict below except close needs truthful readable
            // complete data: complete inventory, no gesture barrier, and a
            // non-minimized readable fact. Anything less holds without
            // settling, but latches conservatively from retained evidence
            // so a later readable report reconciles instead of dropping
            // the operation as external input.
            let readable = self.world.staged.observation.inventory_complete
                && !self.world.staged.gesture_barrier
                && self
                    .world
                    .staged
                    .facts
                    .get(&retained.window)
                    .is_some_and(|fact| fact.minimized != Some(true) && fact.rect.is_some());
            let Some(current) = readable
                .then(|| {
                    self.world
                        .staged
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
            if let Some(pending) = retained
                .pending
                .as_ref()
                .and_then(PropertyAttempt::as_frame)
                && !dpi_basis_changed(pending.dpi_before, pending.dpi_after)
                && pending.disposition == AttemptDisposition::Refused
                && let (Some(pre), Some(readback)) = (pending.purpose.prewrite(), pending.readback)
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
            // An already-issued older world can land after this target was
            // retained. Its latest mechanical attempt is self-echo evidence,
            // not a float completion or permission to repeat either write.
            let fresh_frame = self
                .world
                .staged
                .observation
                .property_attempts
                .iter()
                .filter_map(PropertyAttempt::as_frame)
                .find(|evidence| {
                    evidence.window == retained.window
                        && !frame_watermark_stale(&self.world.settlement.consumed_frames, evidence)
                });
            let echo_target = match fresh_frame {
                Some(evidence) => crate::world_attempts::is_qualifying_timed_echo(evidence)
                    .then_some(evidence.target),
                None => self
                    .world
                    .settlement
                    .consumed_frames
                    .get(&retained.window)
                    .and_then(|watermark| watermark.timed_echo),
            };
            if echo_target == Some(current) {
                keep.push(retained);
                continue;
            }
            if retained.source.is_some_and(|source| current != source) {
                // Native frame matches neither the command-time
                // source nor the target or latest issued write: newer external
                // input (manual move/resize) wins; the stale target drops
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
        self.world.settlement.retained_floats = keep;
    }
}
