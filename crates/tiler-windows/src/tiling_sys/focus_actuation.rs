#![forbid(unsafe_code)]
use super::*;

/// Exact write/focus expectation for one managed window: the eligible
/// observed row first, otherwise a retained overlay member (KDE `requestFocus`
/// overlay exemption: maximized and fullscreen members are retained, never
/// eligible). Retained rows rebuild from stored identity plus the fresh
/// overlay facts; the caller still revalidates fresh with the full
/// identity/scope/hosted/proof/lifetime gates before any native call. `None`
/// when the token is live nowhere.
pub(super) fn overlay_expected(
    state: &TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    to_token: &str,
) -> Option<ObservedWindow> {
    if let Some(found) = observed.iter().find(|w| w.token == to_token) {
        return Some(found.clone());
    }
    let row = retained.iter().find(|r| r.token == to_token)?;
    let stored = state.member_identity.get(&row.key).cloned()?;
    Some(ObservedWindow {
        hwnd: row.key.hwnd,
        token: row.token.clone(),
        outer: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        visible: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        insets: FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: row.key.hwnd,
            pid: row.key.pid,
            process_creation: row.key.creation.clone(),
            exe_path: stored.exe_path,
            user_sid: stored.user_sid,
            session_id: stored.session_id,
            tag: String::new(),
        },
        facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: true,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }),
        // Synthetic fallback row: no fresh membership read was taken, so it
        // carries no stamp and can never prove a receipt.
        member_tag: None,
    })
}

/// R-MOV-03 mode for the next keyboard move: the last-good live setting,
/// read per move so a saved change applies to subsequent moves only. Proof
/// owners carry no live settings (`None`) and keep the default wrap.
pub(super) fn same_axis_move_for(
    live: Option<&LiveSettings>,
) -> tiler_core::directional::SameAxisMove {
    live.map(|live| live.settings.core.same_axis_move_mode())
        .unwrap_or_default()
}

/// R-MAX-06 mode for the next first-seen maximized tiled admission: the
/// last-good live setting, read per tick so a saved change applies to
/// subsequent admissions only. Proof owners carry no live settings (`None`)
/// and keep the default tile-on-launch clear. Previously admitted
/// clients and Engine topology are untouched: the flag only gates the
/// one-shot clear below, never existing slots.
pub(super) fn born_maximized_keep_overlay_for(live: Option<&LiveSettings>) -> bool {
    live.is_some_and(|live| live.settings.core.born_maximized_keep_overlay())
}

/// Drain one bounded batch of product keyboard intents against fresh complete
/// observations through the retained Engine.
///
/// While `blocked` is set (fullscreen suspension, an open managed gesture, or
/// a just-ended gesture settling) every intent drops safely with one bounded
/// stale line and nothing dispatches. Otherwise each consumed down/repeat
/// carries its chord-time origin (opaque token plus HWND/process-lifetime
/// evidence bound by the callback) and must re-resolve against fresh
/// observation: a chord consumed for A never acts on unrelated B after an
/// external focus change. The only retarget is an explicitly verified
/// owner-caused continuation within the same batch (a prior spent effect
/// moved focus and the fresh foreground is exactly that target), so rapid
/// repeated navigation survives while external focus moves reject loudly.
/// Focus/move intents resolve through the world-owned directional commands
/// and the single spent common effect actuates inside the world apply; this
/// seam only verifies the foreground for its snap chain. Key-ups close the pair
/// and passed intents never dispatch: both are trace-only so held-key traffic
/// stays out of normal logs. Every settled action carries its correlation;
/// enumeration failure settles the action as failed. Log vocabulary is tokens
/// plus op/direction/edge/disposition/outcome/correlation only: no titles, raw
/// native ids, or keys.
/// Owner-side snap queue-loss source (production seam): the pump's bounded
/// diagnostics when the dedicated input thread owns the hook TLS (owner TLS
/// is never the input thread's there), direct hook TLS only on proof/legacy
/// paths. `diag` carries the latest published `snap_dropped` when bridged.
pub(crate) fn owner_snap_dropped(diag: Option<u32>) -> u32 {
    match diag {
        Some(dropped) => dropped,
        None => crate::snapkey::sys::queue_dropped(),
    }
}
