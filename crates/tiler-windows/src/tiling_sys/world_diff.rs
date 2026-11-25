#![forbid(unsafe_code)]
//! Native host-diff facade for the single world owner.
//!
//! Actuator over typed [`WorldReplyV4`](tiler_core::world_report::WorldReplyV4)
//! facts against fresh native observations. All setters and proof re-reads
//! go through [`NativeOps`], implemented by the production `LiveNative` and
//! an in-memory fake in offline tests. Geometry converges from the core reply,
//! never from a second policy: this module honors per-row authority, scope
//! authority, holds, transfers, views, migrations, and one-shot focus
//! requests exactly as the reply states them. Workspace lifecycle needs no
//! actuation: the core owns logical workspaces outright.
//!
//! Source-first inputs (M1): [`FreshHost::from_sources`] converts the real
//! per-tick sources over the accepted W1 capture boundary. Window identity
//! comes from [`WindowKey`](crate::native_guards::WindowKey) (never HWND alone),
//! safety facts from the real
//! [`ObservedWindow`](super::observe::ObservedWindow)/[`RetainedRow`](super::retained::RetainedRow)
//! slices, classification (hidden, iconic, unreadable) from the same owner
//! tables capture used, and frames/homing from captured rows. Lifetimes with
//! no provable facts carry `facts: None` and hold; withheld or never
//! inventoried tokens carry no baseline and hold as unknown. Fixtures diverge
//! live facts per flag to prove each fence independently.
//!
//! Native rules reused rather than reinvented:
//! - [`capture_facts_rideable`](super::world_capture::capture_facts_rideable):
//!   eligible or overlay facts ride; every other `classify` refusal has no
//!   window authority lane.
//! - Frame anti-fighting is core-owned (see `tiler_core::world_frames`):
//!   this module attempts every authorized write through [`NativeOps`],
//!   then reports the mechanical facts (setter disposition, immediate
//!   pre-write and post-write frames, DPI pair) as [`FrameAttempt`](tiler_core::world_report::FrameAttempt)
//!   rows for the next report. The core compares desired and observed and
//!   publishes suppression (tiled constraint-clamp holds, withheld float
//!   tokens), which the next pass obeys without re-writing. Ordinary
//!   geometry emits no operation verdict at all: attempt rows plus
//!   observations are the whole contract. Explicit float placements ride
//!   the same raw-attempt lane with their stable operation id plus the
//!   immediate pre-write frame; the core derives confirmation, proven
//!   refusal, and ambiguity from that evidence plus the staged observation.
//!   This module emits no float outcome verdict: a setter boolean or a
//!   readback alone never settles a placement.
//! - No external verdicts are emitted: retained window/migration transfer
//!   completion derives core-side from raw membership/view attempt facts
//!   (setter disposition plus immediate pre/post proof). Focus-request
//!   outcomes are proven by read-back and counted in stats. This diff
//!   actuates no creation or removal: Windows host-owned domains produce no
//!   `native_creations` rows, so the core learns membership from the next
//!   observed report; float placements ride the raw-attempt lane with their stable operation
//!   id plus the immediate pre-write frame, and the core derives
//!   confirmation, proven refusal, and ambiguity from that evidence plus
//!   the staged observation.
//!
//! Freshness and proof rules (shared core contract):
//! - The reply must be ready, bound to the current adapter/core generation,
//!   and based on the CURRENT watermark, never a dispatched copy. Desired
//!   acceptance is strictly newer than the last applied revision
//!   ([`DiffGate::note_applied`]); anything else authorizes no setter.
//! - Fresh frames, homing, views, and safety re-read through [`NativeOps`]
//!   immediately before every setter; a gesture barrier suspends geometry.
//!   External invalidation is polled before every setter and stops remaining
//!   unsafe writes, including after geometry/float/create/remove/view/
//!   migration/focus writes.
//! - Correlated raw attempts (transfers, float placements, views) and
//!   one-shot focus requests arm BEFORE the first native setter and never repeat
//!   while ambiguous: the transport reports mechanical facts once, and the
//!   core derives every verdict. Refused-proven transfers settle only on
//!   fresh proof of nothing happened plus an explicit native refusal (both
//!   derived core-side from the raw attempt); float refusal is proven by
//!   an unchanged readable preimage in the raw attempt, never by a verdict
//!   here. Third-location proof settles superseded-external instead of
//!   fighting the user. Follow-view failure holds the window transfer as
//!   possibly-executed: the core consumes the raw follow-view attempt and
//!   holds the parent ambiguous, never a refused window.
//! - Desired absence is removal eligibility only: the facade requires a
//!   managed, non-displaced binding plus fresh occupant/visibility proof.
//!   Unknown occupancy keeps.
//! - The writable frame target is `effective_frame` only: `allocation` is the
//!   canonical slot, never a write target.
//! - Partial results are reported honestly; nothing here claims atomicity.

#[cfg(test)]
use super::MonitorArea;
use super::world_capture::{CapturedWorld, OwnerInputs, capture_facts_rideable};
use super::{ObservedWindow, RetainedRow};
use crate::native_guards::WindowKey;
use crate::tiling::WindowFacts;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::intent_data::IntentData;
use tiler_core::world_report::{
    AfterShownMember, ArrivalBracket, ArrivalLocation, AuthorityScope, FocusDisposition,
    FocusFeedback, FocusNoCallReason, FrameAttempt, FramePurpose, FrameSetter, HoldReason,
    LastWritePath, MemberAttempt, MemberDisposition, ScopeAuthority, StateAttempt, StateProperty,
    TransferKind, TransferTargetV4, ViewAttempt, VisibilityAttempt, VisibilityDisposition,
    VisibilityFact, VisibilityReason, VisibilitySample, WorkspaceRef, WorldReplyV4,
};

/// One adapter marker-storage observation for a window lifetime: the exact
/// three durable marker properties the canonical intent record mirrors.
///
/// Crate-visible because the packed native slot lane
/// (`product_hide::sys`) encodes and decodes the same storage; `tiling_sys`
/// re-exports it as the single shared definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct IntentStorage {
    pub(crate) sticky_prior: Option<bool>,
    pub(crate) float: bool,
    pub(crate) tile: bool,
}

impl IntentStorage {
    /// Canonical storage for one desired intent row. Sticky dominates (its
    /// marker already implies float); an explicit float/tile marker rides
    /// alone; the null boundary clears all three markers.
    pub(super) fn from_data(data: Option<IntentData>) -> Self {
        let Some(data) = data else {
            return Self::default();
        };
        if let Some(prior) = data.sticky_prior {
            return Self {
                sticky_prior: Some(prior),
                float: false,
                tile: false,
            };
        }
        match data.float {
            Some(true) => Self {
                sticky_prior: None,
                float: true,
                tile: false,
            },
            Some(false) => Self {
                sticky_prior: None,
                float: false,
                tile: true,
            },
            None => Self::default(),
        }
    }
}

/// Mechanical shape of one state actuation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StateWrite {
    /// No native call ran: the live property or the durable preimage was
    /// unreadable/unwritable, so the target holds with no evidence.
    NotAttempted,
    /// Exactly one native call ran and reported acceptance (`true`) or an
    /// explicit refusal (`false`).
    Called(bool),
    /// Throwing or ambiguous native call.
    Threw,
}

/// Fresh offline window facts for one managed lifetime: the observation-time
/// baseline plus the shape of every fresh re-read through [`NativeOps`].
///
/// Each flag has a distinct native source, never a guessed duplicate:
/// - `key`/`token`: stable member identity and Engine token.
/// - `frame`: last proven frame (`None` = unreadable, never fabricated).
///   Retained maximized/fullscreen overlay rows ride their tile slot here
///   (allocation reasoning and home affinity), not the live maximized frame.
/// - `facts`: the raw native read (`None` = unproven; holds). Distinct from
///   `iconic`/`hidden`: `facts.minimized` is the native minimized bit while
///   `iconic` is the adapter minimized-omission classification; `facts.visible`
///   is native visibility while `hidden` is the adapter hide-ledger state.
/// - `output`/`workspace`: homing at observation time.
/// - `iconic`: adapter minimized omission or explicit minimized entry.
/// - `hidden`: adapter hide-ledger membership.
/// - `sticky`/`floating`: adapter intent lanes from the accepted capture
///   rows (float placement intent never authorizes tiled writes; only a
///   stable `frame_operation` token does). v4 carries no native sticky or
///   observed-workspace fact: `WindowFact.sticky` is `None` on Windows and
///   best-effort exclusion is core-owned, so no adapter fit fence exists.
/// - `live_tag`: live visible-membership tag; must equal the stored tag and
///   be nonempty, mirroring the production visible-lifetime gate.
#[derive(Debug, Clone)]
pub(super) struct FreshWindow {
    pub(super) key: WindowKey,
    pub(super) token: String,
    pub(super) frame: Option<Rect>,
    pub(super) facts: Option<WindowFacts>,
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) iconic: bool,
    pub(super) hidden: bool,
    pub(super) sticky: bool,
    pub(super) floating: bool,
    pub(super) live_tag: Option<String>,
}

/// Observation-time snapshot behind one diff pass, plus the stored
/// visible-membership tags the lifetime gate rechecks against and the live
/// output views. Logical workspace mapping is core-owned; the adapter
/// carries no binding table. Occupancy, visibility, and enumeration stay
/// live reads.
#[derive(Debug, Clone, Default)]
pub(super) struct FreshHost {
    pub(super) baseline: HashMap<String, FreshWindow>,
    pub(super) stored_tags: HashMap<String, String>,
    /// Live output views at observation time (`output -> logical workspace`
    /// echo), seeding proof re-reads. Empty when the adapter has no echoed
    /// view yet.
    pub(super) views: HashMap<String, String>,
}

impl FreshHost {
    /// Source-first conversion over the accepted W1 capture boundary plus
    /// the real per-tick sources it was built from. See the module docs for
    /// the per-field sources. Takes the field-input seam (`OwnerInputs`),
    /// never the whole loop, so the runtime can hold the owner mutably
    /// alongside this read.
    pub(super) fn from_sources(
        state: &OwnerInputs<'_>,
        observed: &[ObservedWindow],
        retained: &[RetainedRow],
        captured: &CapturedWorld,
    ) -> Self {
        let mut facts_of: HashMap<&str, WindowFacts> = HashMap::new();
        for window in observed {
            facts_of.insert(window.token.as_str(), window.facts);
        }
        for row in retained {
            if let Some(facts) = row.facts {
                facts_of.entry(row.token.as_str()).or_insert(facts);
            }
        }
        let mut stored_tags: HashMap<String, String> = HashMap::new();
        for (key, token) in state.member_tokens {
            if let Some(tag) = state.member_tags.get(key) {
                stored_tags.insert(token.clone(), tag.clone());
            }
        }
        let mut baseline: HashMap<String, FreshWindow> = HashMap::new();
        // Logical homing is reply-side: the capture carries screen
        // affinity plus truthful observed receipts. Baselines home to
        // the last proven receipt where capture freshly proved it
        // (never desire); unproven lifetimes hold with no workspace
        // claim so third-location checks never mistake an empty
        // preimage for an external move. Membership and frame writes
        // still gate on core Controlled scope rows plus fresh proof.
        for fact in &captured.windows {
            let token = fact.window.0.as_str();
            let Some(key) = state.member_tokens.iter().find_map(|(key, id)| {
                if id.as_str() == token {
                    Some(key)
                } else {
                    None
                }
            }) else {
                // No provable lifetime: unknown baselines hold.
                continue;
            };
            let hidden = state.hidden_claims.contains_key(key);
            let iconic = fact.minimized == Some(true) || state.minimized_omitted.contains(key);
            // Observation-time homing: the adapter's last applied receipt,
            // never a fresh desire (the core owns observed membership in
            // v4). A lifetime with no proven receipt keeps an empty
            // workspace claim so third-location checks never mistake an
            // unknown preimage for an external move.
            let (output, workspace) =
                state.applied_homing.get(token).cloned().unwrap_or_else(|| {
                    (
                        fact.output
                            .as_ref()
                            .map(|o| o.0.clone())
                            .unwrap_or_default(),
                        String::new(),
                    )
                });
            baseline.insert(
                token.to_owned(),
                FreshWindow {
                    key: key.clone(),
                    token: token.to_owned(),
                    frame: fact.rect,
                    facts: facts_of.get(token).copied(),
                    output,
                    workspace,
                    iconic,
                    hidden,
                    sticky: state
                        .intent_storage
                        .and_then(|map| map.get(key))
                        .is_some_and(|s| s.sticky_prior.is_some()),
                    floating: state
                        .intent_storage
                        .and_then(|map| map.get(key))
                        .is_some_and(|s| s.float),
                    live_tag: state.member_tags.get(key).cloned(),
                },
            );
        }
        // Observation-time views are the adapter's proven applied
        // receipts, never a fallback of desired views: empty means
        // unknown and retains pending flights.
        let views: HashMap<String, String> = state.applied_views.clone();
        Self {
            baseline,
            stored_tags,
            views,
        }
    }
}

/// Freshness gate for one diff pass. Desired acceptance ([`Self::note_applied`])
/// is the core revision the sequencer applied; host results (per-property
/// read-back proof) are owned by the facade stats and raw attempt rows. The
/// two are never conflated: a duplicate revision (same number, already
/// applied) authorizes no setter.
#[derive(Debug, Clone)]
pub(super) struct DiffGate {
    pub(super) adapter: String,
    pub(super) generation: String,
    pub(super) current_obs: u64,
    pub(super) last_rev: u64,
    pub(super) has_accepted: bool,
}

impl DiffGate {
    pub(super) fn fresh_reply(&self, reply: &WorldReplyV4) -> bool {
        reply.ready
            && reply.adapter_incarnation == self.adapter
            && reply.core_generation == self.generation
            && reply.based_on_observation_seq == self.current_obs
            && (!self.has_accepted || reply.state_revision > self.last_rev)
    }

    /// Record one sequencer-applied core revision as the desired baseline.
    /// Called by the owner after [`AckOutcome::Applied`](super::world_sequencer::AckOutcome::Applied);
    /// host application results never move this watermark.
    pub(super) fn note_applied(&mut self, reply: &WorldReplyV4) {
        self.last_rev = reply.state_revision;
        self.has_accepted = true;
    }

    /// Generation reset: new core namespace, no carried ordering. Caller
    /// clears the generation-scoped [`HostDiff`] tables alongside.
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.generation = generation.to_owned();
        self.current_obs = 0;
        self.last_rev = 0;
        self.has_accepted = false;
    }
}

/// Foreground identity for the remembered-focus guard: HWND plus PID plus
/// process creation. Recycled HWNDs and changed foreign windows compare
/// unequal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ForegroundIdentity {
    pub hwnd: u64,
    pub pid: u32,
    pub creation: String,
}

/// Foreground state for the remembered-focus guard: `Unknown` (unreadable,
/// fail closed) is distinct from `None` (verifiably no foreground).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ForegroundState {
    None,
    Unknown,
    Known(ForegroundIdentity),
}

/// Native proof/mutation boundary. Reads are fresh re-reads, never cached
/// observations; writes return `Ok(true)` accepted, `Ok(false)` explicit
/// refusal, `Err(())` throw/ambiguous (may have executed). Occupant,
/// visibility, and enumeration reads are always live: the facade keeps no
/// cache of them, so removal rechecks observe racing host changes.
///
/// Reads take `&mut self`: production reads advance a side observation map
/// and consume one-shot invalidation state, so even proofs are mutating.
/// Callers already hold the actuator mutably.
pub(super) trait NativeOps {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow>;
    fn read_frame(&mut self, token: &str) -> Option<Rect>;
    /// Live output affinity of the window's fresh frame through the
    /// adapter's wire token map. Never committed/desired homing: this is
    /// the observed screen the frame currently sits on (`None` when the
    /// frame or its device mapping is unreadable). Arrival brackets carry
    /// exactly this observed output.
    fn read_output_affinity(&mut self, token: &str) -> Option<String> {
        let _ = token;
        None
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)>;
    fn read_view(&mut self, output: &str) -> Option<String>;
    fn is_foreground(&mut self, token: &str) -> bool;
    fn gesture_barrier(&mut self) -> bool;
    /// Live window DPI in physical pixels per inch (`None` when unreadable,
    /// never guessed). Read around each frame write so the core can tell an
    /// invalidated placement (changed basis, never a stable clamp) from an
    /// app-held size.
    fn read_dpi(&mut self, token: &str) -> Option<u32>;
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()>;
    /// Membership actuation toward `(out, ws)` with the desired visible
    /// frame as an actuation TARGET only (`None` for membership-only
    /// rows such as floats/sticky/overlays, which carry no published
    /// frame). Cross-output tiled movers and sessionless migration members
    /// carry their core-owned placement here (translated and constrained
    /// to the destination work area); the adapter never guesses placement.
    /// The setter boolean is never proof: the caller re-reads exact
    /// homing/frame/visibility before settling Applied.
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()>;
    /// View-independent arrival proof for one migration member. The
    /// migration target view flips only after every member proves, so
    /// member proof must never wait for it: tiled arrivals prove through
    /// the core-published actuation frame landing on the target output,
    /// floats through their preserved frame (`baseline_frame`), always under full
    /// identity/lifetime/scope/visibility gates. The default holds (no
    /// proof); the production seam overrides. The setter boolean is never
    /// proof here either.
    fn migration_arrival_proven(
        &mut self,
        _token: &str,
        _out: &str,
        _ws: &str,
        _frame: Option<Rect>,
        _floating: bool,
        _baseline_frame: Option<Rect>,
    ) -> bool {
        false
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()>;
    /// Nonactivating pure-view switch for a remembered-focus return. Same
    /// ledger and view proof as [`Self::switch_view`] but async
    /// `SetWindowPos` without pumping. Defaults to the pumped path.
    /// Only matching pure views use it; everything else stays pumped.
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.switch_view(output, logical)
    }
    /// One nonactivating native reveal for one window lifetime through the
    /// committed hide-claim path. `Ok(true)` means the claim operation
    /// committed (the caller re-reads visibility; setter success never
    /// proves), `Ok(false)` an explicit refusal/absent origin, and `Err(())`
    /// a throwing/ambiguous call. Fail-closed default: seams that cannot
    /// reveal must hold instead of inventing evidence.
    fn reveal_window(&mut self, _token: &str) -> Result<bool, ()> {
        Err(())
    }
    /// One nonactivating native hide for one window lifetime through the
    /// committed hide-claim path; same disposition shape as
    /// [`Self::reveal_window`]. Fail-closed default.
    fn hide_window(&mut self, _token: &str) -> Result<bool, ()> {
        Err(())
    }
    /// The managed window holding the live foreground under exact identity
    /// and lifetime proof, or `None` when the foreground is unknown,
    /// foreign, or unreadable. Exact managed `WindowId` only; never a bare
    /// HWND guess.
    fn foreground_managed(&mut self) -> Option<String> {
        None
    }
    /// Current foreground state (`Unknown` when unreadable: fail closed).
    /// Compared by HWND plus PID plus process creation, so a recycled HWND
    /// or a changed foreign window never equals the baseline.
    fn foreground_state(&mut self) -> ForegroundState {
        ForegroundState::Unknown
    }
    /// Resolve one drained `EVENT_SYSTEM_FOREGROUND` HWND to its live
    /// foreground identity (`Known` = readable HWND plus PID plus process
    /// creation; `None` for a null HWND; `Unknown` when unreadable, fail
    /// closed). Identity only, never causality: the caller matches it
    /// against the remembered in-flight target plus lifetime proof, and
    /// delivery-time snapshots never substitute for these live reads (a
    /// bare pid or tag alone never proves the full identity).
    fn foreground_identity_of(&mut self, hwnd: u64) -> ForegroundState {
        let _ = hwnd;
        ForegroundState::Unknown
    }
    /// Target GUI-thread focus at one moment: the owning thread plus its
    /// currently active and keyboard-focus windows. Read-only diagnostic
    /// input, never a gate: `Unknown` when unreadable (fail closed, stated
    /// as unknown, never guessed). The default reports unknown; the
    /// production seam reads `GetGUIThreadInfo`, offline fakes script it.
    fn gui_focus_of(&mut self, hwnd: u64) -> GuiFocusState {
        let _ = hwnd;
        GuiFocusState::Unknown
    }
    /// Trace-only switch for diagnostics reads: when false (default) the
    /// seam performs no diagnostic native reads (`gui_focus_of` reports
    /// `Unknown` without touching the OS). The owner sets this from its
    /// trace flag before each apply; offline fakes set it per test.
    fn set_diagnostics_trace(&mut self, _trace: bool) {}
    /// Whether diagnostics reads are armed (see [`Self::set_diagnostics_trace`]).
    fn diagnostics_trace(&self) -> bool {
        false
    }
    /// Adapter-dispatched focus attempt plus the actual per-native-call
    /// record: `primitive_called` tells whether the native setter ran,
    /// with its call/return stamps strictly around the primitive and the
    /// trace-gated pre-call GUI read. The default runs the plain setter
    /// and reports no primitive detail; the production seam overrides.
    fn focus_window_traced(&mut self, token: &str) -> (Result<bool, ()>, FocusCallDiag) {
        let outcome = self.focus_window(token);
        (outcome, FocusCallDiag::default())
    }
    /// Adapter-dispatched focus attempt for one exact retained focus
    /// request: the same record as [`Self::focus_window_traced`] plus the
    /// core-derived selection provenance (explicit command with a frozen
    /// `after_shown` scope) that gates the production exact-desktop
    /// fullscreen exception. The default ignores the flag (no exception);
    /// the production seam overrides. The diff always calls this (never
    /// the unbound form) for the retained focus request.
    fn focus_window_traced_selected(
        &mut self,
        token: &str,
        _selection_provenance: bool,
    ) -> (Result<bool, ()>, FocusCallDiag) {
        self.focus_window_traced(token)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()>;
    /// Trace-only executed visibility posts collected this pass (actual
    /// host API at the native hide/reveal post site, one row per executed
    /// post). Drained once per apply into the pass stats with an honest
    /// omitted count. The default reports none; the production seam drains
    /// its armed accumulator. Never a policy input.
    fn take_visibility_posts(
        &mut self,
    ) -> (Vec<crate::product_hide::sys::VisibilityPostDiag>, usize) {
        (Vec::new(), 0)
    }
    /// Live cursor position in adapter units (same space as screen FULL
    /// rects and fact frames), `None` when unreadable. Read-only; the
    /// warp path below skips its write when already at the target.
    fn read_cursor(&mut self) -> Option<(i32, i32)>;
    /// Mechanical cursor warp toward `(x, y)`: one position setter with no
    /// input synthesis, no click, and no wait. `Ok(true)` is setter
    /// success (the caller re-reads; success alone never proves arrival),
    /// `Ok(false)` is an explicit native refusal, and `Err(())` is a
    /// throwing/ambiguous call. Game, lifetime, and focus vetoes live in
    /// the core pause plus the owner game lane, never here.
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()>;
    /// Host capability gate for the retained-warp lane. Production reads
    /// the declared host capabilities; the offline default refuses.
    fn warp_capable(&mut self) -> bool {
        false
    }
    /// Loop-thread cumulative raw-input pointer generation (`None`
    /// when the raw-input witness is not installed). Read-only;
    /// [`apply_world_reply`] re-reads it immediately before a warp setter
    /// to reject user motion since the request's baseline.
    fn pointer_activity(&mut self) -> Option<u64> {
        None
    }
    /// One external invalidation consumed per call: racing native input that
    /// arrived mid-apply. The facade polls this before every setter.
    fn poll_external_invalidation(&mut self) -> bool {
        false
    }

    /// Fresh native state value for one window property (`None` when
    /// unreadable, foreign, or the lifetime no longer proves). This is the
    /// immediate pre/post read around a state write, never a captured echo.
    /// The default is unreadable (fail closed).
    fn read_state(&mut self, token: &str, property: StateProperty) -> Option<bool> {
        let _ = (token, property);
        None
    }

    /// One native state actuation for the exact target. `NotAttempted` means
    /// no native call ran (unreadable prerequisite or a failed durable
    /// preimage), so the caller reports nothing; `Called` reports the one
    /// call's acceptance/refusal; `Threw` reports an ambiguous call. The
    /// default never calls native code.
    fn write_state(&mut self, token: &str, property: StateProperty, value: bool) -> StateWrite {
        let _ = (token, property, value);
        StateWrite::NotAttempted
    }

    /// Fresh durable marker storage for one exact lifetime (`None` when
    /// unreadable). Used to prove an intent reconcile is already equal (zero
    /// redundant writes).
    fn read_intent_storage(&mut self, token: &str) -> Option<IntentStorage> {
        let _ = token;
        None
    }

    /// Publish the canonical marker storage for one exact lifetime in one
    /// atomic step. `Ok(())` only after the storage reads back canonical;
    /// any failure keeps the previous storage and authorizes no dependent
    /// write. The default fails closed.
    fn write_intent_storage(&mut self, token: &str, data: Option<IntentData>) -> Result<(), ()> {
        let _ = (token, data);
        Err(())
    }
}

/// In-memory offline native. Seeded from a [`FreshHost`] baseline (windows
/// plus views); writes mutate the live maps so proof re-reads observe them.
#[derive(Debug, Default)]
#[cfg(test)]
pub(super) struct FakeNative {
    pub(super) windows: HashMap<String, FreshWindow>,
    pub(super) views: HashMap<String, String>,
    /// Scripted live monitor areas for output-affinity reads (same wire
    /// device->output mapping as production). Empty means affinity is
    /// unreadable, never a guessed output.
    pub(super) areas: Vec<MonitorArea>,
    pub(super) foreground: Option<String>,
    pub(super) gesture: bool,
    /// Tokens whose thread runs a move/size modal loop (offline stand-in
    /// for `GetGUIThreadInfo`).
    pub(super) move_size: HashSet<String>,
    pub(super) fail_frame: HashSet<String>,
    pub(super) throw_frame: HashSet<String>,
    /// Setter accepts but the app holds its frame: the write reports
    /// success while the read-back still mismatches (native app-clamp
    /// lane input).
    pub(super) clamp_frame: HashSet<String>,
    pub(super) fail_membership: HashSet<String>,
    pub(super) throw_membership: HashSet<String>,
    pub(super) fail_view: HashSet<String>,
    pub(super) throw_view: HashSet<String>,
    /// Per-window visibility dispositions for the offline lane. Reveal
    /// refusal/throw and hide refusal/throw map exactly to the setter's
    /// `Ok(false)`/`Err(())` results; a skipped reveal leaves the window
    /// hidden after an accepted call (partial-reveal input).
    pub(super) fail_reveal: HashSet<String>,
    pub(super) throw_reveal: HashSet<String>,
    pub(super) fail_hide: HashSet<String>,
    pub(super) throw_hide: HashSet<String>,
    pub(super) skip_reveal: HashSet<String>,
    /// Every per-window reveal/hide setter attempted, in order (vetoes and
    /// already-satisfied targets record nothing: no setter ran).
    pub(super) reveal_calls: Vec<String>,
    pub(super) hide_calls: Vec<String>,
    pub(super) fail_focus: HashSet<String>,
    pub(super) throw_focus: HashSet<String>,
    /// Native focus setters dispatched, in call order (proves zero-setter
    /// paths such as an already-foreground target record no call).
    pub(super) focus_calls: Vec<String>,
    /// Scripted live cursor position (`None` = unreadable). Warp writes
    /// update it so re-reads observe the move, like the real position.
    pub(super) cursor: Option<(i32, i32)>,
    /// Every warp write attempted, in order (already-there skips record
    /// nothing).
    pub(super) warp_calls: Vec<(i32, i32)>,
    /// Warp setter dispositions for the offline lane.
    pub(super) fail_warp: bool,
    pub(super) throw_warp: bool,
    /// Scripted raw-input pointer generation (`None` = witness not
    /// installed). The warp lane re-reads this immediately before a move.
    pub(super) pointer_activity: Option<u64>,
    /// Scripted declared warp capability (fail-closed default: false).
    pub(super) warp_capable: bool,
    pub(super) pending_invalidation: bool,
    pub(super) frame_writes: usize,
    pub(super) membership_writes: usize,
    /// Frame target passed to each membership actuation, in call order
    /// (`None` = membership-only move, never a frame write).
    pub(super) membership_frames: Vec<Option<Rect>>,
    /// Scripted target GUI-thread focus per HWND (`None` = unreadable).
    /// Absent HWNDs read `Unknown`, like the production fail-closed path.
    pub(super) gui_state: HashMap<u64, GuiFocusState>,
    /// Diagnostics trace switch (see `NativeOps::set_diagnostics_trace`):
    /// `gui_focus_of` performs no scripted read and counts nothing while
    /// false. Tests arm per case.
    pub(super) trace: bool,
    /// Count of actual GUI reads performed (trace-armed only): proves
    /// trace-off passes issue no diagnostics reads.
    pub(super) gui_calls: usize,
    /// Scripted live DPI per window (`None` = unreadable, never guessed).
    pub(super) dpi: HashMap<String, u32>,
    /// Windows whose DPI changes across the write (deterministic stand-in
    /// for a cross-monitor rescale under the call).
    pub(super) dpi_flip: HashSet<String>,
    /// Scripted live native state per `(token, property)`: present = that
    /// value, absent = unreadable (never a fabricated default).
    pub(super) state: HashMap<(String, StateProperty), bool>,
    /// State slots whose native setter reports an explicit refusal.
    pub(super) fail_state: HashSet<(String, StateProperty)>,
    /// State slots whose native setter throws (ambiguous completion).
    pub(super) throw_state: HashSet<(String, StateProperty)>,
    /// Every native state call attempted, in call order.
    pub(super) state_writes: Vec<(String, StateProperty, bool)>,
    /// Scripted R12 preimage publication failure for an above raise.
    pub(super) fail_above_preimage: bool,
    /// Scripted marker storage per token: present = readable storage,
    /// absent = unreadable. Writes update it unless a failure is armed.
    pub(super) intent_storage: HashMap<String, IntentStorage>,
    /// Tokens whose marker storage reads unreadable (holds the reply);
    /// every other unscripted token reads canonical empty storage.
    pub(super) unreadable_intent: HashSet<String>,
    pub(super) fail_intent: HashSet<String>,
    /// Every durable marker-storage publish attempted, in call order.
    pub(super) intent_writes: Vec<(String, Option<IntentData>)>,
}

#[cfg(test)]
impl FakeNative {
    pub(super) fn seeded(host: &FreshHost) -> Self {
        let mut native = Self::default();
        for (token, window) in &host.baseline {
            native.windows.insert(token.clone(), window.clone());
        }
        native.views.clone_from(&host.views);
        // Offline witness/capability stay fail-closed exactly like an
        // unregistered production witness: unavailable generation and no
        // declared warp actuator. Warp-specific rigs opt in explicitly; no
        // test ever touches real raw-input state.
        native
    }

    fn note_write(&mut self) {}
}

/// Offline stand-in for an unmanaged foreign foreground: stable per-token
/// HWND plus PID plus creation, distinct across foreign tokens.
#[cfg(test)]
fn synthetic_foreign_identity(token: &str) -> ForegroundIdentity {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    let hwnd = hasher.finish().max(0x1_0000_0001);
    ForegroundIdentity {
        hwnd,
        pid: u32::MAX,
        creation: token.to_owned(),
    }
}

#[cfg(test)]
impl NativeOps for FakeNative {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.windows.get(token).cloned()
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.windows.get(token).and_then(|w| w.frame)
    }
    fn read_output_affinity(&mut self, token: &str) -> Option<String> {
        let frame = self.windows.get(token).and_then(|w| w.frame)?;
        // No scripted areas means the live device mapping is unreadable:
        // report unknown instead of the resolution helper's placeholder.
        if self.areas.is_empty() {
            return None;
        }
        Some(super::directional_resolution::output_for_rect(
            &self.areas,
            &frame,
        ))
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.windows
            .get(token)
            .map(|w| (w.output.clone(), w.workspace.clone()))
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.views.get(output).cloned()
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.foreground.as_deref() == Some(token)
    }
    fn gesture_barrier(&mut self) -> bool {
        self.gesture
    }
    fn read_dpi(&mut self, token: &str) -> Option<u32> {
        self.dpi.get(token).copied()
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        if self.throw_frame.contains(token) {
            return Err(());
        }
        if self.fail_frame.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if self.dpi_flip.contains(token)
            && let Some(dpi) = self.dpi.get_mut(token)
        {
            *dpi = dpi.wrapping_add(24);
        }
        if !self.clamp_frame.contains(token)
            && let Some(window) = self.windows.get_mut(token)
        {
            window.frame = Some(rect);
        }
        self.frame_writes += 1;
        self.note_write();
        Ok(true)
    }
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
    ) -> Result<bool, ()> {
        if self.throw_membership.contains(token) {
            return Err(());
        }
        if self.fail_membership.contains(token) {
            self.note_write();
            return Ok(false);
        }
        self.membership_frames.push(frame);
        if let Some(window) = self.windows.get_mut(token) {
            window.output = out.to_owned();
            window.workspace = ws.to_owned();
            // Membership-only lane: the offline stand-in never moves
            // frames here (production `LiveNative` writes the core-owned
            // frame under the same gates; geometry still converges
            // through the deferred frame phase). The actuation target
            // rides only as input for the production seam.
            let _ = frame;
        }
        self.membership_writes += 1;
        self.note_write();
        Ok(true)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        let key = format!("{output}/{logical}");
        if self.throw_view.contains(&key) {
            return Err(());
        }
        if self.fail_view.contains(&key) {
            self.note_write();
            return Ok(false);
        }
        self.views.insert(output.to_owned(), logical.to_owned());
        // Production view switches hide departing members and reveal
        // entering ones. The offline stand-in mirrors that visibility so
        // target-matching readbacks can settle the selection binding.
        for window in self.windows.values_mut() {
            if window.output == output {
                window.hidden = window.workspace != logical;
            }
        }
        self.note_write();
        Ok(true)
    }
    fn switch_view_nonactivating(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        self.switch_view(output, logical)
    }
    fn reveal_window(&mut self, token: &str) -> Result<bool, ()> {
        // Every setter attempt records, including refusals and throws: the
        // attempt (not this count) is the at-most-once proof the core
        // reconciles.
        self.reveal_calls.push(token.to_owned());
        if self.throw_reveal.contains(token) {
            return Err(());
        }
        if self.fail_reveal.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if let Some(window) = self.windows.get_mut(token)
            && !self.skip_reveal.contains(token)
        {
            window.hidden = false;
            if let Some(facts) = window.facts.as_mut() {
                facts.visible = true;
            }
        }
        self.note_write();
        Ok(true)
    }
    fn hide_window(&mut self, token: &str) -> Result<bool, ()> {
        self.hide_calls.push(token.to_owned());
        if self.throw_hide.contains(token) {
            return Err(());
        }
        if self.fail_hide.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if let Some(window) = self.windows.get_mut(token) {
            window.hidden = true;
            if let Some(facts) = window.facts.as_mut() {
                facts.visible = false;
            }
        }
        self.note_write();
        Ok(true)
    }
    fn foreground_managed(&mut self) -> Option<String> {
        self.foreground
            .clone()
            .filter(|token| self.windows.contains_key(token))
    }
    fn foreground_state(&mut self) -> ForegroundState {
        let token = match self.foreground.as_deref() {
            None => return ForegroundState::None,
            Some(token) => token,
        };
        if let Some(w) = self.windows.get(token) {
            return ForegroundState::Known(ForegroundIdentity {
                hwnd: w.key.hwnd,
                pid: w.key.pid,
                creation: w.key.creation.clone(),
            });
        }
        ForegroundState::Known(synthetic_foreign_identity(token))
    }
    /// Offline resolution of one drained foreground HWND: managed HWNDs
    /// report their exact stored identity; anything else reports a
    /// synthetic foreign identity (never equal to a managed in-flight, and
    /// never trusted for HWND-plus-pid-plus-creation equality on its own
    /// beyond that mismatch). Mirrors the production live reads.
    fn foreground_identity_of(&mut self, hwnd: u64) -> ForegroundState {
        if hwnd == 0 {
            return ForegroundState::None;
        }
        if let Some(window) = self.windows.values().find(|w| w.key.hwnd == hwnd) {
            return ForegroundState::Known(ForegroundIdentity {
                hwnd: window.key.hwnd,
                pid: window.key.pid,
                creation: window.key.creation.clone(),
            });
        }
        ForegroundState::Known(ForegroundIdentity {
            hwnd,
            pid: u32::MAX,
            creation: format!("foreign-{hwnd}"),
        })
    }
    fn gui_focus_of(&mut self, hwnd: u64) -> GuiFocusState {
        if !self.trace {
            return GuiFocusState::Unknown;
        }
        self.gui_calls = self.gui_calls.saturating_add(1);
        self.gui_state
            .get(&hwnd)
            .copied()
            .unwrap_or(GuiFocusState::Unknown)
    }
    fn set_diagnostics_trace(&mut self, trace: bool) {
        self.trace = trace;
    }
    fn diagnostics_trace(&self) -> bool {
        self.trace
    }
    fn focus_window_traced(&mut self, token: &str) -> (Result<bool, ()>, FocusCallDiag) {
        // Offline mirror of the production record: stamps strictly around
        // the primitive lane, `gui_before` sampled immediately before it
        // when traced. `move_size` is the pre-primitive veto lane (no
        // primitive, honest `Ok(false)`); `fail_focus` is a ran-but-refused
        // primitive; `throw_focus` is a ran-and-threw primitive. The offline
        // mirror runs no prime: `prime_inserted` stays `None` here; the
        // production seam reports the actual outcome.
        if self.move_size.contains(token) {
            self.note_write();
            return (
                Ok(false),
                FocusCallDiag {
                    primitive_called: false,
                    native_return: None,
                    prime_inserted: None,
                    api: None,
                    call_mono_ms: None,
                    ret_mono_ms: None,
                    gui_before: None,
                    refusal_gate: None,
                    refusal_reason: None,
                    target_visible: None,
                    target_cloaked: None,
                    target_hidden: None,
                    fg_identity: None,
                    fg_gate_identity: None,
                    fg_veto_kind: None,
                    fg_readable: None,
                    fg_shell: None,
                    fg_fullscreen: None,
                    fg_visible: None,
                    fg_exe: None,
                    desktop_exception: false,
                },
            );
        }
        let gui_before = if self.trace {
            let hwnd = self.windows.get(token).map(|w| w.key.hwnd).unwrap_or(0);
            self.gui_calls = self.gui_calls.saturating_add(1);
            Some(
                self.gui_state
                    .get(&hwnd)
                    .copied()
                    .unwrap_or(GuiFocusState::Unknown),
            )
        } else {
            None
        };
        let call_mono_ms = super::trace::log_mono_ms();
        // A native setter is dispatched (accepted, refused, or threw): the
        // move/size pre-veto above returns before this point and records
        // nothing here.
        self.focus_calls.push(token.to_owned());
        let (outcome, native_return) = if self.throw_focus.contains(token) {
            (Err(()), None)
        } else if self.fail_focus.contains(token) {
            self.note_write();
            (Ok(false), Some(false))
        } else {
            self.foreground = Some(token.to_owned());
            self.note_write();
            // Simulate the OS activation the primitive caused: the
            // dispatch-time read above keeps the pre value while the
            // read-back landing read below observes the post value.
            if let Some(window) = self.windows.get(token) {
                let hwnd = window.key.hwnd;
                let thread = self
                    .gui_state
                    .get(&hwnd)
                    .copied()
                    .map(|state| match state {
                        GuiFocusState::Known { thread, .. } => thread,
                        GuiFocusState::Unknown => 0,
                    })
                    .unwrap_or(0);
                self.gui_state.insert(
                    hwnd,
                    GuiFocusState::Known {
                        thread,
                        active: hwnd,
                        focus: hwnd,
                    },
                );
            }
            (Ok(true), Some(true))
        };
        let ret_mono_ms = super::trace::log_mono_ms();
        (
            outcome,
            FocusCallDiag {
                primitive_called: true,
                native_return,
                prime_inserted: None,
                api: Some(FOCUS_SETTER_API),
                call_mono_ms: Some(call_mono_ms),
                ret_mono_ms: Some(ret_mono_ms),
                gui_before,
                refusal_gate: None,
                refusal_reason: None,
                target_visible: None,
                target_cloaked: None,
                target_hidden: None,
                fg_identity: None,
                fg_gate_identity: None,
                fg_veto_kind: None,
                fg_readable: None,
                fg_shell: None,
                fg_fullscreen: None,
                fg_visible: None,
                fg_exe: None,
                desktop_exception: false,
            },
        )
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        self.focus_window_traced(token).0
    }

    fn read_cursor(&mut self) -> Option<(i32, i32)> {
        self.cursor
    }
    fn move_cursor(&mut self, x: i32, y: i32) -> Result<bool, ()> {
        if self.throw_warp {
            return Err(());
        }
        if self.fail_warp {
            self.note_write();
            return Ok(false);
        }
        self.cursor = Some((x, y));
        self.warp_calls.push((x, y));
        self.note_write();
        Ok(true)
    }
    fn warp_capable(&mut self) -> bool {
        self.warp_capable
    }
    fn pointer_activity(&mut self) -> Option<u64> {
        self.pointer_activity
    }
    fn poll_external_invalidation(&mut self) -> bool {
        std::mem::take(&mut self.pending_invalidation)
    }
    fn read_state(&mut self, token: &str, property: StateProperty) -> Option<bool> {
        self.state.get(&(token.to_owned(), property)).copied()
    }
    fn write_state(&mut self, token: &str, property: StateProperty, value: bool) -> StateWrite {
        let key = (token.to_owned(), property);
        if property == StateProperty::Above && value && self.fail_above_preimage {
            // Durable preimage publication failed: no native call at all.
            return StateWrite::NotAttempted;
        }
        self.state_writes.push((token.to_owned(), property, value));
        if self.throw_state.contains(&key) {
            return StateWrite::Threw;
        }
        if self.fail_state.contains(&key) {
            return StateWrite::Called(false);
        }
        self.state.insert(key, value);
        StateWrite::Called(true)
    }
    fn read_intent_storage(&mut self, token: &str) -> Option<IntentStorage> {
        if self.unreadable_intent.contains(token) {
            return None;
        }
        Some(self.intent_storage.get(token).copied().unwrap_or_default())
    }
    fn write_intent_storage(&mut self, token: &str, data: Option<IntentData>) -> Result<(), ()> {
        if self.fail_intent.contains(token) {
            return Err(());
        }
        self.intent_writes.push((token.to_owned(), data));
        self.intent_storage
            .insert(token.to_owned(), IntentStorage::from_data(data));
        Ok(())
    }
}

/// At-most-once dispatch fences for transfers, floats, views, and focus
/// requests. All entries are mechanical transport state, never policy verdicts:
/// completion, refusal, and ambiguity all live in the core over reported
/// raw facts plus native observation. Operation-scoped entries live and
/// die with the core generation; [`HostDiff::reset_generation`] clears
/// them.
#[derive(Debug, Default)]
pub(super) struct DispatchFences {
    /// Membership phases already dispatched, per operation window:
    /// a possibly-executed write never repeats without fresh unambiguous
    /// proof. Retired only by [`HostDiff::prune`] (operation left the
    /// reply) or generation reset. Distinct from core `held`: this blocks
    /// the duplicate dispatch itself, the core owns the verdict.
    pub(super) attempted_members: HashSet<(String, String)>,
    /// Retained focus request ids already dispatched: the transport never
    /// re-actuates the same request, and a retired request never inherits
    /// the spent mark of a newer one.
    pub(super) spent_focus: HashSet<String>,
    pub(super) spent_floats: HashSet<String>,
    /// Identity minted for one fresh native attempt row: strictly
    /// increasing, saturating, and reset with the core generation. Staged
    /// rows keep their id across report resends, so the core filters
    /// redeliveries by this counter alone.
    next_attempt_id: u64,
    /// Distinct local view-phase memory per owning core id: the retained
    /// view id itself for standalone views, the window parent for follow,
    /// the migration parent for the target view, and the actual refill id
    /// for refill. Never cleared by membership arrival; only operation
    /// retirement/reset retires a phase. Every view phase is armed (marked)
    /// immediately before its setter; exact target proof may settle without
    /// a dispatch.
    pub(super) spent_views: HashSet<String>,
    /// Cursor-warp spend fence (C3 exact-once): request ids for which this
    /// adapter already made its one real setter attempt. Duplicate
    /// deliveries and later passes never re-attempt the same id. Cleared
    /// only by generation reset (warp ids are generation-scoped).
    pub(super) spent_warps: HashSet<String>,
    /// Last exact warp request id this generation attempted: the report's
    /// `warpSpent`, which tells the core to retire that retained request.
    /// Set immediately before the setter (a refusal or throw still spends
    /// the id) and cleared on generation reset.
    pub(super) warp_spent: Option<String>,
    /// Trace-only hover emission cursor (stage 4b FFC oracle): the
    /// `(cursor-hit, focused, ffc-enabled)` triple of the last emitted
    /// `world-apply` row. Lets a changed hover observation log even when
    /// it minted no effect and dispatched nothing, so absent-hit versus
    /// suppressed-effect stays distinguishable without a competing focus
    /// command. `None` until the first emitted row. Never a policy input,
    /// never pruned, and intentionally kept across generation reset (a
    /// reset alone is not new hover evidence).
    last_hover_trace: Option<(Option<String>, String, bool)>,
}

impl DispatchFences {
    /// Membership-phase fence: true once this operation window attempted
    /// its membership setter. A possibly-executed write never repeats.
    pub(super) fn is_member_attempted(&self, op: &str, window: &str) -> bool {
        self.attempted_members
            .contains(&(op.to_owned(), window.to_owned()))
    }
    /// Arm one membership phase immediately before its setter.
    pub(super) fn mark_member_attempted(&mut self, op: &str, window: &str) {
        self.attempted_members
            .insert((op.to_owned(), window.to_owned()));
    }
    /// Mark one retained focus request spent BEFORE dispatch. Returns false
    /// when already spent: a request never re-actuates.
    pub(super) fn mark_focus_spent(&mut self, id: &str) -> bool {
        self.spent_focus.insert(id.to_owned())
    }
    /// Mint the identity for one fresh native attempt row. Every fresh
    /// attempt takes one id; the same staged row keeps it across resends.
    /// Strictly increasing from 1, reset with the core generation.
    pub(super) fn next_attempt_id(&mut self) -> u64 {
        let id = self.next_attempt_id.max(1);
        self.next_attempt_id = id.saturating_add(1);
        id
    }
    /// Mark one float placement spent BEFORE dispatch. Returns false when
    /// already spent for this operation: the transport never dispatches the
    /// same placement twice. This is strictly at-most-once dispatch, never
    /// a policy verdict: completion, refusal, and ambiguity all live in the
    /// core over the reported raw attempt plus native observation.
    pub(super) fn mark_float_spent(&mut self, op: &str) -> bool {
        self.spent_floats.insert(op.to_owned())
    }
    /// View-phase fence: true once the owning core id attempted its view
    /// setter (follow parent, migration parent, refill id, or retained view
    /// id). Late membership arrival never clears this.
    pub(super) fn is_view_spent(&self, owning: &str) -> bool {
        self.spent_views.contains(owning)
    }
    /// Arm one view phase immediately before its setter. Returns false when
    /// already spent: duplicates never reapply.
    pub(super) fn mark_view_spent(&mut self, owning: &str) -> bool {
        self.spent_views.insert(owning.to_owned())
    }
    /// Spend one exact cursor-warp request id. Marked before the setter
    /// call (or before an already-there satisfaction with no call) and
    /// recorded as the generation's `warpSpent` acknowledgement. Returns
    /// false when this generation already spent that id: never re-attempt.
    pub(super) fn mark_warp_spent(&mut self, id: &str) -> bool {
        if !self.spent_warps.insert(id.to_owned()) {
            return false;
        }
        self.warp_spent = Some(id.to_owned());
        true
    }
    /// The last exact warp id spent in this generation, if any.
    #[must_use]
    pub(super) fn warp_spent(&self) -> Option<&str> {
        self.warp_spent.as_deref()
    }
    /// Trace-only hover-change oracle: true when `(cursor-hit, focused,
    /// ffc-enabled)` differs from the last emitted row (or no row has been
    /// emitted yet). Pure comparison over already-captured values: no
    /// native reads, no policy input.
    pub(super) fn hover_trace_changed(&self, hit: Option<&str>, focused: &str, ffc: bool) -> bool {
        self.last_hover_trace
            .as_ref()
            .is_none_or(|(prev_hit, prev_focused, prev_ffc)| {
                prev_hit.as_deref() != hit || prev_focused != focused || *prev_ffc != ffc
            })
    }
    /// Record the hover triple of an emitted row. Call only when a
    /// `world-apply` row is actually emitted, never on quiet ticks, so a
    /// changed hover keeps triggering until its row is on the trace.
    pub(super) fn note_hover_traced(&mut self, hit: Option<String>, focused: String, ffc: bool) {
        self.last_hover_trace = Some((hit, focused, ffc));
    }
    // Reset the generation-scoped tables together.
    #[allow(dead_code)]
    fn clear_generation(&mut self) {
        self.attempted_members.clear();
        self.spent_focus.clear();
        self.spent_floats.clear();
        self.spent_views.clear();
        self.next_attempt_id = 0;
        self.spent_warps.clear();
        self.warp_spent = None;
    }
}

/// Event-driven in-flight focus target: the exact full native identity
/// (HWND plus PID plus process creation) plus the visible-membership
/// lifetime tag of the last accepted focus setter whose async activation
/// may still land. Set only on an accepted setter that does not prove
/// immediately, cleared on its landing, on any other foreground
/// supersession, on target lifetime end, on a newer accepted setter, and
/// on generation reset. Minimal transport state, never a policy input:
/// the core alone decides the target and every existing fullscreen,
/// game, lifetime, and at-most-once gate stays unchanged. A simultaneous
/// same-window user click landing on the same identity is accepted as an
/// indistinguishable tradeoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InFlightFocus {
    pub(super) token: String,
    pub(super) key: WindowKey,
    pub(super) tag: String,
    /// Originating one-shot focus request whose accepted setter armed this
    /// flight: joins the dispatch row (`focus_dispatch[].id`) to the later
    /// landing or clearing row (`inflight_events[].effect`; field name kept
    /// for trace compatibility).
    pub(super) effect: String,
    /// Shared monotonic stamp of the arming setter's return (same `mono_ms`
    /// basis as the log rows): joins dispatch to landing without claiming
    /// the setter call itself took, or caused, the later arrival.
    pub(super) set_mono_ms: u64,
}

/// One offline diff unit: freshness gate plus the at-most-once tables
/// behind [`apply_world_reply`]. Frame anti-fighting lives in the core
/// (attempt rows out, suppression in the next desire); this side keeps no
/// refusal memory. All maps stay bounded: [`Self::prune`] retains entries
/// against the current reply and baseline every pass, and
/// [`Self::reset_generation`] clears generation-scoped state.
/// (Native workspace lifecycle needs no table: the core owns logical
/// workspaces outright, so the adapter never creates or removes them.)
#[derive(Debug)]
pub(super) struct HostDiff {
    pub(super) gate: DiffGate,
    pub(super) effects: DispatchFences,
    pub(super) in_flight: Option<InFlightFocus>,
    /// Drained `EVENT_SYSTEM_FOREGROUND` HWNDs awaiting resolution at the
    /// next apply entry, in delivery order. The owner loop drains hook
    /// callbacks before every apply and no message pump runs between that
    /// drain and the focus setter, so every entry here strictly predates
    /// the pass's setter: a stale queued edge can never be processed
    /// against a newer in-flight. Consumed (taken) at each apply entry,
    /// cleared on generation reset and on every accepted focus setter.
    pub(super) pending_foreground: Vec<u64>,
    /// Trace-only delivery/owner timestamps parallel to
    /// `pending_foreground`, in the same delivery order (joined by index,
    /// tolerating a shorter sidecar). Capped at
    /// [`MAX_PENDING_FOREGROUND_META`]; excess folds into
    /// `pending_foreground_omitted`. Diagnostic only: resolution matches on
    /// live identity plus lifetime, never on these stamps.
    pub(super) pending_foreground_meta: Vec<super::observe::ForegroundEdgeMeta>,
    /// Drained foreground edges whose metadata exceeded the sidecar cap:
    /// honest omission, never a silent loss (the production
    /// `pending_foreground` HWNDs above still resolve).
    pub(super) pending_foreground_omitted: u64,
    /// Trace-only in-flight clearing rows recorded where no [`ApplyStats`]
    /// is in scope (generation reset): drained into the next pass's stats
    /// at apply entry. Bounded with honest omission folded into the pass
    /// total there.
    pub(super) inflight_stash: Vec<InFlightEventDiag>,
    pub(super) inflight_stash_omitted: u64,
    /// Latest-wins visibility evidence per window lifetime, produced by
    /// this apply pass and drained into the next report by the owner.
    /// Only target-matching live readbacks enter: a mismatched read is no
    /// verdict and must hold, never latch a failure.
    pending_visibility: BTreeMap<String, VisibilityAttempt>,
    /// Durable at-most-once visibility decisions per window lifetime. An
    /// unchanged `(target, revision)` tuple redelivers the original row and
    /// never reaches a native setter again; only a new target/revision
    /// authorizes a new write. Cleared on generation reset.
    visibility_writes: BTreeMap<String, VisibilityAttempt>,
    /// Durable at-most-once focus decisions per request id. Exact
    /// `(id, target, revision)` tuples redeliver; a reused id with a new
    /// tuple replaces the old record. Cleared on generation reset.
    focus_records: BTreeMap<String, FocusFeedback>,
    /// Focus feedback produced this apply pass, drained by the owner into
    /// the sequencer for the next report.
    pending_focus: BTreeMap<String, FocusFeedback>,
    /// One-shot desktop-exception authority for exactly this apply pass:
    /// the owner arms it only when the latest accepted command is one of
    /// the five explicit workspace-selection variants and the reply's
    /// retained explicit focus request is that command's exact outcome.
    /// Consumed at apply entry; observation ticks and every other command
    /// leave it false.
    selection_focus_authority: bool,
    /// Durable at-most-once state writes per `(lifetime, property)`. An
    /// unchanged `(value, command_seq)` redelivers the stored attempt with
    /// zero native calls, so a lost acknowledgement can never re-run a
    /// native write. A new value or command sequence replaces the record.
    /// Retained for departed lifetimes (bounded by windows ever toggled in
    /// the session) so an observation gap cannot re-run a write; cleared on
    /// generation reset.
    state_writes: BTreeMap<(String, StateProperty), StateWriteRecord>,
    /// One-shot automatic maximize clears already applied, per
    /// `(lifetime, property, value)`: an automatic clear is at most once per
    /// tuple, and a later honest opposite non-automatic write re-arms it.
    auto_clear_spent: BTreeSet<(String, StateProperty, bool)>,
    /// Latest-wins state attempts per `(lifetime, property)` produced by
    /// this apply pass, drained into the next report by the owner. Keyed by
    /// the property so a paired Fullscreen→Maximized publish keeps both.
    pending_state: BTreeMap<(String, StateProperty), StateAttempt>,
    /// Last-persisted marker snapshot per lifetime. An absent-baseline
    /// desired row owns no native read, so only a proven publish of this
    /// exact storage proves nothing is owed; anything else holds the reply.
    /// Updated only on a successful publish and cleared on generation
    /// reset.
    intent_persisted: HashMap<String, IntentStorage>,
}

/// Durable at-most-once record for one `(lifetime, property)` state slot.
#[derive(Debug, Clone)]
struct StateWriteRecord {
    value: bool,
    command_seq: Option<u64>,
    attempt: StateAttempt,
}

impl HostDiff {
    pub(super) fn new(adapter: &str, generation: &str) -> Self {
        Self {
            gate: DiffGate {
                adapter: adapter.to_owned(),
                generation: generation.to_owned(),
                current_obs: 0,
                last_rev: 0,
                has_accepted: false,
            },
            effects: DispatchFences::default(),
            in_flight: None,
            pending_foreground: Vec::new(),
            pending_foreground_meta: Vec::new(),
            pending_foreground_omitted: 0,
            inflight_stash: Vec::new(),
            inflight_stash_omitted: 0,
            pending_visibility: BTreeMap::new(),
            visibility_writes: BTreeMap::new(),
            focus_records: BTreeMap::new(),
            pending_focus: BTreeMap::new(),
            selection_focus_authority: false,
            state_writes: BTreeMap::new(),
            auto_clear_spent: BTreeSet::new(),
            pending_state: BTreeMap::new(),
            intent_persisted: HashMap::new(),
        }
    }

    /// Last exact warp id spent in the current core generation, if any:
    /// the next report's `warpSpent`. `None` before any attempt and after
    /// a generation reset.
    pub(super) fn warp_spent(&self) -> Option<String> {
        self.effects.warp_spent().map(str::to_owned)
    }

    /// Drain this pass's state attempts for the next report, ordered by
    /// lifetime and then property for deterministic correlation.
    pub(super) fn take_state_attempts(&mut self) -> Vec<StateAttempt> {
        let mut rows: Vec<StateAttempt> = std::mem::take(&mut self.pending_state)
            .into_values()
            .collect();
        rows.sort_by(|a, b| {
            a.window
                .0
                .cmp(&b.window.0)
                .then_with(|| state_property_rank(a.property).cmp(&state_property_rank(b.property)))
        });
        rows
    }

    /// Arm the one-shot desktop exception for the next apply pass. The
    /// owner derives the bool from the accepted command variant plus the
    /// reply's exact explicit focus correlation; `apply` consumes it.
    pub(super) fn arm_selection_focus(&mut self, authority: bool) {
        self.selection_focus_authority = authority;
    }

    /// Drain the latest target-matching visibility evidence for the next
    /// report. Entries are one per window lifetime; a newer pass replaces
    /// an older row for the same window.
    pub(super) fn take_visibility_attempts(&mut self) -> Vec<VisibilityAttempt> {
        std::mem::take(&mut self.pending_visibility)
            .into_values()
            .collect()
    }

    /// Drain the focus feedback produced by this pass for the next report.
    /// One row per decided request id; latest pass wins.
    pub(super) fn take_focus_feedback(&mut self) -> Vec<FocusFeedback> {
        std::mem::take(&mut self.pending_focus)
            .into_values()
            .collect()
    }

    /// Stash drained foreground HWNDs for the next apply entry, in delivery
    /// order. Called by the owner loop right after the hook-edge drain,
    /// before observation and apply run.
    #[allow(dead_code)]
    pub(super) fn push_foreground_events(&mut self, hwnds: Vec<u64>) {
        self.pending_foreground.extend(hwnds);
    }

    /// Stash drained foreground edges with their delivery/owner timestamps
    /// for the next apply entry, in delivery order. The production drain
    /// calls this with the full pass detail; offline tests keep the
    /// HWND-only entry above. Metadata caps at
    /// [`MAX_PENDING_FOREGROUND_META`] with the excess counted, never
    /// silently dropped from the production queue. The sidecar stays
    /// prefix-aligned: once a gap opens (a drain without detail, a full
    /// sidecar, or an HWND mismatch) later detail never joins an earlier
    /// production HWND by index; the gap counts as honest omission and
    /// missing metadata reads as unknown at resolution, never a wrong
    /// stamp.
    #[allow(dead_code)]
    pub(super) fn push_foreground_drain(
        &mut self,
        hwnds: Vec<u64>,
        meta: Vec<super::observe::ForegroundEdgeMeta>,
    ) {
        self.push_foreground_drain_traced(hwnds, meta, 0);
    }

    /// Traced drain entry carrying the upstream [`EdgeDrain`] detail-cap
    /// omission for the same pass (production only; tests use `0`). The
    /// production queue extends exactly as before; only the diagnostic
    /// prefix attaches. `upstream_omitted` never double-counts: the
    /// per-drain gap already covers it, so the increment takes the max.
    pub(super) fn push_foreground_drain_traced(
        &mut self,
        hwnds: Vec<u64>,
        meta: Vec<super::observe::ForegroundEdgeMeta>,
        upstream_omitted: usize,
    ) {
        let hwnd_len = hwnds.len();
        let base_fg = self.pending_foreground.len();
        let base_meta = self.pending_foreground_meta.len();
        self.pending_foreground.extend(hwnds);
        let mut attached = 0usize;
        if base_fg == base_meta {
            let room = MAX_PENDING_FOREGROUND_META.saturating_sub(base_meta);
            for m in meta.iter().take(room) {
                if self.pending_foreground[base_fg + attached] != m.hwnd {
                    break;
                }
                self.pending_foreground_meta.push(m.clone());
                attached += 1;
            }
        }
        let gap_missing = hwnd_len.saturating_sub(meta.len()).max(upstream_omitted);
        let cap_missing = meta.len().saturating_sub(attached);
        self.pending_foreground_omitted = self
            .pending_foreground_omitted
            .saturating_add(gap_missing.saturating_add(cap_missing) as u64);
    }

    /// Generation reset: new core namespace clears operation-scoped tables
    /// (reused operation ids must never inherit spent/ambiguous/applied
    /// state). The caller re-derives the watermark from the new bootstrap.
    #[allow(dead_code)]
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.gate.reset_generation(generation);
        self.effects.clear_generation();
        if let Some(flight) = self.in_flight.take() {
            // No ApplyStats in scope here: stash the clearing cause for the
            // next pass's stats so the reset never loses the flight's end.
            // No `ops` here, so no GUI read: `gui` stays `None`.
            let row = InFlightEventDiag {
                token: flight.token,
                effect: flight.effect,
                cause: "reset-generation",
                edge_event_ms: None,
                edge_queued_mono_ms: None,
                edge_drain_mono_ms: None,
                resolve_mono_ms: super::trace::log_mono_ms(),
                gui: None,
                hwnd: Some(flight.key.hwnd),
                pid: Some(flight.key.pid),
                creation: Some(flight.key.creation),
            };
            if self.inflight_stash.len() < MAX_INFLIGHT_DIAG {
                self.inflight_stash.push(row);
            } else {
                self.inflight_stash_omitted = self.inflight_stash_omitted.saturating_add(1);
            }
        }
        self.pending_foreground.clear();
        self.pending_foreground_meta.clear();
        self.pending_foreground_omitted = 0;
        self.visibility_writes.clear();
        self.focus_records.clear();
        self.pending_visibility.clear();
        self.pending_focus.clear();
        self.state_writes.clear();
        self.auto_clear_spent.clear();
        self.pending_state.clear();
        self.intent_persisted.clear();
    }

    /// Bound every table against the current reply and baseline: spent and
    /// attempted entries survive only for operation ids the reply still
    /// carries. Attempted membership survives on the live operation alone:
    /// pruning against the fresh baseline would erase the at-most-once
    /// dispatch fence across observation gaps (unreadable/incomplete
    /// inventory) and acknowledgement loss, allowing a possibly-executed
    /// write to repeat. Departed lifetimes retire with the operation id
    /// (the core forgets them on close when the reply drops the transfer).
    ///
    /// State tables prune against the union of the fresh baseline and the
    /// reply's listed lifetimes. A listed lifetime that publishes no state
    /// target for a property drops that `(lifetime, property)` write record,
    /// so a later target at a higher revision performs a fresh native write
    /// instead of redelivering an inert revision forever. A live lifetime
    /// with no row at all (whole-window absence, an observation gap) keeps
    /// its records: absence is never proof a possibly-executed write did not
    /// run. The automatic-clear latch follows the same policy against the
    /// automatic targets alone. Authority is not a prune input: a Hold or
    /// Untouched row still publishes the property and protects its record.
    fn prune(&mut self, reply: &WorldReplyV4, fresh: &FreshHost) {
        let mut live_ops: HashSet<&str> = HashSet::new();
        if let Some(request) = reply.focus_request.as_ref() {
            live_ops.insert(request.id.as_str());
        }
        for transfer in &reply.transfers {
            live_ops.insert(transfer.operation_id.as_str());
        }
        for row in &reply.windows {
            if let Some(op) = row.frame_operation.as_deref()
                && !op.is_empty()
            {
                live_ops.insert(op);
            }
        }
        self.effects
            .spent_focus
            .retain(|id| live_ops.contains(id.as_str()));
        self.effects
            .spent_floats
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .spent_views
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .attempted_members
            .retain(|key| live_ops.contains(key.0.as_str()));
        let live_windows: HashSet<&str> = reply
            .windows
            .iter()
            .map(|row| row.lifetime.0.as_str())
            .collect();
        self.visibility_writes
            .retain(|token, _| live_windows.contains(token.as_str()));
        let live_focus = reply
            .focus_request
            .as_ref()
            .map(|request| request.id.as_str());
        self.focus_records
            .retain(|id, _| live_focus == Some(id.as_str()));
        // State records outlive a whole-window observation gap but never a
        // departed lifetime, and a listed lifetime's published property set
        // is the exact re-arm boundary: a property that disappeared from the
        // row retires its record so a re-issued target writes natively.
        let mut live_state_tokens: HashSet<&str> =
            fresh.baseline.keys().map(String::as_str).collect();
        live_state_tokens.extend(reply.windows.iter().map(|row| row.lifetime.0.as_str()));
        let mut listed_props: HashMap<&str, HashSet<StateProperty>> = HashMap::new();
        let mut listed_auto: HashMap<&str, HashSet<(StateProperty, bool)>> = HashMap::new();
        for row in &reply.windows {
            let token = row.lifetime.0.as_str();
            let props = listed_props.entry(token).or_default();
            let autos = listed_auto.entry(token).or_default();
            for target in &row.state {
                props.insert(target.property);
                if target.automatic_clear {
                    autos.insert((target.property, target.value));
                }
            }
        }
        self.state_writes.retain(|(token, property), _| {
            live_state_tokens.contains(token.as_str())
                && listed_props
                    .get(token.as_str())
                    .is_none_or(|props| props.contains(property))
        });
        self.auto_clear_spent.retain(|(token, property, value)| {
            live_state_tokens.contains(token.as_str())
                && listed_auto
                    .get(token.as_str())
                    .is_none_or(|targets| targets.contains(&(*property, *value)))
        });
    }
}

/// Guarded trace label for one foreground state: `none` (verifiably no
/// foreground), `unknown` (unreadable, fail closed), else the raw
/// `hwnd:pid:creation` triple. Titles, exe paths, and content never ride
/// the trace; the triple joins to `member-admitted` rows (same fields) to
/// identify whether a `foreground-changed` veto saw a managed member or
/// a foreign window. Managed identity alone never proves causality.
pub(super) fn fg_trace_label(state: &ForegroundState) -> String {
    match state {
        ForegroundState::None => "none".to_owned(),
        ForegroundState::Unknown => "unknown".to_owned(),
        ForegroundState::Known(identity) => format!(
            "hwnd:{}:pid:{}:creation:{}",
            identity.hwnd, identity.pid, identity.creation
        ),
    }
}

/// GUI-thread focus for one window's owning thread: the thread plus its
/// currently active and keyboard-focus windows. Numeric identities only:
/// no titles, exe paths, or content ever ride the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuiFocusState {
    Unknown,
    Known {
        thread: u32,
        active: u64,
        focus: u64,
    },
}

/// Guarded trace label for one [`GuiFocusState`]: `unknown` when unreadable,
/// else the raw `thread:active:focus` triple joining to `member-admitted`
/// rows. Readability alone never proves which window the user sees.
pub(super) fn gui_trace_label(state: &GuiFocusState) -> String {
    match state {
        GuiFocusState::Unknown => "unknown".to_owned(),
        GuiFocusState::Known {
            thread,
            active,
            focus,
        } => format!("thread:{thread}:active:{active}:focus:{focus}"),
    }
}

/// Per-native-call focus record: what the adapter dispatched versus what
/// the native setter actually did. `primitive_called` is true only when the
/// `SetForegroundWindow` primitive ran; `api` is `Some` only then, `None`
/// on any pre-primitive veto. `native_return` is the raw primitive bool
/// (`Some(ok)`) whenever the primitive returned one, even when a later
/// post-setter fence turns the wrapper outcome into `Err`; `None` when the
/// primitive never ran (early veto) or never returned a bool (throwing
/// seam). `call_mono_ms`/`ret_mono_ms` strictly bracket
/// the primitive on the shared log `mono_ms` basis, never the surrounding
/// preflight. `gui_before` is the trace-gated target GUI-thread read
/// immediately before the call (`None` when trace is off: suppressed, never
/// `Unknown`-fabricated). The refusal detail below is refusal-only and
/// trace-armed only: `Some` solely on a traced `Ok(false)` pre-primitive
/// veto (scope, focus classification, focus-write veto, move/size veto),
/// `None` on every other outcome including success and off-trace vetos, so
/// success and off-trace paths probe nothing extra. `refusal_gate` names the
/// vetoing gate (`scope`, `classify-focus`, `focus-write-veto`,
/// `move-size-veto`); `refusal_reason` carries the focus-classification
/// reason only (`None` on the other gates). `target_visible`/`target_cloaked`
/// snapshot the observed target facts at dispatch and `target_hidden` the
/// hide-claim ledger at dispatch (identity only, never captions, paths, or
/// app content). `target_visible` is normally `Some(true)`: observation
/// rejects invisible windows up front, but a later facts re-read can race
/// to `visible: false`. Such a target refuses through `scope` if out of
/// scope, otherwise through `classify-focus` with reason `hidden`. `fg_identity`
/// samples the foreground HWND at dispatch as `none` (no foreground),
/// `target` (foreground already names the refused target HWND), or `other`.
/// It is a post-veto HWND relation only, not a fully identity-validated
/// foreground read and not gate-time causal proof: the vetos already decided
/// before this sample, and only a fresh proven read proves arrival.
/// The `fg_gate_*` detail below is refusal-only, trace-armed, and
/// `focus-write-veto`-only: `Some` solely on a traced `Ok(false)`
/// focus-write veto, `None` on every other gate and outcome. It replays the
/// gate's own decision reads (never a new native probe): `fg_gate_identity`
/// is the gate-time foreground HWND relation (`none`/`target`/`other`)
/// from the veto's own foreground sample, distinct from the later
/// post-veto `fg_identity` sample above; `fg_veto_kind` is the coarse veto
/// class (`readable-fullscreen` for a readable captionless-fullscreen
/// foreground, `unreadable` for a vetoed foreground that never validated
/// as the same-pass hidden departure, `hidden-departure` for a veto after
/// the foreground matched the known same-pass hidden departure);
/// `fg_readable` is the visible-lane readability of that sample;
/// `fg_shell`/`fg_fullscreen` are its facts when read (including the hidden
/// read inside the departure lane). `fg_visible` is the visible-read fact
/// on the readable lane, otherwise the existing entry `is_visible`
/// decision. `fg_exe` is the
/// foreground executable basename only, never a full path. `fg_shell` is
/// the observed shell boolean, never a class name; the relation is an HWND
/// relation, never a validated identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct FocusCallDiag {
    pub(super) primitive_called: bool,
    pub(super) native_return: Option<bool>,
    /// Actual bounded-prime outcome (`Some(inserted)`) whenever priming ran,
    /// including an exact-desktop arrival refused by post-prime rechecks.
    /// `None` when priming never ran or returned no count; never proof.
    pub(super) prime_inserted: Option<u8>,
    pub(super) api: Option<&'static str>,
    pub(super) call_mono_ms: Option<u64>,
    pub(super) ret_mono_ms: Option<u64>,
    pub(super) gui_before: Option<GuiFocusState>,
    pub(super) refusal_gate: Option<&'static str>,
    pub(super) refusal_reason: Option<&'static str>,
    pub(super) target_visible: Option<bool>,
    pub(super) target_cloaked: Option<bool>,
    pub(super) target_hidden: Option<bool>,
    pub(super) fg_identity: Option<&'static str>,
    pub(super) fg_gate_identity: Option<&'static str>,
    pub(super) fg_veto_kind: Option<&'static str>,
    pub(super) fg_readable: Option<bool>,
    pub(super) fg_shell: Option<bool>,
    pub(super) fg_fullscreen: Option<bool>,
    pub(super) fg_visible: Option<bool>,
    pub(super) fg_exe: Option<String>,
    /// True only when the foreground-fullscreen veto was passed through the
    /// exact-desktop explicit-selection exception on this call (the setter
    /// may still have refused at a later gate, so this never implies
    /// actuation or arrival). `false` on every other path, so rows without
    /// the exception keep their prior shape.
    pub(super) desktop_exception: bool,
}

/// Trace-only native API vocabulary for focus dispatch rows: the exact
/// setter that ran. `SetForegroundWindow` is the synchronous activating
/// focus call (its boolean return never proves arrival; only the
/// read-back does, and no `GetLastError` is read: the API defines no
/// useful last error). `prime_inserted` is the actual E8 last-input prime
/// outcome (`Some(inserted)` whenever priming ran, including post-prime
/// arrival recheck refusals; `None` when priming never ran); it is never
/// trusted as proof. The nonactivating view path hides/reveals through
/// async `SetWindowPos` without pumping (hide `0x4097`, show `0x4057`, see
/// [`crate::product_hide::sys::NA_POS_HIDE_FLAGS`]/
/// [`crate::product_hide::sys::NA_POS_SHOW_FLAGS`]). A focus activation can
/// land later or never; a visibility post is nonactivating, not a focus
/// setter. Effect rows that dispatched no setter carry `api: None`;
/// executed visibility calls ride `visibility_posts`, never the veto row.
pub(super) const FOCUS_SETTER_API: &str = "SetForegroundWindow";

/// Trace-only per-request focus dispatch record (stage 4b FFC oracle).
/// One row per retained focus request, bounded by
/// [`MAX_FOCUS_DIAG`]: `dispatched` tells whether the native focus setter
/// ran; `setter` is the raw setter disposition (`accepted`/`refused`/
/// `threw`, or `skipped` when no setter ran); `foreground` is the immediate
/// foreground proof read right after dispatch (always present, even when
/// false, so dispatched-but-unproven is distinguishable from no setter);
/// `hold` names the individual hold when the request did not prove
/// (`None` only on proven foreground arrival). `fg_baseline`/`fg_live` ride
/// only the `foreground-changed` veto rows: the entry baseline versus the
/// post-switch live read that vetoed, as [`fg_trace_label`] strings, so a
/// later join against `member-admitted` rows identifies managed versus
/// foreign windows without attributing causality from membership alone.
/// Pure trace state, never a policy input: the core consumes no focus
/// outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct FocusDispatchDiag {
    pub(super) id: String,
    pub(super) target: String,
    pub(super) kind: &'static str,
    pub(super) dispatched: bool,
    pub(super) setter: &'static str,
    pub(super) foreground: bool,
    pub(super) hold: Option<&'static str>,
    pub(super) fg_baseline: Option<String>,
    pub(super) fg_live: Option<String>,
    /// Exact native setter that ran (`SetForegroundWindow` for focus
    /// effects, the async nonactivating `SetWindowPos` path for the
    /// `foreground-changed` veto rows); `None` when no setter ran.
    pub(super) api: Option<&'static str>,
    /// Setter call/return stamps strictly around the native primitive (see
    /// [`FocusCallDiag`]): the actual native call window, not the
    /// post-apply emission time and not the surrounding preflight. `None`
    /// when the primitive never ran. Call-to-return alone never proves
    /// arrival.
    pub(super) call_mono_ms: Option<u64>,
    pub(super) ret_mono_ms: Option<u64>,
    /// Target GUI-thread active/focus read immediately before the primitive
    /// (see [`gui_trace_label`]); `None` when the primitive never ran or
    /// trace is off (suppressed). A dispatch-time read only, never proof
    /// of where focus later landed.
    pub(super) gui_dispatch: Option<String>,
    /// Target GUI-thread active/focus read at the immediate read-back when
    /// the setter proved foreground on this pass (`None` otherwise: no
    /// landing, trace off, or veto). Sampled at `ret_mono_ms`, never at
    /// edge time.
    pub(super) gui_landing: Option<String>,
    /// Owner-clock stamp immediately after the existing `is_foreground`
    /// read-back (TRACE-ONLY, never a native reread). `ret_mono_ms` is the
    /// native return before proof; this is the result proof context. `Some`
    /// only on dispatched rows that ran the read-back; `None` on skipped
    /// rows that never proved. Never a setter-return-as-proof claim.
    pub(super) fg_proof_mono_ms: Option<u64>,
    /// Whether the native setter primitive actually ran for this row
    /// (adapter dispatched vs primitive executed). Pre-primitive vetos
    /// report `dispatched: true` with `primitive_called: false`.
    pub(super) primitive_called: bool,
    /// Raw `SetForegroundWindow` return for this row (`Some(ok)`) whenever
    /// the primitive ran, even when a post-setter fence turns the wrapper
    /// `setter` into `threw`; `None` when the primitive never ran (early
    /// veto) or never returned a bool. Lets a user trace distinguish native
    /// acceptance from wrapper fences without re-reading gates.
    pub(super) native_return: Option<bool>,
    /// Actual bounded-prime outcome (`Some(inserted)`) whenever priming ran,
    /// including post-prime arrival recheck refusals without a setter.
    /// `None` when priming never ran; never arrival proof.
    pub(super) prime_inserted: Option<u8>,
    /// Full target native identity (HWND plus PID plus process creation)
    /// for the `member-admitted` join. `None` only when the target
    /// baseline was unknown.
    pub(super) hwnd: Option<u64>,
    pub(super) pid: Option<u32>,
    pub(super) creation: Option<String>,
    /// Trace-only refusal detail projected from [`FocusCallDiag`]: the
    /// vetoing pre-primitive gate plus non-sensitive safe state, `Some`
    /// solely on a traced `Ok(false)` pre-primitive veto (see
    /// [`FocusCallDiag`]), `None` otherwise. Serialized into the
    /// `world-apply` row only when `refusal_gate` is `Some`; non-refusal
    /// rows omit the keys entirely, so row parsers must tolerate their
    /// absence. Refusal rows retain a null `refusal_reason` outside the
    /// classification gate. `fg_identity` stays a post-veto HWND relation there too,
    /// never gate-time causal proof. The `fg_gate_*` detail rides only the
    /// traced `focus-write-veto` refusal rows (see [`FocusCallDiag`]);
    /// every other refusal row omits those keys entirely.
    pub(super) refusal_gate: Option<&'static str>,
    pub(super) refusal_reason: Option<&'static str>,
    pub(super) target_visible: Option<bool>,
    pub(super) target_cloaked: Option<bool>,
    pub(super) target_hidden: Option<bool>,
    pub(super) fg_identity: Option<&'static str>,
    pub(super) fg_gate_identity: Option<&'static str>,
    pub(super) fg_veto_kind: Option<&'static str>,
    pub(super) fg_readable: Option<bool>,
    pub(super) fg_shell: Option<bool>,
    pub(super) fg_fullscreen: Option<bool>,
    pub(super) fg_visible: Option<bool>,
    pub(super) fg_exe: Option<String>,
    /// Trace-only exact-desktop exception marker projected from
    /// [`FocusCallDiag`]: `true` solely when this dispatch passed the
    /// foreground-fullscreen veto through the explicit-selection desktop
    /// exception. The marker records the gate pass only: a later gate may
    /// still refuse (or the setter may never run), so it never implies
    /// actuation or arrival. Serialized into the `world-apply` row only
    /// when `true`; rows without the exception omit the key entirely.
    pub(super) desktop_exception: bool,
}

/// Trace-only in-flight clearing/landing record: why one remembered focus
/// flight ended. `cause` is one of `landed-own` (exact full-identity plus
/// lifetime match on a drained edge), `superseded-foreign` (another
/// readable foreground), `superseded-new-setter` (a newer accepted setter
/// replaced it), `consumed-na-own-landing` / `superseded-na-guard` (the
/// nonactivating-view guard consumed or cleared it), `lifetime-end` (target
/// departed or recycled before any edge), or `reset-generation` (namespace
/// reset). `edge_*` carry the clearing edge's delivery/owner timestamps
/// (`None` for non-edge clears); `resolve_mono_ms` is the apply-entry or
/// setter moment that cleared it. `gui` is the target GUI-thread
/// active/focus sampled at `resolve_mono_ms` (`None` when trace is off or
/// unreadable: suppressed, never edge-time). Clearing rows never
/// claim the edge caused the activation, only that the remembered flight
/// ended here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct InFlightEventDiag {
    pub(super) token: String,
    pub(super) effect: String,
    pub(super) cause: &'static str,
    pub(super) edge_event_ms: Option<u32>,
    pub(super) edge_queued_mono_ms: Option<u64>,
    pub(super) edge_drain_mono_ms: Option<u64>,
    pub(super) resolve_mono_ms: u64,
    pub(super) gui: Option<String>,
    /// Full remembered target identity (HWND plus PID plus process
    /// creation) for the `member-admitted` join.
    pub(super) hwnd: Option<u64>,
    pub(super) pid: Option<u32>,
    pub(super) creation: Option<String>,
}

/// Trace-only resolution-time GUI read: the target GUI-thread label
/// sampled now, or `None` when trace is off (suppressed: the seam performs
/// no native read). Never a gate, never proof of arrival.
pub(super) fn gui_resolution_label(ops: &mut dyn NativeOps, hwnd: u64) -> Option<String> {
    if !ops.diagnostics_trace() {
        return None;
    }
    Some(gui_trace_label(&ops.gui_focus_of(hwnd)))
}

/// Bound for [`FocusDispatchDiag`] rows per apply pass. The retained focus
/// request is already bounded by the reply byte budget; this caps only the
/// retained trace rows with an honest omitted count in the tick diag.
pub(super) const MAX_FOCUS_DIAG: usize = 8;

/// Bound for [`InFlightEventDiag`] rows per apply pass, with an honest
/// omitted count derived from the unbounded `inflight_total` below.
pub(super) const MAX_INFLIGHT_DIAG: usize = 8;

/// Bound for drained foreground-edge metadata retained per pass for the
/// flight join. The production HWND queue itself is unbounded per pass
/// (every drained edge resolves); only this diagnostic sidecar caps.
pub(super) const MAX_PENDING_FOREGROUND_META: usize = 32;

/// Bound for executed visibility-post rows per apply pass, with an honest
/// omitted count in the tick diag. Single source of truth lives at the
/// post site ([`crate::product_hide::sys::MAX_VISIBILITY_POSTS`]); the
/// source accumulator and this pass total share it, so neither side can
/// drift the cap.
pub(super) const MAX_VISIBILITY_DIAG: usize = crate::product_hide::sys::MAX_VISIBILITY_POSTS;

/// Bound for post-apply proven-homing trace rows per pass, matching the
/// fresh receipt-views cap in the tick diag so arrival and full-view maps
/// share one bound with honest omission.
pub(super) const MAX_RECEIPT_HOMING_DIAG: usize = 16;

/// Honest outcome of one diff pass. `partial` is true when at least one
/// write landed while at least one row held: never atomic.
/// `migration_total`/`migration_proven` bound the migration membership
/// phase per pass (members carried vs members proven at the actuation
/// gates): the deferred target/refill views actuate only after every
/// member proves, so a stuck migration reads as proven < total with
/// held > 0 instead of silent zero-write ambiguity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ApplyStats {
    pub(super) geometry_writes: usize,
    pub(super) membership_writes: usize,
    pub(super) view_writes: usize,
    pub(super) creations: usize,
    pub(super) removals: usize,
    pub(super) effects: usize,
    /// Mechanical cursor warps actually dispatched this pass (no-op
    /// already-there skips and duplicate-observation skips count nothing).
    pub(super) cursor_warps: usize,
    pub(super) held: usize,
    pub(super) migration_total: usize,
    pub(super) migration_proven: usize,
    /// Staged workspace-switch phases dispatched this pass: arrival reveals
    /// that proved ready, arrival-ready focuses dispatched, and departure
    /// hides that completed with view proof. Observability only; the
    /// at-most-once fences live in [`DispatchFences`].
    pub(super) staged_reveals: usize,
    pub(super) staged_focus: usize,
    pub(super) staged_hides: usize,
    pub(super) stopped_mid_apply: bool,
    pub(super) partial: bool,
    /// Trace-only per-effect focus dispatch rows (see [`FocusDispatchDiag`]).
    /// Bounded to [`MAX_FOCUS_DIAG`] entries in reply order.
    pub(super) focus_dispatch: Vec<FocusDispatchDiag>,
    /// Trace-only in-flight clearing/landing rows (see [`InFlightEventDiag`]):
    /// why each remembered focus flight ended this pass. Bounded to
    /// [`MAX_INFLIGHT_DIAG`] entries; the unbounded total rides
    /// `inflight_total` below so the tick diag reports an honest omitted
    /// count past the cap.
    pub(super) inflight_events: Vec<InFlightEventDiag>,
    /// Trace-only count of every in-flight clearing attribution this pass,
    /// including rows past the [`MAX_INFLIGHT_DIAG`] storage cap.
    pub(super) inflight_total: usize,
    /// Trace-only count of drained foreground edges whose metadata exceeded
    /// the [`MAX_PENDING_FOREGROUND_META`] sidecar cap this pass (drained
    /// from the HostDiff counter at apply entry, same point the pending
    /// queue itself is taken). The production `pending_foreground` HWNDs
    /// still resolve; only this diagnostic sidecar folds.
    pub(super) foreground_meta_omitted: u64,
    /// Trace-only executed visibility posts this pass (see
    /// [`crate::product_hide::sys::VisibilityPostDiag`]): the actual host
    /// API that ran per hide/reveal post, in execution order. Bounded to
    /// [`MAX_VISIBILITY_DIAG`] entries; the unbounded total rides
    /// `visibility_total` below so the tick diag reports an honest
    /// omitted count past the cap. Empty unless trace is armed; arrivals
    /// still prove only through the visibility readback, never these rows.
    pub(super) visibility_posts: Vec<crate::product_hide::sys::VisibilityPostDiag>,
    /// Trace-only count of every executed visibility post this pass,
    /// including rows past the [`MAX_VISIBILITY_DIAG`] storage cap.
    pub(super) visibility_total: usize,
    /// Trace-only all-show post boundary: the latest accepted show
    /// `ret_mono_ms` across the drained post rows (`hide == false` with a
    /// posted verdict) this pass. `None` when no show posted (empty arrival,
    /// fully held, or hides-only): never fabricated. Hides alone and
    /// refused shows never stamp this; per-row `posted` verdicts stay
    /// authoritative. Clock-only trace data riding the existing rows; no
    /// new native query.
    pub(super) show_posts_done_mono_ms: Option<u64>,
    /// Trace-only pre-focus all-arrivals-visible proof time: owner-clock
    /// stamp immediately after the existing `staged_arrival_ready` fresh
    /// re-reads prove all required arrivals visible (staged lane only).
    /// `None` on the current-order path (no pre-focus arrival gate) and on
    /// empty targets (no arrival means no authority): never fabricated.
    /// The post-apply homing map (`receipt_homing` at `receipt_proof_mono_ms`)
    /// is the current-order equivalent arrival proof. Clock-only; the
    /// proof itself is the unchanged existing re-reads.
    pub(super) arrivals_visible_mono_ms: Option<u64>,
    /// Trace-only departure-hidden proof time: owner-clock stamp immediately
    /// after an existing successful view `read_view` proof (single-shot
    /// switch, staged hide, migration/follow views). `None` when no view
    /// proof completed this pass. Clock-only alongside the existing proof;
    /// the setter boolean never decides.
    pub(super) departures_hidden_mono_ms: Option<u64>,
    /// Trace-only post-apply arrival map: fresh homing proven by the
    /// existing `receipt_updates` read, captured before merged applied
    /// transfers (fresh-only). Triples `(token, output, view)`, sorted and
    /// bounded; empty means zero fresh arrival proof, never an echo.
    pub(super) receipt_homing: Vec<(String, String, String)>,
    /// Trace-only count of every fresh proven homing entry this pass,
    /// including rows past the cap.
    pub(super) receipt_homing_total: usize,
}

impl ApplyStats {
    /// Trace-only in-flight clearing attribution: counts every clearing in
    /// `inflight_total` and retains only the first [`MAX_INFLIGHT_DIAG`]
    /// rows. Never a policy input: the flight already ended before this
    /// records why.
    pub(super) fn note_inflight(&mut self, row: InFlightEventDiag) {
        self.inflight_total = self.inflight_total.saturating_add(1);
        if self.inflight_events.len() < MAX_INFLIGHT_DIAG {
            self.inflight_events.push(row);
        }
    }

    /// Trace-only executed visibility post: counts every post in
    /// `visibility_total` and retains only the first
    /// [`MAX_VISIBILITY_DIAG`] rows. Never a policy input: the post
    /// already ran before this records which host API executed.
    pub(super) fn note_visibility_post(
        &mut self,
        row: crate::product_hide::sys::VisibilityPostDiag,
    ) {
        self.visibility_total = self.visibility_total.saturating_add(1);
        if self.visibility_posts.len() < MAX_VISIBILITY_DIAG {
            self.visibility_posts.push(row);
        }
    }
}

/// Scope authority for one `(output, workspace)` domain, matching the core
/// envelope: [`ScopeStatus::scope`](tiler_core::world_report::ScopeStatus)
/// is a typed authority scope, and omission inside a held or untouched
/// scope authorizes no write or retirement.
fn scope_controlled(reply: &WorldReplyV4, output: &str, workspace: &str) -> bool {
    let output = OutputId(output.to_owned());
    reply.scopes.iter().any(|status| {
        status.authority == ScopeAuthority::Controlled
            && status.scope.covers_workspace(&output, workspace)
    })
}

/// Exact workspace row authority: a broad output scope never overrides a
/// more specific workspace row (a floating workspace must stay Untouched
/// even when its output scope is Controlled).
fn exact_workspace_controlled(reply: &WorldReplyV4, output: &str, workspace: &str) -> bool {
    let output = OutputId(output.to_owned());
    reply.scopes.iter().any(|status| {
        matches!(
            &status.scope,
            AuthorityScope::Workspace {
                output: scope_output,
                workspace: scope_workspace,
            } if scope_output == &output && scope_workspace.0 == workspace
        ) && status.authority == ScopeAuthority::Controlled
    })
}

/// Whether the output row proves this workspace is the current view. A
/// missing output row or a `None` logical view is not proof that the
/// workspace is current, so it withholds an otherwise-authorized frame
/// write. A named different view is core-owned hidden state: native hide
/// may still lag the view switch, but the departing window must not be
/// reflowed before it hides.
fn row_on_current_view(reply: &WorldReplyV4, output: &str, workspace: &str) -> bool {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.as_deref())
        .is_some_and(|view| view == workspace)
}

/// Mechanical actuation authority for one `(output, workspace)` domain:
/// standalone view switches plus transfer-coupled membership moves and
/// their views. Controlled scopes actuate exactly as before, and floating
/// (Untouched) targets actuate too when their output row is unheld and no
/// gesture barrier is up. A floating selection or migration still needs
/// its native hide/reveal and membership effects with truthful read-back
/// proof; without them the retained core flight could never complete.
/// The switch and membership setters perform no sweep frame writes
/// (hide/reveal effects and core-owned membership-coupled placements
/// only): frame/membership row writes keep the stricter Controlled-only
/// gate above, and the core still owns completion over the reported raw
/// attempts plus the staged observation. Held/unknown scopes, held
/// outputs, and mid-gesture Untouched targets hold with no attempt,
/// exactly as before.
fn scope_actuatable(
    reply: &WorldReplyV4,
    ops: &mut dyn NativeOps,
    output: &str,
    workspace: &str,
) -> bool {
    let scope_output = OutputId(output.to_owned());
    let Some(authority) = reply
        .scopes
        .iter()
        .find(|status| status.scope.covers_workspace(&scope_output, workspace))
        .map(|status| status.authority)
    else {
        return false;
    };
    match authority {
        ScopeAuthority::Controlled => true,
        ScopeAuthority::Untouched => {
            let output_held = reply
                .outputs
                .iter()
                .find(|row| row.output.0 == output)
                .is_none_or(|row| row.hold.is_some());
            !output_held && !ops.gesture_barrier()
        }
        ScopeAuthority::Hold(_) => false,
    }
}

/// Whether one retained warp point is within the adapter's settable
/// geometry bound (the same bound the live setter refuses); an overbound
/// carry is never passed to the OS.
fn warp_point_settable(x: i32, y: i32) -> bool {
    let bound = tiler_core::bounds::GEOMETRY_BOUND;
    (-bound..=bound).contains(&x) && (-bound..=bound).contains(&y)
}

/// Freshness plus racing-input poll before a setter. Consumes at most one
/// pending external invalidation; on any staleness the caller stops before
/// remaining unsafe writes.
fn ensure_fresh(
    reply: &WorldReplyV4,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
) -> bool {
    if !gate.fresh_reply(reply) || gate.current_obs != entry_obs {
        return false;
    }
    if ops.poll_external_invalidation() {
        gate.current_obs = gate.current_obs.saturating_add(1);
        return false;
    }
    true
}

/// Setter gate verdict: pass, hold this row narrowly, or stop the pass.
/// Fencing and identity failures stop (global trust); row-local safety
/// drift holds (one raced row never halts the world).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    Pass,
    Hold,
    Stop,
}

/// What one setter needs from the live re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetterKind {
    /// Tiled frame write: any gesture suspends, overlays and float lanes
    /// never take automatic writes.
    Frame,
    /// Explicit float placement: no automatic authority, so only the stable
    /// token plus strict eligibility authorizes.
    Float,
    /// Membership and focus: overlays may move or focus, ineligible safety
    /// facts may not.
    Membership,
}

pub(super) fn lifetime_ok(fresh: &FreshWindow, stored: &HashMap<String, String>) -> bool {
    if fresh.key.hwnd == 0 || fresh.key.pid == 0 || fresh.key.creation.is_empty() {
        return false;
    }
    let Some(live) = fresh.live_tag.as_deref() else {
        return false;
    };
    if live.is_empty() {
        return false;
    }
    stored.get(&fresh.token).is_some_and(|tag| tag == live)
}

/// One truthful visibility read: the identity/lifetime/project-suppression
/// sample plus the native fullscreen fact used only to classify a bounded
/// hold reason. Missing facts stay `None` (unknown never proves).
#[derive(Debug, Clone, Copy)]
struct VisibilityRead {
    sample: VisibilitySample,
    fullscreen: bool,
    /// Sticky windows stay visible on every workspace: the adapter never
    /// issues a hide for them, matching the combined-switch departure rule.
    sticky: bool,
}

fn read_visibility(
    ops: &mut dyn NativeOps,
    fresh: &FreshHost,
    token: &str,
    obs: u64,
) -> Option<VisibilityRead> {
    let live = ops.read_window(token)?;
    let matched = fresh
        .baseline
        .get(token)
        .is_some_and(|base| base.key == live.key && lifetime_ok(&live, &fresh.stored_tags));
    let facts = live.facts;
    let visible = if live.hidden {
        Some(false)
    } else {
        facts.map(|facts| facts.visible)
    };
    // A committed hide claim is the only proof of project-owned
    // suppression. An unhidden invisible window (cloaked, occlusion) is
    // never claimed as ours.
    let suppression_owned = match visible {
        Some(_) if live.hidden => Some(true),
        Some(_) => Some(false),
        None => None,
    };
    Some(VisibilityRead {
        sample: VisibilitySample {
            obs,
            lifetime_matched: Some(matched),
            visibility: VisibilityFact {
                visible,
                suppression_owned,
            },
            minimized: Some(live.iconic || facts.is_some_and(|facts| facts.minimized)),
            host_hidden: facts.map(|facts| facts.cloaked),
        },
        fullscreen: facts.is_some_and(|facts| facts.captionless_fullscreen),
        sticky: live.sticky,
    })
}

/// Bounded mechanical reason carried by a not-called visibility row. The
/// core's hold/exclusion authority maps to the closed wire vocabulary; a
/// live fullscreen fact is always the more specific `Fullscreen`.
fn visibility_reason(authority: ScopeAuthority, read: &VisibilityRead) -> VisibilityReason {
    if read.fullscreen {
        return VisibilityReason::Fullscreen;
    }
    match authority {
        ScopeAuthority::Controlled => {
            if read.sample.visibility.visible.is_none() {
                VisibilityReason::Unreadable
            } else if read.sample.minimized == Some(true) || read.sample.host_hidden == Some(true) {
                VisibilityReason::ShowBlocked
            } else if read.sample.visibility.visible == Some(false)
                && read.sample.visibility.suppression_owned != Some(true)
            {
                VisibilityReason::Unsupported
            } else {
                VisibilityReason::Empty
            }
        }
        ScopeAuthority::Untouched => VisibilityReason::Unsupported,
        ScopeAuthority::Hold(HoldReason::Unreadable | HoldReason::IncompleteInventory) => {
            VisibilityReason::Unreadable
        }
        ScopeAuthority::Hold(HoldReason::Minimized) => VisibilityReason::ShowBlocked,
        ScopeAuthority::Hold(_) => VisibilityReason::ScopeHeld,
    }
}

fn frame_setter_for(outcome: Result<bool, ()>) -> FrameSetter {
    match outcome {
        Ok(true) => FrameSetter::Accepted,
        Ok(false) => FrameSetter::Refused,
        Err(()) => FrameSetter::Threw,
    }
}

/// Terminal show proof for one visibility sample: the identity/lifetime
/// matched, the window is visible, and no native fact blocks it (minimized
/// or host-hidden). Anything else is a hold, never a show verdict.
fn show_sample_proven(sample: &VisibilitySample) -> bool {
    sample.lifetime_matched == Some(true)
        && sample.visibility.visible == Some(true)
        && sample.minimized == Some(false)
        && sample.host_hidden == Some(false)
}

fn focus_reason_from_visibility(disposition: VisibilityDisposition) -> FocusNoCallReason {
    match disposition {
        VisibilityDisposition::Called {
            setter: FrameSetter::Accepted,
            ..
        } => FocusNoCallReason::ShowBlocked,
        VisibilityDisposition::Called {
            setter: FrameSetter::Refused,
            ..
        }
        | VisibilityDisposition::Called {
            setter: FrameSetter::Threw,
            ..
        } => FocusNoCallReason::NativeRefused,
        VisibilityDisposition::NotCalled { reason } => match reason {
            VisibilityReason::Unreadable => FocusNoCallReason::Unreadable,
            VisibilityReason::ScopeHeld => FocusNoCallReason::ScopeHeld,
            VisibilityReason::Unsupported => FocusNoCallReason::Unsupported,
            VisibilityReason::Fullscreen => FocusNoCallReason::Fullscreen,
            VisibilityReason::Game => FocusNoCallReason::Game,
            VisibilityReason::IdentityStale => FocusNoCallReason::IdentityStale,
            _ => FocusNoCallReason::ShowBlocked,
        },
    }
}

/// One pass's frozen-arrival scan: all members ready, or the first blocked
/// member with the bounded reason its truthful visibility row carries.
struct ArrivalScan {
    ready: bool,
    blocked: Option<FocusNoCallReason>,
    brackets: Vec<ArrivalBracket>,
}

fn scan_arrival(
    members: &[AfterShownMember],
    attempts: &BTreeMap<String, VisibilityAttempt>,
    ops: &mut dyn NativeOps,
    fresh: &FreshHost,
    obs: u64,
) -> ArrivalScan {
    let mut all_ready = true;
    let mut blocked = None;
    let mut brackets = Vec::with_capacity(members.len());
    for member in members {
        let before = read_visibility(ops, fresh, &member.win.0, obs);
        let homing = ops.read_homing(&member.win.0);
        let affinity = ops.read_output_affinity(&member.win.0);
        let frame = ops.read_frame(&member.win.0);
        let after = read_visibility(ops, fresh, &member.win.0, obs);
        let ready_state = before.is_some_and(|read| {
            read.sample.lifetime_matched == Some(true)
                && read.sample.visibility.visible == Some(true)
                && read.sample.minimized == Some(false)
                && read.sample.host_hidden == Some(false)
        }) && after.is_some_and(|read| {
            read.sample.lifetime_matched == Some(true)
                && read.sample.visibility.visible == Some(true)
                && read.sample.minimized == Some(false)
                && read.sample.host_hidden == Some(false)
        }) && homing
            .as_ref()
            .is_some_and(|(out, ws)| out == &member.out.0 && ws == &member.ws)
            && affinity.as_deref() == Some(member.out.0.as_str())
            && frame.is_some()
            && member.frame.is_none_or(|target| frame == Some(target));
        if !ready_state {
            all_ready = false;
        }
        if blocked.is_none()
            && let Some(attempt) = attempts.get(&member.win.0)
            && attempt.target
        {
            match attempt.disposition {
                VisibilityDisposition::Called {
                    setter: FrameSetter::Accepted,
                    ..
                } => {}
                // A satisfied target is a truthful no-call success, not a
                // blocker: the member is already shown.
                VisibilityDisposition::NotCalled {
                    reason: VisibilityReason::Empty,
                } => {}
                _ => blocked = Some(focus_reason_from_visibility(attempt.disposition)),
            }
        }
        let unknown = VisibilitySample {
            obs,
            lifetime_matched: Some(false),
            visibility: VisibilityFact {
                visible: None,
                suppression_owned: None,
            },
            minimized: None,
            host_hidden: None,
        };
        // The output arm carries only the observed live affinity of the
        // member; an unreadable affinity yields no bracket at all (a null
        // output could never prove readiness, so it is not reported as if
        // it were observed evidence).
        if let Some(out) = affinity.as_deref() {
            brackets.push(ArrivalBracket {
                window: member.win.clone(),
                obs,
                before: before.map_or(unknown, |read| read.sample),
                location: ArrivalLocation::Output {
                    out: Some(OutputId(out.to_owned())),
                },
                frame,
                after: after.map_or(unknown, |read| read.sample),
            });
        }
    }
    ArrivalScan {
        ready: all_ready,
        blocked,
        brackets,
    }
}

/// Truthful show half of the visibility lane. Every desired shown row is
/// decided exactly once per `(window, target, revision)` tuple: an
/// unchanged tuple redelivers its original row without a setter, and a
/// satisfied target is a real not-called row. A non-Controlled or
/// unreadable target emits no row (a transient hold is never latchable),
/// and only a Controlled, unsatisfied, identity-clean target reaches one
/// nonactivating reveal. An accepted reveal whose readback does not prove
/// stays pending and emits no row until a later proving read. Setter
/// booleans never prove: the post re-read is the row's fact.
#[allow(clippy::too_many_arguments)]
fn stage_visibility_shows(
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    gate: &mut DiffGate,
    entry_obs: u64,
    attempts: &mut BTreeMap<String, VisibilityAttempt>,
    records: &mut BTreeMap<String, VisibilityAttempt>,
    revealed: &mut HashSet<String>,
    stats: &mut ApplyStats,
) {
    let obs = gate.current_obs;
    for row in &reply.windows {
        let Some(target) = row.visibility else {
            continue;
        };
        if !target.shown {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let token = row.lifetime.0.as_str();
        if let Some(existing) = records.get(token) {
            // A reported row is a verdict for this tuple, called or not:
            // redeliver it without a new native decision. Only a new
            // target or authorization revision may re-decide.
            if existing.target == target.shown && existing.revision == target.revision {
                // An accepted reveal whose immediate readback did not prove
                // is still in flight: re-read (never the setter) and only a
                // proving sample may emit the row. Anything else holds with
                // no row so a later pass can still settle it.
                if matches!(
                    existing.disposition,
                    VisibilityDisposition::Called {
                        setter: FrameSetter::Accepted,
                        ..
                    }
                ) && existing.post.is_none()
                {
                    let Some(read) = read_visibility(ops, fresh, token, obs) else {
                        stats.held += 1;
                        continue;
                    };
                    if !show_sample_proven(&read.sample) {
                        stats.held += 1;
                        continue;
                    }
                    let mut settled = existing.clone();
                    settled.post = Some(read.sample);
                    records.insert(token.to_owned(), settled.clone());
                    attempts.insert(token.to_owned(), settled);
                    continue;
                }
                attempts.insert(token.to_owned(), existing.clone());
                continue;
            }
        }
        let Some(read) = read_visibility(ops, fresh, token, obs) else {
            // Unreadable is transient: emit no row so the core's first-match
            // latch cannot permanently fail a later readable pass.
            stats.held += 1;
            continue;
        };
        let satisfied = show_sample_proven(&read.sample);
        let (disposition, called) = if row.visibility_authority != ScopeAuthority::Controlled {
            // Held/excluded authority is transient: emit no row at all (the
            // core's first-match latch must not pin this pass's hold).
            stats.held += 1;
            continue;
        } else if satisfied {
            (
                VisibilityDisposition::NotCalled {
                    reason: VisibilityReason::Empty,
                },
                false,
            )
        } else if read.sample.lifetime_matched != Some(true) {
            (
                VisibilityDisposition::NotCalled {
                    reason: VisibilityReason::IdentityStale,
                },
                false,
            )
        } else if read.sample.visibility.visible == Some(false) {
            if read.sample.visibility.suppression_owned == Some(false) {
                (
                    VisibilityDisposition::NotCalled {
                        reason: VisibilityReason::Unsupported,
                    },
                    false,
                )
            } else {
                if !ensure_fresh(reply, gate, entry_obs, ops) {
                    stats.stopped_mid_apply = true;
                    break;
                }
                let outcome = ops.reveal_window(token);
                if outcome == Ok(true) {
                    stats.staged_reveals += 1;
                    revealed.insert(token.to_owned());
                }
                (
                    VisibilityDisposition::Called {
                        setter: frame_setter_for(outcome),
                        reason: VisibilityReason::Empty,
                    },
                    true,
                )
            }
        } else {
            (
                VisibilityDisposition::NotCalled {
                    reason: visibility_reason(row.visibility_authority, &read),
                },
                false,
            )
        };
        let accepted = matches!(
            disposition,
            VisibilityDisposition::Called {
                setter: FrameSetter::Accepted,
                ..
            }
        );
        let post = if called {
            read_visibility(ops, fresh, token, gate.current_obs)
        } else {
            Some(read)
        };
        if accepted && post.as_ref().is_none_or(|read| !show_sample_proven(&read.sample)) {
            // Async reveal: the accepted setter has not proven yet. Store
            // the Called record with no post and emit no attempt (hold) so
            // the next pass re-reads without re-actuating; never latch a
            // premature visible:false for an accepted reveal.
            let attempt = VisibilityAttempt {
                window: row.lifetime.clone(),
                target: true,
                revision: target.revision,
                path: LastWritePath::Staged,
                disposition,
                pre: Some(read.sample),
                post: None,
            };
            records.insert(token.to_owned(), attempt);
            stats.held += 1;
            continue;
        }
        let attempt = VisibilityAttempt {
            window: row.lifetime.clone(),
            target: true,
            revision: target.revision,
            path: LastWritePath::Staged,
            disposition,
            pre: Some(read.sample),
            post: post.map(|read| read.sample),
        };
        if matches!(attempt.disposition, VisibilityDisposition::Called { .. }) {
            stats.held += usize::from(post.is_none());
        }
        records.insert(token.to_owned(), attempt.clone());
        attempts.insert(token.to_owned(), attempt);
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
}

/// Truthful hide half of the visibility lane, run after the focus lane.
/// Departing targets hide only under a proven live foreground (or the
/// target's explicit fallback authority); a non-Controlled or unreadable
/// target emits no row and is never actuated, so a transient hold never
/// latches. The same at-most-once tuple rule applies: a reported row,
/// called or not-called, is terminal for its `(window, target, revision)`
/// and is redelivered without re-deciding, while a pending decision emits
/// no row and may still re-decide on a later pass.
#[allow(clippy::too_many_arguments)]
fn stage_visibility_hides(
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    gate: &mut DiffGate,
    entry_obs: u64,
    attempts: &mut BTreeMap<String, VisibilityAttempt>,
    records: &mut BTreeMap<String, VisibilityAttempt>,
    focus_records: &BTreeMap<String, FocusFeedback>,
    stats: &mut ApplyStats,
) {
    let obs = gate.current_obs;
    for row in &reply.windows {
        let Some(target) = row.visibility else {
            continue;
        };
        if target.shown {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let token = row.lifetime.0.as_str();
        if let Some(existing) = records.get(token) {
            // A reported row is a verdict for this tuple, called or not:
            // redeliver it without a new native decision. Pending decisions
            // emit no row above and may still re-decide on a later pass.
            if existing.target == target.shown && existing.revision == target.revision {
                attempts.insert(token.to_owned(), existing.clone());
                continue;
            }
        }
        let Some(read) = read_visibility(ops, fresh, token, obs) else {
            // Unreadable is transient: emit no row so the core's first-match
            // latch cannot permanently fail a later readable pass.
            stats.held += 1;
            continue;
        };
        if row.visibility_authority != ScopeAuthority::Controlled {
            // Held/excluded authority is transient: emit no row at all (the
            // core's first-match latch must not pin this pass's hold).
            stats.held += 1;
            continue;
        }
        if read.sticky {
            // Sticky windows are visible on every workspace; a hide would
            // contradict the adapter's own combined-switch departure rule.
            // Report the truthful no-call instead of actuating.
            let attempt = VisibilityAttempt {
                window: row.lifetime.clone(),
                target: false,
                revision: target.revision,
                path: LastWritePath::Staged,
                disposition: VisibilityDisposition::NotCalled {
                    reason: VisibilityReason::Empty,
                },
                pre: Some(read.sample),
                post: Some(read.sample),
            };
            records.insert(token.to_owned(), attempt.clone());
            attempts.insert(token.to_owned(), attempt);
            continue;
        }
        let hidden = read.sample.visibility.visible == Some(false)
            && read.sample.minimized == Some(false)
            && read.sample.host_hidden == Some(false)
            && read.sample.visibility.suppression_owned == Some(true);
        let mut path = LastWritePath::Staged;
        let disposition = if hidden {
            VisibilityDisposition::NotCalled {
                reason: VisibilityReason::Empty,
            }
        } else if read.sample.lifetime_matched != Some(true) {
            VisibilityDisposition::NotCalled {
                reason: VisibilityReason::IdentityStale,
            }
        } else if read.sample.visibility.visible == Some(false) {
            VisibilityDisposition::NotCalled {
                reason: visibility_reason(row.visibility_authority, &read),
            }
        } else if read.sample.minimized == Some(true) || read.sample.host_hidden == Some(true) {
            VisibilityDisposition::NotCalled {
                reason: VisibilityReason::ShowBlocked,
            }
        } else {
            // The target is still visible: only a proven live foreground
            // authorizes hiding, or the target's own fallback authority
            // when the retained focus request cannot prove. Only a focus
            // request that froze an arrival (`after_shown`) and was issued
            // for this target revision gates the hide; an unrelated or
            // later request never blocks a departure.
            let request = reply.focus_request.as_ref().filter(|request| {
                request.after_shown.is_some() && request.issued_revision <= target.revision
            });
            let decision = match request {
                None => Some(true),
                Some(request) => focus_records
                    .get(&request.id)
                    .filter(|record| {
                        record.target == request.target
                            && record.revision == request.issued_revision
                    })
                    .and_then(|record| match record.disposition {
                        FocusDisposition::Called {
                            setter: FrameSetter::Accepted,
                            ..
                        } => {
                            if ops.is_foreground(&request.target.0) {
                                Some(true)
                            } else if record.foreground_proven {
                                // Proven at dispatch but the live foreground
                                // is elsewhere now: a real not-called hold.
                                Some(false)
                            } else {
                                // Accepted but not landed yet (async focus):
                                // pending, never a NotCalled latch.
                                None
                            }
                        }
                        // NotCalled, refused, or threw: the focus verdict is
                        // terminal for this tuple.
                        _ => Some(false),
                    }),
            };
            match decision {
                None => {
                    // The focus decision is still pending: hold without a
                    // row so a later pass can hide under real proof.
                    stats.held += 1;
                    continue;
                }
                Some(true) => VisibilityDisposition::Called {
                    setter: FrameSetter::Accepted,
                    reason: VisibilityReason::Empty,
                },
                Some(false) if target.fallback => {
                    path = LastWritePath::Fallback;
                    VisibilityDisposition::Called {
                        setter: FrameSetter::Accepted,
                        reason: VisibilityReason::Empty,
                    }
                }
                Some(false) => VisibilityDisposition::NotCalled {
                    reason: VisibilityReason::FocusUnproven,
                },
            }
        };
        let called = matches!(disposition, VisibilityDisposition::Called { .. });
        let disposition = if called {
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            let outcome = ops.hide_window(token);
            if outcome == Ok(true) {
                stats.staged_hides += 1;
                stats.departures_hidden_mono_ms = Some(super::trace::log_mono_ms());
            }
            VisibilityDisposition::Called {
                setter: frame_setter_for(outcome),
                reason: VisibilityReason::Empty,
            }
        } else {
            disposition
        };
        let post = if called {
            read_visibility(ops, fresh, token, gate.current_obs)
        } else {
            Some(read)
        };
        let attempt = VisibilityAttempt {
            window: row.lifetime.clone(),
            target: false,
            revision: target.revision,
            path,
            disposition,
            pre: Some(read.sample),
            post: post.map(|read| read.sample),
        };
        records.insert(token.to_owned(), attempt.clone());
        attempts.insert(token.to_owned(), attempt);
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
}

/// In-flight landing proof for one remembered focus target: the live
/// window still names the exact full native identity (HWND plus PID plus
/// process creation, never HWND alone) with the same lifetime tag the
/// accepted setter stored. A recycled HWND, a changed tag, or a same-PID
/// new window never qualifies.
fn inflight_arrived(ops: &mut dyn NativeOps, fresh: &FreshHost, flight: &InFlightFocus) -> bool {
    if fresh
        .stored_tags
        .get(&flight.token)
        .is_none_or(|tag| tag != &flight.tag)
    {
        return false;
    }
    let Some(live) = ops.read_window(&flight.token) else {
        return false;
    };
    live.key == flight.key && lifetime_ok(&live, &fresh.stored_tags)
}

fn overlay_guarded(window: &FreshWindow) -> bool {
    window
        .facts
        .is_some_and(|facts| facts.captionless_fullscreen || facts.maximized)
        || window.sticky
        || window.floating
}

/// Full fresh safety/identity gate immediately before a setter, over the
/// LIVE re-read (never the observation preimage alone):
/// - current watermark; any gesture suspends frame writes;
/// - the live lifetime still names the same key with a matching stored tag
///   (a recycled HWND authorizes nothing): fencing failures stop;
/// - no fresh hidden/iconic/unreadable state and no fresh facts refusal: the
///   full [`classify`](crate::tiling::classify) runs on the live read, so
///   cloaked, elevated, shell, tool, owned, dialog, no-activate, hidden, and
///   minimized facts all fail closed, while fullscreen/maximized overlays
///   ride their reserved lane: row-local safety drift holds;
/// - [`SetterKind::Frame`] additionally requires a non-guarded live window
///   (no overlay, sticky, float, or fit-excluded frame writes);
///   [`SetterKind::Float`] requires strict eligibility (no overlay target,
///   no fit-excluded writes; sticky rides only under an explicit retained
///   `frame_operation` token, which is the only write authority this lane
///   checks); [`SetterKind::Membership`] allows overlays to move and focus.
fn pre_setter_ok(
    gate: &DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
    kind: SetterKind,
) -> Gate {
    if gate.current_obs != entry_obs {
        return Gate::Stop;
    }
    if matches!(kind, SetterKind::Frame | SetterKind::Float) && ops.gesture_barrier() {
        return Gate::Stop;
    }
    let Some(live) = ops.read_window(&base.token) else {
        return Gate::Hold;
    };
    if live.key != base.key {
        return Gate::Stop;
    }
    if !lifetime_ok(&live, stored) {
        return Gate::Stop;
    }
    if live.hidden {
        return Gate::Hold;
    }
    match kind {
        SetterKind::Membership => {
            // Membership-only moves never write a frame, so an iconic
            // (minimized, unobservable) subject proves through its lifetime
            // alone: `facts` is unreadable there by definition. A readable
            // row must still be rideable; an unreadable row only passes
            // when the adapter classified it iconic (nothing else proves).
            if let Some(facts) = live.facts {
                if !capture_facts_rideable(&facts) {
                    return Gate::Hold;
                }
            } else if !live.iconic {
                return Gate::Hold;
            }
        }
        SetterKind::Frame | SetterKind::Float => {
            // Frame/float writes require the full readable, non-iconic,
            // framed state and rideable facts.
            if live.iconic || live.frame.is_none() {
                return Gate::Hold;
            }
            let Some(facts) = live.facts else {
                return Gate::Hold;
            };
            if matches!(kind, SetterKind::Frame) {
                if !capture_facts_rideable(&facts) {
                    return Gate::Hold;
                }
                if overlay_guarded(&live) {
                    return Gate::Hold;
                }
            } else {
                // Strict eligibility: the full classify gate (cloaked,
                // elevated, shell, tool, owned, dialog, no-activate, hidden,
                // minimized, plus fullscreen/maximized overlay protection).
                // Ordinary floating intent alone is allowed, and sticky rides
                // too: this lane only ever writes under an explicit retained
                // `frame_operation` token (directional half-snaps), so ordinary
                // unsolicited sticky frame writes stay prohibited by the token
                // gate plus the tiled lane.
                if crate::tiling::classify(&facts).is_err() {
                    return Gate::Hold;
                }
            }
        }
    }
    Gate::Pass
}

/// Membership/view proof over a fresh re-read: the live lifetime still
/// matches (key plus stored tag) AND the live homing names the expected
/// target. The setter boolean is never proof.
fn homing_proven(
    ops: &mut dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
    output: &str,
    workspace: &str,
) -> bool {
    let Some(live) = ops.read_window(&base.token) else {
        return false;
    };
    if live.key != base.key || !lifetime_ok(&live, stored) {
        return false;
    }
    ops.read_homing(&base.token)
        .is_some_and(|(out, ws)| out == output && ws == workspace)
}

/// Read one homing as the carried logical reference (`None` = unreadable).
fn homing_pair(homing: Option<(String, String)>) -> Option<WorkspaceRef> {
    homing.map(|(out, ws)| WorkspaceRef::Logical {
        output: OutputId(out),
        workspace: tiler_core::directional::WorkspaceId(ws),
    })
}

/// Read one observed output view as the carried logical reference (`None`
/// = unreadable). Windows is native-free: every view ref is logical.
fn view_ref(out: &str, view: Option<String>) -> Option<WorkspaceRef> {
    view.map(|ws| WorkspaceRef::Logical {
        output: OutputId(out.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws),
    })
}

/// Migration-member arrival proof shared by every gate in the migration
/// section: the retained homing proof first (covers post-view arrivals
/// and logical seams), else the view-independent physical proof (covers
/// pre-view redirect arrivals whose target view flips only after every
/// member proves). The setter boolean is never proof in either lane.
fn migration_member_proven(
    ops: &mut dyn NativeOps,
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    id: &str,
    out: &str,
    ws: &str,
) -> bool {
    let base = &fresh.baseline[id];
    if homing_proven(ops, base, &fresh.stored_tags, out, ws) {
        return true;
    }
    let (frame, floating, baseline) = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == id)
        .map(|row| {
            let floating = row.floating || base.floating;
            let frame = if floating { None } else { row.effective_frame };
            (frame, floating, base.frame)
        })
        .unwrap_or((None, false, base.frame));
    ops.migration_arrival_proven(id, out, ws, frame, floating, baseline)
}

fn window_transfer_for<'a>(reply: &'a WorldReplyV4, window: &str) -> Option<&'a TransferTargetV4> {
    reply.transfers.iter().find(|t| {
        matches!(t.kind, TransferKind::Window(_)) && {
            match &t.kind {
                TransferKind::Window(id) => id.0 == window,
                _ => false,
            }
        }
    })
}

/// Pending whole-workspace migration arrival for one row: a retained
/// migration flight targets the row's home while proven homing still
/// names elsewhere. Mirrors the window-transfer deferral so pre-arrival
/// member slots write only through the migration membership phase, never
/// as bare sweep geometry.
fn migration_arrival_pending(
    reply: &WorldReplyV4,
    ops: &mut dyn NativeOps,
    token: &str,
    output: &str,
    workspace: &str,
) -> bool {
    reply.transfers.iter().any(|t| {
        matches!(t.kind, TransferKind::Migration)
            && t.target_output.0 == output
            && t.target_workspace == workspace
            && ops
                .read_homing(token)
                .is_some_and(|(out, ws)| out != t.target_output.0 || ws != t.target_workspace)
    })
}

/// Follow-view outcome for one window transfer whose membership proof
/// already arrived. The core consumes the raw follow-view attempt and
/// holds the parent ambiguous on a miss: no view outcome is ever claimed
/// on a window id here, and no verdict is emitted.
enum FollowOutcome {
    Shown,
    Dispatched,
    /// Already spent or freshness lost: silent hold, no dispatch.
    Held,
}

#[allow(clippy::too_many_arguments)]
fn actuate_follow_view(
    reply: &WorldReplyV4,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    transfer: &TransferTargetV4,
    effects: &mut DispatchFences,
    fresh: &FreshHost,
    view_out: &mut Vec<ViewAttempt>,
    attempt_ms: u64,
) -> FollowOutcome {
    let out = transfer.target_output.0.clone();
    let ws = transfer.target_workspace.clone();
    let parent = transfer.operation_id.clone();
    if ops.read_view(&out).as_deref() == Some(ws.as_str()) {
        return FollowOutcome::Shown;
    }
    // Late membership arrival never rearms a spent follow-view phase:
    // exact target proof above reports nothing, otherwise an attempted
    // phase holds silently without another switch.
    if effects.is_view_spent(&parent) {
        stats.held += 1;
        return FollowOutcome::Held;
    }
    if !ensure_fresh(reply, gate, entry_obs, ops) {
        stats.stopped_mid_apply = true;
        return FollowOutcome::Held;
    }
    // Arm the follow-view phase immediately before dispatch so a
    // possibly-executed switch never repeats.
    if !effects.mark_view_spent(&parent) {
        stats.held += 1;
        return FollowOutcome::Held;
    }
    let pre = ops.read_view(&out);
    let baseline = fresh.views.get(&out).cloned();
    let result = ops.switch_view(&out, &ws);
    let proof = ops.read_view(&out);
    // Mechanical facts only: the setter boolean is never proof, the
    // pre/post/baseline views are the refusal/third-location preimage,
    // and no outcome is claimed on the window id. The core holds the
    // parent ambiguous on a miss (never a refused window).
    view_out.push(ViewAttempt {
        operation_id: parent,
        target_output: transfer.target_output.clone(),
        target_workspace: ws.clone(),
        disposition: frame_setter(result.as_ref().ok().copied()),
        pre: view_ref(&out, pre),
        post: view_ref(&out, proof.clone()),
        baseline: view_ref(&out, baseline),
        attempt_ms,
    });
    if result.is_ok() && proof.as_deref() == Some(ws.as_str()) {
        stats.view_writes += 1;
        stats.departures_hidden_mono_ms = Some(super::trace::log_mono_ms());
        FollowOutcome::Dispatched
    } else {
        // Membership is proven but the follow view is not: hold without
        // claiming a view outcome on the window id.
        stats.held += 1;
        FollowOutcome::Dispatched
    }
}

fn finalize(stats: &mut ApplyStats) {
    let writes = stats.geometry_writes
        + stats.membership_writes
        + stats.view_writes
        + stats.creations
        + stats.removals
        + stats.effects
        + stats.cursor_warps;
    stats.partial = writes > 0 && (stats.held > 0 || stats.stopped_mid_apply);
}

/// Latest accepted show `ret_mono_ms` in one drained visibility-post
/// batch: the max return stamp over posted shows (`hide == false` with a
/// posted verdict). Hides alone, refused shows, and empty batches yield
/// `None`, so a hides-only pass never stamps the show phase. Clock-only
/// trace data from the existing rows; no new native query.
pub(super) fn show_boundary_ret_max(
    rows: &[crate::product_hide::sys::VisibilityPostDiag],
) -> Option<u64> {
    rows.iter()
        .filter(|row| !row.hide && row.posted)
        .map(|row| row.ret_mono_ms)
        .max()
}

/// Drain executed visibility posts (actual host API per hide/reveal post)
/// into the pass stats with an honest omitted count. The source
/// accumulator already capped storage with its own unbounded total, so
/// this adds that total once (no per-row double count) and retains only
/// up to the shared cap. Called before every `finalize` so
/// racing-invalidation exits never lose an executed post.
fn drain_visibility_posts(stats: &mut ApplyStats, ops: &mut dyn NativeOps) {
    let (rows, total) = ops.take_visibility_posts();
    let returned = rows.len();
    // All-show post boundary: the latest accepted show return stamp in
    // this drain. Monotonic max across drains, so the value is the
    // boundary after every show issued this pass. No posted show means
    // no stamp (empty arrival and hides-only stays N/A, never
    // fabricated). No new native query.
    if let Some(latest) = show_boundary_ret_max(&rows) {
        stats.show_posts_done_mono_ms = Some(
            stats
                .show_posts_done_mono_ms
                .map_or(latest, |prev| prev.max(latest)),
        );
    }
    for row in rows {
        stats.note_visibility_post(row);
    }
    // The source total already counts the returned rows (each one bumped
    // it through `note_visibility_post` above), so add only the remainder
    // past storage: no per-row double count, honest omission preserved.
    stats.visibility_total = stats
        .visibility_total
        .saturating_add(total.saturating_sub(returned));
}

/// Record one proven window-transfer arrival plus its follow view.
/// Pre-proven arrivals (no setter ran) record no write; post-setter proof
/// records one membership write. Follow actuation reports its raw attempt
/// for the core to hold the parent ambiguous on a miss. `count_write`
/// marks a confirmed native write (post-setter proof).
#[allow(clippy::too_many_arguments)]
fn settle_transfer_proven(
    reply: &WorldReplyV4,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    transfer: &TransferTargetV4,
    effects: &mut DispatchFences,
    fresh: &FreshHost,
    view_out: &mut Vec<ViewAttempt>,
    attempt_ms: u64,
    count_write: bool,
) {
    if count_write {
        stats.membership_writes += 1;
    }
    if transfer.follow {
        let _ = actuate_follow_view(
            reply, gate, entry_obs, ops, stats, transfer, effects, fresh, view_out, attempt_ms,
        );
    }
}

/// Logical no-effect receipt for a floating membership-only move, shared
/// with the receipt merge below: the live window still names the key with
/// a matching stored tag and the physical frame is preserved exactly.
/// Never reinterprets the physical output (no affinity check): the
/// logical arrival is proven by the explicit commanded application
/// (identity plus preserved frame plus visibility), not by screen
/// position.
pub(super) fn float_no_effect_proven(
    ops: &mut dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
) -> bool {
    if base.frame.is_none() {
        return false;
    }
    let Some(live) = ops.read_window(&base.token) else {
        return false;
    };
    live.key == base.key
        && lifetime_ok(&live, stored)
        && live.frame == base.frame
        && live.frame.is_some()
}

/// Map one setter disposition onto the mechanical attempt vocabulary.
/// `Some(true)` is an accepted call (never proof by itself), `Some(false)`
/// an explicit refusal, `None` a throwing/ambiguous call.
fn frame_setter(wrote: Option<bool>) -> FrameSetter {
    match wrote {
        Some(true) => FrameSetter::Accepted,
        Some(false) => FrameSetter::Refused,
        None => FrameSetter::Threw,
    }
}

/// Ordered property phase rank: the paired publish from `ToggleMaximize`
/// while fullscreen carries `Fullscreen=false` plus `Maximized=true`, so the
/// clear must land before the dependent maximize. Sticky is last because its
/// marker lane may rehome; above sits between them as an independent bit.
fn state_property_rank(property: StateProperty) -> u8 {
    match property {
        StateProperty::Fullscreen => 0,
        StateProperty::Maximized => 1,
        StateProperty::Above => 2,
        StateProperty::Sticky => 3,
        // The convention all core callers follow. Any new property must
        // decide its phase here deliberately.
    }
}

/// Reconcile every retained intent desire into durable marker storage before
/// any dependent write.
///
/// A desired row for a window outside this reply's fresh baseline owns no
/// native read: it is absorbed only when the last proven publish already
/// recorded exactly this storage (nothing is owed), and otherwise holds the
/// WHOLE reply. An unreadable current storage or a failed atomic publish
/// holds the same way: no visibility, state, frame, or membership write may
/// run while the marker preimage the core believes it changed is unproven,
/// and the next reply retries. Equal storage writes nothing. Absent rows
/// carry no authority and touch nothing.
#[allow(clippy::too_many_arguments)]
fn reconcile_intent(
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    gate: &mut DiffGate,
    entry_obs: u64,
    persisted: &mut HashMap<String, IntentStorage>,
) -> Option<()> {
    for row in &reply.intent_desired {
        let desired = IntentStorage::from_data(row.data);
        if !fresh.baseline.contains_key(&row.win.0) {
            if persisted.get(&row.win.0) == Some(&desired) {
                // A prior proven publish of exactly this storage proves the
                // absent window owes nothing: absorb with no read and no
                // write.
                continue;
            }
            return None;
        }
        let current = ops.read_intent_storage(&row.win.0)?;
        if current != desired {
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                return None;
            }
            if ops.write_intent_storage(&row.win.0, row.data).is_err() {
                return None;
            }
            persisted.insert(row.win.0.clone(), desired);
        }
    }
    Some(())
}

/// Stage controlled native state targets in ordered property phases.
///
/// Only `StateTarget.authority == Controlled` actuates; anything else is the
/// core's explicit hold and reports nothing. Hidden/iconic rows are skipped
/// unless phase 0 revealed them this pass, and unreadable pre-reads hold with
/// no attempt and no native call, so a possibly-executed write is never
/// invented and an unreadable prerequisite is never papered over.
///
/// The durable `(value, command_seq)` record is checked BEFORE the
/// automatic-clear latch. A redelivery preserves the original evidence and
/// stamp (revision, millis, pre/post, disposition) and never touches native:
/// the core correlates the attempt against the revision it originally acted
/// on, so suppressing a lost-ack redelivery would strand the target. Only
/// then does the one-shot automatic-clear latch apply to requests that are
/// not a redelivery.
#[allow(clippy::too_many_arguments)]
fn stage_state_targets(
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    gate: &mut DiffGate,
    entry_obs: u64,
    revealed: &HashSet<String>,
    state_writes: &mut BTreeMap<(String, StateProperty), StateWriteRecord>,
    auto_clear_spent: &mut BTreeSet<(String, StateProperty, bool)>,
    pending_state: &mut BTreeMap<(String, StateProperty), StateAttempt>,
    attempt_ms: u64,
    stats: &mut ApplyStats,
) {
    for row in &reply.windows {
        if row.state.is_empty() {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            return;
        }
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            continue;
        }
        let Some(base) = fresh.baseline.get(&row.lifetime.0) else {
            continue;
        };
        // A window shown by the visibility phase this pass may proceed from
        // its stale observation preimage; the show lane already proved the
        // live visibility transition.
        if !revealed.contains(&row.lifetime.0) && (base.hidden || base.iconic) {
            continue;
        }
        let mut targets = row.state.clone();
        targets.sort_by_key(|target| state_property_rank(target.property));
        for target in targets {
            if target.authority != ScopeAuthority::Controlled {
                continue;
            }
            let key = (row.lifetime.0.clone(), target.property);
            if let Some(record) = state_writes.get(&key)
                && record.value == target.value
                && record.command_seq == target.command_seq
            {
                // Acknowledgement lost: redeliver the original attempt
                // unchanged (revision, stamps, pre/post, disposition) and
                // never touch native again. This precedes the one-shot
                // automatic-clear latch so a redelivered automatic clear is
                // still reported and the core can correlate the revision it
                // originally acted on.
                pending_state.insert(key, record.attempt.clone());
                continue;
            }
            if target.automatic_clear
                && auto_clear_spent.contains(&(key.0.clone(), key.1, target.value))
            {
                // One-shot: this exact automatic clear already ran once and
                // the record above proves this is not a redelivery of it.
                continue;
            }
            let Some(pre) = ops.read_state(&row.lifetime.0, target.property) else {
                stats.held += 1;
                continue;
            };
            if pre == target.value {
                // Already at the target: no native call, no attempt row.
                // The reply's own observation proves the value.
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                return;
            }
            let disposition = match ops.write_state(&row.lifetime.0, target.property, target.value)
            {
                StateWrite::NotAttempted => {
                    stats.held += 1;
                    continue;
                }
                StateWrite::Called(accepted) => frame_setter(Some(accepted)),
                StateWrite::Threw => frame_setter(None),
            };
            let attempt = StateAttempt {
                window: row.lifetime.clone(),
                property: target.property,
                target: target.value,
                // The acted-on reply's revision, never a target-local one:
                // the core correlates against [binding revision, latest].
                revision: reply.state_revision,
                disposition,
                pre: Some(pre),
                post: ops.read_state(&row.lifetime.0, target.property),
                attempt_ms,
            };
            state_writes.insert(
                key.clone(),
                StateWriteRecord {
                    value: target.value,
                    command_seq: target.command_seq,
                    attempt: attempt.clone(),
                },
            );
            if target.automatic_clear {
                auto_clear_spent.insert((key.0.clone(), key.1, target.value));
            } else {
                // An honest explicit write re-arms the one-shot automatic
                // clear for this property.
                auto_clear_spent
                    .retain(|(token, property, _)| token != &key.0 || *property != key.1);
            }
            pending_state.insert(key, attempt);
        }
    }
}

/// Diff one ready reply against fresh proof and actuate through `ops`.
///
/// The caller owns generation/observation advancement: `state.gate.current_obs`
/// is the CURRENT watermark at entry. Racing input is consumed through
/// [`NativeOps::poll_external_invalidation`] before every setter; the core
/// consumes no verdicts from this pass (float placements never did: their
/// verdicts live in the core). Every attempted frame write appends one
/// mechanical [`FrameAttempt`] to `frame_out`, every membership write one
/// [`MemberAttempt`] to `member_out`, every view write one
/// [`ViewAttempt`] to `view_out` (each stamped with `attempt_ms`, the tick's
/// monotonic clock, and one freshly minted `attempt_id`); the core owns all
/// completion and the next desire is obeyed as-is. v4 carries native state
/// targets on the desire rows, but this facade has no state-attempt sink: a
/// state write is not actuated here (see the module report for the gap).
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(super) fn apply_world_reply(
    state: &mut HostDiff,
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    frame_out: &mut Vec<FrameAttempt>,
    member_out: &mut Vec<MemberAttempt>,
    view_out: &mut Vec<ViewAttempt>,
    attempt_ms: u64,
) -> ApplyStats {
    apply_world_reply_inner(
        state, reply, fresh, ops, frame_out, member_out, view_out, attempt_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_world_reply_inner(
    state: &mut HostDiff,
    reply: &WorldReplyV4,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    frame_out: &mut Vec<FrameAttempt>,
    member_out: &mut Vec<MemberAttempt>,
    view_out: &mut Vec<ViewAttempt>,
    attempt_ms: u64,
) -> ApplyStats {
    let mut stats = ApplyStats::default();
    let selection_focus_authority = std::mem::take(&mut state.selection_focus_authority);
    let entry_obs = state.gate.current_obs;

    if !state.gate.fresh_reply(reply) {
        stats.held += 1;
        stats.stopped_mid_apply = true;
        return stats;
    }
    state.prune(reply, fresh);
    // One monotonic sample for this apply entry (same epoch as the log
    // stamp): joins setter returns below to the clearing rows recorded
    // here. A sampling instant only, never a wait or a gate.
    let entry_mono_ms = super::trace::log_mono_ms();
    // Stashed reset-generation clearings recorded where no stats were in
    // scope ride this pass's stats with their honest total.
    for row in state.inflight_stash.drain(..) {
        stats.note_inflight(row);
    }
    stats.inflight_total = stats
        .inflight_total
        .saturating_add(std::mem::take(&mut state.inflight_stash_omitted) as usize);
    // Lifetime end for the remembered in-flight target: a departed or
    // recycled lifetime (missing baseline, drifted key, or rotated stored
    // tag) clears before any bracket can excuse it.
    if let Some(flight) = state.in_flight.as_ref() {
        let dead = fresh
            .baseline
            .get(&flight.token)
            .is_none_or(|base| base.key != flight.key)
            || fresh
                .stored_tags
                .get(&flight.token)
                .is_none_or(|tag| tag != &flight.tag);
        if dead && let Some(flight) = state.in_flight.take() {
            let resolve_mono_ms = entry_mono_ms;
            let gui = gui_resolution_label(ops, flight.key.hwnd);
            let (hwnd, pid, creation) =
                (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
            stats.note_inflight(InFlightEventDiag {
                token: flight.token,
                effect: flight.effect,
                cause: "lifetime-end",
                edge_event_ms: None,
                edge_queued_mono_ms: None,
                edge_drain_mono_ms: None,
                resolve_mono_ms,
                gui,
                hwnd: Some(hwnd),
                pid: Some(pid),
                creation: Some(creation),
            });
        }
    }
    // Event-driven in-flight resolution: every foreground edge drained
    // since the last pass resolves now, in delivery order, against the
    // in-flight remembered by the previous setter. An exact full-identity
    // (HWND plus PID plus creation) plus lifetime match is the product's
    // own landing arriving on its own (cleared, even with no switch
    // running); any other readable foreground is a supersession (cleared,
    // including a transient foreign competitor the target later returns
    // past); an unreadable edge keeps the in-flight for the bracket to
    // fail closed on. Delivery-time snapshots never decide: only these
    // live reads plus the stored lifetime qualify, so a recycled HWND, a
    // changed tag, or a same-PID new window never matches. Edge/drain
    // timestamps ride the clearing rows for the deferred-vs-delayed join
    // but never influence the match.
    let pending_meta = std::mem::take(&mut state.pending_foreground_meta);
    stats.foreground_meta_omitted = std::mem::take(&mut state.pending_foreground_omitted);
    for (index, hwnd) in std::mem::take(&mut state.pending_foreground)
        .into_iter()
        .enumerate()
    {
        // Prefix-aligned sidecar only: a missing entry (gap, cap, or HWND
        // mismatch) reads as unknown timestamps, never a later edge's
        // stamp joined by bare index.
        let meta = pending_meta.get(index).filter(|m| m.hwnd == hwnd);
        let resolved = ops.foreground_identity_of(hwnd);
        match (&resolved, state.in_flight.as_ref()) {
            (ForegroundState::Known(live), Some(flight))
                if live.hwnd == flight.key.hwnd
                    && live.pid == flight.key.pid
                    && live.creation == flight.key.creation
                    && inflight_arrived(ops, fresh, flight) =>
            {
                if let Some(flight) = state.in_flight.take() {
                    let gui = gui_resolution_label(ops, flight.key.hwnd);
                    let (hwnd, pid, creation) =
                        (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                    stats.note_inflight(InFlightEventDiag {
                        token: flight.token,
                        effect: flight.effect,
                        cause: "landed-own",
                        edge_event_ms: meta.map(|m| m.event_ms),
                        edge_queued_mono_ms: meta.map(|m| m.queued_mono_ms),
                        edge_drain_mono_ms: meta.map(|m| m.drain_mono_ms),
                        resolve_mono_ms: entry_mono_ms,
                        gui,
                        hwnd: Some(hwnd),
                        pid: Some(pid),
                        creation: Some(creation),
                    });
                }
            }
            (ForegroundState::Known(_), Some(_)) => {
                if let Some(flight) = state.in_flight.take() {
                    let gui = gui_resolution_label(ops, flight.key.hwnd);
                    let (hwnd, pid, creation) =
                        (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                    stats.note_inflight(InFlightEventDiag {
                        token: flight.token,
                        effect: flight.effect,
                        cause: "superseded-foreign",
                        edge_event_ms: meta.map(|m| m.event_ms),
                        edge_queued_mono_ms: meta.map(|m| m.queued_mono_ms),
                        edge_drain_mono_ms: meta.map(|m| m.drain_mono_ms),
                        resolve_mono_ms: entry_mono_ms,
                        gui,
                        hwnd: Some(hwnd),
                        pid: Some(pid),
                        creation: Some(creation),
                    });
                }
            }
            _ => {}
        }
    }

    // Logical workspace ids need no native binding: the core owns them
    // outright, so membership, view, and migration targets never wait for
    // a binding that will never arrive.

    let HostDiff {
        gate,
        effects,
        in_flight,
        pending_foreground,
        pending_foreground_meta,
        pending_foreground_omitted: _,
        inflight_stash: _,
        inflight_stash_omitted: _,
        pending_visibility,
        visibility_writes,
        focus_records,
        pending_focus,
        selection_focus_authority: _,
        state_writes,
        auto_clear_spent,
        pending_state,
        intent_persisted,
    } = state;

    // 0a. Intent reconcile precedes every dependent write: markers are the
    // preimage the core's own intent records and sticky/float lanes ride.
    // Any unreadable/failed publish holds the whole reply and retries.
    if reconcile_intent(
        reply,
        fresh,
        ops,
        gate,
        entry_obs,
        intent_persisted,
    )
    .is_none()
    {
        stats.held += 1;
        stats.stopped_mid_apply = true;
        finalize(&mut stats);
        return stats;
    }

    // Entry foreground for the NA remembered-focus guard.
    let mut fg_baseline = ops.foreground_state();
    // Outputs whose pure view took the nonactivating lane; only effects
    // homed there take the identity guard.
    let mut na_views: HashSet<(String, String)> = HashSet::new();
    // Shown targets revealed earlier in this pass: the frame row gate is
    // a pre-pass baseline, so these may now proceed while `base.hidden`
    // still names the observation preimage.
    let mut revealed_this_pass: HashSet<String> = HashSet::new();

    // 0. Visibility shows before frames: a hidden target cannot carry a
    // writable frame until it is actually shown, and the focus lane needs
    // the truthful show evidence before it may dispatch.
    stage_visibility_shows(
        reply,
        fresh,
        ops,
        gate,
        entry_obs,
        pending_visibility,
        visibility_writes,
        &mut revealed_this_pass,
        &mut stats,
    );
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 0b. Independent state targets in ordered property phases (the paired
    // fullscreen clear precedes its dependent maximize). Each property
    // proves its own authority, preimage, and freshness.
    stage_state_targets(
        reply,
        fresh,
        ops,
        gate,
        entry_obs,
        &revealed_this_pass,
        state_writes,
        auto_clear_spent,
        pending_state,
        attempt_ms,
        &mut stats,
    );
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 1. Window rows: per-property authority against FRESH proof.
    for row in &reply.windows {
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            stats.held += 1;
            continue;
        }
        let Some(base) = fresh.baseline.get(&row.lifetime.0) else {
            stats.held += 1;
            continue;
        };
        // Safety facts with no authority lane hold narrowly: hidden,
        // iconic, unreadable, and game/excluded facts never take automatic
        // frame writes. Overlays keep membership but take no tiled frame
        // writes. A window revealed by the show lane this pass may proceed
        // from its stale observation preimage; the live pre-setter gate
        // below still demands current visibility. The membership lane below
        // still runs under these facts: an iconic subject carries no frame,
        // so its transfer settles through membership alone.
        let revealed = revealed_this_pass.contains(&row.lifetime.0);
        if !revealed
            && base
                .facts
                .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            stats.held += 1;
            continue;
        }
        let frame_controlled = row.frame_authority == ScopeAuthority::Controlled;
        let member_controlled = row.membership_authority == ScopeAuthority::Controlled;
        let transfer = window_transfer_for(reply, &row.lifetime.0);
        let frame_blocked = (base.hidden && !revealed) || base.iconic || base.frame.is_none();
        if frame_controlled && frame_blocked {
            // Nothing to write: hidden/iconic/unreadable preimages take no
            // automatic frame write (membership still runs below).
            stats.held += 1;
        } else if frame_controlled
            && row.effective_frame.is_some()
            && (transfer.is_some_and(|t| {
                ops.read_homing(&row.lifetime.0)
                    .is_some_and(|(out, ws)| out != t.target_output.0 || ws != t.target_workspace)
            }) || migration_arrival_pending(
                reply,
                ops,
                &row.lifetime.0,
                &row.output.0,
                &row.workspace,
            ))
        {
            // Deferred until membership proof: the target frame belongs to
            // the destination output, so writing it pre-move would place
            // foreign geometry on the source. Never hold-count a correctly
            // deferred frame.
        } else if frame_controlled
            && row.effective_frame.is_some()
            && !row_on_current_view(reply, &row.output.0, &row.workspace)
        {
            // The output row is missing, names no logical view, or names
            // a different view: none of these proves this workspace is
            // current, so the frame write withholds. A named different view
            // is logically hidden even if its native hide has not landed
            // yet; a departing workspace reflow must never write the hidden
            // frame, and the reveal republishes it once the view returns.
            // Never hold-count a correctly withheld frame (same as a
            // deferred move).
        } else if frame_controlled && let Some(target) = row.effective_frame {
            if overlay_guarded(base) {
                // Reserved lanes (overlay, sticky, float, fit-excluded)
                // never take automatic tiled writes.
                stats.held += 1;
            } else {
                let live = ops.read_frame(&row.lifetime.0);
                match live {
                    None => stats.held += 1,
                    Some(live_rect) if live_rect == target => {
                        // Exact: no write needed. The core clears retained
                        // frame memory on the observed match itself.
                    }
                    Some(_) => {
                        if !ensure_fresh(reply, gate, entry_obs, ops) {
                            stats.stopped_mid_apply = true;
                            break;
                        } else {
                            match pre_setter_ok(
                                gate,
                                entry_obs,
                                ops,
                                base,
                                &fresh.stored_tags,
                                SetterKind::Frame,
                            ) {
                                Gate::Hold => {
                                    stats.held += 1;
                                }
                                Gate::Stop => {
                                    stats.stopped_mid_apply = true;
                                    break;
                                }
                                Gate::Pass => {
                                    let dpi_before = ops.read_dpi(&row.lifetime.0);
                                    let wrote = ops.write_frame(&row.lifetime.0, target).ok();
                                    let proof = ops.read_frame(&row.lifetime.0);
                                    let dpi_after = ops.read_dpi(&row.lifetime.0);
                                    // Mechanical facts only: the setter
                                    // boolean is never proof, and the
                                    // readback never carries a verdict. The
                                    // core compares desired and observed.
                                    frame_out.push(FrameAttempt {
                                        window: row.lifetime.clone(),
                                        target,
                                        disposition: frame_setter(wrote),
                                        readback: proof,
                                        dpi_before,
                                        dpi_after,
                                        attempt_id: effects.next_attempt_id(),
                                        attempt_ms: Some(attempt_ms),
                                        purpose: FramePurpose::Layout,
                                    });
                                    match (wrote, proof) {
                                        (Some(true), Some(proven)) if proven == target => {
                                            stats.geometry_writes += 1;
                                        }
                                        (Some(true), _) => {
                                            stats.geometry_writes += 1;
                                            stats.held += 1;
                                        }
                                        _ => {
                                            // Explicit refusal, throw, or
                                            // unreadable proof: bounded
                                            // transient lane upstream. No
                                            // verdict: ordinary geometry
                                            // results are proven by
                                            // read-back, not by invented
                                            // operation ids.
                                            stats.held += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Core-owned frame suppression (stage 3a): the identical pair
        // holds upstream. No write, no re-proof, no verdict; the held
        // count preserves the historical suppressed-tick witness.
        if row.effective_frame.is_some()
            && matches!(
                row.frame_authority,
                ScopeAuthority::Hold(HoldReason::ConstraintClamp)
            )
        {
            stats.held += 1;
        }
        // Membership rides only under a retained core window transfer with
        // its stable operation identity. A Controlled frame authority with
        // no effective frame contradicts the publish contract (the
        // effective frame is the only write target): hold fail-closed. A
        // frame-blocked row already hold-counted above.
        if frame_controlled && !frame_blocked && row.effective_frame.is_none() {
            stats.held += 1;
        }
        if !member_controlled {
            continue;
        }
        let Some(t) = transfer else {
            continue;
        };
        let op = t.operation_id.clone();
        let target_out = t.target_output.0.clone();
        let target_ws = t.target_workspace.clone();
        // Never actuate membership into a held or untouched scope.
        if !scope_controlled(reply, &target_out, &target_ws) {
            stats.held += 1;
            continue;
        }
        if homing_proven(ops, base, &fresh.stored_tags, &target_out, &target_ws) {
            // Pre-proven arrival: no setter runs, so the core still needs a
            // consumed no-call receipt. Windows has no native workspace
            // property, so a same-output send would otherwise hold forever
            // waiting for a raw post that never comes. Held targets skip
            // it: possibly-executed evidence must not be settled by a
            // no-call row. A stale reply or racing invalidation stops the
            // pass before any receipt is emitted.
            if t.held {
                stats.held += 1;
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            if !effects.is_member_attempted(&op, &row.lifetime.0) {
                effects.mark_member_attempted(&op, &row.lifetime.0);
                let at_target = homing_pair(Some((target_out.clone(), target_ws.clone())));
                member_out.push(MemberAttempt {
                    operation_id: op.clone(),
                    window: row.lifetime.clone(),
                    target_output: t.target_output.clone(),
                    target_workspace: target_ws.clone(),
                    disposition: MemberDisposition::AlreadyAtTarget,
                    pre: at_target.clone(),
                    post: at_target,
                    attempt_ms,
                });
            }
            // Only the follow phase may still need actuation.
            settle_transfer_proven(
                reply, gate, entry_obs, ops, &mut stats, t, effects, fresh, view_out, attempt_ms,
                false,
            );
            continue;
        }
        if t.held || effects.is_member_attempted(&op, &row.lifetime.0) {
            // Held or already attempted: a possibly-executed write never
            // repeats. Source homing after a throw cannot prove
            // non-execution, so the core holds from its retained evidence
            // until completion, proven refusal, or third-location proof
            // settles. No verdict is emitted here.
            stats.held += 1;
            continue;
        }
        let Some(home) = ops.read_homing(&row.lifetime.0) else {
            stats.held += 1;
            continue;
        };
        let base_home = (base.output.clone(), base.workspace.clone());
        if base.workspace.is_empty() {
            // Unknown preimage (missing receipts): never positive
            // third-location evidence. Hold for the next proven observation
            // instead of misclassifying unknown as external; the transfer
            // stays retained and retries. No raw attempt is reported: an
            // unknown preimage never proves anything.
            stats.held += 1;
            continue;
        }
        let at_source = home == base_home;
        let partial = (home.0 == target_out && home.1 == base_home.1)
            || (home.0 == base_home.0 && home.1 == target_ws);
        if !at_source && !partial {
            // Live homing off both the source and the target: hold without
            // dispatch and report nothing. The core derives external
            // supersession from staged observation (unknown preimages
            // never count, contradictory receipts hold there too).
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Membership,
        ) {
            Gate::Hold => {
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        // Arm the membership phase immediately before dispatch so a
        // possibly-executed write never repeats.
        effects.mark_member_attempted(&op, &row.lifetime.0);
        let before = ops.read_homing(&row.lifetime.0);
        // Tiled rows carry their core-owned desired frame as the actuation
        // target (the production seam writes it as part of the move);
        // membership-only lanes (floats/sticky/overlays, frameless holds)
        // pass `None` and preserve the physical frame. An iconic/hidden/
        // unreadable preimage (`frame_blocked`) has no writable frame, so a
        // membership move never writes geometry for it. Same-output movers
        // need no translation (the view batch proves on the shared
        // screen); cross-output movers ride the published slot or the
        // sessionless migration placement.
        let frame_target = if row.floating || base.floating || frame_blocked {
            None
        } else {
            row.effective_frame
        };
        let moved = ops.move_membership(&row.lifetime.0, &target_out, &target_ws, frame_target);
        // Floating membership-only moves preserve the physical frame, so the
        // native homing read still names the source even though the logical
        // membership write landed. Prove that no-effect crossing from the
        // untouched fresh frame and report the applied target as the raw
        // post; otherwise the core holds the transfer forever (there is no
        // physical move to observe).
        let float_ok = (row.floating || base.floating)
            && float_no_effect_proven(ops, base, &fresh.stored_tags);
        // Re-read for the mechanical post proof BEFORE any early
        // invalidation break: the setter boolean is never proof, and the
        // raw attempt must survive even when racing input stopped the pass.
        // The core derives confirmation, proven refusal, and ambiguity from
        // this raw attempt plus the staged observation.
        let post = if float_ok {
            homing_pair(Some((target_out.clone(), target_ws.clone())))
        } else {
            homing_pair(ops.read_homing(&row.lifetime.0))
        };
        member_out.push(MemberAttempt {
            operation_id: op.clone(),
            window: row.lifetime.clone(),
            target_output: t.target_output.clone(),
            target_workspace: target_ws.clone(),
            disposition: MemberDisposition::Called(frame_setter(moved.as_ref().ok().copied())),
            pre: homing_pair(before),
            post,
            attempt_ms,
        });
        if gate.current_obs != entry_obs {
            stats.held += 1;
            stats.stopped_mid_apply = true;
            break;
        }
        // A throw that still landed (ambiguous call, proven target)
        // reports like any other attempt; the setter boolean alone never
        // counts a confirmed write.
        let proven = homing_proven(ops, base, &fresh.stored_tags, &target_out, &target_ws);
        if moved.is_ok() && (proven || (moved == Ok(true) && float_ok)) {
            settle_transfer_proven(
                reply, gate, entry_obs, ops, &mut stats, t, effects, fresh, view_out, attempt_ms,
                true,
            );
        } else {
            // Explicit refusal, throw, partial write, or unreadable proof:
            // the reported raw attempt holds the core ambiguous without
            // re-actuation. No verdict is emitted here.
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 1b. Explicit float placements: the stable `frame_operation` token is
    // necessary but never sufficient. The row carries no frame authority
    // (core publishes `Untouched`), so the token plus the full fresh gates
    // below are the only write authority. Ordinary floats never ride here.
    // This lane reports mechanical write facts only (setter disposition,
    // immediate pre-write and post-write frames, DPI pair) with exact
    // operation-id plus window plus target correlation; the core derives
    // confirmation, proven refusal, and ambiguity. No outcome verdict is
    // emitted here: a setter boolean or a readback alone never settles a
    // placement, and completion comes from native observation.
    for row in &reply.windows {
        let op = row.frame_operation.as_deref().unwrap_or("");
        if op.is_empty() {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            stats.held += 1;
            continue;
        }
        let Some(target) = row.effective_frame else {
            // No placement target: hold without consuming the placement.
            stats.held += 1;
            continue;
        };
        let Some(base) = fresh.baseline.get(&row.lifetime.0) else {
            // No provable lifetime: hold without consuming the placement.
            stats.held += 1;
            continue;
        };
        if base.hidden || base.iconic || base.frame.is_none() {
            stats.held += 1;
            continue;
        }
        if base
            .facts
            .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            stats.held += 1;
            continue;
        }
        if ops.gesture_barrier() {
            stats.held += 1;
            continue;
        }
        // Strictly at-most-once dispatch, never a policy verdict: the
        // spent mark alone blocks repeats, and the core owns the verdict
        // over the reported raw attempt. Holds below never consume the
        // placement; only a dispatch arms the phase.
        let live = ops.read_frame(&row.lifetime.0);
        match live {
            Some(rect) if rect == target => {
                // Already at the target: no write, no attempt, no verdict.
                // The core confirms from the staged observation itself.
                continue;
            }
            None => {
                stats.held += 1;
                continue;
            }
            _ => {}
        }
        // Manual-change race: the live frame left the observation baseline.
        // Never fight the user; hold for the next stable observation.
        let live_rect = live.expect("matched above");
        if base.frame.is_some_and(|baseline| baseline != live_rect) {
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Float,
        ) {
            Gate::Hold => {
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        // Arm the float phase immediately pre-dispatch, after every
        // safety/authority gate passed, so holds never consume placement.
        // Strictly at-most-once dispatch, never a policy verdict.
        if !effects.mark_float_spent(op) {
            stats.held += 1;
            continue;
        }
        let dpi_before = ops.read_dpi(&row.lifetime.0);
        let prewrite = ops.read_frame(&row.lifetime.0);
        let wrote = ops.write_frame(&row.lifetime.0, target).ok();
        let proof = ops.read_frame(&row.lifetime.0);
        let dpi_after = ops.read_dpi(&row.lifetime.0);
        // Mechanical facts only: the setter boolean is never proof, the
        // pre-write frame is the refusal preimage, and the readback never
        // carries a verdict. The core correlates by operation id plus
        // window plus target and settles from this evidence plus the next
        // staged observation.
        frame_out.push(FrameAttempt {
            window: row.lifetime.clone(),
            target,
            disposition: frame_setter(wrote),
            readback: proof,
            dpi_before,
            dpi_after,
            attempt_id: effects.next_attempt_id(),
            attempt_ms: Some(attempt_ms),
            purpose: FramePurpose::FloatPlacement {
                operation_id: op.to_owned(),
                prewrite,
            },
        });
        if wrote == Some(true) {
            stats.geometry_writes += 1;
        }
        if proof != Some(target) {
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 1c. (removed) Native state targets ride the v4 desire rows; this
    // facade has no state-attempt sink, so no clear is dispatched here.

    // Paired migration source-refill views defer to the migration path
    // below: a refill is the View whose `parent` names this migration, and
    // switching the source view before member arrival proof would hide
    // unmoved members. A View whose parent names no Migration transfer is
    // not deferred and dispatches in the normal view pass.
    let migration_ops: HashSet<&str> = reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.operation_id.as_str())
        .collect();
    let mut deferred_refills: HashSet<&str> = HashSet::new();
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::View) {
            continue;
        }
        if t.parent
            .as_deref()
            .is_some_and(|parent| migration_ops.contains(parent))
        {
            deferred_refills.insert(t.operation_id.as_str());
        }
    }

    // 3. Output views: retained transfer targets only, proven on the exact
    // target output. Unresolvable targets hold with no attempt recorded, so
    // the retained core target retries on a later report. Floating
    // (Untouched) targets actuate their hide/reveal under the
    // view-actuatable gate below; every other authority rule is unchanged.
    // Every dispatch reports its raw attempt; the core derives
    // confirmation, proven refusal, ambiguity, and external supersession
    // from the raw facts plus staged echoes. No verdict is emitted here.
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::View) {
            continue;
        }
        if deferred_refills.contains(t.operation_id.as_str()) {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let out = t.target_output.0.clone();
        let ws = t.target_workspace.clone();
        let op = t.operation_id.clone();
        // Selection-owned arrival views are not combined switches: the
        // per-window visibility lane makes the real show/hide calls and the
        // focus lane proves foreground before departing hides. Running a
        // combined switch here would hide departures before the frozen
        // focus has any chance to prove.
        if reply
            .focus_request
            .as_ref()
            .and_then(|request| request.after_shown.as_ref())
            .is_some_and(|after| after.op == op)
            && exact_workspace_controlled(reply, &out, &ws)
            // A memberless target has no per-window reveal lane behind this
            // skip; only the combined switch can move the view at all.
            && reply.windows.iter().any(|row| {
                row.output.0 == out
                    && row.workspace == ws
                    && row.visibility.is_some_and(|visibility| visibility.shown)
            })
        {
            continue;
        }
        if !scope_actuatable(reply, ops, &out, &ws) {
            stats.held += 1;
            continue;
        }
        if ops.read_view(&out).as_deref() == Some(ws.as_str()) {
            // Already shown: no write, no attempt. The core settles from
            // the staged echo itself; a redundant no-op switch would only
            // fabricate a setter row.
            continue;
        }
        if t.held || effects.is_view_spent(&op) {
            // Held or already attempted: a failed switch cannot prove
            // non-execution, so retrying could fight a user who switched
            // away after an executed attempt. Late target proof above
            // reports nothing; this fence is never rearmed by other
            // proofs. The core derives any third-location supersession
            // from staged echoes plus retained raw evidence.
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        // The nonactivating lane only for a pure view matching the retained
        // focus request's proven live homing. Everything else stays pumped.
        let na = reply.focus_request.as_ref().is_some_and(|request| {
            ops.read_homing(&request.target.0)
                .is_some_and(|(o, w)| o == out && w == ws)
        });
        // Arm the view phase immediately before dispatch; late proofs
        // cannot rearm it, so a possibly-executed switch never repeats.
        if !effects.mark_view_spent(&op) {
            stats.held += 1;
            continue;
        }
        let pre = ops.read_view(&out);
        let baseline = fresh.views.get(&out).cloned();
        let result = if na {
            ops.switch_view_nonactivating(&out, &ws)
        } else {
            ops.switch_view(&out, &ws)
        };
        let proof = ops.read_view(&out);
        // Mechanical facts only: the raw attempt carries the observed
        // pre/baseline/post views; the setter boolean is never proof.
        view_out.push(ViewAttempt {
            operation_id: op.clone(),
            target_output: t.target_output.clone(),
            target_workspace: ws.clone(),
            disposition: frame_setter(result.as_ref().ok().copied()),
            pre: view_ref(&out, pre),
            post: view_ref(&out, proof.clone()),
            baseline: view_ref(&out, baseline),
            attempt_ms,
        });
        if result.is_ok() && proof.as_deref() == Some(ws.as_str()) {
            stats.view_writes += 1;
            stats.departures_hidden_mono_ms = Some(super::trace::log_mono_ms());
            if na {
                na_views.insert((out.clone(), ws.clone()));
            }
        } else {
            // Setter refusal, throw, or unreadable proof: the core holds
            // from its retained desire without re-actuation.
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 3b. Whole-workspace migration: membership-only moves under one stable
    // operation, then views. Floating (Untouched) targets actuate their
    // membership-coupled moves and views under the same mechanical gate
    // as standalone views: member frames ride only the core-owned
    // placement (floats carry none and preserve their frame), every
    // dispatch reports its raw attempt, and the core owns every verdict.
    // No view switches until ALL fresh member arrival proof is in: a
    // partial failure holds truthfully under the same id, and the paired
    // source refill actuates only after the target view proves. No verdict
    // is emitted here.
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::Migration) {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let op = t.operation_id.clone();
        let out = t.target_output.0.clone();
        let ws = t.target_workspace.clone();
        if !scope_actuatable(reply, ops, &out, &ws) {
            stats.held += 1;
            continue;
        }
        let members: Vec<String> = reply
            .windows
            .iter()
            .filter(|w| w.output.0 == out && w.workspace == ws)
            .map(|w| w.lifetime.0.clone())
            .collect();
        // A memberless migration carries no membership phase: the target and
        // refill views below still actuate. Only unknown (unbaselined)
        // members hold; an empty set proceeds to the view gates.
        if members.iter().any(|id| !fresh.baseline.contains_key(id)) {
            stats.held += 1;
            continue;
        }
        for id in &members {
            let base = &fresh.baseline[id];
            if migration_member_proven(ops, reply, fresh, id, &out, &ws) {
                stats.migration_total += 1;
                stats.migration_proven += 1;
                continue;
            }
            stats.migration_total += 1;
            if t.held || effects.is_member_attempted(&op, id) {
                // Held or already attempted: a possibly-executed write
                // never repeats. The core holds from its retained raw
                // evidence until arrival proves.
                stats.held += 1;
                continue;
            }
            let Some(home) = ops.read_homing(id) else {
                stats.held += 1;
                continue;
            };
            if base.workspace.is_empty() {
                // Unknown preimage (missing receipts): never positive
                // third-location evidence and never a dispatch. Hold
                // without reporting so the migration retries on the next
                // proven observation.
                stats.held += 1;
                continue;
            }
            let at_source = home.0 == base.output && home.1 == base.workspace;
            let partial_progress = (home.0 == out && home.1 == base.workspace)
                || (home.0 == base.output && home.1 == ws);
            if !at_source && !partial_progress {
                // Live homing off both the source and the target: hold
                // without dispatch and report nothing. The core derives
                // external supersession from staged observation.
                stats.held += 1;
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            match pre_setter_ok(
                gate,
                entry_obs,
                ops,
                base,
                &fresh.stored_tags,
                SetterKind::Membership,
            ) {
                Gate::Hold => {
                    // Row-local safety drift: skip this member but keep
                    // converging the rest; the all-homed check below holds
                    // the operation truthfully until every arrival proves.
                    stats.held += 1;
                    continue;
                }
                Gate::Stop => {
                    stats.stopped_mid_apply = true;
                    break;
                }
                Gate::Pass => {}
            }
            // Arm the member phase immediately before dispatch so a
            // possibly-executed write never repeats.
            effects.mark_member_attempted(&op, id);
            // Migration members ride their core-owned desired frames as
            // actuation targets exactly like window transfers (tiled
            // slots and sessionless cross-output placements); floats and
            // frameless holds carry none and preserve their frame.
            let member_frame = reply
                .windows
                .iter()
                .find(|row| row.lifetime.0 == *id)
                .map(|row| {
                    if row.floating || base.floating {
                        None
                    } else {
                        row.effective_frame
                    }
                })
                .unwrap_or(None);
            let before = ops.read_homing(id);
            let moved = ops.move_membership(id, &out, &ws, member_frame);
            // Mechanical facts only, staged before any early-invalidation
            // break so the raw attempt survives racing input: the setter
            // boolean is never proof. The core derives arrival, refusal, and
            // ambiguity from this raw attempt plus the staged observation.
            let post = ops.read_homing(id);
            member_out.push(MemberAttempt {
                operation_id: op.clone(),
                window: tiler_core::directional::WindowId(id.clone()),
                target_output: t.target_output.clone(),
                target_workspace: ws.clone(),
                disposition: MemberDisposition::Called(frame_setter(moved.as_ref().ok().copied())),
                pre: homing_pair(before),
                post: homing_pair(post),
                attempt_ms,
            });
            if gate.current_obs != entry_obs {
                stats.held += 1;
                stats.stopped_mid_apply = true;
                break;
            }
            if moved.is_ok() && migration_member_proven(ops, reply, fresh, id, &out, &ws) {
                stats.membership_writes += 1;
                stats.migration_proven += 1;
            } else {
                // Explicit refusal, throw, or unreadable proof: the
                // reported raw attempt holds the core without re-actuation.
                stats.held += 1;
            }
        }
        if stats.stopped_mid_apply {
            break;
        }
        let all_homed = members
            .iter()
            .all(|id| migration_member_proven(ops, reply, fresh, id, &out, &ws));
        if !all_homed {
            // Partial arrival or partial failure holds truthfully under
            // the same id; the core owns the verdict over the raw
            // attempts, never a replay.
            stats.held += 1;
            continue;
        }
        // Target view only after every member arrival proof. The phase is
        // owned by the migration parent: late member proof never rearms an
        // attempted target view, and exact target proof reports nothing.
        // Successful members/views are never replayed.
        if ops.read_view(&out).as_deref() != Some(ws.as_str()) {
            if effects.is_view_spent(&op) {
                // Late member proof never rearms an attempted target view:
                // silent hold without another switch.
                stats.held += 1;
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            if !effects.mark_view_spent(&op) {
                // Raced arming: another phase spent it first; silent hold.
                stats.held += 1;
                continue;
            }
            let pre = ops.read_view(&out);
            let baseline = fresh.views.get(&out).cloned();
            let result = ops.switch_view(&out, &ws);
            let proof = ops.read_view(&out);
            // Mechanical facts only; the core settles from this attempt
            // plus the next staged echo. Proof decides, never the setter
            // boolean: a throw that still landed proceeds to refill.
            view_out.push(ViewAttempt {
                operation_id: op.clone(),
                target_output: t.target_output.clone(),
                target_workspace: ws.clone(),
                disposition: frame_setter(result.as_ref().ok().copied()),
                pre: view_ref(&out, pre),
                post: view_ref(&out, proof.clone()),
                baseline: view_ref(&out, baseline),
                attempt_ms,
            });
            if proof.as_deref() == Some(ws.as_str()) {
                if result.is_ok() {
                    stats.view_writes += 1;
                    stats.departures_hidden_mono_ms = Some(super::trace::log_mono_ms());
                }
            } else {
                stats.held += 1;
                continue;
            }
        }
        // Paired source refill only after the target view proves: never
        // ahead of arrival. The refill is the View whose `parent` names
        // this migration. It owns its own view phase under the actual
        // refill id, with its own held/actuation/spent gates: a floating
        // (Untouched) refill actuates exactly like a standalone floating
        // view once the target proof is in.
        if let Some(refill) = reply.transfers.iter().find(|v| {
            matches!(v.kind, TransferKind::View) && v.parent.as_deref() == Some(op.as_str())
        }) {
            let refill_out = refill.target_output.0.clone();
            let refill_ws = refill.target_workspace.clone();
            if ops.read_view(&refill_out).as_deref() == Some(refill_ws.as_str()) {
                // Already shown: nothing to report; the core settles from
                // the staged echo itself.
            } else if refill.held
                || !scope_actuatable(reply, ops, &refill_out, &refill_ws)
                || effects.is_view_spent(&refill.operation_id)
            {
                // Held, scope-held, or already attempted refill:
                // never switch a view that must remain untouched. The
                // core derives any third-location supersession from
                // staged echoes plus retained raw evidence.
                stats.held += 1;
                continue;
            } else {
                if !ensure_fresh(reply, gate, entry_obs, ops) {
                    stats.stopped_mid_apply = true;
                    break;
                }
                let pre = ops.read_view(&refill_out);
                let baseline_refill = fresh.views.get(&refill_out).cloned();
                if !effects.mark_view_spent(&refill.operation_id) {
                    // Raced arming: silent hold without another switch.
                    stats.held += 1;
                    continue;
                }
                let result = ops.switch_view(&refill_out, &refill_ws);
                let proof = ops.read_view(&refill_out);
                // Mechanical facts only; the core settles from this
                // attempt plus the next staged echo.
                view_out.push(ViewAttempt {
                    operation_id: refill.operation_id.clone(),
                    target_output: refill.target_output.clone(),
                    target_workspace: refill_ws.clone(),
                    disposition: frame_setter(result.as_ref().ok().copied()),
                    pre: view_ref(&refill_out, pre),
                    post: view_ref(&refill_out, proof.clone()),
                    baseline: view_ref(&refill_out, baseline_refill),
                    attempt_ms,
                });
                if proof.as_deref() == Some(refill_ws.as_str()) {
                    if result.is_ok() {
                        stats.view_writes += 1;
                        stats.departures_hidden_mono_ms = Some(super::trace::log_mono_ms());
                    }
                } else {
                    stats.held += 1;
                    continue;
                }
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 4. Retained focus request: spent BEFORE dispatch, proven by read-back,
    // gated on the exact live view (switch-before-focus). A spent request
    // never re-actuates, and a retired request never steals focus later.
    // The core consumes no focus outcomes; the spent proof plus the
    // foreground read-back is the whole contract. Trace-only per-request
    // rows land in `stats.focus_dispatch` (bounded, see MAX_FOCUS_DIAG):
    // `dispatched` plus the raw setter disposition and the immediate
    // foreground proof even when false distinguish dispatched-but-unproven
    // from no setter; `hold` names the individual hold. Static-hover retry
    // behavior is unchanged: no extra dispatch, no re-actuation.
    for request in reply.focus_request.iter() {
        // Bounded trace push helper: request order, honest truncation (the
        // tick reports the omitted count from the recorded total).
        macro_rules! push_focus {
            ($dispatched:expr, $setter:expr, $foreground:expr, $hold:expr) => {{
                if stats.focus_dispatch.len() < MAX_FOCUS_DIAG {
                    stats.focus_dispatch.push(FocusDispatchDiag {
                        id: request.id.clone(),
                        target: request.target.0.clone(),
                        kind: request.cause.as_str(),
                        dispatched: $dispatched,
                        setter: $setter,
                        foreground: $foreground,
                        hold: $hold,
                        fg_baseline: None,
                        fg_live: None,
                        api: None,
                        call_mono_ms: None,
                        ret_mono_ms: None,
                        fg_proof_mono_ms: None,
                        gui_dispatch: None,
                        gui_landing: None,
                        primitive_called: false,
                        native_return: None,
                        prime_inserted: None,
                        hwnd: None,
                        pid: None,
                        creation: None,
                        refusal_gate: None,
                        refusal_reason: None,
                        target_visible: None,
                        target_cloaked: None,
                        target_hidden: None,
                        fg_identity: None,
                        fg_gate_identity: None,
                        fg_veto_kind: None,
                        fg_readable: None,
                        fg_shell: None,
                        fg_fullscreen: None,
                        fg_visible: None,
                        fg_exe: None,
                        desktop_exception: false,
                    });
                }
            }};
        }
        if gate.current_obs != entry_obs {
            push_focus!(false, "skipped", false, Some("stopped"));
            stats.stopped_mid_apply = true;
            break;
        }
        if !gate.fresh_reply(reply) {
            push_focus!(false, "skipped", false, Some("stale"));
            stats.stopped_mid_apply = true;
            break;
        }
        // Durable at-most-once for the native call: an exact
        // `(id, target, revision)` row, called or not-called, is a terminal
        // verdict for that tuple and redelivers verbatim without another
        // native decision. Only a new target or issuing revision re-decides.
        if let Some(existing) = focus_records.get(&request.id) {
            // A reported focus row is a verdict for this tuple, called or
            // not: redeliver it without a new native decision. Only a new
            // target or authorization revision may re-decide.
            if existing.target == request.target && existing.revision == request.issued_revision {
                let hold = match existing.disposition {
                    FocusDisposition::Called { .. } => "already-spent",
                    FocusDisposition::NotCalled { .. } => "already-decided",
                };
                push_focus!(false, "skipped", false, Some(hold));
                pending_focus.insert(request.id.clone(), existing.clone());
                continue;
            }
        }
        let mut arrival_brackets: Vec<ArrivalBracket> = Vec::new();
        // Frozen after_shown gate: every required member must be shown,
        // readable, identity/lifetime clean, homed and framed before this
        // request may dispatch. A member whose truthful visibility row is a
        // no-call (or a refused/threw call) is the evidence that withholds
        // focus; it becomes a correlated not-called focus row. Anything
        // still pending defers with no row so a later pass can decide.
        if let Some(after) = request.after_shown.as_ref() {
            let scan = scan_arrival(
                &after.members,
                pending_visibility,
                ops,
                fresh,
                gate.current_obs,
            );
            if let Some(reason) = scan.blocked {
                let pre = ops.foreground_managed().map(WindowId);
                let feedback = FocusFeedback {
                    id: request.id.clone(),
                    target: request.target.clone(),
                    revision: request.issued_revision,
                    disposition: FocusDisposition::NotCalled { reason },
                    pre: pre.clone(),
                    post: pre,
                    foreground_proven: false,
                    arrival: scan.brackets,
                    attempt_ms,
                };
                focus_records.insert(request.id.clone(), feedback.clone());
                pending_focus.insert(request.id.clone(), feedback);
                push_focus!(false, "skipped", false, Some("show-blocked"));
                stats.held += 1;
                continue;
            }
            if !scan.ready {
                push_focus!(false, "skipped", false, Some("arrival-pending"));
                stats.held += 1;
                continue;
            }
            arrival_brackets = scan.brackets;
        }
        let Some(base) = fresh.baseline.get(&request.target.0) else {
            push_focus!(false, "skipped", false, Some("unknown-baseline"));
            stats.held += 1;
            continue;
        };
        // Same-pass view reveal: section 3 reveals entering members
        // synchronously on a pure view switch, but `fresh.baseline` is
        // the pre-pass preimage, so a view-return target still reads
        // hidden there. Re-read live and proceed only when the target
        // now reads visible with a frame; otherwise hold without
        // marking spent. Minimized/iconic targets never clear this
        // way (reveal does not restore them).
        let live_reveal: Option<FreshWindow> = if base.hidden || base.iconic || base.frame.is_none()
        {
            ops.read_window(&request.target.0)
                .filter(|live| !live.hidden && !live.iconic && live.frame.is_some())
        } else {
            None
        };
        if (base.hidden || base.iconic || base.frame.is_none()) && live_reveal.is_none() {
            push_focus!(false, "skipped", false, Some("hidden-or-unreadable"));
            stats.held += 1;
            continue;
        }
        // The fresh reveal only lifts the stale hidden/unreadable
        // eligibility and facts check below. The pre-setter identity gate
        // keeps the original pre-pass `base`: comparing the recycled live
        // read against itself would lose key drift when `read_window`
        // first sees a recycled HWND with a matching/spoofed old tag.
        let check: &FreshWindow = live_reveal.as_ref().unwrap_or(base);
        if check
            .facts
            .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            push_focus!(false, "skipped", false, Some("ineligible-facts"));
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            push_focus!(false, "skipped", false, Some("stale"));
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Membership,
        ) {
            Gate::Hold => {
                push_focus!(false, "skipped", false, Some("pre-setter-hold"));
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                push_focus!(false, "skipped", false, Some("pre-setter-stop"));
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        // Switch-before-focus liveness: the CURRENT live homing (not the
        // observation preimage) must name a controlled live scope whose
        // exact-output view is shown. A moved lifetime focuses only where
        // it actually lives. The staged lane below relaxes only the
        // shown-view half under arrival-ready proof; the sticky lane
        // relaxes it only for a freshly proven sticky lifetime onto the
        // shown view of the SAME output under selected effective view
        // authority. `read_homing` stays truthful throughout (never
        // normalized): membership is untouched here.
        let arrival_ready_lane = request.after_shown.is_some();
        let Some((live_out, live_ws)) = ops.read_homing(&request.target.0) else {
            push_focus!(false, "skipped", false, Some("unknown-homing"));
            stats.held += 1;
            continue;
        };
        if !scope_controlled(reply, &live_out, &live_ws) {
            push_focus!(false, "skipped", false, Some("held-scope"));
            stats.held += 1;
            continue;
        }
        // One live shown-view read for this output, reused by every lane
        // below (never a second native read).
        let shown_view = ops.read_view(&live_out);
        // Narrow sticky return lane state: the shown workspace the sticky
        // focus is allowed onto. `None` on every other path.
        let mut sticky_shown: Option<String> = None;
        if shown_view.as_deref() != Some(live_ws.as_str()) && !arrival_ready_lane {
            // Sticky return lane: the core may return focus to a
            // remembered sticky window whose actual home differs from
            // the selected shown workspace. A sticky window is visible
            // on every workspace of its output, but that visibility
            // alone grants nothing. Require every freshness/safety
            // proof: same output with its shown view under Controlled
            // authority (the home scope above already is), plus a fresh
            // live read proving the sticky lane, the exact lifetime
            // key, the stored tag, visibility, and rideable facts.
            // Ordinary windows, wrong outputs, unshown views, unsafe
            // scopes, and stale/recycled identities all hold below.
            // Never a policy input: the core already selected this
            // exact target; this classifies freshness/safety only.
            let sticky_ready = shown_view.as_deref().is_some_and(|shown_ws| {
                scope_controlled(reply, &live_out, shown_ws)
                    && ops.read_window(&request.target.0).is_some_and(|live| {
                        live.sticky
                            && live.key == base.key
                            && lifetime_ok(&live, &fresh.stored_tags)
                            && !live.hidden
                            && !live.iconic
                            && live.frame.is_some()
                            && live
                                .facts
                                .is_some_and(|facts| capture_facts_rideable(&facts))
                    })
            });
            if !sticky_ready {
                // The live workspace is not shown on its output:
                // focusing now would steal focus from the wrong view.
                // Hold for the view.
                push_focus!(false, "skipped", false, Some("view-not-shown"));
                stats.held += 1;
                continue;
            }
            sticky_shown = shown_view.clone();
        }
        // NA-only foreground identity guard: a changed or unreadable
        // foreground during the nonactivating switch vetoes unless the
        // target already holds it or the change is exactly the remembered
        // in-flight target landing (the product's own prior accepted
        // activation, never a supersession). Pumped paths keep existing
        // behavior. No fallback exemption: a fallback move also vetoes,
        // and any other change vetoes with the in-flight cleared as
        // superseded. A double zero baseline (None/None) still fails
        // closed: verifiably no foreground at both reads proves nothing
        // about the switch, so only a target already holding foreground
        // or an exact in-flight landing proceeds. The guard keys on the
        // switched view: ordinary targets home where shown, while a sticky
        // return focuses from its actual home onto the shown view of the
        // same output. Routing the sticky lane on its shown view keeps a
        // competing-foreground veto from being skipped only because the
        // actual home mismatches; every other path keeps its exact home
        // key, and pumped paths keep existing behavior.
        let na_applies = na_views.contains(&(live_out.clone(), live_ws.clone()))
            || sticky_shown.as_deref().is_some_and(|shown_ws| {
                na_views.contains(&(live_out.clone(), shown_ws.to_owned()))
            });
        if na_applies {
            let fg_live = ops.foreground_state();
            let changed_or_unknown = match (&fg_baseline, &fg_live) {
                (ForegroundState::Known(a), ForegroundState::Known(b)) => a != b,
                _ => true,
            };
            if changed_or_unknown && !ops.is_foreground(&request.target.0) {
                // Event-driven own-landing excuse: the live foreground
                // names the exact full in-flight identity (HWND plus PID
                // plus creation) with its lifetime still proven, so this
                // change is the product's own prior activation arriving
                // mid-switch. Anything else (including a recycled HWND, a
                // changed tag, or a same-PID new window) is not the
                // in-flight target and still vetoes.
                let own_landing = match (&fg_live, &*in_flight) {
                    (ForegroundState::Known(live), Some(flight)) => {
                        live.hwnd == flight.key.hwnd
                            && live.pid == flight.key.pid
                            && live.creation == flight.key.creation
                            && inflight_arrived(ops, fresh, flight)
                    }
                    _ => false,
                };
                if own_landing {
                    // Own landing: no veto, no hold. The in-flight clears
                    // as landed; the trailing setter below replaces it.
                    if let Some(flight) = in_flight.take() {
                        let gui = gui_resolution_label(ops, flight.key.hwnd);
                        let (hwnd, pid, creation) =
                            (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                        stats.note_inflight(InFlightEventDiag {
                            token: flight.token,
                            effect: flight.effect,
                            cause: "consumed-na-own-landing",
                            edge_event_ms: None,
                            edge_queued_mono_ms: None,
                            edge_drain_mono_ms: None,
                            resolve_mono_ms: super::trace::log_mono_ms(),
                            gui,
                            hwnd: Some(hwnd),
                            pid: Some(pid),
                            creation: Some(creation),
                        });
                    }
                } else {
                    // Other-foreground supersession clears the in-flight;
                    // an unreadable read leaves it for the next bracket.
                    if matches!(fg_live, ForegroundState::Known(_))
                        && let Some(flight) = in_flight.take()
                    {
                        let gui = gui_resolution_label(ops, flight.key.hwnd);
                        let (hwnd, pid, creation) =
                            (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                        stats.note_inflight(InFlightEventDiag {
                            token: flight.token,
                            effect: flight.effect,
                            cause: "superseded-na-guard",
                            edge_event_ms: None,
                            edge_queued_mono_ms: None,
                            edge_drain_mono_ms: None,
                            resolve_mono_ms: super::trace::log_mono_ms(),
                            gui,
                            hwnd: Some(hwnd),
                            pid: Some(pid),
                            creation: Some(creation),
                        });
                    }
                    // Veto with causal evidence: the entry baseline versus
                    // the post-switch live read ride the trace so a later
                    // join against `member-admitted` rows identifies
                    // managed versus foreign windows, not causality.
                    // The effect stays unspent. The effect dispatched no
                    // setter, so `api` is `None`: the nonactivating view
                    // switch that ran (if any) rides the `views` attempt
                    // rows, never this row.
                    if stats.focus_dispatch.len() < MAX_FOCUS_DIAG {
                        stats.focus_dispatch.push(FocusDispatchDiag {
                            id: request.id.clone(),
                            target: request.target.0.clone(),
                            kind: request.cause.as_str(),
                            dispatched: false,
                            setter: "skipped",
                            foreground: false,
                            hold: Some("foreground-changed"),
                            fg_baseline: Some(fg_trace_label(&fg_baseline)),
                            fg_live: Some(fg_trace_label(&fg_live)),
                            api: None,
                            call_mono_ms: None,
                            ret_mono_ms: None,
                            fg_proof_mono_ms: None,
                            gui_dispatch: None,
                            gui_landing: None,
                            primitive_called: false,
                            native_return: None,
                            prime_inserted: None,
                            hwnd: Some(base.key.hwnd),
                            pid: Some(base.key.pid),
                            creation: Some(base.key.creation.clone()),
                            refusal_gate: None,
                            refusal_reason: None,
                            target_visible: None,
                            target_cloaked: None,
                            target_hidden: None,
                            fg_identity: None,
                            fg_gate_identity: None,
                            fg_veto_kind: None,
                            fg_readable: None,
                            fg_shell: None,
                            fg_fullscreen: None,
                            fg_visible: None,
                            fg_exe: None,
                            desktop_exception: false,
                        });
                    }
                    stats.held += 1;
                    continue;
                }
            }
        }
        if ops.is_foreground(&request.target.0) {
            // Target equality satisfies the request: no setter runs. Report
            // the truthful no-call verdict (zero setters) and keep the
            // frozen arrival brackets for the arrival proof. `NoReason` is
            // the non-terminal bare token: the core settles the request from
            // the next observation's equality (`already-target`), exactly as
            // it does when the target already holds the foreground at issue.
            let pre = ops.foreground_managed().map(WindowId);
            let feedback = FocusFeedback {
                id: request.id.clone(),
                target: request.target.clone(),
                revision: request.issued_revision,
                disposition: FocusDisposition::NotCalled {
                    reason: FocusNoCallReason::NoReason,
                },
                pre: pre.clone(),
                post: pre,
                foreground_proven: true,
                arrival: arrival_brackets,
                attempt_ms,
            };
            focus_records.insert(request.id.clone(), feedback.clone());
            pending_focus.insert(request.id.clone(), feedback);
            push_focus!(false, "skipped", false, Some("already-foreground"));
            continue;
        }
        if !effects.mark_focus_spent(&request.id) {
            push_focus!(false, "skipped", false, Some("already-spent"));
            continue;
        }
        if arrival_ready_lane {
            stats.staged_focus += 1;
        }
        // Foreground read-back is authoritative even if the setter throws:
        // a throwing call that still landed proves focus. The per-call
        // record rides the trace on the shared log `mono_ms` basis with
        // stamps strictly around the native primitive (never the
        // surrounding preflight), alongside the raw setter disposition,
        // the primitive-vs-dispatched split, and the immediate foreground
        // proof even when false, so a dispatched-but-unproven setter is
        // distinguishable from no setter at all. The trace-gated
        // target GUI-thread read immediately before the primitive rides
        // along as a dispatch-time observation only, never proof of
        // arrival; the landing GUI below is sampled at the read-back.
        let focus_pre = ops.foreground_managed().map(WindowId);
        let selection_provenance = selection_focus_authority;
        let (setter_outcome, call_diag) =
            ops.focus_window_traced_selected(&request.target.0, selection_provenance);
        let setter = match &setter_outcome {
            Ok(true) => "accepted",
            Ok(false) => "refused",
            Err(()) => "threw",
        };
        let foreground = ops.is_foreground(&request.target.0);
        let focus_post = ops.foreground_managed().map(WindowId);
        let feedback = FocusFeedback {
            id: request.id.clone(),
            target: request.target.clone(),
            revision: request.issued_revision,
            disposition: FocusDisposition::Called {
                setter: frame_setter_for(setter_outcome),
                reason: VisibilityReason::Empty,
            },
            pre: focus_pre,
            post: focus_post,
            foreground_proven: foreground,
            arrival: arrival_brackets,
            attempt_ms,
        };
        focus_records.insert(request.id.clone(), feedback.clone());
        pending_focus.insert(request.id.clone(), feedback);
        // Owner-clock proof stamp immediately after the existing read-back:
        // result context only, never a setter-return-as-proof claim. No
        // native reread, no authority or product logic change.
        let fg_proof_mono_ms = Some(super::trace::log_mono_ms());
        let gui_landing = if foreground {
            gui_resolution_label(ops, base.key.hwnd)
        } else {
            None
        };
        let armed_ret_mono_ms = call_diag.ret_mono_ms.unwrap_or(entry_mono_ms);
        if setter_outcome == Ok(true) {
            // A newer accepted setter supersedes: drop any drained edges
            // ahead of the new in-flight (no message pump runs mid-pass,
            // so this is empty today; the clear keeps a stale queued edge
            // from ever being processed against the new target).
            pending_foreground.clear();
            pending_foreground_meta.clear();
            // Event-driven in-flight tracking: an accepted setter that
            // does not prove immediately stays remembered by exact full
            // native identity plus lifetime until a drained foreground
            // event resolves its landing or supersession. An immediate
            // landing clears (nothing to remember); a newer accepted
            // setter replaces. No blocking, timers, timeouts, retry, or
            // replay.
            if foreground {
                if let Some(flight) = in_flight.take() {
                    let gui = gui_resolution_label(ops, flight.key.hwnd);
                    let (hwnd, pid, creation) =
                        (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                    stats.note_inflight(InFlightEventDiag {
                        token: flight.token,
                        effect: flight.effect,
                        cause: "superseded-new-setter",
                        edge_event_ms: None,
                        edge_queued_mono_ms: None,
                        edge_drain_mono_ms: None,
                        resolve_mono_ms: armed_ret_mono_ms,
                        gui,
                        hwnd: Some(hwnd),
                        pid: Some(pid),
                        creation: Some(creation),
                    });
                }
            } else if let Some(tag) = fresh.stored_tags.get(&request.target.0).cloned()
                && !tag.is_empty()
                && let Some(flight) = in_flight.replace(InFlightFocus {
                    token: request.target.0.clone(),
                    key: base.key.clone(),
                    tag,
                    effect: request.id.clone(),
                    set_mono_ms: armed_ret_mono_ms,
                })
            {
                let gui = gui_resolution_label(ops, flight.key.hwnd);
                let (hwnd, pid, creation) =
                    (flight.key.hwnd, flight.key.pid, flight.key.creation.clone());
                stats.note_inflight(InFlightEventDiag {
                    token: flight.token,
                    effect: flight.effect,
                    cause: "superseded-new-setter",
                    edge_event_ms: None,
                    edge_queued_mono_ms: None,
                    edge_drain_mono_ms: None,
                    resolve_mono_ms: armed_ret_mono_ms,
                    gui,
                    hwnd: Some(hwnd),
                    pid: Some(pid),
                    creation: Some(creation),
                });
            }
        }
        if stats.focus_dispatch.len() < MAX_FOCUS_DIAG {
            stats.focus_dispatch.push(FocusDispatchDiag {
                id: request.id.clone(),
                target: request.target.0.clone(),
                kind: request.cause.as_str(),
                dispatched: true,
                setter,
                foreground,
                hold: if foreground {
                    None
                } else {
                    Some("foreground-unproven")
                },
                fg_baseline: None,
                fg_live: None,
                api: if call_diag.primitive_called {
                    call_diag.api
                } else {
                    None
                },
                call_mono_ms: call_diag.call_mono_ms,
                ret_mono_ms: call_diag.ret_mono_ms,
                fg_proof_mono_ms,
                gui_dispatch: call_diag.gui_before.as_ref().map(gui_trace_label),
                gui_landing,
                primitive_called: call_diag.primitive_called,
                native_return: call_diag.native_return,
                prime_inserted: call_diag.prime_inserted,
                hwnd: Some(base.key.hwnd),
                pid: Some(base.key.pid),
                creation: Some(base.key.creation.clone()),
                refusal_gate: call_diag.refusal_gate,
                refusal_reason: call_diag.refusal_reason,
                target_visible: call_diag.target_visible,
                target_cloaked: call_diag.target_cloaked,
                target_hidden: call_diag.target_hidden,
                fg_identity: call_diag.fg_identity,
                fg_gate_identity: call_diag.fg_gate_identity,
                fg_veto_kind: call_diag.fg_veto_kind,
                fg_readable: call_diag.fg_readable,
                fg_shell: call_diag.fg_shell,
                fg_fullscreen: call_diag.fg_fullscreen,
                fg_visible: call_diag.fg_visible,
                fg_exe: call_diag.fg_exe,
                desktop_exception: call_diag.desktop_exception,
            });
        }
        if foreground {
            stats.effects += 1;
            fg_baseline = ops.foreground_state();
        } else {
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }

    // 4b. Departure hides after the focus decision. The per-window lane
    // re-reads the live foreground: without positive proof it records a
    // truthful not-called row (or takes the target's explicit fallback
    // path) and never hides; a non-Controlled or unreadable target emits no
    // row at all, so a transient hold never latches. The durable record
    // makes a possibly-executed hide at-most-once.
    stage_visibility_hides(
        reply,
        fresh,
        ops,
        gate,
        entry_obs,
        pending_visibility,
        visibility_writes,
        focus_records,
        &mut stats,
    );
    if stats.stopped_mid_apply {
        drain_visibility_posts(&mut stats, ops);
        finalize(&mut stats);
        return stats;
    }

    // 5. Cursor warp (C3): exact-once application of the one retained
    // request. Only `reply.warp_request` is acted on; there is no
    // point-only lane. The adapter requires the declared host capability,
    // the exact request target holding the live managed foreground, fresh
    // scope/frame/lifetime proof, a known and unchanged pointer generation
    // re-read immediately before the move, and a settable point. A failed
    // gate holds without spending so the core may re-publish the retained
    // request; a spent id never re-attempts, even after refusal or throw.
    // An already-there cursor satisfies the request with no setter call but
    // still spends the exact id (core retires only on `warpSpent`).
    if let Some(warp) = reply.warp_request.as_ref() {
        let id = warp.id.as_str();
        let (x, y) = warp.point;
        if effects.spent_warps.contains(id) {
            // Duplicate delivery/pass of an already-spent request id.
        } else if !ops.warp_capable() {
            // Capability refused: no call, no spend, request retained.
            stats.held += 1;
        } else if gate.current_obs != entry_obs
            || !gate.fresh_reply(reply)
            || !ensure_fresh(reply, gate, entry_obs, ops)
        {
            stats.stopped_mid_apply = true;
        } else {
            let settable = warp_point_settable(x, y);
            let foreground_exact =
                ops.foreground_managed().as_deref() == Some(warp.target.0.as_str());
            if !settable || !foreground_exact {
                stats.held += 1;
            } else {
                match fresh.baseline.get(&warp.target.0) {
                    None => {
                        stats.held += 1;
                    }
                    Some(base) => {
                        match pre_setter_ok(
                            gate,
                            entry_obs,
                            ops,
                            base,
                            &fresh.stored_tags,
                            SetterKind::Membership,
                        ) {
                            Gate::Hold => {
                                stats.held += 1;
                            }
                            Gate::Stop => {
                                stats.stopped_mid_apply = true;
                            }
                            Gate::Pass => {
                                // Switch-before-focus liveness on the live
                                // homing, which must still name the exact
                                // retained destination, plus the actual
                                // observed foreground: the focus must have
                                // landed, not merely been dispatched.
                                let shown = ops.read_homing(&warp.target.0).is_some_and(
                                    |(out, ws)| {
                                        out == warp.output.0
                                            && ws == warp.workspace.0
                                            && scope_controlled(reply, &out, &ws)
                                            && ops.read_view(&out).as_deref()
                                                == Some(ws.as_str())
                                    },
                                );
                                if !shown || !ops.is_foreground(&warp.target.0) {
                                    stats.held += 1;
                                } else if ops.read_cursor() == Some((x, y)) {
                                    // Already there: no setter call, but the
                                    // exact id is spent so core retires it.
                                    effects.mark_warp_spent(id);
                                } else if warp.pointer_baseline.is_none()
                                    || ops.pointer_activity() != warp.pointer_baseline
                                {
                                    // Unknown or newer raw-input pointer
                                    // activity: user motion wins, no call and
                                    // no spend so a later reply may still act.
                                    stats.held += 1;
                                } else {
                                    // Mark BEFORE the one setter attempt: a
                                    // refusal or throw still spends the id,
                                    // and a duplicate never retries.
                                    effects.mark_warp_spent(id);
                                    match ops.move_cursor(x, y) {
                                        Err(()) => {
                                            stats.stopped_mid_apply = true;
                                        }
                                        Ok(false) => {
                                            stats.held += 1;
                                        }
                                        Ok(true) => {
                                            stats.cursor_warps += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
        }
    }

    drain_visibility_posts(&mut stats, ops);
    finalize(&mut stats);
    stats
}
