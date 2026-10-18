#![forbid(unsafe_code)]

//! Scoped managed-send identity, fresh proof, and serialized effect handoff
//! (armed for numbered send-follow 1..9; trailing send-0, stays, relative
//! sends, and outputs stay refused).
//!
//! Nothing here arms a send or enables a holder. This module is the proven
//! seam the live native backend
//! ([`crate::workspace_send_native::LiveManagedSendBackend`]) runs on the
//! dedicated effect owner:
//!
//! - [`ManagedSendTarget`]: the admission-pinned identity. Private fields
//!   (PID, WID, app-lifetime generation, source domain, target SID,
//!   topology revision, operation identity) are bound once at admission
//!   from retained engine truth — never re-sampled at dispatch — and read
//!   only through getters. A replacement focus window or a relaunched PID
//!   lifetime can never substitute through this type.
//! - [`prove_managed_send`]: the pure fresh-proof verdict over
//!   [`SendFreshEvidence`]. Scope/current membership, ordinary visible
//!   non-fullscreen state, unchanged lifetime/topology, pinned source
//!   homing, fresh focus naming the exact target with SLPS/direct-AX/
//!   NSWorkspace agreement, fresh CG ownership/geometry, and both
//!   positional AX and WindowServer hits at the grab point must all name
//!   the pinned target. Unknown, unreadable, stale, or disagreeing
//!   evidence refuses — including a pure focus swap that leaves the pinned
//!   subject otherwise managed and readable. Sequential proof is not an
//!   atomic OS transaction: the future owner repeats the applicable fresh
//!   proof immediately before each actuation edge.
//! - [`SendEffectHandoff`]: the serialized single-flight lane between the
//!   engine (nonblocking [`SendEffectHandoff::submit`]), one dedicated
//!   effect owner (blocking take/prove/act/finish off the engine, AppKit,
//!   and per-app observation owners), and the main thread (nonblocking
//!   [`SendEffectHandoff::poll_completion_for`]). Requests and completions
//!   fence on the same operation identity plus lifetime generation and
//!   topology revision; stale results drop counted without replay.
//!   Cancellation is a request-to-stop ([`SendEffectHandoff::cancel`]):
//!   pending and completed requests drop, an inflight one stays busy and
//!   flagged until its terminal completion or a shutdown
//!   [`SendEffectHandoff::release`] after the owner thread has exited.
//!
//! Native integration: the dedicated owner thread runs current
//! engine-managed facts plus fresh create/use/drop AX reads plus the
//! bounded [`crate::space_drag_transfer::run_drag_leg`] drag there,
//! calling [`prove_managed_send`] before each actuation edge. Pending
//! key-up and mouse-up release edges bypass fresh target gates
//! (release-first executor guarantee — release never re-proves, it always
//! releases).
//! Only submission and completion observation run on the main thread, never
//! AX waits or native settlement. No periodic AX proof query is added by
//! this lane: one submission proves once per edge on the owner thread.

use std::sync::Mutex;

use tiler_core::session::DomainKey;

use crate::runtime_types::{ClaimedSelect, parse_window_id};

/// Admission-pinned managed-send target: the only window a live send may
/// grab. All fields are private so admission pins them once and every
/// later reader compares against the same pins — no replacement focus
/// window, no relaunched lifetime, no moved source domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSendTarget {
    pid: u32,
    wid: u32,
    window: String,
    app_generation: u64,
    source: DomainKey,
    target_sid: u64,
    revision: u64,
    operation_id: String,
}

impl ManagedSendTarget {
    /// Pin a target from explicit admission-time truth. Refuses empty or
    /// zero identities rather than guessing.
    pub fn bind(
        pid: u32,
        wid: u32,
        app_generation: u64,
        source: DomainKey,
        target_sid: u64,
        revision: u64,
        operation_id: &str,
    ) -> Result<Self, String> {
        if pid == 0 || wid == 0 {
            return Err("send target unpinned: zero PID/WID; refusing".to_owned());
        }
        if target_sid == 0 {
            return Err("send target unpinned: zero target SID; refusing".to_owned());
        }
        if operation_id.is_empty() {
            return Err("send target unpinned: empty operation identity; refusing".to_owned());
        }
        Ok(Self {
            window: format!("{pid}_{wid}"),
            pid,
            wid,
            app_generation,
            source,
            target_sid,
            revision,
            operation_id: operation_id.to_owned(),
        })
    }

    /// Rebuild the pinned target from an admitted claim. Every admission
    /// pin must be present and consistent (subject parses to the pinned
    /// PID/WID, generation and source domain pinned at admission, target
    /// SID and revision bound, operation identity minted): a claim without
    /// admission pins — e.g. a generation sampled later at dispatch —
    /// refuses here instead of actuating.
    pub fn from_claim(claim: &ClaimedSelect) -> Result<Self, String> {
        let subject = claim.send_subject.as_deref().ok_or_else(|| {
            "send target unpinned: claim carries no subject window; refusing".to_owned()
        })?;
        let (pid, wid) = parse_window_id(subject).ok_or_else(|| {
            format!("send target unpinned: subject {subject:?} unparsable; refusing")
        })?;
        let app_generation = claim.send_generation.ok_or_else(|| {
            "send target unpinned: claim carries no app generation; refusing".to_owned()
        })?;
        let source = claim.send_source.clone().ok_or_else(|| {
            "send target unpinned: claim carries no source domain; refusing".to_owned()
        })?;
        let target_sid = claim.expected_sid.ok_or_else(|| {
            "send target unpinned: claim carries no target SID; refusing".to_owned()
        })?;
        let revision = claim.topology_revision.ok_or_else(|| {
            "send target unpinned: claim carries no topology revision; refusing".to_owned()
        })?;
        let operation_id = claim.operation_id.as_deref().ok_or_else(|| {
            "send target unpinned: claim carries no operation identity; refusing".to_owned()
        })?;
        let target = Self::bind(
            pid,
            wid,
            app_generation,
            source,
            target_sid,
            revision,
            operation_id,
        )?;
        if target.window != subject {
            return Err(format!(
                "send target unpinned: subject {subject:?} mismatches pinned PID/WID; refusing"
            ));
        }
        Ok(target)
    }

    /// Pinned subject PID.
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// Pinned subject window ID.
    #[must_use]
    pub const fn wid(&self) -> u32 {
        self.wid
    }

    /// Pinned `"<pid>_<wid>"` subject identity.
    #[must_use]
    pub fn window(&self) -> &str {
        &self.window
    }

    /// Admission-pinned app-lifetime generation (PID-reuse fence).
    #[must_use]
    pub const fn app_generation(&self) -> u64 {
        self.app_generation
    }

    /// Admission-pinned source domain homing the subject.
    #[must_use]
    pub fn source(&self) -> &DomainKey {
        &self.source
    }

    /// Admission-bound target SID.
    #[must_use]
    pub const fn target_sid(&self) -> u64 {
        self.target_sid
    }

    /// Admission-bound topology revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Pinned request/operation identity (`op-<command_seq>`).
    #[must_use]
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
}

/// Fresh evidence for one [`prove_managed_send`] verdict. Every leg is a
/// single fresh read on the future effect-owner thread (never a poll, never
/// cached): `None` means unknown/unreadable and always refuses.
#[derive(Debug, Clone, PartialEq)]
pub struct SendFreshEvidence {
    /// Subject still homed to a currently-actuated managed domain.
    pub managed_current: Option<bool>,
    /// Subject currently reads native fullscreen (a fullscreen app owns
    /// its Space — never dragged).
    pub fullscreen: Option<bool>,
    /// Subject currently reads AX minimized (a minimized window has no
    /// input-reaching titlebar — never dragged).
    pub minimized: Option<bool>,
    /// Fresh lifetime generation for the subject PID.
    pub live_generation: Option<u64>,
    /// Fresh topology revision. Always required: an unknown or
    /// unreadable revision refuses (no select-backend-style skip leg for
    /// product send proof).
    pub live_revision: Option<u64>,
    /// Fresh domain homing the subject window.
    pub live_source: Option<DomainKey>,
    /// Pinned source domain still currently actuated.
    pub source_current: Option<bool>,
    /// Fresh observed focus window (`"<pid>_<wid>"`).
    pub focused_window: Option<String>,
    /// Fresh SLPS/direct-AX/NSWorkspace focus agreement on the target.
    pub focus_agrees: Option<bool>,
    /// Fresh CG ownership/geometry names the pinned target.
    pub geometry_ok: Option<bool>,
    /// Backend-selected fresh grab point where both hit-tests were taken.
    /// `None` (or non-finite) refuses: a permit can never bind a point the
    /// proof did not cover.
    pub grab_point: Option<(f64, f64)>,
    /// Fresh positional AX hit-test window at the grab point.
    pub ax_hit: Option<String>,
    /// Fresh WindowServer hit-test window at the grab point.
    pub ws_hit: Option<String>,
}

/// One fresh-proof refusal: which leg failed. Unknown/unreadable legs
/// refuse exactly like mismatches — never guessed, never retried here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendProofRefusal {
    UnknownManaged,
    NotManaged,
    UnknownFullscreen,
    Fullscreen,
    UnknownMinimized,
    Minimized,
    UnknownGeneration,
    GenerationMismatch,
    UnknownRevision,
    RevisionDrift,
    UnknownSource,
    SourceMoved,
    UnknownSourceCurrent,
    SourceHidden,
    UnknownFocus,
    FocusMismatch,
    UnknownAgreement,
    FocusDisagree,
    UnknownGeometry,
    GeometryMismatch,
    UnknownGrabPoint,
    UnknownAxHit,
    AxMismatch,
    UnknownWsHit,
    WsMismatch,
}

impl SendProofRefusal {
    /// Stable log token for the refusing leg.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::UnknownManaged => "refused-send-unknown-managed",
            Self::NotManaged => "refused-send-not-managed",
            Self::UnknownFullscreen => "refused-send-unknown-fullscreen",
            Self::Fullscreen => "refused-send-fullscreen",
            Self::UnknownMinimized => "refused-send-unknown-minimized",
            Self::Minimized => "refused-send-minimized",
            Self::UnknownGeneration => "refused-send-unknown-generation",
            Self::GenerationMismatch => "refused-send-stale-lifetime",
            Self::UnknownRevision => "refused-send-unknown-revision",
            Self::RevisionDrift => "refused-send-revision-drift",
            Self::UnknownSource => "refused-send-unknown-source",
            Self::SourceMoved => "refused-send-source-moved",
            Self::UnknownSourceCurrent => "refused-send-unknown-source-current",
            Self::SourceHidden => "refused-send-source-hidden",
            Self::UnknownFocus => "refused-send-unknown-focus",
            Self::FocusMismatch => "refused-send-focus-mismatch",
            Self::UnknownAgreement => "refused-send-unknown-agreement",
            Self::FocusDisagree => "refused-send-focus-disagree",
            Self::UnknownGeometry => "refused-send-unknown-geometry",
            Self::GeometryMismatch => "refused-send-geometry-mismatch",
            Self::UnknownGrabPoint => "refused-send-unknown-grab-point",
            Self::UnknownAxHit => "refused-send-unknown-ax-hit",
            Self::AxMismatch => "refused-send-ax-mismatch",
            Self::UnknownWsHit => "refused-send-unknown-ws-hit",
            Self::WsMismatch => "refused-send-ws-mismatch",
        }
    }
}

/// Blind-evidence root: which single unreadable read collapsed one
/// [`SendFreshEvidence`] to unknown. Diagnostic only — the proof verdict
/// stays [`SendProofRefusal::UnknownManaged`] for every root, so managed
/// proof, permits, and refusal behavior never change. Token-only (never
/// window content, never posted values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlindRoot {
    /// Proven engine reply that the subject is no longer managed
    /// (removed, excluded, or unhomed retained truth — never a guess).
    EngineFactsUnmanaged,
    /// Unproven engine read: the bounded query elapsed, the inbox closed,
    /// or the engine was down. Never a managed verdict — refuses without
    /// actuation, never an admission-pin substitute.
    EngineFactsNoReply,
    /// The joined fresh topology (display legs plus SLS rows) unreadable.
    Topology,
    /// The fresh SLS Space membership for the subject unreadable.
    Membership,
}

impl BlindRoot {
    /// Stable log token for the blind root.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::EngineFactsUnmanaged => "engine-facts-unmanaged",
            Self::EngineFactsNoReply => "engine-facts-noreply",
            Self::Topology => "topology",
            Self::Membership => "membership",
        }
    }
}

/// Pure fresh-proof verdict for one pinned target: every leg must name the
/// pinned target, or the send refuses without actuation and without replay.
/// A pure focus swap — the subject still managed and readable while focus
/// names another window — refuses at [`SendProofRefusal::FocusMismatch`].
pub fn prove_managed_send(
    target: &ManagedSendTarget,
    fresh: &SendFreshEvidence,
) -> Result<(), SendProofRefusal> {
    match fresh.managed_current {
        None => return Err(SendProofRefusal::UnknownManaged),
        Some(false) => return Err(SendProofRefusal::NotManaged),
        Some(true) => {}
    }
    match fresh.fullscreen {
        None => return Err(SendProofRefusal::UnknownFullscreen),
        Some(true) => return Err(SendProofRefusal::Fullscreen),
        Some(false) => {}
    }
    match fresh.minimized {
        None => return Err(SendProofRefusal::UnknownMinimized),
        Some(true) => return Err(SendProofRefusal::Minimized),
        Some(false) => {}
    }
    match fresh.live_generation {
        None => return Err(SendProofRefusal::UnknownGeneration),
        Some(generation) if generation != target.app_generation => {
            return Err(SendProofRefusal::GenerationMismatch);
        }
        Some(_) => {}
    }
    match fresh.live_revision {
        None => return Err(SendProofRefusal::UnknownRevision),
        Some(live) if live != target.revision => {
            return Err(SendProofRefusal::RevisionDrift);
        }
        Some(_) => {}
    }
    match fresh.live_source.as_ref() {
        None => return Err(SendProofRefusal::UnknownSource),
        Some(source) if *source != target.source => {
            return Err(SendProofRefusal::SourceMoved);
        }
        Some(_) => {}
    }
    match fresh.source_current {
        None => return Err(SendProofRefusal::UnknownSourceCurrent),
        Some(false) => return Err(SendProofRefusal::SourceHidden),
        Some(true) => {}
    }
    match fresh.focused_window.as_deref() {
        None => return Err(SendProofRefusal::UnknownFocus),
        Some(focused) if focused != target.window => {
            return Err(SendProofRefusal::FocusMismatch);
        }
        Some(_) => {}
    }
    match fresh.focus_agrees {
        None => return Err(SendProofRefusal::UnknownAgreement),
        Some(false) => return Err(SendProofRefusal::FocusDisagree),
        Some(true) => {}
    }
    match fresh.geometry_ok {
        None => return Err(SendProofRefusal::UnknownGeometry),
        Some(false) => return Err(SendProofRefusal::GeometryMismatch),
        Some(true) => {}
    }
    match fresh.grab_point {
        Some((x, y)) if x.is_finite() && y.is_finite() => {}
        _ => return Err(SendProofRefusal::UnknownGrabPoint),
    }
    match fresh.ax_hit.as_deref() {
        None => return Err(SendProofRefusal::UnknownAxHit),
        Some(hit) if hit != target.window => return Err(SendProofRefusal::AxMismatch),
        Some(_) => {}
    }
    match fresh.ws_hit.as_deref() {
        None => return Err(SendProofRefusal::UnknownWsHit),
        Some(hit) if hit != target.window => return Err(SendProofRefusal::WsMismatch),
        Some(_) => {}
    }
    Ok(())
}

/// Proven outcome carried by one effect completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendEffectOutcome {
    Applied,
    RefusedProven,
    PossiblyExecuted,
}

/// One effect-owner completion for a submitted target: fenced by the same
/// operation identity plus lifetime generation and topology revision. A
/// completion naming another operation, lifetime, or revision is stale —
/// dropped counted, never replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendCompletion {
    pub operation_id: String,
    pub app_generation: u64,
    pub revision: u64,
    pub outcome: SendEffectOutcome,
}

#[derive(Debug, Default)]
struct Lane {
    pending: Option<ManagedSendTarget>,
    inflight: Option<ManagedSendTarget>,
    completion: Option<SendCompletion>,
    /// Owner-visible stop request for an inflight request. Set only by
    /// [`SendEffectHandoff::cancel`] while inflight stands; cleared by the
    /// terminal [`SendEffectHandoff::finish`] or [`SendEffectHandoff::release`].
    cancel_requested: bool,
    stale_dropped: u64,
}

/// Serialized single-flight send-effect lane. The engine submits
/// nonblocking, exactly one dedicated effect owner takes/proves/actuates/
/// finishes off the engine/AppKit/per-app observation owners, and the main
/// thread polls completion nonblocking. Busy while a request is pending,
/// inflight, or completed-but-unpolled: concurrent submits refuse instead
/// of overlapping, and a sent press is never replayed. Cancellation is a
/// request-to-stop, never a lane reopen: while an effect is inflight the
/// lane stays busy until the terminal completion or a shutdown release
/// after the owner thread has exited — a cancelled inflight never admits
/// a new physical effect concurrently.
#[derive(Debug, Default)]
pub struct SendEffectHandoff {
    inner: Mutex<Lane>,
}

impl SendEffectHandoff {
    /// Empty idle lane.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nonblocking engine-side submission. Refuses busy while a request is
    /// pending, inflight, or completed-but-unpolled.
    pub fn submit(&self, target: ManagedSendTarget) -> Result<(), &'static str> {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if lane.pending.is_some() || lane.inflight.is_some() || lane.completion.is_some() {
            return Err("send-effect-busy");
        }
        lane.pending = Some(target);
        Ok(())
    }

    /// Whether a request is pending, inflight, or completed-but-unpolled.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        let lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        lane.pending.is_some() || lane.inflight.is_some() || lane.completion.is_some()
    }

    /// Dedicated-owner take: move one pending request to inflight
    /// (`None` when idle — the owner waits without blocking submitters).
    pub fn take_for_owner(&self) -> Option<ManagedSendTarget> {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let taken = lane.pending.take()?;
        lane.inflight = Some(taken.clone());
        Some(taken)
    }

    /// Dedicated-owner finish: stage the completion for the main thread.
    /// Returns false (counted, no replay) when nothing is inflight or the
    /// completion names another operation, lifetime, or revision. A
    /// terminal finish also resolves a pending stop request: it is the
    /// completion — not the cancel — that frees the active lane.
    pub fn finish(&self, completion: SendCompletion) -> bool {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(inflight) = lane.inflight.as_ref() else {
            lane.stale_dropped += 1;
            return false;
        };
        if completion.operation_id != inflight.operation_id
            || completion.app_generation != inflight.app_generation
            || completion.revision != inflight.revision
        {
            lane.stale_dropped += 1;
            return false;
        }
        lane.inflight = None;
        lane.completion = Some(completion);
        lane.cancel_requested = false;
        true
    }

    /// Nonblocking main-thread poll: take the completion only when it
    /// names the expected target's operation, lifetime, and revision.
    /// A stale completion drops counted and reports `None` — never
    /// replayed, never applied to the wrong target.
    pub fn poll_completion_for(&self, target: &ManagedSendTarget) -> Option<SendCompletion> {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let completion = lane.completion.take()?;
        if completion.operation_id != target.operation_id
            || completion.app_generation != target.app_generation
            || completion.revision != target.revision
        {
            lane.stale_dropped += 1;
            return None;
        }
        Some(completion)
    }

    /// Explicit cancellation (request-to-stop): drop any pending or
    /// completed-but-unpolled request and flag a stop for an inflight one.
    /// An inflight effect is NOT cleared and the lane stays busy — only
    /// its terminal [`SendEffectHandoff::finish`] or a shutdown
    /// [`SendEffectHandoff::release`] after the owner thread has exited
    /// frees the active lane, so a cancelled inflight never admits a new
    /// physical effect concurrently. Returns whether anything was
    /// outstanding.
    pub fn cancel(&self) -> bool {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let had = lane.pending.is_some() || lane.inflight.is_some() || lane.completion.is_some();
        lane.pending = None;
        lane.completion = None;
        if lane.inflight.is_some() {
            lane.cancel_requested = true;
        }
        had
    }

    /// Owner-visible stop request: true once [`SendEffectHandoff::cancel`]
    /// ran while an effect was inflight and until its terminal finish or
    /// a shutdown release. The dedicated owner polls this nonblocking
    /// between proof/actuation edges and stops early without posting
    /// further edges.
    #[must_use]
    pub fn cancel_requested(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .cancel_requested
    }

    /// Shutdown release: drop any pending, inflight, or completed request
    /// and clear a stop request. Call only after the dedicated owner
    /// thread has exited (joined) — never while the owner may still
    /// actuate, or the single-lane physical-effect invariant breaks.
    /// Returns whether anything was outstanding.
    pub fn release(&self) -> bool {
        let mut lane = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let had = lane.pending.is_some() || lane.inflight.is_some() || lane.completion.is_some();
        lane.pending = None;
        lane.inflight = None;
        lane.completion = None;
        lane.cancel_requested = false;
        had
    }

    /// Counted stale completions dropped without replay.
    #[must_use]
    pub fn stale_dropped(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .stale_dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::{OutputId, WorkspaceId};

    fn domain(output: &str, workspace: &str) -> DomainKey {
        DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        }
    }

    fn source_domain() -> DomainKey {
        domain("macos-display-1", "space-20")
    }

    fn other_domain() -> DomainKey {
        domain("macos-display-1", "space-7")
    }

    fn target() -> ManagedSendTarget {
        ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-3")
            .expect("fixture target binds")
    }

    fn fresh_ok() -> SendFreshEvidence {
        SendFreshEvidence {
            managed_current: Some(true),
            fullscreen: Some(false),
            minimized: Some(false),
            live_generation: Some(1),
            live_revision: Some(9),
            live_source: Some(source_domain()),
            source_current: Some(true),
            focused_window: Some("4242_7".to_owned()),
            focus_agrees: Some(true),
            geometry_ok: Some(true),
            grab_point: Some((100.0, 200.0)),
            ax_hit: Some("4242_7".to_owned()),
            ws_hit: Some("4242_7".to_owned()),
        }
    }

    #[test]
    fn bind_pins_identity_and_rejects_guesses() {
        let pinned = target();
        assert_eq!(pinned.pid(), 4242);
        assert_eq!(pinned.wid(), 7);
        assert_eq!(pinned.window(), "4242_7");
        assert_eq!(pinned.app_generation(), 1);
        assert_eq!(pinned.source(), &source_domain());
        assert_eq!(pinned.target_sid(), 3);
        assert_eq!(pinned.revision(), 9);
        assert_eq!(pinned.operation_id(), "op-3");
        for (pid, wid, sid, op) in [
            (0, 7, 3, "op-3"),
            (4242, 0, 3, "op-3"),
            (4242, 7, 0, "op-3"),
            (4242, 7, 3, ""),
        ] {
            assert!(
                ManagedSendTarget::bind(pid, wid, 1, source_domain(), sid, 9, op).is_err(),
                "pid={pid} wid={wid} sid={sid} op={op:?} refuses"
            );
        }
    }

    #[test]
    fn from_claim_requires_every_admission_pin() {
        use crate::runtime_types::InputSemanticAction;
        let action = InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        };
        let mut claim = ClaimedSelect::tap(action.clone());
        claim.send_subject = Some("4242_7".to_owned());
        claim.send_generation = Some(1);
        claim.send_source = Some(source_domain());
        claim.expected_sid = Some(3);
        claim.topology_revision = Some(9);
        claim.operation_id = Some("op-3".to_owned());
        let pinned = ManagedSendTarget::from_claim(&claim).expect("pinned claim converts");
        assert_eq!(pinned, target());
        // Each missing pin refuses: a generation sampled later at
        // dispatch (or any other admission gap) never becomes a target.
        for drop_pin in [
            "subject",
            "generation",
            "source",
            "target",
            "revision",
            "operation",
        ] {
            let mut claim = claim.clone();
            match drop_pin {
                "subject" => claim.send_subject = None,
                "generation" => claim.send_generation = None,
                "source" => claim.send_source = None,
                "target" => claim.expected_sid = None,
                "revision" => claim.topology_revision = None,
                _ => claim.operation_id = None,
            }
            assert!(
                ManagedSendTarget::from_claim(&claim).is_err(),
                "{drop_pin} refuses"
            );
        }
        // A subject that no longer parses to its PID/WID refuses.
        let mut claim = claim.clone();
        claim.send_subject = Some("not-a-window".to_owned());
        assert!(ManagedSendTarget::from_claim(&claim).is_err());
    }

    #[test]
    fn proof_accepts_fully_named_target() {
        assert!(prove_managed_send(&target(), &fresh_ok()).is_ok());
    }

    #[test]
    fn proof_refuses_unknown_revision_alongside_drift() {
        // Product send proof requires the fresh revision: unknown refuses
        // (no select-backend-style skip leg), mismatch refuses, exact
        // succeeds.
        let pinned = target();
        let mut fresh = fresh_ok();
        fresh.live_revision = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownRevision)
        );
        let mut fresh = fresh_ok();
        fresh.live_revision = Some(10);
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::RevisionDrift)
        );
        assert_eq!(
            SendProofRefusal::UnknownRevision.token(),
            "refused-send-unknown-revision"
        );
    }

    #[test]
    fn proof_refuses_scope_and_fullscreen_before_focus() {
        let pinned = target();
        for (mutate, want) in [
            (
                (|fresh: &mut SendFreshEvidence| fresh.managed_current = Some(false))
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::NotManaged,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.managed_current = None)
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownManaged,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.fullscreen = Some(true))
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::Fullscreen,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.fullscreen = None)
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownFullscreen,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.minimized = Some(true))
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::Minimized,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.minimized = None)
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownMinimized,
            ),
        ] {
            let mut fresh = fresh_ok();
            mutate(&mut fresh);
            assert_eq!(prove_managed_send(&pinned, &fresh), Err(want), "{want:?}");
        }
    }

    #[test]
    fn proof_refuses_pid_reuse_and_revision_drift() {
        let pinned = target();
        // Relaunched lifetime under the same PID: the pinned generation
        // no longer matches — never grab the new window.
        let mut fresh = fresh_ok();
        fresh.live_generation = Some(2);
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::GenerationMismatch)
        );
        let mut fresh = fresh_ok();
        fresh.live_generation = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownGeneration)
        );
        // Display/Space change mid-plan supersedes.
        let mut fresh = fresh_ok();
        fresh.live_revision = Some(10);
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::RevisionDrift)
        );
    }

    #[test]
    fn proof_refuses_moved_or_hidden_source() {
        let pinned = target();
        let mut fresh = fresh_ok();
        fresh.live_source = Some(other_domain());
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::SourceMoved)
        );
        let mut fresh = fresh_ok();
        fresh.live_source = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownSource)
        );
        let mut fresh = fresh_ok();
        fresh.source_current = Some(false);
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::SourceHidden)
        );
        let mut fresh = fresh_ok();
        fresh.source_current = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownSourceCurrent)
        );
    }

    #[test]
    fn proof_refuses_pure_focus_swap_while_subject_stays_managed() {
        // The key managed-scope verdict: the subject is still managed,
        // readable, same lifetime and domain — but focus names another
        // window. Never substitute the subsequently focused window.
        let pinned = target();
        let mut fresh = fresh_ok();
        fresh.focused_window = Some("4242_9".to_owned());
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::FocusMismatch)
        );
        let mut fresh = fresh_ok();
        fresh.focused_window = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownFocus)
        );
        let mut fresh = fresh_ok();
        fresh.focus_agrees = Some(false);
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::FocusDisagree)
        );
        let mut fresh = fresh_ok();
        fresh.focus_agrees = None;
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::UnknownAgreement)
        );
    }

    #[test]
    fn proof_refuses_geometry_and_point_hit_gaps() {
        let pinned = target();
        for (mutate, want) in [
            (
                (|fresh: &mut SendFreshEvidence| fresh.geometry_ok = Some(false))
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::GeometryMismatch,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.geometry_ok = None)
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownGeometry,
            ),
            (
                (|fresh: &mut SendFreshEvidence| {
                    fresh.ax_hit = Some("4242_9".to_owned());
                }) as fn(&mut SendFreshEvidence),
                SendProofRefusal::AxMismatch,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.ax_hit = None) as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownAxHit,
            ),
            (
                (|fresh: &mut SendFreshEvidence| {
                    fresh.ws_hit = Some("4242_9".to_owned());
                }) as fn(&mut SendFreshEvidence),
                SendProofRefusal::WsMismatch,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.ws_hit = None) as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownWsHit,
            ),
            (
                (|fresh: &mut SendFreshEvidence| fresh.grab_point = None)
                    as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownGrabPoint,
            ),
            (
                (|fresh: &mut SendFreshEvidence| {
                    fresh.grab_point = Some((f64::NAN, 200.0));
                }) as fn(&mut SendFreshEvidence),
                SendProofRefusal::UnknownGrabPoint,
            ),
        ] {
            let mut fresh = fresh_ok();
            mutate(&mut fresh);
            assert_eq!(prove_managed_send(&pinned, &fresh), Err(want), "{want:?}");
        }
        // Both point hits must name the pinned target at the grab point:
        // one agreeing leg never excuses the other.
        let mut fresh = fresh_ok();
        fresh.ax_hit = Some("4242_7".to_owned());
        fresh.ws_hit = Some("4242_9".to_owned());
        assert_eq!(
            prove_managed_send(&pinned, &fresh),
            Err(SendProofRefusal::WsMismatch)
        );
    }

    fn completion_for(pinned: &ManagedSendTarget, outcome: SendEffectOutcome) -> SendCompletion {
        SendCompletion {
            operation_id: pinned.operation_id().to_owned(),
            app_generation: pinned.app_generation(),
            revision: pinned.revision(),
            outcome,
        }
    }

    #[test]
    fn handoff_serializes_submit_take_finish_poll() {
        let lane = SendEffectHandoff::new();
        assert!(!lane.is_busy());
        assert!(lane.take_for_owner().is_none());
        let pinned = target();
        lane.submit(pinned.clone()).expect("idle submits");
        assert!(lane.is_busy());
        // Concurrent presses serialize/refuse instead of overlapping.
        assert_eq!(lane.submit(pinned.clone()), Err("send-effect-busy"));
        let taken = lane.take_for_owner().expect("owner takes");
        assert_eq!(taken, pinned);
        assert!(lane.take_for_owner().is_none());
        // A sent press is never replayed through a second take.
        assert!(lane.is_busy());
        assert!(
            lane.finish(completion_for(&pinned, SendEffectOutcome::Applied)),
            "matching finish stages"
        );
        let done = lane
            .poll_completion_for(&pinned)
            .expect("main polls the completion");
        assert_eq!(done.outcome, SendEffectOutcome::Applied);
        // Polling consumes: no replay, lane idles for the next request.
        assert!(lane.poll_completion_for(&pinned).is_none());
        assert!(!lane.is_busy());
        lane.submit(pinned).expect("idle again after poll");
        assert!(lane.cancel(), "cancel releases the pending request");
        assert!(!lane.is_busy());
        assert!(!lane.cancel(), "second cancel reports idle");
    }

    #[test]
    fn handoff_rejects_stale_completions_without_replay() {
        let lane = SendEffectHandoff::new();
        let pinned = target();
        // Finish with nothing inflight: stale, counted, no completion.
        assert!(
            !lane.finish(completion_for(&pinned, SendEffectOutcome::Applied)),
            "no inflight refuses"
        );
        assert_eq!(lane.stale_dropped(), 1);
        lane.submit(pinned.clone()).expect("submits");
        lane.take_for_owner().expect("takes");
        // Wrong operation identity: another request's completion.
        let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
        stale.operation_id = "op-9".to_owned();
        assert!(!lane.finish(stale), "wrong operation is stale");
        assert_eq!(lane.stale_dropped(), 2);
        // Relaunched lifetime under the same PID.
        let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
        stale.app_generation = 2;
        assert!(!lane.finish(stale), "wrong lifetime is stale");
        assert_eq!(lane.stale_dropped(), 3);
        // Superseded revision.
        let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
        stale.revision = 10;
        assert!(!lane.finish(stale), "wrong revision is stale");
        assert_eq!(lane.stale_dropped(), 4);
        // The inflight request still stands; the correct finish lands.
        assert!(lane.is_busy());
        assert!(lane.finish(completion_for(&pinned, SendEffectOutcome::RefusedProven)));
        // A poll for another target's identity drops the stored
        // completion instead of applying it to the wrong target.
        let other = ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-9")
            .expect("other operation binds");
        assert!(lane.poll_completion_for(&other).is_none());
        assert_eq!(lane.stale_dropped(), 5);
        assert!(!lane.is_busy());
    }

    #[test]
    fn handoff_cancel_clears_every_stage() {
        // Pending and completed requests drop and idle the lane.
        for stage in ["pending", "completed"] {
            let lane = SendEffectHandoff::new();
            let pinned = target();
            assert!(!lane.cancel_requested(), "{stage} starts unflagged");
            lane.submit(pinned.clone()).expect("submits");
            if stage == "completed" {
                lane.take_for_owner().expect("takes");
                assert!(lane.finish(completion_for(&pinned, SendEffectOutcome::Applied)));
            }
            assert!(lane.is_busy(), "{stage} is busy");
            assert!(lane.cancel(), "{stage} cancels");
            assert!(!lane.is_busy(), "{stage} idles after cancel");
            assert!(!lane.cancel_requested(), "{stage} flags no owner stop");
            assert_eq!(lane.stale_dropped(), 0, "cancel is not stale");
        }
        // Inflight cancel is a request-to-stop, not a lane reopen: the
        // lane stays busy and flagged, refuses new physical effects, and
        // only the terminal finish frees it.
        let lane = SendEffectHandoff::new();
        let pinned = target();
        lane.submit(pinned.clone()).expect("submits");
        lane.take_for_owner().expect("takes");
        assert!(lane.cancel(), "inflight cancels");
        assert!(lane.is_busy(), "inflight cancel keeps the lane busy");
        assert!(lane.cancel_requested(), "owner sees the stop request");
        assert_eq!(
            lane.submit(pinned.clone()),
            Err("send-effect-busy"),
            "no concurrent physical effect while inflight stands"
        );
        assert!(
            lane.finish(completion_for(&pinned, SendEffectOutcome::PossiblyExecuted)),
            "terminal finish still lands after cancel"
        );
        assert!(
            !lane.cancel_requested(),
            "terminal finish resolves the stop request"
        );
        let done = lane
            .poll_completion_for(&pinned)
            .expect("main polls the terminal completion");
        assert_eq!(done.outcome, SendEffectOutcome::PossiblyExecuted);
        assert!(!lane.is_busy());
        assert_eq!(lane.stale_dropped(), 0, "cancel is not stale");
    }

    #[test]
    fn handoff_release_frees_only_after_owner_exit() {
        // Shutdown path: cancel flags the stop, and release frees the lane
        // once the owner thread has exited (joined) — never while the
        // owner may still actuate.
        let lane = SendEffectHandoff::new();
        let pinned = target();
        assert!(!lane.release(), "idle release reports idle");
        lane.submit(pinned.clone()).expect("submits");
        lane.take_for_owner().expect("takes");
        assert!(lane.cancel(), "inflight cancels");
        assert!(lane.is_busy(), "lane stays busy for the exiting owner");
        assert!(lane.release(), "shutdown release frees the active lane");
        assert!(!lane.is_busy());
        assert!(!lane.cancel_requested());
        lane.submit(pinned)
            .expect("next request submits after release");
    }
}
