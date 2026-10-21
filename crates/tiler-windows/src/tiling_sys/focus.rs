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

/// Managed origins for chord resolution: eligible observed windows plus
/// verified overlay-retained members (maximized/fullscreen). An overlay
/// member is retained, never eligible, so eligible-only origins break Win+M
/// restore, Win+F11 exit, focus while overlaid, and send from a maximized
/// foreground. Only existing members with full identity plus live
/// lifetime-tag match ride along: no foreign or invented members. Exactness
/// stays with the per-intent owner recheck.
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
        let Some(stored) = state.member_identity.get(&row.key) else {
            continue;
        };
        if stored.pid != row.key.pid
            || stored.process_creation != row.key.creation
            || stored.user_sid != me.user_sid
            || stored.session_id != me.session_id
        {
            continue;
        }
        let live_tag = crate::product_hide::sys::read_member_tag(row.key.hwnd);
        let Some(tag) = state.member_tags.get(&row.key) else {
            continue;
        };
        if !crate::native_guards::visible_lifetime_ok(tag, live_tag.as_deref()) {
            continue;
        }
        if !scope_allows(&state.scope, &stored.exe_path) {
            continue;
        }
        if !hosted_gate_allows(
            &stored.exe_path,
            row.key.hwnd,
            stored.pid,
            &state.scope_hosts,
        ) {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let Some(entry) = entries.iter().find(|e| e.hwnd == row.key.hwnd) else {
                continue;
            };
            if verify_proof_owned(row.key.hwnd, entry, me).is_err() {
                continue;
            }
        }
        out.push(SnapOrigin {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            pid: row.key.pid,
            creation: row.key.creation.clone(),
        });
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

/// Fast active-border refresh for one loop wake: reads only the foreground
/// window (fresh identity plus one fresh frame), without full window observation,
/// so gesture following does not wait for reconciliation or the 100ms poll.
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
    if border_eligible(
        true,
        true,
        window.visible,
        window.facts.minimized,
        window.facts.captionless_fullscreen,
        window.facts.maximized,
        window.facts.cloaked,
        hidden_workspace,
        border_shell,
    )
    .is_err()
    {
        hide_border(
            state,
            if window.facts.minimized {
                "minimized"
            } else if window.facts.captionless_fullscreen {
                "fullscreen"
            } else if window.facts.maximized {
                "maximized"
            } else if window.facts.cloaked {
                "cloaked"
            } else if hidden_workspace {
                "hidden-workspace"
            } else if border_shell {
                "shell"
            } else {
                "no-target"
            },
        );
        return;
    }
    // Mirror the target's topmost band so a topmost active window stays
    // bordered; ordinary targets demote a previously topmost ring. Read-only
    // style query, never a foreign write.
    let topmost = crate::ffi::window_exstyle(foreground) & WS_EX_TOPMOST != 0;
    let dpi = crate::ffi::dpi_for_window(foreground);
    let Some((width_px, gap_px, radius_px)) = scale_style(&state.border.style, dpi) else {
        hide_border(state, "dpi-scale");
        return;
    };
    if width_px == 0 {
        hide_border(state, "zero-width");
        return;
    }
    let Some(outer) = border_outer_rect(window.visible, gap_px, width_px) else {
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
