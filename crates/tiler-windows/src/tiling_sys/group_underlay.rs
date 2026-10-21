#![forbid(unsafe_code)]
use super::*;

/// Read-only resolved active group for the underlay fill: the correlated
/// world result projected over a fresh observation. Field names mirror the
/// retired Engine plan so the render below is untouched.
struct ResolvedGroup {
    base_revision: u64,
    group: tiler_core::directional::NodeId,
    focused_leaf: tiler_core::directional::NodeId,
    members: Vec<tiler_core::active_group::ActiveGroupMember>,
    bounds: Rect,
}

/// Hide the underlay fill with a change-only `group-underlay` log line.
/// Bridged hides post through the handoff like the border; the ack logs.
pub(super) fn hide_underlay(state: &mut TileLoop, reason: &str) {
    if state.render_bridge.is_some() {
        hide_render_carrier(state, RenderCarrier::Underlay, reason);
        return;
    }
    let was_visible = state.underlay_overlay.is_visible();
    state.underlay_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.underlay_last.as_deref() == Some(signature.as_str()) && !was_visible {
        return;
    }
    state.underlay_last = Some(signature);
    // Diagnostic key first (no product effect): see `hide_border`.
    note_render_hidden(state, RenderCarrier::Underlay, format!("hidden:{reason}"));
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "group-underlay",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Hide the drop preview with change-only logging (no-op when already
/// hidden). Every cancel, refusal, zero, self/centre/outside, suspend,
/// teardown, and drift path funnels here so no stale rectangle survives.
/// Bridged hides post through the handoff; the ack logs. Settle-time finish
/// uses [`hide_preview_finish`] instead so the Engine drop runs only after
/// the renderer acknowledges the hide.
pub(super) fn hide_preview(state: &mut TileLoop, reason: &str) {
    if state.render_bridge.is_some() {
        hide_render_carrier(state, RenderCarrier::Preview, reason);
        return;
    }
    let was_visible = state.preview_overlay.is_visible();
    state.preview_overlay.hide();
    let signature = format!("hidden:{reason}");
    if state.preview_last.as_deref() == Some(signature.as_str()) && !was_visible {
        return;
    }
    state.preview_last = Some(signature);
    // Diagnostic key first (no product effect): see `hide_border`.
    note_render_hidden(state, RenderCarrier::Preview, format!("hidden:{reason}"));
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "drag-preview",
            "tick": state.tick,
            "outcome": "hidden",
            "reason": reason,
        }),
    );
}

/// Level-observed Win+Shift chord: either Win side plus any Shift side held.
/// Extras are allowed (only these keys are read) and either press order works
/// because this samples levels, never sequences edges. Independent of the
/// keyboard-hook gesture path, so a bare hold with no other key still reads.
pub(super) fn chord_held_now() -> (bool, bool) {
    let down = |vk: u32| crate::ffi::async_key_down(vk);
    aggregate_chord_keys(
        down(VK_LWIN),
        down(VK_RWIN),
        down(VK_SHIFT),
        down(VK_LSHIFT),
        down(VK_RSHIFT),
    )
}

/// Classify one open move/size gesture at START through the official
/// `WM_NCHITTEST` result at the current cursor position, returning the
/// classification plus the probe facts that produced it.
/// `EVENT_SYSTEM_MOVESIZESTART` does not distinguish move from resize;
/// asking the window itself (through `DefWindowProc`, system-marshalled
/// like `WM_GETMINMAXINFO`) is a bounded START-drain heuristic: no new
/// hook, no SC_MOVE/SC_SIZE interception, no deferred rect comparison
/// (which cannot show a stationary start). Hung or unreadable windows
/// classify `Unknown` and never trigger. Exactly one cursor read and at
/// most one hit-test send per call; callers log the probe instead of
/// re-sampling.
pub(super) fn classify_move_size_start(hwnd_u64: u64) -> crate::group_underlay::MoveSizeStart {
    let cursor = crate::ffi::cursor_pos();
    let hit = match cursor {
        Some((cx, cy)) => crate::ffi::hit_test_at(hwnd_u64, cx, cy),
        None => None,
    };
    crate::group_underlay::classify_move_size_probe(cursor, hit)
}

/// Fresh outer rectangle for one HWND via a single `GetWindowRect` read.
/// START-time fallback only, when `stable` has no pre-gesture entry. `None`
/// on failure: the caller treats the gesture as no-start.
pub(super) fn live_outer_rect(hwnd_u64: u64) -> Option<Rect> {
    let rect = crate::ffi::window_rect(hwnd_u64)?;
    let w = rect.right.saturating_sub(rect.left);
    let h = rect.bottom.saturating_sub(rect.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Rect {
        x: rect.left,
        y: rect.top,
        w,
        h,
    })
}

/// Fresh renderable check for one underlay anchor candidate: a live,
/// visible, non-iconic, non-cloaked top-level window. DWM cloak-query
/// failure fails closed (not renderable). Smallest causal gate so a stale
/// Engine member (minimized/hidden/cloaked between reconciles) never wins
/// the lowest-in-z anchor.
fn underlay_anchor_renderable(hwnd_u64: u64) -> bool {
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    if !crate::ffi::is_visible(hwnd_u64) {
        return false;
    }
    if crate::ffi::is_iconic(hwnd_u64) {
        return false;
    }
    crate::ffi::cloaked(hwnd_u64).is_some_and(|cloaked| !cloaked)
}

/// Map one Engine `no-group` reason to the closed underlay hide vocabulary.
/// `NoParentGroup` is the root-leaf focus the resolver reports; focus drift
/// between the fresh foreground read and the retained focus reports
/// `focus-changed` and may restore on the next valid refresh.
fn underlay_no_group_reason(reason: NoGroupReason) -> &'static str {
    match reason {
        NoGroupReason::Pending => "pending",
        NoGroupReason::Diverged => "diverged",
        NoGroupReason::NoParentGroup => "root-leaf",
        NoGroupReason::FocusMismatch | NoGroupReason::FocusUnmapped => "focus-changed",
        NoGroupReason::NoSession
        | NoGroupReason::DomainMismatch
        | NoGroupReason::StaleRevision
        | NoGroupReason::NoTree => "no-group",
    }
}

/// Fast group-underlay refresh for one loop wake: the movement-only staged
/// lifetime (A Win+Shift chord, B focused interactive move) over the
/// Engine-projected source-group union.
///
/// Trigger is the shared OR: level-observed Win+Shift (either side, extras
/// allowed, either order) or a classified `Move` gesture on the actual
/// foreground window. Resize alone never triggers; the chord stays
/// independent during resize. Move-start evidence never depends on modifier
/// observation.
///
/// Geometry is the authoritative Engine route only: `CoreCommand::ActiveGroup`
/// through `resolve_active_group`/`describe_active_group` with the fresh
/// actual focused token (never an unfocused dragged id, which would persist
/// false retained focus). Pending/drag residue fails closed in the resolver;
/// the union shown is always the projected source union, never live dragged
/// bounds. The fill lands directly beneath the lowest renderable member in
/// `EnumWindows` order (the border surface included when visible), i.e. below
/// members and below the active border.
pub(super) fn refresh_group_underlay(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    if !state.underlay.enabled {
        hide_underlay(state, "disabled");
        return;
    }
    let (win_held, shift_held) = chord_held_now();
    let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        hide_underlay(state, "no-target");
        return;
    }
    if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(foreground)
        || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(foreground)
    {
        return;
    }
    // Fresh actual focused-subject match: only a classified move on this exact
    // foreground window feeds the move arm. Stale START entries for other
    // windows never match, and END/removal clearing drops them.
    let move_active = state
        .move_kind
        .get(&foreground)
        .is_some_and(|kind| *kind == MoveSizeKind::Move);
    if !tiler_core::visual::group_underlay_trigger(chord, move_active) {
        hide_underlay(state, "idle");
        return;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == foreground);
        let Some(entry) = entry else {
            hide_underlay(state, "allowlist-changed");
            return;
        };
        if verify_proof_owned(foreground, entry, me).is_err() {
            hide_underlay(state, "identity-changed");
            return;
        }
    }
    let hwnd = foreground as isize as HWND;
    let window = match observe_window(hwnd, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(ObserveFailure::Known(_, reason)) => {
            hide_underlay(
                state,
                match reason {
                    SkipReason::Minimized => "minimized",
                    SkipReason::Maximized => "maximized",
                    SkipReason::Fullscreen => "fullscreen",
                    _ => "no-target",
                },
            );
            return;
        }
        Err(ObserveFailure::Unknown) => {
            hide_underlay(state, "no-target");
            return;
        }
    };
    if !scope_allows(&state.scope, &window.identity.exe_path)
        || !hosted_gate_allows(
            &window.identity.exe_path,
            window.hwnd,
            window.identity.pid,
            &state.scope_hosts,
        )
    {
        hide_underlay(state, "scope-excluded");
        return;
    }
    if window.facts.minimized {
        hide_underlay(state, "minimized");
        return;
    }
    if window.facts.maximized {
        hide_underlay(state, "maximized");
        return;
    }
    if window.facts.captionless_fullscreen {
        hide_underlay(state, "fullscreen");
        return;
    }
    // Tiled membership only: a focused window with no Engine membership has
    // no group to highlight.
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == window.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        hide_underlay(state, "floating");
        return;
    };
    let Some(loc) = state.member_home(&member_key) else {
        hide_underlay(state, "no-group");
        return;
    };
    // Floats hold no tile group: the fill stays hidden on a focused float
    // while the active border still marks it. Floating workspaces hold no
    // tiled group either: every member frame stays native.
    if engine_is_float(state, &loc.output, &loc.workspace, &window.token)
        || !workspace_mode_tiled(state, &loc.output, &loc.workspace)
    {
        hide_underlay(state, "floating");
        return;
    }
    let Some(domain_bounds) = areas
        .iter()
        .find(|a| a.device == loc.output)
        .and_then(|area| tiling_domain_bounds_with(area.work, state.outer_gap))
    else {
        hide_underlay(state, "no-group");
        return;
    };
    // Copies for the diagnostics payload: bounds plus gap ride the
    // geometry report below.
    let domain_gap = state.inner_gap;
    // World-routed active-group resolution: the core owns the highlight
    // projection outright over a fresh observation, and the correlated
    // result carries everything the renderer needs. Read-only: never
    // commits, never writes geometry.
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        hide_underlay(state, "observe-failed");
        return;
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::ActiveGroup),
    );
    let Some(result) = reply
        .as_ref()
        .and_then(|reply| reply.command_result.as_ref())
    else {
        hide_underlay(state, "no-group");
        return;
    };
    let found = match result {
        tiler_core::world_report::CommandResult::ActiveGroup {
            base_revision,
            group,
            focused_leaf,
            focused_window: _,
            members,
            bounds,
        } => (base_revision, group, focused_leaf, members, bounds),
        tiler_core::world_report::CommandResult::NoGroup { reason, .. } => {
            hide_underlay(state, underlay_no_group_reason(*reason));
            return;
        }
        _ => {
            hide_underlay(state, "no-group");
            return;
        }
    };
    let (base_revision, group, focused_leaf, members, bounds) = found;
    let found = ResolvedGroup {
        base_revision: *base_revision,
        group: group.clone(),
        focused_leaf: focused_leaf.clone(),
        members: members.clone(),
        bounds: *bounds,
    };
    let found = &found;
    if underlay_eligible(
        true,
        true,
        true,
        window.facts.maximized,
        window.facts.captionless_fullscreen,
        false,
    )
    .is_err()
    {
        hide_underlay(state, "no-group");
        return;
    }
    let dpi = crate::ffi::dpi_for_window(foreground);
    let Some((width_px, gap_px, _)) = scale_style(&state.border.style, dpi) else {
        hide_underlay(state, "dpi-scale");
        return;
    };
    let extension_logical = tiler_core::visual::group_underlay_effective_extension(
        state.underlay.style.extension,
        state.border.style.width,
    );
    let Some(extension_px) = scale_to_physical(extension_logical, dpi) else {
        hide_underlay(state, "dpi-scale");
        return;
    };
    let Some(outer) = underlay_outer_rect(found.bounds, gap_px, width_px, extension_px) else {
        hide_underlay(state, "geometry");
        return;
    };
    // Anchor beneath the lowest renderable member; the renderer's own border
    // surface sorts into the same comparison renderer-side so the fill stays
    // below it too. Member candidates stay identity-valid (still managed)
    // and freshly renderable (visible, non-iconic, non-cloaked), each
    // carrying its own lifetime identity for individual renderer-side
    // validation; stale Engine members between reconciles fail closed to
    // `anchor-missing`. The owner never pushes its possibly stale border
    // HWND through the handoff.
    let candidates: Vec<UnderlayCandidate> = found
        .members
        .iter()
        .filter_map(|member| {
            state
                .snap_origins
                .iter()
                .find(|(_, origin)| origin.token == member.window.0)
                .map(|(hwnd, _)| *hwnd)
        })
        .filter(|hwnd| state.managed.contains(hwnd) && underlay_anchor_renderable(*hwnd))
        .filter_map(|hwnd| {
            let key = state.member_tokens.keys().find(|key| key.hwnd == hwnd)?;
            Some(UnderlayCandidate {
                hwnd,
                lifetime: WindowLifetime {
                    pid: key.pid,
                    creation: key.creation.clone(),
                    member_tag: crate::product_hide::sys::read_member_tag(hwnd),
                },
            })
        })
        .collect();
    let z_hwnds: Vec<u64> = candidates.iter().map(|candidate| candidate.hwnd).collect();
    let Some(anchor) = lowest_in_z(&z_hwnds) else {
        hide_underlay(state, "anchor-missing");
        return;
    };
    let Some(anchor_lifetime) = candidates
        .iter()
        .find(|candidate| candidate.hwnd == anchor)
        .map(|candidate| candidate.lifetime.clone())
    else {
        hide_underlay(state, "anchor-missing");
        return;
    };
    let topmost = crate::ffi::window_exstyle(anchor) & WS_EX_TOPMOST != 0;
    let color = (
        state.underlay.style.alpha,
        state.underlay.style.color.0,
        state.underlay.style.color.1,
        state.underlay.style.color.2,
    );
    let color_hex = crate::group_underlay::render_color_argb((
        state.underlay.style.color,
        state.underlay.style.alpha,
    ));
    // Bounded geometry observability only: the Engine union, immediate-parent
    // group/leaf, member rects, DPI/pad inputs, and domain/revision ride the
    // already-logged shown/redrew/moved lines. No new lines, no geometry or
    // placement change; the change-gated signature below is untouched.
    // Hoisted above the present so the bridged post can stash the same
    // payload for its acknowledgement-driven log line.
    let member_windows: Vec<String> = found
        .members
        .iter()
        .map(|member| member.window.0.clone())
        .collect();
    let member_rects: Vec<Rect> = found.members.iter().map(|member| member.rect).collect();
    let geometry_debug = underlay_geometry_debug(crate::group_underlay::UnderlayGeometryReport {
        union_rect: found.bounds,
        group: found.group.0.as_str(),
        focused_leaf: found.focused_leaf.0.as_str(),
        member_windows: &member_windows,
        member_rects: &member_rects,
        dpi,
        gap_px,
        width_px,
        extension_px,
        domain_bounds,
        domain_gap,
        base_revision: found.base_revision,
    });
    // Bridged present: post the owner-resolved union frame with the live
    // member candidates the renderer revalidates individually and
    // re-resolves to a fresh lowest-in-z anchor, plus the foreground
    // identity and monitor fulls for the renderer-side switch/fullscreen
    // fence. The applied outcome logs on the acknowledgement.
    if state.render_bridge.is_some() {
        let foreground = WindowLifetime {
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
            member_tag: crate::product_hide::sys::read_member_tag(window.hwnd),
        };
        post_underlay_frame(
            state,
            outer,
            color,
            &window.token,
            topmost,
            anchor,
            anchor_lifetime,
            candidates,
            foreground,
            monitor_fulls(areas),
            geometry_debug,
        );
        return;
    }
    let outcome = state
        .underlay_overlay
        .show_fill_at(outer, color, &window.token, topmost, anchor);
    match outcome {
        OverlayOutcome::Hidden => {
            let failure = format!(
                "hidden:present-failed:{}",
                state.underlay_overlay.last_error().unwrap_or("unknown")
            );
            let changed_failure = state.underlay_last.as_deref() != Some(failure.as_str());
            if changed_failure {
                state.underlay_last = Some(failure);
            }
            // Diagnostic key (no product effect): see the border branch above.
            note_render_hidden(
                state,
                RenderCarrier::Underlay,
                format!(
                    "failure:{}",
                    state.underlay_overlay.last_error().unwrap_or("unknown")
                ),
            );
            if changed_failure || state.trace {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": state.tick,
                        "outcome": "hidden",
                        "reason": "present-failed",
                        "last_error": state.underlay_overlay.last_error(),
                    }),
                );
            }
        }
        OverlayOutcome::Unchanged => {
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                topmost,
                color_hex,
                state.underlay_overlay.snapshot()["dib_checksum"],
            );
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if state.trace && changed {
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": state.tick,
                        "outcome": "unchanged",
                        "target": window.token,
                    }),
                );
            }
        }
        OverlayOutcome::Moved if state.trace => {
            // Routine follows under the same target aggregate into the
            // per-carrier ~1s window instead of logging per move.
            let sample = same_thread_sample(
                "moved",
                window.token.clone(),
                [outer.x, outer.y, outer.w, outer.h],
                color_hex.clone(),
                String::new(),
            );
            if render_same_thread_gate(
                state,
                RenderCarrier::Underlay,
                window.token.as_str(),
                Instant::now(),
                true,
                sample,
            ) {
                return;
            }
            let mut event = serde_json::json!({
                    "event": "group-underlay",
                    "tick": state.tick,
                    "outcome": "moved",
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
            });
            if let Some(fields) = geometry_debug.as_object() {
                for (key, value) in fields {
                    event[key] = value.clone();
                }
            }
            log_json_at(&state.log_path, event);
        }
        OverlayOutcome::Moved => {}
        _ => {
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}",
                window.token,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                topmost,
                color_hex,
                state.underlay_overlay.snapshot()["dib_checksum"],
            );
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            // Routine same-target repaints aggregate into the per-carrier ~1s
            // window (shared decision: no duplicate line); transitions log
            // immediately below.
            let checksum_hex = state.underlay_overlay.snapshot()["dib_checksum"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if render_same_thread_gate(
                state,
                RenderCarrier::Underlay,
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
            let mut event = serde_json::json!({
                    "event": "group-underlay",
                    "tick": state.tick,
                    "outcome": match outcome {
                        OverlayOutcome::Shown => "shown",
                        OverlayOutcome::Redrew => "redrew",
                        _ => "shown",
                    },
                    "target": window.token,
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "color": color_hex,
                    "members": found.members.len(),
                    "dib_checksum": state.underlay_overlay.snapshot()["dib_checksum"],
            });
            if let Some(fields) = geometry_debug.as_object() {
                for (key, value) in fields {
                    event[key] = value.clone();
                }
            }
            log_json_at(&state.log_path, event);
        }
    }
}
