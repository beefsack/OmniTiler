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
//! mutation, no FFI, no sequencing: observation/request/command counters and
//! acknowledgement belong to the dependent sequencer unit, which is
//! deliberately not built here.
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
//! - Core logical topology: committed membership, logical ring/active
//!   view, and the mechanical device-to-wire output tokens that serve as
//!   the wire-opaque output ids (native device keys such as
//!   `\\.\DISPLAY1` fail the core `is_opaque_id` gate, so the existing
//!   `output_token` source maps them collision-free).
//! - `monitors.rs` `MonitorArea`: device-keyed work (placement) and full
//!   (overlay) rectangles. `MonitorArea` carries no primary flag; the owner
//!   orders `all_monitors` primary-first, so capture derives primary from
//!   the `areas.first()` device, never from managed output order (which may
//!   be reordered relative to the native list).
//! - Owner intent lanes (`floated`, `sticky`, `restart_reject_hold`,
//!   `fixed_auto_seen`, `fixed_tile_overrides`, `fixed_origin_overrides`,
//!   `born_fullscreen`, `born_first_exit`), lifetime tags (`member_tags`),
//!   stored identities (`member_identity`), snapshots (`member_rects` /
//!   `float_rects`), last-good constraints (`hint_logged`) and the Engine
//!   exception read (`engine_is_float`) are all consumed read-only.
//!
//! Truthfulness rules enforced here (W1 discriminators):
//! - Domains carry raw work (placement) bounds plus the configured
//!   `outer_gap` separately: the core insets exactly once through
//!   `inset_world_domain`/`world_publish`. The adapter-side inset helper
//!   runs offline only as a validates-the-margin check (a work area that
//!   cannot carry the margin holds instead of tiling un-inset).
//! - Every carried row re-validates its token against `member_tokens` for
//!   the same `WindowKey`. A mismatched lifetime inherits no flags and no
//!   lanes: its domain holds instead of carrying a partial controlled list.
//! - The stored member identity must name the same lifetime (`pid` and
//!   `process_creation` equal the key): drifted tables hold instead of
//!   scoping or classifying another process.
//! - Fresh native facts ride only when the typed report can express them:
//!   eligible, or an overlay (`fullscreen`/`maximized`) with a dedicated
//!   lane. Cloaked, elevated, shell, tool, owned, dialog, no-activate,
//!   hidden, or minimized facts have no window authority lane
//!   (`fit_excluded` is fit semantics only and is dropped at the session
//!   boundary in `observed_window_from_engine`), so the domain holds
//!   instead of authorizing geometry, membership, or effects. Scope
//!   exclusion holds the same way: unlike the legacy row assembly (which
//!   rides a row and gates writes in `writable_tokens`), the typed report
//!   has no scope authority lane, so riding would authorize writes.
//!   Maximized/fullscreen overlays keep their reserved tiled allocation
//!   where the shared contract supports it and are never blanket-floated:
//!   born-fullscreen slotless (floating) stays distinct from ordinary
//!   tiled-fullscreen reserved (tiled with fullscreen set).
//! - Hidden members additionally prove the claim association: the
//!   `HiddenRecord` claim identity must equal the member key, the claim tag
//!   must satisfy `model::valid_claim_tag`, and a stamped lifetime tag must
//!   exist. The claim tag (hidden-claim nonce) and the member tag
//!   (visible-membership lifetime) are different native properties: only
//!   presence is required of the member tag, never equality. Unproven
//!   hidden lifetimes hold even when iconic: minimization is never granted
//!   on unverified identity.
//! - Hidden non-iconic members ride their snapshot rectangles with the
//!   durable hide-time maximized preimage (`claim.show.maximized`), the
//!   adapter intent lanes and fresh same-tick rows when present. Fullscreen
//!   rides only verified sources (a token-matched fresh observed/retained
//!   row, or the `born_fullscreen` hold lane); a hidden member with no
//!   verifiable fullscreen holds its domain. Ordinary hidden fullscreen
//!   rides reserved (tiled lane with fullscreen set), never floated:
//!   floating comes only from the intent lanes. Nothing hidden defaults
//!   flags to `false` to authorize writes.
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
//!   rectangles plus work areas only. Unknown inventory never fabricates
//!   an empty row: any member that cannot resolve to a verified row rides
//!   an unreadable fact (rect `None`) and clears the global completeness
//!   flag.
//!
//! Production prerequisites consumed read-only (no FFI here):
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
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_report::{ScreenReport, ScreenView, WindowFact};

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
    pub cloaked: bool,
    pub shell: bool,
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
    pub sticky: &'a BTreeMap<WindowKey, bool>,
    pub floated: &'a BTreeSet<WindowKey>,
    pub restart_reject_hold: &'a BTreeSet<WindowKey>,
    pub born_fullscreen: &'a BTreeSet<WindowKey>,
    pub born_first_exit: &'a BTreeSet<WindowKey>,
    pub fixed_auto_seen: &'a BTreeSet<WindowKey>,
    pub fixed_tile_overrides: &'a BTreeSet<WindowKey>,
    pub fixed_origin_overrides: &'a BTreeSet<WindowKey>,
    pub hidden_claims: &'a BTreeMap<WindowKey, HiddenRecord>,
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
    pub inner_gap: i32,
    pub outer_gap: i32,
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
    /// Overlay enable flags for the presentation ingress.
    pub presentation_border_enabled: bool,
    pub presentation_underlay_enabled: bool,
    /// Host-scaled presentation style (gap, border width, resolved underlay
    /// extension) in physical pixels. DPI conversion runs tick-side before
    /// this seam; zeros on unreadable DPI (the adapter keeps its own
    /// scale veto render-side).
    pub presentation_style: (i32, i32, i32),
    /// Focused-fullscreen-game lane: the core skips presentation work
    /// (hidden shapes, empty pager) instead of projecting under the game.
    pub presentation_paused: bool,
    /// Native top-to-bottom stacking snapshot from the latest observation
    /// (first is topmost). Capture maps it to opaque lifetimes for the
    /// pager order; unknown keys drop, and an empty snapshot falls back to
    /// observed-then-retained order.
    pub presentation_native_order: Vec<crate::native_guards::WindowKey>,
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
        sticky: &'a BTreeMap<WindowKey, bool>,
        floated: &'a BTreeSet<WindowKey>,
        restart_reject_hold: &'a BTreeSet<WindowKey>,
        born_fullscreen: &'a BTreeSet<WindowKey>,
        born_first_exit: &'a BTreeSet<WindowKey>,
        fixed_auto_seen: &'a BTreeSet<WindowKey>,
        fixed_tile_overrides: &'a BTreeSet<WindowKey>,
        fixed_origin_overrides: &'a BTreeSet<WindowKey>,
        hidden_claims: &'a BTreeMap<WindowKey, HiddenRecord>,
        minimized_omitted: &'a HashSet<WindowKey>,
        pending_closed: &'a BTreeMap<String, PendingClosed>,
        scope: &'a [String],
        settings_live: &'a Option<LiveSettings>,
        output_tokens: &'a HashMap<String, String>,
        active_output: &'a str,
        applied_homing: &'a HashMap<String, (String, String)>,
        applied_views: &'a HashMap<String, String>,
        inner_gap: i32,
        outer_gap: i32,
        active: &'a HashSet<u64>,
        cursor: Option<(i32, i32)>,
        hit_hwnd: Option<u64>,
        gesture: Option<tiler_core::gesture::GestureIngress>,
        presentation_foreground: Option<PresentationForeground>,
        presentation_chord: bool,
        presentation_move: String,
        presentation_border_enabled: bool,
        presentation_underlay_enabled: bool,
        presentation_style: (i32, i32, i32),
        presentation_paused: bool,
        presentation_native_order: Vec<crate::native_guards::WindowKey>,
    ) -> OwnerInputs<'a> {
        OwnerInputs {
            member_tokens,
            member_identity,
            member_tags,
            member_rects,
            float_rects,
            hint_logged,
            sticky,
            floated,
            restart_reject_hold,
            born_fullscreen,
            born_first_exit,
            fixed_auto_seen,
            fixed_tile_overrides,
            fixed_origin_overrides,
            hidden_claims,
            minimized_omitted,
            pending_closed,
            scope,
            settings_live,
            output_tokens,
            active_output,
            applied_homing,
            applied_views,
            inner_gap,
            outer_gap,
            active,
            cursor,
            hit_hwnd,
            gesture,
            presentation_foreground,
            presentation_chord,
            presentation_move,
            presentation_border_enabled,
            presentation_underlay_enabled,
            presentation_style,
            presentation_paused,
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
            sticky: &self.sticky,
            floated: &self.floated,
            restart_reject_hold: &self.restart_reject_hold,
            born_fullscreen: &self.born_fullscreen,
            born_first_exit: &self.born_first_exit,
            fixed_auto_seen: &self.fixed_auto_seen,
            fixed_tile_overrides: &self.fixed_tile_overrides,
            fixed_origin_overrides: &self.fixed_origin_overrides,
            hidden_claims: &self.hidden_claims,
            minimized_omitted: &self.minimized_omitted,
            pending_closed: &self.pending_closed,
            scope: &self.scope,
            settings_live: &self.settings_live,
            output_tokens: &self.output_tokens,
            active_output: self.active_output.as_str(),
            applied_homing: &self.applied_homing,
            applied_views: &self.applied_views,
            inner_gap: self.inner_gap,
            outer_gap: self.outer_gap,
            active: &self.active,
            cursor: None,
            hit_hwnd: None,
            gesture: None,
            presentation_foreground: None,
            presentation_chord: false,
            presentation_move: String::new(),
            presentation_border_enabled: false,
            presentation_underlay_enabled: false,
            presentation_style: (0, 0, 0),
            presentation_paused: false,
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
    /// fresh flags, live frame), the movement-only chord level, the
    /// classified focused Move subject token, and the host-scaled style.
    /// Fact collection only: the core owns every verdict from here. The
    /// shell/hidden flags mirror the fast overlay path exactly (targeted
    /// popup gate plus hide-claim membership). A known-but-frameless
    /// foreground (minimized, maximized, fullscreen, cloaked) still carries
    /// its token and accurate flags with no frame; anything else unreadable
    /// carries nothing.
    #[allow(clippy::too_many_lines)]
    pub(super) fn presentation_tick_inputs(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        foreground_hwnd: u64,
    ) -> (
        Option<PresentationForeground>,
        bool,
        String,
        (i32, i32, i32),
    ) {
        use crate::group_underlay::MoveSizeKind;
        let foreground = if foreground_hwnd == 0 {
            None
        } else {
            let hwnd = foreground_hwnd as isize as HWND;
            match super::observe::observe_window(hwnd, me, fulls, &mut self.tokens) {
                Ok(window) => {
                    let exe_file =
                        super::shell::exe_file_name(&window.identity.exe_path).to_ascii_lowercase();
                    let border_shell = window.facts.shell
                        || super::shell::is_border_shell_popup(
                            &super::observe::class_of(hwnd),
                            &exe_file,
                        );
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
                        cloaked: window.facts.cloaked,
                        shell: border_shell,
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
                        cloaked: reason == SkipReason::Cloaked,
                        shell: false,
                        hidden_workspace,
                        frame: None,
                    })
                }
                Err(_) => None,
            }
        };
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
        let dpi = crate::ffi::dpi_for_window(foreground_hwnd);
        let (width_px, gap_px, _) =
            crate::active_border::scale_style(&self.border.style, dpi).unwrap_or((0, 0, 0));
        let extension_logical = tiler_core::visual::group_underlay_effective_extension(
            self.underlay.style.extension,
            self.border.style.width,
        );
        let extension_px =
            crate::active_border::scale_to_physical(extension_logical, dpi).unwrap_or(0);
        (
            foreground,
            chord,
            move_token,
            (gap_px, width_px, extension_px),
        )
    }
}

/// Captured world: the typed core observation pieces the sequencer unit
/// will envelope into a `WorldReport`. Sequencing fields (contract version,
/// owner/incarnation/generation, observation/request/command counters,
/// commands, raw attempts) and view echoes are deliberately absent here:
/// screens plus window facts are the only topology prerequisites, with the
/// sampled cursor and the retained targeting settings riding as core
/// ingress (never an adapter placement or output policy). Views echo the
/// core's last desired logical views out of band.
#[derive(Debug, Clone)]
pub(super) struct CapturedWorld {
    pub(super) screens: Vec<ScreenReport>,
    pub(super) windows: Vec<WindowFact>,
    pub(super) closed: Vec<WindowId>,
    /// Adapter-observed applied logical views per screen (visibility-proven
    /// receipts from `world_native_receipts::observed_view_receipts`). Empty means
    /// unknown: the core treats missing echoes as gaps and retains pending
    /// flights, never as proof.
    pub(super) observed_views: Vec<ScreenView>,
    /// Workspace scope mode from the saved `core.workspace.mode` (restart
    /// adopted, like the owner state). Drives core ring/view/history,
    /// hotplug, startup, and migration policy for the report.
    pub(super) workspace_mode: tiler_core::workspace_scope::WorkspaceMode,
    /// Equal-workspace-count parity capability for the report.
    pub(super) equal_workspace_count: bool,
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
    /// [`crate::world_cursor`]).
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
    pub(super) default_tiled: bool,
    pub(super) fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate,
    pub(super) migration_source_refill: tiler_core::output_selection::MigrationSourceRefill,
    /// Uniform presentation ingress (stage 5): normalized foreground, style,
    /// and movement facts plus the native stacking snapshot. The sequencer
    /// envelopes it into the report; the core publishes the reply
    /// presentation from it.
    pub(super) presentation: tiler_core::world_report::PresentationIngress,
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

/// Bootstrap restoration ingress: none (deleted wrapper).
/// Floating membership for one row: sticky wins unconditionally; the D4
/// automatic-fixed latch floats unless an explicit tile win dominates;
/// marker-derived adapter float intent (floats, restart-holds, born holds)
/// floats unless a first-exit mark already returned the member to the tiled
/// lane. Never inferred from hints alone. No legacy Engine exception lane:
/// the core owns float exceptions outright.
fn capture_floating(
    state: &OwnerInputs<'_>,
    key: &crate::native_guards::WindowKey,
    hidden: bool,
) -> bool {
    let sticky = state.sticky.contains_key(key);
    if sticky {
        return true;
    }
    let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
    if fixed_auto && !fixed_suppress {
        return true;
    }
    let first_exit =
        state.born_first_exit.contains(key) && (!hidden || !state.floated.contains(key));
    if first_exit {
        return false;
    }
    state.floated.contains(key)
        || state.restart_reject_hold.contains(key)
        || state.born_fullscreen.contains(key)
}

/// The three fixed-size lanes, preserved distinctly: the explicit tile-win
/// lanes (`fixed_tile_overrides` runtime plus `fixed_origin_overrides`
/// durable) dominate as `fixed_suppress`; the D4 automatic latch
/// (`fixed_auto_seen`) reports only when neither sticky intent nor a
/// durable win reframes it (production stamp parity: sticky dominates
/// automatic origin and durable wins never acquire it).
fn capture_fixed(state: &OwnerInputs<'_>, key: &crate::native_guards::WindowKey) -> (bool, bool) {
    let durable =
        state.fixed_tile_overrides.contains(key) || state.fixed_origin_overrides.contains(key);
    let sticky = state.sticky.contains_key(key);
    let automatic = state.fixed_auto_seen.contains(key) && !sticky && !durable;
    (automatic, durable)
}

/// Whether fresh native facts may ride a typed report row: eligible, or an
/// overlay (`fullscreen`/`maximized`) with a dedicated `EngineWindow` lane.
/// Every other `classify` refusal (cloaked, elevated, shell, tool, owned,
/// dialog, no-activate, hidden, minimized) has no window authority lane:
/// `fit_excluded` is fit semantics only and is dropped at the session
/// boundary (`observed_window_from_engine`), so carrying such a row as
/// tiled would publish controlled frame authority for a window the adapter
/// must not touch. Callers hold the domain instead.
pub(super) fn capture_facts_rideable(facts: &WindowFacts) -> bool {
    match crate::tiling::classify(facts) {
        Ok(()) => true,
        Err(SkipReason::Fullscreen | SkipReason::Maximized) => true,
        Err(_) => false,
    }
}

/// Uniform presentation ingress (stage 5): normalized foreground facts
/// plus host-scaled style and movement facts from the seam, with the
/// native top-to-bottom stacking snapshot derived from the same
/// observed/retained products as the old pager order (observed eligible
/// members in enumeration order, then retained rows with fresh frames).
/// Hidden members absent from both keep their retained stacking slot
/// core-side. Unknown lifetimes never admit members.
fn presentation_ingress(
    state: &OwnerInputs<'_>,
    observed: Option<&[ObservedWindow]>,
    retained: &[RetainedRow],
) -> tiler_core::world_report::PresentationIngress {
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
    // Pager frame rows, one per token with any source: fresh native frames
    // (observed visible, then retained overlay rows) fill the live slot;
    // the adapter's float slots fill the float slot; tile slots fill the
    // tile slot. First source wins per slot; the core applies the live >
    // float > tile precedence. Never a fresh observation beyond the
    // enumeration tables, never an adapter precedence decision.
    let mut rows: BTreeMap<String, tiler_core::world_report::PresentationPagerFrame> =
        BTreeMap::new();
    if let Some(observed) = observed {
        for window in observed {
            rows.entry(window.token.clone()).or_insert(
                tiler_core::world_report::PresentationPagerFrame {
                    window: tiler_core::directional::WindowId(window.token.clone()),
                    live: Some(window.visible),
                    float_frame: None,
                    tile_frame: None,
                },
            );
        }
    }
    for row in retained {
        if let Some(rect) = row.rect {
            rows.entry(row.token.clone()).or_insert(
                tiler_core::world_report::PresentationPagerFrame {
                    window: tiler_core::directional::WindowId(row.token.clone()),
                    live: Some(rect),
                    float_frame: None,
                    tile_frame: None,
                },
            );
        }
    }
    for (token, frame) in state.float_rects.iter() {
        rows.entry(token.clone())
            .or_insert_with(|| tiler_core::world_report::PresentationPagerFrame {
                window: tiler_core::directional::WindowId(token.clone()),
                live: None,
                float_frame: None,
                tile_frame: None,
            })
            .float_frame = Some(*frame);
    }
    for (token, frame) in state.member_rects.iter() {
        rows.entry(token.clone())
            .or_insert_with(|| tiler_core::world_report::PresentationPagerFrame {
                window: tiler_core::directional::WindowId(token.clone()),
                live: None,
                float_frame: None,
                tile_frame: None,
            })
            .tile_frame = Some(*frame);
    }
    let foreground = state.presentation_foreground.as_ref();
    tiler_core::world_report::PresentationIngress {
        foreground: foreground
            .map(|facts| facts.token.clone())
            .unwrap_or_default(),
        foreground_minimized: foreground.is_some_and(|facts| facts.minimized),
        foreground_maximized: foreground.is_some_and(|facts| facts.maximized),
        foreground_fullscreen: foreground.is_some_and(|facts| facts.fullscreen),
        foreground_cloaked: foreground.is_some_and(|facts| facts.cloaked),
        foreground_hidden_workspace: foreground.is_some_and(|facts| facts.hidden_workspace),
        foreground_shell: foreground.is_some_and(|facts| facts.shell),
        foreground_frame: foreground.and_then(|facts| facts.frame),
        border_enabled: state.presentation_border_enabled,
        underlay_enabled: state.presentation_underlay_enabled,
        border_gap_px: state.presentation_style.0,
        border_width_px: state.presentation_style.1,
        underlay_extension_px: state.presentation_style.2,
        chord_held: state.presentation_chord,
        move_window: state.presentation_move.clone(),
        paused: state.presentation_paused,
        pager_order: order
            .into_iter()
            .map(tiler_core::directional::WindowId)
            .collect(),
        pager_frames: rows.into_values().collect(),
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
    let mut facts: Vec<WindowFact> = Vec::new();
    let mut closed_all: Vec<WindowId> = Vec::new();
    let mut held_members: Vec<String> = Vec::new();
    let mut inventory_complete = !enum_failed;
    // An unreadable fact (rect `None`) holds its staged scope instead of
    // authorizing geometry, never a silent absence: every withheld member
    // below rides one. Minimized facts ride separately with no geometry.
    macro_rules! unreadable_fact {
        ($facts:expr, $held:expr, $complete:expr, $token:expr, $mapped:expr, $sticky:expr) => {{
            $facts.push(WindowFact {
                window: WindowId($token.to_owned()),
                output: OutputId($mapped.clone()),
                rect: None,
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: $sticky,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
                minimized: false,
                // No facade readback on the default path: unknown, never
                // proof of absence.
                observed_workspace: None,
                native_workspace: None,
            });
            $held.push($token.to_owned());
            $complete = false;
        }};
    }
    macro_rules! minimized_fact {
        ($facts:expr, $token:expr, $mapped:expr, $sticky:expr) => {{
            $facts.push(WindowFact {
                window: WindowId($token.to_owned()),
                output: OutputId($mapped.clone()),
                rect: None,
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: $sticky,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
                minimized: true,
                observed_workspace: None,
                native_workspace: None,
            });
        }};
    }
    if enum_failed {
        // A failed enumeration proves nothing about any scope: every
        // inventoried scope holds below and completeness clears, even for
        // empty managed workspaces (which would otherwise read as a
        // successful empty and authorize cleanup).
    }

    // Screens ride the live monitor areas in native order plus the
    // mechanical device-to-wire map only. No rings/views/tiling/history:
    // the core mints and owns its logical workspaces outright.
    for area in areas {
        let mapped = state
            .output_tokens
            .get(&area.device)
            .cloned()
            .unwrap_or_else(|| "o?".to_owned());
        let mapped_valid = mapped != "o?";
        let inset_ok =
            crate::tiling::tiling_domain_bounds_with(area.work, state.outer_gap).is_some();
        let area_missing = !mapped_valid || !inset_ok;
        if area_missing {
            inventory_complete = false;
        }
        let screen_complete = !area_missing && !enum_failed;
        screens.push(ScreenReport {
            output: OutputId(mapped.clone()),
            full_rect: Some(area.full),
            work_area: Some(area.work),
            gap: state.inner_gap,
            outer_gap: state.outer_gap,
            complete: screen_complete,
            primary: areas.first().is_some_and(|a| a.device == area.device),
        });
    }
    let screen_outputs: HashSet<String> = screens.iter().map(|s| s.output.0.clone()).collect();
    let first_output: String = screens
        .first()
        .map(|s| s.output.0.clone())
        .unwrap_or_else(|| "o?".to_owned());
    // Output affinity for one frame: live placement geometry, never a
    // policy table. Unknown devices fall back to the first screen with
    // incompleteness marked by the caller.
    let affinity_of = |rect: &Rect| -> String {
        let device = super::directional_resolution::output_for_rect(areas, rect);
        state
            .output_tokens
            .get(&device)
            .cloned()
            .unwrap_or_else(|| first_output.clone())
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
        // unreadable with first-screen affinity; explicit queued closes
        // still ride beside the hold. Bare absence never becomes a close.
        for key in &keys {
            let sticky = state.sticky.contains_key(key);
            if let Some(token) = state.member_tokens.get(key) {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    first_output,
                    sticky
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
                // nothing.
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    bound,
                    affinity_of(&window.visible),
                    state.sticky.contains_key(&key)
                );
                continue;
            }
            let hidden = state.hidden_claims.contains_key(&key);
            // Scope fence: stored identity for bound members (drift holds),
            // live identity for brand-new windows.
            let exe_path: Option<String> = match state.member_identity.get(&key) {
                Some(identity)
                    if identity.pid == key.pid && identity.process_creation == key.creation =>
                {
                    Some(identity.exe_path.clone())
                }
                Some(_) => {
                    unreadable_fact!(
                        facts,
                        held_members,
                        inventory_complete,
                        token,
                        affinity_of(&window.visible),
                        state.sticky.contains_key(&key)
                    );
                    continue;
                }
                None => Some(window.identity.exe_path.clone()),
            };
            let exe_path = exe_path.unwrap_or_default();
            if !crate::tiling::scope_allows(state.scope, &exe_path) {
                // Scoped inventory ignores foreign (out-of-scope,
                // never-member) windows: they are not managed inventory
                // and must not fence scoped commands. A bound member
                // that drifts out of scope is still managed inventory,
                // so it holds narrowly as unreadable.
                if !state.member_tokens.contains_key(&key) {
                    continue;
                }
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    affinity_of(&window.visible),
                    state.sticky.contains_key(&key)
                );
                continue;
            }
            let sticky = state.sticky.contains_key(&key);
            let (fixed_auto, fixed_suppress) = capture_fixed(state, &key);
            if hidden {
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
                        affinity_of(&window.visible),
                        sticky
                    );
                    continue;
                }
                let record = state
                    .hidden_claims
                    .get(&key)
                    .expect("associated claim exists");
                if record.iconic
                    || record.claim.show.minimized
                    || state.minimized_omitted.contains(&key)
                {
                    minimized_fact!(facts, token, affinity_of(&window.visible), sticky);
                    continue;
                }
                if !capture_facts_rideable(&window.facts) {
                    unreadable_fact!(
                        facts,
                        held_members,
                        inventory_complete,
                        token,
                        affinity_of(&window.visible),
                        sticky
                    );
                    continue;
                }
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
                        affinity_of(&window.visible),
                        sticky
                    );
                    continue;
                };
                let mapped = affinity_of(&rect);
                let maximized = window.facts.maximized || record.claim.show.maximized;
                let fullscreen =
                    window.facts.captionless_fullscreen || state.born_fullscreen.contains(&key);
                let hints = state
                    .hint_logged
                    .get(&token)
                    .copied()
                    .unwrap_or_else(WindowSizeHints::none);
                facts.push(WindowFact {
                    window: WindowId(token.clone()),
                    output: OutputId(mapped),
                    rect: Some(rect),
                    floating,
                    fit_excluded: floating || fullscreen || sticky || maximized,
                    fullscreen,
                    maximized,
                    sticky,
                    fixed_auto: fixed_auto && !fixed_suppress,
                    fixed_suppress,
                    hints,
                    minimized: false,
                    observed_workspace: None,
                    native_workspace: None,
                });
                continue;
            }
            if !capture_facts_rideable(&window.facts) {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    token,
                    affinity_of(&window.visible),
                    sticky
                );
                continue;
            }
            let floating = capture_floating(state, &key, false);
            let mapped = affinity_of(&window.visible);
            let hints = state
                .hint_logged
                .get(&token)
                .copied()
                .unwrap_or_else(WindowSizeHints::none);
            facts.push(WindowFact {
                window: WindowId(token.clone()),
                output: OutputId(mapped),
                rect: Some(window.visible),
                floating,
                fit_excluded: floating
                    || sticky
                    || window.facts.captionless_fullscreen
                    || window.facts.maximized,
                fullscreen: window.facts.captionless_fullscreen,
                maximized: window.facts.maximized,
                sticky,
                fixed_auto: fixed_auto && !fixed_suppress,
                fixed_suppress,
                hints,
                minimized: false,
                observed_workspace: None,
                native_workspace: None,
            });
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
            if *live_token != row.token {
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    first_output.clone(),
                    state.sticky.contains_key(key)
                );
                continue;
            }
            if state.hidden_claims.contains_key(key) {
                continue;
            }
            let sticky = state.sticky.contains_key(key);
            let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
            if state.minimized_omitted.contains(key)
                && row.rect.is_none()
                && !row.maximized
                && !row.fullscreen
            {
                let mapped = state
                    .member_rects
                    .get(live_token)
                    .map(affinity_of)
                    .unwrap_or_else(|| first_output.clone());
                minimized_fact!(facts, live_token, mapped, sticky);
                continue;
            }
            let Some(row_facts) = row.facts else {
                let mapped = row
                    .rect
                    .as_ref()
                    .map(affinity_of)
                    .unwrap_or_else(|| first_output.clone());
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    mapped,
                    sticky
                );
                continue;
            };
            if !capture_facts_rideable(&row_facts) {
                let mapped = row
                    .rect
                    .as_ref()
                    .map(affinity_of)
                    .unwrap_or_else(|| first_output.clone());
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    mapped,
                    sticky
                );
                continue;
            }
            let overlay = row.maximized || row.fullscreen;
            let kept = state.member_rects.get(live_token).copied();
            let rect = match (overlay, row.rect, kept) {
                (true, _, Some(slot)) => Some(slot),
                (_, Some(fresh), _) => Some(fresh),
                _ => None,
            };
            let Some(rect) = rect else {
                let mapped = row
                    .rect
                    .as_ref()
                    .map(affinity_of)
                    .unwrap_or_else(|| first_output.clone());
                unreadable_fact!(
                    facts,
                    held_members,
                    inventory_complete,
                    live_token,
                    mapped,
                    sticky
                );
                continue;
            };
            let floating = capture_floating(state, key, false);
            let mapped = affinity_of(&rect);
            let hints = crate::tiling::retained_overlay_hint(
                state.hint_logged.get(live_token).copied(),
                true,
                overlay,
                floating,
                state.born_fullscreen.contains(key),
                true,
            );
            facts.push(WindowFact {
                window: WindowId(live_token.clone()),
                output: OutputId(mapped),
                rect: Some(rect),
                floating,
                fit_excluded: floating || sticky || row.fullscreen || row.maximized,
                fullscreen: row.fullscreen,
                maximized: row.maximized,
                sticky,
                fixed_auto: fixed_auto && !fixed_suppress,
                fixed_suppress,
                hints,
                minimized: false,
                observed_workspace: None,
                native_workspace: None,
            });
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
            let sticky = state.sticky.contains_key(key);
            let (fixed_auto, fixed_suppress) = capture_fixed(state, key);
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
                    first_output.clone(),
                    sticky
                );
                continue;
            }
            if record.iconic || record.claim.show.minimized || state.minimized_omitted.contains(key)
            {
                let mapped = state
                    .member_rects
                    .get(live_token)
                    .map(affinity_of)
                    .unwrap_or_else(|| first_output.clone());
                minimized_fact!(facts, live_token, mapped, sticky);
                continue;
            }
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
                    first_output.clone(),
                    sticky
                );
                continue;
            };
            let mapped = affinity_of(&rect);
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
            let hints = state
                .hint_logged
                .get(live_token)
                .copied()
                .unwrap_or_else(WindowSizeHints::none);
            facts.push(WindowFact {
                window: WindowId(live_token.clone()),
                output: OutputId(mapped),
                rect: Some(rect),
                floating,
                fit_excluded: floating || fullscreen || sticky || maximized,
                fullscreen,
                maximized,
                sticky,
                fixed_auto: fixed_auto && !fixed_suppress,
                fixed_suppress,
                hints,
                minimized: false,
                observed_workspace: None,
                native_workspace: None,
            });
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
            let mapped = state
                .member_rects
                .get(live_token)
                .map(affinity_of)
                .unwrap_or_else(|| first_output.clone());
            minimized_fact!(facts, live_token, mapped, state.sticky.contains_key(key));
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
                first_output.clone(),
                state.sticky.contains_key(key)
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

    // Topology from the same device order the owner syncs: live backing ids
    // in native order, the current view, the live full output geometry
    // Full rectangles plus work areas are the hotplug placement facts
    // (never a frame proxy) and the primary flag. `all_monitors` orders
    // primary-first, so primary derives from the `areas.first()` device:
    // managed output order may be reordered relative to the native list
    // and must never decide it.
    // Screens carry no native workspace vocabulary: FULL rectangles plus
    // work areas only. The core mints and owns its logical workspaces.
    let inventory_complete = inventory_complete
        && !screens.is_empty()
        && !facts
            .iter()
            .any(|fact| fact.rect.is_none() && !fact.minimized);
    // Truthful applied receipts (never echoed desire): per-window logical
    // homing only where fresh identity/visibility proves it, per-screen
    // views only where exactly one non-sticky workspace shows members (or a
    // memberless active shows provable emptiness). Unknown stays unknown so
    // the core retains pending flights instead of settling on a guess. An
    // empty workspace has no windows, so its view never proves here by
    // echo; the actuation batch proves it by hiding every prior member.
    let mut windows = facts;
    super::world_native_receipts::attach_workspace_receipts(
        state,
        observed,
        retained,
        areas,
        &mut windows,
    );
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
            .map(|rect| super::directional_resolution::output_for_rect(areas, &rect))
            .and_then(|device| state.output_tokens.get(&device).cloned())
            .map(OutputId)
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
    CapturedWorld {
        screens,
        windows,
        closed,
        observed_views: super::world_native_receipts::observed_view_receipts(
            state, observed, areas,
        ),
        workspace_mode: crate::settings::startup_mode(state.settings_live.as_ref()),
        equal_workspace_count: false,
        inventory_complete,
        gesture_barrier: !state.active.is_empty(),
        gesture: state.gesture.clone(),
        focused_global: observed
            .iter()
            .find(|w| w.hwnd == foreground_hwnd)
            .map(|w| w.token.clone())
            .unwrap_or_default(),
        action_foreground,
        action_cached,
        action_outputs,
        default_tiled: state
            .settings_live
            .as_ref()
            .map(|live| live.settings.core.workspace.default_tiled)
            .unwrap_or(true),
        fixed_size_predicate: admission::fixed_predicate_for(state.settings_live.as_ref()),
        migration_source_refill: state
            .settings_live
            .as_ref()
            .map(|live| live.settings.core.migration_source_refill_mode())
            .unwrap_or_default(),
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
        dropped_stale_closed,
        #[cfg(test)]
        unhomed_closed,
        held_members,
        presentation: presentation_ingress(state, Some(observed), retained),
    }
}
