use super::*;

use super::render_ack::poll_render_carrier;

/// Owner-side dedicated-render bridge (product path only). The renderer
/// thread creates, pumps, presents and destroys all three overlay carriers;
/// the owner posts versioned desired frames plus safety epochs and consumes
/// acknowledged applied state. Proof paths keep `None` and stay same-thread.
/// All owner control calls route through the bridge so nothing touches
/// renderer state from this thread; renderer windows are data (HWND `u64`)
/// here, never dereferenced.
pub(super) struct RenderBridge {
    /// Shared handoff channels (desired slots, safety epochs, acks, IDs).
    pub(super) channels: Arc<RenderChannels>,
    /// Renderer thread handle (lifecycle + bounded join). `None` only in
    /// `#[cfg(test)]` offline fixtures, which post through channels without
    /// a live renderer thread; production always holds `Some`.
    pub(super) handle: Option<RenderHandle>,
    /// Next per-carrier generation (monotonic; pairs frames with acks).
    pub(super) gens: [u64; 3],
    /// Hide already posted and unanswered per carrier (transition-only posts;
    /// quiet ticks send nothing).
    pub(super) hide_posted: [bool; 3],
    /// Reason stashed for the pending hide acknowledgement log line.
    pub(super) hide_reason: [Option<String>; 3],
    /// Verified hidden pairs per carrier: the last `(generation, safety)`
    /// acknowledged with a `Hidden` outcome. Lets the finish wait succeed
    /// immediately when its hide ack was already consumed by the ack poll,
    /// instead of timing out on an already-proven hide.
    pub(super) hidden_ack: [Option<(u64, u64)>; 3],
    /// Last acknowledged applied note per carrier (the posted context paired
    /// at ack time: diagnostics payloads, sample correlation, hover prior).
    /// Content truth (geometry/colour/target) comes from the ack applied
    /// payload itself, never from this pairing.
    pub(super) applied_note: [Option<PostedNote>; 3],
    /// Last acknowledged applied ack per carrier (generation, outcome,
    /// checksum). Updated on every ack even when the note pairing aged out,
    /// so visibility and the stop snapshot stay truthful regardless.
    pub(super) applied_ack: [Option<CarrierAck>; 3],
    /// Whether the current applied mirror is a known state. A proven
    /// `Hidden`, a `Refused`, or a show-family ack is known; an unverified
    /// `Hidden` or any `Unknown` outcome is unknown and must never report a
    /// known visibility, satisfy a hide wait, or arm an actuation.
    pub(super) applied_known: [bool; 3],
    /// Admission open: closed at graceful shutdown before the input stop
    /// wait so no stale show can enter while the input thread winds down.
    pub(super) admission_open: bool,
    /// Recently posted log notes per carrier by generation (bounded history;
    /// pairs arriving acks with the geometry/signature context the ack
    /// itself does not carry).
    pub(super) posted: [VecDeque<(u64, PostedNote)>; 3],
    /// Last acknowledged visibility per carrier (from acks, never desired).
    /// Lets owner logic that reasons about a shown carrier (underlay anchor
    /// candidates, cancel fallbacks) read applied state with the bridge.
    pub(super) applied_visible: [bool; 3],
    /// Bounded pending gesture drop (bridged finish only): the preview hide
    /// is posted with its safety epoch, and the Engine drop waits for the
    /// proven `Hidden` acknowledgement, serviced from render acks at loop
    /// level. Re-entered with fresh observations and gates before every
    /// focus/Engine/foreign write; refused narrowly on timeout with no
    /// drop and no reconcile. Single slot: one gesture settles at a time.
    pub(super) pending: Option<PendingDrop>,
}

/// Log extras stashed at post time for one desired frame: diagnostics
/// payloads, sample correlation, and hover-prior descriptor. Content truth
/// (geometry/colour/target) rides the ack applied payload, never this
/// pairing, so a missing pairing logs extras-free lines or nothing rather
/// than an older payload as new. Bounded and numeric/opaque-only.
#[derive(Debug, Clone)]
pub(super) struct PostedNote {
    pub(super) debug: Option<serde_json::Value>,
    pub(super) correlation: String,
    pub(super) prior: String,
}

/// Acknowledged carrier visibility: the last acked applied state with the
/// bridge, else the same-thread carrier state. Owner logic that reasons
/// about a shown carrier reads this, never desired state.
pub(super) fn render_carrier_visible(state: &TileLoop, carrier: RenderCarrier) -> bool {
    if let Some(bridge) = state.render_bridge.as_ref() {
        return bridge.applied_visible[carrier.index()];
    }
    match carrier {
        RenderCarrier::Border => state.border_overlay.is_visible(),
        RenderCarrier::Underlay => state.underlay_overlay.is_visible(),
        RenderCarrier::Preview => state.preview_overlay.is_visible(),
    }
}

/// Owned overlay window for exclusion checks: the renderer-published
/// immutable identity when the bridge is active, else the same-thread
/// carrier window. Owner observation and WinEvents exclude exactly these, so
/// our own move/paint events can never retarget tiling or the overlays.
pub(super) fn owned_overlay_hwnd(state: &TileLoop, carrier: RenderCarrier) -> Option<u64> {
    if let Some(bridge) = state.render_bridge.as_ref() {
        return bridge.channels.overlay_ids()[carrier.index()];
    }
    match carrier {
        RenderCarrier::Border => state.border_overlay.hwnd(),
        RenderCarrier::Underlay => state.underlay_overlay.hwnd(),
        RenderCarrier::Preview => state.preview_overlay.hwnd(),
    }
}

/// Next per-carrier generation for one post or hide.
fn render_next_gen(bridge: &mut RenderBridge, carrier: RenderCarrier) -> u64 {
    bridge.gens[carrier.index()] += 1;
    bridge.gens[carrier.index()]
}

/// Remember one posted note, keeping a bounded per-carrier history.
fn render_remember(
    bridge: &mut RenderBridge,
    carrier: RenderCarrier,
    generation: u64,
    note: PostedNote,
) {
    let history = &mut bridge.posted[carrier.index()];
    history.push_back((generation, note));
    while history.len() > 8 {
        history.pop_front();
    }
}

/// Signal the renderer work wake after an owner post/hide/stop so the
/// renderer pump breaks its quantum early instead of idling the full 10ms.
/// Best-effort: a missing wake keeps the periodic fallback.
fn signal_render_work(state: &TileLoop) {
    if let Some(bridge) = state.render_bridge.as_ref()
        && let Some(handle) = bridge.handle.as_ref()
    {
        handle.notify_work();
    }
}

/// Post one border frame every owner refresh (latest-wins coalesces bursts
/// on the renderer, so reposting unchanged content costs one slot, never a
/// backlog). Unchanged reposts are required, not redundant: the renderer
/// re-probes the live surface on every present (visibility, rect, z against
/// the fresh target) and reasserts follow/z drift plus fresh suppression,
/// exactly like the same-thread cached-equality probe. A refused or
/// present-failed frame therefore retries on the next refresh with no
/// separate backoff. Colour carries the configured fallback plus the theme
/// choice; the renderer owns the live accent query and resolves the painted
/// colour itself, so no owner-side accent FFI runs per tick.
#[allow(clippy::too_many_arguments)]
pub(super) fn post_border_frame(
    state: &mut TileLoop,
    outer: Rect,
    style_px: (i32, i32, i32),
    fallback: (u8, u8, u8),
    use_theme: bool,
    token: &str,
    target_hwnd: u64,
    lifetime: WindowLifetime,
    foreground: WindowLifetime,
    fulls: Vec<Rect>,
    topmost: bool,
) {
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    if !bridge.admission_open {
        return;
    };
    let safety = bridge.channels.safety();
    let generation = render_next_gen(bridge, RenderCarrier::Border);
    let frame = BorderDesired {
        generation,
        safety,
        outer,
        style_px,
        color: fallback,
        use_theme,
        target_token: token.to_owned(),
        target_hwnd,
        lifetime,
        foreground,
        fulls,
        // No managed-fullscreen bypass rule exists for overlay display:
        // the suspend-write bypasses stay owner-side. Carried explicitly
        // (false) so the renderer never invents an exemption.
        fullscreen_bypass: false,
        topmost,
    };
    bridge.channels.post(DesiredFrame::Border(frame));
    bridge.hide_posted[RenderCarrier::Border.index()] = false;
    render_remember(
        bridge,
        RenderCarrier::Border,
        generation,
        PostedNote {
            debug: None,
            correlation: String::new(),
            prior: String::new(),
        },
    );
    signal_render_work(state);
}

/// Post one underlay frame every owner refresh (latest-wins, like the
/// border): unchanged reposts re-probe the live surface and re-resolve the
/// fresh lowest-in-z anchor, so follow/z drift and fresh suppression never
/// stall on an unchanged frame. `candidates` carry their own lifetime
/// identities for individual renderer-side validation; the owner-chosen
/// anchor rides along as the unreadable-order fallback only. The renderer's
/// own border surface sorts into the anchor comparison renderer-side, so the
/// owner never pushes a possibly stale border HWND through the handoff.
#[allow(clippy::too_many_arguments)]
pub(super) fn post_underlay_frame(
    state: &mut TileLoop,
    outer: Rect,
    color: (u8, u8, u8, u8),
    token: &str,
    topmost: bool,
    anchor: u64,
    anchor_lifetime: WindowLifetime,
    candidates: Vec<UnderlayCandidate>,
    foreground: WindowLifetime,
    fulls: Vec<Rect>,
    debug: serde_json::Value,
) {
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    if !bridge.admission_open {
        return;
    }
    let safety = bridge.channels.safety();
    let generation = render_next_gen(bridge, RenderCarrier::Underlay);
    let frame = UnderlayDesired {
        generation,
        safety,
        outer,
        color,
        anchor_token: token.to_owned(),
        anchor_hwnd: anchor,
        anchor_lifetime,
        candidates,
        foreground,
        fulls,
        topmost,
    };
    bridge.channels.post(DesiredFrame::Underlay(frame));
    bridge.hide_posted[RenderCarrier::Underlay.index()] = false;
    render_remember(
        bridge,
        RenderCarrier::Underlay,
        generation,
        PostedNote {
            debug: Some(debug),
            correlation: String::new(),
            prior: String::new(),
        },
    );
    signal_render_work(state);
}

/// Post one preview frame every owner refresh (latest-wins, like the
/// border): unchanged reposts re-probe the live surface, so a hidden,
/// misplaced, refused, or present-failed preview retries on the next sample
/// with no separate backoff. The mover lifetime identity fences the present;
/// the mover band is never carried (the preview plane is always topmost).
#[allow(clippy::too_many_arguments)]
pub(super) fn post_preview_frame(
    state: &mut TileLoop,
    outer: Rect,
    color: (u8, u8, u8, u8),
    token: &str,
    anchor_hwnd: u64,
    lifetime: WindowLifetime,
    foreground: Option<WindowLifetime>,
    correlation: &str,
    prior: &str,
) {
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    if !bridge.admission_open {
        return;
    }
    let safety = bridge.channels.safety();
    let generation = render_next_gen(bridge, RenderCarrier::Preview);
    let frame = PreviewDesired {
        generation,
        safety,
        outer,
        color,
        anchor_token: token.to_owned(),
        anchor_hwnd,
        lifetime,
        foreground,
    };
    bridge.channels.post(DesiredFrame::Preview(frame));
    bridge.hide_posted[RenderCarrier::Preview.index()] = false;
    render_remember(
        bridge,
        RenderCarrier::Preview,
        generation,
        PostedNote {
            debug: None,
            correlation: correlation.to_owned(),
            prior: prior.to_owned(),
        },
    );
    signal_render_work(state);
}

/// Hide one carrier through the bridge (transition-only): bump the safety
/// epoch, stamp the watermark and drop any queued Show so no late frame can
/// resurrect this carrier. The `Hidden` log line fires on the renderer
/// acknowledgement in [`poll_render_acks`], never here. Cancel, suspend,
/// finish and reset share this ordering with their reason logged there.
/// A repeated hide with the same reason stays idempotent (no new epoch);
/// a changed reason posts one new hide generation/epoch so the ack logs the
/// true transition instead of silently keeping the old reason. Visibility is
/// never forged here and the pending finish slot is untouched: only the
/// renderer ack proves hidden.
pub(super) fn hide_render_carrier(state: &mut TileLoop, carrier: RenderCarrier, reason: &str) {
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    if bridge.hide_posted[carrier.index()]
        && bridge.hide_reason[carrier.index()].as_deref() == Some(reason)
    {
        return;
    }
    // Reason transition (or first hide): fall through to post one new hide
    // epoch below. A repeated reason stays idempotent above.
    let generation = render_next_gen(bridge, carrier);
    bridge.channels.hide(carrier, generation);
    bridge.hide_posted[carrier.index()] = true;
    bridge.hide_reason[carrier.index()] = Some(reason.to_owned());
    signal_render_work(state);
}

/// Preview-finish hide for the same-thread path only (proof paths and the
/// degraded same-thread fallback): the bridged product path never blocks
/// here. It hides through [`hide_render_carrier`] and queues the drop as a
/// bounded pending owner operation serviced from render acks at loop level
/// (see `service_pending_drop`), so this function stays inline-hide only.
/// True after the inline hide; the caller refuses the drop narrowly on its
/// own timeout path via the pending service.
pub(super) fn hide_preview_finish(state: &mut TileLoop, reason: &str) -> bool {
    hide_preview(state, reason);
    true
}

/// Drain renderer lifecycle events for the owner writer (owner tick).
pub(super) fn drain_render_failures(state: &mut TileLoop) {
    let log_path = state.log_path.clone();
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    let Some(handle) = bridge.handle.as_mut() else {
        return;
    };
    handle.drain_failures(&mut |event: &RenderFailEvent| {
        log_json_at(
            &log_path,
            if event.recovered {
                serde_json::json!({
                    "event": "render-available",
                    "op": event.op,
                    "producer": "render-thread",
                })
            } else {
                let mut line = serde_json::json!({
                    "event": "render-unavailable",
                    "op": event.op,
                    "attempt": event.attempt,
                    "retry_secs": event.retry_secs,
                    "producer": "render-thread",
                });
                if let Some(detail) = event.detail.as_ref() {
                    line["cause"] = serde_json::Value::from(detail.clone());
                }
                line
            },
        );
    });
}

/// Record one acknowledgement into the applied mirror (bridge only). Runs on
/// every ack, even when the posted-note pairing aged out: visibility,
/// known-ness, and the last applied payload/checksum/generation stay
/// truthful regardless of log pairing. Only a proven `Hidden` records its
/// pair for the finish service; an unverified `Hidden` or any `Unknown`
/// outcome marks the carrier unknown without touching the last-known
/// visibility, so no log, snapshot, or actuation can mistake it for a known
/// state.
pub(super) fn render_record_applied(
    state: &mut TileLoop,
    carrier: RenderCarrier,
    ack: &crate::render_handoff::CarrierAck,
) {
    let Some(bridge) = state.render_bridge.as_mut() else {
        return;
    };
    let index = carrier.index();
    bridge.applied_ack[index] = Some(ack.clone());
    match &ack.outcome {
        RenderOutcome::Hidden if ack.hidden_proven => {
            bridge.applied_visible[index] = false;
            bridge.applied_known[index] = true;
            bridge.hidden_ack[index] = Some((ack.generation, ack.safety));
            bridge.applied_note[index] = None;
        }
        RenderOutcome::Hidden | RenderOutcome::Unknown(_) => {
            bridge.applied_known[index] = false;
        }
        RenderOutcome::Refused(_) => {
            bridge.applied_visible[index] = false;
            bridge.applied_known[index] = true;
            bridge.applied_note[index] = None;
        }
        _ => {
            bridge.applied_visible[index] = true;
            bridge.applied_known[index] = true;
            if let Some((_, note)) = bridge.posted[index]
                .iter()
                .find(|(posted_gen, _)| *posted_gen == ack.generation)
                .cloned()
            {
                bridge.applied_note[index] = Some(note);
            }
        }
    }
}

/// Hide reason for one ack: a present failure (`Hidden` with renderer error
/// detail) reports `present-failed` truthfully with the error, never the
/// hide-transition reason. An actual hide ack keeps the posted reason.
pub(super) fn render_hide_reason(
    state: &TileLoop,
    carrier: RenderCarrier,
    ack: &crate::render_handoff::CarrierAck,
) -> String {
    if ack.last_error.is_some() {
        return "present-failed".to_owned();
    }
    state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.hide_reason[carrier.index()].clone())
        .unwrap_or_else(|| "hidden".to_owned())
}

/// Poll renderer acknowledgements once per owner tick and log applied
/// outcomes from acknowledged state only: every line below consumes the ack
/// (generation, outcome, checksum, span, failure) plus the posted note for
/// that generation, never desired state as rendered. Change-only signatures
/// ride the existing `border_last`/`underlay_last`/`preview_last` so the
/// bridged vocabulary matches the same-thread vocabulary line for line.
/// Routine successful acks (Shown/Redrew/Unchanged with applied content, plus
/// trace-only Moved) under an unchanged transition key aggregate into the
/// per-carrier ~1s window; due summaries flush on this existing turn below.
/// Failures and visibility/target/safety/proof/reason transitions retain
/// their immediate paths.
pub(super) fn poll_render_acks(state: &mut TileLoop, now: Instant) {
    for carrier in RenderCarrier::all() {
        poll_render_carrier(state, carrier, now);
    }
    let log_path = state.log_path.clone();
    for line in state.render_frames.take_due(now) {
        log_json_at(&log_path, line);
    }
}
