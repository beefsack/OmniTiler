#![forbid(unsafe_code)]

//! Live `spaces-drag-transfer` runner: native backend plus the bounded
//! command over two existing SIDs. The transfer legs themselves run through
//! the shared executor in [`crate::space_drag_transfer`]; this module owns
//! the native posts, per-leg freshness, lifecycle, and reporting.
//!
//! Two switch-chord routes (explicit per invocation, never mixed):
//! relative arrows (default: tagged `Ctrl+Left`/`Ctrl+Right` with
//! SecondaryFn, selected from SLS adjacency) and `--numbered` (tagged
//! Control-only Desktop-N digits for the original/temp SLS ordinals,
//! resolved from the configured holders before any input — enabled
//! standard chords only, never a settings write).
//!
//! Native routes (all owned-helper only): HID mouse posts (Silica parity:
//! HID tap, zero flags, own-switch tag) and HID key-chord posts (tagged, so
//! the tap passes our own switch chords while consuming the user's).
//! Every window-targeted post re-proves session gates, the exact held
//! Child, and the shared bundle/oracle boundary; positional proof is the
//! per-leg setup (fresh owned AX frame, grab point, CG join, WindowServer
//! input hit-test and AX element-at-position identity) PLUS a fresh geometry
//! re-proof immediately before mouse-down and mouse-dragged at the identical
//! point. Mouse-up releases at that same proven point, never a retargeted
//! coordinate. Cursor baseline/restore uses the eventless warp only (never
//! a `MouseMoved` event, so no input is delivered onto a baseline that may
//! sit over a protected surface).

#[cfg(target_os = "macos")]
mod live_impl {
    use crate::mc_probe::{
        ActionBudget, GhosttyWindow, LegLatency, McError, TopologySnapshot, validate_create_effect,
    };
    use crate::space_drag_transfer::{DragBackend, DragStep, PostCounts};
    use crate::space_switch_edges::edge_synth_gates;

    /// Guarded HID mouse post (Silica parity: HID tap, zero flags,
    /// own-switch tag). Shared construction with the user-run diagnostics
    /// ([`crate::synth::hid_mouse_post`]): same source, tap, flags, and
    /// tag — no drifting duplicate. Safe maintained `objc2` wrappers
    /// only -- no new `unsafe`.
    fn post_mouse_event(step: DragStep, point: (f64, f64)) -> Result<(), String> {
        let kind = crate::synth::HidMouseKind::from_drag_step(step)
            .ok_or_else(|| "refusing non-mouse step on the mouse post".to_owned())?;
        crate::synth::hid_mouse_post(
            kind,
            point,
            0,
            Some(crate::input_match::OWN_SWITCH_USER_DATA),
        )?;
        Ok(())
    }

    /// Current cursor position via a null-source event (read-only; its
    /// location stamps the live cursor). Safe maintained binding; `None`
    /// when non-finite.
    fn cursor_position() -> Option<(f64, f64)> {
        use objc2_core_graphics::CGEvent;
        let event = CGEvent::new(None)?;
        let point = CGEvent::location(Some(&event));
        if point.x.is_finite() && point.y.is_finite() {
            Some((point.x, point.y))
        } else {
            None
        }
    }

    /// Live backend over the exact held Child. `begin_leg` is fallible
    /// per-leg setup run before any post: chord selection, fresh owned AX
    /// frame, grab point, CG join, and the order-free hit-test. Chord steps
    /// post tagged HID keys; mouse steps post tagged HID mouse events.
    pub(crate) struct LiveDragBackend<'a> {
        pub child: &'a mut std::process::Child,
        pub budget: &'a mut ActionBudget,
        pub pid: u32,
        pub wid: u32,
        pub nonce: String,
        pub helper_path: String,
        pub display: usize,
        pub temp: u64,
        /// Per-leg switch chords (arrows or numbered digits): selected in
        /// `begin_leg`; `post_key` never hardcodes a route's flags.
        pub chords: crate::space_drag_transfer::SwitchChords,
        pub keycode: u32,
        pub flags: u64,
        pub point: (f64, f64),
        pub last_point: (f64, f64),
        counts: PostCounts,
    }

    impl LiveDragBackend<'_> {
        fn prove(&mut self, edge: &str) -> Result<crate::synth::OwnedPostTarget, String> {
            edge_synth_gates(edge).map_err(|e| e.to_string())?;
            let target = crate::synth::OwnedPostTarget::from_held_child(
                self.child,
                self.pid,
                self.wid,
                &self.helper_path,
            )
            .map_err(|e| format!("{edge} target refused: {e}"))?;
            crate::synth::boundary_gate(target.pid(), target.wid())
                .map_err(|e| format!("{edge} boundary refused: {e}"))?;
            Ok(target)
        }

        /// Fresh owned AX frame plus the FIRST provably clear titlebar point
        /// (up to five fractions across the bar), run once per leg before any
        /// post (refusing here posts nothing). Each candidate must pass BOTH
        /// WindowServer input identity and fresh AX element-at-position
        /// identity against the exact held `(pid, wid)`;
        /// no owner-name exemptions are made — a genuinely occluded candidate is
        /// skipped, and no clear candidate refuses the leg. The per-post
        /// oracle in `prove` is unchanged (focus/identity bar); the
        /// geometry/z bar below is never a duplicate of it.
        fn refresh_grab(&mut self, leg: &str) -> Result<(), String> {
            let owned = crate::ax::authorize_owned_action(self.pid, &self.nonce)
                .map_err(|e| format!("{leg} ownership refused: {e}"))?;
            if crate::ownership::is_protected_bundle(&owned.bundle_id) {
                return Err(format!("{leg} refusing protected bundle"));
            }
            let frame = crate::ax::read_window_owned(&owned, self.pid, self.wid)
                .map_err(|e| format!("{leg} AX frame unreadable: {e}"))?
                .ax_frame
                .ok_or_else(|| format!("{leg} AX frame missing"))?;
            let candidates = crate::space_drag_transfer::candidate_grab_points(frame)
                .map_err(|e| format!("{leg} {e}"))?;
            let entry = crate::cg::verify_wid_owned_by_pid(self.pid, self.wid)
                .map_err(|e| format!("{leg} CG join refused: {e}"))?;
            let all = crate::ffi::cg_window_list_basic()
                .map_err(|e| format!("{leg} CG list unreadable: {e}"))?;
            let mut last_err = String::from("no candidate evaluated");
            for point in candidates {
                let hit = match crate::ffi::ax_hit_test_owner(point.0, point.1) {
                    Err(e) => {
                        last_err = format!("AX hit-test unreadable: {e}");
                        continue;
                    }
                    Ok(hit) => hit,
                };
                let input_wid = match crate::ffi::sls_window_at_point(point.0, point.1) {
                    Ok(wid) => wid,
                    Err(e) => {
                        last_err = e;
                        continue;
                    }
                };
                if let Err(e) = crate::space_drag_transfer::hit_test_input_grab(
                    point, &entry, &all, hit, input_wid,
                ) {
                    last_err = e;
                    continue;
                }
                self.point = point;
                self.last_point = point;
                return self.reprove_point(leg);
            }
            Err(format!(
                "{leg} grab refused: no provably clear titlebar point (tried 5; last: {last_err})"
            ))
        }

        /// Positional re-proof at the CURRENT grab point (no recompute, no
        /// retarget): fresh CG join and two input-reaching identities, both
        /// required to equal the exact held `(pid, wid)`. CG rectangles alone
        /// cannot distinguish transparent backing from input occlusion.
        /// Runs at per-leg setup via
        /// [`Self::refresh_grab`] and again immediately before mouse-down
        /// and mouse-dragged; mouse-up releases at this same proven point.
        fn reprove_point(&self, leg: &str) -> Result<(), String> {
            let point = self.point;
            let entry = crate::cg::verify_wid_owned_by_pid(self.pid, self.wid)
                .map_err(|e| format!("{leg} CG join refused: {e}"))?;
            let all = crate::ffi::cg_window_list_basic()
                .map_err(|e| format!("{leg} CG list unreadable: {e}"))?;
            let hit = crate::ffi::ax_hit_test_owner(point.0, point.1)
                .map_err(|e| format!("{leg} AX hit-test unreadable: {e}"))?;
            let input_wid = crate::ffi::sls_window_at_point(point.0, point.1)?;
            crate::space_drag_transfer::hit_test_input_grab(point, &entry, &all, hit, input_wid)
                .map_err(|e| format!("{leg} {e}"))?;
            Ok(())
        }
    }

    impl DragBackend for LiveDragBackend<'_> {
        fn begin_leg(&mut self, goal: u64) -> Result<(), String> {
            self.budget
                .spend_main("drag-leg")
                .map_err(|e| format!("budget exhausted before leg: {e}"))?;
            // Per-leg chord through the shared pure selection (the same
            // `for_goal` the fake backend records in tests): the temp goal
            // posts outward, any other goal posts the return chord.
            let (keycode, flags, _) = self.chords.for_goal(goal, self.temp);
            self.keycode = keycode;
            self.flags = flags;
            self.refresh_grab("leg-setup")
        }

        fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
            let edge = match step {
                DragStep::MouseMove => "drag-move",
                DragStep::MouseDown => "drag-down",
                DragStep::MouseDragged => "drag-grab",
                DragStep::MouseUp => "drag-up",
                _ => return Err("refusing non-mouse step on the mouse post".to_owned()),
            };
            let _ = self.prove(edge)?;
            // Fresh positional re-proof immediately before the button goes
            // down (and before the grab drag) at the identical point: the
            // per-leg setup proof above may predate the move by the chord
            // settles, so a window that slid under the cursor in between
            // refuses here before any down/drag. Release reuses this same
            // proven point and never retargets.
            if matches!(step, DragStep::MouseDown | DragStep::MouseDragged) {
                let leg = if step == DragStep::MouseDown {
                    "drag-down-fresh"
                } else {
                    "drag-grab-fresh"
                };
                self.reprove_point(leg)?;
            }
            post_mouse_event(step, self.point)?;
            match step {
                DragStep::MouseDown => self.counts.mouse_down += 1,
                DragStep::MouseUp => self.counts.mouse_up += 1,
                _ => {}
            }
            Ok(())
        }

        fn post_key(&mut self, down: bool) -> Result<(), String> {
            let edge = if down {
                "drag-chord-down"
            } else {
                "drag-chord-up"
            };
            let target = self.prove(edge)?;
            let key = u16::try_from(self.keycode)
                .map_err(|_| format!("keycode {} out of range", self.keycode))?;
            crate::synth::post_guarded_with_tag(
                &target,
                key,
                self.flags,
                down,
                crate::synth::GuardedPostRoute::Hid,
                Some(crate::input_match::OWN_SWITCH_USER_DATA),
            )
            .map_err(|e| format!("{edge} refused: {e}"))?;
            if down {
                self.counts.key_down += 1;
            } else {
                self.counts.key_up += 1;
            }
            Ok(())
        }

        fn read_membership(&mut self) -> Option<Vec<u64>> {
            crate::cg::verify_wid_owned_by_pid(self.pid, self.wid).ok()?;
            crate::spaces::window_spaces(self.wid).ok()
        }

        fn read_current(&mut self) -> Option<u64> {
            crate::mc_lifecycle::live::fresh_snapshot()
                .ok()?
                .displays
                .get(self.display)?
                .current
        }

        fn counts(&self) -> PostCounts {
            self.counts
        }
    }

    /// SLPS focus of one exact owned helper with oracle agreement (same
    /// proven method as the switch probe; never Ghostty, never forced).
    fn focus_helper(pid: u32, wid: u32) -> Result<(), String> {
        crate::focus::dispatch_focus(pid, wid)
            .map_err(|e| format!("focus dispatch refused: {e}"))?;
        crate::focus::await_agreement(pid, wid, std::time::Duration::from_secs(5))
            .map(|_| ())
            .map_err(|e| format!("focus agreement unproven: {e}"))?;
        crate::focus::read_agreement(pid, wid)
            .map(|_| ())
            .map_err(|e| format!("focus oracle disagrees before drag: {e}"))
    }

    /// One independent-recovery leg: MC goto the goal current, public
    /// self-join through the held Child, exact membership poll. Any
    /// shortfall refuses before any synthetic post. A goto close shortfall
    /// is recorded (never swallowed): Mission Control left open would
    /// overlay the later synthetic legs.
    #[allow(clippy::too_many_arguments)]
    fn recover_leg(
        from: &TopologySnapshot,
        child: &mut std::process::Child,
        pid: u32,
        nonce: &str,
        helper_path: &str,
        wid: u32,
        display: usize,
        goal: u64,
        goto_leg: &'static str,
        join_leg: &'static str,
        budget: &mut ActionBudget,
        latencies: &mut Vec<LegLatency>,
        notes: &mut Vec<String>,
    ) -> Result<TopologySnapshot, McError> {
        let t = std::time::Instant::now();
        let (snap, close) =
            crate::mc_lifecycle::goto_sid_verified(from, display, goal, budget, false)?;
        latencies.push(LegLatency {
            leg: goto_leg.to_owned(),
            ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
        if let Some(c) = close {
            notes.push(format!("{goto_leg} MC close shortfall: {c}"));
        }
        let t = std::time::Instant::now();
        crate::space_recovery::run_join_leg_held(child, pid, nonce, helper_path, wid)?;
        latencies.push(LegLatency {
            leg: join_leg.to_owned(),
            ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
        crate::space_recovery::poll_membership_held(child, pid, helper_path, wid, goal)
            .map_err(McError::Blocked)?;
        Ok(snap)
    }

    /// Ordered per-display topology equality (spaces order + current):
    /// proves the run restored the exact desktop layout, never just the
    /// current SID.
    fn topology_equal(before: &TopologySnapshot, after: &TopologySnapshot) -> bool {
        before.displays.len() == after.displays.len()
            && before
                .displays
                .iter()
                .zip(after.displays.iter())
                .all(|(a, b)| a.spaces == b.spaces && a.current == b.current)
    }

    /// Structured failure: current restore via MC AX (never a synthetic
    /// replay), MC close, exact-Child stops, absence + Ghostty + caffeinate
    /// evidence. `None` helper/topology legs report unknown (`None`), never
    /// a vacuous pass. `temp_residue` names an internally created temp that
    /// may still exist (JSON field plus hint; never a broadened removal).
    /// Never a PID guess, never a broad kill.
    #[allow(clippy::too_many_arguments)]
    fn fail(
        stage: &str,
        error: McError,
        caffeinate: &mut crate::mc_lifecycle::live::CaffeinateGuard,
        child: Option<&mut std::process::Child>,
        pid: Option<u32>,
        display: usize,
        original: u64,
        snap0: Option<&TopologySnapshot>,
        ghostty_before: &[GhosttyWindow],
        budget: &mut ActionBudget,
        notes: &mut Vec<String>,
        temp_residue: Option<u64>,
        synthetic: bool,
    ) -> (i32, String) {
        notes.push(format!("{stage}: {error}"));
        if let Some(t) = temp_residue {
            notes.push(crate::space_drag_transfer::temp_residue_hint(t, original));
        }
        match crate::mc_lifecycle::live::fresh_snapshot() {
            Ok(after) => {
                if after.displays.get(display).and_then(|d| d.current) == Some(original) {
                    notes.push(format!("recovery: already on original {original}; no goto"));
                } else if after.locate(original).is_some_and(|(di, _)| di == display) {
                    match crate::mc_lifecycle::goto_sid_verified(
                        &after, display, original, budget, true,
                    ) {
                        Ok(_) => notes.push(format!("recovery: MC AX goto original {original}")),
                        Err(e) => notes.push(format!("recovery goto refused: {e}")),
                    }
                } else {
                    notes.push(format!(
                        "recovery: original {original} not on display {display}; leaving currents"
                    ));
                }
            }
            Err(e) => notes.push(format!("recovery snapshot unreadable ({e}); goto blocked")),
        }
        crate::mc_lifecycle::safe_close_mc(notes);
        let stop = match (child, pid) {
            (Some(c), Some(p)) => {
                let s = crate::session::stop_exact_child(c, p);
                serde_json::json!({"pid": p, "exited": s.exited, "reaped": s.reaped, "record_removed": s.record_removed})
            }
            _ => serde_json::json!({"note": "no helper spawned yet; nothing stopped"}),
        };
        let helpers_absent: Option<bool> = pid.map(|p| {
            !crate::ffi::process_exists(p as i32)
                && crate::ffi::cg_window_list_all()
                    .map(|list| list.iter().all(|w| w.pid != p))
                    .unwrap_or(false)
        });
        let currents_restored: Option<bool> =
            match (crate::mc_lifecycle::live::fresh_snapshot().ok(), snap0) {
                (Some(now), Some(before)) => Some(topology_equal(before, &now)),
                _ => None,
            };
        let ghostty_ok = crate::mc_lifecycle::snapshot_ghostty_windows()
            .ok()
            .is_some_and(|after| {
                crate::mc_probe::ghostty_stable_across_switch(ghostty_before, &after)
            });
        let reaped = caffeinate.stop();
        (
            1,
            serde_json::json!({
                "stage": stage,
                "action": "spaces-drag-transfer",
                "error": notes.join("; "),
                "helper_stop": stop,
                "helpers_absent": helpers_absent,
                "currents_restored": currents_restored,
                "ghostty_stable": ghostty_ok,
                "caffeinate_reaped": reaped,
                "temp_residue": temp_residue,
                "synthetic_events_posted": synthetic,
                "mutates": true,
            })
            .to_string(),
        )
    }

    /// Resolve both `--numbered` holders from one display's SLS order over
    /// the live session store, preserving the gate-vs-system split in
    /// [`McError`]. No settings write on any path — enable the default
    /// chords through the explicit `spaces-desktop-apply` route instead. No
    /// live touch beyond the holder reads; callers run this before any
    /// synthetic post.
    fn resolve_numbered_chords(
        spaces_in_order: &[u64],
        original: u64,
        temp: u64,
    ) -> Result<crate::desktop_shortcuts::NumberedChordPair, McError> {
        let mut store = crate::desktop_shortcuts::LiveStore::new();
        crate::desktop_shortcuts::resolve_numbered_pair(&mut store, spaces_in_order, original, temp)
            .map_err(|e| match e {
                crate::desktop_shortcuts::NumberedResolveError::Gate(why) => McError::Blocked(why),
                crate::desktop_shortcuts::NumberedResolveError::Unreadable(why) => {
                    McError::System(why)
                }
            })
    }

    /// Bounded drag-transfer command on one display. `Some(temp)` runs over
    /// two EXISTING user SIDs (creates/destroys no Space). `None`
    /// (`--temporary`) first requires the exact single-Space baseline
    /// `[original]` on the original's display, creates exactly one temp
    /// through the existing MC adoption gates, and removes ONLY that SID
    /// after the helper stops (any refusal names the exact residue plus the
    /// `mc-recover` hint; removals are never replayed, never broadened).
    /// Spawns one owned regular-policy helper with the `--space-join` poll,
    /// proves independent public self-join recovery O->T->O first (refuses
    /// before any synthetic post when unavailable), then runs the shared
    /// executor outward plus drag-back. Performs no SLS move, writes no
    /// preferences, and never targets Ghostty (snapshot-only baseline plus
    /// hit-test refusal). `numbered` posts the configured Desktop-N chords
    /// for the SLS ordinals (default `false` posts the relative arrows).
    pub fn run_spaces_drag_transfer(
        original: u64,
        temp: Option<u64>,
        seconds: u64,
        allow_untested: bool,
        numbered: bool,
    ) -> Result<String, (i32, String)> {
        use crate::space_drag_transfer::{validate_drag_args, validate_drag_args_temporary};
        use crate::symbolic_hotkeys::{KEY_LEFT_ARROW, KEY_RIGHT_ARROW};

        match temp {
            Some(t) => {
                validate_drag_args(original, t, seconds, allow_untested).map_err(|e| (2, e))?;
            }
            None => {
                validate_drag_args_temporary(original, seconds, allow_untested)
                    .map_err(|e| (2, e))?;
            }
        }
        if let Err(e) = crate::mc_lifecycle::live::action_gates() {
            return Err((1, e.to_string()));
        }
        // Ghostty baseline FIRST (read-only; never a post/focus target).
        let ghostty_before = crate::mc_lifecycle::snapshot_ghostty_windows()
            .map_err(|e| (1, format!("baseline-ghostty: {e}")))?;
        let focus_before = crate::mc_lifecycle::capture_focus_legs();
        let snap0 = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| (1, format!("baseline-topology: {e}")))?;
        let (display, _) = snap0.locate(original).ok_or_else(|| {
            (
                1,
                format!("original Space {original} not uniquely located; refusing"),
            )
        })?;
        if !crate::ffi::session_state().unlocked() {
            return Err((1, "window-server session not unlocked; refusing".to_owned()));
        }
        let mut caffeinate = crate::mc_lifecycle::live::CaffeinateGuard::spawn(seconds)
            .map_err(|e| (1, format!("caffeinate spawn failed: {e}")))?;
        let mut budget = ActionBudget::new(seconds);
        let mut notes: Vec<String> = Vec::new();
        let mut latencies: Vec<LegLatency> = Vec::new();
        // Pre-helper refusals funnel through `fail` (caffeinate stops last
        // there); nothing has posted synthetically yet. The temp leg carries
        // the resident-temp evidence for failures after temp creation.
        macro_rules! fail_gate {
            ($stage:expr, $err:expr, $temp:expr) => {
                return Err(fail(
                    $stage,
                    $err,
                    &mut caffeinate,
                    None,
                    None,
                    display,
                    original,
                    Some(&snap0),
                    &ghostty_before,
                    &mut budget,
                    &mut notes,
                    $temp,
                    false,
                ))
            };
        }
        // Temp resolution: a named preexisting SID is only adopted on the
        // original's display; `--temporary` (None) creates exactly one temp
        // on that display through the existing MC adoption gates, after
        // refusing on anything but the exact single-Space baseline. The
        // post-create snapshot rides along for every post-create leg
        // (adjacency, first recovery goto): the pre-create baseline no
        // longer names the Space set once the temp exists.
        let (temp, created_temp, created_snapshot): (u64, Option<u64>, Option<TopologySnapshot>) =
            match temp {
                Some(t) => {
                    if snap0.locate(t).is_none_or(|(di, _)| di != display) {
                        fail_gate!(
                            "locate-temp",
                            McError::Blocked(format!(
                                "temp Space {t} not uniquely on display {display}; refusing"
                            )),
                            None
                        );
                    }
                    (t, None, None)
                }
                None => {
                    if let Err(reason) = crate::space_drag_transfer::require_single_space_baseline(
                        &snap0, display, original,
                    ) {
                        fail_gate!("temporary-baseline", McError::Blocked(reason), None);
                    }
                    let t = std::time::Instant::now();
                    let create_outcome = match crate::mc_lifecycle::live::mc_add_space(
                        display,
                        &mut budget,
                        false,
                    ) {
                        Ok(outcome) => outcome,
                        Err(e) => fail_gate!("temporary-create", e, None),
                    };
                    if let Some(c) = create_outcome.close_error {
                        notes.push(format!("temporary create close shortfall: {c}"));
                    }
                    let created_snapshot = match crate::mc_lifecycle::live::await_snapshot(&|s| {
                        validate_create_effect(&snap0, s, display).is_ok()
                    }) {
                        Ok(s) => s,
                        Err(e) => fail_gate!("temporary-create-readback", e, None),
                    };
                    let created = match validate_create_effect(&snap0, &created_snapshot, display) {
                        Ok(sid) => sid,
                        Err(e) => fail_gate!("temporary-create-adopt", e, None),
                    };
                    if let Err(e) = crate::mc_lifecycle::require_user_temp(created) {
                        fail_gate!("temporary-create-type", e, None);
                    }
                    latencies.push(LegLatency {
                        leg: "temporary-create".to_owned(),
                        ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                    notes.push(format!(
                    "temporary temp {created} adopted by exactly-one-new-SID diff; ONLY this SID is ever removed (residue hint: mc-recover --temp {created} --original {original})"
                ));
                    (created, Some(created), Some(created_snapshot))
                }
            };
        // Post-create reasoning surface: the created snapshot once the temp
        // exists, else the pre-create baseline (named-temp mode changes no
        // Space set). Adjacency and the first recovery goto read this.
        let live_topology: &TopologySnapshot = match &created_snapshot {
            Some(s) => s,
            None => &snap0,
        };
        // Resident-temp evidence for failure reports after this point (the
        // internally created SID while it may still exist; cleared once its
        // removal is proven). Never broadens removal.
        let mut temp_resident = created_temp;
        // Switch-chord selection (explicit per invocation, never mixed).
        // Arrows (default): both enabled arrows corroborated (no settings
        // write to enable them), then one chord moves exactly one Space, so
        // SLS-order adjacency selects each leg's arrow. Numbered: the
        // configured Desktop-N chords for the SLS ordinals (no adjacency
        // needed — a digit jumps directly; no settings write either).
        // Both read the post-create surface (see above), never the stale
        // pre-create baseline, and both refuse before any synthetic post.
        let chords = if numbered {
            let pair = match resolve_numbered_chords(
                &live_topology.displays[display].spaces,
                original,
                temp,
            ) {
                Ok(c) => c,
                Err(e) => fail_gate!("numbered-chords", e, temp_resident),
            };
            // Discriminator evidence: the exact SLS order plus the selected
            // Desktop-N chords ride the report, so a no-effect leg
            // distinguishes a wrong-chord setup from an ineffective chord
            // without a rerun.
            notes.push(format!(
                "numbered selection: SLS order {:?} on display {display}, original ordinal {} (Desktop-{} holder id {} keycode {:#x}) temp ordinal {} (Desktop-{} holder id {} keycode {:#x}), both Control-only {:#x} tagged",
                live_topology.displays[display].spaces,
                pair.return_desktop,
                pair.return_desktop,
                pair.return_symbolic,
                pair.return_key,
                pair.outward_desktop,
                pair.outward_desktop,
                pair.outward_symbolic,
                pair.outward_key,
                crate::input_match::SWITCH_CTRL_BIT,
            ));
            crate::space_drag_transfer::SwitchChords::numbered(
                (pair.outward_key, pair.outward_flags),
                (pair.return_key, pair.return_flags),
            )
        } else {
            // Read-only bindings: the single-chord transfer needs both enabled
            // arrows corroborated (no settings write to enable them).
            let bindings = match crate::symbolic_hotkeys::read_bindings() {
                Ok(b) => b,
                Err(e) => fail_gate!("bindings", McError::System(e), temp_resident),
            };
            notes.push(format!(
                "bindings ctrl_left={} ctrl_right={} over {} entries",
                bindings.corroboration.ctrl_left_enabled,
                bindings.corroboration.ctrl_right_enabled,
                bindings.corroboration.entries_seen
            ));
            if !bindings.corroboration.both_enabled() {
                fail_gate!(
                    "bindings",
                    McError::Blocked(
                        "arrow legs ineligible: Ctrl+Left/Right not both corroborated enabled (no settings write); refusing"
                            .to_owned()
                    ),
                    temp_resident
                );
            }
            // One chord moves exactly one Space: require SLS-order adjacency
            // (no multi-chord loop here), selecting each leg's arrow from it.
            // The internally created temp appends at the row end, so it is
            // adjacent by construction; a named temp must already be adjacent.
            let (out_key, back_key) = match crate::space_switch_edges::gesture_sign(
                &live_topology.displays[display].spaces,
                original,
                temp,
            ) {
                Some(s) if s > 0.0 => (KEY_RIGHT_ARROW, KEY_LEFT_ARROW),
                Some(_) => (KEY_LEFT_ARROW, KEY_RIGHT_ARROW),
                None => {
                    fail_gate!(
                        "adjacency",
                        McError::Blocked(
                            "O and T are not adjacent in SLS order; single-chord transfer refused"
                                .to_owned()
                        ),
                        temp_resident
                    );
                }
            };
            // Discriminator evidence: the exact SLS order plus the selected
            // keycodes ride the report, so a no-effect leg distinguishes a
            // wrong-arrow setup from an ineffective chord without a rerun.
            notes.push(format!(
                "arrow selection: SLS order {:?} on display {display}, outward keycode {out_key} return keycode {back_key}",
                live_topology.displays[display].spaces
            ));
            crate::space_drag_transfer::SwitchChords::arrows(out_key, back_key)
        };

        // Owned-helper spawn placement (fixture correction): the helper's
        // legacy spawn origin sits under the protected terminal on this
        // desktop, so every titlebar candidate hit-tests foreign. Read the
        // single display's visible frame (main-thread AppKit, no AX) plus a
        // fresh normal-layer layout, fit the spawn-size rect outside every
        // onscreen same-layer rect, and have the helper create its own
        // window there (self-action at creation; the controller never writes
        // frames). Above-normal chrome never enters the avoid set: the menu
        // bar is outside the visible frame and the spanning Dock surface is
        // not an input occluder (input-identity finding, not a name
        // exemption). Any shortfall refuses with zero synthetic posts.
        let ((place_cg_x, place_cg_y), (appkit_x, appkit_y)) = {
            let ns = match crate::ffi::ns_display_snapshot() {
                Ok(s) => s,
                Err(e) => fail_gate!(
                    "helper-place",
                    McError::Unsupported(format!("display snapshot unavailable: {e}")),
                    temp_resident
                ),
            };
            if ns.len() != 1 || snap0.displays.len() != 1 {
                fail_gate!(
                    "helper-place",
                    McError::Blocked(format!(
                        "helper placement needs exactly one display (screens={}, snapshot displays={}); refusing multi-display guess",
                        ns.len(),
                        snap0.displays.len()
                    )),
                    temp_resident
                );
            }
            let primary_h = match crate::topology::primary_height(&ns) {
                Some(h) => h,
                None => fail_gate!(
                    "helper-place",
                    McError::Blocked(
                        "primary display height unknown; refusing placement guess".to_owned()
                    ),
                    temp_resident
                ),
            };
            let visible = match crate::topology::join_displays(&ns)[0].visible {
                Some(v) => v,
                None => fail_gate!(
                    "helper-place",
                    McError::Blocked(
                        "visible frame unconvertible; refusing placement guess".to_owned()
                    ),
                    temp_resident
                ),
            };
            let all = match crate::ffi::cg_window_list_basic() {
                Ok(a) => a,
                Err(e) => fail_gate!(
                    "helper-place",
                    McError::System(format!("CG list unreadable for placement: {e}")),
                    temp_resident
                ),
            };
            if all.len() >= crate::config::MAX_WINDOWS {
                fail_gate!(
                    "helper-place",
                    McError::Blocked(
                        "CG snapshot completeness unknown; refusing placement".to_owned()
                    ),
                    temp_resident
                );
            }
            let avoid: Vec<(f64, f64, f64, f64)> = all
                .iter()
                .filter(|w| w.onscreen && w.layer == 0)
                .map(|w| (w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h))
                .collect();
            // Search the slack-expanded rect (content plus titlebar headroom
            // above): the spawn argument positions the AppKit *content*
            // origin exactly, while the settled frame adds host chrome above
            // it. The content origin sits one slack below the expanded
            // origin, so the settled frame stays inside the verified rect.
            let search_h = crate::space_drag_transfer::HELPER_SPAWN_H
                + crate::space_drag_transfer::TITLEBAR_SLACK;
            let origin = crate::space_drag_transfer::uncovered_helper_origin(
                (visible.x, visible.y, visible.w, visible.h),
                &avoid,
                crate::space_drag_transfer::HELPER_SPAWN_W,
                search_h,
            );
            let (ex, ey) = match origin {
                Some(o) => o,
                None => fail_gate!(
                    "helper-place",
                    McError::Blocked(format!(
                        "no uncovered {}x{search_h:.0} rect inside visible {}x{}@{:.0},{:.0} over {} same-layer rects; refusing (never Ghostty, never guessed)",
                        crate::space_drag_transfer::HELPER_SPAWN_W,
                        visible.w,
                        visible.h,
                        visible.x,
                        visible.y,
                        avoid.len()
                    )),
                    temp_resident
                ),
            };
            let (ox, oy) = (ex, ey + crate::space_drag_transfer::TITLEBAR_SLACK);
            notes.push(format!(
                "helper placement: content origin ({ox:.0},{oy:.0}) from expanded ({ex:.0},{ey:.0}) over {} same-layer rects in visible {:.0}x{:.0}@{:.0},{:.0}",
                avoid.len(),
                visible.w,
                visible.h,
                visible.x,
                visible.y
            ));
            // Controller-side AppKit origin for the helper's self-positioning
            // spawn argument (pure inverse of the AppKit->CG conversion,
            // same snapshot row as the placement above).
            match crate::space_drag_transfer::cg_top_left_to_appkit_origin(
                ox,
                oy,
                crate::space_drag_transfer::HELPER_SPAWN_W,
                crate::space_drag_transfer::HELPER_SPAWN_H,
                primary_h,
            ) {
                Some(o) => {
                    notes.push(format!(
                        "helper placement: AppKit spawn origin ({:.0},{:.0})",
                        o.0, o.1
                    ));
                    ((ox, oy), o)
                }
                None => fail_gate!(
                    "helper-place",
                    McError::Blocked("origin conversion refused; never guessed".to_owned()),
                    temp_resident
                ),
            }
        };

        // Spawn the owned regular-policy helper with the self-join poll.
        let nonce = crate::probe_target::mint_nonce().map_err(|e| (1, format!("nonce: {e}")))?;
        let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, seconds)
            .map(|s| {
                s.with_regular_policy(true)
                    .with_space_join(true)
                    .with_origin(appkit_x, appkit_y)
            })
            .map_err(|e| (2, e))?;
        let mut raw = crate::probe_target::start_helper_process(&spec)
            .map_err(|e| (1, format!("helper spawn failed: {e}")))?;
        if let Err(e) = crate::probe_target::await_handshake(&mut raw, &spec) {
            return Err(fail(
                "helper-handshake",
                McError::System(format!("helper spawn failed: {e}")),
                &mut caffeinate,
                Some(&mut raw.child),
                Some(raw.pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                false,
            ));
        }
        if let Err(e) = crate::probe_target::record_spawn(raw.pid, &spec, &raw.helper_path) {
            return Err(fail(
                "helper-record",
                McError::System(format!("helper spawn failed: {e}")),
                &mut caffeinate,
                Some(&mut raw.child),
                Some(raw.pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                false,
            ));
        }
        let crate::probe_target::UnreadySpawn {
            mut child,
            pid,
            helper_path,
            lines,
            line_drops: _,
        } = raw;
        if std::thread::Builder::new()
            .name("drag-transfer-drain".to_owned())
            .spawn(move || while lines.recv().is_ok() {})
            .is_err()
        {
            return Err(fail(
                "helper-drain",
                McError::System("drain spawn failed".to_owned()),
                &mut caffeinate,
                Some(&mut child),
                Some(pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                false,
            ));
        }
        macro_rules! fail_child {
            ($stage:expr, $err:expr) => {
                return Err(fail(
                    $stage,
                    $err,
                    &mut caffeinate,
                    Some(&mut child),
                    Some(pid),
                    display,
                    original,
                    Some(&snap0),
                    &ghostty_before,
                    &mut budget,
                    &mut notes,
                    temp_resident,
                    false,
                ))
            };
        }
        let owned = match crate::ax::authorize_owned_action(pid, &nonce) {
            Ok(o) => o,
            Err(e) => {
                return Err(fail(
                    "baseline-identity",
                    McError::Ownership(e.to_string()),
                    &mut caffeinate,
                    Some(&mut child),
                    Some(pid),
                    display,
                    original,
                    Some(&snap0),
                    &ghostty_before,
                    &mut budget,
                    &mut notes,
                    temp_resident,
                    false,
                ));
            }
        };
        if crate::ownership::is_protected_bundle(&owned.bundle_id) {
            return Err(fail(
                "baseline-identity",
                McError::Ownership("helper bound to a protected bundle; refusing".to_owned()),
                &mut caffeinate,
                Some(&mut child),
                Some(pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                false,
            ));
        }
        let wid = match crate::focus::join_helper_wid(pid) {
            Ok(w) => w,
            Err(e) => fail_child!("baseline-join", McError::Ownership(e.to_string())),
        };
        match crate::spaces::window_spaces_owned(&owned, pid, wid) {
            Ok(m) if crate::space_recovery::membership_is_exact(&m, original) => {}
            Ok(m) => fail_child!(
                "baseline-membership",
                McError::Blocked(format!("helper membership {m:?}, not exactly [{original}]"))
            ),
            Err(e) => fail_child!("baseline-membership", McError::System(e.to_string())),
        }
        // Placement postcondition (not dead reckoning): the helper's LIVE
        // rects (CG bounds plus AX frame, freshly read) must not overlap any
        // onscreen Ghostty rect. AX/CG/content spaces differ by host
        // window-chrome metrics, so exact equality with the requested origin
        // is the wrong check; requested-vs-actual is logged for diagnosis
        // only. Any overlap is a fixture fault with zero synthetic posts.
        {
            let live_entry = match crate::cg::verify_wid_owned_by_pid(pid, wid) {
                Ok(e) => e,
                Err(e) => {
                    fail_child!("helper-place-readback", McError::Ownership(e.to_string()))
                }
            };
            let live_ax = match crate::ax::read_window_owned(&owned, pid, wid) {
                Ok(r) => match r.ax_frame {
                    Some(f) => f,
                    None => fail_child!(
                        "helper-place-readback",
                        McError::System("placed AX frame unreadable; refusing".to_owned())
                    ),
                },
                Err(e) => fail_child!("helper-place-readback", McError::System(e.to_string())),
            };
            let ghostly = match crate::mc_lifecycle::snapshot_ghostty_windows() {
                Ok(g) => g,
                Err(e) => fail_child!(
                    "helper-place-readback",
                    McError::System(format!("Ghostty snapshot unreadable: {e}"))
                ),
            };
            let onscreen_ghostly: Vec<(f64, f64, f64, f64)> = ghostly
                .iter()
                .filter(|g| g.onscreen)
                .map(|g| (g.x, g.y, g.w, g.h))
                .collect();
            let live_rects = [
                (
                    "CG",
                    (
                        live_entry.bounds.x,
                        live_entry.bounds.y,
                        live_entry.bounds.w,
                        live_entry.bounds.h,
                    ),
                ),
                ("AX", (live_ax.x, live_ax.y, live_ax.w, live_ax.h)),
            ];
            for (name, (lx, ly, lw, lh)) in live_rects {
                let live_rect = crate::topology::PointsRect {
                    x: lx,
                    y: ly,
                    w: lw,
                    h: lh,
                };
                for g in &onscreen_ghostly {
                    let ghostly_rect = crate::topology::PointsRect {
                        x: g.0,
                        y: g.1,
                        w: g.2,
                        h: g.3,
                    };
                    if live_rect.overlap_area(&ghostly_rect) > 0.0 {
                        fail_child!(
                            "helper-place-readback",
                            McError::Blocked(format!(
                                "helper {name} rect ({lx:.0},{ly:.0} {lw:.0}x{lh:.0}) overlaps onscreen Ghostty ({:.0},{:.0} {:.0}x{:.0}); refusing misplaced fixture (zero synthetic posts)",
                                g.0, g.1, g.2, g.3
                            ))
                        );
                    }
                }
            }
            notes.push(format!(
                "helper placed CG ({:.0},{:.0} {:.0}x{:.0}) AX ({:.0},{:.0} {:.0}x{:.0}), requested content ({place_cg_x:.0},{place_cg_y:.0}); no overlap over {} onscreen Ghostty rects",
                live_entry.bounds.x,
                live_entry.bounds.y,
                live_entry.bounds.w,
                live_entry.bounds.h,
                live_ax.x,
                live_ax.y,
                live_ax.w,
                live_ax.h,
                onscreen_ghostly.len()
            ));
        }
        let baseline_frame = match crate::ax::read_window_owned(&owned, pid, wid) {
            Ok(r) => match r.ax_frame {
                Some(f) => f,
                None => fail_child!(
                    "baseline-frame",
                    McError::System("helper AX frame unreadable; refusing".to_owned())
                ),
            },
            Err(e) => fail_child!("baseline-frame", McError::System(e.to_string())),
        };
        if let Err(e) = focus_helper(pid, wid) {
            fail_child!("baseline-focus", McError::Blocked(e));
        }
        // Cursor baseline (read-only stamp; restored post-release by an
        // eventless warp, never fought when the user moved it).
        let cursor_baseline = cursor_position();
        notes.push(format!(
            "cursor baseline: {}",
            cursor_baseline
                .map(|(x, y)| format!("({x:.0}, {y:.0})"))
                .unwrap_or_else(|| "unreadable".to_owned())
        ));

        // Independent recovery FIRST: public self-join O->T->O through MC
        // gotos (reused held helpers). Any shortfall refuses BEFORE any
        // synthetic post. The first leg starts from the post-create surface
        // (the temp exists there); the pre-create baseline would mismatch
        // the Space set.
        let snap_t = match recover_leg(
            live_topology,
            &mut child,
            pid,
            &nonce,
            &helper_path,
            wid,
            display,
            temp,
            "recover-goto-temp",
            "recover-join-temp",
            &mut budget,
            &mut latencies,
            &mut notes,
        ) {
            Ok(s) => s,
            Err(e) => fail_child!("recover-temp", e),
        };
        let snap_o = match recover_leg(
            &snap_t,
            &mut child,
            pid,
            &nonce,
            &helper_path,
            wid,
            display,
            original,
            "recover-goto-original",
            "recover-join-original",
            &mut budget,
            &mut latencies,
            &mut notes,
        ) {
            Ok(s) => s,
            Err(e) => fail_child!("recover-original", e),
        };
        let _ = snap_o;
        if let Err(e) = crate::mc_observe::await_ghostty_restored(&ghostty_before) {
            fail_child!("recover-settle", e);
        }
        notes.push(
            "independent self-join recovery O->T->O proven; synthetic legs follow".to_owned(),
        );

        // Synthetic transfer through the shared executor (legs spend the
        // main budget inside per-leg setup; no unbounded waits).
        if let Err(e) = focus_helper(pid, wid) {
            fail_child!("transfer-focus", McError::Blocked(e));
        }
        // Pre-synthetic input-identity gate (read-only): BOTH native
        // hit-tests must already join the exact helper at the titlebar
        // center after the self-join recovery, before any post. A mismatch
        // is a fixture observation fault (zero posts), never a transfer
        // attempt. Each synthetic leg re-proves this in its own setup.
        // Fresh authorization (never the baseline identity): LaunchServices
        // may publish the bundled-helper bundle mid-run, rebinding the live
        // bundle from the probe one to the dedicated helper one — the
        // established per-leg re-authorization pattern.
        {
            let fresh_owned = match crate::ax::authorize_owned_action(pid, &nonce) {
                Ok(o) => o,
                Err(e) => fail_child!("transfer-identity", McError::Ownership(e.to_string())),
            };
            let frame = match crate::ax::read_window_owned(&fresh_owned, pid, wid) {
                Ok(r) => match r.ax_frame {
                    Some(f) => f,
                    None => fail_child!(
                        "transfer-identity",
                        McError::System("transfer AX frame unreadable; refusing".to_owned())
                    ),
                },
                Err(e) => fail_child!("transfer-identity", McError::System(e.to_string())),
            };
            let point = match crate::space_drag_transfer::titlebar_grab_point(frame) {
                Ok(p) => p,
                Err(e) => fail_child!("transfer-identity", McError::Blocked(e)),
            };
            let entry = match crate::cg::verify_wid_owned_by_pid(pid, wid) {
                Ok(e) => e,
                Err(e) => fail_child!("transfer-identity", McError::Ownership(e.to_string())),
            };
            let all = match crate::ffi::cg_window_list_basic() {
                Ok(a) => a,
                Err(e) => fail_child!("transfer-identity", McError::System(e.to_string())),
            };
            let hit = match crate::ffi::ax_hit_test_owner(point.0, point.1) {
                Ok(h) => h,
                Err(e) => fail_child!("transfer-identity", McError::System(e)),
            };
            let input_wid = match crate::ffi::sls_window_at_point(point.0, point.1) {
                Ok(w) => w,
                Err(e) => fail_child!("transfer-identity", McError::System(e)),
            };
            match crate::space_drag_transfer::hit_test_input_grab(point, &entry, &all, hit, input_wid)
            {
                Ok(()) => notes.push(format!(
                    "transfer-identity: AX owner=({},{}) SLS wid={} both join helper at ({:.0},{:.0}); synthetic legs follow",
                    hit.0, hit.1, input_wid, point.0, point.1
                )),
                Err(e) => fail_child!(
                    "transfer-identity",
                    McError::Blocked(format!(
                        "pre-synthetic input identity refuses at ({:.0},{:.0}): AX owner=({},{}) SLS wid={input_wid}: {e} (zero synthetic posts)",
                        point.0, point.1, hit.0, hit.1
                    ))
                ),
            }
        }
        let t = std::time::Instant::now();
        let (roundtrip, drag_end) = {
            let mut backend = LiveDragBackend {
                child: &mut child,
                budget: &mut budget,
                pid,
                wid,
                nonce: nonce.clone(),
                helper_path: helper_path.clone(),
                display,
                temp,
                chords,
                // `begin_leg` reselects before every post; the initial pair
                // is the outward triple (the roundtrip posts outward first).
                keycode: chords.outward_key,
                flags: chords.outward_flags,
                point: (0.0, 0.0),
                last_point: (0.0, 0.0),
                counts: PostCounts::default(),
            };
            let report = crate::space_drag_transfer::run_drag_roundtrip(
                &mut backend,
                original,
                temp,
                &mut notes,
            );
            (report, backend.last_point)
        };
        latencies.push(LegLatency {
            leg: "synthetic-roundtrip".to_owned(),
            ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
        let synthetic_posts = roundtrip.posts_total > 0;

        // An unresolved release STOPS all further MC/recovery actions (do
        // not enter Mission Control while the mouse could be held -- MC is
        // deliberately left as-is here, not even closed): caffeinate + exact
        // helper stop only, then read-only evidence and an explicit
        // out-of-tap recovery report.
        if roundtrip.unresolved {
            notes.push(
                "UNRESOLVED synthetic edge: no MC/recovery action follows (MC left as-is); out-of-tap recovery: use the physical mouse and keyboard (click to release any held drag, Mission Control thumbnail to return the owned helper), then Force Quit/Activity Monitor on the exact helper PID below if needed"
                    .to_owned(),
            );
            if let Some(t) = temp_resident {
                notes.push(crate::space_drag_transfer::temp_residue_hint(t, original));
            }
            let reaped = caffeinate.stop();
            let stop = crate::session::stop_exact_child(&mut child, pid);
            let helpers_absent = !crate::ffi::process_exists(pid as i32)
                && crate::ffi::cg_window_list_all()
                    .map(|list| list.iter().all(|w| w.pid != pid))
                    .unwrap_or(false);
            return Err((
                1,
                serde_json::json!({
                    "outcome": "drag-transfer-residue",
                    "action": "spaces-drag-transfer",
                    "original": original,
                    "temp": temp,
                    "numbered": numbered,
                    "display": display,
                    "pid": pid,
                    "wid": wid,
                    "outward": {
                        "ok": roundtrip.outward.ok,
                        "membership": roundtrip.outward.membership,
                        "error": roundtrip.outward.error,
                    },
                    "inverse": roundtrip.inverse.as_ref().map(|i| serde_json::json!({
                        "ok": i.ok,
                        "membership": i.membership,
                        "error": i.error,
                    })),
                    "helper_stopped_exact": stop.exited,
                    "helper_stop": {"exited": stop.exited, "reaped": stop.reaped, "record_removed": stop.record_removed},
                    "helpers_absent": helpers_absent,
                    "caffeinate_reaped": reaped,
                    "temp_residue": temp_resident,
                    "synthetic_events_posted": synthetic_posts,
                    "out_of_tap_recovery": "physical mouse/keyboard; exact helper PID above; no agent MC/recovery action was taken after the unresolved edge",
                    "notes": notes,
                    "latencies": latencies,
                })
                .to_string(),
            ));
        }

        // A failed leg never inverts (the executor guarantees it); restore
        // independently only what the synthetic legs may have moved.
        let mut restored_independently = false;
        if !roundtrip.outward.ok || !roundtrip.inverse.as_ref().is_some_and(|i| i.ok) {
            match crate::space_recovery::poll_membership_held(
                &mut child,
                pid,
                &helper_path,
                wid,
                original,
            ) {
                Ok(_) => {
                    // Helper still home: only the current may need restore.
                    match crate::mc_lifecycle::live::fresh_snapshot() {
                        Err(e) => notes.push(format!("restore snapshot unreadable: {e}")),
                        Ok(after)
                            if after.displays.get(display).and_then(|d| d.current)
                                == Some(original) =>
                        {
                            notes.push(format!("restore: already on original {original}; no goto"));
                        }
                        Ok(after) => {
                            if after.locate(original).is_some_and(|(di, _)| di == display) {
                                match crate::mc_lifecycle::goto_sid_verified(
                                    &after,
                                    display,
                                    original,
                                    &mut budget,
                                    true,
                                ) {
                                    Ok(_) => notes
                                        .push(format!("restore: MC AX goto original {original}")),
                                    Err(e) => notes.push(format!("restore goto refused: {e}")),
                                }
                            } else {
                                notes.push(format!(
                                    "restore: original {original} not located; leaving currents"
                                ));
                            }
                        }
                    }
                }
                Err(_) => {
                    // Helper may have moved without proof: goto original
                    // current, then the independent self-join home.
                    if let Ok(after) = crate::mc_lifecycle::live::fresh_snapshot()
                        && after.locate(original).is_some_and(|(di, _)| di == display)
                        && crate::mc_lifecycle::goto_sid_verified(
                            &after,
                            display,
                            original,
                            &mut budget,
                            true,
                        )
                        .is_ok()
                        && crate::space_recovery::run_join_leg_held(
                            &mut child,
                            pid,
                            &nonce,
                            &helper_path,
                            wid,
                        )
                        .is_ok()
                        && crate::space_recovery::poll_membership_held(
                            &mut child,
                            pid,
                            &helper_path,
                            wid,
                            original,
                        )
                        .is_ok()
                    {
                        restored_independently = true;
                        notes.push("restore: independent self-join home proven".to_owned());
                    } else {
                        notes.push("restore: independent self-join home unproven".to_owned());
                    }
                }
            }
        }

        // Pointer restore: eventless warp home only when the baseline was
        // readable and the cursor still sits where the drag left it; a
        // user-moved (or unreadable) cursor is left alone, and that gates
        // completion. The warp delivers no input event, so a baseline over
        // a protected surface (e.g. the terminal) is never synthesized
        // into -- a refused/unavailable warp leaves the cursor and reports,
        // never a `MouseMoved` fallback. Bounded poll-verify.
        let mut pointer_restored = false;
        match (cursor_position(), cursor_baseline) {
            (_, None) => {
                notes.push("pointer: baseline unreadable; restore unverifiable".to_owned());
            }
            (now, Some(base)) => {
                match crate::space_drag_transfer::pointer_restore_decision(now, drag_end, base) {
                    crate::space_drag_transfer::PointerRestore::AlreadyHome => {
                        pointer_restored = true;
                        notes.push("pointer: already at baseline; no move".to_owned());
                    }
                    crate::space_drag_transfer::PointerRestore::LeaveUserMoved => {
                        notes.push(
                            "pointer: cursor moved by user; left as-is, completion refused"
                                .to_owned(),
                        );
                    }
                    crate::space_drag_transfer::PointerRestore::Restore { to } => {
                        if edge_synth_gates("pointer-restore").is_ok() {
                            match crate::ffi::warp_cursor_position(to.0, to.1) {
                                Ok(()) => {
                                    let deadline = std::time::Instant::now()
                                        + std::time::Duration::from_millis(500);
                                    while std::time::Instant::now() < deadline {
                                        if cursor_position().is_some_and(|at| {
                                            (at.0 - to.0).abs() <= 1.0 && (at.1 - to.1).abs() <= 1.0
                                        }) {
                                            pointer_restored = true;
                                            break;
                                        }
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                    }
                                }
                                Err(e) => {
                                    notes.push(format!(
                                        "pointer: warp refused ({e}); no event posted, restore unverified"
                                    ));
                                }
                            }
                        } else {
                            notes.push(
                                "pointer: restore gates refused; no warp, no event posted"
                                    .to_owned(),
                            );
                        }
                        notes.push(format!(
                            "pointer: restore warp attempted; restored={pointer_restored}"
                        ));
                    }
                }
            }
        }

        // Cleanup + verification (frame read while alive, exact stop,
        // absence, temp removal, ordered topology, Ghostty stability +
        // natural focus return observed, never forced). The caffeinate
        // companion stays up through the last mutation (temp removal) and
        // stops after it.
        crate::mc_lifecycle::safe_close_mc(&mut notes);
        // Re-authorize as in per-leg setup: LaunchServices may publish the
        // helper's bundle identity after spawn, invalidating the early
        // snapshot. A refused fresh authorization remains unknown, never equal.
        let fresh_owned = crate::ax::authorize_owned_action(pid, &nonce).ok();
        let frame_after = fresh_owned
            .as_ref()
            .and_then(|o| crate::ax::read_window_owned(o, pid, wid).ok())
            .and_then(|r| r.ax_frame);
        if fresh_owned.is_none() {
            notes.push("helper final frame authorize refused; unknown, never equal".to_owned());
        }
        let frame_exact = frame_after.is_some_and(|f| f == baseline_frame);
        match frame_after {
            None => notes.push("helper frame exact across run: false (final AX frame unreadable; unknown, never equal)".to_owned()),
            Some(f) => match crate::space_drag_transfer::ax_frame_mismatch(baseline_frame, f) {
                None => notes.push("helper frame exact across run: true".to_owned()),
                Some(detail) => notes.push(format!("helper frame exact across run: false ({detail})")),
            },
        }
        let stop = crate::session::stop_exact_child(&mut child, pid);
        notes.push(format!(
            "helper exact-Child stop: exited={} reaped={} record_removed={}",
            stop.exited, stop.reaped, stop.record_removed
        ));
        if !stop.exited {
            return Err(fail(
                "helper-stop",
                McError::System("owned helper did not exit bounded; record retained".to_owned()),
                &mut caffeinate,
                Some(&mut child),
                Some(pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                synthetic_posts,
            ));
        }
        let stop_full = stop.exited && stop.reaped && stop.record_removed;
        let helpers_absent = !crate::ffi::process_exists(pid as i32)
            && crate::ffi::cg_window_list_all()
                .map(|list| list.iter().all(|w| w.pid != pid))
                .unwrap_or(false);
        // Temporary-mode teardown: remove ONLY the internally created SID
        // (a named preexisting temp is never removed here) after proving
        // the current back on the original. The removal spends the cleanup
        // phase (it is this run's own cleanup); a refused removal is
        // residue naming the exact SID plus the mc-recover hint -- never a
        // replay, never a broadened cleanup. The caffeinate companion stays
        // up through this removal (the last mutation) and stops after it.
        if let Some(created) = created_temp {
            notes
                .push("temporary: caffeinate held through temp removal (last mutation)".to_owned());
            let t = std::time::Instant::now();
            let restore_from = match crate::mc_lifecycle::live::fresh_snapshot() {
                Ok(s) => s,
                Err(e) => {
                    notes.push(format!(
                        "temporary temp {created} pre-remove snapshot unreadable; residue hint: mc-recover --temp {created} --original {original}"
                    ));
                    return Err(fail(
                        "temporary-pre-remove",
                        McError::System(format!("pre-remove snapshot unreadable: {e}")),
                        &mut caffeinate,
                        None,
                        Some(pid),
                        display,
                        original,
                        Some(&snap0),
                        &ghostty_before,
                        &mut budget,
                        &mut notes,
                        temp_resident,
                        synthetic_posts,
                    ));
                }
            };
            if restore_from.displays.get(display).and_then(|d| d.current) != Some(original) {
                if restore_from
                    .locate(original)
                    .is_some_and(|(di, _)| di == display)
                {
                    match crate::mc_lifecycle::goto_sid_verified(
                        &restore_from,
                        display,
                        original,
                        &mut budget,
                        true,
                    ) {
                        Ok(_) => notes.push(format!(
                            "temporary: MC AX goto original {original} before remove"
                        )),
                        Err(e) => {
                            notes.push(format!(
                                "temporary temp {created} restore-goto refused; residue hint: mc-recover --temp {created} --original {original}"
                            ));
                            return Err(fail(
                                "temporary-restore-goto",
                                e,
                                &mut caffeinate,
                                None,
                                Some(pid),
                                display,
                                original,
                                Some(&snap0),
                                &ghostty_before,
                                &mut budget,
                                &mut notes,
                                temp_resident,
                                synthetic_posts,
                            ));
                        }
                    }
                } else {
                    notes.push(format!(
                        "temporary temp {created}: original {original} not on display {display}; leaving currents, residue hint: mc-recover --temp {created} --original {original}"
                    ));
                    return Err(fail(
                        "temporary-restore-locate",
                        McError::Blocked(format!(
                            "original {original} not uniquely on display {display} before temp removal; refusing"
                        )),
                        &mut caffeinate,
                        None,
                        Some(pid),
                        display,
                        original,
                        Some(&snap0),
                        &ghostty_before,
                        &mut budget,
                        &mut notes,
                        temp_resident,
                        synthetic_posts,
                    ));
                }
            } else {
                notes.push(format!(
                    "temporary: already on original {original}; no goto before remove"
                ));
            }
            let mut temp_remove_dispatched = false;
            match crate::mc_lifecycle::remove_sid_verified(
                created,
                &mut budget,
                true,
                &mut temp_remove_dispatched,
            ) {
                Ok((scan, close)) => {
                    notes.push(format!(
                        "temporary temp {created} removed ({} candidates clear, {} shell skipped)",
                        scan.checked, scan.shell_skipped
                    ));
                    if let Some(c) = close {
                        notes.push(format!("temporary remove close shortfall: {c}"));
                    }
                }
                Err(e) => {
                    notes.push(format!(
                        "temporary temp {created} removal refused; residue hint: mc-recover --temp {created} --original {original}"
                    ));
                    return Err(fail(
                        "temporary-remove",
                        e,
                        &mut caffeinate,
                        None,
                        Some(pid),
                        display,
                        original,
                        Some(&snap0),
                        &ghostty_before,
                        &mut budget,
                        &mut notes,
                        temp_resident,
                        synthetic_posts,
                    ));
                }
            }
            // Removal proven: no resident temp remains for later reports.
            temp_resident = None;
            latencies.push(LegLatency {
                leg: "temporary-remove".to_owned(),
                ms: t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            });
        }
        // Caffeinate stops after the last mutation (temp removal above, if
        // any); read-only verification follows.
        if let Err(e) = crate::mc_observe::await_ghostty_restored(&ghostty_before) {
            notes.push(format!("final transition settle: {e}"));
        }
        let reaped = caffeinate.stop();
        let snap_final = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| {
            fail(
                "verify-topology",
                McError::System(format!("final snapshot unreadable: {e}")),
                &mut caffeinate,
                None,
                Some(pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                synthetic_posts,
            )
        })?;
        let topology_restored = topology_equal(&snap0, &snap_final);
        let ghostty_after = crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| {
            fail(
                "verify-ghostty",
                McError::System(format!("Ghostty snapshot unreadable: {e}")),
                &mut caffeinate,
                None,
                Some(pid),
                display,
                original,
                Some(&snap0),
                &ghostty_before,
                &mut budget,
                &mut notes,
                temp_resident,
                synthetic_posts,
            )
        })?;
        let ghostty_ok =
            crate::mc_probe::ghostty_stable_across_switch(&ghostty_before, &ghostty_after);
        if let Some(detail) =
            crate::mc_probe::ghostty_mismatch_detail(&ghostty_before, &ghostty_after)
        {
            notes.push(format!("ghostty stability: false ({detail})"));
        } else {
            notes.push("ghostty stability: true (onscreen excluded by design)".to_owned());
        }
        let focus_after = crate::mc_lifecycle::capture_focus_legs();
        let natural_return = crate::mc_probe::focus_ghostty_returned(&focus_after);
        if !natural_return {
            notes.push(
                "no natural Ghostty focus return observed after helper stop (reported, not claimed)"
                    .to_owned(),
            );
        }
        let inverse_ok = roundtrip.inverse.as_ref().is_some_and(|i| i.ok);
        // Conditional evidence: only actually-proven legs are claimed.
        let mut evidence = vec!["independent self-join recovery proven before any synthetic post"];
        if roundtrip.outward.ok {
            evidence.push("outward drag proven by exact post-release membership+current readback");
        }
        if inverse_ok {
            evidence.push("inverse drag proven by exact post-release membership+current readback");
        }
        if restored_independently {
            evidence.push("independent self-join home proven after a failed synthetic leg");
        }
        let complete = roundtrip.outward.ok
            && inverse_ok
            && roundtrip.releases_balanced
            && !roundtrip.unresolved
            && frame_exact
            && topology_restored
            && stop_full
            && helpers_absent
            && reaped
            && pointer_restored
            && ghostty_ok;
        let outcome = if complete {
            "drag-transfer-complete"
        } else {
            "drag-transfer-residue"
        };
        let report = serde_json::json!({
            "outcome": outcome,
            "original": original,
            "temp": temp,
            "numbered": numbered,
            "temp_created_internally": created_temp.is_some(),
            "display": display,
            "pid": pid,
            "wid": wid,
            "outward": {
                "ok": roundtrip.outward.ok,
                "membership": roundtrip.outward.membership,
                "current": roundtrip.outward.current,
                "pending_recovered": roundtrip.outward.pending_recovered,
                "counts": roundtrip.outward.counts,
                "error": roundtrip.outward.error,
            },
            "inverse": roundtrip.inverse.as_ref().map(|i| serde_json::json!({
                "ok": i.ok,
                "membership": i.membership,
                "current": i.current,
                "pending_recovered": i.pending_recovered,
                "counts": i.counts,
                "error": i.error,
            })),
            "restored_independently": restored_independently,
            "topology_restored": topology_restored,
            "frame_exact": frame_exact,
            "helper_stopped_exact": stop_full,
            "helper_stop": {"exited": stop.exited, "reaped": stop.reaped, "record_removed": stop.record_removed},
            "helpers_absent": helpers_absent,
            "ghostty_stable": ghostty_ok,
            "natural_focus_returned": natural_return,
            "focus_before": focus_before,
            "focus_after": focus_after,
            "pointer_restored": pointer_restored,
            "caffeinate_reaped": reaped,
            "synthetic_events_posted": synthetic_posts,
            "live_evidence": evidence.join("; "),
            "notes": notes,
            "latencies": latencies,
        });
        let body = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
        if complete { Ok(body) } else { Err((1, body)) }
    }
}

#[cfg(target_os = "macos")]
pub use live_impl::run_spaces_drag_transfer;

#[cfg(not(target_os = "macos"))]
pub fn run_spaces_drag_transfer(
    original: u64,
    temp: Option<u64>,
    seconds: u64,
    allow_untested: bool,
    numbered: bool,
) -> Result<String, (i32, String)> {
    let _ = numbered;
    match temp {
        Some(t) => {
            crate::space_drag_transfer::validate_drag_args(original, t, seconds, allow_untested)
                .map_err(|e| (2, e))?;
        }
        None => {
            crate::space_drag_transfer::validate_drag_args_temporary(
                original,
                seconds,
                allow_untested,
            )
            .map_err(|e| (2, e))?;
        }
    }
    Err((1, "spaces-drag-transfer is macOS-only".to_owned()))
}
