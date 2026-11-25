#![forbid(unsafe_code)]
use super::*;

/// Chord-time origin snapshot for one managed window: HWND plus the
/// process-lifetime evidence the owner rechecks. Only the token may enter logs.
fn snap_origin_of(window: &ObservedWindow) -> SnapOrigin {
    SnapOrigin {
        hwnd: window.hwnd,
        token: window.token.clone(),
        pid: window.identity.pid,
        creation: window.identity.process_creation.clone(),
    }
}

/// Record a verified own focus advance for later intents in the same batch
/// and across bounded drains; cleared on external focus, mismatch,
/// suspension, or gesture. Retained-aware: a focus-ok into a maximized member
/// advances from its retained row, never a stale None. Shared by the ordinary
/// tiled focus arm and the float-origin focus arm.
pub(super) fn note_focus_advance(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    to: &str,
) {
    state.snap_advance = observed
        .iter()
        .find(|w| w.token == to)
        .map(snap_origin_of)
        .or_else(|| {
            retained
                .iter()
                .find(|r| r.token == to)
                .map(|row| crate::snapkey::SnapOrigin {
                    hwnd: row.key.hwnd,
                    token: row.token.clone(),
                    pid: row.key.pid,
                    creation: row.key.creation.clone(),
                })
        });
}

/// One verified member origin: existing member with full stored identity, a
/// live matching lifetime tag, an in-scope executable, the hosted-child gate,
/// and (for proof owners) the frozen allowlist. Exactness stays with the
/// per-intent owner recheck.
fn verified_snap_origin(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
    token: &str,
) -> Option<SnapOrigin> {
    let stored = state.member_identity.get(key)?;
    if stored.pid != key.pid
        || stored.process_creation != key.creation
        || stored.user_sid != me.user_sid
        || stored.session_id != me.session_id
    {
        return None;
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    let tag = state.member_tags.get(key)?;
    if !crate::native_guards::visible_lifetime_ok(tag, live_tag.as_deref()) {
        return None;
    }
    if !scope_allows(&state.scope, &stored.exe_path) {
        return None;
    }
    if !hosted_gate_allows(
        &stored.exe_path,
        key.hwnd,
        stored.pid,
        &state.scope_hosts,
    ) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == key.hwnd)?;
        if verify_proof_owned(key.hwnd, entry, me).is_err() {
            return None;
        }
    }
    Some(SnapOrigin {
        hwnd: key.hwnd,
        token: token.to_owned(),
        pid: key.pid,
        creation: key.creation.clone(),
    })
}

/// Managed origins for chord resolution: eligible observed windows, verified
/// overlay-retained members (maximized/fullscreen), and verified iconic
/// members that Windows can hold as foreground. Only existing members with
/// full identity plus a live lifetime-tag match ride along: no foreign or
/// invented members. Exactness stays with the per-intent owner recheck.
fn managed_origins(
    state: &TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> Vec<SnapOrigin> {
    let mut out: Vec<SnapOrigin> = observed.iter().map(snap_origin_of).collect();
    for row in retained {
        if !row.maximized && !row.fullscreen {
            continue;
        }
        if let Some(origin) = verified_snap_origin(state, me, &row.key, &row.token) {
            out.push(origin);
        }
    }
    // Iconic members are observed-ineligible, so they are neither in the fresh
    // rows nor the overlay-retained set, yet Windows can hold an iconic window
    // as foreground. A verified existing member stays an origin so an
    // iconic-subject command reaches the core settlement path instead of being
    // refused as unmanaged before it runs. Membership is live-checked here so
    // stale tokens for non-iconic or dead windows never enter the map.
    for (key, token) in &state.member_tokens {
        if !live_is_iconic(key.hwnd) {
            continue;
        }
        if out.iter().any(|origin| origin.hwnd == key.hwnd) {
            continue;
        }
        if let Some(origin) = verified_snap_origin(state, me, key, token) {
            out.push(origin);
        }
    }
    out
}

/// Refresh cached management state from a complete observation: managed set,
/// stable rects, gesture retention, and the origin map the keyboard callback
/// binds chords against. The origin map is retained-aware (verified overlay
/// members included) so a maximized foreground still binds a chord origin.
/// Exactness stays with the per-intent owner recheck.
pub(super) fn publish_managed(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    state.managed = observed.iter().map(|w| w.hwnd).collect();
    state.stable = observed.iter().map(|w| (w.hwnd, w.visible)).collect();
    state
        .gesture_before
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .move_kind
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state.snap_origins = managed_origins(state, me, observed, retained)
        .into_iter()
        .map(|origin| (origin.hwnd, origin))
        .collect();
    // Drop pre-gesture state for windows that left management.
    state
        .esc_latched
        .retain(|hwnd| state.managed.contains(hwnd));
    state
        .gesture_esc_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_cursor
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_key
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_tag
        .retain(|hwnd, _| state.managed.contains(hwnd));
}

/// Hide the border overlay with a change-only `active-border` log line.
/// With an active render bridge the hide posts through the handoff (safety
/// epoch plus watermark) and the `Hidden` line fires on the renderer
/// acknowledgement; otherwise the same-thread carrier hides inline.
pub(super) fn hide_border(state: &mut TileLoop, reason: &str) {
    if state.render_bridge.is_some() {
        hide_render_carrier(state, RenderCarrier::Border, reason);
        return;
    }
    let was_visible = state.border_overlay.is_visible();
    state.border_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.border_last.as_deref() == Some(signature.as_str()) && !was_visible {
        return;
    }
    state.border_last = Some(signature);
    // Diagnostic key first (no product effect): a later show under the same
    // target must log its lifecycle, never aggregate against a stale show key.
    note_render_hidden(state, RenderCarrier::Border, format!("hidden:{reason}"));
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "active-border",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Anchored outer rectangle and core-provided paint style from the applied
/// anchored-border row (v4 carries the ring as a witness frame plus per-side
/// outsets, never a precomputed outer rect). The reply's foreground token must
/// name this live target, the row must be visible, and the witness must join
/// the same target at the reply's capture observation. The LIVE frame is
/// expanded by the uniform core-provided outsets; width and radius come from
/// the core descriptor (width > 0, radius >= 0, outsets >= width), with the
/// gap derived as `outsets[0] - width`. Missing/non-uniform outsets, a missing
/// or unjoined witness, an observation mismatch, or frame arithmetic overflow
/// is the geometry veto (`None`), never a stale or guessed rectangle. Pure
/// over its arguments so the anchored path unit-tests without a desktop.
fn anchored_outer_rect(
    presentation_foreground: &str,
    border: &tiler_core::world_report::BorderPresentationV4,
    target: &str,
    live_frame: Rect,
    capture_obs: u64,
) -> Option<(Rect, i32, i32, i32)> {
    if presentation_foreground != target || !border.visible {
        return None;
    }
    let witness = border.witness.as_ref()?;
    // The row must be about this exact live target at every leg: the
    // border subject, the witness subject (`win`), the observed foreground
    // token, and the capture observation the reply is based on. A cross-window
    // or stale row never paints at the current foreground frame.
    if border
        .subject
        .as_ref()
        .is_none_or(|subject| subject.0 != target)
        || witness.win.0 != target
        || witness.foreground.0 != target
        || witness.obs != capture_obs
    {
        return None;
    }
    let [left, top, right, bottom] = border.outsets?;
    if left != top || top != right || right != bottom {
        return None;
    }
    let width = border.width?;
    let radius = border.radius?;
    if width <= 0 || radius < 0 {
        return None;
    }
    let gap = left.checked_sub(width)?;
    if gap < 0 {
        return None;
    }
    Some((
        Rect {
            x: live_frame.x.checked_sub(left)?,
            y: live_frame.y.checked_sub(top)?,
            w: live_frame.w.checked_add(left)?.checked_add(right)?,
            h: live_frame.h.checked_add(top)?.checked_add(bottom)?,
        },
        width,
        gap,
        radius,
    ))
}

/// Fast active-border refresh for one loop wake: reads only the foreground
/// window (fresh identity plus fresh visibility facts), without full window
/// observation, so gesture following never waits on reconciliation. Shapes
/// are tick-derived: the outer ring rides the last applied reply, and the
/// fresh facts only hide (never paint) when the live foreground moved on.
/// Scope fences match tiling exactly: normal `--scope-exe`/`--scope-host-child`
/// or the frozen allowlist (plus owned-helper verification) in proof mode.
/// The overlay HWND itself is same-process and never resolves an identity, so
/// its own move/paint events cannot retarget the border.
pub(super) fn refresh_active_border(state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect]) {
    if !state.border.enabled {
        hide_border(state, "disabled");
        return;
    }
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        hide_border(state, "no-target");
        return;
    }
    if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(foreground) {
        return;
    }
    let hwnd = foreground as isize as HWND;
    // Proof gate before any frame read: frozen-allowlist membership plus
    // owned-helper verification, mirroring the geometry gate. A foreign
    // foreground in proof mode is never a border target.
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == foreground);
        let Some(entry) = entry else {
            hide_border(state, "allowlist-changed");
            return;
        };
        if verify_proof_owned(foreground, entry, me).is_err() {
            hide_border(state, "identity-changed");
            return;
        }
    }
    let window = match observe_window(hwnd, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(ObserveFailure::Known(_, reason)) => {
            hide_border(
                state,
                match reason {
                    SkipReason::Minimized => "minimized",
                    SkipReason::Maximized => "maximized",
                    SkipReason::Fullscreen => "fullscreen",
                    SkipReason::Cloaked => "cloaked",
                    SkipReason::Shell => "shell",
                    _ => "no-target",
                },
            );
            return;
        }
        Err(ObserveFailure::Unknown) => {
            hide_border(state, "no-target");
            return;
        }
    };
    // Normal-mode scope fence (proof mode already gated above): out-of-scope
    // executables never paint, mirroring observation membership.
    if !scope_allows(&state.scope, &window.identity.exe_path)
        || !hosted_gate_allows(
            &window.identity.exe_path,
            window.hwnd,
            window.identity.pid,
            &state.scope_hosts,
        )
    {
        hide_border(state, "scope-excluded");
        return;
    }
    let hidden_workspace = state.hidden_claims.keys().any(|k| k.hwnd == foreground)
        && state
            .member_home(
                &state
                    .hidden_claims
                    .keys()
                    .find(|k| k.hwnd == foreground)
                    .cloned()
                    .expect("hidden key present"),
            )
            .is_some_and(|loc| loc.hidden);
    // Targeted shell-popup gate (border-only): base tiling shell plus
    // Start/taskbar-flyout/Alt+Tab/TaskView analogues. Ordinary dialogs,
    // tool windows, and owned windows stay eligible here.
    let exe_file = exe_file_name(&window.identity.exe_path).to_ascii_lowercase();
    let border_shell = window.facts.shell || is_border_shell_popup(&class_of(hwnd), &exe_file);
    // Core-owned eligibility from normalized fresh facts (the same pure
    // policy the core evaluates tick-side for the reply ingress):
    // ordinary floats and scoped unmanaged foregrounds carry no tiled
    // token and stay eligible here.
    if let Err(reason) =
        tiler_core::visual::border_eligibility(&tiler_core::visual::BorderEligibilityFacts {
            enabled: true,
            has_window: true,
            minimized: window.facts.minimized,
            fullscreen: window.facts.captionless_fullscreen,
            maximized: window.facts.maximized,
            cloaked: window.facts.cloaked,
            hidden_workspace,
            shell: border_shell,
        })
    {
        hide_border(state, reason.as_str());
        return;
    }
    // Mirror the target's topmost band so a topmost active window stays
    // bordered; ordinary targets demote a previously topmost ring. Read-only
    // style query, never a foreign write.
    let Some(exstyle) = crate::ffi::window_exstyle_checked(foreground) else {
        hide_border(state, "unreadable");
        return;
    };
    let topmost = exstyle & WS_EX_TOPMOST != 0;
    // Uniform presentation: the outer ring rides the last applied reply
    // (computed tick-side from the same normalized facts and core-scaled
    // style); fresh facts only hide when the live foreground moved on.
    // v4 carries the anchored ring as the witness frame plus per-side
    // outsets, never a precomputed outer rect. Expand the LIVE frame by the
    // core-provided outsets and consume the core width/gap/radius. Missing,
    // non-uniform or unjoined witness/outsets/style, an observation mismatch,
    // or frame arithmetic overflow is the geometry veto, never a stale paint.
    let Some((outer, width_px, gap_px, radius_px)) = state.world.last_reply().and_then(|reply| {
        anchored_outer_rect(
            &reply.presentation.foreground,
            &reply.presentation.border,
            &window.token,
            window.visible,
            reply.based_on_observation_seq,
        )
    }) else {
        hide_border(state, "geometry");
        return;
    };
    // Shared owner resolve first: the first consumer to observe a moved
    // version runs the single WinRT resolve, and the border plus the pager
    // both paint from that same snapshot (plain data, no second query).
    let accent = super::accent_owner::refresh_shared_accent(state);
    // Bridged present: post the owner-resolved frame (geometry, physical
    // style, configured fallback colour plus theme choice and the shared
    // accent snapshot, fresh target
    // lifetime identity, foreground identity, and monitor fulls) and return.
    // The target is the live foreground here, so the foreground proof
    // clones the target lifetime: any later switch refuses renderer-side.
    // The renderer paints the posted snapshot and owns the fullscreen
    // predicate, so neither queries nor the predicate runs owner-side per
    // tick. The applied outcome logs on the
    // renderer acknowledgement in `poll_render_acks`.
    if state.render_bridge.is_some() {
        // Unmanaged foregrounds carry no member tag by construction (tags
        // stamp only on admission): `None` here is honest untagged
        // evidence, fenced pid/creation-only renderer-side with the
        // weakness reported in the ack. The observe-to-tag read is an
        // existing native race (close/reuse between reads); pid/creation
        // plus the next-tick owner re-fence close it. No foreign property
        // is read for this.
        let lifetime = WindowLifetime {
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
            member_tag: crate::product_hide::sys::read_member_tag(window.hwnd),
        };
        post_border_frame(
            state,
            outer,
            (width_px, gap_px, radius_px),
            state.border.style.color,
            state.border.style.use_theme,
            accent,
            &window.token,
            window.hwnd,
            lifetime.clone(),
            lifetime,
            fulls.to_vec(),
            topmost,
        );
        return;
    }
    let style = state.border.style;
    let outcome = state.border_overlay.show_at(
        outer,
        width_px,
        gap_px,
        radius_px,
        &style,
        &window.token,
        topmost,
        window.hwnd,
        accent,
    );
    match outcome {
        OverlayOutcome::Hidden => {
            // Bounded failure line with the concrete error; change-only
            // unless tracing. State was already hidden by the overlay.
            // NOTE: the shown signature must NOT be installed first: doing
            // so flips border_last between shown and failure every tick and
            // re-logs the same failure per tick. Compare/set only failure.
            let failure = format!(
                "hidden:present-failed:{}",
                state.border_overlay.last_error().unwrap_or("unknown")
            );
            let changed_failure = state.border_last.as_deref() != Some(failure.as_str());
            if changed_failure {
                state.border_last = Some(failure);
            }
            // Diagnostic key (no product effect): a later show under the same
            // target must log its lifecycle, never aggregate against a stale
            // show key.
            note_render_hidden(
                state,
                RenderCarrier::Border,
                format!(
                    "failure:{}",
                    state.border_overlay.last_error().unwrap_or("unknown")
                ),
            );
            if changed_failure || state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": state.tick,
                        "outcome": "hidden",
                        "reason": "present-failed",
                        "last_error": state.border_overlay.last_error(),
                    }),
                );
            }
        }
        OverlayOutcome::Unchanged => {
            // Resolved accent-or-fallback colour rides the signature so a
            // theme change that repaints the same geometry still logs once;
            // the accent source rides it too so a source flip logs even
            // when both paints share one RGB. An unchanged surface with a
            // changed signature is exactly that flip: log it in
            // production (normal trace-silent Unchanged stays silent).
            let color_hex = state
                .border_overlay
                .current_color()
                .map(crate::active_border::render_color)
                .unwrap_or_else(|| crate::active_border::render_color(state.border.style.color));
            let sourced =
                crate::active_border::accent_source_label(state.border_overlay.from_accent());
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                width_px,
                gap_px,
                topmost,
                color_hex,
                sourced
            );
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if changed {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": state.tick,
                        "outcome": "unchanged",
                        "target": window.token,
                        "color": color_hex,
                        "source": sourced,
                    }),
                );
            }
        }
        OverlayOutcome::Moved if state.trace => {
            // Position-only follows are trace-only; production stays quiet
            // off the lifecycle transitions below. Routine follows under the
            // same target aggregate into the per-carrier ~1s window instead
            // of logging per move.
            let sample = same_thread_sample(
                "moved",
                window.token.clone(),
                [outer.x, outer.y, outer.w, outer.h],
                String::new(),
                String::new(),
            );
            if render_same_thread_gate(
                state,
                RenderCarrier::Border,
                window.token.as_str(),
                Instant::now(),
                true,
                sample,
            ) {
                return;
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": state.tick,
                    "outcome": "moved",
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "style_px": [width_px, gap_px, radius_px],
                }),
            );
        }
        OverlayOutcome::Moved => {}
        _ => {
            let color_hex = state
                .border_overlay
                .current_color()
                .map(crate::active_border::render_color)
                .unwrap_or_else(|| crate::active_border::render_color(state.border.style.color));
            let sourced =
                crate::active_border::accent_source_label(state.border_overlay.from_accent());
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                width_px,
                gap_px,
                topmost,
                color_hex,
                sourced
            );
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            // Routine same-target repaints aggregate into the per-carrier ~1s
            // window (shared decision: no duplicate line); transitions log
            // immediately below.
            let checksum_hex = state.border_overlay.snapshot()["dib_checksum"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if render_same_thread_gate(
                state,
                RenderCarrier::Border,
                window.token.as_str(),
                Instant::now(),
                changed || state.trace,
                same_thread_sample(
                    match outcome {
                        OverlayOutcome::Shown => "shown",
                        OverlayOutcome::Redrew => "redrew",
                        _ => "shown",
                    },
                    window.token.clone(),
                    [outer.x, outer.y, outer.w, outer.h],
                    color_hex.clone(),
                    checksum_hex,
                ),
            ) {
                return;
            }
            if !(changed || state.trace) {
                return;
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": state.tick,
                    "outcome": match outcome {
                        OverlayOutcome::Shown => "shown",
                        OverlayOutcome::Redrew => "redrew",
                        _ => "shown",
                    },
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "style_px": [width_px, gap_px, radius_px],
                    "color": color_hex,
                    "source": sourced,
                    "dib_checksum": state.border_overlay.snapshot()["dib_checksum"],
                }),
            );
        }
    }
}

#[cfg(test)]
mod anchored_outer_tests {
    use super::anchored_outer_rect;
    use tiler_core::directional::WindowId;
    use tiler_core::geometry::Rect;
    use tiler_core::world_report::{BorderPresentationV4, FrameWitness};

    const OBS: u64 = 7;

    fn live_frame() -> Rect {
        Rect {
            x: 200,
            y: 100,
            w: 40,
            h: 90,
        }
    }

    fn witness() -> FrameWitness {
        FrameWitness {
            win: WindowId("win-a".to_owned()),
            obs: OBS,
            frame: Rect {
                x: 100,
                y: 50,
                w: 40,
                h: 90,
            },
            foreground: WindowId("win-a".to_owned()),
        }
    }

    fn visible_border() -> BorderPresentationV4 {
        BorderPresentationV4 {
            visible: true,
            reason: None,
            subject: Some(WindowId("win-a".to_owned())),
            outsets: Some([5, 5, 5, 5]),
            width: Some(2),
            radius: Some(4),
            witness: Some(witness()),
        }
    }

    #[test]
    fn live_frame_expands_by_uniform_outsets_and_carries_core_style() {
        let (outer, width, gap, radius) = anchored_outer_rect(
            "win-a",
            &visible_border(),
            "win-a",
            live_frame(),
            OBS,
        )
        .expect("joined visible row yields the anchored outer rect");
        assert_eq!(
            outer,
            Rect {
                x: 195,
                y: 95,
                w: 50,
                h: 100,
            }
        );
        assert_eq!((width, gap, radius), (2, 3, 4));
    }

    #[test]
    fn non_uniform_outsets_veto_geometry() {
        let mut border = visible_border();
        border.outsets = Some([5, 6, 5, 5]);
        assert!(
            anchored_outer_rect("win-a", &border, "win-a", live_frame(), OBS).is_none()
        );
    }

    #[test]
    fn missing_witness_or_outsets_veto_geometry() {
        let mut no_witness = visible_border();
        no_witness.witness = None;
        assert!(
            anchored_outer_rect("win-a", &no_witness, "win-a", live_frame(), OBS).is_none()
        );
        let mut no_outsets = visible_border();
        no_outsets.outsets = None;
        assert!(
            anchored_outer_rect("win-a", &no_outsets, "win-a", live_frame(), OBS).is_none()
        );
    }

    #[test]
    fn hidden_row_or_other_foreground_vetoes_geometry() {
        let mut hidden = visible_border();
        hidden.visible = false;
        assert!(
            anchored_outer_rect("win-a", &hidden, "win-a", live_frame(), OBS).is_none()
        );
        assert!(
            anchored_outer_rect("win-b", &visible_border(), "win-a", live_frame(), OBS)
                .is_none()
        );
    }

    #[test]
    fn row_legs_must_all_join_the_live_target() {
        // Border subject missing or naming another window: the row is not
        // about this target, even with a plausible witness frame.
        let mut no_subject = visible_border();
        no_subject.subject = None;
        assert!(
            anchored_outer_rect("win-a", &no_subject, "win-a", live_frame(), OBS).is_none()
        );
        let mut other_subject = visible_border();
        other_subject.subject = Some(WindowId("win-b".to_owned()));
        assert!(
            anchored_outer_rect("win-a", &other_subject, "win-a", live_frame(), OBS).is_none()
        );
        // Witness subject leg (`win`) and observed-foreground leg must join
        // the same live target as the presentation foreground token.
        let mut cross_win = visible_border();
        cross_win.witness = Some(FrameWitness {
            win: WindowId("win-b".to_owned()),
            ..witness()
        });
        assert!(
            anchored_outer_rect("win-a", &cross_win, "win-a", live_frame(), OBS).is_none()
        );
        let mut cross_foreground = visible_border();
        cross_foreground.witness = Some(FrameWitness {
            foreground: WindowId("win-b".to_owned()),
            ..witness()
        });
        assert!(
            anchored_outer_rect("win-a", &cross_foreground, "win-a", live_frame(), OBS)
                .is_none()
        );
    }

    #[test]
    fn witness_obs_mismatch_vetoes_geometry() {
        let mut border = visible_border();
        border.witness = Some(FrameWitness {
            obs: OBS + 1,
            ..witness()
        });
        assert!(
            anchored_outer_rect("win-a", &border, "win-a", live_frame(), OBS).is_none()
        );
    }

    #[test]
    fn stale_witness_frame_does_not_move_the_outer_rect() {
        let mut border = visible_border();
        border.witness = Some(FrameWitness {
            frame: Rect {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            },
            ..witness()
        });
        let (outer, width, gap, radius) = anchored_outer_rect(
            "win-a",
            &border,
            "win-a",
            live_frame(),
            OBS,
        )
        .expect("a stale witness frame never vetoes a joined live frame");
        assert_eq!(
            outer,
            Rect {
                x: 195,
                y: 95,
                w: 50,
                h: 100,
            }
        );
        assert_eq!((width, gap, radius), (2, 3, 4));
    }

    #[test]
    fn core_width_and_radius_drive_the_returned_style() {
        let mut border = visible_border();
        border.width = Some(7);
        border.radius = Some(0);
        border.outsets = Some([10, 10, 10, 10]);
        let (outer, width, gap, radius) = anchored_outer_rect(
            "win-a",
            &border,
            "win-a",
            live_frame(),
            OBS,
        )
        .expect("core style and outsets yield a joined anchored rect");
        assert_eq!(
            outer,
            Rect {
                x: 190,
                y: 90,
                w: 60,
                h: 110,
            }
        );
        assert_eq!((width, gap, radius), (7, 3, 0));
    }

    #[test]
    fn missing_invalid_or_too_wide_core_style_vetoes_geometry() {
        let mut zero_width = visible_border();
        zero_width.width = Some(0);
        assert!(
            anchored_outer_rect("win-a", &zero_width, "win-a", live_frame(), OBS).is_none()
        );
        let mut missing_width = visible_border();
        missing_width.width = None;
        assert!(
            anchored_outer_rect("win-a", &missing_width, "win-a", live_frame(), OBS).is_none()
        );
        let mut missing_radius = visible_border();
        missing_radius.radius = None;
        assert!(
            anchored_outer_rect("win-a", &missing_radius, "win-a", live_frame(), OBS).is_none()
        );
        let mut negative_radius = visible_border();
        negative_radius.radius = Some(-1);
        assert!(
            anchored_outer_rect("win-a", &negative_radius, "win-a", live_frame(), OBS).is_none()
        );
        let mut too_wide = visible_border();
        too_wide.outsets = Some([1, 1, 1, 1]);
        assert!(
            anchored_outer_rect("win-a", &too_wide, "win-a", live_frame(), OBS).is_none()
        );
    }

    #[test]
    fn overflow_vetoes_geometry_instead_of_wrapping() {
        let mut border = visible_border();
        border.outsets = Some([i32::MAX, i32::MAX, i32::MAX, i32::MAX]);
        assert!(
            anchored_outer_rect("win-a", &border, "win-a", live_frame(), OBS).is_none()
        );
    }
}
