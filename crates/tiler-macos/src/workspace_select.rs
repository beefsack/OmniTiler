#![forbid(unsafe_code)]

//! Reply-driven native workspace selector: the core owns targeting,
//! history, and reuse-vs-create. The drain actuates exactly the
//! reply-issued plan (live binding lookup or single authorized creation),
//! stages raw attempt evidence with genuine reads on every path, and never
//! invents verdicts.

use crate::runtime_types::InputSemanticAction;
use std::collections::BTreeSet;
use tiler_core::directional::OutputId;
use tiler_core::world_report::FrameSetter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSnapshot {
    pub display: String,
    pub spaces: Vec<u64>,
    pub current: u64,
}

impl SelectionSnapshot {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.display.is_empty()
            || self.spaces.is_empty()
            || self.spaces.contains(&0)
            || !self.spaces.contains(&self.current)
            || self
                .spaces
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.spaces.len()
        {
            return Err("workspace row/current identity unprovable".to_owned());
        }
        Ok(())
    }
}

/// Fresh single-display user-Space row for `display` (existing user Spaces
/// in native row order plus the current Space). Read-only: one SLS snapshot
/// plus one type read per SID. Shared by the explicit batch backend and the
/// menu-bar indicator so both map ordinals over the identical row.
///
/// Display identity for the product single-display rule lives beside it in
/// [`fresh_display_count`].
pub(crate) fn fresh_user_row(display: usize) -> Result<SelectionSnapshot, String> {
    let all = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
    let Some(row) = all.displays.get(display) else {
        return Err("display row missing".to_owned());
    };
    user_row_parts(row.identifier.clone(), &row.spaces, row.current)
}

/// Fresh native display-row count for the product single-display rule (one
/// fresh read-only row query, no mutation).
pub(crate) fn fresh_display_count() -> Result<usize, String> {
    crate::mc_lifecycle::live::fresh_snapshot()
        .map(|s| s.displays.len())
        .map_err(|e| e.to_string())
}

/// User-Space row over the tile controller's already-read cadence snapshot
/// rows (same rule as [`fresh_user_row`]: existing user Spaces in native
/// order plus the current Space; proven non-user Spaces omitted, never
/// invented). The indicator and its click mapping share one consistent read
/// with the topology revision this way.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn user_row_from_rows(
    rows: &[crate::ffi::SlsDisplayRow],
    display: usize,
) -> Result<SelectionSnapshot, String> {
    let Some(row) = rows.get(display) else {
        return Err("display row missing".to_owned());
    };
    user_row_parts(row.identifier.clone(), &row.spaces, row.current)
}

/// Shared row assembly over one display's identifier, native-order Space
/// list, and current Space: user-typed Spaces only, identity validated.
fn user_row_parts(
    identifier: Option<String>,
    spaces: &[u64],
    current: Option<u64>,
) -> Result<SelectionSnapshot, String> {
    let mut user = Vec::new();
    for sid in spaces {
        match crate::ffi::sls_space_type(*sid).map_err(|e| e.to_string())? {
            crate::ffi::SlsSpaceType::User => user.push(*sid),
            _ => continue,
        }
    }
    Ok(SelectionSnapshot {
        display: identifier.ok_or("display identity unreadable")?,
        spaces: user,
        current: current.ok_or("current Space unreadable")?,
    })
}

/// Fresh output key for one display index over the live snapshot (same
/// reads as [`fresh_user_row`]): the CG display id joined by exact UUID,
/// mapped to the core output name. Refuses when the join is unprovable -
/// the drain then refuses instead of switching the wrong display.
pub(crate) fn fresh_output_key(display: usize) -> Result<String, String> {
    let all = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
    let Some(row) = all.displays.get(display) else {
        return Err("display row missing".to_owned());
    };
    let want = row
        .identifier
        .as_deref()
        .ok_or("display identity unreadable")?;
    let screens =
        crate::ffi::ns_display_snapshot().map_err(|e| format!("display snapshot: {e}"))?;
    for screen in screens {
        if screen.uuid.as_deref() == Some(want) && screen.cg_id != 0 {
            return Ok(crate::topology::output_key(screen.cg_id));
        }
    }
    Err("no CG display ID joins the managed display; refusing without an exact mapping".to_owned())
}

/// Live topology revision for the bound-claim fence: the same snapshot
/// fingerprint the tile controller ticks from (one NSScreen/CG read plus
/// one SLS row read, no mutation). Read only on indicator-click drains -
/// never a steady-state poll - so an extra snapshot here costs nothing
/// unless a segment was actually clicked. `None` when any leg is
/// unreadable (the drain then fences on the SID leg alone, and `execute`
/// still drift-checks order and display identity inside).
fn live_revision() -> Option<u64> {
    let ns = crate::ffi::ns_display_snapshot().ok()?;
    let rows = crate::ffi::sls_display_rows().ok()?;
    let separate = crate::ffi::ns_separate_spaces();
    Some(crate::topology::build_snapshot(&ns, &rows, separate).revision)
}

pub trait SelectionBackend {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String>;
    /// Native display-row count for the product single-display rule (one
    /// fresh read-only row query, no mutation). The explicit batch CLI
    /// resolves its own display and never consults this.
    fn display_count(&mut self) -> Result<usize, String>;
    /// Live topology revision backing the current snapshot (`None` when
    /// the backend cannot report one - offline fakes without a revision
    /// simply skip the revision leg; bound indicator claims still fence
    /// on the SID leg). The live backend reports the fresh snapshot
    /// fingerprint.
    fn revision(&mut self) -> Option<u64> {
        None
    }
    /// Fence the reply plan's target output against the live display this
    /// backend actuates: the drain refuses a plan naming any other output
    /// instead of switching the wrong display. The pinned hardware/UUID
    /// join stays wherever the backend reads its fresh row; this only
    /// additionally matches the logical output name.
    fn check_output(&mut self, output: &str) -> Result<(), String>;
    fn empty(&mut self, sid: u64) -> Result<bool, String>;
    /// One create dispatch only; caller independently adopts an exact append.
    ///
    /// Typed mechanical disposition at the actual native touch, never a
    /// string to classify downstream:
    /// - `Ok(())` means the OS accepted the dispatch, even when arrival
    ///   is pending, the post read is unreadable, or the MC close
    ///   shortfall is unproven. The drain stays `Accepted` and lets the
    ///   core hold on genuine reads.
    /// - `Err(refused)` means no native effect was attempted (pre-call
    ///   gates, drift, unreadable pre-row, cancelled before dispatch).
    /// - `Err(threw)` means a partial or erroneous dispatch where the
    ///   effect is unknown (sent-press failures, partial posts).
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), DispatchError>;
    /// One select dispatch only; caller independently verifies arrival.
    /// Same typed disposition as [`SelectionBackend::create`].
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), DispatchError>;
}

/// Mechanical native dispatch disposition at the actual post/touch.
///
/// `Ok(())` from the backend means `Accepted`; `Err` carries whether
/// nothing was attempted (`Refused`) or a partial/error dispatch ran
/// (`Threw`). The drain stages this verbatim and never classifies error
/// strings or arrival verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchError {
    pub setter: FrameSetter,
    pub reason: String,
}

impl DispatchError {
    pub(crate) fn accepted(reason: impl Into<String>) -> Self {
        Self {
            setter: FrameSetter::Accepted,
            reason: reason.into(),
        }
    }

    pub(crate) fn from_mc(error: crate::mc_probe::McError) -> Self {
        if matches!(error, crate::mc_probe::McError::Dispatch(_)) {
            Self::threw(error.to_string())
        } else {
            Self::refused(error.to_string())
        }
    }

    #[must_use]
    pub fn refused(reason: impl Into<String>) -> Self {
        Self {
            setter: FrameSetter::Refused,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn threw(reason: impl Into<String>) -> Self {
        Self {
            setter: FrameSetter::Threw,
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for DispatchError {}

#[derive(Default)]
pub struct SelectionState {
    /// Exact-created trailing SID from the latest reply-driven execution,
    /// captured immediately after the verified append - BEFORE the select
    /// dispatch. A later select failure or ambiguous readback still leaves
    /// the Space behind, so the caller adopts this on BOTH outcomes (and
    /// clears it) instead of learning the SID only from a successful
    /// effect. `None` when the latest execution reused (no append) or never
    /// appended.
    last_created: Option<u64>,
    /// Exact-append proof rows from the latest reply-driven execution,
    /// captured with `last_created` (the `before`/`after` native rows around
    /// the single append plus the snapshot display identity). The drain
    /// adopts the created SID into lifecycle ownership from this single
    /// take; cleared together with `last_created` so a stale proof can
    /// never adopt twice.
    last_proof: Option<TrailingAppendProof>,
    /// Creation desire operation ids already spent by this owner. Spent
    /// before the native create call, at most once: a repeated drain of
    /// the same claim refuses instead of creating twice. Desire op ids are
    /// unique per engine lifetime and this state lives per owner-thread
    /// run alongside that engine, so the key needs no wider scope.
    /// The owner and this state are dropped with the runtime/core lifetime.
    spent_creations: BTreeSet<String>,
    /// Local pending native creation receipts: one per attempted desire
    /// whose post-order is not yet readable. The first successfully
    /// observed full post-order completes the receipt exactly once (original
    /// attempt stamp preserved); until then nothing is submitted. Never a
    /// fabricated before-echo or empty after-order. Survives select refusal
    /// and cancellation so physically created ownership is never lost.
    /// Keyed by creation operation id for dedup/redelivery.
    pending_creations: std::collections::BTreeMap<String, PendingCreation>,
}

/// Exact native append proof for one trailing creation: the snapshot
/// display identity plus the native rows around the single verified
/// append. Rows travel by value so the drain never re-reads topology it
/// already proved; the main side adopts `created` into lifecycle ownership
/// from this single take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingAppendProof {
    pub display_id: String,
    pub before: Vec<u64>,
    pub after: Vec<u64>,
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SelectionEffect {
    pub from: u64,
    pub target: u64,
    pub created: Option<u64>,
    pub changed: bool,
    /// Observation-time native current for the target output captured at
    /// dispatch (the reply plan baseline). Stages as the view attempt's
    /// `native_baseline`; `None` when unread at dispatch.
    pub baseline: Option<u64>,
}

impl SelectionState {
    /// Take the exact-append proof the latest reply-driven execution
    /// verified, if any (clears both the proof and the SID slot). Call
    /// after every trailing execution - success or failure - exactly once:
    /// the drain adopts the created SID into lifecycle ownership from this
    /// single take, so it can never adopt twice. Always available (the sole
    /// production caller is cross-platform; the macOS-only gate was a cfg
    /// leak).
    pub(crate) fn take_trailing_proof(&mut self) -> Option<TrailingAppendProof> {
        self.last_created.take();
        self.last_proof.take()
    }

    /// Spend one creation desire operation identity before the native call.
    /// Returns false when already spent: the caller must refuse without
    /// creating (double drain, retried claim - never a second append for
    /// one authorization).
    pub(crate) fn spend_creation(&mut self, operation_id: &str) -> bool {
        if operation_id.is_empty() {
            return false;
        }
        self.spent_creations.insert(operation_id.to_owned())
    }

    /// Whether one creation operation is already spent (duplicate claim or
    /// redelivery without a reset). Never clears pending: a duplicate
    /// redelivery must not replay the native append, but the original
    /// pending receipt still completes once when readable.
    pub(crate) fn is_creation_spent(&self, operation_id: &str) -> bool {
        self.spent_creations.contains(operation_id)
    }

    /// Retain one local pending creation receipt after a dispatched create
    /// whose post-order is not yet readable (or not yet an exact verified
    /// delta). Keeps the exact desire identity, the genuine pre-order, the
    /// actual dispatch setter, and the original attempt stamp. Idempotent
    /// per operation id: redelivery preserves the original stamp and never
    /// duplicates.
    pub(crate) fn retain_pending_creation(&mut self, pending: PendingCreation) {
        self.pending_creations
            .entry(pending.operation_id.clone())
            .or_insert(pending);
    }

    /// Pending receipt count (test and drain plumbing only).
    #[cfg(test)]
    pub(crate) fn pending_creation_count(&self) -> usize {
        self.pending_creations.len()
    }

    /// Try to complete every retained pending receipt against one fresh
    /// backend snapshot. The first successfully observed full post-order
    /// completes that operation exactly once (removed from pending) with
    /// its original attempt stamp; unreadable or invalid rows stay
    /// retained with nothing submitted. Never invents an after-order from
    /// the before echo. Returns completed reports in operation-id order.
    #[cfg(test)]
    pub(crate) fn poll_pending_creations(
        &mut self,
        backend: &mut impl SelectionBackend,
    ) -> Vec<CreationReport> {
        let Ok(row) = backend.snapshot() else {
            return Vec::new();
        };
        if row.validate().is_err() {
            return Vec::new();
        }
        let mut ready = Vec::new();
        self.pending_creations.retain(|_, pending| {
            if let Some(report) = pending.observe(&row, u64::MAX) {
                ready.push(report);
                false
            } else {
                true
            }
        });
        ready
    }

    pub(crate) fn take_pending_creations(&mut self) -> Vec<PendingCreation> {
        std::mem::take(&mut self.pending_creations)
            .into_values()
            .collect()
    }
}

/// Local pending native creation receipt: an attempted desire whose
/// post-order is not yet readable. Carries the exact desire identity, the
/// genuine pre-order, the actual dispatch setter, and the original attempt
/// monotonic stamp. Completed exactly once by the first successfully
/// observed full post-order; never submitted as a before-echo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingCreation {
    pub operation_id: String,
    pub logical: String,
    pub output: OutputId,
    pub display_id: String,
    pub order_before: Vec<u64>,
    pub setter: FrameSetter,
    pub attempt_ms: u64,
}

impl PendingCreation {
    pub(crate) fn observe(
        &self,
        row: &SelectionSnapshot,
        read_started_ms: u64,
    ) -> Option<CreationReport> {
        if row.validate().is_err()
            || row.display != self.display_id
            || read_started_ms <= self.attempt_ms
        {
            return None;
        }
        Some(CreationReport {
            operation_id: self.operation_id.clone(),
            logical: self.logical.clone(),
            output: self.output.clone(),
            setter: self.setter,
            order_before: self.order_before.clone(),
            order_after: row.spaces.clone(),
            // A later-turn delta is not causal identity proof.
            created: None,
            attempt_ms: self.attempt_ms,
        })
    }
}

/// Native creation attempt evidence for one drain-executed desire: the
/// exact desire identity (operation plus logical plus output), the
/// mechanical setter for the create leg alone, the genuine native orders
/// around it, and the verified appended desktop if any. The view attempt
/// is reported separately; this row only ever describes the create leg.
/// `order_after` is the first successfully observed full post-order, never
/// a before-echo; `attempt_ms` is the original dispatch stamp preserved
/// across dedup and redelivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreationReport {
    pub operation_id: String,
    pub logical: String,
    pub output: OutputId,
    pub setter: FrameSetter,
    pub order_before: Vec<u64>,
    pub order_after: Vec<u64>,
    pub created: Option<u64>,
    pub attempt_ms: u64,
}

/// Verified select outcome: the proven effect plus its raw attempt reads.
/// `setter` is always `Accepted` here; only verified arrivals (or the
/// already-there no-op) succeed - an unreadable post read after a
/// successful dispatch stays `Accepted` with `post: None` instead of an
/// invented `Threw`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectSuccess {
    pub effect: SelectionEffect,
    pub setter: FrameSetter,
    pub pre: Option<u64>,
    pub post: Option<u64>,
    pub creation: Option<Box<CreationReport>>,
    pub attempt_ms: u64,
}

/// Failed select outcome: the honest reason plus the genuine reads around
/// it. `setter` is `Refused` when nothing was actuated (pre-actuation
/// gates, fences, drift) and `Threw` when native work may have run
/// (ambiguous post-actuation states, never blindly repeated). `pre` is
/// the genuine fresh current wherever a row was read; `post` is a genuine
/// confirming re-read wherever one was taken (`None` where unreadable -
/// unknown holds, never a fabricated proof). `creation` rides the create
/// leg's own report only when that leg produced a readable verified
/// receipt; otherwise the receipt stays locally pending and nothing is
/// submitted. `attempt_ms` is the dispatch stamp for redelivery idempotency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectFailure {
    pub reason: String,
    pub setter: FrameSetter,
    pub pre: Option<u64>,
    pub post: Option<u64>,
    pub baseline: Option<u64>,
    pub creation: Option<Box<CreationReport>>,
    pub attempt_ms: u64,
}

/// Narrow runtime-created trailing-SID ownership for lifecycle cleanup
/// (offline pure decision only - no live touch here; the live remover calls
/// `remove_sid_verified` per eligible SID and reports residue honestly).
///
/// Rules (shared trailing-empty semantics, literal native order):
/// - Only SIDs this runtime created (`adopt_created`) are ever eligible;
///   baseline/preexisting SIDs never are - even when proven empty.
/// - An owned SID is eligible only when proven empty (`Ok(true)`), not
///   current, and removing it keeps at least
///   [`tiler_core::workspace::MIN_WORKSPACES`].
/// - Unknown emptiness (`Err`), occupied (`Ok(false)`), or current-owned
///   SIDs are never eligible: they report residue instead of removing.
/// - Foreign chrome outside the explicit evidence-based shell predicates
///   remains unknown/occupied; the live `verify_space_empty` refuses it.
///
/// Cleanup iterates owned in reverse (newest first) so a subsequent
/// occupied trailing append cleans newest-first without shifting older
/// ownership. Shutdown calls [`TrailingLifecycle::cleanup_plan`] then
/// removes each eligible SID exactly once (never replayed); unknown
/// removal leaves residue reported, never retried in the same pass.
#[derive(Debug, Clone, Default)]
pub struct TrailingLifecycle {
    baseline: BTreeSet<u64>,
    owned: Vec<u64>,
}

impl TrailingLifecycle {
    /// New owner over the preexisting baseline row (never removed).
    #[must_use]
    pub fn new(baseline: &[u64]) -> Self {
        Self {
            baseline: baseline.iter().copied().collect(),
            owned: Vec::new(),
        }
    }

    /// Adopt one runtime-created SID (exact append only: nonzero, not
    /// baseline, not already owned). Baseline SIDs and duplicates refuse.
    pub fn adopt_created(&mut self, sid: u64) -> Result<(), String> {
        if sid == 0 {
            return Err("trailing ownership: zero SID unadoptable".to_owned());
        }
        if self.baseline.contains(&sid) {
            return Err("trailing ownership: baseline SID never owned".to_owned());
        }
        if self.owned.contains(&sid) {
            return Err("trailing ownership: SID already owned".to_owned());
        }
        self.owned.push(sid);
        Ok(())
    }

    /// Owned SIDs in adoption order (oldest first).
    #[must_use]
    pub fn owned(&self) -> &[u64] {
        &self.owned
    }

    /// Pure cleanup plan over the current native row: partitions owned SIDs
    /// present in `spaces` into `(removable, residue)`. `empty` proves each
    /// owned SID (`Ok(true)` empty, `Ok(false)` occupied, `Err` unknown -
    /// unknown never removes). Current-owned, baseline, absent, occupied,
    /// unknown, and below-floor removals all report residue with an explicit
    /// reason instead of removing. Removable lists newest-first; residue
    /// lists `(sid, reason)` in owned order.
    #[must_use]
    pub fn cleanup_plan(
        &self,
        spaces: &[u64],
        current: u64,
        empty: &dyn Fn(u64) -> Result<bool, String>,
    ) -> (Vec<u64>, Vec<(u64, String)>) {
        let mut removable = Vec::new();
        let mut residue = Vec::new();
        let mut remaining = spaces.len();
        for &sid in self.owned.iter().rev() {
            if self.baseline.contains(&sid) {
                residue.push((sid, "baseline SID never removed".to_owned()));
                continue;
            }
            if !spaces.contains(&sid) {
                residue.push((sid, "owned SID absent from native row".to_owned()));
                continue;
            }
            if sid == current {
                residue.push((sid, "owned SID is current; never removed".to_owned()));
                continue;
            }
            match empty(sid) {
                Ok(true) => {
                    if remaining <= tiler_core::workspace::MIN_WORKSPACES {
                        residue.push((
                            sid,
                            "removal would breach shared minimum; residue".to_owned(),
                        ));
                        continue;
                    }
                    removable.push(sid);
                    remaining -= 1;
                }
                Ok(false) => {
                    residue.push((sid, "owned SID occupied; retained".to_owned()));
                }
                Err(reason) => {
                    residue.push((
                        sid,
                        format!("owned SID emptiness unknown; residue ({reason})"),
                    ));
                }
            }
        }
        (removable, residue)
    }

    /// Forget one SID after its verified removal (narrow ownership release).
    /// Unknown/non-owned SIDs refuse instead of mutating ownership.
    pub fn release_removed(&mut self, sid: u64) -> Result<(), String> {
        let Some(pos) = self.owned.iter().position(|s| *s == sid) else {
            return Err("trailing ownership: SID not owned".to_owned());
        };
        self.owned.remove(pos);
        Ok(())
    }
}

/// Select-only product binding subset: exactly the 19 armed rows -
/// numbered existing-Space selects `1..9`, the trailing `index 0`
/// select-or-create, the 8 relative-select rows, and
/// the previous toggle. Never adds send chords (the numbered send-follow
/// subset lives in [`crate::workspace_send::bindings`]); the trailing
/// `index 0` arms through
/// [`crate::workspace_input::TRAILING_CAPABILITY_ENABLED`],
/// and the explicit `spaces-select` batch retains it. Driven by the
/// catalogue `implemented` flags plus [`crate::workspace_input::is_armed_select`]
/// (single source with the engine admission gate); the preview arms this
/// subset with no-focus dispatch and main-thread drain.
#[must_use]
pub fn bindings() -> Vec<crate::runtime_types::TileBinding> {
    bindings_with(crate::workspace_input::TRAILING_CAPABILITY_ENABLED)
}

/// Capability-explicit binding subset: the production route above with
/// `trailing_enabled` threaded from the single capability gate. Disabled
/// drops the trailing `index 0` row (Ctrl+0) back to 18 rows; production
/// is armed at 19 rows.
#[must_use]
pub fn bindings_with(trailing_enabled: bool) -> Vec<crate::runtime_types::TileBinding> {
    crate::workspace_input::workspace_catalogue()
        .into_iter()
        .filter(|row| {
            row.implemented
                && crate::workspace_input::is_armed_select_with(&row.action, trailing_enabled)
        })
        .filter_map(|row| row.binding())
        .collect()
}

/// Adopt the runtime-created trailing SID from a verified select effect
/// into the lifecycle owner (no-op when the effect created nothing).
/// Narrow ownership bookkeeping for the tile main loop: exact-append SIDs
/// this runtime created adopt; baseline/duplicate/zero SIDs refuse without
/// mutating ownership. Returns whether an SID was adopted.
pub fn note_effect_created(
    lifecycle: &mut TrailingLifecycle,
    effect: &SelectionEffect,
) -> Result<bool, String> {
    let Some(sid) = effect.created else {
        return Ok(false);
    };
    lifecycle.adopt_created(sid)?;
    Ok(true)
}

/// Live product select gates sampled by the main-thread tile controller
/// immediately before draining one claimed select (plain booleans, no OS
/// touch here). Any denial refuses the claimed action narrowly without
/// replay and advances no history - exactly like topology drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectGates {
    pub session_unlocked: bool,
    pub secure_active: bool,
    pub lock_screen: bool,
}

/// Full-claim drain (offline-testable core of the product handoff: tests
/// inject fake backends here; the tile controller injects the live backend
/// through [`drain_live_claimed`]). Reply-driven targeting only - see
/// [`drain_claimed_select_with`]. Portable unit tests exercise this on
/// every host.
#[cfg(test)]
pub(crate) fn drain_claimed_select(
    state: &mut SelectionState,
    backend: &mut impl SelectionBackend,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
) -> Result<SelectSuccess, SelectFailure> {
    drain_claimed_select_with(
        state,
        backend,
        claim,
        gates,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Parse one `space-<sid>` native binding to its live SID (`None` on any
/// malformed, zero, or unparsable binding - never guessed).
pub(crate) fn parse_native_binding(binding: &str) -> Option<u64> {
    binding
        .strip_prefix("space-")?
        .parse::<u64>()
        .ok()
        .filter(|sid| *sid != 0)
}

/// Capability-explicit full-claim drain: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local). Core owns targeting/history/reuse-vs-create; the
/// drain only looks up the reply-issued binding live (fenced against the
/// live display identity) and creates at most once under an explicit,
/// unspent creation desire. Every path returns its typed setter plus
/// genuine reads: `Accepted` only on verified arrival (or the already-there
/// no-op — an unreadable post read after a successful dispatch stays
/// `Accepted`, never an invented `Threw`); `Refused` before any native
/// effect with a genuine confirming re-read wherever a row was read;
/// `Threw` for ambiguous post-actuation states. The core owns every
/// verdict from the staged row plus the observation — the adapter never
/// classifies error strings.
pub(crate) fn drain_claimed_select_with(
    state: &mut SelectionState,
    backend: &mut impl SelectionBackend,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
    trailing_enabled: bool,
) -> Result<SelectSuccess, SelectFailure> {
    // A new execution supersedes any unclaimed append record: each leg
    // appends at most once, and `take_trailing_proof` clears after every
    // trailing run, so a stale SID can never adopt twice. The spent
    // creation set persists across drains: one authorization creates at
    // most once, so a repeated drain refuses instead of re-appending.
    // Local pending creation receipts persist across drains until their
    // first readable full post-order completes them exactly once.
    state.last_created = None;
    state.last_proof = None;
    let baseline = claim.select_baseline_sid;
    let attempt_ms = crate::input_eligibility::now_ms();
    // Refused with no reads: nothing actuated, nothing read.
    let refused_bare = |reason: String| SelectFailure {
        reason,
        setter: FrameSetter::Refused,
        pre: None,
        post: None,
        baseline,
        creation: None,
        attempt_ms,
    };
    if !gates.session_unlocked {
        return Err(refused_bare(
            "select gated: session not unlocked".to_owned(),
        ));
    }
    if gates.secure_active {
        return Err(refused_bare("select gated: Secure Input active".to_owned()));
    }
    if gates.lock_screen {
        return Err(refused_bare("select gated: lock screen".to_owned()));
    }
    // Trailing index 0 arms only through the single capability gate (tests
    // pass an explicit local).
    if !crate::workspace_input::is_armed_select_with(&claim.action, trailing_enabled) {
        return Err(refused_bare(
            "product select refuses unarmed actions".to_owned(),
        ));
    }
    // Product targeting rule: exactly ONE native display row. Zero or
    // multiple rows refuse before any native touch - first-display guessing
    // could switch the wrong display, so limited single-display support is
    // the documented rule (multi-display per-display targeting is TBD). A
    // display plugged/unplugged mid-drain is still caught by the exact-row
    // checks below.
    match backend.display_count() {
        Ok(1) => {}
        Ok(n) => {
            return Err(refused_bare(format!(
                "product select refuses: expected exactly one native display, found {n}"
            )));
        }
        Err(e) => {
            return Err(refused_bare(format!(
                "product select refuses: display count unreadable ({e})"
            )));
        }
    }
    // Reply plan presence: the engine binds the retained view target,
    // live binding or creation desire, and baseline at dispatch. A claim
    // without an operation identity (duplicate path) or without a target
    // never actuates - missing bindings never create, never replay.
    if claim.operation_id.is_none() {
        return Err(refused_bare(
            "select refuses: unbound claim with no operation identity; no actuation".to_owned(),
        ));
    }
    let Some(target_output) = claim.select_target_output.clone() else {
        return Err(refused_bare(
            "select refuses: no retained view target; no actuation".to_owned(),
        ));
    };
    let Some(target_workspace) = claim.select_target_workspace.clone() else {
        return Err(refused_bare(
            "select refuses: no retained view target; no actuation".to_owned(),
        ));
    };
    // Logical output identity fence against the live display: the plan
    // must name the display this backend actuates, never another output.
    if let Err(reason) = backend.check_output(&target_output) {
        return Err(refused_bare(reason));
    }
    // Indicator revision fence only (render-time revision vs live): taps
    // carry no revision and skip this leg. The expected SID itself is NOT
    // an actuation target - only a staleness fence applied after the reply
    // binding resolves below.
    if claim.expected_sid.is_some()
        && let (Some(want), Some(live)) = (claim.topology_revision, backend.revision())
        && want != live
    {
        return Err(refused_bare(format!(
            "stale bound select discarded: topology revision moved between admission and drain ({want} != {live}); no select dispatched"
        )));
    }
    // One genuine confirming re-read for refusal evidence: proves the
    // desktop untouched (post equals pre) or surfaces drift/unknown
    // (post differs/unreadable). Never fabricated, never a verdict.
    let confirm_post =
        |backend: &mut dyn SelectionBackend, pre: Option<u64>| -> (Option<u64>, Option<u64>) {
            match backend.snapshot() {
                Ok(row) => match row.validate() {
                    Ok(()) => (pre, Some(row.current)),
                    Err(_) => (pre, None),
                },
                Err(_) => (pre, None),
            }
        };
    if let Some(binding) = claim.select_native_binding.clone() {
        let Some(sid) = parse_native_binding(&binding) else {
            return Err(refused_bare(format!(
                "select refuses: unparsable native binding {binding:?}; no actuation"
            )));
        };
        // Indicator staleness fence: the resolved reply binding must still
        // name the rendered SID, or the click's generation moved.
        if let Some(expected_sid) = claim.expected_sid
            && expected_sid != sid
        {
            // Fence-only path: confirm the live row for genuine evidence.
            let (pre, post) = match backend.snapshot() {
                Ok(row) => match row.validate() {
                    Ok(()) => (Some(row.current), Some(row.current)),
                    Err(_) => (None, None),
                },
                Err(_) => (None, None),
            };
            return Err(SelectFailure {
                reason: format!(
                    "stale bound select discarded: reply binding Space {sid} no longer names rendered Space {expected_sid}; no select dispatched"
                ),
                setter: FrameSetter::Refused,
                pre,
                post,
                baseline,
                creation: None,
                attempt_ms,
            });
        }
        let before = match backend.snapshot() {
            Ok(row) => row,
            Err(e) => {
                return Err(refused_bare(format!(
                    "select refuses: fresh row unreadable ({e})"
                )));
            }
        };
        if let Err(e) = before.validate() {
            return Err(refused_bare(format!(
                "select refuses: fresh row unprovable ({e})"
            )));
        }
        if !before.spaces.contains(&sid) {
            let (pre, post) = confirm_post(&mut *backend, Some(before.current));
            return Err(SelectFailure {
                reason: format!(
                    "stale bound select discarded: target Space {sid} not in live row for {target_output}/{target_workspace}; no select dispatched"
                ),
                setter: FrameSetter::Refused,
                pre,
                post,
                baseline,
                creation: None,
                attempt_ms,
            });
        }
        if before.current == sid {
            return Ok(SelectSuccess {
                effect: SelectionEffect {
                    from: before.current,
                    target: sid,
                    created: None,
                    changed: false,
                    baseline,
                },
                setter: FrameSetter::Accepted,
                pre: Some(before.current),
                post: Some(before.current),
                creation: None,
                attempt_ms,
            });
        }
        if let Err(e) = backend.select(&before, sid) {
            // Typed mechanical disposition: `Refused` means nothing was
            // attempted (genuine unchanged reads); `Threw` means a partial
            // or erroneous dispatch ran (genuine post read, never a guess).
            let post = match backend.snapshot() {
                Ok(row) => row.validate().ok().map(|()| row.current),
                Err(_) => None,
            };
            return Err(SelectFailure {
                reason: format!("{}; created SID None", e.reason),
                setter: e.setter,
                pre: Some(before.current),
                post,
                baseline,
                creation: None,
                attempt_ms,
            });
        }
        let after = match backend.snapshot() {
            Ok(row) => row,
            Err(_) => {
                // Successful dispatch with an unreadable post read stays
                // Accepted - never an invented Threw. The core holds on
                // the genuine pre-write read.
                return Ok(SelectSuccess {
                    effect: SelectionEffect {
                        from: before.current,
                        target: sid,
                        created: None,
                        changed: true,
                        baseline,
                    },
                    setter: FrameSetter::Accepted,
                    pre: Some(before.current),
                    post: None,
                    creation: None,
                    attempt_ms,
                });
            }
        };
        if after.validate().is_err()
            || after.display != before.display
            || after.spaces != before.spaces
            || after.current != sid
        {
            // Successful dispatch, but the readback disagrees: stage the
            // genuine read post so the core - not a string classifier -
            // derives third-location, abandonment, or hold.
            return Ok(SelectSuccess {
                effect: SelectionEffect {
                    from: before.current,
                    target: sid,
                    created: None,
                    changed: true,
                    baseline,
                },
                setter: FrameSetter::Accepted,
                pre: Some(before.current),
                post: (after.display == before.display)
                    .then(|| after.validate().ok().map(|()| after.current))
                    .flatten(),
                creation: None,
                attempt_ms,
            });
        }
        return Ok(SelectSuccess {
            effect: SelectionEffect {
                from: before.current,
                target: sid,
                created: None,
                changed: true,
                baseline,
            },
            setter: FrameSetter::Accepted,
            pre: Some(before.current),
            post: Some(sid),
            creation: None,
            attempt_ms,
        });
    }
    // Unbound target: only an explicit creation desire authorizes exactly
    // one native append. The desire must name this exact logical on this
    // exact output; anything else refuses without creating. Missing
    // bindings never create.
    let Some(creation_op) = claim.select_creation_operation.clone() else {
        return Err(refused_bare(
            "select refuses: target unbound with no creation desire; missing binding never creates"
                .to_owned(),
        ));
    };
    if claim.select_creation_logical.as_deref() != Some(target_workspace.as_str())
        || claim.select_creation_output.as_deref() != Some(target_output.as_str())
    {
        return Err(refused_bare(
            "select refuses: creation desire mismatches retained target; no actuation".to_owned(),
        ));
    }
    // Duplicate or redelivery of an already-spent desire refuses without
    // replay, but the original pending receipt (if any) is preserved with
    // its original stamp: no second native append, never a dropped
    // ownership.
    if state.is_creation_spent(&creation_op) {
        let (pre, post) = match backend.snapshot() {
            Ok(row) => match row.validate() {
                Ok(()) => (Some(row.current), Some(row.current)),
                Err(_) => (None, None),
            },
            Err(_) => (None, None),
        };
        return Err(SelectFailure {
            reason: format!("select refuses: creation {creation_op} already spent; no replay"),
            setter: FrameSetter::Refused,
            pre,
            post,
            baseline,
            creation: None,
            attempt_ms,
        });
    }
    let mk_creation = |setter: FrameSetter,
                       order_before: Vec<u64>,
                       order_after: Vec<u64>,
                       created: Option<u64>| {
        Box::new(CreationReport {
            operation_id: creation_op.clone(),
            logical: target_workspace.clone(),
            output: OutputId(target_output.clone()),
            setter,
            order_before,
            order_after,
            created,
            attempt_ms,
        })
    };
    // Real readable full pre-order is required before any actuation: an
    // unreadable or unprovable row refuses with no native create and no
    // fabricated receipt. Spending happens only after this gate, so an
    // unreadable pre-order never consumes the single authorization.
    let before = match backend.snapshot() {
        Ok(row) => row,
        Err(e) => {
            return Err(SelectFailure {
                reason: format!("select refuses: fresh row unreadable ({e})"),
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                baseline,
                creation: None,
                attempt_ms,
            });
        }
    };
    if let Err(e) = before.validate() {
        return Err(SelectFailure {
            reason: format!("select refuses: fresh row unprovable ({e})"),
            setter: FrameSetter::Refused,
            pre: None,
            post: None,
            baseline,
            creation: None,
            attempt_ms,
        });
    }
    if before.spaces.len() >= tiler_core::workspace::MAX_ORDERED {
        let (pre, post) = confirm_post(&mut *backend, Some(before.current));
        return Err(SelectFailure {
            reason: "shared workspace catalogue bound reached".to_owned(),
            setter: FrameSetter::Refused,
            pre,
            post,
            baseline,
            creation: None,
            attempt_ms,
        });
    }
    // Spend exactly once per core lifetime, immediately before the native
    // call. A repeated drain refuses above without a second append.
    if !state.spend_creation(&creation_op) {
        let (pre, post) = match backend.snapshot() {
            Ok(row) => match row.validate() {
                Ok(()) => (Some(row.current), Some(row.current)),
                Err(_) => (None, None),
            },
            Err(_) => (None, None),
        };
        return Err(SelectFailure {
            reason: format!("select refuses: creation {creation_op} already spent; no replay"),
            setter: FrameSetter::Refused,
            pre,
            post,
            baseline,
            creation: None,
            attempt_ms,
        });
    }
    // Helper: retain a local pending receipt for a dispatched create whose
    // post-order is not yet a verified exact append. Nothing is submitted
    // until the first successfully observed full post-order completes it
    // exactly once with the original stamp.
    let retain_pending = |state: &mut SelectionState, setter: FrameSetter| {
        state.retain_pending_creation(PendingCreation {
            operation_id: creation_op.clone(),
            logical: target_workspace.clone(),
            output: OutputId(target_output.clone()),
            display_id: before.display.clone(),
            order_before: before.spaces.clone(),
            setter,
            attempt_ms,
        });
    };
    if let Err(e) = backend.create(&before) {
        // Typed disposition, never a string classifier:
        // - Refused: no native effect was attempted. The genuine re-read
        //   (when readable) proves the desktop untouched.
        // - Threw: a partial or erroneous dispatch may have run. The
        //   drain reports the typed setter verbatim so the core holds
        //   instead of settling a proven refusal. Retain a local pending
        //   receipt with the genuine pre-order and original stamp when the
        //   post is unreadable; submit nothing fabricated.
        if e.setter == FrameSetter::Refused {
            let after = backend
                .snapshot()
                .ok()
                .filter(|row| row.display == before.display && row.validate().is_ok());
            return Err(SelectFailure {
                reason: e.reason,
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                baseline,
                creation: after.map(|after| {
                    mk_creation(
                        FrameSetter::Refused,
                        before.spaces.clone(),
                        after.spaces,
                        None,
                    )
                }),
                attempt_ms,
            });
        }
        let after = backend
            .snapshot()
            .ok()
            .filter(|row| row.display == before.display && row.validate().is_ok());
        let creation =
            after.map(|after| mk_creation(e.setter, before.spaces.clone(), after.spaces, None));
        if creation.is_none() {
            retain_pending(state, e.setter);
        }
        return Err(SelectFailure {
            reason: e.reason,
            setter: e.setter,
            pre: None,
            post: None,
            baseline,
            creation,
            attempt_ms,
        });
    }
    let after = match backend.snapshot() {
        Ok(row) => row,
        Err(_) => {
            // Dispatched but post unreadable: retain locally until the
            // first readable full post-order. Submit nothing fabricated;
            // the view holds on the genuine pre-write read.
            retain_pending(state, FrameSetter::Accepted);
            return Err(SelectFailure {
                reason: "create dispatched; post-order unreadable, receipt pending".to_owned(),
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                baseline,
                creation: None,
                attempt_ms,
            });
        }
    };
    if after.validate().is_err()
        || after.display != before.display
        || after.current != before.current
        || after.spaces.len() != before.spaces.len() + 1
        || !after.spaces.starts_with(&before.spaces)
    {
        // Readable but not an exact same-display append. A readable row
        // with no delta means the desktop is simply not visible yet:
        // retain pending so the first later delta completes exactly once
        // (emitting a no-delta now would turn that later delta into a
        // changed replay). A genuinely disagreeing row (outside race or
        // multiple additions) completes immediately as Threw with the
        // genuine after-image and no guessed SID; otherwise retain.
        let readable = after.validate().is_ok() && after.display == before.display;
        let no_delta = readable && after.spaces == before.spaces && after.current == before.current;
        if readable && !no_delta {
            let order_after = after.spaces.clone();
            return Err(SelectFailure {
                reason: "create effect is not an exact same-display append; adoption refused"
                    .to_owned(),
                setter: FrameSetter::Threw,
                pre: None,
                post: None,
                baseline,
                creation: Some(mk_creation(
                    FrameSetter::Threw,
                    before.spaces.clone(),
                    order_after,
                    None,
                )),
                attempt_ms,
            });
        }
        retain_pending(state, FrameSetter::Accepted);
        return Err(SelectFailure {
            reason: if readable {
                "create effect not yet visible; receipt pending".to_owned()
            } else {
                "create effect unreadable; receipt pending".to_owned()
            },
            setter: FrameSetter::Refused,
            pre: None,
            post: None,
            baseline,
            creation: None,
            attempt_ms,
        });
    }
    let sid = *after.spaces.last().expect("appended row");
    // Capture immediately after the verified append: the Space exists from
    // here even if the select below fails or reads back ambiguous.
    state.last_created = Some(sid);
    state.last_proof = Some(TrailingAppendProof {
        display_id: before.display.clone(),
        before: before.spaces.clone(),
        after: after.spaces.clone(),
        created: sid,
    });
    let created = Some(sid);
    let creation_ok = mk_creation(
        FrameSetter::Accepted,
        before.spaces.clone(),
        after.spaces.clone(),
        created,
    );
    let now = match backend.snapshot() {
        Ok(row) => row,
        Err(e) => {
            return Err(SelectFailure {
                reason: format!(
                    "workspace topology unreadable before select; created SID {created:?} ({e})"
                ),
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                baseline,
                creation: Some(creation_ok),
                attempt_ms,
            });
        }
    };
    if now.validate().is_err()
        || now.display != after.display
        || now.spaces != after.spaces
        || now.current != before.current
    {
        return Err(SelectFailure {
            reason: format!(
                "workspace topology changed before select; created SID {created:?}; no select dispatched"
            ),
            setter: FrameSetter::Refused,
            pre: Some(before.current),
            post: now.validate().ok().map(|()| now.current),
            baseline,
            creation: Some(creation_ok),
            attempt_ms,
        });
    }
    if let Err(e) = backend.select(&now, sid) {
        // Verified append, typed select disposition: the Space persists
        // (adopted from the proof on both outcomes) while the view holds.
        // `Refused` means the select never ran; `Threw` means it may have.
        let post = match backend.snapshot() {
            Ok(row) => row.validate().ok().map(|()| row.current),
            Err(_) => None,
        };
        return Err(SelectFailure {
            reason: format!("{}; created SID {created:?}", e.reason),
            setter: e.setter,
            pre: Some(now.current),
            post,
            baseline,
            creation: Some(creation_ok),
            attempt_ms,
        });
    }
    let settled = match backend.snapshot() {
        Ok(row) => row,
        Err(_) => {
            // Successful dispatch with an unreadable post read stays
            // Accepted - never an invented Threw.
            return Ok(SelectSuccess {
                effect: SelectionEffect {
                    from: before.current,
                    target: sid,
                    created,
                    changed: true,
                    baseline,
                },
                setter: FrameSetter::Accepted,
                pre: Some(before.current),
                post: None,
                creation: Some(creation_ok),
                attempt_ms,
            });
        }
    };
    if settled.validate().is_err()
        || settled.display != now.display
        || settled.spaces != now.spaces
        || settled.current != sid
    {
        // Successful dispatch, disagreeing readback: stage the genuine
        // read so the core derives third-location, abandonment, or hold.
        return Ok(SelectSuccess {
            effect: SelectionEffect {
                from: before.current,
                target: sid,
                created,
                changed: true,
                baseline,
            },
            setter: FrameSetter::Accepted,
            pre: Some(before.current),
            post: settled.validate().ok().map(|()| settled.current),
            creation: Some(creation_ok),
            attempt_ms,
        });
    }
    Ok(SelectSuccess {
        effect: SelectionEffect {
            from: before.current,
            target: sid,
            created,
            changed: true,
            baseline,
        },
        setter: FrameSetter::Accepted,
        pre: Some(before.current),
        post: Some(sid),
        creation: Some(creation_ok),
        attempt_ms,
    })
}

/// Product drain budget (seconds): 30 = the 15 s MC cleanup reserve (which
/// main-phase legs must clear - a smaller budget would refuse every leg
/// inside the reserve) plus a 15 s work window, which covers the worst-case
/// single-goto wall time (~8 s action timeout + 0.3 s stabilize + press
/// legs + 3 s effect settle ≈ 12 s) with margin while halving the previous
/// 60 s main-pump stall exposure. No cleanup legs run in the product drain.
/// Live-macOS-only constant (consumed by the macOS-gated live drain).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PRODUCT_DRAIN_BUDGET_SECS: u64 = 30;
/// Caffeinate companion bound (seconds) around the product drain: covers
/// the drain budget plus reap margin and self-terminates via `-t` even if
/// the reaper wedges. The held child handle is signalled/waited by exact
/// PID only (never by name); `Drop` is the backstop.
/// Live-macOS-only constant (consumed by the macOS-gated live drain).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PRODUCT_DRAIN_CAFFEINATE_SECS: u64 = 60;

/// Executable-owner label for the trailing-probe diagnostic line. Exact
/// matches on the two proven system executables only; everything else reads
/// `other`, unreadable reads `unreadable`. Never prints a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrailingExeKind {
    WindowServer,
    A11yServer,
    Other,
    Unreadable,
}

impl TrailingExeKind {
    fn of(exe: Option<&str>) -> Self {
        match exe {
            Some(path) if path == crate::mc_probe::WINDOWSERVER_EXE => Self::WindowServer,
            Some(path) if path == crate::mc_probe::A11Y_SERVER_EXE => Self::A11yServer,
            Some(_) => Self::Other,
            None => Self::Unreadable,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::WindowServer => "window-server",
            Self::A11yServer => "a11y-server",
            Self::Other => "other",
            Self::Unreadable => "unreadable",
        }
    }
}

/// First blocking candidate at the trailing-emptiness decision: the exact
/// window that proves occupied (`Ok(false)`) or unknown (`Err`). IDs plus
/// geometry and membership only - never titles, content, or paths.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrailingProbeBlocker {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub onscreen: bool,
    pub w: f64,
    pub h: f64,
    pub members: Option<Vec<u64>>,
    pub self_probe: bool,
    pub exe_kind: TrailingExeKind,
}

/// One bounded metadata-only trailing-probe line: SID, verdict, counts,
/// and the exact first blocker when present. Pure; the live probe emits
/// exactly one such line per outcome. No titles, content, or paths.
#[must_use]
pub(crate) fn format_trailing_probe_line(
    sid: u64,
    verdict: &str,
    checked: usize,
    shell_skipped: usize,
    blocker: Option<&TrailingProbeBlocker>,
    reason: Option<&str>,
    self_pid: u32,
) -> String {
    let mut line = format!(
        "tiler-macos select route=native phase=trailing-probe sid={sid} verdict={verdict} checked={checked} shell_skipped={shell_skipped} self_pid={self_pid}"
    );
    if let Some(blocker) = blocker {
        line.push_str(&format!(
            " blocker=WID{}/PID{} bundle={} layer={} onscreen={} wh={:.0}x{:.0} members={:?} self_probe={} exe={}",
            blocker.wid,
            blocker.pid,
            blocker.bundle,
            blocker.layer,
            blocker.onscreen,
            blocker.w,
            blocker.h,
            blocker.members,
            blocker.self_probe,
            blocker.exe_kind.label(),
        ));
    }
    if let Some(reason) = reason {
        line.push_str(&format!(" reason={reason}"));
    }
    line
}

/// Pure injected scan core behind [`probe_trailing_emptiness`]: identical
/// verdict rule over caller-supplied reads (user-SID set plus one
/// `(window, membership)` pair per enumerated window, in enumeration
/// order). Returns the verdict plus the counts and first blocker the
/// diagnostic line reports. Unknown membership or zero-area aborts with
/// the same `Err` strings as the live probe, so an earlier occupied
/// candidate yields to the later unknown - never guessed empty.
pub(crate) fn scan_trailing_windows(
    sid: u64,
    self_pid: u32,
    user: &[u64],
    windows: &[(crate::mc_probe::WindowMember, Result<Vec<u64>, String>)],
) -> (
    Result<bool, String>,
    usize,
    usize,
    Option<TrailingProbeBlocker>,
) {
    let mut checked = 0usize;
    let mut shell_skipped = 0usize;
    let mut first_occupied: Option<TrailingProbeBlocker> = None;
    for (member, spaces) in windows {
        if crate::mc_probe::classify_window_full(
            member,
            spaces.as_ref().ok().map(Vec::as_slice),
            user,
            sid,
        )
        .0 == crate::mc_probe::WindowClass::Shell
        {
            shell_skipped += 1;
            continue;
        }
        checked += 1;
        let detail = |members: Option<Vec<u64>>| TrailingProbeBlocker {
            wid: member.wid,
            pid: member.pid,
            bundle: member.bundle.clone(),
            layer: member.layer,
            onscreen: member.onscreen,
            w: member.w,
            h: member.h,
            members,
            self_probe: member.pid == self_pid,
            exe_kind: TrailingExeKind::of(member.exe.as_deref()),
        };
        let owned_spaces = match spaces {
            Ok(spaces) => spaces.clone(),
            Err(_) => {
                let blocker = detail(None);
                return (
                    Err("workspace emptiness unknown".to_owned()),
                    checked,
                    shell_skipped,
                    Some(blocker),
                );
            }
        };
        match candidate_occupied(
            member.wid,
            member.pid,
            member.w,
            member.h,
            &owned_spaces,
            sid,
        ) {
            Ok(true) => {
                if first_occupied.is_none() {
                    first_occupied = Some(detail(Some(owned_spaces)));
                }
            }
            Ok(false) => {}
            Err(reason) => {
                let blocker = detail(Some(owned_spaces));
                return (Err(reason), checked, shell_skipped, Some(blocker));
            }
        }
    }
    match first_occupied {
        Some(blocker) => (Ok(false), checked, shell_skipped, Some(blocker)),
        None => (Ok(true), checked, shell_skipped, None),
    }
}

/// Proven-emptiness scan for one trailing SID over fresh native reads (SLS
/// user rows plus full window membership; shell entries skip, zero-area and
/// unreadable candidates read unknown - never empty). Shared by the
/// main-thread select backend and the effect-owner trailing preparation so
/// both prove the identical rule: no AppKit touch anywhere inside, safe on
/// any thread.
pub(crate) fn probe_trailing_emptiness(sid: u64) -> Result<bool, String> {
    let self_pid = std::process::id();
    let user = crate::mc_observe::fresh_user_spaces().map_err(|e| e.to_string())?;
    let all = crate::mc_observe::enumerate_window_members().map_err(|e| e.to_string())?;
    let windows: Vec<(crate::mc_probe::WindowMember, Result<Vec<u64>, String>)> = all
        .into_iter()
        .map(|w| {
            let member = crate::mc_probe::WindowMember {
                wid: w.wid,
                pid: w.pid,
                bundle: w.bundle,
                layer: w.layer,
                onscreen: w.onscreen,
                w: w.w,
                h: w.h,
                exe: w.exe,
                exe_alias: w.exe_alias,
                ax_windows_absent: w.ax_windows_absent,
            };
            (member, w.spaces)
        })
        .collect();
    let (verdict, checked, shell_skipped, blocker) =
        scan_trailing_windows(sid, self_pid, &user, &windows);
    let (verdict_str, reason) = match verdict.as_ref() {
        Ok(true) => ("empty", None),
        Ok(false) => ("occupied", None),
        Err(reason) => ("unknown", Some(reason.clone())),
    };
    eprintln!(
        "{}",
        format_trailing_probe_line(
            sid,
            verdict_str,
            checked,
            shell_skipped,
            blocker.as_ref(),
            reason.as_deref(),
            self_pid,
        )
    );
    verdict
}

/// Live product drain for one claimed armed select: builds the proven
/// [`NativeSelection`] backend over the single native display row and runs
/// [`drain_claimed`]. The single-display rule inside `drain_claimed`
/// refuses zero/multiple rows before any MC touch, so `display: 0` names
/// the only row - never a guess. Reuses the native selector, never
/// duplicates it. Blocks the caller up to the [`PRODUCT_DRAIN_BUDGET_SECS`]
/// budget plus the effect settle window under a bounded reaped
/// caffeinate companion; the tap callback and the serial engine never
/// block (they handed off asynchronously through the claim slot).
/// Returns the verified effect plus whether the caffeinate companion
/// reaped cleanly (arrival truth dominates: a verified effect still
/// returns `Ok` when only the reaper failed).
/// Live-macOS-only outside tests (the non-test caller is gated to macOS);
/// kept compiled on every host for the portable unit tests.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn drain_live_claimed(
    state: &mut SelectionState,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
) -> Result<(SelectSuccess, bool), SelectFailure> {
    drain_live_claimed_with(
        state,
        claim,
        gates,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit live drain: the production route above with
/// `trailing_enabled` threaded from the single capability gate.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn drain_live_claimed_with(
    state: &mut SelectionState,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
    trailing_enabled: bool,
) -> Result<(SelectSuccess, bool), SelectFailure> {
    if !crate::workspace_input::is_armed_select_with(&claim.action, trailing_enabled) {
        return Err(SelectFailure {
            reason: "product select refuses unarmed actions".to_owned(),
            setter: FrameSetter::Refused,
            pre: None,
            post: None,
            baseline: claim.select_baseline_sid,
            creation: None,
            attempt_ms: crate::input_eligibility::now_ms(),
        });
    }
    let mut awake =
        match crate::mc_lifecycle::live::CaffeinateGuard::spawn(PRODUCT_DRAIN_CAFFEINATE_SECS) {
            Ok(awake) => awake,
            Err(e) => {
                return Err(SelectFailure {
                    reason: format!("product select refuses: caffeinate unspawnable ({e})"),
                    setter: FrameSetter::Refused,
                    pre: None,
                    post: None,
                    baseline: claim.select_baseline_sid,
                    creation: None,
                    attempt_ms: crate::input_eligibility::now_ms(),
                });
            }
        };
    let mut backend = NativeSelection {
        display: 0,
        budget: crate::mc_probe::ActionBudget::new(PRODUCT_DRAIN_BUDGET_SECS),
    };
    let outcome = drain_claimed_select_with(state, &mut backend, claim, gates, trailing_enabled);
    let reaped = awake.stop();
    outcome.map(|success| (success, reaped))
}

/// Live proven-emptiness read for one trailing SID on the tile main
/// thread (same proof as the drain's `NativeSelection::empty`: managed
/// windows homed exactly to the SID, shell excluded, zero-area/unreadable
/// candidates unknown - never empty). Bounded under its own short budget;
/// unknown refuses removal instead of guessing. Shutdown-path only: the
/// cadence never runs removals mid-loop.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn trailing_emptiness(sid: u64, display: usize) -> Result<bool, String> {
    let mut backend = NativeSelection {
        display,
        budget: crate::mc_probe::ActionBudget::new(TRAILING_CLEANUP_BUDGET_SECS),
    };
    backend.empty(sid)
}

/// Bounded shutdown-cleanup budget (seconds): one worst-case verified
/// removal runs an MC open leg plus a remove press (each bounded by the
/// [`crate::mc_probe::MC_ACTION_TIMEOUT_MS`] 8 s action timeout) plus the
/// [`crate::mc_probe::MC_EFFECT_SETTLE_MS`] 3 s async-settle poll, with the
/// per-SID emptiness proof (one CG/SLS enumeration, sub-second) on top -
/// ≈12 s worst case per owned SID. 30 s covers the common single-owned-SID
/// shutdown with margin inside the same order as
/// [`PRODUCT_DRAIN_BUDGET_SECS`]; the loop re-proves fresh per removal and
/// any budget exhaustion surfaces as honest per-SID residue, never a silent
/// skip. No cleanup legs run in the product select drain.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const TRAILING_CLEANUP_BUDGET_SECS: u64 = 30;

/// After shell classification, zero-area candidate membership is ambiguous,
/// not proof of either a reusable empty Space or an occupied document.
pub(crate) fn candidate_occupied(
    wid: u32,
    pid: u32,
    w: f64,
    h: f64,
    spaces: &[u64],
    sid: u64,
) -> Result<bool, String> {
    if !spaces.contains(&sid) {
        return Ok(false);
    }
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(format!(
            "trailing emptiness unknown: zero-area/unreadable candidate WID{wid}/PID{pid}"
        ));
    }
    Ok(true)
}

/// One reply-driven diagnostic batch step over an injected backend: map
/// the action to its planner-owned core command (output `None` - the core
/// resolves cursor/focus, never an adapter guess), dispatch it through a
/// fresh sequencer ticket, bind the accepted reply's plan, drain it
/// through the production executor, and stage the raw view plus creation
/// attempts back into the session for the next report. No inbox, no
/// threads: the whole step runs synchronously on the caller. Returns the
/// verified success or the typed failure (both staged before return).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_batch_step(
    session: &mut crate::world_runtime::WorldSession,
    retained: &mut crate::runtime_core::RetainedCore,
    state: &mut SelectionState,
    ctx: &std::sync::Arc<crate::runtime_types::NativeContext>,
    backend: &mut impl SelectionBackend,
    action: InputSemanticAction,
    trailing_enabled: bool,
    gates: &SelectGates,
) -> Result<SelectSuccess, SelectFailure> {
    use crate::world_command_map::select_command_with_capability;
    use crate::world_runtime::{DispatchContext, build_dispatch_report};
    let fail = |reason: String| SelectFailure {
        reason,
        setter: FrameSetter::Refused,
        pre: None,
        post: None,
        baseline: None,
        creation: None,
        attempt_ms: crate::input_eligibility::now_ms(),
    };
    let command = match select_command_with_capability(&action, None, trailing_enabled) {
        Some(command) => command,
        None => {
            return Err(fail(format!(
                "spaces-select refuses unarmed action {action:?}; no actuation"
            )));
        }
    };
    let ticket = match session.sequencer().try_begin_request(Some(command.clone())) {
        Some(ticket) => ticket,
        None => {
            return Err(fail(
                "spaces-select refuses: engine single-flight busy; no actuation".to_owned(),
            ));
        }
    };
    let Some(report) = build_dispatch_report(
        session,
        retained,
        &DispatchContext::Native(std::sync::Arc::clone(ctx)),
        &[],
        ticket.observation_seq,
        ticket.request_seq,
        ticket.command_seq,
        Some(command),
        None,
        &ticket.request_id,
    ) else {
        session.sequencer().clear_flight();
        return Err(fail(
            "spaces-select refuses: no native context to report; no actuation".to_owned(),
        ));
    };
    let reply = match session.engine_mut().handle_report(&report) {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
        tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
        tiler_core::world_engine::WorldOutcome::Duplicate(_) => {
            session.sequencer().clear_flight();
            return Err(fail(
                "spaces-select refuses: duplicate report; no actuation".to_owned(),
            ));
        }
        tiler_core::world_engine::WorldOutcome::Rejected { kind, message } => {
            session.sequencer().clear_flight();
            return Err(fail(format!(
                "spaces-select refused: core rejected {kind} ({message}); no actuation"
            )));
        }
    };
    match session.sequencer().accept_reply(&reply) {
        crate::world_sequencer::ReplyVerdict::Applied(_) => {}
        crate::world_sequencer::ReplyVerdict::Refused { reason } => {
            session.sequencer().clear_flight();
            return Err(fail(format!(
                "spaces-select refuses: reply refused as {reason}; no actuation"
            )));
        }
    }
    let operation = ticket
        .command_seq
        .map(crate::world_sequencer::command_operation_id);
    let mut claim = crate::runtime_types::ClaimedSelect::tap(action);
    claim.operation_id = operation.clone();
    if let Some(op) = operation.as_deref() {
        crate::runtime_thread::bind_select_plan(session, ctx, &mut claim, op);
    }
    // The drain actuates exactly the bound plan and returns its typed
    // setter plus genuine reads; stage both rows before returning so the
    // next batch action settles against proven evidence. The batch state
    // owns spend-once across actions: one desire creates at most once per
    // batch, and the proof adopts from the same state.
    let op_id = operation.clone().unwrap_or_default();
    match drain_claimed_select_with(state, backend, claim, gates, trailing_enabled) {
        Ok(success) => {
            session.stage_view_attempt(
                &op_id,
                success.setter,
                success.pre,
                success.post,
                success.effect.baseline,
            );
            if let Some(report) = success.creation.as_ref() {
                session.stage_native_creation(
                    &report.operation_id,
                    &report.logical,
                    report.output.clone(),
                    report.setter,
                    &report.order_before,
                    &report.order_after,
                    report.created,
                    report.attempt_ms,
                );
            }
            Ok(success)
        }
        Err(failure) => {
            session.stage_view_attempt(
                &op_id,
                failure.setter,
                failure.pre,
                failure.post,
                failure.baseline,
            );
            if let Some(report) = failure.creation.as_ref() {
                session.stage_native_creation(
                    &report.operation_id,
                    &report.logical,
                    report.output.clone(),
                    report.setter,
                    &report.order_before,
                    &report.order_after,
                    report.created,
                    report.attempt_ms,
                );
            }
            Err(failure)
        }
    }
}

pub fn run(display: usize, actions: &[InputSemanticAction]) -> Result<String, (i32, String)> {
    if actions.is_empty()
        || actions.len() > 16
        || actions.iter().any(|a| {
            !matches!(
                a,
                InputSemanticAction::WorkspaceSelect { .. }
                    | InputSemanticAction::WorkspaceRelative { .. }
                    | InputSemanticAction::WorkspacePrevious
            )
        })
    {
        return Err((
            2,
            "spaces-select needs 1..=16 select-only --action IDs (bounded native batch)".to_owned(),
        ));
    }
    crate::mc_lifecycle::live::action_gates().map_err(|e| (1, e.to_string()))?;
    let mut awake =
        crate::mc_lifecycle::live::CaffeinateGuard::spawn(120).map_err(|e| (1, e.to_string()))?;
    // Ephemeral reply-driven engine for the batch: the same WorldEngine
    // plus adopted native topology the product lane converges through, so
    // each action dispatches its positional core command, binds the
    // accepted reply's plan, drains through the production executor, and
    // stages raw attempts back into the session for the next report.
    // History lives core-side across the batch and dies with it.
    let clock = std::sync::Arc::new(crate::world_sequencer::ObservationClock::new());
    let mut session = crate::world_runtime::WorldSession::new(
        tiler_core::size_hints::FixedSizePredicate::BothAxes,
        true,
        crate::focus_route::FocusRoute::Slps,
        clock,
    );
    let mut retained = crate::runtime_core::RetainedCore::default();
    let mut select_state = SelectionState::default();
    let mut backend = NativeSelection {
        display,
        budget: crate::mc_probe::ActionBudget::new(120),
    };
    let mut effects = Vec::new();
    let mut failure: Option<String> = None;
    let mut created_sids: Vec<u64> = Vec::new();
    for action in actions {
        let start = std::time::Instant::now();
        // Truthful native facts per action: fresh snapshot plus fresh
        // gates - never a cached topology.
        let ns = crate::ffi::ns_display_snapshot().map_err(|e| {
            (
                1,
                format!("spaces-select refuses: display snapshot unreadable ({e})"),
            )
        })?;
        let rows = crate::ffi::sls_display_rows().map_err(|e| {
            (
                1,
                format!("spaces-select refuses: space rows unreadable ({e})"),
            )
        })?;
        let separate = crate::ffi::ns_separate_spaces();
        let snapshot = crate::topology::build_snapshot(&ns, &rows, separate);
        let (ctx, _skipped) = crate::runtime_types::NativeContext::derive(&snapshot, &|sid| {
            crate::ffi::sls_space_type(sid).ok()
        });
        let ctx = std::sync::Arc::new(ctx);
        let gates = SelectGates {
            session_unlocked: matches!(
                crate::ffi::session_state().lock,
                crate::ffi::SessionLock::Unlocked
            ),
            secure_active: matches!(crate::ffi::secure_input_known(), Some(true)),
            lock_screen: crate::ffi::frontmost_app()
                .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
                .unwrap_or(true),
        };
        let outcome = run_batch_step(
            &mut session,
            &mut retained,
            &mut select_state,
            &ctx,
            &mut backend,
            action.clone(),
            crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
            &gates,
        );
        match outcome {
            Ok(success) => {
                // Adopt verified appends into the batch creation record.
                if let Some(proof) = select_state.take_trailing_proof() {
                    created_sids.push(proof.created);
                }
                effects.push(
                    serde_json::json!({"effect": success.effect, "ms": start.elapsed().as_millis()}),
                );
            }
            Err(failed) => {
                // A failed append still persists: adopt it before reporting.
                if let Some(proof) = select_state.take_trailing_proof() {
                    created_sids.push(proof.created);
                }
                failure = Some(failed.reason.clone());
                effects.push(
                    serde_json::json!({"refused": failed.reason, "ms": start.elapsed().as_millis()}),
                );
                break;
            }
        }
    }
    let closed = crate::mc_lifecycle::live::mc_close().map_err(|e| e.to_string());
    let layout_after = crate::mc_lifecycle::live::fresh_snapshot()
        .map(|s| s.sanitized())
        .map_err(|e| e.to_string());
    let reaped = awake.stop();
    let report = serde_json::json!({"action":"spaces-select","effects":effects,"failure":failure,"created_sids":created_sids,"layout_after":layout_after,
        "mc_close":closed,"caffeinate_reaped":reaped,
        "note":"reply-driven batch: core targeting with live binding lookup and explicit creation desires; history lasts this batch only"}).to_string();
    if failure.is_none() && closed.is_ok() && layout_after.is_ok() && reaped {
        Ok(report)
    } else {
        Err((1, report))
    }
}

struct NativeSelection {
    display: usize,
    budget: crate::mc_probe::ActionBudget,
}

/// Explicit exact-SID cleanup, including batches that appended more than one
/// Space. Existing two-Space `mc-recover` deliberately keeps its strict shape.
/// No implicit selection or broad cleanup: occupied/current/last/unknown refuses.
pub fn remove_empty(space: u64, allow_untested: bool) -> Result<String, (i32, String)> {
    if space == 0 || !allow_untested {
        return Err((
            2,
            "mc-remove-empty needs nonzero SID and --allow-untested".to_owned(),
        ));
    }
    use crate::mc_lifecycle::live;
    live::action_gates().map_err(|e| (1, e.to_string()))?;
    let protected =
        crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| (1, e.to_string()))?;
    let mut awake = live::CaffeinateGuard::spawn(60).map_err(|e| (1, e.to_string()))?;
    let mut budget = crate::mc_probe::ActionBudget::new(60);
    let mut dispatched = false;
    let result =
        crate::mc_lifecycle::remove_sid_verified(space, &mut budget, false, &mut dispatched)
            .map(|(scan, close)| serde_json::json!({"scan":scan,"close_shortfall":close}))
            .map_err(|e| e.to_string());
    let closed = live::mc_close().map_err(|e| e.to_string());
    let protected_restored = crate::mc_observe::await_ghostty_restored(&protected).is_ok();
    let reaped = awake.stop();
    let ok = result
        .as_ref()
        .is_ok_and(|v| v["close_shortfall"].is_null())
        && closed.is_ok()
        && protected_restored
        && reaped;
    let body = serde_json::json!({"action":"mc-remove-empty","space":space,"result":result,
        "remove_dispatched":dispatched,"mc_close":closed,"protected_restored":protected_restored,"caffeinate_reaped":reaped,"complete":ok}).to_string();
    if ok { Ok(body) } else { Err((1, body)) }
}

impl SelectionBackend for NativeSelection {
    fn display_count(&mut self) -> Result<usize, String> {
        fresh_display_count()
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        fresh_user_row(self.display)
    }
    fn revision(&mut self) -> Option<u64> {
        live_revision()
    }
    fn check_output(&mut self, output: &str) -> Result<(), String> {
        let live = fresh_output_key(self.display)?;
        if live != output {
            return Err(format!(
                "product select refuses: reply target output {output} is not the live display {live}; no actuation"
            ));
        }
        Ok(())
    }
    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        probe_trailing_emptiness(sid)
    }
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), DispatchError> {
        if self
            .snapshot()
            .map_err(|e| DispatchError::refused(format!("pre-create row unreadable ({e})")))?
            != *before
        {
            return Err(DispatchError::refused("pre-create topology drift"));
        }
        let outcome =
            crate::mc_lifecycle::live::mc_add_space(self.display, &mut self.budget, false)
                .map_err(DispatchError::from_mc)?;
        if let Some(e) = outcome.close_error {
            eprintln!("tiler-macos select create close shortfall accepted ({e})");
        }
        Ok(())
    }
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), DispatchError> {
        // Fresh gates immediately pre-dispatch: a lock, Secure Input
        // capture, or lock-screen foreground that closed after the drain
        // sampled its gates refuses here instead of pressing into it.
        // Cheap read-only calls (session/Secure Input are any-thread C
        // calls; the frontmost model-object read carries the reviewed
        // off-main justification); unknown session denies. No atomicity is
        // promised past this point - a transition mid-press is still
        // refused without replay by the drift and readback checks below
        // (the MC legs re-gate session/trust at every leg boundary too).
        match crate::ffi::session_state().lock {
            crate::ffi::SessionLock::Unlocked => {}
            _ => {
                return Err(DispatchError::refused(
                    "select refused: session not unlocked pre-dispatch",
                ));
            }
        }
        if crate::ffi::secure_input_known() == Some(true) {
            return Err(DispatchError::refused(
                "select refused: Secure Input active pre-dispatch",
            ));
        }
        if crate::ffi::frontmost_app()
            .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
            .unwrap_or(true)
        {
            return Err(DispatchError::refused(
                "select refused: lock screen pre-dispatch",
            ));
        }
        if self
            .snapshot()
            .map_err(|e| DispatchError::refused(format!("pre-select row unreadable ({e})")))?
            != *before
        {
            return Err(DispatchError::refused("pre-select topology drift"));
        }
        let outcome = crate::mc_lifecycle::live::mc_goto_sid(sid, &mut self.budget, false)
            .map_err(DispatchError::from_mc)?;
        if let Some(e) = outcome.close_error {
            eprintln!("tiler-macos select close shortfall accepted ({e})");
        }
        Ok(())
    }
}
