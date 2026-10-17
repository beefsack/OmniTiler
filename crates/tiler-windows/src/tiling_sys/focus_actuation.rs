#![forbid(unsafe_code)]
use super::*;

/// Outcome of one Engine focus actuation plus setter/readback trace.
/// `setter_accepted` is the raw `SetForegroundWindow` BOOL (never trusted as
/// proof); `prime_inserted` is the bounded `SendInput` accepted-event count
/// for the E8 last-input prime (0..=2, never trusted as proof);
/// `attach_ok` reports the bounded `AttachThreadInput` coupling to the
/// pre-call foreground thread (never trusted as proof); `eventual` is true
/// only when the exact-foreground readback matched after the pumped settle
/// rather than immediately (never trusted beyond the readback itself);
/// `deferred` stays false: no synchronous foreign message waits, the
/// callback remains promptly available. Token-safe ints/bools for the
/// production log: no native ids.
pub(super) struct FocusActuation {
    pub(super) outcome: &'static str,
    pub(super) setter_accepted: bool,
    pub(super) prime_inserted: u8,
    pub(super) attach_ok: bool,
    pub(super) eventual: bool,
    pub(super) deferred: bool,
}

/// Acquire last-input rights immediately before `SetForegroundWindow`: one
/// unassigned E8 down/up pair (`VK_MASK`, `dwExtraInfo` 0) via `SendInput`.
/// The OS grants foreground rights to the last input provider, and E8 carries
/// no modifier, character, or Start-menu side effect (same technique as the
/// accepted Win-up mask). Zero means the prime contributed nothing; the caller
/// still attempts the setter and the exact foreground readback decides.
/// A partial single insert runs one bounded E8-up release so no virtual key
/// stays held. Runs on the owner loop thread, never in the LL callback, with
/// no wait, no `AttachThreadInput`, no Alt, and no policy/registry effect.
fn prime_foreground_rights() -> u8 {
    crate::ffi::prime_foreground_key(VK_MASK as u16)
}

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
    })
}

/// Actuate one Engine focus plan on the exact target window: fresh
/// revalidation with the process held open across the call, one bounded E8
/// `SendInput` last-input prime, a bounded `AttachThreadInput` coupling of
/// the owner thread to the pre-call foreground thread, then exactly one
/// `SetForegroundWindow` with immediate detach, an exact immediate
/// `GetForegroundWindow` readback, and - on an immediate miss - a bounded
/// pumped settle (own queue only, hook stays live) distinguishing deferred
/// delivery from no transfer. The prime count, the attach result, and the
/// setter return are never trusted; only an exact foreground readback settles
/// `focus-ok`. There is no Alt injection and no `SendMessageTimeoutW` wait.
/// Proof mode keeps the frozen-allowlist gate; normal mode skips it.
///
/// This runs on the loop thread, never inside the low-level hook callback.
/// No call here waits on foreign input: `AttachThreadInput` only links
/// queues, `SetForegroundWindow` returns without waiting, detach runs
/// immediately after the single setter, and the settle only pumps the owner's
/// own queue. Revalidation is identity/liveness, not responsiveness - a hung
/// target cannot stall these calls because none of them blocks on it.
pub(super) fn actuate_focus(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    to_token: &str,
) -> FocusActuation {
    #[cfg(test)]
    if let Some(outcome) = INJECTED_FOCUS.with(|slot| *slot.borrow()) {
        PENDING_COUNTERS.with(|slot| {
            slot.borrow_mut().focus += 1;
        });
        let _ = (state, me, fulls, observed, retained, to_token);
        return FocusActuation {
            outcome,
            setter_accepted: false,
            prime_inserted: 0,
            attach_ok: false,
            eventual: false,
            deferred: false,
        };
    }
    let unsettled = |outcome: &'static str| FocusActuation {
        outcome,
        setter_accepted: false,
        prime_inserted: 0,
        attach_ok: false,
        eventual: false,
        deferred: false,
    };
    // Eligible observed target first; otherwise a verified retained overlay
    // member (KDE `requestFocus` overlay exemption): maximized and fullscreen
    // members are retained, never eligible, so eligible-only lookup never
    // focuses them. The expected identity below still revalidates fresh with
    // the full identity/scope/hosted/proof/lifetime gates; only the geometry
    // classifier is focus-relaxed (overlays allowed, everything else still
    // refused). Reuses the retained-aware origin set's membership rule.
    let owned_expected = overlay_expected(state, observed, retained, to_token);
    let Some(expected) = owned_expected.as_ref() else {
        return unsettled("vanished");
    };
    let proof_mode = state.allowlist.is_some();
    let member_key = crate::workspace::WindowKey {
        hwnd: expected.hwnd,
        pid: expected.identity.pid,
        creation: expected.identity.process_creation.clone(),
    };
    let stored_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        stored_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => return unsettled(reason),
    };
    if crate::ffi::foreground_window() == target.window.hwnd {
        // Already exact foreground: no input and no setter needed.
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted: false,
            prime_inserted: 0,
            attach_ok: false,
            eventual: false,
            deferred: false,
        };
    }
    let prime_inserted = prime_foreground_rights();
    // Real fullscreen arrival fence immediately before the native setter: a
    // vetoing foreground (or an elevated foreground) that arrived during
    // validation/priming is never stolen from. The geometry path below then
    // vetoes honestly instead of riding along. No wait, no retry. Verified
    // managed overlays bypass like a managed maximize.
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        return FocusActuation {
            outcome: "focus-skipped-fence",
            setter_accepted: false,
            prime_inserted,
            attach_ok: false,
            eventual: false,
            deferred: false,
        };
    }
    // Couple the owner input queue to the pre-call foreground thread so the
    // single setter below runs attached (AHK ladder rung one). Skipped when
    // there is no foreground thread or it is already ours. No wait, no Alt.
    // `GetWindowThreadProcessId` RETURNS the thread id and writes the process
    // id to its out-param: the tid comes from the return value, never from
    // the pid slot (a pid-as-tid attach was the 024139 false negative).
    let current_tid = crate::ffi::current_thread_id();
    let foreground_before = crate::ffi::foreground_window();
    let foreground_tid = if foreground_before == 0 {
        0
    } else {
        crate::ffi::window_thread_id(foreground_before)
    };
    let attach_ok = foreground_tid != 0
        && foreground_tid != current_tid
        && crate::ffi::attach_thread_input(current_tid, foreground_tid, true);
    let setter_accepted = crate::ffi::set_foreground_window(target.window.hwnd);
    if attach_ok {
        crate::ffi::attach_thread_input(current_tid, foreground_tid, false);
    }
    // The held process keeps liveness authority across the call; recheck the
    // HWND pid afterward against recycling.
    let _ = &target.held;
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return FocusActuation {
            outcome: "pid-changed",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    if crate::ffi::foreground_window() == target.window.hwnd {
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    // Immediate miss with an accepted setter does not establish no transfer:
    // the system can deliver foreground after the call returns. Pump the
    // owner's own queue (new intents queue bounded for the next drain; the
    // hook callback stays prompt) and re-read, bounded with no foreign wait.
    let mut eventual = false;
    for _ in 0..FOCUS_SETTLE_ROUNDS {
        pump_wait(FOCUS_SETTLE_POLL_MS);
        if crate::ffi::foreground_window() == target.window.hwnd {
            eventual = true;
            break;
        }
    }
    // Recycled-HWND guard covers the settle window too.
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return FocusActuation {
            outcome: "pid-changed",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    if eventual {
        return FocusActuation {
            outcome: "focus-ok",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual,
            deferred: false,
        };
    }
    if !setter_accepted {
        return FocusActuation {
            outcome: "focus-unverified",
            setter_accepted,
            prime_inserted,
            attach_ok,
            eventual: false,
            deferred: false,
        };
    }
    FocusActuation {
        outcome: "focus-unverified",
        setter_accepted,
        prime_inserted,
        attach_ok,
        eventual: false,
        deferred: false,
    }
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
/// owner-caused continuation within the same batch (a prior actuation moved
/// focus and the fresh foreground is exactly that target), so rapid repeated
/// navigation survives while external focus moves reject loudly. Focus intents
/// actuate the Engine `FocusDirectional` target with an exact foreground
/// readback; move intents apply the `MoveDirectional` geometry through the
/// shared write path and report its settled summary. Key-ups close the pair
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
