#![forbid(unsafe_code)]
//! Native host-diff facade for the single world owner.
//!
//! Actuator over typed [`WorldReply`](tiler_core::world_report::WorldReply)
//! facts against fresh native observations. All setters and proof re-reads
//! go through [`NativeOps`], implemented by the production `LiveNative` and
//! an in-memory fake in offline tests. Geometry converges from the core reply,
//! never from a second policy: this module honors per-row authority, scope
//! authority, holds, transfers, views, migrations, and one-shot effects
//! exactly as the reply states them. Workspace lifecycle needs no
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
//!   (setter disposition plus immediate pre/post proof). Creation, removal,
//!   and effect outcomes are proven by read-back and counted in stats;
//!   float placements ride the raw-attempt lane with their stable operation
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
//!   migration/effect writes.
//! - Correlated raw attempts (transfers, float placements, views) and
//!   one-shot effects arm BEFORE the first native setter and never repeat
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

use super::world_capture::{CapturedWorld, OwnerInputs, capture_facts_rideable};
use super::{ObservedWindow, RetainedRow};
use crate::native_guards::WindowKey;
use crate::tiling::WindowFacts;
use std::collections::{HashMap, HashSet};
use tiler_core::directional::OutputId;
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    ClearAttempt, EffectKind, FrameAttempt, FrameSetter, HoldReason, MemberAttempt, ScopeAuthority,
    TransferKind, TransferTarget, ViewAttempt, WorldReply,
};

/// Fresh offline window facts for one managed lifetime: the observation-time
/// baseline plus the shape of every fresh re-read through [`NativeOps`].
///
/// Each flag has a distinct native source, never a guessed duplicate:
/// - `key`/`token`: stable member identity and Engine token.
/// - `frame`: last proven frame (`None` = unreadable, never fabricated).
///   Retained maximized/fullscreen overlay rows ride their tile slot here
///   (allocation reasoning and home affinity), not the live maximized frame.
/// - `observed_frame`: observation-time live frame before that slot
///   substitution (the retained row's fresh frame for overlay rows with
///   observed evidence, the observed visible frame for live-lane rows).
///   `None` means no observed live frame this tick and never falls back to
///   the slot: a coincidental slot/live equality must not fabricate proof.
///   The maximize-clear race guard compares the diff-time live re-read
///   against this, never the slot: the production live lane re-observes
///   the maximized DWM frame through `observe_window`, so comparing
///   against the slot would hold every maximized clear forever with no
///   user race present.
/// - `facts`: the raw native read (`None` = unproven; holds). Distinct from
///   `iconic`/`hidden`: `facts.minimized` is the native minimized bit while
///   `iconic` is the adapter minimized-omission classification; `facts.visible`
///   is native visibility while `hidden` is the adapter hide-ledger state.
/// - `output`/`workspace`: homing at observation time.
/// - `iconic`: adapter minimized omission or explicit minimized entry.
/// - `hidden`: adapter hide-ledger membership.
/// - `sticky`/`floating`/`fit_excluded`: adapter intent lanes from the
///   accepted capture rows (float placement intent never authorizes tiled
///   writes; only a stable `frame_operation` token does).
/// - `live_tag`: live visible-membership tag; must equal the stored tag and
///   be nonempty, mirroring the production visible-lifetime gate.
#[derive(Debug, Clone)]
pub(super) struct FreshWindow {
    pub(super) key: WindowKey,
    pub(super) token: String,
    pub(super) frame: Option<Rect>,
    pub(super) observed_frame: Option<Rect>,
    pub(super) facts: Option<WindowFacts>,
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) iconic: bool,
    pub(super) hidden: bool,
    pub(super) sticky: bool,
    pub(super) floating: bool,
    pub(super) fit_excluded: bool,
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
        // Which capture lane each fact rode: the live lane over the current
        // enumeration, or the retained lane for members with no live row.
        // The retained overlay lane substitutes the tile slot for `rect`;
        // the race baseline below keeps the pre-substitution live frame.
        let observed_tokens: HashSet<&str> = observed
            .iter()
            .map(|window| window.token.as_str())
            .collect();
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
            let iconic = fact.minimized || state.minimized_omitted.contains(key);
            // Observation-time live frame before retained-slot substitution:
            // retained overlay rows carry their fresh live frame in the
            // retained row while `rect` rides the tile slot. Live-lane rows
            // observe no substitution, so the fact rect is already the live
            // frame. A retained row with no observed frame yields `None`:
            // unavailable evidence is never backfilled from the slot, so a
            // coincidental slot/live equality cannot fabricate proof.
            let observed_frame = if observed_tokens.contains(token) {
                fact.rect
            } else {
                retained
                    .iter()
                    .find(|row| row.token == token)
                    .and_then(|row| row.rect)
            };
            // Observation-time homing: the retained proven receipt where
            // capture freshly revalidated it this tick, else the live
            // screen affinity with no workspace claim.
            let (output, workspace) = match &fact.observed_workspace {
                Some(proven_ws) => state
                    .applied_homing
                    .get(token)
                    .filter(|(_, ws)| ws == proven_ws)
                    .cloned()
                    .unwrap_or_else(|| (fact.output.0.clone(), proven_ws.clone())),
                None => (fact.output.0.clone(), String::new()),
            };
            baseline.insert(
                token.to_owned(),
                FreshWindow {
                    key: key.clone(),
                    token: token.to_owned(),
                    frame: fact.rect,
                    observed_frame,
                    facts: facts_of.get(token).copied(),
                    output,
                    workspace,
                    iconic,
                    hidden,
                    sticky: fact.sticky,
                    floating: fact.floating,
                    fit_excluded: fact.fit_excluded,
                    live_tag: state.member_tags.get(key).cloned(),
                },
            );
        }
        // Observation-time views are the adapter's proven applied
        // receipts from capture, never a fallback of desired views:
        // empty means unknown and retains pending flights.
        let views: HashMap<String, String> = captured
            .observed_views
            .iter()
            .map(|view| (view.output.0.clone(), view.view.clone()))
            .collect();
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
    pub(super) fn fresh_reply(&self, reply: &WorldReply) -> bool {
        reply.ready
            && reply.adapter_incarnation == self.adapter
            && reply.core_generation == self.generation
            && reply.based_on_observation_seq == self.current_obs
            && (!self.has_accepted || reply.state_revision > self.last_rev)
    }

    /// Record one sequencer-applied core revision as the desired baseline.
    /// Called by the owner after [`AckOutcome::Applied`](super::world_sequencer::AckOutcome::Applied);
    /// host application results never move this watermark.
    pub(super) fn note_applied(&mut self, reply: &WorldReply) {
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
    /// Mechanical nonactivating maximize-clear for one core-authorized
    /// `clear_operation` desire: restores the window in place without
    /// activating it. `Ok(true)` is setter success (the caller re-reads the
    /// maximized state; success alone never proves the clear),
    /// `Ok(false)` is an explicit native refusal, and `Err(())` is a
    /// throwing/ambiguous call. The default throws (no actuation): the
    /// production seam overrides; offline fakes script it explicitly.
    fn clear_maximized(&mut self, token: &str) -> Result<bool, ()> {
        let _ = token;
        Err(())
    }
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
    fn focus_window(&mut self, token: &str) -> Result<bool, ()>;
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
    /// One external invalidation consumed per call: racing native input that
    /// arrived mid-apply. The facade polls this before every setter.
    fn poll_external_invalidation(&mut self) -> bool {
        false
    }
}

/// In-memory offline native. Seeded from a [`FreshHost`] baseline (windows
/// plus views); writes mutate the live maps so proof re-reads observe them.
#[derive(Debug, Default)]
#[cfg(test)]
pub(super) struct FakeNative {
    pub(super) windows: HashMap<String, FreshWindow>,
    pub(super) views: HashMap<String, String>,
    pub(super) foreground: Option<String>,
    pub(super) gesture: bool,
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
    pub(super) fail_focus: HashSet<String>,
    pub(super) throw_focus: HashSet<String>,
    /// Scripted clear dispositions for the offline lane: explicit native
    /// refusal vs throwing/ambiguous call. A successful offline clear
    /// flips the live maximized bit so re-reads observe it.
    pub(super) fail_clear: HashSet<String>,
    pub(super) throw_clear: HashSet<String>,
    /// Every clear setter attempted, in order (vetoes record nothing: no
    /// setter ran).
    pub(super) clear_calls: Vec<String>,
    /// Scripted live cursor position (`None` = unreadable). Warp writes
    /// update it so re-reads observe the move, like the real position.
    pub(super) cursor: Option<(i32, i32)>,
    /// Every warp write attempted, in order (already-there skips record
    /// nothing).
    pub(super) warp_calls: Vec<(i32, i32)>,
    /// Warp setter dispositions for the offline lane.
    pub(super) fail_warp: bool,
    pub(super) throw_warp: bool,
    pub(super) pending_invalidation: bool,
    pub(super) frame_writes: usize,
    pub(super) membership_writes: usize,
    /// Scripted live DPI per window (`None` = unreadable, never guessed).
    pub(super) dpi: HashMap<String, u32>,
    /// Windows whose DPI changes across the write (deterministic stand-in
    /// for a cross-monitor rescale under the call).
    pub(super) dpi_flip: HashSet<String>,
}

#[cfg(test)]
impl FakeNative {
    pub(super) fn seeded(host: &FreshHost) -> Self {
        let mut native = Self::default();
        for (token, window) in &host.baseline {
            native.windows.insert(token.clone(), window.clone());
        }
        native.views.clone_from(&host.views);
        native
    }

    fn note_write(&mut self) {}
}

#[cfg(test)]
impl NativeOps for FakeNative {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.windows.get(token).cloned()
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.windows.get(token).and_then(|w| w.frame)
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
        self.note_write();
        Ok(true)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        if self.throw_focus.contains(token) {
            return Err(());
        }
        if self.fail_focus.contains(token) {
            self.note_write();
            return Ok(false);
        }
        self.foreground = Some(token.to_owned());
        self.note_write();
        Ok(true)
    }
    fn clear_maximized(&mut self, token: &str) -> Result<bool, ()> {
        // Every setter attempt records, including throws: the attempt row
        // (not this count) is the at-most-once proof the core reconciles.
        self.clear_calls.push(token.to_owned());
        if self.throw_clear.contains(token) {
            return Err(());
        }
        if self.fail_clear.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if let Some(window) = self.windows.get_mut(token)
            && let Some(facts) = window.facts.as_mut()
        {
            facts.maximized = false;
        }
        self.note_write();
        Ok(true)
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
    fn poll_external_invalidation(&mut self) -> bool {
        std::mem::take(&mut self.pending_invalidation)
    }
}

/// At-most-once dispatch fences for transfers, floats, views, and effects.
/// All entries are mechanical transport state, never policy verdicts:
/// completion, refusal, and ambiguity all live in the core over reported
/// raw facts plus native observation. Operation-scoped entries live and
/// die with the core generation; [`HostDiff::reset_generation`] clears
/// them.
#[derive(Debug, Default)]
pub(super) struct EffectTracker {
    /// Membership phases already dispatched, per operation window:
    /// a possibly-executed write never repeats without fresh unambiguous
    /// proof. Retired only by [`HostDiff::prune`] (operation left the
    /// reply) or generation reset. Distinct from core `held`: this blocks
    /// the duplicate dispatch itself, the core owns the verdict.
    pub(super) attempted_members: HashSet<(String, String)>,
    pub(super) spent_effects: HashSet<String>,
    pub(super) spent_floats: HashSet<String>,
    /// Distinct maximize-clear phases per core operation id: armed (marked)
    /// immediately before the clear setter (or before reporting a veto
    /// refusal), so a spent clear never re-actuates. Only operation
    /// retirement/reset retires a phase.
    pub(super) spent_clears: HashSet<String>,
    /// Distinct local view-phase memory per owning core id: the retained
    /// view id itself for standalone views, the window parent for follow,
    /// the migration parent for the target view, and the actual refill id
    /// for refill. Never cleared by membership arrival; only operation
    /// retirement/reset retires a phase. Every view phase is armed (marked)
    /// immediately before its setter; exact target proof may settle without
    /// a dispatch.
    pub(super) spent_views: HashSet<String>,
    /// Cursor-warp watermark (stage 4b): the observation sequence whose
    /// warp already ran. One mechanical warp per observation at most, so a
    /// duplicate delivery of the same accepted reply never moves the
    /// cursor twice. Cleared only by generation reset, never by prune:
    /// later warps ride newer observations with newer focus effects.
    pub(super) last_warp_obs: Option<u64>,
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

impl EffectTracker {
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
    /// Mark one effect spent BEFORE dispatch. Returns false when already
    /// spent: duplicates never reapply.
    pub(super) fn mark_effect_spent(&mut self, id: &str) -> bool {
        self.spent_effects.insert(id.to_owned())
    }
    /// Mark one float placement spent BEFORE dispatch. Returns false when
    /// already spent for this operation: the transport never dispatches the
    /// same placement twice. This is strictly at-most-once dispatch, never
    /// a policy verdict: completion, refusal, and ambiguity all live in the
    /// core over the reported raw attempt plus native observation.
    pub(super) fn mark_float_spent(&mut self, op: &str) -> bool {
        self.spent_floats.insert(op.to_owned())
    }
    /// Mark one maximize-clear spent BEFORE dispatch (or before reporting
    /// a veto refusal). Returns false when already spent for this
    /// operation: the transport never dispatches the same clear twice, and
    /// a vetoed desire reports exactly one refused attempt. Strictly
    /// at-most-once dispatch, never a policy verdict.
    pub(super) fn mark_clear_spent(&mut self, op: &str) -> bool {
        self.spent_clears.insert(op.to_owned())
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
        self.spent_effects.clear();
        self.spent_floats.clear();
        self.spent_clears.clear();
        self.spent_views.clear();
        self.last_warp_obs = None;
    }
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
    pub(super) effects: EffectTracker,
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
            effects: EffectTracker::default(),
        }
    }

    /// Generation reset: new core namespace clears operation-scoped tables
    /// (reused operation ids must never inherit spent/ambiguous/applied
    /// state). The caller re-derives the watermark from the new bootstrap.
    #[allow(dead_code)]
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.gate.reset_generation(generation);
        self.effects.clear_generation();
    }

    /// Bound every table against the current reply and baseline: spent and
    /// attempted entries survive only for operation ids the reply still
    /// carries. Attempted membership survives on the live operation alone:
    /// pruning against the fresh baseline would erase the at-most-once
    /// dispatch fence across observation gaps (unreadable/incomplete
    /// inventory) and acknowledgement loss, allowing a possibly-executed
    /// write to repeat. Departed lifetimes retire with the operation id
    /// (the core forgets them on close when the reply drops the transfer).
    fn prune(&mut self, reply: &WorldReply, _fresh: &FreshHost) {
        let mut live_ops: HashSet<&str> = HashSet::new();
        for effect in &reply.effects {
            live_ops.insert(effect.id.as_str());
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
            if let Some(op) = row.clear_operation.as_deref()
                && !op.is_empty()
            {
                live_ops.insert(op);
            }
        }
        self.effects
            .spent_effects
            .retain(|id| live_ops.contains(id.as_str()));
        self.effects
            .spent_floats
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .spent_clears
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .spent_views
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .attempted_members
            .retain(|key| live_ops.contains(key.0.as_str()));
    }
}

/// Trace-only per-effect focus dispatch record (stage 4b FFC oracle).
/// One row per `reply.effects` entry in order, bounded by
/// [`MAX_FOCUS_DIAG`]: `dispatched` tells whether the native focus setter
/// ran; `setter` is the raw setter disposition (`accepted`/`refused`/
/// `threw`, or `skipped` when no setter ran); `foreground` is the immediate
/// foreground proof read right after dispatch (always present, even when
/// false, so dispatched-but-unproven is distinguishable from no setter);
/// `hold` names the individual hold when the effect did not prove
/// (`None` only on proven foreground arrival). Pure trace state, never a
/// policy input: the core consumes no effect outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct FocusDispatchDiag {
    pub(super) id: String,
    pub(super) target: String,
    pub(super) kind: &'static str,
    pub(super) dispatched: bool,
    pub(super) setter: &'static str,
    pub(super) foreground: bool,
    pub(super) hold: Option<&'static str>,
}

/// Trace-only per-clear hold record for the common maximize-clear lane
/// (stage 5 directional): one row per published `clear_operation` desire
/// that dispatched no setter this pass, naming the exact pre-dispatch gate
/// that held it. Pure trace state, never a policy input: the core consumes
/// no hold reasons. Veto gates (`scope-not-controlled`, `identity-drift`,
/// `policy-veto`) additionally emit their correlated refused attempt; every
/// other reason is a silent hold with no attempt and no spent mark, so a
/// later readable report retries the still-undispatched desire.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ClearHoldDiag {
    pub(super) op: String,
    pub(super) window: String,
    pub(super) hold: &'static str,
}

/// Bound for [`FocusDispatchDiag`] rows per apply pass. Effects themselves
/// are already bounded by the reply byte budget; this caps only the retained
/// trace rows with an honest omitted count in the tick diag.
pub(super) const MAX_FOCUS_DIAG: usize = 8;

/// Bound for [`ClearHoldDiag`] rows per apply pass, with an honest omitted
/// count in the tick diag.
pub(super) const MAX_CLEAR_HOLD_DIAG: usize = 8;

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
    /// Mechanical maximize-clears actually dispatched this pass (setter ran;
    /// success alone never proves the clear, the core settles from the
    /// reported attempt plus the next observation).
    pub(super) clear_writes: usize,
    pub(super) creations: usize,
    pub(super) removals: usize,
    pub(super) effects: usize,
    /// Mechanical cursor warps actually dispatched this pass (no-op
    /// already-there skips and duplicate-observation skips count nothing).
    pub(super) cursor_warps: usize,
    pub(super) held: usize,
    pub(super) migration_total: usize,
    pub(super) migration_proven: usize,
    pub(super) stopped_mid_apply: bool,
    pub(super) partial: bool,
    /// Trace-only per-effect focus dispatch rows (see [`FocusDispatchDiag`]).
    /// Bounded to [`MAX_FOCUS_DIAG`] entries in reply order.
    pub(super) focus_dispatch: Vec<FocusDispatchDiag>,
    /// Trace-only per-clear hold rows (see [`ClearHoldDiag`]): one entry
    /// per published clear desire that dispatched no setter this pass.
    /// Bounded to [`MAX_CLEAR_HOLD_DIAG`] entries in reply order; the
    /// unbounded total rides `clear_holds_total` below so the tick diag
    /// reports an honest omitted count past the cap.
    pub(super) clear_holds: Vec<ClearHoldDiag>,
    /// Trace-only count of every clear-hold attribution this pass,
    /// including rows past the [`MAX_CLEAR_HOLD_DIAG`] storage cap.
    /// The tick diag derives `clear_holds_omitted` from this total minus
    /// the stored rows, so a pass with more held desires than storage
    /// still reports the excess instead of a false zero.
    pub(super) clear_holds_total: usize,
}

impl ApplyStats {
    /// Trace-only hold attribution for one published clear desire that
    /// dispatched no setter. Counts every attribution in
    /// `clear_holds_total` and retains only the first
    /// [`MAX_CLEAR_HOLD_DIAG`] rows. Never a policy input.
    pub(super) fn note_clear_hold(&mut self, op: &str, window: &str, hold: &'static str) {
        self.clear_holds_total = self.clear_holds_total.saturating_add(1);
        if self.clear_holds.len() < MAX_CLEAR_HOLD_DIAG {
            self.clear_holds.push(ClearHoldDiag {
                op: op.to_owned(),
                window: window.to_owned(),
                hold,
            });
        }
    }
}

/// Scope authority for one `(output, workspace)` domain, matching the core
/// envelope: `ScopeStatus.scope` is `"{output}/{workspace}"`, and omission
/// inside a held or untouched scope authorizes no write or retirement.
fn scope_controlled(reply: &WorldReply, output: &str, workspace: &str) -> bool {
    let scope = format!("{output}/{workspace}");
    reply
        .scopes
        .iter()
        .any(|status| status.scope == scope && status.authority == ScopeAuthority::Controlled)
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
    reply: &WorldReply,
    ops: &mut dyn NativeOps,
    output: &str,
    workspace: &str,
) -> bool {
    let scope = format!("{output}/{workspace}");
    let Some(authority) = reply
        .scopes
        .iter()
        .find(|status| status.scope == scope)
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

/// Freshness plus racing-input poll before a setter. Consumes at most one
/// pending external invalidation; on any staleness the caller stops before
/// remaining unsafe writes.
fn ensure_fresh(
    reply: &WorldReply,
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

fn overlay_guarded(window: &FreshWindow) -> bool {
    window
        .facts
        .is_some_and(|facts| facts.captionless_fullscreen || facts.maximized)
        || window.sticky
        || window.floating
        || window.fit_excluded
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
    if live.hidden || live.iconic || live.frame.is_none() {
        return Gate::Hold;
    }
    let Some(facts) = live.facts else {
        return Gate::Hold;
    };
    match kind {
        SetterKind::Frame => {
            if !capture_facts_rideable(&facts) {
                return Gate::Hold;
            }
            if overlay_guarded(&live) {
                return Gate::Hold;
            }
        }
        SetterKind::Float => {
            // Strict eligibility: the full classify gate (cloaked,
            // elevated, shell, tool, owned, dialog, no-activate, hidden,
            // minimized, plus fullscreen/maximized overlay protection) and
            // the live adapter lanes. Ordinary floating intent alone is
            // allowed, and sticky rides too: this lane only ever writes
            // under an explicit retained `frame_operation` token (directional
            // half-snaps), so ordinary unsolicited sticky frame writes stay
            // prohibited by the token gate plus the tiled lane. Only
            // fit-excluded holds here.
            if crate::tiling::classify(&facts).is_err() {
                return Gate::Hold;
            }
            if live.fit_excluded {
                return Gate::Hold;
            }
        }
        SetterKind::Membership => {
            if !capture_facts_rideable(&facts) {
                return Gate::Hold;
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

/// Read one homing as the carried report pair (`None` = unreadable).
fn homing_pair(homing: Option<(String, String)>) -> Option<(OutputId, String)> {
    homing.map(|(out, ws)| (OutputId(out), ws))
}

/// Migration-member arrival proof shared by every gate in the migration
/// section: the retained homing proof first (covers post-view arrivals
/// and logical seams), else the view-independent physical proof (covers
/// pre-view redirect arrivals whose target view flips only after every
/// member proves). The setter boolean is never proof in either lane.
fn migration_member_proven(
    ops: &mut dyn NativeOps,
    reply: &WorldReply,
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

fn window_transfer_for<'a>(reply: &'a WorldReply, window: &str) -> Option<&'a TransferTarget> {
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
    reply: &WorldReply,
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
    reply: &WorldReply,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    transfer: &TransferTarget,
    effects: &mut EffectTracker,
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
        setter: frame_setter(result.as_ref().ok().copied()),
        pre,
        post: proof.clone(),
        baseline,
        native_pre: None,
        native_post: None,
        native_baseline: None,
        attempt_ms,
    });
    if result.is_ok() && proof.as_deref() == Some(ws.as_str()) {
        stats.view_writes += 1;
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

/// Record one proven window-transfer arrival plus its follow view.
/// Pre-proven arrivals (no setter ran) record no write; post-setter proof
/// records one membership write. Follow actuation reports its raw attempt
/// for the core to hold the parent ambiguous on a miss. `count_write`
/// marks a confirmed native write (post-setter proof).
#[allow(clippy::too_many_arguments)]
fn settle_transfer_proven(
    reply: &WorldReply,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    transfer: &TransferTarget,
    effects: &mut EffectTracker,
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

/// Diff one ready reply against fresh proof and actuate through `ops`.
///
/// The caller owns generation/observation advancement: `state.gate.current_obs`
/// is the CURRENT watermark at entry. Racing input is consumed through
/// [`NativeOps::poll_external_invalidation`] before every setter; the core
/// consumes no verdicts from this pass (float placements never did: their
/// verdicts live in the core). Every attempted frame write appends one
/// mechanical [`FrameAttempt`] to `frame_out`, every membership write one
/// [`MemberAttempt`] to `member_out`, every view write one
/// [`ViewAttempt`] to `view_out`, and every maximize-clear write one
/// mechanical [`ClearAttempt`](tiler_core::world_report::ClearAttempt) to
/// `clear_out` for the next report (each stamped with `attempt_ms`, the
/// tick's monotonic clock); the core owns all completion and the next
/// desire is obeyed as-is.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(super) fn apply_world_reply(
    state: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
    frame_out: &mut Vec<FrameAttempt>,
    member_out: &mut Vec<MemberAttempt>,
    view_out: &mut Vec<ViewAttempt>,
    clear_out: &mut Vec<ClearAttempt>,
    attempt_ms: u64,
) -> ApplyStats {
    let mut stats = ApplyStats::default();
    let entry_obs = state.gate.current_obs;

    if !state.gate.fresh_reply(reply) {
        stats.held += 1;
        stats.stopped_mid_apply = true;
        return stats;
    }
    state.prune(reply, fresh);

    // Logical workspace ids need no native binding: the core owns them
    // outright, so membership, view, and migration targets never wait for
    // a binding that will never arrive.

    let HostDiff { gate, effects } = state;

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
        // writes. Overlays keep membership but take no tiled frame writes.
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
        let frame_controlled = row.frame_authority == ScopeAuthority::Controlled;
        let member_controlled = row.membership_authority == ScopeAuthority::Controlled;
        let transfer = window_transfer_for(reply, &row.lifetime.0);
        let arrival_pending = transfer.is_some_and(|t| {
            ops.read_homing(&row.lifetime.0)
                .is_some_and(|(out, ws)| out != t.target_output.0 || ws != t.target_workspace)
        }) || migration_arrival_pending(
            reply,
            ops,
            &row.lifetime.0,
            &row.output.0,
            &row.workspace,
        );
        // The writable frame target is `effective_frame` only: `allocation`
        // is the canonical slot, never a write target.
        if frame_controlled && row.effective_frame.is_some() && arrival_pending {
            // Deferred until membership proof: the target frame belongs to
            // the destination output, so writing it pre-move would place
            // foreign geometry on the source. Never hold-count a correctly
            // deferred frame.
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
                                        setter: frame_setter(wrote),
                                        readback: proof,
                                        dpi_before,
                                        dpi_after,
                                        attempt_ms,
                                        operation_id: None,
                                        prewrite: None,
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
        // effective frame is the only write target): hold fail-closed.
        if frame_controlled && row.effective_frame.is_none() {
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
            // Pre-proven arrival (no setter ran): the core settles from
            // staged observation alone. Only the follow phase may still
            // need actuation.
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
        // pass `None` and preserve the physical frame. Same-output movers
        // need no translation (the view batch proves on the shared
        // screen); cross-output movers ride the published slot or the
        // sessionless migration placement.
        let frame_target = if row.floating || base.floating {
            None
        } else {
            row.effective_frame
        };
        let moved = ops.move_membership(&row.lifetime.0, &target_out, &target_ws, frame_target);
        // Re-read for the mechanical post proof BEFORE any early
        // invalidation break: the setter boolean is never proof, and the
        // raw attempt must survive even when racing input stopped the pass.
        // The core derives confirmation, proven refusal, and ambiguity from
        // this raw attempt plus the staged observation.
        let post = ops.read_homing(&row.lifetime.0);
        member_out.push(MemberAttempt {
            operation_id: op.clone(),
            window: row.lifetime.clone(),
            target_output: t.target_output.clone(),
            target_workspace: target_ws.clone(),
            setter: frame_setter(moved.as_ref().ok().copied()),
            pre: homing_pair(before),
            post: homing_pair(post),
            native_pre: None,
            native_post: None,
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
        let float_ok = base.floating && float_no_effect_proven(ops, base, &fresh.stored_tags);
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
            setter: frame_setter(wrote),
            readback: proof,
            dpi_before,
            dpi_after,
            attempt_ms,
            operation_id: Some(op.to_owned()),
            prewrite,
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
        finalize(&mut stats);
        return stats;
    }

    // 1c. Explicit maximize-clears: the one-shot `clear_operation` token
    // is necessary but never sufficient. The row carries no frame
    // authority for the clear; the token plus the fresh gates below are
    // the only write authority. Ordinary maximized windows (no token)
    // never ride here.
    //
    // This lane reports mechanical write facts only (setter disposition
    // plus immediate pre/post maximized states) with exact operation-id
    // plus window correlation; the core derives confirmation, proven
    // refusal, ambiguity, and external abandonment. No outcome verdict is
    // emitted here: a setter boolean or a readback alone never settles a
    // clear, and completion comes from native observation.
    //
    // Native vetoes (fullscreen, elevated/shell/tool/dialog policy
    // failures, identity/lifetime races, uncontrolled scopes) MUST produce
    // a correlated refused attempt rather than leave a silent unattempted
    // active desire: the phase arms (spent) before reporting refusal, so a
    // vetoed desire retires with no loop and no re-actuation. Gaps
    // (unreadable, hidden, minimized, cloaked, gesture, manual frame race)
    // hold with no attempt and no spent mark, so a later readable report
    // retries the still-undispatched desire.
    for row in &reply.windows {
        let op = row.clear_operation.as_deref().unwrap_or("");
        if op.is_empty() {
            continue;
        }
        let token = row.lifetime.0.clone();
        // A stale pass stops before dispatching any remaining desire: the
        // first unprocessed desire still attributes the stop so the trace
        // distinguishes it from a row-narrow hold (which skips one row and
        // keeps converging the rest). Stops end the pass; holds do not.
        if gate.current_obs != entry_obs {
            stats.note_clear_hold(op, &token, "stale-stopped");
            stats.stopped_mid_apply = true;
            break;
        }
        // Bounded trace-only hold attribution for this desire: one row per
        // pre-dispatch gate, in reply order. Never a policy input.
        macro_rules! push_clear_hold {
            ($reason:expr) => {{
                stats.note_clear_hold(op, &token, $reason);
            }};
        }
        // Best-known maximized states for a veto refusal row: the live
        // re-read when it proves, else the observation baseline (possibly
        // unreadable, which the core reconciles as ambiguity, never proof).
        let baseline_maximized = fresh
            .baseline
            .get(&token)
            .and_then(|base| base.facts.map(|facts| facts.maximized));
        // A veto refusal spends the phase and reports exactly one refused
        // attempt with best-known pre/post states. Never silent, never
        // re-actuated. The hold reason rides the trace alongside the
        // refused attempt.
        let mut veto_refused = |clear_out: &mut Vec<ClearAttempt>,
                                stats: &mut ApplyStats,
                                hold: &'static str,
                                pre: Option<bool>,
                                post: Option<bool>| {
            stats.note_clear_hold(op, &token, hold);
            if effects.mark_clear_spent(op) {
                clear_out.push(ClearAttempt {
                    operation_id: op.to_owned(),
                    window: row.lifetime.clone(),
                    setter: FrameSetter::Refused,
                    pre_maximized: pre,
                    post_maximized: post,
                    attempt_ms,
                });
            }
        };
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            veto_refused(
                clear_out,
                &mut stats,
                "scope-not-controlled",
                baseline_maximized,
                baseline_maximized,
            );
            stats.held += 1;
            continue;
        }
        let Some(base) = fresh.baseline.get(&token) else {
            push_clear_hold!("unknown-baseline");
            stats.held += 1;
            continue;
        };
        if base.hidden || base.iconic || base.frame.is_none() || base.observed_frame.is_none() {
            push_clear_hold!("hidden-or-unreadable");
            stats.held += 1;
            continue;
        }
        if ops.gesture_barrier() {
            push_clear_hold!("gesture-barrier");
            stats.held += 1;
            continue;
        }
        let Some(live) = ops.read_window(&token) else {
            push_clear_hold!("unknown-live");
            stats.held += 1;
            continue;
        };
        if live.key != base.key {
            veto_refused(
                clear_out,
                &mut stats,
                "identity-drift",
                baseline_maximized,
                None,
            );
            stats.held += 1;
            continue;
        }
        let live_maximized = live.facts.map(|facts| facts.maximized);
        let Some(live_facts) = live.facts else {
            push_clear_hold!("unreadable-facts");
            stats.held += 1;
            continue;
        };
        match crate::tiling::classify(&live_facts) {
            Ok(()) => {}
            Err(crate::tiling::SkipReason::Maximized) => {}
            Err(
                crate::tiling::SkipReason::Fullscreen
                | crate::tiling::SkipReason::Elevated
                | crate::tiling::SkipReason::Shell
                | crate::tiling::SkipReason::Tool
                | crate::tiling::SkipReason::OwnedDialog
                | crate::tiling::SkipReason::Dialog
                | crate::tiling::SkipReason::NoActivate,
            ) => {
                veto_refused(
                    clear_out,
                    &mut stats,
                    "policy-veto",
                    live_maximized.or(baseline_maximized),
                    live_maximized.or(baseline_maximized),
                );
                stats.held += 1;
                continue;
            }
            Err(_) => {
                push_clear_hold!("classify-hold");
                stats.held += 1;
                continue;
            }
        }
        if !live_maximized.unwrap_or(true) {
            // Already unmaximized: no write, no attempt, no hold row. The
            // core settles from the staged observation itself (an external
            // clear without a correlated attempt abandons; it never
            // continues).
            continue;
        }
        // Manual-change race: the live frame left the observation-time live
        // frame. Never fight the user; hold for the next stable observation.
        // The baseline is the pre-substitution live frame
        // (`observed_frame`), never the retained tile slot: retained
        // maximized/fullscreen rows ride their slot in `frame` while the
        // native window shows the maximized frame, and the production live
        // lane re-observes that maximized frame through `observe_window`.
        // Any disagreement between the two live reads still holds.
        if base.observed_frame != live.frame {
            push_clear_hold!("frame-race");
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            push_clear_hold!("stale-stopped");
            stats.stopped_mid_apply = true;
            break;
        }
        // Arm the clear phase immediately pre-dispatch, after every
        // safety/authority gate passed, so holds never consume the desire.
        // Strictly at-most-once dispatch, never a policy verdict.
        if !effects.mark_clear_spent(op) {
            push_clear_hold!("already-spent");
            stats.held += 1;
            continue;
        }
        let pre = ops
            .read_window(&token)
            .and_then(|window| window.facts.map(|facts| facts.maximized));
        let wrote = ops.clear_maximized(&token).ok();
        let post = ops
            .read_window(&token)
            .and_then(|window| window.facts.map(|facts| facts.maximized));
        // Mechanical facts only: the setter boolean is never proof, the
        // pre/post maximized states are the refusal/confirmation preimage,
        // and the core settles from this evidence plus the next staged
        // observation.
        clear_out.push(ClearAttempt {
            operation_id: op.to_owned(),
            window: row.lifetime.clone(),
            setter: frame_setter(wrote),
            pre_maximized: pre,
            post_maximized: post,
            attempt_ms,
        });
        if wrote == Some(true) {
            stats.clear_writes += 1;
        }
        if post != Some(false) {
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return stats;
    }

    // Paired migration source-refill views (`{migration-op}-refill`) defer
    // to the migration path below: switching the source view before member
    // arrival proof would hide unmoved members.
    let mut deferred_refills: HashSet<&str> = HashSet::new();
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::View) {
            continue;
        }
        if let Some(prefix) = t.operation_id.strip_suffix("-refill")
            && reply
                .transfers
                .iter()
                .any(|m| matches!(m.kind, TransferKind::Migration) && m.operation_id == prefix)
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
        if !scope_actuatable(reply, ops, &out, &ws) {
            stats.held += 1;
            continue;
        }
        if ops.read_view(&out).as_deref() == Some(ws.as_str()) {
            // Already shown: no write, no attempt. The core settles from
            // the staged echo itself.
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
        let pre = ops.read_view(&out);
        let baseline = fresh.views.get(&out).cloned();
        // Arm the view phase immediately before dispatch; late proofs
        // cannot rearm it, so a possibly-executed switch never repeats.
        if !effects.mark_view_spent(&op) {
            stats.held += 1;
            continue;
        }
        let result = ops.switch_view(&out, &ws);
        let proof = ops.read_view(&out);
        // Mechanical facts only: the setter boolean is never proof, and
        // no outcome is claimed here. The core settles from this attempt
        // plus the next staged echo.
        view_out.push(ViewAttempt {
            operation_id: op,
            target_output: t.target_output.clone(),
            target_workspace: ws.clone(),
            setter: frame_setter(result.as_ref().ok().copied()),
            pre,
            post: proof.clone(),
            baseline,
            native_pre: None,
            native_post: None,
            native_baseline: None,
            attempt_ms,
        });
        if result.is_ok() && proof.as_deref() == Some(ws.as_str()) {
            stats.view_writes += 1;
        } else {
            // Setter refusal, throw, or unreadable proof: the reported raw
            // attempt holds the core without re-actuation.
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
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
                setter: frame_setter(moved.as_ref().ok().copied()),
                pre: homing_pair(before),
                post: homing_pair(post),
                native_pre: None,
                native_post: None,
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
                setter: frame_setter(result.as_ref().ok().copied()),
                pre,
                post: proof.clone(),
                baseline,
                native_pre: None,
                native_post: None,
                native_baseline: None,
                attempt_ms,
            });
            if proof.as_deref() == Some(ws.as_str()) {
                if result.is_ok() {
                    stats.view_writes += 1;
                }
            } else {
                stats.held += 1;
                continue;
            }
        }
        // Paired source refill only after the target view proves: never
        // ahead of arrival. The refill owns its own view phase under the
        // actual refill id, with its own held/actuation/spent gates: a
        // floating (Untouched) refill actuates exactly like a standalone
        // floating view once the target proof is in.
        let refill_op = format!("{op}-refill");
        if let Some(refill) = reply
            .transfers
            .iter()
            .find(|v| matches!(v.kind, TransferKind::View) && v.operation_id == refill_op)
        {
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
                    setter: frame_setter(result.as_ref().ok().copied()),
                    pre,
                    post: proof.clone(),
                    baseline: baseline_refill,
                    native_pre: None,
                    native_post: None,
                    native_baseline: None,
                    attempt_ms,
                });
                if proof.as_deref() == Some(refill_ws.as_str()) {
                    if result.is_ok() {
                        stats.view_writes += 1;
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
        finalize(&mut stats);
        return stats;
    }

    // 4. One-shot effects: spent BEFORE dispatch, proven by read-back, gated
    // on the exact live view (switch-before-focus). Stale effects drop
    // (at-most-once), never steal focus later. Effects emit no verdict:
    // the core consumes no effect outcomes; the spent proof plus the
    // foreground read-back is the whole contract. Trace-only per-effect
    // rows land in `stats.focus_dispatch` (bounded, see MAX_FOCUS_DIAG):
    // `dispatched` plus the raw setter disposition and the immediate
    // foreground proof even when false distinguish dispatched-but-unproven
    // from no setter; `hold` names the individual hold. Static-hover retry
    // behavior is unchanged: no extra dispatch, no re-actuation.
    for effect in &reply.effects {
        // Bounded trace push helper: reply order, honest truncation (the
        // tick reports the omitted count from the reply length).
        macro_rules! push_focus {
            ($dispatched:expr, $setter:expr, $foreground:expr, $hold:expr) => {{
                if stats.focus_dispatch.len() < MAX_FOCUS_DIAG {
                    stats.focus_dispatch.push(FocusDispatchDiag {
                        id: effect.id.clone(),
                        target: effect.target.0.clone(),
                        kind: effect.kind.as_str(),
                        dispatched: $dispatched,
                        setter: $setter,
                        foreground: $foreground,
                        hold: $hold,
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
        let Some(base) = fresh.baseline.get(&effect.target.0) else {
            push_focus!(false, "skipped", false, Some("unknown-baseline"));
            stats.held += 1;
            continue;
        };
        if base.hidden || base.iconic || base.frame.is_none() {
            push_focus!(false, "skipped", false, Some("hidden-or-unreadable"));
            stats.held += 1;
            continue;
        }
        if base
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
        // it actually lives.
        let Some((live_out, live_ws)) = ops.read_homing(&effect.target.0) else {
            push_focus!(false, "skipped", false, Some("unknown-homing"));
            stats.held += 1;
            continue;
        };
        if !scope_controlled(reply, &live_out, &live_ws) {
            push_focus!(false, "skipped", false, Some("held-scope"));
            stats.held += 1;
            continue;
        }
        if ops.read_view(&live_out).as_deref() != Some(live_ws.as_str()) {
            // The live workspace is not shown on its output: focusing now
            // would steal focus from the wrong view. Hold for the view.
            push_focus!(false, "skipped", false, Some("view-not-shown"));
            stats.held += 1;
            continue;
        }
        if !effects.mark_effect_spent(&effect.id) {
            push_focus!(false, "skipped", false, Some("already-spent"));
            continue;
        }
        // Foreground read-back is authoritative even if the setter throws:
        // a throwing call that still landed proves focus. Both the raw
        // setter disposition and the immediate foreground proof ride the
        // trace row even when the proof is false, so a dispatched but
        // unproven setter is distinguishable from no setter at all.
        let setter_outcome = ops.focus_window(&effect.target.0);
        let setter = match &setter_outcome {
            Ok(true) => "accepted",
            Ok(false) => "refused",
            Err(()) => "threw",
        };
        let foreground = ops.is_foreground(&effect.target.0);
        if foreground {
            push_focus!(true, setter, true, None);
            stats.effects += 1;
        } else {
            push_focus!(true, setter, false, Some("foreground-unproven"));
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }

    // 5. Cursor warp (stage 4b): one mechanical position setter per
    // observation at most, and only correlated with the focus that
    // actually results. The core publishes the point only alongside its
    // focus effects; the adapter warps only when the last focus effect in
    // order still targets the window holding the observed foreground. A
    // stale point with no focus effect, a held or failed focus, focus
    // that drifted elsewhere, racing input, or a new fullscreen game all
    // fail that proof and never move the cursor. An already-there read
    // consumes the observation with no write; an explicit refusal holds
    // without retry (the focus already happened, the next cross-output
    // focus re-warms); a throw stops the pass like every other setter.
    // Duplicate deliveries of the same observation never move the cursor
    // twice.
    if let Some((x, y)) = reply.cursor_warp {
        if gate.current_obs != entry_obs || !gate.fresh_reply(reply) {
            stats.stopped_mid_apply = true;
        } else if effects.last_warp_obs == Some(reply.based_on_observation_seq) {
            // Duplicate delivery of an already-warped observation.
        } else if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
        } else {
            // The last focus effect in order dictates the resulting focus
            // (section 4 applies effects in order): warp only for that
            // exact target while it holds the observed foreground.
            let target = reply
                .effects
                .iter()
                .rfind(|effect| effect.kind == EffectKind::Focus)
                .map(|effect| effect.target.clone());
            let mut correlated = false;
            match target {
                None => {
                    // Stale point with no focus effect: consume below
                    // without motion.
                }
                Some(target) => match fresh.baseline.get(&target.0) {
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
                                // homing (never the observation preimage),
                                // plus the actual observed foreground: the
                                // focus must have landed, not merely been
                                // dispatched.
                                let shown = ops.read_homing(&target.0).is_some_and(|(out, ws)| {
                                    scope_controlled(reply, &out, &ws)
                                        && ops.read_view(&out).as_deref() == Some(ws.as_str())
                                });
                                if shown && ops.is_foreground(&target.0) {
                                    correlated = true;
                                } else {
                                    stats.held += 1;
                                }
                            }
                        }
                    }
                },
            }
            effects.last_warp_obs = Some(reply.based_on_observation_seq);
            if correlated {
                if ops.read_cursor() == Some((x, y)) {
                    // Already there: consume without a write.
                } else {
                    match ops.move_cursor(x, y) {
                        Err(()) => {
                            // Throwing call: the pass stops, and the
                            // observation is still consumed so a duplicate
                            // delivery never re-yanks a cursor the user may
                            // have moved since.
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
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
        }
    }

    finalize(&mut stats);
    stats
}
