#![forbid(unsafe_code)]
use super::*;

/// Revalidate one write target immediately before `SetWindowPos`: fresh
/// eligibility, scope, fresh full identity compared against the cached
/// expectation and the frozen allowlist, with the process held open across
/// the write. Returns the fresh observation plus the held process (liveness
/// guard).
pub(super) struct WriteTarget {
    pub(super) window: ObservedWindow,
    pub(super) held: HeldProcess,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn revalidate_target(
    expected: &ObservedWindow,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
    proof_mode: bool,
    allowlist: Option<&Vec<AllowEntry>>,
    scope: &[String],
    member_tag: Option<&str>,
    scope_hosts: &[ScopeHostChild],
    allow_maximized: bool,
) -> std::result::Result<WriteTarget, &'static str> {
    let hwnd = expected.hwnd as isize as HWND;
    let pid = crate::ffi::window_pid(expected.hwnd);
    if pid == 0 || pid != expected.identity.pid {
        return Err("pid-changed");
    }
    let held = HeldProcess::open(pid).map_err(|_| "identity-changed")?;
    let ident = held.identity().map_err(|_| "identity-changed")?;
    if ident.pid != pid
        || ident.process_creation != expected.identity.process_creation
        || ident.exe_path != expected.identity.exe_path
        || ident.user_sid != expected.identity.user_sid
        || ident.session_id != expected.identity.session_id
    {
        return Err("identity-changed");
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return Err("identity-changed");
    }
    let rid = held.integrity().map_err(|_| "identity-changed")?;
    if !is_medium_rid(rid) {
        return Err("integrity-changed");
    }
    let fresh = observe_window(hwnd, me, fulls, tokens).map_err(|_| "ineligible")?;
    if !scope_allows(scope, &fresh.identity.exe_path) {
        return Err("scope-excluded");
    }
    // Visible lifetime gate: the live member tag must equal the stored tag,
    // so a same-process HWND reuse (same HWND/PID/creation, fresh window)
    // authorizes no geometry write and no focus actuation. Untracked windows
    // (no stored tag) never pass, only admission stamps.
    let live_tag = crate::product_hide::sys::read_member_tag(fresh.hwnd);
    let Some(stored) = member_tag else {
        return Err("identity-changed");
    };
    if !crate::native_guards::visible_lifetime_ok(stored, live_tag.as_deref()) {
        return Err("identity-changed");
    }
    // Hosted-child fence, fresh (never from the tick's observation): a newly
    // appearing hosted app authorizes no geometry write and no focus
    // actuation under another app's membership.
    if !hosted_gate_allows(
        &fresh.identity.exe_path,
        fresh.hwnd,
        fresh.identity.pid,
        scope_hosts,
    ) {
        return Err("scope-excluded");
    }
    if proof_mode {
        let entries = allowlist.ok_or("allowlist-missing")?;
        let Some(entry) = entries
            .iter()
            .find(|entry| allow_match(entry, &fresh.identity))
        else {
            return Err("allowlist-changed");
        };
        // Owned-helper gate immediately before the setter: sibling
        // exe/class/tag, no ordinary fallback.
        verify_proof_owned(fresh.hwnd, entry, me)?;
    }
    // Focus carries no geometry write, so it stays allowed onto a maximized
    // or fullscreen member (KDE `requestFocus` overlay exemption); geometry
    // never allows either. Every other skip still refuses on both paths.
    if allow_maximized {
        crate::tiling::classify_focus(&fresh.facts).map_err(|reason| reason.as_str())?;
    } else {
        classify(&fresh.facts).map_err(|reason| reason.as_str())?;
    }
    if fresh.token != expected.token {
        return Err("identity-changed");
    }
    Ok(WriteTarget {
        window: fresh,
        held,
    })
}

/// Recheck the HWND's current pid after the write (recycled-HWND guard).
/// Registered test fakes answer from their scripted pid/liveness.
pub(super) fn pid_current(hwnd_u64: u64, pid: u32) -> bool {
    #[cfg(test)]
    if crate::product_hide::sys::fake::active(hwnd_u64) {
        return crate::product_hide::sys::fake::with(hwnd_u64, |w| w.alive && w.pid == pid)
            .unwrap_or(false);
    }
    let current = crate::ffi::window_pid(hwnd_u64);
    current != 0 && current == pid
}

/// Fresh maximized read on one HWND (single `IsZoomed` query, no frame read).
pub(super) fn is_zoomed_now(hwnd_u64: u64) -> bool {
    crate::ffi::is_zoomed(hwnd_u64)
}

/// Live overlay flags for one HWND (item 20): maximized from `IsZoomed`,
/// fullscreen from the live caption bit plus the live extended frame against
/// the monitor full rects (same read shape as the foreground veto, without
/// cloak/visibility). Fullscreen is `None` when the frame is unreadable
/// (minimized/hidden/foreign): the caller falls back to the retained row
/// facts, never a fabricated value. Captioned windows are never fullscreen.
/// No window is touched: three bounded reads, no setters.
#[must_use]
pub fn live_overlay_flags(hwnd_u64: u64, fulls: &[Rect]) -> (bool, Option<bool>) {
    #[cfg(test)]
    if let Some(flags) = INJECTED_OVERLAY.with(|slot| slot.borrow_mut().pop_front()) {
        return flags;
    }
    let maximized = crate::ffi::is_zoomed(hwnd_u64);
    let style_opt = crate::ffi::window_style_checked(hwnd_u64);
    let style_ok = style_opt.is_some();
    let style = style_opt.unwrap_or(0);
    if style_ok && style & WS_CAPTION != 0 {
        return (maximized, Some(false));
    }
    let Some(visible_raw) = crate::ffi::extended_frame_bounds(hwnd_u64) else {
        return (maximized, None);
    };
    let covering = rect_from_win(visible_raw)
        .is_some_and(|visible| is_borderless_fullscreen(true, visible, fulls));
    if covering {
        // Fullscreen needs captionless evidence: a covering frame with an
        // unreadable style stays unknown, never fabricated.
        return (maximized, if style_ok { Some(true) } else { None });
    }
    (maximized, Some(false))
}

// Probe one armed wake: `(expired, lost, done)` for the routing seam.
pub(super) fn restore_wake_probe(
    state: &TileLoop,
    wake: &RestoreWake,
    now: Instant,
) -> (bool, bool, bool) {
    let expired = now >= wake.until;
    let lost = state
        .member_tokens
        .get(&wake.key)
        .is_none_or(|live| *live != wake.token)
        || !crate::ffi::is_window(wake.key.hwnd);
    (expired, lost, !is_zoomed_now(wake.key.hwnd))
}

/// Restore one maximized window to its normal placement without activating
/// it: the official `GetWindowPlacement`/`SetWindowPlacement` route with
/// `SW_SHOWNOACTIVATE` (restores in place, active window stays active) plus
/// `WPF_ASYNCWINDOWPLACEMENT` (the request posts to the owner thread, so the
/// loop never blocks on a hung window). Single bounded call pair, no wait, no
/// retry. Returns dispatch vs observed completion honestly: `restored` only
/// when the window no longer reads maximized, `dispatched` when the setter
/// accepted but the async completion is still pending (converges next tick),
/// `threw` when nothing was dispatched.
pub(super) fn restore_zoom_placement(hwnd_u64: u64) -> &'static str {
    // Immediate pre-placement revalidation: a window that closed (or already
    // restored) between the tick observation and this write takes no write.
    if !crate::ffi::is_window(hwnd_u64) {
        return "threw";
    }
    if !is_zoomed_now(hwnd_u64) {
        return "restored";
    }
    let Some(mut placement) = crate::ffi::window_placement(hwnd_u64) else {
        return "threw";
    };
    if placement.showCmd != SW_SHOWMAXIMIZED as u32 {
        return if is_zoomed_now(hwnd_u64) {
            "dispatched"
        } else {
            "restored"
        };
    }
    placement.showCmd = SW_SHOWNOACTIVATE as u32;
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    if !crate::ffi::set_window_placement(hwnd_u64, &placement) {
        return "threw";
    }
    if is_zoomed_now(hwnd_u64) {
        "dispatched"
    } else {
        "restored"
    }
}

/// Toggle one verified managed window's native maximize state. Maximize uses
/// the official async route (`ShowWindowAsync`, never blocks on a hung
/// target); restore uses the nonactivating placement path above. Returns the
/// readback-settled outcome for the bounded lifecycle log: `dispatched`
/// whenever the setter accepted but native completion is still pending
/// (converges next tick; the waiter proves `IsZoomed` separately).
pub(super) fn toggle_zoom_async(hwnd_u64: u64, wanted: bool) -> &'static str {
    if wanted {
        if !crate::ffi::show_window_async(hwnd_u64, SW_MAXIMIZE) {
            return "threw";
        }
    } else {
        match restore_zoom_placement(hwnd_u64) {
            "restored" => return "restored",
            "dispatched" => return "dispatched",
            _ => return "threw",
        }
    }
    if is_zoomed_now(hwnd_u64) == wanted {
        if wanted { "maximized" } else { "restored" }
    } else {
        "dispatched"
    }
}
