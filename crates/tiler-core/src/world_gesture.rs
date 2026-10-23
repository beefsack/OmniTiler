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
//! ingress is observation, never a command). Cross-output releases keep the
//! existing send semantics (membership move plus follow, no pointer
//! placement); same-output releases keep the resolver placement. The
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
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, DragHoverPrior, OutputDomain};
use crate::world_engine::WorldEngine;
use crate::world_report::{CommandResult, TransferKind, WorldReport};

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
        let ingress = match report.gesture.as_ref() {
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
        if !self.gesture_start_valid(report, ingress, &home, held_scopes) {
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
            start_seq: report.observation_seq,
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
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let ingress = match report.gesture.as_ref() {
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
            Some(ingress) => Some(ingress.clone()),
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
                if report.closed.contains(&retained.window) {
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
                    GesturePhase::Started if retained.start_seq == report.observation_seq => {
                        GestureOutcome::Held
                    }
                    GesturePhase::Started | GesturePhase::Updated => {
                        self.gesture_updated(report, &ingress, held_scopes)
                    }
                    GesturePhase::Released => self.gesture_released(report, &ingress, held_scopes),
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
    /// complete inventory, readable unheld tiled homing, and no overlay,
    /// float, or sticky lane. Unmanaged, minimized, hidden (unreadable),
    /// fullscreen/maximized, floating, or held starts fail closed with no
    /// retained session. `home` is the retained membership homing when
    /// known (never yanked by the observed screen), else the staged
    /// homing for a genuinely new lifetime.
    fn gesture_start_valid(
        &self,
        report: &WorldReport,
        ingress: &GestureIngress,
        home: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if !report.inventory_complete {
            return false;
        }
        if report.closed.contains(&ingress.window) {
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
        if fact.rect.is_none() || fact.minimized {
            return false;
        }
        if fact.fullscreen || fact.maximized {
            return false;
        }
        if fact.floating || fact.sticky {
            return false;
        }
        true
    }

    /// Fences for a continuing gesture report: the retained lifetime must
    /// still be live, readable, unminimized, homed where it started, and
    /// inside a readable unheld scope under complete inventory. Gaps hold;
    /// explicit closes clear (handled by the caller).
    fn gesture_continue_fences(
        &self,
        report: &WorldReport,
        retained: &RetainedGesture,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if !report.inventory_complete {
            return false;
        }
        if report.closed.contains(&retained.window) {
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
        if fact.rect.is_none() || fact.minimized {
            return false;
        }
        if fact.fullscreen || fact.maximized {
            return false;
        }
        if fact.floating || fact.sticky {
            return false;
        }
        true
    }

    /// Release-frame ground truth for the retained window: the staged
    /// observed frame, when readable.
    fn gesture_release_frame(&self, retained: &RetainedGesture) -> Option<crate::geometry::Rect> {
        self.staged
            .facts
            .get(&retained.window)
            .and_then(|fact| fact.rect)
    }

    /// Whether one report runs under a focused fullscreen game pause:
    /// the adapter-observed focused lifetime names a different window
    /// whose staged fact is fullscreen. Normalized report facts only,
    /// never platform reads. Under the pause no overlay publishes and
    /// no release commits; the session holds for the terminal phase.
    fn gesture_game_paused(&self, report: &WorldReport, retained: &RetainedGesture) -> bool {
        if report.focused_global.is_empty() || report.focused_global == retained.window.0 {
            return false;
        }
        let focused = WindowId(report.focused_global.clone());
        self.staged
            .facts
            .get(&focused)
            .is_some_and(|fact| fact.fullscreen)
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
        report: &WorldReport,
        ingress: &GestureIngress,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let retained = self
            .gesture
            .clone()
            .expect("updated runs with a retained session");
        if report.closed.contains(&retained.window) {
            self.gesture = None;
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        if !self.gesture_continue_fences(report, &retained, held_scopes) {
            return GestureOutcome::Held;
        }
        if self.gesture_game_paused(report, &retained) {
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
                    report,
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
                self.gesture_local_event(report, &retained, &command, held_scopes)
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
        ingress: &GestureIngress,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> GestureOutcome {
        let retained = self
            .gesture
            .clone()
            .expect("released runs with a retained session");
        self.gesture = None;
        if self.gesture_game_paused(report, &retained) {
            self.record_gesture_terminal(retained.id, false);
            return GestureOutcome::Settled {
                committed: false,
                result: None,
            };
        }
        if report.closed.contains(&retained.window)
            || !self.gesture_continue_fences(report, &retained, held_scopes)
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
        let outcome = match intent {
            Some(GestureIntent::MoveDrop { x, y }) => {
                self.gesture_move_release(report, &retained, x, y, &by_id, held_scopes)
            }
            Some(GestureIntent::PointerResize {
                direction,
                boundary,
                direction2,
                boundary2,
            }) => self.gesture_resize_release(
                report,
                &retained,
                direction,
                boundary,
                direction2,
                boundary2,
                held_scopes,
            ),
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
        // `committed` is true only on a real move/resize commit.
        let committed = matches!(
            outcome,
            GestureOutcome::Settled {
                committed: true,
                ..
            }
        );
        self.record_gesture_terminal(retained.id, committed);
        outcome
    }

    /// Commit a move release: same-output points keep the resolver
    /// placement through the local drop; cross-output points keep the
    /// existing send semantics (membership move plus follow, no pointer
    /// placement). Unplaceable points refuse with snap-back and no
    /// transfer. The committed reply publishes the desired geometry and
    /// the retained transfer plus focus effects; the adapter applies that
    /// reply mechanically with fresh proof.
    fn gesture_move_release(
        &mut self,
        report: &WorldReport,
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
                let Some(event) = self.gesture_local_event(report, retained, &command, held_scopes)
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
                match self.execute_output_send(
                    report,
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
        report: &WorldReport,
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
        let Some(event) = self.gesture_local_event(report, retained, &command, held_scopes) else {
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

    /// Build the local Engine event envelope for one gesture preview,
    /// drop, or resize: identical observation binding to
    /// `run_local_operation` (mover-homed domain, shielded rows, inset
    /// bounds, accepted revision), without consuming a command sequence.
    /// `None` selects the same refusal the operation route returns, with
    /// no mutation.
    fn gesture_local_event(
        &self,
        report: &WorldReport,
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
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&report.owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return None;
        };
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
    /// preview: identical binding to `operation_drag_pair` (pointer domain
    /// as destination, source-first combined rows, inset pair bounds),
    /// without committing and without consuming a command sequence.
    /// `None` selects the same refusal the operation route returns.
    fn gesture_pair_preview(
        &mut self,
        report: &WorldReport,
        retained: &RetainedGesture,
        command: &CoreCommand,
        pointer_key: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<Option<CommandResult>> {
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
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&report.owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return None;
        };
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
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_target),
            domain_key,
            outer_gap: domain_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: combined,
            closed_windows: domain_report.closed.clone(),
            directional: Some(vec![
                (inset_source, source_key),
                (inset_target, domain_report.key.clone()),
            ]),
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        match self.engine.handle(&event) {
            CoreReply::DragPreview(plan) => Some(Self::command_result_from_reply(
                &event.command,
                &CoreReply::DragPreview(plan),
            )),
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
