#![forbid(unsafe_code)]
//! Source-first full-world observation capture (W1, before sequencing).
//!
//! Pure, offline and read-only over the real owner plus real per-tick
//! products: [`TileLoop`] tables, [`ObservedWindow`](super::observe::ObservedWindow),
//! [`RetainedRow`]/[`HiddenRecord`]/[`PendingClosed`](super::retained::RetainedRow),
//! [`MonitorArea`](super::monitors::MonitorArea) and the core logical rings.
//! No parallel
//! DTOs: fixtures instantiate these exact production structures through the
//! same [`capture_world`] constructor the runtime will use. No Engine
//! mutation, no native writes, no sequencing: observation/request/command
//! counters and acknowledgement belong to the dependent sequencer unit,
//! which is deliberately not built here. The only native reads are the
//! lightweight per-claim reads for project-suppressed windows
//! ([`read_hidden_claim`], through the safe `ffi` wrappers), which the tick
//! builds into [`OwnerInputs::hidden_reads`] before capture so this body
//! stays pure over its seam.
//!
//! Source map:
//! - `observe.rs` `ObservedWindow`: identity-first observation with
//!   iconic-before-frame ordering. Lifetimes derive directly from the
//!   observed identity (`hwnd`, `pid`, `process_creation`) as the full
//!   [`WindowKey`](crate::native_guards::WindowKey), never HWND alone.
//! - `observation.rs` `TileLoop::observe`: minimized omission
//!   (`minimized_omitted`), scope/hosted gates and the per-tick retained
//!   vec. Identity-less `Unknown` evidence never becomes a member row.
//! - `retained.rs` `RetainedRow` (`rect: None` plus `facts: None` is
//!   frameless: unreadable or scope-excluded, never minimized),
//!   `HiddenRecord` (durable hide-time show preimage
//!   `claim.show: WindowShowState` plus hide-time `iconic`) and
//!   `PendingClosed` (homed verified-close obligations).
//! - The mechanical device-to-wire output tokens that serve as the
//!   wire-opaque output ids (native device keys such as `\\.\DISPLAY1` fail
//!   the core `is_opaque_id` gate, so the existing `output_token` source
//!   maps them collision-free).
//! - `monitors.rs` `MonitorArea`: device-keyed work (placement) and full
//!   (overlay) rectangles plus the effective per-monitor DPI measured at
//!   the enumeration seam. `MonitorArea` carries no primary flag; the owner
//!   orders `all_monitors` primary-first, so the report-level
//!   `primary_output` derives from the `areas.first()` device, never from
//!   managed output order (which may be reordered relative to the native
//!   list).
//! - Owner intent lanes (`floated`, `sticky`, `restart_reject_hold`,
//!   `fixed_auto_seen`, `fixed_tile_overrides`, `fixed_origin_overrides`,
//!   `born_fullscreen`, `born_first_exit`), lifetime tags (`member_tags`),
//!   stored identities (`member_identity`), snapshots (`member_rects` /
//!   `float_rects`), recorded constraints (`hint_logged`) and the durable
//!   hide claims (`hidden_claims`) are consumed read-only. The intent
//!   lanes pick only the adapter-local snapshot source below; none of them
//!   is a published v4 fact.
//!
//! v4 report truthfulness rules enforced here:
//! - Screens carry the stable wire output id, the FULL rectangle, the work
//!   area, the per-output scale (`effective DPI / 96`, measured at the
//!   monitor enumeration seam) and neutral identity metadata. Gaps and
//!   primary are not screen keys: the core derives per-output physical gaps
//!   from its settings and this scale, and the host designation rides
//!   exactly once as the report-level `primary_output`. An unmeasurable
//!   scale or an unmapped device never fabricates a row: an identifiable
//!   output with an unknown scale is held narrowly ([`HeldScreenRow`],
//!   `unreadable`), an unidentifiable device carries no row, and both clear
//!   completeness.
//! - Window rows carry raw facts only. `sticky` and `native_workspace` are
//!   native facts and Windows has neither (`sticky: None`,
//!   `NativeWorkspaceFact::Omitted`), so no native rows exist. The neutral
//!   facts the recorded observation proves ride: `tool_window`
//!   (`WS_EX_TOOLWINDOW`), `owned` (`GW_OWNER`), `above` (`WS_EX_TOPMOST`),
//!   `executable` (the bound/observed process image), `process_id` (the
//!   lifetime key's pid) and `window_type: Dialog` for the proven generic
//!   dialog class. Every other neutral fact stays `None`: `privileged`
//!   because the observation never reads integrity, `class`/`title`/`role`/
//!   `app_id` because the class and title reads are not carried into the
//!   observation this pure capture consumes, and the remaining lanes because
//!   Windows has no producer. Never a fabricated `false`. `hints` is the
//!   recorded constraint (`hint_logged`) or `None`, never an empty-hint
//!   stand-in. Two fresh-read lanes supply these facts: the ordinary visible
//!   observation (`WindowFacts`) and the lightweight per-claim read for a
//!   project-suppressed member with no live observed row ([`HiddenClaimRead`]).
//!   `host_hidden` is the fresh native cloak read (never project
//!   suppression), `visibility.visible` the fresh native shown state,
//!   `visibility.suppression_owned` the durable hide-claim presence, and
//!   `suppression_available` is claimed only where that claim proves
//!   suppression worked. The core derives floating, fit-exclusion, and
//!   fixed-size intent from its own settings and these facts; the adapter
//!   publishes none of them.
//! - Unknown output affinity is `output: None`, never the first screen or a
//!   placeholder id: a frame whose device has no live wire id reports no
//!   affinity rather than a guess.
//! - Every carried row re-validates its token against `member_tokens` for
//!   the same `WindowKey`. A mismatched lifetime inherits no flags and no
//!   lanes: its domain holds instead of carrying a partial controlled list.
//! - The stored member identity must name the same lifetime (`pid` and
//!   `process_creation` equal the key): drifted tables hold instead of
//!   scoping or classifying another process.
//! - Fresh native facts ride only when the typed report can express them:
//!   eligible, or an overlay (`fullscreen`/`maximized`) with a dedicated
//!   lane. Cloaked, elevated, shell, tool, owned, dialog, no-activate,
//!   hidden, or minimized facts have no window authority lane, so the
//!   domain holds instead of authorizing geometry, membership, or effects.
//!   Scope exclusion holds the same way: the typed report has no scope
//!   authority lane, so riding would authorize writes.
//! - Hidden members additionally prove the claim association: the
//!   `HiddenRecord` claim identity must equal the member key, the claim tag
//!   must satisfy `model::valid_claim_tag`, and a stamped lifetime tag must
//!   exist. The claim tag (hidden-claim nonce) and the member tag
//!   (visible-membership lifetime) are different native properties: only
//!   presence is required of the member tag, never equality. Unproven
//!   hidden lifetimes hold even when iconic: minimization is never granted
//!   on unverified identity.
//! - Hidden non-iconic members ride their snapshot rectangles with the
//!   durable hide-time maximized preimage (`claim.show.maximized`) and the
//!   fresh observed row when present. Fullscreen rides only verified
//!   sources (a token-matched fresh observed/retained row, or the
//!   `born_fullscreen` hold lane); a hidden member with no verifiable
//!   fullscreen holds its domain. The snapshot source (float or tile slot)
//!   is adapter-local placement memory, never a published float intent.
//!   Their native facts come only from that claim's lightweight live read:
//!   `host_hidden` is the DWM cloak, `visibility.visible` the shown state,
//!   and `owned`/`tool_window`/`window_type` their real reads. A claim with
//!   no read holds as unreadable; nothing hidden defaults flags to `false`
//!   to authorize writes.
//! - Frameless retained rows (`facts: None`) and scope-excluded members are
//!   never minimized: frameless iconic omission yields an explicit
//!   minimized entry, anything else frameless holds its domain as
//!   unreadable. Scope exclusion derives from the stored identity through
//!   production `scope_allows`; the hosted-child gate needs live child
//!   enumeration so it cannot re-run offline, but its retained consequence
//!   (membership kept with a frameless row) holds identically.
//! - Omission is never a close: only queued `PendingClosed` obligations
//!   ride the global close list. A close observed live again is stale and
//!   drops; closes homed on unknown workspaces stay pending with the caller.
//! - A failed enumeration (`None`, mirroring `TileLoop::observe`) holds
//!   every staged scope as incomplete and clears completeness: a failed
//!   enum never encodes as successful emptiness, and absence on a failed
//!   tick infers no close and no minimize. Explicit queued closes still
//!   ride beside the hold, matching the incomplete-inventory contract.
//! - Screens carry no adjacency and no native workspace vocabulary: FULL
//!   rectangles plus work areas and a measured per-output scale only.
//!   Unknown inventory never fabricates an empty row: any member that
//!   cannot resolve to a verified row rides an unreadable fact (rect
//!   `None`) and clears the global completeness flag.
//!
//! Production prerequisites consumed read-only (rest of the module is pure;
//! only [`read_hidden_claim`] touches native reads, for claims without a
//! live observed row):
//! - `assignments::audit_hidden_claims` retires dead claims through the
//!   identity-safe reveal path; `queue_verified_close` gates close queueing
//!   on the stored lifetime tag no longer reading live; admission stamps and
//!   re-verifies the visible-membership tag via `visible_lifetime_ok` (plus
//!   same-HWND reuse repair). Capture re-checks only what its inputs carry
//!   (claim-tag format, identity triple, token match, tag presence) and
//!   holds on any gap.

use super::*;
use crate::model::ProcessIdentity;
use crate::native_guards::WindowKey;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::intent_data::IntentData;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_report::{
    AuthorityScope, FullscreenOwner, HeldScreenRow, IntentRecord, MAX_IDENTITY_TEXT_BYTES,
    NativeWorkspaceFact, OutputIdentity, PresentationIngressV4, PresentationPagerFrameV4,
    RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScreenReport, VisibilityFact, WindowFact,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{WS_EX_TOOLWINDOW, WS_EX_TOPMOST};

/// Normalized foreground facts for one tick's uniform presentation ingress
/// (stage 5): the genuinely observed foreground lifetime (proof-verified
/// adapter-side, including unmanaged lifetimes) plus its fresh visibility
/// facts and live frame. The tick observes the foreground once per report
/// (the same `observe_window` product the fast overlay path reads per wake)
/// and the core publishes the border/underlay shapes from here. `None` when
/// there is no foreground window or it cannot be observed.
#[derive(Debug, Clone, Default)]
pub(super) struct PresentationForeground {
    pub token: String,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Native host-hidden (DWM cloak) read, the v4 `hostHidden` fact.
    pub host_hidden: bool,
    pub hidden_workspace: bool,
    pub frame: Option<Rect>,
}

/// Field-input seam for the single runtime WorldEngine owner.
///
/// The owner (`WorldOwner` inside `TileLoop`) borrows mutably while capture,
/// sequencing, and actuation read owner tables. Threading `&TileLoop` through
/// those paths self-borrows (`&mut self.world` plus `&self`) and cannot
/// compile at the production tick call site. This struct carries exactly the
/// read-only owner fields the world route consumes, with the same field
/// names as `TileLoop`, so capture bodies read unchanged while the runtime
/// builds one value from disjoint field borrows (`state.owner_inputs()`).
/// The legacy `&TileLoop` call shape is gone; tests construct the seam the
/// same way production does.
#[derive(Debug)]
pub(super) struct OwnerInputs<'a> {
    pub member_tokens: &'a BTreeMap<WindowKey, String>,
    pub member_identity: &'a BTreeMap<WindowKey, ProcessIdentity>,
    pub member_tags: &'a BTreeMap<WindowKey, String>,
    pub member_rects: &'a HashMap<String, Rect>,
    pub float_rects: &'a HashMap<String, Rect>,
    pub hint_logged: &'a HashMap<String, WindowSizeHints>,
    pub restart_reject_hold: &'a BTreeSet<WindowKey>,
    pub born_fullscreen: &'a BTreeSet<WindowKey>,
    pub born_first_exit: &'a BTreeSet<WindowKey>,
    pub fixed_auto_seen: &'a BTreeSet<WindowKey>,
    pub fixed_origin_overrides: &'a BTreeSet<WindowKey>,
    pub hidden_claims: &'a BTreeMap<WindowKey, HiddenRecord>,
    /// Fresh lightweight live reads for project-suppressed claim lifetimes
    /// with no live observed row (T4), keyed by the exact lifetime. Built by
    /// the tick from the safe native wrappers before capture (capture stays
    /// pure over this seam) and `None` for offline fixtures that supply
    /// their own reads. A claim with no entry holds as unreadable rather
    /// than inventing suppression or host hiding.
    pub hidden_reads: Option<&'a BTreeMap<WindowKey, HiddenClaimRead>>,
    /// Owned fullscreen metadata for this tick's bound lifetimes, keyed by
    /// the exact lifetime (built by the tick from the marker read before
    /// capture, or `None` for offline fixtures). A fullscreen fact whose key
    /// has no entry is app-owned: absence of metadata never proves the
    /// project owns the entry, so it is never exitable here.
    pub(super) fullscreen_meta: Option<&'a BTreeMap<WindowKey, super::fullscreen::FullscreenMeta>>,
    /// Canonical marker-storage observations for this tick's bound
    /// lifetimes, keyed by the exact lifetime (built by the tick before
    /// capture, or `None` for offline fixtures). A key with no entry
    /// publishes no intent record: an unprovided read is unknown, never an
    /// inferred desire.
    pub(super) intent_storage: Option<&'a BTreeMap<WindowKey, super::world_diff::IntentStorage>>,
    pub minimized_omitted: &'a HashSet<WindowKey>,
    pub pending_closed: &'a BTreeMap<String, PendingClosed>,
    pub scope: &'a [String],
    pub settings_live: &'a Option<LiveSettings>,
    /// Per-tick sampled cursor in adapter units (same space as screen FULL
    /// rectangles), `None` when unreadable, out of carried bounds, or
    /// deliberately unsampled (focused fullscreen game). Never a placement
    /// or output policy here: only the carried observation the core resolves.
    pub cursor: Option<(i32, i32)>,
    /// Raw native window under the sampled cursor (`0`/absent when none):
    /// the `WindowFromPoint` read for the same tick as [`OwnerInputs::cursor`],
    /// resolved to an exact managed lifetime below. `None` whenever the
    /// cursor itself is unsampled (game lane, unreadable), so a hit never
    /// travels without its position. Never focus or placement policy: only
    /// the raw observation the binding resolves.
    pub hit_hwnd: Option<u64>,
    /// Normalized gesture ingress for one tick (stage 4a): the exact open
    /// gesture lifetime, START-sampled kind, live cursor, and terminal
    /// phase. Built by the tick from native START-drain tables (fact
    /// collection only); the core owns every target/release decision.
    /// `None` on observation-only ticks. Terminal phases ride an explicit
    /// override stashed by the END path, never derived here.
    pub gesture: Option<tiler_core::gesture::GestureIngress>,
    /// Mechanical device-to-wire-output map (native device keys to opaque
    /// `oN` ids). Native-wire ids only, never rings/views/tiling/history.
    pub output_tokens: &'a HashMap<String, String>,
    /// Adapter-read physical identity per live GDI device name (same keys as
    /// `areas[].device`). Absent entry is explicit unknown.
    pub monitor_identities: &'a HashMap<String, OutputIdentity>,
    /// Adapter-retained cached output context as a neutral device fact
    /// (empty when none): the report maps it to wire for the core-owned
    /// migration/control action legs. Never an adapter output decision.
    pub active_output: &'a str,
    /// Adapter-retained last applied logical homing per Engine token
    /// (from the last applied reply rows). Receipts prove this commitment
    /// with fresh evidence; the core remains the sole homing authority.
    pub applied_homing: &'a HashMap<String, (String, String)>,
    /// Adapter-retained last applied logical views per output (from the last
    /// applied reply outputs). View receipts prove this commitment.
    pub applied_views: &'a HashMap<String, String>,
    pub active: &'a HashSet<u64>,
    /// Normalized foreground facts for the uniform presentation ingress
    /// (stage 5): `None` when there is no foreground window. The tick
    /// observes the foreground once per report; capture copies the facts
    /// into the report ingress verbatim.
    pub presentation_foreground: Option<PresentationForeground>,
    /// Movement-only Win+Shift chord level for the underlay trigger.
    pub presentation_chord: bool,
    /// Classified focused Move subject token (`""` when none): the move arm
    /// of the underlay trigger.
    pub presentation_move: String,
    /// Native top-to-bottom stacking snapshot from the latest observation
    /// (first is topmost). Capture maps it to opaque lifetimes for the
    /// pager order; unknown keys drop, and an empty snapshot falls back to
    /// observed-then-retained order.
    pub presentation_native_order: Vec<crate::native_guards::WindowKey>,
}

#[cfg(test)]
impl<'a> OwnerInputs<'a> {
    /// Offline fixture seam: ride the canonical intent-record lane exactly
    /// as a production tick's durable marker read does. `inputs()` leaves
    /// the lane absent (no inferred desire); tests modeling durable
    /// sticky/float markers attach one map for the whole fixture.
    pub(super) fn with_intent_storage(
        mut self,
        storage: &'a BTreeMap<WindowKey, super::world_diff::IntentStorage>,
    ) -> Self {
        self.intent_storage = Some(storage);
        self
    }
}

impl TileLoop {
    /// Build the read-only world-route seam from disjoint field borrows.
    /// The production tick holds `&mut self.world` alongside this value;
    /// every argument borrows a different field, never the whole loop, so
    /// the borrow checker accepts the split.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn owner_inputs<'a>(
        member_tokens: &'a BTreeMap<WindowKey, String>,
        member_identity: &'a BTreeMap<WindowKey, ProcessIdentity>,
        member_tags: &'a BTreeMap<WindowKey, String>,
        member_rects: &'a HashMap<String, Rect>,
        float_rects: &'a HashMap<String, Rect>,
        hint_logged: &'a HashMap<String, WindowSizeHints>,
        restart_reject_hold: &'a BTreeSet<WindowKey>,
        born_fullscreen: &'a BTreeSet<WindowKey>,
        born_first_exit: &'a BTreeSet<WindowKey>,
        fixed_auto_seen: &'a BTreeSet<WindowKey>,
        fixed_origin_overrides: &'a BTreeSet<WindowKey>,
        hidden_claims: &'a BTreeMap<WindowKey, HiddenRecord>,
        minimized_omitted: &'a HashSet<WindowKey>,
        pending_closed: &'a BTreeMap<String, PendingClosed>,
        scope: &'a [String],
        settings_live: &'a Option<LiveSettings>,
        output_tokens: &'a HashMap<String, String>,
        monitor_identities: &'a HashMap<String, OutputIdentity>,
        active_output: &'a str,
        applied_homing: &'a HashMap<String, (String, String)>,
        applied_views: &'a HashMap<String, String>,
        active: &'a HashSet<u64>,
        cursor: Option<(i32, i32)>,
        hit_hwnd: Option<u64>,
        gesture: Option<tiler_core::gesture::GestureIngress>,
        presentation_foreground: Option<PresentationForeground>,
        presentation_chord: bool,
        presentation_move: String,
        presentation_native_order: Vec<crate::native_guards::WindowKey>,
    ) -> OwnerInputs<'a> {
        OwnerInputs {
            member_tokens,
            member_identity,
            member_tags,
            member_rects,
            float_rects,
            hint_logged,
            restart_reject_hold,
            born_fullscreen,
            born_first_exit,
            fixed_auto_seen,
            fixed_origin_overrides,
            hidden_claims,
            hidden_reads: None,
            fullscreen_meta: None,
            intent_storage: None,
            minimized_omitted,
            pending_closed,
            scope,
            settings_live,
            output_tokens,
            monitor_identities,
            active_output,
            applied_homing,
            applied_views,
            active,
            cursor,
            hit_hwnd,
            gesture,
            presentation_foreground,
            presentation_chord,
            presentation_move,
            presentation_native_order,
        }
    }

    /// Test/offline convenience: build the seam from a shared borrow.
    /// Production ticks use the disjoint associated function above to hold
    /// `&mut world` alongside; offline code holds no such split. The cursor
    /// reads absent: tests needing one override the field after construction.
    #[cfg(test)]
    pub(super) fn inputs(&self) -> OwnerInputs<'_> {
        OwnerInputs {
            member_tokens: &self.member_tokens,
            member_identity: &self.member_identity,
            member_tags: &self.member_tags,
            member_rects: &self.member_rects,
            float_rects: &self.float_rects,
            hint_logged: &self.hint_logged,
            restart_reject_hold: &self.restart_reject_hold,
            born_fullscreen: &self.born_fullscreen,
            born_first_exit: &self.born_first_exit,
            fixed_auto_seen: &self.fixed_auto_seen,
            fixed_origin_overrides: &self.fixed_origin_overrides,
            hidden_claims: &self.hidden_claims,
            hidden_reads: None,
            fullscreen_meta: None,
            intent_storage: Some(&self.intent_snapshot),
            minimized_omitted: &self.minimized_omitted,
            pending_closed: &self.pending_closed,
            scope: &self.scope,
            settings_live: &self.settings_live,
            output_tokens: &self.output_tokens,
            monitor_identities: &self.monitor_identities,
            active_output: self.active_output.as_str(),
            applied_homing: &self.applied_homing,
            applied_views: &self.applied_views,
            active: &self.active,
            cursor: None,
            hit_hwnd: None,
            gesture: None,
            presentation_foreground: None,
            presentation_chord: false,
            presentation_move: String::new(),
            presentation_native_order: Vec::new(),
        }
    }

    /// Normalized gesture ingress for one tick (stage 4a): the exact open
    /// gesture lifetime, stable gesture id, START-sampled kind, live and
    /// START cursors, and phase. Fact collection only: the single open
    /// managed gesture maps to core facts from the START-drain tables
    /// (token binding, PRE-gesture frame, id, modal kind, Esc latch); a
    /// stashed terminal override (release/cancel with its END-captured
    /// facts) takes precedence. The first tick carrying an open gesture
    /// reports `Started` with the down-point cursor (the core binds its
    /// immutable START truth from it); later ticks report `Updated` with
    /// the live cursor. Every phase carries the stable id plus the START
    /// down point, so a lost Started never loses the zero gate or the
    /// at-most-once identity. Multi-gesture opens, unmanaged races,
    /// unknown START kinds, missing PRE snapshots, and missing ids emit
    /// nothing (fail closed; the barrier bool still holds scopes). The core
    /// owns every target/release decision from here.
    pub(super) fn gesture_ingress(
        &mut self,
        cursor: Option<(i32, i32)>,
    ) -> Option<tiler_core::gesture::GestureIngress> {
        use tiler_core::gesture::{GestureIngress, GestureKind, GesturePhase};
        if let Some(stashed) = self.gesture_override.clone() {
            return Some(stashed);
        }
        let mut movers: Vec<u64> = self
            .active
            .iter()
            .copied()
            .filter(|hwnd| self.managed.contains(hwnd) && self.gesture_before.contains_key(hwnd))
            .collect();
        movers.sort_unstable();
        if movers.len() != 1 {
            return None;
        }
        let hwnd = movers[0];
        let start = self.gesture_preview_start.get(&hwnd)?.clone();
        let before = self.gesture_before.get(&hwnd).copied()?;
        // Stable gesture id, allocated once at native START drain and
        // retained through settle. Absent only when the START tables were
        // never captured: fail closed, never an anonymous ingress.
        let id = self.gesture_seq.get(&hwnd).copied()?;
        let kind = match self.move_kind.get(&hwnd) {
            Some(crate::group_underlay::MoveSizeKind::Move) => GestureKind::Move,
            Some(crate::group_underlay::MoveSizeKind::Resize) => GestureKind::Resize,
            _ => return None,
        };
        // The START down point rides every ingress (never derived from the
        // live sample), so a first Updated after a lost Started keeps the
        // true zero gate. `None` only when the down point was unreadable.
        let down = self.gesture_start_cursor.get(&hwnd).copied();
        // First sighting reports the start: down-point cursor and modal
        // truth frozen once, so the core zero-gates later samples against
        // the true down point. Later sightings carry the live cursor.
        if self.gesture_started_sent.insert(hwnd) {
            Some(GestureIngress {
                id,
                window: tiler_core::directional::WindowId(start.token.clone()),
                kind,
                cursor: down.or(cursor),
                start_cursor: down,
                phase: GesturePhase::Started,
                esc: self.esc_latched.contains(&hwnd),
                modal_move: kind == GestureKind::Move,
                start_frame: before,
            })
        } else {
            Some(GestureIngress {
                id,
                window: tiler_core::directional::WindowId(start.token.clone()),
                kind,
                cursor,
                start_cursor: down,
                phase: GesturePhase::Updated,
                esc: self.esc_latched.contains(&hwnd),
                modal_move: kind == GestureKind::Move,
                start_frame: before,
            })
        }
    }

    /// Normalized presentation facts for one tick's uniform presentation
    /// ingress (stage 5): the foreground observation (proof-verified token,
    /// fresh flags, live frame), the movement-only chord level, and the
    /// classified focused Move subject token. Fact collection only: the
    /// core owns every verdict from here. The foreground mirrors the fast
    /// overlay path's native facts (iconic/zoom/fullscreen/cloak plus hide-claim
    /// membership for the logical hidden-workspace leg); the adapter-side
    /// shell projection has no v4 ingress lane and is not produced. A
    /// known-but-frameless foreground (minimized, maximized, fullscreen,
    /// cloaked) still carries its token and accurate flags with no frame;
    /// anything else unreadable carries nothing.
    #[allow(clippy::too_many_lines)]
    pub(super) fn presentation_tick_inputs(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        foreground_hwnd: u64,
        observed: &[ObservedWindow],
    ) -> (Option<PresentationForeground>, bool, String) {
        use crate::group_underlay::MoveSizeKind;
        // Prefer the tick's own observation: it is the same enumeration the
        // command/observation carries, so presentation focus never diverges
        // from the staged rows and needs no extra native read. Only an
        // ineligible/missing foreground falls through to the native probe.
        let observed_foreground = observed.iter().find(|row| row.hwnd == foreground_hwnd);
        #[allow(unused_mut)]
        let mut foreground = if let Some(window) = observed_foreground {
            let key = crate::native_guards::WindowKey {
                hwnd: window.hwnd,
                pid: window.identity.pid,
                creation: window.identity.process_creation.clone(),
            };
            let hidden_workspace = self.hidden_claims.keys().any(|k| k.hwnd == foreground_hwnd)
                && self.member_home(&key).is_some_and(|loc| loc.hidden);
            Some(PresentationForeground {
                token: window.token.clone(),
                minimized: window.facts.minimized,
                maximized: window.facts.maximized,
                fullscreen: window.facts.captionless_fullscreen,
                host_hidden: window.facts.cloaked,
                hidden_workspace,
                frame: Some(window.visible),
            })
        } else if foreground_hwnd == 0 {
            None
        } else {
            let hwnd = foreground_hwnd as isize as HWND;
            match super::observe::observe_window(hwnd, me, fulls, &mut self.tokens) {
                Ok(window) => {
                    let key = crate::native_guards::WindowKey {
                        hwnd: window.hwnd,
                        pid: window.identity.pid,
                        creation: window.identity.process_creation.clone(),
                    };
                    let hidden_workspace =
                        self.hidden_claims.keys().any(|k| k.hwnd == foreground_hwnd)
                            && self.member_home(&key).is_some_and(|loc| loc.hidden);
                    Some(PresentationForeground {
                        token: window.token.clone(),
                        minimized: window.facts.minimized,
                        maximized: window.facts.maximized,
                        fullscreen: window.facts.captionless_fullscreen,
                        host_hidden: window.facts.cloaked,
                        hidden_workspace,
                        frame: Some(window.visible),
                    })
                }
                Err(super::observe::ObserveFailure::Known(known, reason)) => {
                    let key = crate::native_guards::WindowKey {
                        hwnd: known.identity.hwnd,
                        pid: known.identity.pid,
                        creation: known.identity.process_creation.clone(),
                    };
                    let hidden_workspace =
                        self.hidden_claims.keys().any(|k| k.hwnd == foreground_hwnd)
                            && self.member_home(&key).is_some_and(|loc| loc.hidden);
                    Some(PresentationForeground {
                        token: known.token.clone(),
                        minimized: reason == SkipReason::Minimized,
                        maximized: reason == SkipReason::Maximized,
                        fullscreen: reason == SkipReason::Fullscreen,
                        host_hidden: reason == SkipReason::Cloaked,
                        hidden_workspace,
                        frame: None,
                    })
                }
                Err(_) => None,
            }
        };
        // Offline fixtures inject observed rows for synthetic HWNDs; the
        // native foreground re-read above cannot see them. Reuse the injected
        // row so presentation/focus ingress matches the same observation.
        #[cfg(test)]
        if observed_foreground.is_none()
            && let Some(window) = super::INJECTED_OBSERVE.with(|slot| {
                slot.borrow().as_ref().and_then(|(observed, _, failed)| {
                    (!*failed)
                        .then(|| observed.iter().find(|row| row.hwnd == foreground_hwnd))
                        .flatten()
                        .cloned()
                })
            })
        {
            let key = crate::native_guards::WindowKey {
                hwnd: window.hwnd,
                pid: window.identity.pid,
                creation: window.identity.process_creation.clone(),
            };
            let hidden_workspace = self.hidden_claims.keys().any(|k| k.hwnd == foreground_hwnd)
                && self.member_home(&key).is_some_and(|loc| loc.hidden);
            foreground = Some(PresentationForeground {
                token: window.token.clone(),
                minimized: window.facts.minimized,
                maximized: window.facts.maximized,
                fullscreen: window.facts.captionless_fullscreen,
                host_hidden: window.facts.cloaked,
                hidden_workspace,
                frame: Some(window.visible),
            });
        }
        let (win_held, shift_held) = super::group_underlay::chord_held_now();
        let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
        let move_token = if self
            .move_kind
            .get(&foreground_hwnd)
            .is_some_and(|kind| *kind == MoveSizeKind::Move)
        {
            foreground
                .as_ref()
                .map(|facts| facts.token.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        (foreground, chord, move_token)
    }
}

/// Captured world: the typed core observation pieces the sequencer unit
/// envelopes into a `WorldReportV4`. Sequencing fields (contract version,
/// owner/incarnation/generation, observation/request/command counters,
/// commands, attempts, native/held lanes, retained intent) are deliberately
/// absent here: screens, held screens and window facts are the observation
/// prerequisites, with the sampled cursor and the retained targeting
/// settings riding as core ingress (never an adapter placement or output
/// policy). Observed logical views/membership are core-owned in v4 and are
/// not captured.
#[derive(Debug, Clone)]
pub(super) struct CapturedWorld {
    pub(super) screens: Vec<ScreenReport>,
    /// Identifiable outputs whose per-output scale could not be measured:
    /// held narrowly (`unreadable`) instead of carrying a fabricated scale.
    pub(super) held_screens: Vec<HeldScreenRow>,
    pub(super) windows: Vec<WindowFact>,
    /// Canonical intent records for observed exact member lifetimes: one row
    /// per bound token with a WindowFact this capture, sourced only from the
    /// tick's canonical marker reads (`data` is the encoded core record,
    /// never an inferred desire). Empty when no read proves a marker.
    pub(super) intent_records: Vec<IntentRecord>,
    pub(super) closed: Vec<WindowId>,
    /// Cursor-first switch target for subsequent selections (report ingress
    /// only; never an adapter output decision). Backfills the default when
    /// no live settings exist (proof owners).
    pub(super) switch_target: tiler_core::cursor_targeting::SwitchTarget,
    /// Fresh-window placement for subsequent admissions (report ingress
    /// only; never an adapter placement policy). Marker-restored lifetimes
    /// are restored intent, not fresh placement (core-owned).
    pub(super) fresh_placement: tiler_core::cursor_targeting::FreshPlacement,
    /// Retained focus-follows-cursor setting (stage 4b actuates it in
    /// [`crate::world_cursor`]).
    pub(super) focus_follows_cursor: bool,
    /// Retained cursor-warp setting (stage 4b actuates it in
    /// [`crate::world_cursor`] once C3 lands; tests read the captured
    /// projection today). Kept as adapter truth, never a policy input.
    #[allow(dead_code)]
    pub(super) cursor_warp: bool,
    /// Normalized cursor position in adapter units (`None` when unreadable
    /// or deliberately unsampled). Never continuous polling: one sample per
    /// capture, none during focused fullscreen game.
    pub(super) cursor: Option<(i32, i32)>,
    /// Normalized cursor-hit managed lifetime for the same tick as
    /// [`CapturedWorld::cursor`]: the exact managed window under the cursor
    /// (`None` when the cursor is unsampled, or the point lands on foreign,
    /// hidden, or otherwise unmanaged pixels). Native hit test plus exact
    /// managed binding, never a focus or placement input.
    pub(super) cursor_hit: Option<WindowId>,
    pub(super) inventory_complete: bool,
    pub(super) gesture_barrier: bool,
    /// Normalized gesture ingress for one tick (stage 4a): the exact open
    /// gesture lifetime, START-sampled kind, live cursor, and phase. The
    /// core retains the session and owns every target/release decision.
    /// `None` on observation-only ticks.
    pub(super) gesture: Option<tiler_core::gesture::GestureIngress>,
    pub(super) focused_global: String,
    /// Provenance of [`CapturedWorld::focused_global`]: `true` plus a
    /// nonempty id when a fresh observed foreground names it, `true` plus
    /// empty when the native read positively saw no foreground window
    /// (`GetForegroundWindow` returned none), `false` plus empty when the
    /// foreground is unreadable or unbound (unknown, never no-client).
    pub(super) foreground_known: bool,
    /// `None` is snapshot-only: this capture has no event-coverage interval,
    /// so it never asserts a gap or a covered span.
    pub(super) focus_changes: Option<tiler_core::world_report::FocusChanges>,
    /// Neutral migration/control action foreground: the live foreground
    /// window's frame grounded to a live device and mapped to wire
    /// (`None` without a readable foreground frame or on an unmapped
    /// device). Same rectangle rule as observed affinity, including
    /// unmanaged windows; the core filters it to the known registry and
    /// applies it as the first action-context leg. Never an adapter
    /// decision.
    pub(super) action_foreground: Option<OutputId>,
    /// Neutral migration/control action cache: the adapter-retained cached
    /// output context mapped to wire (`None` when empty or unmapped). The
    /// core filters it to the known registry like every other leg and
    /// applies it as the third action-context leg; a mapped-but-removed
    /// entry still selects and refuses downstream. Never an adapter
    /// decision.
    pub(super) action_cached: Option<OutputId>,
    /// Neutral ordered known-output registry for the migration/control
    /// action context: sorted native registry keys (including
    /// disconnected mappings, never reordered) translated to wire. Empty
    /// only when the adapter knows no output at all.
    pub(super) action_outputs: Vec<OutputId>,
    /// Host-designated primary output: the primary-first monitor area's
    /// device mapped to wire. `None` when the host designates none or the
    /// primary device has no live wire id (unknown, never a replacement).
    pub(super) primary_output: Option<OutputId>,
    /// Foreground output resolved from the same capture: the observed foreground
    /// frame's exact native rectangle mapped through `output_for_rect` over the
    /// carried monitor areas and then through the mechanical `output_tokens` map
    /// (`None` when the foreground is unknown/empty, the frame is absent, the
    /// rectangle maps to no carried area, the device has no live wire id, or the
    /// resolved output disagrees with the foreground WindowFact's output). The
    /// sequencer publishes it as `PresentationIngressV4.foreground_out`; a
    /// disagreement fail-closes to `None` (the core then falls back to the fact).
    pub(super) foreground_output: Option<OutputId>,
    /// Uniform presentation ingress (stage 5): normalized foreground,
    /// movement, chord, stacking order and live pager frames. The sequencer
    /// envelopes it as `PresentationRowV4::Valid`; the core publishes the
    /// reply presentation from it.
    pub(super) presentation: PresentationIngressV4,
    /// Closes observed live again: stale, dropped, never carried.
    /// (Read by the 2b owner cutover; capture retains the evidence meanwhile.)
    #[allow(dead_code)]
    pub(super) dropped_stale_closed: Vec<String>,
    /// Closes homed on unknown workspaces: uncarried, retained by the caller.
    #[cfg(test)]
    pub(super) unhomed_closed: Vec<String>,
    /// Member tokens withheld by unreadable facts (never silently dropped).
    /// (Read by the 2b owner cutover; capture retains the evidence meanwhile.)
    #[allow(dead_code)]
    pub(super) held_members: Vec<String>,
}

/// Adapter-local snapshot slot selection for one hidden row: sticky wins
/// unconditionally; the D4 automatic-fixed latch floats unless an explicit
/// tile win dominates; marker-derived adapter float intent (floats,
/// restart-holds, born holds) floats unless a first-exit mark already
/// returned the member to the tiled lane. It only picks the retained float
/// vs tile frame source for a hidden snapshot; v4 publishes no float
/// intent, so this is never a report fact. No legacy Engine exception
/// lane: the core owns float exceptions outright.
fn capture_floating(
    state: &OwnerInputs<'_>,
    key: &crate::native_guards::WindowKey,
    hidden: bool,
) -> bool {
    let storage = state.intent_storage.and_then(|map| map.get(key));
    let sticky = storage.is_some_and(|s| s.sticky_prior.is_some());
    let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
    crate::native_guards::float_lane(
        sticky,
        fixed_auto,
        fixed_suppress,
        state.born_first_exit.contains(key),
        storage.is_some_and(|s| s.float),
        state.restart_reject_hold.contains(key),
        state.born_fullscreen.contains(key),
        hidden,
    )
}

/// The three fixed-size lanes, preserved distinctly for the snapshot-slot
/// decision: the explicit tile-win lanes (`fixed_tile_overrides` runtime
/// plus `fixed_origin_overrides` durable) dominate as `fixed_suppress`; the
/// D4 automatic latch (`fixed_auto_seen`) reports only when neither sticky
/// intent nor a durable win reframes it (production stamp parity: sticky
/// dominates automatic origin and durable wins never acquire it). No lane
/// is published in v4.
fn capture_fixed(state: &OwnerInputs<'_>, key: &crate::native_guards::WindowKey) -> (bool, bool) {
    let storage = state.intent_storage.and_then(|map| map.get(key));
    let durable = storage.is_some_and(|s| s.tile) || state.fixed_origin_overrides.contains(key);
    let sticky = storage.is_some_and(|s| s.sticky_prior.is_some());
    let automatic = state.fixed_auto_seen.contains(key) && !sticky && !durable;
    (automatic, durable)
}

/// One project-suppressed (`SW_HIDE`) claim lifetime's lightweight live read
/// (T4). Such a member is absent from the ordinary visible observation, so
/// capture cannot reuse `WindowFacts`; these are the only native inputs that
/// classify it truthfully: the shown state (`visibility.visible`), the DWM
/// cloak (`host_hidden`, not project suppression), the tool-window and
/// topmost style bits, native ownership, and the generic dialog kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HiddenClaimRead {
    /// Fresh `IsWindowVisible` read. Project-suppressed windows normally read
    /// `false`; the native truth rides, never the claim's target.
    pub visible: bool,
    /// Fresh DWM cloak read: the only `host_hidden` source for these rows.
    pub cloaked: bool,
    /// `GW_OWNER` non-null: native ownership, independent of dialog kind.
    pub owned: bool,
    /// `WS_EX_TOOLWINDOW` set.
    pub tool_window: bool,
    /// `WS_EX_TOPMOST` set.
    pub topmost: bool,
    /// Unowned generic Win32 dialog class (`#32770`), mirroring the observed
    /// `facts.dialog` kind gate. The class text itself stays unpublished:
    /// `class` is declared unavailable on this route.
    pub dialog: bool,
}

/// Real lightweight live read for one project-suppressed claim HWND, through
/// the safe `ffi` wrappers. Fail closed: an invalid handle or any unreadable
/// required input (cloak, style word, class name) returns `None`, so capture
/// holds that scope instead of inventing a native verdict.
pub(super) fn read_hidden_claim(hwnd: u64) -> Option<HiddenClaimRead> {
    if !crate::ffi::is_window(hwnd) {
        return None;
    }
    let visible = crate::ffi::is_visible(hwnd);
    let cloaked = crate::ffi::cloaked(hwnd)?;
    crate::ffi::window_style_checked(hwnd)?;
    let exstyle = crate::ffi::window_exstyle_checked(hwnd)?;
    let owned = crate::ffi::owner_of(hwnd) != 0;
    let class = crate::ffi::try_class_name(hwnd)?;
    Some(HiddenClaimRead {
        visible,
        cloaked,
        owned,
        tool_window: exstyle & WS_EX_TOOLWINDOW != 0,
        topmost: exstyle & WS_EX_TOPMOST != 0,
        dialog: !owned && class == crate::tiling::GENERIC_DIALOG_CLASS,
    })
}

/// Live reads for every hidden claim without a live observed row: the
/// production tick builds this before capture and threads it through
/// [`OwnerInputs::hidden_reads`]. Keys already observed ride the ordinary
/// visible lane, and a failed read stays absent so capture holds exactly
/// that claim's scope. A failed enumeration already holds every scope, so
/// no reads are attempted then.
pub(super) fn hidden_claim_reads(
    hidden_claims: &BTreeMap<WindowKey, HiddenRecord>,
    observed: Option<&[ObservedWindow]>,
) -> BTreeMap<WindowKey, HiddenClaimRead> {
    let Some(observed) = observed else {
        return BTreeMap::new();
    };
    let observed_keys: HashSet<WindowKey> = observed
        .iter()
        .map(|window| WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        })
        .collect();
    hidden_claims
        .keys()
        .filter(|key| !observed_keys.contains(*key))
        .filter_map(|key| read_hidden_claim(key.hwnd).map(|read| (key.clone(), read)))
        .collect()
}

/// One captured fact row's inputs. Grouped so every call site states the
/// exact truth it carries, and no field can silently pick up a default.
struct FactSeed<'a> {
    token: &'a str,
    /// Bound lifetime key: the source of the process id and of the durable
    /// hide-claim verdict.
    key: &'a WindowKey,
    /// Executable the recorded/observed identity proves for this lifetime,
    /// `None` when identity is unread or drifted.
    executable: Option<&'a str>,
    /// Durable hide-claim presence: the only suppression-ownership proof.
    suppression_owned: bool,
    /// Recorded constraint (`hint_logged`) or `None`, never an empty stand-in.
    hints: Option<WindowSizeHints>,
    output: Option<OutputId>,
    rect: Option<Rect>,
    /// Fresh native read for this tick (`None` for unreadable/minimized
    /// rows): the sole source of shown state, the native cloak fact, and the
    /// neutral style facts below.
    flags: Option<&'a WindowFacts>,
    /// Lightweight live read for a project-suppressed claim with no live
    /// observed row: the only source of shown state, cloak, `owned`,
    /// `tool_window`, `above`, and the generic dialog kind for that lane.
    /// Mutually exclusive with `flags` by construction; `None` keeps the
    /// flags lane.
    live: Option<&'a HiddenClaimRead>,
    /// Explicit host-hidden override for rows with no fresh native read
    /// (hidden-workspace claims); `None` keeps the flags-derived value.
    host_hidden: Option<bool>,
    fullscreen: Option<bool>,
    /// Owned fullscreen metadata for this exact lifetime from the tick's
    /// marker read (`None` for unreadable/minimized lanes and for any
    /// lifetime without owned metadata). A fullscreen row with no metadata
    /// is app-owned: never exitable, never a fabricated project preimage.
    fullscreen_meta: Option<&'a super::fullscreen::FullscreenMeta>,
    maximized: Option<bool>,
    minimized: Option<bool>,
}

/// One v4 window fact built from capture truth. Every neutral lane the
/// recorded native read proves is carried (`tool_window`, `owned`,
/// `executable`, `process_id`, `#32770`-to-dialog `window_type`); lanes with
/// no producer on this route stay `None`, never a fabricated default.
/// `privileged` stays `None`: the current observation never reads integrity
/// (the legacy `elevated` bit is a hardcoded false, not a read). `class`
/// stays `None`: it is declared unavailable on this route, and the hidden
/// claim lane publishes only its generic-dialog kind, never the class text.
/// `hints` is the recorded constraint or `None`. The durable hide claim is
/// the only suppression ownership proof: presence proves this lifetime is
/// suppressed (and that suppression worked), absence proves the adapter owns
/// no suppression for it.
fn window_fact(seed: FactSeed<'_>) -> WindowFact {
    let FactSeed {
        token,
        key,
        executable,
        suppression_owned,
        hints,
        output,
        rect,
        flags,
        live,
        host_hidden,
        fullscreen,
        fullscreen_meta,
        maximized,
        minimized,
    } = seed;
    // Fullscreen ownership is proven only by the tick's owned metadata for
    // this exact lifetime; a live fullscreen row without it is app-owned and
    // never exitable through the project. Unreadable/non-fullscreen rows
    // publish neither field (unknown, never a fabricated app default).
    let (fullscreen_owner, fullscreen_exitable) = match fullscreen {
        Some(true) if fullscreen_meta.is_some() => (Some(FullscreenOwner::Tiler), Some(true)),
        Some(true) => (Some(FullscreenOwner::App), Some(false)),
        _ => (None, None),
    };
    WindowFact {
        window: WindowId(token.to_owned()),
        output,
        rect,
        fullscreen,
        maximized,
        // Windows has no native sticky fact (W3): logical sticky composes
        // core-side through suppression.
        sticky: None,
        minimized,
        hints,
        native_workspace: NativeWorkspaceFact::Omitted,
        host_hidden: host_hidden
            .or_else(|| flags.map(|facts| facts.cloaked))
            .or_else(|| live.map(|read| read.cloaked)),
        privileged: None,
        managed: None,
        accepts_input: None,
        activation_inhibited: None,
        // `WS_EX_TOOLWINDOW`, `GW_OWNER` and `WS_EX_TOPMOST` come from the
        // same fresh read as the frame (or the claim lane's lightweight
        // live read); a held row carries no such fact.
        tool_window: flags
            .map(|facts| facts.tool_window)
            .or_else(|| live.map(|read| read.tool_window)),
        owned: flags
            .map(|facts| facts.owned)
            .or_else(|| live.map(|read| read.owned)),
        transient: None,
        modal: None,
        resizable: None,
        above: flags
            .map(|facts| facts.topmost)
            .or_else(|| live.map(|read| read.topmost)),
        below: None,
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: executable.map(str::to_owned),
        process_id: Some(key.pid.to_string()),
        class: None,
        title: None,
        // Normal for an eligible ordinary window; `#32770` dialog rows keep
        // their dialog kind. Shell/tool/owned/dialog rows never reach this
        // direct-fact constructor (capture holds them as unreadable).
        window_type: flags
            .map(|facts| facts.dialog)
            .or_else(|| live.map(|read| read.dialog))
            .map(|dialog| {
                if dialog {
                    tiler_core::world_report::WindowType::Dialog
                } else {
                    tiler_core::world_report::WindowType::Normal
                }
            }),
        activation_policy: None,
        layer: None,
        fullscreen_owner,
        fullscreen_exitable,
        // Availability is the adapter's capability to suppress this window,
        // not proof that a claim already exists: ordinary readable rows are
        // suppressible on demand, hidden rows proved it, and unreadable rows
        // stay unknown. `visibility.suppression_owned` carries the current
        // durable claim separately.
        suppression_available: if suppression_owned || flags.is_some() || live.is_some() {
            Some(true)
        } else {
            None
        },
        visibility: VisibilityFact {
            visible: flags
                .map(|facts| facts.visible)
                .or_else(|| live.map(|read| read.visible)),
            suppression_owned: Some(suppression_owned),
        },
    }
}

/// Stored executable for a bound lifetime, only when the stored identity
/// still names the same process (`pid` and `process_creation` equal the
/// key): a drifted identity proves nothing about this lifetime.
fn bound_executable<'a>(state: &'a OwnerInputs<'_>, key: &WindowKey) -> Option<&'a str> {
    state
        .member_identity
        .get(key)
        .filter(|identity| identity.pid == key.pid && identity.process_creation == key.creation)
        .map(|identity| identity.exe_path.as_str())
}

/// Whether fresh native facts may ride a typed report row: eligible, or an
/// overlay (`fullscreen`/`maximized`) with a dedicated `EngineWindow` lane.
/// Every other `classify` refusal (cloaked, elevated, shell, tool, owned,
/// dialog, no-activate, hidden, minimized) has no window authority lane, so
/// carrying such a row as tiled would publish controlled frame authority
/// for a window the adapter must not touch. Callers hold the domain
/// instead.
pub(super) fn capture_facts_rideable(facts: &WindowFacts) -> bool {
    match crate::tiling::classify(facts) {
        Ok(()) => true,
        Err(SkipReason::Fullscreen | SkipReason::Maximized) => true,
        Err(_) => false,
    }
}

/// Resolve one live frame rectangle to its exact carried-output wire token.
/// Uses the core rectangle rule over the carried monitor areas and the
/// mechanical device->wire map only: a rectangle that names no carried area,
/// or a device with no live wire id, is `None` (never the `display-?`
/// fallback used by the direction-only resolver).
fn foreground_output_for_rect(
    areas: &[MonitorArea],
    rect: &Rect,
    output_tokens: &HashMap<String, String>,
) -> Option<OutputId> {
    // `output_for_rect` breaks an all-zero overlap tie by returning the
    // first entry, so prove the frame actually lands on a carried area
    // before delegating: an off-inventory rectangle (a transient or
    // moved-away frame) reports no output rather than the first screen.
    if !super::directional_resolution::frame_overlaps_output(areas, rect) {
        return None;
    }
    let entries: Vec<tiler_core::output_selection::OutputEntry> = areas
        .iter()
        .map(|area| tiler_core::output_selection::OutputEntry {
            id: OutputId(area.device.clone()),
            rect: area.full,
        })
        .collect();
    let device = tiler_core::output_selection::output_for_rect(&entries, rect)?;
    output_tokens.get(&device.0).cloned().map(OutputId)
}

/// Uniform presentation ingress (stage 5): normalized foreground facts
/// plus movement/chord facts from the seam, with the native top-to-bottom
/// stacking snapshot derived from the same observed/retained products as
/// the pager order (observed eligible members in enumeration order, then
/// retained rows with fresh frames). Hidden members absent from both keep
/// their retained stacking slot core-side. Unknown lifetimes never admit
/// members. v4 carries exactly `{win, live}` pager rows: only fresh native
/// frames ride, never the adapter's retained float/tile slots, and no
/// adapter border/underlay style is an ingress key (the core derives it
/// from its settings and per-output scale). The frame witness join is not
/// implemented on Windows, so `witness` stays unknown (`None`). Capture
/// resolves the observed foreground output separately in
/// [`CapturedWorld::foreground_output`] from the same observed frame and the
/// carried areas; the sequencer injects it as `foreground_out`, so this
/// builder never guesses one (`foreground_out` stays `None` here).
fn presentation_ingress(
    state: &OwnerInputs<'_>,
    observed: Option<&[ObservedWindow]>,
    retained: &[RetainedRow],
    foreground_hwnd: u64,
) -> PresentationIngressV4 {
    // Stable token per enumerated key: observed and retained rows carry
    // theirs; hidden members absent from the enumeration keep theirs in
    // `member_tokens`.
    let mut tokens: HashMap<crate::native_guards::WindowKey, &str> = HashMap::new();
    if let Some(observed) = observed {
        for window in observed {
            tokens.insert(
                crate::native_guards::WindowKey {
                    hwnd: window.hwnd,
                    pid: window.identity.pid,
                    creation: window.identity.process_creation.clone(),
                },
                window.token.as_str(),
            );
        }
    }
    for row in retained {
        tokens.entry(row.key.clone()).or_insert(row.token.as_str());
    }
    for (key, token) in state.member_tokens.iter() {
        tokens.entry(key.clone()).or_insert(token.as_str());
    }
    // Native top-to-bottom order mapped to opaque lifetimes; unknown keys
    // drop. An empty snapshot (test seeding without an observation) falls
    // back to observed-then-retained order. Duplicates keep their first
    // occurrence.
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for key in &state.presentation_native_order {
        if let Some(token) = tokens.get(key)
            && seen.insert(token)
        {
            order.push((*token).to_owned());
        }
    }
    if order.is_empty() {
        if let Some(observed) = observed {
            for window in observed {
                if seen.insert(window.token.as_str()) {
                    order.push(window.token.clone());
                }
            }
        }
        for row in retained {
            if row.rect.is_some() && seen.insert(row.token.as_str()) {
                order.push(row.token.clone());
            }
        }
    }
    // One live pager source per token: the fresh native frame from this
    // tick's enumeration (observed visible, then retained overlay rows).
    // Retained float/tile slots are adapter placement memory, not v4 ingress
    // rows, so a slot without a fresh frame renders no miniature rather
    // than echoing a stale rectangle.
    let mut frames: BTreeMap<String, PresentationPagerFrameV4> = BTreeMap::new();
    if let Some(observed) = observed {
        for window in observed {
            frames
                .entry(window.token.clone())
                .or_insert(PresentationPagerFrameV4 {
                    window: WindowId(window.token.clone()),
                    live: Some(window.visible),
                });
        }
    }
    for row in retained {
        if let Some(rect) = row.rect {
            frames
                .entry(row.token.clone())
                .or_insert(PresentationPagerFrameV4 {
                    window: WindowId(row.token.clone()),
                    live: Some(rect),
                });
        }
    }
    // The tick's own observed row is the same foreground truth when the
    // caller supplied one without a separate presentation read (offline
    // sequencer commands and any direct capture). Production ticks still
    // carry the richer presentation facts, including ineligible windows.
    let observed_foreground = if state.presentation_foreground.is_none() {
        observed
            .and_then(|rows| rows.iter().find(|row| row.hwnd == foreground_hwnd))
            .map(|window| PresentationForeground {
                token: window.token.clone(),
                minimized: window.facts.minimized,
                maximized: window.facts.maximized,
                fullscreen: window.facts.captionless_fullscreen,
                host_hidden: window.facts.cloaked,
                hidden_workspace: false,
                frame: Some(window.visible),
            })
    } else {
        None
    };
    let foreground = state
        .presentation_foreground
        .as_ref()
        .or(observed_foreground.as_ref());
    PresentationIngressV4 {
        foreground: foreground
            .map(|facts| facts.token.clone())
            .unwrap_or_default(),
        foreground_minimized: foreground.map(|facts| facts.minimized),
        foreground_maximized: foreground.map(|facts| facts.maximized),
        foreground_fullscreen: foreground.map(|facts| facts.fullscreen),
        foreground_host_hidden: foreground.map(|facts| facts.host_hidden),
        foreground_hidden_workspace: foreground.map(|facts| facts.hidden_workspace),
        foreground_frame: foreground.and_then(|facts| facts.frame),
        chord_held: state.presentation_chord,
        move_window: state.presentation_move.clone(),
        pager_order: order
            .into_iter()
            .map(tiler_core::directional::WindowId)
            .collect(),
        pager_frames: frames.into_values().collect(),
        witness: None,
        foreground_out: None,
    }
}

/// Resolve one raw cursor-hit HWND to the exact managed lifetime under the
/// cursor, if any. The map covers only exactly-bound members (the stored
/// token matches the observed/retained token for the same full identity):
/// foreign pixels, owned overlays, recycled HWNDs, and unbound newcomers
/// resolve to nothing, never to a guess. A child window or owned dialog
/// walks up through its parent/owner chain to the managed top-level frame
/// (bounded steps); a chain that leaves managed membership reports
/// nothing. Pure over the caller-supplied tables plus the native owner
/// step, so offline fixtures script the step.
pub(super) fn resolve_cursor_hit(
    hit_hwnd: u64,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_tokens: &BTreeMap<WindowKey, String>,
    owner_step: &dyn Fn(u64) -> u64,
) -> Option<WindowId> {
    if hit_hwnd == 0 {
        return None;
    }
    let mut bound: HashMap<u64, String> = HashMap::new();
    for window in observed {
        let key = WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        if member_tokens
            .get(&key)
            .is_some_and(|bound_token| bound_token == &window.token)
        {
            bound.insert(window.hwnd, window.token.clone());
        }
    }
    for row in retained {
        if member_tokens
            .get(&row.key)
            .is_some_and(|bound_token| bound_token == &row.token)
        {
            bound.entry(row.key.hwnd).or_insert(row.token.clone());
        }
    }
    let mut hwnd = hit_hwnd;
    for _ in 0..32 {
        if let Some(token) = bound.get(&hwnd) {
            return Some(WindowId(token.clone()));
        }
        let next = owner_step(hwnd);
        if next == 0 || next == hwnd {
            return None;
        }
        hwnd = next;
    }
    None
}

/// Adapter-side bound on one output identity row. Every text field must fit
/// the core's identity grammar (`MAX_IDENTITY_TEXT_BYTES` UTF-8 bytes); an
/// overbound row is handed to the core as fully unknown rather than
/// truncated. The core holds a screen whose identity text breaks the
/// grammar, so the adapter must never hand it an invalid row (and a
/// truncated string would name a different panel).
fn bounded_identity(identity: OutputIdentity) -> OutputIdentity {
    let fits = |text: &Option<String>| {
        text.as_deref()
            .is_none_or(|value| value.len() <= MAX_IDENTITY_TEXT_BYTES)
    };
    if fits(&identity.manufacturer)
        && fits(&identity.model)
        && fits(&identity.serial)
        && fits(&identity.connector)
    {
        identity
    } else {
        OutputIdentity::unknown()
    }
}

/// Capture one complete world observation from the real owner plus real
/// per-tick products. Read-only over `state`; `observed` is the current
/// tick's enumeration output (`None` when enumeration itself failed, exactly
/// like `TileLoop::observe`: every inventoried scope holds and completeness
/// clears, never a successful empty); `retained`/`areas` are the current
/// tick's products; `foreground_hwnd` is the caller-read live foreground
/// (0 when none). A member that cannot resolve to a verified row holds its
/// domain rather than riding a partial controlled list.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(super) fn capture_world(
    state: &OwnerInputs<'_>,
    observed: Option<&[ObservedWindow]>,
    retained: &[RetainedRow],
    areas: &[MonitorArea],
    foreground_hwnd: u64,
) -> CapturedWorld {
    let enum_failed = observed.is_none();
    let observed: &[ObservedWindow] = observed.unwrap_or(&[]);
    // Liveness for stale-close detection: every member token plus every
    // carried observed/retained token. A queued close observed live again
    // is stale and drops; it never rides as a contradictory live+closed row.
    let mut live: HashSet<&str> = HashSet::new();
    for token in state.member_tokens.values() {
        live.insert(token.as_str());
    }
    for window in observed {
        live.insert(window.token.as_str());
    }
    for row in retained {
        live.insert(row.token.as_str());
    }

    let mut screens: Vec<ScreenReport> = Vec::new();
    let mut held_screens: Vec<HeldScreenRow> = Vec::new();
    let mut facts: Vec<WindowFact> = Vec::new();
    let mut closed_all: Vec<WindowId> = Vec::new();
    let mut held_members: Vec<String> = Vec::new();
    let mut inventory_complete = !enum_failed;
    // An unreadable fact (rect `None`, every native flag/state unknown)
    // holds its staged scope instead of authorizing geometry, never a silent
    // absence: every withheld member below rides one. Lifetime-bound
    // identity (the key's pid, a proved executable, suppression presence)
    // still rides: those are identity, not a frame read. Minimized facts
    // ride separately with no geometry.
    macro_rules! unreadable_fact {
        ($facts:expr, $held:expr, $complete:expr, $token:expr, $key:expr, $executable:expr, $owned:expr) => {{
            $facts.push(window_fact(FactSeed {
                token: $token.as_str(),
                key: $key,
                executable: $executable,
                suppression_owned: $owned,
                hints: None,
                output: None,
                rect: None,
                flags: None,
                live: None,
                host_hidden: None,
                fullscreen: None,
                fullscreen_meta: None,
                maximized: None,
                minimized: None,
            }));
            $held.push($token.to_owned());
            $complete = false;
        }};
    }
    macro_rules! minimized_fact {
        ($facts:expr, $token:expr, $key:expr, $output:expr, $executable:expr, $hints:expr, $owned:expr) => {{
            $facts.push(window_fact(FactSeed {
                token: $token.as_str(),
                key: $key,
                executable: $executable,
                suppression_owned: $owned,
                hints: $hints,
                output: $output,
                rect: None,
                flags: None,
                live: None,
                host_hidden: None,
                fullscreen: None,
                fullscreen_meta: None,
                maximized: None,
                minimized: Some(true),
            }));
        }};
    }
    // Screens ride the live monitor areas in native order plus the
    // mechanical device-to-wire map only. No rings/views/tiling/history:
    // the core mints and owns its logical workspaces outright, and derives
    // physical gaps from the logical settings times the measured scale.
    for area in areas {
        let Some(mapped) = state.output_tokens.get(&area.device).cloned() else {
            // No live wire id: the row is unidentifiable, so it carries no
            // screen row at all (never a fabricated id) and topology is
            // unknown.
            inventory_complete = false;
            continue;
        };
        if area.dpi == 0 {
            // Unmeasured effective scale: hold the identifiable screen
            // narrowly instead of carrying a fabricated scale.
            let diagnostic = RowDiagnostic {
                scope: AuthorityScope::Output(OutputId(mapped.clone())),
                row: RowDiagnosticLane::Screen,
                id: Some(mapped.clone()),
                kind: RowDiagnosticKind::Unreadable,
            };
            held_screens.push(HeldScreenRow {
                output: OutputId(mapped),
                // Index in the *carried* screens array (unmapped devices
                // never occupy a slot): the narrow-hold contract's ordinal.
                ordinal: screens.len(),
                diagnostic,
            });
            inventory_complete = false;
            continue;
        }
        screens.push(ScreenReport {
            output: OutputId(mapped),
            full_rect: Some(area.full),
            work_area: Some(area.work),
            complete: !enum_failed,
            // Effective per-monitor DPI measured at the enumeration seam:
            // adapter units per logical pixel.
            scale: f64::from(area.dpi) / 96.0,
            // Adapter-read physical identity for this exact device; a device
            // the map does not know is explicit unknown, never fabricated.
            // An overbound row is unknown too: the core holds screens whose
            // identity text breaks its grammar, so the adapter fails closed
            // instead of handing over an invalid or truncated row.
            identity: bounded_identity(
                state
                    .monitor_identities
                    .get(&area.device)
                    .cloned()
                    .unwrap_or_else(OutputIdentity::unknown),
            ),
        });
    }
    // Every device the adapter knows a live wire id for, whether or not its
    // scale was readable: close homing keys on the mapping, not the screen
    // row's completeness.
    let screen_outputs: HashSet<String> = areas
        .iter()
        .filter_map(|area| state.output_tokens.get(&area.device).cloned())
        .collect();
    // Output affinity for one frame: live placement geometry, never a
    // policy table. An unknown device (or one with no live wire id) has no
    // affinity: `None`, never the first screen or a placeholder id.
    let affinity_of = |rect: &Rect| -> Option<OutputId> {
        let device = super::directional_resolution::output_for_rect(areas, rect);
        state.output_tokens.get(&device).cloned().map(OutputId)
    };
    // Seed the member-key universe from mechanical tables only: bound
    // members, hidden claims, minimized omissions, and retained rows. New
    // windows enter through the observed lane below.
    let mut keys: BTreeSet<WindowKey> = BTreeSet::new();
    for key in state.member_tokens.keys() {
        keys.insert(key.clone());
    }
    for key in state.hidden_claims.keys() {
        keys.insert(key.clone());
    }
    for key in state.minimized_omitted.iter() {
        keys.insert(key.clone());
    }
    for row in retained {
        keys.insert(row.key.clone());
    }
    if enum_failed {
        // A failed enumeration proves nothing: every bound lifetime rides
        // unreadable with unknown affinity; explicit queued closes still
        // ride beside the hold. Bare absence never becomes a close.
        for key in &keys {
            if let Some(token) = state.member_tokens.get(key) {
                let owned = state.hidden_claims.contains_key(key);
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    key,
                    bound_executable(state, key),
                    owned
                );
            } else {
                inventory_complete = false;
            }
        }
    } else {
        // Visible lane over live observation: every observed window rides a
        // fact (new lifetimes admit through the ordinary path; the core
        // assigns logical homing). Identity/scope/classify gates hold
        // narrowly as unreadable, never as silent absence.
        for window in observed {
            let key = WindowKey {
                hwnd: window.hwnd,
                pid: window.identity.pid,
                creation: window.identity.process_creation.clone(),
            };
            let token = state
                .member_tokens
                .get(&key)
                .cloned()
                .unwrap_or_else(|| window.token.clone());
            if let Some(bound) = state.member_tokens.get(&key)
                && bound != &window.token
            {
                // Recycled lifetime under a stale number: hold, inherit
                // nothing (no executable, no flags).
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    bound,
                    &key,
                    None,
                    state.hidden_claims.contains_key(&key)
                );
                continue;
            }
            let owned = state.hidden_claims.contains_key(&key);
            // Scope fence: stored identity for bound members (drift holds),
            // live identity for brand-new windows.
            let exe_path: &str = match state.member_identity.get(&key) {
                Some(identity)
                    if identity.pid == key.pid && identity.process_creation == key.creation =>
                {
                    identity.exe_path.as_str()
                }
                Some(_) => {
                    // Drifted identity: the row holds with no executable.
                    unreadable_fact!(
                        facts,
                        held_members,
                        inventory_complete,
                        token,
                        &key,
                        None,
                        owned
                    );
                    continue;
                }
                None => window.identity.exe_path.as_str(),
            };
            if !crate::tiling::scope_allows(state.scope, exe_path) {
                // Scoped inventory ignores foreign (out-of-scope,
                // never-member) windows: they are not managed inventory
                // and must not fence scoped commands. A bound member
                // that drifts out of scope is still managed inventory,
                // so it holds narrowly as unreadable (the identity still
                // rides: it is read, only the frame is not).
                if !state.member_tokens.contains_key(&key) {
                    continue;
                }
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    &key,
                    Some(exe_path),
                    owned
                );
                continue;
            }
            if owned {
                // Hidden lane dominates: the claim association must prove
                // before any row, snapshot, or minimized entry.
                let associated = state.hidden_claims.get(&key).is_some_and(|record| {
                    crate::model::valid_claim_tag(&record.claim.tag)
                        && record.claim.hwnd == key.hwnd
                        && record.claim.process.pid == key.pid
                        && record.claim.process.process_creation == key.creation
                }) && state
                    .member_tags
                    .get(&key)
                    .is_some_and(|tag| !tag.is_empty());
                if !associated {
                    unreadable_fact!(
                        facts,
                        held_members,
                        inventory_complete,
                        token,
                        &key,
                        Some(exe_path),
                        owned
                    );
                    continue;
                }
                let record = state
                    .hidden_claims
                    .get(&key)
                    .expect("associated claim exists");
                let hints = state.hint_logged.get(&token).copied();
                if record.iconic
                    || record.claim.show.minimized
                    || state.minimized_omitted.contains(&key)
                {
                    minimized_fact!(
                        facts,
                        token,
                        &key,
                        affinity_of(&window.visible),
                        Some(exe_path),
                        hints,
                        owned
                    );
                    continue;
                }
                // The claim above proves this lifetime is intentionally
                // suppressed: its native visibility is hidden by design,
                // so the visible-row eligibility gate must not turn it
                // unreadable. Keeping the row readable preserves its
                // logical homing (and its workspace's scope) while away.
                let floating = capture_floating(state, &key, true);
                let snapshot = crate::native_guards::hidden_snapshot_rect(
                    floating,
                    state.float_rects.get(&token).copied(),
                    state.member_rects.get(&token).copied(),
                );
                let Some(rect) = snapshot else {
                    unreadable_fact!(
                        facts,
                        held_members,
                        inventory_complete,
                        token,
                        &key,
                        Some(exe_path),
                        owned
                    );
                    continue;
                };
                let maximized = window.facts.maximized || record.claim.show.maximized;
                let fullscreen =
                    window.facts.captionless_fullscreen || state.born_fullscreen.contains(&key);
                facts.push(window_fact(FactSeed {
                    token: token.as_str(),
                    key: &key,
                    executable: Some(exe_path),
                    suppression_owned: owned,
                    hints,
                    output: affinity_of(&rect),
                    rect: Some(rect),
                    flags: Some(&window.facts),
                    live: None,
                    host_hidden: None,
                    fullscreen: Some(fullscreen),
                    fullscreen_meta: state.fullscreen_meta.and_then(|map| map.get(&key)),
                    maximized: Some(maximized),
                    minimized: Some(false),
                }));
                continue;
            }
            if !capture_facts_rideable(&window.facts) {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    &key,
                    Some(exe_path),
                    owned
                );
                continue;
            }
            facts.push(window_fact(FactSeed {
                token: token.as_str(),
                key: &key,
                executable: Some(exe_path),
                suppression_owned: owned,
                hints: state.hint_logged.get(&token).copied(),
                output: affinity_of(&window.visible),
                rect: Some(window.visible),
                flags: Some(&window.facts),
                live: None,
                host_hidden: None,
                fullscreen: Some(window.facts.captionless_fullscreen),
                fullscreen_meta: state.fullscreen_meta.and_then(|map| map.get(&key)),
                maximized: Some(window.facts.maximized),
                minimized: Some(false),
            }));
        }
        // Retained occupancy for bound members with no live row: overlay
        // slots ride their last-known allocation, frameless iconic omission
        // minimizes, everything else holds as unreadable.
        let observed_tokens: HashSet<&str> = observed.iter().map(|w| w.token.as_str()).collect();
        for row in retained {
            if observed_tokens.contains(row.token.as_str()) {
                continue;
            }
            if facts.iter().any(|f| f.window.0 == row.token) {
                continue;
            }
            let key = &row.key;
            // Retained occupancy for windows that never became members is
            // not managed inventory: ignore it without fencing scoped
            // commands. Member rows keep every genuine hold below.
            let Some(live_token) = state.member_tokens.get(key) else {
                continue;
            };
            let owned = state.hidden_claims.contains_key(key);
            let executable = bound_executable(state, key);
            if *live_token != row.token {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    owned
                );
                continue;
            }
            if owned {
                continue;
            }
            if state.minimized_omitted.contains(key)
                && row.rect.is_none()
                && !row.maximized
                && !row.fullscreen
            {
                minimized_fact!(
                    facts,
                    live_token,
                    key,
                    state.member_rects.get(live_token).and_then(affinity_of),
                    executable,
                    state.hint_logged.get(live_token).copied(),
                    owned
                );
                continue;
            }
            let Some(row_facts) = row.facts else {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    owned
                );
                continue;
            };
            if !capture_facts_rideable(&row_facts) {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    owned
                );
                continue;
            }
            let overlay = row.maximized || row.fullscreen;
            // Prefer the live frame this tick observed; the retained tile
            // slot is placement memory and only stands in when the row has
            // no live frame. Neither present keeps the unreadable hold.
            let rect = row
                .rect
                .or_else(|| state.member_rects.get(live_token).copied());
            let Some(rect) = rect else {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    owned
                );
                continue;
            };
            let floating = capture_floating(state, key, false);
            let hints = crate::tiling::retained_overlay_hint(
                state.hint_logged.get(live_token).copied(),
                true,
                overlay,
                floating,
                state.born_fullscreen.contains(key),
                true,
            );
            // The freshly validated native read rides this row's flags lane:
            // the overlay row is readable and its cloak/ownership/style
            // facts are current, not stale.
            facts.push(window_fact(FactSeed {
                token: live_token.as_str(),
                key,
                executable,
                suppression_owned: owned,
                hints: Some(hints),
                output: affinity_of(&rect),
                rect: Some(rect),
                flags: Some(&row_facts),
                live: None,
                host_hidden: None,
                fullscreen: Some(row.fullscreen),
                fullscreen_meta: state.fullscreen_meta.and_then(|map| map.get(key)),
                maximized: Some(row.maximized),
                minimized: Some(false),
            }));
        }
        // Hidden-claim members with no live row: snapshot or minimized lanes.
        // Orphaned claims (no member token) are not managed inventory:
        // ignore them without fencing scoped commands.
        for (key, record) in state.hidden_claims {
            if facts.iter().any(|f| {
                state
                    .member_tokens
                    .get(key)
                    .is_some_and(|t| t == &f.window.0)
            }) {
                continue;
            }
            let Some(live_token) = state.member_tokens.get(key) else {
                continue;
            };
            let executable = bound_executable(state, key);
            let associated = crate::model::valid_claim_tag(&record.claim.tag)
                && record.claim.hwnd == key.hwnd
                && record.claim.process.pid == key.pid
                && record.claim.process.process_creation == key.creation
                && state
                    .member_tags
                    .get(key)
                    .is_some_and(|tag| !tag.is_empty());
            if !associated {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    true
                );
                continue;
            }
            if record.iconic || record.claim.show.minimized || state.minimized_omitted.contains(key)
            {
                minimized_fact!(
                    facts,
                    live_token,
                    key,
                    state.member_rects.get(live_token).and_then(affinity_of),
                    executable,
                    state.hint_logged.get(live_token).copied(),
                    true
                );
                continue;
            }
            // T4: the durable claim proves project suppression, never host
            // hiding. The lightweight live read is the only source of this
            // row's shown state, DWM cloak, ownership, tool-window bit, and
            // dialog kind; with no read the scope holds instead of
            // fabricating a native verdict.
            let Some(read) = state.hidden_reads.and_then(|reads| reads.get(key)) else {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    true
                );
                continue;
            };
            let floating = capture_floating(state, key, true);
            let snapshot = crate::native_guards::hidden_snapshot_rect(
                floating,
                state.float_rects.get(live_token).copied(),
                state.member_rects.get(live_token).copied(),
            );
            let Some(rect) = snapshot else {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    key,
                    executable,
                    true
                );
                continue;
            };
            // Hidden fullscreen truth: born holds plus the retained overlay
            // flags for the same lifetime (a hidden ordinary fullscreen keeps
            // its pre-hide frame state through the retained row; the durable
            // claim preimage carries maximized only). Maximized unions the
            // durable preimage with the retained flags the same way.
            let retained_overlay = retained
                .iter()
                .find(|row| row.key == *key && row.token == *live_token)
                .map(|row| (row.fullscreen, row.maximized))
                .unwrap_or((false, false));
            let maximized = record.claim.show.maximized || retained_overlay.1;
            let fullscreen = state.born_fullscreen.contains(key) || retained_overlay.0;
            // Recorded constraint or explicit unknown: absence stays `None`,
            // never a fabricated empty hints stand-in.
            let hints = state.hint_logged.get(live_token).copied();
            // No live observed row this tick: the claim lane's lightweight
            // live read carries the native shown state, cloak, ownership,
            // tool-window bit, and dialog kind, while the preimage-proven
            // overlays still ride.
            facts.push(window_fact(FactSeed {
                token: live_token.as_str(),
                key,
                executable,
                suppression_owned: true,
                hints,
                output: affinity_of(&rect),
                rect: Some(rect),
                flags: None,
                live: Some(read),
                host_hidden: None,
                fullscreen: Some(fullscreen),
                fullscreen_meta: state.fullscreen_meta.and_then(|map| map.get(key)),
                maximized: Some(maximized),
                minimized: Some(false),
            }));
        }
        // Iconic omission without any row minimizes with no frame.
        // Omission keys that never became members are not managed
        // inventory: ignore them without fencing scoped commands.
        for key in state.minimized_omitted.iter() {
            if facts.iter().any(|f| {
                state
                    .member_tokens
                    .get(key)
                    .is_some_and(|t| t == &f.window.0)
            }) {
                continue;
            }
            let Some(live_token) = state.member_tokens.get(key) else {
                continue;
            };
            minimized_fact!(
                facts,
                live_token,
                key,
                state.member_rects.get(live_token).and_then(affinity_of),
                bound_executable(state, key),
                state.hint_logged.get(live_token).copied(),
                state.hidden_claims.contains_key(key)
            );
        }
        // Bound members with no live, retained, hidden, or minimized row
        // hold as unreadable: a successful enumeration that omits a bound
        // lifetime proves nothing (never an implicit close, never silent
        // absence). Verified closes ride the close lane below, never a
        // fact, so queued closes skip this hold.
        for key in &keys {
            if facts.iter().any(|f| {
                state
                    .member_tokens
                    .get(key)
                    .is_some_and(|t| t == &f.window.0)
            }) {
                continue;
            }
            let Some(live_token) = state.member_tokens.get(key) else {
                continue;
            };
            if state.pending_closed.contains_key(live_token.as_str()) {
                continue;
            }
            unreadable_fact!(
                facts,
                held_members,
                inventory_complete,
                live_token,
                key,
                bound_executable(state, key),
                state.hidden_claims.contains_key(key)
            );
        }
    }
    // Explicit closes ride even beside a hold: a queued verified close is
    // honored inside incomplete inventory; only implicit missing=closed
    // inference needs a provably complete list, which this capture never
    // performs.
    for token in state.pending_closed.keys() {
        if live.contains(token.as_str()) {
            continue;
        }
        closed_all.push(WindowId(token.clone()));
    }

    facts.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    closed_all.sort_by(|a, b| a.0.cmp(&b.0));
    held_members.sort();
    held_members.dedup();

    let mut dropped_stale_closed: Vec<String> = Vec::new();
    let mut unhomed_closed: Vec<String> = Vec::new();
    for (token, homing) in state.pending_closed {
        if live.contains(token.as_str()) {
            dropped_stale_closed.push(token.clone());
            continue;
        }
        // Unhomed when the recorded device has no live wire id: the close
        // stays pending with the caller and rides no report.
        let homed = state
            .output_tokens
            .get(&homing.output)
            .is_some_and(|id| screen_outputs.contains(id));
        if !homed {
            unhomed_closed.push(token.clone());
        }
    }
    dropped_stale_closed.sort();
    dropped_stale_closed.dedup();
    unhomed_closed.sort();
    unhomed_closed.dedup();
    let unhomed_set: HashSet<&str> = unhomed_closed.iter().map(String::as_str).collect();
    let closed: Vec<WindowId> = closed_all
        .into_iter()
        .filter(|id| !unhomed_set.contains(id.0.as_str()))
        .collect();

    // Full rectangles plus work areas are the hotplug placement facts
    // (never a frame proxy) and the host-designated primary flag.
    // `all_monitors` orders primary-first, so primary derives from the
    // `areas.first()` device: managed output order may be reordered
    // relative to the native list and must never decide it.
    let primary_output = areas
        .first()
        .and_then(|area| state.output_tokens.get(&area.device))
        .cloned()
        .map(OutputId);
    let inventory_complete = inventory_complete
        && !screens.is_empty()
        && !facts
            .iter()
            .any(|fact| fact.rect.is_none() && fact.minimized != Some(true));
    // Facts are the whole window lane: v4 has no adapter receipt or
    // workspace projection to attach.
    let windows = facts;
    // Canonical intent rows for observed exact member lifetimes: the key
    // must still be bound and its WindowFact must exist in this capture.
    // Sticky dominates (its marker already implies float); an explicit
    // float/tile marker rides alone; all-absent storage publishes nothing.
    // Unreadable storage has no map entry, so it never fabricates a record.
    // Deduped by token (first bound lifetime wins).
    let mut intent_records: Vec<IntentRecord> = Vec::new();
    let mut intent_seen: HashSet<&str> = HashSet::new();
    for (key, token) in state.member_tokens {
        if !windows.iter().any(|fact| fact.window.0 == *token) {
            continue;
        }
        let Some(storage) = state.intent_storage.and_then(|map| map.get(key)) else {
            continue;
        };
        let data = if let Some(prior) = storage.sticky_prior {
            IntentData::new(Some(true), Some(prior))
        } else if storage.float {
            IntentData::new(Some(true), None)
        } else if storage.tile {
            IntentData::new(Some(false), None)
        } else {
            continue;
        };
        if !intent_seen.insert(token.as_str()) {
            continue;
        }
        intent_records.push(IntentRecord {
            win: WindowId(token.clone()),
            data: tiler_core::intent_data::encode_intent_data(&data),
        });
    }
    // Neutral migration/control action context (core-owned policy):
    // the live foreground frame grounded to a live device and mapped to
    // wire, the adapter-retained cache mapped to wire, and the ordered
    // known-output registry (sorted native keys, including disconnected
    // mappings) translated to wire. Same rectangle rule as observed
    // affinity, including unmanaged windows; unreadable frames and
    // unmapped devices stay unknown. The core filters every leg to the
    // registry with first-entry fallback and refuses when nothing
    // resolves. Never an adapter decision.
    let action_foreground = if foreground_hwnd == 0 {
        None
    } else {
        crate::ffi::window_rect(foreground_hwnd)
            .and_then(rect_from_win)
            .and_then(|rect| foreground_output_for_rect(areas, &rect, state.output_tokens))
    };
    let action_cached = if state.active_output.is_empty() {
        None
    } else {
        state
            .output_tokens
            .get(state.active_output)
            .cloned()
            .map(OutputId)
    };
    let mut known: Vec<&String> = state.output_tokens.keys().collect();
    known.sort();
    let action_outputs: Vec<OutputId> = known
        .into_iter()
        .filter_map(|device| state.output_tokens.get(device))
        .cloned()
        .map(OutputId)
        .collect();
    let presentation = presentation_ingress(state, Some(observed), retained, foreground_hwnd);
    // The observed foreground frame's exact output, joined to the
    // foreground WindowFact: a resolved frame that disagrees with the fact
    // proves nothing (the frame and the fact are not the same observation),
    // so it fails closed and the core falls back to the fact alone.
    let foreground_output = if presentation.foreground.is_empty() {
        None
    } else {
        let fact_output = windows
            .iter()
            .find(|fact| fact.window.0 == presentation.foreground)
            .and_then(|fact| fact.output.clone());
        let raw = presentation
            .foreground_frame
            .and_then(|rect| foreground_output_for_rect(areas, &rect, state.output_tokens));
        match (raw, fact_output) {
            (Some(raw), Some(fact)) if raw != fact => None,
            (raw, _) => raw,
        }
    };
    CapturedWorld {
        screens,
        held_screens,
        windows,
        intent_records,
        closed,
        // One foreground sample drives both fields: the presentation
        // ingress already resolved the tick's observed row when no richer
        // presentation facts were supplied.
        focused_global: presentation.foreground.clone(),
        // The caller's foreground read is the proof: `0` positively saw no
        // foreground window (known, empty id); a nonzero handle is known
        // only when this tick resolves it to a verified lifetime.
        foreground_known: foreground_hwnd == 0 || !presentation.foreground.is_empty(),
        // Snapshot-only capture: no event-coverage interval exists here.
        focus_changes: None,
        cursor: state.cursor,
        // The hit resolves only when the cursor itself was sampled: the
        // game lane and unreadable ticks carry no cursor and no hit.
        cursor_hit: match (state.cursor, state.hit_hwnd) {
            (Some(_), Some(hwnd)) => {
                resolve_cursor_hit(hwnd, observed, retained, state.member_tokens, &|step| {
                    let parent = crate::ffi::parent_of(step);
                    if parent != 0 {
                        parent
                    } else {
                        crate::ffi::owner_of(step)
                    }
                })
            }
            _ => None,
        },
        gesture_barrier: !state.active.is_empty(),
        gesture: state.gesture.clone(),
        action_foreground,
        action_cached,
        action_outputs,
        primary_output,
        inventory_complete,
        dropped_stale_closed,
        #[cfg(test)]
        unhomed_closed,
        held_members,
        switch_target: state
            .settings_live
            .as_ref()
            .map(|live| live.settings.core.switch_target_mode())
            .unwrap_or_default(),
        fresh_placement: state
            .settings_live
            .as_ref()
            .map(|live| live.settings.core.fresh_placement_mode())
            .unwrap_or_default(),
        focus_follows_cursor: state
            .settings_live
            .as_ref()
            .is_some_and(|live| live.settings.core.focus_follows_cursor),
        cursor_warp: state
            .settings_live
            .as_ref()
            .is_some_and(|live| live.settings.core.cursor_warp),
        foreground_output,
        presentation,
    }
}

#[cfg(test)]
mod foreground_output_tests {
    use super::super::rmax03_adapter_tests::test_state;
    use super::super::{MonitorArea, ObservedWindow};
    use super::capture_world;
    use tiler_core::directional::OutputId;
    use tiler_core::geometry::Rect;

    fn area(device: &str, x: i32) -> MonitorArea {
        MonitorArea {
            device: device.to_owned(),
            work: Rect {
                x,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x,
                y: 0,
                w: 800,
                h: 600,
            },
            dpi: 96,
        }
    }

    fn areas() -> Vec<MonitorArea> {
        vec![area("DEV-A", 0), area("DEV-B", 800)]
    }

    fn facts() -> crate::tiling::WindowFacts {
        crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            topmost: false,
            dialog: false,
        }
    }

    fn observed_window(hwnd: u64, token: &str, pid: u32, creation: &str, x: i32) -> ObservedWindow {
        let visible = Rect {
            x,
            y: 0,
            w: 40,
            h: 90,
        };
        let outer = Rect {
            x: x - 8,
            y: -8,
            w: 56,
            h: 106,
        };
        ObservedWindow {
            hwnd,
            token: token.to_owned(),
            outer,
            visible,
            insets: crate::tiling::FrameInsets::measure(outer, visible),
            identity: super::super::ObservedTarget {
                hwnd,
                pid,
                process_creation: creation.to_owned(),
                exe_path: "C:\\Apps\\app.exe".to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                tag: format!("tag-{token}"),
            },
            facts: facts(),
            member_tag: Some(format!("tag-{token}")),
        }
    }

    fn foreground(token: &str, frame: Option<Rect>) -> super::PresentationForeground {
        super::PresentationForeground {
            token: token.to_owned(),
            minimized: false,
            maximized: false,
            fullscreen: false,
            host_hidden: false,
            hidden_workspace: false,
            frame,
        }
    }

    #[test]
    fn resolved_foreground_frame_names_the_wire_output() {
        let mut state = test_state();
        state.ensure_device("DEV-A");
        state.ensure_device("DEV-B");
        let areas = areas();
        let observed = vec![observed_window(101, "win-fg", 201, "proc-fg", 900)];
        let mut inputs = state.inputs();
        inputs.presentation_foreground = Some(foreground(
            "win-fg",
            Some(Rect {
                x: 900,
                y: 0,
                w: 40,
                h: 90,
            }),
        ));
        let captured = capture_world(&inputs, Some(&observed), &[], &areas, 101);
        let expected = OutputId(state.wire_of("DEV-B").expect("DEV-B carries a wire id"));
        assert_eq!(captured.foreground_output, Some(expected));
    }

    #[test]
    fn unresolvable_foreground_rect_stays_unknown() {
        let mut state = test_state();
        state.ensure_device("DEV-A");
        state.ensure_device("DEV-B");
        let areas = areas();
        let observed = vec![observed_window(101, "win-fg", 201, "proc-fg", 0)];
        let mut inputs = state.inputs();
        // Outside every carried area: the frame proves no output. The
        // foreground WindowFact still rides its own affinity separately.
        inputs.presentation_foreground = Some(foreground(
            "win-fg",
            Some(Rect {
                x: 5000,
                y: 0,
                w: 40,
                h: 90,
            }),
        ));
        let captured = capture_world(&inputs, Some(&observed), &[], &areas, 101);
        assert!(captured.foreground_output.is_none());
    }

    #[test]
    fn fact_and_raw_disagreement_fails_closed() {
        let mut state = test_state();
        state.ensure_device("DEV-A");
        state.ensure_device("DEV-B");
        let areas = areas();
        // The observed row (and so its WindowFact) sits on DEV-B while the
        // presentation frame names DEV-A: neither proves the same output.
        let observed = vec![observed_window(101, "win-fg", 201, "proc-fg", 900)];
        let mut inputs = state.inputs();
        inputs.presentation_foreground = Some(foreground(
            "win-fg",
            Some(Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 90,
            }),
        ));
        let captured = capture_world(&inputs, Some(&observed), &[], &areas, 101);
        assert!(captured.foreground_output.is_none());
    }
}

#[cfg(test)]
mod tests {
    /// Adapter identity side-map capture: a screen carries exactly the
    /// adapter-read identity for its own device, and an absent key is
    /// explicit unknown.
    mod identity_capture_tests {
        use super::super::super::MonitorArea;
        use super::super::super::rmax03_adapter_tests::test_state;
        use super::super::capture_world;
        use tiler_core::geometry::Rect;
        use tiler_core::world_report::{MAX_IDENTITY_TEXT_BYTES, OutputIdentity};

        fn area(device: &str, x: i32) -> MonitorArea {
            MonitorArea {
                device: device.to_owned(),
                work: Rect {
                    x,
                    y: 0,
                    w: 800,
                    h: 560,
                },
                full: Rect {
                    x,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                dpi: 96,
            }
        }

        fn areas() -> Vec<MonitorArea> {
            vec![area("DEV-A", 0), area("DEV-B", 800)]
        }

        fn identity(
            manufacturer: &str,
            model: &str,
            serial: &str,
            connector: &str,
        ) -> OutputIdentity {
            OutputIdentity {
                manufacturer: Some(manufacturer.to_owned()),
                model: Some(model.to_owned()),
                serial: Some(serial.to_owned()),
                connector: Some(connector.to_owned()),
            }
        }

        /// Identity carried on the screen row for one live device (the
        /// capture keys screens by wire output id, so the wire id must be
        /// read before the seam borrows the state).
        fn screen_identity(state: &super::super::super::TileLoop, device: &str) -> OutputIdentity {
            let wire = state.wire_of(device).expect("device carries a wire id");
            let captured = capture_world(&state.inputs(), Some(&[]), &[], &areas(), 0);
            let screen = captured
                .screens
                .iter()
                .find(|screen| screen.output.0 == wire)
                .expect("device rides a screen row");
            screen.identity.clone()
        }

        #[test]
        fn identity_rides_the_device_side_map() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            state.ensure_device("DEV-B");
            let expected = identity("Acme", "Panel-9000", "SN-123", "DP-1");
            state
                .monitor_identities
                .insert("DEV-A".to_owned(), expected.clone());

            assert_eq!(screen_identity(&state, "DEV-A"), expected);
            assert_eq!(screen_identity(&state, "DEV-B"), OutputIdentity::unknown());
        }

        #[test]
        fn identity_missing_from_the_map_is_unknown() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            state.ensure_device("DEV-B");

            assert_eq!(screen_identity(&state, "DEV-A"), OutputIdentity::unknown());
            assert_eq!(screen_identity(&state, "DEV-B"), OutputIdentity::unknown());
        }

        #[test]
        fn identity_overbound_text_fails_closed() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            state.ensure_device("DEV-B");
            let overbound = "m".repeat(MAX_IDENTITY_TEXT_BYTES + 1);
            state.monitor_identities.insert(
                "DEV-A".to_owned(),
                identity("Acme", &overbound, "SN-123", "DP-1"),
            );

            // The adapter never hands the core a row its identity grammar
            // must hold, and never truncates one into a different panel.
            assert_eq!(screen_identity(&state, "DEV-A"), OutputIdentity::unknown());
        }

        #[test]
        fn identity_text_boundary_is_exactly_256_bytes() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            state.ensure_device("DEV-B");
            let at_bound = "m".repeat(MAX_IDENTITY_TEXT_BYTES);
            state.monitor_identities.insert(
                "DEV-A".to_owned(),
                identity("Acme", &at_bound, "SN-123", "DP-1"),
            );
            let carried = screen_identity(&state, "DEV-A");
            assert_eq!(carried.model.as_deref(), Some(at_bound.as_str()));

            // Any one field over the bound turns the whole row unknown.
            let overbound = "m".repeat(MAX_IDENTITY_TEXT_BYTES + 1);
            state.monitor_identities.insert(
                "DEV-A".to_owned(),
                identity("Acme", "Panel-9000", "SN-123", &overbound),
            );
            assert_eq!(screen_identity(&state, "DEV-A"), OutputIdentity::unknown());
        }

        #[test]
        fn identity_entry_for_absent_device_is_ignored() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            state.ensure_device("DEV-B");
            state.monitor_identities.insert(
                "DEV-GONE".to_owned(),
                identity("Acme", "Panel-9000", "SN-123", "DP-1"),
            );

            assert_eq!(screen_identity(&state, "DEV-A"), OutputIdentity::unknown());
            assert_eq!(screen_identity(&state, "DEV-B"), OutputIdentity::unknown());
        }
    }

    /// Retained-row truth and presentation ingress regressions: overlays ride
    /// their fresh frame and flags, hidden claims keep absent hints absent,
    /// and a fullscreen foreground never blanks other windows' presentation.
    mod retained_and_presentation_tests {
        use super::super::super::rmax03_adapter_tests::test_state;
        use super::super::super::{
            HiddenRecord, MonitorArea, ObservedTarget, ObservedWindow, RetainedRow,
        };
        use super::super::{HiddenClaimRead, PresentationForeground, capture_world};
        use crate::model::{ProcessIdentity, WindowIdentity};
        use crate::native_guards::WindowKey;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_report::WindowType;

        fn area(device: &str) -> MonitorArea {
            MonitorArea {
                device: device.to_owned(),
                work: Rect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1040,
                },
                full: Rect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
                dpi: 96,
            }
        }

        fn process(pid: u32, creation: &str) -> ProcessIdentity {
            ProcessIdentity {
                pid,
                process_creation: creation.to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                exe_path: "C:\\Apps\\app.exe".to_owned(),
            }
        }

        fn key(hwnd: u64, pid: u32, creation: &str) -> WindowKey {
            WindowKey {
                hwnd,
                pid,
                creation: creation.to_owned(),
            }
        }

        fn facts(overlay: bool) -> crate::tiling::WindowFacts {
            crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: overlay,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                topmost: false,
                dialog: false,
            }
        }

        fn observed(hwnd: u64, token: &str, pid: u32, creation: &str) -> ObservedWindow {
            let visible = Rect {
                x: 10,
                y: 10,
                w: 300,
                h: 200,
            };
            let outer = Rect {
                x: 2,
                y: 2,
                w: 316,
                h: 216,
            };
            ObservedWindow {
                hwnd,
                token: token.to_owned(),
                outer,
                visible,
                insets: crate::tiling::FrameInsets::measure(outer, visible),
                identity: ObservedTarget {
                    hwnd,
                    pid,
                    process_creation: creation.to_owned(),
                    exe_path: "C:\\Apps\\app.exe".to_owned(),
                    user_sid: "S-1-5-21-1000".to_owned(),
                    session_id: 1,
                    tag: "0123456789abcdef".to_owned(),
                },
                facts: facts(false),
                member_tag: Some("0123456789abcdef".to_owned()),
            }
        }

        #[test]
        fn retained_overlay_rides_live_frame_and_flags() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            let key = key(501, 601, "proc-501");
            state.member_tokens.insert(key.clone(), "w-overlay".to_owned());
            // Stale adapter tile slot: the live retained frame must win.
            state.member_rects.insert(
                "w-overlay".to_owned(),
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            );
            let live = Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            };
            let retained = vec![RetainedRow {
                key,
                token: "w-overlay".to_owned(),
                rect: Some(live),
                maximized: true,
                fullscreen: false,
                facts: Some(facts(true)),
            }];
            let captured = capture_world(&state.inputs(), Some(&[]), &retained, &[area("DEV-A")], 0);
            let fact = captured
                .windows
                .iter()
                .find(|fact| fact.window.0 == "w-overlay")
                .expect("retained overlay rides a fact row");
            assert_eq!(fact.rect, Some(live), "live frame wins over the tile slot");
            assert_eq!(fact.tool_window, Some(false), "flags lane is published");
            assert_eq!(fact.owned, Some(false), "flags lane is published");
            assert_eq!(fact.window_type, Some(WindowType::Normal));
            assert_eq!(fact.host_hidden, Some(false), "flags-derived cloak");
        }

        #[test]
        fn hidden_claim_without_hint_stays_unknown() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            let key = key(701, 801, "proc-701");
            state.member_tokens.insert(key.clone(), "w-hidden".to_owned());
            state.member_tags.insert(key.clone(), "0123456789abcdef".to_owned());
            state.member_rects.insert(
                "w-hidden".to_owned(),
                Rect {
                    x: 20,
                    y: 20,
                    w: 400,
                    h: 300,
                },
            );
            state.hidden_claims.insert(
                key.clone(),
                HiddenRecord {
                    claim: WindowIdentity {
                        hwnd: key.hwnd,
                        process: process(key.pid, &key.creation),
                        tag: "0123456789abcdef".to_owned(),
                        kind: crate::model::WindowClaimKind::Helper,
                        show: crate::model::WindowShowState::default(),
                    },
                    iconic: false,
                },
            );
            let mut inputs = state.inputs();
            let reads = std::collections::BTreeMap::from([(
                key.clone(),
                HiddenClaimRead {
                    visible: false,
                    cloaked: false,
                    owned: false,
                    tool_window: false,
                    topmost: false,
                    dialog: false,
                },
            )]);
            inputs.hidden_reads = Some(&reads);
            let captured = capture_world(&inputs, Some(&[]), &[], &[area("DEV-A")], 0);
            let fact = captured
                .windows
                .iter()
                .find(|fact| fact.window.0 == "w-hidden")
                .expect("hidden claim rides a fact row");
            assert!(fact.hints.is_none(), "absent hint entry stays unknown");
        }

        #[test]
        fn hidden_claim_with_recorded_hint_carries_it() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            let key = key(702, 802, "proc-702");
            state.member_tokens.insert(key.clone(), "w-hidden2".to_owned());
            state.member_tags.insert(key.clone(), "0123456789abcdef".to_owned());
            state.member_rects.insert(
                "w-hidden2".to_owned(),
                Rect {
                    x: 20,
                    y: 20,
                    w: 400,
                    h: 300,
                },
            );
            let hints = WindowSizeHints {
                min_w: Some(320),
                min_h: Some(240),
                max_w: None,
                max_h: None,
            };
            state.hint_logged.insert("w-hidden2".to_owned(), hints);
            state.hidden_claims.insert(
                key.clone(),
                HiddenRecord {
                    claim: WindowIdentity {
                        hwnd: key.hwnd,
                        process: process(key.pid, &key.creation),
                        tag: "0123456789abcdef".to_owned(),
                        kind: crate::model::WindowClaimKind::Helper,
                        show: crate::model::WindowShowState::default(),
                    },
                    iconic: false,
                },
            );
            let mut inputs = state.inputs();
            let reads = std::collections::BTreeMap::from([(
                key.clone(),
                HiddenClaimRead {
                    visible: false,
                    cloaked: false,
                    owned: false,
                    tool_window: false,
                    topmost: false,
                    dialog: false,
                },
            )]);
            inputs.hidden_reads = Some(&reads);
            let captured = capture_world(&inputs, Some(&[]), &[], &[area("DEV-A")], 0);
            let fact = captured
                .windows
                .iter()
                .find(|fact| fact.window.0 == "w-hidden2")
                .expect("hidden claim rides a fact row");
            assert_eq!(fact.hints, Some(hints), "recorded hint still rides");
        }

        #[test]
        fn above_fact_comes_from_live_topmost_reads() {
            // The observed lane publishes its fresh `WindowFacts` bit, never
            // a default `None` while that read exists.
            for topmost in [false, true] {
                let mut state = test_state();
                state.ensure_device("DEV-A");
                let mut row = observed(103, "w-flag-top", 203, "proc-flag");
                row.facts.topmost = topmost;
                let captured = capture_world(
                    &state.inputs(),
                    Some(&[row]),
                    &[],
                    &[area("DEV-A")],
                    0,
                );
                let fact = captured
                    .windows
                    .iter()
                    .find(|fact| fact.window.0 == "w-flag-top")
                    .expect("observed row rides a fact row");
                assert_eq!(fact.above, Some(topmost), "observed topmost bit rides");
            }

            // The hidden-claim lane publishes the same bit from its
            // lightweight live read when no observed row exists.
            for topmost in [false, true] {
                let mut state = test_state();
                state.ensure_device("DEV-A");
                let claim_key = key(704, 804, "proc-704");
                state
                    .member_tokens
                    .insert(claim_key.clone(), "w-claim-top".to_owned());
                state
                    .member_tags
                    .insert(claim_key.clone(), "0123456789abcdef".to_owned());
                state.member_rects.insert(
                    "w-claim-top".to_owned(),
                    Rect {
                        x: 20,
                        y: 20,
                        w: 400,
                        h: 300,
                    },
                );
                state.hidden_claims.insert(
                    claim_key.clone(),
                    HiddenRecord {
                        claim: WindowIdentity {
                            hwnd: claim_key.hwnd,
                            process: process(claim_key.pid, &claim_key.creation),
                            tag: "0123456789abcdef".to_owned(),
                            kind: crate::model::WindowClaimKind::Helper,
                            show: crate::model::WindowShowState::default(),
                        },
                        iconic: false,
                    },
                );
                let mut inputs = state.inputs();
                let reads = std::collections::BTreeMap::from([(
                    claim_key.clone(),
                    HiddenClaimRead {
                        visible: false,
                        cloaked: false,
                        owned: false,
                        tool_window: false,
                        topmost,
                        dialog: false,
                    },
                )]);
                inputs.hidden_reads = Some(&reads);
                let captured = capture_world(&inputs, Some(&[]), &[], &[area("DEV-A")], 0);
                let fact = captured
                    .windows
                    .iter()
                    .find(|fact| fact.window.0 == "w-claim-top")
                    .expect("hidden claim rides a fact row");
                assert_eq!(fact.above, Some(topmost), "hidden topmost bit rides");
            }
        }

        #[test]
        fn fullscreen_foreground_does_not_blank_other_outputs() {
            let mut state = test_state();
            state.ensure_device("DEV-A");
            let observed = vec![
                observed(101, "win-a", 201, "proc-a"),
                observed(102, "win-b", 202, "proc-b"),
            ];
            let mut inputs = state.inputs();
            inputs.presentation_foreground = Some(PresentationForeground {
                token: "win-a".to_owned(),
                minimized: false,
                maximized: false,
                fullscreen: true,
                host_hidden: false,
                hidden_workspace: false,
                frame: Some(Rect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1080,
                }),
            });
            let captured = capture_world(&inputs, Some(&observed), &[], &[area("DEV-A")], 101);
            // Suspension is core-owned: capture still publishes the truthful
            // ingress for every other window instead of default-empty rows.
            let order: Vec<&str> = captured
                .presentation
                .pager_order
                .iter()
                .map(|row| row.0.as_str())
                .collect();
            assert_eq!(order, vec!["win-a", "win-b"]);
            assert_eq!(captured.presentation.pager_frames.len(), 2);
            assert_eq!(
                captured.presentation.pager_frames[0].live,
                Some(observed[0].visible)
            );
            assert_eq!(
                captured.presentation.pager_frames[1].live,
                Some(observed[1].visible)
            );
        }
    }
}
