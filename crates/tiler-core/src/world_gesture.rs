//! Retained gesture lifecycle (stage 4a): normalized ingress, core-owned
//! target/overlay/release, and edge-share policy.
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. The adapter reports one
//! [`GestureIngress`](crate::gesture::GestureIngress) per report while a
//! native move/size gesture is open (exact window lifetime, stable gesture
//! id, START-sampled kind, live and START cursors, terminal phase). The
//! core retains the session, resolves the pointer target, publishes the
//! drop-overlay shape through the existing
//! [`CommandResult::DragPreview`](crate::world_report::CommandResult),
//! and commits the release over the existing session drag/resize
//! machinery. The adapter only observes, captures, renders, and applies.
//!
//! No second policy: preview and release synthesize the same [`CoreCommand`]
//! shapes the operation route carries and run them through the same
//! [`Engine`] resolver, without consuming a command sequence (gesture
//! ingress is observation, never a command). Cross-output releases run
//! the same source-first pair drop the preview showed (live coordinates,
//! targeted slot and orientation, membership move plus follow); empty
//! destinations and points the pair cannot place keep the send
//! admission. Same-output releases keep the resolver placement. The
//! adapter keeps only native lifetime/freshness/input fences plus the
//! render-hide acknowledgement around the committed reply; at-most-once
//! and completion effects are core-owned through the stable gesture id
//! and the drained terminal record.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::gesture::{
    GestureIngress, GestureIntent, GestureKind, GesturePhase, classify_gesture, prefer_modal_move,
    resolve_drop_destination,
};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, DragHoverPrior, OutputDomain};
use crate::world_engine::{RetainedTransfer, WorldEngine};
use crate::world_report::{CommandResult, TransferKind, TransferTarget, WorldReport};

/// Retained gesture session: one open native move/size gesture bound to an
/// exact window lifetime plus its stable adapter gesture id. `start_frame`
/// is the observed frame from the starting report (release-intent ground
/// truth); `hover` is the carried sticky group-edge prior the preview
/// resolver owns; `home` is the START-validated logical homing (never
/// rebound mid-gesture). `start_cursor` is the true native down point when
/// the adapter read it (an explicit `Started` or the carried START truth),
/// so the zero-movement gate never depends on a lost Started report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedGesture {
    pub(crate) window: WindowId,
    pub(crate) id: u64,
    pub(crate) kind: GestureKind,
    pub(crate) start_seq: u64,
    pub(crate) start_frame: crate::geometry::Rect,
    pub(crate) start_cursor: Option<(i32, i32)>,
    pub(crate) start_exact: bool,
    pub(crate) modal_move: bool,
    pub(crate) hover: Option<DragHoverPrior>,
    pub(crate) home: DomainKey,
}

/// Outcome of one report's gesture handling: `Absent` runs no gesture
/// work; `Held` retains the session with no commit and no overlay;
/// `Settled` processed a preview or terminal phase (overlay result when
/// `Some`, boxed: the overlay plan dwarfs the other arms), committing
/// sessions only when `committed` (the caller then resweeps and restages
/// before publishing, exactly like the command path).
pub(crate) enum GestureOutcome {
    Absent,
    Held,
    Settled {
        committed: bool,
        result: Option<Box<CommandResult>>,
    },
}

impl WorldEngine {
    /// Stable gesture operation identity: the window plus its starting
    /// observation sequence. Retained transfers settle on arrival proof;
    /// the identity stays unique across reports without consuming command
    /// sequences.
    fn gesture_operation_id(retained: &RetainedGesture) -> String {
        format!("gesture-{}-{}", retained.window.0, retained.start_seq)
    }

    /// Bind one report's normalized gesture ingress to a retained session
    /// (stage 4a, observation half). Runs after staging but before
    /// settlement and the sweep, so the sweep converges with the shielded
    /// homing (the mover stays in its START session even when observed
    /// away) and the later release commits against intact sessions.
    /// Validates the exact lifetime, readable fact, complete inventory,
    /// and unheld tiled homing; invalid starts fail closed with no
    /// session. Restages on success so settlement and the sweep read the
    /// shield. Never commits, never previews: the act half runs after
    /// the sweep.
    ///
    /// At-most-once by stable gesture id, never by window plus frame:
    /// - An ingress at or below the consumed watermark is a replay and
    ///   refuses without binding (terminal or not; Updated/Started
    ///   replays never clear the watermark).
    /// - A fresh id (`Started`, or an implicit terminal on one wake)
    ///   binds when the start validates. A terminal that fails to bind
    ///   still consumes its id and records an uncommitted terminal, so a
    ///   retry can never commit.
    /// - A fresh id newer than a dead or held retained session
    ///   supersedes it: the old session is replaced without waiting for
    ///   an old terminal that will never come.
    /// - A `Cancelled` phase with no retained session consumes its id
    ///   and records without binding (no shield for a dead gesture).
    pub(crate) fn gesture_bind(
        &mut self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        let staged_seq = self.staged.observation.observation_seq;
        let ingress_owned = self.staged.observation.gesture.clone();
        let ingress = match ingress_owned.as_ref() {
            Some(ingress) if ingress.esc => {
                // Host-owned Esc/cancel fact clears only the matching
                // retained session, immediately, even before the sweep. A
                // stale Esc for another id never clears a newer session,
                // but still consumes its own id so a retry stays dead.
                if let Some(retained) = self.gesture.clone()
                    && retained.window == ingress.window
                    && retained.id == ingress.id
                {
                    self.gesture = None;
                    self.record_gesture_terminal(ingress.id, false);
                } else {
                    self.record_gesture_terminal(ingress.id, false);
                }
                return false;
            }
            Some(ingress) => ingress,
            None => return false,
        };
        if self.gesture_replayed(ingress.id) {
            // Consumed replay, terminal or not (id 0 included: the
            // watermark starts at 0, so a missing id can never bind):
            // never rebind, never clear the watermark.
            return false;
        }
        if let Some(retained) = self.gesture.clone() {
            if retained.window == ingress.window && retained.id == ingress.id {
                return true;
            }
            if ingress.id != 0 && ingress.id > self.consumed_gesture_seq && ingress.id > retained.id
            {
                // Fresh id supersedes the dead/held session below: fall
                // through and replace it.
            } else {
                // Stale newcomer while one is retained: hold the old
                // session and ignore, never a silent replacement.
                return true;
            }
        }
        if ingress.phase == GesturePhase::Cancelled {
            // A cancel with no retained session consumes its id and
            // records an uncommitted terminal without binding: no shield
            // for a dead gesture, and a retry stays dead.
            self.record_gesture_terminal(ingress.id, false);
            return false;
        }
        // Implicit bind: a terminal or update phase with no retained
        // session binds from the carried start frame (START and END may
        // share one wake). The home is the retained membership homing
        // when known (never yanked by the observed screen), else the
        // staged homing for a genuinely new lifetime.
        let home = self
            .retained_homing(&ingress.window)
            .or_else(|| self.staged.homing.get(&ingress.window).cloned());
        let Some(home) = home else {
            if ingress.phase == GesturePhase::Released {
                // Unbindable terminal still consumes its identity and
                // records, so a retry can never commit.
                self.record_gesture_terminal(ingress.id, false);
            }
            return false;
        };
        if !self.gesture_start_valid(ingress, &home, held_scopes) {
            if matches!(
                ingress.phase,
                GesturePhase::Released | GesturePhase::Cancelled
            ) {
                // Unbindable terminal still consumes its identity and
                // records, so a retry can never commit.
                self.record_gesture_terminal(ingress.id, false);
            }
            return false;
        }
        // The START cursor rides every ingress (never derived from the
        // live sample), so a first Updated after a lost Started keeps the
        // true zero gate. Exact exactly when the down point was read.
        let start_cursor = ingress.start_cursor.or(ingress.cursor);
        self.gesture = Some(RetainedGesture {
            window: ingress.window.clone(),
            id: ingress.id,
            kind: ingress.kind,
            start_seq: staged_seq,
            start_frame: ingress.start_frame,
            start_cursor,
            start_exact: ingress.start_cursor.is_some(),
            modal_move: ingress.modal_move,
            hover: None,
            home,
        });
        // Restage so settlement and the sweep read the shielded homing.
        self.build_staged_domains(report);
        true
    }

    /// Act on one report's normalized gesture ingress against the retained
    /// session (stage 4a, commit half). Runs after the sweep: refreshes
    /// the pointer target and drop-overlay shape on updates, commits the
    /// release, and clears on cancel. Never rejects the report.
    ///
    /// Identity is the stable gesture id, never the window alone: only an
    /// ingress naming the retained window AND id acts on the session. A
    /// stale ingress (older id, or any id at/below the watermark while a
    /// newer session is retained) holds the session and is ignored; a
    /// stale terminal additionally consumes and records without mutating,
    /// so the adapter drain can clear its stashed override. Every
    /// incorporated terminal records exactly one precise result
    /// (committed or not) for honest adapter logging.
    ///
    /// The START cursor and modal truth are immutable once bound (the
    /// first ingress owns them): later updates never overwrite, so the
    /// zero-movement gate compares every live sample against the true
    /// down point. A missing ingress while a session is retained holds
    /// the session, except an explicit close of the retained lifetime,
    /// which clears exactly (a hidden or incomplete departure holds,
    /// never a false close).
    pub(crate) fn gesture_act(
        &mut self,
        report: &WorldReport,
        issue: &crate::world_engine::EventIssue,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let ingress = match self.staged.observation.gesture.clone() {
            Some(ingress) if ingress.esc => {
                // Host-owned Esc/cancel fact: clears only the matching
                // session regardless of phase. Never commits; the native
                // move already restored. A stale Esc still consumes its
                // own id so a retry stays dead, but never clears newer.
                if let Some(retained) = self.gesture.clone()
                    && retained.window == ingress.window
                    && retained.id == ingress.id
                {
                    self.gesture = None;
                    self.record_gesture_terminal(ingress.id, false);
                    return GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    };
                }
                self.record_gesture_terminal(ingress.id, false);
                return GestureOutcome::Absent;
            }
            Some(ingress) => Some(ingress),
            None => None,
        };
        match (self.gesture.clone(), ingress) {
            (None, None) => GestureOutcome::Absent,
            (None, Some(_)) => {
                // No retained session: the bind half either refused this
                // ingress (replay, unbindable terminal already consumed
                // and recorded there) or ran on an earlier report whose
                // session has since cleared. Either way there is nothing
                // to act on.
                GestureOutcome::Absent
            }
            (Some(retained), None) => {
                // A missing ingress while a session is retained holds the
                // session, never an implicit cancel: only an explicit
                // close clears here (a cancel phase or Esc clears above).
                // The next ingress continues the gesture. Hidden or
                // incomplete departures hold, never a false close.
                if self.staged.closes.contains(&retained.window) {
                    self.gesture = None;
                    self.record_gesture_terminal(retained.id, false);
                    return GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    };
                }
                GestureOutcome::Held
            }
            (Some(retained), Some(ingress)) => {
                if ingress.window != retained.window || ingress.id != retained.id {
                    // A different lifetime or a stale id while one is
                    // retained holds the old session and ignores the
                    // newcomer: the adapter fails multi-gestures closed
                    // natively, so this is unreachable in production,
                    // never a silent replacement. A stale terminal still
                    // consumes and records without mutating, so the
                    // adapter drain can clear its stashed override; the
                    // newer session is untouched.
                    if matches!(
                        ingress.phase,
                        GesturePhase::Released | GesturePhase::Cancelled
                    ) {
                        self.record_gesture_terminal(ingress.id, false);
                    }
                    self.gesture = Some(retained);
                    return GestureOutcome::Held;
                }
                match ingress.phase {
                    // A repeated start changes nothing retained: the
                    // START cursor and modal truth are immutable once
                    // bound. An explicit start on its bind report holds
                    // (bind only, no overlay at the down point); every
                    // other sample previews through the update path.
                    GesturePhase::Started
                        if retained.start_seq == self.staged.observation.observation_seq =>
                    {
                        GestureOutcome::Held
                    }
                    GesturePhase::Started | GesturePhase::Updated => {
                        self.gesture_updated(issue, &ingress, held_scopes)
                    }
                    GesturePhase::Released => {
                        self.gesture_released(report, issue, &ingress, held_scopes)
                    }
                    GesturePhase::Cancelled => {
                        self.gesture = None;
                        self.record_gesture_terminal(retained.id, false);
                        GestureOutcome::Settled {
                            committed: false,
                            result: None,
                        }
                    }
                }
            }
        }
    }

    /// Validate one gesture start: exact known lifetime, readable fact,
    /// complete inventory, readable unheld tiled homing, and no overlay
    /// or sticky lane. Unmanaged, minimized, hidden (unreadable),
    /// fullscreen/maximized, ordinary floating, or held starts fail closed
    /// with no retained session. Owned core-composed floats (unsupported
    /// maximize, overlay bit set, retained float exception) bind and
    /// continue so an explicit user drag unmaximizes to the release
    /// placement. `home` is the retained membership homing when
    /// known (never yanked by the observed screen), else the staged
    /// homing for a genuinely new lifetime.
    fn gesture_start_valid(
        &self,
        ingress: &GestureIngress,
        home: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if !self.staged.observation.inventory_complete {
            return false;
        }
        if self.staged.closes.contains(&ingress.window) {
            return false;
        }
        let Some(domain) = self.staged.domains.iter().find(|d| d.key == *home) else {
            return false;
        };
        if !domain.readable || held_scopes.contains(home) {
            return false;
        }
        if self
            .tiling
            .get(home)
            .is_some_and(|mode| *mode != crate::world_report::TilingMode::Tiled)
        {
            return false;
        }
        let Some(fact) = self.staged.facts.get(&ingress.window) else {
            return false;
        };
        let derived = self.derived_for(&ingress.window);
        if derived.hidden || derived.hold || derived.excluded {
            return false;
        }
        if fact.rect.is_none() || fact.minimized == Some(true) {
            return false;
        }
        if derived.overlay_fullscreen || derived.overlay_maximized {
            return false;
        }
        if derived.floating || derived.sticky {
            // Composed-float-only permit: an owned core-composed overlay
            // on a float base binds and continues; ordinary floats and
            // sticky still refuse with no session.
            if !self.is_owned_composed_float(&ingress.window) {
                return false;
            }
        }
        true
    }

    /// Fences for a continuing gesture report: the retained lifetime must
    /// still be live, readable, unminimized, homed where it started, and
    /// inside a readable unheld scope under complete inventory. Gaps hold;
    /// explicit closes clear (handled by the caller).
    fn gesture_continue_fences(
        &self,
        retained: &RetainedGesture,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if !self.staged.observation.inventory_complete {
            return false;
        }
        if self.staged.closes.contains(&retained.window) {
            return false;
        }
        let Some(home) = self.staged.homing.get(&retained.window) else {
            return false;
        };
        if home != &retained.home {
            return false;
        }
        let Some(domain) = self.staged.domains.iter().find(|d| d.key == *home) else {
            return false;
        };
        if !domain.readable || held_scopes.contains(home) {
            return false;
        }
        let Some(fact) = self.staged.facts.get(&retained.window) else {
            return false;
        };
        let derived = self.derived_for(&retained.window);
        if derived.hidden || derived.hold || derived.excluded {
            return false;
        }
        if fact.rect.is_none() || fact.minimized == Some(true) {
            return false;
        }
        if derived.overlay_fullscreen || derived.overlay_maximized {
            return false;
        }
        if derived.floating || derived.sticky {
            // Composed-float-only permit, mirroring the start gate above.
            if !self.is_owned_composed_float(&retained.window) {
                return false;
            }
        }
        true
    }

    /// Whether one window is an owned core-composed float: `caps.maximize`
    /// unsupported, the lifetime-local overlay bit set, a retained floating
    /// (non-sticky) exception, and no native overlay. The composed permit
    /// below keys off exactly this: ordinary floats and sticky never pass.
    fn is_owned_composed_float(&self, window: &WindowId) -> bool {
        if self.capabilities().is_some_and(|caps| caps.maximize) {
            return false;
        }
        if !self.composed_maximized.contains(window) {
            return false;
        }
        let derived = self.derived_for(window);
        if !derived.floating || derived.sticky {
            return false;
        }
        if derived.overlay_fullscreen || derived.overlay_maximized {
            return false;
        }
        self.engine.domain_keys().iter().any(|key| {
            self.engine
                .session(key)
                .is_some_and(|session| session.is_exception(window))
        })
    }

    /// Release-frame ground truth for the retained window: the staged
    /// observed frame, when readable.
    fn gesture_release_frame(&self, retained: &RetainedGesture) -> Option<crate::geometry::Rect> {
        self.staged
            .facts
            .get(&retained.window)
            .and_then(|fact| fact.rect)
    }

    /// Whether the staged observation runs under a focused fullscreen game
    /// pause: the staged focused lifetime names a different window whose
    /// staged fact is fullscreen. Normalized staged facts only, never
    /// platform reads. Under the pause no overlay publishes and no release
    /// commits; the session holds for the terminal phase. The gesture
    /// subject itself is exempt: a focused fullscreen subject still acts.
    fn gesture_game_paused(&self, retained: &RetainedGesture) -> bool {
        if self.staged.observation.focused_global.is_empty()
            || self.staged.observation.focused_global == retained.window.0
        {
            return false;
        }
        let focused = WindowId(self.staged.observation.focused_global.clone());
        self.staged
            .facts
            .get(&focused)
            .is_some_and(|fact| fact.fullscreen == Some(true))
    }

    /// Handle an Updated phase: refresh the pointer target and the
    /// drop-overlay shape. Move gestures with a readable cursor resolve
    /// through the same Engine preview the operation route uses;
    /// cursor-less updates hold with no overlay work. Resize gestures
    /// never publish an overlay. Fence failures suppress the overlay but
    /// retain the session for the terminal phase. The hover prior
    /// advances only on a published overlay, never on a hold: the core
    /// owns every gate (zero, game, fences, Engine refusal), so the
    /// adapter never needs to second-guess a published result.
    fn gesture_updated(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        ingress: &GestureIngress,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let retained = self
            .gesture
            .clone()
            .expect("updated runs with a retained session");
        if self.staged.closes.contains(&retained.window) {
            self.gesture = None;
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        if !self.gesture_continue_fences(&retained, held_scopes) {
            return GestureOutcome::Held;
        }
        if self.gesture_game_paused(&retained) {
            return GestureOutcome::Held;
        }
        if self.is_owned_composed_float(&retained.window) {
            // Owned composed floats publish no drag preview: the overlay
            // is already the work area, and the release adopts the native
            // frame. Hold the session for the terminal phase.
            return GestureOutcome::Held;
        }
        if retained.kind != GestureKind::Move {
            return GestureOutcome::Held;
        }
        let Some((cursor_x, cursor_y)) = ingress.cursor else {
            // No cursor work without a cursor: hold the session, publish
            // no overlay.
            return GestureOutcome::Held;
        };
        if Some((cursor_x, cursor_y)) == retained.start_cursor && retained.start_exact {
            // Zero movement against the immutable START point: the
            // gesture continues, the next moved sample may show. Only
            // exact sessions gate here: an implicit bind never saw the
            // down point, so its first live sample cannot prove zero.
            return GestureOutcome::Held;
        }
        // Cross-output points resolve through the pair preview (occupied
        // destinations) exactly like the operation route; same-output
        // points keep the local route. The pointer domain resolves over
        // staged work-area bounds, matching `operation_drag`. The pair
        // route carries the capture domain as its source; the local route
        // carries none.
        let preview = match self.drag_pointer_domain_staged(cursor_x, cursor_y, held_scopes) {
            Some((pointer_key, _)) if pointer_key != retained.home => {
                let command = CoreCommand::DragPreview {
                    window: retained.window.0.clone(),
                    x: cursor_x,
                    y: cursor_y,
                    hover_prior: retained.hover.clone(),
                    source: Some(retained.home.clone()),
                };
                return match self.gesture_pair_preview(
                    issue,
                    &retained,
                    &command,
                    &pointer_key,
                    held_scopes,
                ) {
                    Some(result) => {
                        if let Some(CommandResult::DragPreview { hover, .. }) = result.as_ref()
                            && let Some(session) = self.gesture.as_mut()
                        {
                            session.hover = hover.clone();
                        }
                        GestureOutcome::Settled {
                            committed: false,
                            result: result.map(Box::new),
                        }
                    }
                    None => GestureOutcome::Held,
                };
            }
            _ => {
                let command = CoreCommand::DragPreview {
                    window: retained.window.0.clone(),
                    x: cursor_x,
                    y: cursor_y,
                    hover_prior: retained.hover.clone(),
                    source: None,
                };
                self.gesture_local_event(issue, &retained, &command, held_scopes)
            }
        };
        let Some(event) = preview else {
            return GestureOutcome::Held;
        };
        match self.engine.handle(&event) {
            CoreReply::DragPreview(plan) => {
                let result =
                    Self::command_result_from_reply(&event.command, &CoreReply::DragPreview(plan));
                if let Some(CommandResult::DragPreview { hover, .. }) = result.as_ref()
                    && let Some(session) = self.gesture.as_mut()
                {
                    session.hover = hover.clone();
                }
                GestureOutcome::Settled {
                    committed: false,
                    result: result.map(Box::new),
                }
            }
            // Engine refusals (self/centre/outside/unchanged) and session
            // mismatches suppress the overlay but retain the session.
            _ => GestureOutcome::Held,
        }
    }

    /// Handle a Released phase: derive the release intent from the
    /// retained start frame versus the release frame plus cursor (modal
    /// truth overriding a rescale-misread resize into a move, exactly like
    /// the retired adapter derivation) and commit a move or resize
    /// release. Terminal always: every path clears the retained session,
    /// consumes its id, and records exactly one precise result
    /// (`committed` true only on a real move/resize commit), so the
    /// adapter logs completion honestly instead of equating the reply
    /// with the commit. Cursor-less resizes never commit (a native resize
    /// always carries a pointer position; without one fail closed). Under
    /// the focused-fullscreen game pause nothing commits.
    fn gesture_released(
        &mut self,
        report: &WorldReport,
        issue: &crate::world_engine::EventIssue,
        ingress: &GestureIngress,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let retained = self
            .gesture
            .clone()
            .expect("released runs with a retained session");
        self.gesture = None;
        if self.gesture_game_paused(&retained) {
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        if self.staged.closes.contains(&retained.window)
            || !self.gesture_continue_fences(&retained, held_scopes)
        {
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        let Some(release_frame) = self.gesture_release_frame(&retained) else {
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        };
        let classified = classify_gesture(&retained.start_frame, &release_frame, ingress.cursor);
        let intent =
            classified.map(|intent| prefer_modal_move(intent, ingress.cursor, retained.modal_move));
        // Cursor-less resize intents never commit: fail closed with no
        // commit on the already-cleared session.
        if ingress.cursor.is_none() && matches!(intent, Some(GestureIntent::PointerResize { .. })) {
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        let owned = self.staged_carriers();
        let by_id: BTreeMap<&WindowId, &EngineWindow> = by_id_of(&owned);
        // Owned composed floats never route into the tiled drag admission:
        // a real move/resize adopts the staged release frame as the new
        // Session float placement, with no Engine handle, no transfer,
        // and no placement token. A release observed on another output
        // adopts in that output's effective view through the existing
        // membership owner.
        let owned_float = self.is_owned_composed_float(&retained.window);
        let outcome = match intent {
            Some(GestureIntent::MoveDrop { x, y }) if !owned_float => {
                self.gesture_move_release(issue, &retained, x, y, &by_id, held_scopes)
            }
            Some(GestureIntent::PointerResize {
                direction,
                boundary,
                direction2,
                boundary2,
            }) if !owned_float => self.gesture_resize_release(
                issue,
                &retained,
                direction,
                boundary,
                direction2,
                boundary2,
                held_scopes,
            ),
            Some(GestureIntent::MoveDrop { .. }) | Some(GestureIntent::PointerResize { .. }) => {
                self.gesture_float_release(report, issue, &retained, release_frame, held_scopes)
            }
            None => {
                // No change (or a cursor-less move): snap back with no
                // commit. The published snapshot reasserts accepted state.
                GestureOutcome::Settled {
                    committed: false,
                    result: None,
                }
            }
        };
        // Exactly one precise terminal record per incorporated release:
        // `committed` is true only on a real move/resize commit. A
        // committed explicit user move/resize clears the lifetime-local
        // core-composed maximize bit before the caller's resweep/restage
        // and publication, so the reflow publishes the committed base.
        // An explicit composed-float drag to a valid but empty
        // destination clears the bit inside the float lane before
        // destination admission even though the release refuses with no
        // commit, so the reflow publishes the ordinary dragged base.
        // No-op, cancel, refused, and cursor-less terminals never clear;
        // delayed/clamped `FrameAttempt` readbacks never reach this lane.
        // The tiled base is whatever the committed Session drag/resize
        // already retained; the composed-float base is the adopted release
        // frame: no placement token is minted on either lane, unlike the
        // explicit toggle-off restore lane.
        let committed = matches!(
            outcome,
            GestureOutcome::Settled {
                committed: true,
                ..
            }
        );
        if committed {
            self.composed_maximized.remove(&retained.window);
        }
        self.record_gesture_terminal(retained.id, committed);
        outcome
    }

    /// Commit a move release: same-output points keep the resolver
    /// placement through the local drop; cross-output points run the
    /// pair-targeted drop at the live coordinates (the slot and
    /// orientation the preview showed), retaining the window transfer
    /// with member/follow settlement exactly like the send path. Empty
    /// destinations and points the pair cannot place keep the send
    /// admission. Unplaceable points refuse with snap-back and no
    /// transfer. The committed reply publishes the desired geometry and
    /// the retained transfer plus focus effects; the adapter applies that
    /// reply mechanically with fresh proof.
    fn gesture_move_release(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        x: i32,
        y: i32,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let fulls: Vec<(OutputId, crate::geometry::Rect)> = self
            .live_topology_entries()
            .into_iter()
            .map(|entry| (entry.id, entry.rect))
            .collect();
        let home_key = retained.home.clone();
        let home_inset = self
            .staged
            .domains
            .iter()
            .find(|d| d.key == home_key)
            .and_then(|domain| inset_bounds_of(&home_key, domain));
        let destination = resolve_drop_destination(
            &fulls,
            &home_key.output,
            x,
            y,
            home_inset,
            |output| self.gesture_dest_tiled(output),
            |output| self.gesture_dest_inset(output, held_scopes),
        );
        match destination {
            crate::gesture::DropDestination::SameOutput => {
                let command = CoreCommand::DragDrop {
                    window: retained.window.0.clone(),
                    x,
                    y,
                    hover_prior: retained.hover.clone(),
                    source: None,
                };
                let Some(event) = self.gesture_local_event(issue, retained, &command, held_scopes)
                else {
                    return GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    };
                };
                match self.engine.handle(&event) {
                    CoreReply::Tiled(_) => GestureOutcome::Settled {
                        // The Engine committed the resolved placement
                        // inside `handle`; the snapshot below publishes
                        // the desired geometry for native actuation.
                        committed: true,
                        result: None,
                    },
                    _ => GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    },
                }
            }
            crate::gesture::DropDestination::Cross { output } => {
                let Some(workspace) = self.effective_view(&output) else {
                    return GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    };
                };
                let operation_id = Self::gesture_operation_id(retained);
                // Attempt the pair-targeted drop at the live coordinates
                // (the slot the preview showed). Anything the pair cannot
                // place falls through to the send admission below, which
                // also serves empty destinations; send refusals snap back
                // as before.
                if let Some(outcome) = self.gesture_pair_drop(
                    issue,
                    retained,
                    &operation_id,
                    &output,
                    &workspace,
                    x,
                    y,
                    held_scopes,
                ) {
                    return outcome;
                }
                match self.execute_output_send(
                    issue,
                    &operation_id,
                    &retained.window,
                    &output,
                    &workspace,
                    true,
                    by_id,
                    held_scopes,
                ) {
                    Ok((transfer, _)) => {
                        if let TransferKind::Window(window) = &transfer.target.kind {
                            let target = DomainKey {
                                output: transfer.target.target_output.clone(),
                                workspace: WorkspaceId(transfer.target.target_workspace.clone()),
                            };
                            self.commit_send_member(window, &target);
                        }
                        self.retained.push(transfer);
                        GestureOutcome::Settled {
                            committed: true,
                            result: None,
                        }
                    }
                    Err(_) => GestureOutcome::Settled {
                        committed: false,
                        result: None,
                    },
                }
            }
            crate::gesture::DropDestination::Refused(_) => GestureOutcome::Settled {
                committed: false,
                result: None,
            },
        }
    }

    /// Whether one output can receive a cross-output gesture drop: its
    /// ring head names a tiled workspace. `None` when the output owns no
    /// ring (the resolver refuses with no-view).
    fn gesture_dest_tiled(&self, output: &OutputId) -> Option<bool> {
        let ring = self.mapping.get(&output.0)?;
        let head = ring.first()?;
        let key = DomainKey {
            output: output.clone(),
            workspace: WorkspaceId(head.clone()),
        };
        Some(
            self.tiling
                .get(&key)
                .copied()
                .unwrap_or(crate::world_report::TilingMode::Tiled)
                == crate::world_report::TilingMode::Tiled,
        )
    }

    /// Inset work-area bounds for one cross-output drop destination: the
    /// current view's staged bounds inset once, readable and unheld only.
    /// `None` refuses with unknown-output.
    fn gesture_dest_inset(
        &self,
        output: &OutputId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<crate::geometry::Rect> {
        let view = self.effective_view(output)?;
        let domain = self
            .staged
            .domains
            .iter()
            .filter(|d| d.readable && !held_scopes.contains(&d.key))
            .find(|d| d.key.output == *output && d.key.workspace.0 == view)?;
        inset_bounds_of(&domain.key, domain)
    }

    /// Commit a resize release: single-axis and corner pointer resizes run
    /// the same Engine path the operation route uses (including the
    /// existing share policy), with no command consumption. Unchanged or
    /// refused derivations snap back with no commit.
    #[allow(clippy::too_many_arguments)]
    fn gesture_resize_release(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        direction: &str,
        boundary: i32,
        direction2: Option<&str>,
        boundary2: Option<i32>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let command = CoreCommand::PointerResize {
            window: retained.window.0.clone(),
            direction: direction.to_owned(),
            boundary,
            direction2: direction2.map(str::to_owned),
            boundary2,
        };
        let Some(event) = self.gesture_local_event(issue, retained, &command, held_scopes) else {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        };
        // The mover must still home where the gesture started:
        // membership drift between start and release snaps back.
        if event.domain_key != retained.home {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        match self.engine.handle(&event) {
            CoreReply::Tiled(_)
            | CoreReply::Projection(_)
            | CoreReply::Resize(_)
            | CoreReply::DragPreview(_) => GestureOutcome::Settled {
                committed: true,
                result: None,
            },
            _ => GestureOutcome::Settled {
                committed: false,
                result: None,
            },
        }
    }

    /// Commit an owned composed-float release: adopt the staged release
    /// frame as the new Session float placement (live geometry plus the
    /// durable retained copy), revoke any stale same-window placement
    /// token the explicit lanes minted, and report committed so the caller
    /// clears the overlay bit before resweep/publication. The destination
    /// is the actual [`Self::logical_homing`] for the release report: the
    /// release runs after the retained gesture clears, so the homing sees
    /// the native receipt, held, and unknown evidence exactly like any
    /// other observation. Same-homing adopts in place; a different homing
    /// to a session-owned destination moves the existing exception record
    /// with the existing extract/insert primitives, like the sticky
    /// cross-output path. An explicit drag to a valid readable unheld
    /// destination clears the bit before destination admission,
    /// independent of whether a destination session exists: an empty
    /// destination (no session) then refuses with no transfer and no
    /// minted session, and the window flows through exactly the existing
    /// ordinary-float observation path, so its base is whatever that path
    /// already produces. Unhomed, unreadable or held targets and
    /// malformed frames refuse with the bit retained. No Engine handle,
    /// no transfer, no placement token on the float lane: the native
    /// frame already moved, so the commit only records it.
    fn gesture_float_release(
        &mut self,
        report: &WorldReport,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        release_frame: crate::geometry::Rect,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        if release_frame.w <= 0
            || release_frame.h <= 0
            || release_frame.x.checked_add(release_frame.w).is_none()
            || release_frame.y.checked_add(release_frame.h).is_none()
        {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        let Some(target_key) = self.logical_homing(report, &retained.window) else {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        };
        if target_key == retained.home {
            let Some(session) = self.engine.session_mut(&retained.home) else {
                return GestureOutcome::Settled {
                    committed: false,
                    result: None,
                };
            };
            if !session.adopt_float_placement(&retained.window, release_frame) {
                return GestureOutcome::Settled {
                    committed: false,
                    result: None,
                };
            }
            self.retained_floats
                .retain(|held| held.window != retained.window);
            return GestureOutcome::Settled {
                committed: true,
                result: None,
            };
        }
        // Cross-output: the destination domain must already be staged
        // readable and unheld; unreadable or held targets refuse with
        // the bit retained. An explicit drag clears the bit before
        // destination admission, independent of whether a destination
        // session exists: an empty destination (no session) then refuses
        // with no transfer and no minted session, matching the ordinary
        // float dragged outcome on the same observation.
        let readable = self
            .staged
            .domains
            .iter()
            .any(|domain| domain.readable && domain.key == target_key);
        if !readable || held_scopes.contains(&target_key) {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        self.composed_maximized.remove(&retained.window);
        if self.engine.session(&target_key).is_none()
            || self.engine.session(&retained.home).is_none()
        {
            // Restage without the cleared gesture shield so the scopes
            // below converge the same unshielded staging the ordinary
            // refused-gesture path swept: no transfer, no membership
            // change from the gesture lane, no minted session.
            self.build_staged_domains(report);
            // Converge exactly the two scopes the shield diverged with
            // the existing sweep, mirroring the report sweep tail: the
            // stale home scope reconciles empty and the empty
            // destination reconciles its observed carrier.
            let owner = issue.owner.clone();
            let correlation = issue.correlation.clone();
            let domains = self.staged.domains.clone();
            for domain in &domains {
                if domain.key != target_key && domain.key != retained.home {
                    continue;
                }
                if !domain.readable {
                    continue;
                }
                self.sweep_domain(domain, &owner, &correlation, held_scopes);
            }
            self.settle_intent_admissions();
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        let mut ids = BTreeSet::new();
        ids.insert(retained.window.clone());
        let moved = self
            .engine
            .session_mut(&retained.home)
            .map(|session| session.extract_exception_floats(&ids));
        let Some(moved) = moved else {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        };
        if moved.is_empty() {
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        self.engine
            .session_mut(&target_key)
            .expect("target session checked")
            .insert_exception_floats(moved, &target_key);
        let adopted = self
            .engine
            .session_mut(&target_key)
            .expect("target session checked")
            .adopt_float_placement(&retained.window, release_frame);
        if !adopted {
            let mut back = BTreeSet::new();
            back.insert(retained.window.clone());
            let moved = self
                .engine
                .session_mut(&target_key)
                .expect("target session checked")
                .extract_exception_floats(&back);
            if let Some(home) = self.engine.session_mut(&retained.home) {
                home.insert_exception_floats(moved, &retained.home);
            }
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        self.commit_send_member(&retained.window, &target_key);
        self.retained_floats
            .retain(|held| held.window != retained.window);
        GestureOutcome::Settled {
            committed: true,
            result: None,
        }
    }

    /// Build the local Engine event envelope for one gesture preview,
    /// drop, or resize: identical observation binding to
    /// `run_local_operation` (mover-homed domain, shielded rows, inset
    /// bounds, accepted revision), without consuming a command sequence.
    /// `None` selects the same refusal the operation route returns, with
    /// no mutation.
    fn gesture_local_event(
        &self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        command: &CoreCommand,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<CoreEvent> {
        let window = match command {
            CoreCommand::DragDrop { window, .. }
            | CoreCommand::DragPreview { window, .. }
            | CoreCommand::PointerResize { window, .. } => WindowId(window.clone()),
            _ => return None,
        };
        if window != retained.window {
            return None;
        }
        let (domain_key, domain_report) = self.operation_domain_staged(command, held_scopes)?;
        if domain_key != retained.home {
            return None;
        }
        let owner = issue.owner.clone();
        let correlation = issue.correlation.clone();
        let accepted = self
            .engine
            .session(&domain_key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let staged_output = OutputDomain {
            id: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            bounds: domain_report.bounds,
            gap: domain_report.gap,
            adjacent: BTreeMap::new(),
        };
        let inset =
            crate::world_engine::inset_world_domain(&staged_output, domain_report.outer_gap)?;
        Some(CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset),
            domain_key,
            outer_gap: domain_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: self.shielded_windows(&retained.home, &domain_report.windows),
            closed_windows: domain_report.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        })
    }

    /// Build the source-first pair Engine event for a cross-output
    /// preview or drop: identical binding to `operation_drag_pair`
    /// (pointer domain as destination, source-first combined rows, inset
    /// pair bounds), without consuming a command sequence. `None`
    /// selects the same refusal the operation route returns.
    ///
    /// Gap convention follows the directional pair: `outer_gap` carries
    /// the source gap and `directional_target_outer_gap` the destination
    /// gap, so both sessions retain their own staged gaps.
    fn gesture_pair_event(
        &self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        command: &CoreCommand,
        pointer_key: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<CoreEvent> {
        let source_key = retained.home.clone();
        let domain_key = pointer_key.clone();
        let source_report = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == source_key)?
            .clone();
        if held_scopes.contains(&source_key) {
            return None;
        }
        let domain_report = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == domain_key)?
            .clone();
        let owner = issue.owner.clone();
        let correlation = issue.correlation.clone();
        let accepted = self
            .engine
            .session(&domain_key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let mut combined = self.shielded_windows(&source_key, &source_report.windows);
        combined.extend(self.shielded_windows(&domain_key, &domain_report.windows));
        let source_output_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_report.bounds,
            gap: source_report.gap,
            adjacent: BTreeMap::new(),
        };
        let domain_output_domain = OutputDomain {
            id: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            bounds: domain_report.bounds,
            gap: domain_report.gap,
            adjacent: BTreeMap::new(),
        };
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_output_domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&domain_output_domain, domain_report.outer_gap),
        ) else {
            return None;
        };
        Some(CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_target),
            domain_key,
            outer_gap: source_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: combined,
            closed_windows: domain_report.closed.clone(),
            directional: Some(vec![
                (inset_source, source_key),
                (inset_target, domain_report.key.clone()),
            ]),
            directional_target_outer_gap: Some(domain_report.outer_gap),
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        })
    }

    /// Resolve one cross-output preview through the shared pair event:
    /// read-only, never commits. `None` holds the session with no
    /// overlay, exactly as before.
    fn gesture_pair_preview(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        command: &CoreCommand,
        pointer_key: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<Option<CommandResult>> {
        let event = self.gesture_pair_event(issue, retained, command, pointer_key, held_scopes)?;
        match self.engine.handle(&event) {
            CoreReply::DragPreview(plan) => Some(Self::command_result_from_reply(
                &event.command,
                &CoreReply::DragPreview(plan),
            )),
            _ => None,
        }
    }

    /// Commit one cross-output drop through the shared pair event at the
    /// live coordinates: the Engine commits the pointer-targeted
    /// placement inside `handle`, and the caller retains the window
    /// transfer with member/follow settlement exactly like the send
    /// path, so arrival proof and focus behave unchanged. `Some` only
    /// on a committed placement; `None` (pair inapplicable or refused,
    /// including empty destinations with no usable session) lets the
    /// caller fall back to the send admission.
    #[allow(clippy::too_many_arguments)]
    fn gesture_pair_drop(
        &mut self,
        issue: &crate::world_engine::EventIssue,
        retained: &RetainedGesture,
        operation_id: &str,
        output: &OutputId,
        workspace: &str,
        x: i32,
        y: i32,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<GestureOutcome> {
        // No pair attempt under a pending window flight for the mover
        // or a migration owning the destination cell: the fallback
        // refuses through the same send checks.
        if self.retained.iter().any(|r| match &r.target.kind {
            TransferKind::Window(w) => w == &retained.window,
            TransferKind::View | TransferKind::Migration => false,
        }) {
            return None;
        }
        if self.retained.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::Migration)
                && r.target.target_output == *output
                && r.target.target_workspace == workspace
        }) {
            return None;
        }
        let pointer_key = DomainKey {
            output: output.clone(),
            workspace: WorkspaceId(workspace.to_owned()),
        };
        let command = CoreCommand::DragDrop {
            window: retained.window.0.clone(),
            x,
            y,
            hover_prior: retained.hover.clone(),
            source: Some(retained.home.clone()),
        };
        let event =
            self.gesture_pair_event(issue, retained, &command, &pointer_key, held_scopes)?;
        match self.engine.handle(&event) {
            CoreReply::Tiled(_) => {
                let transfer = RetainedTransfer {
                    target: TransferTarget {
                        operation_id: operation_id.to_owned(),
                        kind: TransferKind::Window(retained.window.clone()),
                        target_output: output.clone(),
                        target_workspace: workspace.to_owned(),
                        follow: true,
                        held: false,
                    },
                    policy: false,
                    source_output: retained.home.output.clone(),
                    source_workspace: retained.home.workspace.0.clone(),
                    members: Vec::new(),
                    migration_refill: None,
                    migration_initial_empty: false,
                    migration_source_index: None,
                    migration_source_view: None,
                    migration_target_view: None,
                    migration_terminal: None,
                    pending_member: None,
                    pending_members: BTreeMap::new(),
                    pending_view: None,
                };
                if let TransferKind::Window(window) = &transfer.target.kind {
                    let target = DomainKey {
                        output: transfer.target.target_output.clone(),
                        workspace: WorkspaceId(transfer.target.target_workspace.clone()),
                    };
                    self.commit_send_member(window, &target);
                }
                self.retained.push(transfer);
                Some(GestureOutcome::Settled {
                    committed: true,
                    result: None,
                })
            }
            _ => None,
        }
    }

    /// Staged carrier index for gesture planning: homed Engine carriers
    /// keyed by lifetime, mirroring `execute_command`.
    fn staged_carriers(&self) -> Vec<EngineWindow> {
        let mut owned: Vec<EngineWindow> = Vec::new();
        for domain in &self.staged.domains {
            owned.extend(domain.windows.iter().cloned());
        }
        owned
    }
}

/// Carrier index for gesture planning: homed Engine carriers keyed by
/// lifetime.
fn by_id_of(owned: &[EngineWindow]) -> BTreeMap<&WindowId, &EngineWindow> {
    let mut by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
    for window in owned {
        by_id.insert(&window.window, window);
    }
    by_id
}

/// Inset work-area bounds for one staged domain: the retained outer gap
/// insets once, exactly like the sweep. `None` on geometric exhaustion.
fn inset_bounds_of(
    key: &DomainKey,
    domain: &crate::world_engine::StagedDomain,
) -> Option<crate::geometry::Rect> {
    let staged = OutputDomain {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        bounds: domain.bounds,
        gap: domain.gap,
        adjacent: BTreeMap::new(),
    };
    crate::world_engine::inset_world_domain(&staged, domain.outer_gap).map(|inset| inset.bounds)
}

#[cfg(test)]
mod staged_pause_tests {
    //! Staged-observation game-pause pin: the gesture predicate exempts its
    //! own subject while the cursor predicate does not. Nested
    //! private-predicate tests; they drive the real `stage_facts` lane,
    //! never a production test seam.

    use crate::directional::{OutputId, WindowId, WorkspaceId};
    use crate::gesture::GestureKind;
    use crate::session::DomainKey;
    use crate::world_directional::tests::{fact, report_full, screen};
    use crate::world_engine::WorldEngine;

    use super::RetainedGesture;

    fn retained(window: &str) -> RetainedGesture {
        RetainedGesture {
            window: WindowId(window.to_owned()),
            id: 7,
            kind: GestureKind::Move,
            start_seq: 1,
            start_frame: crate::geometry::Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 90,
            },
            start_cursor: Some((10, 20)),
            start_exact: true,
            modal_move: false,
            hover: None,
            home: DomainKey {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            },
        }
    }

    #[test]
    fn game_pause_exempts_gesture_subject_only() {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let mut game = fact("game", "out-a", 0);
        game.fullscreen = Some(true);
        let report = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![game],
            Vec::new(),
            "game",
            None,
            None,
        );
        engine.stage_facts(&report);
        // The cursor lane pauses on a focused fullscreen game with no
        // subject exemption; the gesture lane pauses for other subjects
        // but exempts its own.
        assert!(crate::world_cursor::cursor_game_paused(&engine));
        assert!(engine.gesture_game_paused(&retained("win-a")));
        assert!(!engine.gesture_game_paused(&retained("game")));
        // No-pause control: refocusing onto a plain window on the next
        // staged observation clears both predicates.
        let clear = report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            Vec::new(),
            "win-a",
            None,
            None,
        );
        engine.stage_facts(&clear);
        assert!(!crate::world_cursor::cursor_game_paused(&engine));
        assert!(!engine.gesture_game_paused(&retained("win-a")));
        assert!(!engine.gesture_game_paused(&retained("game")));
    }
}
