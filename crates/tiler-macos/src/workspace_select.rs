#![forbid(unsafe_code)]

//! Select-only native workspace controller. Explicit batches retain two-view
//! history for their lifetime; no transfer, input interception, preference
//! writes, automatic deletion or background lifecycle is implied.

use crate::runtime_types::InputSemanticAction;
use std::collections::{BTreeMap, BTreeSet};

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

/// Live topology revision for the bound-claim fence: the same snapshot
/// fingerprint the tile controller ticks from (one NSScreen/CG read plus
/// one SLS row read, no mutation). Read only on indicator-click drains —
/// never a steady-state poll — so an extra snapshot here costs nothing
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
    /// the backend cannot report one — offline fakes without a revision
    /// simply skip the revision leg; bound indicator claims still fence
    /// on the SID leg). The live backend reports the fresh snapshot
    /// fingerprint.
    fn revision(&mut self) -> Option<u64> {
        None
    }
    fn empty(&mut self, sid: u64) -> Result<bool, String>;
    /// One create dispatch only; caller independently adopts an exact append.
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String>;
    /// One select dispatch only; caller independently verifies arrival.
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), String>;
}

#[derive(Default)]
pub struct SelectionState {
    previous: BTreeMap<String, u64>,
    /// Exact-created trailing SID from the latest `execute`, captured
    /// immediately after the verified append — BEFORE the select dispatch.
    /// A later select failure or ambiguous readback still leaves the Space
    /// behind, so the caller adopts this on BOTH outcomes (and clears it)
    /// instead of learning the SID only from a successful effect. `None`
    /// when the latest execution reused (no append) or never appended.
    last_created: Option<u64>,
    /// Exact-append proof rows from the latest `execute`, captured with
    /// `last_created` (the `before`/`after` native rows around the single
    /// append plus the snapshot display identity). The drain pairs this
    /// with the claim's engine-minted token and reports the binding back
    /// (see [`crate::trailing_binding`]); cleared together with
    /// `last_created` so a stale proof can never bind twice.
    last_proof: Option<TrailingAppendProof>,
}

/// Exact native append proof for one trailing creation: the snapshot
/// display identity plus the native rows around the single verified
/// append. The engine pairs `created` with its pending token through
/// [`TrailingBindingStore::bind_exact_append`](crate::trailing_binding::TrailingBindingStore::bind_exact_append)
/// (exact same-output append only - drift refuses); rows travel by value
/// so the drain never re-reads topology it already proved.
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
}

impl SelectionState {
    /// Record a verified follow arrival for the send drain: the per-display
    /// previous becomes the source the send followed away from (so
    /// `workspace-previous` flips back to it). Called only on exact
    /// membership-plus-current readback, never on refusals or ambiguous
    /// output — same rule as the select history advance.
    ///
    /// Shared with the live send settlement: the tile notes arrival from
    /// the settled effect completion (see
    /// [`crate::workspace_send::LiveSendController`]).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn note_arrival(&mut self, display: &str, from: u64) {
        self.previous.insert(display.to_owned(), from);
    }

    /// Take the exact-append proof the latest `execute` verified, if any
    /// (clears both the proof and the SID slot). Call after every trailing
    /// execution - success or failure - exactly once: the drain adopts the
    /// created SID into lifecycle ownership AND reports the token/SID
    /// binding back to the engine from this single take, so neither leg
    /// can adopt or bind twice.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn take_trailing_proof(&mut self) -> Option<TrailingAppendProof> {
        self.last_created.take();
        self.last_proof.take()
    }

    pub fn execute(
        &mut self,
        backend: &mut impl SelectionBackend,
        action: InputSemanticAction,
    ) -> Result<SelectionEffect, String> {
        // A new execution supersedes any unclaimed append record: each leg
        // appends at most once, and `take_trailing_proof` clears after every
        // trailing run, so a stale SID can never adopt twice.
        self.last_created = None;
        self.last_proof = None;
        let before = backend.snapshot()?;
        before.validate()?;
        let mut created = None;
        let target = match action {
            InputSemanticAction::WorkspaceSelect { index: 0 } => {
                let last = *before.spaces.last().expect("validated nonempty row");
                let empty = backend.empty(last)?;
                if before.spaces.len() >= tiler_core::workspace::MIN_WORKSPACES
                    && tiler_core::workspace::trailing_reusable(empty, !empty)
                {
                    last
                } else {
                    if before.spaces.len() >= tiler_core::workspace::MAX_ORDERED {
                        return Err("shared workspace catalogue bound reached".to_owned());
                    }
                    backend.create(&before)?;
                    let after = backend.snapshot()?;
                    after.validate()?;
                    if after.display != before.display
                        || after.current != before.current
                        || after.spaces.len() != before.spaces.len() + 1
                        || !after.spaces.starts_with(&before.spaces)
                    {
                        return Err(
                            "create effect is not an exact same-display append; adoption refused"
                                .to_owned(),
                        );
                    }
                    let sid = *after.spaces.last().expect("appended row");
                    created = Some(sid);
                    // Capture immediately after the verified append: the
                    // Space exists from here even if the select below
                    // fails or reads back ambiguous.
                    self.last_created = Some(sid);
                    self.last_proof = Some(TrailingAppendProof {
                        display_id: before.display.clone(),
                        before: before.spaces.clone(),
                        after: after.spaces.clone(),
                        created: sid,
                    });
                    sid
                }
            }
            InputSemanticAction::WorkspaceSelect { index } => {
                let pos = tiler_core::workspace::select_existing(before.spaces.len(), index)
                    .ok_or("numbered workspace does not exist")?;
                before.spaces[pos]
            }
            InputSemanticAction::WorkspaceRelative { prev } => {
                let pos = before
                    .spaces
                    .iter()
                    .position(|s| *s == before.current)
                    .expect("validated current");
                let next = if prev {
                    (pos + before.spaces.len() - 1) % before.spaces.len()
                } else {
                    (pos + 1) % before.spaces.len()
                };
                before.spaces[next]
            }
            InputSemanticAction::WorkspacePrevious => {
                let sid = *self
                    .previous
                    .get(&before.display)
                    .ok_or("no previous workspace in this controller lifetime")?;
                if !before.spaces.contains(&sid) {
                    return Err("previous workspace no longer exists".to_owned());
                }
                sid
            }
            _ => {
                return Err(
                    "select-only controller refuses window-transfer/basic actions".to_owned(),
                );
            }
        };
        let now = backend.snapshot()?;
        now.validate()?;
        let mut expected = before.spaces.clone();
        if let Some(sid) = created {
            expected.push(sid);
        }
        if now.display != before.display || now.spaces != expected || now.current != before.current
        {
            return Err(format!(
                "workspace topology changed before select; created SID {created:?}; no select dispatched"
            ));
        }
        if target != now.current {
            backend
                .select(&now, target)
                .map_err(|e| format!("{e}; created SID {created:?}"))?;
            let after = backend.snapshot()?;
            if after.display != now.display || after.spaces != now.spaces || after.current != target
            {
                return Err(format!(
                    "select effect unproven; created SID {created:?}; history unchanged"
                ));
            }
            self.previous.insert(before.display, before.current);
        }
        Ok(SelectionEffect {
            from: before.current,
            target,
            created,
            changed: target != before.current,
        })
    }
}

/// Narrow runtime-created trailing-SID ownership for lifecycle cleanup
/// (offline pure decision only — no live touch here; the live remover calls
/// `remove_sid_verified` per eligible SID and reports residue honestly).
///
/// Rules (shared trailing-empty semantics, literal native order):
/// - Only SIDs this runtime created (`adopt_created`) are ever eligible;
///   baseline/preexisting SIDs never are — even when proven empty.
/// - An owned SID is eligible only when proven empty (`Ok(true)`), not
///   current, and removing it keeps at least
///   [`tiler_core::workspace::MIN_WORKSPACES`].
/// - Unknown emptiness (`Err`), occupied (`Ok(false)`), or current-owned
///   SIDs are never eligible: they report residue instead of removing.
/// - Foreign chrome blocking is modeled as unknown/occupied by the
///   emptiness proof (the live `verify_space_empty` refuses it) — never
///   exempted, never removed.
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
    /// owned SID (`Ok(true)` empty, `Ok(false)` occupied, `Err` unknown —
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
/// replay and advances no history — exactly like topology drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectGates {
    pub session_unlocked: bool,
    pub secure_active: bool,
    pub lock_screen: bool,
}

/// Unbound-claim shorthand over [`drain_claimed_select`] (skips the
/// bound-claim fence exactly like a relative/previous tap). Test-only shim
/// over the production wrapper (gate const); capability-explicit tests call
/// [`drain_claimed_select_with`] directly with a local `trailing_enabled`.
#[cfg(test)]
pub(crate) fn drain_claimed(
    state: &mut SelectionState,
    backend: &mut impl SelectionBackend,
    action: InputSemanticAction,
    gates: &SelectGates,
) -> Result<SelectionEffect, String> {
    drain_claimed_select(
        state,
        backend,
        crate::runtime_types::ClaimedSelect::tap(action),
        gates,
    )
}

/// Full-claim drain (offline-testable core of the product handoff: tests
/// inject fake backends here; the tile controller injects the live backend
/// through [`drain_live_claimed`]). Gates refuse first, then the unarmed and
/// multi-display rules refuse before any effect leg; bound claims
/// additionally refuse when their ordinal no longer names the expected SID
/// (a topology reorder between engine admission and this drain) or when
/// their topology revision drifted — an ordinal is never interpreted
/// against a different generation. Otherwise a single
/// [`SelectionState::execute`] runs (single dispatch with asynchronous
/// settle polling, never a replay; history advances only on verified
/// arrival). The claim/release protocol around this call (not the slot
/// itself) serializes concurrent presses: the slot stays claimed while
/// this blocks, so engine enqueues refuse busy instead of overlapping.
/// Numbered taps bind at engine admission and indicator segment clicks ride
/// their immutable render-time SID/revision binding, each fenced fresh here
/// AND at the pre-dispatch leg inside `execute` (the MC legs re-fence
/// session/trust at every leg boundary too); relative and previous taps
/// ride binding-free. Busy, session, Secure Input, no-window, multi-display,
/// and verified-history semantics are identical for both lanes — the
/// binding only narrows which ordinal may dispatch, never widens it.
/// Live-macOS-only outside tests (the non-test caller is gated to macOS);
/// portable unit tests exercise this on every host.
#[cfg(test)]
pub(crate) fn drain_claimed_select(
    state: &mut SelectionState,
    backend: &mut impl SelectionBackend,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
) -> Result<SelectionEffect, String> {
    drain_claimed_select_with(
        state,
        backend,
        claim,
        gates,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit full-claim drain: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local).
pub(crate) fn drain_claimed_select_with(
    state: &mut SelectionState,
    backend: &mut impl SelectionBackend,
    claim: crate::runtime_types::ClaimedSelect,
    gates: &SelectGates,
    trailing_enabled: bool,
) -> Result<SelectionEffect, String> {
    if !gates.session_unlocked {
        return Err("select gated: session not unlocked".to_owned());
    }
    if gates.secure_active {
        return Err("select gated: Secure Input active".to_owned());
    }
    if gates.lock_screen {
        return Err("select gated: lock screen".to_owned());
    }
    // Trailing index 0 arms only through the single capability gate (tests
    // pass an explicit local). The explicit `spaces-select` batch bypasses
    // this gate via `execute`.
    if !crate::workspace_input::is_armed_select_with(&claim.action, trailing_enabled) {
        return Err("product select refuses unarmed actions".to_owned());
    }
    // Product targeting rule: exactly ONE native display row. Zero or
    // multiple rows refuse before any MC touch and advance no history —
    // first-display guessing could switch the wrong display, so limited
    // single-display support is the documented rule (multi-display
    // per-display targeting is TBD). A display plugged/unplugged mid-drain
    // is still caught by the exact-identifier drift checks inside execute.
    match backend.display_count() {
        Ok(1) => {}
        Ok(n) => {
            return Err(format!(
                "product select refuses: expected exactly one native display, found {n}"
            ));
        }
        Err(e) => {
            return Err(format!(
                "product select refuses: display count unreadable ({e})"
            ));
        }
    }
    // Bound-claim fence: numbered claims pin their admission-time SID
    // plus the topology revision; a reorder or any revision move between
    // engine admission and this drain refuses instead of selecting the
    // new occupant of the ordinal. Relative, previous, and trailing
    // `index 0` taps carry no expected SID and skip this leg entirely;
    // trailing creation or reuse is decided by the fresh-read `execute`
    // below, never here. The `index as usize - 1` ordinal read would
    // underflow on zero, so it is skipped, never wrapped.
    if let Some(expected_sid) = claim.expected_sid {
        let InputSemanticAction::WorkspaceSelect { index } = claim.action else {
            return Err("bound select claim carries a non-numbered action; refusing".to_owned());
        };
        if index != 0 {
            let fresh = backend
                .snapshot()
                .map_err(|e| format!("bound select refuses: fresh row unreadable ({e})"))?;
            fresh
                .validate()
                .map_err(|e| format!("bound select refuses: fresh row unprovable ({e})"))?;
            match fresh.spaces.get(index as usize - 1) {
                Some(sid) if *sid == expected_sid => {}
                Some(_) => {
                    return Err(format!(
                        "stale bound select discarded: native row moved between admission and drain (row {index} changed); no select dispatched"
                    ));
                }
                None => {
                    return Err(format!(
                        "stale bound select discarded: native row {index} no longer exists; no select dispatched"
                    ));
                }
            }
        }
        if let (Some(want), Some(live)) = (claim.topology_revision, backend.revision())
            && want != live
        {
            return Err(format!(
                "stale bound select discarded: topology revision moved between admission and drain ({want} != {live}); no select dispatched"
            ));
        }
    }
    state.execute(backend, claim.action)
}

/// Product drain budget (seconds): 30 = the 15 s MC cleanup reserve (which
/// main-phase legs must clear — a smaller budget would refuse every leg
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

/// Proven-emptiness scan for one trailing SID over fresh native reads (SLS
/// user rows plus full window membership; shell entries skip, zero-area and
/// unreadable candidates read unknown — never empty). Shared by the
/// main-thread select backend and the effect-owner trailing preparation so
/// both prove the identical rule: no AppKit touch anywhere inside, safe on
/// any thread.
pub(crate) fn probe_trailing_emptiness(sid: u64) -> Result<bool, String> {
    let user = crate::mc_observe::fresh_user_spaces().map_err(|e| e.to_string())?;
    let all = crate::mc_observe::enumerate_window_members().map_err(|e| e.to_string())?;
    let mut empty = true;
    for w in all {
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
        };
        if crate::mc_probe::classify_window_full(
            &member,
            w.spaces.as_ref().ok().map(Vec::as_slice),
            &user,
            sid,
        )
        .0 == crate::mc_probe::WindowClass::Shell
        {
            continue;
        }
        let spaces = w
            .spaces
            .map_err(|_| "workspace emptiness unknown".to_owned())?;
        if candidate_occupied(member.wid, member.pid, member.w, member.h, &spaces, sid)? {
            empty = false;
        }
    }
    Ok(empty)
}

/// Live product drain for one claimed armed select: builds the proven
/// [`NativeSelection`] backend over the single native display row and runs
/// [`drain_claimed`]. The single-display rule inside `drain_claimed`
/// refuses zero/multiple rows before any MC touch, so `display: 0` names
/// the only row — never a guess. Reuses the native selector, never
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
) -> Result<(SelectionEffect, bool), String> {
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
) -> Result<(SelectionEffect, bool), String> {
    if !crate::workspace_input::is_armed_select_with(&claim.action, trailing_enabled) {
        return Err("product select refuses unarmed actions".to_owned());
    }
    let mut awake =
        crate::mc_lifecycle::live::CaffeinateGuard::spawn(PRODUCT_DRAIN_CAFFEINATE_SECS)
            .map_err(|e| format!("product select refuses: caffeinate unspawnable ({e})"))?;
    let mut backend = NativeSelection {
        display: 0,
        budget: crate::mc_probe::ActionBudget::new(PRODUCT_DRAIN_BUDGET_SECS),
        created: Vec::new(),
    };
    let effect = drain_claimed_select_with(state, &mut backend, claim, gates, trailing_enabled);
    let reaped = awake.stop();
    effect.map(|effect| (effect, reaped))
}

/// Live proven-emptiness read for one trailing SID on the tile main
/// thread (same proof as the drain's `NativeSelection::empty`: managed
/// windows homed exactly to the SID, shell excluded, zero-area/unreadable
/// candidates unknown — never empty). Bounded under its own short budget;
/// unknown refuses removal instead of guessing. Shutdown-path only: the
/// cadence never runs removals mid-loop.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn trailing_emptiness(sid: u64, display: usize) -> Result<bool, String> {
    let mut backend = NativeSelection {
        display,
        budget: crate::mc_probe::ActionBudget::new(TRAILING_CLEANUP_BUDGET_SECS),
        created: Vec::new(),
    };
    backend.empty(sid)
}

/// Bounded shutdown-cleanup budget (seconds): one worst-case verified
/// removal runs an MC open leg plus a remove press (each bounded by the
/// [`crate::mc_probe::MC_ACTION_TIMEOUT_MS`] 8 s action timeout) plus the
/// [`crate::mc_probe::MC_EFFECT_SETTLE_MS`] 3 s async-settle poll, with the
/// per-SID emptiness proof (one CG/SLS enumeration, sub-second) on top —
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
    let mut backend = NativeSelection {
        display,
        budget: crate::mc_probe::ActionBudget::new(120),
        created: Vec::new(),
    };
    let mut state = SelectionState::default();
    let mut effects = Vec::new();
    let mut failure = None;
    for action in actions {
        let start = std::time::Instant::now();
        match state.execute(&mut backend, action.clone()) {
            Ok(effect) => {
                effects.push(serde_json::json!({"effect":effect,"ms":start.elapsed().as_millis()}))
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let closed = crate::mc_lifecycle::live::mc_close().map_err(|e| e.to_string());
    let layout_after = crate::mc_lifecycle::live::fresh_snapshot()
        .map(|s| s.sanitized())
        .map_err(|e| e.to_string());
    let reaped = awake.stop();
    let report = serde_json::json!({"action":"spaces-select","effects":effects,"failure":failure,"created_sids":backend.created,"layout_after":layout_after,
        "mc_close":closed,"caffeinate_reaped":reaped,
        "note":"requested selection/creation persists; no transfer, deletion, settings writes or hotkeys; history lasts this batch only"}).to_string();
    if failure.is_none() && closed.is_ok() && layout_after.is_ok() && reaped {
        Ok(report)
    } else {
        Err((1, report))
    }
}

struct NativeSelection {
    display: usize,
    budget: crate::mc_probe::ActionBudget,
    created: Vec<u64>,
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
    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        probe_trailing_emptiness(sid)
    }
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String> {
        if self.snapshot()? != *before {
            return Err("pre-create topology drift".to_owned());
        }
        let raw = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let outcome =
            crate::mc_lifecycle::live::mc_add_space(self.display, &mut self.budget, false)
                .map_err(|e| e.to_string())?;
        let after = crate::mc_lifecycle::live::await_snapshot(&|s| {
            crate::mc_probe::validate_create_effect(&raw, s, self.display).is_ok()
        })
        .map_err(|e| e.to_string())?;
        let sid = crate::mc_probe::validate_create_effect(&raw, &after, self.display)
            .map_err(|e| e.to_string())?;
        self.created.push(sid);
        if let Some(e) = outcome.close_error {
            return Err(format!("created but MC close unproven: {e}"));
        }
        Ok(())
    }
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), String> {
        // Fresh gates immediately pre-dispatch: a lock, Secure Input
        // capture, or lock-screen foreground that closed after the drain
        // sampled its gates refuses here instead of pressing into it.
        // Cheap read-only calls (session/Secure Input are any-thread C
        // calls; the frontmost model-object read carries the reviewed
        // off-main justification); unknown session denies. No atomicity is
        // promised past this point — a transition mid-press is still
        // refused without replay by the drift and readback checks below
        // (the MC legs re-gate session/trust at every leg boundary too).
        match crate::ffi::session_state().lock {
            crate::ffi::SessionLock::Unlocked => {}
            _ => return Err("select refused: session not unlocked pre-dispatch".to_owned()),
        }
        if crate::ffi::secure_input_known() == Some(true) {
            return Err("select refused: Secure Input active pre-dispatch".to_owned());
        }
        if crate::ffi::frontmost_app()
            .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
            .unwrap_or(true)
        {
            return Err("select refused: lock screen pre-dispatch".to_owned());
        }
        if self.snapshot()? != *before {
            return Err("pre-select topology drift".to_owned());
        }
        let raw = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let (_, close) = crate::mc_lifecycle::goto_sid_verified(
            &raw,
            self.display,
            sid,
            &mut self.budget,
            false,
        )
        .map_err(|e| e.to_string())?;
        if let Some(e) = close {
            return Err(format!("selected but MC close unproven: {e}"));
        }
        Ok(())
    }
}
